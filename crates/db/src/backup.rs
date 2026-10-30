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

fn quoted(cols: &[String]) -> String {
    cols.iter()
        .map(|c| format!("\"{}\"", c.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(", ")
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

/// Dump every table in one repeatable-read transaction.
pub async fn dump(conn: &mut PgConnection) -> DbResult<Vec<u8>> {
    let mut tx = conn.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *tx)
        .await?;
    let tables = table_order(&mut tx).await?;
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    for table in &tables {
        let cols = columns(&mut tx, table).await?;
        let sql = format!("COPY \"{table}\" ({}) TO STDOUT", quoted(&cols));
        let mut stream = tx.copy_out_raw(&sql).await?;
        let mut data = Vec::new();
        while let Some(chunk) = stream.try_next().await? {
            data.extend_from_slice(&chunk);
        }
        let cols_joined = cols.join(",");
        out.extend_from_slice(&(table.len() as u32).to_be_bytes());
        out.extend_from_slice(table.as_bytes());
        out.extend_from_slice(&(cols_joined.len() as u32).to_be_bytes());
        out.extend_from_slice(cols_joined.as_bytes());
        out.extend_from_slice(&(data.len() as u64).to_be_bytes());
        out.extend_from_slice(&data);
    }
    tx.commit().await?;
    Ok(out)
}

struct DumpTable<'a> {
    name: String,
    cols: Vec<String>,
    data: &'a [u8],
}

fn parse(container: &[u8]) -> DbResult<Vec<DumpTable<'_>>> {
    let bad = || DbError::Conflict("the backup contents are not a Madrasa dump".into());
    if !container.starts_with(MAGIC) {
        return Err(bad());
    }
    let mut pos = MAGIC.len();
    let mut tables = Vec::new();
    let take = |pos: &mut usize, n: usize| -> DbResult<&[u8]> {
        let end = pos.checked_add(n).ok_or_else(bad)?;
        let slice = container.get(*pos..end).ok_or_else(bad)?;
        *pos = end;
        Ok(slice)
    };
    while pos < container.len() {
        let name_len =
            u32::from_be_bytes(take(&mut pos, 4)?.try_into().map_err(|_| bad())?) as usize;
        let name = String::from_utf8(take(&mut pos, name_len)?.to_vec()).map_err(|_| bad())?;
        let cols_len =
            u32::from_be_bytes(take(&mut pos, 4)?.try_into().map_err(|_| bad())?) as usize;
        let cols = String::from_utf8(take(&mut pos, cols_len)?.to_vec()).map_err(|_| bad())?;
        let data_len =
            u64::from_be_bytes(take(&mut pos, 8)?.try_into().map_err(|_| bad())?) as usize;
        let data = take(&mut pos, data_len)?;
        tables.push(DumpTable {
            name,
            cols: cols
                .split(',')
                .filter(|c| !c.is_empty())
                .map(str::to_owned)
                .collect(),
            data,
        });
    }
    Ok(tables)
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
    let tables = parse(container)?;
    let mut tx = conn.begin().await?;
    let existing = table_order(&mut tx).await?;
    for t in &tables {
        if !existing.contains(&t.name) {
            return Err(DbError::Conflict(format!(
                "the backup contains a table this version does not know: {}",
                t.name
            )));
        }
        let cols = columns(&mut tx, &t.name).await?;
        if t.cols != cols {
            return Err(DbError::Conflict(format!(
                "the columns of {} differ from this version",
                t.name
            )));
        }
    }
    for name in &existing {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "ALTER TABLE \"{name}\" DISABLE TRIGGER USER"
        )))
        .execute(&mut *tx)
        .await?;
    }
    let all = existing
        .iter()
        .map(|n| format!("\"{n}\""))
        .collect::<Vec<_>>()
        .join(", ");
    sqlx::query(sqlx::AssertSqlSafe(format!("TRUNCATE {all} CASCADE")))
        .execute(&mut *tx)
        .await?;
    let mut rows = 0u64;
    // Load in the current dependency order so foreign keys are satisfied row by row.
    for name in &existing {
        let Some(t) = tables.iter().find(|t| &t.name == name) else {
            continue;
        };
        if t.data.is_empty() {
            continue;
        }
        let sql = format!("COPY \"{}\" ({}) FROM STDIN", t.name, quoted(&t.cols));
        let mut sink = tx.copy_in_raw(&sql).await?;
        sink.send(t.data).await?;
        rows += sink.finish().await?;
    }
    for name in &existing {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "ALTER TABLE \"{name}\" ENABLE TRIGGER USER"
        )))
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(RestoreReport {
        tables: tables.len(),
        rows,
    })
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
         RETURNING id, school_id, trigger, status, started_at, finished_at, file_path, file_size, error",
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
         RETURNING id, school_id, trigger, status, started_at, finished_at, file_path, file_size, error",
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
        "SELECT id, school_id, trigger, status, started_at, finished_at, file_path, file_size, error
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
        "SELECT id, school_id, trigger, status, started_at, finished_at, file_path, file_size, error
         FROM backup_runs WHERE school_id = $1 AND deleted_at IS NULL
         ORDER BY started_at DESC LIMIT $2",
    )
    .bind(school_id)
    .bind(limit)
    .fetch_all(conn)
    .await?)
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
