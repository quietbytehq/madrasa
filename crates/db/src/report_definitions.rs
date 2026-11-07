//! Saved custom reports (M2 stage 3). The `definition` JSON is validated by the API against
//! its field catalogue before it is stored; this module only stores and lists.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::audit::{self, snapshot, Action};
use crate::{map_unique, Actor, DbResult};
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ReportDefinition {
    pub id: Uuid,
    pub school_id: Uuid,
    pub name: String,
    pub description: String,
    pub source: String,
    pub definition: Value,
    pub owner_user_id: Uuid,
    pub is_shared: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub updated_by: Option<Uuid>,
    pub deleted_at: Option<chrono::DateTime<chrono::Utc>>,
    pub hlc: String,
}

#[derive(Debug, Clone)]
pub struct NewDefinition {
    pub school_id: Uuid,
    pub name: String,
    pub description: String,
    pub source: String,
    pub definition: Value,
    pub owner_user_id: Uuid,
    pub is_shared: bool,
}

pub async fn create(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    input: NewDefinition,
) -> DbResult<ReportDefinition> {
    let id = new_id();
    let row = sqlx::query_as::<_, ReportDefinition>(
        "INSERT INTO report_definitions (id, school_id, name, description, source, definition, owner_user_id, is_shared, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING *",
    )
    .bind(id)
    .bind(input.school_id)
    .bind(input.name.trim())
    .bind(input.description.trim())
    .bind(input.source)
    .bind(input.definition)
    .bind(input.owner_user_id)
    .bind(input.is_shared)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a report with this name"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "report_definition",
        Some(id),
        None,
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

#[derive(Debug, Clone)]
pub struct DefinitionUpdate {
    pub name: String,
    pub description: String,
    pub source: String,
    pub definition: Value,
    pub is_shared: bool,
}

pub async fn update(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    input: DefinitionUpdate,
) -> DbResult<ReportDefinition> {
    let before = get(&mut *conn, id).await?;
    let row = sqlx::query_as::<_, ReportDefinition>(
        "UPDATE report_definitions SET name = $2, description = $3, source = $4, definition = $5, is_shared = $6,
                updated_at = now(), updated_by = $7, hlc = $8
         WHERE id = $1 AND deleted_at IS NULL RETURNING *",
    )
    .bind(id)
    .bind(input.name.trim())
    .bind(input.description.trim())
    .bind(input.source)
    .bind(input.definition)
    .bind(input.is_shared)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a report with this name"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "report_definition",
        Some(id),
        Some(snapshot(&before)),
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

pub async fn delete(conn: &mut PgConnection, hlc: &Hlc, actor: Actor, id: Uuid) -> DbResult<()> {
    let before = get(&mut *conn, id).await?;
    sqlx::query(
        "UPDATE report_definitions SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1",
    )
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
        "report_definition",
        Some(id),
        Some(snapshot(&before)),
        None,
    )
    .await?;
    Ok(())
}

pub async fn get(conn: &mut PgConnection, id: Uuid) -> DbResult<ReportDefinition> {
    sqlx::query_as::<_, ReportDefinition>(
        "SELECT * FROM report_definitions WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| crate::DbError::NotFound("report".into()))
}

/// Shared reports plus the caller's own, by name.
pub async fn list_visible(
    conn: &mut PgConnection,
    school_id: Uuid,
    user_id: Uuid,
) -> DbResult<Vec<ReportDefinition>> {
    Ok(sqlx::query_as::<_, ReportDefinition>(
        "SELECT * FROM report_definitions WHERE school_id = $1 AND deleted_at IS NULL AND (is_shared OR owner_user_id = $2)
         ORDER BY lower(name)",
    )
    .bind(school_id)
    .bind(user_id)
    .fetch_all(conn)
    .await?)
}
