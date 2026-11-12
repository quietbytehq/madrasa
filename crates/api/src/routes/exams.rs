//! Exams: assessment setup, grading scheme, maximum marks, marks entry with the submit →
//! approve workflow, section result sheets with analysis, and report cards.
//!
//! Setup (assessments, weights, maximum marks, grading) needs `academics.manage`. Marks
//! entry needs `exams.manage` and, for teachers, an assignment to the section and subject (or
//! being its class teacher). Approval and reopening need `exams.approve`.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use madrasa_core::exams::{self, GradingScheme, SheetStatus};
use madrasa_core::permissions::Permission;
use madrasa_db::models::{AssessmentType, MarkSheet, Section, SheetSummaryRow};
use madrasa_db::{academics, exams as repo, schools, staff, students};
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
        .route("/exams/report-cards/pdf", get(report_cards_pdf))
        .route("/exams/sheets", get(list_sheets))
        .route("/exams/sheet", get(get_sheet).put(put_sheet))
        .route("/exams/sheet/template", get(sheet_template))
        .route("/exams/sheet/upload", post(upload_sheet))
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

/// Marks are entered only for entered components; a calculated one has no sheets.
async fn load_entered_assessment(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    id: Uuid,
) -> ApiResult<AssessmentType> {
    let a = load_assessment(c, user, id).await?;
    if a.kind == "calculated" {
        return Err(ApiError::validation(
            "This component is calculated from others; it has no marks sheet.",
        ));
    }
    Ok(a)
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
    /// The term the component belongs to (defaults to the year's first term).
    #[serde(default)]
    pub term_id: Option<Uuid>,
    /// `entered` (default) or `calculated`.
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub formula: Option<madrasa_core::exams::Formula>,
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
    let kind = req.kind.clone().unwrap_or_else(|| "entered".into());
    if !["entered", "calculated"].contains(&kind.as_str()) {
        return Err(ApiError::validation("kind must be entered or calculated."));
    }
    let mut c = state.pool.acquire().await?;
    year_of(&mut c, &user, req.academic_year_id).await?;
    let term_id = match req.term_id {
        Some(t) => {
            let term = repo::get_term(&mut c, t).await?;
            if term.academic_year_id != req.academic_year_id {
                return Err(ApiError::validation("The term belongs to another year."));
            }
            Some(t)
        }
        None => repo::list_terms(&mut c, req.academic_year_id)
            .await?
            .first()
            .map(|t| t.id),
    };
    let formula = if kind == "calculated" {
        let f = req
            .formula
            .clone()
            .ok_or_else(|| ApiError::validation("A calculated component needs a formula."))?;
        super::exam_terms::validate_formula(&mut c, req.academic_year_id, term_id, None, &f)
            .await?;
        Some(serde_json::to_value(f).map_err(|_| ApiError::internal())?)
    } else {
        None
    };
    let a = repo::create_component(
        &mut c,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        req.academic_year_id,
        repo::NewComponent {
            name: req.name,
            weight: req.weight,
            position: req.position,
            term_id,
            kind,
            formula,
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(a)))
}

#[derive(Debug, Deserialize)]
pub struct DefaultsRequest {
    pub academic_year_id: Uuid,
    /// 1 = one term "Year" with the four assessments (as before); 2 = two terms with the
    /// assessments split between them.
    #[serde(default)]
    pub terms: Option<u8>,
}

