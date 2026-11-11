//! What a school calls: join, push, pull, acknowledge, leave. The school always calls; the
//! hub never does. After joining, every call carries the school's sync token.

use axum::extract::{DefaultBodyLimit, FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use madrasa_core::escrow::{DepositRequest, EscrowStatus, SeenRequest};
use madrasa_core::session_token;
use madrasa_core::sync::{
    check_versions, AckRequest, AckResponse, JoinRequest, JoinResponse, LeaveRequest, PullRequest,
    PullResponse, PushRequest, PushResponse, BATCH_SIZE,
};
use madrasa_db::sync;
use serde::Serialize;

use super::schools::{internal, join_code_hash};
use super::JsonBody;
use crate::auth::bearer_token;
use crate::tenants::{self, Ending, HubSchool, NewTenant};
use crate::{escrow, store, HubError, HubResult, HubState};

/// A first upload sends whole records by the hundred; the default 2 MB body limit is too
/// small for a batch of long audit rows.
const MAX_SYNC_BODY: usize = 64 * 1024 * 1024;

pub fn router() -> Router<HubState> {
    Router::new()
        .route("/join", post(join))
        .route("/sync/status", get(status))
        .route("/sync/push", post(push))
        .route("/sync/pull", post(pull))
        .route("/sync/ack", post(ack))
        .route("/sync/leave", post(leave))
        .route(
            "/sync/escrow",
            get(escrow_status)
                .put(escrow_deposit)
                .delete(escrow_withdraw),
        )
        .route("/sync/escrow/seen", post(escrow_seen))
        .layer(DefaultBodyLimit::max(MAX_SYNC_BODY))
}

/// The school a sync token belongs to. Extracting it fails with `401` for an unknown token
/// and `410 school_removed` when head office removed the school.
pub struct SyncSchool(pub HubSchool);

impl FromRequestParts<HubState> for SyncSchool {
    type Rejection = HubError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &HubState,
    ) -> Result<Self, Self::Rejection> {
        let token = bearer_token(parts).ok_or_else(HubError::unauthenticated)?;
        let mut conn = state.pool.acquire().await?;
        let school = tenants::find_by_token(&mut conn, &session_token::hash(token))
            .await?
            .ok_or_else(|| {
                HubError::new(
                    StatusCode::UNAUTHORIZED,
                    "unknown_school",
                    "This hub does not recognise the school's key. The school may have left; join again with a new invitation.",
                )
            })?;
        match school.status.as_str() {
            "active" => Ok(SyncSchool(school)),
            "removed" => Err(HubError::new(
                StatusCode::GONE,
                "school_removed",
                match school.leave_reason.as_deref().filter(|r| !r.is_empty()) {
                    Some(reason) => {
                        format!("Head office removed this school from the network: {reason}")
                    }
                    None => "Head office removed this school from the network.".to_string(),
                },
            )),
            _ => Err(HubError::unauthenticated()),
        }
    }
}

fn version_refusal(school_schema: i64) -> HubResult<()> {
    check_versions(tenants::hub_schema_version(), school_schema)
        .map_err(|e| HubError::new(StatusCode::CONFLICT, "hub_too_old", e.to_string()))
}

async fn hub_name(state: &HubState) -> HubResult<String> {
    let mut conn = state.pool.acquire().await?;
    Ok(store::setting(&mut conn, store::KEY_HUB_NAME)
        .await?
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default())
}

