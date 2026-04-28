use crate::database::models::{L3CorrectionJob, TerminologyEntry, TranscriptCorrection};
use sqlx::SqlitePool;
use tracing::{error, info};

pub struct TerminologyRepository;

impl TerminologyRepository {
    // ── Terminology CRUD ──

    pub async fn get_all(pool: &SqlitePool) -> Result<Vec<TerminologyEntry>, sqlx::Error> {
        sqlx::query_as::<_, TerminologyEntry>(
            "SELECT * FROM terminology ORDER BY priority DESC, updated_at DESC",
        )
        .fetch_all(pool)
        .await
    }

    pub async fn get_enabled(
        pool: &SqlitePool,
    ) -> Result<Vec<TerminologyEntry>, sqlx::Error> {
        sqlx::query_as::<_, TerminologyEntry>(
            "SELECT * FROM terminology WHERE enabled = 1 ORDER BY priority DESC, updated_at DESC",
        )
        .fetch_all(pool)
        .await
    }

    pub async fn get_by_id(
        pool: &SqlitePool,
        id: &str,
    ) -> Result<Option<TerminologyEntry>, sqlx::Error> {
        sqlx::query_as::<_, TerminologyEntry>("SELECT * FROM terminology WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await
    }

    pub async fn upsert(
        pool: &SqlitePool,
        entry: &TerminologyEntry,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO terminology (id, original, replacement, language, case_sensitive, whole_word, enabled, priority, category, description, source_type, package_id, package_name, import_batch_id, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(original, language) DO UPDATE SET
                replacement = excluded.replacement,
                case_sensitive = excluded.case_sensitive,
                whole_word = excluded.whole_word,
                enabled = excluded.enabled,
                priority = excluded.priority,
                category = excluded.category,
                description = excluded.description,
                source_type = excluded.source_type,
                package_id = excluded.package_id,
                package_name = excluded.package_name,
                import_batch_id = excluded.import_batch_id,
                updated_at = excluded.updated_at",
        )
        .bind(&entry.id)
        .bind(&entry.original)
        .bind(&entry.replacement)
        .bind(&entry.language)
        .bind(entry.case_sensitive)
        .bind(entry.whole_word)
        .bind(entry.enabled)
        .bind(&entry.priority)
        .bind(&entry.category)
        .bind(&entry.description)
        .bind(&entry.source_type)
        .bind(&entry.package_id)
        .bind(&entry.package_name)
        .bind(&entry.import_batch_id)
        .bind(&entry.created_at)
        .bind(&entry.updated_at)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn delete(
        pool: &SqlitePool,
        id: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM terminology WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn set_enabled(
        pool: &SqlitePool,
        id: &str,
        enabled: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE terminology SET enabled = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(enabled as i64)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn set_package_enabled(
        pool: &SqlitePool,
        package_id: &str,
        enabled: bool,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE terminology SET enabled = ?, updated_at = datetime('now') WHERE package_id = ?",
        )
        .bind(enabled as i64)
        .bind(package_id)
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn count_by_batch(
        pool: &SqlitePool,
        import_batch_id: &str,
    ) -> Result<i64, sqlx::Error> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM terminology WHERE import_batch_id = ?",
        )
        .bind(import_batch_id)
        .fetch_one(pool)
        .await?;
        Ok(row.0)
    }

    pub async fn delete_by_batch(
        pool: &SqlitePool,
        import_batch_id: &str,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM terminology WHERE import_batch_id = ?")
            .bind(import_batch_id)
            .execute(pool)
            .await?;
        info!(
            "Rolled back import batch {}: {} entries removed",
            import_batch_id,
            result.rows_affected()
        );
        Ok(result.rows_affected())
    }

    // ── L3 Correction Jobs ──

    pub async fn find_active_l3_job(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Option<L3CorrectionJob>, sqlx::Error> {
        sqlx::query_as::<_, L3CorrectionJob>(
            "SELECT * FROM l3_correction_jobs WHERE meeting_id = ? AND status IN ('queued', 'running')",
        )
        .bind(meeting_id)
        .fetch_optional(pool)
        .await
    }

    pub async fn insert_l3_job(
        pool: &SqlitePool,
        id: &str,
        meeting_id: &str,
        status: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO l3_correction_jobs (id, meeting_id, status) VALUES (?, ?, ?)",
        )
        .bind(id)
        .bind(meeting_id)
        .bind(status)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn update_l3_job_status(
        pool: &SqlitePool,
        id: &str,
        status: &str,
        error_detail: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE l3_correction_jobs SET status = ?, error_detail = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(status)
        .bind(error_detail)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn get_l3_job(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Option<L3CorrectionJob>, sqlx::Error> {
        sqlx::query_as::<_, L3CorrectionJob>(
            "SELECT * FROM l3_correction_jobs WHERE meeting_id = ?",
        )
        .bind(meeting_id)
        .fetch_optional(pool)
        .await
    }

    pub async fn get_pending_l3_jobs(
        pool: &SqlitePool,
    ) -> Result<Vec<L3CorrectionJob>, sqlx::Error> {
        sqlx::query_as::<_, L3CorrectionJob>(
            "SELECT * FROM l3_correction_jobs WHERE status IN ('queued', 'running')",
        )
        .fetch_all(pool)
        .await
    }

    pub async fn increment_l3_attempt(
        pool: &SqlitePool,
        id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE l3_correction_jobs SET attempt_count = attempt_count + 1, status = 'queued', updated_at = datetime('now') WHERE id = ?",
        )
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    // ── Transcript Corrections (L3 Suggestions) ──

    pub async fn insert_corrections(
        pool: &SqlitePool,
        corrections: &[TranscriptCorrection],
    ) -> Result<(), sqlx::Error> {
        if corrections.is_empty() {
            return Ok(());
        }
        for c in corrections {
            sqlx::query(
                "INSERT INTO transcript_corrections (id, meeting_id, job_id, original_span, suggested_text, occurrences_json, language, correction_type, reason, source_snapshot_hash, status, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&c.id)
            .bind(&c.meeting_id)
            .bind(&c.job_id)
            .bind(&c.original_span)
            .bind(&c.suggested_text)
            .bind(&c.occurrences_json)
            .bind(&c.language)
            .bind(&c.correction_type)
            .bind(&c.reason)
            .bind(&c.source_snapshot_hash)
            .bind(&c.status)
            .bind(&c.created_at)
            .execute(pool)
            .await?;
        }
        Ok(())
    }

    pub async fn get_corrections_for_meeting(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Vec<TranscriptCorrection>, sqlx::Error> {
        sqlx::query_as::<_, TranscriptCorrection>(
            "SELECT * FROM transcript_corrections WHERE meeting_id = ? ORDER BY created_at",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await
    }

    pub async fn get_pending_corrections(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Vec<TranscriptCorrection>, sqlx::Error> {
        sqlx::query_as::<_, TranscriptCorrection>(
            "SELECT * FROM transcript_corrections WHERE meeting_id = ? AND status = 'pending' ORDER BY created_at",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await
    }

    pub async fn update_correction_status(
        pool: &SqlitePool,
        id: &str,
        status: &str,
        reviewed_by: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE transcript_corrections SET status = ?, reviewed_by = ?, reviewed_at = datetime('now') WHERE id = ?",
        )
        .bind(status)
        .bind(reviewed_by)
        .bind(id)
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn accept_corrections_for_term(
        pool: &SqlitePool,
        meeting_id: &str,
        original_span: &str,
        reviewed_by: Option<&str>,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE transcript_corrections SET status = 'accepted', reviewed_by = ?, reviewed_at = datetime('now') WHERE meeting_id = ? AND original_span = ? AND status = 'pending'",
        )
        .bind(reviewed_by)
        .bind(meeting_id)
        .bind(original_span)
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn obsoletize_corrections(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE transcript_corrections SET status = 'obsolete' WHERE meeting_id = ? AND status = 'pending'",
        )
        .bind(meeting_id)
        .execute(pool)
        .await?;
        Ok(result.rows_affected())
    }
}
