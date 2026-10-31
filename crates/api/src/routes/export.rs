//! Excel/CSV export of every list, with the same permissions and teacher scope as the list.

use axum::extract::{Query, State};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use chrono::NaiveDate;
use madrasa_core::attendance;
use madrasa_core::permissions::Permission;
use madrasa_db::{academics, attendance as att_repo, fees, staff, students};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::export::{build, file_response};
use crate::{ApiError, ApiResult, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/export/students", get(students_list))
        .route("/export/staff", get(staff_list))
        .route("/export/fees/vouchers", get(vouchers))
        .route("/export/fees/defaulters", get(defaulters))
        .route("/export/attendance/absentees", get(absentees))
        .route("/export/attendance/register", get(register))
        .route("/export/exams/results", get(results))
}

fn s(v: impl ToString) -> String {
    v.to_string()
}
fn opt<T: ToString>(v: Option<T>) -> String {
    v.map(|x| x.to_string()).unwrap_or_default()
}
fn h(items: &[&str]) -> Vec<String> {
    items.iter().map(|x| x.to_string()).collect()
}

#[derive(Debug, Deserialize)]
pub struct StudentsQuery {
    pub format: Option<String>,
    pub q: Option<String>,
    pub class_id: Option<Uuid>,
    pub section_id: Option<Uuid>,
    pub status: Option<String>,
}

async fn students_list(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<StudentsQuery>,
) -> ApiResult<Response> {
    let mut c = state.pool.acquire().await?;
    let scope = user.student_scope(&mut c).await?;
    let page = students::list(
        &mut c,
        user.user.school_id,
        &students::StudentFilter {
            q: q.q.clone(),
            class_id: q.class_id,
            section_id: q.section_id,
            status: q.status.clone(),
            section_scope: match scope {
                crate::auth::StudentScope::All => None,
                crate::auth::StudentScope::Sections(ids) => Some(ids),
            },
        },
        students::StudentSort::Class,
        false,
        1,
        100_000,
    )
    .await?;
    let headers = h(&[
        "Student number",
        "Name",
        "Father's name",
        "Gender",
        "Date of birth",
        "Class",
        "Section",
        "Roll no.",
        "Status",
        "Admission date",
    ]);
    let rows: Vec<Vec<String>> = page
        .items
        .iter()
        .map(|r| {
            vec![
                s(&r.student_number),
                s(&r.full_name),
                s(&r.father_name),
                s(&r.gender),
                s(r.date_of_birth),
                opt(r.class_name.clone()),
                opt(r.section_name.clone()),
                opt(r.roll_no),
                s(&r.status),
                s(r.admission_date),
            ]
        })
        .collect();
    let (name, bytes) = build(
        "students",
        q.format.as_deref().unwrap_or("xlsx"),
        &headers,
        &rows,
    )?;
    Ok(file_response(&name, bytes))
}

#[derive(Debug, Deserialize)]
pub struct StaffQuery {
    pub format: Option<String>,
    pub q: Option<String>,
}

async fn staff_list(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<StaffQuery>,
) -> ApiResult<Response> {
    user.require(Permission::TeachersRead)?;
    let mut c = state.pool.acquire().await?;
    let list = staff::list(&mut c, user.user.school_id, q.q.as_deref()).await?;
    let headers = h(&[
        "Staff number",
        "Name",
        "Father's name",
        "Gender",
        "Phone",
        "Email",
        "CNIC",
        "Designation",
        "Joined on",
        "Status",
    ]);
    let rows: Vec<Vec<String>> = list
        .iter()
        .map(|r| {
            vec![
                s(&r.staff_number),
                s(&r.full_name),
                s(&r.father_name),
                s(&r.gender),
                opt(r.phone.clone()),
                opt(r.email.clone()),
                opt(r.cnic.clone()),
                s(&r.designation),
                opt(r.joined_on),
                s(&r.status),
            ]
        })
        .collect();
    let (name, bytes) = build(
        "staff",
        q.format.as_deref().unwrap_or("xlsx"),
        &headers,
        &rows,
    )?;
    Ok(file_response(&name, bytes))
}

#[derive(Debug, Deserialize)]
pub struct VouchersQuery {
    pub format: Option<String>,
    pub month: Option<NaiveDate>,
    pub class_id: Option<Uuid>,
    pub section_id: Option<Uuid>,
    pub status: Option<String>,
    pub q: Option<String>,
}

