use std::borrow::Cow;
use log::{debug, warn};

/// A compiled L2 correction rule ready for application.
#[derive(Debug, Clone)]
pub struct TermRule {
    pub id: String,
    pub original: String,
    pub replacement: String,
    pub language: String,
    pub whole_word: bool,
    pub case_sensitive: bool,
    pub priority: String,
    /// If true, the regex rule failed to compile and this rule is inactive.
    pub invalid: bool,
    /// Compiled regex pattern
    pattern: String,
}

/// Error when building a regex rule from a terminology entry.
#[derive(Debug)]
pub struct RuleBuildError {
    pub term_id: String,
    pub message: String,
}

impl std::fmt::Display for RuleBuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Rule {}: {}", self.term_id, self.message)
    }
}

impl TermRule {
    /// Build a TermRule from a terminology entry. Returns Err if the regex cannot compile.
    pub fn build(
        id: &str,
        original: &str,
        replacement: &str,
        language: &str,
        whole_word: bool,
        case_sensitive: bool,
        priority: &str,
    ) -> Result<Self, RuleBuildError> {
        let escaped = regex::escape(original);
        let pattern = if whole_word {
            match language {
                "ja" | "zh" | "zh-CN" | "zh-TW" => {
                    // CJK: use look-behind/ahead for whole-word matching
                    // fancy-regex is required here; fall back to substring if unavailable
                    if cfg!(feature = "fancy-regex") {
                        format!(r"(?<!\p{{Katakana}}|\p{{Hiragana}}|\p{{Han}}|\p{{Latin}}|\d){}(?!\p{{Katakana}}|\p{{Hiragana}}|\p{{Han}}|\p{{Latin}}|\d)", escaped)
                    } else {
                        // Fallback: non-whole-word matching for CJK
                        warn!("fancy-regex feature not enabled, CJK whole-word rule '{}' will use substring matching", original);
                        escaped
                    }
                }
                _ => {
                    // English/other: use \b word boundaries
                    format!(r"\b{}\b", escaped)
                }
            }
        } else {
            escaped
        };

        // Verify pattern compiles with regex (and fancy-regex if applicable)
        let _ = regex::Regex::new(&pattern).map_err(|e| RuleBuildError {
            term_id: id.to_string(),
            message: format!("Regex compilation failed: {}", e),
        })?;

        Ok(TermRule {
            id: id.to_string(),
            original: original.to_string(),
            replacement: replacement.to_string(),
            language: language.to_string(),
            whole_word,
            case_sensitive,
            priority: priority.to_string(),
            invalid: false,
            pattern,
        })
    }

    /// Apply this rule to text. Returns true if a replacement was made.
    fn apply(&self, text: &mut String) -> bool {
        let re = match regex::Regex::new(&self.pattern) {
            Ok(r) => r,
            Err(_) => return false,
        };
        let replacement = &self.replacement;
        let result = re.replace_all(text, replacement.as_str());

        if let Cow::Owned(new_text) = result {
            *text = new_text;
            true
        } else {
            // No match - Cow::Borrowed means text unchanged
            if !self.case_sensitive {
                // For case-insensitive, try case-insensitive regex
                let ci_pattern = format!("(?i){}", self.pattern);
                if let Ok(ci_re) = regex::Regex::new(&ci_pattern) {
                    let ci_result = ci_re.replace_all(text, replacement.as_str());
                    if let Cow::Owned(new_text) = ci_result {
                        *text = new_text;
                        return true;
                    }
                }
            }
            false
        }
    }
}

