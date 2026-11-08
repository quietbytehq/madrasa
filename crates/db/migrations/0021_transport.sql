-- Standard stage 5: transport. Vehicles, routes with ordered stops, and student assignments
-- per academic year. The monthly transport charge joins the fee voucher when the setting
-- `transport.fee_head_id` names a fee head. Empty on Lite.

CREATE TABLE transport_vehicles (
    id               UUID PRIMARY KEY,
    school_id        UUID NOT NULL REFERENCES schools (id),
    registration_no  TEXT NOT NULL,
    make_model       TEXT NOT NULL DEFAULT '',
    capacity         INTEGER CHECK (capacity IS NULL OR capacity >= 0),
    driver_name      TEXT NOT NULL DEFAULT '',
    driver_phone     TEXT NOT NULL DEFAULT '',
    helper_name      TEXT NOT NULL DEFAULT '',
    helper_phone     TEXT NOT NULL DEFAULT '',
    active           BOOLEAN NOT NULL DEFAULT true,
    notes            TEXT NOT NULL DEFAULT '',
    custom           JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by       UUID,
    deleted_at       TIMESTAMPTZ,
    hlc              TEXT NOT NULL
);
CREATE UNIQUE INDEX transport_vehicles_reg_uq ON transport_vehicles (school_id, upper(registration_no)) WHERE deleted_at IS NULL;

CREATE TABLE transport_routes (
    id           UUID PRIMARY KEY,
    school_id    UUID NOT NULL REFERENCES schools (id),
    name         TEXT NOT NULL,
    code         TEXT NOT NULL DEFAULT '',
    vehicle_id   UUID REFERENCES transport_vehicles (id),
    -- Default monthly charge; a stop or an assignment may override it.
    monthly_fee  NUMERIC(14,2) NOT NULL DEFAULT 0 CHECK (monthly_fee >= 0),
    active       BOOLEAN NOT NULL DEFAULT true,
    notes        TEXT NOT NULL DEFAULT '',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by   UUID,
    deleted_at   TIMESTAMPTZ,
    hlc          TEXT NOT NULL
);
CREATE UNIQUE INDEX transport_routes_name_uq ON transport_routes (school_id, lower(name)) WHERE deleted_at IS NULL;

CREATE TABLE transport_stops (
    id           UUID PRIMARY KEY,
    school_id    UUID NOT NULL REFERENCES schools (id),
    route_id     UUID NOT NULL REFERENCES transport_routes (id),
    name         TEXT NOT NULL,
    position     INTEGER NOT NULL DEFAULT 0,
    pickup_time  TIME,
    drop_time    TIME,
    monthly_fee  NUMERIC(14,2) CHECK (monthly_fee IS NULL OR monthly_fee >= 0),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by   UUID,
    deleted_at   TIMESTAMPTZ,
    hlc          TEXT NOT NULL
);
CREATE INDEX transport_stops_route_idx ON transport_stops (route_id, position) WHERE deleted_at IS NULL;

CREATE TABLE transport_assignments (
    id                UUID PRIMARY KEY,
    school_id         UUID NOT NULL REFERENCES schools (id),
    student_id        UUID NOT NULL REFERENCES students (id),
    academic_year_id  UUID NOT NULL REFERENCES academic_years (id),
    route_id          UUID NOT NULL REFERENCES transport_routes (id),
    stop_id           UUID NOT NULL REFERENCES transport_stops (id),
    from_date         DATE NOT NULL,
    to_date           DATE CHECK (to_date IS NULL OR to_date >= from_date),
    -- Overrides the stop's and the route's monthly charge for this student.
    amount            NUMERIC(14,2) CHECK (amount IS NULL OR amount >= 0),
    notes             TEXT NOT NULL DEFAULT '',
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by        UUID,
    deleted_at        TIMESTAMPTZ,
    hlc               TEXT NOT NULL
);
-- One open assignment per student and year; a change of route ends the previous one.
CREATE UNIQUE INDEX transport_assignments_open_uq ON transport_assignments (student_id, academic_year_id) WHERE to_date IS NULL AND deleted_at IS NULL;
CREATE INDEX transport_assignments_route_idx ON transport_assignments (route_id, academic_year_id) WHERE deleted_at IS NULL;
CREATE INDEX transport_assignments_student_idx ON transport_assignments (student_id, academic_year_id) WHERE deleted_at IS NULL;

-- Grants for existing Standard installs; Lite roles receive them when the edition is switched.
INSERT INTO permissions (id, school_id, role_id, code, hlc)
SELECT gen_random_uuid(), r.school_id, r.id, p.code, '0000000000000-00000-000000000000'
FROM roles r
JOIN (VALUES ('principal', 'transport.read'), ('principal', 'transport.manage'),
             ('admin', 'transport.read'), ('admin', 'transport.manage'),
             ('accountant', 'transport.read')) AS p(role, code)
  ON p.role = r.code
WHERE r.is_system AND r.deleted_at IS NULL
  AND EXISTS (SELECT 1 FROM settings s WHERE s.school_id IS NULL AND s.key = 'edition'
              AND s.deleted_at IS NULL AND s.value::text IN ('"standard"', '"nexus"'))
ON CONFLICT (role_id, code) DO NOTHING;
