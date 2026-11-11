//! The hub's own tables (`nexus` schema): users, sessions, settings and the audit log.
//! Every mutation writes the audit log in the same transaction, as in the school schema.

use chrono::{DateTime, Duration, Utc};
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use sqlx::{Connection, PgConnection, PgPool};
use uuid::Uuid;

/// Embedded hub migrations (`crates/nexus/migrations`).
pub static HUB_MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// The schema that holds the hub's own tables. Each school has its own schema beside it.
pub const HUB_SCHEMA: &str = "nexus";

pub const KEY_NODE_ID: &str = "node_id";
pub const KEY_HUB_NAME: &str = "hub.name";

/// Open the hub pool. Every connection starts with `search_path` on the hub schema, so the
/// hub's queries never see a school's tables by accident. The URL holds the password: it is
/// never logged.
pub async fn connect(database_url: &str) -> Result<PgPool, sqlx::Error> {
    let mut conn = PgConnection::connect(database_url).await?;
    sqlx::query("CREATE SCHEMA IF NOT EXISTS nexus")
        .execute(&mut conn)
        .await?;
    conn.close().await.ok();
    PgPoolOptions::new()
        .max_connections(16)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                sqlx::query("SET search_path TO nexus")
                    .execute(&mut *conn)
                    .await?;
                Ok(())
            })
        })
        .connect(database_url)
        .await
}

/// Apply pending hub migrations. Safe to call on every start.
pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    HUB_MIGRATOR.run(pool).await
}

// ---- settings ---------------------------------------------------------------------------

pub async fn setting(conn: &mut PgConnection, key: &str) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar::<_, Value>(
        "SELECT value FROM hub_settings WHERE key = $1 AND deleted_at IS NULL",
    )
    .bind(key)
    .fetch_optional(conn)
    .await
}

pub async fn set_setting(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Option<Uuid>,
    key: &str,
    value: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO hub_settings (id, key, value, updated_by, hlc) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now(),
             updated_by = EXCLUDED.updated_by, deleted_at = NULL, hlc = EXCLUDED.hlc",
    )
    .bind(new_id())
    .bind(key)
    .bind(value)
    .bind(actor)
    .bind(hlc.now().to_string())
    .execute(conn)
    .await?;
    Ok(())
}

/// The hub's node id for HLC stamps, created on first start.
pub async fn node_id(conn: &mut PgConnection) -> Result<u64, sqlx::Error> {
    if let Some(n) = setting(&mut *conn, KEY_NODE_ID)
        .await?
        .and_then(|v| v.as_u64())
    {
        return Ok(n);
    }
    // 48 bits so it fits the HLC node field.
    let node = rand::random::<u64>() & 0xffff_ffff_ffff;
    set_setting(conn, &Hlc::new(node), None, KEY_NODE_ID, Value::from(node)).await?;
    Ok(node)
}

// ---- audit ------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AuditEntry {
    pub id: Uuid,
    pub actor_user_id: Option<Uuid>,
    pub action: String,
    pub entity: String,
    pub entity_id: Option<Uuid>,
    pub old_value: Option<Value>,
    pub new_value: Option<Value>,
    pub at: DateTime<Utc>,
}

