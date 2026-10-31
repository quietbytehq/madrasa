//! Attendance: school calendar, daily sheets per section, corrections with history, student
//! history, monthly register, absentee list, daily status, staff attendance and the dashboard
//! summary. Teachers are limited to their own sections (`CurrentUser::section_scope`).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{Datelike, Duration, NaiveDate};
use madrasa_core::attendance::{self, AttendanceStatus, AttendanceSummary, DayKind, StaffStatus};
use madrasa_core::permissions::Permission;
use madrasa_db::models::{
    AbsenteeRow, AttendanceSheet, Holiday, RosterRow, SectionDayStatus, StaffDayRow, StudentDayRow,
};
use madrasa_db::{academics, attendance as repo, students, users};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::{ApiError, ApiResult, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/attendance/calendar", get(get_calendar))
        .route(
            "/attendance/calendar/weekend",
            axum::routing::put(put_weekend),
        )
        .route("/attendance/calendar/holidays", post(create_holiday))
        .route(
            "/attendance/calendar/holidays/{id}",
            axum::routing::delete(delete_holiday),
        )
        .route("/attendance/sheet", get(get_sheet).put(put_sheet))
        .route("/attendance/sheet/history", get(sheet_history))
        .route("/attendance/status", get(status))
        .route("/attendance/absentees", get(absentees))
        .route("/attendance/register", get(register))
        .route("/attendance/students/{id}", get(student_attendance))
        .route("/attendance/staff", get(staff_day).put(put_staff_day))
        .route("/attendance/staff/{id}", get(staff_history))
        .route("/dashboard", get(dashboard))
}

type JsonBody<T> = Result<Json<T>, axum::extract::rejection::JsonRejection>;

fn today() -> NaiveDate {
    chrono::Utc::now().date_naive()
}

fn parse_status(s: &str) -> ApiResult<AttendanceStatus> {
    s.parse()
        .map_err(|_| ApiError::validation("Status must be present, absent, late or leave."))
}

