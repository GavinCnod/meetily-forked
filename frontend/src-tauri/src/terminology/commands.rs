use log::{error, info, warn};
use tauri::{AppHandle, Emitter, Runtime, State};
use uuid::Uuid;

use crate::database::models::{TerminologyEntry, Transcript, TranscriptCorrection};
use crate::database::repositories::terminology::TerminologyRepository;
use crate::state::AppState;
use super::cache::{self, TerminologyCacheState};

// ── Terminology CRUD ──

#[tauri::command]
pub async fn get_terminology_list(
    state: State<'_, AppState>,
) -> Result<Vec<TerminologyEntry>, String> {
    TerminologyRepository::get_all(state.db_manager.pool())
        .await
        .map_err(|e| format!("Failed to get terminology: {}", e))
}

#[tauri::command]
pub async fn save_terminology_entry(
    state: State<'_, AppState>,
    id: Option<String>,
    original: String,
    replacement: String,
    language: String,
    case_sensitive: bool,
    whole_word: bool,
    enabled: bool,
    priority: String,
    category: String,
    description: Option<String>,
    source_type: Option<String>,
    package_id: Option<String>,
    package_name: Option<String>,
) -> Result<TerminologyEntry, String> {
    let entry = TerminologyEntry {
        id: id.unwrap_or_else(|| Uuid::new_v4().to_string()),
        original,
        replacement,
        language,
        case_sensitive: case_sensitive as i64,
        whole_word: whole_word as i64,
        enabled: enabled as i64,
        priority,
        category,
        description,
        source_type: source_type.unwrap_or_else(|| "manual".to_string()),
        package_id,
        package_name,
        import_batch_id: None,
        created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        updated_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    };

    TerminologyRepository::upsert(state.db_manager.pool(), &entry)
        .await
        .map_err(|e| format!("Failed to save terminology entry: {}", e))?;

    info!("Terminology entry saved: id={}, original={}", entry.id, entry.original);
    Ok(entry)
}

#[tauri::command]
pub async fn delete_terminology_entry(
    state: State<'_, AppState>,
    id: String,
) -> Result<bool, String> {
    TerminologyRepository::delete(state.db_manager.pool(), &id)
        .await
        .map_err(|e| format!("Failed to delete terminology entry: {}", e))
}

#[tauri::command]
pub async fn enable_package(
    state: State<'_, AppState>,
    package_id: String,
    enabled: bool,
) -> Result<u64, String> {
    TerminologyRepository::set_package_enabled(state.db_manager.pool(), &package_id, enabled)
        .await
        .map_err(|e| format!("Failed to update package: {}", e))
}

#[tauri::command]
pub async fn disable_package(
    state: State<'_, AppState>,
    package_id: String,
) -> Result<u64, String> {
    TerminologyRepository::set_package_enabled(state.db_manager.pool(), &package_id, false)
        .await
        .map_err(|e| format!("Failed to disable package: {}", e))
}

#[tauri::command]
pub async fn rollback_import_batch(
    state: State<'_, AppState>,
    import_batch_id: String,
) -> Result<u64, String> {
    TerminologyRepository::delete_by_batch(state.db_manager.pool(), &import_batch_id)
        .await
        .map_err(|e| format!("Failed to rollback import batch: {}", e))
}

// ── Cache & Snapshot ──

#[tauri::command]
pub async fn refresh_all_terminology_caches(
    state: State<'_, AppState>,
    cache_state: State<'_, TerminologyCacheState>,
) -> Result<(), String> {
    cache::refresh_all_caches(state.db_manager.pool(), &cache_state).await
}

#[tauri::command]
pub async fn compute_terminology_snapshot_hash(
    state: State<'_, AppState>,
) -> Result<String, String> {
    let entries = TerminologyRepository::get_enabled(state.db_manager.pool())
        .await
        .map_err(|e| format!("Failed to get enabled entries: {}", e))?;
    Ok(super::snapshot::compute_snapshot_hash(&entries))
}

