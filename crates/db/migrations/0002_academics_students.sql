-- M1 stage 1: academic structure, staff, students, guardians.

CREATE TABLE academic_years (
    id          UUID PRIMARY KEY,
    school_id   UUID NOT NULL REFERENCES schools (id),
    name        TEXT NOT NULL,
    start_date  DATE NOT NULL,
    end_date    DATE NOT NULL CHECK (end_date > start_date),
    start_month SMALLINT NOT NULL CHECK (start_month IN (4, 8)),
    is_active   BOOLEAN NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID,
    deleted_at  TIMESTAMPTZ,
    hlc         TEXT NOT NULL
);
CREATE UNIQUE INDEX academic_years_name_uq ON academic_years (school_id, name) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX academic_years_active_uq ON academic_years (school_id) WHERE is_active AND deleted_at IS NULL;

CREATE TABLE classes (
    id          UUID PRIMARY KEY,
    school_id   UUID NOT NULL REFERENCES schools (id),
    name        TEXT NOT NULL,
    level       INTEGER NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID,
    deleted_at  TIMESTAMPTZ,
    hlc         TEXT NOT NULL
);
CREATE UNIQUE INDEX classes_name_uq ON classes (school_id, lower(name)) WHERE deleted_at IS NULL;

CREATE TABLE subjects (
    id          UUID PRIMARY KEY,
    school_id   UUID NOT NULL REFERENCES schools (id),
    name        TEXT NOT NULL,
    code        TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID,
    deleted_at  TIMESTAMPTZ,
    hlc         TEXT NOT NULL
);
CREATE UNIQUE INDEX subjects_code_uq ON subjects (school_id, upper(code)) WHERE deleted_at IS NULL;

CREATE TABLE class_subjects (
    id          UUID PRIMARY KEY,
    school_id   UUID NOT NULL REFERENCES schools (id),
    class_id    UUID NOT NULL REFERENCES classes (id),
    subject_id  UUID NOT NULL REFERENCES subjects (id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID,
    deleted_at  TIMESTAMPTZ,
    hlc         TEXT NOT NULL,
    UNIQUE (class_id, subject_id)
);

CREATE TABLE staff (
    id            UUID PRIMARY KEY,
    school_id     UUID NOT NULL REFERENCES schools (id),
    user_id       UUID REFERENCES users (id),
    staff_number  TEXT NOT NULL,
    full_name     TEXT NOT NULL,
    father_name   TEXT NOT NULL DEFAULT '',
    gender        TEXT NOT NULL DEFAULT 'other' CHECK (gender IN ('male', 'female', 'other')),
    phone         TEXT,
    email         TEXT,
    cnic          TEXT,
    designation   TEXT NOT NULL DEFAULT '',
    joined_on     DATE,
    status        TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'left')),
    custom        JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by    UUID,
    deleted_at    TIMESTAMPTZ,
    hlc           TEXT NOT NULL
);
CREATE UNIQUE INDEX staff_number_uq ON staff (school_id, upper(staff_number)) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX staff_user_uq ON staff (user_id) WHERE user_id IS NOT NULL AND deleted_at IS NULL;

CREATE TABLE sections (
    id                UUID PRIMARY KEY,
    school_id         UUID NOT NULL REFERENCES schools (id),
    class_id          UUID NOT NULL REFERENCES classes (id),
    academic_year_id  UUID NOT NULL REFERENCES academic_years (id),
    name              TEXT NOT NULL,
    class_teacher_id  UUID REFERENCES staff (id),
    capacity          INTEGER,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by        UUID,
    deleted_at        TIMESTAMPTZ,
    hlc               TEXT NOT NULL
);
CREATE UNIQUE INDEX sections_name_uq ON sections (class_id, academic_year_id, upper(name)) WHERE deleted_at IS NULL;
CREATE INDEX sections_year_idx ON sections (academic_year_id);

