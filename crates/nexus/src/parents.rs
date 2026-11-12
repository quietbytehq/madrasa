//! Parent app accounts at the hub (docs/adr/0006-parent-app.md): accounts, their sessions,
//! the one-time access codes a school asks for, and the links from an account to guardian
//! records of joined schools. Nothing here reads a school's data.

use chrono::{DateTime, Duration, Utc};
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use madrasa_core::parent_app::{mask_phone, GuardianAccess, ACCESS_CODE_DAYS};
use serde::Serialize;
use sqlx::PgConnection;
use uuid::Uuid;

/// A phone stays signed in for this long after it was last used.
pub fn session_lifetime() -> Duration {
    Duration::days(30)
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Parent {
    pub id: Uuid,
    pub phone: String,
    pub full_name: String,
    /// Argon2id PHC string. Never serialized.
    #[serde(skip_serializing)]
    pub password_hash: String,
    #[serde(skip_serializing)]
    pub failed_login_count: i32,
    #[serde(skip_serializing)]
    pub locked_until: Option<DateTime<Utc>>,
    pub last_login_at: Option<DateTime<Utc>>,
}

const PARENT_COLS: &str =
    "id, phone, full_name, password_hash, failed_login_count, locked_until, last_login_at";

pub async fn find_by_phone(
    conn: &mut PgConnection,
    phone: &str,
) -> Result<Option<Parent>, sqlx::Error> {
    sqlx::query_as::<_, Parent>(sqlx::AssertSqlSafe(format!(
        "SELECT {PARENT_COLS} FROM hub_parents WHERE phone = $1 AND deleted_at IS NULL"
    )))
    .bind(phone)
    .fetch_optional(conn)
    .await
}

pub async fn get(conn: &mut PgConnection, id: Uuid) -> Result<Option<Parent>, sqlx::Error> {
    sqlx::query_as::<_, Parent>(sqlx::AssertSqlSafe(format!(
        "SELECT {PARENT_COLS} FROM hub_parents WHERE id = $1 AND deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await
}

pub async fn create(
    conn: &mut PgConnection,
    hlc: &Hlc,
    phone: &str,
    full_name: &str,
    password_hash: &str,
) -> Result<Parent, sqlx::Error> {
    sqlx::query_as::<_, Parent>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO hub_parents (id, phone, full_name, password_hash, hlc) VALUES ($1, $2, $3, $4, $5)
         RETURNING {PARENT_COLS}"
    )))
    .bind(new_id())
    .bind(phone)
    .bind(full_name)
    .bind(password_hash)
    .bind(hlc.now().to_string())
    .fetch_one(conn)
    .await
}

/// Set a new password, clear a lock, and sign every phone out.
pub async fn set_password(
    conn: &mut PgConnection,
    hlc: &Hlc,
    id: Uuid,
    password_hash: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE hub_parents SET password_hash = $2, failed_login_count = 0, locked_until = NULL,
             updated_at = now(), hlc = $3 WHERE id = $1",
    )
    .bind(id)
    .bind(password_hash)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE hub_parent_sessions SET revoked_at = now(), updated_at = now(), hlc = $2
         WHERE parent_id = $1 AND revoked_at IS NULL",
    )
    .bind(id)
    .bind(hlc.now().to_string())
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn record_failed_login(
    conn: &mut PgConnection,
    hlc: &Hlc,
    id: Uuid,
    failed_count: i32,
    locked_until: Option<DateTime<Utc>>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE hub_parents SET failed_login_count = $2, locked_until = $3, updated_at = now(), hlc = $4 WHERE id = $1",
    )
    .bind(id)
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
    id: Uuid,
    now: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE hub_parents SET failed_login_count = 0, locked_until = NULL, last_login_at = $2,
             updated_at = now(), hlc = $3 WHERE id = $1",
    )
    .bind(id)
    .bind(now)
    .bind(hlc.now().to_string())
    .execute(conn)
    .await?;
    Ok(())
}

// ---- sessions -------------------------------------------------------------------------------

pub async fn create_session(
    conn: &mut PgConnection,
    hlc: &Hlc,
    parent_id: Uuid,
    token_hash: &str,
    now: DateTime<Utc>,
) -> Result<DateTime<Utc>, sqlx::Error> {
    let expires_at = now + session_lifetime();
    sqlx::query(
        "INSERT INTO hub_parent_sessions (id, parent_id, token_hash, expires_at, last_seen_at, hlc)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(new_id())
    .bind(parent_id)
    .bind(token_hash)
    .bind(expires_at)
    .bind(now)
    .bind(hlc.now().to_string())
    .execute(conn)
    .await?;
    Ok(expires_at)
}

/// The live session with this token, and its account. Slides the expiry forward (at most
/// once a minute).
pub async fn find_live_session(
    conn: &mut PgConnection,
    hlc: &Hlc,
    token_hash: &str,
    now: DateTime<Utc>,
) -> Result<Option<(Uuid, Parent)>, sqlx::Error> {
    let row: Option<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT id, parent_id FROM hub_parent_sessions
         WHERE token_hash = $1 AND revoked_at IS NULL AND deleted_at IS NULL AND expires_at > $2",
    )
    .bind(token_hash)
    .bind(now)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((session_id, parent_id)) = row else {
        return Ok(None);
    };
    let Some(parent) = get(conn, parent_id).await? else {
        return Ok(None);
    };
    sqlx::query(
        "UPDATE hub_parent_sessions SET last_seen_at = $2, expires_at = $3, updated_at = now(), hlc = $4
         WHERE id = $1 AND last_seen_at < $2 - interval '1 minute'",
    )
    .bind(session_id)
    .bind(now)
    .bind(now + session_lifetime())
    .bind(hlc.now().to_string())
    .execute(conn)
    .await?;
    Ok(Some((session_id, parent)))
}

