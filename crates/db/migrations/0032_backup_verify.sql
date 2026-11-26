-- Pilot stage 4: every backup is checked after it is written (restored into a temporary
-- database and counted, and the file read back from disk). The outcome is kept with the run.
ALTER TABLE backup_runs
    ADD COLUMN verify_status TEXT CHECK (verify_status IN ('verified', 'failed', 'skipped')),
    ADD COLUMN verify_message TEXT,
    ADD COLUMN verified_at TIMESTAMPTZ;
