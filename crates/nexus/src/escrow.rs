//! Backup key escrow at the hub (ADR 0005 §6): the hub's key pair, the sealed keys schools
//! deposit, and the record of every release. The cryptography is in `madrasa_core::escrow`.
//!
//! Nothing here can read a school's key: opening a deposit needs the escrow passphrase,
//! which is given with each release and never stored or logged.

use chrono::{DateTime, Utc};
use madrasa_core::escrow::{Deposit, DepositInfo, EscrowKeys, EscrowStatus, ReleaseInfo};
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::store;

/// Hub setting holding the escrow key pair (private key wrapped by the passphrase).
pub const KEY_ESCROW: &str = "escrow.keys";

/// The hub's escrow key pair, when escrow has been set up.
pub async fn keys(conn: &mut PgConnection) -> Result<Option<EscrowKeys>, sqlx::Error> {
    Ok(store::setting(conn, KEY_ESCROW)
        .await?
        .and_then(|v| serde_json::from_value(v).ok()))
}

pub async fn save_keys(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Option<Uuid>,
    keys: &EscrowKeys,
) -> Result<(), sqlx::Error> {
    let value = serde_json::to_value(keys).expect("escrow keys serialize");
    store::set_setting(conn, hlc, actor, KEY_ESCROW, value).await
}

/// A school's live deposit.
#[derive(Debug, Clone)]
pub struct LiveDeposit {
    pub id: Uuid,
    pub key_id: String,
    pub sealed: Deposit,
    pub deposited_at: DateTime<Utc>,
}

pub async fn live_deposit(
    conn: &mut PgConnection,
    hub_school_id: Uuid,
) -> Result<Option<LiveDeposit>, sqlx::Error> {
    let row: Option<(Uuid, String, serde_json::Value, DateTime<Utc>)> = sqlx::query_as(
        "SELECT id, key_id, sealed, deposited_at FROM hub_escrow_deposits
         WHERE hub_school_id = $1 AND deleted_at IS NULL",
    )
    .bind(hub_school_id)
    .fetch_optional(conn)
    .await?;
    Ok(row.and_then(|(id, key_id, sealed, deposited_at)| {
        Some(LiveDeposit {
            id,
            key_id,
            sealed: serde_json::from_value(sealed).ok()?,
            deposited_at,
        })
    }))
}

/// End a school's live deposit and erase the sealed key. `reason` is one of `replaced`,
/// `withdrawn`, `left`, `removed`, `purged`. Returns whether there was one.
pub async fn end_deposit(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Option<Uuid>,
    hub_school_id: Uuid,
    reason: &str,
) -> Result<bool, sqlx::Error> {
    let done = sqlx::query(
        "UPDATE hub_escrow_deposits SET sealed = NULL, deleted_at = now(), ended_reason = $2,
             updated_at = now(), updated_by = $3, hlc = $4
         WHERE hub_school_id = $1 AND deleted_at IS NULL",
    )
    .bind(hub_school_id)
    .bind(reason)
    .bind(actor)
    .bind(hlc.now().to_string())
    .execute(conn)
    .await?;
    Ok(done.rows_affected() > 0)
}

/// Store a school's deposit, replacing an earlier one.
pub async fn store_deposit(
    conn: &mut PgConnection,
    hlc: &Hlc,
    hub_school_id: Uuid,
    deposit: &Deposit,
) -> Result<DepositInfo, sqlx::Error> {
    end_deposit(conn, hlc, None, hub_school_id, "replaced").await?;
    let deposited_at: DateTime<Utc> = sqlx::query_scalar(
        "INSERT INTO hub_escrow_deposits (id, hub_school_id, key_id, sealed, hlc)
         VALUES ($1, $2, $3, $4, $5) RETURNING deposited_at",
    )
    .bind(new_id())
    .bind(hub_school_id)
    .bind(&deposit.key_id)
    .bind(serde_json::to_value(deposit).expect("a deposit serializes"))
    .bind(hlc.now().to_string())
    .fetch_one(conn)
    .await?;
    Ok(DepositInfo {
        key_id: deposit.key_id.clone(),
        deposited_at,
    })
}

/// Every release of one school's key, newest first.
pub async fn releases_for(
    conn: &mut PgConnection,
    hub_school_id: Uuid,
) -> Result<Vec<ReleaseInfo>, sqlx::Error> {
    let rows: Vec<(Uuid, DateTime<Utc>, String, String, String, bool)> = sqlx::query_as(
        "SELECT id, released_at, released_by_name, reason, key_id, seen_at IS NOT NULL
         FROM hub_escrow_releases WHERE hub_school_id = $1 AND deleted_at IS NULL
         ORDER BY released_at DESC, id DESC",
    )
    .bind(hub_school_id)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(id, released_at, released_by, reason, key_id, seen)| ReleaseInfo {
                id,
                released_at,
                released_by,
                reason,
                key_id,
                seen,
            },
        )
        .collect())
}

/// What a school is told about escrow.
pub async fn status_for(
    conn: &mut PgConnection,
    hub_school_id: Uuid,
) -> Result<EscrowStatus, sqlx::Error> {
    Ok(EscrowStatus {
        public_key: keys(conn).await?.map(|k| k.public_key),
        deposit: live_deposit(conn, hub_school_id)
            .await?
            .map(|d| DepositInfo {
                key_id: d.key_id,
                deposited_at: d.deposited_at,
            }),
        releases: releases_for(conn, hub_school_id).await?,
    })
}

/// Record a release. The key itself is never stored.
#[allow(clippy::too_many_arguments)]
pub async fn record_release(
    conn: &mut PgConnection,
    hlc: &Hlc,
    hub_school_id: Uuid,
    deposit: &LiveDeposit,
    released_by: Uuid,
    released_by_name: &str,
    reason: &str,
) -> Result<(Uuid, DateTime<Utc>), sqlx::Error> {
    sqlx::query_as(
        "INSERT INTO hub_escrow_releases
             (id, hub_school_id, deposit_id, key_id, released_by, released_by_name, reason, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $5, $8) RETURNING id, released_at",
    )
    .bind(new_id())
    .bind(hub_school_id)
    .bind(deposit.id)
    .bind(&deposit.key_id)
    .bind(released_by)
    .bind(released_by_name)
    .bind(reason)
    .bind(hlc.now().to_string())
    .fetch_one(conn)
    .await
}

/// The school confirmed it recorded these releases. Only its own releases are touched.
pub async fn mark_seen(
    conn: &mut PgConnection,
    hlc: &Hlc,
    hub_school_id: Uuid,
    ids: &[Uuid],
) -> Result<u64, sqlx::Error> {
    if ids.is_empty() {
        return Ok(0);
    }
    let done = sqlx::query(
        "UPDATE hub_escrow_releases SET seen_at = now(), updated_at = now(), hlc = $3
         WHERE hub_school_id = $1 AND id = ANY($2) AND seen_at IS NULL",
    )
    .bind(hub_school_id)
    .bind(ids)
    .bind(hlc.now().to_string())
    .execute(conn)
    .await?;
    Ok(done.rows_affected())
}

/// Wrong passphrases by one user in the last quarter of an hour.
pub async fn recent_failures(conn: &mut PgConnection, user: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT count(*) FROM hub_audit_log
         WHERE actor_user_id = $1 AND action = 'escrow_passphrase_failed'
           AND at > now() - interval '15 minutes'",
    )
    .bind(user)
    .fetch_one(conn)
    .await
}
