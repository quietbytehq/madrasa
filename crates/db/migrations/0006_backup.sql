-- M1 stage 5: encrypted backup and restore. Backups and restores are audited with their own
-- actions; every run (manual or scheduled) is recorded for the dashboard status.

ALTER TABLE audit_log DROP CONSTRAINT IF EXISTS audit_log_action_check;
ALTER TABLE audit_log ADD CONSTRAINT audit_log_action_check
    CHECK (action IN ('create', 'update', 'delete', 'login', 'login_failed', 'lockout', 'logout',
                      'backup', 'restore', 'backup_password_failed'));

CREATE TABLE backup_runs (
    id           UUID PRIMARY KEY,
    school_id    UUID NOT NULL REFERENCES schools (id),
    trigger      TEXT NOT NULL CHECK (trigger IN ('manual', 'scheduled')),
    status       TEXT NOT NULL CHECK (status IN ('running', 'ok', 'failed')),
    started_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at  TIMESTAMPTZ,
    file_path    TEXT,
    file_size    BIGINT,
    error        TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by   UUID,
    deleted_at   TIMESTAMPTZ,
    hlc          TEXT NOT NULL
);
CREATE INDEX backup_runs_school_started_idx ON backup_runs (school_id, started_at DESC);
