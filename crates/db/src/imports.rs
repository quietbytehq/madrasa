//! Import batches: what an import created or changed, and how to undo it.
//!
//! The API applies an import inside one transaction and records one row per effect. A
//! rollback walks those rows newest first: created entities are soft-deleted (vouchers are
//! cancelled), updated entities get the stored old values back. Everything is audited.

use crate::audit::{self, Action};
use crate::models::{ImportBatch, ImportBatchRow};
use crate::{Actor, DbError, DbResult};
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct NewBatch {
    pub school_id: Uuid,
    pub kind: String,
    pub file_name: String,
    pub options: Value,
    pub total_rows: i32,
    pub applied_by: Uuid,
}

pub async fn create_batch(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    b: NewBatch,
) -> DbResult<ImportBatch> {
    let row = sqlx::query_as::<_, ImportBatch>(
        "INSERT INTO import_batches (id, school_id, kind, file_name, status, options, total_rows, applied_by, updated_by, hlc)
         VALUES ($1, $2, $3, $4, 'applied', $5, $6, $7, $8, $9)
         RETURNING id, school_id, kind, file_name, status, options, total_rows, created_rows, updated_rows, skipped_rows,
                   applied_by, applied_at, rolled_back_at",
    )
    .bind(new_id())
    .bind(b.school_id)
    .bind(&b.kind)
    .bind(&b.file_name)
    .bind(&b.options)
    .bind(b.total_rows)
    .bind(b.applied_by)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(conn)
    .await?;
    Ok(row)
}

/// Record one effect of the batch.
pub async fn add_row(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    batch: &ImportBatch,
    row_no: i32,
    action: &str,
    entity: &str,
    entity_id: Option<Uuid>,
    old_value: Option<Value>,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO import_batch_rows (id, school_id, batch_id, row_no, action, entity, entity_id, old_value, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(new_id())
    .bind(batch.school_id)
    .bind(batch.id)
    .bind(row_no)
    .bind(action)
    .bind(entity)
    .bind(entity_id)
    .bind(old_value)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .execute(conn)
    .await?;
    Ok(())
}

/// Store the final counts and audit the batch.
pub async fn finish_batch(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    created: i32,
    updated: i32,
    skipped: i32,
) -> DbResult<ImportBatch> {
    let row = sqlx::query_as::<_, ImportBatch>(
        "UPDATE import_batches SET created_rows = $2, updated_rows = $3, skipped_rows = $4, updated_at = now(), updated_by = $5, hlc = $6
         WHERE id = $1
         RETURNING id, school_id, kind, file_name, status, options, total_rows, created_rows, updated_rows, skipped_rows,
                   applied_by, applied_at, rolled_back_at",
    )
    .bind(id)
    .bind(created)
    .bind(updated)
    .bind(skipped)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "import_batch",
        Some(row.id),
        None,
        Some(audit::snapshot(&row)),
    )
    .await?;
    Ok(row)
}

pub async fn get_batch(conn: &mut PgConnection, id: Uuid) -> DbResult<ImportBatch> {
    sqlx::query_as::<_, ImportBatch>(
        "SELECT id, school_id, kind, file_name, status, options, total_rows, created_rows, updated_rows, skipped_rows,
                applied_by, applied_at, rolled_back_at
         FROM import_batches WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("import batch".into()))
}

pub async fn list_batches(
    conn: &mut PgConnection,
    school_id: Uuid,
    limit: i64,
) -> DbResult<Vec<ImportBatch>> {
    Ok(sqlx::query_as::<_, ImportBatch>(
        "SELECT id, school_id, kind, file_name, status, options, total_rows, created_rows, updated_rows, skipped_rows,
                applied_by, applied_at, rolled_back_at
         FROM import_batches WHERE school_id = $1 AND deleted_at IS NULL
         ORDER BY applied_at DESC LIMIT $2",
    )
    .bind(school_id)
    .bind(limit)
    .fetch_all(conn)
    .await?)
}

pub async fn batch_rows(conn: &mut PgConnection, batch_id: Uuid) -> DbResult<Vec<ImportBatchRow>> {
    Ok(sqlx::query_as::<_, ImportBatchRow>(
        "SELECT id, batch_id, row_no, action, entity, entity_id, old_value FROM import_batch_rows
         WHERE batch_id = $1 AND deleted_at IS NULL ORDER BY row_no DESC, created_at DESC",
    )
    .bind(batch_id)
    .fetch_all(conn)
    .await?)
}

/// Soft-delete one row of a table by id (tables with the sync columns).
async fn soft_delete(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    table: &str,
    id: Uuid,
) -> DbResult<()> {
    let allowed = [
        "students",
        "guardians",
        "student_guardians",
        "enrollments",
        "staff",
    ];
    if !allowed.contains(&table) {
        return Err(DbError::Conflict(format!("cannot roll back {table}")));
    }
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE \"{table}\" SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1 AND deleted_at IS NULL"
    )))
    .bind(id)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Delete,
        table.trim_end_matches('s'),
        Some(id),
        Some(serde_json::json!({ "rolled_back_import": true })),
        None,
    )
    .await?;
    Ok(())
}

