//! Backup key escrow: setting it up (`nexus.admin`), seeing which schools have left a key
//! and every release (`nexus.admin` or `nexus.escrow`), releasing a key (`nexus.escrow`,
//! the escrow passphrase and a reason).
//!
//! The passphrase and the released key pass through these handlers and go nowhere else:
//! not into a log line, an audit row or an error message.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use madrasa_core::escrow::{self as crypto, EscrowError, EscrowKeys};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{required, JsonBody};
use crate::auth::CurrentHubUser;
use crate::permissions::HubPermission;
use crate::{escrow, store, tenants, HubError, HubResult, HubState};

/// Wrong passphrases one user may offer in a quarter of an hour.
const MAX_FAILURES: i64 = 5;

pub fn router() -> Router<HubState> {
    Router::new()
        .route("/escrow", get(overview))
        .route("/escrow/setup", post(setup))
        .route("/escrow/passphrase", post(change_passphrase))
        .route("/schools/{id}/escrow/release", post(release))
}

fn not_set_up() -> HubError {
    HubError::new(
        StatusCode::CONFLICT,
        "escrow_not_set_up",
        "Key escrow has not been set up on this hub yet.",
    )
}

fn wrong_passphrase() -> HubError {
    HubError::new(
        StatusCode::UNAUTHORIZED,
        "escrow_passphrase_invalid",
        "The escrow passphrase is incorrect.",
    )
}

/// Create the key pair on a blocking thread (Argon2id is slow on purpose).
pub(crate) async fn new_keys(passphrase: String) -> HubResult<EscrowKeys> {
    if let Some(problem) = crypto::passphrase_problem(&passphrase) {
        return Err(HubError::validation(problem));
    }
    tokio::task::spawn_blocking(move || crypto::generate(&passphrase))
        .await
        .map_err(|_| HubError::internal())?
        .map_err(|_| HubError::internal())
}

async fn note_failure(state: &HubState, user: Uuid, what: &str, school: Option<Uuid>) {
    if let Ok(mut conn) = state.pool.acquire().await {
        let _ = store::audit(
            &mut conn,
            &state.hlc,
            Some(user),
            "escrow_passphrase_failed",
            "hub_escrow",
            school,
            None,
            Some(serde_json::json!({ "operation": what })),
        )
        .await;
    }
}

async fn throttle(state: &HubState, user: Uuid) -> HubResult<()> {
    let mut conn = state.pool.acquire().await?;
    if escrow::recent_failures(&mut conn, user).await? >= MAX_FAILURES {
        return Err(HubError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "escrow_locked",
            "Too many wrong escrow passphrases. Wait fifteen minutes and try again.",
        ));
    }
    Ok(())
}

