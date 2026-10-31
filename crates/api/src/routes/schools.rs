use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use madrasa_core::locale;
use madrasa_core::permissions::Permission;
use madrasa_db::models::School;
use madrasa_db::schools;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::{ApiError, ApiResult, AppState};

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
    /// Symbol for the currency, e.g. `Rs`, `$`.
    pub currency_symbol: String,
    /// ISO 3166-1 alpha-2.
    pub country: String,
    pub country_name: String,
    /// IANA time zone name.
    pub time_zone: String,
    /// What the national identity number is called here (`CNIC / B-Form`, `National ID`…).
    pub id_label: String,
    /// Numeric dates are read day-first or month-first.
    pub date_order: locale::DateOrder,
}

impl From<School> for SchoolSummary {
    fn from(s: School) -> Self {
        let country = locale::country_or_default(&s.country);
        let symbol = locale::currency(&s.currency)
            .map(|c| c.symbol.to_string())
            .unwrap_or_else(|| s.currency.clone());
        Self {
            id: s.id,
            name: s.name,
            code: s.code,
            city: s.city,
            is_demo: s.is_demo,
            academic_year_start_month: s.academic_year_start_month,
            currency: s.currency,
            currency_symbol: symbol,
            country: s.country,
            country_name: country.name.into(),
            time_zone: s.time_zone,
            id_label: country.id_label.into(),
            date_order: country.date_order,
        }
    }
}

/// Mirrored by `SchoolProfileUpdate` in the UI. Omitted fields are left unchanged.
#[derive(Debug, Deserialize, Default)]
pub struct ProfileBody {
    pub name: Option<String>,
    pub city: Option<String>,
    pub currency: Option<String>,
    pub country: Option<String>,
    pub time_zone: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/schools/current", get(current).patch(update_current))
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

/// Change the school's name, city, currency, country or time zone (Settings → School).
async fn update_current(
    State(state): State<AppState>,
    user: CurrentUser,
    body: Result<Json<ProfileBody>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<Json<SchoolSummary>> {
    user.require(Permission::SettingsManage)?;
    let Json(b) = body?;
    if b.name.as_deref().is_some_and(|n| n.trim().is_empty()) {
        return Err(ApiError::validation("School name is required."));
    }
    if let Some(c) = b.country.as_deref() {
        if locale::country(c).is_none() {
            return Err(ApiError::validation("Choose a country from the list."));
        }
    }
    if let Some(c) = b.currency.as_deref() {
        if locale::currency(c).is_none() {
            return Err(ApiError::validation("Choose a currency from the list."));
        }
    }
    if let Some(z) = b.time_zone.as_deref() {
        if locale::time_zone(z).is_none() {
            return Err(ApiError::validation(
                "Time zone must be an IANA name such as Asia/Karachi or America/New_York.",
            ));
        }
    }
    let mut conn = state.pool.acquire().await?;
    let school = schools::update_profile(
        &mut conn,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        &schools::SchoolProfileUpdate {
            name: b.name,
            city: b.city,
            currency: b.currency,
            country: b.country,
            time_zone: b.time_zone,
        },
    )
    .await?;
    drop(conn);
    if let Err(e) = state.reload_profile().await {
        tracing::warn!(error = %e, "school profile not reloaded after update");
    }
    Ok(Json(school.into()))
}
