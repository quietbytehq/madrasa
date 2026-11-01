//! Settings → Users: every login of the school with its role, linked staff member, status and
//! last login, and the same create / change role / disable / reset actions as the staff record.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use madrasa_core::permissions::Permission;
use madrasa_db::models::User;
use madrasa_db::{roles, staff, users};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::routes::staff::{
    apply_login_update, apply_password_reset, create_login_for, load_scoped_staff, CreateLoginBody,
    ResetPasswordBody, UpdateLoginBody,
};
use crate::routes::UserSummary;
use crate::{ApiError, ApiResult, AppState};

/// Mirrored by `UserAccount` in the UI.
#[derive(Debug, Serialize)]
pub struct UserAccount {
    #[serde(flatten)]
    pub user: UserSummary,
    /// The staff record this login belongs to, if any (the setup administrator has none).
    pub staff_id: Option<Uuid>,
    pub staff_name: Option<String>,
    pub last_login_at: Option<DateTime<Utc>>,
}

/// Mirrored by `CreateUserBody` in the UI: a login for a staff member who has none.
#[derive(Debug, Deserialize)]
pub struct CreateUserBody {
    pub staff_id: Uuid,
    pub username: String,
    pub password: String,
    pub role: String,
}

type JsonBody<T> = Result<Json<T>, axum::extract::rejection::JsonRejection>;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/users", get(list).post(create))
        .route("/users/{id}", axum::routing::patch(update))
        .route("/users/{id}/password", axum::routing::post(reset_password))
}

async fn account(c: &mut sqlx::PgConnection, user: &User) -> ApiResult<UserAccount> {
    let role_codes = roles::codes_for_user(c, user.id).await?;
    let member = staff::find_by_user(c, user.id).await?;
    Ok(UserAccount {
        user: UserSummary::from_user(user, role_codes),
        staff_id: member.as_ref().map(|m| m.id),
        staff_name: member.map(|m| m.full_name),
        last_login_at: user.last_login_at,
    })
}

async fn load_scoped_user(
    c: &mut sqlx::PgConnection,
    current: &CurrentUser,
    id: Uuid,
) -> ApiResult<User> {
    let u = users::get(c, id).await?;
    if u.school_id != current.user.school_id || u.deleted_at.is_some() {
        return Err(ApiError::not_found("user"));
    }
    Ok(u)
}

/// Every login of the caller's school (`users.manage`).
async fn list(
    State(state): State<AppState>,
    current: CurrentUser,
) -> ApiResult<Json<Vec<UserAccount>>> {
    current.require(Permission::UsersManage)?;
    let mut conn = state.pool.acquire().await?;
    let rows = users::list(&mut conn, current.user.school_id).await?;
    let mut out = Vec::with_capacity(rows.len());
    for user in &rows {
        out.push(account(&mut conn, user).await?);
    }
    Ok(Json(out))
}

/// Create a login for a staff member who has none (`users.manage`).
async fn create(
    State(state): State<AppState>,
    current: CurrentUser,
    body: JsonBody<CreateUserBody>,
) -> ApiResult<Json<UserAccount>> {
    current.require(Permission::UsersManage)?;
    let Json(b) = body?;
    let mut tx = state.pool.begin().await?;
    let member = load_scoped_staff(&mut tx, &current, b.staff_id).await?;
    let member = create_login_for(
        &mut tx,
        &state,
        &current,
        member,
        &CreateLoginBody {
            username: b.username,
            password: b.password,
            role: b.role,
        },
    )
    .await?;
    let uid = member.user_id.ok_or_else(ApiError::internal)?;
    let user = users::get(&mut tx, uid).await?;
    let out = account(&mut tx, &user).await?;
    tx.commit().await?;
    Ok(Json(out))
}

/// Change the role or enable/disable a login (`users.manage`).
async fn update(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
    body: JsonBody<UpdateLoginBody>,
) -> ApiResult<Json<UserAccount>> {
    current.require(Permission::UsersManage)?;
    let Json(b) = body?;
    let mut tx = state.pool.begin().await?;
    load_scoped_user(&mut tx, &current, id).await?;
    apply_login_update(&mut tx, &state, &current, id, &b).await?;
    let user = users::get(&mut tx, id).await?;
    let out = account(&mut tx, &user).await?;
    tx.commit().await?;
    Ok(Json(out))
}

/// Set a new password for a login (`users.manage`).
async fn reset_password(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
    body: JsonBody<ResetPasswordBody>,
) -> ApiResult<Json<UserAccount>> {
    current.require(Permission::UsersManage)?;
    let Json(b) = body?;
    let mut tx = state.pool.begin().await?;
    load_scoped_user(&mut tx, &current, id).await?;
    apply_password_reset(&mut tx, &state, &current, id, &b.password).await?;
    let user = users::get(&mut tx, id).await?;
    let out = account(&mut tx, &user).await?;
    tx.commit().await?;
    Ok(Json(out))
}
