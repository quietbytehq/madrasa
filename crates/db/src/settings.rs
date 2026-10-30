//! Key/value settings. `school_id = None` is instance-wide (edition, node id, backup keys).

use crate::audit::{self, snapshot, Action};
use crate::models::Setting;
use crate::{Actor, DbResult};
use madrasa_core::edition::Edition;
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

pub const KEY_EDITION: &str = "edition";
pub const KEY_NODE_ID: &str = "node_id";
pub const KEY_SETUP_COMPLETED_AT: &str = "setup_completed_at";
pub const KEY_BACKUP_KEYS: &str = "backup.keys";
pub const KEY_ACADEMIC_YEAR_START_MONTH: &str = "academic_year.start_month";
pub const KEY_CURRENCY: &str = "currency";

/// Keys whose values must never appear in the audit log in clear.
const REDACTED_KEYS: &[&str] = &[KEY_BACKUP_KEYS];

pub async fn get(
    conn: &mut PgConnection,
    school_id: Option<Uuid>,
    key: &str,
) -> DbResult<Option<Value>> {
    Ok(sqlx::query_scalar::<_, Value>(
        "SELECT value FROM settings WHERE school_id IS NOT DISTINCT FROM $1 AND key = $2 AND deleted_at IS NULL",
    )
    .bind(school_id)
    .bind(key)
    .fetch_optional(conn)
    .await?)
}

/// Upsert a setting and audit the change (secret keys are logged as `"<redacted>"`).
pub async fn set(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Option<Uuid>,
    key: &str,
    value: Value,
) -> DbResult<Setting> {
    let old = get(&mut *conn, school_id, key).await?;
    let setting = sqlx::query_as::<_, Setting>(
        "INSERT INTO settings (id, school_id, key, value, updated_by, hlc) VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (school_id, key) DO UPDATE
           SET value = EXCLUDED.value, updated_at = now(), updated_by = EXCLUDED.updated_by,
               hlc = EXCLUDED.hlc, deleted_at = NULL
         RETURNING id, school_id, key, value, updated_at, updated_by, hlc",
    )
    .bind(new_id())
    .bind(school_id)
    .bind(key)
    .bind(&value)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    let redact = |v: Option<Value>| {
        v.map(|v| {
            if REDACTED_KEYS.contains(&key) {
                Value::String("<redacted>".into())
            } else {
                v
            }
        })
    };
    let (action, old_json) = match old {
        Some(old) => (Action::Update, redact(Some(old))),
        None => (Action::Create, None),
    };
    let new_json = redact(Some(snapshot(&setting.value)));
    audit::record(
        conn,
        hlc,
        actor,
        action,
        "setting",
        Some(setting.id),
        old_json.map(|v| serde_json::json!({ "key": key, "value": v })),
        new_json.map(|v| serde_json::json!({ "key": key, "value": v })),
    )
    .await?;
    Ok(setting)
}

/// Instance edition; defaults to Lite when unset or unparseable.
pub async fn edition(conn: &mut PgConnection) -> DbResult<Edition> {
    let value = get(conn, None, KEY_EDITION).await?;
    Ok(value
        .and_then(|v| v.as_str().map(str::to_owned))
        .and_then(|s| s.parse().ok())
        .unwrap_or(Edition::DEFAULT))
}

pub async fn set_edition(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    edition: Edition,
) -> DbResult<()> {
    set(
        conn,
        hlc,
        actor,
        None,
        KEY_EDITION,
        Value::String(edition.as_str().into()),
    )
    .await?;
    Ok(())
}

/// Stable per-install node id for the hybrid logical clock; created on first call.
pub async fn node_id(conn: &mut PgConnection) -> DbResult<u64> {
    if let Some(v) = get(&mut *conn, None, KEY_NODE_ID).await? {
        if let Some(n) = v.as_u64() {
            return Ok(n);
        }
    }
    let node = rand_node();
    let hlc = Hlc::new(node);
    set(
        conn,
        &hlc,
        Actor::SYSTEM,
        None,
        KEY_NODE_ID,
        Value::from(node),
    )
    .await?;
    Ok(node)
}

fn rand_node() -> u64 {
    // 48 bits so it fits the HLC node field.
    (uuid::Uuid::new_v4().as_u128() as u64) & 0xffff_ffff_ffff
}

/// Has first-run setup completed? True once a non-demo or demo school exists and the marker is set.
pub async fn setup_completed(conn: &mut PgConnection) -> DbResult<bool> {
    Ok(get(conn, None, KEY_SETUP_COMPLETED_AT).await?.is_some())
}
