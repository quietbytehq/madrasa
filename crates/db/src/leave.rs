//! Leave (Standard stage 2): staff leave types, balances per year, requests with approval
//! that mark `staff_attendance`, student leave requests that mark attendance entries, and
//! the monthly staff register. Rules live in `madrasa_core::leave`.

use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use madrasa_core::leave::{self as rules, Balance, Calendar, HalfDay, RequestStatus};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::audit::{self, snapshot, Action};
use crate::models::AcademicYear;
use crate::{academics, attendance, map_unique, settings, Actor, DbError, DbResult};

/// Note written on attendance rows marked by an approved request.
pub const APPROVED_NOTE: &str = "Leave (approved)";
/// Setting: the staff member who may decide leave besides `leave.approve` holders.
pub const KEY_APPROVER: &str = "leave.approver_staff_id";

// ---- types ----------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct LeaveType {
    pub id: Uuid,
    pub school_id: Uuid,
    pub name: String,
    pub code: String,
    pub entitlement_days: Decimal,
    pub carry_over_max: Decimal,
    pub needs_approval: bool,
    pub position: i32,
    pub hlc: String,
}

#[derive(Debug, Clone)]
pub struct LeaveTypeInput {
    pub name: String,
    pub code: String,
    pub entitlement_days: Decimal,
    pub carry_over_max: Decimal,
    pub needs_approval: bool,
    pub position: i32,
}

fn check_type(input: &LeaveTypeInput) -> DbResult<()> {
    if input.name.trim().is_empty() {
        return Err(DbError::Conflict("the leave type needs a name".into()));
    }
    if input.entitlement_days < Decimal::ZERO || input.carry_over_max < Decimal::ZERO {
        return Err(DbError::Conflict("days cannot be negative".into()));
    }
    if input.entitlement_days > Decimal::from(366) || input.carry_over_max > Decimal::from(366) {
        return Err(DbError::Conflict("days cannot exceed a year".into()));
    }
    Ok(())
}

const TYPE_COLS: &str =
    "id, school_id, name, code, entitlement_days, carry_over_max, needs_approval, position, hlc";

pub async fn list_types(conn: &mut PgConnection, school_id: Uuid) -> DbResult<Vec<LeaveType>> {
    Ok(sqlx::query_as::<_, LeaveType>(sqlx::AssertSqlSafe(format!(
        "SELECT {TYPE_COLS} FROM leave_types WHERE school_id = $1 AND deleted_at IS NULL ORDER BY position, name"
    )))
    .bind(school_id)
    .fetch_all(conn)
    .await?)
}