/// Append one row to the hub audit log. Call inside the transaction of the change.
// Positional arguments mirror the hub_audit_log columns, as in `madrasa_db::audit::record`.
#[allow(clippy::too_many_arguments)]
pub async fn audit(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Option<Uuid>,
    action: &str,
    entity: &str,
    entity_id: Option<Uuid>,
    old_value: Option<Value>,
    new_value: Option<Value>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO hub_audit_log (id, actor_user_id, action, entity, entity_id, old_value, new_value, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(new_id())
    .bind(actor)
    .bind(action)
    .bind(entity)
    .bind(entity_id)
    .bind(old_value)
    .bind(new_value)
    .bind(hlc.now().to_string())
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn audit_recent(
    conn: &mut PgConnection,
    limit: i64,
    offset: i64,
) -> Result<Vec<AuditEntry>, sqlx::Error> {
    sqlx::query_as::<_, AuditEntry>(
        "SELECT id, actor_user_id, action, entity, entity_id, old_value, new_value, at
         FROM hub_audit_log ORDER BY at DESC, id DESC LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(conn)
    .await
}

// ---- users ------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct HubUser {
    pub id: Uuid,
    pub username: String,
    pub full_name: String,
    pub email: Option<String>,
    /// Argon2id PHC string. Never serialized: not in responses, not in the audit log.
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub permissions: Vec<String>,
    pub is_active: bool,
    #[serde(skip_serializing)]
    pub failed_login_count: i32,
    pub locked_until: Option<DateTime<Utc>>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const USER_COLS: &str = "id, username, full_name, email, password_hash, permissions, is_active, \
    failed_login_count, locked_until, last_login_at, created_at, updated_at";

fn user_sql(tail: &str) -> sqlx::AssertSqlSafe<String> {
    sqlx::AssertSqlSafe(format!("SELECT {USER_COLS} FROM hub_users {tail}"))
}

pub async fn count_users(conn: &mut PgConnection) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT count(*) FROM hub_users WHERE deleted_at IS NULL")
        .fetch_one(conn)
        .await
}

pub async fn list_users(conn: &mut PgConnection) -> Result<Vec<HubUser>, sqlx::Error> {
    sqlx::query_as::<_, HubUser>(user_sql("WHERE deleted_at IS NULL ORDER BY username"))
        .fetch_all(conn)
        .await
}

pub async fn get_user(conn: &mut PgConnection, id: Uuid) -> Result<Option<HubUser>, sqlx::Error> {
    sqlx::query_as::<_, HubUser>(user_sql("WHERE id = $1 AND deleted_at IS NULL"))
        .bind(id)
        .fetch_optional(conn)
        .await
}

pub async fn find_user(
    conn: &mut PgConnection,
    username: &str,
) -> Result<Option<HubUser>, sqlx::Error> {
    sqlx::query_as::<_, HubUser>(user_sql(
        "WHERE lower(username) = lower($1) AND deleted_at IS NULL",
    ))
    .bind(username.trim())
    .fetch_optional(conn)
    .await
}

pub struct NewHubUser {
    pub username: String,
    pub full_name: String,
    pub email: Option<String>,
    pub password_hash: String,
    pub permissions: Vec<String>,
}

/// Create a hub user. `Ok(None)` when the username is taken.
pub async fn create_user(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Option<Uuid>,
    input: NewHubUser,
) -> Result<Option<HubUser>, sqlx::Error> {
    let id = new_id();
    let res = sqlx::query(
        "INSERT INTO hub_users (id, username, full_name, email, password_hash, permissions, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(input.username.trim().to_lowercase())
    .bind(input.full_name.trim())
    .bind(input.email.map(|e| e.trim().to_lowercase()).filter(|e| !e.is_empty()))
    .bind(&input.password_hash)
    .bind(&input.permissions)
    .bind(actor)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await;
    match res {
        Ok(_) => {}
        Err(sqlx::Error::Database(db)) if db.is_unique_violation() => return Ok(None),
        Err(e) => return Err(e),
    }
    let user = get_user(&mut *conn, id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    audit(
        conn,
        hlc,
        actor,
        "create",
        "hub_user",
        Some(id),
        None,
        Some(json!(user)),
    )
    .await?;
    Ok(Some(user))
}

#[derive(Debug, Default)]
pub struct HubUserChange {
    pub full_name: Option<String>,
    pub email: Option<Option<String>>,
    pub permissions: Option<Vec<String>>,
    pub is_active: Option<bool>,
}

pub async fn update_user(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Option<Uuid>,
    before: &HubUser,
    change: HubUserChange,
) -> Result<HubUser, sqlx::Error> {
    let full_name = change.full_name.unwrap_or_else(|| before.full_name.clone());
    let email = change.email.unwrap_or_else(|| before.email.clone());
    let permissions = change
        .permissions
        .unwrap_or_else(|| before.permissions.clone());
    let is_active = change.is_active.unwrap_or(before.is_active);
    sqlx::query(
        "UPDATE hub_users SET full_name = $2, email = $3, permissions = $4, is_active = $5,
             updated_at = now(), updated_by = $6, hlc = $7 WHERE id = $1",
    )
    .bind(before.id)
    .bind(full_name.trim())
    .bind(
        email
            .map(|e| e.trim().to_lowercase())
            .filter(|e| !e.is_empty()),
    )
    .bind(&permissions)
    .bind(is_active)
    .bind(actor)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await?;
    // A disabled account loses its sessions at once.
    if !is_active {
        revoke_user_sessions(&mut *conn, hlc, before.id).await?;
    }
    let after = get_user(&mut *conn, before.id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    audit(
        conn,
        hlc,
        actor,
        "update",
        "hub_user",
        Some(before.id),
        Some(json!(before)),
        Some(json!(after)),
    )
    .await?;
    Ok(after)
}

/// Set a new password hash; ends the user's sessions and clears any lock.
pub async fn set_password(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Option<Uuid>,
    user_id: Uuid,
    password_hash: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE hub_users SET password_hash = $2, failed_login_count = 0, locked_until = NULL,
             updated_at = now(), updated_by = $3, hlc = $4 WHERE id = $1",
    )
    .bind(user_id)
    .bind(password_hash)
    .bind(actor)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await?;
    revoke_user_sessions(&mut *conn, hlc, user_id).await?;
    // The audit row says that it happened, never the hash.
    audit(
        conn,
        hlc,
        actor,
        "password_reset",
        "hub_user",
        Some(user_id),
        None,
        None,
    )
    .await
}

pub async fn record_failed_login(
    conn: &mut PgConnection,
    hlc: &Hlc,
    user_id: Uuid,
    failed_count: i32,
    locked_until: Option<DateTime<Utc>>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE hub_users SET failed_login_count = $2, locked_until = $3, updated_at = now(), hlc = $4
         WHERE id = $1",
    )
    .bind(user_id)
    .bind(failed_count)
    .bind(locked_until)
    .bind(hlc.now().to_string())
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn record_login(
    conn: &mut PgConnection,
    hlc: &Hlc,
    user_id: Uuid,
    now: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE hub_users SET failed_login_count = 0, locked_until = NULL, last_login_at = $2,
             updated_at = now(), hlc = $3 WHERE id = $1",
    )
    .bind(user_id)
    .bind(now)
    .bind(hlc.now().to_string())
    .execute(conn)
    .await?;
    Ok(())
}

/// Active users holding `nexus.admin`, optionally not counting one user.
pub async fn count_admins(
    conn: &mut PgConnection,
    except: Option<Uuid>,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT count(*) FROM hub_users
         WHERE deleted_at IS NULL AND is_active AND 'nexus.admin' = ANY(permissions)
           AND ($1::uuid IS NULL OR id <> $1)",
    )
    .bind(except)
    .fetch_one(conn)
    .await
}

// ---- sessions ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct HubSession {
    pub id: Uuid,
    pub user_id: Uuid,
    pub expires_at: DateTime<Utc>,
}

pub async fn create_session(
    conn: &mut PgConnection,
    hlc: &Hlc,
    user_id: Uuid,
    token_hash: &str,
    expires_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO hub_sessions (id, user_id, token_hash, expires_at, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $2, $5)",
    )
    .bind(new_id())
    .bind(user_id)
    .bind(token_hash)
    .bind(expires_at)
    .bind(hlc.now().to_string())
    .execute(conn)
    .await?;
    Ok(())
}

