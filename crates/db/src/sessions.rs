use crate::audit::{self, Action};
use crate::models::{Session, User};
use crate::{Actor, DbResult};
use chrono::{DateTime, Duration, Utc};
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use sqlx::PgConnection;

/// Create a session row for a token hash.
pub async fn create(
    conn: &mut PgConnection,
    hlc: &Hlc,
    user: &User,
    token_hash: &str,
    expires_at: DateTime<Utc>,
) -> DbResult<Session> {
    let id = new_id();
    Ok(sqlx::query_as::<_, Session>(
        "INSERT INTO sessions (id, school_id, user_id, token_hash, expires_at, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $3, $6)
         RETURNING id, school_id, user_id, token_hash, expires_at, last_seen_at, revoked_at, created_at, hlc",
    )
    .bind(id)
    .bind(user.school_id)
    .bind(user.id)
    .bind(token_hash)
    .bind(expires_at)
    .bind(hlc.now().to_string())
    .fetch_one(conn)
    .await?)
}

/// Find a live session (not expired, not revoked) and its active user.
pub async fn find_live(
    conn: &mut PgConnection,
    token_hash: &str,
    now: DateTime<Utc>,
) -> DbResult<Option<(Session, User)>> {
    let session = sqlx::query_as::<_, Session>(
        "SELECT id, school_id, user_id, token_hash, expires_at, last_seen_at, revoked_at, created_at, hlc
         FROM sessions WHERE token_hash = $1 AND revoked_at IS NULL AND expires_at > $2 AND deleted_at IS NULL",
    )
    .bind(token_hash)
    .bind(now)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(session) = session else {
        return Ok(None);
    };
    let user = sqlx::query_as::<_, User>(
        "SELECT * FROM users WHERE id = $1 AND deleted_at IS NULL AND is_active",
    )
    .bind(session.user_id)
    .fetch_optional(conn)
    .await?;
    Ok(user.map(|u| (session, u)))
}

/// Sliding expiry: extend if the session was last seen more than a minute ago.
pub async fn touch(
    conn: &mut PgConnection,
    hlc: &Hlc,
    session: &Session,
    now: DateTime<Utc>,
    lifetime: Duration,
) -> DbResult<()> {
    if now - session.last_seen_at < Duration::minutes(1) {
        return Ok(());
    }
    sqlx::query(
        "UPDATE sessions SET last_seen_at = $2, expires_at = $3, updated_at = now(), hlc = $4 WHERE id = $1",
    )
    .bind(session.id)
    .bind(now)
    .bind(now + lifetime)
    .bind(hlc.now().to_string())
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn revoke(conn: &mut PgConnection, hlc: &Hlc, session: &Session) -> DbResult<()> {
    sqlx::query(
        "UPDATE sessions SET revoked_at = now(), updated_at = now(), hlc = $2 WHERE id = $1",
    )
    .bind(session.id)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        Actor::user(session.user_id, session.school_id),
        Action::Logout,
        "session",
        Some(session.id),
        None,
        None,
    )
    .await?;
    Ok(())
}
