-- Working offline (ADR 0009).
--
-- A change that may reach the server twice (queued on a LAN client while the school server
-- was out of reach and sent on reconnect, or sent again after a dropped connection) carries an
-- Idempotency-Key. The first answer is kept here and returned for every repeat, so the change
-- is applied exactly once. Kept for 30 days; never synced (the keys belong to this server).
CREATE TABLE idempotency_keys (
    id             UUID PRIMARY KEY,           -- the key itself: a UUID v7 made by the client
    school_id      UUID NOT NULL REFERENCES schools (id),
    user_id        UUID NOT NULL REFERENCES users (id),
    method         TEXT NOT NULL,
    path           TEXT NOT NULL,
    request_hash   TEXT NOT NULL,              -- SHA-256 of method, path and body
    state          TEXT NOT NULL CHECK (state IN ('running', 'done')),
    status         INTEGER,
    response       BYTEA,
    response_type  TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by     UUID,
    deleted_at     TIMESTAMPTZ,
    hlc            TEXT NOT NULL
);
CREATE INDEX idempotency_keys_created_idx ON idempotency_keys (created_at);

-- A receipt taken on a LAN client while the server was out of reach is printed with a
-- provisional number (P-<device>-<n>); the real receipt keeps it so either can be looked up.
ALTER TABLE fee_receipts ADD COLUMN provisional_no TEXT;
CREATE UNIQUE INDEX fee_receipts_provisional_uq ON fee_receipts (school_id, provisional_no)
    WHERE provisional_no IS NOT NULL;
