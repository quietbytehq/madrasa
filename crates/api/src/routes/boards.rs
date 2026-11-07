//! External examination boards (M2 stage 4): boards, exams, candidates, result import and the
//! computed result sheet.

use std::collections::{BTreeMap, HashMap};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use madrasa_core::boards as rules;
use madrasa_core::boards::{BoardBand, CandidateStatus, SubjectStatus};
use madrasa_core::imports::ImportKind;
use madrasa_core::permissions::Permission;
use madrasa_db::boards as repo;
use madrasa_db::boards::{BoardExam, BoardResult, BoardSubject, Candidate, ExamBoard};
use madrasa_db::imports as batches;
use madrasa_db::{academics, settings, students};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::imports as engine;
use crate::{ApiError, ApiResult, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/boards", get(list_boards).post(create_board))
        .route(
            "/boards/{id}",
            axum::routing::put(update_board).delete(delete_board),
        )
        .route("/boards/exams", get(list_exams).post(create_exam))
        .route(
            "/boards/exams/{id}",
            get(get_exam).put(update_exam).delete(delete_exam),
        )
        .route(
            "/boards/exams/{id}/candidates",
            get(list_candidates).post(register_candidates),
        )
        .route(
            "/boards/candidates/{id}",
            axum::routing::put(update_candidate).delete(remove_candidate),
        )
        .route("/boards/exams/{id}/results", get(results_sheet))
        .route("/boards/exams/{id}/results/parse", post(parse_results))
        .route("/boards/exams/{id}/results/preview", post(preview_results))
        .route("/boards/exams/{id}/results/apply", post(apply_results))
        .route("/students/{id}/board-results", get(student_board_results))
}

fn bands_of(board: &ExamBoard) -> Vec<BoardBand> {
    serde_json::from_value(board.grade_bands.clone()).unwrap_or_default()
}

// ---- boards ---------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct BoardRequest {
    pub name: String,
    pub short_code: String,
    #[serde(default)]
    pub country: Option<String>,
    #[serde(default)]
    pub grading_mode: Option<String>,
    #[serde(default)]
    pub pass_percent: Option<Decimal>,
    #[serde(default)]
    pub grade_bands: Vec<BoardBand>,
}

fn board_input(req: BoardRequest, school_country: &str) -> ApiResult<repo::BoardInput> {
    if req.name.trim().is_empty() {
        return Err(ApiError::validation("The board needs a name."));
    }
    if req.short_code.trim().is_empty() || req.short_code.trim().len() > 12 {
        return Err(ApiError::validation(
            "Give the board a short code of up to 12 characters.",
        ));
    }
    let grading_mode = req.grading_mode.unwrap_or_else(|| "marks".into());
    if !["marks", "grades", "both"].contains(&grading_mode.as_str()) {
        return Err(ApiError::validation(
            "grading_mode must be marks, grades or both.",
        ));
    }
    let pass_percent = req.pass_percent.unwrap_or(Decimal::from(33));
    if pass_percent < Decimal::ZERO || pass_percent > Decimal::from(100) {
        return Err(ApiError::validation(
            "The pass percentage must be between 0 and 100.",
        ));
    }
    let bands = rules::validate_bands(&req.grade_bands).map_err(ApiError::validation)?;
    Ok(repo::BoardInput {
        name: req.name,
        short_code: req.short_code.to_uppercase(),
        country: req.country.unwrap_or_else(|| school_country.into()),
        grading_mode,
        pass_percent,
        grade_bands: serde_json::to_value(bands).unwrap_or(Value::Array(Vec::new())),
    })
}

async fn list_boards(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<ExamBoard>>> {
    user.require(Permission::ExamsRead)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(repo::list_boards(&mut c, user.user.school_id).await?))
}

async fn create_board(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<BoardRequest>,
) -> ApiResult<(StatusCode, Json<ExamBoard>)> {
    user.require(Permission::AcademicsManage)?;
    let mut c = state.pool.acquire().await?;
    let school = madrasa_db::schools::get(&mut c, user.user.school_id).await?;
    let input = board_input(req, &school.country)?;
    let row = repo::create_board(&mut c, &state.hlc, user.actor(), school.id, input).await?;
    Ok((StatusCode::CREATED, Json(row)))
}

async fn load_board(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    id: Uuid,
) -> ApiResult<ExamBoard> {
    let b = repo::get_board(c, id).await?;
    if b.school_id != user.user.school_id {
        return Err(ApiError::not_found("board"));
    }
    Ok(b)
}

async fn update_board(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<BoardRequest>,
) -> ApiResult<Json<ExamBoard>> {
    user.require(Permission::AcademicsManage)?;
    let mut c = state.pool.acquire().await?;
    let existing = load_board(&mut c, &user, id).await?;
    let input = board_input(req, &existing.country)?;
    Ok(Json(
        repo::update_board(&mut c, &state.hlc, user.actor(), id, input).await?,
    ))
}

