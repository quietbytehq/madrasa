use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use madrasa_core::password::{hash_password, validate_password, verify_password};
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
        .route("/auth/password", post(change_own_password))
}

async fn login(
    State(state): State<HubState>,
    body: JsonBody<LoginRequest>,
) -> HubResult<Json<LoginResponse>> {
    let Json(req) = body?;
    let now = Utc::now();
    let mut conn = state.pool.acquire().await?;
    let Some(user) = store::find_user(&mut conn, &req.username).await? else {
        // Answered, counted and locked exactly like a wrong password on a real account, so
        // the answers do not tell which usernames exist.
        return Err(
            if state
                .unknown_logins
                .refuse(&state.lockout, &req.username, &req.password, now)
            {
                HubError::account_locked()
            } else {
                HubError::invalid_credentials()
            },
        );
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

#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

/// Change one's own password, whatever one's permissions: the current password is asked for,
/// the new one must meet the usual rule, and every other session of the user is ended. The
/// same rules as a school login (`POST /auth/password` of the school API): a wrong current
/// password counts as a failed login, so a console left open cannot be used to guess, and
/// after the usual number of them the account is locked for the usual time.
async fn change_own_password(
    State(state): State<HubState>,
    current: CurrentHubUser,
    body: JsonBody<ChangePasswordRequest>,
) -> HubResult<StatusCode> {
    let Json(req) = body?;
    let now = Utc::now();
    let mut tx = state.pool.begin().await?;
    // Read again inside the transaction: the lock and the hash as they are now.
    let user = store::get_user(&mut tx, current.user.id)
        .await?
        .ok_or_else(HubError::unauthenticated)?;
    if state.lockout.is_locked(user.locked_until, now) {
        return Err(HubError::account_locked());
    }
    if !verify_password(&req.current_password, &user.password_hash) {
        let next = state
            .lockout
            .after_failure(user.failed_login_count.max(0) as u32, now);
        store::record_failed_login(
            &mut tx,
            &state.hlc,
            user.id,
            next.failed_count as i32,
            next.locked_until,
        )
        .await?;
        tx.commit().await?;
        if next.locked_until.is_some() {
            tracing::warn!(user = %user.username, "hub account locked after wrong current passwords");
        }
        return Err(HubError::validation("The current password is not right."));
    }
    validate_password(&req.new_password)
        .map_err(|e| HubError::validation(format!("New password: {e}.")))?;
    if req.new_password == req.current_password {
        return Err(HubError::validation(
            "The new password must be different from the current one.",
        ));
    }
    let hash = hash_password(&req.new_password).map_err(|_| HubError::internal())?;
    store::set_password(
        &mut tx,
        &state.hlc,
        Some(user.id),
        user.id,
        &hash,
        Some(current.session.id),
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
