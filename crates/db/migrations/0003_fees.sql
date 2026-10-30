-- M1 stage 2: fees. Money is NUMERIC(14,2); receipts are immutable (reversal rows only).

CREATE TABLE fee_heads (
    id            UUID PRIMARY KEY,
    school_id     UUID NOT NULL REFERENCES schools (id),
    name          TEXT NOT NULL,
    code          TEXT NOT NULL,
    is_recurring  BOOLEAN NOT NULL DEFAULT TRUE,   -- monthly; FALSE = one-time (admission)
    position      INTEGER NOT NULL DEFAULT 0,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by    UUID,
    deleted_at    TIMESTAMPTZ,
    hlc           TEXT NOT NULL
);
CREATE UNIQUE INDEX fee_heads_code_uq ON fee_heads (school_id, upper(code)) WHERE deleted_at IS NULL;

-- Amount of a fee head for a class in an academic year.
CREATE TABLE fee_structures (
    id                UUID PRIMARY KEY,
    school_id         UUID NOT NULL REFERENCES schools (id),
    academic_year_id  UUID NOT NULL REFERENCES academic_years (id),
    class_id          UUID NOT NULL REFERENCES classes (id),
    fee_head_id       UUID NOT NULL REFERENCES fee_heads (id),
    amount            NUMERIC(14,2) NOT NULL CHECK (amount >= 0),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by        UUID,
    deleted_at        TIMESTAMPTZ,
    hlc               TEXT NOT NULL,
    UNIQUE (academic_year_id, class_id, fee_head_id)
);

-- Concessions other than the sibling discount, with approval.
CREATE TABLE fee_concessions (
    id            UUID PRIMARY KEY,
    school_id     UUID NOT NULL REFERENCES schools (id),
    student_id    UUID NOT NULL REFERENCES students (id),
    kind          TEXT NOT NULL CHECK (kind IN ('staff_child', 'scholarship', 'custom')),
    percent       NUMERIC(5,2) CHECK (percent IS NULL OR (percent >= 0 AND percent <= 100)),
    amount        NUMERIC(14,2) CHECK (amount IS NULL OR amount >= 0),
    reason        TEXT NOT NULL,
    approved_by   UUID NOT NULL REFERENCES users (id),
    approved_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    valid_from    DATE NOT NULL,
    valid_to      DATE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by    UUID,
    deleted_at    TIMESTAMPTZ,
    hlc           TEXT NOT NULL,
    CHECK (percent IS NOT NULL OR amount IS NOT NULL)
);
CREATE INDEX fee_concessions_student_idx ON fee_concessions (student_id);

CREATE TABLE fee_vouchers (
    id                    UUID PRIMARY KEY,
    school_id             UUID NOT NULL REFERENCES schools (id),
    student_id            UUID NOT NULL REFERENCES students (id),
    academic_year_id      UUID NOT NULL REFERENCES academic_years (id),
    section_id            UUID REFERENCES sections (id),
    voucher_no            TEXT NOT NULL,
    billing_month         DATE NOT NULL,              -- first day of the month
    issue_date            DATE NOT NULL,
    due_date              DATE NOT NULL,
    gross                 NUMERIC(14,2) NOT NULL,
    sibling_discount      NUMERIC(14,2) NOT NULL DEFAULT 0,
    concession_discount   NUMERIC(14,2) NOT NULL DEFAULT 0,
    net_monthly           NUMERIC(14,2) NOT NULL,
    arrears               NUMERIC(14,2) NOT NULL DEFAULT 0,
    total_due             NUMERIC(14,2) NOT NULL,
    late_fee              NUMERIC(14,2) NOT NULL DEFAULT 0,  -- assessed late fee
    late_fee_waived       BOOLEAN NOT NULL DEFAULT FALSE,
    waiver_reason         TEXT,
    waived_by             UUID REFERENCES users (id),
    waived_at             TIMESTAMPTZ,
    net_paid              NUMERIC(14,2) NOT NULL DEFAULT 0,  -- Σ receipts incl. reversals
    status                TEXT NOT NULL DEFAULT 'unpaid' CHECK (status IN ('unpaid', 'partial', 'paid', 'carried', 'cancelled')),
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by            UUID,
    deleted_at            TIMESTAMPTZ,
    hlc                   TEXT NOT NULL
);
CREATE UNIQUE INDEX fee_vouchers_no_uq ON fee_vouchers (school_id, voucher_no);
CREATE UNIQUE INDEX fee_vouchers_student_month_uq ON fee_vouchers (student_id, billing_month) WHERE deleted_at IS NULL AND status <> 'cancelled';
CREATE INDEX fee_vouchers_month_idx ON fee_vouchers (school_id, billing_month);
CREATE INDEX fee_vouchers_student_idx ON fee_vouchers (student_id);