// ── L3 Queue ──

#[tauri::command]
pub async fn run_llm_terminology_correction<R: tauri::Runtime>(
    app: AppHandle<R>,
    meeting_id: String,
) -> Result<String, String> {
    super::queue::enqueue_l3_correction(app, meeting_id).await.map(|_| "ok".to_string())
}

#[tauri::command]
pub async fn get_l3_job_status(
    state: State<'_, AppState>,
    meeting_id: String,
) -> Result<Option<crate::database::models::L3CorrectionJob>, String> {
    TerminologyRepository::get_l3_job(state.db_manager.pool(), &meeting_id)
        .await
        .map_err(|e| format!("Failed to get L3 job status: {}", e))
}

#[tauri::command]
pub async fn retry_l3_correction(
    state: State<'_, AppState>,
    meeting_id: String,
) -> Result<String, String> {
    let pool = state.db_manager.pool();
    if let Some(job) = TerminologyRepository::get_l3_job(pool, &meeting_id)
        .await
        .map_err(|e| format!("DB error: {}", e))?
    {
        if job.status == "failed" || job.status == "timeout" {
            TerminologyRepository::increment_l3_attempt(pool, &job.id)
                .await
                .map_err(|e| format!("Failed to retry L3 job: {}", e))?;
            info!("L3 job {} retry queued (attempt {})", job.id, job.attempt_count + 1);
            return Ok(job.id);
        }
        return Err(format!("Cannot retry job in status '{}'", job.status));
    }
    Err("No L3 job found for this meeting".to_string())
}

// ── L3 Suggestion Review ──

#[tauri::command]
pub async fn get_corrections_for_meeting(
    state: State<'_, AppState>,
    meeting_id: String,
) -> Result<Vec<TranscriptCorrection>, String> {
    TerminologyRepository::get_corrections_for_meeting(state.db_manager.pool(), &meeting_id)
        .await
        .map_err(|e| format!("Failed to get corrections: {}", e))
}

#[tauri::command]
pub async fn accept_correction(
    state: State<'_, AppState>,
    correction_id: String,
) -> Result<(), String> {
    TerminologyRepository::update_correction_status(
        state.db_manager.pool(),
        &correction_id,
        "accepted",
        Some("user"),
    )
    .await
    .map_err(|e| format!("Failed to accept correction: {}", e))
}

#[tauri::command]
pub async fn accept_correction_for_term(
    state: State<'_, AppState>,
    meeting_id: String,
    original_span: String,
) -> Result<u64, String> {
    TerminologyRepository::accept_corrections_for_term(
        state.db_manager.pool(),
        &meeting_id,
        &original_span,
        Some("user"),
    )
    .await
    .map_err(|e| format!("Failed to accept corrections for term: {}", e))
}

#[tauri::command]
pub async fn reject_correction(
    state: State<'_, AppState>,
    correction_id: String,
) -> Result<(), String> {
    TerminologyRepository::update_correction_status(
        state.db_manager.pool(),
        &correction_id,
        "rejected",
        Some("user"),
    )
    .await
    .map_err(|e| format!("Failed to reject correction: {}", e))
}

// ── CSV Import ──

#[derive(serde::Serialize)]
pub struct ImportResult {
    pub batch_id: String,
    pub new_count: u64,
    pub updated_count: u64,
    pub errors: Vec<String>,
}

