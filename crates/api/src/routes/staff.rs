//! Staff profiles and teaching assignments.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::NaiveDate;
use madrasa_core::locale;
use madrasa_core::password::{hash_password, validate_password};
use madrasa_core::permissions::{Permission, RoleCode};

use crate::routes::students::{id_error, phone_error};
use madrasa_db::models::{SectionTeacher, Staff};
use madrasa_db::{academics, roles, staff};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::{ApiError, ApiResult, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/staff", get(list).post(create))
        .route("/staff/{id}", get(get_one).patch(update))
        .route("/staff/{id}/login", post(create_login).patch(update_login))
        .route("/staff/{id}/login/password", post(reset_login_password))
        .route(
            "/staff/{id}/assignments",
            get(assignments).put(set_assignments),
        )
}

type JsonBody<T> = Result<Json<T>, axum::extract::rejection::JsonRejection>;

#[derive(Debug, Deserialize)]
pub struct StaffQuery {
    pub q: Option<String>,
}

/// Staff row plus login role codes (if linked).
#[derive(Debug, Serialize)]
pub struct StaffView {
    #[serde(flatten)]
    pub staff: Staff,
    pub roles: Vec<String>,
    pub username: Option<String>,
    /// Whether the login may sign in; `None` without a login.
    pub login_active: Option<bool>,
}

/// Teachers see colleagues' names and roles, not their phone, email or ID number.
fn redact_for(user: &CurrentUser, mut v: StaffView) -> StaffView {
    if !user.can(Permission::TeachersManage) {
        v.staff.phone = None;
        v.staff.email = None;
        v.staff.cnic = None;
    }
    v
}

async fn view(c: &mut sqlx::PgConnection, s: Staff) -> ApiResult<StaffView> {
    let (roles, username, login_active) = match s.user_id {
        Some(uid) => {
            let roles = roles::codes_for_user(c, uid).await?;
            let account = madrasa_db::users::get(c, uid).await.ok();
            (
                roles,
                account.as_ref().map(|u| u.username.clone()),
                account.map(|u| u.is_active),
            )
        }
        None => (Vec::new(), None, None),
    };
    Ok(StaffView {
        staff: s,
        roles,
        username,
        login_active,
    })
}

async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<StaffQuery>,
) -> ApiResult<Json<Vec<StaffView>>> {
    user.require(Permission::TeachersRead)?;
    let mut c = state.pool.acquire().await?;
    let rows = staff::list(&mut c, user.user.school_id, q.q.as_deref()).await?;
    let mut out = Vec::with_capacity(rows.len());
    for s in rows {
        out.push(redact_for(&user, view(&mut c, s).await?));
    }
    Ok(Json(out))
}