/// The live session for a token hash and its active user, if any.
pub async fn find_live_session(
    conn: &mut PgConnection,
    token_hash: &str,
    now: DateTime<Utc>,
) -> Result<Option<(HubSession, HubUser)>, sqlx::Error> {
    let session = sqlx::query_as::<_, HubSession>(
        "SELECT id, user_id, expires_at FROM hub_sessions
         WHERE token_hash = $1 AND revoked_at IS NULL AND deleted_at IS NULL AND expires_at > $2",
    )
    .bind(token_hash)
    .bind(now)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(session) = session else {
        return Ok(None);
    };
    match get_user(conn, session.user_id).await? {
        Some(user) if user.is_active => Ok(Some((session, user))),
        _ => Ok(None),
    }
}

/// Slide the expiry forward. Skipped when the session was touched in the last minute.
pub async fn touch_session(
    conn: &mut PgConnection,
    hlc: &Hlc,
    session_id: Uuid,
    now: DateTime<Utc>,
    lifetime: Duration,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE hub_sessions SET last_seen_at = $2, expires_at = $3, updated_at = now(), hlc = $4
         WHERE id = $1 AND last_seen_at < $2 - interval '1 minute'",
    )
    .bind(session_id)
    .bind(now)
    .bind(now + lifetime)
    .bind(hlc.now().to_string())
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn revoke_session(
    conn: &mut PgConnection,
    hlc: &Hlc,
    session_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE hub_sessions SET revoked_at = now(), updated_at = now(), hlc = $2
         WHERE id = $1 AND revoked_at IS NULL",
    )
    .bind(session_id)
    .bind(hlc.now().to_string())
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn revoke_user_sessions(
    conn: &mut PgConnection,
    hlc: &Hlc,
    user_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE hub_sessions SET revoked_at = now(), updated_at = now(), hlc = $2
         WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(user_id)
    .bind(hlc.now().to_string())
    .execute(conn)
    .await?;
    Ok(())
}