/// Put the stored old values back on a student or staff row (only the columns present in
/// the JSON, which are the ones the import changed).
async fn restore_old(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    table: &str,
    id: Uuid,
    old: &Value,
) -> DbResult<()> {
    let allowed_cols: &[&str] = match table {
        "students" => &[
            "legacy_id",
            "full_name",
            "father_name",
            "date_of_birth",
            "gender",
            "cnic",
            "address",
            "admission_date",
            "board_roll_no",
        ],
        "staff" => &[
            "full_name",
            "father_name",
            "gender",
            "phone",
            "email",
            "cnic",
            "designation",
            "joined_on",
        ],
        "guardians" => &[
            "full_name",
            "phone",
            "email",
            "cnic",
            "occupation",
            "address",
        ],
        _ => return Err(DbError::Conflict(format!("cannot roll back {table}"))),
    };
    let Some(obj) = old.as_object() else {
        return Ok(());
    };
    for (col, value) in obj {
        if !allowed_cols.contains(&col.as_str()) {
            continue;
        }
        let sql = format!(
            "UPDATE \"{table}\" SET \"{col}\" = $2, updated_at = now(), updated_by = $3, hlc = $4 WHERE id = $1"
        );
        let q = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(id);
        let q = match (col.as_str(), value) {
            ("date_of_birth" | "admission_date" | "joined_on", Value::String(s)) => {
                q.bind(s.parse::<chrono::NaiveDate>().ok())
            }
            ("date_of_birth" | "admission_date" | "joined_on", _) => {
                q.bind(None::<chrono::NaiveDate>)
            }
            (_, Value::String(s)) => q.bind(Some(s.clone())),
            (_, Value::Null) => q.bind(None::<String>),
            (_, other) => q.bind(Some(other.to_string())),
        };
        q.bind(actor.user_id)
            .bind(hlc.now().to_string())
            .execute(&mut *conn)
            .await?;
    }
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        table.trim_end_matches('s'),
        Some(id),
        Some(serde_json::json!({ "rolled_back_import": true })),
        Some(old.clone()),
    )
    .await?;
    Ok(())
}

/// Undo a batch. Returns how many effects were reverted.
pub async fn rollback(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    batch_id: Uuid,
) -> DbResult<usize> {
    let batch = get_batch(&mut *conn, batch_id).await?;
    if batch.status != "applied" {
        return Err(DbError::Conflict(
            "This import was already rolled back".into(),
        ));
    }
    let rows = batch_rows(&mut *conn, batch_id).await?;
    let mut reverted = 0usize;
    for r in &rows {
        let Some(id) = r.entity_id else { continue };
        match (r.action.as_str(), r.entity.as_str()) {
            ("created", "student") => soft_delete(&mut *conn, hlc, actor, "students", id).await?,
            ("created", "guardian") => soft_delete(&mut *conn, hlc, actor, "guardians", id).await?,
            ("created", "student_guardian") => {
                soft_delete(&mut *conn, hlc, actor, "student_guardians", id).await?
            }
            ("created", "enrollment") => {
                soft_delete(&mut *conn, hlc, actor, "enrollments", id).await?
            }
            ("created", "staff") => soft_delete(&mut *conn, hlc, actor, "staff", id).await?,
            ("created", "fee_voucher") => {
                crate::fees::cancel_voucher(&mut *conn, hlc, actor, id, "Import rolled back")
                    .await?;
            }
            ("updated", "student") => {
                restore_old(
                    &mut *conn,
                    hlc,
                    actor,
                    "students",
                    id,
                    r.old_value.as_ref().unwrap_or(&Value::Null),
                )
                .await?
            }
            ("updated", "staff") => {
                restore_old(
                    &mut *conn,
                    hlc,
                    actor,
                    "staff",
                    id,
                    r.old_value.as_ref().unwrap_or(&Value::Null),
                )
                .await?
            }
            ("updated", "guardian") => {
                restore_old(
                    &mut *conn,
                    hlc,
                    actor,
                    "guardians",
                    id,
                    r.old_value.as_ref().unwrap_or(&Value::Null),
                )
                .await?
            }
            _ => continue,
        }
        reverted += 1;
    }
    sqlx::query(
        "UPDATE import_batches SET status = 'rolled_back', rolled_back_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1",
    )
    .bind(batch_id)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "import_batch",
        Some(batch_id),
        Some(serde_json::json!({ "status": "applied" })),
        Some(serde_json::json!({ "status": "rolled_back", "reverted": reverted })),
    )
    .await?;
    Ok(reverted)
}
