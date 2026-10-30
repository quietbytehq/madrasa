//! Exams: assessment setup, grading scheme, maximum marks, marks entry with the submit →
//! approve workflow, section result sheets with analysis, and report cards.
//!
//! Setup (assessments, weights, maximum marks, grading) needs `academics.manage`. Marks
//! entry needs `exams.manage` and, for teachers, an assignment to the section and subject (or
//! being its class teacher). Approval and reopening need `exams.approve`.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use madrasa_core::exams::{self, GradingScheme, Score, SheetStatus};
use madrasa_core::permissions::Permission;
use madrasa_db::models::{AssessmentType, MarkRow, MarkSheet, Section, SheetSummaryRow};
use madrasa_db::{academics, attendance, exams as repo, schools, staff, students};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::{ApiError, ApiResult, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/exams/assessments",
            get(list_assessments).post(create_assessment),
        )
        .route("/exams/assessments/defaults", post(default_assessments))
        .route(
            "/exams/assessments/weights",
            axum::routing::put(put_weights),
        )
        .route(
            "/exams/assessments/{id}",
            axum::routing::patch(rename_assessment).delete(delete_assessment),
        )
        .route("/exams/grading", get(get_grading).put(put_grading))
        .route("/exams/max-marks", get(get_max_marks).put(put_max_marks))
        .route("/exams/sheets", get(list_sheets))
        .route("/exams/sheet", get(get_sheet).put(put_sheet))
        .route("/exams/sheets/{id}/submit", post(submit_sheet))
        .route("/exams/sheets/{id}/approve", post(approve_sheet))
        .route("/exams/sheets/{id}/reopen", post(reopen_sheet))
        .route("/exams/results/section", get(section_results))
        .route(
            "/exams/results/section/report-cards",
            get(section_report_cards),
        )
        .route("/exams/results/student/{id}", get(student_report_card))
}

type JsonBody<T> = Result<Json<T>, axum::extract::rejection::JsonRejection>;

// ---- scope helpers ------------------------------------------------------------------------

/// What a teacher may enter marks for: `None` = everything; otherwise (section, subject)
/// pairs where `subject = None` means every subject of the section (class teacher).
async fn marks_scope(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
) -> ApiResult<Option<Vec<(Uuid, Option<Uuid>)>>> {
    if user.can(Permission::AcademicsManage) || !user.roles.iter().any(|r| r == "teacher") {
        return Ok(None);
    }
    let Some(st) = staff::find_by_user(c, user.user.id).await? else {
        return Ok(Some(Vec::new()));
    };
    let mut pairs: Vec<(Uuid, Option<Uuid>)> = staff::assignments(c, st.id)
        .await?
        .into_iter()
        .map(|a| (a.section_id, a.subject_id))
        .collect();
    for sid in staff::section_scope(c, st.id).await? {
        let is_class_teacher = academics::get_section(c, sid)
            .await
            .map(|s| s.class_teacher_id == Some(st.id))
            .unwrap_or(false);
        if is_class_teacher {
            pairs.push((sid, None));
        }
    }
    Ok(Some(pairs))
}

fn allows(scope: &Option<Vec<(Uuid, Option<Uuid>)>>, section_id: Uuid, subject_id: Uuid) -> bool {
    match scope {
        None => true,
        Some(pairs) => pairs
            .iter()
            .any(|(s, sub)| *s == section_id && (sub.is_none() || *sub == Some(subject_id))),
    }
}

async fn load_section(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    section_id: Uuid,
) -> ApiResult<Section> {
    let section = academics::get_section(c, section_id).await?;
    if section.school_id != user.user.school_id {
        return Err(ApiError::not_found("section"));
    }
    if let Some(scope) = user.section_scope(c).await? {
        if !scope.contains(&section.id) {
            return Err(ApiError::forbidden());
        }
    }
    Ok(section)
}

async fn load_assessment(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    id: Uuid,
) -> ApiResult<AssessmentType> {
    let a = repo::get_assessment(c, id).await?;
    if a.school_id != user.user.school_id {
        return Err(ApiError::not_found("assessment"));
    }
    Ok(a)
}

#[derive(Debug, Serialize)]
pub struct SectionHeader {
    pub id: Uuid,
    pub class_id: Uuid,
    pub class_name: String,
    pub section_name: String,
    pub class_teacher_name: Option<String>,
}

async fn section_header(c: &mut sqlx::PgConnection, s: &Section) -> ApiResult<SectionHeader> {
    let class = academics::get_class(c, s.class_id).await?;
    let teacher = match s.class_teacher_id {
        Some(id) => Some(staff::get(c, id).await?.full_name),
        None => None,
    };
    Ok(SectionHeader {
        id: s.id,
        class_id: class.id,
        class_name: class.name,
        section_name: s.name.clone(),
        class_teacher_name: teacher,
    })
}

// ---- assessments --------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct YearQuery {
    pub academic_year_id: Uuid,
}

