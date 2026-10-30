//! Append-only audit log. Every repository mutation calls [`record`] in the same transaction.

use crate::models::AuditEntry;
use crate::{Actor, DbResult};
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use serde::Serialize;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

/// Audit actions. Stored as lowercase text (see the CHECK constraint in the migration).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Create,
    Update,
    Delete,
    Login,
    LoginFailed,
    Lockout,
    Logout,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Action::Create => "create",
            Action::Update => "update",
            Action::Delete => "delete",
            Action::Login => "login",
            Action::LoginFailed => "login_failed",
            Action::Lockout => "lockout",
            Action::Logout => "logout",
        }
    }
}

/// Serialize a row for the log. Types skip secret fields via `#[serde(skip)]`.
pub fn snapshot<T: Serialize>(row: &T) -> Value {
    serde_json::to_value(row).unwrap_or(Value::Null)
}

/// Write one audit row.
// Positional arguments mirror the audit_log columns; a builder would add noise at ~30 call sites.
#[allow(clippy::too_many_arguments)]
pub async fn record(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    action: Action,
    entity: &str,
    entity_id: Option<Uuid>,
    old_value: Option<Value>,
    new_value: Option<Value>,
) -> DbResult<Uuid> {
    let id = new_id();
    sqlx::query(
        "INSERT INTO audit_log (id, school_id, actor_user_id, action, entity, entity_id, old_value, new_value, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(id)
    .bind(actor.school_id)
    .bind(actor.user_id)
    .bind(action.as_str())
    .bind(entity)
    .bind(entity_id)
    .bind(old_value)
    .bind(new_value)
    .bind(hlc.now().to_string())
    .execute(conn)
    .await?;
    Ok(id)
}

/// Most recent entries for a school (or instance-wide when `school_id` is `None`).
pub async fn list_recent(
    conn: &mut PgConnection,
    school_id: Option<Uuid>,
    limit: i64,
) -> DbResult<Vec<AuditEntry>> {
    let rows = sqlx::query_as::<_, AuditEntry>(
        "SELECT * FROM audit_log WHERE ($1::uuid IS NULL OR school_id = $1)
         ORDER BY at DESC, hlc DESC LIMIT $2",
    )
    .bind(school_id)
    .bind(limit)
    .fetch_all(conn)
    .await?;
    Ok(rows)
}

/// All entries for one entity, oldest first.
pub async fn for_entity(
    conn: &mut PgConnection,
    entity: &str,
    entity_id: Uuid,
) -> DbResult<Vec<AuditEntry>> {
    let rows = sqlx::query_as::<_, AuditEntry>(
        "SELECT * FROM audit_log WHERE entity = $1 AND entity_id = $2 ORDER BY at ASC, hlc ASC",
    )
    .bind(entity)
    .bind(entity_id)
    .fetch_all(conn)
    .await?;
    Ok(rows)
}