async fn vouchers(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<VouchersQuery>,
) -> ApiResult<Response> {
    user.require(Permission::FeesRead)?;
    let mut c = state.pool.acquire().await?;
    let list = fees::list_vouchers(
        &mut c,
        user.user.school_id,
        &fees::VoucherFilter {
            billing_month: q.month,
            class_id: q.class_id,
            section_id: q.section_id,
            status: q.status.clone(),
            student_id: None,
            q: q.q.clone(),
        },
        100_000,
    )
    .await?;
    let headers = h(&[
        "Voucher no.",
        "Month",
        "Due date",
        "Student number",
        "Student",
        "Class",
        "Section",
        "Gross",
        "Sibling discount",
        "Concession",
        "Arrears",
        "Total due",
        "Late fee",
        "Paid",
        "Status",
    ]);
    let rows: Vec<Vec<String>> = list
        .iter()
        .map(|v| {
            vec![
                s(&v.voucher_no),
                s(v.billing_month),
                s(v.due_date),
                s(&v.student_number),
                s(&v.student_name),
                opt(v.class_name.clone()),
                opt(v.section_name.clone()),
                s(v.gross),
                s(v.sibling_discount),
                s(v.concession_discount),
                s(v.arrears),
                s(v.total_due),
                s(v.late_fee),
                s(v.net_paid),
                s(&v.status),
            ]
        })
        .collect();
    let (name, bytes) = build(
        "fee-vouchers",
        q.format.as_deref().unwrap_or("xlsx"),
        &headers,
        &rows,
    )?;
    Ok(file_response(&name, bytes))
}

#[derive(Debug, Deserialize)]
pub struct DefaultersQuery {
    pub format: Option<String>,
    pub as_of: Option<NaiveDate>,
    pub class_id: Option<Uuid>,
    pub min_days: Option<i32>,
}

async fn defaulters(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<DefaultersQuery>,
) -> ApiResult<Response> {
    user.require(Permission::FeesRead)?;
    let mut c = state.pool.acquire().await?;
    let list = fees::defaulters(
        &mut c,
        user.user.school_id,
        q.as_of.unwrap_or_else(crate::clock::today),
        q.class_id,
        q.min_days.unwrap_or(0),
    )
    .await?;
    let headers = h(&[
        "Student number",
        "Student",
        "Father's name",
        "Class",
        "Section",
        "Guardian phone",
        "Oldest due date",
        "Days overdue",
        "Unpaid vouchers",
        "Balance",
    ]);
    let rows: Vec<Vec<String>> = list
        .iter()
        .map(|d| {
            vec![
                s(&d.student_number),
                s(&d.student_name),
                s(&d.father_name),
                opt(d.class_name.clone()),
                opt(d.section_name.clone()),
                opt(d.guardian_phone.clone()),
                s(d.oldest_due_date),
                s(d.days_overdue),
                s(d.unpaid_vouchers),
                s(d.balance),
            ]
        })
        .collect();
    let (name, bytes) = build(
        "defaulters",
        q.format.as_deref().unwrap_or("xlsx"),
        &headers,
        &rows,
    )?;
    Ok(file_response(&name, bytes))
}

#[derive(Debug, Deserialize)]
pub struct AbsenteesQuery {
    pub format: Option<String>,
    pub date: Option<NaiveDate>,
    pub class_id: Option<Uuid>,
}

async fn absentees(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<AbsenteesQuery>,
) -> ApiResult<Response> {
    user.require(Permission::AttendanceRead)?;
    let mut c = state.pool.acquire().await?;
    let scope = user.section_scope(&mut c).await?;
    let date = q.date.unwrap_or_else(crate::clock::today);
    let list = att_repo::absentees(
        &mut c,
        user.user.school_id,
        date,
        q.class_id,
        scope.as_deref(),
    )
    .await?;
    let headers = h(&[
        "Date",
        "Student number",
        "Student",
        "Father's name",
        "Class",
        "Section",
        "Roll no.",
        "Status",
        "Note",
        "Guardian phone",
    ]);
    let rows: Vec<Vec<String>> = list
        .iter()
        .map(|r| {
            vec![
                s(date),
                s(&r.student_number),
                s(&r.full_name),
                s(&r.father_name),
                s(&r.class_name),
                s(&r.section_name),
                opt(r.roll_no),
                s(&r.status),
                opt(r.note.clone()),
                opt(r.guardian_phone.clone()),
            ]
        })
        .collect();
    let (name, bytes) = build(
        "absentees",
        q.format.as_deref().unwrap_or("xlsx"),
        &headers,
        &rows,
    )?;
    Ok(file_response(&name, bytes))
}