async fn delete_board(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    user.require(Permission::AcademicsManage)?;
    let mut c = state.pool.acquire().await?;
    load_board(&mut c, &user, id).await?;
    repo::delete_board(&mut c, &state.hlc, user.actor(), id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- exams ----------------------------------------------------------------------------------

/// Mirrored by `BoardExamView` in the UI.
#[derive(Debug, Serialize)]
pub struct ExamView {
    #[serde(flatten)]
    pub exam: BoardExam,
    pub board_name: String,
    pub academic_year_name: String,
    pub class_ids: Vec<Uuid>,
    pub class_names: Vec<String>,
    pub subjects: Vec<BoardSubject>,
    pub candidates: i64,
    pub results: i64,
}

async fn exam_view(c: &mut sqlx::PgConnection, exam: BoardExam) -> ApiResult<ExamView> {
    let board = repo::get_board(c, exam.board_id).await?;
    let year = academics::get_year(c, exam.academic_year_id).await?;
    let class_ids = repo::exam_class_ids(c, exam.id).await?;
    let mut class_names = Vec::new();
    for cid in &class_ids {
        class_names.push(academics::get_class(c, *cid).await?.name);
    }
    let subjects = repo::exam_subjects(c, exam.id).await?;
    let candidates: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM board_candidates WHERE board_exam_id = $1 AND deleted_at IS NULL",
    )
    .bind(exam.id)
    .fetch_one(&mut *c)
    .await?;
    let results: i64 = sqlx::query_scalar(
        "SELECT count(DISTINCT r.candidate_id) FROM board_results r JOIN board_candidates bc ON bc.id = r.candidate_id
         WHERE bc.board_exam_id = $1 AND r.deleted_at IS NULL AND bc.deleted_at IS NULL",
    )
    .bind(exam.id)
    .fetch_one(&mut *c)
    .await?;
    Ok(ExamView {
        exam,
        board_name: board.name,
        academic_year_name: year.name,
        class_ids,
        class_names,
        subjects,
        candidates,
        results,
    })
}

#[derive(Debug, Deserialize)]
pub struct ExamsQuery {
    pub board_id: Option<Uuid>,
    pub academic_year_id: Option<Uuid>,
}

async fn list_exams(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<ExamsQuery>,
) -> ApiResult<Json<Vec<ExamView>>> {
    user.require(Permission::ExamsRead)?;
    let mut c = state.pool.acquire().await?;
    let mut out = Vec::new();
    for e in repo::list_exams(&mut c, user.user.school_id, q.board_id, q.academic_year_id).await? {
        out.push(exam_view(&mut c, e).await?);
    }
    Ok(Json(out))
}

#[derive(Debug, Deserialize)]
pub struct ExamRequest {
    pub board_id: Option<Uuid>,
    pub academic_year_id: Option<Uuid>,
    pub name: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub class_ids: Vec<Uuid>,
    #[serde(default)]
    pub subjects: Vec<repo::SubjectInput>,
}

fn check_subjects(subjects: &[repo::SubjectInput]) -> ApiResult<()> {
    let mut names: Vec<String> = Vec::new();
    for s in subjects {
        if s.name.trim().is_empty() {
            return Err(ApiError::validation("Every subject needs a name."));
        }
        if s.max_marks <= Decimal::ZERO {
            return Err(ApiError::validation(format!(
                "{} needs maximum marks above 0.",
                s.name
            )));
        }
        let key = s.name.trim().to_lowercase();
        if names.contains(&key) {
            return Err(ApiError::validation(format!("{} is listed twice.", s.name)));
        }
        names.push(key);
    }
    Ok(())
}

async fn check_classes(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    ids: &[Uuid],
) -> ApiResult<()> {
    for cid in ids {
        let class = academics::get_class(c, *cid).await?;
        if class.school_id != user.user.school_id {
            return Err(ApiError::not_found("class"));
        }
    }
    Ok(())
}

async fn create_exam(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<ExamRequest>,
) -> ApiResult<(StatusCode, Json<ExamView>)> {
    user.require(Permission::AcademicsManage)?;
    let mut c = state.pool.acquire().await?;
    let board_id = req
        .board_id
        .ok_or_else(|| ApiError::validation("board_id is required."))?;
    load_board(&mut c, &user, board_id).await?;
    let year = match req.academic_year_id {
        Some(id) => academics::get_year(&mut c, id).await?,
        None => academics::active_year(&mut c, user.user.school_id)
            .await?
            .ok_or_else(|| ApiError::validation("No active academic year."))?,
    };
    if year.school_id != user.user.school_id {
        return Err(ApiError::not_found("academic year"));
    }
    if req.name.trim().is_empty() {
        return Err(ApiError::validation("The exam needs a name."));
    }
    check_subjects(&req.subjects)?;
    check_classes(&mut c, &user, &req.class_ids).await?;
    let mut tx = state.pool.begin().await?;
    let exam = repo::create_exam(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        board_id,
        year.id,
        &req.name,
    )
    .await?;
    repo::set_exam_classes(&mut tx, &state.hlc, user.actor(), &exam, &req.class_ids).await?;
    repo::set_exam_subjects(&mut tx, &state.hlc, user.actor(), &exam, &req.subjects).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(exam_view(&mut c, exam).await?)))
}

async fn load_exam(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    id: Uuid,
) -> ApiResult<BoardExam> {
    let e = repo::get_exam(c, id).await?;
    if e.school_id != user.user.school_id {
        return Err(ApiError::not_found("board exam"));
    }
    Ok(e)
}

