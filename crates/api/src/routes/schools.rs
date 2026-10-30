use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use madrasa_core::permissions::Permission;
use madrasa_db::models::School;
use madrasa_db::schools;
use serde::Serialize;
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::{ApiResult, AppState};

/// Mirrored by `SchoolSummary` in the UI.
#[derive(Debug, Serialize)]
pub struct SchoolSummary {
    pub id: Uuid,
    pub name: String,
    pub code: String,
    pub city: String,
    pub is_demo: bool,
    pub academic_year_start_month: i16,
    pub currency: String,
}

impl From<School> for SchoolSummary {
    fn from(s: School) -> Self {
        Self {
            id: s.id,
            name: s.name,
            code: s.code,
            city: s.city,
            is_demo: s.is_demo,
            academic_year_start_month: s.academic_year_start_month,
            currency: s.currency,
        }
    }
}

pub fn router() -> Router<AppState> {
    Router::new().route("/schools/current", get(current))
}

async fn current(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<SchoolSummary>> {
    user.require(Permission::SchoolRead)?;
    let mut conn = state.pool.acquire().await?;
    let school = schools::get(&mut conn, user.user.school_id).await?;
    Ok(Json(school.into()))
}
