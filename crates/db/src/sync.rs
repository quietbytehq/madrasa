//! Sync with a Madrasa Nexus hub: change capture, collecting what to send, applying what
//! arrives, the overwrite log (ADR 0005). The rules are in `madrasa_core::sync`; this module
//! carries them out in SQL. A school's API and the hub both call it, each on its own copy of
//! the school's data, so the two ends cannot drift apart.
//!
//! Nothing here runs for a school that has not joined a hub: the capture trigger is attached
//! by [`enable`] and removed by [`disable`].

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Utc};
use madrasa_core::hlc::{Hlc, HlcStamp};
use madrasa_core::ids::new_id;
use madrasa_core::sync::merge::LocalStamp;
use madrasa_core::sync::{
    insertion_order, plan_update, policy, ApplyReport, Change, LocalRow, Mode, Origin, Overwrite,
    Parked, TablePolicy, META_FIELDS, TABLES,
};
use serde::Serialize;
use serde_json::{Map, Value};
use sqlx::{AssertSqlSafe, Connection, PgConnection};
use uuid::Uuid;

use crate::{DbError, DbResult};

pub mod round;
mod special;

/// Session setting that silences the capture trigger while a received batch is applied, so
/// a change that came from the other side is never sent back to it.
const APPLY_FLAG: &str = "madrasa.sync_apply";

/// Name of the capture trigger on every synced table.
const TRIGGER: &str = "sync_capture";

fn sql(text: String) -> AssertSqlSafe<String> {
    AssertSqlSafe(text)
}

/// Quote a column name read from the catalogue.
fn ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

// ---- state ------------------------------------------------------------------------------

/// The sync state of this copy: the school it belongs to and which side it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncState {
    pub school_id: Uuid,
    pub role: Origin,
    pub enabled: bool,
}

/// What a status screen needs.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SyncStatus {
    pub enabled: bool,
    pub role: Option<Origin>,
    /// Records changed here and not yet sent.
    pub pending: i64,
    /// Incoming records waiting to be applied.
    pub parked: i64,
    /// Entries in the overwrite log.
    pub overwrites: i64,
    /// When the last round completed.
    pub last_sync_at: Option<DateTime<Utc>>,
    /// Why the last attempt failed, in plain words; `None` after a successful round.
    pub last_error: Option<String>,
    /// What the last successful round did.
    pub last_report: Option<Value>,
}

/// When the last round completed, why the last attempt failed, what the last round did.
type LastRound = (Option<DateTime<Utc>>, Option<String>, Option<Value>);

/// The sync state, if this copy has ever been joined.
pub async fn state(conn: &mut PgConnection) -> DbResult<Option<SyncState>> {
    let row: Option<(Uuid, String, bool)> = sqlx::query_as(
        "SELECT school_id, role, enabled FROM sync_state ORDER BY enabled_at LIMIT 1",
    )
    .fetch_optional(conn)
    .await?;
    Ok(row.and_then(|(school_id, role, enabled)| {
        Origin::parse(&role).map(|role| SyncState {
            school_id,
            role,
            enabled,
        })
    }))
}

async fn active(conn: &mut PgConnection) -> DbResult<SyncState> {
    match state(conn).await? {
        Some(s) if s.enabled => Ok(s),
        _ => Err(DbError::Conflict(
            "sync is not switched on for this school".into(),
        )),
    }
}

pub async fn status(conn: &mut PgConnection) -> DbResult<SyncStatus> {
    let st = state(&mut *conn).await?;
    let (pending, parked, overwrites): (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM sync_changes), (SELECT count(*) FROM sync_parked),
                (SELECT count(*) FROM sync_overwrites)",
    )
    .fetch_one(&mut *conn)
    .await?;
    let last: Option<LastRound> = sqlx::query_as(
        "SELECT last_sync_at, last_error, last_report FROM sync_state ORDER BY enabled_at LIMIT 1",
    )
    .fetch_optional(&mut *conn)
    .await?;
    let (last_sync_at, last_error, last_report) = last.unwrap_or_default();
    Ok(SyncStatus {
        enabled: st.is_some_and(|s| s.enabled),
        role: st.map(|s| s.role),
        pending,
        parked,
        overwrites,
        last_sync_at,
        last_error,
        last_report,
    })
}

/// Note how a round went: `Ok(report)` for a completed round, `Err(message)` for one that
/// could not reach the hub or was refused. Local bookkeeping; not audited, not synced.
pub async fn note_round(
    conn: &mut PgConnection,
    outcome: Result<&round::RoundReport, &str>,
) -> DbResult<()> {
    match outcome {
        Ok(report) => {
            sqlx::query(
                "UPDATE sync_state SET last_sync_at = now(), last_error = NULL, last_report = $1",
            )
            .bind(serde_json::to_value(report).unwrap_or(Value::Null))
            .execute(conn)
            .await?;
        }
        Err(message) => {
            sqlx::query("UPDATE sync_state SET last_error = $1")
                .bind(message)
                .execute(conn)
                .await?;
        }
    }
    Ok(())
}

