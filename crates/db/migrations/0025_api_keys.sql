-- Standard stage 9: read-only API keys. A key is shown once and stored as its SHA-256; every
-- call made with it is logged. Empty on Lite (the read-only API is not served there).

CREATE TABLE api_keys (
    id            UUID PRIMARY KEY,
    school_id     UUID NOT NULL REFERENCES schools (id),
    name          TEXT NOT NULL,
    -- The first characters of the key, for the list.
    key_prefix    TEXT NOT NULL,
    key_hash      TEXT NOT NULL UNIQUE,
    scopes        TEXT[] NOT NULL DEFAULT '{}',
    -- Addresses or CIDR ranges allowed to use the key; empty = any.
    ip_allow      TEXT[] NOT NULL DEFAULT '{}',
    expires_at    TIMESTAMPTZ,
    last_used_at  TIMESTAMPTZ,
    created_by    UUID REFERENCES users (id),
    revoked_at    TIMESTAMPTZ,
    revoked_by    UUID REFERENCES users (id),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by    UUID,
    deleted_at    TIMESTAMPTZ,
    hlc           TEXT NOT NULL
);
CREATE INDEX api_keys_school_idx ON api_keys (school_id) WHERE deleted_at IS NULL;

-- Append-only log of read-only API calls.
CREATE TABLE api_key_calls (
    id          UUID PRIMARY KEY,
    school_id   UUID NOT NULL REFERENCES schools (id),
    key_id      UUID NOT NULL REFERENCES api_keys (id),
    called_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    method      TEXT NOT NULL,
    path        TEXT NOT NULL,
    status      INTEGER NOT NULL,
    ip          TEXT NOT NULL DEFAULT '',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID,
    deleted_at  TIMESTAMPTZ,
    hlc         TEXT NOT NULL
);
CREATE INDEX api_key_calls_key_idx ON api_key_calls (key_id, called_at DESC);