CREATE TABLE fee_voucher_lines (
    id            UUID PRIMARY KEY,
    school_id     UUID NOT NULL REFERENCES schools (id),
    voucher_id    UUID NOT NULL REFERENCES fee_vouchers (id),
    fee_head_id   UUID REFERENCES fee_heads (id),
    description   TEXT NOT NULL,
    amount        NUMERIC(14,2) NOT NULL,
    position      INTEGER NOT NULL DEFAULT 0,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by    UUID,
    deleted_at    TIMESTAMPTZ,
    hlc           TEXT NOT NULL
);
CREATE INDEX fee_voucher_lines_voucher_idx ON fee_voucher_lines (voucher_id);

-- Receipts. Append-only: a correction is a new row with a negative amount that references the
-- original (reverses_id). Reprints read the row; nothing is ever edited.
CREATE TABLE fee_receipts (
    id            UUID PRIMARY KEY,
    school_id     UUID NOT NULL REFERENCES schools (id),
    voucher_id    UUID NOT NULL REFERENCES fee_vouchers (id),
    student_id    UUID NOT NULL REFERENCES students (id),
    receipt_no    TEXT NOT NULL,
    paid_on       DATE NOT NULL,
    amount        NUMERIC(14,2) NOT NULL CHECK (amount <> 0),
    method        TEXT NOT NULL CHECK (method IN ('cash', 'bank', 'online', 'one_bill', 'kuickpay')),
    reference     TEXT,
    note          TEXT,
    reverses_id   UUID REFERENCES fee_receipts (id),
    reversal_reason TEXT,
    received_by   UUID NOT NULL REFERENCES users (id),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    hlc           TEXT NOT NULL,
    CHECK ((reverses_id IS NULL AND amount > 0) OR (reverses_id IS NOT NULL AND amount < 0 AND reversal_reason IS NOT NULL))
);
CREATE UNIQUE INDEX fee_receipts_no_uq ON fee_receipts (school_id, receipt_no);
CREATE UNIQUE INDEX fee_receipts_reverses_uq ON fee_receipts (reverses_id) WHERE reverses_id IS NOT NULL;
CREATE INDEX fee_receipts_voucher_idx ON fee_receipts (voucher_id);
CREATE INDEX fee_receipts_paid_on_idx ON fee_receipts (school_id, paid_on);

CREATE FUNCTION fee_receipts_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'fee_receipts are immutable; record a reversal instead';
END;
$$;
CREATE TRIGGER fee_receipts_no_update BEFORE UPDATE ON fee_receipts
    FOR EACH ROW EXECUTE FUNCTION fee_receipts_immutable();
CREATE TRIGGER fee_receipts_no_delete BEFORE DELETE ON fee_receipts
    FOR EACH ROW EXECUTE FUNCTION fee_receipts_immutable();

-- Grant fee permissions to existing system roles.
INSERT INTO permissions (id, school_id, role_id, code, hlc)
SELECT gen_random_uuid(), r.school_id, r.id, p.code, '0000000000000-00000-000000000000'
FROM roles r
JOIN (VALUES ('principal', 'fees.approve'), ('admin', 'fees.manage'), ('admin', 'fees.approve')) AS p(role, code)
  ON p.role = r.code
WHERE r.is_system AND r.deleted_at IS NULL
ON CONFLICT (role_id, code) DO NOTHING;
