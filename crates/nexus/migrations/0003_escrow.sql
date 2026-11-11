-- Backup key escrow (stage 6, ADR 0005 §6). The hub's own key pair is a hub setting
-- (`escrow.keys`): the public key in clear, the private key wrapped by the escrow passphrase,
-- which is never stored. These tables hold what schools deposit and every release.

CREATE TABLE hub_escrow_deposits (
    id            UUID PRIMARY KEY,
    hub_school_id UUID NOT NULL REFERENCES hub_schools(id),
    -- Which key this is: the public key derived from it (already public in backup files).
    key_id        TEXT NOT NULL,
    -- The school's backup data key sealed to the hub's escrow public key. NULL once the
    -- deposit has ended: the sealed key itself is erased, the row stays as history.
    sealed        JSONB,
    deposited_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Why the deposit ended: replaced, withdrawn, left, removed or purged.
    ended_reason  TEXT CHECK (ended_reason IN ('replaced', 'withdrawn', 'left', 'removed', 'purged')),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by    UUID,
    deleted_at    TIMESTAMPTZ,
    hlc           TEXT NOT NULL,
    -- A live deposit holds a sealed key; an ended one holds none.
    CONSTRAINT hub_escrow_deposits_sealed_check
        CHECK ((deleted_at IS NULL) = (sealed IS NOT NULL))
);
-- One live deposit for each school.
CREATE UNIQUE INDEX hub_escrow_deposits_live_uq ON hub_escrow_deposits (hub_school_id)
    WHERE deleted_at IS NULL;

CREATE TABLE hub_escrow_releases (
    id               UUID PRIMARY KEY,
    hub_school_id    UUID NOT NULL REFERENCES hub_schools(id),
    deposit_id       UUID NOT NULL REFERENCES hub_escrow_deposits(id),
    key_id           TEXT NOT NULL,
    released_by      UUID NOT NULL REFERENCES hub_users(id),
    -- The releasing user's name as it was, for the school to read.
    released_by_name TEXT NOT NULL,
    reason           TEXT NOT NULL CHECK (btrim(reason) <> ''),
    released_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- When the school confirmed it had recorded the release in its own audit log.
    seen_at          TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by       UUID,
    deleted_at       TIMESTAMPTZ,
    hlc              TEXT NOT NULL
);
CREATE INDEX hub_escrow_releases_school_idx ON hub_escrow_releases (hub_school_id, released_at DESC);
