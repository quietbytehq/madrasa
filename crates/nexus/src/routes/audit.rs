use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;

use crate::auth::CurrentHubUser;
use crate::permissions::HubPermission;
use crate::store::{self, AuditEntry};
use crate::{HubResult, HubState};

#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub page_size: Option<i64>,
}

pub fn router() -> Router<HubState> {
    Router::new().route("/audit", get(list))
}

/// The hub's own audit log, newest first (`nexus.admin`).
async fn list(
    State(state): State<HubState>,
    current: CurrentHubUser,
    q: Result<Query<AuditQuery>, axum::extract::rejection::QueryRejection>,
) -> HubResult<Json<Vec<AuditEntry>>> {
    current.require(HubPermission::Admin)?;
    let Query(q) = q?;
    let size = q.page_size.unwrap_or(50).clamp(1, 200);
    let page = q.page.unwrap_or(1).max(1);
    let mut conn = state.pool.acquire().await?;
    Ok(Json(
        store::audit_recent(&mut conn, size, (page - 1) * size).await?,
    ))
}
