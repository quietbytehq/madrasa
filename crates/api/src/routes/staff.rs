//! Staff profiles and teaching assignments.

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::NaiveDate;
use madrasa_core::locale;
use madrasa_core::permissions::Permission;

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
}

async fn view(c: &mut sqlx::PgConnection, s: Staff) -> ApiResult<StaffView> {
    let (roles, username) = match s.user_id {
        Some(uid) => {
            let roles = roles::codes_for_user(c, uid).await?;
            let username = madrasa_db::users::get(c, uid)
                .await
                .ok()
                .map(|u| u.username);
            (roles, username)
        }
        None => (Vec::new(), None),
    };
    Ok(StaffView {
        staff: s,
        roles,
        username,
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
        out.push(view(&mut c, s).await?);
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
    Ok(Json(view(&mut c, s).await?))
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
