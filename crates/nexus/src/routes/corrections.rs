//! Head office corrections (`nexus.admin`): find a record in a joined school's copy and
//! correct a name or a contact detail. The change is made in the school's schema at the
//! hub, so the next sync carries it down; if the school changed the same field meanwhile,
//! head office wins and the school's value goes to the overwrite log (ADR 0005 §4).
//!
//! Only the fields listed here can be changed. Everything else about a school's records
//! is the school's to edit.

use axum::extract::{Path, Query, State};
use axum::routing::{get, patch};
use axum::{Json, Router};
use madrasa_db::{audit, Actor};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::{AssertSqlSafe, Row};
use uuid::Uuid;

use super::JsonBody;
use crate::auth::CurrentHubUser;
use crate::permissions::HubPermission;
use crate::tenants::{self, HubSchool};
use crate::{store, HubError, HubResult, HubState};

/// A field head office may correct.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Field {
    pub name: &'static str,
    /// May not be left empty.
    pub required: bool,
    /// Stored as "no value" when left empty (otherwise as empty text).
    #[serde(skip)]
    nullable: bool,
}

const fn required(name: &'static str) -> Field {
    Field {
        name,
        required: true,
        nullable: false,
    }
}
const fn optional(name: &'static str) -> Field {
    Field {
        name,
        required: false,
        nullable: true,
    }
}
const fn text(name: &'static str) -> Field {
    Field {
        name,
        required: false,
        nullable: false,
    }
}

/// A kind of record head office may correct.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Kind {
    pub code: &'static str,
    #[serde(skip)]
    table: &'static str,
    /// The entity name the school's own audit log uses.
    #[serde(skip)]
    entity: &'static str,
    /// A second column that helps tell records apart (a number, a phone).
    #[serde(skip)]
    hint: &'static str,
    pub fields: &'static [Field],
}

pub const KINDS: [Kind; 4] = [
    Kind {
        code: "school",
        table: "schools",
        entity: "school",
        hint: "code",
        fields: &[required("name"), text("city")],
    },
    Kind {
        code: "students",
        table: "students",
        entity: "student",
        hint: "student_number",
        fields: &[
            required("full_name"),
            required("father_name"),
            text("address"),
        ],
    },
    Kind {
        code: "guardians",
        table: "guardians",
        entity: "guardian",
        hint: "phone",
        fields: &[
            required("full_name"),
            optional("phone"),
            optional("email"),
            text("address"),
        ],
    },
    Kind {
        code: "staff",
        table: "staff",
        entity: "staff",
        hint: "staff_number",
        fields: &[
            required("full_name"),
            text("father_name"),
            optional("phone"),
            optional("email"),
        ],
    },
];

/// Longest value accepted for any field.
const MAX_LEN: usize = 200;
const PAGE: i64 = 25;

fn kind(code: &str) -> HubResult<&'static Kind> {
    KINDS
        .iter()
        .find(|k| k.code == code)
        .ok_or_else(|| HubError::not_found("This kind of record"))
}

/// The first field is the record's name.
fn title_field(kind: &Kind) -> &'static str {
    kind.fields[0].name
}

pub fn router() -> Router<HubState> {
    Router::new()
        .route("/corrections/kinds", get(kinds))
        .route("/schools/{id}/records/{kind}", get(search))
        .route("/schools/{id}/records/{kind}/{record}", patch(correct))
}

async fn kinds(current: CurrentHubUser) -> HubResult<Json<&'static [Kind]>> {
    current.require(HubPermission::Admin)?;
    Ok(Json(&KINDS))
}

async fn joined_school(state: &HubState, id: Uuid) -> HubResult<HubSchool> {
    let mut conn = state.pool.acquire().await?;
    let school = tenants::get(&mut conn, id)
        .await?
        .ok_or_else(|| HubError::not_found("School"))?;
    if school.status != "active" {
        return Err(HubError::conflict(
            "Only the records of a school that is joined can be corrected.",
        ));
    }
    Ok(school)
}

/// A record as the console shows it: its name, a hint, and the correctable fields.
#[derive(Debug, Serialize)]
pub struct Record {
    pub id: Uuid,
    pub title: String,
    pub hint: Option<String>,
    pub fields: Map<String, Value>,
}

fn select_list(kind: &Kind) -> String {
    let fields: Vec<&str> = kind.fields.iter().map(|f| f.name).collect();
    format!("id, {}::text AS hint_, {}", kind.hint, fields.join(", "))
}

