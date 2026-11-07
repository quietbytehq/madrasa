//! Timetable (Standard stage 1): setup (shifts, rooms, lessons, rules), the generator, the
//! timetables with manual edits, the views by section, teacher and room, printouts, and
//! substitutions (absences, cover suggestions, cover sheet). Every route needs the
//! `timetable` feature (403 `feature_unavailable` on Lite) and `timetable.read` or
//! `timetable.manage`.

use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::{Datelike, NaiveDate};
use madrasa_core::edition::Feature;
use madrasa_core::permissions::Permission;
use madrasa_core::timetable as engine;
use madrasa_core::timetable::solver::edit;
use madrasa_core::timetable::substitutions as subs;
use madrasa_core::timetable::{check, lessons as engine_lessons};
use madrasa_db::models::AcademicYear;
use madrasa_db::timetable as repo;
use madrasa_db::timetable::{
    Absence, Break, Card, Cover, Lesson, PeriodRef, PeriodTime, Room, Rule, Shift, ShiftDay,
    TimeOff, Timetable, YearSetup,
};
use madrasa_db::{academics, staff as staff_repo};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::reports::{self, Lang};
use crate::timetable_jobs::{JobView, RunInput};
use crate::{ApiError, ApiResult, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/timetable/setup", get(setup))
        .route("/timetable/shifts", post(create_shift))
        .route("/timetable/shifts/default", post(create_default_shift))
        .route(
            "/timetable/shifts/{id}",
            put(update_shift).delete(delete_shift),
        )
        .route("/timetable/rooms", get(list_rooms).post(create_room))
        .route(
            "/timetable/rooms/{id}",
            put(update_room).delete(delete_room),
        )
        .route("/timetable/lessons", post(create_lesson))
        .route("/timetable/lessons/seed", post(seed_lessons))
        .route(
            "/timetable/lessons/{id}",
            put(update_lesson).delete(delete_lesson),
        )
        .route("/timetable/rules", put(set_rule))
        .route("/timetable/generate", post(generate))
        .route("/timetable/generate/{id}", get(job))
        .route("/timetable/generate/{id}/cancel", post(cancel_job))
        .route("/timetable/timetables", post(create_timetable))
        .route(
            "/timetable/timetables/{id}",
            get(view_timetable)
                .patch(rename_timetable)
                .delete(delete_timetable),
        )
        .route(
            "/timetable/timetables/{id}/publish",
            post(publish_timetable),
        )
        .route("/timetable/timetables/{id}/options", get(slot_options))
        .route("/timetable/timetables/{id}/move", post(move_card))
        .route("/timetable/timetables/{id}/swap-options", get(swap_options))
        .route("/timetable/timetables/{id}/swap", post(swap_cards))
        .route("/timetable/timetables/{id}/lock", post(lock_card))
        .route("/timetable/current", get(current_timetable))
        .route("/timetable/mine", get(mine))
        .route("/timetable/print", get(print_pdf))
        .route("/timetable/substitutions", get(substitutions_day))
        .route("/timetable/absences", post(create_absence))
        .route(
            "/timetable/absences/{id}",
            axum::routing::delete(delete_absence),
        )
        .route("/timetable/covers", put(set_cover).delete(clear_cover))
        .route("/timetable/cover-sheet", get(cover_sheet))
        .route("/timetable/cover-sheet/pdf", get(cover_sheet_pdf))
}

// ---- helpers --------------------------------------------------------------------------------

fn gate(state: &AppState, user: &CurrentUser, manage: bool) -> ApiResult<()> {
    state.require_feature(Feature::Timetable)?;
    user.require(if manage {
        Permission::TimetableManage
    } else {
        Permission::TimetableRead
    })
}

