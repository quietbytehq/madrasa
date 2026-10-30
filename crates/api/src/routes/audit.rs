use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use madrasa_core::permissions::Permission;
use madrasa_db::audit;
use madrasa_db::models::AuditEntry;
use serde::Deserialize;

use crate::auth::CurrentUser;
use crate::{ApiResult, AppState};

#[derive(Debug, Deserialize)]
pub struct AuditQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    50
}

pub fn router() -> Router<AppState> {
    Router::new().route("/audit", get(recent))
}

async fn recent(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<AuditQuery>,
) -> ApiResult<Json<Vec<AuditEntry>>> {
    user.require(Permission::AuditRead)?;
    let mut conn = state.pool.acquire().await?;
    let rows =
        audit::list_recent(&mut conn, Some(user.user.school_id), q.limit.clamp(1, 500)).await?;
    Ok(Json(rows))
}
