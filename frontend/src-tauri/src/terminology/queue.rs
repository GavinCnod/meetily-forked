use std::sync::Arc;
use std::time::Duration;
use log::{error, info, warn};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::database::models::TranscriptCorrection;
use crate::database::repositories::terminology::TerminologyRepository;
use super::commands::TerminologySettings;

const L3_TIMEOUT_SECS: u64 = 60;
const L3_DEFAULT_MODEL: &str = "qwen2.5:7b";
const L3_FALLBACK_MODEL: &str = "qwen2.5:3b";

/// Serial concurrency limiter for L3 correction jobs.
static L3_SEMAPHORE: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

/// Enqueue and execute an L3 LLM correction job for a meeting.
/// Called when recording stops. Runs asynchronously.
pub async fn enqueue_l3_correction<R: Runtime>(
    app: AppHandle<R>,
    meeting_id: String,
) -> Result<(), String> {
    let pool = {
        let state = app.state::<crate::state::AppState>();
        state.db_manager.pool().clone()
    };

    // Idempotent check
    if let Some(job) = TerminologyRepository::find_active_l3_job(&pool, &meeting_id)
        .await
        .map_err(|e| format!("DB error: {}", e))?
    {
        info!("L3 job already active for meeting {}: job={}", meeting_id, job.id);
        return Ok(());
    }

    // Persist job record (入队即写DB)
    let job_id = uuid::Uuid::new_v4().to_string();
    TerminologyRepository::insert_l3_job(&pool, &job_id, &meeting_id, "queued")
        .await
        .map_err(|e| format!("Failed to enqueue L3 job: {}", e))?;

    info!("L3 job {} enqueued for meeting {}", job_id, meeting_id);

    // Spawn async execution
    let app_clone = app.clone();
    let mid = meeting_id.clone();
    let jid = job_id.clone();

    tauri::async_runtime::spawn(async move {
        let _permit = L3_SEMAPHORE.acquire().await;

        let pool = app_clone
            .state::<crate::state::AppState>()
            .db_manager
            .pool()
            .clone();

        // Mark running
        let _ = TerminologyRepository::update_l3_job_status(&pool, &jid, "running", None).await;

        // Collect enabled terminology entries for context
        let terminology_ctx = match TerminologyRepository::get_enabled(&pool).await {
            Ok(entries) => entries,
            Err(e) => {
                error!("L3: Failed to load terminology: {}", e);
                let _ = TerminologyRepository::update_l3_job_status(
                    &pool, &jid, "failed", Some(&e.to_string()),
                ).await;
                return;
            }
        };

        // Read transcript text for the meeting
        let transcript_text = match load_meeting_transcript(&pool, &mid).await {
            Ok(text) => text,
            Err(e) => {
                error!("L3: Failed to load transcript: {}", e);
                let _ = TerminologyRepository::update_l3_job_status(
                    &pool, &jid, "failed", Some(&e.to_string()),
                ).await;
                return;
            }
        };

        if transcript_text.trim().is_empty() {
            info!("L3: No transcript text for meeting {}, skipping", mid);
            let _ = TerminologyRepository::update_l3_job_status(&pool, &jid, "done", None).await;
            return;
        }

        // Build L3 correction prompt
        let (system_prompt, user_prompt) = build_l3_prompt(&transcript_text, &terminology_ctx);

        // Get configured LLM provider from settings
        let (provider_str, model_name, api_key, ollama_endpoint) =
            match get_summary_config(&pool).await {
                Ok(cfg) => cfg,
                Err(e) => {
                    error!("L3: Failed to get summary config: {}", e);
                    let _ = TerminologyRepository::update_l3_job_status(
                        &pool, &jid, "failed", Some(&e),
                    ).await;
                    return;
                }
            };

        let provider = match crate::summary::llm_client::LLMProvider::from_str(&provider_str) {
            Ok(p) => p,
            Err(e) => {
                // Fall back to Ollama
                warn!("L3: Unknown provider '{}', falling back to Ollama: {}", provider_str, e);
                crate::summary::llm_client::LLMProvider::Ollama
            }
        };

        let client = reqwest::Client::new();
        let active_model = model_name.clone();
        let mut correction_result: Option<Vec<TranscriptCorrection>> = None;
        let mut last_error: Option<String> = None;

        // Try primary model, then fallback
        for attempt_model in [model_name.as_str(), L3_FALLBACK_MODEL] {
            match tokio::time::timeout(
                Duration::from_secs(L3_TIMEOUT_SECS),
                crate::summary::llm_client::generate_summary(
                    &client,
                    &provider,
                    attempt_model,
                    &api_key,
                    &system_prompt,
                    &user_prompt,
                    ollama_endpoint.as_deref(),
                    None,
                    Some(2048),
                    Some(0.3),
                    None,
                    None,
                    None,
                ),
            )
            .await
            {
                Ok(Ok(response)) => {
                    // Parse LLM response into structured suggestions
                    match parse_l3_response(&response, &mid, &jid) {
                        Ok(suggestions) => {
                            if suggestions.is_empty() {
                                info!("L3: No corrections suggested by LLM for meeting {}", mid);
                            }
                            correction_result = Some(suggestions);
                            break;
                        }
                        Err(e) => {
                            warn!("L3: Failed to parse response with model {}: {}", attempt_model, e);
                            last_error = Some(format!("Parse error: {}", e));
                            continue;
                        }
                    }
                }
                Ok(Err(e)) => {
                    warn!("L3: LLM call failed with model {}: {}", attempt_model, e);
                    last_error = Some(format!("LLM error: {}", e));
                }
                Err(_) => {
                    warn!("L3: LLM call timed out with model {}", attempt_model);
                    last_error = Some("timeout".to_string());
                }
            }
        }

        match correction_result {
            Some(suggestions) => {
                let count = suggestions.len();
                if let Err(e) = TerminologyRepository::insert_corrections(&pool, &suggestions).await {
                    error!("L3: Failed to insert suggestions: {}", e);
                    let _ = TerminologyRepository::update_l3_job_status(
                        &pool, &jid, "failed", Some(&e.to_string()),
                    ).await;
                    return;
                }

                let _ = TerminologyRepository::update_l3_job_status(&pool, &jid, "done", None).await;
                info!("L3 job {} completed: {} suggestions for meeting {}", jid, count, mid);

                // Emit event to notify frontend
                let _ = app_clone.emit("llm-corrections-ready", serde_json::json!({
                    "meeting_id": mid,
                    "job_id": jid,
                    "suggestion_count": count,
                }));
            }
            None => {
                let err_msg = last_error.unwrap_or_else(|| "unknown error".to_string());
                let _ = TerminologyRepository::update_l3_job_status(
                    &pool, &jid, "failed", Some(&err_msg),
                ).await;
                error!("L3 job {} failed: {}", jid, err_msg);
            }
        }
    });

    Ok(())
}