pub async fn get_type(conn: &mut PgConnection, id: Uuid) -> DbResult<LeaveType> {
    sqlx::query_as::<_, LeaveType>(sqlx::AssertSqlSafe(format!(
        "SELECT {TYPE_COLS} FROM leave_types WHERE id = $1 AND deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("leave type".into()))
}

pub async fn create_type(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    input: &LeaveTypeInput,
) -> DbResult<LeaveType> {
    check_type(input)?;
    let row = sqlx::query_as::<_, LeaveType>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO leave_types (id, school_id, name, code, entitlement_days, carry_over_max, needs_approval, position, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING {TYPE_COLS}"
    )))
    .bind(new_id())
    .bind(school_id)
    .bind(input.name.trim())
    .bind(input.code.trim())
    .bind(input.entitlement_days)
    .bind(input.carry_over_max)
    .bind(input.needs_approval)
    .bind(input.position)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a leave type with this name"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "leave_type",
        Some(row.id),
        None,
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

pub async fn update_type(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    input: &LeaveTypeInput,
) -> DbResult<LeaveType> {
    check_type(input)?;
    let old = get_type(&mut *conn, id).await?;
    let row = sqlx::query_as::<_, LeaveType>(sqlx::AssertSqlSafe(format!(
        "UPDATE leave_types SET name = $2, code = $3, entitlement_days = $4, carry_over_max = $5, needs_approval = $6, position = $7,
                updated_at = now(), updated_by = $8, hlc = $9
         WHERE id = $1 AND deleted_at IS NULL RETURNING {TYPE_COLS}"
    )))
    .bind(id)
    .bind(input.name.trim())
    .bind(input.code.trim())
    .bind(input.entitlement_days)
    .bind(input.carry_over_max)
    .bind(input.needs_approval)
    .bind(input.position)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a leave type with this name"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "leave_type",
        Some(id),
        Some(snapshot(&old)),
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

pub async fn delete_type(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
) -> DbResult<()> {
    let old = get_type(&mut *conn, id).await?;
    let live: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM leave_requests WHERE leave_type_id = $1 AND status IN ('pending', 'approved') AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    if live > 0 {
        return Err(DbError::Conflict(
            "this leave type has pending or approved requests".into(),
        ));
    }
    sqlx::query("UPDATE leave_types SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1")
        .bind(id)
        .bind(actor.user_id)
        .bind(hlc.now().to_string())
        .execute(&mut *conn)
        .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Delete,
        "leave_type",
        Some(id),
        Some(snapshot(&old)),
        None,
    )
    .await?;
    Ok(())
}

// ---- balances -------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BalanceRow {
    pub id: Option<Uuid>,
    pub staff_id: Uuid,
    pub staff_name: String,
    pub leave_type_id: Uuid,
    pub type_name: String,
    pub entitlement: Decimal,
    pub carried_over: Decimal,
    pub adjustment: Decimal,
    pub used: Decimal,
    pub pending: Decimal,
    pub note: String,
}

impl BalanceRow {
    pub fn balance(&self) -> Balance {
        Balance {
            entitlement: self.entitlement,
            carried_over: self.carried_over,
            adjustment: self.adjustment,
            used: self.used,
            pending: self.pending,
        }
    }
}

/// Every (active staff member × leave type) for the year, with the days used and pending.
/// `staff_id` narrows to one person.
pub async fn balances(
    conn: &mut PgConnection,
    school_id: Uuid,
    year_id: Uuid,
    staff_id: Option<Uuid>,
) -> DbResult<Vec<BalanceRow>> {
    Ok(sqlx::query_as::<_, BalanceRow>(
        "SELECT b.id, st.id AS staff_id, st.full_name AS staff_name, t.id AS leave_type_id, t.name AS type_name,
                COALESCE(b.entitlement, t.entitlement_days) AS entitlement,
                COALESCE(b.carried_over, 0) AS carried_over,
                COALESCE(b.adjustment, 0) AS adjustment,
                COALESCE((SELECT sum(r.days) FROM leave_requests r WHERE r.staff_id = st.id AND r.leave_type_id = t.id
                           AND r.academic_year_id = $2 AND r.status = 'approved' AND r.deleted_at IS NULL), 0) AS used,
                COALESCE((SELECT sum(r.days) FROM leave_requests r WHERE r.staff_id = st.id AND r.leave_type_id = t.id
                           AND r.academic_year_id = $2 AND r.status = 'pending' AND r.deleted_at IS NULL), 0) AS pending,
                COALESCE(b.note, '') AS note
         FROM staff st
         CROSS JOIN leave_types t
         LEFT JOIN leave_balances b ON b.staff_id = st.id AND b.leave_type_id = t.id AND b.academic_year_id = $2 AND b.deleted_at IS NULL
         WHERE st.school_id = $1 AND st.deleted_at IS NULL AND st.status = 'active'
           AND t.school_id = $1 AND t.deleted_at IS NULL
           AND ($3::uuid IS NULL OR st.id = $3)
         ORDER BY st.full_name, t.position, t.name",
    )
    .bind(school_id)
    .bind(year_id)
    .bind(staff_id)
    .fetch_all(conn)
    .await?)
}

async fn balance_of(
    conn: &mut PgConnection,
    school_id: Uuid,
    year_id: Uuid,
    staff_id: Uuid,
    type_id: Uuid,
) -> DbResult<Balance> {
    Ok(balances(conn, school_id, year_id, Some(staff_id))
        .await?
        .into_iter()
        .find(|b| b.leave_type_id == type_id)
        .map(|b| b.balance())
        .unwrap_or_default())
}

#[derive(Debug, Clone)]
pub struct BalanceInput {
    pub staff_id: Uuid,
    pub leave_type_id: Uuid,
    pub entitlement: Option<Decimal>,
    pub carried_over: Decimal,
    pub adjustment: Decimal,
    pub note: String,
}

/// Set (or create) one balance row; audited.
pub async fn set_balance(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    year_id: Uuid,
    input: &BalanceInput,
) -> DbResult<()> {
    let old: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT to_jsonb(b) FROM leave_balances b WHERE staff_id = $1 AND leave_type_id = $2 AND academic_year_id = $3 AND deleted_at IS NULL",
    )
    .bind(input.staff_id)
    .bind(input.leave_type_id)
    .bind(year_id)
    .fetch_optional(&mut *conn)
    .await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO leave_balances (id, school_id, staff_id, leave_type_id, academic_year_id, entitlement, carried_over, adjustment, note, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
         ON CONFLICT (staff_id, leave_type_id, academic_year_id) WHERE deleted_at IS NULL DO UPDATE
           SET entitlement = EXCLUDED.entitlement, carried_over = EXCLUDED.carried_over, adjustment = EXCLUDED.adjustment,
               note = EXCLUDED.note, updated_at = now(), updated_by = EXCLUDED.updated_by, hlc = EXCLUDED.hlc
         RETURNING id",
    )
    .bind(new_id())
    .bind(school_id)
    .bind(input.staff_id)
    .bind(input.leave_type_id)
    .bind(year_id)
    .bind(input.entitlement)
    .bind(input.carried_over)
    .bind(input.adjustment)
    .bind(input.note.trim())
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        if old.is_some() { Action::Update } else { Action::Create },
        "leave_balance",
        Some(id),
        old,
        Some(json!({
            "staff_id": input.staff_id, "leave_type_id": input.leave_type_id, "academic_year_id": year_id,
            "entitlement": input.entitlement, "carried_over": input.carried_over, "adjustment": input.adjustment, "note": input.note
        })),
    )
    .await?;
    Ok(())
}

