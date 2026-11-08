-- Standard stage 6: hostel. Hostels with rooms and numbered beds, residents allocated per
-- academic year, and the leave-out / visitor register. The monthly hostel charge joins the
-- fee voucher under the hostel's fee head. Empty on Lite.

CREATE TABLE hostels (
    id                UUID PRIMARY KEY,
    school_id         UUID NOT NULL REFERENCES schools (id),
    name              TEXT NOT NULL,
    kind              TEXT NOT NULL DEFAULT 'mixed' CHECK (kind IN ('boys', 'girls', 'mixed')),
    warden_name       TEXT NOT NULL DEFAULT '',
    warden_phone      TEXT NOT NULL DEFAULT '',
    -- Default monthly charge; a room or an allocation may override it.
    monthly_fee       NUMERIC(14,2) NOT NULL DEFAULT 0 CHECK (monthly_fee >= 0),
    fee_head_id       UUID REFERENCES fee_heads (id),
    -- Leave-outs need the guardian's consent recorded.
    requires_consent  BOOLEAN NOT NULL DEFAULT true,
    active            BOOLEAN NOT NULL DEFAULT true,
    notes             TEXT NOT NULL DEFAULT '',
    custom            JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by        UUID,
    deleted_at        TIMESTAMPTZ,
    hlc               TEXT NOT NULL
);
CREATE UNIQUE INDEX hostels_name_uq ON hostels (school_id, lower(name)) WHERE deleted_at IS NULL;

CREATE TABLE hostel_rooms (
    id           UUID PRIMARY KEY,
    school_id    UUID NOT NULL REFERENCES schools (id),
    hostel_id    UUID NOT NULL REFERENCES hostels (id),
    floor        TEXT NOT NULL DEFAULT '',
    name         TEXT NOT NULL,
    beds         INTEGER NOT NULL DEFAULT 1 CHECK (beds >= 0),
    monthly_fee  NUMERIC(14,2) CHECK (monthly_fee IS NULL OR monthly_fee >= 0),
    active       BOOLEAN NOT NULL DEFAULT true,
    notes        TEXT NOT NULL DEFAULT '',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by   UUID,
    deleted_at   TIMESTAMPTZ,
    hlc          TEXT NOT NULL
);
CREATE UNIQUE INDEX hostel_rooms_name_uq ON hostel_rooms (hostel_id, lower(name)) WHERE deleted_at IS NULL;

CREATE TABLE hostel_allocations (
    id                UUID PRIMARY KEY,
    school_id         UUID NOT NULL REFERENCES schools (id),
    student_id        UUID NOT NULL REFERENCES students (id),
    academic_year_id  UUID NOT NULL REFERENCES academic_years (id),
    hostel_id         UUID NOT NULL REFERENCES hostels (id),
    room_id           UUID NOT NULL REFERENCES hostel_rooms (id),
    bed_no            INTEGER NOT NULL CHECK (bed_no >= 1),
    from_date         DATE NOT NULL,
    to_date           DATE CHECK (to_date IS NULL OR to_date >= from_date),
    -- Overrides the room's and the hostel's monthly charge for this student.
    amount            NUMERIC(14,2) CHECK (amount IS NULL OR amount >= 0),
    notes             TEXT NOT NULL DEFAULT '',
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by        UUID,
    deleted_at        TIMESTAMPTZ,
    hlc               TEXT NOT NULL
);
-- One open allocation per student and year, and one resident per bed.
CREATE UNIQUE INDEX hostel_allocations_open_uq ON hostel_allocations (student_id, academic_year_id) WHERE to_date IS NULL AND deleted_at IS NULL;
CREATE UNIQUE INDEX hostel_allocations_bed_uq ON hostel_allocations (room_id, bed_no) WHERE to_date IS NULL AND deleted_at IS NULL;
CREATE INDEX hostel_allocations_hostel_idx ON hostel_allocations (hostel_id, academic_year_id) WHERE deleted_at IS NULL;

-- Leave-outs (a resident goes out and comes back) and visitors (someone comes and goes).
CREATE TABLE hostel_register (
    id                    UUID PRIMARY KEY,
    school_id             UUID NOT NULL REFERENCES schools (id),
    hostel_id             UUID NOT NULL REFERENCES hostels (id),
    student_id            UUID NOT NULL REFERENCES students (id),
    kind                  TEXT NOT NULL CHECK (kind IN ('leave_out', 'visitor')),
    started_at            TIMESTAMPTZ NOT NULL,
    expected_back         TIMESTAMPTZ,
    ended_at              TIMESTAMPTZ,
    visitor_name          TEXT NOT NULL DEFAULT '',
    visitor_relationship  TEXT NOT NULL DEFAULT '',
    visitor_phone         TEXT NOT NULL DEFAULT '',
    guardian_consent      BOOLEAN NOT NULL DEFAULT false,
    consent_note          TEXT NOT NULL DEFAULT '',
    recorded_by           UUID REFERENCES users (id),
    closed_by             UUID REFERENCES users (id),
    notes                 TEXT NOT NULL DEFAULT '',
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by            UUID,
    deleted_at            TIMESTAMPTZ,
    hlc                   TEXT NOT NULL
);
CREATE INDEX hostel_register_open_idx ON hostel_register (hostel_id, started_at) WHERE ended_at IS NULL AND deleted_at IS NULL;
CREATE INDEX hostel_register_student_idx ON hostel_register (student_id, started_at) WHERE deleted_at IS NULL;

-- Grants for existing Standard installs; Lite roles receive them when the edition is switched.
INSERT INTO permissions (id, school_id, role_id, code, hlc)
SELECT gen_random_uuid(), r.school_id, r.id, p.code, '0000000000000-00000-000000000000'
FROM roles r
JOIN (VALUES ('principal', 'hostel.read'), ('principal', 'hostel.manage'),
             ('admin', 'hostel.read'), ('admin', 'hostel.manage'),
             ('accountant', 'hostel.read')) AS p(role, code)
  ON p.role = r.code
WHERE r.is_system AND r.deleted_at IS NULL
  AND EXISTS (SELECT 1 FROM settings s WHERE s.school_id IS NULL AND s.key = 'edition'
              AND s.deleted_at IS NULL AND s.value::text IN ('"standard"', '"nexus"'))
ON CONFLICT (role_id, code) DO NOTHING;
