//! What a parent sees in the app (docs/adr/0006-parent-app.md): their children, and for
//! one child attendance, results and the report card, fees, the school's notices and the
//! conversation with the class teacher.
//!
//! **Who may see what.** Every route here takes a parent session, and a child is named by
//! the school (the hub's id for it) and the student. Before anything is read:
//! 1. the account must hold a live link to a guardian record of that school, and the
//!    school must still be joined;
//! 2. inside that school's data, and nothing else, that guardian record must be a guardian
//!    of that student.
//!
//! When either fails the answer is `404`, the same for another family's child, another
//! school and a link that has ended, so nothing can be learned from the difference.
//!
//! Everything is read from the hub's copy of the school, so every answer says when the
//! school last synced. The only writes are a parent's own messages and their "read up to"
//! mark; both are school records and reach the school at its next sync.

use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::{header, Method, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use madrasa_core::attendance::AttendanceSummary;
use madrasa_core::parent_app::{clean_message, Side};
use madrasa_db::models::School;
use madrasa_db::results::ReportCard;
use madrasa_db::{academics, attendance, fees, parent_app, results, schools, students, Actor};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, Postgres, Transaction};
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

use super::parents::CurrentParent;
use super::JsonBody;
use crate::parents;
use crate::tenants::{self, HubSchool};
use crate::{HubError, HubResult, HubState};

/// A parent may send this many messages about one child within an hour.
const MESSAGES_PER_HOUR: i64 = 20;

pub fn router() -> Router<HubState> {
    Router::new()
        .route("/parent/children", get(children))
        .route("/parent/children/{school}/{student}", get(overview))
        .route(
            "/parent/children/{school}/{student}/attendance",
            get(attendance_month),
        )
        .route(
            "/parent/children/{school}/{student}/results",
            get(report_card),
        )
        .route("/parent/children/{school}/{student}/fees", get(fee_status))
        .route("/parent/children/{school}/{student}/notices", get(notices))
        .route(
            "/parent/children/{school}/{student}/messages",
            get(conversation).post(send),
        )
        .route(
            "/parent/children/{school}/{student}/messages/read",
            post(mark_read),
        )
        .layer(app_cors())
}

/// Cross-origin access for the parent app, whose pages are served from inside the app and
/// not from the hub. Any origin may call, because nothing here trusts the origin: every
/// request must carry the parent's token in the `Authorization` header, which a browser
/// never adds by itself. Cookies are not used and are not allowed across origins.
pub(crate) fn app_cors() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
        .max_age(Duration::from_secs(3600))
}

// ---- scoping --------------------------------------------------------------------------------

fn no_child() -> HubError {
    HubError::not_found("The child")
}

/// One child of the signed-in parent, opened: the school, the guardian record the parent
/// acts as there, and a transaction that sees that school's data and nothing else.
struct Opened<'a> {
    school: HubSchool,
    /// The school's own record in its copy (currency, time zone).
    record: School,
    guardian_id: Uuid,
    student_id: Uuid,
    tx: Transaction<'a, Postgres>,
}

impl Opened<'_> {
    fn today(&self) -> NaiveDate {
        match madrasa_core::locale::time_zone(&self.record.time_zone) {
            Some(tz) => Utc::now().with_timezone(&tz).date_naive(),
            None => Utc::now().date_naive(),
        }
    }

    fn synced<T: Serialize>(&self, data: T) -> Json<Synced<T>> {
        Json(Synced {
            synced_at: self.school.last_sync_at,
            data,
        })
    }
}

