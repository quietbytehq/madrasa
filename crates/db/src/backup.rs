//! Logical dump and restore of the whole database, plus the backup run history.
//!
//! The dump is a plain container of `COPY ... TO STDOUT` text per table (parents before
//! children, so a restore can load it in order). It carries no secrets beyond what the
//! database holds; encryption happens in `madrasa_core::backup::archive`. A restore empties
//! every table and loads the dump inside one transaction, with user triggers disabled so the
//! append-only audit log and the immutable receipts can be reloaded.

use crate::audit::{self, Action};
use crate::models::BackupRun;
use crate::{Actor, DbError, DbResult};
use futures_util::TryStreamExt;
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use sqlx::{Connection, PgConnection};
use std::collections::{BTreeMap, HashMap, HashSet};
use uuid::Uuid;

const MAGIC: &[u8] = b"MDUMP1\n";
const SKIP: &[&str] = &["_sqlx_migrations"];

/// Tables whose row counts are shown in a restore preview.
pub const COUNTED_TABLES: &[&str] = &[
    "schools",
    "users",
    "staff",
    "students",
    "guardians",
    "fee_vouchers",
    "fee_receipts",
    "attendance_sheets",
    "mark_sheets",
    "audit_log",
];

pub async fn migration_versions(conn: &mut PgConnection) -> DbResult<Vec<i64>> {
    Ok(
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations ORDER BY version")
            .fetch_all(conn)
            .await?,
    )
}

/// Every base table in dependency order: a table comes after the tables it references.
pub async fn table_order(conn: &mut PgConnection) -> DbResult<Vec<String>> {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables
         WHERE table_schema = 'public' AND table_type = 'BASE TABLE' ORDER BY table_name",
    )
    .fetch_all(&mut *conn)
    .await?;
    let tables: Vec<String> = tables
        .into_iter()
        .filter(|t| !SKIP.contains(&t.as_str()))
        .collect();
    let edges: Vec<(String, String)> = sqlx::query_as(
        "SELECT tc.table_name AS child, tp.table_name AS parent
         FROM information_schema.table_constraints tc
         JOIN information_schema.referential_constraints rc ON rc.constraint_name = tc.constraint_name
           AND rc.constraint_schema = tc.constraint_schema
         JOIN information_schema.table_constraints tp ON tp.constraint_name = rc.unique_constraint_name
           AND tp.constraint_schema = rc.unique_constraint_schema
         WHERE tc.constraint_type = 'FOREIGN KEY' AND tc.table_schema = 'public'",
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut deps: HashMap<&str, HashSet<&str>> = tables
        .iter()
        .map(|t| (t.as_str(), HashSet::new()))
        .collect();
    for (child, parent) in &edges {
        if child != parent {
            if let Some(d) = deps.get_mut(child.as_str()) {
                d.insert(parent.as_str());
            }
        }
    }
    let mut ordered: Vec<String> = Vec::with_capacity(tables.len());
    let mut done: HashSet<&str> = HashSet::new();
    while ordered.len() < tables.len() {
        let next = tables
            .iter()
            .find(|t| {
                !done.contains(t.as_str()) && deps[t.as_str()].iter().all(|p| done.contains(p))
            })
            .ok_or_else(|| DbError::Conflict("circular table dependencies".into()))?;
        done.insert(next.as_str());
        ordered.push(next.clone());
    }
    Ok(ordered)
}

pub async fn columns(conn: &mut PgConnection, table: &str) -> DbResult<Vec<String>> {
    Ok(sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns
         WHERE table_schema = 'public' AND table_name = $1 ORDER BY ordinal_position",
    )
    .bind(table)
    .fetch_all(conn)
    .await?)
}

/// One name as a quoted SQL identifier (a `"` inside it is doubled).
fn ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn quoted(cols: &[String]) -> String {
    cols.iter().map(|c| ident(c)).collect::<Vec<_>>().join(", ")
}

/// Row counts of the tables shown in a preview.
pub async fn record_counts(conn: &mut PgConnection) -> DbResult<BTreeMap<String, i64>> {
    let mut out = BTreeMap::new();
    for t in COUNTED_TABLES {
        let n: i64 =
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM \"{t}\"")))
                .fetch_one(&mut *conn)
                .await?;
        out.insert((*t).to_string(), n);
    }
    Ok(out)
}

/// Rows of every table, by name.
pub type TableCounts = BTreeMap<String, i64>;

/// How much of a table goes into one record of the dump. A big table is written as several
/// records one after the other, so no more than this much of the database is ever held in
/// memory at a time, whatever the size of the school.
const SEGMENT: usize = 4 * 1024 * 1024;

