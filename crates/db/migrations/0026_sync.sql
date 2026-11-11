-- Sync with a Madrasa Nexus hub (docs/adr/0005-nexus-hub-and-sync.md).
--
-- These tables and functions exist on every install and stay empty and unused until a
-- school joins a hub: the capture trigger is attached to the school tables only then
-- (madrasa_db::sync::enable) and removed again when the school leaves.
--
-- They are the sync engine's own bookkeeping. They are local to each copy of a school's
-- data and are never synced themselves, so they carry a UUID v7 key and school_id like
-- every table, but not the sync columns (updated_at, updated_by, deleted_at, hlc), which
-- would have no meaning on them.

-- UUID v7 for rows written by the capture trigger (the application generates its own).
CREATE FUNCTION sync_uuid_v7() RETURNS uuid AS $$
    SELECT encode(
        set_bit(
            set_bit(
                overlay(uuid_send(gen_random_uuid())
                        PLACING substring(int8send((extract(epoch FROM clock_timestamp()) * 1000)::bigint) FROM 3)
                        FROM 1 FOR 6),
                52, 1),
            53, 1),
        'hex')::uuid;
$$ LANGUAGE sql VOLATILE;

-- One row per school once it has joined: which side this copy is, and whether capture is on.
CREATE TABLE sync_state (
    id         UUID PRIMARY KEY DEFAULT sync_uuid_v7(),
    school_id  UUID NOT NULL UNIQUE REFERENCES schools(id),
    role       TEXT NOT NULL CHECK (role IN ('school', 'head_office')),
    enabled    BOOLEAN NOT NULL DEFAULT TRUE,
    enabled_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE SEQUENCE sync_change_seq;

-- What still has to be sent to the other side: one row per changed record.
CREATE TABLE sync_changes (
    id         UUID PRIMARY KEY DEFAULT sync_uuid_v7(),
    school_id  UUID NOT NULL REFERENCES schools(id),
    table_name TEXT NOT NULL,
    row_id     UUID NOT NULL,
    -- NULL: send the whole record (it is new to the other side). Otherwise the changed fields.
    fields     TEXT[],
    -- Order of sending; a record changed again moves to the end.
    seq        BIGINT NOT NULL,
    hlc        TEXT NOT NULL,
    changed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (table_name, row_id)
);
CREATE INDEX sync_changes_seq_idx ON sync_changes (seq);

-- Field-level timestamps: for each field changed after its record was created, the stamp
-- of its current value, which side wrote it, and the latest stamp received from the other
-- side (so a replayed change is recognised).
CREATE TABLE sync_field_stamps (
    id         UUID PRIMARY KEY DEFAULT sync_uuid_v7(),
    school_id  UUID NOT NULL REFERENCES schools(id),
    table_name TEXT NOT NULL,
    row_id     UUID NOT NULL,
    field      TEXT NOT NULL,
    hlc        TEXT NOT NULL,
    origin     TEXT NOT NULL CHECK (origin IN ('school', 'head_office')),
    remote_hlc TEXT,
    UNIQUE (table_name, row_id, field)
);

-- The overwrite log: every value that sync replaced or refused.
CREATE TABLE sync_overwrites (
    id              UUID PRIMARY KEY,
    school_id       UUID NOT NULL REFERENCES schools(id),
    table_name      TEXT NOT NULL,
    row_id          UUID NOT NULL,
    field           TEXT NOT NULL,
    kept_value      JSONB,
    discarded_value JSONB,
    kept_origin     TEXT NOT NULL CHECK (kept_origin IN ('school', 'head_office')),
    kept_hlc        TEXT NOT NULL,
    discarded_hlc   TEXT NOT NULL,
    -- The user whose edit lost, when the record says so.
    discarded_by    UUID,
    reason          TEXT NOT NULL,
    at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- FALSE while the other side has not been told about it yet.
    reported        BOOLEAN NOT NULL DEFAULT FALSE
);
CREATE INDEX sync_overwrites_at_idx ON sync_overwrites (at DESC);
CREATE INDEX sync_overwrites_unreported_idx ON sync_overwrites (at) WHERE NOT reported;

-- Incoming records that could not be applied yet, with the reason; retried at every sync.
CREATE TABLE sync_parked (
    id         UUID PRIMARY KEY,
    school_id  UUID NOT NULL REFERENCES schools(id),
    table_name TEXT NOT NULL,
    row_id     UUID NOT NULL,
    change     JSONB NOT NULL,
    reason     TEXT NOT NULL,
    attempts   INTEGER NOT NULL DEFAULT 1,
    first_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (table_name, row_id)
);

-- A record as JSON for sending: numbers become strings so that money never passes through
-- a floating-point number on the way ("2500.00" stays "2500.00").
CREATE FUNCTION sync_row(rec jsonb) RETURNS jsonb AS $$
    SELECT COALESCE(
        jsonb_object_agg(e.key,
            CASE WHEN jsonb_typeof(e.value) = 'number' THEN to_jsonb(e.value #>> '{}') ELSE e.value END),
        '{}'::jsonb)
    FROM jsonb_each(rec) e;
$$ LANGUAGE sql IMMUTABLE;

-- Change capture. Attached to a table as
--   CREATE TRIGGER sync_capture AFTER INSERT OR UPDATE ON <table>
--   FOR EACH ROW EXECUTE FUNCTION sync_capture('<mode>', '<untracked columns, comma separated>')
-- It notes which record changed and which fields, and stamps the changed fields of a
-- field-level table. It does nothing while a batch from the other side is being applied
-- (madrasa.sync_apply = 'on'), so a received change is never sent back.
CREATE FUNCTION sync_capture() RETURNS trigger AS $$
DECLARE
    v_mode      text := TG_ARGV[0];
    v_untracked text[] := string_to_array(COALESCE(TG_ARGV[1], ''), ',');
    v_new       jsonb := to_jsonb(NEW);
    v_old       jsonb;
    v_school    uuid;
    v_role      text;
    v_changed   text[];
BEGIN
    IF current_setting('madrasa.sync_apply', true) = 'on' THEN
        RETURN NULL;
    END IF;
    IF TG_TABLE_NAME = 'schools' THEN
        v_school := NEW.id;
    ELSE
        v_school := (v_new ->> 'school_id')::uuid;
    END IF;
    IF v_school IS NULL THEN
        RETURN NULL;
    END IF;
    SELECT role INTO v_role FROM sync_state WHERE school_id = v_school AND enabled;
    IF v_role IS NULL THEN
        RETURN NULL;
    END IF;

    IF TG_OP = 'INSERT' THEN
        INSERT INTO sync_changes (school_id, table_name, row_id, fields, seq, hlc)
        VALUES (v_school, TG_TABLE_NAME, NEW.id, NULL, nextval('sync_change_seq'), v_new ->> 'hlc')
        ON CONFLICT (table_name, row_id) DO UPDATE
            SET fields = NULL, seq = EXCLUDED.seq, hlc = EXCLUDED.hlc, changed_at = now();
        RETURN NULL;
    END IF;

    v_old := to_jsonb(OLD);
    SELECT array_agg(n.key ORDER BY n.key) INTO v_changed
    FROM jsonb_each(v_new) n
    WHERE n.key <> ALL (v_untracked) AND n.value IS DISTINCT FROM v_old -> n.key;
    IF v_changed IS NULL THEN
        RETURN NULL;
    END IF;

    INSERT INTO sync_changes (school_id, table_name, row_id, fields, seq, hlc)
    VALUES (v_school, TG_TABLE_NAME, NEW.id, v_changed, nextval('sync_change_seq'), v_new ->> 'hlc')
    ON CONFLICT (table_name, row_id) DO UPDATE
        SET fields = CASE
                WHEN sync_changes.fields IS NULL THEN NULL
                ELSE (SELECT array_agg(DISTINCT f ORDER BY f)
                      FROM unnest(sync_changes.fields || EXCLUDED.fields) f)
            END,
            seq = EXCLUDED.seq, hlc = EXCLUDED.hlc, changed_at = now();

    IF v_mode = 'fields' THEN
        INSERT INTO sync_field_stamps (school_id, table_name, row_id, field, hlc, origin)
        SELECT v_school, TG_TABLE_NAME, NEW.id, f, v_new ->> 'hlc', v_role FROM unnest(v_changed) f
        ON CONFLICT (table_name, row_id, field) DO UPDATE
            SET hlc = EXCLUDED.hlc, origin = EXCLUDED.origin;
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;