/// Recover pending L3 jobs on app restart.
pub async fn recover_pending_tasks<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let pool = app
        .state::<crate::state::AppState>()
        .db_manager
        .pool()
        .clone();

    let pending = TerminologyRepository::get_pending_l3_jobs(&pool)
        .await
        .map_err(|e| format!("Failed to query pending L3 jobs: {}", e))?;

    if pending.is_empty() {
        return Ok(());
    }

    info!("Recovering {} pending L3 jobs", pending.len());

    for job in pending {
        // Re-queue jobs that were interrupted
        if job.status == "running" {
            info!(
                "Re-queuing interrupted L3 job {} for meeting {}",
                job.id, job.meeting_id
            );
            let _ = TerminologyRepository::increment_l3_attempt(&pool, &job.id).await;
        }
        // Re-spawn queued jobs
        let app_clone = app.clone();
        let mid = job.meeting_id.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = enqueue_l3_correction(app_clone, mid).await {
                error!("Failed to recover L3 job: {}", e);
            }
        });
    }

    Ok(())
}

/// Load the full transcript text for a meeting.
async fn load_meeting_transcript(
    pool: &sqlx::SqlitePool,
    meeting_id: &str,
) -> Result<String, String> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT transcript FROM transcripts WHERE meeting_id = ? ORDER BY timestamp"
    )
    .bind(meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to load transcripts: {}", e))?;

    Ok(rows.into_iter().map(|(t,)| t).collect::<Vec<_>>().join("\n"))
}

