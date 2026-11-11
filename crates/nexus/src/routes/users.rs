//! Hub users (`nexus.admin`): list, create, change permissions, disable, reset password.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use madrasa_core::password::{hash_password, validate_password};
use serde::Deserialize;
use uuid::Uuid;

use super::{clean_username, required, JsonBody};
use crate::auth::CurrentHubUser;
use crate::permissions::{parse_codes, HubPermission};
use crate::store::{self, HubUser, HubUserChange, NewHubUser};
use crate::{HubError, HubResult, HubState};

#[derive(Debug, Deserialize)]
pub struct CreateUser {
    pub username: String,
    pub full_name: String,
    #[serde(default)]
    pub email: Option<String>,
    pub password: String,
    pub permissions: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUser {
    #[serde(default)]
    pub full_name: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub permissions: Option<Vec<String>>,
    #[serde(default)]
    pub is_active: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ResetPassword {
    pub password: String,
}

pub fn router() -> Router<HubState> {
    Router::new()
        .route("/users", get(list).post(create))
        .route("/users/{id}", patch(update))
        .route("/users/{id}/password", post(reset_password))
}

async fn list(
    State(state): State<HubState>,
    current: CurrentHubUser,
) -> HubResult<Json<Vec<HubUser>>> {
    current.require(HubPermission::Admin)?;
    let mut conn = state.pool.acquire().await?;
    Ok(Json(store::list_users(&mut conn).await?))
}

async fn create(
    State(state): State<HubState>,
    current: CurrentHubUser,
    body: JsonBody<CreateUser>,
) -> HubResult<(StatusCode, Json<HubUser>)> {
    current.require(HubPermission::Admin)?;
    let Json(b) = body?;
    let username = clean_username(&b.username)?;
    let full_name = required(&b.full_name, "The full name")?;
    let permissions = parse_codes(&b.permissions).map_err(HubError::validation)?;
    validate_password(&b.password).map_err(|e| HubError::validation(format!("Password: {e}.")))?;
    let password_hash = hash_password(&b.password).map_err(|_| HubError::internal())?;
    let mut tx = state.pool.begin().await?;
    let user = store::create_user(
        &mut tx,
        &state.hlc,
        Some(current.user.id),
        NewHubUser {
            username,
            full_name,
            email: b.email,
            password_hash,
            permissions,
        },
    )
    .await?
    .ok_or_else(|| HubError::conflict("A user with this username already exists."))?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(user)))
}

async fn update(
    State(state): State<HubState>,
    current: CurrentHubUser,
    Path(id): Path<Uuid>,
    body: JsonBody<UpdateUser>,
) -> HubResult<Json<HubUser>> {
    current.require(HubPermission::Admin)?;
    let Json(b) = body?;
    let mut tx = state.pool.begin().await?;
    let before = store::get_user(&mut tx, id)
        .await?
        .ok_or_else(|| HubError::not_found("User"))?;
    let permissions = match &b.permissions {
        Some(codes) => Some(parse_codes(codes).map_err(HubError::validation)?),
        None => None,
    };
    let full_name = match &b.full_name {
        Some(n) => Some(required(n, "The full name")?),
        None => None,
    };
    // The hub must always keep one active administrator, or nobody could manage it.
    let was_admin = before.is_active && before.permissions.iter().any(|p| p == "nexus.admin");
    let stays_admin = b.is_active.unwrap_or(before.is_active)
        && permissions
            .as_ref()
            .unwrap_or(&before.permissions)
            .iter()
            .any(|p| p == "nexus.admin");
    if was_admin && !stays_admin && store::count_admins(&mut tx, Some(id)).await? == 0 {
        return Err(HubError::conflict(
            "This is the only active administrator. Give another user nexus.admin first.",
        ));
    }
    let after = store::update_user(
        &mut tx,
        &state.hlc,
        Some(current.user.id),
        &before,
        HubUserChange {
            full_name,
            email: b.email.map(Some),
            permissions,
            is_active: b.is_active,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(after))
}

async fn reset_password(
    State(state): State<HubState>,
    current: CurrentHubUser,
    Path(id): Path<Uuid>,
    body: JsonBody<ResetPassword>,
) -> HubResult<Json<HubUser>> {
    // Anyone may change their own password; changing another user's needs nexus.admin.
    if id != current.user.id {
        current.require(HubPermission::Admin)?;
    }
    let Json(b) = body?;
    validate_password(&b.password).map_err(|e| HubError::validation(format!("Password: {e}.")))?;
    let password_hash = hash_password(&b.password).map_err(|_| HubError::internal())?;
    let mut tx = state.pool.begin().await?;
    let user = store::get_user(&mut tx, id)
        .await?
        .ok_or_else(|| HubError::not_found("User"))?;
    store::set_password(
        &mut tx,
        &state.hlc,
        Some(current.user.id),
        id,
        &password_hash,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(user))
}
