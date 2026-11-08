-- Standard stage 4: library. Catalogue (books with copies), loans to students and staff
-- with renewals, fines and waivers. Loan rules live in the settings (`library.rules`).
-- Empty on Lite.

CREATE TABLE library_books (
    id          UUID PRIMARY KEY,
    school_id   UUID NOT NULL REFERENCES schools (id),
    title       TEXT NOT NULL,
    author      TEXT NOT NULL DEFAULT '',
    publisher   TEXT NOT NULL DEFAULT '',
    isbn        TEXT NOT NULL DEFAULT '',
    category    TEXT NOT NULL DEFAULT '',
    language    TEXT NOT NULL DEFAULT '',
    subject     TEXT NOT NULL DEFAULT '',
    year        INTEGER,
    custom      JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID,
    deleted_at  TIMESTAMPTZ,
    hlc         TEXT NOT NULL
);
CREATE INDEX library_books_title_idx ON library_books (school_id, lower(title)) WHERE deleted_at IS NULL;

CREATE TABLE library_copies (
    id            UUID PRIMARY KEY,
    school_id     UUID NOT NULL REFERENCES schools (id),
    book_id       UUID NOT NULL REFERENCES library_books (id),
    accession_no  TEXT NOT NULL,
    shelf         TEXT NOT NULL DEFAULT '',
    condition     TEXT NOT NULL DEFAULT 'good' CHECK (condition IN ('good', 'fair', 'poor')),
    status        TEXT NOT NULL DEFAULT 'available' CHECK (status IN ('available', 'on_loan', 'lost', 'withdrawn')),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by    UUID,
    deleted_at    TIMESTAMPTZ,
    hlc           TEXT NOT NULL
);
CREATE UNIQUE INDEX library_copies_accession_uq ON library_copies (school_id, upper(accession_no)) WHERE deleted_at IS NULL;
CREATE INDEX library_copies_book_idx ON library_copies (book_id) WHERE deleted_at IS NULL;

CREATE TABLE library_loans (
    id              UUID PRIMARY KEY,
    school_id       UUID NOT NULL REFERENCES schools (id),
    copy_id         UUID NOT NULL REFERENCES library_copies (id),
    member_kind     TEXT NOT NULL CHECK (member_kind IN ('student', 'staff')),
    member_id       UUID NOT NULL,
    issued_on       DATE NOT NULL,
    due_on          DATE NOT NULL,
    returned_on     DATE,
    renewals        INTEGER NOT NULL DEFAULT 0,
    issued_by       UUID REFERENCES users (id),
    returned_by     UUID REFERENCES users (id),
    -- The copy was reported lost at return; the fine then holds the replacement charge.
    lost            BOOLEAN NOT NULL DEFAULT false,
    fine            NUMERIC(14,2) NOT NULL DEFAULT 0,
    fine_waived     NUMERIC(14,2) NOT NULL DEFAULT 0,
    waiver_reason   TEXT NOT NULL DEFAULT '',
    waived_by       UUID REFERENCES users (id),
    -- Students: the one-off voucher the fine was posted to. Staff: settled by hand.
    fine_voucher_id UUID REFERENCES fee_vouchers (id),
    fine_settled_on DATE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by      UUID,
    deleted_at      TIMESTAMPTZ,
    hlc             TEXT NOT NULL
);
CREATE UNIQUE INDEX library_loans_open_copy_uq ON library_loans (copy_id) WHERE returned_on IS NULL AND deleted_at IS NULL;
CREATE INDEX library_loans_member_idx ON library_loans (member_kind, member_id, returned_on) WHERE deleted_at IS NULL;
CREATE INDEX library_loans_due_idx ON library_loans (school_id, due_on) WHERE returned_on IS NULL AND deleted_at IS NULL;

-- Grants for existing Standard installs; Lite roles receive them when the edition is switched.
INSERT INTO permissions (id, school_id, role_id, code, hlc)
SELECT gen_random_uuid(), r.school_id, r.id, p.code, '0000000000000-00000-000000000000'
FROM roles r
JOIN (VALUES ('principal', 'library.read'), ('principal', 'library.manage'),
             ('admin', 'library.read'), ('admin', 'library.manage'),
             ('accountant', 'library.read'), ('teacher', 'library.read')) AS p(role, code)
  ON p.role = r.code
WHERE r.is_system AND r.deleted_at IS NULL
  AND EXISTS (SELECT 1 FROM settings s WHERE s.school_id IS NULL AND s.key = 'edition'
              AND s.deleted_at IS NULL AND s.value::text IN ('"standard"', '"nexus"'))
ON CONFLICT (role_id, code) DO NOTHING;
