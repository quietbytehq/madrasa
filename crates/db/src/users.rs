use crate::audit::{self, snapshot, Action};
use crate::models::User;
use crate::{map_unique, Actor, DbError, DbResult};
use chrono::{DateTime, Utc};
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use sqlx::PgConnection;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct NewUser {
    pub school_id: Uuid,
    pub username: String,
    pub full_name: String,
    pub email: Option<String>,
    /// Argon2id PHC string from `madrasa_core::password::hash_password`.
    pub password_hash: String,
}

pub async fn create(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    input: NewUser,
) -> DbResult<User> {
    let id = new_id();
    let user = sqlx::query_as::<_, User>(
        "INSERT INTO users (id, school_id, username, full_name, email, password_hash, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING *",
    )
    .bind(id)
    .bind(input.school_id)
    .bind(input.username.trim().to_lowercase())
    .bind(input.full_name.trim())
    .bind(input.email.map(|e| e.trim().to_lowercase()))
    .bind(&input.password_hash)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a user with this username"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "user",
        Some(id),
        None,
        Some(snapshot(&user)),
    )
    .await?;
    Ok(user)
}

pub async fn get(conn: &mut PgConnection, id: Uuid) -> DbResult<User> {
    sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1 AND deleted_at IS NULL")
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or_else(|| DbError::NotFound("user".into()))
}

pub async fn find_by_username(
    conn: &mut PgConnection,
    school_id: Uuid,
    username: &str,
) -> DbResult<Option<User>> {
    Ok(sqlx::query_as::<_, User>(
        "SELECT * FROM users WHERE school_id = $1 AND lower(username) = lower($2) AND deleted_at IS NULL",
    )
    .bind(school_id)
    .bind(username.trim())
    .fetch_optional(conn)
    .await?)
}

pub async fn list(conn: &mut PgConnection, school_id: Uuid) -> DbResult<Vec<User>> {
    Ok(sqlx::query_as::<_, User>(
        "SELECT * FROM users WHERE school_id = $1 AND deleted_at IS NULL ORDER BY username",
    )
    .bind(school_id)
    .fetch_all(conn)
    .await?)
}

pub async fn count(conn: &mut PgConnection, school_id: Uuid) -> DbResult<i64> {
    Ok(
        sqlx::query_scalar(
            "SELECT count(*) FROM users WHERE school_id = $1 AND deleted_at IS NULL",
        )
        .bind(school_id)
        .fetch_one(conn)
        .await?,
    )
}

#[derive(Debug, Clone, Default)]
pub struct UserUpdate {
    pub full_name: Option<String>,
    pub email: Option<Option<String>>,
    pub is_active: Option<bool>,
}

pub async fn update(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    patch: UserUpdate,
) -> DbResult<User> {
    let before = get(&mut *conn, id).await?;
    let full_name = patch.full_name.unwrap_or_else(|| before.full_name.clone());
    let email = patch.email.unwrap_or_else(|| before.email.clone());
    let is_active = patch.is_active.unwrap_or(before.is_active);
    let after = sqlx::query_as::<_, User>(
        "UPDATE users SET full_name = $2, email = $3, is_active = $4, updated_at = now(), updated_by = $5, hlc = $6
         WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .bind(full_name.trim())
    .bind(email)
    .bind(is_active)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "user",
        Some(id),
        Some(snapshot(&before)),
        Some(snapshot(&after)),
    )
    .await?;
    Ok(after)
}

/// Replace the password hash (administrator reset). Audited as `password_reset`; the hash is
/// never written to the log (`User` skips it when serialised).
pub async fn set_password(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    password_hash: &str,
) -> DbResult<User> {
    let before = get(&mut *conn, id).await?;
    let after = sqlx::query_as::<_, User>(
        "UPDATE users SET password_hash = $2, failed_login_count = 0, locked_until = NULL,
            updated_at = now(), updated_by = $3, hlc = $4 WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .bind(password_hash)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::PasswordReset,
        "user",
        Some(id),
        Some(snapshot(&before)),
        Some(snapshot(&after)),
    )
    .await?;
    Ok(after)
}

pub async fn soft_delete(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
) -> DbResult<()> {
    let before = get(&mut *conn, id).await?;
    sqlx::query(
        "UPDATE users SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1",
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
        "user",
        Some(id),
        Some(snapshot(&before)),
        None,
    )
    .await?;
    Ok(())
}

/// Record a failed login: bump the counter, lock if the policy says so, audit it.
pub async fn record_failed_login(
    conn: &mut PgConnection,
    hlc: &Hlc,
    id: Uuid,
    failed_count: i32,
    locked_until: Option<DateTime<Utc>>,
) -> DbResult<()> {
    let user = get(&mut *conn, id).await?;
    sqlx::query(
        "UPDATE users SET failed_login_count = $2, locked_until = $3, updated_at = now(), hlc = $4 WHERE id = $1",
    )
    .bind(id)
    .bind(failed_count)
    .bind(locked_until)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await?;
    let actor = Actor::system_for(user.school_id);
    audit::record(
        &mut *conn,
        hlc,
        actor,
        Action::LoginFailed,
        "user",
        Some(id),
        None,
        None,
    )
    .await?;
    if locked_until.is_some() {
        audit::record(
            conn,
            hlc,
            actor,
            Action::Lockout,
            "user",
            Some(id),
            None,
            Some(serde_json::json!({ "locked_until": locked_until })),
        )
        .await?;
    }
    Ok(())
}

/// Record a successful login: reset the counter and lock, stamp `last_login_at`.
pub async fn record_successful_login(conn: &mut PgConnection, hlc: &Hlc, id: Uuid) -> DbResult<()> {
    let user = get(&mut *conn, id).await?;
    sqlx::query(
        "UPDATE users SET failed_login_count = 0, locked_until = NULL, last_login_at = now(), updated_at = now(), hlc = $2
         WHERE id = $1",
    )
    .bind(id)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        Actor::user(id, user.school_id),
        Action::Login,
        "user",
        Some(id),
        None,
        None,
    )
    .await?;
    Ok(())
}
