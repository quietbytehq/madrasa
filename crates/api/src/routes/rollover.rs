//! Academic year rollover (`academics.manage`): preview what will happen, then apply.

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use madrasa_core::permissions::Permission;
use madrasa_db::rollover::{self, RolloverOptions, RolloverPreview, RolloverReport};

use crate::auth::CurrentUser;
use crate::{ApiResult, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/academic-years/rollover/preview", get(preview))
        .route("/academic-years/rollover", post(apply))
}

type JsonBody<T> = Result<Json<T>, axum::extract::rejection::JsonRejection>;

async fn preview(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<RolloverPreview>> {
    user.require(Permission::AcademicsManage)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(rollover::preview(&mut c, user.user.school_id).await?))
}

async fn apply(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<RolloverOptions>,
) -> ApiResult<Json<RolloverReport>> {
    user.require(Permission::AcademicsManage)?;
    let Json(options) = body?;
    let mut tx = state.pool.begin().await?;
    let report = rollover::apply(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        &options,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(report))
}
