//! Backup and restore endpoints. Everything needs `backup.manage` except the status used by
//! the dashboard, which any logged-in user of the school may read.

use axum::extract::{Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use madrasa_core::permissions::Permission;
use madrasa_db::models::BackupRun;
use serde::Deserialize;

use crate::auth::CurrentUser;
use crate::backup::{
    self, BackupFile, BackupSettings, BackupStatus, RestorePreview, RestoreResult, Secret,
};
use crate::{ApiResult, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/backup/status", get(status))
        .route("/backup/settings", get(get_settings).put(put_settings))
        .route("/backup/run", post(run))
        .route("/backup/runs", get(runs))
        .route("/backup/files", get(files))
        .route("/backup/preview", post(preview))
        .route("/backup/restore", post(restore))
        .route("/backup/password", post(change_password))
        .route("/backup/unlock", post(unlock))
}

type JsonBody<T> = Result<Json<T>, axum::extract::rejection::JsonRejection>;

async fn status(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<BackupStatus>> {
    let mut c = state.pool.acquire().await?;
    Ok(Json(backup::status(&mut c, user.user.school_id).await?))
}

async fn get_settings(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<BackupSettings>> {
    user.require(Permission::BackupManage)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        backup::load_settings(&mut c, user.user.school_id).await?,
    ))
}

async fn put_settings(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<BackupSettings>,
) -> ApiResult<Json<BackupSettings>> {
    user.require(Permission::BackupManage)?;
    let Json(s) = body?;
    Ok(Json(
        backup::save_settings(&state, user.actor(), user.user.school_id, &s).await?,
    ))
}

async fn run(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<BackupRun>> {
    user.require(Permission::BackupManage)?;
    Ok(Json(
        backup::run_backup(&state, user.actor(), user.user.school_id, "manual").await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct RunsQuery {
    pub limit: Option<i64>,
}

async fn runs(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<RunsQuery>,
) -> ApiResult<Json<Vec<BackupRun>>> {
    user.require(Permission::BackupManage)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        madrasa_db::backup::recent_runs(
            &mut c,
            user.user.school_id,
            q.limit.unwrap_or(20).clamp(1, 200),
        )
        .await?,
    ))
}

async fn files(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<BackupFile>>> {
    user.require(Permission::BackupManage)?;
    let mut c = state.pool.acquire().await?;
    let s = backup::load_settings(&mut c, user.user.school_id).await?;
    Ok(Json(backup::files_in(&s)))
}

#[derive(Debug, Deserialize)]
pub struct PathRequest {
    pub path: String,
}

async fn preview(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<PathRequest>,
) -> ApiResult<Json<RestorePreview>> {
    user.require(Permission::BackupManage)?;
    let Json(req) = body?;
    Ok(Json(backup::preview(&state, &req.path).await?))
}

#[derive(Debug, Deserialize)]
pub struct RestoreRequest {
    pub path: String,
    pub password: Option<String>,
    pub recovery_key: Option<String>,
}

async fn restore(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<RestoreRequest>,
) -> ApiResult<Json<RestoreResult>> {
    user.require(Permission::BackupManage)?;
    let Json(req) = body?;
    let secret = Secret {
        password: req.password,
        recovery_key: req.recovery_key,
    };
    Ok(Json(
        backup::restore(&state, user.actor(), &req.path, &secret).await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct PasswordChangeRequest {
    pub current_password: Option<String>,
    pub recovery_key: Option<String>,
    pub new_password: String,
}

async fn change_password(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<PasswordChangeRequest>,
) -> ApiResult<axum::http::StatusCode> {
    user.require(Permission::BackupManage)?;
    let Json(req) = body?;
    let secret = Secret {
        password: req.current_password,
        recovery_key: req.recovery_key,
    };
    backup::change_password(
        &state,
        user.actor(),
        user.user.school_id,
        &secret,
        &req.new_password,
    )
    .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct UnlockRequest {
    pub password: Option<String>,
    pub recovery_key: Option<String>,
}

async fn unlock(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<UnlockRequest>,
) -> ApiResult<axum::http::StatusCode> {
    user.require(Permission::BackupManage)?;
    let Json(req) = body?;
    let secret = Secret {
        password: req.password,
        recovery_key: req.recovery_key,
    };
    backup::unlock(&state, user.actor(), user.user.school_id, &secret).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