/// Carry each staff member's unused days of `from_year` into `to_year`, capped by the type.
/// Returns how many balances were written.
pub async fn carry_over(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    from_year: Uuid,
    to_year: Uuid,
) -> DbResult<usize> {
    if from_year == to_year {
        return Err(DbError::Conflict(
            "carry over goes from one year to the next".into(),
        ));
    }
    let types: HashMap<Uuid, LeaveType> = list_types(&mut *conn, school_id)
        .await?
        .into_iter()
        .map(|t| (t.id, t))
        .collect();
    let last = balances(&mut *conn, school_id, from_year, None).await?;
    let current: HashMap<(Uuid, Uuid), BalanceRow> = balances(&mut *conn, school_id, to_year, None)
        .await?
        .into_iter()
        .map(|b| ((b.staff_id, b.leave_type_id), b))
        .collect();
    let mut n = 0;
    for b in last {
        let Some(t) = types.get(&b.leave_type_id) else {
            continue;
        };
        let carry = rules::carry_over(b.balance().available(), t.carry_over_max);
        let cur = current.get(&(b.staff_id, b.leave_type_id));
        if cur.is_some_and(|c| c.carried_over == carry) || (cur.is_none() && carry == Decimal::ZERO)
        {
            continue;
        }
        set_balance(
            &mut *conn,
            hlc,
            actor,
            school_id,
            to_year,
            &BalanceInput {
                staff_id: b.staff_id,
                leave_type_id: b.leave_type_id,
                entitlement: cur.and_then(|c| c.id.map(|_| c.entitlement)),
                carried_over: carry,
                adjustment: cur.map(|c| c.adjustment).unwrap_or(Decimal::ZERO),
                note: cur.map(|c| c.note.clone()).unwrap_or_default(),
            },
        )
        .await?;
        n += 1;
    }
    Ok(n)
}

// ---- staff requests ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct LeaveRequest {
    pub id: Uuid,
    pub school_id: Uuid,
    pub staff_id: Uuid,
    pub leave_type_id: Uuid,
    pub academic_year_id: Uuid,
    pub from_date: NaiveDate,
    pub to_date: NaiveDate,
    pub half_day: Option<String>,
    pub days: Decimal,
    pub reason: String,
    pub attachment_note: String,
    pub status: String,
    pub requested_by: Option<Uuid>,
    pub decided_by: Option<Uuid>,
    pub decided_at: Option<DateTime<Utc>>,
    pub decision_note: String,
    pub created_at: DateTime<Utc>,
    pub hlc: String,
}

impl LeaveRequest {
    pub fn status(&self) -> RequestStatus {
        self.status.parse().unwrap_or(RequestStatus::Pending)
    }
}