/// Apply all enabled L2 terminology corrections to the raw STT text.
/// Returns the corrected text and the number of corrections applied.
/// Uses Cow<str> - no allocation when no rules match.
pub fn apply_terminology_correction<'a>(text: &'a str, rules: &[TermRule]) -> (Cow<'a, str>, u32) {
    if rules.is_empty() || text.is_empty() {
        return (Cow::Borrowed(text), 0);
    }

    let start = std::time::Instant::now();
    let mut result = text.to_string();
    let mut corrections_applied: u32 = 0;

    for rule in rules {
        if rule.invalid {
            continue;
        }
        if rule.apply(&mut result) {
            corrections_applied += 1;
        }
    }

    let elapsed = start.elapsed();
    if elapsed.as_millis() > 50 {
        debug!(
            "L2 correction took {}ms for {} rules on {} chars ({} corrections)",
            elapsed.as_millis(),
            rules.len(),
            text.chars().count(),
            corrections_applied
        );
    }

    if corrections_applied == 0 {
        (Cow::Borrowed(text), 0)
    } else {
        (Cow::Owned(result), corrections_applied)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_japanese_katakana_replacement() {
        let rule = TermRule::build(
            "t1", "ポリウレタン", "ポリウレタン",
            "ja", false, false, "high"
        ).unwrap();

        let mut text = "ポリウレタンの使用について".to_string();
        assert!(rule.apply(&mut text));
        assert_eq!(text, "ポリウレタンの使用について");
    }

    #[test]
    fn test_english_whole_word() {
        let rule = TermRule::build(
            "t2", "MSDS", "Material Safety Data Sheet",
            "en", true, false, "high"
        ).unwrap();

        let mut text = "Check the MSDS before handling".to_string();
        assert!(rule.apply(&mut text));
        assert_eq!(text, "Check the Material Safety Data Sheet before handling");
    }

    #[test]
    fn test_no_false_positive_on_whole_word() {
        let rule = TermRule::build(
            "t3", "MSDS", "Material Safety Data Sheet",
            "en", true, false, "high"
        ).unwrap();

        let mut text = "MSDS1234 is fake".to_string();
        // With whole_word=true, "MSDS1234" should NOT match "MSDS"
        assert!(!rule.apply(&mut text));
        assert_eq!(text, "MSDS1234 is fake");
    }

    #[test]
    fn test_cow_no_allocation_when_no_match() {
        let rule = TermRule::build(
            "t4", "不存在", "replacement",
            "zh", false, false, "normal"
        ).unwrap();

        let text = "普通的文本没有任何匹配".to_string();
        let (result, count) = apply_terminology_correction(&text, &[rule]);
        assert_eq!(count, 0);
        // Cow::Borrowed means no allocation
        match result {
            Cow::Borrowed(s) => assert_eq!(s, text),
            Cow::Owned(_) => panic!("Should not allocate when no match"),
        }
    }

    #[test]
    fn test_chinese_character_replacement() {
        let rule = TermRule::build(
            "t5", "甲本二亿情酸纸", "甲苯二异氰酸酯",
            "zh", false, false, "high"
        ).unwrap();

        let (result, count) = apply_terminology_correction(
            "这种物质是甲本二亿情酸纸的危险品",
            &[rule],
        );
        assert_eq!(count, 1);
        assert!(result.contains("甲苯二异氰酸酯"));
    }

    #[test]
    fn test_multiple_rules() {
        let rules = vec![
            TermRule::build("r1", "H225", "H225 (极易燃液体)",
                "en", true, false, "high").unwrap(),
            TermRule::build("r2", "TDI", "甲苯二异氰酸酯(TDI)",
                "en", true, false, "high").unwrap(),
        ];

        let (result, count) = apply_terminology_correction(
            "H225 and TDI are hazardous",
            &rules,
        );
        assert_eq!(count, 2);
        assert!(result.contains("H225 (极易燃液体)"));
        assert!(result.contains("甲苯二异氰酸酯(TDI)"));
    }

    #[test]
    fn test_invalid_rule_is_skipped() {
        // Create a rule with an invalid regex pattern manually
        let mut bad_rule = TermRule::build(
            "bad", "ok", "fine", "en", false, false, "normal"
        ).unwrap();
        bad_rule.invalid = true;

        let (result, count) = apply_terminology_correction("ok test", &[bad_rule]);
        assert_eq!(count, 0);
        assert_eq!(result, "ok test");
    }
}