/// The two checks described at the top of this file. `404` when either fails.
async fn open<'a>(
    state: &'a HubState,
    current: &CurrentParent,
    hub_school_id: Uuid,
    student_id: Uuid,
) -> HubResult<Opened<'a>> {
    let mut conn = state.pool.acquire().await?;
    let guardians: Vec<Uuid> = parents::links(&mut conn, current.parent.id)
        .await?
        .into_iter()
        .filter(|l| l.hub_school_id == hub_school_id)
        .map(|l| l.guardian_id)
        .collect();
    if guardians.is_empty() {
        return Err(no_child());
    }
    let school = tenants::get(&mut conn, hub_school_id)
        .await?
        .filter(|s| s.status == "active")
        .ok_or_else(no_child)?;
    drop(conn);
    let mut tx = tenants::begin(&state.pool, &school).await?;
    let guardian_id: Uuid = sqlx::query_scalar(
        "SELECT sg.guardian_id FROM student_guardians sg
         JOIN guardians g ON g.id = sg.guardian_id AND g.deleted_at IS NULL
         JOIN students s ON s.id = sg.student_id AND s.deleted_at IS NULL
         WHERE sg.student_id = $1 AND sg.guardian_id = ANY($2) AND sg.deleted_at IS NULL
         ORDER BY sg.is_primary DESC, sg.guardian_id LIMIT 1",
    )
    .bind(student_id)
    .bind(&guardians)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(no_child)?;
    let record = schools::get(&mut tx, school.school_id).await?;
    Ok(Opened {
        school,
        record,
        guardian_id,
        student_id,
        tx,
    })
}

/// An answer with the time the school last synced: what is shown is as of then.
#[derive(Debug, Serialize)]
pub struct Synced<T: Serialize> {
    pub synced_at: Option<DateTime<Utc>>,
    #[serde(flatten)]
    pub data: T,
}

// ---- children -------------------------------------------------------------------------------

/// A child as the app lists it.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ChildRow {
    pub student_id: Uuid,
    #[serde(skip_serializing)]
    pub guardian_id: Uuid,
    pub full_name: String,
    pub student_number: String,
    /// `active`, or why the child is no longer at the school.
    pub status: String,
    /// `father`, `mother`, `guardian` or `other`.
    pub relationship: String,
    pub class_name: Option<String>,
    pub section_name: Option<String>,
    pub roll_no: Option<i32>,
}

const CHILD_SQL: &str =
    "SELECT DISTINCT ON (s.full_name, s.id) s.id AS student_id, sg.guardian_id, s.full_name, s.student_number,
            s.status, sg.relationship, cur.class_name, cur.section_name, cur.roll_no
     FROM student_guardians sg
     JOIN guardians g ON g.id = sg.guardian_id AND g.deleted_at IS NULL
     JOIN students s ON s.id = sg.student_id AND s.deleted_at IS NULL
     LEFT JOIN LATERAL (SELECT c.name AS class_name, sec.name AS section_name, e.roll_no
                        FROM enrollments e
                        JOIN academic_years y ON y.id = e.academic_year_id AND y.is_active AND y.deleted_at IS NULL
                        JOIN sections sec ON sec.id = e.section_id AND sec.deleted_at IS NULL
                        JOIN classes c ON c.id = sec.class_id
                        WHERE e.student_id = s.id AND e.deleted_at IS NULL LIMIT 1) cur ON TRUE
     WHERE sg.guardian_id = ANY($1) AND sg.deleted_at IS NULL AND ($2::uuid IS NULL OR s.id = $2)
     ORDER BY s.full_name, s.id, sg.is_primary DESC, sg.guardian_id";

async fn child_rows(
    conn: &mut PgConnection,
    guardians: &[Uuid],
    only: Option<Uuid>,
) -> Result<Vec<ChildRow>, sqlx::Error> {
    sqlx::query_as::<_, ChildRow>(CHILD_SQL)
        .bind(guardians)
        .bind(only)
        .fetch_all(conn)
        .await
}

#[derive(Debug, Serialize)]
pub struct Child {
    /// The hub's id for the school; with `student_id` it names the child in every route.
    pub school_ref: Uuid,
    pub school_name: String,
    pub synced_at: Option<DateTime<Utc>>,
    #[serde(flatten)]
    pub child: ChildRow,
    /// Messages from the class teacher the parent has not read.
    pub unread: i64,
}