async fn get_exam(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ExamView>> {
    user.require(Permission::ExamsRead)?;
    let mut c = state.pool.acquire().await?;
    let exam = load_exam(&mut c, &user, id).await?;
    Ok(Json(exam_view(&mut c, exam).await?))
}

async fn update_exam(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<ExamRequest>,
) -> ApiResult<Json<ExamView>> {
    user.require(Permission::AcademicsManage)?;
    let mut c = state.pool.acquire().await?;
    let exam = load_exam(&mut c, &user, id).await?;
    let status = req.status.unwrap_or_else(|| exam.status.clone());
    if !["announced", "results_imported", "closed"].contains(&status.as_str()) {
        return Err(ApiError::validation(
            "status must be announced, results_imported or closed.",
        ));
    }
    if req.name.trim().is_empty() {
        return Err(ApiError::validation("The exam needs a name."));
    }
    check_subjects(&req.subjects)?;
    check_classes(&mut c, &user, &req.class_ids).await?;
    let mut tx = state.pool.begin().await?;
    let exam = repo::update_exam(&mut tx, &state.hlc, user.actor(), id, &req.name, &status).await?;
    repo::set_exam_classes(&mut tx, &state.hlc, user.actor(), &exam, &req.class_ids).await?;
    repo::set_exam_subjects(&mut tx, &state.hlc, user.actor(), &exam, &req.subjects).await?;
    tx.commit().await?;
    Ok(Json(exam_view(&mut c, exam).await?))
}

async fn delete_exam(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    user.require(Permission::AcademicsManage)?;
    let mut c = state.pool.acquire().await?;
    load_exam(&mut c, &user, id).await?;
    let mut tx = state.pool.begin().await?;
    repo::delete_exam(&mut tx, &state.hlc, user.actor(), id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- candidates -----------------------------------------------------------------------------

async fn list_candidates(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<Candidate>>> {
    user.require(Permission::ExamsRead)?;
    let mut c = state.pool.acquire().await?;
    load_exam(&mut c, &user, id).await?;
    Ok(Json(repo::candidates(&mut c, id).await?))
}

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    #[serde(default)]
    pub student_ids: Vec<Uuid>,
    /// Every active student of the class (current enrolment) …
    #[serde(default)]
    pub class_id: Option<Uuid>,
    /// … or of the section.
    #[serde(default)]
    pub section_id: Option<Uuid>,
    #[serde(default)]
    pub group_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RegisterResult {
    pub added: usize,
    pub candidates: Vec<Candidate>,
}

async fn register_candidates(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<RegisterRequest>,
) -> ApiResult<Json<RegisterResult>> {
    user.require(Permission::AcademicsManage)?;
    let mut c = state.pool.acquire().await?;
    let exam = load_exam(&mut c, &user, id).await?;
    let mut ids = req.student_ids.clone();
    if req.class_id.is_some() || req.section_id.is_some() {
        let page = students::list(
            &mut c,
            user.user.school_id,
            &students::StudentFilter {
                q: None,
                class_id: req.class_id,
                section_id: req.section_id,
                status: Some("active".into()),
                section_scope: None,
            },
            students::StudentSort::Class,
            false,
            1,
            2000,
        )
        .await?;
        ids.extend(page.items.iter().map(|s| s.id));
    }
    ids.sort();
    ids.dedup();
    if ids.is_empty() {
        return Err(ApiError::validation(
            "Choose students, a class or a section.",
        ));
    }
    let added = repo::register(
        &mut c,
        &state.hlc,
        user.actor(),
        &exam,
        &ids,
        req.group_name.as_deref(),
    )
    .await?;
    Ok(Json(RegisterResult {
        added,
        candidates: repo::candidates(&mut c, exam.id).await?,
    }))
}

#[derive(Debug, Deserialize)]
pub struct CandidateRequest {
    #[serde(default)]
    pub roll_no: Option<String>,
    #[serde(default)]
    pub registration_no: Option<String>,
    #[serde(default)]
    pub group_name: Option<String>,
}

async fn update_candidate(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<CandidateRequest>,
) -> ApiResult<Json<Candidate>> {
    user.require(Permission::AcademicsManage)?;
    let mut c = state.pool.acquire().await?;
    let cand = repo::get_candidate(&mut c, id).await?;
    load_exam(&mut c, &user, cand.board_exam_id).await?;
    Ok(Json(
        repo::update_candidate(
            &mut c,
            &state.hlc,
            user.actor(),
            id,
            req.roll_no.as_deref(),
            req.registration_no.as_deref(),
            req.group_name.as_deref(),
        )
        .await?,
    ))
}

async fn remove_candidate(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    user.require(Permission::AcademicsManage)?;
    let mut c = state.pool.acquire().await?;
    let cand = repo::get_candidate(&mut c, id).await?;
    load_exam(&mut c, &user, cand.board_exam_id).await?;
    repo::remove_candidate(&mut c, &state.hlc, user.actor(), id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- results sheet --------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct SubjectCell {
    pub board_subject_id: Uuid,
    pub obtained: Option<Decimal>,
    pub max_marks: Decimal,
    pub grade: Option<String>,
    pub status: Option<SubjectStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SheetRow {
    pub candidate: Candidate,
    pub cells: Vec<SubjectCell>,
    pub total: Decimal,
    pub max: Decimal,
    pub percent: Option<Decimal>,
    pub grade: Option<String>,
    pub status: CandidateStatus,
    pub position: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubjectSummary {
    pub board_subject_id: Uuid,
    pub name: String,
    pub appeared: usize,
    pub passed: usize,
    pub average_percent: Option<Decimal>,
}

/// Mirrored by `BoardResultSheet` in the UI.
#[derive(Debug, Serialize)]
pub struct ResultSheet {
    pub exam: ExamView,
    pub board: ExamBoard,
    pub rows: Vec<SheetRow>,
    pub subjects: Vec<SubjectSummary>,
    pub candidates: usize,
    pub with_results: usize,
    pub passed: usize,
    pub pass_percent: Option<Decimal>,
}

fn status_of(s: &str) -> Option<SubjectStatus> {
    SubjectStatus::parse(s)
}

pub(crate) fn build_sheet(
    exam: ExamView,
    board: ExamBoard,
    candidates: Vec<Candidate>,
    results: Vec<BoardResult>,
) -> ResultSheet {
    let bands = bands_of(&board);
    let by_candidate: HashMap<Uuid, Vec<&BoardResult>> =
        results.iter().fold(HashMap::new(), |mut m, r| {
            m.entry(r.candidate_id).or_default().push(r);
            m
        });
    let mut rows: Vec<SheetRow> = Vec::new();
    for cand in candidates {
        let mine = by_candidate.get(&cand.id).cloned().unwrap_or_default();
        let mut cells = Vec::new();
        let mut outcomes = Vec::new();
        for s in &exam.subjects {
            match mine.iter().find(|r| r.board_subject_id == s.id) {
                Some(r) => {
                    let status = status_of(&r.status).unwrap_or(SubjectStatus::Absent);
                    outcomes.push(rules::SubjectResult {
                        obtained: r.obtained,
                        max_marks: r.max_marks,
                        status,
                        grade: r.grade.clone(),
                    });
                    cells.push(SubjectCell {
                        board_subject_id: s.id,
                        obtained: r.obtained,
                        max_marks: r.max_marks,
                        grade: r.grade.clone(),
                        status: Some(status),
                    });
                }
                None => cells.push(SubjectCell {
                    board_subject_id: s.id,
                    obtained: None,
                    max_marks: s.max_marks,
                    grade: None,
                    status: None,
                }),
            }
        }
        let totals = rules::candidate_totals(&outcomes, board.pass_percent, &bands);
        rows.push(SheetRow {
            candidate: cand,
            cells,
            total: totals.total,
            max: totals.max,
            percent: totals.percent,
            grade: totals.grade,
            status: totals.status,
            position: None,
        });
    }
    let percents: Vec<Option<Decimal>> = rows
        .iter()
        .map(|r| {
            if r.status == CandidateStatus::Pending {
                None
            } else {
                r.percent
            }
        })
        .collect();
    for (row, pos) in rows.iter_mut().zip(rules::candidate_positions(&percents)) {
        row.position = pos;
    }
    let subjects = exam
        .subjects
        .iter()
        .map(|s| {
            let cells: Vec<&SubjectCell> = rows
                .iter()
                .filter_map(|r| r.cells.iter().find(|c| c.board_subject_id == s.id))
                .filter(|c| c.status.is_some())
                .collect();
            let appeared = cells
                .iter()
                .filter(|c| c.status != Some(SubjectStatus::Absent))
                .count();
            let passed = cells
                .iter()
                .filter(|c| c.status == Some(SubjectStatus::Pass))
                .count();
            let pcts: Vec<Decimal> = cells
                .iter()
                .filter(|c| c.status != Some(SubjectStatus::Absent))
                .filter_map(|c| {
                    c.obtained
                        .map(|o| madrasa_core::exams::percent(o, c.max_marks))
                })
                .collect();
            let average_percent = (!pcts.is_empty()).then(|| {
                madrasa_core::exams::round2(
                    pcts.iter().sum::<Decimal>() / Decimal::from(pcts.len() as u32),
                )
            });
            SubjectSummary {
                board_subject_id: s.id,
                name: s.name.clone(),
                appeared,
                passed,
                average_percent,
            }
        })
        .collect();
    let candidates = rows.len();
    let with_results = rows
        .iter()
        .filter(|r| r.status != CandidateStatus::Pending)
        .count();
    let passed = rows
        .iter()
        .filter(|r| r.status == CandidateStatus::Pass)
        .count();
    let pass_percent = (with_results > 0).then(|| {
        madrasa_core::exams::round2(
            Decimal::from(passed as u32) * Decimal::from(100) / Decimal::from(with_results as u32),
        )
    });
    ResultSheet {
        exam,
        board,
        rows,
        subjects,
        candidates,
        with_results,
        passed,
        pass_percent,
    }
}

pub(crate) async fn sheet_for(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    exam_id: Uuid,
) -> ApiResult<ResultSheet> {
    let exam = load_exam(c, user, exam_id).await?;
    let board = repo::get_board(c, exam.board_id).await?;
    let view = exam_view(c, exam).await?;
    let candidates = repo::candidates(c, exam_id).await?;
    let results = repo::results_for_exam(c, exam_id).await?;
    Ok(build_sheet(view, board, candidates, results))
}

async fn results_sheet(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ResultSheet>> {
    user.require(Permission::ExamsRead)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(sheet_for(&mut c, &user, id).await?))
}

/// A student's board results across exams (profile Results tab).
#[derive(Debug, Serialize)]
pub struct StudentBoardResult {
    pub exam: ExamView,
    pub board_name: String,
    pub row: SheetRow,
}

async fn student_board_results(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<StudentBoardResult>>> {
    user.require(Permission::ExamsRead)?;
    let mut c = state.pool.acquire().await?;
    let student = students::get(&mut c, id).await?;
    if student.school_id != user.user.school_id {
        return Err(ApiError::not_found("student"));
    }
    let scope = user.student_scope(&mut c).await?;
    let mut out = Vec::new();
    for cand in repo::candidates_of_student(&mut c, id).await? {
        if !scope.allows_section(cand.section_id) {
            continue;
        }
        let sheet = sheet_for(&mut c, &user, cand.board_exam_id).await?;
        if let Some(row) = sheet.rows.into_iter().find(|r| r.candidate.id == cand.id) {
            out.push(StudentBoardResult {
                board_name: sheet.board.name.clone(),
                exam: sheet.exam,
                row,
            });
        }
    }
    Ok(Json(out))
}

// ---- result import --------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    /// One row per candidate, one column per subject.
    #[default]
    Wide,
    /// One row per candidate and subject.
    Long,
}

#[derive(Debug, Deserialize)]
pub struct ParseRequest {
    pub file_name: String,
    pub content_base64: String,
    #[serde(default)]
    pub sheet: Option<String>,
}

/// Mirrored by `BoardParsedFile` in the UI. Mapping keys: `roll_no`, `registration_no`,
/// `subject`, `obtained`, `grade`, `status`, `remarks` and, for the wide layout,
/// `marks:<board_subject_id>` and `grade:<board_subject_id>`.
#[derive(Debug, Serialize)]
pub struct ParsedResults {
    pub file_name: String,
    pub sheets: Vec<String>,
    pub sheet: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub mapping: BTreeMap<String, usize>,
    pub layout: Layout,
    pub mapping_saved: bool,
}

fn norm(h: &str) -> String {
    madrasa_core::imports::normalize_header(h).replace(' ', "_")
}

fn guess_mapping(
    headers: &[String],
    subjects: &[BoardSubject],
) -> (BTreeMap<String, usize>, Layout) {
    let mut m = BTreeMap::new();
    for (i, h) in headers.iter().enumerate() {
        let n = norm(h);
        let key = match n.as_str() {
            "roll_no" | "roll" | "roll_number" | "board_roll_no" | "roll_no_" => Some("roll_no"),
            "registration_no" | "reg_no" | "registration" | "registration_number" => {
                Some("registration_no")
            }
            "subject" | "paper" | "subject_name" => Some("subject"),
            "marks" | "obtained" | "marks_obtained" | "obtained_marks" | "score" => {
                Some("obtained")
            }
            "grade" => Some("grade"),
            "status" | "result" => Some("status"),
            "remarks" | "remark" => Some("remarks"),
            _ => None,
        };
        if let Some(k) = key {
            m.entry(k.to_string()).or_insert(i);
            continue;
        }
        if let Some(s) = subjects.iter().find(|s| norm(&s.name) == n) {
            m.insert(format!("marks:{}", s.id), i);
        } else if let Some(s) = subjects.iter().find(|s| {
            let sn = norm(&s.name);
            n == format!("{sn}_grade") || n == format!("{sn}_gr")
        }) {
            m.insert(format!("grade:{}", s.id), i);
        }
    }
    let layout = if m.contains_key("subject") && m.contains_key("obtained") {
        Layout::Long
    } else {
        Layout::Wide
    };
    (m, layout)
}

fn mapping_key(board_id: Uuid) -> String {
    format!("boards.mapping.{board_id}")
}

#[derive(Debug, Serialize, Deserialize)]
struct SavedBoardMapping {
    headers: Vec<String>,
    mapping: BTreeMap<String, usize>,
    layout: Layout,
}

async fn parse_results(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<ParseRequest>,
) -> ApiResult<Json<ParsedResults>> {
    user.require(Permission::AcademicsManage)?;
    let mut c = state.pool.acquire().await?;
    let exam = load_exam(&mut c, &user, id).await?;
    let subjects = repo::exam_subjects(&mut c, exam.id).await?;
    let (sheets, chosen, headers, rows) = engine::parse_file(
        ImportKind::Students,
        &req.file_name,
        &req.content_base64,
        req.sheet.as_deref(),
    )?;
    let sheet = chosen.into_iter().next().unwrap_or_default();
    let (mut mapping, mut layout) = guess_mapping(&headers, &subjects);
    let mut mapping_saved = false;
    if let Some(saved) = settings::get(
        &mut c,
        Some(user.user.school_id),
        &mapping_key(exam.board_id),
    )
    .await?
    .and_then(|v| serde_json::from_value::<SavedBoardMapping>(v).ok())
    {
        let same = saved.headers.len() == headers.len()
            && saved
                .headers
                .iter()
                .zip(&headers)
                .all(|(a, b)| norm(a) == norm(b));
        if same {
            mapping = saved.mapping;
            layout = saved.layout;
            mapping_saved = true;
        }
    }
    Ok(Json(ParsedResults {
        file_name: req.file_name,
        sheets,
        sheet,
        headers,
        rows,
        mapping,
        layout,
        mapping_saved,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ImportRequest {
    #[serde(default)]
    pub file_name: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub mapping: BTreeMap<String, usize>,
    #[serde(default)]
    pub layout: Layout,
    #[serde(default)]
    pub save_mapping: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlannedResult {
    pub board_subject_id: Uuid,
    pub subject: String,
    pub obtained: Option<Decimal>,
    pub max_marks: Decimal,
    pub grade: Option<String>,
    pub status: SubjectStatus,
    pub remarks: Option<String>,
    /// A result for this candidate and subject exists and will be replaced.
    pub replaces: bool,
}

/// Mirrored by `BoardImportRow` in the UI.
#[derive(Debug, Clone, Serialize)]
pub struct PlannedRow {
    pub row_no: usize,
    pub roll_no: String,
    pub candidate_id: Option<Uuid>,
    pub candidate_name: Option<String>,
    pub results: Vec<PlannedResult>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize, Default)]
pub struct ImportSummary {
    pub rows: usize,
    pub candidates: usize,
    pub results: usize,
    pub replaced: usize,
    pub errors: usize,
}

#[derive(Debug, Serialize)]
pub struct ImportPreview {
    pub rows: Vec<PlannedRow>,
    pub summary: ImportSummary,
    pub unmapped: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ImportApplied {
    pub batch_id: Uuid,
    pub summary: ImportSummary,
}

fn cell<'a>(row: &'a [String], mapping: &BTreeMap<String, usize>, key: &str) -> Option<&'a str> {
    mapping
        .get(key)
        .and_then(|i| row.get(*i))
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
}

/// Parse one marks cell: a number, or an absent/withheld marker.
fn parse_marks(
    raw: &str,
    max: Decimal,
) -> Result<(Option<Decimal>, Option<SubjectStatus>), String> {
    let t = raw.trim();
    if let Some(st) = SubjectStatus::parse(t) {
        if matches!(st, SubjectStatus::Absent | SubjectStatus::Withheld) {
            return Ok((None, Some(st)));
        }
    }
    let n: Decimal =
        madrasa_core::imports::parse_money(t).ok_or_else(|| format!("'{t}' is not a number"))?;
    if n < Decimal::ZERO || n > max {
        return Err(format!("{t} is outside 0–{}", max.normalize()));
    }
    if n.scale() > 2 {
        return Err(format!("{t} has more than two decimals"));
    }
    Ok((Some(n), None))
}

/// Validate every row against the exam's candidates and subjects.
fn plan(
    exam_subjects: &[BoardSubject],
    candidates: &[Candidate],
    existing: &[BoardResult],
    bands: &[BoardBand],
    pass_percent: Decimal,
    req: &ImportRequest,
) -> ImportPreview {
    let by_roll: HashMap<String, &Candidate> = candidates
        .iter()
        .filter_map(|c| c.roll_no.as_ref().map(|r| (r.trim().to_lowercase(), c)))
        .collect();
    let by_reg: HashMap<String, &Candidate> = candidates
        .iter()
        .filter_map(|c| {
            c.registration_no
                .as_ref()
                .map(|r| (r.trim().to_lowercase(), c))
        })
        .collect();
    let subject_by_name: HashMap<String, &BoardSubject> =
        exam_subjects.iter().map(|s| (norm(&s.name), s)).collect();
    let has_result = |cand: Uuid, subject: Uuid| {
        existing
            .iter()
            .any(|r| r.candidate_id == cand && r.board_subject_id == subject)
    };
    let mut rows: Vec<PlannedRow> = Vec::new();
    for (i, raw) in req.rows.iter().enumerate() {
        if raw.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        let row_no = i + 2;
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let roll = cell(raw, &req.mapping, "roll_no").unwrap_or("").to_string();
        let reg = cell(raw, &req.mapping, "registration_no").unwrap_or("");
        let candidate = by_roll
            .get(&roll.to_lowercase())
            .or_else(|| by_reg.get(&reg.to_lowercase()))
            .copied();
        if roll.is_empty() && reg.is_empty() {
            errors.push("No roll number or registration number.".into());
        } else if candidate.is_none() {
            errors.push(format!(
                "No candidate with roll number '{roll}' in this exam."
            ));
        }
        let mut results = Vec::new();
        let remarks = cell(raw, &req.mapping, "remarks").map(str::to_string);
        let mut add =
            |s: &BoardSubject, marks: Option<&str>, grade: Option<&str>, status: Option<&str>| {
                let (obtained, marker) = match marks {
                    Some(m) => match parse_marks(m, s.max_marks) {
                        Ok(v) => v,
                        Err(e) => {
                            errors.push(format!("{}: {e}", s.name));
                            return;
                        }
                    },
                    None => (None, None),
                };
                let given = status.and_then(|st| {
                    let p = SubjectStatus::parse(st);
                    if p.is_none() {
                        warnings.push(format!("{}: status '{st}' not understood", s.name));
                    }
                    p
                });
                if obtained.is_none() && marker.is_none() && given.is_none() && grade.is_none() {
                    return;
                }
                let outcome = rules::subject_outcome(
                    obtained,
                    s.max_marks,
                    given.or(marker),
                    grade.map(str::to_string),
                    pass_percent,
                    bands,
                );
                results.push(PlannedResult {
                    board_subject_id: s.id,
                    subject: s.name.clone(),
                    obtained: outcome.obtained,
                    max_marks: s.max_marks,
                    grade: outcome.grade,
                    status: outcome.status,
                    remarks: remarks.clone(),
                    replaces: candidate.is_some_and(|c| has_result(c.id, s.id)),
                });
            };
        match req.layout {
            Layout::Wide => {
                for s in exam_subjects {
                    let marks = cell(raw, &req.mapping, &format!("marks:{}", s.id));
                    let grade = cell(raw, &req.mapping, &format!("grade:{}", s.id));
                    add(s, marks, grade, None);
                }
            }
            Layout::Long => match cell(raw, &req.mapping, "subject") {
                None => errors.push("No subject.".into()),
                Some(name) => match subject_by_name.get(&norm(name)) {
                    None => errors.push(format!("Subject '{name}' is not part of this exam.")),
                    Some(s) => add(
                        s,
                        cell(raw, &req.mapping, "obtained"),
                        cell(raw, &req.mapping, "grade"),
                        cell(raw, &req.mapping, "status"),
                    ),
                },
            },
        }
        if results.is_empty() && errors.is_empty() {
            warnings.push("No marks in this row.".into());
        }
        rows.push(PlannedRow {
            row_no,
            roll_no: roll,
            candidate_id: candidate.map(|c| c.id),
            candidate_name: candidate.map(|c| c.full_name.clone()),
            results,
            errors,
            warnings,
        });
    }
    let mut cands: Vec<Uuid> = rows.iter().filter_map(|r| r.candidate_id).collect();
    cands.sort();
    cands.dedup();
    let summary = ImportSummary {
        rows: rows.len(),
        candidates: cands.len(),
        results: rows
            .iter()
            .filter(|r| r.errors.is_empty())
            .map(|r| r.results.len())
            .sum(),
        replaced: rows
            .iter()
            .filter(|r| r.errors.is_empty())
            .flat_map(|r| &r.results)
            .filter(|x| x.replaces)
            .count(),
        errors: rows.iter().filter(|r| !r.errors.is_empty()).count(),
    };
    let mapped: Vec<usize> = req.mapping.values().copied().collect();
    let unmapped = req
        .headers
        .iter()
        .enumerate()
        .filter(|(i, h)| !mapped.contains(i) && !h.trim().is_empty())
        .map(|(_, h)| h.clone())
        .collect();
    ImportPreview {
        rows,
        summary,
        unmapped,
    }
}

async fn plan_for(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    exam_id: Uuid,
    req: &ImportRequest,
) -> ApiResult<(BoardExam, ExamBoard, ImportPreview)> {
    let exam = load_exam(c, user, exam_id).await?;
    let board = repo::get_board(c, exam.board_id).await?;
    if req.rows.len() > engine::MAX_ROWS {
        return Err(ApiError::validation(format!(
            "At most {} rows per file.",
            engine::MAX_ROWS
        )));
    }
    if !req.mapping.contains_key("roll_no") && !req.mapping.contains_key("registration_no") {
        return Err(ApiError::validation(
            "Map the roll number (or registration number) column.",
        ));
    }
    let subjects = repo::exam_subjects(c, exam.id).await?;
    let candidates = repo::candidates(c, exam.id).await?;
    let existing = repo::results_for_exam(c, exam.id).await?;
    let preview = plan(
        &subjects,
        &candidates,
        &existing,
        &bands_of(&board),
        board.pass_percent,
        req,
    );
    Ok((exam, board, preview))
}

async fn preview_results(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<ImportRequest>,
) -> ApiResult<Json<ImportPreview>> {
    user.require(Permission::AcademicsManage)?;
    let mut c = state.pool.acquire().await?;
    let (_, _, preview) = plan_for(&mut c, &user, id, &req).await?;
    Ok(Json(preview))
}

async fn apply_results(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<ImportRequest>,
) -> ApiResult<Json<ImportApplied>> {
    user.require(Permission::AcademicsManage)?;
    let mut c = state.pool.acquire().await?;
    let (exam, board, preview) = plan_for(&mut c, &user, id, &req).await?;
    if preview.summary.errors > 0 {
        return Err(ApiError::validation(format!(
            "{} rows have errors; fix them or remove them from the file.",
            preview.summary.errors
        )));
    }
    if preview.summary.results == 0 {
        return Err(ApiError::validation("The file holds no marks to import."));
    }
    let mut tx = state.pool.begin().await?;
    let batch = batches::create_batch(
        &mut tx,
        &state.hlc,
        user.actor(),
        batches::NewBatch {
            school_id: user.user.school_id,
            kind: "board_results".into(),
            file_name: if req.file_name.is_empty() {
                "results".into()
            } else {
                req.file_name.clone()
            },
            options: json!({ "board_exam_id": exam.id, "layout": req.layout }),
            total_rows: preview.summary.rows as i32,
            applied_by: user.user.id,
        },
    )
    .await?;
    let (mut created, mut updated) = (0i32, 0i32);
    for row in &preview.rows {
        let Some(cand) = row.candidate_id else {
            continue;
        };
        for r in &row.results {
            let change = repo::upsert_result(
                &mut tx,
                &state.hlc,
                user.actor(),
                repo::ResultInput {
                    candidate_id: cand,
                    board_subject_id: r.board_subject_id,
                    obtained: r.obtained,
                    max_marks: r.max_marks,
                    grade: r.grade.clone(),
                    status: r.status.as_str().into(),
                    remarks: r.remarks.clone(),
                    batch_id: Some(batch.id),
                },
            )
            .await?;
            match change {
                repo::ResultChange::Created(res) => {
                    created += 1;
                    batches::add_row(
                        &mut tx,
                        &state.hlc,
                        user.actor(),
                        &batch,
                        row.row_no as i32,
                        "created",
                        "board_result",
                        Some(res.id),
                        None,
                    )
                    .await?;
                }
                repo::ResultChange::Updated { before, after } => {
                    updated += 1;
                    batches::add_row(
                        &mut tx,
                        &state.hlc,
                        user.actor(),
                        &batch,
                        row.row_no as i32,
                        "updated",
                        "board_result",
                        Some(after.id),
                        Some(serde_json::to_value(&before).unwrap_or(Value::Null)),
                    )
                    .await?;
                }
            }
        }
    }
    batches::finish_batch(
        &mut tx,
        &state.hlc,
        user.actor(),
        batch.id,
        created,
        updated,
        0,
    )
    .await?;
    if exam.status == "announced" {
        repo::update_exam(
            &mut tx,
            &state.hlc,
            user.actor(),
            exam.id,
            &exam.name,
            "results_imported",
        )
        .await?;
    }
    if req.save_mapping {
        settings::set(
            &mut tx,
            &state.hlc,
            user.actor(),
            Some(user.user.school_id),
            &mapping_key(board.id),
            serde_json::to_value(SavedBoardMapping {
                headers: req.headers.clone(),
                mapping: req.mapping.clone(),
                layout: req.layout,
            })
            .map_err(|_| ApiError::internal())?,
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Json(ImportApplied {
        batch_id: batch.id,
        summary: preview.summary,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guesses_roll_and_subject_columns_and_the_layout() {
        let subjects = vec![
            BoardSubject {
                id: Uuid::now_v7(),
                board_exam_id: Uuid::nil(),
                name: "English".into(),
                subject_id: None,
                max_marks: Decimal::from(100),
                position: 0,
            },
            BoardSubject {
                id: Uuid::now_v7(),
                board_exam_id: Uuid::nil(),
                name: "Mathematics".into(),
                subject_id: None,
                max_marks: Decimal::from(100),
                position: 1,
            },
        ];
        let headers = vec![
            "Roll No".into(),
            "Name".into(),
            "English".into(),
            "MATHEMATICS".into(),
            "English Grade".into(),
        ];
        let (m, layout) = guess_mapping(&headers, &subjects);
        assert_eq!(layout, Layout::Wide);
        assert_eq!(m.get("roll_no"), Some(&0));
        assert_eq!(m.get(&format!("marks:{}", subjects[0].id)), Some(&2));
        assert_eq!(m.get(&format!("marks:{}", subjects[1].id)), Some(&3));
        assert_eq!(m.get(&format!("grade:{}", subjects[0].id)), Some(&4));
        let headers = vec![
            "Roll Number".into(),
            "Subject".into(),
            "Marks Obtained".into(),
            "Status".into(),
        ];
        let (m, layout) = guess_mapping(&headers, &subjects);
        assert_eq!(layout, Layout::Long);
        assert_eq!(
            (
                m.get("roll_no"),
                m.get("subject"),
                m.get("obtained"),
                m.get("status")
            ),
            (Some(&0), Some(&1), Some(&2), Some(&3))
        );
    }

    #[test]
    fn marks_cells_accept_numbers_and_absent_markers_only() {
        let max = Decimal::from(75);
        assert_eq!(
            parse_marks("60", max).unwrap(),
            (Some(Decimal::from(60)), None)
        );
        assert_eq!(
            parse_marks("ABS", max).unwrap(),
            (None, Some(SubjectStatus::Absent))
        );
        assert_eq!(
            parse_marks("RL", max).unwrap(),
            (None, Some(SubjectStatus::Withheld))
        );
        assert!(parse_marks("80", max).is_err());
        assert!(parse_marks("sixty", max).is_err());
        assert!(parse_marks("12.345", max).is_err());
    }
}