/// A request with the names shown in lists.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct LeaveRequestView {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub request: LeaveRequest,
    pub staff_name: String,
    pub staff_number: String,
    pub type_name: String,
    pub decided_by_name: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RequestFilter {
    pub academic_year_id: Option<Uuid>,
    pub staff_id: Option<Uuid>,
    pub status: Option<RequestStatus>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

const REQ_COLS: &str = "r.id, r.school_id, r.staff_id, r.leave_type_id, r.academic_year_id, r.from_date, r.to_date, r.half_day, r.days,
            r.reason, r.attachment_note, r.status, r.requested_by, r.decided_by, r.decided_at, r.decision_note, r.created_at, r.hlc";

pub async fn list_requests(
    conn: &mut PgConnection,
    school_id: Uuid,
    f: &RequestFilter,
) -> DbResult<Vec<LeaveRequestView>> {
    Ok(sqlx::query_as::<_, LeaveRequestView>(sqlx::AssertSqlSafe(format!(
        "SELECT {REQ_COLS}, st.full_name AS staff_name, st.staff_number, t.name AS type_name, u.full_name AS decided_by_name
         FROM leave_requests r
         JOIN staff st ON st.id = r.staff_id
         JOIN leave_types t ON t.id = r.leave_type_id
         LEFT JOIN users u ON u.id = r.decided_by
         WHERE r.school_id = $1 AND r.deleted_at IS NULL
           AND ($2::uuid IS NULL OR r.academic_year_id = $2)
           AND ($3::uuid IS NULL OR r.staff_id = $3)
           AND ($4::text IS NULL OR r.status = $4)
           AND ($5::date IS NULL OR r.to_date >= $5)
           AND ($6::date IS NULL OR r.from_date <= $6)
         ORDER BY (r.status = 'pending') DESC, r.from_date DESC, r.created_at DESC"
    )))
    .bind(school_id)
    .bind(f.academic_year_id)
    .bind(f.staff_id)
    .bind(f.status.map(|s| s.as_str()))
    .bind(f.from)
    .bind(f.to)
    .fetch_all(conn)
    .await?)
}

pub async fn get_request(conn: &mut PgConnection, id: Uuid) -> DbResult<LeaveRequest> {
    sqlx::query_as::<_, LeaveRequest>(sqlx::AssertSqlSafe(format!(
        "SELECT {REQ_COLS} FROM leave_requests r WHERE r.id = $1 AND r.deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("leave request".into()))
}

/// The school calendar for a date range: weekend days and holidays.
pub async fn calendar(
    conn: &mut PgConnection,
    school_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> DbResult<Calendar> {
    Ok(Calendar {
        weekend: attendance::weekend_days(&mut *conn, school_id).await?,
        holidays: attendance::holidays(&mut *conn, school_id, Some(from), Some(to))
            .await?
            .into_iter()
            .map(|h| (h.starts_on, h.ends_on))
            .collect(),
    })
}

#[derive(Debug, Clone)]
pub struct NewRequest {
    pub staff_id: Uuid,
    pub leave_type_id: Uuid,
    pub from_date: NaiveDate,
    pub to_date: NaiveDate,
    pub half_day: Option<HalfDay>,
    pub reason: String,
    pub attachment_note: String,
    pub requested_by: Uuid,
}

async fn year_for(
    conn: &mut PgConnection,
    school_id: Uuid,
    date: NaiveDate,
) -> DbResult<AcademicYear> {
    academics::list_years(conn, school_id)
        .await?
        .into_iter()
        .find(|y| y.start_date <= date && date <= y.end_date)
        .ok_or_else(|| DbError::Conflict("the dates are outside every academic year".into()))
}

/// Create a request (pending, or approved at once when the type needs no approval);
/// refuses overlaps with the staff member's live requests and more days than remain.
pub async fn create_request(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    input: &NewRequest,
) -> DbResult<LeaveRequest> {
    let t = get_type(&mut *conn, input.leave_type_id).await?;
    if t.school_id != school_id {
        return Err(DbError::NotFound("leave type".into()));
    }
    let year = year_for(&mut *conn, school_id, input.from_date).await?;
    if input.to_date > year.end_date {
        return Err(DbError::Conflict(
            "the leave runs past the end of the academic year".into(),
        ));
    }
    let cal = calendar(&mut *conn, school_id, input.from_date, input.to_date).await?;
    let working = rules::working_days(input.from_date, input.to_date, &cal);
    rules::check_dates(
        input.from_date,
        input.to_date,
        input.half_day,
        working.len(),
    )
    .map_err(DbError::Conflict)?;
    let days = rules::leave_days(working.len(), input.half_day);
    let live = list_requests(
        &mut *conn,
        school_id,
        &RequestFilter {
            staff_id: Some(input.staff_id),
            from: Some(input.from_date),
            to: Some(input.to_date),
            ..Default::default()
        },
    )
    .await?;
    if live.iter().any(|r| r.request.status().is_live()) {
        return Err(DbError::Conflict(
            "this staff member already has leave requested for these dates".into(),
        ));
    }
    let balance = balance_of(&mut *conn, school_id, year.id, input.staff_id, t.id).await?;
    rules::check_balance(days, &balance).map_err(DbError::Conflict)?;
    let status = if t.needs_approval {
        RequestStatus::Pending
    } else {
        RequestStatus::Approved
    };
    let row = sqlx::query_as::<_, LeaveRequest>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO leave_requests AS r (id, school_id, staff_id, leave_type_id, academic_year_id, from_date, to_date, half_day, days, reason,
                                    attachment_note, status, requested_by, decided_by, decided_at, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $13, $16) RETURNING {REQ_COLS}"
    )))
    .bind(new_id())
    .bind(school_id)
    .bind(input.staff_id)
    .bind(t.id)
    .bind(year.id)
    .bind(input.from_date)
    .bind(input.to_date)
    .bind(input.half_day.map(HalfDay::as_str))
    .bind(days)
    .bind(input.reason.trim())
    .bind(input.attachment_note.trim())
    .bind(status.as_str())
    .bind(input.requested_by)
    .bind(if status == RequestStatus::Approved { Some(input.requested_by) } else { None })
    .bind(if status == RequestStatus::Approved { Some(Utc::now()) } else { None })
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        &mut *conn,
        hlc,
        actor,
        Action::Create,
        "leave_request",
        Some(row.id),
        None,
        Some(snapshot(&row)),
    )
    .await?;
    if status == RequestStatus::Approved {
        mark_staff_leave(
            &mut *conn,
            hlc,
            actor,
            &row,
            &t,
            &working,
            input.requested_by,
        )
        .await?;
    }
    Ok(row)
}

/// Write `leave` rows for every working day of an approved request.
async fn mark_staff_leave(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    r: &LeaveRequest,
    t: &LeaveType,
    working: &[NaiveDate],
    marked_by: Uuid,
) -> DbResult<()> {
    let note = format!(
        "Leave: {}{}",
        t.name,
        r.half_day
            .as_deref()
            .map(|h| format!(" ({h})"))
            .unwrap_or_default()
    );
    for d in working {
        let old: Option<serde_json::Value> = sqlx::query_scalar(
            "SELECT json_build_object('status', status, 'note', note) FROM staff_attendance WHERE staff_id = $1 AND date = $2 AND deleted_at IS NULL",
        )
        .bind(r.staff_id)
        .bind(d)
        .fetch_optional(&mut *conn)
        .await?;
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO staff_attendance (id, school_id, staff_id, date, status, note, marked_by, leave_request_id, updated_by, hlc)
             VALUES ($1, $2, $3, $4, 'leave', $5, $6, $7, $8, $9)
             ON CONFLICT (staff_id, date) WHERE deleted_at IS NULL DO UPDATE
               SET status = 'leave', note = EXCLUDED.note, marked_by = EXCLUDED.marked_by, arrived_at = NULL,
                   leave_request_id = EXCLUDED.leave_request_id, updated_at = now(), updated_by = EXCLUDED.updated_by, hlc = EXCLUDED.hlc
             RETURNING id",
        )
        .bind(new_id())
        .bind(r.school_id)
        .bind(r.staff_id)
        .bind(d)
        .bind(&note)
        .bind(marked_by)
        .bind(r.id)
        .bind(actor.user_id)
        .bind(hlc.now().to_string())
        .fetch_one(&mut *conn)
        .await?;
        audit::record(
            &mut *conn,
            hlc,
            actor,
            if old.is_some() { Action::Update } else { Action::Create },
            "staff_attendance",
            Some(id),
            old,
            Some(json!({ "staff_id": r.staff_id, "date": d, "status": "leave", "note": note, "leave_request_id": r.id })),
        )
        .await?;
    }
    Ok(())
}