async fn get_one(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<StaffView>> {
    user.require(Permission::TeachersRead)?;
    let mut c = state.pool.acquire().await?;
    let s = staff::get(&mut c, id).await?;
    if s.school_id != user.user.school_id {
        return Err(ApiError::not_found("staff member"));
    }
    Ok(Json(redact_for(&user, view(&mut c, s).await?)))
}

#[derive(Debug, Deserialize)]
pub struct StaffBody {
    pub staff_number: Option<String>,
    pub full_name: Option<String>,
    pub father_name: Option<String>,
    pub gender: Option<String>,
    #[serde(default, deserialize_with = "crate::routes::double_option")]
    pub phone: Option<Option<String>>,
    #[serde(default, deserialize_with = "crate::routes::double_option")]
    pub email: Option<Option<String>>,
    #[serde(default, deserialize_with = "crate::routes::double_option")]
    pub cnic: Option<Option<String>>,
    pub designation: Option<String>,
    #[serde(default, deserialize_with = "crate::routes::double_option")]
    pub joined_on: Option<Option<NaiveDate>>,
    pub status: Option<String>,
    #[serde(default, deserialize_with = "crate::routes::double_option")]
    pub user_id: Option<Option<Uuid>>,
}

fn clean_phone(country: &str, v: Option<Option<String>>) -> ApiResult<Option<Option<String>>> {
    match v {
        Some(Some(p)) if !p.trim().is_empty() => Ok(Some(Some(
            locale::normalise_phone_for(country, &p).ok_or_else(|| phone_error(country))?,
        ))),
        Some(_) => Ok(Some(None)),
        None => Ok(None),
    }
}

fn clean_cnic(country: &str, v: Option<Option<String>>) -> ApiResult<Option<Option<String>>> {
    match v {
        Some(Some(p)) if !p.trim().is_empty() => Ok(Some(Some(
            locale::normalise_national_id(country, &p).ok_or_else(|| id_error(country))?,
        ))),
        Some(_) => Ok(Some(None)),
        None => Ok(None),
    }
}

fn check_gender(g: &str) -> ApiResult<()> {
    if matches!(g, "male" | "female" | "other") {
        Ok(())
    } else {
        Err(ApiError::validation(
            "Gender must be male, female or other.",
        ))
    }
}

async fn check_user(
    c: &mut sqlx::PgConnection,
    school_id: Uuid,
    user_id: Option<Uuid>,
) -> ApiResult<()> {
    if let Some(uid) = user_id {
        if madrasa_db::users::get(c, uid).await?.school_id != school_id {
            return Err(ApiError::not_found("user"));
        }
    }
    Ok(())
}

async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<StaffBody>,
) -> ApiResult<Json<StaffView>> {
    let country = state.profile().country;
    user.require(Permission::TeachersManage)?;
    let Json(b) = body?;
    let full_name = b
        .full_name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .ok_or_else(|| ApiError::validation("Full name is required."))?;
    let gender = b.gender.unwrap_or_else(|| "other".into());
    check_gender(&gender)?;
    let mut tx = state.pool.begin().await?;
    let sid = user.user.school_id;
    let user_id = b.user_id.flatten();
    check_user(&mut tx, sid, user_id).await?;
    let staff_number = match b
        .staff_number
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        Some(n) => n.to_string(),
        None => format!(
            "S-{:04}",
            madrasa_db::students::next_sequence(&mut tx, &state.hlc, sid, "staff_number").await?
        ),
    };
    let s = staff::create(
        &mut tx,
        &state.hlc,
        user.actor(),
        staff::NewStaff {
            school_id: sid,
            user_id,
            staff_number,
            full_name: full_name.to_string(),
            father_name: b.father_name.unwrap_or_default(),
            gender,
            phone: clean_phone(&country, b.phone)?.flatten(),
            email: b.email.flatten().filter(|e| !e.trim().is_empty()),
            cnic: clean_cnic(&country, b.cnic)?.flatten(),
            designation: b.designation.unwrap_or_default(),
            joined_on: b.joined_on.flatten(),
            custom: serde_json::json!({}),
        },
    )
    .await?;
    let v = view(&mut tx, s).await?;
    tx.commit().await?;
    Ok(Json(v))
}