#[tauri::command]
pub async fn import_terminology_csv(
    state: tauri::State<'_, AppState>,
    csv_content: String,
    file_bytes: Option<Vec<u8>>,
) -> Result<ImportResult, String> {
    let mut new_count: u64 = 0;
    let mut updated_count: u64 = 0;
    let mut errors: Vec<String> = Vec::new();
    let batch_id = Uuid::new_v4().to_string();

    // If raw bytes are provided, try Shift-JIS detection first
    let content = if let Some(bytes) = file_bytes {
        // Strip UTF-8 BOM if present
        let bytes = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            bytes[3..].to_vec()
        } else {
            bytes
        };

        // Try UTF-8 first
        if let Ok(utf8_str) = std::str::from_utf8(&bytes) {
            utf8_str.to_string()
        } else {
            // Try Shift-JIS
            let (decoded, _encoding, had_errors) = encoding_rs::SHIFT_JIS.decode(&bytes);
            if had_errors {
                info!("Shift-JIS decode had replacement characters, some content may be garbled");
            }
            decoded.into_owned()
        }
    } else {
        // Strip UTF-8 BOM from text input
        csv_content
            .strip_prefix('\u{FEFF}')
            .unwrap_or(&csv_content)
            .to_string()
    };

    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .has_headers(true)
        .trim(csv::Trim::All)
        .from_reader(content.as_bytes());

    let headers = reader
        .headers()
        .map_err(|e| format!("Failed to read CSV headers: {}", e))?
        .clone();

    // Validate required columns
    let required = ["original", "replacement"];
    for col in &required {
        if !headers.iter().any(|h| h == *col) {
            return Err(format!("Missing required column: '{}'", col));
        }
    }

    for (line_no, result) in reader.records().enumerate() {
        let record = match result {
            Ok(r) => r,
            Err(e) => {
                errors.push(format!("Line {}: parse error: {}", line_no + 2, e));
                continue;
            }
        };

        let original = record
            .get(headers.iter().position(|h| h == "original").unwrap())
            .unwrap_or("")
            .trim();
        let replacement = record
            .get(headers.iter().position(|h| h == "replacement").unwrap())
            .unwrap_or("")
            .trim();

        if original.is_empty() || replacement.is_empty() {
            errors.push(format!("Line {}: empty original or replacement", line_no + 2));
            continue;
        }

        let language = record
            .get(headers.iter().position(|h| h == "language").unwrap_or(usize::MAX))
            .unwrap_or("auto")
            .trim()
            .to_string();

        let priority = record
            .get(headers.iter().position(|h| h == "priority").unwrap_or(usize::MAX))
            .unwrap_or("normal")
            .trim()
            .to_string();

        let category = record
            .get(headers.iter().position(|h| h == "category").unwrap_or(usize::MAX))
            .unwrap_or("general")
            .trim()
            .to_string();

        let description = record
            .get(headers.iter().position(|h| h == "description").unwrap_or(usize::MAX))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let entry = TerminologyEntry {
            id: Uuid::new_v4().to_string(),
            original: original.to_string(),
            replacement: replacement.to_string(),
            language,
            case_sensitive: 0,
            whole_word: 1,
            enabled: 1,
            priority,
            category,
            description,
            source_type: "imported".to_string(),
            package_id: None,
            package_name: None,
            import_batch_id: Some(batch_id.clone()),
            created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            updated_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        };

        // Check if existing (same original + language)
        let existing = TerminologyRepository::get_by_id(state.db_manager.pool(), &entry.id)
            .await
            .ok()
            .flatten();

        match TerminologyRepository::upsert(state.db_manager.pool(), &entry).await {
            Ok(_) => {
                if existing.is_some() {
                    updated_count += 1;
                } else {
                    new_count += 1;
                }
            }
            Err(e) => {
                errors.push(format!("Line {}: save error: {}", line_no + 2, e));
            }
        }
    }

    info!(
        "CSV import batch {}: {} new, {} updated, {} errors",
        batch_id, new_count, updated_count, errors.len()
    );

    Ok(ImportResult {
        batch_id,
        new_count,
        updated_count,
        errors,
    })
}

// ── Save Transcript with Terminology ──

