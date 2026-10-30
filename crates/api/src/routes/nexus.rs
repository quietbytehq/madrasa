//! Nexus hub endpoints. Stub in M0; exists so feature gating is exercised end to end.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use madrasa_core::edition::Feature;
use madrasa_core::permissions::Permission;
use serde::Serialize;

use crate::auth::CurrentUser;
use crate::{ApiResult, AppState};

#[derive(Debug, Serialize)]
pub struct NexusStatus {
    pub joined_schools: u32,
    pub status: &'static str,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/nexus/status", get(status))
}

async fn status(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<NexusStatus>> {
    state.require_feature(Feature::NexusHub)?;
    user.require(Permission::NexusAdmin)?;
    Ok(Json(NexusStatus {
        joined_schools: 0,
        status: "not_configured",
    }))
}