// ---- overview -------------------------------------------------------------------------------

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DepositRow {
    pub school_id: Uuid,
    pub school_name: String,
    pub school_code: String,
    pub key_id: String,
    pub deposited_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ReleaseRow {
    pub id: Uuid,
    pub school_id: Uuid,
    pub school_name: String,
    pub school_code: String,
    pub released_by: String,
    pub reason: String,
    pub released_at: DateTime<Utc>,
    /// When the school recorded the release in its own audit log.
    pub seen_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct Overview {
    /// The hub has an escrow key pair; schools can leave their keys.
    pub set_up: bool,
    pub deposits: Vec<DepositRow>,
    pub releases: Vec<ReleaseRow>,
}

async fn overview(
    State(state): State<HubState>,
    current: CurrentHubUser,
) -> HubResult<Json<Overview>> {
    if !current.can(HubPermission::Admin) {
        current.require(HubPermission::Escrow)?;
    }
    let mut conn = state.pool.acquire().await?;
    let set_up = escrow::keys(&mut conn).await?.is_some();
    let deposits = sqlx::query_as::<_, DepositRow>(
        "SELECT s.id AS school_id, s.name AS school_name, s.code AS school_code, d.key_id, d.deposited_at
         FROM hub_escrow_deposits d JOIN hub_schools s ON s.id = d.hub_school_id
         WHERE d.deleted_at IS NULL ORDER BY s.name, s.id",
    )
    .fetch_all(&mut *conn)
    .await?;
    let releases = sqlx::query_as::<_, ReleaseRow>(
        "SELECT r.id, s.id AS school_id, s.name AS school_name, s.code AS school_code,
                r.released_by_name AS released_by, r.reason, r.released_at, r.seen_at
         FROM hub_escrow_releases r JOIN hub_schools s ON s.id = r.hub_school_id
         WHERE r.deleted_at IS NULL ORDER BY r.released_at DESC, r.id DESC LIMIT 200",
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(Json(Overview {
        set_up,
        deposits,
        releases,
    }))
}

// ---- setting up -----------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SetupBody {
    pub passphrase: String,
}

/// Create the hub's escrow key pair. Done once; the passphrase is not stored, so losing it
/// means no deposited key can ever be released.
async fn setup(
    State(state): State<HubState>,
    current: CurrentHubUser,
    body: JsonBody<SetupBody>,
) -> HubResult<StatusCode> {
    current.require(HubPermission::Admin)?;
    let Json(b) = body?;
    let already = || HubError::conflict("Key escrow is already set up on this hub.");
    {
        let mut conn = state.pool.acquire().await?;
        if escrow::keys(&mut conn).await?.is_some() {
            return Err(already());
        }
    }
    let keys = new_keys(b.passphrase).await?;
    let mut tx = state.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('madrasa-nexus-escrow'))")
        .execute(&mut *tx)
        .await?;
    if escrow::keys(&mut tx).await?.is_some() {
        return Err(already());
    }
    escrow::save_keys(&mut tx, &state.hlc, Some(current.user.id), &keys).await?;
    store::audit(
        &mut tx,
        &state.hlc,
        Some(current.user.id),
        "escrow_setup",
        "hub_escrow",
        None,
        None,
        None,
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct PassphraseBody {
    pub current: String,
    pub new: String,
}

/// Change the escrow passphrase. The key pair stays, so deposits keep working.
async fn change_passphrase(
    State(state): State<HubState>,
    current: CurrentHubUser,
    body: JsonBody<PassphraseBody>,
) -> HubResult<StatusCode> {
    current.require(HubPermission::Escrow)?;
    let Json(b) = body?;
    if let Some(problem) = crypto::passphrase_problem(&b.new) {
        return Err(HubError::validation(problem));
    }
    throttle(&state, current.user.id).await?;
    let keys = {
        let mut conn = state.pool.acquire().await?;
        escrow::keys(&mut conn).await?.ok_or_else(not_set_up)?
    };
    let changed =
        tokio::task::spawn_blocking(move || crypto::change_passphrase(&keys, &b.current, &b.new))
            .await
            .map_err(|_| HubError::internal())?;
    let keys = match changed {
        Ok(k) => k,
        Err(EscrowError::Passphrase) => {
            note_failure(&state, current.user.id, "change_passphrase", None).await;
            return Err(wrong_passphrase());
        }
        Err(_) => return Err(HubError::internal()),
    };
    let mut tx = state.pool.begin().await?;
    escrow::save_keys(&mut tx, &state.hlc, Some(current.user.id), &keys).await?;
    store::audit(
        &mut tx,
        &state.hlc,
        Some(current.user.id),
        "escrow_passphrase_changed",
        "hub_escrow",
        None,
        None,
        None,
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- releasing ------------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ReleaseBody {
    pub passphrase: String,
    pub reason: String,
}

#[derive(Serialize)]
pub struct Released {
    pub release_id: Uuid,
    pub school_name: String,
    pub school_code: String,
    pub released_at: DateTime<Utc>,
    /// The school's backup key as a printable code. Shown once; the hub does not keep it.
    pub released_key: String,
}

/// Release a school's backup key. Needs the permission, the passphrase and a reason; the
/// release is written to the hub's audit log and the school is told at its next sync.
async fn release(
    State(state): State<HubState>,
    current: CurrentHubUser,
    Path(id): Path<Uuid>,
    body: JsonBody<ReleaseBody>,
) -> HubResult<Json<Released>> {
    current.require(HubPermission::Escrow)?;
    let Json(b) = body?;
    let reason = required(&b.reason, "A reason")?;
    throttle(&state, current.user.id).await?;
    let (school, keys, deposit) = {
        let mut conn = state.pool.acquire().await?;
        let school = tenants::get(&mut conn, id)
            .await?
            .ok_or_else(|| HubError::not_found("School"))?;
        // A school that is no longer joined could not be told about the release.
        if school.status != "active" {
            return Err(HubError::conflict(
                "Only the key of a school that is joined can be released.",
            ));
        }
        let keys = escrow::keys(&mut conn).await?.ok_or_else(not_set_up)?;
        let deposit = escrow::live_deposit(&mut conn, id).await?.ok_or_else(|| {
            HubError::new(
                StatusCode::CONFLICT,
                "no_deposit",
                "This school has not left its backup key with head office.",
            )
        })?;
        (school, keys, deposit)
    };
    let context = crypto::context_for(school.school_id);
    let sealed = deposit.sealed.clone();
    let passphrase = b.passphrase;
    let opened =
        tokio::task::spawn_blocking(move || crypto::open(&keys, &passphrase, &sealed, &context))
            .await
            .map_err(|_| HubError::internal())?;
    let mut dek = match opened {
        Ok(dek) => dek,
        Err(EscrowError::Passphrase) => {
            note_failure(&state, current.user.id, "release", Some(id)).await;
            return Err(wrong_passphrase());
        }
        Err(_) => {
            return Err(HubError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "deposit_damaged",
                "The stored key is damaged and cannot be released. Ask the school to leave its key again.",
            ));
        }
    };
    let mut tx = state.pool.begin().await?;
    let (release_id, released_at) = escrow::record_release(
        &mut tx,
        &state.hlc,
        id,
        &deposit,
        current.user.id,
        &current.user.full_name,
        &reason,
    )
    .await?;
    store::audit(
        &mut tx,
        &state.hlc,
        Some(current.user.id),
        "escrow_release",
        "hub_school",
        Some(id),
        None,
        Some(serde_json::json!({
            "release_id": release_id, "reason": reason, "key_id": deposit.key_id,
            "school": school.name,
        })),
    )
    .await?;
    tx.commit().await?;
    let released_key = crypto::format_released_key(&dek);
    dek.fill(0);
    tracing::info!(school = %school.code, "a school's backup key was released");
    Ok(Json(Released {
        release_id,
        school_name: school.name,
        school_code: school.code,
        released_at,
        released_key,
    }))
}