/// Remove the attendance rows an approved request wrote (the days become unmarked again).
async fn unmark_staff_leave(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    request_id: Uuid,
) -> DbResult<usize> {
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "UPDATE staff_attendance SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3
         WHERE leave_request_id = $1 AND deleted_at IS NULL RETURNING id",
    )
    .bind(request_id)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_all(&mut *conn)
    .await?;
    for id in &ids {
        audit::record(
            &mut *conn,
            hlc,
            actor,
            Action::Delete,
            "staff_attendance",
            Some(*id),
            Some(json!({ "leave_request_id": request_id, "status": "leave" })),
            None,
        )
        .await?;
    }
    Ok(ids.len())
}

async fn set_request_status(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    r: &LeaveRequest,
    status: RequestStatus,
    decided_by: Option<Uuid>,
    note: &str,
) -> DbResult<LeaveRequest> {
    let row = sqlx::query_as::<_, LeaveRequest>(sqlx::AssertSqlSafe(format!(
        "UPDATE leave_requests AS r SET status = $2, decided_by = COALESCE($3, decided_by), decided_at = CASE WHEN $3::uuid IS NULL THEN decided_at ELSE now() END,
                decision_note = $4, updated_at = now(), updated_by = $5, hlc = $6
         WHERE id = $1 RETURNING {REQ_COLS}"
    )))
    .bind(r.id)
    .bind(status.as_str())
    .bind(decided_by)
    .bind(note.trim())
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "leave_request",
        Some(r.id),
        Some(snapshot(r)),
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

/// Approve or reject a pending request. Approval marks the staff attendance.
pub async fn decide_request(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    approve: bool,
    note: &str,
    decided_by: Uuid,
) -> DbResult<LeaveRequest> {
    let r = get_request(&mut *conn, id).await?;
    let to = if approve {
        RequestStatus::Approved
    } else {
        RequestStatus::Rejected
    };
    if !r.status().can_become(to) {
        return Err(DbError::Conflict(format!(
            "a {} request cannot be {}",
            r.status,
            to.as_str()
        )));
    }
    if approve {
        let balance = balance_of(
            &mut *conn,
            r.school_id,
            r.academic_year_id,
            r.staff_id,
            r.leave_type_id,
        )
        .await?;
        // The request's own days are in `pending`; they must fit the available balance.
        if r.days > balance.available() {
            return Err(DbError::Conflict(format!(
                "only {} days are left for this leave type",
                balance.available().normalize()
            )));
        }
    }
    let row = set_request_status(&mut *conn, hlc, actor, &r, to, Some(decided_by), note).await?;
    if approve {
        let t = get_type(&mut *conn, r.leave_type_id).await?;
        let cal = calendar(&mut *conn, r.school_id, r.from_date, r.to_date).await?;
        let working = rules::working_days(r.from_date, r.to_date, &cal);
        mark_staff_leave(&mut *conn, hlc, actor, &row, &t, &working, decided_by).await?;
    }
    Ok(row)
}

/// Withdraw a pending or approved request; an approved one's attendance rows are removed.
pub async fn withdraw_request(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    note: &str,
) -> DbResult<LeaveRequest> {
    let r = get_request(&mut *conn, id).await?;
    if !r.status().can_become(RequestStatus::Withdrawn) {
        return Err(DbError::Conflict(format!(
            "a {} request cannot be withdrawn",
            r.status
        )));
    }
    let row = set_request_status(
        &mut *conn,
        hlc,
        actor,
        &r,
        RequestStatus::Withdrawn,
        None,
        note,
    )
    .await?;
    if r.status() == RequestStatus::Approved {
        unmark_staff_leave(&mut *conn, hlc, actor, id).await?;
    }
    Ok(row)
}

// ---- student requests -------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StudentLeaveRequest {
    pub id: Uuid,
    pub school_id: Uuid,
    pub student_id: Uuid,
    pub academic_year_id: Uuid,
    pub from_date: NaiveDate,
    pub to_date: NaiveDate,
    pub reason: String,
    pub guardian_id: Option<Uuid>,
    pub status: String,
    pub recorded_by: Option<Uuid>,
    pub decided_by: Option<Uuid>,
    pub decided_at: Option<DateTime<Utc>>,
    pub decision_note: String,
    pub created_at: DateTime<Utc>,
    pub hlc: String,
}

