//! Academic years, classes, sections and subjects.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use madrasa_core::permissions::Permission;
use madrasa_db::academics;
use madrasa_db::models::{AcademicYear, Class, Section, Subject};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::{ApiError, ApiResult, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/academic-years", get(list_years).post(create_year))
        .route("/academic-years/{id}/activate", post(activate_year))
        .route("/classes", get(list_classes).post(create_class))
        .route("/classes/{id}", axum::routing::patch(update_class))
        .route(
            "/classes/{id}/subjects",
            get(class_subjects).put(set_class_subjects),
        )
        .route("/sections", get(list_sections).post(create_section))
        .route("/sections/{id}", axum::routing::patch(update_section))
        .route("/subjects", get(list_subjects).post(create_subject))
        .route("/subjects/{id}", axum::routing::patch(update_subject))
}

type JsonBody<T> = Result<Json<T>, axum::extract::rejection::JsonRejection>;

// ---- academic years ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct NewYearBody {
    /// Calendar year the academic year starts in, e.g. 2026.
    pub start_year: i32,
    pub start_month: i16,
    /// Optional override of the generated name (`2026-27`).
    pub name: Option<String>,
    pub activate: Option<bool>,
}

async fn list_years(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<AcademicYear>>> {
    user.require(Permission::AcademicsRead)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        academics::list_years(&mut c, user.user.school_id).await?,
    ))
}

async fn create_year(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<NewYearBody>,
) -> ApiResult<Json<AcademicYear>> {
    user.require(Permission::AcademicsManage)?;
    let Json(b) = body?;
    if !(1..=12).contains(&b.start_month) {
        return Err(ApiError::validation(
            "Academic year start month must be between 1 and 12.",
        ));
    }
    if !(2000..=2100).contains(&b.start_year) {
        return Err(ApiError::validation(
            "Start year must be between 2000 and 2100.",
        ));
    }
    let (name, start, end) = academics::default_year_bounds(b.start_year, b.start_month as u32)
        .ok_or_else(|| ApiError::validation("Invalid year."))?;
    let mut tx = state.pool.begin().await?;
    let year = academics::create_year(
        &mut tx,
        &state.hlc,
        user.actor(),
        academics::NewAcademicYear {
            school_id: user.user.school_id,
            name: b.name.filter(|n| !n.trim().is_empty()).unwrap_or(name),
            start_date: start,
            end_date: end,
            start_month: b.start_month,
        },
    )
    .await?;
    let year = if b.activate.unwrap_or(false) {
        academics::activate_year(&mut tx, &state.hlc, user.actor(), year.id).await?
    } else {
        year
    };
    tx.commit().await?;
    Ok(Json(year))
}

async fn activate_year(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<AcademicYear>> {
    user.require(Permission::AcademicsManage)?;
    let mut tx = state.pool.begin().await?;
    let year = academics::get_year(&mut tx, id).await?;
    if year.school_id != user.user.school_id {
        return Err(ApiError::not_found("academic year"));
    }
    let year = academics::activate_year(&mut tx, &state.hlc, user.actor(), id).await?;
    tx.commit().await?;
    Ok(Json(year))
}

// ---- classes ------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ClassBody {
    pub name: Option<String>,
    pub level: Option<i32>,
}

async fn list_classes(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<Class>>> {
    user.require(Permission::AcademicsRead)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        academics::list_classes(&mut c, user.user.school_id).await?,
    ))
}

async fn create_class(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<ClassBody>,
) -> ApiResult<Json<Class>> {
    user.require(Permission::AcademicsManage)?;
    let Json(b) = body?;
    let name = b
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .ok_or_else(|| ApiError::validation("Class name is required."))?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        academics::create_class(
            &mut c,
            &state.hlc,
            user.actor(),
            user.user.school_id,
            name,
            b.level.unwrap_or(0),
        )
        .await?,
    ))
}