/// The schema version of this copy: the number of the latest migration applied.
pub async fn schema_version(conn: &mut PgConnection) -> DbResult<i64> {
    Ok(
        sqlx::query_scalar("SELECT COALESCE(max(version), 0) FROM _sqlx_migrations WHERE success")
            .fetch_one(conn)
            .await?,
    )
}

/// Tables that sync and exist in this schema, as (policy, column names).
async fn synced_tables(conn: &mut PgConnection) -> DbResult<Vec<&'static TablePolicy>> {
    let mut out = Vec::new();
    for p in TABLES.iter().filter(|p| p.is_synced()) {
        let exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(p.table)
            .fetch_one(&mut *conn)
            .await?;
        if exists {
            out.push(p);
        }
    }
    Ok(out)
}

/// How many tables carry the capture trigger (0 for a school that has not joined).
pub async fn triggers_installed(conn: &mut PgConnection) -> DbResult<i64> {
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM pg_trigger t JOIN pg_class c ON c.oid = t.tgrelid
         WHERE t.tgname = $1 AND NOT t.tgisinternal AND c.relnamespace = current_schema()::regnamespace",
    )
    .bind(TRIGGER)
    .fetch_one(conn)
    .await?)
}

async fn install_triggers(conn: &mut PgConnection) -> DbResult<()> {
    for p in synced_tables(&mut *conn).await? {
        let table = p.table;
        sqlx::query(sql(format!("DROP TRIGGER IF EXISTS {TRIGGER} ON {table}")))
            .execute(&mut *conn)
            .await?;
        // Both arguments come from the static policy, never from data.
        sqlx::query(sql(format!(
            "CREATE TRIGGER {TRIGGER} AFTER INSERT OR UPDATE ON {table}
             FOR EACH ROW EXECUTE FUNCTION sync_capture('{}', '{}', '{}')",
            p.mode.as_str(),
            p.untracked_columns().join(","),
            p.derived.join(",")
        )))
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

async fn remove_triggers(conn: &mut PgConnection) -> DbResult<()> {
    for p in synced_tables(&mut *conn).await? {
        sqlx::query(sql(format!(
            "DROP TRIGGER IF EXISTS {TRIGGER} ON {}",
            p.table
        )))
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

/// Switch change capture on for a school: from now on every insert and update of a synced
/// table is noted for sending. `role` says which side this copy is. Call it again after a
/// migration added tables; it is idempotent.
pub async fn enable(conn: &mut PgConnection, school_id: Uuid, role: Origin) -> DbResult<()> {
    // Joining again starts the "last sync" record afresh; switching capture back on for a
    // school that is already joined (after an upgrade) keeps it.
    sqlx::query(
        "INSERT INTO sync_state (school_id, role, enabled) VALUES ($1, $2, TRUE)
         ON CONFLICT (school_id) DO UPDATE SET
             role = EXCLUDED.role,
             last_sync_at = CASE WHEN sync_state.enabled THEN sync_state.last_sync_at END,
             last_error = CASE WHEN sync_state.enabled THEN sync_state.last_error END,
             last_report = CASE WHEN sync_state.enabled THEN sync_state.last_report END,
             enabled_at = CASE WHEN sync_state.enabled THEN sync_state.enabled_at ELSE now() END,
             enabled = TRUE",
    )
    .bind(school_id)
    .bind(role.as_str())
    .execute(&mut *conn)
    .await?;
    install_triggers(conn).await
}

/// The school's own `schools` record as a whole-record change: what a school sends to join
/// a hub, before anything is queued.
pub async fn school_record(conn: &mut PgConnection, school_id: Uuid) -> DbResult<Change> {
    let mut tx = conn.begin().await?;
    sqlx::query("SELECT set_config('TimeZone', 'UTC', true)")
        .execute(&mut *tx)
        .await?;
    let row: Option<Value> =
        sqlx::query_scalar("SELECT sync_row(to_jsonb(t)) FROM schools t WHERE id = $1")
            .bind(school_id)
            .fetch_optional(&mut *tx)
            .await?;
    tx.commit().await?;
    let Some(Value::Object(row)) = row else {
        return Err(DbError::NotFound("school".into()));
    };
    let hlc = row
        .get("hlc")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    Ok(Change {
        table: "schools".into(),
        id: school_id,
        fields: None,
        row,
        stamps: BTreeMap::new(),
        hlc,
        seq: 0,
    })
}

/// Start the other copy of a school: store its `schools` record (the first change a school
/// sends) and switch capture on as `role`. The hub calls this when a school joins.
pub async fn adopt_school(conn: &mut PgConnection, school: &Change, role: Origin) -> DbResult<()> {
    let Some(p) =
        policy("schools").filter(|_| school.table == "schools" && school.fields.is_none())
    else {
        return Err(DbError::Conflict(
            "a school joins with its school record".into(),
        ));
    };
    let mut tx = conn.begin().await?;
    sqlx::query("SELECT set_config('TimeZone', 'UTC', true), set_config($1, 'on', true)")
        .bind(APPLY_FLAG)
        .execute(&mut *tx)
        .await?;
    let clock = Hlc::new(0);
    let mut ap = Applier {
        state: SyncState {
            school_id: school.id,
            role,
            enabled: true,
        },
        hlc: &clock,
        columns: HashMap::new(),
        report: ApplyReport::default(),
        vouchers: HashSet::new(),
        sheets: HashSet::new(),
    };
    let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM schools WHERE id = $1)")
        .bind(school.id)
        .fetch_one(&mut *tx)
        .await?;
    if !exists {
        ap.insert_many(&mut tx, p, &[school]).await?;
    }
    sqlx::query("SELECT set_config($1, 'off', true)")
        .bind(APPLY_FLAG)
        .execute(&mut *tx)
        .await?;
    enable(&mut tx, school.id, role).await?;
    tx.commit().await?;
    Ok(())
}

/// Switch sync off: remove the triggers and forget what was waiting to be sent or applied.
/// The school's own records are not touched. The overwrite log is kept as history.
pub async fn disable(conn: &mut PgConnection) -> DbResult<()> {
    remove_triggers(&mut *conn).await?;
    for stmt in [
        "UPDATE sync_state SET enabled = FALSE",
        "DELETE FROM sync_changes",
        "DELETE FROM sync_field_stamps",
        "DELETE FROM sync_parked",
    ] {
        sqlx::query(stmt).execute(&mut *conn).await?;
    }
    Ok(())
}

/// Queue every existing record for sending (the first upload after joining, or a full
/// resend). Tables are queued parents first so records arrive after the ones they refer to.
/// Returns how many records were queued.
pub async fn queue_snapshot(conn: &mut PgConnection) -> DbResult<i64> {
    let st = active(&mut *conn).await?;
    let tables = synced_tables(&mut *conn).await?;
    let names: Vec<&str> = tables.iter().map(|p| p.table).collect();
    let edges: Vec<(String, String)> = sqlx::query_as(
        "SELECT c.relname::text, p.relname::text FROM pg_constraint k
         JOIN pg_class c ON c.oid = k.conrelid JOIN pg_class p ON p.oid = k.confrelid
         WHERE k.contype = 'f' AND c.relnamespace = current_schema()::regnamespace",
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut queued = 0i64;
    for table in insertion_order(&names, &edges) {
        let owner = if table == "schools" {
            "id"
        } else {
            "school_id"
        };
        let done = sqlx::query(sql(format!(
            "INSERT INTO sync_changes (school_id, table_name, row_id, fields, seq, hlc)
             SELECT $1, '{table}', s.id, NULL, nextval('sync_change_seq'), s.hlc
             FROM (SELECT id, hlc FROM {table} WHERE {owner} = $1 ORDER BY id) s
             ON CONFLICT (table_name, row_id) DO UPDATE
                 SET fields = NULL, seq = EXCLUDED.seq, hlc = EXCLUDED.hlc, changed_at = now()"
        )))
        .bind(st.school_id)
        .execute(&mut *conn)
        .await?;
        queued += done.rows_affected() as i64;
    }
    Ok(queued)
}

// ---- sending ----------------------------------------------------------------------------

/// A row of `sync_changes`: table, record, changed fields (`None` = whole record), sequence
/// number, stamp.
type PendingRow = (String, Uuid, Option<Vec<String>>, i64, String);

/// The next batch to send, oldest change first. Values are read from the records as they
/// are now, so several edits of one record travel as one change. Run it in a transaction.
pub async fn collect(conn: &mut PgConnection, limit: i64) -> DbResult<Vec<Change>> {
    let mut tx = conn.begin().await?;
    // Timestamps are written in UTC on both sides so equal moments compare equal.
    sqlx::query("SELECT set_config('TimeZone', 'UTC', true)")
        .execute(&mut *tx)
        .await?;
    let pending: Vec<PendingRow> = sqlx::query_as(
        "SELECT table_name, row_id, fields, seq, hlc FROM sync_changes ORDER BY seq LIMIT $1",
    )
    .bind(limit)
    .fetch_all(&mut *tx)
    .await?;

    let mut ids_by_table: BTreeMap<&str, Vec<Uuid>> = BTreeMap::new();
    for (table, id, ..) in &pending {
        ids_by_table.entry(table.as_str()).or_default().push(*id);
    }
    let mut rows: HashMap<(String, Uuid), Map<String, Value>> = HashMap::new();
    let mut stamps: HashMap<(String, Uuid), BTreeMap<String, String>> = HashMap::new();
    for (table, ids) in &ids_by_table {
        let Some(p) = policy(table).filter(|p| p.is_synced()) else {
            continue;
        };
        let found: Vec<(Uuid, Value)> = sqlx::query_as(sql(format!(
            "SELECT id, sync_row(to_jsonb(t)) FROM {} t WHERE id = ANY($1)",
            p.table
        )))
        .bind(ids)
        .fetch_all(&mut *tx)
        .await?;
        for (id, value) in found {
            if let Value::Object(map) = value {
                rows.insert((table.to_string(), id), map);
            }
        }
        if p.mode == Mode::Fields {
            let found: Vec<(Uuid, String, String)> = sqlx::query_as(
                "SELECT row_id, field, hlc FROM sync_field_stamps WHERE table_name = $1 AND row_id = ANY($2)",
            )
            .bind(table)
            .bind(ids)
            .fetch_all(&mut *tx)
            .await?;
            for (id, field, hlc) in found {
                stamps
                    .entry((table.to_string(), id))
                    .or_default()
                    .insert(field, hlc);
            }
        }
    }

    let mut out = Vec::with_capacity(pending.len());
    let mut stale: Vec<i64> = Vec::new();
    for (table, id, fields, seq, hlc) in pending {
        let key = (table.clone(), id);
        let (Some(p), Some(mut row)) =
            (policy(&table).filter(|p| p.is_synced()), rows.remove(&key))
        else {
            // A table this version no longer syncs, or a record that is gone: nothing to send.
            stale.push(seq);
            continue;
        };
        row.retain(|k, _| !p.is_excluded(k));
        let mut field_stamps = BTreeMap::new();
        if let Some(list) = &fields {
            row.retain(|k, _| k == "id" || META_FIELDS.contains(&k.as_str()) || list.contains(k));
            if let Some(known) = stamps.get(&key) {
                for f in list {
                    if let Some(s) = known.get(f) {
                        field_stamps.insert(f.clone(), s.clone());
                    }
                }
            }
        }
        // The record's own stamp as it is now (it may be later than when the change was noted).
        let hlc = row
            .get("hlc")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or(hlc);
        out.push(Change {
            table,
            id,
            fields,
            row,
            stamps: field_stamps,
            hlc,
            seq,
        });
    }
    if !stale.is_empty() {
        sqlx::query("DELETE FROM sync_changes WHERE seq = ANY($1)")
            .bind(&stale)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(out)
}

/// The other side has dealt with these changes: forget them. A record changed again since
/// it was sent has a new sequence number and stays queued.
pub async fn acknowledge(conn: &mut PgConnection, seqs: &[i64]) -> DbResult<u64> {
    if seqs.is_empty() {
        return Ok(0);
    }
    Ok(sqlx::query("DELETE FROM sync_changes WHERE seq = ANY($1)")
        .bind(seqs)
        .execute(conn)
        .await?
        .rows_affected())
}

// ---- receiving --------------------------------------------------------------------------

enum Outcome {
    Applied,
    Unchanged,
    /// Could not be applied now; the reason is plain language.
    Failed(String),
}

/// Why a record could not be stored, for errors that are about the data and not about the
/// connection. `None` means the error is not one to park a record for.
fn data_error(err: &sqlx::Error) -> Option<String> {
    let sqlx::Error::Database(db) = err else {
        return None;
    };
    let code = db.code().map(|c| c.to_string()).unwrap_or_default();
    // The constraint name says which rule; the error detail may quote values, so it is not used.
    let rule = db
        .constraint()
        .map(|c| format!(" ({c})"))
        .unwrap_or_default();
    match code.as_str() {
        "23503" => Some(format!(
            "It refers to a record that has not arrived here yet{rule}."
        )),
        "23505" => Some(format!(
            "Another record here already uses the same value{rule}."
        )),
        c if c.starts_with("23") => Some(format!("It breaks a rule of the school data{rule}.")),
        // Bad value for a column, unknown column or table, or a rule raised by a trigger.
        c if c.starts_with("22") || c.starts_with("42") || c == "P0001" => {
            Some("It could not be stored by this version of Madrasa.".to_string())
        }
        _ => None,
    }
}

struct Applier<'a> {
    state: SyncState,
    /// This copy's clock: stamps the records a merge has to rewrite.
    hlc: &'a Hlc,
    columns: HashMap<&'static str, Vec<String>>,
    report: ApplyReport,
    /// Vouchers whose paid amount must be recounted from their receipts after the batch.
    vouchers: HashSet<Uuid>,
    /// Attendance sheets whose totals must be recounted from their entries after the batch.
    sheets: HashSet<Uuid>,
}

impl Applier<'_> {
    async fn columns_of(
        &mut self,
        conn: &mut PgConnection,
        table: &'static str,
    ) -> DbResult<&Vec<String>> {
        if !self.columns.contains_key(table) {
            let cols: Vec<String> = sqlx::query_scalar(
                "SELECT attname::text FROM pg_attribute
                 WHERE attrelid = to_regclass($1) AND attnum > 0 AND NOT attisdropped ORDER BY attnum",
            )
            .bind(table)
            .fetch_all(&mut *conn)
            .await?;
            self.columns.insert(table, cols);
        }
        Ok(&self.columns[table])
    }

    /// The record with placeholders for the columns that are never sent.
    fn payload(p: &TablePolicy, change: &Change) -> Map<String, Value> {
        let mut row = change.row.clone();
        for (column, placeholder) in p.exclude {
            row.remove(*column);
            if let Some(v) = placeholder.and_then(|text| serde_json::from_str::<Value>(text).ok()) {
                row.insert((*column).to_string(), v);
            }
        }
        row
    }

    /// Does the record belong to the school this copy is for? Anything else is refused.
    fn belongs(&self, table: &str, change: &Change) -> bool {
        let expected = self.state.school_id.to_string();
        let owner = if table == "schools" {
            Some(change.id.to_string())
        } else {
            change
                .row
                .get("school_id")
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        // A partial change carries no school_id; the record it updates was checked on arrival.
        owner.is_none_or(|o| o == expected)
    }

    /// Insert several new records of one table in one statement.
    async fn insert_many(
        &mut self,
        conn: &mut PgConnection,
        p: &'static TablePolicy,
        changes: &[&Change],
    ) -> Result<(), sqlx::Error> {
        let payloads: Vec<Value> = changes
            .iter()
            .map(|c| Value::Object(Self::payload(p, c)))
            .collect();
        let known = self
            .columns_of(conn, p.table)
            .await
            .map_err(|e| match e {
                DbError::Sqlx(e) => e,
                other => sqlx::Error::Protocol(other.to_string()),
            })?
            .clone();
        let first = payloads[0].as_object().cloned().unwrap_or_default();
        let cols: Vec<String> = known
            .iter()
            .filter(|c| first.contains_key(*c))
            .map(|c| ident(c))
            .collect();
        let list = cols.join(", ");
        sqlx::query(sql(format!(
            "INSERT INTO {t} ({list}) SELECT {list} FROM jsonb_populate_recordset(NULL::{t}, $1)",
            t = p.table
        )))
        .bind(Value::Array(payloads))
        .execute(conn)
        .await?;
        Ok(())
    }

    /// Apply one change. Database rule violations come back as `Outcome::Failed`.
    async fn apply_one(&mut self, conn: &mut PgConnection, change: &Change) -> DbResult<Outcome> {
        let Some(p) = policy(&change.table).filter(|p| p.is_synced()) else {
            // A table this version does not sync: nothing to do, nothing to wait for.
            return Ok(Outcome::Unchanged);
        };
        if !self.belongs(p.table, change) {
            tracing::warn!(table = p.table, "sync: refused a record of another school");
            return Ok(Outcome::Unchanged);
        }
        let mut sp = conn.begin().await?;
        match self.try_apply(&mut sp, p, change).await {
            Ok(outcome) => {
                sp.commit().await?;
                Ok(outcome)
            }
            Err(DbError::Sqlx(e)) => match data_error(&e) {
                Some(reason) => {
                    sp.rollback().await?;
                    Ok(Outcome::Failed(reason))
                }
                None => Err(DbError::Sqlx(e)),
            },
            Err(other) => Err(other),
        }
    }

    async fn try_apply(
        &mut self,
        conn: &mut PgConnection,
        p: &'static TablePolicy,
        change: &Change,
    ) -> DbResult<Outcome> {
        let table = p.table;
        let local: Option<Value> = sqlx::query_scalar(sql(format!(
            "SELECT sync_row(to_jsonb(t)) FROM {table} t WHERE id = $1 FOR UPDATE"
        )))
        .bind(change.id)
        .fetch_optional(&mut *conn)
        .await?;

        let Some(Value::Object(local)) = local else {
            if change.fields.is_some() {
                return Ok(Outcome::Failed(
                    "The record these changes belong to has not arrived here yet.".into(),
                ));
            }
            // Receipts and attendance have their own rules for a record that meets a twin.
            if let Some(outcome) = self.insert_special(conn, p, change).await? {
                return Ok(outcome);
            }
            self.insert_many(conn, p, &[change]).await?;
            self.touch(p.table, &change.row);
            return Ok(Outcome::Applied);
        };
        self.touch(table, &local);
        self.touch(table, &change.row);

        let dirty: Option<Option<Vec<String>>> = sqlx::query_scalar(
            "SELECT fields FROM sync_changes WHERE table_name = $1 AND row_id = $2",
        )
        .bind(table)
        .bind(change.id)
        .fetch_optional(&mut *conn)
        .await?;
        let whole_dirty = matches!(dirty, Some(None));
        let dirty_fields = dirty.flatten().unwrap_or_default();
        let mut stamps: BTreeMap<String, LocalStamp> = BTreeMap::new();
        if p.mode == Mode::Fields {
            let known: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
                "SELECT field, hlc, origin, remote_hlc FROM sync_field_stamps
                 WHERE table_name = $1 AND row_id = $2",
            )
            .bind(table)
            .bind(change.id)
            .fetch_all(&mut *conn)
            .await?;
            for (field, hlc, origin, remote_hlc) in known {
                stamps.insert(
                    field,
                    LocalStamp {
                        hlc,
                        origin: Origin::parse(&origin).unwrap_or(self.state.role),
                        remote_hlc,
                    },
                );
            }
        }
        let plan = plan_update(
            p,
            self.state.role,
            &LocalRow {
                row: &local,
                whole_dirty,
                dirty_fields: &dirty_fields,
                stamps: &stamps,
            },
            change,
        );

        if plan.changes_row() {
            let known = self.columns_of(conn, table).await?.clone();
            let mut cols: Vec<&str> = plan.set.iter().map(String::as_str).collect();
            if plan.take_meta {
                cols.extend(
                    META_FIELDS
                        .iter()
                        .copied()
                        .filter(|m| change.row.contains_key(*m)),
                );
            }
            let cols: Vec<String> = cols
                .into_iter()
                .filter(|c| known.iter().any(|k| k == c))
                .map(ident)
                .collect();
            if !cols.is_empty() {
                let list = cols.join(", ");
                sqlx::query(sql(format!(
                    "UPDATE {table} SET ({list}) = (SELECT {list} FROM jsonb_populate_record(NULL::{table}, $2))
                     WHERE id = $1"
                )))
                .bind(change.id)
                .bind(Value::Object(change.row.clone()))
                .execute(&mut *conn)
                .await?;
            }
        }

        let remote = self.state.role.other();
        for (field, stamp) in &plan.applied_stamps {
            sqlx::query(
                "INSERT INTO sync_field_stamps (school_id, table_name, row_id, field, hlc, origin, remote_hlc)
                 VALUES ($1, $2, $3, $4, $5, $6, $5)
                 ON CONFLICT (table_name, row_id, field) DO UPDATE
                     SET hlc = EXCLUDED.hlc, origin = EXCLUDED.origin, remote_hlc = EXCLUDED.remote_hlc",
            )
            .bind(self.state.school_id)
            .bind(table)
            .bind(change.id)
            .bind(field)
            .bind(stamp)
            .bind(remote.as_str())
            .execute(&mut *conn)
            .await?;
        }
        let local_hlc = local.get("hlc").and_then(Value::as_str).unwrap_or_default();
        for (field, stamp) in &plan.seen_stamps {
            sqlx::query(
                "INSERT INTO sync_field_stamps (school_id, table_name, row_id, field, hlc, origin, remote_hlc)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT (table_name, row_id, field) DO UPDATE
                     SET remote_hlc = GREATEST(sync_field_stamps.remote_hlc, EXCLUDED.remote_hlc)",
            )
            .bind(self.state.school_id)
            .bind(table)
            .bind(change.id)
            .bind(field)
            .bind(local_hlc)
            .bind(self.state.role.as_str())
            .bind(stamp)
            .execute(&mut *conn)
            .await?;
        }
        if !plan.clear_dirty.is_empty() && !whole_dirty {
            sqlx::query(
                "UPDATE sync_changes
                 SET fields = COALESCE((SELECT array_agg(f) FROM unnest(fields) f WHERE f <> ALL($3)), '{}')
                 WHERE table_name = $1 AND row_id = $2",
            )
            .bind(table)
            .bind(change.id)
            .bind(&plan.clear_dirty)
            .execute(&mut *conn)
            .await?;
            sqlx::query(
                "DELETE FROM sync_changes WHERE table_name = $1 AND row_id = $2 AND fields = '{}'",
            )
            .bind(table)
            .bind(change.id)
            .execute(&mut *conn)
            .await?;
        }
        for o in &plan.overwrites {
            let logged = log_overwrite(conn, self.state.school_id, o, false).await?;
            self.report.overwrites.push(logged);
        }
        Ok(if plan.changes_row() {
            Outcome::Applied
        } else {
            Outcome::Unchanged
        })
    }
}

/// Fold a later change of the same record into one that is still waiting.
fn overlay(base: &mut Change, newer: Change) {
    match (&mut base.fields, newer.fields.clone()) {
        // A whole record replaces whatever was waiting.
        (_, None) => {
            *base = newer;
            return;
        }
        // Changed fields on top of a waiting whole record: it stays a whole record.
        (None, Some(_)) => {}
        (Some(have), Some(more)) => {
            for f in more {
                if !have.contains(&f) {
                    have.push(f);
                }
            }
        }
    }
    for (k, v) in newer.row {
        base.row.insert(k, v);
    }
    base.stamps.extend(newer.stamps);
    if newer.hlc > base.hlc {
        base.hlc = newer.hlc;
    }
    base.seq = newer.seq;
}

struct Waiting {
    change: Change,
    reason: String,
    attempts: i32,
}

fn wait(list: &mut Vec<Waiting>, change: Change, reason: String) {
    match list
        .iter_mut()
        .find(|w| w.change.table == change.table && w.change.id == change.id)
    {
        Some(w) => {
            overlay(&mut w.change, change);
            w.reason = reason;
        }
        None => list.push(Waiting {
            change,
            reason,
            attempts: 0,
        }),
    }
}

/// Apply a batch received from the other side.
///
/// New records are inserted, changed fields are merged by the rules of
/// `madrasa_core::sync::merge`, values that lose are written to `sync_overwrites`, and a
/// record that cannot be stored yet is retried at the end and then parked with the reason.
/// One bad record never stops the rest. The whole batch is one transaction (or savepoint).
pub async fn apply(
    conn: &mut PgConnection,
    hlc: &Hlc,
    changes: &[Change],
) -> DbResult<ApplyReport> {
    let mut tx = conn.begin().await?;
    let st = active(&mut tx).await?;
    sqlx::query("SELECT set_config('TimeZone', 'UTC', true), set_config($1, 'on', true)")
        .bind(APPLY_FLAG)
        .execute(&mut *tx)
        .await?;
    let mut ap = Applier {
        state: st,
        hlc,
        columns: HashMap::new(),
        report: ApplyReport::default(),
        vouchers: HashSet::new(),
        sheets: HashSet::new(),
    };

    // Records parked by earlier batches are retried after this one.
    let parked: Vec<(Value, String, i32)> =
        sqlx::query_as("SELECT change, reason, attempts FROM sync_parked ORDER BY first_at, id")
            .fetch_all(&mut *tx)
            .await?;
    let mut waiting: Vec<Waiting> = parked
        .into_iter()
        .filter_map(|(change, reason, attempts)| {
            serde_json::from_value::<Change>(change)
                .ok()
                .map(|change| Waiting {
                    change,
                    reason,
                    attempts,
                })
        })
        .collect();

    // Fold every received stamp into this copy's clock first, so anything written here from
    // now on carries a later stamp than what arrived.
    let wall = Utc::now().timestamp_millis().max(0) as u64;
    for c in changes {
        for stamp in std::iter::once(&c.hlc).chain(c.stamps.values()) {
            if let Ok(parsed) = stamp.parse::<HlcStamp>() {
                hlc.observe(parsed, wall);
            }
            if ap.report.max_hlc.as_ref().is_none_or(|m| stamp > m) {
                ap.report.max_hlc = Some(stamp.clone());
            }
        }
    }

    let mut i = 0;
    // Records before this index go in one by one (a batch insert of their run failed).
    let mut one_by_one_until = 0;
    while i < changes.len() {
        let c = &changes[i];
        ap.report.handled.push(c.seq);
        // A run of new records of one table goes in as one statement (a first upload is
        // almost entirely such runs).
        let mut end = i + 1;
        if c.fields.is_none() {
            while end < changes.len()
                && changes[end].fields.is_none()
                && changes[end].table == c.table
            {
                end += 1;
            }
        }
        let run = &changes[i..end];
        let fast = match policy(&c.table).filter(|p| p.is_synced()) {
            Some(p)
                if i >= one_by_one_until
                    && run.len() > 1
                    && run.iter().all(|c| ap.belongs(p.table, c))
                    && ap.may_insert_together(&mut tx, p, run).await? =>
            {
                let ids: Vec<Uuid> = run.iter().map(|c| c.id).collect();
                let existing: Vec<Uuid> = sqlx::query_scalar(sql(format!(
                    "SELECT id FROM {} WHERE id = ANY($1)",
                    p.table
                )))
                .bind(&ids)
                .fetch_all(&mut *tx)
                .await?;
                if existing.is_empty() {
                    let refs: Vec<&Change> = run.iter().collect();
                    let mut sp = tx.begin().await?;
                    match ap.insert_many(&mut sp, p, &refs).await {
                        Ok(()) => {
                            sp.commit().await?;
                            true
                        }
                        Err(e) if data_error(&e).is_some() => {
                            sp.rollback().await?;
                            false
                        }
                        Err(e) => return Err(e.into()),
                    }
                } else {
                    false
                }
            }
            _ => false,
        };
        if fast {
            ap.report.applied += run.len() as u32;
            for c in run {
                ap.touch(&c.table, &c.row);
            }
            for c in &run[1..] {
                ap.report.handled.push(c.seq);
            }
            i = end;
            continue;
        }
        one_by_one_until = one_by_one_until.max(end);
        match ap.apply_one(&mut tx, c).await? {
            Outcome::Applied => ap.report.applied += 1,
            Outcome::Unchanged => ap.report.unchanged += 1,
            Outcome::Failed(reason) => wait(&mut waiting, c.clone(), reason),
        }
        i += 1;
    }

    // Retry what is waiting until a pass makes no progress: a child that arrived before its
    // parent goes in on the second pass.
    loop {
        let before = waiting.len();
        let mut still = Vec::new();
        for mut w in waiting {
            match ap.apply_one(&mut tx, &w.change).await? {
                Outcome::Applied => ap.report.applied += 1,
                Outcome::Unchanged => ap.report.unchanged += 1,
                Outcome::Failed(reason) => {
                    w.reason = reason;
                    still.push(w);
                }
            }
        }
        waiting = still;
        if waiting.is_empty() || waiting.len() == before {
            break;
        }
    }

    // Derived values follow from what is now here: paid amounts from receipts, sheet totals
    // from entries. Both sides compute them the same way, so they are not sent.
    ap.recount(&mut tx).await?;

    sqlx::query("DELETE FROM sync_parked")
        .execute(&mut *tx)
        .await?;
    for w in &waiting {
        sqlx::query(
            "INSERT INTO sync_parked (id, school_id, table_name, row_id, change, reason, attempts)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(new_id())
        .bind(st.school_id)
        .bind(&w.change.table)
        .bind(w.change.id)
        .bind(serde_json::to_value(&w.change).unwrap_or(Value::Null))
        .bind(&w.reason)
        .bind(w.attempts + 1)
        .execute(&mut *tx)
        .await?;
        ap.report.parked.push(Parked {
            table: w.change.table.clone(),
            id: w.change.id,
            reason: w.reason.clone(),
        });
    }

    sqlx::query("SELECT set_config($1, 'off', true)")
        .bind(APPLY_FLAG)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(ap.report)
}

// ---- overwrite log ----------------------------------------------------------------------

async fn log_overwrite(
    conn: &mut PgConnection,
    school_id: Uuid,
    o: &Overwrite,
    reported: bool,
) -> DbResult<Overwrite> {
    let uid = o.uid.unwrap_or_else(new_id);
    let at = o.at.unwrap_or_else(Utc::now);
    sqlx::query(
        "INSERT INTO sync_overwrites (id, school_id, table_name, row_id, field, kept_value, discarded_value,
             kept_origin, kept_hlc, discarded_hlc, discarded_by, reason, at, reported)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(uid)
    .bind(school_id)
    .bind(&o.table)
    .bind(o.id)
    .bind(&o.field)
    .bind(&o.kept)
    .bind(&o.discarded)
    .bind(o.kept_origin.as_str())
    .bind(&o.kept_hlc)
    .bind(&o.discarded_hlc)
    .bind(o.discarded_by)
    .bind(&o.reason)
    .bind(at)
    .bind(reported)
    .execute(conn)
    .await?;
    Ok(Overwrite {
        uid: Some(uid),
        at: Some(at),
        ..o.clone()
    })
}

/// Store log entries detected by the other side, so both sides hold the full log. Returns
/// the ids stored (or already present); the sender marks those as reported.
pub async fn store_remote_overwrites(
    conn: &mut PgConnection,
    entries: &[Overwrite],
) -> DbResult<Vec<Uuid>> {
    if entries.is_empty() {
        return Ok(Vec::new());
    }
    let st = active(&mut *conn).await?;
    let mut ids = Vec::with_capacity(entries.len());
    for o in entries {
        let logged = log_overwrite(&mut *conn, st.school_id, o, true).await?;
        ids.extend(logged.uid);
    }
    Ok(ids)
}

type OverwriteCols = (
    Uuid,
    String,
    Uuid,
    String,
    Option<Value>,
    Option<Value>,
    String,
    String,
    String,
    Option<Uuid>,
    String,
    DateTime<Utc>,
);

const OVERWRITE_COLS: &str =
    "id, table_name, row_id, field, kept_value, discarded_value, kept_origin, \
    kept_hlc, discarded_hlc, discarded_by, reason, at";

fn overwrite_from(r: OverwriteCols) -> Overwrite {
    Overwrite {
        uid: Some(r.0),
        at: Some(r.11),
        table: r.1,
        id: r.2,
        field: r.3,
        kept: r.4.unwrap_or(Value::Null),
        discarded: r.5.unwrap_or(Value::Null),
        kept_origin: Origin::parse(&r.6).unwrap_or(Origin::HeadOffice),
        kept_hlc: r.7,
        discarded_hlc: r.8,
        discarded_by: r.9,
        reason: r.10,
    }
}

/// Log entries the other side has not been told about yet, oldest first.
pub async fn unreported_overwrites(
    conn: &mut PgConnection,
    limit: i64,
) -> DbResult<Vec<Overwrite>> {
    let rows: Vec<OverwriteCols> = sqlx::query_as(sql(format!(
        "SELECT {OVERWRITE_COLS} FROM sync_overwrites WHERE NOT reported ORDER BY at, id LIMIT $1"
    )))
    .bind(limit)
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().map(overwrite_from).collect())
}

pub async fn mark_overwrites_reported(conn: &mut PgConnection, ids: &[Uuid]) -> DbResult<()> {
    if !ids.is_empty() {
        sqlx::query("UPDATE sync_overwrites SET reported = TRUE WHERE id = ANY($1)")
            .bind(ids)
            .execute(conn)
            .await?;
    }
    Ok(())
}

/// The overwrite log, newest first.
pub async fn list_overwrites(
    conn: &mut PgConnection,
    limit: i64,
    offset: i64,
) -> DbResult<Vec<Overwrite>> {
    let rows: Vec<OverwriteCols> = sqlx::query_as(sql(format!(
        "SELECT {OVERWRITE_COLS} FROM sync_overwrites ORDER BY at DESC, id DESC LIMIT $1 OFFSET $2"
    )))
    .bind(limit)
    .bind(offset)
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().map(overwrite_from).collect())
}

/// Incoming records waiting to be applied, oldest first.
pub async fn list_parked(conn: &mut PgConnection) -> DbResult<Vec<Parked>> {
    let rows: Vec<(String, Uuid, String)> =
        sqlx::query_as("SELECT table_name, row_id, reason FROM sync_parked ORDER BY first_at, id")
            .fetch_all(conn)
            .await?;
    Ok(rows
        .into_iter()
        .map(|(table, id, reason)| Parked { table, id, reason })
        .collect())
}

/// Tables of this schema that are neither in the sync policy nor sync bookkeeping. Empty
/// unless a migration added a table without classifying it (a test asserts that).
pub async fn unclassified_tables(conn: &mut PgConnection) -> DbResult<Vec<String>> {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name::text FROM information_schema.tables
         WHERE table_schema = current_schema() AND table_type = 'BASE TABLE' ORDER BY 1",
    )
    .fetch_all(conn)
    .await?;
    let known: HashSet<&str> = TABLES.iter().map(|p| p.table).collect();
    Ok(tables
        .into_iter()
        .filter(|t| {
            !known.contains(t.as_str()) && !t.starts_with("sync_") && t != "_sqlx_migrations"
        })
        .collect())
}