async fn year_of(
    c: &mut PgConnection,
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

#[derive(Debug, Deserialize)]
pub struct YearQuery {
    pub academic_year_id: Option<Uuid>,
}

fn engine_err(e: String) -> ApiError {
    ApiError::validation(e)
}

/// A name for a select list. Mirrored by `Named` in the UI.
#[derive(Debug, Clone, Serialize)]
pub struct Named {
    pub id: Uuid,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// A lesson with the names of what it joins. Mirrored by `LessonView` in the UI.
#[derive(Debug, Clone, Serialize)]
pub struct LessonView {
    #[serde(flatten)]
    pub lesson: Lesson,
    pub subject: String,
    pub subject_short: String,
    pub teachers: Vec<String>,
    pub sections: Vec<String>,
    pub rooms: Vec<String>,
}

fn lesson_views(ys: &YearSetup) -> Vec<LessonView> {
    let m = &ys.setup.master;
    let name_of = |list: &[(String, String)], id: &Uuid| -> Option<String> {
        let id = id.to_string();
        list.iter().find(|(i, _)| *i == id).map(|(_, n)| n.clone())
    };
    let teachers: Vec<(String, String)> = m
        .teachers
        .iter()
        .map(|t| (t.id.clone(), t.name.clone()))
        .collect();
    let classes: Vec<(String, String)> = m
        .classes
        .iter()
        .map(|c| (c.id.clone(), c.name.clone()))
        .collect();
    let rooms: Vec<(String, String)> = m
        .rooms
        .iter()
        .map(|r| (r.id.clone(), r.name.clone()))
        .collect();
    ys.lessons
        .iter()
        .map(|l| {
            let subj = m.subjects.iter().find(|s| s.id == l.subject_id.to_string());
            LessonView {
                lesson: l.clone(),
                subject: subj.map(|s| s.name.clone()).unwrap_or_default(),
                subject_short: subj.map(|s| s.short_name.clone()).unwrap_or_default(),
                teachers: l
                    .teacher_ids
                    .iter()
                    .filter_map(|t| name_of(&teachers, t))
                    .collect(),
                sections: l
                    .section_ids
                    .iter()
                    .filter_map(|s| name_of(&classes, s))
                    .collect(),
                rooms: l
                    .room_ids
                    .iter()
                    .filter_map(|r| name_of(&rooms, r))
                    .collect(),
            }
        })
        .collect()
}

fn section_names(ys: &YearSetup) -> Vec<Named> {
    ys.setup
        .master
        .classes
        .iter()
        .filter_map(|c| {
            let id: Uuid = c.id.parse().ok()?;
            Some(Named {
                id,
                name: c.name.clone(),
                class_id: ys.sections.iter().find(|s| s.id == id).map(|s| s.class_id),
                code: None,
            })
        })
        .collect()
}

fn teacher_names(ys: &YearSetup) -> Vec<Named> {
    ys.setup
        .master
        .teachers
        .iter()
        .filter_map(|t| {
            Some(Named {
                id: t.id.parse().ok()?,
                name: t.name.clone(),
                class_id: None,
                code: Some(t.short_name.clone()),
            })
        })
        .collect()
}

// ---- setup ----------------------------------------------------------------------------------

/// Mirrored by `TimetableSetup` in the UI.
#[derive(Debug, Serialize)]
pub struct SetupView {
    pub academic_year: AcademicYear,
    pub shifts: Vec<Shift>,
    pub rooms: Vec<Room>,
    pub lessons: Vec<LessonView>,
    pub rules: Vec<Rule>,
    pub sections: Vec<Named>,
    pub teachers: Vec<Named>,
    pub subjects: Vec<Named>,
    pub timetables: Vec<Timetable>,
    pub published_id: Option<Uuid>,
    /// The pre-check: what stops or hinders generation, with fixes.
    pub check: check::CheckReport,
    pub load: engine_lessons::LoadSummary,
    pub school_rules: repo::SchoolRules,
}

async fn setup(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<YearQuery>,
) -> ApiResult<Json<SetupView>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    let year = year_of(&mut c, &user, q.academic_year_id).await?;
    let ys = repo::setup_for_year(&mut c, user.user.school_id, &year, None).await?;
    let timetables = repo::list_timetables(&mut c, year.id).await?;
    Ok(Json(SetupView {
        published_id: ys.timetable.as_ref().map(|t| t.id),
        check: check::run(&ys.setup),
        load: engine_lessons::summarize(&ys.setup),
        school_rules: ys
            .rules
            .iter()
            .find(|r| r.kind == "school")
            .map(Rule::school_rules)
            .unwrap_or_default(),
        lessons: lesson_views(&ys),
        sections: section_names(&ys),
        teachers: teacher_names(&ys),
        subjects: ys
            .subjects
            .iter()
            .map(|s| Named {
                id: s.id,
                name: s.name.clone(),
                class_id: None,
                code: Some(s.code.clone()),
            })
            .collect(),
        academic_year: year,
        shifts: ys.shifts,
        rooms: ys.rooms,
        rules: ys.rules,
        timetables,
    }))
}

// ---- shifts ---------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ShiftRequest {
    pub academic_year_id: Option<Uuid>,
    pub name: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    pub days: Vec<ShiftDay>,
    pub period_times: Vec<PeriodTime>,
    #[serde(default)]
    pub breaks: Vec<Break>,
    #[serde(default)]
    pub position: i32,
}

fn yes() -> bool {
    true
}

impl ShiftRequest {
    fn input(&self) -> repo::ShiftInput {
        repo::ShiftInput {
            name: self.name.clone(),
            enabled: self.enabled,
            days: self.days.clone(),
            period_times: self.period_times.clone(),
            breaks: self.breaks.clone(),
            position: self.position,
        }
    }
}

async fn create_shift(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<ShiftRequest>,
) -> ApiResult<(StatusCode, Json<Shift>)> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    let year = year_of(&mut tx, &user, req.academic_year_id).await?;
    let row = repo::create_shift(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        year.id,
        &req.input(),
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[derive(Debug, Deserialize)]
pub struct DefaultShiftRequest {
    pub academic_year_id: Option<Uuid>,
    #[serde(default)]
    pub name: Option<String>,
}

async fn create_default_shift(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<DefaultShiftRequest>,
) -> ApiResult<(StatusCode, Json<Shift>)> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    let year = year_of(&mut tx, &user, req.academic_year_id).await?;
    let input = repo::default_shift_input(req.name.as_deref().unwrap_or("Morning"));
    let row = repo::create_shift(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        year.id,
        &input,
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(row)))
}

async fn load_shift(c: &mut PgConnection, user: &CurrentUser, id: Uuid) -> ApiResult<Shift> {
    let s = repo::get_shift(c, id).await?;
    if s.school_id != user.user.school_id {
        return Err(ApiError::not_found("shift"));
    }
    Ok(s)
}

async fn update_shift(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<ShiftRequest>,
) -> ApiResult<Json<Shift>> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    load_shift(&mut tx, &user, id).await?;
    let row = repo::update_shift(&mut tx, &state.hlc, user.actor(), id, &req.input()).await?;
    tx.commit().await?;
    Ok(Json(row))
}

