use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use madrasa_core::permissions::Permission;
use madrasa_db::{roles, users};

use crate::auth::CurrentUser;
use crate::routes::UserSummary;
use crate::{ApiResult, AppState};

pub fn router() -> Router<AppState> {
    Router::new().route("/users", get(list))
}

/// Admin-only: list users of the caller's school.
async fn list(
    State(state): State<AppState>,
    current: CurrentUser,
) -> ApiResult<Json<Vec<UserSummary>>> {
    current.require(Permission::UsersManage)?;
    let mut conn = state.pool.acquire().await?;
    let rows = users::list(&mut conn, current.user.school_id).await?;
    let mut out = Vec::with_capacity(rows.len());
    for user in &rows {
        let role_codes = roles::codes_for_user(&mut conn, user.id).await?;
        out.push(UserSummary::from_user(user, role_codes));
    }
    Ok(Json(out))
}