fn read(kind: &Kind, row: &sqlx::postgres::PgRow) -> Result<Record, sqlx::Error> {
    let mut fields = Map::new();
    for f in kind.fields {
        let value: Option<String> = row.try_get(f.name)?;
        fields.insert(f.name.to_string(), value.map_or(Value::Null, Value::String));
    }
    Ok(Record {
        id: row.try_get("id")?,
        title: fields[title_field(kind)]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        hint: row.try_get("hint_")?,
        fields,
    })
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub page: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct Found {
    pub items: Vec<Record>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

/// Find records of one kind in a school's copy, by name or by the hint column.
async fn search(
    State(state): State<HubState>,
    current: CurrentHubUser,
    Path((id, kind_code)): Path<(Uuid, String)>,
    q: Result<Query<SearchQuery>, axum::extract::rejection::QueryRejection>,
) -> HubResult<Json<Found>> {
    current.require(HubPermission::Admin)?;
    let Query(q) = q?;
    let kind = kind(&kind_code)?;
    let school = joined_school(&state, id).await?;
    let page = q.page.unwrap_or(1).max(1);
    let needle = q.q.trim();
    // `%` and `_` in what was typed are matched as themselves.
    let pattern = format!(
        "%{}%",
        needle
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );
    let title = title_field(kind);
    let filter = format!(
        "deleted_at IS NULL AND ($1 = '%%' OR {title} ILIKE $1 OR {}::text ILIKE $1)",
        kind.hint
    );
    let mut tx = tenants::begin(&state.pool, &school).await?;
    let total: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
        "SELECT count(*) FROM {} WHERE {filter}",
        kind.table
    )))
    .bind(&pattern)
    .fetch_one(&mut *tx)
    .await?;
    let rows = sqlx::query(AssertSqlSafe(format!(
        "SELECT {} FROM {} WHERE {filter} ORDER BY {title}, id LIMIT {PAGE} OFFSET $2",
        select_list(kind),
        kind.table
    )))
    .bind(&pattern)
    .bind((page - 1) * PAGE)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    let items = rows
        .iter()
        .map(|r| read(kind, r))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(Found {
        items,
        total,
        page,
        page_size: PAGE,
    }))
}

#[derive(Debug, Deserialize)]
pub struct Correction {
    /// Field name → new value. Only the fields of the kind are accepted.
    pub fields: Map<String, Value>,
}

#[derive(Debug, Serialize)]
pub struct Corrected {
    pub record: Record,
    /// The fields whose value really changed.
    pub changed: Vec<String>,
}

/// Check one new value against its field. `Ok(None)` stores "no value".
fn clean(field: &Field, value: &Value) -> HubResult<Option<String>> {
    let label = field.name.replace('_', " ");
    let raw = match value {
        Value::Null => "",
        Value::String(s) => s.trim(),
        _ => return Err(HubError::validation(format!("The {label} must be text."))),
    };
    if raw.is_empty() {
        if field.required {
            return Err(HubError::validation(format!(
                "The {label} cannot be empty."
            )));
        }
        return Ok(if field.nullable {
            None
        } else {
            Some(String::new())
        });
    }
    if raw.chars().count() > MAX_LEN {
        return Err(HubError::validation(format!(
            "The {label} is too long (at most {MAX_LEN} characters)."
        )));
    }
    if raw.chars().any(char::is_control) {
        return Err(HubError::validation(format!(
            "The {label} contains characters that cannot be used."
        )));
    }
    if field.name == "email" && !(raw.contains('@') && !raw.contains(char::is_whitespace)) {
        return Err(HubError::validation("The email address is not valid."));
    }
    Ok(Some(raw.to_string()))
}