async fn delete_shift(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    load_shift(&mut tx, &user, id).await?;
    repo::delete_shift(&mut tx, &state.hlc, user.actor(), id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- rooms ----------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RoomRequest {
    pub name: String,
    #[serde(default)]
    pub short_name: String,
    #[serde(default)]
    pub building: String,
    #[serde(default)]
    pub capacity: Option<i32>,
}

impl RoomRequest {
    fn input(&self) -> repo::RoomInput {
        repo::RoomInput {
            name: self.name.clone(),
            short_name: self.short_name.clone(),
            building: self.building.clone(),
            capacity: self.capacity,
        }
    }
}

async fn list_rooms(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<Room>>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(repo::list_rooms(&mut c, user.user.school_id).await?))
}

async fn create_room(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<RoomRequest>,
) -> ApiResult<(StatusCode, Json<Room>)> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    let row = repo::create_room(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        &req.input(),
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(row)))
}

async fn load_room(c: &mut PgConnection, user: &CurrentUser, id: Uuid) -> ApiResult<Room> {
    let r = repo::get_room(c, id).await?;
    if r.school_id != user.user.school_id {
        return Err(ApiError::not_found("room"));
    }
    Ok(r)
}

async fn update_room(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<RoomRequest>,
) -> ApiResult<Json<Room>> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    load_room(&mut tx, &user, id).await?;
    let row = repo::update_room(&mut tx, &state.hlc, user.actor(), id, &req.input()).await?;
    tx.commit().await?;
    Ok(Json(row))
}

async fn delete_room(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    load_room(&mut tx, &user, id).await?;
    repo::delete_room(&mut tx, &state.hlc, user.actor(), id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- lessons --------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct LessonRequest {
    pub academic_year_id: Option<Uuid>,
    pub subject_id: Uuid,
    #[serde(default)]
    pub teacher_ids: Vec<Uuid>,
    pub section_ids: Vec<Uuid>,
    #[serde(default)]
    pub room_ids: Vec<Uuid>,
    #[serde(default = "one")]
    pub periods_per_card: i32,
    pub cards_per_week: i32,
}

fn one() -> i32 {
    1
}

impl LessonRequest {
    async fn input(
        &self,
        c: &mut PgConnection,
        user: &CurrentUser,
        year: &AcademicYear,
    ) -> ApiResult<repo::LessonInput> {
        let school = user.user.school_id;
        let subject = academics::get_subject(c, self.subject_id).await?;
        if subject.school_id != school {
            return Err(ApiError::not_found("subject"));
        }
        for id in &self.section_ids {
            let s = academics::get_section(c, *id).await?;
            if s.school_id != school || s.academic_year_id != year.id {
                return Err(ApiError::validation(
                    "A section named by the lesson is not in this academic year.",
                ));
            }
        }
        for id in &self.teacher_ids {
            let t = staff_repo::get(c, *id).await?;
            if t.school_id != school {
                return Err(ApiError::not_found("staff"));
            }
        }
        for id in &self.room_ids {
            let r = repo::get_room(c, *id).await?;
            if r.school_id != school {
                return Err(ApiError::not_found("room"));
            }
        }
        Ok(repo::LessonInput {
            subject_id: self.subject_id,
            teacher_ids: self.teacher_ids.clone(),
            section_ids: self.section_ids.clone(),
            room_ids: self.room_ids.clone(),
            periods_per_card: self.periods_per_card,
            cards_per_week: self.cards_per_week,
        })
    }
}

async fn create_lesson(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<LessonRequest>,
) -> ApiResult<(StatusCode, Json<Lesson>)> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    let year = year_of(&mut tx, &user, req.academic_year_id).await?;
    let input = req.input(&mut tx, &user, &year).await?;
    let row = repo::create_lesson(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        year.id,
        &input,
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(row)))
}

async fn load_lesson(c: &mut PgConnection, user: &CurrentUser, id: Uuid) -> ApiResult<Lesson> {
    let l = repo::get_lesson(c, id).await?;
    if l.school_id != user.user.school_id {
        return Err(ApiError::not_found("lesson"));
    }
    Ok(l)
}

async fn update_lesson(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<LessonRequest>,
) -> ApiResult<Json<Lesson>> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    let old = load_lesson(&mut tx, &user, id).await?;
    let year = year_of(&mut tx, &user, Some(old.academic_year_id)).await?;
    let input = req.input(&mut tx, &user, &year).await?;
    let row = repo::update_lesson(&mut tx, &state.hlc, user.actor(), id, &input).await?;
    tx.commit().await?;
    Ok(Json(row))
}