-- Subject-teaching assignments. Class-teacher duty is sections.class_teacher_id.
CREATE TABLE section_teachers (
    id          UUID PRIMARY KEY,
    school_id   UUID NOT NULL REFERENCES schools (id),
    staff_id    UUID NOT NULL REFERENCES staff (id),
    section_id  UUID NOT NULL REFERENCES sections (id),
    subject_id  UUID REFERENCES subjects (id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID,
    deleted_at  TIMESTAMPTZ,
    hlc         TEXT NOT NULL,
    UNIQUE NULLS NOT DISTINCT (staff_id, section_id, subject_id)
);
CREATE INDEX section_teachers_staff_idx ON section_teachers (staff_id);

CREATE TABLE students (
    id                    UUID PRIMARY KEY,
    school_id             UUID NOT NULL REFERENCES schools (id),
    student_number        TEXT NOT NULL,
    legacy_id             TEXT,
    full_name             TEXT NOT NULL,
    father_name           TEXT NOT NULL,
    date_of_birth         DATE NOT NULL,
    gender                TEXT NOT NULL CHECK (gender IN ('male', 'female', 'other')),
    cnic                  TEXT,
    address               TEXT NOT NULL DEFAULT '',
    admission_date        DATE NOT NULL,
    status                TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'left', 'graduated')),
    left_on               DATE,
    left_reason           TEXT,
    board_roll_no         TEXT,
    board_registration_no TEXT,
    photo                 BYTEA,
    photo_mime            TEXT,
    custom                JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by            UUID,
    deleted_at            TIMESTAMPTZ,
    hlc                   TEXT NOT NULL
);
CREATE UNIQUE INDEX students_number_uq ON students (school_id, upper(student_number)) WHERE deleted_at IS NULL;
CREATE INDEX students_school_status_idx ON students (school_id, status);
CREATE INDEX students_cnic_idx ON students (school_id, cnic) WHERE cnic IS NOT NULL;
CREATE INDEX students_name_idx ON students (school_id, lower(full_name));

CREATE TABLE enrollments (
    id                UUID PRIMARY KEY,
    school_id         UUID NOT NULL REFERENCES schools (id),
    student_id        UUID NOT NULL REFERENCES students (id),
    academic_year_id  UUID NOT NULL REFERENCES academic_years (id),
    section_id        UUID NOT NULL REFERENCES sections (id),
    roll_no           INTEGER,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by        UUID,
    deleted_at        TIMESTAMPTZ,
    hlc               TEXT NOT NULL
);
CREATE UNIQUE INDEX enrollments_student_year_uq ON enrollments (student_id, academic_year_id) WHERE deleted_at IS NULL;
CREATE INDEX enrollments_section_idx ON enrollments (section_id);

CREATE TABLE guardians (
    id          UUID PRIMARY KEY,
    school_id   UUID NOT NULL REFERENCES schools (id),
    full_name   TEXT NOT NULL,
    phone       TEXT,
    email       TEXT,
    cnic        TEXT,
    occupation  TEXT NOT NULL DEFAULT '',
    address     TEXT NOT NULL DEFAULT '',
    custom      JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID,
    deleted_at  TIMESTAMPTZ,
    hlc         TEXT NOT NULL
);
CREATE INDEX guardians_phone_idx ON guardians (school_id, phone) WHERE phone IS NOT NULL;
CREATE INDEX guardians_cnic_idx ON guardians (school_id, cnic) WHERE cnic IS NOT NULL;

CREATE TABLE student_guardians (
    id            UUID PRIMARY KEY,
    school_id     UUID NOT NULL REFERENCES schools (id),
    student_id    UUID NOT NULL REFERENCES students (id),
    guardian_id   UUID NOT NULL REFERENCES guardians (id),
    relationship  TEXT NOT NULL CHECK (relationship IN ('father', 'mother', 'guardian', 'other')),
    is_primary    BOOLEAN NOT NULL DEFAULT FALSE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by    UUID,
    deleted_at    TIMESTAMPTZ,
    hlc           TEXT NOT NULL,
    UNIQUE (student_id, guardian_id)
);
CREATE INDEX student_guardians_guardian_idx ON student_guardians (guardian_id);

-- Per-school counters (student numbers etc.), incremented under a row lock.
CREATE TABLE school_sequences (
    id          UUID PRIMARY KEY,
    school_id   UUID NOT NULL REFERENCES schools (id),
    key         TEXT NOT NULL,
    next_value  INTEGER NOT NULL DEFAULT 1,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID,
    deleted_at  TIMESTAMPTZ,
    hlc         TEXT NOT NULL,
    UNIQUE (school_id, key)
);

-- Grant the new academics permissions to existing system roles.
INSERT INTO permissions (id, school_id, role_id, code, hlc)
SELECT gen_random_uuid(), r.school_id, r.id, p.code, '0000000000000-00000-000000000000'
FROM roles r
JOIN (VALUES ('principal', 'academics.read'), ('principal', 'academics.manage'),
             ('admin', 'academics.read'), ('admin', 'academics.manage'),
             ('accountant', 'academics.read'), ('teacher', 'academics.read'), ('teacher', 'teachers.read')) AS p(role, code)
  ON p.role = r.code
WHERE r.is_system AND r.deleted_at IS NULL
ON CONFLICT (role_id, code) DO NOTHING;
