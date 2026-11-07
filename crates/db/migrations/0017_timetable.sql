-- Standard stage 1: timetable. Shifts (periods, breaks, working days), rooms, lessons,
-- per-year rules (time-off grids, teacher limits, section shifts), generated timetables
-- with their cards, and substitutions (absences and covers). All tables stay empty on Lite.

CREATE TABLE timetable_shifts (
    id                UUID PRIMARY KEY,
    school_id         UUID NOT NULL REFERENCES schools (id),
    academic_year_id  UUID NOT NULL REFERENCES academic_years (id),
    name              TEXT NOT NULL,
    enabled           BOOLEAN NOT NULL DEFAULT true,
    -- Seven entries, Monday first: { "working": bool, "periods": n }.
    days              JSONB NOT NULL DEFAULT '[]'::jsonb,
    -- Period times in order: [{ "start": "08:00", "end": "08:40" }, …].
    period_times      JSONB NOT NULL DEFAULT '[]'::jsonb,
    -- [{ "after_period": 3, "name": "Break", "cannot_cross": true }].
    breaks            JSONB NOT NULL DEFAULT '[]'::jsonb,
    position          INTEGER NOT NULL DEFAULT 0,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by        UUID,
    deleted_at        TIMESTAMPTZ,
    hlc               TEXT NOT NULL
);
CREATE UNIQUE INDEX timetable_shifts_name_uq ON timetable_shifts (academic_year_id, lower(name)) WHERE deleted_at IS NULL;

CREATE TABLE rooms (
    id          UUID PRIMARY KEY,
    school_id   UUID NOT NULL REFERENCES schools (id),
    name        TEXT NOT NULL,
    short_name  TEXT NOT NULL,
    building    TEXT NOT NULL DEFAULT '',
    capacity    INTEGER,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID,
    deleted_at  TIMESTAMPTZ,
    hlc         TEXT NOT NULL
);
CREATE UNIQUE INDEX rooms_name_uq ON rooms (school_id, lower(name)) WHERE deleted_at IS NULL;

CREATE TABLE timetable_lessons (
    id                UUID PRIMARY KEY,
    school_id         UUID NOT NULL REFERENCES schools (id),
    academic_year_id  UUID NOT NULL REFERENCES academic_years (id),
    subject_id        UUID NOT NULL REFERENCES subjects (id),
    teacher_ids       UUID[] NOT NULL DEFAULT '{}',
    section_ids       UUID[] NOT NULL DEFAULT '{}',
    room_ids          UUID[] NOT NULL DEFAULT '{}',
    periods_per_card  INTEGER NOT NULL DEFAULT 1 CHECK (periods_per_card BETWEEN 1 AND 3),
    cards_per_week    INTEGER NOT NULL DEFAULT 1 CHECK (cards_per_week >= 1),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by        UUID,
    deleted_at        TIMESTAMPTZ,
    hlc               TEXT NOT NULL
);
CREATE INDEX timetable_lessons_year_idx ON timetable_lessons (academic_year_id) WHERE deleted_at IS NULL;

-- Per-year rules for one teacher, section or room, and the school-wide solver settings
-- (kind 'school', entity_id NULL).
CREATE TABLE timetable_rules (
    id                UUID PRIMARY KEY,
    school_id         UUID NOT NULL REFERENCES schools (id),
    academic_year_id  UUID NOT NULL REFERENCES academic_years (id),
    kind              TEXT NOT NULL CHECK (kind IN ('school', 'teacher', 'section', 'room')),
    entity_id         UUID,
    -- Teachers: shifts they teach in (empty = all enabled). Sections: the one shift attended.
    shift_ids         UUID[] NOT NULL DEFAULT '{}',
    home_room_id      UUID REFERENCES rooms (id),
    -- [{ "shift": id, "day": 0, "period": 1, "state": 1|2 }] (1 = avoid, 2 = never).
    time_off          JSONB NOT NULL DEFAULT '[]'::jsonb,
    -- Teacher limits or school settings (max per day, per week, consecutive, …).
    limits            JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by        UUID,
    deleted_at        TIMESTAMPTZ,
    hlc               TEXT NOT NULL
);
CREATE UNIQUE INDEX timetable_rules_entity_uq ON timetable_rules (academic_year_id, kind, entity_id) NULLS NOT DISTINCT WHERE deleted_at IS NULL;

CREATE TABLE timetables (
    id                UUID PRIMARY KEY,
    school_id         UUID NOT NULL REFERENCES schools (id),
    academic_year_id  UUID NOT NULL REFERENCES academic_years (id),
    name              TEXT NOT NULL,
    status            TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'published')),
    -- The generator's report (placed/unplaced, reasons, quality) as JSON.
    report            JSONB,
    generated_at      TIMESTAMPTZ,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by        UUID,
    deleted_at        TIMESTAMPTZ,
    hlc               TEXT NOT NULL
);
CREATE UNIQUE INDEX timetables_published_uq ON timetables (academic_year_id) WHERE status = 'published' AND deleted_at IS NULL;