fn io_failed(e: std::io::Error) -> DbError {
    DbError::Conflict(format!("the backup could not be written: {e}"))
}

fn write_record<W: std::io::Write>(
    out: &mut W,
    table: &str,
    cols: &str,
    data: &[u8],
) -> std::io::Result<()> {
    out.write_all(&(table.len() as u32).to_be_bytes())?;
    out.write_all(table.as_bytes())?;
    out.write_all(&(cols.len() as u32).to_be_bytes())?;
    out.write_all(cols.as_bytes())?;
    out.write_all(&(data.len() as u64).to_be_bytes())?;
    out.write_all(data)
}

/// Rows of every table (the tables a dump holds), counted now.
pub async fn table_counts(conn: &mut PgConnection) -> DbResult<TableCounts> {
    let mut out = BTreeMap::new();
    for t in table_order(&mut *conn).await? {
        let n: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM {}",
            ident(&t)
        )))
        .fetch_one(&mut *conn)
        .await?;
        out.insert(t, n);
    }
    Ok(out)
}

/// Dump every table in one repeatable-read transaction, writing as it goes (into a
/// compressor, for a backup), and say how many rows of each table that snapshot held.
pub async fn dump_into<W: std::io::Write + Send>(
    conn: &mut PgConnection,
    out: &mut W,
) -> DbResult<TableCounts> {
    dump_segmented(conn, out, SEGMENT).await
}

/// [`dump_into`] with the record size given (tests make a small table span many records).
#[doc(hidden)]
pub async fn dump_segmented<W: std::io::Write + Send>(
    conn: &mut PgConnection,
    out: &mut W,
    segment_size: usize,
) -> DbResult<TableCounts> {
    let mut tx = conn.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    let tables = table_order(&mut tx).await?;
    out.write_all(MAGIC).map_err(io_failed)?;
    let mut segment: Vec<u8> = Vec::with_capacity(segment_size.min(SEGMENT) + 64 * 1024);
    for table in &tables {
        let cols = columns(&mut tx, table).await?;
        let sql = format!("COPY {} ({}) TO STDOUT", ident(table), quoted(&cols));
        let cols_joined = cols.join(",");
        let mut written = false;
        {
            let mut stream = tx.copy_out_raw(&sql).await?;
            while let Some(chunk) = stream.try_next().await? {
                segment.extend_from_slice(&chunk);
                if segment.len() >= segment_size {
                    write_record(out, table, &cols_joined, &segment).map_err(io_failed)?;
                    segment.clear();
                    written = true;
                }
            }
        }
        // An empty table is written too: a restore checks its columns.
        if !segment.is_empty() || !written {
            write_record(out, table, &cols_joined, &segment).map_err(io_failed)?;
            segment.clear();
        }
    }
    let counts = table_counts(&mut tx).await?;
    tx.commit().await?;
    Ok(counts)
}

/// [`dump_into`] a buffer: the whole dump in memory. For tests and small databases.
pub async fn dump(conn: &mut PgConnection) -> DbResult<Vec<u8>> {
    let mut out = Vec::new();
    dump_into(conn, &mut out).await?;
    Ok(out)
}

/// Tables and rows loaded by a restore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreReport {
    pub tables: usize,
    pub rows: u64,
}

/// Refuse a dump made with a different schema version.
pub fn check_schema(current: &[i64], backup: &[i64]) -> DbResult<()> {
    if current != backup {
        return Err(DbError::Conflict(
            "This backup was made with a different version of Madrasa; update the app to the version that made it before restoring".into(),
        ));
    }
    Ok(())
}

/// Replace every table's contents with the dump, in one transaction.
pub async fn restore(conn: &mut PgConnection, container: &[u8]) -> DbResult<RestoreReport> {
    let mut tx = conn.begin().await?;
    let report = restore_in(&mut tx, container).await?;
    tx.commit().await?;
    Ok(report)
}

/// [`restore`] inside a transaction the caller holds, so that what must happen with the
/// restore (keeping a school's link to its Nexus hub) commits or fails together with it.
pub async fn restore_in(tx: &mut PgConnection, container: &[u8]) -> DbResult<RestoreReport> {
    restore_stream(tx, container).await
}

/// One record's heading in a dump.
struct RecordHead {
    table: String,
    cols: Vec<String>,
    len: u64,
}

fn not_a_dump() -> DbError {
    DbError::Conflict("the backup contents are not a Madrasa dump".into())
}

