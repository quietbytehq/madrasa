use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use madrasa_core::password::verify_password;
use madrasa_core::session_token;
use madrasa_db::{roles, schools, sessions, users};
use serde::{Deserialize, Serialize};

use crate::auth::CurrentUser;
use crate::routes::UserSummary;
use crate::{ApiError, ApiResult, AppState};

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub user: UserSummary,
}

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub user: UserSummary,
    pub permissions: Vec<&'static str>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
}

/// Dummy hash used to keep timing similar when the username does not exist.
const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$Q0M2Qz6Uq2kZ0t4uY3O0+3o2wFzqJ3Yw1x0o0Zy5x2E";

async fn login(
    State(state): State<AppState>,
    body: Result<Json<LoginRequest>, axum::extract::rejection::JsonRejection>,
) -> ApiResult<Json<LoginResponse>> {
    let Json(req) = body?;
    let mut conn = state.pool.acquire().await?;
    // Lite: one school. Standard+ will take the school from the request/host.
    let school = schools::current(&mut conn)
        .await?
        .ok_or_else(ApiError::invalid_credentials)?;
    let user = users::find_by_username(&mut conn, school.id, &req.username).await?;

    let Some(user) = user else {
        let _ = verify_password(&req.password, DUMMY_HASH);
        return Err(ApiError::invalid_credentials());
    };
    let now = Utc::now();
    if state.lockout.is_locked(user.locked_until, now) {
        return Err(ApiError::account_locked());
    }
    if !user.is_active {
        return Err(ApiError::account_inactive());
    }
    if !verify_password(&req.password, &user.password_hash) {
        let next = state
            .lockout
            .after_failure(user.failed_login_count.max(0) as u32, now);
        users::record_failed_login(
            &mut conn,
            &state.hlc,
            user.id,
            next.failed_count as i32,
            next.locked_until,
        )
        .await?;
        return Err(if next.locked_until.is_some() {
            ApiError::account_locked()
        } else {
            ApiError::invalid_credentials()
        });
    }

    users::record_successful_login(&mut conn, &state.hlc, user.id).await?;
    let token = session_token::generate();
    let expires_at = now + session_token::default_lifetime();
    sessions::create(&mut conn, &state.hlc, &user, &token.hash, expires_at).await?;
    let role_codes = roles::codes_for_user(&mut conn, user.id).await?;
    Ok(Json(LoginResponse {
        token: token.token,
        expires_at,
        user: UserSummary::from_user(&user, role_codes),
    }))
}

async fn logout(
    State(state): State<AppState>,
    current: CurrentUser,
) -> ApiResult<axum::http::StatusCode> {
    let mut conn = state.pool.acquire().await?;
    sessions::revoke(&mut conn, &state.hlc, &current.session).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

async fn me(current: CurrentUser) -> Json<MeResponse> {
    Json(MeResponse {
        user: UserSummary::from_user(&current.user, current.roles.clone()),
        permissions: current.permissions.codes(),
    })
}