impl StudentLeaveRequest {
    pub fn status(&self) -> RequestStatus {
        self.status.parse().unwrap_or(RequestStatus::Pending)
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct StudentLeaveView {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub request: StudentLeaveRequest,
    pub student_number: String,
    pub full_name: String,
    pub class_name: String,
    pub section_id: Option<Uuid>,
    pub section_name: String,
    pub guardian_name: Option<String>,
    pub decided_by_name: Option<String>,
    /// Working days in the range.
    pub days: i64,
}

#[derive(Debug, Clone, Default)]
pub struct StudentFilter {
    pub academic_year_id: Option<Uuid>,
    pub student_id: Option<Uuid>,
    pub section_ids: Option<Vec<Uuid>>,
    pub status: Option<RequestStatus>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

const SREQ_COLS: &str = "r.id, r.school_id, r.student_id, r.academic_year_id, r.from_date, r.to_date, r.reason, r.guardian_id, r.status,
            r.recorded_by, r.decided_by, r.decided_at, r.decision_note, r.created_at, r.hlc";

pub async fn list_student_requests(
    conn: &mut PgConnection,
    school_id: Uuid,
    f: &StudentFilter,
) -> DbResult<Vec<StudentLeaveView>> {
    let rows = sqlx::query_as::<_, StudentLeaveView>(sqlx::AssertSqlSafe(format!(
        "SELECT {SREQ_COLS}, s.student_number, s.full_name, COALESCE(c.name, '') AS class_name, e.section_id, COALESCE(sec.name, '') AS section_name,
                g.full_name AS guardian_name, u.full_name AS decided_by_name, 0::bigint AS days
         FROM student_leave_requests r
         JOIN students s ON s.id = r.student_id
         LEFT JOIN enrollments e ON e.student_id = r.student_id AND e.academic_year_id = r.academic_year_id AND e.deleted_at IS NULL
         LEFT JOIN sections sec ON sec.id = e.section_id
         LEFT JOIN classes c ON c.id = sec.class_id
         LEFT JOIN guardians g ON g.id = r.guardian_id
         LEFT JOIN users u ON u.id = r.decided_by
         WHERE r.school_id = $1 AND r.deleted_at IS NULL
           AND ($2::uuid IS NULL OR r.academic_year_id = $2)
           AND ($3::uuid IS NULL OR r.student_id = $3)
           AND ($4::uuid[] IS NULL OR e.section_id = ANY($4))
           AND ($5::text IS NULL OR r.status = $5)
           AND ($6::date IS NULL OR r.to_date >= $6)
           AND ($7::date IS NULL OR r.from_date <= $7)
         ORDER BY (r.status = 'pending') DESC, r.from_date DESC, r.created_at DESC"
    )))
    .bind(school_id)
    .bind(f.academic_year_id)
    .bind(f.student_id)
    .bind(f.section_ids.as_deref())
    .bind(f.status.map(|s| s.as_str()))
    .bind(f.from)
    .bind(f.to)
    .fetch_all(&mut *conn)
    .await?;
    if rows.is_empty() {
        return Ok(rows);
    }
    let from = rows
        .iter()
        .map(|r| r.request.from_date)
        .min()
        .unwrap_or_default();
    let to = rows
        .iter()
        .map(|r| r.request.to_date)
        .max()
        .unwrap_or_default();
    let cal = calendar(conn, school_id, from, to).await?;
    Ok(rows
        .into_iter()
        .map(|mut r| {
            r.days = rules::working_days(r.request.from_date, r.request.to_date, &cal).len() as i64;
            r
        })
        .collect())
}

pub async fn get_student_request(
    conn: &mut PgConnection,
    id: Uuid,
) -> DbResult<StudentLeaveRequest> {
    sqlx::query_as::<_, StudentLeaveRequest>(sqlx::AssertSqlSafe(format!(
        "SELECT {SREQ_COLS} FROM student_leave_requests r WHERE r.id = $1 AND r.deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("student leave request".into()))
}

#[derive(Debug, Clone)]
pub struct NewStudentRequest {
    pub student_id: Uuid,
    pub from_date: NaiveDate,
    pub to_date: NaiveDate,
    pub reason: String,
    pub guardian_id: Option<Uuid>,
    pub recorded_by: Uuid,
}

pub async fn create_student_request(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    input: &NewStudentRequest,
) -> DbResult<StudentLeaveRequest> {
    let year = year_for(&mut *conn, school_id, input.from_date).await?;
    if input.to_date > year.end_date {
        return Err(DbError::Conflict(
            "the leave runs past the end of the academic year".into(),
        ));
    }
    let cal = calendar(&mut *conn, school_id, input.from_date, input.to_date).await?;
    let working = rules::working_days(input.from_date, input.to_date, &cal);
    rules::check_dates(input.from_date, input.to_date, None, working.len())
        .map_err(DbError::Conflict)?;
    let live = list_student_requests(
        &mut *conn,
        school_id,
        &StudentFilter {
            student_id: Some(input.student_id),
            from: Some(input.from_date),
            to: Some(input.to_date),
            ..Default::default()
        },
    )
    .await?;
    if live.iter().any(|r| r.request.status().is_live()) {
        return Err(DbError::Conflict(
            "this student already has leave requested for these dates".into(),
        ));
    }
    let row = sqlx::query_as::<_, StudentLeaveRequest>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO student_leave_requests AS r (id, school_id, student_id, academic_year_id, from_date, to_date, reason, guardian_id, recorded_by, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9, $10) RETURNING {SREQ_COLS}"
    )))
    .bind(new_id())
    .bind(school_id)
    .bind(input.student_id)
    .bind(year.id)
    .bind(input.from_date)
    .bind(input.to_date)
    .bind(input.reason.trim())
    .bind(input.guardian_id)
    .bind(input.recorded_by)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "student_leave_request",
        Some(row.id),
        None,
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

/// Mark the student's entry `leave` on every existing sheet of the range.
async fn mark_student_leave(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    r: &StudentLeaveRequest,
) -> DbResult<usize> {
    #[derive(sqlx::FromRow)]
    struct Hit {
        id: Uuid,
        sheet_id: Uuid,
        status: String,
        note: Option<String>,
    }
    let hits = sqlx::query_as::<_, Hit>(
        "SELECT e.id, e.sheet_id, e.status, e.note FROM attendance_entries e
         WHERE e.student_id = $1 AND e.date BETWEEN $2 AND $3 AND e.deleted_at IS NULL",
    )
    .bind(r.student_id)
    .bind(r.from_date)
    .bind(r.to_date)
    .fetch_all(&mut *conn)
    .await?;
    let mut sheets = Vec::new();
    for h in &hits {
        sqlx::query(
            "UPDATE attendance_entries SET status = 'leave', note = $2, leave_request_id = $3, updated_at = now(), updated_by = $4, hlc = $5 WHERE id = $1",
        )
        .bind(h.id)
        .bind(APPROVED_NOTE)
        .bind(r.id)
        .bind(actor.user_id)
        .bind(hlc.now().to_string())
        .execute(&mut *conn)
        .await?;
        audit::record(
            &mut *conn,
            hlc,
            actor,
            Action::Update,
            "attendance_entry",
            Some(h.id),
            Some(json!({ "status": h.status, "note": h.note })),
            Some(json!({ "status": "leave", "note": APPROVED_NOTE, "leave_request_id": r.id })),
        )
        .await?;
        if !sheets.contains(&h.sheet_id) {
            sheets.push(h.sheet_id);
        }
    }
    recount_sheets(&mut *conn, hlc, actor, &sheets).await?;
    Ok(hits.len())
}

/// Entries an approved request marked go back to `absent` (the student was away).
async fn unmark_student_leave(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    request_id: Uuid,
) -> DbResult<usize> {
    let hits: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "UPDATE attendance_entries SET status = 'absent', note = NULL, leave_request_id = NULL, updated_at = now(), updated_by = $2, hlc = $3
         WHERE leave_request_id = $1 AND deleted_at IS NULL RETURNING id, sheet_id",
    )
    .bind(request_id)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_all(&mut *conn)
    .await?;
    let mut sheets = Vec::new();
    for (id, sheet) in &hits {
        audit::record(
            &mut *conn,
            hlc,
            actor,
            Action::Update,
            "attendance_entry",
            Some(*id),
            Some(json!({ "status": "leave", "leave_request_id": request_id })),
            Some(json!({ "status": "absent" })),
        )
        .await?;
        if !sheets.contains(sheet) {
            sheets.push(*sheet);
        }
    }
    recount_sheets(&mut *conn, hlc, actor, &sheets).await?;
    Ok(hits.len())
}

async fn recount_sheets(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    sheets: &[Uuid],
) -> DbResult<()> {
    for s in sheets {
        sqlx::query(
            "UPDATE attendance_sheets SET
                present = (SELECT count(*) FROM attendance_entries WHERE sheet_id = $1 AND status = 'present' AND deleted_at IS NULL),
                absent = (SELECT count(*) FROM attendance_entries WHERE sheet_id = $1 AND status = 'absent' AND deleted_at IS NULL),
                late = (SELECT count(*) FROM attendance_entries WHERE sheet_id = $1 AND status = 'late' AND deleted_at IS NULL),
                leave = (SELECT count(*) FROM attendance_entries WHERE sheet_id = $1 AND status = 'leave' AND deleted_at IS NULL),
                updated_at = now(), updated_by = $2, hlc = $3
             WHERE id = $1",
        )
        .bind(s)
        .bind(actor.user_id)
        .bind(hlc.now().to_string())
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

async fn set_student_status(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    r: &StudentLeaveRequest,
    status: RequestStatus,
    decided_by: Option<Uuid>,
    note: &str,
) -> DbResult<StudentLeaveRequest> {
    let row = sqlx::query_as::<_, StudentLeaveRequest>(sqlx::AssertSqlSafe(format!(
        "UPDATE student_leave_requests AS r SET status = $2, decided_by = COALESCE($3, decided_by), decided_at = CASE WHEN $3::uuid IS NULL THEN decided_at ELSE now() END,
                decision_note = $4, updated_at = now(), updated_by = $5, hlc = $6
         WHERE id = $1 RETURNING {SREQ_COLS}"
    )))
    .bind(r.id)
    .bind(status.as_str())
    .bind(decided_by)
    .bind(note.trim())
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "student_leave_request",
        Some(r.id),
        Some(snapshot(r)),
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

pub async fn decide_student_request(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    approve: bool,
    note: &str,
    decided_by: Uuid,
) -> DbResult<StudentLeaveRequest> {
    let r = get_student_request(&mut *conn, id).await?;
    let to = if approve {
        RequestStatus::Approved
    } else {
        RequestStatus::Rejected
    };
    if !r.status().can_become(to) {
        return Err(DbError::Conflict(format!(
            "a {} request cannot be {}",
            r.status,
            to.as_str()
        )));
    }
    let row = set_student_status(&mut *conn, hlc, actor, &r, to, Some(decided_by), note).await?;
    if approve {
        mark_student_leave(&mut *conn, hlc, actor, &row).await?;
    }
    Ok(row)
}

pub async fn withdraw_student_request(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    note: &str,
) -> DbResult<StudentLeaveRequest> {
    let r = get_student_request(&mut *conn, id).await?;
    if !r.status().can_become(RequestStatus::Withdrawn) {
        return Err(DbError::Conflict(format!(
            "a {} request cannot be withdrawn",
            r.status
        )));
    }
    let row = set_student_status(
        &mut *conn,
        hlc,
        actor,
        &r,
        RequestStatus::Withdrawn,
        None,
        note,
    )
    .await?;
    if r.status() == RequestStatus::Approved {
        unmark_student_leave(&mut *conn, hlc, actor, id).await?;
    }
    Ok(row)
}

/// Students of a section on approved leave on a date → request id. Empty on Lite.
pub async fn approved_student_leave_on(
    conn: &mut PgConnection,
    section_id: Uuid,
    date: NaiveDate,
) -> DbResult<HashMap<Uuid, Uuid>> {
    let rows: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT r.student_id, r.id FROM student_leave_requests r
         JOIN enrollments e ON e.student_id = r.student_id AND e.academic_year_id = r.academic_year_id AND e.deleted_at IS NULL
         WHERE e.section_id = $1 AND r.status = 'approved' AND r.deleted_at IS NULL AND $2 BETWEEN r.from_date AND r.to_date",
    )
    .bind(section_id)
    .bind(date)
    .fetch_all(conn)
    .await?;
    Ok(rows.into_iter().collect())
}

// ---- approver setting --------------------------------------------------------------------------

pub async fn approver(conn: &mut PgConnection, school_id: Uuid) -> DbResult<Option<Uuid>> {
    Ok(settings::get(conn, Some(school_id), KEY_APPROVER)
        .await?
        .and_then(|v| v.as_str().and_then(|s| s.parse().ok())))
}

pub async fn set_approver(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    staff_id: Option<Uuid>,
) -> DbResult<()> {
    settings::set(
        conn,
        hlc,
        actor,
        Some(school_id),
        KEY_APPROVER,
        match staff_id {
            Some(id) => json!(id.to_string()),
            None => serde_json::Value::Null,
        },
    )
    .await?;
    Ok(())
}

// ---- staff register ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct StaffMark {
    pub staff_id: Uuid,
    pub date: NaiveDate,
    pub status: String,
    pub arrived_at: Option<NaiveTime>,
    pub leave_request_id: Option<Uuid>,
}

/// Every staff attendance mark of the school in a date range.
pub async fn staff_marks(
    conn: &mut PgConnection,
    school_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> DbResult<Vec<StaffMark>> {
    Ok(sqlx::query_as::<_, StaffMark>(
        "SELECT staff_id, date, status, arrived_at, leave_request_id FROM staff_attendance
         WHERE school_id = $1 AND date BETWEEN $2 AND $3 AND deleted_at IS NULL ORDER BY date",
    )
    .bind(school_id)
    .bind(from)
    .bind(to)
    .fetch_all(conn)
    .await?)
}
