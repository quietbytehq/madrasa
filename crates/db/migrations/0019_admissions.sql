-- Standard stage 3: admissions pipeline. Configurable stages per school, applications with
-- the applicant's details, and a dated history of every stage change. Empty on Lite.

CREATE TABLE admission_stages (
    id          UUID PRIMARY KEY,
    school_id   UUID NOT NULL REFERENCES schools (id),
    code        TEXT NOT NULL,
    name        TEXT NOT NULL,
    kind        TEXT NOT NULL DEFAULT 'open' CHECK (kind IN ('open', 'admitted', 'rejected', 'withdrawn')),
    position    INTEGER NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID,
    deleted_at  TIMESTAMPTZ,
    hlc         TEXT NOT NULL
);
CREATE UNIQUE INDEX admission_stages_code_uq ON admission_stages (school_id, code) WHERE deleted_at IS NULL;

CREATE TABLE admission_applications (
    id                     UUID PRIMARY KEY,
    school_id              UUID NOT NULL REFERENCES schools (id),
    application_no         TEXT NOT NULL,
    academic_year_id       UUID NOT NULL REFERENCES academic_years (id),
    class_id               UUID REFERENCES classes (id),
    stage_id               UUID NOT NULL REFERENCES admission_stages (id),
    enquiry_date           DATE NOT NULL,
    source                 TEXT NOT NULL DEFAULT '',
    -- Applicant (same shape as a student).
    full_name              TEXT NOT NULL,
    father_name            TEXT NOT NULL DEFAULT '',
    date_of_birth          DATE,
    gender                 TEXT NOT NULL DEFAULT '' CHECK (gender IN ('', 'male', 'female', 'other')),
    cnic                   TEXT,
    address                TEXT NOT NULL DEFAULT '',
    phone                  TEXT,
    email                  TEXT,
    previous_school        TEXT NOT NULL DEFAULT '',
    -- Guardian.
    guardian_name          TEXT NOT NULL DEFAULT '',
    guardian_relationship  TEXT NOT NULL DEFAULT 'father' CHECK (guardian_relationship IN ('father', 'mother', 'guardian', 'other')),
    guardian_phone         TEXT,
    guardian_cnic          TEXT,
    guardian_email         TEXT,
    guardian_occupation    TEXT NOT NULL DEFAULT '',
    -- Test and interview.
    test_date              DATE,
    test_marks             NUMERIC(6,2),
    test_total             NUMERIC(6,2),
    interview_date         DATE,
    interview_notes        TEXT NOT NULL DEFAULT '',
    -- Offer.
    offer_date             DATE,
    offer_fee              NUMERIC(14,2),
    offer_expires          DATE,
    notes                  TEXT NOT NULL DEFAULT '',
    custom                 JSONB NOT NULL DEFAULT '{}'::jsonb,
    -- Set when the application was converted into a student.
    student_id             UUID REFERENCES students (id),
    admitted_on            DATE,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by             UUID,
    deleted_at             TIMESTAMPTZ,
    hlc                    TEXT NOT NULL
);
CREATE UNIQUE INDEX admission_applications_no_uq ON admission_applications (school_id, application_no) WHERE deleted_at IS NULL;
CREATE INDEX admission_applications_year_stage_idx ON admission_applications (academic_year_id, stage_id) WHERE deleted_at IS NULL;
CREATE INDEX admission_applications_name_idx ON admission_applications (school_id, lower(full_name)) WHERE deleted_at IS NULL;

-- Every stage change, dated (the application's creation is the first row).
CREATE TABLE admission_events (
    id              UUID PRIMARY KEY,
    school_id       UUID NOT NULL REFERENCES schools (id),
    application_id  UUID NOT NULL REFERENCES admission_applications (id),
    from_stage_id   UUID REFERENCES admission_stages (id),
    to_stage_id     UUID NOT NULL REFERENCES admission_stages (id),
    note            TEXT NOT NULL DEFAULT '',
    changed_by      UUID REFERENCES users (id),
    changed_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by      UUID,
    deleted_at      TIMESTAMPTZ,
    hlc             TEXT NOT NULL
);
CREATE INDEX admission_events_application_idx ON admission_events (application_id, changed_at);

-- Grants for existing Standard installs; Lite roles receive them when the edition is switched.
INSERT INTO permissions (id, school_id, role_id, code, hlc)
SELECT gen_random_uuid(), r.school_id, r.id, p.code, '0000000000000-00000-000000000000'
FROM roles r
JOIN (VALUES ('principal', 'admissions.read'), ('principal', 'admissions.manage'),
             ('admin', 'admissions.read'), ('admin', 'admissions.manage'),
             ('accountant', 'admissions.read')) AS p(role, code)
  ON p.role = r.code
WHERE r.is_system AND r.deleted_at IS NULL
  AND EXISTS (SELECT 1 FROM settings s WHERE s.school_id IS NULL AND s.key = 'edition'
              AND s.deleted_at IS NULL AND s.value::text IN ('"standard"', '"nexus"'))
ON CONFLICT (role_id, code) DO NOTHING;

-- Enquiry imports go through the import engine.
ALTER TABLE import_batches DROP CONSTRAINT IF EXISTS import_batches_kind_check;
ALTER TABLE import_batches ADD CONSTRAINT import_batches_kind_check
    CHECK (kind IN ('students', 'staff', 'opening_balances', 'board_results', 'enquiries'));
