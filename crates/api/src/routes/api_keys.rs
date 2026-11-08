//! Settings → API (Standard stage 9): keys for the read-only API. Needs `settings.manage`
//! and the `readonly_api` feature. A key is returned once, at creation.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use madrasa_core::api_keys::Scope;
use madrasa_core::edition::Feature;
use madrasa_core::permissions::Permission;
use madrasa_db::api_keys as repo;
use madrasa_db::api_keys::{ApiKey, ApiKeyCall, ApiKeyInput};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::{ApiError, ApiResult, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/settings/api-keys", get(list).post(create))
        .route("/settings/api-keys/{id}", axum::routing::delete(revoke))
        .route("/settings/api-keys/{id}/calls", get(calls))
}

fn gate(state: &AppState, user: &CurrentUser) -> ApiResult<()> {
    state.require_feature(Feature::ReadonlyApi)?;
    user.require(Permission::SettingsManage)
}

/// Mirrored by `ApiKeysView` in the UI.
#[derive(Debug, Serialize)]
pub struct ApiKeysView {
    pub keys: Vec<ApiKey>,
    pub scopes: Vec<&'static str>,
    pub rate_limit_per_minute: u32,
    /// Where the read-only API lives, relative to the server's address.
    pub base_path: &'static str,
}

async fn list(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<ApiKeysView>> {
    gate(&state, &user)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(ApiKeysView {
        keys: repo::list(&mut c, user.user.school_id).await?,
        scopes: Scope::ALL.iter().map(|s| s.as_str()).collect(),
        rate_limit_per_minute: madrasa_core::api_keys::RATE_LIMIT_PER_MINUTE,
        base_path: crate::ro::RO_PREFIX,
    }))
}

/// Mirrored by `CreatedApiKey`: the key itself appears here and nowhere else.
#[derive(Debug, Serialize)]
pub struct CreatedApiKey {
    pub key: String,
    #[serde(flatten)]
    pub row: ApiKey,
}

async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<ApiKeyInput>,
) -> ApiResult<(StatusCode, Json<CreatedApiKey>)> {
    gate(&state, &user)?;
    let mut tx = state.pool.begin().await?;
    let (key, row) = repo::create(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        &body,
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(CreatedApiKey { key, row })))
}

async fn owned(c: &mut sqlx::PgConnection, user: &CurrentUser, id: Uuid) -> ApiResult<ApiKey> {
    let k = repo::get(c, id).await?;
    if k.school_id != user.user.school_id {
        return Err(ApiError::not_found("API key"));
    }
    Ok(k)
}

async fn revoke(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ApiKey>> {
    gate(&state, &user)?;
    let mut tx = state.pool.begin().await?;
    owned(&mut tx, &user, id).await?;
    let k = repo::revoke(&mut tx, &state.hlc, user.actor(), id).await?;
    tx.commit().await?;
    state.ro_limits.forget(id);
    Ok(Json(k))
}

#[derive(Debug, Deserialize)]
pub struct CallsQuery {
    pub limit: Option<i64>,
}

async fn calls(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Query(q): Query<CallsQuery>,
) -> ApiResult<Json<Vec<ApiKeyCall>>> {
    gate(&state, &user)?;
    let mut c = state.pool.acquire().await?;
    owned(&mut c, &user, id).await?;
    Ok(Json(
        repo::calls(&mut c, id, q.limit.unwrap_or(100).clamp(1, 1000)).await?,
    ))
}