/// A school joins with a one-time join code and its own school record.
async fn join(
    State(state): State<HubState>,
    body: JsonBody<JoinRequest>,
) -> HubResult<Json<JoinResponse>> {
    let Json(req) = body?;
    let refused = || {
        HubError::new(
            StatusCode::FORBIDDEN,
            "bad_join_code",
            "This join code is not valid. It may have been used already, expired or been revoked. Ask head office for a new invitation.",
        )
    };
    let hash = join_code_hash(&req.join_code).ok_or_else(refused)?;
    let invitation: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM hub_invitations
         WHERE code_hash = $1 AND used_at IS NULL AND revoked_at IS NULL AND expires_at > now()
           AND deleted_at IS NULL",
    )
    .bind(&hash)
    .fetch_optional(&state.pool)
    .await?;
    let invitation_id = invitation.ok_or_else(refused)?;
    version_refusal(req.schema_version)?;
    if req.school.table != "schools" || req.school.fields.is_some() {
        return Err(HubError::validation(
            "A school joins with its school record.",
        ));
    }
    let text = |key: &str| {
        req.school
            .row
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string()
    };
    let (name, code) = (text("name"), text("code"));
    if name.is_empty() || code.is_empty() {
        return Err(HubError::validation(
            "The school record has no name or code.",
        ));
    }
    let token = session_token::generate();
    let school = tenants::create(
        &state.pool,
        &state.hlc,
        NewTenant {
            invitation_id,
            school: &req.school,
            name,
            code,
            school_version: req.app_version.clone(),
            school_schema: req.schema_version,
            edition: req.edition.clone(),
            token_hash: token.hash,
        },
    )
    .await
    .map_err(internal)?
    .ok_or_else(refused)?;
    tracing::info!(school = %school.code, "a school joined the network");
    Ok(Json(JoinResponse {
        school_ref: school.id,
        token: token.token,
        hub_name: hub_name(&state).await?,
        hub_version: state.version.to_string(),
        hub_schema_version: tenants::hub_schema_version(),
    }))
}

#[derive(Debug, Serialize)]
pub struct SyncStatusResponse {
    pub status: String,
    pub hub_name: String,
    pub hub_version: String,
    pub hub_schema_version: i64,
    pub school_name: String,
}

async fn status(
    State(state): State<HubState>,
    SyncSchool(school): SyncSchool,
) -> HubResult<Json<SyncStatusResponse>> {
    Ok(Json(SyncStatusResponse {
        status: school.status,
        hub_name: hub_name(&state).await?,
        hub_version: state.version.to_string(),
        hub_schema_version: tenants::hub_schema_version(),
        school_name: school.name,
    }))
}

/// The school sends its changes; the hub merges them into the school's schema.
async fn push(
    State(state): State<HubState>,
    SyncSchool(school): SyncSchool,
    body: JsonBody<PushRequest>,
) -> HubResult<Json<PushResponse>> {
    let Json(req) = body?;
    version_refusal(req.schema_version)?;
    let mut tx = tenants::begin(&state.pool, &school).await?;
    let report = sync::apply(&mut tx, &state.hlc, &req.changes).await?;
    let stored = sync::store_remote_overwrites(&mut tx, &req.overwrites).await?;
    sync::mark_overwrites_reported(&mut tx, &req.stored_overwrites).await?;
    // Every hub log entry the school has not confirmed, not only this batch's: a lost
    // response must not leave the school without them.
    let overwrites = sync::unreported_overwrites(&mut tx, 500).await?;
    tenants::touch(
        &mut tx,
        &state.hlc,
        school.id,
        Some(&req.app_version),
        Some(req.schema_version),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(PushResponse {
        handled: report.handled,
        applied: report.applied,
        parked: report.parked,
        overwrites,
        stored_overwrites: stored,
        max_hlc: report.max_hlc,
    }))
}

/// The school asks for head office's changes.
async fn pull(
    State(state): State<HubState>,
    SyncSchool(school): SyncSchool,
    body: JsonBody<PullRequest>,
) -> HubResult<Json<PullResponse>> {
    let Json(req) = body?;
    let limit = req.limit.clamp(1, BATCH_SIZE);
    let mut tx = tenants::begin(&state.pool, &school).await?;
    let changes = sync::collect(&mut tx, limit as i64).await?;
    tx.commit().await?;
    let more = changes.len() == limit;
    Ok(Json(PullResponse { changes, more }))
}

/// The school has dealt with a pulled batch: the hub forgets those changes.
async fn ack(
    State(state): State<HubState>,
    SyncSchool(school): SyncSchool,
    body: JsonBody<AckRequest>,
) -> HubResult<Json<AckResponse>> {
    let Json(req) = body?;
    let mut tx = tenants::begin(&state.pool, &school).await?;
    sync::acknowledge(&mut tx, &req.handled).await?;
    let stored = sync::store_remote_overwrites(&mut tx, &req.overwrites).await?;
    sync::mark_overwrites_reported(&mut tx, &req.stored_overwrites).await?;
    tenants::touch(&mut tx, &state.hlc, school.id, None, None).await?;
    tx.commit().await?;
    Ok(Json(AckResponse {
        stored_overwrites: stored,
    }))
}