async fn delete_lesson(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    load_lesson(&mut tx, &user, id).await?;
    repo::delete_lesson(&mut tx, &state.hlc, user.actor(), id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct SeedRequest {
    pub academic_year_id: Option<Uuid>,
    /// Sessions per week for every seeded lesson (default 5, one a day).
    #[serde(default = "five")]
    pub cards_per_week: i32,
}

fn five() -> i32 {
    5
}

#[derive(Debug, Serialize)]
pub struct SeedResult {
    pub created: usize,
}

/// Create lessons from the teacher allocation grid for every (section, subject) without one.
async fn seed_lessons(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<SeedRequest>,
) -> ApiResult<Json<SeedResult>> {
    gate(&state, &user, true)?;
    if !(1..=40).contains(&req.cards_per_week) {
        return Err(ApiError::validation(
            "Sessions per week must be between 1 and 40.",
        ));
    }
    let mut tx = state.pool.begin().await?;
    let year = year_of(&mut tx, &user, req.academic_year_id).await?;
    let created = repo::seed_lessons_from_allocations(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        year.id,
        req.cards_per_week,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(SeedResult { created }))
}

// ---- rules ----------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RuleRequest {
    pub academic_year_id: Option<Uuid>,
    /// `school`, `teacher`, `section` or `room`.
    pub kind: String,
    #[serde(default)]
    pub entity_id: Option<Uuid>,
    #[serde(default)]
    pub shift_ids: Vec<Uuid>,
    #[serde(default)]
    pub home_room_id: Option<Uuid>,
    #[serde(default)]
    pub time_off: Vec<TimeOff>,
    /// `TeacherLimits` for a teacher, `SchoolRules` for the school, otherwise `{}`.
    #[serde(default)]
    pub limits: Value,
}

async fn set_rule(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<RuleRequest>,
) -> ApiResult<Json<Rule>> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    let year = year_of(&mut tx, &user, req.academic_year_id).await?;
    let school = user.user.school_id;
    match (req.kind.as_str(), req.entity_id) {
        ("school", None) => {
            serde_json::from_value::<repo::SchoolRules>(req.limits.clone())
                .map_err(|_| ApiError::validation("The school rules are incomplete."))?;
        }
        ("teacher", Some(id)) => {
            if staff_repo::get(&mut tx, id).await?.school_id != school {
                return Err(ApiError::not_found("staff"));
            }
            serde_json::from_value::<repo::TeacherLimits>(req.limits.clone())
                .map_err(|_| ApiError::validation("The teacher limits are not numbers."))?;
        }
        ("section", Some(id)) => {
            let s = academics::get_section(&mut tx, id).await?;
            if s.school_id != school || s.academic_year_id != year.id {
                return Err(ApiError::not_found("section"));
            }
        }
        ("room", Some(id)) => {
            if repo::get_room(&mut tx, id).await?.school_id != school {
                return Err(ApiError::not_found("room"));
            }
        }
        _ => {
            return Err(ApiError::validation(
                "Rules are for the school, a teacher, a section or a room.",
            ))
        }
    }
    for id in &req.shift_ids {
        load_shift(&mut tx, &user, *id).await?;
    }
    if let Some(r) = req.home_room_id {
        load_room(&mut tx, &user, r).await?;
    }
    let limits = if req.limits.is_null() {
        serde_json::json!({})
    } else {
        req.limits.clone()
    };
    let row = repo::set_rule(
        &mut tx,
        &state.hlc,
        user.actor(),
        school,
        year.id,
        &repo::RuleInput {
            kind: req.kind.clone(),
            entity_id: req.entity_id,
            shift_ids: req.shift_ids.clone(),
            home_room_id: req.home_room_id,
            time_off: req.time_off.clone(),
            limits,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(row))
}

// ---- generation -----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct GenerateRequest {
    pub academic_year_id: Option<Uuid>,
    /// Fill an existing timetable (its locked cards stay) or, when absent, create a new one.
    #[serde(default)]
    pub timetable_id: Option<Uuid>,
    #[serde(default)]
    pub name: Option<String>,
    /// Seconds the generator may take (1–600; default 60).
    #[serde(default = "sixty")]
    pub time_limit_seconds: u32,
    /// Seconds spent improving quality afterwards (default: the school rule).
    #[serde(default)]
    pub improve_seconds: Option<u32>,
    #[serde(default)]
    pub seed: Option<u64>,
}

fn sixty() -> u32 {
    60
}

async fn load_timetable(
    c: &mut PgConnection,
    user: &CurrentUser,
    id: Uuid,
) -> ApiResult<Timetable> {
    let t = repo::get_timetable(c, id).await?;
    if t.school_id != user.user.school_id {
        return Err(ApiError::not_found("timetable"));
    }
    Ok(t)
}

async fn generate(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<GenerateRequest>,
) -> ApiResult<(StatusCode, Json<JobView>)> {
    gate(&state, &user, true)?;
    if !(1..=600).contains(&req.time_limit_seconds) {
        return Err(ApiError::validation(
            "The time limit is between 1 and 600 seconds.",
        ));
    }
    let school = user.user.school_id;
    let mut tx = state.pool.begin().await?;
    let year = year_of(&mut tx, &user, req.academic_year_id).await?;
    let timetable = match req.timetable_id {
        Some(id) => {
            let t = load_timetable(&mut tx, &user, id).await?;
            if t.academic_year_id != year.id {
                return Err(ApiError::validation(
                    "The timetable belongs to another academic year.",
                ));
            }
            t
        }
        None => {
            let n = repo::list_timetables(&mut tx, year.id).await?.len() + 1;
            let name = req
                .name
                .clone()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| format!("Timetable {n}"));
            repo::create_timetable(&mut tx, &state.hlc, user.actor(), school, year.id, &name)
                .await?
        }
    };
    if state.timetable_jobs.running_for(timetable.id).is_some() {
        return Err(ApiError::conflict(
            "This timetable is being generated already. Wait for it to finish or cancel it.",
        ));
    }
    let ys = repo::setup_for_year(&mut tx, school, &year, Some(timetable.id)).await?;
    tx.commit().await?;
    let report = check::run(&ys.setup);
    if report.errors > 0 {
        let first = report
            .findings
            .iter()
            .find(|f| f.level == "error")
            .map(|f| format!("{} {}", f.message, f.fix))
            .unwrap_or_default();
        let mut e = ApiError::validation(format!("Fix the setup before generating: {first}"));
        e.details = serde_json::to_value(&report).ok();
        return Err(e);
    }
    let locked: Vec<engine::PlacedCard> = ys
        .setup
        .timetable
        .as_ref()
        .map(|t| t.cards.iter().filter(|c| c.locked).cloned().collect())
        .unwrap_or_default();
    let improve = req
        .improve_seconds
        .unwrap_or(ys.setup.settings.improve_seconds)
        .min(300);
    let id = state.timetable_jobs.start(
        state.pool.clone(),
        state.hlc.clone(),
        user.actor(),
        RunInput {
            school_id: school,
            academic_year_id: year.id,
            timetable_id: timetable.id,
            setup: ys.setup,
            locked,
            time_limit: Duration::from_secs(u64::from(req.time_limit_seconds)),
            improve: Duration::from_secs(u64::from(improve)),
            seed: req.seed.unwrap_or(1),
        },
    );
    let view = state
        .timetable_jobs
        .get(school, id)
        .ok_or_else(ApiError::internal)?;
    Ok((StatusCode::ACCEPTED, Json(view)))
}

