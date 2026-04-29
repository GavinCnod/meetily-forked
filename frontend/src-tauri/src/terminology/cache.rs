use std::sync::Arc;
use tokio::sync::RwLock;
use log::info;

use crate::database::models::TerminologyEntry;
use super::corrector::TermRule;
use super::prompt::build_initial_prompt;
use super::snapshot::compute_snapshot_hash;

/// Thread-safe terminology cache holding compiled L2 rules, L1 prompt, and snapshot hash.
pub struct TerminologyCache {
    pub rules: Vec<TermRule>,
    pub l1_prompt: String,
    pub l1_excluded_ids: Vec<String>,
    pub snapshot_hash: String,
    pub total_count: usize,
    pub enabled_count: usize,
}

pub struct TerminologyCacheState {
    cache: Arc<RwLock<TerminologyCache>>,
}

impl TerminologyCacheState {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(TerminologyCache {
                rules: Vec::new(),
                l1_prompt: String::new(),
                l1_excluded_ids: Vec::new(),
                snapshot_hash: String::new(),
                total_count: 0,
                enabled_count: 0,
            })),
        }
    }

    pub fn cache_ref(&self) -> Arc<RwLock<TerminologyCache>> {
        self.cache.clone()
    }

    /// Get a snapshot of the current L2 rules for reading.
    pub async fn get_rules(&self) -> Vec<TermRule> {
        self.cache.read().await.rules.clone()
    }

    /// Get the current L1 initial_prompt string.
    pub async fn get_l1_prompt(&self) -> String {
        self.cache.read().await.l1_prompt.clone()
    }

    /// Check if L1 prompt is available (has content).
    pub async fn has_l1_prompt(&self) -> bool {
        !self.cache.read().await.l1_prompt.is_empty()
    }

    /// Get current snapshot hash.
    pub async fn get_hash(&self) -> String {
        self.cache.read().await.snapshot_hash.clone()
    }

    /// Check if terminology correction is effectively available.
    pub async fn has_rules(&self) -> bool {
        self.cache.read().await.enabled_count > 0
    }
}

/// Full refresh: rebuild cache from all enabled terminology entries in DB.
pub async fn refresh_all_caches(
    pool: &sqlx::SqlitePool,
    cache_state: &TerminologyCacheState,
) -> Result<(), String> {
    use crate::database::repositories::terminology::TerminologyRepository;

    let all = TerminologyRepository::get_all(pool)
        .await
        .map_err(|e| format!("Failed to load terminology: {}", e))?;

    let total_count = all.len();
    let mut rules = Vec::new();
    let mut invalid_ids = Vec::new();

    for entry in &all {
        if entry.enabled == 0 {
            continue;
        }
        match TermRule::build(
            &entry.id,
            &entry.original,
            &entry.replacement,
            &entry.language,
            entry.whole_word != 0,
            entry.case_sensitive != 0,
            &entry.priority,
        ) {
            Ok(rule) => rules.push(rule),
            Err(e) => {
                log::warn!(
                    "Rule build failed for term '{}' (id={}): {}. Marking as invalid.",
                    entry.original, entry.id, e
                );
                invalid_ids.push(entry.id.clone());
            }
        }
    }

    // Sort: high priority first, then by updated_at
    rules.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| a.original.cmp(&b.original))
    });

    let enabled_count = rules.len();
    let enabled_entries: Vec<TerminologyEntry> = all.iter().filter(|e| e.enabled != 0).cloned().collect();
    let snapshot_hash = compute_snapshot_hash(&enabled_entries);

    // Build L1 initial_prompt from high-priority entries
    let (l1_prompt, l1_excluded_ids) = build_initial_prompt(&all);

    let l1_prompt_len = l1_prompt.len();

    // Push L1 prompt to Whisper engine for soft token biasing
    crate::whisper_engine::set_l1_prompt(l1_prompt.clone());

    let cache_ref = cache_state.cache_ref();
    let mut cache = cache_ref.write().await;
    *cache = TerminologyCache {
        rules,
        l1_prompt,
        l1_excluded_ids,
        snapshot_hash,
        total_count,
        enabled_count,
    };

    info!(
        "Terminology cache refreshed: {} total, {} enabled, {} rules compiled, {} invalid, L1 prompt {} chars (hash={})",
        total_count,
        enabled_count,
        cache.rules.len(),
        invalid_ids.len(),
        l1_prompt_len,
        &cache.snapshot_hash[..8]
    );

    Ok(())
}
