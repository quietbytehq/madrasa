use crate::audit::{self, snapshot, Action};
use crate::models::CustomFieldDefinition;
use crate::{map_unique, Actor, DbResult};
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct NewCustomField {
    pub school_id: Uuid,
    pub entity: String,
    pub key: String,
    pub label: String,
    pub field_type: String,
    pub required: bool,
    pub options: Value,
    pub position: i32,
}

pub async fn create(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    input: NewCustomField,
) -> DbResult<CustomFieldDefinition> {
    let id = new_id();
    let row = sqlx::query_as::<_, CustomFieldDefinition>(
        "INSERT INTO custom_field_definitions (id, school_id, entity, key, label, field_type, required, options, position, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) RETURNING *",
    )
    .bind(id)
    .bind(input.school_id)
    .bind(input.entity)
    .bind(input.key)
    .bind(input.label)
    .bind(input.field_type)
    .bind(input.required)
    .bind(input.options)
    .bind(input.position)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a custom field with this key"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "custom_field_definition",
        Some(id),
        None,
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

pub async fn list(
    conn: &mut PgConnection,
    school_id: Uuid,
    entity: &str,
) -> DbResult<Vec<CustomFieldDefinition>> {
    Ok(sqlx::query_as::<_, CustomFieldDefinition>(
        "SELECT * FROM custom_field_definitions WHERE school_id = $1 AND entity = $2 AND deleted_at IS NULL
         ORDER BY position, key",
    )
    .bind(school_id)
    .bind(entity)
    .fetch_all(conn)
    .await?)
}
