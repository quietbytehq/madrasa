-- M2 stage 4: external examination boards (BISE, Federal Board, AKU-EB, Cambridge…), their
-- exams, the school's candidates and the results imported from the board.

CREATE TABLE exam_boards (
    id            UUID PRIMARY KEY,
    school_id     UUID NOT NULL REFERENCES schools (id),
    name          TEXT NOT NULL,
    short_code    TEXT NOT NULL,
    country       TEXT NOT NULL DEFAULT 'PK',
    grading_mode  TEXT NOT NULL DEFAULT 'marks' CHECK (grading_mode IN ('marks', 'grades', 'both')),
    pass_percent  NUMERIC(5,2) NOT NULL DEFAULT 33 CHECK (pass_percent >= 0 AND pass_percent <= 100),
    -- [{ "grade": "A+", "min_percent": 80 }, …] highest first; empty = no grades.
    grade_bands   JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by    UUID,
    deleted_at    TIMESTAMPTZ,
    hlc           TEXT NOT NULL
);
CREATE UNIQUE INDEX exam_boards_name_uq ON exam_boards (school_id, lower(name)) WHERE deleted_at IS NULL;

CREATE TABLE board_exams (
    id                UUID PRIMARY KEY,
    school_id         UUID NOT NULL REFERENCES schools (id),
    board_id          UUID NOT NULL REFERENCES exam_boards (id),
    academic_year_id  UUID NOT NULL REFERENCES academic_years (id),
    name              TEXT NOT NULL,
    status            TEXT NOT NULL DEFAULT 'announced' CHECK (status IN ('announced', 'results_imported', 'closed')),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by        UUID,
    deleted_at        TIMESTAMPTZ,
    hlc               TEXT NOT NULL
);
CREATE UNIQUE INDEX board_exams_name_uq ON board_exams (board_id, academic_year_id, lower(name)) WHERE deleted_at IS NULL;

CREATE TABLE board_exam_classes (
    id             UUID PRIMARY KEY,
    school_id      UUID NOT NULL REFERENCES schools (id),
    board_exam_id  UUID NOT NULL REFERENCES board_exams (id),
    class_id       UUID NOT NULL REFERENCES classes (id),
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by     UUID,
    deleted_at     TIMESTAMPTZ,
    hlc            TEXT NOT NULL
);
CREATE UNIQUE INDEX board_exam_classes_uq ON board_exam_classes (board_exam_id, class_id) WHERE deleted_at IS NULL;

CREATE TABLE board_exam_subjects (
    id             UUID PRIMARY KEY,
    school_id      UUID NOT NULL REFERENCES schools (id),
    board_exam_id  UUID NOT NULL REFERENCES board_exams (id),
    name           TEXT NOT NULL,
    -- The school subject this board paper corresponds to, if any.
    subject_id     UUID REFERENCES subjects (id),
    max_marks      NUMERIC(6,2) NOT NULL CHECK (max_marks > 0),
    position       INTEGER NOT NULL DEFAULT 0,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by     UUID,
    deleted_at     TIMESTAMPTZ,
    hlc            TEXT NOT NULL
);
CREATE UNIQUE INDEX board_exam_subjects_uq ON board_exam_subjects (board_exam_id, lower(name)) WHERE deleted_at IS NULL;

CREATE TABLE board_candidates (
    id               UUID PRIMARY KEY,
    school_id        UUID NOT NULL REFERENCES schools (id),
    board_exam_id    UUID NOT NULL REFERENCES board_exams (id),
    student_id       UUID NOT NULL REFERENCES students (id),
    roll_no          TEXT,
    registration_no  TEXT,
    group_name       TEXT,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by       UUID,
    deleted_at       TIMESTAMPTZ,
    hlc              TEXT NOT NULL
);
CREATE UNIQUE INDEX board_candidates_student_uq ON board_candidates (board_exam_id, student_id) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX board_candidates_roll_uq ON board_candidates (board_exam_id, roll_no) WHERE deleted_at IS NULL AND roll_no IS NOT NULL;

CREATE TABLE board_results (
    id                UUID PRIMARY KEY,
    school_id         UUID NOT NULL REFERENCES schools (id),
    candidate_id      UUID NOT NULL REFERENCES board_candidates (id),
    board_subject_id  UUID NOT NULL REFERENCES board_exam_subjects (id),
    obtained          NUMERIC(6,2) CHECK (obtained IS NULL OR obtained >= 0),
    max_marks         NUMERIC(6,2) NOT NULL CHECK (max_marks > 0),
    grade             TEXT,
    status            TEXT NOT NULL CHECK (status IN ('pass', 'fail', 'absent', 'withheld')),
    remarks           TEXT,
    batch_id          UUID REFERENCES import_batches (id),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by        UUID,
    deleted_at        TIMESTAMPTZ,
    hlc               TEXT NOT NULL
);
CREATE UNIQUE INDEX board_results_uq ON board_results (candidate_id, board_subject_id) WHERE deleted_at IS NULL;
CREATE INDEX board_results_batch_idx ON board_results (batch_id) WHERE deleted_at IS NULL;

ALTER TABLE import_batches DROP CONSTRAINT IF EXISTS import_batches_kind_check;
ALTER TABLE import_batches ADD CONSTRAINT import_batches_kind_check
    CHECK (kind IN ('students', 'staff', 'opening_balances', 'board_results'));