async fn year_of(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    id: Uuid,
) -> ApiResult<madrasa_db::models::AcademicYear> {
    let y = academics::get_year(c, id).await?;
    if y.school_id != user.user.school_id {
        return Err(ApiError::not_found("academic year"));
    }
    Ok(y)
}

async fn list_assessments(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<YearQuery>,
) -> ApiResult<Json<Vec<AssessmentType>>> {
    user.require(Permission::ExamsRead)?;
    let mut c = state.pool.acquire().await?;
    year_of(&mut c, &user, q.academic_year_id).await?;
    Ok(Json(
        repo::list_assessments(&mut c, q.academic_year_id).await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct NewAssessmentRequest {
    pub academic_year_id: Uuid,
    pub name: String,
    #[serde(default)]
    pub weight: Decimal,
    #[serde(default)]
    pub position: i32,
}

async fn create_assessment(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<NewAssessmentRequest>,
) -> ApiResult<(StatusCode, Json<AssessmentType>)> {
    user.require(Permission::AcademicsManage)?;
    let Json(req) = body?;
    if req.name.trim().is_empty() {
        return Err(ApiError::validation("Assessment name is required."));
    }
    let mut c = state.pool.acquire().await?;
    year_of(&mut c, &user, req.academic_year_id).await?;
    let a = repo::create_assessment(
        &mut c,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        req.academic_year_id,
        &req.name,
        req.weight,
        req.position,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(a)))
}

async fn default_assessments(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<YearQuery>,
) -> ApiResult<Json<Vec<AssessmentType>>> {
    user.require(Permission::AcademicsManage)?;
    let Json(req) = body?;
    let mut c = state.pool.acquire().await?;
    year_of(&mut c, &user, req.academic_year_id).await?;
    Ok(Json(
        repo::create_default_assessments(
            &mut c,
            &state.hlc,
            user.actor(),
            user.user.school_id,
            req.academic_year_id,
        )
        .await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct RenameRequest {
    pub name: String,
    #[serde(default)]
    pub position: i32,
}

async fn rename_assessment(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: JsonBody<RenameRequest>,
) -> ApiResult<Json<AssessmentType>> {
    user.require(Permission::AcademicsManage)?;
    let Json(req) = body?;
    let mut c = state.pool.acquire().await?;
    load_assessment(&mut c, &user, id).await?;
    Ok(Json(
        repo::rename_assessment(
            &mut c,
            &state.hlc,
            user.actor(),
            id,
            &req.name,
            req.position,
        )
        .await?,
    ))
}

async fn delete_assessment(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    user.require(Permission::AcademicsManage)?;
    let mut c = state.pool.acquire().await?;
    load_assessment(&mut c, &user, id).await?;
    repo::delete_assessment(&mut c, &state.hlc, user.actor(), id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct WeightsRequest {
    pub academic_year_id: Uuid,
    pub weights: Vec<WeightLine>,
}

#[derive(Debug, Deserialize)]
pub struct WeightLine {
    pub id: Uuid,
    pub weight: Decimal,
}

async fn put_weights(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<WeightsRequest>,
) -> ApiResult<Json<Vec<AssessmentType>>> {
    user.require(Permission::AcademicsManage)?;
    let Json(req) = body?;
    let values: Vec<Decimal> = req.weights.iter().map(|w| w.weight).collect();
    exams::validate_weights(&values).map_err(|e| ApiError::validation(format!("{e}.")))?;
    let mut c = state.pool.acquire().await?;
    year_of(&mut c, &user, req.academic_year_id).await?;
    let pairs: Vec<(Uuid, Decimal)> = req.weights.iter().map(|w| (w.id, w.weight)).collect();
    Ok(Json(
        repo::set_weights(
            &mut c,
            &state.hlc,
            user.actor(),
            req.academic_year_id,
            &pairs,
        )
        .await?,
    ))
}

// ---- grading ------------------------------------------------------------------------------

async fn get_grading(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<GradingScheme>> {
    user.require(Permission::ExamsRead)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(repo::grading(&mut c, user.user.school_id).await?))
}

async fn put_grading(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<GradingScheme>,
) -> ApiResult<Json<GradingScheme>> {
    user.require(Permission::AcademicsManage)?;
    let Json(scheme) = body?;
    let scheme = scheme
        .normalized()
        .map_err(|e| ApiError::validation(format!("{e}.")))?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        repo::set_grading(
            &mut c,
            &state.hlc,
            user.actor(),
            user.user.school_id,
            scheme,
        )
        .await?,
    ))
}

// ---- maximum marks ------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct MaxMarksQuery {
    pub assessment_id: Uuid,
    pub class_id: Uuid,
}

/// Mirrored by `MaxMarksLine` in the UI.
#[derive(Debug, Serialize)]
pub struct MaxMarksLine {
    pub subject_id: Uuid,
    pub subject_name: String,
    pub subject_code: String,
    pub max_marks: Option<Decimal>,
}

async fn max_marks_lines(
    c: &mut sqlx::PgConnection,
    school_id: Uuid,
    assessment_id: Uuid,
    class_id: Uuid,
) -> ApiResult<Vec<MaxMarksLine>> {
    let subject_ids = academics::class_subject_ids(c, class_id).await?;
    let subjects = academics::list_subjects(c, school_id).await?;
    let maxes = repo::max_marks(c, assessment_id, class_id).await?;
    Ok(subjects
        .into_iter()
        .filter(|s| subject_ids.contains(&s.id))
        .map(|s| MaxMarksLine {
            max_marks: maxes
                .iter()
                .find(|m| m.subject_id == s.id)
                .map(|m| m.max_marks),
            subject_id: s.id,
            subject_name: s.name,
            subject_code: s.code,
        })
        .collect())
}

async fn get_max_marks(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<MaxMarksQuery>,
) -> ApiResult<Json<Vec<MaxMarksLine>>> {
    user.require(Permission::ExamsRead)?;
    let mut c = state.pool.acquire().await?;
    load_assessment(&mut c, &user, q.assessment_id).await?;
    Ok(Json(
        max_marks_lines(&mut c, user.user.school_id, q.assessment_id, q.class_id).await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct MaxMarksRequest {
    pub assessment_id: Uuid,
    pub class_id: Uuid,
    pub lines: Vec<MaxMarksInput>,
}

#[derive(Debug, Deserialize)]
pub struct MaxMarksInput {
    pub subject_id: Uuid,
    pub max_marks: Decimal,
}

async fn put_max_marks(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<MaxMarksRequest>,
) -> ApiResult<Json<Vec<MaxMarksLine>>> {
    user.require(Permission::AcademicsManage)?;
    let Json(req) = body?;
    let mut tx = state.pool.begin().await?;
    load_assessment(&mut tx, &user, req.assessment_id).await?;
    let class = academics::get_class(&mut tx, req.class_id).await?;
    if class.school_id != user.user.school_id {
        return Err(ApiError::not_found("class"));
    }
    let allowed = academics::class_subject_ids(&mut tx, req.class_id).await?;
    for line in &req.lines {
        if !allowed.contains(&line.subject_id) {
            return Err(ApiError::validation(
                "A subject in the list is not taught in this class.",
            ));
        }
        if line.max_marks < Decimal::ZERO {
            return Err(ApiError::validation("Maximum marks cannot be negative."));
        }
        repo::set_max_marks(
            &mut tx,
            &state.hlc,
            user.actor(),
            user.user.school_id,
            req.assessment_id,
            req.class_id,
            line.subject_id,
            line.max_marks,
        )
        .await?;
    }
    let lines = max_marks_lines(
        &mut tx,
        user.user.school_id,
        req.assessment_id,
        req.class_id,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(lines))
}

// ---- sheets -------------------------------------------------------------------------------

async fn user_name(c: &mut sqlx::PgConnection, id: Option<Uuid>) -> Option<String> {
    match id {
        Some(id) => madrasa_db::users::get(c, id)
            .await
            .ok()
            .map(|u| u.full_name),
        None => None,
    }
}

#[derive(Debug, Deserialize)]
pub struct SheetsQuery {
    pub assessment_id: Uuid,
    pub section_id: Uuid,
}

/// Mirrored by `SheetSummary` in the UI.
#[derive(Debug, Serialize)]
pub struct SheetSummary {
    #[serde(flatten)]
    pub row: SheetSummaryRow,
    pub students: usize,
    pub can_enter: bool,
}

#[derive(Debug, Serialize)]
pub struct SheetsView {
    pub assessment: AssessmentType,
    pub section: SectionHeader,
    pub sheets: Vec<SheetSummary>,
}

async fn list_sheets(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<SheetsQuery>,
) -> ApiResult<Json<SheetsView>> {
    user.require(Permission::ExamsRead)?;
    let mut c = state.pool.acquire().await?;
    let assessment = load_assessment(&mut c, &user, q.assessment_id).await?;
    let section = load_section(&mut c, &user, q.section_id).await?;
    let scope = marks_scope(&mut c, &user).await?;
    let students = repo::roster(&mut c, section.id).await?.len();
    let rows = repo::sheet_summaries(&mut c, assessment.id, section.id).await?;
    let sheets = rows
        .into_iter()
        .map(|row| SheetSummary {
            can_enter: user.can(Permission::ExamsManage)
                && row.max_marks.is_some()
                && allows(&scope, section.id, row.subject_id),
            row,
            students,
        })
        .collect();
    Ok(Json(SheetsView {
        assessment,
        section: section_header(&mut c, &section).await?,
        sheets,
    }))
}

#[derive(Debug, Deserialize)]
pub struct SheetQuery {
    pub assessment_id: Uuid,
    pub section_id: Uuid,
    pub subject_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct SheetStudent {
    pub student_id: Uuid,
    pub student_number: String,
    pub full_name: String,
    pub roll_no: Option<i32>,
    pub obtained: Option<Decimal>,
    pub is_absent: bool,
}

/// Mirrored by `MarksSheetView` in the UI.
#[derive(Debug, Serialize)]
pub struct MarksSheetView {
    pub assessment: AssessmentType,
    pub section: SectionHeader,
    pub subject_id: Uuid,
    pub subject_name: String,
    pub max_marks: Option<Decimal>,
    pub sheet: Option<MarkSheet>,
    pub status: SheetStatus,
    pub submitted_by_name: Option<String>,
    pub approved_by_name: Option<String>,
    pub students: Vec<SheetStudent>,
    pub can_edit: bool,
    pub can_submit: bool,
    pub can_approve: bool,
    pub can_reopen: bool,
}

async fn sheet_view(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    assessment: AssessmentType,
    section: Section,
    subject_id: Uuid,
) -> ApiResult<MarksSheetView> {
    let subject = academics::get_subject(c, subject_id).await?;
    if subject.school_id != user.user.school_id
        || !academics::class_subject_ids(c, section.class_id)
            .await?
            .contains(&subject_id)
    {
        return Err(ApiError::not_found("subject"));
    }
    let max = repo::max_marks(c, assessment.id, section.class_id)
        .await?
        .into_iter()
        .find(|m| m.subject_id == subject_id)
        .map(|m| m.max_marks);
    let sheet = repo::sheet(c, assessment.id, section.id, subject_id).await?;
    let status: SheetStatus = sheet
        .as_ref()
        .and_then(|s| s.status.parse().ok())
        .unwrap_or_default();
    let marks = match &sheet {
        Some(s) => repo::marks(c, s.id).await?,
        None => Vec::new(),
    };
    let submitted_by_name = user_name(c, sheet.as_ref().and_then(|s| s.submitted_by)).await;
    let approved_by_name = user_name(c, sheet.as_ref().and_then(|s| s.approved_by)).await;
    let students = repo::roster(c, section.id)
        .await?
        .into_iter()
        .map(|r| {
            let m = marks.iter().find(|m| m.student_id == r.student_id);
            SheetStudent {
                student_id: r.student_id,
                student_number: r.student_number,
                full_name: r.full_name,
                roll_no: r.roll_no,
                obtained: m.and_then(|m| m.obtained),
                is_absent: m.map(|m| m.is_absent).unwrap_or(false),
            }
        })
        .collect();
    let scope = marks_scope(c, user).await?;
    let in_scope = allows(&scope, section.id, subject_id);
    let manage = user.can(Permission::ExamsManage) && in_scope;
    let approve = user.can(Permission::ExamsApprove);
    Ok(MarksSheetView {
        assessment,
        section: section_header(c, &section).await?,
        subject_id,
        subject_name: subject.name,
        max_marks: max,
        can_edit: manage && max.is_some() && status == SheetStatus::Draft,
        can_submit: manage && sheet.is_some() && status == SheetStatus::Draft,
        can_approve: approve && status == SheetStatus::Submitted,
        can_reopen: approve && status != SheetStatus::Draft,
        sheet,
        status,
        submitted_by_name,
        approved_by_name,
        students,
    })
}

async fn get_sheet(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<SheetQuery>,
) -> ApiResult<Json<MarksSheetView>> {
    user.require(Permission::ExamsRead)?;
    let mut c = state.pool.acquire().await?;
    let assessment = load_assessment(&mut c, &user, q.assessment_id).await?;
    let section = load_section(&mut c, &user, q.section_id).await?;
    Ok(Json(
        sheet_view(&mut c, &user, assessment, section, q.subject_id).await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct MarkInputRequest {
    pub student_id: Uuid,
    pub obtained: Option<Decimal>,
    #[serde(default)]
    pub is_absent: bool,
}

/// Mirrored by `SaveMarksRequest` in the UI.
#[derive(Debug, Deserialize)]
pub struct SaveMarksRequest {
    pub assessment_id: Uuid,
    pub section_id: Uuid,
    pub subject_id: Uuid,
    pub marks: Vec<MarkInputRequest>,
}

async fn put_sheet(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<SaveMarksRequest>,
) -> ApiResult<Json<MarksSheetView>> {
    user.require(Permission::ExamsManage)?;
    let Json(req) = body?;
    let mut tx = state.pool.begin().await?;
    let assessment = load_assessment(&mut tx, &user, req.assessment_id).await?;
    let section = load_section(&mut tx, &user, req.section_id).await?;
    let scope = marks_scope(&mut tx, &user).await?;
    if !allows(&scope, section.id, req.subject_id) {
        return Err(ApiError::forbidden());
    }
    let input: Vec<repo::MarkInput> = req
        .marks
        .iter()
        .map(|m| repo::MarkInput {
            student_id: m.student_id,
            obtained: m.obtained,
            is_absent: m.is_absent,
        })
        .collect();
    repo::save_marks(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        assessment.id,
        section.id,
        req.subject_id,
        &input,
    )
    .await?;
    let view = sheet_view(&mut tx, &user, assessment, section, req.subject_id).await?;
    tx.commit().await?;
    Ok(Json(view))
}

async fn sheet_by_id(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    id: Uuid,
) -> ApiResult<(MarkSheet, AssessmentType, Section)> {
    let sheet = repo::get_sheet(c, id).await?;
    if sheet.school_id != user.user.school_id {
        return Err(ApiError::not_found("marks sheet"));
    }
    let assessment = repo::get_assessment(c, sheet.assessment_type_id).await?;
    let section = load_section(c, user, sheet.section_id).await?;
    Ok((sheet, assessment, section))
}

async fn submit_sheet(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<MarksSheetView>> {
    user.require(Permission::ExamsManage)?;
    let mut tx = state.pool.begin().await?;
    let (sheet, assessment, section) = sheet_by_id(&mut tx, &user, id).await?;
    let scope = marks_scope(&mut tx, &user).await?;
    if !allows(&scope, section.id, sheet.subject_id) {
        return Err(ApiError::forbidden());
    }
    repo::submit_sheet(&mut tx, &state.hlc, user.actor(), id, user.user.id).await?;
    let view = sheet_view(&mut tx, &user, assessment, section, sheet.subject_id).await?;
    tx.commit().await?;
    Ok(Json(view))
}

async fn approve_sheet(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<MarksSheetView>> {
    user.require(Permission::ExamsApprove)?;
    let mut tx = state.pool.begin().await?;
    let (sheet, assessment, section) = sheet_by_id(&mut tx, &user, id).await?;
    repo::approve_sheet(&mut tx, &state.hlc, user.actor(), id, user.user.id).await?;
    let view = sheet_view(&mut tx, &user, assessment, section, sheet.subject_id).await?;
    tx.commit().await?;
    Ok(Json(view))
}

async fn reopen_sheet(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<MarksSheetView>> {
    user.require(Permission::ExamsApprove)?;
    let mut tx = state.pool.begin().await?;
    let (sheet, assessment, section) = sheet_by_id(&mut tx, &user, id).await?;
    repo::reopen_sheet(&mut tx, &state.hlc, user.actor(), id).await?;
    let view = sheet_view(&mut tx, &user, assessment, section, sheet.subject_id).await?;
    tx.commit().await?;
    Ok(Json(view))
}

// ---- results ------------------------------------------------------------------------------

#[derive(Debug, Serialize, Clone)]
pub struct SubjectHeader {
    pub subject_id: Uuid,
    pub subject_name: String,
    pub max_marks: Decimal,
    pub sheet_status: SheetStatus,
}

#[derive(Debug, Serialize, Clone)]
pub struct SubjectMark {
    pub subject_id: Uuid,
    pub obtained: Option<Decimal>,
    pub is_absent: bool,
    pub percent: Decimal,
    pub grade: String,
}

#[derive(Debug, Serialize)]
pub struct ResultRow {
    pub student_id: Uuid,
    pub student_number: String,
    pub full_name: String,
    pub roll_no: Option<i32>,
    pub marks: Vec<SubjectMark>,
    pub total: Decimal,
    pub max: Decimal,
    pub percent: Decimal,
    pub grade: String,
    pub pass: bool,
    pub position: u32,
}

#[derive(Debug, Serialize)]
pub struct SubjectAnalysis {
    pub subject_id: Uuid,
    pub subject_name: String,
    pub max_marks: Decimal,
    pub appeared: u32,
    pub absent: u32,
    pub average_percent: Option<Decimal>,
    pub highest: Option<Decimal>,
    pub lowest: Option<Decimal>,
    pub passed: u32,
    pub failed: u32,
}

/// Mirrored by `ResultSheet` in the UI.
#[derive(Debug, Serialize)]
pub struct ResultSheet {
    pub assessment: AssessmentType,
    pub section: SectionHeader,
    pub subjects: Vec<SubjectHeader>,
    pub rows: Vec<ResultRow>,
    pub analysis: Vec<SubjectAnalysis>,
    pub grading: GradingScheme,
    /// Every subject's sheet is approved.
    pub complete: bool,
}

fn grade_of(scheme: &GradingScheme, pct: Decimal) -> String {
    scheme
        .grade_for(pct)
        .map(|b| b.grade.clone())
        .unwrap_or_default()
}

async fn build_result_sheet(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    assessment: AssessmentType,
    section: Section,
) -> ApiResult<ResultSheet> {
    let grading = repo::grading(c, user.user.school_id).await?;
    let subjects_all = academics::list_subjects(c, user.user.school_id).await?;
    let summaries = repo::sheet_summaries(c, assessment.id, section.id).await?;
    let subjects: Vec<SubjectHeader> = summaries
        .iter()
        .filter_map(|s| {
            s.max_marks.map(|max| SubjectHeader {
                subject_id: s.subject_id,
                subject_name: s.subject_name.clone(),
                max_marks: max,
                sheet_status: s
                    .status
                    .as_deref()
                    .and_then(|st| st.parse().ok())
                    .unwrap_or_default(),
            })
        })
        .collect();
    let _ = subjects_all;
    let rows_raw = repo::section_marks(c, assessment.id, section.id).await?;
    let by_key: HashMap<(Uuid, Uuid), &MarkRow> = rows_raw
        .iter()
        .map(|r| ((r.student_id, r.subject_id), r))
        .collect();
    let roster = repo::roster(c, section.id).await?;
    let mut rows: Vec<ResultRow> = roster
        .into_iter()
        .map(|r| {
            let marks: Vec<SubjectMark> = subjects
                .iter()
                .map(|s| {
                    let m = by_key.get(&(r.student_id, s.subject_id));
                    let obtained = m.and_then(|m| m.obtained);
                    let is_absent = m.map(|m| m.is_absent).unwrap_or(false);
                    let pct = exams::percent(obtained.unwrap_or(Decimal::ZERO), s.max_marks);
                    SubjectMark {
                        subject_id: s.subject_id,
                        obtained,
                        is_absent,
                        percent: pct,
                        grade: grade_of(&grading, pct),
                    }
                })
                .collect();
            let totals = exams::totals(subjects.iter().zip(&marks).map(|(s, m)| Score {
                max: s.max_marks,
                obtained: m.obtained,
            }));
            ResultRow {
                student_id: r.student_id,
                student_number: r.student_number,
                full_name: r.full_name,
                roll_no: r.roll_no,
                marks,
                total: totals.obtained,
                max: totals.max,
                grade: grade_of(&grading, totals.percent),
                pass: grading.is_pass(totals.percent),
                percent: totals.percent,
                position: 0,
            }
        })
        .collect();
    let positions = exams::positions(&rows.iter().map(|r| r.percent).collect::<Vec<_>>());
    for (row, pos) in rows.iter_mut().zip(positions) {
        row.position = pos;
    }
    let analysis = subjects
        .iter()
        .map(|s| {
            let mut appeared = 0u32;
            let mut absent = 0u32;
            let mut passed = 0u32;
            let mut sum = Decimal::ZERO;
            let mut highest: Option<Decimal> = None;
            let mut lowest: Option<Decimal> = None;
            for row in &rows {
                let m = row
                    .marks
                    .iter()
                    .find(|m| m.subject_id == s.subject_id)
                    .unwrap();
                if m.is_absent {
                    absent += 1;
                } else if let Some(o) = m.obtained {
                    appeared += 1;
                    sum += m.percent;
                    highest = Some(highest.map_or(o, |h| h.max(o)));
                    lowest = Some(lowest.map_or(o, |l| l.min(o)));
                    if grading.is_pass(m.percent) {
                        passed += 1;
                    }
                }
            }
            SubjectAnalysis {
                subject_id: s.subject_id,
                subject_name: s.subject_name.clone(),
                max_marks: s.max_marks,
                appeared,
                absent,
                average_percent: (appeared > 0)
                    .then(|| exams::round2(sum / Decimal::from(appeared))),
                highest,
                lowest,
                passed,
                failed: appeared - passed,
            }
        })
        .collect();
    let complete = !subjects.is_empty()
        && subjects
            .iter()
            .all(|s| s.sheet_status == SheetStatus::Approved);
    Ok(ResultSheet {
        assessment,
        section: section_header(c, &section).await?,
        subjects,
        rows,
        analysis,
        grading,
        complete,
    })
}

/// Result sheet of a section for a caller (permission already checked), used by exports.
pub(crate) async fn result_sheet_for(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    assessment_id: Uuid,
    section_id: Uuid,
) -> ApiResult<ResultSheet> {
    let assessment = load_assessment(c, user, assessment_id).await?;
    let section = load_section(c, user, section_id).await?;
    build_result_sheet(c, user, assessment, section).await
}

async fn section_results(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<SheetsQuery>,
) -> ApiResult<Json<ResultSheet>> {
    user.require(Permission::ExamsRead)?;
    let mut c = state.pool.acquire().await?;
    let assessment = load_assessment(&mut c, &user, q.assessment_id).await?;
    let section = load_section(&mut c, &user, q.section_id).await?;
    Ok(Json(
        build_result_sheet(&mut c, &user, assessment, section).await?,
    ))
}

// ---- report cards -------------------------------------------------------------------------

#[derive(Debug, Serialize, Clone)]
pub struct AssessmentHeader {
    pub id: Uuid,
    pub name: String,
    pub weight: Decimal,
    /// The student has marks in at least one subject of this assessment.
    pub held: bool,
}

#[derive(Debug, Serialize)]
pub struct CardCell {
    pub max_marks: Option<Decimal>,
    pub obtained: Option<Decimal>,
    pub is_absent: bool,
    pub percent: Option<Decimal>,
    pub approved: bool,
}

#[derive(Debug, Serialize)]
pub struct CardSubject {
    pub subject_id: Uuid,
    pub subject_name: String,
    /// One per assessment, in order.
    pub cells: Vec<CardCell>,
    pub weighted_percent: Option<Decimal>,
    pub grade: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CardAssessmentTotal {
    pub assessment_id: Uuid,
    pub total: Decimal,
    pub max: Decimal,
    pub percent: Option<Decimal>,
    pub grade: Option<String>,
    pub position: Option<u32>,
    pub students: usize,
}

#[derive(Debug, Serialize)]
pub struct SchoolHeader {
    pub name: String,
    pub code: String,
    pub city: String,
}

#[derive(Debug, Serialize)]
pub struct AttendanceLine {
    pub present: u32,
    pub total: u32,
    pub percent: Option<f64>,
}

/// Mirrored by `ReportCard` in the UI.
#[derive(Debug, Serialize)]
pub struct ReportCard {
    pub school: SchoolHeader,
    pub academic_year: String,
    pub student_id: Uuid,
    pub student_number: String,
    pub full_name: String,
    pub father_name: String,
    pub class_name: Option<String>,
    pub section_name: Option<String>,
    pub roll_no: Option<i32>,
    pub assessments: Vec<AssessmentHeader>,
    pub subjects: Vec<CardSubject>,
    pub totals: Vec<CardAssessmentTotal>,
    pub overall_percent: Option<Decimal>,
    pub overall_grade: Option<String>,
    pub overall_remarks: Option<String>,
    pub pass: Option<bool>,
    pub attendance: AttendanceLine,
    pub grading: GradingScheme,
}

async fn build_report_card(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    student_id: Uuid,
    year: &madrasa_db::models::AcademicYear,
    section: Option<&Section>,
    positions_cache: &mut HashMap<Uuid, HashMap<Uuid, (u32, usize)>>,
) -> ApiResult<ReportCard> {
    let student = students::get(c, student_id).await?;
    let school = schools::get(c, user.user.school_id).await?;
    let grading = repo::grading(c, user.user.school_id).await?;
    let assessments_all = repo::list_assessments(c, year.id).await?;
    let rows = repo::student_marks(c, student_id, year.id).await?;
    let subjects_all = academics::list_subjects(c, user.user.school_id).await?;
    let (class_name, section_name, roll_no) = match section {
        Some(s) => {
            let class = academics::get_class(c, s.class_id).await?;
            let roll = repo::roster(c, s.id)
                .await?
                .into_iter()
                .find(|r| r.student_id == student_id)
                .and_then(|r| r.roll_no);
            (Some(class.name), Some(s.name.clone()), roll)
        }
        None => (None, None, None),
    };
    let assessments: Vec<AssessmentHeader> = assessments_all
        .iter()
        .map(|a| AssessmentHeader {
            id: a.id,
            name: a.name.clone(),
            weight: a.weight,
            held: rows.iter().any(|r| r.assessment_type_id == a.id),
        })
        .collect();
    let mut subject_ids: Vec<Uuid> = rows.iter().map(|r| r.subject_id).collect();
    if let Some(s) = section {
        subject_ids.extend(academics::class_subject_ids(c, s.class_id).await?);
    }
    subject_ids.sort();
    subject_ids.dedup();
    let mut subjects: Vec<CardSubject> = subject_ids
        .iter()
        .filter_map(|sid| {
            let name = subjects_all.iter().find(|s| s.id == *sid)?.name.clone();
            let cells: Vec<CardCell> = assessments
                .iter()
                .map(|a| {
                    match rows
                        .iter()
                        .find(|r| r.assessment_type_id == a.id && r.subject_id == *sid)
                    {
                        Some(r) => CardCell {
                            max_marks: Some(r.max_marks),
                            obtained: r.obtained,
                            is_absent: r.is_absent,
                            percent: Some(exams::percent(
                                r.obtained.unwrap_or(Decimal::ZERO),
                                r.max_marks,
                            )),
                            approved: r.sheet_status == SheetStatus::Approved.as_str(),
                        },
                        None => CardCell {
                            max_marks: None,
                            obtained: None,
                            is_absent: false,
                            percent: None,
                            approved: false,
                        },
                    }
                })
                .collect();
            let parts: Vec<(Decimal, Score)> = assessments
                .iter()
                .zip(&cells)
                .filter_map(|(a, cell)| {
                    cell.max_marks.map(|max| {
                        (
                            a.weight,
                            Score {
                                max,
                                obtained: cell.obtained,
                            },
                        )
                    })
                })
                .collect();
            let weighted = exams::weighted_percent(&parts);
            Some(CardSubject {
                subject_id: *sid,
                subject_name: name,
                cells,
                weighted_percent: weighted,
                grade: weighted.map(|w| grade_of(&grading, w)),
            })
        })
        .collect();
    subjects.sort_by(|a, b| a.subject_name.cmp(&b.subject_name));

    let mut totals = Vec::new();
    for a in &assessments {
        let scores: Vec<Score> = subjects
            .iter()
            .filter_map(|s| {
                let i = assessments.iter().position(|x| x.id == a.id)?;
                let cell = &s.cells[i];
                cell.max_marks.map(|max| Score {
                    max,
                    obtained: cell.obtained,
                })
            })
            .collect();
        let held = !scores.is_empty();
        let t = exams::totals(scores);
        let (position, count) = match (section, held) {
            (Some(sec), true) => {
                let ranks = match positions_cache.get(&a.id) {
                    Some(r) => r,
                    None => {
                        let assessment = repo::get_assessment(c, a.id).await?;
                        let sheet = build_result_sheet(c, user, assessment, sec.clone()).await?;
                        let map: HashMap<Uuid, (u32, usize)> = sheet
                            .rows
                            .iter()
                            .map(|r| (r.student_id, (r.position, sheet.rows.len())))
                            .collect();
                        positions_cache.entry(a.id).or_insert(map)
                    }
                };
                ranks
                    .get(&student_id)
                    .map(|(p, n)| (Some(*p), *n))
                    .unwrap_or((None, 0))
            }
            _ => (None, 0),
        };
        totals.push(CardAssessmentTotal {
            assessment_id: a.id,
            total: t.obtained,
            max: t.max,
            percent: held.then_some(t.percent),
            grade: held.then(|| grade_of(&grading, t.percent)),
            position,
            students: count,
        });
    }
    let overall_parts: Vec<(Decimal, Score)> = assessments
        .iter()
        .zip(&totals)
        .filter(|(_, t)| t.max > Decimal::ZERO)
        .map(|(a, t)| {
            (
                a.weight,
                Score {
                    max: t.max,
                    obtained: Some(t.total),
                },
            )
        })
        .collect();
    let overall = exams::weighted_percent(&overall_parts);
    let band = overall.and_then(|o| grading.grade_for(o).cloned());
    let att = attendance::student_summary(c, student_id, year.start_date, year.end_date).await?;
    Ok(ReportCard {
        school: SchoolHeader {
            name: school.name,
            code: school.code,
            city: school.city,
        },
        academic_year: year.name.clone(),
        student_id,
        student_number: student.student_number,
        full_name: student.full_name,
        father_name: student.father_name,
        class_name,
        section_name,
        roll_no,
        assessments,
        subjects,
        totals,
        overall_percent: overall,
        overall_grade: band.as_ref().map(|b| b.grade.clone()),
        overall_remarks: band.as_ref().map(|b| b.remarks.clone()),
        pass: overall.map(|o| grading.is_pass(o)),
        attendance: AttendanceLine {
            present: att.present + att.late,
            total: att.total(),
            percent: att.percent(),
        },
        grading,
    })
}

#[derive(Debug, Deserialize)]
pub struct CardQuery {
    pub academic_year_id: Option<Uuid>,
}

async fn student_report_card(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Query(q): Query<CardQuery>,
) -> ApiResult<Json<ReportCard>> {
    user.require(Permission::ExamsRead)?;
    let mut c = state.pool.acquire().await?;
    let student = students::get(&mut c, id).await?;
    if student.school_id != user.user.school_id {
        return Err(ApiError::not_found("student"));
    }
    let year = match q.academic_year_id {
        Some(y) => year_of(&mut c, &user, y).await?,
        None => academics::active_year(&mut c, user.user.school_id)
            .await?
            .ok_or_else(|| ApiError::validation("No active academic year."))?,
    };
    let section = match students::section_in_year(&mut c, id, year.id).await? {
        Some(sid) => Some(load_section(&mut c, &user, sid).await?),
        None => {
            if user.section_scope(&mut c).await?.is_some() {
                return Err(ApiError::forbidden());
            }
            None
        }
    };
    let mut cache = HashMap::new();
    Ok(Json(
        build_report_card(&mut c, &user, id, &year, section.as_ref(), &mut cache).await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct SectionCardsQuery {
    pub section_id: Uuid,
}

async fn section_report_cards(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<SectionCardsQuery>,
) -> ApiResult<Json<Vec<ReportCard>>> {
    user.require(Permission::ExamsRead)?;
    let mut c = state.pool.acquire().await?;
    let section = load_section(&mut c, &user, q.section_id).await?;
    let year = year_of(&mut c, &user, section.academic_year_id).await?;
    let mut cache = HashMap::new();
    let mut out = Vec::new();
    for r in repo::roster(&mut c, section.id).await? {
        out.push(
            build_report_card(
                &mut c,
                &user,
                r.student_id,
                &year,
                Some(&section),
                &mut cache,
            )
            .await?,
        );
    }
    Ok(Json(out))
}