pub async fn revoke_session(
    conn: &mut PgConnection,
    hlc: &Hlc,
    session_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE hub_parent_sessions SET revoked_at = now(), updated_at = now(), hlc = $2
         WHERE id = $1 AND revoked_at IS NULL",
    )
    .bind(session_id)
    .bind(hlc.now().to_string())
    .execute(conn)
    .await?;
    Ok(())
}

// ---- access codes ---------------------------------------------------------------------------

/// A code that can still be used.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LiveCode {
    pub id: Uuid,
    pub hub_school_id: Uuid,
    pub guardian_id: Uuid,
    pub guardian_name: String,
}

/// Store a new code's hash for a guardian; earlier unused codes for the same guardian stop
/// working. Returns when the new one expires.
pub async fn issue_code(
    conn: &mut PgConnection,
    hlc: &Hlc,
    hub_school_id: Uuid,
    guardian_id: Uuid,
    guardian_name: &str,
    code_hash: &str,
    now: DateTime<Utc>,
) -> Result<DateTime<Utc>, sqlx::Error> {
    revoke_codes(conn, hlc, hub_school_id, Some(guardian_id)).await?;
    let expires_at = now + Duration::days(ACCESS_CODE_DAYS);
    sqlx::query(
        "INSERT INTO hub_parent_access_codes (id, hub_school_id, guardian_id, guardian_name, code_hash, expires_at, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(new_id())
    .bind(hub_school_id)
    .bind(guardian_id)
    .bind(guardian_name)
    .bind(code_hash)
    .bind(expires_at)
    .bind(hlc.now().to_string())
    .execute(conn)
    .await?;
    Ok(expires_at)
}

/// Unused codes of one guardian, or of the whole school, stop working.
pub async fn revoke_codes(
    conn: &mut PgConnection,
    hlc: &Hlc,
    hub_school_id: Uuid,
    guardian_id: Option<Uuid>,
) -> Result<u64, sqlx::Error> {
    let done = sqlx::query(
        "UPDATE hub_parent_access_codes SET revoked_at = now(), updated_at = now(), hlc = $3
         WHERE hub_school_id = $1 AND ($2::uuid IS NULL OR guardian_id = $2)
           AND used_at IS NULL AND revoked_at IS NULL",
    )
    .bind(hub_school_id)
    .bind(guardian_id)
    .bind(hlc.now().to_string())
    .execute(conn)
    .await?;
    Ok(done.rows_affected())
}

/// The code with this hash, when it has not been used, revoked or expired. Locks the row,
/// so two phones cannot both use one code.
pub async fn live_code(
    conn: &mut PgConnection,
    code_hash: &str,
    now: DateTime<Utc>,
) -> Result<Option<LiveCode>, sqlx::Error> {
    sqlx::query_as::<_, LiveCode>(
        "SELECT id, hub_school_id, guardian_id, guardian_name FROM hub_parent_access_codes
         WHERE code_hash = $1 AND used_at IS NULL AND revoked_at IS NULL AND expires_at > $2 AND deleted_at IS NULL
         FOR UPDATE",
    )
    .bind(code_hash)
    .bind(now)
    .fetch_optional(conn)
    .await
}

pub async fn use_code(
    conn: &mut PgConnection,
    hlc: &Hlc,
    code_id: Uuid,
    parent_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE hub_parent_access_codes SET used_at = now(), used_by = $2, updated_at = now(), hlc = $3 WHERE id = $1",
    )
    .bind(code_id)
    .bind(parent_id)
    .bind(hlc.now().to_string())
    .execute(conn)
    .await?;
    Ok(())
}

// ---- links ------------------------------------------------------------------------------------

/// A guardian record an account may act as: which school, which guardian.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Link {
    pub hub_school_id: Uuid,
    pub guardian_id: Uuid,
}

/// The account's live links to schools that are still joined.
pub async fn links(conn: &mut PgConnection, parent_id: Uuid) -> Result<Vec<Link>, sqlx::Error> {
    sqlx::query_as::<_, Link>(
        "SELECT l.hub_school_id, l.guardian_id FROM hub_parent_links l
         JOIN hub_schools s ON s.id = l.hub_school_id AND s.status = 'active' AND s.deleted_at IS NULL
         WHERE l.parent_id = $1 AND l.ended_at IS NULL AND l.deleted_at IS NULL
         ORDER BY l.linked_at, l.id",
    )
    .bind(parent_id)
    .fetch_all(conn)
    .await
}

