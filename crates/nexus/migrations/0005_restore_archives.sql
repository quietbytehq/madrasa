-- Copies of a school set aside by head office (ADR 0007). When a school restores a
-- backup while joined, its records are older than head office's copy; head office keeps
-- that copy as an archive schema and starts a fresh one from what the school now holds.
-- An archive is read by nobody; it is kept so nothing head office held is lost unnoticed,
-- and it is dropped when the school is purged.

CREATE TABLE hub_school_archives (
    id                UUID PRIMARY KEY,
    hub_school_id     UUID NOT NULL REFERENCES hub_schools(id),
    -- `a_` and 32 hex digits; a schema of the same shape as the school's own.
    schema_name       TEXT NOT NULL,
    -- Why it was set aside: `restored` (the school restored a backup).
    reason            TEXT NOT NULL CHECK (reason IN ('restored')),
    -- When the restored backup was made and when the school restored it, as it said.
    backup_created_at TIMESTAMPTZ,
    restored_at       TIMESTAMPTZ,
    -- Set when the school was purged and the schema dropped.
    dropped_at        TIMESTAMPTZ,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by        UUID,
    deleted_at        TIMESTAMPTZ,
    hlc               TEXT NOT NULL
);
CREATE UNIQUE INDEX hub_school_archives_schema_uq ON hub_school_archives (schema_name);
CREATE INDEX hub_school_archives_school_idx ON hub_school_archives (hub_school_id);
