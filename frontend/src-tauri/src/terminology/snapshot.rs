use sha2::{Digest, Sha256};

use crate::database::models::TerminologyEntry;

/// Compute a SHA-256 snapshot hash of the enabled terminology entries.
/// This hash is saved alongside transcripts to prove which rule set was active.
pub fn compute_snapshot_hash(entries: &[TerminologyEntry]) -> String {
    let mut hasher = Sha256::new();
    for entry in entries {
        hasher.update(entry.id.as_bytes());
        hasher.update(entry.original.as_bytes());
        hasher.update(entry.replacement.as_bytes());
        hasher.update(entry.language.as_bytes());
        hasher.update(entry.case_sensitive.to_string().as_bytes());
        hasher.update(entry.whole_word.to_string().as_bytes());
        hasher.update(entry.priority.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}
