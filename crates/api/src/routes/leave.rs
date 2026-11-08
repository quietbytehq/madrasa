//! Leave (Standard stage 2): staff leave types, balances with carry-over, requests with
//! approval (marking staff attendance), student leave requests (marking attendance sheets),
//! the approver setting and the monthly staff register. Every route needs the `leave`
//! feature and `leave.read`; changes need `leave.manage`; decisions on staff leave need
//! `leave.approve` or the configured approver. Teachers see their own requests and may
//! request their own leave; they may decide student leave for their sections.

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::{Datelike, NaiveDate, NaiveTime};
use madrasa_core::attendance as core_att;
use madrasa_core::edition::Feature;
use madrasa_core::leave::{Balance, HalfDay, RequestStatus};
use madrasa_core::permissions::Permission;
use madrasa_db::leave as repo;
use madrasa_db::leave::{
    BalanceRow, LeaveRequest, LeaveRequestView, LeaveType, StudentLeaveRequest, StudentLeaveView,
};
use madrasa_db::models::{AcademicYear, Staff};
use madrasa_db::{academics, staff as staff_repo, students};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::{ApiError, ApiResult, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/leave/types", get(list_types).post(create_type))
        .route("/leave/types/{id}", put(update_type).delete(delete_type))
        .route("/leave/settings", get(get_settings).put(put_settings))
        .route("/leave/balances", get(list_balances).put(put_balance))
        .route("/leave/balances/carry-over", post(run_carry_over))
        .route("/leave/requests", get(list_requests).post(create_request))
        .route("/leave/requests/{id}/decide", post(decide_request))
        .route("/leave/requests/{id}/withdraw", post(withdraw_request))
        .route(
            "/leave/students",
            get(list_student_requests).post(create_student_request),
        )
        .route("/leave/students/{id}/decide", post(decide_student_request))
        .route(
            "/leave/students/{id}/withdraw",
            post(withdraw_student_request),
        )
        .route("/leave/staff-register", get(staff_register))
}

// ---- helpers --------------------------------------------------------------------------------

fn gate(state: &AppState, user: &CurrentUser, perm: Permission) -> ApiResult<()> {
    state.require_feature(Feature::Leave)?;
    user.require(perm)
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

/// The staff record of the caller, if the login is linked to one.
async fn own_staff(c: &mut PgConnection, user: &CurrentUser) -> ApiResult<Option<Staff>> {
    Ok(staff_repo::find_by_user(c, user.user.id).await?)
}

/// Office users see everyone; others only themselves.
async fn staff_scope(c: &mut PgConnection, user: &CurrentUser) -> ApiResult<Option<Uuid>> {
    if user.can(Permission::LeaveManage) || user.can(Permission::LeaveApprove) {
        return Ok(None);
    }
    Ok(Some(own_staff(c, user).await?.map(|s| s.id).ok_or_else(
        || ApiError::not_found("staff record for this login"),
    )?))
}

/// `leave.approve`, or the configured approver.
async fn may_decide_staff(c: &mut PgConnection, user: &CurrentUser) -> ApiResult<bool> {
    if user.can(Permission::LeaveApprove) {
        return Ok(true);
    }
    let Some(approver) = repo::approver(c, user.user.school_id).await? else {
        return Ok(false);
    };
    Ok(own_staff(c, user).await?.is_some_and(|s| s.id == approver))
}

async fn load_staff(c: &mut PgConnection, user: &CurrentUser, id: Uuid) -> ApiResult<Staff> {
    let s = staff_repo::get(c, id).await?;
    if s.school_id != user.user.school_id {
        return Err(ApiError::not_found("staff"));
    }
    Ok(s)
}

fn parse_status(s: Option<&str>) -> ApiResult<Option<RequestStatus>> {
    match s {
        None | Some("") => Ok(None),
        Some(v) => v.parse().map(Some).map_err(|_| {
            ApiError::validation("Status is pending, approved, rejected or withdrawn.")
        }),
    }
}

// ---- types ----------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct TypeRequest {
    pub name: String,
    #[serde(default)]
    pub code: String,
    pub entitlement_days: Decimal,
    #[serde(default)]
    pub carry_over_max: Decimal,
    #[serde(default = "yes")]
    pub needs_approval: bool,
    #[serde(default)]
    pub position: i32,
}