async fn job(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<JobView>> {
    gate(&state, &user, false)?;
    state
        .timetable_jobs
        .get(user.user.school_id, id)
        .map(Json)
        .ok_or_else(|| ApiError::not_found("generation run"))
}

async fn cancel_job(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    gate(&state, &user, true)?;
    if state.timetable_jobs.cancel(user.user.school_id, id) {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("generation run"))
    }
}

// ---- timetables -----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct TimetableRequest {
    pub academic_year_id: Option<Uuid>,
    pub name: String,
}

async fn create_timetable(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<TimetableRequest>,
) -> ApiResult<(StatusCode, Json<Timetable>)> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    let year = year_of(&mut tx, &user, req.academic_year_id).await?;
    let row = repo::create_timetable(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        year.id,
        &req.name,
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[derive(Debug, Deserialize)]
pub struct RenameRequest {
    pub name: String,
}

async fn rename_timetable(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<RenameRequest>,
) -> ApiResult<Json<Timetable>> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    load_timetable(&mut tx, &user, id).await?;
    let row = repo::rename_timetable(&mut tx, &state.hlc, user.actor(), id, &req.name).await?;
    tx.commit().await?;
    Ok(Json(row))
}

async fn delete_timetable(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    let t = load_timetable(&mut tx, &user, id).await?;
    if t.status == "published" {
        return Err(ApiError::conflict(
            "Publish another timetable first; the school runs on this one.",
        ));
    }
    repo::delete_timetable(&mut tx, &state.hlc, user.actor(), id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn publish_timetable(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Timetable>> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    load_timetable(&mut tx, &user, id).await?;
    let row = repo::publish_timetable(&mut tx, &state.hlc, user.actor(), id).await?;
    tx.commit().await?;
    Ok(Json(row))
}

/// A cover duty of the teacher whose timetable is shown. Mirrored by `CoverDuty` in the UI.
#[derive(Debug, Clone, Serialize)]
pub struct CoverDuty {
    pub date: NaiveDate,
    pub shift_id: Uuid,
    pub period: i16,
    pub length: i16,
    pub lesson_id: Uuid,
    pub subject: String,
    pub sections: Vec<String>,
    pub absent_teacher: String,
}

/// A timetable with everything needed to draw it. Mirrored by `TimetableView` in the UI.
#[derive(Debug, Serialize)]
pub struct TimetableView {
    pub timetable: Option<Timetable>,
    pub academic_year: AcademicYear,
    pub shifts: Vec<Shift>,
    pub lessons: Vec<LessonView>,
    pub cards: Vec<Card>,
    pub rooms: Vec<Room>,
    pub sections: Vec<Named>,
    pub teachers: Vec<Named>,
    /// The independent checker's findings (empty = consistent).
    pub problems: Vec<String>,
    pub job: Option<JobView>,
    /// Cover duties (own timetable only).
    pub covers: Vec<CoverDuty>,
    /// Today's weekday, 0 = Monday (own timetable only).
    pub today: Option<u8>,
}

async fn build_view(
    c: &mut PgConnection,
    state: &AppState,
    user: &CurrentUser,
    year: AcademicYear,
    timetable_id: Option<Uuid>,
) -> ApiResult<TimetableView> {
    let ys = repo::setup_for_year(c, user.user.school_id, &year, timetable_id).await?;
    let cards = match &ys.timetable {
        Some(t) => repo::cards(c, t.id).await?,
        None => Vec::new(),
    };
    let problems = match &ys.setup.timetable {
        Some(t) => {
            let bend = ys.setup.settings.allow_relaxation;
            let enforce = engine::solver::verify::Enforce {
                same_day: !bend,
                subject_day: !bend,
                consecutive: !bend,
                per_day: !bend,
            };
            engine::solver::verify::check_with(&ys.setup, t, enforce)
        }
        None => Vec::new(),
    };
    Ok(TimetableView {
        job: ys
            .timetable
            .as_ref()
            .and_then(|t| state.timetable_jobs.running_for(t.id)),
        lessons: lesson_views(&ys),
        sections: section_names(&ys),
        teachers: teacher_names(&ys),
        timetable: ys.timetable,
        academic_year: year,
        shifts: ys.shifts,
        cards,
        rooms: ys.rooms,
        problems,
        covers: Vec::new(),
        today: None,
    })
}

async fn view_timetable(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<TimetableView>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    let t = load_timetable(&mut c, &user, id).await?;
    let year = year_of(&mut c, &user, Some(t.academic_year_id)).await?;
    Ok(Json(
        build_view(&mut c, &state, &user, year, Some(id)).await?,
    ))
}

/// The published timetable of the year (what the school runs on), or an empty view.
async fn current_timetable(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<YearQuery>,
) -> ApiResult<Json<TimetableView>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    let year = year_of(&mut c, &user, q.academic_year_id).await?;
    Ok(Json(build_view(&mut c, &state, &user, year, None).await?))
}

/// The logged-in teacher's own week from the published timetable, with cover duties.
async fn mine(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<TimetableView>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    let staff = staff_repo::find_by_user(&mut c, user.user.id)
        .await?
        .ok_or_else(|| ApiError::not_found("staff record for this login"))?;
    let year = year_of(&mut c, &user, None).await?;
    let mut view = build_view(&mut c, &state, &user, year.clone(), None).await?;
    view.lessons
        .retain(|l| l.lesson.teacher_ids.contains(&staff.id));
    let mine: Vec<Uuid> = view.lessons.iter().map(|l| l.lesson.id).collect();
    view.cards.retain(|c| mine.contains(&c.lesson_id));
    let today = crate::clock::today();
    view.today = Some(today.weekday().num_days_from_monday() as u8);
    // Cover duties this week and next.
    let ys = repo::setup_for_year(&mut c, user.user.school_id, &year, None).await?;
    let all_lessons = lesson_views(&ys);
    let teacher_name = |id: Uuid| {
        ys.setup
            .master
            .teachers
            .iter()
            .find(|t| t.id == id.to_string())
            .map(|t| t.name.clone())
            .unwrap_or_default()
    };
    let from = today - chrono::Duration::days(i64::from(today.weekday().num_days_from_monday()));
    let to = from + chrono::Duration::days(13);
    for cv in repo::list_covers(&mut c, year.id).await? {
        if cv.cover_staff_id == Some(staff.id) && cv.date >= from && cv.date <= to {
            if let Some(l) = all_lessons.iter().find(|l| l.lesson.id == cv.lesson_id) {
                view.covers.push(CoverDuty {
                    date: cv.date,
                    shift_id: cv.shift_id,
                    period: cv.period,
                    length: cv.length,
                    lesson_id: cv.lesson_id,
                    subject: l.subject.clone(),
                    sections: l.sections.clone(),
                    absent_teacher: teacher_name(cv.absent_staff_id),
                });
            }
        }
    }
    Ok(Json(view))
}

// ---- manual edits ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CardQuery {
    pub lesson_id: Uuid,
    pub number: u32,
}