async fn update_class(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: JsonBody<ClassBody>,
) -> ApiResult<Json<Class>> {
    user.require(Permission::AcademicsManage)?;
    let Json(b) = body?;
    let mut c = state.pool.acquire().await?;
    if academics::get_class(&mut c, id).await?.school_id != user.user.school_id {
        return Err(ApiError::not_found("class"));
    }
    if b.name.as_deref().is_some_and(|n| n.trim().is_empty()) {
        return Err(ApiError::validation("Class name is required."));
    }
    Ok(Json(
        academics::update_class(
            &mut c,
            &state.hlc,
            user.actor(),
            id,
            b.name.as_deref(),
            b.level,
        )
        .await?,
    ))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SubjectIds {
    pub subject_ids: Vec<Uuid>,
}

async fn class_subjects(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SubjectIds>> {
    user.require(Permission::AcademicsRead)?;
    let mut c = state.pool.acquire().await?;
    if academics::get_class(&mut c, id).await?.school_id != user.user.school_id {
        return Err(ApiError::not_found("class"));
    }
    Ok(Json(SubjectIds {
        subject_ids: academics::class_subject_ids(&mut c, id).await?,
    }))
}

async fn set_class_subjects(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: JsonBody<SubjectIds>,
) -> ApiResult<Json<SubjectIds>> {
    user.require(Permission::AcademicsManage)?;
    let Json(b) = body?;
    let mut tx = state.pool.begin().await?;
    if academics::get_class(&mut tx, id).await?.school_id != user.user.school_id {
        return Err(ApiError::not_found("class"));
    }
    for sid in &b.subject_ids {
        if academics::get_subject(&mut tx, *sid).await?.school_id != user.user.school_id {
            return Err(ApiError::not_found("subject"));
        }
    }
    let ids = academics::set_class_subjects(&mut tx, &state.hlc, user.actor(), id, &b.subject_ids)
        .await?;
    tx.commit().await?;
    Ok(Json(SubjectIds { subject_ids: ids }))
}

// ---- subjects -----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SubjectBody {
    pub name: Option<String>,
    pub code: Option<String>,
}

async fn list_subjects(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<Subject>>> {
    user.require(Permission::AcademicsRead)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        academics::list_subjects(&mut c, user.user.school_id).await?,
    ))
}

async fn create_subject(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<SubjectBody>,
) -> ApiResult<Json<Subject>> {
    user.require(Permission::AcademicsManage)?;
    let Json(b) = body?;
    let name = b
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .ok_or_else(|| ApiError::validation("Subject name is required."))?;
    let code = b
        .code
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .ok_or_else(|| ApiError::validation("Subject code is required."))?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        academics::create_subject(
            &mut c,
            &state.hlc,
            user.actor(),
            user.user.school_id,
            name,
            code,
        )
        .await?,
    ))
}

async fn update_subject(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: JsonBody<SubjectBody>,
) -> ApiResult<Json<Subject>> {
    user.require(Permission::AcademicsManage)?;
    let Json(b) = body?;
    let mut c = state.pool.acquire().await?;
    if academics::get_subject(&mut c, id).await?.school_id != user.user.school_id {
        return Err(ApiError::not_found("subject"));
    }
    Ok(Json(
        academics::update_subject(
            &mut c,
            &state.hlc,
            user.actor(),
            id,
            b.name.as_deref(),
            b.code.as_deref(),
        )
        .await?,
    ))
}

// ---- sections -----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SectionQuery {
    pub academic_year_id: Option<Uuid>,
    pub class_id: Option<Uuid>,
}

/// Section with display names, for the UI.
#[derive(Debug, Serialize)]
pub struct SectionView {
    #[serde(flatten)]
    pub section: Section,
    pub class_name: String,
    pub class_level: i32,
    pub class_teacher_name: Option<String>,
    pub student_count: i64,
}

