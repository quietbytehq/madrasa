//! Settings → Nexus: join a hub, see the sync state, sync now, leave (feature `nexus_link`,
//! permission `settings.manage`). Leaving the backup key with head office, or taking it
//! back, also needs `backup.manage`.

use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use madrasa_core::edition::Feature;
use madrasa_core::permissions::Permission;
use madrasa_core::sync::Overwrite;
use madrasa_db::sync;
use serde::Deserialize;

use crate::auth::CurrentUser;
use crate::backup::Secret;
use crate::nexus_link::{self, EscrowView, LeaveOutcome, LinkStatus};
use crate::{ApiResult, AppState};

type JsonBody<T> = Result<Json<T>, axum::extract::rejection::JsonRejection>;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/nexus/link", get(status))
        .route("/nexus/link/join", post(join))
        .route("/nexus/link/sync", post(sync_now))
        .route("/nexus/link/leave", post(leave))
        .route("/nexus/link/overwrites", get(overwrites))
        .route(
            "/nexus/link/escrow",
            get(escrow_status)
                .post(escrow_deposit)
                .delete(escrow_withdraw),
        )
}

fn gate(state: &AppState, user: &CurrentUser) -> ApiResult<()> {
    state.require_feature(Feature::NexusLink)?;
    user.require(Permission::SettingsManage)
}

async fn status(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<LinkStatus>> {
    gate(&state, &user)?;
    Ok(Json(nexus_link::status(&state).await?))
}

#[derive(Debug, Deserialize)]
pub struct JoinBody {
    pub hub_url: String,
    pub join_code: String,
}

async fn join(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<JoinBody>,
) -> ApiResult<Json<LinkStatus>> {
    gate(&state, &user)?;
    let Json(b) = body?;
    Ok(Json(
        nexus_link::join(&state, user.actor(), &b.hub_url, &b.join_code).await?,
    ))
}

async fn sync_now(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<LinkStatus>> {
    gate(&state, &user)?;
    Ok(Json(nexus_link::sync_now(&state, user.actor()).await?))
}

#[derive(Debug, Default, Deserialize)]
pub struct LeaveBody {
    #[serde(default)]
    pub reason: String,
}

async fn leave(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<LeaveBody>,
) -> ApiResult<Json<LeaveOutcome>> {
    gate(&state, &user)?;
    let Json(b) = body?;
    Ok(Json(
        nexus_link::leave(&state, user.actor(), &b.reason).await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct PageQuery {
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub page_size: Option<i64>,
}

/// The overwrite log: every value sync replaced or refused, newest first.
async fn overwrites(
    State(state): State<AppState>,
    user: CurrentUser,
    q: Result<Query<PageQuery>, axum::extract::rejection::QueryRejection>,
) -> ApiResult<Json<Vec<Overwrite>>> {
    gate(&state, &user)?;
    let Query(q) = q?;
    let size = q.page_size.unwrap_or(50).clamp(1, 200);
    let page = q.page.unwrap_or(1).max(1);
    let mut conn = state.pool.acquire().await?;
    Ok(Json(
        sync::list_overwrites(&mut conn, size, (page - 1) * size).await?,
    ))
}

// ---- backup key escrow --------------------------------------------------------------------

fn escrow_gate(state: &AppState, user: &CurrentUser) -> ApiResult<()> {
    gate(state, user)?;
    user.require(Permission::BackupManage)
}

/// What head office holds for this school, and every release of its key.
async fn escrow_status(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<EscrowView>> {
    gate(&state, &user)?;
    Ok(Json(nexus_link::escrow_status(&state).await?))
}

/// Leave the backup key with head office: needs the backup password or the recovery key.
async fn escrow_deposit(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<Secret>,
) -> ApiResult<Json<EscrowView>> {
    escrow_gate(&state, &user)?;
    let Json(mut secret) = body?;
    // Only the school's own secrets prove it may hand its key over.
    secret.released_key = None;
    Ok(Json(
        nexus_link::escrow_deposit(&state, user.actor(), &secret).await?,
    ))
}

async fn escrow_withdraw(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<EscrowView>> {
    escrow_gate(&state, &user)?;
    Ok(Json(
        nexus_link::escrow_withdraw(&state, user.actor()).await?,
    ))
}