/// Correct fields of one record in a school's copy. The school gets the change at its next
/// sync; both audit logs (the hub's and the school's own) say that head office made it.
async fn correct(
    State(state): State<HubState>,
    current: CurrentHubUser,
    Path((id, kind_code, record)): Path<(Uuid, String, Uuid)>,
    body: JsonBody<Correction>,
) -> HubResult<Json<Corrected>> {
    current.require(HubPermission::Admin)?;
    let Json(b) = body?;
    let kind = kind(&kind_code)?;
    if b.fields.is_empty() {
        return Err(HubError::validation("Nothing to change."));
    }
    let mut wanted: Vec<(&Field, Option<String>)> = Vec::new();
    for (name, value) in &b.fields {
        let field = kind.fields.iter().find(|f| f.name == name).ok_or_else(|| {
            HubError::validation(format!(
                "Head office cannot change \"{name}\" of this kind of record."
            ))
        })?;
        wanted.push((field, clean(field, value)?));
    }
    let school = joined_school(&state, id).await?;

    let mut tx = tenants::begin(&state.pool, &school).await?;
    let before = sqlx::query(AssertSqlSafe(format!(
        "SELECT {} FROM {} WHERE id = $1 AND deleted_at IS NULL FOR UPDATE",
        select_list(kind),
        kind.table
    )))
    .bind(record)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| HubError::not_found("The record"))?;
    let before = read(kind, &before)?;
    let changes: Vec<(&Field, Option<String>)> = wanted
        .into_iter()
        .filter(|(f, new)| before.fields[f.name].as_str() != new.as_deref())
        .collect();
    if changes.is_empty() {
        tx.commit().await?;
        return Ok(Json(Corrected {
            record: before,
            changed: Vec::new(),
        }));
    }
    let sets: Vec<String> = changes
        .iter()
        .enumerate()
        .map(|(i, (f, _))| format!("{} = ${}", f.name, i + 3))
        .collect();
    let sql = format!(
        "UPDATE {} SET {}, updated_at = now(), updated_by = NULL, hlc = $2 WHERE id = $1 RETURNING {}",
        kind.table,
        sets.join(", "),
        select_list(kind)
    );
    let mut query = sqlx::query(AssertSqlSafe(sql))
        .bind(record)
        .bind(state.hlc.now().to_string());
    for (_, value) in &changes {
        query = query.bind(value.as_deref());
    }
    let after = query.fetch_one(&mut *tx).await.map_err(|e| match &e {
        // A value the school's own rules refuse (a duplicate, a check).
        sqlx::Error::Database(db) if db.code().is_some_and(|c| c.starts_with("23")) => {
            HubError::conflict("The school's records do not allow this value.")
        }
        _ => e.into(),
    })?;
    let after = read(kind, &after)?;
    let changed: Vec<String> = changes.iter().map(|(f, _)| f.name.to_string()).collect();
    let pick = |from: &Record| -> Value {
        Value::Object(
            changed
                .iter()
                .map(|name| (name.clone(), from.fields[name].clone()))
                .collect(),
        )
    };
    let (old, mut new) = (pick(&before), pick(&after));
    // The school's own audit log, in its schema: it syncs down with the change.
    let mut for_school = new.clone();
    for_school["corrected_by"] = Value::String(format!("Head office: {}", current.user.full_name));
    audit::record(
        &mut tx,
        &state.hlc,
        Actor::system_for(school.school_id),
        audit::Action::Update,
        kind.entity,
        Some(record),
        Some(old.clone()),
        Some(for_school),
    )
    .await?;
    if kind.code == "school" {
        // The register shows the school under its corrected name.
        if let Some(name) = after.fields["name"].as_str() {
            sqlx::query(AssertSqlSafe(format!(
                "UPDATE {}.hub_schools SET name = $2, updated_at = now(), updated_by = $3, hlc = $4 WHERE id = $1",
                store::HUB_SCHEMA
            )))
            .bind(school.id)
            .bind(name)
            .bind(current.user.id)
            .bind(state.hlc.now().to_string())
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;

    new["kind"] = Value::String(kind.code.to_string());
    new["record_id"] = Value::String(record.to_string());
    let mut conn = state.pool.acquire().await?;
    store::audit(
        &mut conn,
        &state.hlc,
        Some(current.user.id),
        "correct",
        "hub_school",
        Some(school.id),
        Some(old),
        Some(new),
    )
    .await?;
    Ok(Json(Corrected {
        record: after,
        changed,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn values_are_trimmed_checked_and_emptied_the_right_way() {
        let name = required("full_name");
        assert_eq!(
            clean(&name, &json!("  Ali Khan ")).unwrap(),
            Some("Ali Khan".into())
        );
        assert_eq!(
            clean(&name, &json!("  ")).unwrap_err().message,
            "The full name cannot be empty."
        );
        assert!(clean(&name, &json!(12)).is_err());
        assert!(clean(&name, &json!("x".repeat(201))).is_err());
        assert!(clean(&name, &json!("Ali\u{0}Khan")).is_err());
        // Urdu names are names.
        assert_eq!(
            clean(&name, &json!("علی خان")).unwrap(),
            Some("علی خان".into())
        );
        // An emptied optional value is "no value"; an emptied text column is empty text.
        assert_eq!(clean(&optional("phone"), &json!("")).unwrap(), None);
        assert_eq!(clean(&optional("phone"), &Value::Null).unwrap(), None);
        assert_eq!(
            clean(&text("address"), &json!(" ")).unwrap(),
            Some(String::new())
        );
        assert!(clean(&optional("email"), &json!("not an email")).is_err());
        assert!(clean(&optional("email"), &json!("a b@example.org")).is_err());
        assert_eq!(
            clean(&optional("email"), &json!("sana@example.org")).unwrap(),
            Some("sana@example.org".into())
        );
    }

    #[test]
    fn every_kind_names_itself_and_only_plain_columns() {
        for k in KINDS {
            assert!(
                k.fields[0].required,
                "{}: the first field is the name",
                k.code
            );
            for name in k.fields.iter().map(|f| f.name).chain([k.table, k.hint]) {
                assert!(
                    name.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                    "{name} goes into SQL as an identifier"
                );
            }
            assert_eq!(kind(k.code).unwrap().table, k.table);
        }
        assert!(
            kind("fee_receipts").is_err(),
            "money is never corrected from head office"
        );
        assert!(kind("users").is_err());
    }
}