async fn list_sections(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<SectionQuery>,
) -> ApiResult<Json<Vec<SectionView>>> {
    user.require(Permission::AcademicsRead)?;
    let mut c = state.pool.acquire().await?;
    let sections =
        academics::list_sections(&mut c, user.user.school_id, q.academic_year_id, q.class_id)
            .await?;
    let classes = academics::list_classes(&mut c, user.user.school_id).await?;
    let mut out = Vec::with_capacity(sections.len());
    for s in sections {
        let class = classes.iter().find(|k| k.id == s.class_id);
        let teacher = match s.class_teacher_id {
            Some(id) => madrasa_db::staff::get(&mut c, id)
                .await
                .ok()
                .map(|t| t.full_name),
            None => None,
        };
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM enrollments e JOIN students st ON st.id = e.student_id AND st.deleted_at IS NULL AND st.status = 'active' WHERE e.section_id = $1 AND e.deleted_at IS NULL")
            .bind(s.id)
            .fetch_one(&mut *c)
            .await?;
        out.push(SectionView {
            class_name: class.map(|k| k.name.clone()).unwrap_or_default(),
            class_level: class.map(|k| k.level).unwrap_or_default(),
            class_teacher_name: teacher,
            student_count: count,
            section: s,
        });
    }
    Ok(Json(out))
}

#[derive(Debug, Deserialize)]
pub struct NewSectionBody {
    pub class_id: Uuid,
    pub academic_year_id: Uuid,
    pub name: String,
    pub class_teacher_id: Option<Uuid>,
    pub capacity: Option<i32>,
}

async fn create_section(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<NewSectionBody>,
) -> ApiResult<Json<Section>> {
    user.require(Permission::AcademicsManage)?;
    let Json(b) = body?;
    if b.name.trim().is_empty() {
        return Err(ApiError::validation("Section name is required."));
    }
    let mut c = state.pool.acquire().await?;
    let sid = user.user.school_id;
    if academics::get_class(&mut c, b.class_id).await?.school_id != sid
        || academics::get_year(&mut c, b.academic_year_id)
            .await?
            .school_id
            != sid
    {
        return Err(ApiError::not_found("class or academic year"));
    }
    check_teacher(&mut c, sid, b.class_teacher_id).await?;
    Ok(Json(
        academics::create_section(
            &mut c,
            &state.hlc,
            user.actor(),
            academics::NewSection {
                school_id: sid,
                class_id: b.class_id,
                academic_year_id: b.academic_year_id,
                name: b.name,
                class_teacher_id: b.class_teacher_id,
                capacity: b.capacity,
            },
        )
        .await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct SectionPatch {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "crate::routes::double_option")]
    pub class_teacher_id: Option<Option<Uuid>>,
    #[serde(default, deserialize_with = "crate::routes::double_option")]
    pub capacity: Option<Option<i32>>,
}

async fn update_section(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: JsonBody<SectionPatch>,
) -> ApiResult<Json<Section>> {
    user.require(Permission::AcademicsManage)?;
    let Json(b) = body?;
    let mut c = state.pool.acquire().await?;
    if academics::get_section(&mut c, id).await?.school_id != user.user.school_id {
        return Err(ApiError::not_found("section"));
    }
    if let Some(Some(t)) = b.class_teacher_id {
        check_teacher(&mut c, user.user.school_id, Some(t)).await?;
    }
    Ok(Json(
        academics::update_section(
            &mut c,
            &state.hlc,
            user.actor(),
            id,
            academics::SectionUpdate {
                name: b.name,
                class_teacher_id: b.class_teacher_id,
                capacity: b.capacity,
            },
        )
        .await?,
    ))
}

async fn check_teacher(
    c: &mut sqlx::PgConnection,
    school_id: Uuid,
    staff_id: Option<Uuid>,
) -> ApiResult<()> {
    if let Some(id) = staff_id {
        if madrasa_db::staff::get(c, id).await?.school_id != school_id {
            return Err(ApiError::not_found("staff member"));
        }
    }
    Ok(())
}
