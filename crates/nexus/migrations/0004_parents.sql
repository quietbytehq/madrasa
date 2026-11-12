-- Parent app accounts (docs/adr/0006-parent-app.md). A parent account belongs to the hub,
-- not to a school: its password never enters a school's schema. It is linked to guardian
-- records of joined schools; only a school can open such a link, with a one-time access
-- code. No school data is stored in these tables.

CREATE TABLE hub_parents (
    id                 UUID PRIMARY KEY,
    -- The account name: a phone number in digits, with a leading + kept.
    phone              TEXT NOT NULL,
    -- The guardian's name when the account was opened; a greeting, nothing more.
    full_name          TEXT NOT NULL,
    password_hash      TEXT NOT NULL,
    failed_login_count INTEGER NOT NULL DEFAULT 0,
    locked_until       TIMESTAMPTZ,
    last_login_at      TIMESTAMPTZ,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by         UUID,
    deleted_at         TIMESTAMPTZ,
    hlc                TEXT NOT NULL
);
CREATE UNIQUE INDEX hub_parents_phone_uq ON hub_parents (phone) WHERE deleted_at IS NULL;

-- An account's access to one guardian record of one school.
CREATE TABLE hub_parent_links (
    id            UUID PRIMARY KEY,
    parent_id     UUID NOT NULL REFERENCES hub_parents(id),
    hub_school_id UUID NOT NULL REFERENCES hub_schools(id),
    -- guardians.id in the school's schema.
    guardian_id   UUID NOT NULL,
    linked_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    ended_at      TIMESTAMPTZ,
    -- Why it ended: the school ended it, a newer code replaced it, or the school left, was
    -- removed or was purged.
    ended_reason  TEXT CHECK (ended_reason IN ('school', 'replaced', 'left', 'removed', 'purged')),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by    UUID,
    deleted_at    TIMESTAMPTZ,
    hlc           TEXT NOT NULL,
    CONSTRAINT hub_parent_links_ended_check CHECK ((ended_at IS NULL) = (ended_reason IS NULL))
);
-- One live link for each guardian record: a guardian's access belongs to one account.
CREATE UNIQUE INDEX hub_parent_links_live_uq ON hub_parent_links (hub_school_id, guardian_id)
    WHERE ended_at IS NULL;
CREATE INDEX hub_parent_links_parent_idx ON hub_parent_links (parent_id) WHERE ended_at IS NULL;

-- One-time access codes issued at a school's request.
CREATE TABLE hub_parent_access_codes (
    id            UUID PRIMARY KEY,
    hub_school_id UUID NOT NULL REFERENCES hub_schools(id),
    guardian_id   UUID NOT NULL,
    guardian_name TEXT NOT NULL,
    -- SHA-256 of the code; the code is shown once to the school and never stored.
    code_hash     TEXT NOT NULL UNIQUE,
    expires_at    TIMESTAMPTZ NOT NULL,
    used_at       TIMESTAMPTZ,
    used_by       UUID REFERENCES hub_parents(id),
    revoked_at    TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by    UUID,
    deleted_at    TIMESTAMPTZ,
    hlc           TEXT NOT NULL
);
CREATE INDEX hub_parent_access_codes_guardian_idx ON hub_parent_access_codes (hub_school_id, guardian_id);

CREATE TABLE hub_parent_sessions (
    id           UUID PRIMARY KEY,
    parent_id    UUID NOT NULL REFERENCES hub_parents(id),
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
CREATE UNIQUE INDEX hub_parent_sessions_token_uq ON hub_parent_sessions (token_hash);
CREATE INDEX hub_parent_sessions_parent_idx ON hub_parent_sessions (parent_id);
