-- M1 stage 3: attendance. One sheet per section per day; one entry per enrolled student.
-- Sheets are submitted on first save; later changes need attendance.correct and are audited.

-- Holidays and closures (inclusive date ranges). Weekend days live in settings
-- (`attendance.weekend_days`, ISO weekday numbers, default Sunday).
CREATE TABLE holidays (
    id          UUID PRIMARY KEY,
    school_id   UUID NOT NULL REFERENCES schools (id),
    name        TEXT NOT NULL,
    starts_on   DATE NOT NULL,
    ends_on     DATE NOT NULL CHECK (ends_on >= starts_on),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID,
    deleted_at  TIMESTAMPTZ,
    hlc         TEXT NOT NULL
);
CREATE INDEX holidays_school_dates_idx ON holidays (school_id, starts_on, ends_on) WHERE deleted_at IS NULL;

CREATE TABLE attendance_sheets (
    id                UUID PRIMARY KEY,
    school_id         UUID NOT NULL REFERENCES schools (id),
    section_id        UUID NOT NULL REFERENCES sections (id),
    academic_year_id  UUID NOT NULL REFERENCES academic_years (id),
    date              DATE NOT NULL,
    marked_by         UUID REFERENCES users (id),
    submitted_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    present           INTEGER NOT NULL DEFAULT 0,
    absent            INTEGER NOT NULL DEFAULT 0,
    late              INTEGER NOT NULL DEFAULT 0,
    leave             INTEGER NOT NULL DEFAULT 0,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by        UUID,
    deleted_at        TIMESTAMPTZ,
    hlc               TEXT NOT NULL
);
CREATE UNIQUE INDEX attendance_sheets_section_date_uq ON attendance_sheets (section_id, date) WHERE deleted_at IS NULL;
CREATE INDEX attendance_sheets_school_date_idx ON attendance_sheets (school_id, date) WHERE deleted_at IS NULL;

CREATE TABLE attendance_entries (
    id          UUID PRIMARY KEY,
    school_id   UUID NOT NULL REFERENCES schools (id),
    sheet_id    UUID NOT NULL REFERENCES attendance_sheets (id),
    student_id  UUID NOT NULL REFERENCES students (id),
    date        DATE NOT NULL,  -- copied from the sheet for fast per-student history
    status      TEXT NOT NULL CHECK (status IN ('present', 'absent', 'late', 'leave')),
    note        TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID,
    deleted_at  TIMESTAMPTZ,
    hlc         TEXT NOT NULL
);
CREATE UNIQUE INDEX attendance_entries_sheet_student_uq ON attendance_entries (sheet_id, student_id) WHERE deleted_at IS NULL;
CREATE INDEX attendance_entries_student_date_idx ON attendance_entries (student_id, date DESC) WHERE deleted_at IS NULL;
CREATE INDEX attendance_entries_date_status_idx ON attendance_entries (school_id, date, status) WHERE deleted_at IS NULL;

CREATE TABLE staff_attendance (
    id          UUID PRIMARY KEY,
    school_id   UUID NOT NULL REFERENCES schools (id),
    staff_id    UUID NOT NULL REFERENCES staff (id),
    date        DATE NOT NULL,
    status      TEXT NOT NULL CHECK (status IN ('present', 'absent', 'leave')),
    note        TEXT,
    marked_by   UUID REFERENCES users (id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID,
    deleted_at  TIMESTAMPTZ,
    hlc         TEXT NOT NULL
);
CREATE UNIQUE INDEX staff_attendance_staff_date_uq ON staff_attendance (staff_id, date) WHERE deleted_at IS NULL;
CREATE INDEX staff_attendance_school_date_idx ON staff_attendance (school_id, date) WHERE deleted_at IS NULL;

-- Corrections after submission: principal and admin.
INSERT INTO permissions (id, school_id, role_id, code, hlc)
SELECT gen_random_uuid(), r.school_id, r.id, p.code, '0000000000000-00000-000000000000'
FROM roles r
JOIN (VALUES ('principal', 'attendance.correct'), ('admin', 'attendance.correct')) AS p(role, code)
  ON p.role = r.code
WHERE r.is_system AND r.deleted_at IS NULL
ON CONFLICT (role_id, code) DO NOTHING;