async fn default_assessments(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<DefaultsRequest>,
) -> ApiResult<Json<Vec<AssessmentType>>> {
    user.require(Permission::AcademicsManage)?;
    let Json(req) = body?;
    let mut c = state.pool.acquire().await?;
    year_of(&mut c, &user, req.academic_year_id).await?;
    Ok(Json(
        repo::create_default_structure(
            &mut c,
            &state.hlc,
            user.actor(),
            user.user.school_id,
            req.academic_year_id,
            req.terms.unwrap_or(1),
        )
        .await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct RenameRequest {
    pub name: String,
    #[serde(default)]
    pub position: i32,
    #[serde(default)]
    pub term_id: Option<Uuid>,
    #[serde(default)]
    pub formula: Option<madrasa_core::exams::Formula>,
}

async fn rename_assessment(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: JsonBody<RenameRequest>,
) -> ApiResult<Json<AssessmentType>> {
    user.require(Permission::AcademicsManage)?;
    let Json(req) = body?;
    if req.name.trim().is_empty() {
        return Err(ApiError::validation("Assessment name is required."));
    }
    let mut c = state.pool.acquire().await?;
    let existing = load_assessment(&mut c, &user, id).await?;
    let term_id = match req.term_id {
        Some(t) => {
            let term = repo::get_term(&mut c, t).await?;
            if term.academic_year_id != existing.academic_year_id {
                return Err(ApiError::validation("The term belongs to another year."));
            }
            Some(t)
        }
        None => existing.term_id,
    };
    let formula = if existing.kind == "calculated" {
        let f = req
            .formula
            .clone()
            .or_else(|| {
                existing
                    .formula
                    .clone()
                    .and_then(|v| serde_json::from_value(v).ok())
            })
            .ok_or_else(|| ApiError::validation("A calculated component needs a formula."))?;
        super::exam_terms::validate_formula(
            &mut c,
            existing.academic_year_id,
            term_id,
            Some(id),
            &f,
        )
        .await?;
        Some(serde_json::to_value(f).map_err(|_| ApiError::internal())?)
    } else {
        None
    };
    Ok(Json(
        repo::update_component(
            &mut c,
            &state.hlc,
            user.actor(),
            id,
            &req.name,
            req.position,
            term_id,
            formula,
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
    load_entered_assessment(&mut c, &user, q.assessment_id).await?;
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
    load_entered_assessment(&mut tx, &user, req.assessment_id).await?;
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
    let assessment = load_entered_assessment(&mut c, &user, q.assessment_id).await?;
    let section = load_section(&mut c, &user, q.section_id).await?;
    let scope = marks_scope(&mut c, &user).await?;
    let takes = madrasa_db::subjects::effective_for_section(&mut c, section.id).await?;
    let rows = repo::sheet_summaries(&mut c, assessment.id, section.id).await?;
    let sheets = rows
        .into_iter()
        .map(|row| {
            let students = takes
                .values()
                .filter(|ids| ids.contains(&row.subject_id))
                .count();
            SheetSummary {
                can_enter: user.can(Permission::ExamsManage)
                    && row.max_marks.is_some()
                    && allows(&scope, section.id, row.subject_id),
                row,
                students,
            }
        })
        // A subject nobody in the section takes has no sheet (unless one already exists).
        .filter(|s| s.students > 0 || s.row.sheet_id.is_some())
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
    let students = repo::subject_roster(c, section.id, subject_id)
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
    let assessment = load_entered_assessment(&mut c, &user, q.assessment_id).await?;
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
    Ok(Json(save_sheet(&state, &user, req).await?))
}

/// Save a draft marks sheet (on-screen entry and Excel upload share this): scope check, then
/// the repository's validation (max marks, roster) and audit.
async fn save_sheet(
    state: &AppState,
    user: &CurrentUser,
    req: SaveMarksRequest,
) -> ApiResult<MarksSheetView> {
    let mut tx = state.pool.begin().await?;
    let assessment = load_entered_assessment(&mut tx, user, req.assessment_id).await?;
    let section = load_section(&mut tx, user, req.section_id).await?;
    let scope = marks_scope(&mut tx, user).await?;
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
    let view = sheet_view(&mut tx, user, assessment, section, req.subject_id).await?;
    tx.commit().await?;
    Ok(view)
}

// ---- marks via Excel ----------------------------------------------------------------------

const TEMPLATE_HEADERS: [&str; 5] = [
    "Student number",
    "Student name",
    "Roll no",
    "Marks",
    "Absent",
];

/// An Excel sheet for one section, subject and assessment: student numbers and names filled
/// in, the maximum marks in the heading, a Marks column and an Absent column (Y). Anyone who
/// may read the sheet may download it; uploading needs `exams.manage` and the teacher scope.
async fn sheet_template(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<SheetQuery>,
) -> ApiResult<Response> {
    user.require(Permission::ExamsRead)?;
    let mut c = state.pool.acquire().await?;
    let assessment = load_entered_assessment(&mut c, &user, q.assessment_id).await?;
    let section = load_section(&mut c, &user, q.section_id).await?;
    let view = sheet_view(&mut c, &user, assessment, section, q.subject_id).await?;
    let max = view
        .max_marks
        .map(|m| m.normalize().to_string())
        .unwrap_or_else(|| "?".into());
    let headers: Vec<String> = TEMPLATE_HEADERS
        .iter()
        .map(|h| match *h {
            "Marks" => format!("Marks (max {max})"),
            "Absent" => "Absent (Y)".to_string(),
            other => other.to_string(),
        })
        .collect();
    let rows: Vec<Vec<String>> = view
        .students
        .iter()
        .map(|s| {
            vec![
                s.student_number.clone(),
                s.full_name.clone(),
                s.roll_no.map(|r| r.to_string()).unwrap_or_default(),
                if s.is_absent {
                    String::new()
                } else {
                    s.obtained
                        .map(|o| o.normalize().to_string())
                        .unwrap_or_default()
                },
                if s.is_absent {
                    "Y".into()
                } else {
                    String::new()
                },
            ]
        })
        .collect();
    let name = format!(
        "marks-{}-{}-{}-{}",
        view.section.class_name, view.section.section_name, view.subject_name, view.assessment.name
    )
    .chars()
    .map(|c| {
        if c.is_alphanumeric() || c == '-' {
            c
        } else {
            '_'
        }
    })
    .collect::<String>();
    let (file_name, bytes) = crate::export::build(&name, "xlsx", &headers, &rows)?;
    Ok(crate::export::file_response(&file_name, bytes))
}

/// Mirrored by `UploadMarksRequest` in the UI.
#[derive(Debug, Deserialize)]
pub struct UploadMarksRequest {
    pub assessment_id: Uuid,
    pub section_id: Uuid,
    pub subject_id: Uuid,
    pub file_name: String,
    pub content_base64: String,
}

/// One row the upload could not use.
#[derive(Debug, Clone, Serialize)]
pub struct UploadIssue {
    pub row_no: usize,
    pub message: String,
}

/// Mirrored by `UploadMarksResult` in the UI.
#[derive(Debug, Serialize)]
pub struct UploadMarksResult {
    pub sheet: Option<MarksSheetView>,
    pub saved: usize,
    pub issues: Vec<UploadIssue>,
}

/// Read a filled template and save it as a draft sheet, through the same checks as on-screen
/// entry (scope, roster, maximum marks). Nothing is saved when any row is wrong; the issues
/// come back with row numbers so the sheet can be corrected and uploaded again. Submission
/// and approval then work as usual.
async fn upload_sheet(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<UploadMarksRequest>,
) -> ApiResult<Json<UploadMarksResult>> {
    user.require(Permission::ExamsManage)?;
    let Json(req) = body?;
    let (view, max) = {
        let mut c = state.pool.acquire().await?;
        let assessment = load_entered_assessment(&mut c, &user, req.assessment_id).await?;
        let section = load_section(&mut c, &user, req.section_id).await?;
        let scope = marks_scope(&mut c, &user).await?;
        if !allows(&scope, section.id, req.subject_id) {
            return Err(ApiError::forbidden());
        }
        let view = sheet_view(&mut c, &user, assessment, section, req.subject_id).await?;
        if !view.can_edit {
            return Err(ApiError::validation(
                "This sheet is not open for entry (submitted or approved). Reopen it first.",
            ));
        }
        let max = view.max_marks;
        (view, max)
    };
    let (_, _, headers, rows) = crate::imports::parse_file(
        madrasa_core::imports::ImportKind::Students,
        &req.file_name,
        &req.content_base64,
        None,
    )?;
    let col = |name: &str| {
        headers
            .iter()
            .position(|h| h.trim().to_lowercase().starts_with(name))
    };
    let (Some(c_number), Some(c_marks)) = (col("student number"), col("marks")) else {
        return Err(ApiError::validation(
            "Use the downloaded template: it needs the \"Student number\" and \"Marks\" columns.",
        ));
    };
    let c_absent = col("absent");
    let mut issues = Vec::new();
    let mut marks = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (i, row) in rows.iter().enumerate() {
        let row_no = i + 2;
        let number = row.get(c_number).map(|s| s.trim()).unwrap_or("");
        if number.is_empty() {
            continue;
        }
        let Some(student) = view
            .students
            .iter()
            .find(|s| s.student_number.eq_ignore_ascii_case(number))
        else {
            issues.push(UploadIssue {
                row_no,
                message: format!("Student {number} is not in this section"),
            });
            continue;
        };
        if !seen.insert(student.student_id) {
            issues.push(UploadIssue {
                row_no,
                message: format!("Student {number} appears twice"),
            });
            continue;
        }
        let absent = c_absent
            .and_then(|c| row.get(c))
            .map(|v| {
                matches!(
                    v.trim().to_lowercase().as_str(),
                    "y" | "yes" | "a" | "absent" | "1"
                )
            })
            .unwrap_or(false);
        let raw = row.get(c_marks).map(|s| s.trim()).unwrap_or("");
        let obtained = if absent || raw.is_empty() {
            None
        } else {
            match raw.parse::<Decimal>() {
                Ok(d) if d >= Decimal::ZERO && max.is_none_or(|m| d <= m) => Some(d),
                Ok(_) => {
                    issues.push(UploadIssue {
                        row_no,
                        message: format!(
                            "Marks for {number} must be between 0 and {}",
                            max.map(|m| m.normalize().to_string())
                                .unwrap_or_else(|| "the maximum".into())
                        ),
                    });
                    continue;
                }
                Err(_) => {
                    issues.push(UploadIssue {
                        row_no,
                        message: format!("Marks for {number} are not a number: {raw}"),
                    });
                    continue;
                }
            }
        };
        if obtained.is_none() && !absent {
            // Blank marks: leave the student untouched rather than recording zero.
            continue;
        }
        marks.push(MarkInputRequest {
            student_id: student.student_id,
            obtained,
            is_absent: absent,
        });
    }
    if !issues.is_empty() {
        return Ok(Json(UploadMarksResult {
            sheet: None,
            saved: 0,
            issues,
        }));
    }
    if marks.is_empty() {
        return Err(ApiError::validation(
            "The file has no marks to save. Fill the Marks column (or Y under Absent).",
        ));
    }
    let saved = marks.len();
    let sheet = save_sheet(
        &state,
        &user,
        SaveMarksRequest {
            assessment_id: req.assessment_id,
            section_id: req.section_id,
            subject_id: req.subject_id,
            marks,
        },
    )
    .await?;
    Ok(Json(UploadMarksResult {
        sheet: Some(sheet),
        saved,
        issues: Vec::new(),
    }))
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

pub use madrasa_db::results::{
    build_report_card, build_result_sheet, evaluate_rows, grade_of, marks_for, section_header,
    AssessmentHeader, AttendanceLine, CardAssessmentTotal, CardCell, CardSubject, CardTerm,
    ReportCard, ResultRow, ResultSheet, SchoolHeader, SectionHeader, SubjectAnalysis,
    SubjectHeader, SubjectMark,
};

/// Result sheet of a section for a caller (permission already checked), used by exports.
pub(crate) async fn result_sheet_for(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    assessment_id: Uuid,
    section_id: Uuid,
) -> ApiResult<ResultSheet> {
    let assessment = load_assessment(c, user, assessment_id).await?;
    let section = load_section(c, user, section_id).await?;
    Ok(build_result_sheet(c, user.user.school_id, assessment, section).await?)
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
        build_result_sheet(&mut c, user.user.school_id, assessment, section).await?,
    ))
}

// ---- report cards -------------------------------------------------------------------------

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
        build_report_card(
            &mut c,
            user.user.school_id,
            id,
            &year,
            section.as_ref(),
            &mut cache,
        )
        .await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct SectionCardsQuery {
    pub section_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct CardsPdfQuery {
    pub section_id: Option<Uuid>,
    pub student_id: Option<Uuid>,
    pub academic_year_id: Option<Uuid>,
    #[serde(default)]
    pub lang: Option<String>,
}

/// Report cards as a PDF typeset like the reports (v0.2.0-alpha B3): every card of a
/// section, or one student's card. Same access rules as the JSON cards.
async fn report_cards_pdf(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<CardsPdfQuery>,
) -> ApiResult<axum::response::Response> {
    user.require(Permission::ExamsRead)?;
    let mut c = state.pool.acquire().await?;
    let year = match q.academic_year_id {
        Some(y) => year_of(&mut c, &user, y).await?,
        None => academics::active_year(&mut c, user.user.school_id)
            .await?
            .ok_or_else(|| ApiError::validation("No active academic year."))?,
    };
    let lang = match q.lang.as_deref() {
        Some("ur") => crate::reports::Lang::Ur,
        _ => crate::reports::Lang::En,
    };
    let (cards, stem) = match (q.student_id, q.section_id) {
        (Some(student_id), _) => {
            let student = students::get(&mut c, student_id).await?;
            if student.school_id != user.user.school_id {
                return Err(ApiError::not_found("student"));
            }
            let section = match students::section_in_year(&mut c, student_id, year.id).await? {
                Some(sid) => Some(load_section(&mut c, &user, sid).await?),
                None => {
                    if user.section_scope(&mut c).await?.is_some() {
                        return Err(ApiError::forbidden());
                    }
                    None
                }
            };
            let mut cache = HashMap::new();
            let card = build_report_card(
                &mut c,
                user.user.school_id,
                student_id,
                &year,
                section.as_ref(),
                &mut cache,
            )
            .await?;
            (
                vec![card],
                format!("report-card-{}", student.student_number),
            )
        }
        (None, Some(section_id)) => {
            let section = load_section(&mut c, &user, section_id).await?;
            let header = section_header(&mut c, &section).await?;
            let cards = section_cards_for(&mut c, &user, &year, &section).await?;
            (
                cards,
                format!("report-cards-{}-{}", header.class_name, header.section_name),
            )
        }
        (None, None) => return Err(ApiError::validation("Give a section_id or a student_id.")),
    };
    let school = schools::get(&mut c, user.user.school_id).await?;
    let mut report = crate::reports::Report::new(
        "exams.report_cards",
        lang,
        &school.name,
        crate::reports::text::tr(lang, "report.exams.report_cards"),
    );
    report.cards = cards;
    let name = format!("{}.pdf", stem.replace(' ', "-"));
    let bytes = tokio::task::spawn_blocking(move || crate::reports::pdf::render(&report))
        .await
        .map_err(|_| ApiError::internal())??;
    Ok(crate::export::file_response(&name, bytes))
}

/// Every student's report card for a section (shared with the report library).
pub(crate) async fn section_cards_for(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    year: &madrasa_db::models::AcademicYear,
    section: &Section,
) -> ApiResult<Vec<ReportCard>> {
    load_section(c, user, section.id).await?;
    let mut cache = HashMap::new();
    let mut out = Vec::new();
    for r in repo::roster(c, section.id).await? {
        out.push(
            build_report_card(
                c,
                user.user.school_id,
                r.student_id,
                year,
                Some(section),
                &mut cache,
            )
            .await?,
        );
    }
    Ok(out)
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
    Ok(Json(
        section_cards_for(&mut c, &user, &year, &section).await?,
    ))
}
