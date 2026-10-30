-- M1 stage 6: Excel/CSV import batches. Every applied import is a batch that can be rolled
-- back in one click: created rows are soft-deleted, updated rows get their old values back.

CREATE TABLE import_batches (
    id             UUID PRIMARY KEY,
    school_id      UUID NOT NULL REFERENCES schools (id),
    kind           TEXT NOT NULL CHECK (kind IN ('students', 'staff', 'opening_balances')),
    file_name      TEXT NOT NULL,
    status         TEXT NOT NULL CHECK (status IN ('applied', 'rolled_back')),
    options        JSONB NOT NULL DEFAULT '{}'::jsonb,
    total_rows     INTEGER NOT NULL DEFAULT 0,
    created_rows   INTEGER NOT NULL DEFAULT 0,
    updated_rows   INTEGER NOT NULL DEFAULT 0,
    skipped_rows   INTEGER NOT NULL DEFAULT 0,
    applied_by     UUID REFERENCES users (id),
    applied_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    rolled_back_at TIMESTAMPTZ,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by     UUID,
    deleted_at     TIMESTAMPTZ,
    hlc            TEXT NOT NULL
);
CREATE INDEX import_batches_school_idx ON import_batches (school_id, applied_at DESC);

CREATE TABLE import_batch_rows (
    id          UUID PRIMARY KEY,
    school_id   UUID NOT NULL REFERENCES schools (id),
    batch_id    UUID NOT NULL REFERENCES import_batches (id),
    row_no      INTEGER NOT NULL,
    action      TEXT NOT NULL CHECK (action IN ('created', 'updated', 'skipped')),
    entity      TEXT NOT NULL,   -- student | guardian | staff | fee_voucher | student_guardian | enrollment
    entity_id   UUID,
    old_value   JSONB,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID,
    deleted_at  TIMESTAMPTZ,
    hlc         TEXT NOT NULL
);
CREATE INDEX import_batch_rows_batch_idx ON import_batch_rows (batch_id, row_no);