/// Section of the caller's school, within the teacher scope.
async fn load_section(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    section_id: Uuid,
) -> ApiResult<madrasa_db::models::Section> {
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

// ---- calendar -----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CalendarQuery {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

#[derive(Debug, Serialize)]
pub struct CalendarView {
    pub weekend_days: Vec<u32>,
    pub holidays: Vec<Holiday>,
}

async fn calendar_view(
    c: &mut sqlx::PgConnection,
    school_id: Uuid,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
) -> ApiResult<CalendarView> {
    Ok(CalendarView {
        weekend_days: repo::weekend_days(c, school_id).await?,
        holidays: repo::holidays(c, school_id, from, to).await?,
    })
}

async fn get_calendar(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<CalendarQuery>,
) -> ApiResult<Json<CalendarView>> {
    user.require(Permission::AttendanceRead)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        calendar_view(&mut c, user.user.school_id, q.from, q.to).await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct WeekendRequest {
    pub weekend_days: Vec<u32>,
}

async fn put_weekend(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<WeekendRequest>,
) -> ApiResult<Json<CalendarView>> {
    user.require(Permission::AcademicsManage)?;
    let Json(req) = body?;
    attendance::validate_weekend(&req.weekend_days).map_err(ApiError::validation)?;
    let mut c = state.pool.acquire().await?;
    repo::set_weekend_days(
        &mut c,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        &req.weekend_days,
    )
    .await?;
    Ok(Json(
        calendar_view(&mut c, user.user.school_id, None, None).await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct HolidayRequest {
    pub name: String,
    pub starts_on: NaiveDate,
    pub ends_on: Option<NaiveDate>,
}

async fn create_holiday(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<HolidayRequest>,
) -> ApiResult<(StatusCode, Json<Holiday>)> {
    user.require(Permission::AcademicsManage)?;
    let Json(req) = body?;
    if req.name.trim().is_empty() {
        return Err(ApiError::validation("Holiday name is required."));
    }
    let ends_on = req.ends_on.unwrap_or(req.starts_on);
    if ends_on < req.starts_on {
        return Err(ApiError::validation(
            "The end date must not be before the start date.",
        ));
    }
    let mut c = state.pool.acquire().await?;
    let h = repo::create_holiday(
        &mut c,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        &req.name,
        req.starts_on,
        ends_on,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(h)))
}

async fn delete_holiday(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    user.require(Permission::AcademicsManage)?;
    let mut c = state.pool.acquire().await?;
    let mine = repo::holidays(&mut c, user.user.school_id, None, None)
        .await?
        .iter()
        .any(|h| h.id == id);
    if !mine {
        return Err(ApiError::not_found("holiday"));
    }
    repo::delete_holiday(&mut c, &state.hlc, user.actor(), id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- sheets -------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SheetQuery {
    pub section_id: Uuid,
    pub date: NaiveDate,
}

#[derive(Debug, Serialize)]
pub struct SectionHeader {
    pub id: Uuid,
    pub class_id: Uuid,
    pub class_name: String,
    pub section_name: String,
    pub class_teacher_name: Option<String>,
}

/// Mirrored by `SheetView` in the UI.
#[derive(Debug, Serialize)]
pub struct SheetView {
    pub section: SectionHeader,
    pub date: NaiveDate,
    pub day_kind: DayKind,
    pub sheet: Option<AttendanceSheet>,
    pub marked_by_name: Option<String>,
    pub students: Vec<RosterRow>,
    /// The caller may save this sheet (first submission, or a correction they are allowed).
    pub can_edit: bool,
    /// Saving now would be a correction of a submitted sheet.
    pub is_correction: bool,
}

async fn section_header(
    c: &mut sqlx::PgConnection,
    section: &madrasa_db::models::Section,
) -> ApiResult<SectionHeader> {
    let class = academics::get_class(c, section.class_id).await?;
    let teacher = match section.class_teacher_id {
        Some(id) => Some(madrasa_db::staff::get(c, id).await?.full_name),
        None => None,
    };
    Ok(SectionHeader {
        id: section.id,
        class_id: class.id,
        class_name: class.name,
        section_name: section.name.clone(),
        class_teacher_name: teacher,
    })
}

async fn sheet_view(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    section: madrasa_db::models::Section,
    date: NaiveDate,
) -> ApiResult<SheetView> {
    let day_kind = repo::day_kind(c, user.user.school_id, date).await?;
    let sheet = repo::sheet(c, section.id, date).await?;
    let marked_by_name = match sheet.as_ref().and_then(|s| s.marked_by) {
        Some(id) => users::get(c, id).await.ok().map(|u| u.full_name),
        None => None,
    };
    let students = repo::roster(c, section.id, date).await?;
    let is_correction = sheet.is_some();
    let can_edit = day_kind.is_school_day()
        && user.can(Permission::AttendanceManage)
        && (!is_correction || user.can(Permission::AttendanceCorrect));
    Ok(SheetView {
        section: section_header(c, &section).await?,
        date,
        day_kind,
        sheet,
        marked_by_name,
        students,
        can_edit,
        is_correction,
    })
}

async fn get_sheet(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<SheetQuery>,
) -> ApiResult<Json<SheetView>> {
    user.require(Permission::AttendanceRead)?;
    let mut c = state.pool.acquire().await?;
    let section = load_section(&mut c, &user, q.section_id).await?;
    Ok(Json(sheet_view(&mut c, &user, section, q.date).await?))
}

#[derive(Debug, Deserialize)]
pub struct EntryRequest {
    pub student_id: Uuid,
    pub status: String,
    pub note: Option<String>,
}

/// Mirrored by `SaveSheetRequest` in the UI. Students not listed are present.
#[derive(Debug, Deserialize)]
pub struct SaveSheetRequest {
    pub section_id: Uuid,
    pub date: NaiveDate,
    pub entries: Vec<EntryRequest>,
}

async fn put_sheet(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<SaveSheetRequest>,
) -> ApiResult<Json<SheetView>> {
    user.require(Permission::AttendanceManage)?;
    let Json(req) = body?;
    if req.date > today() {
        return Err(ApiError::validation(
            "Attendance cannot be marked for a future date.",
        ));
    }
    let mut entries = Vec::with_capacity(req.entries.len());
    for e in &req.entries {
        entries.push(repo::EntryInput {
            student_id: e.student_id,
            status: parse_status(&e.status)?,
            note: e.note.clone(),
        });
    }
    let mut tx = state.pool.begin().await?;
    let section = load_section(&mut tx, &user, req.section_id).await?;
    let already = repo::sheet(&mut tx, section.id, req.date).await?.is_some();
    let correcting = user.can(Permission::AttendanceCorrect);
    if already && !correcting {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "already_submitted",
            "Attendance for this day is already submitted. Ask an administrator to correct it.",
        ));
    }
    repo::save_sheet(
        &mut tx,
        &state.hlc,
        user.actor(),
        repo::SaveSheet {
            school_id: user.user.school_id,
            section_id: section.id,
            date: req.date,
            marked_by: user.user.id,
            entries,
        },
        correcting,
    )
    .await?;
    let view = sheet_view(&mut tx, &user, section, req.date).await?;
    tx.commit().await?;
    Ok(Json(view))
}

/// Mirrored by `SheetHistoryEntry` in the UI.
#[derive(Debug, Serialize)]
pub struct SheetHistoryEntry {
    pub at: chrono::DateTime<chrono::Utc>,
    pub actor_name: Option<String>,
    pub action: String,
    pub entity: String,
    pub student_name: Option<String>,
    pub old_value: Option<serde_json::Value>,
    pub new_value: Option<serde_json::Value>,
}

async fn sheet_history(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<SheetQuery>,
) -> ApiResult<Json<Vec<SheetHistoryEntry>>> {
    user.require(Permission::AttendanceRead)?;
    let mut c = state.pool.acquire().await?;
    let section = load_section(&mut c, &user, q.section_id).await?;
    let Some(sheet) = repo::sheet(&mut c, section.id, q.date).await? else {
        return Ok(Json(Vec::new()));
    };
    let mut out = Vec::new();
    let mut names: std::collections::HashMap<Uuid, String> = Default::default();
    for e in repo::sheet_history(&mut c, sheet.id).await? {
        let actor_name = match e.actor_user_id {
            Some(id) => match names.get(&id) {
                Some(n) => Some(n.clone()),
                None => {
                    let n = users::get(&mut c, id).await.ok().map(|u| u.full_name);
                    if let Some(n) = &n {
                        names.insert(id, n.clone());
                    }
                    n
                }
            },
            None => None,
        };
        let student_id = e
            .new_value
            .as_ref()
            .and_then(|v| v["student_id"].as_str())
            .and_then(|s| s.parse::<Uuid>().ok());
        let student_name = match student_id {
            Some(id) => students::get(&mut c, id).await.ok().map(|s| s.full_name),
            None => None,
        };
        out.push(SheetHistoryEntry {
            at: e.at,
            actor_name,
            action: e.action,
            entity: e.entity,
            student_name,
            old_value: e.old_value,
            new_value: e.new_value,
        });
    }
    Ok(Json(out))
}

// ---- status / absentees / register ---------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct DateQuery {
    pub date: Option<NaiveDate>,
}

/// Mirrored by `StatusTotals` in the UI.
#[derive(Debug, Serialize, Default)]
pub struct StatusTotals {
    pub sections: usize,
    pub marked: usize,
    pub enrolled: i64,
    pub present: i64,
    pub absent: i64,
    pub late: i64,
    pub leave: i64,
    /// (present + late) / marked students, one decimal; `None` until a sheet is submitted.
    pub percent: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct StatusView {
    pub date: NaiveDate,
    pub day_kind: DayKind,
    pub sections: Vec<SectionDayStatus>,
    pub totals: StatusTotals,
}

fn totals_of(sections: &[SectionDayStatus]) -> StatusTotals {
    let mut t = StatusTotals {
        sections: sections.len(),
        ..Default::default()
    };
    for s in sections {
        t.enrolled += s.enrolled;
        if s.sheet_id.is_some() {
            t.marked += 1;
            t.present += i64::from(s.present.unwrap_or(0));
            t.absent += i64::from(s.absent.unwrap_or(0));
            t.late += i64::from(s.late.unwrap_or(0));
            t.leave += i64::from(s.leave.unwrap_or(0));
        }
    }
    let marked_students = t.present + t.absent + t.late + t.leave;
    if marked_students > 0 {
        t.percent =
            Some(((t.present + t.late) as f64 * 1000.0 / marked_students as f64).round() / 10.0);
    }
    t
}

async fn status_view(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    date: NaiveDate,
) -> ApiResult<StatusView> {
    let day_kind = repo::day_kind(c, user.user.school_id, date).await?;
    let scope = user.section_scope(c).await?;
    let sections = match academics::active_year(c, user.user.school_id).await? {
        Some(y) => repo::daily_status(c, user.user.school_id, y.id, date, scope.as_deref()).await?,
        None => Vec::new(),
    };
    let totals = totals_of(&sections);
    Ok(StatusView {
        date,
        day_kind,
        sections,
        totals,
    })
}

async fn status(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<DateQuery>,
) -> ApiResult<Json<StatusView>> {
    user.require(Permission::AttendanceRead)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        status_view(&mut c, &user, q.date.unwrap_or_else(today)).await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct AbsenteeQuery {
    pub date: Option<NaiveDate>,
    pub class_id: Option<Uuid>,
}

async fn absentees(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<AbsenteeQuery>,
) -> ApiResult<Json<Vec<AbsenteeRow>>> {
    user.require(Permission::AttendanceRead)?;
    let mut c = state.pool.acquire().await?;
    let scope = user.section_scope(&mut c).await?;
    Ok(Json(
        repo::absentees(
            &mut c,
            user.user.school_id,
            q.date.unwrap_or_else(today),
            q.class_id,
            scope.as_deref(),
        )
        .await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct RegisterQuery {
    pub section_id: Uuid,
    /// Any date in the month.
    pub month: NaiveDate,
}

#[derive(Debug, Serialize)]
pub struct RegisterDay {
    pub date: NaiveDate,
    pub kind: DayKind,
    pub marked: bool,
}

#[derive(Debug, Serialize)]
pub struct RegisterRow {
    pub student_id: Uuid,
    pub student_number: String,
    pub full_name: String,
    pub roll_no: Option<i32>,
    /// One per day of the month, `None` where no sheet exists or the student has no entry.
    pub statuses: Vec<Option<String>>,
    pub summary: AttendanceSummary,
}

/// Mirrored by `RegisterView` in the UI.
#[derive(Debug, Serialize)]
pub struct RegisterView {
    pub section: SectionHeader,
    pub month: NaiveDate,
    pub days: Vec<RegisterDay>,
    pub rows: Vec<RegisterRow>,
    pub school_days: usize,
}

async fn register(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<RegisterQuery>,
) -> ApiResult<Json<RegisterView>> {
    user.require(Permission::AttendanceRead)?;
    let mut c = state.pool.acquire().await?;
    let section = load_section(&mut c, &user, q.section_id).await?;
    let (weekend, holidays) = repo::calendar(&mut c, user.user.school_id).await?;
    let days = attendance::month_days(q.month);
    let (first, last) = (days[0], *days.last().unwrap_or(&days[0]));
    let entries = repo::section_entries(&mut c, section.id, first, last).await?;
    let roster = repo::roster(&mut c, section.id, first).await?;
    let marked_dates: std::collections::HashSet<NaiveDate> =
        entries.iter().map(|e| e.date).collect();
    let day_views: Vec<RegisterDay> = days
        .iter()
        .map(|d| RegisterDay {
            date: *d,
            kind: attendance::day_kind(*d, &weekend, &holidays),
            marked: marked_dates.contains(d),
        })
        .collect();
    let school_days = day_views.iter().filter(|d| d.kind.is_school_day()).count();
    let mut by_student: std::collections::HashMap<Uuid, Vec<Option<String>>> = roster
        .iter()
        .map(|r| (r.student_id, vec![None; days.len()]))
        .collect();
    for e in &entries {
        let idx = (e.date - first).num_days() as usize;
        if let Some(row) = by_student.get_mut(&e.student_id) {
            if idx < row.len() {
                row[idx] = Some(e.status.clone());
            }
        }
    }
    let rows = roster
        .into_iter()
        .map(|r| {
            let statuses = by_student.remove(&r.student_id).unwrap_or_default();
            let summary = attendance::summarize(
                statuses
                    .iter()
                    .flatten()
                    .filter_map(|s| s.parse::<AttendanceStatus>().ok()),
            );
            RegisterRow {
                student_id: r.student_id,
                student_number: r.student_number,
                full_name: r.full_name,
                roll_no: r.roll_no,
                statuses,
                summary,
            }
        })
        .collect();
    Ok(Json(RegisterView {
        section: section_header(&mut c, &section).await?,
        month: first,
        days: day_views,
        rows,
        school_days,
    }))
}

// ---- student history ----------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RangeQuery {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

/// Mirrored by `StudentAttendanceView` in the UI.
#[derive(Debug, Serialize)]
pub struct StudentAttendanceView {
    pub student_id: Uuid,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub summary: AttendanceSummary,
    pub percent: Option<f64>,
    pub days: Vec<StudentDayRow>,
}

async fn student_attendance(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Query(q): Query<RangeQuery>,
) -> ApiResult<Json<StudentAttendanceView>> {
    user.require(Permission::AttendanceRead)?;
    let mut c = state.pool.acquire().await?;
    let student = students::get(&mut c, id).await?;
    if student.school_id != user.user.school_id {
        return Err(ApiError::not_found("student"));
    }
    let year = academics::active_year(&mut c, user.user.school_id).await?;
    if let Some(scope) = user.section_scope(&mut c).await? {
        let current = match &year {
            Some(y) => students::section_in_year(&mut c, id, y.id).await?,
            None => None,
        };
        if !current.is_some_and(|s| scope.contains(&s)) {
            return Err(ApiError::forbidden());
        }
    }
    let to = q.to.unwrap_or_else(today);
    let from = q.from.unwrap_or_else(|| {
        year.as_ref()
            .map(|y| y.start_date)
            .unwrap_or(to - Duration::days(90))
    });
    if from > to {
        return Err(ApiError::validation(
            "The range start must not be after its end.",
        ));
    }
    let summary = repo::student_summary(&mut c, id, from, to).await?;
    let days = repo::student_history(&mut c, id, from, to).await?;
    Ok(Json(StudentAttendanceView {
        student_id: id,
        from,
        to,
        percent: summary.percent(),
        summary,
        days,
    }))
}

// ---- staff --------------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct StaffDayView {
    pub date: NaiveDate,
    pub day_kind: DayKind,
    pub rows: Vec<StaffDayRow>,
    pub can_edit: bool,
}

async fn staff_day_view(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    date: NaiveDate,
) -> ApiResult<StaffDayView> {
    let day_kind = repo::day_kind(c, user.user.school_id, date).await?;
    let rows = repo::staff_day(c, user.user.school_id, date).await?;
    Ok(StaffDayView {
        date,
        can_edit: day_kind.is_school_day()
            && user.can(Permission::TeachersManage)
            && date <= today(),
        day_kind,
        rows,
    })
}

async fn staff_day(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<DateQuery>,
) -> ApiResult<Json<StaffDayView>> {
    user.require(Permission::TeachersRead)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        staff_day_view(&mut c, &user, q.date.unwrap_or_else(today)).await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct StaffEntryRequest {
    pub staff_id: Uuid,
    pub status: String,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SaveStaffDayRequest {
    pub date: NaiveDate,
    pub entries: Vec<StaffEntryRequest>,
}

async fn put_staff_day(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<SaveStaffDayRequest>,
) -> ApiResult<Json<StaffDayView>> {
    user.require(Permission::TeachersManage)?;
    let Json(req) = body?;
    if req.date > today() {
        return Err(ApiError::validation(
            "Attendance cannot be marked for a future date.",
        ));
    }
    let mut entries = Vec::with_capacity(req.entries.len());
    for e in &req.entries {
        entries.push(repo::StaffEntryInput {
            staff_id: e.staff_id,
            status: e
                .status
                .parse::<StaffStatus>()
                .map_err(|_| ApiError::validation("Status must be present, absent or leave."))?,
            note: e.note.clone(),
        });
    }
    let mut tx = state.pool.begin().await?;
    repo::save_staff_day(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        req.date,
        user.user.id,
        &entries,
    )
    .await?;
    let view = staff_day_view(&mut tx, &user, req.date).await?;
    tx.commit().await?;
    Ok(Json(view))
}

#[derive(Debug, Serialize, Default)]
pub struct StaffSummary {
    pub present: u32,
    pub absent: u32,
    pub leave: u32,
}

#[derive(Debug, Serialize)]
pub struct StaffHistoryView {
    pub staff_id: Uuid,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub summary: StaffSummary,
    pub days: Vec<madrasa_db::models::StaffAttendance>,
}

async fn staff_history(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Query(q): Query<RangeQuery>,
) -> ApiResult<Json<StaffHistoryView>> {
    user.require(Permission::TeachersRead)?;
    let mut c = state.pool.acquire().await?;
    let st = madrasa_db::staff::get(&mut c, id).await?;
    if st.school_id != user.user.school_id {
        return Err(ApiError::not_found("staff member"));
    }
    let to = q.to.unwrap_or_else(today);
    let from = q.from.unwrap_or(to - Duration::days(90));
    let days = repo::staff_history(&mut c, id, from, to).await?;
    let mut summary = StaffSummary::default();
    for d in &days {
        match d.status.as_str() {
            "present" => summary.present += 1,
            "absent" => summary.absent += 1,
            "leave" => summary.leave += 1,
            _ => {}
        }
    }
    Ok(Json(StaffHistoryView {
        staff_id: id,
        from,
        to,
        summary,
        days,
    }))
}

// ---- dashboard ----------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct UnmarkedSection {
    pub section_id: Uuid,
    pub class_name: String,
    pub section_name: String,
    pub class_teacher_name: Option<String>,
}

/// Mirrored by `DashboardView` in the UI. Attendance is `None` when the caller cannot read it;
/// students, staff and fees figures are `None` without the matching read permission.
#[derive(Debug, Serialize)]
pub struct DashboardView {
    pub date: NaiveDate,
    pub day_kind: DayKind,
    pub attendance: Option<DashboardAttendance>,
    pub backup: crate::backup::BackupStatus,
    pub students_active: Option<i64>,
    pub staff_active: Option<i64>,
    pub fees: Option<DashboardFees>,
}

#[derive(Debug, Serialize)]
pub struct DashboardFees {
    /// Net receipts from the first of this month to today.
    pub collected_this_month: rust_decimal::Decimal,
    pub month_start: NaiveDate,
    /// Students with a balance past its due date today.
    pub defaulters: usize,
    pub outstanding: rust_decimal::Decimal,
}

#[derive(Debug, Serialize)]
pub struct DashboardAttendance {
    pub totals: StatusTotals,
    pub unmarked: Vec<UnmarkedSection>,
}

async fn dashboard(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<DashboardView>> {
    let mut c = state.pool.acquire().await?;
    let date = today();
    let day_kind = repo::day_kind(&mut c, user.user.school_id, date).await?;
    let attendance = if user.can(Permission::AttendanceRead) {
        let view = status_view(&mut c, &user, date).await?;
        let unmarked = view
            .sections
            .iter()
            .filter(|s| s.sheet_id.is_none() && s.enrolled > 0)
            .map(|s| UnmarkedSection {
                section_id: s.section_id,
                class_name: s.class_name.clone(),
                section_name: s.section_name.clone(),
                class_teacher_name: s.class_teacher_name.clone(),
            })
            .collect();
        Some(DashboardAttendance {
            totals: view.totals,
            unmarked,
        })
    } else {
        None
    };
    let backup = crate::backup::status(&mut c, user.user.school_id).await?;
    let students_active = if user.can(Permission::StudentsRead) {
        Some(students::count(&mut c, user.user.school_id, Some("active")).await?)
    } else {
        None
    };
    let staff_active = if user.can(Permission::TeachersRead) {
        Some(madrasa_db::staff::count(&mut c, user.user.school_id).await?)
    } else {
        None
    };
    let fees = if user.can(Permission::FeesRead) {
        let month_start = NaiveDate::from_ymd_opt(date.year(), date.month(), 1).unwrap_or(date);
        let collected =
            madrasa_db::fees::collected_between(&mut c, user.user.school_id, month_start, date)
                .await?;
        let defaulters =
            madrasa_db::fees::defaulters(&mut c, user.user.school_id, date, None, 1).await?;
        Some(DashboardFees {
            collected_this_month: collected,
            month_start,
            defaulters: defaulters.len(),
            outstanding: madrasa_core::fees::money(defaulters.iter().map(|d| d.balance).sum()),
        })
    } else {
        None
    };
    Ok(Json(DashboardView {
        date,
        day_kind,
        attendance,
        backup,
        students_active,
        staff_active,
        fees,
    }))
}
