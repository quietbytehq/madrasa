-- M2 stage 6: subject allocation per student.
-- A class subject is compulsory unless marked elective; subject groups name a set of
-- electives per class; a student's own subjects live on the enrolment. Absent any
-- allocation a student takes every class subject, so existing schools see no change.

ALTER TABLE class_subjects ADD COLUMN is_elective BOOLEAN NOT NULL DEFAULT FALSE;

CREATE TABLE subject_groups (
    id          UUID PRIMARY KEY,
    school_id   UUID NOT NULL REFERENCES schools (id),
    class_id    UUID NOT NULL REFERENCES classes (id),
    name        TEXT NOT NULL,
    position    INTEGER NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID,
    deleted_at  TIMESTAMPTZ,
    hlc         TEXT NOT NULL
);
CREATE UNIQUE INDEX subject_groups_name_uq ON subject_groups (class_id, lower(name)) WHERE deleted_at IS NULL;

CREATE TABLE subject_group_subjects (
    id          UUID PRIMARY KEY,
    school_id   UUID NOT NULL REFERENCES schools (id),
    group_id    UUID NOT NULL REFERENCES subject_groups (id),
    subject_id  UUID NOT NULL REFERENCES subjects (id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID,
    deleted_at  TIMESTAMPTZ,
    hlc         TEXT NOT NULL,
    UNIQUE (group_id, subject_id)
);

CREATE TABLE student_subjects (
    id             UUID PRIMARY KEY,
    school_id      UUID NOT NULL REFERENCES schools (id),
    enrollment_id  UUID NOT NULL REFERENCES enrollments (id),
    subject_id     UUID NOT NULL REFERENCES subjects (id),
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by     UUID,
    deleted_at     TIMESTAMPTZ,
    hlc            TEXT NOT NULL,
    UNIQUE (enrollment_id, subject_id)
);
CREATE INDEX student_subjects_enrollment_idx ON student_subjects (enrollment_id);

ALTER TABLE enrollments ADD COLUMN subject_group_id UUID REFERENCES subject_groups (id);

ALTER TABLE import_batches DROP CONSTRAINT IF EXISTS import_batches_kind_check;
ALTER TABLE import_batches ADD CONSTRAINT import_batches_kind_check
    CHECK (kind IN ('students', 'staff', 'opening_balances', 'board_results', 'student_subjects'));