async fn edit_setup(
    c: &mut PgConnection,
    user: &CurrentUser,
    id: Uuid,
) -> ApiResult<(AcademicYear, YearSetup)> {
    let t = load_timetable(c, user, id).await?;
    let year = year_of(c, user, Some(t.academic_year_id)).await?;
    let ys = repo::setup_for_year(c, user.user.school_id, &year, Some(id)).await?;
    Ok((year, ys))
}

async fn slot_options(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Query(q): Query<CardQuery>,
) -> ApiResult<Json<Vec<edit::SlotInfo>>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    let (_, ys) = edit_setup(&mut c, &user, id).await?;
    Ok(Json(
        edit::slot_options(&ys.setup, &q.lesson_id.to_string(), q.number).map_err(engine_err)?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct MoveRequest {
    pub lesson_id: Uuid,
    pub number: u32,
    pub day: u8,
    pub period: u8,
    #[serde(default)]
    pub room_id: Option<Uuid>,
}

async fn save_edit(
    c: &mut PgConnection,
    state: &AppState,
    user: &CurrentUser,
    timetable_id: Uuid,
    tt: engine::Timetable,
) -> ApiResult<()> {
    let placed: Vec<repo::PlacedInput> = tt
        .cards
        .iter()
        .filter_map(repo::PlacedInput::from_engine)
        .collect();
    repo::replace_cards(
        c,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        timetable_id,
        &placed,
        None,
    )
    .await?;
    Ok(())
}

async fn move_card(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<MoveRequest>,
) -> ApiResult<Json<TimetableView>> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    let (year, ys) = edit_setup(&mut tx, &user, id).await?;
    let tt = edit::move_card(
        &ys.setup,
        &req.lesson_id.to_string(),
        req.number,
        req.day,
        req.period,
        req.room_id.map(|r| r.to_string()),
    )
    .map_err(engine_err)?;
    save_edit(&mut tx, &state, &user, id, tt).await?;
    let view = build_view(&mut tx, &state, &user, year, Some(id)).await?;
    tx.commit().await?;
    Ok(Json(view))
}

async fn swap_options(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Query(q): Query<CardQuery>,
) -> ApiResult<Json<Vec<edit::SwapInfo>>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    let (_, ys) = edit_setup(&mut c, &user, id).await?;
    Ok(Json(
        edit::swap_options(&ys.setup, &q.lesson_id.to_string(), q.number).map_err(engine_err)?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct SwapRequest {
    pub lesson_id: Uuid,
    pub number: u32,
    pub with_lesson_id: Uuid,
    pub with_number: u32,
}

async fn swap_cards(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<SwapRequest>,
) -> ApiResult<Json<TimetableView>> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    let (year, ys) = edit_setup(&mut tx, &user, id).await?;
    let tt = edit::swap_cards(
        &ys.setup,
        &req.lesson_id.to_string(),
        req.number,
        &req.with_lesson_id.to_string(),
        req.with_number,
    )
    .map_err(engine_err)?;
    save_edit(&mut tx, &state, &user, id, tt).await?;
    let view = build_view(&mut tx, &state, &user, year, Some(id)).await?;
    tx.commit().await?;
    Ok(Json(view))
}

#[derive(Debug, Deserialize)]
pub struct LockRequest {
    pub lesson_id: Uuid,
    pub number: i32,
    pub locked: bool,
}

async fn lock_card(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<LockRequest>,
) -> ApiResult<Json<Card>> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    load_timetable(&mut tx, &user, id).await?;
    let row = repo::set_card_locked(
        &mut tx,
        &state.hlc,
        user.actor(),
        id,
        req.lesson_id,
        req.number,
        req.locked,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(row))
}

// ---- printouts ------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct PrintQuery {
    pub academic_year_id: Option<Uuid>,
    #[serde(default)]
    pub timetable_id: Option<Uuid>,
    /// `section`, `teacher` or `room`.
    pub kind: String,
    pub id: Uuid,
    #[serde(default)]
    pub lang: Option<String>,
}