#[derive(Debug, Deserialize)]
pub struct RegisterQuery {
    pub format: Option<String>,
    pub section_id: Uuid,
    pub month: NaiveDate,
}

async fn register(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<RegisterQuery>,
) -> ApiResult<Response> {
    user.require(Permission::AttendanceRead)?;
    let mut c = state.pool.acquire().await?;
    let section = academics::get_section(&mut c, q.section_id).await?;
    if section.school_id != user.user.school_id {
        return Err(ApiError::not_found("section"));
    }
    if let Some(scope) = user.section_scope(&mut c).await? {
        if !scope.contains(&section.id) {
            return Err(ApiError::forbidden());
        }
    }
    let (weekend, holidays) = att_repo::calendar(&mut c, user.user.school_id).await?;
    let days = attendance::month_days(q.month);
    let first = days[0];
    let last = *days.last().unwrap_or(&first);
    let entries = att_repo::section_entries(&mut c, section.id, first, last).await?;
    let roster = att_repo::roster(&mut c, section.id, first).await?;
    let mut headers = h(&["Roll no.", "Student number", "Student"]);
    for d in &days {
        let kind = attendance::day_kind(*d, &weekend, &holidays);
        headers.push(match kind {
            attendance::DayKind::School => d.format("%d").to_string(),
            attendance::DayKind::Weekend => format!("{} (off)", d.format("%d")),
            attendance::DayKind::Holiday { name } => format!("{} ({name})", d.format("%d")),
        });
    }
    headers.extend(h(&["Present", "Absent", "Late", "Leave", "%"]));
    let rows: Vec<Vec<String>> = roster
        .iter()
        .map(|r| {
            let mut row = vec![opt(r.roll_no), s(&r.student_number), s(&r.full_name)];
            let mut summary = attendance::AttendanceSummary::default();
            for d in &days {
                let e = entries
                    .iter()
                    .find(|e| e.student_id == r.student_id && e.date == *d);
                match e {
                    Some(e) => {
                        if let Ok(st) = e.status.parse::<attendance::AttendanceStatus>() {
                            summary.add(st);
                        }
                        row.push(
                            match e.status.as_str() {
                                "present" => "P",
                                "absent" => "A",
                                "late" => "L",
                                "leave" => "Lv",
                                other => other,
                            }
                            .into(),
                        );
                    }
                    None => row.push(String::new()),
                }
            }
            row.extend([
                s(summary.present),
                s(summary.absent),
                s(summary.late),
                s(summary.leave),
                opt(summary.percent()),
            ]);
            row
        })
        .collect();
    let (name, bytes) = build(
        "attendance-register",
        q.format.as_deref().unwrap_or("xlsx"),
        &headers,
        &rows,
    )?;
    Ok(file_response(&name, bytes))
}

#[derive(Debug, Deserialize)]
pub struct ResultsQuery {
    pub format: Option<String>,
    pub assessment_id: Uuid,
    pub section_id: Uuid,
}

async fn results(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<ResultsQuery>,
) -> ApiResult<Response> {
    user.require(Permission::ExamsRead)?;
    let mut c = state.pool.acquire().await?;
    let sheet =
        crate::routes::exams::result_sheet_for(&mut c, &user, q.assessment_id, q.section_id)
            .await?;
    let mut headers = h(&["Roll no.", "Student number", "Student"]);
    for sub in &sheet.subjects {
        headers.push(format!("{} /{}", sub.subject_name, sub.max_marks));
    }
    headers.extend(h(&["Total", "Max", "%", "Grade", "Result", "Position"]));
    let rows: Vec<Vec<String>> = sheet
        .rows
        .iter()
        .map(|r| {
            let mut row = vec![opt(r.roll_no), s(&r.student_number), s(&r.full_name)];
            for m in &r.marks {
                row.push(if m.is_absent {
                    "A".into()
                } else {
                    opt(m.obtained)
                });
            }
            row.extend([
                s(r.total),
                s(r.max),
                s(r.percent),
                s(&r.grade),
                (if r.pass { "Pass" } else { "Fail" }).into(),
                s(r.position),
            ]);
            row
        })
        .collect();
    let (name, bytes) = build(
        "results",
        q.format.as_deref().unwrap_or("xlsx"),
        &headers,
        &rows,
    )?;
    Ok(file_response(&name, bytes))
}
