//! Consent manager (M2 stage 8): consent types, consents per student (history kept) and
//! bulk recording per section.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use chrono::NaiveDate;
use madrasa_core::consents::{self as rules, Current, GivenBy, Renewal, Required};
use madrasa_core::permissions::Permission;
use madrasa_db::consents as repo;
use madrasa_db::consents::{ConsentType, ConsentView};
use madrasa_db::models::AcademicYear;
use madrasa_db::{academics, guardians, students};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::{ApiError, ApiResult, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/consent-types", get(list_types).post(create_type))
        .route("/consent-types/{id}", put(update_type).delete(delete_type))
        .route(
            "/students/{id}/consents",
            get(student_consents).post(record_student_consent),
        )
        .route(
            "/sections/{id}/consents",
            get(section_consents).post(record_section_consents),
        )
}

// ---- types ----------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct TypeRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub given_by: GivenBy,
    pub required: Required,
    #[serde(default)]
    pub min_level: Option<i32>,
    #[serde(default)]
    pub max_level: Option<i32>,
    pub renewal: Renewal,
    #[serde(default)]
    pub position: i32,
}

impl TypeRequest {
    fn input(&self) -> ApiResult<repo::ConsentTypeInput> {
        if self.name.trim().is_empty() {
            return Err(ApiError::validation("The consent type needs a name."));
        }
        if self.required == Required::ClassRange
            && self.min_level.is_none()
            && self.max_level.is_none()
        {
            return Err(ApiError::validation(
                "Give the class range (from and/or to level) for a consent required by class.",
            ));
        }
        Ok(repo::ConsentTypeInput {
            name: self.name.clone(),
            description: self.description.clone(),
            given_by: self.given_by,
            required: self.required,
            min_level: if self.required == Required::ClassRange {
                self.min_level
            } else {
                None
            },
            max_level: if self.required == Required::ClassRange {
                self.max_level
            } else {
                None
            },
            renewal: self.renewal,
            position: self.position,
        })
    }
}

async fn list_types(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<ConsentType>>> {
    user.require(Permission::StudentsRead)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(repo::list_types(&mut c, user.user.school_id).await?))
}

async fn create_type(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<TypeRequest>,
) -> ApiResult<(StatusCode, Json<ConsentType>)> {
    user.require(Permission::AcademicsManage)?;
    let input = req.input()?;
    let mut tx = state.pool.begin().await?;
    let row = repo::create_type(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        &input,
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(row)))
}

async fn load_type(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    id: Uuid,
) -> ApiResult<ConsentType> {
    let t = repo::get_type(c, id).await?;
    if t.school_id != user.user.school_id {
        return Err(ApiError::not_found("consent type"));
    }
    Ok(t)
}

async fn update_type(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<TypeRequest>,
) -> ApiResult<Json<ConsentType>> {
    user.require(Permission::AcademicsManage)?;
    let input = req.input()?;
    let mut tx = state.pool.begin().await?;
    load_type(&mut tx, &user, id).await?;
    let row = repo::update_type(&mut tx, &state.hlc, user.actor(), id, &input).await?;
    tx.commit().await?;
    Ok(Json(row))
}

