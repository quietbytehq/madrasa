-- M2 stage 8: consent manager. Consent types defined by the school and an append-only
-- history of consents per student (a new status is a new row; the old one stays).

CREATE TABLE consent_types (
    id           UUID PRIMARY KEY,
    school_id    UUID NOT NULL REFERENCES schools (id),
    name         TEXT NOT NULL,
    description  TEXT NOT NULL DEFAULT '',
    given_by     TEXT NOT NULL DEFAULT 'guardian' CHECK (given_by IN ('guardian', 'student', 'either')),
    required     TEXT NOT NULL DEFAULT 'all' CHECK (required IN ('none', 'all', 'class_range')),
    min_level    INTEGER,
    max_level    INTEGER,
    renewal      TEXT NOT NULL DEFAULT 'none' CHECK (renewal IN ('none', 'yearly')),
    position     INTEGER NOT NULL DEFAULT 0,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by   UUID,
    deleted_at   TIMESTAMPTZ,
    hlc          TEXT NOT NULL
);
CREATE UNIQUE INDEX consent_types_name_uq ON consent_types (school_id, lower(name)) WHERE deleted_at IS NULL;

CREATE TABLE consents (
    id                UUID PRIMARY KEY,
    school_id         UUID NOT NULL REFERENCES schools (id),
    student_id        UUID NOT NULL REFERENCES students (id),
    consent_type_id   UUID NOT NULL REFERENCES consent_types (id),
    status            TEXT NOT NULL CHECK (status IN ('given', 'refused', 'withdrawn')),
    guardian_id       UUID REFERENCES guardians (id),
    academic_year_id  UUID REFERENCES academic_years (id),
    recorded_on       DATE NOT NULL,
    recorded_by       UUID REFERENCES users (id),
    note              TEXT NOT NULL DEFAULT '',
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by        UUID,
    deleted_at        TIMESTAMPTZ,
    hlc               TEXT NOT NULL
);
CREATE INDEX consents_student_idx ON consents (student_id, consent_type_id, recorded_on DESC);