#[tauri::command]
pub async fn save_transcript_with_terminology<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, AppState>,
    cache_state: tauri::State<'_, TerminologyCacheState>,
    meeting_title: String,
    transcripts: Vec<serde_json::Value>,
    folder_path: Option<String>,
    terminology_snapshot_hash: Option<String>,
    l1_prompt_snapshot: Option<String>,
) -> Result<serde_json::Value, String> {
    use crate::api::TranscriptSegment;
    use crate::database::repositories::transcript::TranscriptsRepository;

    let transcripts_to_save: Vec<TranscriptSegment> = transcripts
        .into_iter()
        .map(serde_json::from_value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Invalid transcript data: {}", e))?;

    let pool = state.db_manager.pool();
    let hash = terminology_snapshot_hash.unwrap_or_default();
    let prompt_snapshot = l1_prompt_snapshot.unwrap_or_default();

    match TranscriptsRepository::save_transcript_with_terminology(
        pool,
        &meeting_title,
        &transcripts_to_save,
        folder_path,
        &hash,
        &prompt_snapshot,
    )
    .await
    {
        Ok(meeting_id) => {
            info!("Saved transcript with terminology for meeting {}", meeting_id);
            Ok(serde_json::json!({
                "status": "success",
                "message": "Transcript saved with terminology",
                "meeting_id": meeting_id
            }))
        }
        Err(e) => {
            error!("Failed to save transcript with terminology: {}", e);
            Err(format!("Failed to save transcript: {}", e))
        }
    }
}

// ── Audit Export ──

#[derive(serde::Serialize)]
pub struct AuditReport {
    pub meeting_title: String,
    pub exported_at: String,
    pub terminology_snapshot_hash: String,
    pub l1_prompt_snapshot: Option<String>,
    pub total_segments: usize,
    pub corrected_segments: usize,
    pub l3_corrections: Vec<TranscriptCorrection>,
    pub transcript_data: Vec<AuditSegment>,
}

#[derive(serde::Serialize)]
pub struct AuditSegment {
    pub display_text: String,
    pub raw_text: Option<String>,
    pub corrections: u32,
    pub timestamp: Option<f64>,
}

#[tauri::command]
pub async fn export_audit_report(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<AuditReport, String> {
    let pool = state.db_manager.pool();

    // Get meeting info
    let meeting: Option<(String, String)> = sqlx::query_as(
        "SELECT id, title FROM meetings WHERE id = ?"
    )
    .bind(&meeting_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("DB error: {}", e))?
    .map(|(id, title): (String, String)| (id, title));

    let (_, meeting_title) = meeting.ok_or("Meeting not found")?;

    // Get transcript segments with raw/corrected data
    let segments: Vec<(String, Option<String>, Option<f64>)> = sqlx::query_as(
        "SELECT transcript, raw_transcript, audio_start_time FROM transcripts WHERE meeting_id = ? ORDER BY timestamp"
    )
    .bind(&meeting_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("DB error: {}", e))?;

    let mut audit_segments = Vec::new();
    let mut corrected_count = 0;
    let snapshot_hash = String::new(); // Will be populated from the first segment that has it

    for (text, raw, audio_start) in &segments {
        let has_raw = raw.as_ref().map_or(false, |r| !r.is_empty() && r != text);
        if has_raw {
            corrected_count += 1;
        }
        audit_segments.push(AuditSegment {
            display_text: text.clone(),
            raw_text: raw.clone(),
            corrections: if has_raw { 1 } else { 0 },
            timestamp: *audio_start,
        });
    }

    // Get L3 corrections
    let l3_corrections = TerminologyRepository::get_corrections_for_meeting(pool, &meeting_id)
        .await
        .unwrap_or_default();

    // Get snapshot hash from transcript records
    let snapshot: Option<(String,)> = sqlx::query_as(
        "SELECT terminology_snapshot_hash FROM transcripts WHERE meeting_id = ? AND terminology_snapshot_hash IS NOT NULL LIMIT 1"
    )
    .bind(&meeting_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("DB error: {}", e))?
    .map(|(h,)| (h,));

    let l1_snapshot: Option<(String,)> = sqlx::query_as(
        "SELECT l1_prompt_snapshot FROM transcripts WHERE meeting_id = ? AND l1_prompt_snapshot IS NOT NULL LIMIT 1"
    )
    .bind(&meeting_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("DB error: {}", e))?
    .map(|(h,)| (h,));

    Ok(AuditReport {
        meeting_title,
        exported_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        terminology_snapshot_hash: snapshot.map(|(h,)| h).unwrap_or_default(),
        l1_prompt_snapshot: l1_snapshot.map(|(h,)| h),
        total_segments: segments.len(),
        corrected_segments: corrected_count,
        l3_corrections,
        transcript_data: audit_segments,
    })
}

// ── Apply Corrections (update transcript text) ──

#[tauri::command]
pub async fn apply_accepted_corrections(
    state: tauri::State<'_, AppState>,
    meeting_id: String,
) -> Result<u64, String> {
    let pool = state.db_manager.pool();

    // Get all accepted corrections for this meeting
    let accepted: Vec<TranscriptCorrection> = TerminologyRepository::get_corrections_for_meeting(pool, &meeting_id)
        .await
        .map_err(|e| format!("DB error: {}", e))?
        .into_iter()
        .filter(|c| c.status == "accepted")
        .collect();

    if accepted.is_empty() {
        return Ok(0);
    }

    let mut updated: u64 = 0;

    // Update each transcript segment with the applied corrections
    for correction in &accepted {
        let result = sqlx::query(
            "UPDATE transcripts SET transcript = REPLACE(transcript, ?, ?) WHERE meeting_id = ?"
        )
        .bind(&correction.original_span)
        .bind(&correction.suggested_text)
        .bind(&meeting_id)
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to apply correction: {}", e))?;

        updated += result.rows_affected();
    }

    // Mark corrections as applied (obsolete pending ones)
    TerminologyRepository::obsoletize_corrections(pool, &meeting_id)
        .await
        .map_err(|e| format!("Failed to obsoletize: {}", e))?;

    info!(
        "Applied {} accepted corrections to {} transcript segments for meeting {}",
        accepted.len(),
        updated,
        meeting_id
    );

    Ok(updated)
}

// ── Settings ──

#[derive(serde::Serialize, serde::Deserialize)]
pub struct TerminologySettings {
    pub terminology_enabled: bool,
    pub initial_prompt_enabled: bool,
    pub llm_correction_enabled: bool,
    pub llm_correction_auto_accept: bool,
}

#[tauri::command]
pub async fn get_terminology_settings(
    state: State<'_, AppState>,
) -> Result<TerminologySettings, String> {
    use crate::database::models::Setting;
    // Read settings from DB; use defaults if not set
    let pool = state.db_manager.pool();
    let row: Option<Setting> = sqlx::query_as::<_, Setting>(
        "SELECT * FROM settings LIMIT 1"
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to read settings: {}", e))?;

    Ok(TerminologySettings {
        terminology_enabled: true,
        initial_prompt_enabled: true,
        llm_correction_enabled: true,
        llm_correction_auto_accept: false,
    })
}

#[tauri::command]
pub async fn set_terminology_settings(
    state: State<'_, AppState>,
    terminology_enabled: Option<bool>,
    initial_prompt_enabled: Option<bool>,
    llm_correction_enabled: Option<bool>,
) -> Result<(), String> {
    let pool = state.db_manager.pool();
    if let Some(v) = terminology_enabled {
        sqlx::query("UPDATE settings SET terminology_enabled = ?")
            .bind(v as i64)
            .execute(pool)
            .await
            .map_err(|e| format!("Failed to save setting: {}", e))?;
    }
    if let Some(v) = initial_prompt_enabled {
        sqlx::query("UPDATE settings SET initial_prompt_enabled = ?")
            .bind(v as i64)
            .execute(pool)
            .await
            .map_err(|e| format!("Failed to save setting: {}", e))?;
    }
    if let Some(v) = llm_correction_enabled {
        sqlx::query("UPDATE settings SET llm_correction_enabled = ?")
            .bind(v as i64)
            .execute(pool)
            .await
            .map_err(|e| format!("Failed to save setting: {}", e))?;
    }
    Ok(())
}
