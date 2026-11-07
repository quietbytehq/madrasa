//! Consent manager (M2 stage 8): consent types defined by the school and the consents
//! recorded per student. A consent is append-only: every status change is a new row, so the
//! history is kept; every row is audit-logged.

use chrono::{DateTime, NaiveDate, Utc};
use madrasa_core::consents::{GivenBy, Renewal, Required, Rule};
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::audit::{self, snapshot, Action};
use crate::{map_unique, Actor, DbError, DbResult};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ConsentType {
    pub id: Uuid,
    pub school_id: Uuid,
    pub name: String,
    pub description: String,
    pub given_by: String,
    pub required: String,
    pub min_level: Option<i32>,
    pub max_level: Option<i32>,
    pub renewal: String,
    pub position: i32,
    pub hlc: String,
}

impl ConsentType {
    pub fn rule(&self) -> Rule {
        Rule {
            required: match self.required.as_str() {
                "all" => Required::All,
                "class_range" => Required::ClassRange,
                _ => Required::None,
            },
            min_level: self.min_level,
            max_level: self.max_level,
            renewal: if self.renewal == "yearly" {
                Renewal::Yearly
            } else {
                Renewal::None
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConsentTypeInput {
    pub name: String,
    pub description: String,
    pub given_by: GivenBy,
    pub required: Required,
    pub min_level: Option<i32>,
    pub max_level: Option<i32>,
    pub renewal: Renewal,
    pub position: i32,
}

fn given_by_str(g: GivenBy) -> &'static str {
    match g {
        GivenBy::Guardian => "guardian",
        GivenBy::Student => "student",
        GivenBy::Either => "either",
    }
}

fn required_str(r: Required) -> &'static str {
    match r {
        Required::None => "none",
        Required::All => "all",
        Required::ClassRange => "class_range",
    }
}

fn check(input: &ConsentTypeInput) -> DbResult<()> {
    if input.name.trim().is_empty() {
        return Err(DbError::Conflict("consent type name is required".into()));
    }
    if let (Some(a), Some(b)) = (input.min_level, input.max_level) {
        if a > b {
            return Err(DbError::Conflict(
                "the class range ends before it starts".into(),
            ));
        }
    }
    Ok(())
}

const TYPE_COLS: &str =
    "id, school_id, name, description, given_by, required, min_level, max_level, renewal, position, hlc";

pub async fn list_types(conn: &mut PgConnection, school_id: Uuid) -> DbResult<Vec<ConsentType>> {
    Ok(sqlx::query_as::<_, ConsentType>(sqlx::AssertSqlSafe(format!(
        "SELECT {TYPE_COLS} FROM consent_types WHERE school_id = $1 AND deleted_at IS NULL ORDER BY position, name"
    )))
    .bind(school_id)
    .fetch_all(conn)
    .await?)
}

pub async fn get_type(conn: &mut PgConnection, id: Uuid) -> DbResult<ConsentType> {
    sqlx::query_as::<_, ConsentType>(sqlx::AssertSqlSafe(format!(
        "SELECT {TYPE_COLS} FROM consent_types WHERE id = $1 AND deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("consent type".into()))
}

pub async fn create_type(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    input: &ConsentTypeInput,
) -> DbResult<ConsentType> {
    check(input)?;
    let row = sqlx::query_as::<_, ConsentType>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO consent_types (id, school_id, name, description, given_by, required, min_level, max_level, renewal, position, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) RETURNING {TYPE_COLS}"
    )))
    .bind(new_id())
    .bind(school_id)
    .bind(input.name.trim())
    .bind(input.description.trim())
    .bind(given_by_str(input.given_by))
    .bind(required_str(input.required))
    .bind(input.min_level)
    .bind(input.max_level)
    .bind(if input.renewal == Renewal::Yearly { "yearly" } else { "none" })
    .bind(input.position)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a consent type with this name"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "consent_type",
        Some(row.id),
        None,
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

pub async fn update_type(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    input: &ConsentTypeInput,
) -> DbResult<ConsentType> {
    check(input)?;
    let old = get_type(&mut *conn, id).await?;
    let row = sqlx::query_as::<_, ConsentType>(sqlx::AssertSqlSafe(format!(
        "UPDATE consent_types SET name = $2, description = $3, given_by = $4, required = $5, min_level = $6,
                max_level = $7, renewal = $8, position = $9, updated_at = now(), updated_by = $10, hlc = $11
         WHERE id = $1 AND deleted_at IS NULL RETURNING {TYPE_COLS}"
    )))
    .bind(id)
    .bind(input.name.trim())
    .bind(input.description.trim())
    .bind(given_by_str(input.given_by))
    .bind(required_str(input.required))
    .bind(input.min_level)
    .bind(input.max_level)
    .bind(if input.renewal == Renewal::Yearly { "yearly" } else { "none" })
    .bind(input.position)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a consent type with this name"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "consent_type",
        Some(id),
        Some(snapshot(&old)),
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

/// Soft-delete a type. Its recorded consents stay in the history but are no longer shown.
pub async fn delete_type(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
) -> DbResult<()> {
    let old = get_type(&mut *conn, id).await?;
    sqlx::query(
        "UPDATE consent_types SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1",
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
        "consent_type",
        Some(id),
        Some(snapshot(&old)),
        None,
    )
    .await?;
    Ok(())
}

// ---- consents -------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Consent {
    pub id: Uuid,
    pub school_id: Uuid,
    pub student_id: Uuid,
    pub consent_type_id: Uuid,
    pub status: String,
    pub guardian_id: Option<Uuid>,
    pub academic_year_id: Option<Uuid>,
    pub recorded_on: NaiveDate,
    pub recorded_by: Option<Uuid>,
    pub note: String,
    pub created_at: DateTime<Utc>,
}

/// A consent row with the names shown in histories and reports.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ConsentView {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub consent: Consent,
    pub type_name: String,
    pub guardian_name: Option<String>,
    pub recorded_by_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewConsent {
    pub student_id: Uuid,
    pub consent_type_id: Uuid,
    pub status: madrasa_core::consents::Status,
    pub guardian_id: Option<Uuid>,
    pub academic_year_id: Option<Uuid>,
    pub recorded_on: NaiveDate,
    pub note: String,
}

const CONSENT_COLS: &str = "c.id, c.school_id, c.student_id, c.consent_type_id, c.status, c.guardian_id, c.academic_year_id,
            c.recorded_on, c.recorded_by, c.note, c.created_at";

/// Record a status (a new history row); audited.
pub async fn record(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    input: &NewConsent,
) -> DbResult<Consent> {
    let row = sqlx::query_as::<_, Consent>(
        "INSERT INTO consents (id, school_id, student_id, consent_type_id, status, guardian_id, academic_year_id,
                               recorded_on, recorded_by, note, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $9, $11)
         RETURNING id, school_id, student_id, consent_type_id, status, guardian_id, academic_year_id,
                   recorded_on, recorded_by, note, created_at",
    )
    .bind(new_id())
    .bind(school_id)
    .bind(input.student_id)
    .bind(input.consent_type_id)
    .bind(input.status.as_str())
    .bind(input.guardian_id)
    .bind(input.academic_year_id)
    .bind(input.recorded_on)
    .bind(actor.user_id)
    .bind(input.note.trim())
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "consent",
        Some(row.id),
        None,
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

/// Every recorded row of a student, newest first.
pub async fn history(conn: &mut PgConnection, student_id: Uuid) -> DbResult<Vec<ConsentView>> {
    Ok(sqlx::query_as::<_, ConsentView>(sqlx::AssertSqlSafe(format!(
        "SELECT {CONSENT_COLS}, t.name AS type_name, g.full_name AS guardian_name, u.full_name AS recorded_by_name
         FROM consents c
         JOIN consent_types t ON t.id = c.consent_type_id
         LEFT JOIN guardians g ON g.id = c.guardian_id
         LEFT JOIN users u ON u.id = c.recorded_by
         WHERE c.student_id = $1 AND c.deleted_at IS NULL AND t.deleted_at IS NULL
         ORDER BY c.recorded_on DESC, c.created_at DESC"
    )))
    .bind(student_id)
    .fetch_all(conn)
    .await?)
}

/// The latest row per (student, type) for the given students.
pub async fn latest_for_students(
    conn: &mut PgConnection,
    student_ids: &[Uuid],
) -> DbResult<Vec<ConsentView>> {
    Ok(
        sqlx::query_as::<_, ConsentView>(sqlx::AssertSqlSafe(format!(
            "SELECT DISTINCT ON (c.student_id, c.consent_type_id) {CONSENT_COLS},
                t.name AS type_name, g.full_name AS guardian_name, u.full_name AS recorded_by_name
         FROM consents c
         JOIN consent_types t ON t.id = c.consent_type_id AND t.deleted_at IS NULL
         LEFT JOIN guardians g ON g.id = c.guardian_id
         LEFT JOIN users u ON u.id = c.recorded_by
         WHERE c.student_id = ANY($1) AND c.deleted_at IS NULL
         ORDER BY c.student_id, c.consent_type_id, c.recorded_on DESC, c.created_at DESC"
        )))
        .bind(student_ids)
        .fetch_all(conn)
        .await?,
    )
}
