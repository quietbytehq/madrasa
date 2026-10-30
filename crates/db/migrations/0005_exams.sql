-- M1 stage 4: exams. Assessment types per academic year with weights, maximum marks per
-- class and subject, marks sheets (one subject of one section per assessment) with a
-- draft → submitted → approved workflow, and marks per student. Marks are NUMERIC(6,2).

CREATE TABLE assessment_types (
    id                UUID PRIMARY KEY,
    school_id         UUID NOT NULL REFERENCES schools (id),
    academic_year_id  UUID NOT NULL REFERENCES academic_years (id),
    name              TEXT NOT NULL,
    position          INTEGER NOT NULL DEFAULT 0,
    weight            NUMERIC(5,2) NOT NULL DEFAULT 0 CHECK (weight >= 0 AND weight <= 100),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by        UUID,
    deleted_at        TIMESTAMPTZ,
    hlc               TEXT NOT NULL
);
CREATE UNIQUE INDEX assessment_types_name_uq ON assessment_types (academic_year_id, lower(name)) WHERE deleted_at IS NULL;

CREATE TABLE assessment_max_marks (
    id                  UUID PRIMARY KEY,
    school_id           UUID NOT NULL REFERENCES schools (id),
    assessment_type_id  UUID NOT NULL REFERENCES assessment_types (id),
    class_id            UUID NOT NULL REFERENCES classes (id),
    subject_id          UUID NOT NULL REFERENCES subjects (id),
    max_marks           NUMERIC(6,2) NOT NULL CHECK (max_marks > 0),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by          UUID,
    deleted_at          TIMESTAMPTZ,
    hlc                 TEXT NOT NULL
);
CREATE UNIQUE INDEX assessment_max_marks_uq ON assessment_max_marks (assessment_type_id, class_id, subject_id) WHERE deleted_at IS NULL;

CREATE TABLE mark_sheets (
    id                  UUID PRIMARY KEY,
    school_id           UUID NOT NULL REFERENCES schools (id),
    assessment_type_id  UUID NOT NULL REFERENCES assessment_types (id),
    section_id          UUID NOT NULL REFERENCES sections (id),
    subject_id          UUID NOT NULL REFERENCES subjects (id),
    status              TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'submitted', 'approved')),
    submitted_by        UUID REFERENCES users (id),
    submitted_at        TIMESTAMPTZ,
    approved_by         UUID REFERENCES users (id),
    approved_at         TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by          UUID,
    deleted_at          TIMESTAMPTZ,
    hlc                 TEXT NOT NULL
);
CREATE UNIQUE INDEX mark_sheets_uq ON mark_sheets (assessment_type_id, section_id, subject_id) WHERE deleted_at IS NULL;
CREATE INDEX mark_sheets_section_idx ON mark_sheets (section_id) WHERE deleted_at IS NULL;

CREATE TABLE marks (
    id          UUID PRIMARY KEY,
    school_id   UUID NOT NULL REFERENCES schools (id),
    sheet_id    UUID NOT NULL REFERENCES mark_sheets (id),
    student_id  UUID NOT NULL REFERENCES students (id),
    obtained    NUMERIC(6,2) CHECK (obtained IS NULL OR obtained >= 0),
    is_absent   BOOLEAN NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID,
    deleted_at  TIMESTAMPTZ,
    hlc         TEXT NOT NULL
);
CREATE UNIQUE INDEX marks_sheet_student_uq ON marks (sheet_id, student_id) WHERE deleted_at IS NULL;
CREATE INDEX marks_student_idx ON marks (student_id) WHERE deleted_at IS NULL;

-- Approval of marks sheets: principal.
INSERT INTO permissions (id, school_id, role_id, code, hlc)
SELECT gen_random_uuid(), r.school_id, r.id, 'exams.approve', '0000000000000-00000-000000000000'
FROM roles r
WHERE r.code = 'principal' AND r.is_system AND r.deleted_at IS NULL
ON CONFLICT (role_id, code) DO NOTHING;
