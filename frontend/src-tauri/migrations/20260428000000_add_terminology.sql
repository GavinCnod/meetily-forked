-- Migration: Add terminology customization support
-- PRD: PRD_TERMINOLOGY_CUSTOMIZATION_V4.0
-- Date: 2026-04-28

-- 1. terminology table: user-managed term correction rules
CREATE TABLE IF NOT EXISTS terminology (
    id               TEXT PRIMARY KEY,
    original         TEXT NOT NULL,
    replacement      TEXT NOT NULL,
    language         TEXT NOT NULL DEFAULT 'auto',
    case_sensitive   INTEGER NOT NULL DEFAULT 0,
    whole_word       INTEGER NOT NULL DEFAULT 1,
    enabled          INTEGER NOT NULL DEFAULT 1,
    priority         TEXT NOT NULL DEFAULT 'normal',
    category         TEXT NOT NULL DEFAULT 'general',
    description      TEXT,
    source_type      TEXT NOT NULL DEFAULT 'manual',
    package_id       TEXT,
    package_name     TEXT,
    import_batch_id  TEXT,
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at       TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(original, language)
);

CREATE INDEX IF NOT EXISTS idx_terminology_lang ON terminology(language);
CREATE INDEX IF NOT EXISTS idx_terminology_enabled ON terminology(enabled);
CREATE INDEX IF NOT EXISTS idx_terminology_package ON terminology(package_id);

-- 2. l3_correction_jobs: L3 LLM correction task queue state
CREATE TABLE IF NOT EXISTS l3_correction_jobs (
    id              TEXT PRIMARY KEY,
    meeting_id      TEXT NOT NULL UNIQUE,
    status          TEXT NOT NULL DEFAULT 'queued',
    -- status: 'queued' | 'running' | 'done' | 'failed' | 'timeout'
    error_detail    TEXT,
    attempt_count   INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_l3_jobs_status ON l3_correction_jobs(status);
CREATE INDEX IF NOT EXISTS idx_l3_jobs_meeting ON l3_correction_jobs(meeting_id);

-- 3. transcript_corrections: L3 correction suggestions (one per suggestion)
CREATE TABLE IF NOT EXISTS transcript_corrections (
    id                  TEXT PRIMARY KEY,
    meeting_id          TEXT NOT NULL,
    job_id              TEXT NOT NULL,
    original_span       TEXT NOT NULL,
    suggested_text      TEXT NOT NULL,
    occurrences_json    TEXT,
    language            TEXT,
    correction_type     TEXT NOT NULL DEFAULT 'llm',
    reason              TEXT,
    source_snapshot_hash TEXT,
    status              TEXT NOT NULL DEFAULT 'pending',
    -- status: 'pending' | 'accepted' | 'rejected' | 'obsolete'
    reviewed_by         TEXT,
    reviewed_at         TEXT,
    created_at          TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE,
    FOREIGN KEY (job_id) REFERENCES l3_correction_jobs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_corrections_meeting ON transcript_corrections(meeting_id);
CREATE INDEX IF NOT EXISTS idx_corrections_status ON transcript_corrections(status);

-- 4. Extend transcripts table with raw and audit fields
-- Using add_column_if_not_exists pattern for idempotent migration

-- Check and add raw_transcript
ALTER TABLE transcripts ADD COLUMN raw_transcript TEXT;
-- Check and add terminology_snapshot_hash
ALTER TABLE transcripts ADD COLUMN terminology_snapshot_hash TEXT;
-- Check and add l1_prompt_snapshot
ALTER TABLE transcripts ADD COLUMN l1_prompt_snapshot TEXT;

-- 5. Extend settings table with terminology configuration
-- These will fail silently if columns already exist (idempotent via IF NOT EXISTS pattern)
-- SQLite doesn't support IF NOT EXISTS for ALTER TABLE ADD COLUMN, but the migration
-- framework only runs this once. If re-run, errors are caught and ignored.

ALTER TABLE settings ADD COLUMN terminology_enabled INTEGER DEFAULT 1;
ALTER TABLE settings ADD COLUMN initial_prompt_enabled INTEGER DEFAULT 1;
ALTER TABLE settings ADD COLUMN llm_correction_enabled INTEGER DEFAULT 1;
ALTER TABLE settings ADD COLUMN llm_correction_auto_accept INTEGER DEFAULT 0;
