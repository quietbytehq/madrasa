-- Nexus sync, stage 4: when this copy last synced and how it went.
--
-- Kept in the sync bookkeeping, not in settings: a setting change is audited, the audit row
-- is itself a record to sync, and every sync would then leave one more record to send.
ALTER TABLE sync_state
    ADD COLUMN last_sync_at TIMESTAMPTZ,
    -- Why the last attempt failed, in plain words; NULL after a successful sync.
    ADD COLUMN last_error   TEXT,
    -- What the last successful round did (madrasa_db::sync::round::RoundReport).
    ADD COLUMN last_report  JSONB;
