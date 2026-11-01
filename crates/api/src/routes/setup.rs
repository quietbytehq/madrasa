//! First-run setup. Open only while no setup has completed.

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use madrasa_core::backup;
use madrasa_core::locale;
use madrasa_core::password::{hash_password, validate_password};
use madrasa_core::permissions::RoleCode;
use madrasa_db::{academics, attendance, roles, schools, settings, users, Actor};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::{ApiError, ApiResult, AppState};

#[derive(Debug, Serialize)]
pub struct SetupStatus {
    pub needs_setup: bool,
}

/// Mirrored by `SetupRequest` in the UI.
#[derive(Debug, Deserialize)]
pub struct SetupRequest {
    pub school_name: String,
    pub school_code: String,
    #[serde(default)]
    pub city: String,
    pub academic_year_start_month: i16,
    /// ISO 3166-1 alpha-2; decides the default currency, time zone, weekend and ID format.
    #[serde(default = "default_country")]
    pub country: String,
    /// ISO 4217; the country's currency when omitted.
    #[serde(default)]
    pub currency: Option<String>,
    /// IANA name; the country's main zone when omitted.
    #[serde(default)]
    pub time_zone: Option<String>,
    /// Create and activate the current academic year (the wizard always asks for this).
    #[serde(default)]
    pub first_year: bool,
    /// Also create the country's usual classes (KG–12 or K–12) with a section A each.
    #[serde(default)]
    pub default_classes: bool,
    pub admin_full_name: String,
    pub admin_username: String,
    pub admin_password: String,
    pub backup_password: String,
}

impl SetupRequest {
    fn country_info(&self) -> &'static locale::CountryInfo {
        locale::country_or_default(&self.country)
    }
    /// The chosen currency, or the country's.
    pub fn currency(&self) -> String {
        self.currency
            .as_deref()
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .map(str::to_uppercase)
            .unwrap_or_else(|| self.country_info().currency.into())
    }
    /// The chosen time zone, or the country's main one.
    pub fn time_zone(&self) -> String {
        self.time_zone
            .as_deref()
            .map(str::trim)
            .filter(|z| !z.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| self.country_info().time_zone.into())
    }
}

/// The calendar year the current academic year started in: this year if its start month has
/// passed (or is now), else last year. Setting up in October with an April start gives the
/// year that began in April; in February it gives the previous April.
pub fn first_year_start(today: chrono::NaiveDate, start_month: i16) -> i32 {
    use chrono::Datelike;
    if (today.month() as i16) >= start_month {
        today.year()
    } else {
        today.year() - 1
    }
}

/// The class preset for a country: K–12 in North America, KG–12 elsewhere.
pub fn default_class_names(country: &str) -> Vec<&'static str> {
    let first = if matches!(country.trim().to_ascii_uppercase().as_str(), "US" | "CA") {
        "K"
    } else {
        "KG"
    };
    std::iter::once(first)
        .chain([
            "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12",
        ])
        .collect()
}

fn default_country() -> String {
    locale::DEFAULT_COUNTRY.into()
}

#[derive(Debug, Serialize)]
pub struct SetupResponse {
    pub school_id: Uuid,
    pub admin_user_id: Uuid,
    /// Shown exactly once; never stored.
    pub recovery_key: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/setup/status", get(status))
        .route("/setup", post(run_setup))
}

async fn status(State(state): State<AppState>) -> ApiResult<Json<SetupStatus>> {
    let mut conn = state.pool.acquire().await?;
    let done = settings::setup_completed(&mut conn).await?;
    Ok(Json(SetupStatus { needs_setup: !done }))
}

pub fn validate(req: &SetupRequest) -> Result<(), ApiError> {
    let code = req.school_code.trim();
    if req.school_name.trim().is_empty() {
        return Err(ApiError::validation("School name is required."));
    }
    let code_ok = code.len() >= 2
        && code.len() <= 12
        && code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && !code.starts_with('-')
        && !code.ends_with('-');
    if !code_ok {
        return Err(ApiError::validation(
            "School code must be 2–12 letters, digits or hyphens.",
        ));
    }
    if !(1..=12).contains(&req.academic_year_start_month) {
        return Err(ApiError::validation(
            "Academic year start month must be between 1 and 12.",
        ));
    }
    let country = locale::country(&req.country)
        .ok_or_else(|| ApiError::validation("Choose a country from the list."))?;
    if let Some(c) = req
        .currency
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
    {
        if locale::currency(c).is_none() {
            return Err(ApiError::validation("Choose a currency from the list."));
        }
    }
    if let Some(z) = req
        .time_zone
        .as_deref()
        .map(str::trim)
        .filter(|z| !z.is_empty())
    {
        if locale::time_zone(z).is_none() {
            return Err(ApiError::validation(
                "Time zone must be an IANA name such as Asia/Karachi or America/New_York.",
            ));
        }
    }
    let _ = country;
    if req.admin_full_name.trim().is_empty() {
        return Err(ApiError::validation("Administrator name is required."));
    }
    let username = req.admin_username.trim();
    if username.len() < 3
        || !username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
    {
        return Err(ApiError::validation(
            "Username must be at least 3 characters: letters, digits, dots or underscores.",
        ));
    }
    validate_password(&req.admin_password)
        .map_err(|e| ApiError::validation(format!("Administrator password: {e}.")))?;
    validate_password(&req.backup_password)
        .map_err(|e| ApiError::validation(format!("Backup password: {e}.")))?;
    Ok(())
}

