-- Madrasa Nexus hub: the hub's own tables. They live in the `nexus` schema (the pool sets
-- the search path); each school's data lives in its own schema and never here.
-- Same rules as the school schema: UUID v7 keys, sync columns, soft delete, append-only audit.

CREATE TABLE hub_users (
    id                 UUID PRIMARY KEY,
    username           TEXT NOT NULL,
    full_name          TEXT NOT NULL,
    email              TEXT,
    password_hash      TEXT NOT NULL,
    -- Permission codes held by this user: nexus.admin, nexus.reports, nexus.escrow.
    permissions        TEXT[] NOT NULL DEFAULT '{}',
    is_active          BOOLEAN NOT NULL DEFAULT TRUE,
    failed_login_count INTEGER NOT NULL DEFAULT 0,
    locked_until       TIMESTAMPTZ,
    last_login_at      TIMESTAMPTZ,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by         UUID,
    deleted_at         TIMESTAMPTZ,
    hlc                TEXT NOT NULL,
    CONSTRAINT hub_users_permissions_check
        CHECK (permissions <@ ARRAY['nexus.admin', 'nexus.reports', 'nexus.escrow'])
);
CREATE UNIQUE INDEX hub_users_username_uq ON hub_users (lower(username)) WHERE deleted_at IS NULL;

CREATE TABLE hub_sessions (
    id           UUID PRIMARY KEY,
    user_id      UUID NOT NULL REFERENCES hub_users(id),
    -- SHA-256 of the bearer token; the token itself is never stored.
    token_hash   TEXT NOT NULL,
    expires_at   TIMESTAMPTZ NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at   TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by   UUID,
    deleted_at   TIMESTAMPTZ,
    hlc          TEXT NOT NULL
);
CREATE UNIQUE INDEX hub_sessions_token_uq ON hub_sessions (token_hash);
CREATE INDEX hub_sessions_user_idx ON hub_sessions (user_id);

CREATE TABLE hub_settings (
    id         UUID PRIMARY KEY,
    key        TEXT NOT NULL UNIQUE,
    value      JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by UUID,
    deleted_at TIMESTAMPTZ,
    hlc        TEXT NOT NULL
);

-- Append-only: who did what on the hub, with the old and new value.
CREATE TABLE hub_audit_log (
    id            UUID PRIMARY KEY,
    actor_user_id UUID,
    action        TEXT NOT NULL,
    entity        TEXT NOT NULL,
    entity_id     UUID,
    old_value     JSONB,
    new_value     JSONB,
    at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    hlc           TEXT NOT NULL
);
CREATE INDEX hub_audit_log_at_idx ON hub_audit_log (at DESC);
CREATE INDEX hub_audit_log_entity_idx ON hub_audit_log (entity, entity_id);

CREATE FUNCTION hub_audit_log_immutable() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'hub_audit_log is append-only';
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER hub_audit_log_no_update BEFORE UPDATE ON hub_audit_log
    FOR EACH ROW EXECUTE FUNCTION hub_audit_log_immutable();
CREATE TRIGGER hub_audit_log_no_delete BEFORE DELETE ON hub_audit_log
    FOR EACH ROW EXECUTE FUNCTION hub_audit_log_immutable();