/// The school leaves. Its token stops working; its data stays as an archive until purged.
async fn leave(
    State(state): State<HubState>,
    SyncSchool(school): SyncSchool,
    body: JsonBody<LeaveRequest>,
) -> HubResult<Json<()>> {
    let Json(req) = body?;
    let reason = if req.reason.trim().is_empty() {
        "The school left."
    } else {
        req.reason.trim()
    };
    tenants::end(&state.pool, &state.hlc, None, &school, Ending::Left, reason)
        .await
        .map_err(internal)?;
    tracing::info!(school = %school.code, "a school left the network");
    Ok(Json(()))
}

// ---- backup key escrow (the school's side) -----------------------------------------------------

/// What the school is told: the hub's escrow public key, its own deposit, and every release
/// of its key.
async fn escrow_status(
    State(state): State<HubState>,
    SyncSchool(school): SyncSchool,
) -> HubResult<Json<EscrowStatus>> {
    let mut conn = state.pool.acquire().await?;
    Ok(Json(escrow::status_for(&mut conn, school.id).await?))
}

/// The school leaves its backup key, sealed to the hub's escrow public key. The hub cannot
/// open it without the escrow passphrase, so it checks only the shape.
async fn escrow_deposit(
    State(state): State<HubState>,
    SyncSchool(school): SyncSchool,
    body: JsonBody<DepositRequest>,
) -> HubResult<Json<EscrowStatus>> {
    let Json(req) = body?;
    let d = &req.deposit;
    let sized = |text: &str, max: usize| !text.is_empty() && text.len() <= max;
    if d.version != 1
        || !sized(&d.ephemeral_public, 64)
        || !sized(&d.nonce, 32)
        || !sized(&d.ciphertext, 128)
        || !sized(&d.key_id, 64)
    {
        return Err(HubError::validation("This is not a sealed backup key."));
    }
    let mut tx = state.pool.begin().await?;
    if escrow::keys(&mut tx).await?.is_none() {
        return Err(HubError::new(
            StatusCode::CONFLICT,
            "escrow_not_set_up",
            "Head office has not set up key escrow yet.",
        ));
    }
    escrow::store_deposit(&mut tx, &state.hlc, school.id, d).await?;
    store::audit(
        &mut tx,
        &state.hlc,
        None,
        "escrow_deposit",
        "hub_school",
        Some(school.id),
        None,
        Some(serde_json::json!({ "key_id": d.key_id })),
    )
    .await?;
    let status = escrow::status_for(&mut tx, school.id).await?;
    tx.commit().await?;
    Ok(Json(status))
}

/// The school takes its key back: the sealed key is erased.
async fn escrow_withdraw(
    State(state): State<HubState>,
    SyncSchool(school): SyncSchool,
) -> HubResult<Json<EscrowStatus>> {
    let mut tx = state.pool.begin().await?;
    if escrow::end_deposit(&mut tx, &state.hlc, None, school.id, "withdrawn").await? {
        store::audit(
            &mut tx,
            &state.hlc,
            None,
            "escrow_withdraw",
            "hub_school",
            Some(school.id),
            None,
            None,
        )
        .await?;
    }
    let status = escrow::status_for(&mut tx, school.id).await?;
    tx.commit().await?;
    Ok(Json(status))
}

/// The school has written these releases into its own audit log.
async fn escrow_seen(
    State(state): State<HubState>,
    SyncSchool(school): SyncSchool,
    body: JsonBody<SeenRequest>,
) -> HubResult<Json<EscrowStatus>> {
    let Json(req) = body?;
    let mut conn = state.pool.acquire().await?;
    escrow::mark_seen(&mut conn, &state.hlc, school.id, &req.ids).await?;
    Ok(Json(escrow::status_for(&mut conn, school.id).await?))
}