/// Get the configured summary provider/model for L3 correction.
async fn get_summary_config(
    pool: &sqlx::SqlitePool,
) -> Result<(String, String, String, Option<String>), String> {
    let row: Option<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT provider, model, ollamaEndpoint, ollamaApiKey FROM settings LIMIT 1"
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("DB error: {}", e))?;

    match row {
        Some((provider, model, endpoint, api_key)) => Ok((
            provider,
            model,
            api_key.unwrap_or_default(),
            endpoint,
        )),
        None => {
            // Default to Ollama
            Ok(("ollama".to_string(), L3_DEFAULT_MODEL.to_string(), String::new(), None))
        }
    }
}

/// Build the L3 correction prompt.
fn build_l3_prompt(
    transcript: &str,
    terminology: &[crate::database::models::TerminologyEntry],
) -> (String, String) {
    let system_prompt = "You are a specialized terminology correction assistant for chemical manufacturing meetings.\n\
        Your task is to correct STT (speech-to-text) errors in meeting transcripts.\n\n\
        STRICT RULES:\n\
        1. ONLY correct clear STT recognition errors. Do NOT rephrase, summarize, or expand the text.\n\
        2. Focus on: chemical names, CAS numbers, GHS codes, UN numbers, and safety terminology.\n\
        3. Each correction must preserve the EXACT original span you are correcting (at least 3-4 characters).\n\
        4. If a span appears multiple times but should only be corrected in some contexts, flag it with the specific context.\n\
        5. Do NOT add explanations, comments, or information not present in the original transcript.\n\
        6. Output MUST be valid JSON array.\n\n\
        OUTPUT FORMAT:\n\
        Return a JSON array of correction objects:\n\
        [{\"original_span\": \"text to replace\", \"suggested_text\": \"corrected text\", \"reason\": \"brief reason in English\", \"language\": \"ja|zh|en\", \"correction_type\": \"chemical|ghs_code|cas_number|un_number|general\"}]";

    let term_list: String = terminology
        .iter()
        .take(50) // Limit context to avoid prompt overflow
        .map(|e| format!("- {} → {} ({})", e.original, e.replacement, e.language))
        .collect::<Vec<_>>()
        .join("\n");

    let user_prompt = format!(
        "Correct STT errors in the following meeting transcript.\n\n\
         KNOWN TERMINOLOGY (for reference):\n{}\n\n\
         TRANSCRIPT:\n{}\n\n\
         Find and correct STT recognition errors. Return JSON array only.",
        if term_list.is_empty() { "(none)" } else { &term_list },
        transcript,
    );

    (system_prompt.to_string(), user_prompt)
}

/// Parse the LLM response into structured TranscriptCorrection objects.
fn parse_l3_response(
    response: &str,
    meeting_id: &str,
    job_id: &str,
) -> Result<Vec<TranscriptCorrection>, String> {
    // Extract JSON from response (may be wrapped in markdown code blocks)
    let json_str = response
        .trim()
        .strip_prefix("```json")
        .or_else(|| response.trim().strip_prefix("```"))
        .map(|s| s.strip_suffix("```").unwrap_or(s).trim())
        .unwrap_or(response.trim());

    let items: Vec<serde_json::Value> = serde_json::from_str(json_str)
        .map_err(|e| format!("Invalid JSON response: {} | Response: {}", e, &response[..response.len().min(200)]))?;

    let corrections: Vec<TranscriptCorrection> = items
        .iter()
        .filter_map(|item| {
            let original_span = item.get("original_span")?.as_str()?;
            let suggested_text = item.get("suggested_text")?.as_str()?;
            if original_span.len() < 2 || suggested_text.is_empty() {
                return None;
            }
            Some(TranscriptCorrection {
                id: uuid::Uuid::new_v4().to_string(),
                meeting_id: meeting_id.to_string(),
                job_id: job_id.to_string(),
                original_span: original_span.to_string(),
                suggested_text: suggested_text.to_string(),
                occurrences_json: None, // Populated by frontend during display
                language: item.get("language").and_then(|v| v.as_str()).map(String::from),
                correction_type: item.get("correction_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("llm")
                    .to_string(),
                reason: item.get("reason").and_then(|v| v.as_str()).map(String::from),
                source_snapshot_hash: None,
                status: "pending".to_string(),
                reviewed_by: None,
                reviewed_at: None,
                created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            })
        })
        .collect();

    Ok(corrections)
}
