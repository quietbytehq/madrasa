-- The school register and invitations (stage 4). Each joined school has a schema of its own
-- next to this one; `schema_name` says which. No school data is stored in these tables.

CREATE TABLE hub_schools (
    id             UUID PRIMARY KEY,
    -- The school's own id (schools.id in its schema).
    school_id      UUID NOT NULL,
    name           TEXT NOT NULL,
    code           TEXT NOT NULL,
    -- t_<32 hex digits>; empty once the data has been purged.
    schema_name    TEXT NOT NULL,
    -- joining: the schema is being created. active: syncing. left: the school left.
    -- removed: head office removed it. purged: its data has been deleted.
    status         TEXT NOT NULL
        CHECK (status IN ('joining', 'active', 'left', 'removed', 'purged')),
    -- SHA-256 of the school's sync token; the token itself is never stored.
    token_hash     TEXT,
    school_version TEXT,
    school_schema  BIGINT,
    edition        TEXT,
    joined_at      TIMESTAMPTZ,
    last_sync_at   TIMESTAMPTZ,
    left_at        TIMESTAMPTZ,
    leave_reason   TEXT,
    purged_at      TIMESTAMPTZ,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by     UUID,
    deleted_at     TIMESTAMPTZ,
    hlc            TEXT NOT NULL,
    CONSTRAINT hub_schools_schema_check
        CHECK (schema_name = '' OR schema_name ~ '^t_[0-9a-f]{32}$')
);
CREATE UNIQUE INDEX hub_schools_schema_uq ON hub_schools (schema_name) WHERE schema_name <> '';
CREATE UNIQUE INDEX hub_schools_token_uq ON hub_schools (token_hash) WHERE token_hash IS NOT NULL;

CREATE TABLE hub_invitations (
    id          UUID PRIMARY KEY,
    school_name TEXT NOT NULL,
    note        TEXT,
    -- SHA-256 of the join code; the code is shown once when the invitation is created.
    code_hash   TEXT NOT NULL UNIQUE,
    expires_at  TIMESTAMPTZ NOT NULL,
    used_at     TIMESTAMPTZ,
    used_by     UUID REFERENCES hub_schools(id),
    revoked_at  TIMESTAMPTZ,
    created_by  UUID REFERENCES hub_users(id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID,
    deleted_at  TIMESTAMPTZ,
    hlc         TEXT NOT NULL
);
