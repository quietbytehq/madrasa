-- M2 stage 3: saved custom reports. The definition is JSON built only from the field
-- catalogue (source, columns, filters, sort, grouping, limit); it never holds SQL.

CREATE TABLE report_definitions (
    id            UUID PRIMARY KEY,
    school_id     UUID NOT NULL REFERENCES schools (id),
    name          TEXT NOT NULL,
    description   TEXT NOT NULL DEFAULT '',
    source        TEXT NOT NULL,
    definition    JSONB NOT NULL,
    owner_user_id UUID NOT NULL REFERENCES users (id),
    is_shared     BOOLEAN NOT NULL DEFAULT FALSE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by    UUID,
    deleted_at    TIMESTAMPTZ,
    hlc           TEXT NOT NULL
);
CREATE UNIQUE INDEX report_definitions_name_uq ON report_definitions (school_id, lower(name)) WHERE deleted_at IS NULL;
CREATE INDEX report_definitions_school_idx ON report_definitions (school_id) WHERE deleted_at IS NULL;

-- Shared custom reports: principal and admin.
INSERT INTO permissions (id, school_id, role_id, code, hlc)
SELECT gen_random_uuid(), r.school_id, r.id, p.code, '0000000000000-00000-000000000000'
FROM roles r
JOIN (VALUES ('principal', 'reports.manage'), ('admin', 'reports.manage')) AS p(role, code)
  ON p.role = r.code
WHERE r.is_system AND r.deleted_at IS NULL
ON CONFLICT (role_id, code) DO NOTHING;
