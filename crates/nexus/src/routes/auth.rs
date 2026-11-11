use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use madrasa_core::password::verify_password;
use madrasa_core::session_token;
use serde::{Deserialize, Serialize};

use super::JsonBody;
use crate::auth::CurrentHubUser;
use crate::store::{self, HubUser};
use crate::{HubError, HubResult, HubState};

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    /// Shown to the client once; the hub stores only its hash.
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub user: HubUser,
}

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub user: HubUser,
    pub hub_name: Option<String>,
}

pub fn router() -> Router<HubState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
}

/// Dummy hash used to keep timing similar when the username does not exist.
const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$Q0M2Qz6Uq2kZ0t4uY3O0+3o2wFzqJ3Yw1x0o0Zy5x2E";

async fn login(
    State(state): State<HubState>,
    body: JsonBody<LoginRequest>,
) -> HubResult<Json<LoginResponse>> {
    let Json(req) = body?;
    let now = Utc::now();
    let mut conn = state.pool.acquire().await?;
    let Some(user) = store::find_user(&mut conn, &req.username).await? else {
        let _ = verify_password(&req.password, DUMMY_HASH);
        return Err(HubError::invalid_credentials());
    };
    if state.lockout.is_locked(user.locked_until, now) {
        return Err(HubError::account_locked());
    }
    if !verify_password(&req.password, &user.password_hash) {
        let next = state
            .lockout
            .after_failure(user.failed_login_count.max(0) as u32, now);
        store::record_failed_login(
            &mut conn,
            &state.hlc,
            user.id,
            next.failed_count as i32,
            next.locked_until,
        )
        .await?;
        if next.locked_until.is_some() {
            tracing::warn!(user = %user.username, "hub account locked after failed logins");
            return Err(HubError::account_locked());
        }
        return Err(HubError::invalid_credentials());
    }
    if !user.is_active {
        return Err(HubError::account_inactive());
    }
    let token = session_token::generate();
    let expires_at = now + session_token::default_lifetime();
    store::record_login(&mut conn, &state.hlc, user.id, now).await?;
    store::create_session(&mut conn, &state.hlc, user.id, &token.hash, expires_at).await?;
    let user = store::get_user(&mut conn, user.id)
        .await?
        .ok_or_else(HubError::internal)?;
    Ok(Json(LoginResponse {
        token: token.token,
        expires_at,
        user,
    }))
}

async fn logout(State(state): State<HubState>, current: CurrentHubUser) -> HubResult<Json<()>> {
    let mut conn = state.pool.acquire().await?;
    store::revoke_session(&mut conn, &state.hlc, current.session.id).await?;
    Ok(Json(()))
}

async fn me(State(state): State<HubState>, current: CurrentHubUser) -> HubResult<Json<MeResponse>> {
    let mut conn = state.pool.acquire().await?;
    let hub_name = store::setting(&mut conn, store::KEY_HUB_NAME)
        .await?
        .and_then(|v| v.as_str().map(str::to_string));
    Ok(Json(MeResponse {
        user: current.user,
        hub_name,
    }))
}
