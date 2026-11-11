//! A school's link to its Madrasa Nexus hub: where the hub is and the sync token. The link
//! is an ordinary, audited setting that changes only on joining and leaving; the token is
//! sealed with the instance key (ADR 0004) and never reaches the audit log or a screen.
//! When the school last synced is local bookkeeping in `sync_state`, not a setting.

use chrono::{DateTime, Utc};
use madrasa_core::hlc::Hlc;
use madrasa_core::secrets;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{settings, Actor, DbError, DbResult};

/// How a membership ended.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ended {
    pub at: DateTime<Utc>,
    /// `school` (the school left) or `head_office` (the hub removed it).
    pub by: String,
    pub reason: String,
}

/// Everything about the link except the token.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub hub_url: String,
    pub hub_name: String,
    /// The hub's id for this membership.
    pub school_ref: Uuid,
    pub joined_at: DateTime<Utc>,
    /// Set when the school is no longer joined; kept so the screen can say what happened.
    #[serde(default)]
    pub ended: Option<Ended>,
}

impl Link {
    /// Joined and syncing.
    pub fn is_active(&self) -> bool {
        self.ended.is_none()
    }
}

pub async fn get(conn: &mut PgConnection, school_id: Uuid) -> DbResult<Option<Link>> {
    Ok(
        settings::get(conn, Some(school_id), settings::KEY_NEXUS_LINK)
            .await?
            .and_then(|v| serde_json::from_value(v).ok()),
    )
}

pub async fn set(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    link: &Link,
) -> DbResult<()> {
    let value = serde_json::to_value(link).map_err(|e| DbError::Conflict(e.to_string()))?;
    settings::set(
        conn,
        hlc,
        actor,
        Some(school_id),
        settings::KEY_NEXUS_LINK,
        value,
    )
    .await?;
    Ok(())
}

/// The sync token, unsealed. `None` when the school is not joined.
pub async fn token(
    conn: &mut PgConnection,
    hlc: &Hlc,
    school_id: Uuid,
) -> DbResult<Option<String>> {
    let Some(sealed) = settings::get(&mut *conn, Some(school_id), settings::KEY_NEXUS_TOKEN)
        .await?
        .and_then(|v| v.as_str().map(str::to_owned))
        .filter(|s| !s.is_empty())
    else {
        return Ok(None);
    };
    let key = settings::instance_key(conn, hlc).await?;
    let bytes = secrets::open(&key, &sealed).map_err(|_| {
        DbError::Conflict("the stored key for the hub cannot be read; leave and join again".into())
    })?;
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

/// Store the sync token sealed, or forget it (`None`).
pub async fn set_token(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    token: Option<&str>,
) -> DbResult<()> {
    let value = match token {
        Some(t) => {
            let key = settings::instance_key(&mut *conn, hlc).await?;
            Value::String(
                secrets::seal(&key, t.as_bytes()).map_err(|e| DbError::Conflict(e.to_string()))?,
            )
        }
        None => Value::String(String::new()),
    };
    settings::set(
        conn,
        hlc,
        actor,
        Some(school_id),
        settings::KEY_NEXUS_TOKEN,
        value,
    )
    .await?;
    Ok(())
}