/// The next record's heading, or `None` at the clean end of the dump.
fn read_head<R: std::io::Read>(reader: &mut R) -> DbResult<Option<RecordHead>> {
    let mut len4 = [0u8; 4];
    // The end is only clean between two records.
    let mut got = 0;
    while got < 4 {
        match reader.read(&mut len4[got..]) {
            Ok(0) if got == 0 => return Ok(None),
            Ok(0) => return Err(not_a_dump()),
            Ok(n) => got += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return Err(not_a_dump()),
        }
    }
    fn text<R: std::io::Read>(reader: &mut R, len: usize) -> DbResult<String> {
        // Names and column lists are short; a wild length means a damaged file.
        if len > 1024 * 1024 {
            return Err(not_a_dump());
        }
        let mut buf = vec![0u8; len];
        reader.read_exact(&mut buf).map_err(|_| not_a_dump())?;
        String::from_utf8(buf).map_err(|_| not_a_dump())
    }
    let table = text(reader, u32::from_be_bytes(len4) as usize)?;
    let mut cols_len = [0u8; 4];
    reader.read_exact(&mut cols_len).map_err(|_| not_a_dump())?;
    let cols = text(reader, u32::from_be_bytes(cols_len) as usize)?;
    let mut len8 = [0u8; 8];
    reader.read_exact(&mut len8).map_err(|_| not_a_dump())?;
    Ok(Some(RecordHead {
        table,
        cols: cols
            .split(',')
            .filter(|c| !c.is_empty())
            .map(str::to_owned)
            .collect(),
        len: u64::from_be_bytes(len8),
    }))
}

/// Replace every table's contents with a dump that is read as it is loaded (from memory, or
/// straight out of a decompressor), inside the caller's transaction: a dump that turns out
/// to be damaged or from another version fails the transaction and changes nothing.
pub async fn restore_stream<R: std::io::Read + Send>(
    tx: &mut PgConnection,
    mut reader: R,
) -> DbResult<RestoreReport> {
    let mut magic = [0u8; 7];
    debug_assert_eq!(magic.len(), MAGIC.len());
    if reader.read_exact(&mut magic).is_err() || magic != MAGIC {
        return Err(not_a_dump());
    }
    let existing = table_order(&mut *tx).await?;
    for name in &existing {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "ALTER TABLE {} DISABLE TRIGGER USER",
            ident(name)
        )))
        .execute(&mut *tx)
        .await?;
    }
    // Names come from the database's own catalogue; quoted properly all the same.
    let all = quoted(&existing);
    sqlx::query(sqlx::AssertSqlSafe(format!("TRUNCATE {all} CASCADE")))
        .execute(&mut *tx)
        .await?;
    let mut rows = 0u64;
    let mut tables = 0usize;
    let mut buf = vec![0u8; 1024 * 1024];
    // The dump lists parents before children, so loading in its order satisfies the foreign
    // keys. The records of one table follow each other and go into one COPY.
    let mut next = read_head(&mut reader)?;
    while let Some(head) = next.take() {
        if !existing.contains(&head.table) {
            return Err(DbError::Conflict(format!(
                "the backup contains a table this version does not know: {}",
                head.table
            )));
        }
        if columns(&mut *tx, &head.table).await? != head.cols {
            return Err(DbError::Conflict(format!(
                "the columns of {} differ from this version",
                head.table
            )));
        }
        tables += 1;
        let sql = format!(
            "COPY {} ({}) FROM STDIN",
            ident(&head.table),
            quoted(&head.cols)
        );
        let mut sink = tx.copy_in_raw(&sql).await?;
        let mut left = head.len;
        loop {
            while left > 0 {
                let take = left.min(buf.len() as u64) as usize;
                if reader.read_exact(&mut buf[..take]).is_err() {
                    sink.abort("the backup ends in the middle of a table")
                        .await?;
                    return Err(not_a_dump());
                }
                sink.send(&buf[..take]).await?;
                left -= take as u64;
            }
            match read_head(&mut reader) {
                Ok(Some(more)) if more.table == head.table && more.cols == head.cols => {
                    left = more.len;
                }
                Ok(other) => {
                    next = other;
                    break;
                }
                Err(e) => {
                    sink.abort("the backup is damaged").await?;
                    return Err(e);
                }
            }
        }
        rows += sink.finish().await?;
    }
    for name in &existing {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "ALTER TABLE {} ENABLE TRIGGER USER",
            ident(name)
        )))
        .execute(&mut *tx)
        .await?;
    }
    // Nexus sync: the restored queue of unsent changes keeps its sequence numbers, so the
    // counter must continue after them (an acknowledgement names changes by that number).
    sqlx::query(
        "SELECT setval('sync_change_seq',
            GREATEST((SELECT COALESCE(max(seq), 0) FROM sync_changes) + 1, nextval('sync_change_seq')))",
    )
    .execute(&mut *tx)
    .await?;
    Ok(RestoreReport { tables, rows })
}