#[derive(Debug, Serialize)]
pub struct Children {
    pub children: Vec<Child>,
}

/// Every child of the signed-in parent, across the schools they hold access to.
async fn children(
    State(state): State<HubState>,
    current: CurrentParent,
) -> HubResult<Json<Children>> {
    let mut conn = state.pool.acquire().await?;
    let links = parents::links(&mut conn, current.parent.id).await?;
    let mut school_ids: Vec<Uuid> = links.iter().map(|l| l.hub_school_id).collect();
    school_ids.dedup();
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for hub_school_id in school_ids {
        if seen.contains(&hub_school_id) {
            continue;
        }
        seen.push(hub_school_id);
        let Some(school) = tenants::get(&mut conn, hub_school_id)
            .await?
            .filter(|s| s.status == "active")
        else {
            continue;
        };
        let guardians: Vec<Uuid> = links
            .iter()
            .filter(|l| l.hub_school_id == hub_school_id)
            .map(|l| l.guardian_id)
            .collect();
        let mut tx = tenants::begin(&state.pool, &school).await?;
        for child in child_rows(&mut tx, &guardians, None).await? {
            let unread =
                parent_app::unread(&mut tx, child.student_id, child.guardian_id, Side::Guardian)
                    .await?;
            out.push(Child {
                school_ref: school.id,
                school_name: school.name.clone(),
                synced_at: school.last_sync_at,
                child,
                unread,
            });
        }
        tx.commit().await?;
    }
    Ok(Json(Children { children: out }))
}

// ---- overview -------------------------------------------------------------------------------

/// Attendance counts with the percentage the school shows (present and late count as
/// attended).
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Counts {
    pub present: u32,
    pub absent: u32,
    pub late: u32,
    pub leave: u32,
    pub total: u32,
    pub percent: Option<f64>,
}