/// The account that holds the live link of a guardian record, if any.
pub async fn linked_parent(
    conn: &mut PgConnection,
    hub_school_id: Uuid,
    guardian_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT parent_id FROM hub_parent_links WHERE hub_school_id = $1 AND guardian_id = $2 AND ended_at IS NULL",
    )
    .bind(hub_school_id)
    .bind(guardian_id)
    .fetch_optional(conn)
    .await
}

/// Link an account to a guardian record. A link the guardian record had to another account
/// ends (the newer code wins: a phone was replaced).
pub async fn link(
    conn: &mut PgConnection,
    hlc: &Hlc,
    parent_id: Uuid,
    hub_school_id: Uuid,
    guardian_id: Uuid,
) -> Result<(), sqlx::Error> {
    if linked_parent(conn, hub_school_id, guardian_id).await? == Some(parent_id) {
        return Ok(());
    }
    end_links(conn, hlc, hub_school_id, Some(guardian_id), "replaced").await?;
    sqlx::query(
        "INSERT INTO hub_parent_links (id, parent_id, hub_school_id, guardian_id, hlc) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(new_id())
    .bind(parent_id)
    .bind(hub_school_id)
    .bind(guardian_id)
    .bind(hlc.now().to_string())
    .execute(conn)
    .await?;
    Ok(())
}

/// End the live link of one guardian record, or every live link of a school. `reason` is
/// one of `school`, `replaced`, `left`, `removed`, `purged`.
pub async fn end_links(
    conn: &mut PgConnection,
    hlc: &Hlc,
    hub_school_id: Uuid,
    guardian_id: Option<Uuid>,
    reason: &str,
) -> Result<u64, sqlx::Error> {
    let done = sqlx::query(
        "UPDATE hub_parent_links SET ended_at = now(), ended_reason = $3, updated_at = now(), hlc = $4
         WHERE hub_school_id = $1 AND ($2::uuid IS NULL OR guardian_id = $2) AND ended_at IS NULL",
    )
    .bind(hub_school_id)
    .bind(guardian_id)
    .bind(reason)
    .bind(hlc.now().to_string())
    .execute(conn)
    .await?;
    Ok(done.rows_affected())
}

/// A school's membership ended: its parents' links end and unused codes stop working.
pub async fn end_for_school(
    conn: &mut PgConnection,
    hlc: &Hlc,
    hub_school_id: Uuid,
    reason: &str,
) -> Result<(), sqlx::Error> {
    end_links(conn, hlc, hub_school_id, None, reason).await?;
    revoke_codes(conn, hlc, hub_school_id, None).await?;
    Ok(())
}

/// Guardian id, the account's phone number, when it was linked, the last sign-in.
type LinkedRow = (Uuid, String, DateTime<Utc>, Option<DateTime<Utc>>);

/// Which guardians of a school have access, or a code waiting to be used.
pub async fn access_list(
    conn: &mut PgConnection,
    hub_school_id: Uuid,
    now: DateTime<Utc>,
) -> Result<Vec<GuardianAccess>, sqlx::Error> {
    let linked: Vec<LinkedRow> = sqlx::query_as(
        "SELECT l.guardian_id, p.phone, l.linked_at, p.last_login_at
         FROM hub_parent_links l JOIN hub_parents p ON p.id = l.parent_id
         WHERE l.hub_school_id = $1 AND l.ended_at IS NULL ORDER BY l.linked_at, l.id",
    )
    .bind(hub_school_id)
    .fetch_all(&mut *conn)
    .await?;
    let waiting: Vec<(Uuid, DateTime<Utc>)> = sqlx::query_as(
        "SELECT guardian_id, max(expires_at) FROM hub_parent_access_codes
         WHERE hub_school_id = $1 AND used_at IS NULL AND revoked_at IS NULL AND expires_at > $2
         GROUP BY guardian_id ORDER BY 2",
    )
    .bind(hub_school_id)
    .bind(now)
    .fetch_all(conn)
    .await?;
    let mut out: Vec<GuardianAccess> = linked
        .into_iter()
        .map(
            |(guardian_id, phone, since, last_login_at)| GuardianAccess {
                guardian_id,
                state: "active".into(),
                phone_hint: Some(mask_phone(&phone)),
                since: Some(since),
                last_login_at,
                code_expires_at: None,
            },
        )
        .collect();
    for (guardian_id, expires_at) in waiting {
        match out.iter_mut().find(|a| a.guardian_id == guardian_id) {
            // A new code for someone who already has access (a forgotten password).
            Some(existing) => existing.code_expires_at = Some(expires_at),
            None => out.push(GuardianAccess {
                guardian_id,
                state: "code".into(),
                phone_hint: None,
                since: None,
                last_login_at: None,
                code_expires_at: Some(expires_at),
            }),
        }
    }
    Ok(out)
}