// ---- run history --------------------------------------------------------------------------

pub async fn start_run(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    trigger: &str,
) -> DbResult<BackupRun> {
    let run = sqlx::query_as::<_, BackupRun>(
        "INSERT INTO backup_runs (id, school_id, trigger, status, updated_by, hlc)
         VALUES ($1, $2, $3, 'running', $4, $5)
         RETURNING id, school_id, trigger, status, started_at, finished_at, file_path, file_size, error, verify_status, verify_message, verified_at",
    )
    .bind(new_id())
    .bind(school_id)
    .bind(trigger)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    // Audited at the start as well, so the entry is inside the dump the run produces and
    // survives a later restore of that backup.
    audit::record(
        conn,
        hlc,
        actor,
        Action::Backup,
        "backup",
        Some(run.id),
        None,
        Some(serde_json::json!({ "trigger": run.trigger, "status": "running" })),
    )
    .await?;
    Ok(run)
}

pub async fn finish_run(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    result: Result<(String, i64), String>,
) -> DbResult<BackupRun> {
    let (status, path, size, error) = match result {
        Ok((p, s)) => ("ok", Some(p), Some(s), None),
        Err(e) => ("failed", None, None, Some(e)),
    };
    let run = sqlx::query_as::<_, BackupRun>(
        "UPDATE backup_runs SET status = $2, finished_at = now(), file_path = $3, file_size = $4, error = $5,
            updated_at = now(), updated_by = $6, hlc = $7
         WHERE id = $1
         RETURNING id, school_id, trigger, status, started_at, finished_at, file_path, file_size, error, verify_status, verify_message, verified_at",
    )
    .bind(id)
    .bind(status)
    .bind(path)
    .bind(size)
    .bind(error)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Backup,
        "backup",
        Some(run.id),
        None,
        Some(serde_json::json!({
            "trigger": run.trigger, "status": run.status, "file_path": run.file_path,
            "file_size": run.file_size, "error": run.error,
        })),
    )
    .await?;
    Ok(run)
}

pub async fn last_ok_run(conn: &mut PgConnection, school_id: Uuid) -> DbResult<Option<BackupRun>> {
    Ok(sqlx::query_as::<_, BackupRun>(
        "SELECT id, school_id, trigger, status, started_at, finished_at, file_path, file_size, error, verify_status, verify_message, verified_at
         FROM backup_runs WHERE school_id = $1 AND status = 'ok' AND deleted_at IS NULL
         ORDER BY finished_at DESC LIMIT 1",
    )
    .bind(school_id)
    .fetch_optional(conn)
    .await?)
}

pub async fn recent_runs(
    conn: &mut PgConnection,
    school_id: Uuid,
    limit: i64,
) -> DbResult<Vec<BackupRun>> {
    Ok(sqlx::query_as::<_, BackupRun>(
        "SELECT id, school_id, trigger, status, started_at, finished_at, file_path, file_size, error, verify_status, verify_message, verified_at
         FROM backup_runs WHERE school_id = $1 AND deleted_at IS NULL
         ORDER BY started_at DESC LIMIT $2",
    )
    .bind(school_id)
    .bind(limit)
    .fetch_all(conn)
    .await?)
}

/// Record what the check of a finished backup found: `verified`, `failed` or `skipped`,
/// with a sentence a person can read.
pub async fn set_verification(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    status: &str,
    message: &str,
) -> DbResult<BackupRun> {
    let run = sqlx::query_as::<_, BackupRun>(
        "UPDATE backup_runs SET verify_status = $2, verify_message = $3, verified_at = now(),
            updated_at = now(), updated_by = $4, hlc = $5
         WHERE id = $1
         RETURNING id, school_id, trigger, status, started_at, finished_at, file_path, file_size, error, verify_status, verify_message, verified_at",
    )
    .bind(id)
    .bind(status)
    .bind(message)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Backup,
        "backup",
        Some(run.id),
        None,
        Some(serde_json::json!({ "check": status, "message": message })),
    )
    .await?;
    Ok(run)
}

/// Mark runs left 'running' by a crash as failed.
pub async fn fail_stale_runs(conn: &mut PgConnection, hlc: &Hlc) -> DbResult<u64> {
    Ok(sqlx::query(
        "UPDATE backup_runs SET status = 'failed', finished_at = now(), error = 'interrupted', updated_at = now(), hlc = $1
         WHERE status = 'running' AND started_at < now() - interval '2 hours'",
    )
    .bind(hlc.now().to_string())
    .execute(conn)
    .await?
    .rows_affected())
}
