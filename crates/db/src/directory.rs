//! Directory login settings (Standard stage 8): the configuration JSON and the bind
//! password sealed with the instance key.

use madrasa_core::directory::DirectoryConfig;
use madrasa_core::hlc::Hlc;
use madrasa_core::secrets;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{settings, Actor, DbError, DbResult};

pub const KEY_CONFIG: &str = "directory.config";

pub async fn config(conn: &mut PgConnection, school_id: Uuid) -> DbResult<DirectoryConfig> {
    Ok(settings::get(conn, Some(school_id), KEY_CONFIG)
        .await?
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default())
}

pub async fn set_config(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    cfg: &DirectoryConfig,
) -> DbResult<()> {
    cfg.validate().map_err(DbError::Conflict)?;
    settings::set(
        conn,
        hlc,
        actor,
        Some(school_id),
        KEY_CONFIG,
        serde_json::to_value(cfg).unwrap_or(Value::Null),
    )
    .await?;
    Ok(())
}

/// Whether a bind password is stored (the value itself is never returned to the UI).
pub async fn has_bind_password(conn: &mut PgConnection, school_id: Uuid) -> DbResult<bool> {
    Ok(
        settings::get(conn, Some(school_id), settings::KEY_DIRECTORY_BIND_PASSWORD)
            .await?
            .and_then(|v| v.as_str().map(|s| !s.is_empty()))
            .unwrap_or(false),
    )
}

/// The bind password, unsealed with the instance key.
pub async fn bind_password(
    conn: &mut PgConnection,
    hlc: &Hlc,
    school_id: Uuid,
) -> DbResult<Option<String>> {
    let Some(sealed) = settings::get(
        &mut *conn,
        Some(school_id),
        settings::KEY_DIRECTORY_BIND_PASSWORD,
    )
    .await?
    .and_then(|v| v.as_str().map(str::to_owned))
    .filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let key = settings::instance_key(conn, hlc).await?;
    let bytes = secrets::open(&key, &sealed).map_err(|_| {
        DbError::Conflict("the stored bind password cannot be read; enter it again".into())
    })?;
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

pub async fn set_bind_password(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    password: &str,
) -> DbResult<()> {
    let value = if password.is_empty() {
        Value::String(String::new())
    } else {
        let key = settings::instance_key(&mut *conn, hlc).await?;
        Value::String(
            secrets::seal(&key, password.as_bytes())
                .map_err(|e| DbError::Conflict(e.to_string()))?,
        )
    };
    settings::set(
        conn,
        hlc,
        actor,
        Some(school_id),
        settings::KEY_DIRECTORY_BIND_PASSWORD,
        value,
    )
    .await?;
    Ok(())
}