fn yes() -> bool {
    true
}

impl TypeRequest {
    fn input(&self) -> ApiResult<repo::LeaveTypeInput> {
        if self.name.trim().is_empty() {
            return Err(ApiError::validation("The leave type needs a name."));
        }
        Ok(repo::LeaveTypeInput {
            name: self.name.clone(),
            code: self.code.clone(),
            entitlement_days: self.entitlement_days,
            carry_over_max: self.carry_over_max,
            needs_approval: self.needs_approval,
            position: self.position,
        })
    }
}

async fn list_types(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<LeaveType>>> {
    gate(&state, &user, Permission::LeaveRead)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(repo::list_types(&mut c, user.user.school_id).await?))
}

async fn create_type(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<TypeRequest>,
) -> ApiResult<(StatusCode, Json<LeaveType>)> {
    gate(&state, &user, Permission::LeaveManage)?;
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

async fn load_type(c: &mut PgConnection, user: &CurrentUser, id: Uuid) -> ApiResult<LeaveType> {
    let t = repo::get_type(c, id).await?;
    if t.school_id != user.user.school_id {
        return Err(ApiError::not_found("leave type"));
    }
    Ok(t)
}

async fn update_type(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<TypeRequest>,
) -> ApiResult<Json<LeaveType>> {
    gate(&state, &user, Permission::LeaveManage)?;
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
    gate(&state, &user, Permission::LeaveManage)?;
    let mut tx = state.pool.begin().await?;
    load_type(&mut tx, &user, id).await?;
    repo::delete_type(&mut tx, &state.hlc, user.actor(), id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- settings -------------------------------------------------------------------------------

/// Mirrored by `LeaveSettings` in the UI.
#[derive(Debug, Serialize, Deserialize)]
pub struct LeaveSettings {
    /// A staff member who may decide staff leave besides `leave.approve` holders.
    pub approver_staff_id: Option<Uuid>,
    pub approver_name: Option<String>,
}

async fn get_settings(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<LeaveSettings>> {
    gate(&state, &user, Permission::LeaveRead)?;
    let mut c = state.pool.acquire().await?;
    let id = repo::approver(&mut c, user.user.school_id).await?;
    let name = match id {
        Some(id) => staff_repo::get(&mut c, id).await.ok().map(|s| s.full_name),
        None => None,
    };
    Ok(Json(LeaveSettings {
        approver_staff_id: id,
        approver_name: name,
    }))
}

#[derive(Debug, Deserialize)]
pub struct SettingsRequest {
    pub approver_staff_id: Option<Uuid>,
}

async fn put_settings(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<SettingsRequest>,
) -> ApiResult<Json<LeaveSettings>> {
    gate(&state, &user, Permission::LeaveManage)?;
    let mut tx = state.pool.begin().await?;
    let name = match req.approver_staff_id {
        Some(id) => Some(load_staff(&mut tx, &user, id).await?.full_name),
        None => None,
    };
    repo::set_approver(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        req.approver_staff_id,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(LeaveSettings {
        approver_staff_id: req.approver_staff_id,
        approver_name: name,
    }))
}

// ---- balances -------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct BalanceQuery {
    pub academic_year_id: Option<Uuid>,
    pub staff_id: Option<Uuid>,
}

/// Mirrored by `LeaveBalance` in the UI.
#[derive(Debug, Serialize)]
pub struct BalanceView {
    #[serde(flatten)]
    pub row: BalanceRow,
    pub granted: Decimal,
    pub available: Decimal,
    pub remaining: Decimal,
}

fn balance_view(row: BalanceRow) -> BalanceView {
    let b: Balance = row.balance();
    BalanceView {
        granted: b.granted(),
        available: b.available(),
        remaining: b.remaining(),
        row,
    }
}

async fn list_balances(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<BalanceQuery>,
) -> ApiResult<Json<Vec<BalanceView>>> {
    gate(&state, &user, Permission::LeaveRead)?;
    let mut c = state.pool.acquire().await?;
    let year = year_of(&mut c, &user, q.academic_year_id).await?;
    let staff_id = match staff_scope(&mut c, &user).await? {
        Some(own) => Some(own),
        None => q.staff_id,
    };
    let rows = repo::balances(&mut c, user.user.school_id, year.id, staff_id).await?;
    Ok(Json(rows.into_iter().map(balance_view).collect()))
}

#[derive(Debug, Deserialize)]
pub struct BalanceRequest {
    pub academic_year_id: Option<Uuid>,
    pub staff_id: Uuid,
    pub leave_type_id: Uuid,
    #[serde(default)]
    pub entitlement: Option<Decimal>,
    #[serde(default)]
    pub carried_over: Decimal,
    #[serde(default)]
    pub adjustment: Decimal,
    #[serde(default)]
    pub note: String,
}

async fn put_balance(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<BalanceRequest>,
) -> ApiResult<Json<Vec<BalanceView>>> {
    gate(&state, &user, Permission::LeaveManage)?;
    let mut tx = state.pool.begin().await?;
    let year = year_of(&mut tx, &user, req.academic_year_id).await?;
    load_staff(&mut tx, &user, req.staff_id).await?;
    load_type(&mut tx, &user, req.leave_type_id).await?;
    repo::set_balance(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        year.id,
        &repo::BalanceInput {
            staff_id: req.staff_id,
            leave_type_id: req.leave_type_id,
            entitlement: req.entitlement,
            carried_over: req.carried_over,
            adjustment: req.adjustment,
            note: req.note.clone(),
        },
    )
    .await?;
    let rows = repo::balances(&mut tx, user.user.school_id, year.id, Some(req.staff_id)).await?;
    tx.commit().await?;
    Ok(Json(rows.into_iter().map(balance_view).collect()))
}

#[derive(Debug, Deserialize)]
pub struct CarryOverRequest {
    pub from_academic_year_id: Uuid,
    pub to_academic_year_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct CarryOverResult {
    pub written: usize,
}

async fn run_carry_over(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<CarryOverRequest>,
) -> ApiResult<Json<CarryOverResult>> {
    gate(&state, &user, Permission::LeaveManage)?;
    let mut tx = state.pool.begin().await?;
    let from = year_of(&mut tx, &user, Some(req.from_academic_year_id)).await?;
    let to = year_of(&mut tx, &user, Some(req.to_academic_year_id)).await?;
    if to.start_date <= from.start_date {
        return Err(ApiError::validation(
            "Carry over goes from one year into a later one.",
        ));
    }
    let written = repo::carry_over(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        from.id,
        to.id,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(CarryOverResult { written }))
}

// ---- staff requests ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RequestQuery {
    pub academic_year_id: Option<Uuid>,
    pub staff_id: Option<Uuid>,
    pub status: Option<String>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

/// Mirrored by `LeaveRequestsView` in the UI.
#[derive(Debug, Serialize)]
pub struct RequestsView {
    pub requests: Vec<LeaveRequestView>,
    /// The caller may decide staff requests.
    pub can_decide: bool,
    /// The caller's own staff id, when the login is linked to one.
    pub own_staff_id: Option<Uuid>,
}

async fn list_requests(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<RequestQuery>,
) -> ApiResult<Json<RequestsView>> {
    gate(&state, &user, Permission::LeaveRead)?;
    let mut c = state.pool.acquire().await?;
    let staff_id = match staff_scope(&mut c, &user).await? {
        Some(own) => Some(own),
        None => q.staff_id,
    };
    let requests = repo::list_requests(
        &mut c,
        user.user.school_id,
        &repo::RequestFilter {
            academic_year_id: q.academic_year_id,
            staff_id,
            status: parse_status(q.status.as_deref())?,
            from: q.from,
            to: q.to,
        },
    )
    .await?;
    Ok(Json(RequestsView {
        requests,
        can_decide: may_decide_staff(&mut c, &user).await?,
        own_staff_id: own_staff(&mut c, &user).await?.map(|s| s.id),
    }))
}

#[derive(Debug, Deserialize)]
pub struct NewRequestBody {
    /// Absent = the caller's own staff record.
    #[serde(default)]
    pub staff_id: Option<Uuid>,
    pub leave_type_id: Uuid,
    pub from_date: NaiveDate,
    #[serde(default)]
    pub to_date: Option<NaiveDate>,
    #[serde(default)]
    pub half_day: Option<String>,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub attachment_note: String,
}

async fn create_request(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<NewRequestBody>,
) -> ApiResult<(StatusCode, Json<LeaveRequest>)> {
    gate(&state, &user, Permission::LeaveRead)?;
    let mut tx = state.pool.begin().await?;
    let own = own_staff(&mut tx, &user).await?;
    let staff_id = match req.staff_id {
        Some(id) if own.as_ref().is_some_and(|s| s.id == id) => id,
        Some(id) => {
            user.require(Permission::LeaveManage)?;
            load_staff(&mut tx, &user, id).await?.id
        }
        None => own.map(|s| s.id).ok_or_else(|| {
            ApiError::validation("Choose the staff member; this login has no staff record.")
        })?,
    };
    let half_day = match req.half_day.as_deref() {
        None | Some("") => None,
        Some(h) => {
            Some(HalfDay::parse(h).ok_or_else(|| ApiError::validation("A half day is am or pm."))?)
        }
    };
    let row = repo::create_request(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        &repo::NewRequest {
            staff_id,
            leave_type_id: req.leave_type_id,
            from_date: req.from_date,
            to_date: req.to_date.unwrap_or(req.from_date),
            half_day,
            reason: req.reason.clone(),
            attachment_note: req.attachment_note.clone(),
            requested_by: user.user.id,
        },
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(row)))
}

#[derive(Debug, Deserialize)]
pub struct DecisionBody {
    /// `approved` or `rejected`.
    pub decision: String,
    #[serde(default)]
    pub note: String,
}

fn approve_flag(s: &str) -> ApiResult<bool> {
    match s {
        "approved" | "approve" => Ok(true),
        "rejected" | "reject" => Ok(false),
        _ => Err(ApiError::validation(
            "The decision is approved or rejected.",
        )),
    }
}

async fn decide_request(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<DecisionBody>,
) -> ApiResult<Json<LeaveRequest>> {
    gate(&state, &user, Permission::LeaveRead)?;
    let approve = approve_flag(&req.decision)?;
    let mut tx = state.pool.begin().await?;
    if !may_decide_staff(&mut tx, &user).await? {
        return Err(ApiError::forbidden());
    }
    let r = repo::get_request(&mut tx, id).await?;
    if r.school_id != user.user.school_id {
        return Err(ApiError::not_found("leave request"));
    }
    if own_staff(&mut tx, &user)
        .await?
        .is_some_and(|s| s.id == r.staff_id)
        && !user.can(Permission::LeaveApprove)
    {
        return Err(ApiError::validation(
            "You cannot decide your own leave request.",
        ));
    }
    let row = repo::decide_request(
        &mut tx,
        &state.hlc,
        user.actor(),
        id,
        approve,
        &req.note,
        user.user.id,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(row))
}

#[derive(Debug, Deserialize, Default)]
pub struct NoteBody {
    #[serde(default)]
    pub note: String,
}

async fn withdraw_request(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: Option<Json<NoteBody>>,
) -> ApiResult<Json<LeaveRequest>> {
    gate(&state, &user, Permission::LeaveRead)?;
    let note = body.map(|Json(b)| b.note).unwrap_or_default();
    let mut tx = state.pool.begin().await?;
    let r = repo::get_request(&mut tx, id).await?;
    if r.school_id != user.user.school_id {
        return Err(ApiError::not_found("leave request"));
    }
    let mine = own_staff(&mut tx, &user)
        .await?
        .is_some_and(|s| s.id == r.staff_id);
    if !mine && !user.can(Permission::LeaveManage) {
        return Err(ApiError::forbidden());
    }
    let row = repo::withdraw_request(&mut tx, &state.hlc, user.actor(), id, &note).await?;
    tx.commit().await?;
    Ok(Json(row))
}

// ---- student requests -------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct StudentQuery {
    pub academic_year_id: Option<Uuid>,
    pub student_id: Option<Uuid>,
    pub section_id: Option<Uuid>,
    pub status: Option<String>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

/// Mirrored by `StudentLeaveView` in the UI.
#[derive(Debug, Serialize)]
pub struct StudentRequestsView {
    pub requests: Vec<StudentLeaveView>,
    /// Sections the caller may decide for (`None` = every section).
    pub decide_sections: Option<Vec<Uuid>>,
    pub can_manage: bool,
}

/// Teachers: the sections they are class teacher of or teach; office: everything.
async fn student_scope(c: &mut PgConnection, user: &CurrentUser) -> ApiResult<Option<Vec<Uuid>>> {
    if user.can(Permission::LeaveManage) || user.can(Permission::LeaveApprove) {
        return Ok(None);
    }
    let Some(staff) = own_staff(c, user).await? else {
        return Ok(Some(Vec::new()));
    };
    Ok(Some(staff_repo::section_scope(c, staff.id).await?))
}

async fn list_student_requests(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<StudentQuery>,
) -> ApiResult<Json<StudentRequestsView>> {
    gate(&state, &user, Permission::LeaveRead)?;
    let mut c = state.pool.acquire().await?;
    let scope = student_scope(&mut c, &user).await?;
    let section_ids = match (&scope, q.section_id) {
        (Some(s), Some(id)) if s.contains(&id) => Some(vec![id]),
        (Some(s), _) => Some(s.clone()),
        (None, Some(id)) => Some(vec![id]),
        (None, None) => None,
    };
    if let Some(sid) = q.student_id {
        let s = students::get(&mut c, sid).await?;
        if s.school_id != user.user.school_id {
            return Err(ApiError::not_found("student"));
        }
    }
    let requests = repo::list_student_requests(
        &mut c,
        user.user.school_id,
        &repo::StudentFilter {
            academic_year_id: q.academic_year_id,
            student_id: q.student_id,
            section_ids,
            status: parse_status(q.status.as_deref())?,
            from: q.from,
            to: q.to,
        },
    )
    .await?;
    Ok(Json(StudentRequestsView {
        requests,
        decide_sections: scope,
        can_manage: user.can(Permission::LeaveManage),
    }))
}

#[derive(Debug, Deserialize)]
pub struct NewStudentBody {
    pub student_id: Uuid,
    pub from_date: NaiveDate,
    #[serde(default)]
    pub to_date: Option<NaiveDate>,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub guardian_id: Option<Uuid>,
}

async fn create_student_request(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<NewStudentBody>,
) -> ApiResult<(StatusCode, Json<StudentLeaveRequest>)> {
    gate(&state, &user, Permission::LeaveManage)?;
    let mut tx = state.pool.begin().await?;
    let s = students::get(&mut tx, req.student_id).await?;
    if s.school_id != user.user.school_id {
        return Err(ApiError::not_found("student"));
    }
    if let Some(g) = req.guardian_id {
        let linked = madrasa_db::guardians::for_student(&mut tx, s.id).await?;
        if !linked.iter().any(|x| x.id == g) {
            return Err(ApiError::validation(
                "The guardian is not linked to this student.",
            ));
        }
    }
    let row = repo::create_student_request(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        &repo::NewStudentRequest {
            student_id: s.id,
            from_date: req.from_date,
            to_date: req.to_date.unwrap_or(req.from_date),
            reason: req.reason.clone(),
            guardian_id: req.guardian_id,
            recorded_by: user.user.id,
        },
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(row)))
}

async fn may_decide_student(
    c: &mut PgConnection,
    user: &CurrentUser,
    r: &StudentLeaveRequest,
) -> ApiResult<bool> {
    match student_scope(c, user).await? {
        None => Ok(true),
        Some(sections) => {
            let section = students::enrollments(c, r.student_id)
                .await?
                .into_iter()
                .find(|e| e.academic_year_id == r.academic_year_id)
                .map(|e| e.section_id);
            Ok(section.is_some_and(|s| sections.contains(&s)))
        }
    }
}

async fn decide_student_request(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<DecisionBody>,
) -> ApiResult<Json<StudentLeaveRequest>> {
    gate(&state, &user, Permission::LeaveRead)?;
    let approve = approve_flag(&req.decision)?;
    let mut tx = state.pool.begin().await?;
    let r = repo::get_student_request(&mut tx, id).await?;
    if r.school_id != user.user.school_id {
        return Err(ApiError::not_found("student leave request"));
    }
    if !may_decide_student(&mut tx, &user, &r).await? {
        return Err(ApiError::forbidden());
    }
    let row = repo::decide_student_request(
        &mut tx,
        &state.hlc,
        user.actor(),
        id,
        approve,
        &req.note,
        user.user.id,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(row))
}

async fn withdraw_student_request(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: Option<Json<NoteBody>>,
) -> ApiResult<Json<StudentLeaveRequest>> {
    gate(&state, &user, Permission::LeaveManage)?;
    let note = body.map(|Json(b)| b.note).unwrap_or_default();
    let mut tx = state.pool.begin().await?;
    let r = repo::get_student_request(&mut tx, id).await?;
    if r.school_id != user.user.school_id {
        return Err(ApiError::not_found("student leave request"));
    }
    let row = repo::withdraw_student_request(&mut tx, &state.hlc, user.actor(), id, &note).await?;
    tx.commit().await?;
    Ok(Json(row))
}

// ---- staff register ----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct MonthQuery {
    /// `YYYY-MM`; default the current month.
    pub month: Option<String>,
}

/// Mirrored by `StaffRegisterCell` in the UI.
#[derive(Debug, Serialize)]
pub struct RegisterCell {
    pub status: String,
    pub arrived_at: Option<NaiveTime>,
    /// Marked by an approved leave request.
    pub approved_leave: bool,
}

/// Mirrored by `StaffRegisterRow` in the UI.
#[derive(Debug, Serialize)]
pub struct RegisterRow {
    pub staff_id: Uuid,
    pub staff_number: String,
    pub full_name: String,
    pub designation: String,
    /// One per day of the month; `None` = not marked.
    pub cells: Vec<Option<RegisterCell>>,
    pub present: u32,
    pub absent: u32,
    pub late: u32,
    pub leave: u32,
}

/// Mirrored by `StaffRegisterView` in the UI.
#[derive(Debug, Serialize)]
pub struct RegisterView {
    pub month: NaiveDate,
    pub days: Vec<NaiveDate>,
    /// Per day: whether it is a school day.
    pub school_days: Vec<bool>,
    pub rows: Vec<RegisterRow>,
}

async fn staff_register(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<MonthQuery>,
) -> ApiResult<Json<RegisterView>> {
    gate(&state, &user, Permission::LeaveRead)?;
    let today = crate::clock::today();
    let first = match q.month.as_deref() {
        Some(m) => NaiveDate::parse_from_str(&format!("{m}-01"), "%Y-%m-%d")
            .map_err(|_| ApiError::validation("month is YYYY-MM."))?,
        None => NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today),
    };
    let days = core_att::month_days(first);
    let last = *days.last().unwrap_or(&first);
    let mut c = state.pool.acquire().await?;
    let school = user.user.school_id;
    let cal = repo::calendar(&mut c, school, first, last).await?;
    let only = staff_scope(&mut c, &user).await?;
    let mut staff = staff_repo::list(&mut c, school, None).await?;
    staff.retain(|s| s.status == "active" && only.is_none_or(|id| id == s.id));
    let marks = repo::staff_marks(&mut c, school, first, last).await?;
    let mut by_staff: HashMap<Uuid, HashMap<NaiveDate, repo::StaffMark>> = HashMap::new();
    for m in marks {
        by_staff.entry(m.staff_id).or_default().insert(m.date, m);
    }
    let rows = staff
        .iter()
        .map(|s| {
            let mine = by_staff.get(&s.id);
            let cells: Vec<Option<RegisterCell>> = days
                .iter()
                .map(|d| {
                    mine.and_then(|m| m.get(d)).map(|m| RegisterCell {
                        status: m.status.clone(),
                        arrived_at: m.arrived_at,
                        approved_leave: m.leave_request_id.is_some(),
                    })
                })
                .collect();
            let count = |st: &str| cells.iter().flatten().filter(|c| c.status == st).count() as u32;
            RegisterRow {
                staff_id: s.id,
                staff_number: s.staff_number.clone(),
                full_name: s.full_name.clone(),
                designation: s.designation.clone(),
                present: count("present"),
                absent: count("absent"),
                late: count("late"),
                leave: count("leave"),
                cells,
            }
        })
        .collect();
    Ok(Json(RegisterView {
        month: first,
        school_days: days.iter().map(|d| cal.is_working(*d)).collect(),
        days,
        rows,
    }))
}