async fn print_pdf(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<PrintQuery>,
) -> ApiResult<Response> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    let year = year_of(&mut c, &user, q.academic_year_id).await?;
    if let Some(t) = q.timetable_id {
        load_timetable(&mut c, &user, t).await?;
    }
    let school = madrasa_db::schools::get(&mut c, user.user.school_id).await?;
    let ys = repo::setup_for_year(&mut c, user.user.school_id, &year, q.timetable_id).await?;
    let lang = Lang::parse(q.lang.as_deref());
    let kind = reports::timetable::GridKind::parse(&q.kind)
        .ok_or_else(|| ApiError::validation("Print a section, a teacher or a room timetable."))?;
    let report = reports::timetable::grid(
        lang,
        &school.name,
        &year,
        &ys.setup,
        kind,
        &q.id.to_string(),
    )?;
    let bytes = reports::pdf::render(&report)?;
    Ok(crate::export::file_response(
        &format!("{}.pdf", report.file_stem()),
        bytes,
    ))
}

// ---- substitutions --------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct DayQuery {
    pub academic_year_id: Option<Uuid>,
    pub date: Option<NaiveDate>,
}

/// An absence with the teacher's name. Mirrored by `AbsenceView` in the UI.
#[derive(Debug, Serialize)]
pub struct AbsenceView {
    #[serde(flatten)]
    pub absence: Absence,
    pub staff_name: String,
}

/// Mirrored by `SubstitutionsView` in the UI.
#[derive(Debug, Serialize)]
pub struct SubstitutionsView {
    pub academic_year: AcademicYear,
    pub date: NaiveDate,
    pub day: subs::CoverDay,
    pub absences: Vec<AbsenceView>,
    pub counts: subs::CoverCounts,
    pub teachers: Vec<Named>,
    pub shifts: Vec<Shift>,
}

async fn substitutions_day(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<DayQuery>,
) -> ApiResult<Json<SubstitutionsView>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    let year = year_of(&mut c, &user, q.academic_year_id).await?;
    let date = q.date.unwrap_or_else(crate::clock::today);
    let ys = repo::setup_for_year(&mut c, user.user.school_id, &year, None).await?;
    let day =
        subs::cover_day(&ys.setup, &date.format("%Y-%m-%d").to_string()).map_err(engine_err)?;
    let counts = subs::cover_counts(&ys.setup);
    let absences = repo::list_absences(&mut c, year.id)
        .await?
        .into_iter()
        .map(|a| AbsenceView {
            staff_name: ys
                .staff
                .iter()
                .find(|s| s.id == a.staff_id)
                .map(|s| s.full_name.clone())
                .unwrap_or_default(),
            absence: a,
        })
        .collect();
    Ok(Json(SubstitutionsView {
        academic_year: year,
        date,
        day,
        absences,
        counts,
        teachers: teacher_names(&ys),
        shifts: ys.shifts,
    }))
}

#[derive(Debug, Deserialize)]
pub struct AbsenceRequest {
    pub academic_year_id: Option<Uuid>,
    pub staff_id: Uuid,
    pub from_date: NaiveDate,
    #[serde(default)]
    pub to_date: Option<NaiveDate>,
    #[serde(default)]
    pub periods: Vec<PeriodRef>,
    #[serde(default)]
    pub note: String,
}

async fn create_absence(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<AbsenceRequest>,
) -> ApiResult<(StatusCode, Json<Absence>)> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    let year = year_of(&mut tx, &user, req.academic_year_id).await?;
    if staff_repo::get(&mut tx, req.staff_id).await?.school_id != user.user.school_id {
        return Err(ApiError::not_found("staff"));
    }
    for p in &req.periods {
        load_shift(&mut tx, &user, p.shift_id).await?;
    }
    let input = repo::AbsenceInput {
        staff_id: req.staff_id,
        from_date: req.from_date,
        to_date: req.to_date.unwrap_or(req.from_date),
        periods: req.periods.clone(),
        note: req.note.clone(),
    };
    if input.to_date < input.from_date {
        return Err(ApiError::validation("The absence ends before it starts."));
    }
    let row = repo::create_absence(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        year.id,
        &input,
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(row)))
}

