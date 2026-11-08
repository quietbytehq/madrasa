-- Standard stage 7: inventory. Item categories and items, stock locations, stock movements
-- (in, issue, return, write-off, adjustments, transfers) and asset tags for durable items.
-- Stock on hand is the sum of movements per item and location and never goes negative.
-- Empty on Lite.

CREATE TABLE inventory_categories (
    id          UUID PRIMARY KEY,
    school_id   UUID NOT NULL REFERENCES schools (id),
    name        TEXT NOT NULL,
    position    INTEGER NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID,
    deleted_at  TIMESTAMPTZ,
    hlc         TEXT NOT NULL
);
CREATE UNIQUE INDEX inventory_categories_name_uq ON inventory_categories (school_id, lower(name)) WHERE deleted_at IS NULL;

CREATE TABLE inventory_locations (
    id          UUID PRIMARY KEY,
    school_id   UUID NOT NULL REFERENCES schools (id),
    name        TEXT NOT NULL,
    kind        TEXT NOT NULL DEFAULT 'store' CHECK (kind IN ('store', 'lab', 'room', 'office', 'other')),
    active      BOOLEAN NOT NULL DEFAULT true,
    notes       TEXT NOT NULL DEFAULT '',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by  UUID,
    deleted_at  TIMESTAMPTZ,
    hlc         TEXT NOT NULL
);
CREATE UNIQUE INDEX inventory_locations_name_uq ON inventory_locations (school_id, lower(name)) WHERE deleted_at IS NULL;

CREATE TABLE inventory_items (
    id             UUID PRIMARY KEY,
    school_id      UUID NOT NULL REFERENCES schools (id),
    category_id    UUID REFERENCES inventory_categories (id),
    name           TEXT NOT NULL,
    code           TEXT NOT NULL DEFAULT '',
    unit           TEXT NOT NULL DEFAULT 'pcs',
    reorder_level  NUMERIC(14,2) CHECK (reorder_level IS NULL OR reorder_level >= 0),
    -- Durable items carry asset tags (one row per physical piece in inventory_assets).
    is_asset       BOOLEAN NOT NULL DEFAULT false,
    active         BOOLEAN NOT NULL DEFAULT true,
    notes          TEXT NOT NULL DEFAULT '',
    custom         JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by     UUID,
    deleted_at     TIMESTAMPTZ,
    hlc            TEXT NOT NULL
);
CREATE UNIQUE INDEX inventory_items_name_uq ON inventory_items (school_id, lower(name)) WHERE deleted_at IS NULL;
CREATE UNIQUE INDEX inventory_items_code_uq ON inventory_items (school_id, upper(code)) WHERE deleted_at IS NULL AND code <> '';

CREATE TABLE inventory_movements (
    id              UUID PRIMARY KEY,
    school_id       UUID NOT NULL REFERENCES schools (id),
    item_id         UUID NOT NULL REFERENCES inventory_items (id),
    location_id     UUID NOT NULL REFERENCES inventory_locations (id),
    kind            TEXT NOT NULL CHECK (kind IN ('purchase_in', 'issue', 'return', 'write_off', 'adjust_in', 'adjust_out', 'transfer_out', 'transfer_in')),
    quantity        NUMERIC(14,2) NOT NULL CHECK (quantity > 0),
    unit_cost       NUMERIC(14,2) CHECK (unit_cost IS NULL OR unit_cost >= 0),
    total_cost      NUMERIC(14,2) CHECK (total_cost IS NULL OR total_cost >= 0),
    moved_on        DATE NOT NULL,
    -- Issues and returns: who received or brought back the goods.
    recipient_kind  TEXT CHECK (recipient_kind IS NULL OR recipient_kind IN ('staff', 'student', 'room')),
    recipient_id    UUID,
    recipient_name  TEXT NOT NULL DEFAULT '',
    -- Bill, requisition or gate pass number.
    reference       TEXT NOT NULL DEFAULT '',
    -- The two halves of a transfer share this id.
    transfer_id     UUID,
    notes           TEXT NOT NULL DEFAULT '',
    recorded_by     UUID REFERENCES users (id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by      UUID,
    deleted_at      TIMESTAMPTZ,
    hlc             TEXT NOT NULL
);
CREATE INDEX inventory_movements_item_idx ON inventory_movements (item_id, location_id) WHERE deleted_at IS NULL;
CREATE INDEX inventory_movements_date_idx ON inventory_movements (school_id, moved_on) WHERE deleted_at IS NULL;
CREATE INDEX inventory_movements_recipient_idx ON inventory_movements (recipient_kind, recipient_id) WHERE deleted_at IS NULL;

CREATE TABLE inventory_assets (
    id             UUID PRIMARY KEY,
    school_id      UUID NOT NULL REFERENCES schools (id),
    item_id        UUID NOT NULL REFERENCES inventory_items (id),
    tag            TEXT NOT NULL,
    serial_no      TEXT NOT NULL DEFAULT '',
    location_id    UUID REFERENCES inventory_locations (id),
    status         TEXT NOT NULL DEFAULT 'in_store' CHECK (status IN ('in_store', 'in_use', 'under_repair', 'disposed')),
    assigned_kind  TEXT CHECK (assigned_kind IS NULL OR assigned_kind IN ('staff', 'student', 'room')),
    assigned_id    UUID,
    assigned_name  TEXT NOT NULL DEFAULT '',
    purchased_on   DATE,
    cost           NUMERIC(14,2) CHECK (cost IS NULL OR cost >= 0),
    notes          TEXT NOT NULL DEFAULT '',
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by     UUID,
    deleted_at     TIMESTAMPTZ,
    hlc            TEXT NOT NULL
);
CREATE UNIQUE INDEX inventory_assets_tag_uq ON inventory_assets (school_id, upper(tag)) WHERE deleted_at IS NULL;
CREATE INDEX inventory_assets_item_idx ON inventory_assets (item_id) WHERE deleted_at IS NULL;

-- Grants for existing Standard installs; Lite roles receive them when the edition is switched.
INSERT INTO permissions (id, school_id, role_id, code, hlc)
SELECT gen_random_uuid(), r.school_id, r.id, p.code, '0000000000000-00000-000000000000'
FROM roles r
JOIN (VALUES ('principal', 'inventory.read'), ('principal', 'inventory.manage'),
             ('admin', 'inventory.read'), ('admin', 'inventory.manage'),
             ('accountant', 'inventory.read')) AS p(role, code)
  ON p.role = r.code
WHERE r.is_system AND r.deleted_at IS NULL
  AND EXISTS (SELECT 1 FROM settings s WHERE s.school_id IS NULL AND s.key = 'edition'
              AND s.deleted_at IS NULL AND s.value::text IN ('"standard"', '"nexus"'))
ON CONFLICT (role_id, code) DO NOTHING;