impl From<AttendanceSummary> for Counts {
    fn from(s: AttendanceSummary) -> Self {
        Counts {
            present: s.present,
            absent: s.absent,
            late: s.late,
            leave: s.leave,
            total: s.total(),
            percent: s.percent(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Overview {
    pub school_ref: Uuid,
    pub school_name: String,
    #[serde(flatten)]
    pub child: ChildRow,
    pub class_teacher: Option<String>,
    /// The active academic year, when the school has one.
    pub academic_year: Option<String>,
    /// Attendance in the active academic year so far.
    pub attendance: Option<Counts>,
    pub currency: String,
    /// What is owed on open vouchers.
    pub balance: Decimal,
    pub open_vouchers: usize,
    pub unread: i64,
    pub notices: usize,
}

async fn overview(
    State(state): State<HubState>,
    current: CurrentParent,
    Path((school, student)): Path<(Uuid, Uuid)>,
) -> HubResult<Json<Synced<Overview>>> {
    let mut o = open(&state, &current, school, student).await?;
    let child = child_rows(&mut o.tx, &[o.guardian_id], Some(o.student_id))
        .await?
        .into_iter()
        .next()
        .ok_or_else(no_child)?;
    let teacher = parent_app::class_teacher(&mut o.tx, o.student_id).await?;
    let year = academics::active_year(&mut o.tx, o.school.school_id).await?;
    let attendance = match &year {
        Some(y) => Some(
            attendance::student_summary(&mut o.tx, o.student_id, y.start_date, y.end_date)
                .await?
                .into(),
        ),
        None => None,
    };
    let vouchers = fees::vouchers_for_student(&mut o.tx, o.student_id).await?;
    let open_vouchers = vouchers
        .iter()
        .filter(|v| v.status == "unpaid" || v.status == "partial")
        .count();
    let balance = fees::student_balance(&mut o.tx, o.student_id).await?;
    let unread = parent_app::unread(&mut o.tx, o.student_id, o.guardian_id, Side::Guardian).await?;
    let today = o.today();
    let notices = parent_app::notices_for_class(
        &mut o.tx,
        o.school.school_id,
        teacher.as_ref().map(|t| t.class_id),
        Utc::now(),
        today,
    )
    .await?
    .len();
    let data = Overview {
        school_ref: o.school.id,
        school_name: o.school.name.clone(),
        child,
        class_teacher: teacher.and_then(|t| t.teacher_name),
        academic_year: year.map(|y| y.name),
        attendance,
        currency: o.record.currency.clone(),
        balance,
        open_vouchers,
        unread,
        notices,
    };
    let out = o.synced(data);
    o.tx.commit().await?;
    Ok(out)
}

// ---- attendance -----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct MonthQuery {
    /// `YYYY-MM`; this month at the school when left out.
    pub month: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Day {
    pub date: NaiveDate,
    /// `present`, `absent`, `late` or `leave`.
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct YearAttendance {
    pub name: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub summary: Counts,
}

#[derive(Debug, Serialize)]
pub struct AttendanceMonth {
    /// `YYYY-MM`.
    pub month: String,
    /// The days attendance was marked on, oldest first.
    pub days: Vec<Day>,
    pub summary: Counts,
    /// The active academic year as a whole.
    pub year: Option<YearAttendance>,
}

fn month_bounds(month: &str) -> Option<(NaiveDate, NaiveDate)> {
    let first = NaiveDate::parse_from_str(&format!("{}-01", month.trim()), "%Y-%m-%d").ok()?;
    let next = if first.month() == 12 {
        NaiveDate::from_ymd_opt(first.year() + 1, 1, 1)?
    } else {
        NaiveDate::from_ymd_opt(first.year(), first.month() + 1, 1)?
    };
    Some((first, next.pred_opt()?))
}

async fn attendance_month(
    State(state): State<HubState>,
    current: CurrentParent,
    Path((school, student)): Path<(Uuid, Uuid)>,
    Query(q): Query<MonthQuery>,
) -> HubResult<Json<Synced<AttendanceMonth>>> {
    let mut o = open(&state, &current, school, student).await?;
    let month = match q.month {
        Some(m) => m.trim().to_string(),
        None => o.today().format("%Y-%m").to_string(),
    };
    let (from, to) = month_bounds(&month)
        .ok_or_else(|| HubError::validation("The month must look like 2026-11."))?;
    let mut rows = attendance::student_history(&mut o.tx, o.student_id, from, to).await?;
    rows.reverse();
    let mut summary = AttendanceSummary::default();
    for r in &rows {
        if let Ok(status) = r.status.parse() {
            summary.add(status);
        }
    }
    let year = match academics::active_year(&mut o.tx, o.school.school_id).await? {
        Some(y) => Some(YearAttendance {
            summary: attendance::student_summary(&mut o.tx, o.student_id, y.start_date, y.end_date)
                .await?
                .into(),
            name: y.name,
            start_date: y.start_date,
            end_date: y.end_date,
        }),
        None => None,
    };
    let out = o.synced(AttendanceMonth {
        month,
        days: rows
            .into_iter()
            .map(|r| Day {
                date: r.date,
                status: r.status,
            })
            .collect(),
        summary: summary.into(),
        year,
    });
    o.tx.commit().await?;
    Ok(out)
}

// ---- results --------------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct Results {
    /// The active academic year's report card from approved marks only; `None` when the
    /// school has no active year.
    pub card: Option<ReportCard>,
}

/// The child's report card for the active year, by the school's own calculation
/// (`madrasa_db::results`), from approved marks sheets only.
async fn report_card(
    State(state): State<HubState>,
    current: CurrentParent,
    Path((school, student)): Path<(Uuid, Uuid)>,
) -> HubResult<Json<Synced<Results>>> {
    let mut o = open(&state, &current, school, student).await?;
    let card = match academics::active_year(&mut o.tx, o.school.school_id).await? {
        Some(year) => {
            let section = match students::section_in_year(&mut o.tx, o.student_id, year.id).await? {
                Some(id) => Some(academics::get_section(&mut o.tx, id).await?),
                None => None,
            };
            Some(
                results::build_approved_report_card(
                    &mut o.tx,
                    o.school.school_id,
                    o.student_id,
                    &year,
                    section.as_ref(),
                )
                .await?,
            )
        }
        None => None,
    };
    let out = o.synced(Results { card });
    o.tx.commit().await?;
    Ok(out)
}

// ---- fees -----------------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct VoucherLine {
    pub description: String,
    pub amount: Decimal,
}

/// A fee voucher as a parent sees it. Amounts are decimal text, never floats.
#[derive(Debug, Serialize)]
pub struct Voucher {
    pub id: Uuid,
    pub voucher_no: String,
    pub billing_month: NaiveDate,
    pub issue_date: NaiveDate,
    pub due_date: NaiveDate,
    pub lines: Vec<VoucherLine>,
    pub gross: Decimal,
    pub sibling_discount: Decimal,
    pub concession_discount: Decimal,
    pub net_monthly: Decimal,
    pub arrears: Decimal,
    pub total_due: Decimal,
    pub late_fee: Decimal,
    pub net_paid: Decimal,
    /// Still owed on this voucher.
    pub balance: Decimal,
    /// `unpaid`, `partial`, `paid` or `carried` (moved into a later voucher as arrears).
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct Receipt {
    pub id: Uuid,
    pub receipt_no: String,
    pub voucher_no: Option<String>,
    pub paid_on: NaiveDate,
    /// Negative for a reversal.
    pub amount: Decimal,
    pub method: String,
    /// This entry takes an earlier receipt back.
    pub reversal: bool,
}

#[derive(Debug, Serialize)]
pub struct FeeStatus {
    pub currency: String,
    /// What is owed on open vouchers.
    pub balance: Decimal,
    /// Newest month first. Cancelled vouchers are left out.
    pub vouchers: Vec<Voucher>,
    /// Newest first.
    pub receipts: Vec<Receipt>,
}

/// Fee status and vouchers. Read-only: nothing is paid through the app.
async fn fee_status(
    State(state): State<HubState>,
    current: CurrentParent,
    Path((school, student)): Path<(Uuid, Uuid)>,
) -> HubResult<Json<Synced<FeeStatus>>> {
    let mut o = open(&state, &current, school, student).await?;
    let all = fees::vouchers_for_student(&mut o.tx, o.student_id).await?;
    let mut vouchers = Vec::new();
    for v in all.iter().filter(|v| v.status != "cancelled") {
        let lines = fees::voucher_lines(&mut o.tx, v.id)
            .await?
            .into_iter()
            .map(|l| VoucherLine {
                description: l.description,
                amount: l.amount,
            })
            .collect();
        let open = v.status == "unpaid" || v.status == "partial";
        vouchers.push(Voucher {
            id: v.id,
            voucher_no: v.voucher_no.clone(),
            billing_month: v.billing_month,
            issue_date: v.issue_date,
            due_date: v.due_date,
            lines,
            gross: v.gross,
            sibling_discount: v.sibling_discount,
            concession_discount: v.concession_discount,
            net_monthly: v.net_monthly,
            arrears: v.arrears,
            total_due: v.total_due,
            late_fee: v.late_fee,
            net_paid: v.net_paid,
            balance: if open { v.balance() } else { Decimal::ZERO },
            status: v.status.clone(),
        });
    }
    let receipts = fees::receipts_for_student(&mut o.tx, o.student_id)
        .await?
        .into_iter()
        .map(|r| Receipt {
            id: r.id,
            voucher_no: all
                .iter()
                .find(|v| v.id == r.voucher_id)
                .map(|v| v.voucher_no.clone()),
            receipt_no: r.receipt_no,
            paid_on: r.paid_on,
            amount: r.amount,
            method: r.method,
            reversal: r.reverses_id.is_some(),
        })
        .collect();
    let balance = fees::student_balance(&mut o.tx, o.student_id).await?;
    let out = o.synced(FeeStatus {
        currency: o.record.currency.clone(),
        balance,
        vouchers,
        receipts,
    });
    o.tx.commit().await?;
    Ok(out)
}

// ---- notices --------------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct NoticeView {
    pub id: Uuid,
    pub title: String,
    pub body: String,
    pub published_at: Option<DateTime<Utc>>,
    /// Shown until the end of this day.
    pub expires_on: Option<NaiveDate>,
    /// For the whole school rather than for the child's class.
    pub whole_school: bool,
}

#[derive(Debug, Serialize)]
pub struct Notices {
    pub notices: Vec<NoticeView>,
}

/// Published notices for the whole school or for the child's class, newest first.
async fn notices(
    State(state): State<HubState>,
    current: CurrentParent,
    Path((school, student)): Path<(Uuid, Uuid)>,
) -> HubResult<Json<Synced<Notices>>> {
    let mut o = open(&state, &current, school, student).await?;
    let class_id = parent_app::class_teacher(&mut o.tx, o.student_id)
        .await?
        .map(|t| t.class_id);
    let today = o.today();
    let list =
        parent_app::notices_for_class(&mut o.tx, o.school.school_id, class_id, Utc::now(), today)
            .await?
            .into_iter()
            .map(|n| NoticeView {
                id: n.id,
                title: n.title,
                body: n.body,
                published_at: n.published_at,
                expires_on: n.expires_on,
                whole_school: n.class_ids.is_empty(),
            })
            .collect();
    let out = o.synced(Notices { notices: list });
    o.tx.commit().await?;
    Ok(out)
}

// ---- messages -------------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct MessageView {
    pub id: Uuid,
    /// `guardian` (the parent) or `staff` (the class teacher).
    pub sender: String,
    pub sender_name: String,
    pub body: String,
    pub sent_at: DateTime<Utc>,
}

impl From<parent_app::Message> for MessageView {
    fn from(m: parent_app::Message) -> Self {
        MessageView {
            id: m.id,
            sender: m.sender,
            sender_name: m.sender_name,
            body: m.body,
            sent_at: m.sent_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Conversation {
    pub class_teacher: Option<String>,
    pub class_name: Option<String>,
    pub section_name: Option<String>,
    /// False when the child's class has no class teacher to write to.
    pub can_send: bool,
    /// Oldest first.
    pub messages: Vec<MessageView>,
    /// Messages from the class teacher the parent has not read.
    pub unread: i64,
}

/// The conversation about this child between this parent and the class teacher. Reading
/// it does not mark it read; the app says so with `.../messages/read`.
async fn conversation(
    State(state): State<HubState>,
    current: CurrentParent,
    Path((school, student)): Path<(Uuid, Uuid)>,
) -> HubResult<Json<Synced<Conversation>>> {
    let mut o = open(&state, &current, school, student).await?;
    let teacher = parent_app::class_teacher(&mut o.tx, o.student_id).await?;
    let messages = parent_app::conversation(&mut o.tx, o.student_id, o.guardian_id)
        .await?
        .into_iter()
        .map(MessageView::from)
        .collect();
    let unread = parent_app::unread(&mut o.tx, o.student_id, o.guardian_id, Side::Guardian).await?;
    let out = o.synced(Conversation {
        can_send: teacher.as_ref().is_some_and(|t| t.staff_id.is_some()),
        class_name: teacher.as_ref().map(|t| t.class_name.clone()),
        section_name: teacher.as_ref().map(|t| t.section_name.clone()),
        class_teacher: teacher.and_then(|t| t.teacher_name),
        messages,
        unread,
    });
    o.tx.commit().await?;
    Ok(out)
}

#[derive(Debug, Deserialize)]
pub struct SendBody {
    pub body: String,
}

/// A message from the parent to the class teacher. It is written into the hub's copy of
/// the school and reaches the school at its next sync; it is never changed or removed.
async fn send(
    State(state): State<HubState>,
    current: CurrentParent,
    Path((school, student)): Path<(Uuid, Uuid)>,
    body: JsonBody<SendBody>,
) -> HubResult<(StatusCode, Json<MessageView>)> {
    let Json(body) = body?;
    let text = clean_message(&body.body).map_err(HubError::validation)?;
    let mut o = open(&state, &current, school, student).await?;
    let has_teacher = parent_app::class_teacher(&mut o.tx, o.student_id)
        .await?
        .is_some_and(|t| t.staff_id.is_some());
    if !has_teacher {
        return Err(HubError::new(
            StatusCode::CONFLICT,
            "no_class_teacher",
            "This class has no class teacher yet, so there is nobody to write to. Please contact the school.",
        ));
    }
    let recent: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM messages
         WHERE student_id = $1 AND guardian_id = $2 AND sender = 'guardian' AND deleted_at IS NULL
           AND sent_at > now() - interval '1 hour'",
    )
    .bind(o.student_id)
    .bind(o.guardian_id)
    .fetch_one(&mut *o.tx)
    .await?;
    if recent >= MESSAGES_PER_HOUR {
        return Err(HubError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "too_many_messages",
            "You have sent many messages in the last hour. Please wait a while before writing again.",
        ));
    }
    let sender_name: String = sqlx::query_scalar("SELECT full_name FROM guardians WHERE id = $1")
        .bind(o.guardian_id)
        .fetch_one(&mut *o.tx)
        .await?;
    let actor = Actor::system_for(o.school.school_id);
    let message = parent_app::send_message(
        &mut o.tx,
        &state.hlc,
        actor,
        parent_app::NewMessage {
            school_id: o.school.school_id,
            student_id: o.student_id,
            guardian_id: o.guardian_id,
            side: Side::Guardian,
            sender_user_id: None,
            sender_name,
            body: text,
        },
    )
    .await?;
    // Whoever writes has seen what came before.
    parent_app::mark_read(
        &mut o.tx,
        &state.hlc,
        actor,
        o.school.school_id,
        o.student_id,
        o.guardian_id,
        Side::Guardian,
    )
    .await?;
    o.tx.commit().await?;
    Ok((StatusCode::CREATED, Json(message.into())))
}

#[derive(Debug, Serialize)]
pub struct ReadMark {
    pub unread: i64,
}

/// The parent has read the conversation up to now.
async fn mark_read(
    State(state): State<HubState>,
    current: CurrentParent,
    Path((school, student)): Path<(Uuid, Uuid)>,
) -> HubResult<Json<ReadMark>> {
    let mut o = open(&state, &current, school, student).await?;
    parent_app::mark_read(
        &mut o.tx,
        &state.hlc,
        Actor::system_for(o.school.school_id),
        o.school.school_id,
        o.student_id,
        o.guardian_id,
        Side::Guardian,
    )
    .await?;
    let unread = parent_app::unread(&mut o.tx, o.student_id, o.guardian_id, Side::Guardian).await?;
    o.tx.commit().await?;
    Ok(Json(ReadMark { unread }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_bounds_cover_the_whole_month() {
        let d = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).unwrap();
        assert_eq!(
            month_bounds("2026-11"),
            Some((d(2026, 11, 1), d(2026, 11, 30)))
        );
        assert_eq!(
            month_bounds("2026-12"),
            Some((d(2026, 12, 1), d(2026, 12, 31)))
        );
        assert_eq!(
            month_bounds("2028-02"),
            Some((d(2028, 2, 1), d(2028, 2, 29)))
        );
        assert_eq!(
            month_bounds(" 2026-01 "),
            Some((d(2026, 1, 1), d(2026, 1, 31)))
        );
        for bad in ["", "2026", "2026-13", "11-2026", "2026-11-05"] {
            assert_eq!(month_bounds(bad), None, "{bad}");
        }
    }
}
