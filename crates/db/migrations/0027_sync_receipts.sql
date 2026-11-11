-- Nexus sync, stage 3: receipts are always merged (docs/adr/0005-nexus-hub-and-sync.md).
--
-- Receipts stay immutable for every user and every screen. Two narrow exceptions apply only
-- while the sync engine is merging a batch from the other side (madrasa.sync_apply = 'on'):
--
--   * the receipt NUMBER may change, and nothing else, when two different receipts turn out
--     to carry the same number (the later one gets a suffix so both are kept);
--   * a REVERSAL may be removed when the same receipt was reversed on both sides, so the
--     reversal is counted once.
--
-- Both are written to the overwrite log (sync_overwrites) with the old value.
CREATE OR REPLACE FUNCTION fee_receipts_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF current_setting('madrasa.sync_apply', true) = 'on' THEN
        IF TG_OP = 'UPDATE' AND (to_jsonb(NEW) - 'receipt_no') = (to_jsonb(OLD) - 'receipt_no') THEN
            RETURN NEW;
        END IF;
        IF TG_OP = 'DELETE' AND OLD.reverses_id IS NOT NULL THEN
            RETURN OLD;
        END IF;
    END IF;
    RAISE EXCEPTION 'fee_receipts are immutable; record a reversal instead';
END;
$$;

-- Change capture, second version. A record whose only change is a derived column (a
-- sheet's totals after a correction, a voucher's paid amount) now sends its bookkeeping
-- columns (updated_at, updated_by, hlc), so "last changed" agrees on both sides. The
-- derived values themselves are still never sent: each side counts them.
--
-- Trigger arguments: mode, columns that are never tracked (excluded and bookkeeping),
-- derived columns.
CREATE OR REPLACE FUNCTION sync_capture() RETURNS trigger AS $$
DECLARE
    v_mode      text := TG_ARGV[0];
    v_untracked text[] := string_to_array(COALESCE(TG_ARGV[1], ''), ',');
    v_derived   text[] := string_to_array(COALESCE(TG_ARGV[2], ''), ',');
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
    WHERE n.key <> ALL (v_untracked) AND n.key <> ALL (v_derived)
      AND n.value IS DISTINCT FROM v_old -> n.key;
    IF v_changed IS NULL THEN
        -- Only a derived column changed: send the bookkeeping columns, no fields.
        IF EXISTS (SELECT 1 FROM unnest(v_derived) d
                   WHERE d <> '' AND v_new -> d IS DISTINCT FROM v_old -> d) THEN
            v_changed := '{}';
        ELSE
            RETURN NULL;
        END IF;
    END IF;

    INSERT INTO sync_changes (school_id, table_name, row_id, fields, seq, hlc)
    VALUES (v_school, TG_TABLE_NAME, NEW.id, v_changed, nextval('sync_change_seq'), v_new ->> 'hlc')
    ON CONFLICT (table_name, row_id) DO UPDATE
        SET fields = CASE
                WHEN sync_changes.fields IS NULL THEN NULL
                ELSE COALESCE((SELECT array_agg(DISTINCT f ORDER BY f)
                               FROM unnest(sync_changes.fields || EXCLUDED.fields) f), '{}')
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
