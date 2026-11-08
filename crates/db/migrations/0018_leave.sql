-- Standard stage 2: leave. Staff leave types, balances per year, requests with approval,
-- student leave requests, and the links from attendance rows to the request that marked
-- them. Staff attendance gains a `late` status with an arrival time (Standard only).

CREATE TABLE leave_types (
    id                UUID PRIMARY KEY,
    school_id         UUID NOT NULL REFERENCES schools (id),
    name              TEXT NOT NULL,
    code              TEXT NOT NULL DEFAULT '',
    entitlement_days  NUMERIC(5,1) NOT NULL DEFAULT 0 CHECK (entitlement_days >= 0),
    carry_over_max    NUMERIC(5,1) NOT NULL DEFAULT 0 CHECK (carry_over_max >= 0),
    needs_approval    BOOLEAN NOT NULL DEFAULT true,
    position          INTEGER NOT NULL DEFAULT 0,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by        UUID,
    deleted_at        TIMESTAMPTZ,
    hlc               TEXT NOT NULL
);
CREATE UNIQUE INDEX leave_types_name_uq ON leave_types (school_id, lower(name)) WHERE deleted_at IS NULL;

CREATE TABLE leave_balances (
    id                UUID PRIMARY KEY,
    school_id         UUID NOT NULL REFERENCES schools (id),
    staff_id          UUID NOT NULL REFERENCES staff (id),
    leave_type_id     UUID NOT NULL REFERENCES leave_types (id),
    academic_year_id  UUID NOT NULL REFERENCES academic_years (id),
    -- NULL = the type's entitlement.
    entitlement       NUMERIC(5,1),
    carried_over      NUMERIC(5,1) NOT NULL DEFAULT 0,
    adjustment        NUMERIC(5,1) NOT NULL DEFAULT 0,
    note              TEXT NOT NULL DEFAULT '',
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by        UUID,
    deleted_at        TIMESTAMPTZ,
    hlc               TEXT NOT NULL
);
CREATE UNIQUE INDEX leave_balances_uq ON leave_balances (staff_id, leave_type_id, academic_year_id) WHERE deleted_at IS NULL;

CREATE TABLE leave_requests (
    id                UUID PRIMARY KEY,
    school_id         UUID NOT NULL REFERENCES schools (id),
    staff_id          UUID NOT NULL REFERENCES staff (id),
    leave_type_id     UUID NOT NULL REFERENCES leave_types (id),
    academic_year_id  UUID NOT NULL REFERENCES academic_years (id),
    from_date         DATE NOT NULL,
    to_date           DATE NOT NULL CHECK (to_date >= from_date),
    half_day          TEXT CHECK (half_day IN ('am', 'pm')),
    days              NUMERIC(5,1) NOT NULL CHECK (days > 0),
    reason            TEXT NOT NULL DEFAULT '',
    attachment_note   TEXT NOT NULL DEFAULT '',
    status            TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'approved', 'rejected', 'withdrawn')),
    requested_by      UUID REFERENCES users (id),
    decided_by        UUID REFERENCES users (id),
    decided_at        TIMESTAMPTZ,
    decision_note     TEXT NOT NULL DEFAULT '',
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by        UUID,
    deleted_at        TIMESTAMPTZ,
    hlc               TEXT NOT NULL
);
CREATE INDEX leave_requests_staff_idx ON leave_requests (staff_id, from_date, to_date) WHERE deleted_at IS NULL;
CREATE INDEX leave_requests_year_idx ON leave_requests (academic_year_id, status) WHERE deleted_at IS NULL;

CREATE TABLE student_leave_requests (
    id                UUID PRIMARY KEY,
    school_id         UUID NOT NULL REFERENCES schools (id),
    student_id        UUID NOT NULL REFERENCES students (id),
    academic_year_id  UUID NOT NULL REFERENCES academic_years (id),
    from_date         DATE NOT NULL,
    to_date           DATE NOT NULL CHECK (to_date >= from_date),
    reason            TEXT NOT NULL DEFAULT '',
    guardian_id       UUID REFERENCES guardians (id),
    status            TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'approved', 'rejected', 'withdrawn')),
    recorded_by       UUID REFERENCES users (id),
    decided_by        UUID REFERENCES users (id),
    decided_at        TIMESTAMPTZ,
    decision_note     TEXT NOT NULL DEFAULT '',
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by        UUID,
    deleted_at        TIMESTAMPTZ,
    hlc               TEXT NOT NULL
);
CREATE INDEX student_leave_requests_student_idx ON student_leave_requests (student_id, from_date, to_date) WHERE deleted_at IS NULL;
CREATE INDEX student_leave_requests_year_idx ON student_leave_requests (academic_year_id, status) WHERE deleted_at IS NULL;

-- Attendance rows written by an approved request remember it, so a withdrawal can undo them
-- and a corrected sheet keeps them. NULL for every row marked by hand (all Lite rows).
ALTER TABLE staff_attendance ADD COLUMN leave_request_id UUID REFERENCES leave_requests (id);
ALTER TABLE attendance_entries ADD COLUMN leave_request_id UUID REFERENCES student_leave_requests (id);

-- Late marking with an arrival time (Standard; the API refuses `late` on Lite).
ALTER TABLE staff_attendance DROP CONSTRAINT staff_attendance_status_check;
ALTER TABLE staff_attendance ADD CONSTRAINT staff_attendance_status_check
    CHECK (status IN ('present', 'absent', 'late', 'leave'));
ALTER TABLE staff_attendance ADD COLUMN arrived_at TIME;

-- Grants for existing Standard installs; Lite roles receive them when the edition is switched.
INSERT INTO permissions (id, school_id, role_id, code, hlc)
SELECT gen_random_uuid(), r.school_id, r.id, p.code, '0000000000000-00000-000000000000'
FROM roles r
JOIN (VALUES ('principal', 'leave.read'), ('principal', 'leave.manage'), ('principal', 'leave.approve'),
             ('admin', 'leave.read'), ('admin', 'leave.manage'), ('admin', 'leave.approve'),
             ('accountant', 'leave.read'), ('teacher', 'leave.read')) AS p(role, code)
  ON p.role = r.code
WHERE r.is_system AND r.deleted_at IS NULL
  AND EXISTS (SELECT 1 FROM settings s WHERE s.school_id IS NULL AND s.key = 'edition'
              AND s.deleted_at IS NULL AND s.value::text IN ('"standard"', '"nexus"'))
ON CONFLICT (role_id, code) DO NOTHING;
