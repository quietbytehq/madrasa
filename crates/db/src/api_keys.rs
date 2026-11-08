//! Read-only API keys (Standard stage 9): keys stored as hashes with scopes, an IP
//! allow-list and an expiry, and the append-only log of calls made with them.

use chrono::{DateTime, Utc};
use madrasa_core::api_keys as rules;
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::audit::{self, snapshot, Action};
use crate::{Actor, DbError, DbResult};

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ApiKey {
    pub id: Uuid,
    pub school_id: Uuid,
    pub name: String,
    pub key_prefix: String,
    #[serde(skip)]
    pub key_hash: String,
    pub scopes: Vec<String>,
    pub ip_allow: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_by: Option<Uuid>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub hlc: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiKeyInput {
    pub name: String,
    pub scopes: Vec<String>,
    #[serde(default)]
    pub ip_allow: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

const COLS: &str = "id, school_id, name, key_prefix, key_hash, scopes, ip_allow, expires_at, last_used_at, created_by, revoked_at, created_at, hlc";

pub async fn list(conn: &mut PgConnection, school_id: Uuid) -> DbResult<Vec<ApiKey>> {
    Ok(sqlx::query_as::<_, ApiKey>(sqlx::AssertSqlSafe(format!(
        "SELECT {COLS} FROM api_keys WHERE school_id = $1 AND deleted_at IS NULL ORDER BY revoked_at IS NOT NULL, created_at DESC"
    )))
    .bind(school_id)
    .fetch_all(conn)
    .await?)
}

pub async fn get(conn: &mut PgConnection, id: Uuid) -> DbResult<ApiKey> {
    sqlx::query_as::<_, ApiKey>(sqlx::AssertSqlSafe(format!(
        "SELECT {COLS} FROM api_keys WHERE id = $1 AND deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("API key".into()))
}

/// The key behind a presented secret, whatever its state (the caller checks usability).
pub async fn find_by_hash(conn: &mut PgConnection, hash: &str) -> DbResult<Option<ApiKey>> {
    Ok(sqlx::query_as::<_, ApiKey>(sqlx::AssertSqlSafe(format!(
        "SELECT {COLS} FROM api_keys WHERE key_hash = $1 AND deleted_at IS NULL"
    )))
    .bind(hash)
    .fetch_optional(conn)
    .await?)
}

fn check(input: &ApiKeyInput) -> DbResult<Vec<String>> {
    if input.name.trim().is_empty() {
        return Err(DbError::Conflict("the key needs a name".into()));
    }
    let mut scopes = Vec::new();
    for s in &input.scopes {
        let scope = rules::Scope::parse(s)
            .ok_or_else(|| DbError::Conflict(format!("unknown scope \"{s}\"")))?;
        if !scopes.contains(&scope.as_str().to_string()) {
            scopes.push(scope.as_str().to_string());
        }
    }
    if scopes.is_empty() {
        return Err(DbError::Conflict("pick at least one scope".into()));
    }
    rules::validate_allow_list(&input.ip_allow).map_err(DbError::Conflict)?;
    Ok(scopes)
}

/// Create a key; returns the key itself (shown once) with its row.
pub async fn create(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    input: &ApiKeyInput,
) -> DbResult<(String, ApiKey)> {
    let scopes = check(input)?;
    let (key, hash) = rules::generate_key();
    let ip_allow: Vec<String> = input
        .ip_allow
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let row = sqlx::query_as::<_, ApiKey>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO api_keys (id, school_id, name, key_prefix, key_hash, scopes, ip_allow, expires_at, created_by, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9, $10) RETURNING {COLS}"
    )))
    .bind(new_id())
    .bind(school_id)
    .bind(input.name.trim())
    .bind(rules::prefix(&key))
    .bind(&hash)
    .bind(&scopes)
    .bind(&ip_allow)
    .bind(input.expires_at)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "api_key",
        Some(row.id),
        None,
        Some(snapshot(&row)),
    )
    .await?;
    Ok((key, row))
}

pub async fn revoke(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
) -> DbResult<ApiKey> {
    let old = get(&mut *conn, id).await?;
    if old.revoked_at.is_some() {
        return Ok(old);
    }
    let row = sqlx::query_as::<_, ApiKey>(sqlx::AssertSqlSafe(format!(
        "UPDATE api_keys SET revoked_at = now(), revoked_by = $2, updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1 RETURNING {COLS}"
    )))
    .bind(id)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "api_key",
        Some(id),
        Some(snapshot(&old)),
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

/// Note a call: the key's last use and one row in the log.
pub async fn log_call(
    conn: &mut PgConnection,
    hlc: &Hlc,
    key: &ApiKey,
    method: &str,
    path: &str,
    status: u16,
    ip: &str,
) -> DbResult<()> {
    sqlx::query("UPDATE api_keys SET last_used_at = now() WHERE id = $1")
        .bind(key.id)
        .execute(&mut *conn)
        .await?;
    sqlx::query(
        "INSERT INTO api_key_calls (id, school_id, key_id, method, path, status, ip, hlc) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(new_id())
    .bind(key.school_id)
    .bind(key.id)
    .bind(method)
    .bind(path)
    .bind(i32::from(status))
    .bind(ip)
    .bind(hlc.now().to_string())
    .execute(conn)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ApiKeyCall {
    pub id: Uuid,
    pub key_id: Uuid,
    pub called_at: DateTime<Utc>,
    pub method: String,
    pub path: String,
    pub status: i32,
    pub ip: String,
}

pub async fn calls(conn: &mut PgConnection, key_id: Uuid, limit: i64) -> DbResult<Vec<ApiKeyCall>> {
    Ok(sqlx::query_as::<_, ApiKeyCall>(
        "SELECT id, key_id, called_at, method, path, status, ip FROM api_key_calls WHERE key_id = $1 ORDER BY called_at DESC LIMIT $2",
    )
    .bind(key_id)
    .bind(limit)
    .fetch_all(conn)
    .await?)
}