async fn run_setup(
    State(state): State<AppState>,
    body: Result<Json<SetupRequest>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<Json<SetupResponse>> {
    let Json(req) = body?;
    validate(&req)?;

    // Expensive hashing happens outside the transaction.
    let password_hash = hash_password(&req.admin_password)?;
    let backup = backup::setup(&req.backup_password).map_err(|_| ApiError::internal())?;

    let mut tx = state.pool.begin().await?;
    if settings::setup_completed(&mut tx).await? {
        return Err(ApiError::new(
            axum::http::StatusCode::CONFLICT,
            "setup_already_done",
            "Setup has already been completed. Log in instead.",
        ));
    }
    let edition = state.edition();
    let school = schools::create(
        &mut tx,
        &state.hlc,
        Actor::SYSTEM,
        schools::NewSchool {
            name: req.school_name.clone(),
            code: req.school_code.clone(),
            city: req.city.clone(),
            is_demo: false,
            academic_year_start_month: req.academic_year_start_month,
            currency: req.currency(),
            country: req.country.trim().to_uppercase(),
            time_zone: req.time_zone(),
        },
    )
    .await?;
    let actor = Actor::system_for(school.id);
    // The country's usual school weekend; editable later under Attendance → Calendar.
    attendance::set_weekend_days(
        &mut tx,
        &state.hlc,
        actor,
        school.id,
        &locale::default_weekend(&req.country),
    )
    .await?;
    let created_roles =
        roles::create_defaults(&mut tx, &state.hlc, actor, school.id, edition).await?;
    let admin = users::create(
        &mut tx,
        &state.hlc,
        actor,
        users::NewUser {
            school_id: school.id,
            username: req.admin_username.clone(),
            full_name: req.admin_full_name.clone(),
            email: None,
            password_hash,
        },
    )
    .await?;
    for role in created_roles
        .iter()
        .filter(|r| r.code == RoleCode::Principal.code() || r.code == RoleCode::Admin.code())
    {
        roles::assign(&mut tx, &state.hlc, actor, admin.id, role).await?;
    }

    let school_actor = Actor::user(admin.id, school.id);
    settings::set_edition(&mut tx, &state.hlc, Actor::SYSTEM, edition).await?;
    settings::set(
        &mut tx,
        &state.hlc,
        school_actor,
        Some(school.id),
        settings::KEY_ACADEMIC_YEAR_START_MONTH,
        json!(req.academic_year_start_month),
    )
    .await?;
    settings::set(
        &mut tx,
        &state.hlc,
        school_actor,
        Some(school.id),
        settings::KEY_CURRENCY,
        json!("PKR"),
    )
    .await?;
    settings::set(
        &mut tx,
        &state.hlc,
        school_actor,
        Some(school.id),
        settings::KEY_BACKUP_KEYS,
        serde_json::to_value(&backup.key_set).map_err(|_| ApiError::internal())?,
    )
    .await?;
    if req.first_year {
        let today = crate::clock::today();
        let start_year = first_year_start(today, req.academic_year_start_month);
        let (name, start, end) =
            academics::default_year_bounds(start_year, req.academic_year_start_month as u32)
                .ok_or_else(|| ApiError::validation("Academic year start month is invalid."))?;
        let year = academics::create_year(
            &mut tx,
            &state.hlc,
            school_actor,
            academics::NewAcademicYear {
                school_id: school.id,
                name,
                start_date: start,
                end_date: end,
                start_month: req.academic_year_start_month,
            },
        )
        .await?;
        academics::activate_year(&mut tx, &state.hlc, school_actor, year.id).await?;
        if req.default_classes {
            for (level, class_name) in default_class_names(&req.country).iter().enumerate() {
                let class = academics::create_class(
                    &mut tx,
                    &state.hlc,
                    school_actor,
                    school.id,
                    class_name,
                    level as i32,
                )
                .await?;
                academics::create_section(
                    &mut tx,
                    &state.hlc,
                    school_actor,
                    academics::NewSection {
                        school_id: school.id,
                        class_id: class.id,
                        academic_year_id: year.id,
                        name: "A".into(),
                        class_teacher_id: None,
                        capacity: None,
                    },
                )
                .await?;
            }
        }
    }
    settings::set(
        &mut tx,
        &state.hlc,
        school_actor,
        None,
        settings::KEY_SETUP_COMPLETED_AT,
        json!(Utc::now()),
    )
    .await?;
    tx.commit().await?;
    if let Err(e) = state.reload_profile().await {
        tracing::warn!(error = %e, "school profile not reloaded after setup");
    }

    tracing::info!(school = %school.code, "first-run setup completed");
    Ok(Json(SetupResponse {
        school_id: school.id,
        admin_user_id: admin.id,
        recovery_key: backup.recovery_key,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_year_starts_this_year_once_the_start_month_has_passed() {
        let d = |y, m, day| chrono::NaiveDate::from_ymd_opt(y, m, day).unwrap();
        assert_eq!(first_year_start(d(2026, 10, 31), 4), 2026);
        assert_eq!(first_year_start(d(2026, 4, 1), 4), 2026);
        assert_eq!(first_year_start(d(2026, 2, 10), 4), 2025);
        assert_eq!(
            first_year_start(d(2026, 8, 20), 9),
            2025,
            "US school in August"
        );
        assert_eq!(first_year_start(d(2026, 9, 1), 9), 2026);
        assert_eq!(default_class_names("US")[0], "K");
        assert_eq!(default_class_names("pk")[0], "KG");
        assert_eq!(default_class_names("GB").len(), 13);
    }
}
