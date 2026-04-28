use crate::database::models::TerminologyEntry;
use log::debug;

/// Maximum Whisper initial_prompt token budget (whisper.cpp hard limit: ~224 tokens).
const MAX_PROMPT_TOKENS: usize = 220;

/// Fallback token estimation (Plan B): character-based conservative estimate.
/// Whisper uses a GPT-2-style BPE tokenizer internally.
/// ja: ~0.8 tokens per char, zh: ~0.9 tokens per char, en: ~0.35 tokens per char.
fn estimate_tokens(text: &str, lang: &str) -> usize {
    match lang {
        "ja" => (text.chars().count() as f64 * 0.8).ceil() as usize,
        "zh" | "zh-CN" | "zh-TW" => (text.chars().count() as f64 * 0.9).ceil() as usize,
        _ => (text.chars().count() as f64 * 0.4).ceil() as usize,
    }
}

/// Build the L1 initial_prompt string from enabled high-priority terminology entries.
/// Returns the prompt string and a list of excluded term IDs (due to token budget).
pub fn build_initial_prompt(entries: &[TerminologyEntry]) -> (String, Vec<String>) {
    if entries.is_empty() {
        return (String::new(), Vec::new());
    }

    // Filter to high-priority, enabled entries sorted by updated_at descending
    let mut high_priority: Vec<&TerminologyEntry> = entries
        .iter()
        .filter(|e| e.enabled != 0 && e.priority == "high")
        .collect();

    high_priority.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

    let mut excluded: Vec<String> = Vec::new();
    let mut token_budget: usize = 0;
    let mut parts: Vec<String> = Vec::new();

    for entry in &high_priority {
        let term_text = format!("{}: {}", entry.original, entry.replacement);
        let tokens = estimate_tokens(&term_text, &entry.language);

        if token_budget + tokens + 1 > MAX_PROMPT_TOKENS {
            excluded.push(entry.id.clone());
            continue;
        }

        token_budget += tokens + 1; // +1 for separator
        parts.push(term_text);
    }

    let prompt = parts.join("\n");

    if !excluded.is_empty() {
        debug!(
            "L1 prompt: {} terms included, {} excluded (token budget: {}/{})",
            parts.len(),
            excluded.len(),
            token_budget,
            MAX_PROMPT_TOKENS
        );
    }

    (prompt, excluded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_japanese_tokens() {
        let text = "ポリウレタン: polyurethane";
        let tokens = estimate_tokens(text, "ja");
        // ポリウレタン(7 chars * 0.8 = 5.6) + ": polyurethane"(16 chars * 0.35 = 5.6) ≈ 12
        assert!(tokens > 5 && tokens < 30);
    }

    #[test]
    fn test_empty_entries() {
        let (prompt, excluded) = build_initial_prompt(&[]);
        assert!(prompt.is_empty());
        assert!(excluded.is_empty());
    }

    #[test]
    fn test_token_budget_respected() {
        // Create many entries that would exceed budget
        let entries: Vec<TerminologyEntry> = (0..500)
            .map(|i| TerminologyEntry {
                id: format!("id-{}", i),
                original: format!("term-{}", i),
                replacement: format!("replacement-{}", i),
                language: "en".to_string(),
                case_sensitive: 0,
                whole_word: 1,
                enabled: 1,
                priority: "high".to_string(),
                category: "general".to_string(),
                description: None,
                source_type: "manual".to_string(),
                package_id: None,
                package_name: None,
                import_batch_id: None,
                created_at: String::new(),
                updated_at: String::new(),
            })
            .collect();

        let (prompt, excluded) = build_initial_prompt(&entries);
        let estimated = estimate_tokens(&prompt, "en");
        assert!(estimated <= MAX_PROMPT_TOKENS, "estimated {} > max {}", estimated, MAX_PROMPT_TOKENS);
        assert!(!excluded.is_empty(), "should have excluded some entries");
    }
}