CREATE TABLE timetable_cards (
    id            UUID PRIMARY KEY,
    school_id     UUID NOT NULL REFERENCES schools (id),
    timetable_id  UUID NOT NULL REFERENCES timetables (id),
    lesson_id     UUID NOT NULL REFERENCES timetable_lessons (id),
    number        INTEGER NOT NULL CHECK (number >= 1),
    shift_id      UUID NOT NULL REFERENCES timetable_shifts (id),
    day           SMALLINT NOT NULL CHECK (day BETWEEN 0 AND 6),
    period        SMALLINT NOT NULL CHECK (period >= 1),
    length        SMALLINT NOT NULL DEFAULT 1 CHECK (length BETWEEN 1 AND 3),
    room_id       UUID REFERENCES rooms (id),
    locked        BOOLEAN NOT NULL DEFAULT false,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by    UUID,
    deleted_at    TIMESTAMPTZ,
    hlc           TEXT NOT NULL
);
CREATE UNIQUE INDEX timetable_cards_session_uq ON timetable_cards (timetable_id, lesson_id, number) WHERE deleted_at IS NULL;
CREATE INDEX timetable_cards_slot_idx ON timetable_cards (timetable_id, day, period) WHERE deleted_at IS NULL;

-- A teacher away for whole days or some periods; the published timetable says what needs cover.
CREATE TABLE timetable_absences (
    id                UUID PRIMARY KEY,
    school_id         UUID NOT NULL REFERENCES schools (id),
    academic_year_id  UUID NOT NULL REFERENCES academic_years (id),
    staff_id          UUID NOT NULL REFERENCES staff (id),
    from_date         DATE NOT NULL,
    to_date           DATE NOT NULL CHECK (to_date >= from_date),
    -- Empty = the whole day; otherwise [{ "shift": id, "period": n }].
    periods           JSONB NOT NULL DEFAULT '[]'::jsonb,
    note              TEXT NOT NULL DEFAULT '',
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by        UUID,
    deleted_at        TIMESTAMPTZ,
    hlc               TEXT NOT NULL
);
CREATE INDEX timetable_absences_dates_idx ON timetable_absences (academic_year_id, from_date, to_date) WHERE deleted_at IS NULL;

-- Who covers one session of an absent teacher on one date (NULL cover = no cover needed).
CREATE TABLE timetable_covers (
    id                UUID PRIMARY KEY,
    school_id         UUID NOT NULL REFERENCES schools (id),
    academic_year_id  UUID NOT NULL REFERENCES academic_years (id),
    absence_id        UUID NOT NULL REFERENCES timetable_absences (id),
    date              DATE NOT NULL,
    lesson_id         UUID NOT NULL REFERENCES timetable_lessons (id),
    number            INTEGER NOT NULL,
    absent_staff_id   UUID NOT NULL REFERENCES staff (id),
    cover_staff_id    UUID REFERENCES staff (id),
    shift_id          UUID NOT NULL REFERENCES timetable_shifts (id),
    period            SMALLINT NOT NULL,
    length            SMALLINT NOT NULL DEFAULT 1,
    note              TEXT NOT NULL DEFAULT '',
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by        UUID,
    deleted_at        TIMESTAMPTZ,
    hlc               TEXT NOT NULL
);
CREATE UNIQUE INDEX timetable_covers_session_uq ON timetable_covers (absence_id, date, lesson_id, number) WHERE deleted_at IS NULL;
CREATE INDEX timetable_covers_date_idx ON timetable_covers (academic_year_id, date) WHERE deleted_at IS NULL;

-- Grants for existing Standard installs; Lite roles receive them when the edition is switched.
INSERT INTO permissions (id, school_id, role_id, code, hlc)
SELECT gen_random_uuid(), r.school_id, r.id, p.code, '0000000000000-00000-000000000000'
FROM roles r
JOIN (VALUES ('principal', 'timetable.read'), ('principal', 'timetable.manage'),
             ('admin', 'timetable.read'), ('admin', 'timetable.manage'),
             ('accountant', 'timetable.read'), ('teacher', 'timetable.read')) AS p(role, code)
  ON p.role = r.code
WHERE r.is_system AND r.deleted_at IS NULL
  AND EXISTS (SELECT 1 FROM settings s WHERE s.school_id IS NULL AND s.key = 'edition'
              AND s.deleted_at IS NULL AND s.value::text IN ('"standard"', '"nexus"'))
ON CONFLICT (role_id, code) DO NOTHING;