async fn update(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: JsonBody<StaffBody>,
) -> ApiResult<Json<StaffView>> {
    let country = state.profile().country;
    user.require(Permission::TeachersManage)?;
    let Json(b) = body?;
    let mut c = state.pool.acquire().await?;
    if staff::get(&mut c, id).await?.school_id != user.user.school_id {
        return Err(ApiError::not_found("staff member"));
    }
    if let Some(g) = &b.gender {
        check_gender(g)?;
    }
    if b.full_name.as_deref().is_some_and(|n| n.trim().is_empty()) {
        return Err(ApiError::validation("Full name is required."));
    }
    if let Some(st) = &b.status {
        if !matches!(st.as_str(), "active" | "left") {
            return Err(ApiError::validation("Status must be active or left."));
        }
    }
    if let Some(Some(uid)) = b.user_id {
        check_user(&mut c, user.user.school_id, Some(uid)).await?;
    }
    let s = staff::update(
        &mut c,
        &state.hlc,
        user.actor(),
        id,
        staff::StaffUpdate {
            user_id: b.user_id,
            full_name: b.full_name,
            father_name: b.father_name,
            gender: b.gender,
            phone: clean_phone(&country, b.phone)?,
            email: b.email.map(|e| e.filter(|e| !e.trim().is_empty())),
            cnic: clean_cnic(&country, b.cnic)?,
            designation: b.designation,
            joined_on: b.joined_on,
            status: b.status,
            custom: None,
        },
    )
    .await?;
    Ok(Json(view(&mut c, s).await?))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Assignment {
    pub section_id: Uuid,
    pub subject_id: Option<Uuid>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Assignments {
    pub assignments: Vec<Assignment>,
}

fn to_assignments(rows: Vec<SectionTeacher>) -> Assignments {
    Assignments {
        assignments: rows
            .into_iter()
            .map(|r| Assignment {
                section_id: r.section_id,
                subject_id: r.subject_id,
            })
            .collect(),
    }
}

async fn assignments(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Assignments>> {
    user.require(Permission::TeachersRead)?;
    let mut c = state.pool.acquire().await?;
    if staff::get(&mut c, id).await?.school_id != user.user.school_id {
        return Err(ApiError::not_found("staff member"));
    }
    Ok(Json(to_assignments(staff::assignments(&mut c, id).await?)))
}

async fn set_assignments(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: JsonBody<Assignments>,
) -> ApiResult<Json<Assignments>> {
    user.require(Permission::TeachersManage)?;
    let Json(b) = body?;
    let mut tx = state.pool.begin().await?;
    let sid = user.user.school_id;
    if staff::get(&mut tx, id).await?.school_id != sid {
        return Err(ApiError::not_found("staff member"));
    }
    let mut pairs = Vec::with_capacity(b.assignments.len());
    for a in &b.assignments {
        if academics::get_section(&mut tx, a.section_id)
            .await?
            .school_id
            != sid
        {
            return Err(ApiError::not_found("section"));
        }
        if let Some(subj) = a.subject_id {
            if academics::get_subject(&mut tx, subj).await?.school_id != sid {
                return Err(ApiError::not_found("subject"));
            }
        }
        pairs.push((a.section_id, a.subject_id));
    }
    let rows = staff::set_assignments(&mut tx, &state.hlc, user.actor(), id, &pairs).await?;
    tx.commit().await?;
    Ok(Json(to_assignments(rows)))
}

// ---- login accounts (Staff screen → "Create login") ---------------------------------------

/// Mirrored by `CreateLoginBody` in the UI.
#[derive(Debug, Deserialize)]
pub struct CreateLoginBody {
    pub username: String,
    pub password: String,
    /// One of `principal`, `admin`, `accountant`, `teacher`.
    pub role: String,
}

/// Mirrored by `UpdateLoginBody` in the UI. Omitted fields are left unchanged.
#[derive(Debug, Deserialize, Default)]
pub struct UpdateLoginBody {
    pub role: Option<String>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordBody {
    pub password: String,
}

fn role_code(code: &str) -> ApiResult<RoleCode> {
    match code.trim().to_lowercase().as_str() {
        "principal" => Ok(RoleCode::Principal),
        "admin" => Ok(RoleCode::Admin),
        "accountant" => Ok(RoleCode::Accountant),
        "teacher" => Ok(RoleCode::Teacher),
        _ => Err(ApiError::validation(
            "Role must be principal, admin, accountant or teacher.",
        )),
    }
}

fn check_username(username: &str) -> ApiResult<()> {
    if username.len() < 3
        || !username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
    {
        return Err(ApiError::validation(
            "Username must be at least 3 characters: letters, digits, dots or underscores.",
        ));
    }
    Ok(())
}

async fn load_scoped_staff(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    id: Uuid,
) -> ApiResult<Staff> {
    let s = staff::get(c, id).await?;
    if s.school_id != user.user.school_id {
        return Err(ApiError::not_found("staff member"));
    }
    Ok(s)
}

/// Create a login account for a staff member with one role (`users.manage`). The account
/// takes the staff member's name and email; the password must meet the usual rule.
async fn create_login(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: JsonBody<CreateLoginBody>,
) -> ApiResult<Json<StaffView>> {
    user.require(Permission::UsersManage)?;
    let Json(b) = body?;
    let username = b.username.trim().to_lowercase();
    check_username(&username)?;
    let role = role_code(&b.role)?;
    validate_password(&b.password).map_err(|e| ApiError::validation(format!("Password: {e}.")))?;
    let password_hash = hash_password(&b.password)?;
    let sid = user.user.school_id;
    let mut tx = state.pool.begin().await?;
    let member = load_scoped_staff(&mut tx, &user, id).await?;
    if member.user_id.is_some() {
        return Err(ApiError::validation(
            "This staff member already has a login. Change its role or reset the password instead.",
        ));
    }
    if madrasa_db::users::find_by_username(&mut tx, sid, &username)
        .await?
        .is_some()
    {
        return Err(ApiError::validation("That username is already taken."));
    }
    let account = madrasa_db::users::create(
        &mut tx,
        &state.hlc,
        user.actor(),
        madrasa_db::users::NewUser {
            school_id: sid,
            username,
            full_name: member.full_name.clone(),
            email: member.email.clone(),
            password_hash,
        },
    )
    .await?;
    let r = roles::find_by_code(&mut tx, sid, role.code()).await?;
    roles::assign(&mut tx, &state.hlc, user.actor(), account.id, &r).await?;
    let member = staff::update(
        &mut tx,
        &state.hlc,
        user.actor(),
        member.id,
        staff::StaffUpdate {
            user_id: Some(Some(account.id)),
            ..Default::default()
        },
    )
    .await?;
    let v = view(&mut tx, member).await?;
    tx.commit().await?;
    Ok(Json(v))
}

/// Change the role or enable/disable the login (`users.manage`). Nobody can disable their
/// own account.
async fn update_login(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: JsonBody<UpdateLoginBody>,
) -> ApiResult<Json<StaffView>> {
    user.require(Permission::UsersManage)?;
    let Json(b) = body?;
    let sid = user.user.school_id;
    let mut tx = state.pool.begin().await?;
    let member = load_scoped_staff(&mut tx, &user, id).await?;
    let Some(uid) = member.user_id else {
        return Err(ApiError::validation("This staff member has no login yet."));
    };
    if let Some(code) = b.role.as_deref() {
        let role = role_code(code)?;
        let r = roles::find_by_code(&mut tx, sid, role.code()).await?;
        roles::set_role(&mut tx, &state.hlc, user.actor(), uid, &r).await?;
    }
    if let Some(active) = b.is_active {
        if !active && uid == user.user.id {
            return Err(ApiError::validation("You cannot disable your own account."));
        }
        madrasa_db::users::update(
            &mut tx,
            &state.hlc,
            user.actor(),
            uid,
            madrasa_db::users::UserUpdate {
                is_active: Some(active),
                ..Default::default()
            },
        )
        .await?;
    }
    let v = view(&mut tx, member).await?;
    tx.commit().await?;
    Ok(Json(v))
}

/// Set a new password for the staff member's login (`users.manage`). Audited as
/// `password_reset`; lockouts are cleared.
async fn reset_login_password(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: JsonBody<ResetPasswordBody>,
) -> ApiResult<Json<StaffView>> {
    user.require(Permission::UsersManage)?;
    let Json(b) = body?;
    validate_password(&b.password).map_err(|e| ApiError::validation(format!("Password: {e}.")))?;
    let password_hash = hash_password(&b.password)?;
    let mut tx = state.pool.begin().await?;
    let member = load_scoped_staff(&mut tx, &user, id).await?;
    let Some(uid) = member.user_id else {
        return Err(ApiError::validation("This staff member has no login yet."));
    };
    madrasa_db::users::set_password(&mut tx, &state.hlc, user.actor(), uid, &password_hash).await?;
    let v = view(&mut tx, member).await?;
    tx.commit().await?;
    Ok(Json(v))
}