async fn delete_type(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    user.require(Permission::AcademicsManage)?;
    let mut tx = state.pool.begin().await?;
    load_type(&mut tx, &user, id).await?;
    repo::delete_type(&mut tx, &state.hlc, user.actor(), id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- one student ----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct YearQuery {
    pub academic_year_id: Option<Uuid>,
}

async fn year_of(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    id: Option<Uuid>,
) -> ApiResult<AcademicYear> {
    match id {
        Some(id) => {
            let y = academics::get_year(c, id).await?;
            if y.school_id != user.user.school_id {
                return Err(ApiError::not_found("academic year"));
            }
            Ok(y)
        }
        None => academics::active_year(c, user.user.school_id)
            .await?
            .ok_or_else(|| ApiError::conflict("There is no active academic year.")),
    }
}

/// Mirrored by `ConsentState` in the UI.
#[derive(Debug, Serialize)]
pub struct ConsentState {
    #[serde(flatten)]
    pub consent_type: ConsentType,
    /// Whether this student needs the consent (the type's `required` rule applied).
    pub needed: bool,
    pub current: Current,
    pub latest: Option<ConsentView>,
}

#[derive(Debug, Serialize)]
pub struct GuardianOption {
    pub id: Uuid,
    pub full_name: String,
    pub relationship: String,
}

/// Mirrored by `StudentConsents` in the UI.
#[derive(Debug, Serialize)]
pub struct StudentConsents {
    pub academic_year_id: Uuid,
    pub class_level: Option<i32>,
    pub types: Vec<ConsentState>,
    pub guardians: Vec<GuardianOption>,
    pub history: Vec<ConsentView>,
}

async fn class_level_of(
    c: &mut sqlx::PgConnection,
    student_id: Uuid,
    year_id: Uuid,
) -> ApiResult<Option<i32>> {
    let Some(section_id) = students::section_in_year(c, student_id, year_id).await? else {
        return Ok(None);
    };
    let section = academics::get_section(c, section_id).await?;
    Ok(Some(academics::get_class(c, section.class_id).await?.level))
}

fn states(
    types: &[ConsentType],
    latest: &[ConsentView],
    student_id: Uuid,
    class_level: Option<i32>,
    year_id: Uuid,
) -> Vec<ConsentState> {
    types
        .iter()
        .map(|t| {
            let rule = t.rule();
            let mine = latest
                .iter()
                .find(|v| v.consent.student_id == student_id && v.consent.consent_type_id == t.id)
                .cloned();
            let current = rules::current(
                &rule,
                mine.as_ref().and_then(|v| {
                    rules::Status::parse(&v.consent.status).map(|status| rules::Latest {
                        status,
                        academic_year_id: v.consent.academic_year_id,
                        recorded_on: v.consent.recorded_on,
                    })
                }),
                year_id,
            );
            ConsentState {
                consent_type: t.clone(),
                needed: rule.is_required_for(class_level),
                current,
                latest: mine,
            }
        })
        .collect()
}

async fn load_student(c: &mut sqlx::PgConnection, user: &CurrentUser, id: Uuid) -> ApiResult<Uuid> {
    let student = students::get(c, id).await?;
    if student.school_id != user.user.school_id {
        return Err(ApiError::not_found("student"));
    }
    if let Some(allowed) = user.section_scope(c).await? {
        let year = academics::active_year(c, user.user.school_id).await?;
        let section = match year {
            Some(y) => students::section_in_year(c, id, y.id).await?,
            None => None,
        };
        if !section.is_some_and(|s| allowed.contains(&s)) {
            return Err(ApiError::not_found("student"));
        }
    }
    Ok(student.id)
}

async fn student_view(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    student_id: Uuid,
    year: &AcademicYear,
) -> ApiResult<StudentConsents> {
    let types = repo::list_types(c, user.user.school_id).await?;
    let latest = repo::latest_for_students(c, &[student_id]).await?;
    let class_level = class_level_of(c, student_id, year.id).await?;
    let guardians = guardians::for_student(c, student_id)
        .await?
        .into_iter()
        .map(|g| GuardianOption {
            id: g.id,
            full_name: g.full_name,
            relationship: g.relationship,
        })
        .collect();
    Ok(StudentConsents {
        academic_year_id: year.id,
        class_level,
        types: states(&types, &latest, student_id, class_level, year.id),
        guardians,
        history: repo::history(c, student_id).await?,
    })
}

async fn student_consents(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Query(q): Query<YearQuery>,
) -> ApiResult<Json<StudentConsents>> {
    user.require(Permission::StudentsRead)?;
    let mut c = state.pool.acquire().await?;
    let student_id = load_student(&mut c, &user, id).await?;
    let year = year_of(&mut c, &user, q.academic_year_id).await?;
    Ok(Json(student_view(&mut c, &user, student_id, &year).await?))
}

#[derive(Debug, Deserialize)]
pub struct RecordRequest {
    pub consent_type_id: Uuid,
    pub status: rules::Status,
    #[serde(default)]
    pub guardian_id: Option<Uuid>,
    #[serde(default)]
    pub recorded_on: Option<NaiveDate>,
    #[serde(default)]
    pub note: String,
    pub academic_year_id: Option<Uuid>,
}

async fn check_record(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    student_id: Uuid,
    req: &RecordRequest,
) -> ApiResult<ConsentType> {
    let t = load_type(c, user, req.consent_type_id).await?;
    if let Some(gid) = req.guardian_id {
        let linked = guardians::for_student(c, student_id).await?;
        if !linked.iter().any(|g| g.id == gid) {
            return Err(ApiError::validation(
                "The guardian is not linked to this student.",
            ));
        }
    } else if t.given_by == "guardian" && req.status == rules::Status::Given {
        return Err(ApiError::validation(
            "This consent is given by a guardian: pick the guardian who gave it.",
        ));
    }
    if req.note.chars().count() > 500 {
        return Err(ApiError::validation(
            "The note is too long (500 characters).",
        ));
    }
    Ok(t)
}

async fn record_student_consent(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<RecordRequest>,
) -> ApiResult<Json<StudentConsents>> {
    user.require(Permission::StudentsManage)?;
    let mut tx = state.pool.begin().await?;
    let student_id = load_student(&mut tx, &user, id).await?;
    let year = year_of(&mut tx, &user, req.academic_year_id).await?;
    check_record(&mut tx, &user, student_id, &req).await?;
    repo::record(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        &repo::NewConsent {
            student_id,
            consent_type_id: req.consent_type_id,
            status: req.status,
            guardian_id: req.guardian_id,
            academic_year_id: Some(year.id),
            recorded_on: req.recorded_on.unwrap_or_else(crate::clock::today),
            note: req.note.clone(),
        },
    )
    .await?;
    let view = student_view(&mut tx, &user, student_id, &year).await?;
    tx.commit().await?;
    Ok(Json(view))
}

// ---- section --------------------------------------------------------------------------------

/// Mirrored by `SectionConsentRow` in the UI.
#[derive(Debug, Serialize)]
pub struct SectionConsentRow {
    pub student_id: Uuid,
    pub student_number: String,
    pub full_name: String,
    pub roll_no: Option<i32>,
    pub guardians: Vec<GuardianOption>,
    pub cells: Vec<ConsentCell>,
}

#[derive(Debug, Serialize)]
pub struct ConsentCell {
    pub consent_type_id: Uuid,
    pub required: bool,
    pub current: Current,
    pub recorded_on: Option<NaiveDate>,
    pub guardian_name: Option<String>,
}

/// Mirrored by `SectionConsents` in the UI.
#[derive(Debug, Serialize)]
pub struct SectionConsents {
    pub section_id: Uuid,
    pub academic_year_id: Uuid,
    pub class_name: String,
    pub section_name: String,
    pub class_level: i32,
    pub types: Vec<ConsentType>,
    pub students: Vec<SectionConsentRow>,
    pub missing: usize,
}

async fn section_view(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    section_id: Uuid,
) -> ApiResult<SectionConsents> {
    let section = academics::get_section(c, section_id).await?;
    if section.school_id != user.user.school_id {
        return Err(ApiError::not_found("section"));
    }
    if let Some(allowed) = user.section_scope(c).await? {
        if !allowed.contains(&section.id) {
            return Err(ApiError::not_found("section"));
        }
    }
    let class = academics::get_class(c, section.class_id).await?;
    let types = repo::list_types(c, user.user.school_id).await?;
    let roster = madrasa_db::exams::roster(c, section.id).await?;
    let ids: Vec<Uuid> = roster.iter().map(|r| r.student_id).collect();
    let latest = repo::latest_for_students(c, &ids).await?;
    let mut students_out = Vec::with_capacity(roster.len());
    let mut missing = 0usize;
    for r in roster {
        let st = states(
            &types,
            &latest,
            r.student_id,
            Some(class.level),
            section.academic_year_id,
        );
        let cells: Vec<ConsentCell> = st
            .iter()
            .map(|s| {
                if s.needed && s.current.needs_collecting() {
                    missing += 1;
                }
                ConsentCell {
                    consent_type_id: s.consent_type.id,
                    required: s.needed,
                    current: s.current,
                    recorded_on: s.latest.as_ref().map(|l| l.consent.recorded_on),
                    guardian_name: s.latest.as_ref().and_then(|l| l.guardian_name.clone()),
                }
            })
            .collect();
        let guardians = guardians::for_student(c, r.student_id)
            .await?
            .into_iter()
            .map(|g| GuardianOption {
                id: g.id,
                full_name: g.full_name,
                relationship: g.relationship,
            })
            .collect();
        students_out.push(SectionConsentRow {
            student_id: r.student_id,
            student_number: r.student_number,
            full_name: r.full_name,
            roll_no: r.roll_no,
            guardians,
            cells,
        });
    }
    Ok(SectionConsents {
        section_id: section.id,
        academic_year_id: section.academic_year_id,
        class_name: class.name,
        section_name: section.name,
        class_level: class.level,
        types,
        students: students_out,
        missing,
    })
}

async fn section_consents(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SectionConsents>> {
    user.require(Permission::StudentsRead)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(section_view(&mut c, &user, id).await?))
}

#[derive(Debug, Deserialize)]
pub struct BulkLine {
    pub student_id: Uuid,
    pub consent_type_id: Uuid,
    pub status: rules::Status,
    #[serde(default)]
    pub guardian_id: Option<Uuid>,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Deserialize)]
pub struct BulkRequest {
    pub rows: Vec<BulkLine>,
    #[serde(default)]
    pub recorded_on: Option<NaiveDate>,
}

async fn record_section_consents(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<BulkRequest>,
) -> ApiResult<Json<SectionConsents>> {
    user.require(Permission::StudentsManage)?;
    let mut tx = state.pool.begin().await?;
    let view = section_view(&mut tx, &user, id).await?;
    let recorded_on = req.recorded_on.unwrap_or_else(crate::clock::today);
    for line in &req.rows {
        if !view
            .students
            .iter()
            .any(|s| s.student_id == line.student_id)
        {
            return Err(ApiError::not_found("student in this section"));
        }
        let single = RecordRequest {
            consent_type_id: line.consent_type_id,
            status: line.status,
            guardian_id: line.guardian_id,
            recorded_on: Some(recorded_on),
            note: line.note.clone(),
            academic_year_id: Some(view.academic_year_id),
        };
        check_record(&mut tx, &user, line.student_id, &single).await?;
        repo::record(
            &mut tx,
            &state.hlc,
            user.actor(),
            user.user.school_id,
            &repo::NewConsent {
                student_id: line.student_id,
                consent_type_id: line.consent_type_id,
                status: line.status,
                guardian_id: line.guardian_id,
                academic_year_id: Some(view.academic_year_id),
                recorded_on,
                note: line.note.clone(),
            },
        )
        .await?;
    }
    let after = section_view(&mut tx, &user, id).await?;
    tx.commit().await?;
    Ok(Json(after))
}
