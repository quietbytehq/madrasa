//! School-level settings editable by administrators.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use madrasa_core::permissions::Permission;
use madrasa_core::students::normalise_prefix;
use madrasa_db::{settings, students};
use serde::{Deserialize, Serialize};

use crate::auth::CurrentUser;
use crate::{ApiError, ApiResult, AppState};

#[derive(Debug, Serialize, Deserialize)]
pub struct SchoolSettings {
    pub student_number_prefix: String,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/settings/school", get(get_settings).put(put_settings))
}

async fn get_settings(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<SchoolSettings>> {
    user.require(Permission::SettingsManage)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(SchoolSettings {
        student_number_prefix: students::number_prefix(&mut c, user.user.school_id).await?,
    }))
}

async fn put_settings(
    State(state): State<AppState>,
    user: CurrentUser,
    body: Result<Json<SchoolSettings>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<Json<SchoolSettings>> {
    user.require(Permission::SettingsManage)?;
    let Json(b) = body?;
    let prefix = normalise_prefix(&b.student_number_prefix);
    if prefix.is_empty() || prefix.len() > 12 {
        return Err(ApiError::validation(
            "Student number prefix must be 1–12 letters, digits or hyphens.",
        ));
    }
    let mut c = state.pool.acquire().await?;
    settings::set(
        &mut c,
        &state.hlc,
        user.actor(),
        Some(user.user.school_id),
        students::KEY_STUDENT_NUMBER_PREFIX,
        serde_json::json!(prefix),
    )
    .await?;
    Ok(Json(SchoolSettings {
        student_number_prefix: prefix,
    }))
}
