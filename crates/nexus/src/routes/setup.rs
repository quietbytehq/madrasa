//! First-run setup: creates the first administrator. Open only while the hub has no users,
//! and only with the one-time setup token.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use madrasa_core::password::{hash_password, validate_password};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use super::{clean_username, required, JsonBody};
use crate::permissions::HubPermission;
use crate::store::{self, NewHubUser};
use crate::{HubError, HubResult, HubState};

#[derive(Debug, Serialize)]
pub struct SetupStatus {
    pub needs_setup: bool,
    pub hub_name: Option<String>,
}

#[derive(Deserialize)]
pub struct SetupRequest {
    pub setup_token: String,
    pub hub_name: String,
    pub admin_username: String,
    pub admin_full_name: String,
    pub admin_password: String,
    /// Sets key escrow up at once. Optional: an administrator can do it later.
    #[serde(default)]
    pub escrow_passphrase: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SetupResponse {
    pub admin_user_id: Uuid,
}

pub fn router() -> Router<HubState> {
    Router::new()
        .route("/setup/status", get(status))
        .route("/setup", post(run_setup))
}

async fn status(State(state): State<HubState>) -> HubResult<Json<SetupStatus>> {
    let mut conn = state.pool.acquire().await?;
    let needs_setup = store::count_users(&mut conn).await? == 0;
    let hub_name = store::setting(&mut conn, store::KEY_HUB_NAME)
        .await?
        .and_then(|v| v.as_str().map(str::to_string));
    Ok(Json(SetupStatus {
        needs_setup,
        hub_name,
    }))
}

fn already_done() -> HubError {
    HubError::new(
        StatusCode::CONFLICT,
        "setup_already_done",
        "Setup has already been completed. Log in instead.",
    )
}

async fn run_setup(
    State(state): State<HubState>,
    body: JsonBody<SetupRequest>,
) -> HubResult<Json<SetupResponse>> {
    let Json(req) = body?;
    {
        let mut conn = state.pool.acquire().await?;
        if store::count_users(&mut conn).await? > 0 {
            return Err(already_done());
        }
    }
    if !state.setup_token_matches(&req.setup_token) {
        return Err(HubError::new(
            StatusCode::FORBIDDEN,
            "bad_setup_token",
            "The setup token is not correct. It is printed in the server log when the hub first starts, or set in NEXUS_SETUP_TOKEN.",
        ));
    }
    let hub_name = required(&req.hub_name, "The network name")?;
    let username = clean_username(&req.admin_username)?;
    let full_name = required(&req.admin_full_name, "The administrator's name")?;
    validate_password(&req.admin_password)
        .map_err(|e| HubError::validation(format!("Administrator password: {e}.")))?;
    let password_hash = hash_password(&req.admin_password).map_err(|_| HubError::internal())?;
    let escrow_keys = match req.escrow_passphrase.filter(|p| !p.is_empty()) {
        Some(passphrase) => Some(super::escrow::new_keys(passphrase).await?),
        None => None,
    };

    let mut tx = state.pool.begin().await?;
    // Two setups at once: the second waits here, then sees the first one's user.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('madrasa-nexus-setup'))")
        .execute(&mut *tx)
        .await?;
    if store::count_users(&mut tx).await? > 0 {
        return Err(already_done());
    }
    let admin = store::create_user(
        &mut tx,
        &state.hlc,
        None,
        NewHubUser {
            username,
            full_name,
            email: None,
            password_hash,
            permissions: HubPermission::ALL
                .iter()
                .map(|p| p.code().to_string())
                .collect(),
        },
    )
    .await?
    .ok_or_else(already_done)?;
    store::set_setting(
        &mut tx,
        &state.hlc,
        Some(admin.id),
        store::KEY_HUB_NAME,
        Value::from(hub_name.clone()),
    )
    .await?;
    store::audit(
        &mut tx,
        &state.hlc,
        Some(admin.id),
        "setup",
        "hub",
        None,
        None,
        Some(serde_json::json!({ "hub_name": hub_name })),
    )
    .await?;
    if let Some(keys) = &escrow_keys {
        crate::escrow::save_keys(&mut tx, &state.hlc, Some(admin.id), keys).await?;
        store::audit(
            &mut tx,
            &state.hlc,
            Some(admin.id),
            "escrow_setup",
            "hub_escrow",
            None,
            None,
            None,
        )
        .await?;
    }
    tx.commit().await?;
    state.close_setup();
    tracing::info!("hub first-run setup completed");
    Ok(Json(SetupResponse {
        admin_user_id: admin.id,
    }))
}