async fn delete_absence(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    let a = repo::get_absence(&mut tx, id).await?;
    if a.school_id != user.user.school_id {
        return Err(ApiError::not_found("absence"));
    }
    repo::delete_absence(&mut tx, &state.hlc, user.actor(), id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct CoverRequest {
    pub absence_id: Uuid,
    pub date: NaiveDate,
    pub lesson_id: Uuid,
    pub number: i32,
    /// `null` = no cover needed.
    #[serde(default)]
    pub cover_staff_id: Option<Uuid>,
    #[serde(default)]
    pub note: String,
}

/// Decide who covers a session: the session must be one the absence leaves uncovered that day.
async fn set_cover(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<CoverRequest>,
) -> ApiResult<Json<Cover>> {
    gate(&state, &user, true)?;
    let school = user.user.school_id;
    let mut tx = state.pool.begin().await?;
    let absence = repo::get_absence(&mut tx, req.absence_id).await?;
    if absence.school_id != school {
        return Err(ApiError::not_found("absence"));
    }
    let year = year_of(&mut tx, &user, Some(absence.academic_year_id)).await?;
    if let Some(t) = req.cover_staff_id {
        if staff_repo::get(&mut tx, t).await?.school_id != school {
            return Err(ApiError::not_found("staff"));
        }
    }
    let ys = repo::setup_for_year(&mut tx, school, &year, None).await?;
    let date = req.date.format("%Y-%m-%d").to_string();
    let day = subs::cover_day(&ys.setup, &date).map_err(engine_err)?;
    let need = day
        .needs
        .iter()
        .find(|n| {
            n.absence_id == absence.id.to_string()
                && n.lesson_id == req.lesson_id.to_string()
                && n.number == req.number as u32
        })
        .ok_or_else(|| ApiError::validation("That session does not need cover on this date."))?;
    if let Some(t) = req.cover_staff_id {
        let cand = need
            .candidates
            .iter()
            .find(|c| c.teacher_id == t.to_string());
        match cand {
            Some(c) if !c.suitable => {
                return Err(ApiError::validation(format!(
                    "{} cannot cover this session: {}",
                    c.name,
                    c.reasons.join("; ")
                )));
            }
            Some(_) => {}
            None => {
                return Err(ApiError::validation(
                    "This teacher is not on the list of possible covers.",
                ))
            }
        }
    }
    let shift_id: Uuid = need.shift.parse().map_err(|_| ApiError::internal())?;
    let row = repo::set_cover(
        &mut tx,
        &state.hlc,
        user.actor(),
        school,
        year.id,
        &repo::CoverInput {
            absence_id: absence.id,
            date: req.date,
            lesson_id: req.lesson_id,
            number: req.number,
            absent_staff_id: absence.staff_id,
            cover_staff_id: req.cover_staff_id,
            shift_id,
            period: need.period as i16,
            length: need.length as i16,
            note: req.note.clone(),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(row))
}

#[derive(Debug, Deserialize)]
pub struct ClearCoverQuery {
    pub absence_id: Uuid,
    pub date: NaiveDate,
    pub lesson_id: Uuid,
    pub number: i32,
}

async fn clear_cover(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<ClearCoverQuery>,
) -> ApiResult<StatusCode> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    let absence = repo::get_absence(&mut tx, q.absence_id).await?;
    if absence.school_id != user.user.school_id {
        return Err(ApiError::not_found("absence"));
    }
    repo::clear_cover(
        &mut tx,
        &state.hlc,
        user.actor(),
        q.absence_id,
        q.date,
        q.lesson_id,
        q.number,
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct SheetQuery {
    pub academic_year_id: Option<Uuid>,
    pub from: NaiveDate,
    pub to: NaiveDate,
    #[serde(default)]
    pub lang: Option<String>,
}

async fn sheet_days(
    c: &mut PgConnection,
    user: &CurrentUser,
    q: &SheetQuery,
) -> ApiResult<(AcademicYear, YearSetup, Vec<subs::SheetDay>)> {
    if q.to < q.from {
        return Err(ApiError::validation("The range ends before it starts."));
    }
    if (q.to - q.from).num_days() > 62 {
        return Err(ApiError::validation("Print at most two months at a time."));
    }
    let year = year_of(c, user, q.academic_year_id).await?;
    let ys = repo::setup_for_year(c, user.user.school_id, &year, None).await?;
    let days = subs::cover_sheet(
        &ys.setup,
        &q.from.format("%Y-%m-%d").to_string(),
        &q.to.format("%Y-%m-%d").to_string(),
    )
    .map_err(engine_err)?;
    Ok((year, ys, days))
}

async fn cover_sheet(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<SheetQuery>,
) -> ApiResult<Json<Vec<subs::SheetDay>>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    let (_, _, days) = sheet_days(&mut c, &user, &q).await?;
    Ok(Json(days))
}

async fn cover_sheet_pdf(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<SheetQuery>,
) -> ApiResult<Response> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    let school = madrasa_db::schools::get(&mut c, user.user.school_id).await?;
    let (year, _, days) = sheet_days(&mut c, &user, &q).await?;
    let lang = Lang::parse(q.lang.as_deref());
    let report =
        reports::timetable::cover_sheet_report(lang, &school.name, &year, q.from, q.to, &days);
    let bytes = reports::pdf::render(&report)?;
    Ok(crate::export::file_response(
        &format!("{}.pdf", report.file_stem()),
        bytes,
    ))
}
