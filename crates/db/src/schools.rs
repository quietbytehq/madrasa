use crate::audit::{self, snapshot, Action};
use crate::models::School;
use crate::{map_unique, Actor, DbError, DbResult};
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct NewSchool {
    pub name: String,
    pub code: String,
    pub city: String,
    pub is_demo: bool,
    pub academic_year_start_month: i16,
    pub currency: String,
}

pub async fn create(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    input: NewSchool,
) -> DbResult<School> {
    let id = new_id();
    let school = sqlx::query_as::<_, School>(
        "INSERT INTO schools (id, name, code, city, is_demo, academic_year_start_month, currency, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING *",
    )
    .bind(id)
    .bind(input.name.trim())
    .bind(input.code.trim().to_uppercase())
    .bind(input.city.trim())
    .bind(input.is_demo)
    .bind(input.academic_year_start_month)
    .bind(input.currency.trim().to_uppercase())
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a school with this code"))?;
    let actor = Actor {
        school_id: Some(id),
        ..actor
    };
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "school",
        Some(id),
        None,
        Some(snapshot(&school)),
    )
    .await?;
    Ok(school)
}

pub async fn get(conn: &mut PgConnection, id: Uuid) -> DbResult<School> {
    sqlx::query_as::<_, School>("SELECT * FROM schools WHERE id = $1 AND deleted_at IS NULL")
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or_else(|| DbError::NotFound("school".into()))
}

/// The school of a single-school (Lite) install: the oldest live school, if any.
pub async fn current(conn: &mut PgConnection) -> DbResult<Option<School>> {
    Ok(sqlx::query_as::<_, School>(
        "SELECT * FROM schools WHERE deleted_at IS NULL ORDER BY created_at ASC LIMIT 1",
    )
    .fetch_optional(conn)
    .await?)
}

pub async fn count(conn: &mut PgConnection, include_demo: bool) -> DbResult<i64> {
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM schools WHERE deleted_at IS NULL AND ($1 OR NOT is_demo)",
    )
    .bind(include_demo)
    .fetch_one(conn)
    .await?;
    Ok(n)
}
