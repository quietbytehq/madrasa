//! Hostel (Standard stage 6): hostels with rooms and numbered beds, residents allocated per
//! academic year, the leave-out and visitor register, and the monthly charge that joins
//! the fee voucher under the hostel's fee head.

use chrono::{DateTime, NaiveDate, Utc};
use madrasa_core::hlc::Hlc;
use madrasa_core::hostel as rules;
use madrasa_core::ids::new_id;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::audit::{self, snapshot, Action};
use crate::{map_unique, Actor, DbError, DbResult};

// ---- hostels --------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Hostel {
    pub id: Uuid,
    pub school_id: Uuid,
    pub name: String,
    pub kind: String,
    pub warden_name: String,
    pub warden_phone: String,
    pub monthly_fee: Decimal,
    pub fee_head_id: Option<Uuid>,
    pub requires_consent: bool,
    pub active: bool,
    pub notes: String,
    pub custom: Value,
    pub hlc: String,
}

/// A hostel with its fee head name and occupancy. Mirrored by `HostelView` in the UI.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct HostelView {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub hostel: Hostel,
    pub fee_head_name: Option<String>,
    pub rooms: i64,
    pub beds: i64,
    /// Residents with an open allocation (any year).
    pub occupied: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HostelInput {
    pub name: String,
    #[serde(default = "mixed")]
    pub kind: String,
    #[serde(default)]
    pub warden_name: String,
    #[serde(default)]
    pub warden_phone: String,
    #[serde(default)]
    pub monthly_fee: Decimal,
    pub fee_head_id: Option<Uuid>,
    #[serde(default = "yes")]
    pub requires_consent: bool,
    #[serde(default = "yes")]
    pub active: bool,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub custom: Value,
}

fn yes() -> bool {
    true
}
fn mixed() -> String {
    "mixed".into()
}

const HOSTEL_COLS: &str = "h.id, h.school_id, h.name, h.kind, h.warden_name, h.warden_phone, h.monthly_fee, h.fee_head_id, h.requires_consent, h.active, h.notes, h.custom, h.hlc";
const HOSTEL_EXTRA: &str = "fh.name AS fee_head_name,
    (SELECT count(*) FROM hostel_rooms r WHERE r.hostel_id = h.id AND r.deleted_at IS NULL) AS rooms,
    (SELECT coalesce(sum(r.beds), 0) FROM hostel_rooms r WHERE r.hostel_id = h.id AND r.deleted_at IS NULL AND r.active) AS beds,
    (SELECT count(*) FROM hostel_allocations a WHERE a.hostel_id = h.id AND a.to_date IS NULL AND a.deleted_at IS NULL) AS occupied";
const HOSTEL_FROM: &str =
    "FROM hostels h LEFT JOIN fee_heads fh ON fh.id = h.fee_head_id AND fh.deleted_at IS NULL";

pub async fn list_hostels(conn: &mut PgConnection, school_id: Uuid) -> DbResult<Vec<HostelView>> {
    Ok(sqlx::query_as::<_, HostelView>(sqlx::AssertSqlSafe(format!(
        "SELECT {HOSTEL_COLS}, {HOSTEL_EXTRA} {HOSTEL_FROM} WHERE h.school_id = $1 AND h.deleted_at IS NULL ORDER BY h.name"
    )))
    .bind(school_id)
    .fetch_all(conn)
    .await?)
}

pub async fn get_hostel(conn: &mut PgConnection, id: Uuid) -> DbResult<HostelView> {
    sqlx::query_as::<_, HostelView>(sqlx::AssertSqlSafe(format!(
        "SELECT {HOSTEL_COLS}, {HOSTEL_EXTRA} {HOSTEL_FROM} WHERE h.id = $1 AND h.deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("hostel".into()))
}

fn check_hostel(input: &HostelInput) -> DbResult<()> {
    if input.name.trim().is_empty() {
        return Err(DbError::Conflict("the hostel needs a name".into()));
    }
    if !["boys", "girls", "mixed"].contains(&input.kind.as_str()) {
        return Err(DbError::Conflict("kind is boys, girls or mixed".into()));
    }
    if input.monthly_fee < Decimal::ZERO {
        return Err(DbError::Conflict(
            "the monthly fee cannot be negative".into(),
        ));
    }
    Ok(())
}

pub async fn create_hostel(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    input: &HostelInput,
) -> DbResult<HostelView> {
    check_hostel(input)?;
    let id = new_id();
    sqlx::query(
        "INSERT INTO hostels (id, school_id, name, kind, warden_name, warden_phone, monthly_fee, fee_head_id, requires_consent, active, notes, custom, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
    )
    .bind(id)
    .bind(school_id)
    .bind(input.name.trim())
    .bind(&input.kind)
    .bind(input.warden_name.trim())
    .bind(input.warden_phone.trim())
    .bind(input.monthly_fee)
    .bind(input.fee_head_id)
    .bind(input.requires_consent)
    .bind(input.active)
    .bind(input.notes.trim())
    .bind(if input.custom.is_object() { input.custom.clone() } else { json!({}) })
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a hostel with that name"))?;
    let view = get_hostel(&mut *conn, id).await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "hostel",
        Some(id),
        None,
        Some(snapshot(&view.hostel)),
    )
    .await?;
    Ok(view)
}

pub async fn update_hostel(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    input: &HostelInput,
) -> DbResult<HostelView> {
    check_hostel(input)?;
    let old = get_hostel(&mut *conn, id).await?;
    sqlx::query(
        "UPDATE hostels SET name = $2, kind = $3, warden_name = $4, warden_phone = $5, monthly_fee = $6, fee_head_id = $7,
                requires_consent = $8, active = $9, notes = $10, custom = $11, updated_at = now(), updated_by = $12, hlc = $13
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(input.name.trim())
    .bind(&input.kind)
    .bind(input.warden_name.trim())
    .bind(input.warden_phone.trim())
    .bind(input.monthly_fee)
    .bind(input.fee_head_id)
    .bind(input.requires_consent)
    .bind(input.active)
    .bind(input.notes.trim())
    .bind(if input.custom.is_object() { input.custom.clone() } else { json!({}) })
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a hostel with that name"))?;
    let view = get_hostel(&mut *conn, id).await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "hostel",
        Some(id),
        Some(snapshot(&old.hostel)),
        Some(snapshot(&view.hostel)),
    )
    .await?;
    Ok(view)
}

/// Remove a hostel and its rooms; refused while a resident is allocated.
pub async fn delete_hostel(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
) -> DbResult<()> {
    let old = get_hostel(&mut *conn, id).await?;
    if old.occupied > 0 {
        return Err(DbError::Conflict(
            "residents are still allocated in this hostel; end their allocations first".into(),
        ));
    }
    let now = hlc.now().to_string();
    sqlx::query("UPDATE hostel_rooms SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE hostel_id = $1 AND deleted_at IS NULL")
        .bind(id)
        .bind(actor.user_id)
        .bind(&now)
        .execute(&mut *conn)
        .await?;
    sqlx::query("UPDATE hostels SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1")
        .bind(id)
        .bind(actor.user_id)
        .bind(&now)
        .execute(&mut *conn)
        .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Delete,
        "hostel",
        Some(id),
        Some(snapshot(&old.hostel)),
        None,
    )
    .await?;
    Ok(())
}

// ---- rooms ----------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Room {
    pub id: Uuid,
    pub school_id: Uuid,
    pub hostel_id: Uuid,
    pub floor: String,
    pub name: String,
    pub beds: i32,
    pub monthly_fee: Option<Decimal>,
    pub active: bool,
    pub notes: String,
    pub hlc: String,
}

/// A room with its residents counted. Mirrored by `RoomView` in the UI.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RoomView {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub room: Room,
    pub occupied: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RoomInput {
    #[serde(default)]
    pub floor: String,
    pub name: String,
    pub beds: i32,
    pub monthly_fee: Option<Decimal>,
    #[serde(default = "yes")]
    pub active: bool,
    #[serde(default)]
    pub notes: String,
}

const ROOM_COLS: &str = "r.id, r.school_id, r.hostel_id, r.floor, r.name, r.beds, r.monthly_fee, r.active, r.notes, r.hlc";
const ROOM_EXTRA: &str = "(SELECT count(*) FROM hostel_allocations a WHERE a.room_id = r.id AND a.to_date IS NULL AND a.deleted_at IS NULL) AS occupied";

pub async fn list_rooms(conn: &mut PgConnection, hostel_id: Uuid) -> DbResult<Vec<RoomView>> {
    Ok(sqlx::query_as::<_, RoomView>(sqlx::AssertSqlSafe(format!(
        "SELECT {ROOM_COLS}, {ROOM_EXTRA} FROM hostel_rooms r WHERE r.hostel_id = $1 AND r.deleted_at IS NULL ORDER BY r.floor, r.name"
    )))
    .bind(hostel_id)
    .fetch_all(conn)
    .await?)
}

pub async fn get_room(conn: &mut PgConnection, id: Uuid) -> DbResult<RoomView> {
    sqlx::query_as::<_, RoomView>(sqlx::AssertSqlSafe(format!(
        "SELECT {ROOM_COLS}, {ROOM_EXTRA} FROM hostel_rooms r WHERE r.id = $1 AND r.deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("room".into()))
}

fn check_room(input: &RoomInput) -> DbResult<()> {
    if input.name.trim().is_empty() {
        return Err(DbError::Conflict("the room needs a name or number".into()));
    }
    if input.beds < 0 {
        return Err(DbError::Conflict("beds cannot be negative".into()));
    }
    if input.monthly_fee.is_some_and(|f| f < Decimal::ZERO) {
        return Err(DbError::Conflict(
            "the room's monthly fee cannot be negative".into(),
        ));
    }
    Ok(())
}

pub async fn create_room(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    hostel_id: Uuid,
    input: &RoomInput,
) -> DbResult<RoomView> {
    check_room(input)?;
    let hostel = get_hostel(&mut *conn, hostel_id).await?;
    let id = new_id();
    sqlx::query(
        "INSERT INTO hostel_rooms (id, school_id, hostel_id, floor, name, beds, monthly_fee, active, notes, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(id)
    .bind(hostel.hostel.school_id)
    .bind(hostel_id)
    .bind(input.floor.trim())
    .bind(input.name.trim())
    .bind(input.beds)
    .bind(input.monthly_fee)
    .bind(input.active)
    .bind(input.notes.trim())
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a room with that name in the hostel"))?;
    let view = get_room(&mut *conn, id).await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "hostel_room",
        Some(id),
        None,
        Some(snapshot(&view.room)),
    )
    .await?;
    Ok(view)
}

pub async fn update_room(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    input: &RoomInput,
) -> DbResult<RoomView> {
    check_room(input)?;
    let old = get_room(&mut *conn, id).await?;
    let highest: Option<i32> = sqlx::query_scalar(
        "SELECT max(bed_no) FROM hostel_allocations WHERE room_id = $1 AND to_date IS NULL AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    if highest.is_some_and(|h| h > input.beds) {
        return Err(DbError::Conflict(
            "a resident is in a bed beyond the new count; move them first".into(),
        ));
    }
    sqlx::query(
        "UPDATE hostel_rooms SET floor = $2, name = $3, beds = $4, monthly_fee = $5, active = $6, notes = $7,
                updated_at = now(), updated_by = $8, hlc = $9 WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(input.floor.trim())
    .bind(input.name.trim())
    .bind(input.beds)
    .bind(input.monthly_fee)
    .bind(input.active)
    .bind(input.notes.trim())
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a room with that name in the hostel"))?;
    let view = get_room(&mut *conn, id).await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "hostel_room",
        Some(id),
        Some(snapshot(&old.room)),
        Some(snapshot(&view.room)),
    )
    .await?;
    Ok(view)
}

pub async fn delete_room(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
) -> DbResult<()> {
    let old = get_room(&mut *conn, id).await?;
    if old.occupied > 0 {
        return Err(DbError::Conflict(
            "residents are still in this room; move them first".into(),
        ));
    }
    sqlx::query("UPDATE hostel_rooms SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1")
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
        "hostel_room",
        Some(id),
        Some(snapshot(&old.room)),
        None,
    )
    .await?;
    Ok(())
}

// ---- allocations ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Allocation {
    pub id: Uuid,
    pub school_id: Uuid,
    pub student_id: Uuid,
    pub academic_year_id: Uuid,
    pub hostel_id: Uuid,
    pub room_id: Uuid,
    pub bed_no: i32,
    pub from_date: NaiveDate,
    pub to_date: Option<NaiveDate>,
    pub amount: Option<Decimal>,
    pub notes: String,
    pub hlc: String,
}

/// An allocation with the student, hostel and room named and the charge that applies.
/// Mirrored by `AllocationView` in the UI.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AllocationView {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub allocation: Allocation,
    pub student_number: String,
    pub student_name: String,
    pub gender: String,
    pub class_name: String,
    pub section_name: String,
    pub roll_no: Option<i32>,
    pub guardian_phone: String,
    pub hostel_name: String,
    pub room_name: String,
    pub floor: String,
    pub charge: Decimal,
}

const ALLOC_COLS: &str = "a.id, a.school_id, a.student_id, a.academic_year_id, a.hostel_id, a.room_id, a.bed_no, a.from_date, a.to_date, a.amount, a.notes, a.hlc";
const ALLOC_EXTRA: &str = "st.student_number, st.full_name AS student_name, st.gender,
    coalesce(c.name, '') AS class_name, coalesce(sec.name, '') AS section_name, e.roll_no,
    coalesce((SELECT g.phone FROM student_guardians sg JOIN guardians g ON g.id = sg.guardian_id
              WHERE sg.student_id = st.id AND sg.deleted_at IS NULL AND g.deleted_at IS NULL
              ORDER BY sg.is_primary DESC, sg.created_at LIMIT 1), '') AS guardian_phone,
    h.name AS hostel_name, r.name AS room_name, r.floor,
    coalesce(a.amount, r.monthly_fee, h.monthly_fee) AS charge";
const ALLOC_FROM: &str = "FROM hostel_allocations a
    JOIN students st ON st.id = a.student_id
    JOIN hostels h ON h.id = a.hostel_id
    JOIN hostel_rooms r ON r.id = a.room_id
    LEFT JOIN enrollments e ON e.student_id = a.student_id AND e.academic_year_id = a.academic_year_id AND e.deleted_at IS NULL
    LEFT JOIN sections sec ON sec.id = e.section_id
    LEFT JOIN classes c ON c.id = sec.class_id";

#[derive(Debug, Clone, Default)]
pub struct AllocationFilter {
    pub academic_year_id: Option<Uuid>,
    pub hostel_id: Option<Uuid>,
    pub room_id: Option<Uuid>,
    pub class_id: Option<Uuid>,
    pub student_id: Option<Uuid>,
    pub q: Option<String>,
    pub open_only: bool,
}

pub async fn list_allocations(
    conn: &mut PgConnection,
    school_id: Uuid,
    f: &AllocationFilter,
) -> DbResult<Vec<AllocationView>> {
    Ok(sqlx::query_as::<_, AllocationView>(sqlx::AssertSqlSafe(format!(
        "SELECT {ALLOC_COLS}, {ALLOC_EXTRA} {ALLOC_FROM}
         WHERE a.school_id = $1 AND a.deleted_at IS NULL
           AND ($2::uuid IS NULL OR a.academic_year_id = $2)
           AND ($3::uuid IS NULL OR a.hostel_id = $3)
           AND ($4::uuid IS NULL OR a.room_id = $4)
           AND ($5::uuid IS NULL OR sec.class_id = $5)
           AND ($6::uuid IS NULL OR a.student_id = $6)
           AND ($7::text IS NULL OR st.full_name ILIKE '%' || $7 || '%' OR st.student_number ILIKE '%' || $7 || '%')
           AND (NOT $8 OR a.to_date IS NULL)
         ORDER BY h.name, r.floor, r.name, a.bed_no, a.from_date DESC"
    )))
    .bind(school_id)
    .bind(f.academic_year_id)
    .bind(f.hostel_id)
    .bind(f.room_id)
    .bind(f.class_id)
    .bind(f.student_id)
    .bind(f.q.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(f.open_only)
    .fetch_all(conn)
    .await?)
}

pub async fn get_allocation(conn: &mut PgConnection, id: Uuid) -> DbResult<AllocationView> {
    sqlx::query_as::<_, AllocationView>(sqlx::AssertSqlSafe(format!(
        "SELECT {ALLOC_COLS}, {ALLOC_EXTRA} {ALLOC_FROM} WHERE a.id = $1 AND a.deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("allocation".into()))
}

#[derive(Debug, Clone, Deserialize)]
pub struct AllocateInput {
    pub student_id: Uuid,
    pub academic_year_id: Uuid,
    pub room_id: Uuid,
    pub bed_no: i32,
    pub from_date: NaiveDate,
    pub amount: Option<Decimal>,
    #[serde(default)]
    pub notes: String,
}

/// Put a student in a bed from a date. The bed must exist and be free; an open allocation
/// of the same year ends the day before (or is removed when it had not started yet).
pub async fn allocate(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    input: &AllocateInput,
) -> DbResult<AllocationView> {
    let room = get_room(&mut *conn, input.room_id).await?;
    if room.room.school_id != school_id {
        return Err(DbError::NotFound("room".into()));
    }
    if !room.room.active {
        return Err(DbError::Conflict("the room is not in use".into()));
    }
    if !rules::bed_is_valid(input.bed_no, room.room.beds) {
        return Err(DbError::Conflict(format!(
            "bed {} does not exist; the room has {} beds",
            input.bed_no, room.room.beds
        )));
    }
    if input.amount.is_some_and(|a| a < Decimal::ZERO) {
        return Err(DbError::Conflict("the amount cannot be negative".into()));
    }
    let enrolled: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM enrollments e JOIN students s ON s.id = e.student_id
         WHERE e.student_id = $1 AND e.academic_year_id = $2 AND e.deleted_at IS NULL AND s.deleted_at IS NULL AND s.school_id = $3)",
    )
    .bind(input.student_id)
    .bind(input.academic_year_id)
    .bind(school_id)
    .fetch_one(&mut *conn)
    .await?;
    if !enrolled {
        return Err(DbError::Conflict(
            "the student is not enrolled in that academic year".into(),
        ));
    }
    let open = sqlx::query_as::<_, Allocation>(sqlx::AssertSqlSafe(format!(
        "SELECT {ALLOC_COLS} FROM hostel_allocations a
         WHERE a.student_id = $1 AND a.academic_year_id = $2 AND a.to_date IS NULL AND a.deleted_at IS NULL"
    )))
    .bind(input.student_id)
    .bind(input.academic_year_id)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(prev) = open {
        if prev.room_id == input.room_id
            && prev.bed_no == input.bed_no
            && prev.amount == input.amount
        {
            return get_allocation(conn, prev.id).await;
        }
        let now = hlc.now().to_string();
        if prev.from_date >= input.from_date {
            sqlx::query("UPDATE hostel_allocations SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1")
                .bind(prev.id)
                .bind(actor.user_id)
                .bind(&now)
                .execute(&mut *conn)
                .await?;
        } else {
            sqlx::query("UPDATE hostel_allocations SET to_date = $2, updated_at = now(), updated_by = $3, hlc = $4 WHERE id = $1")
                .bind(prev.id)
                .bind(input.from_date.pred_opt().unwrap_or(input.from_date))
                .bind(actor.user_id)
                .bind(&now)
                .execute(&mut *conn)
                .await?;
        }
        audit::record(
            &mut *conn,
            hlc,
            actor,
            Action::Update,
            "hostel_allocation",
            Some(prev.id),
            Some(snapshot(&prev)),
            Some(json!({ "ended_for": input.room_id, "bed_no": input.bed_no })),
        )
        .await?;
    }
    let taken: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM hostel_allocations WHERE room_id = $1 AND bed_no = $2 AND to_date IS NULL AND deleted_at IS NULL)",
    )
    .bind(input.room_id)
    .bind(input.bed_no)
    .fetch_one(&mut *conn)
    .await?;
    if taken {
        return Err(DbError::Conflict(format!(
            "bed {} in room {} is taken",
            input.bed_no, room.room.name
        )));
    }
    let id = new_id();
    sqlx::query(
        "INSERT INTO hostel_allocations (id, school_id, student_id, academic_year_id, hostel_id, room_id, bed_no, from_date, amount, notes, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    )
    .bind(id)
    .bind(school_id)
    .bind(input.student_id)
    .bind(input.academic_year_id)
    .bind(room.room.hostel_id)
    .bind(room.room.id)
    .bind(input.bed_no)
    .bind(input.from_date)
    .bind(input.amount)
    .bind(input.notes.trim())
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "an open allocation for the student or the bed"))?;
    let view = get_allocation(&mut *conn, id).await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "hostel_allocation",
        Some(id),
        None,
        Some(snapshot(&view.allocation)),
    )
    .await?;
    Ok(view)
}

#[derive(Debug, Clone, Deserialize)]
pub struct AllocationPatch {
    /// `Some(None)` reopens; `Some(Some(d))` ends on `d`.
    pub to_date: Option<Option<NaiveDate>>,
    pub amount: Option<Option<Decimal>>,
    pub notes: Option<String>,
}

pub async fn update_allocation(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    patch: &AllocationPatch,
) -> DbResult<AllocationView> {
    let old = get_allocation(&mut *conn, id).await?;
    let a = &old.allocation;
    let to_date = patch.to_date.unwrap_or(a.to_date);
    if to_date.is_some_and(|t| t < a.from_date) {
        return Err(DbError::Conflict(
            "the end date is before the start date".into(),
        ));
    }
    let amount = patch.amount.unwrap_or(a.amount);
    if amount.is_some_and(|x| x < Decimal::ZERO) {
        return Err(DbError::Conflict("the amount cannot be negative".into()));
    }
    sqlx::query(
        "UPDATE hostel_allocations SET to_date = $2, amount = $3, notes = $4, updated_at = now(), updated_by = $5, hlc = $6
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(to_date)
    .bind(amount)
    .bind(patch.notes.as_deref().map(str::trim).unwrap_or(&a.notes))
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "an open allocation for the student or the bed"))?;
    let view = get_allocation(&mut *conn, id).await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "hostel_allocation",
        Some(id),
        Some(snapshot(&old.allocation)),
        Some(snapshot(&view.allocation)),
    )
    .await?;
    Ok(view)
}

pub async fn delete_allocation(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
) -> DbResult<()> {
    let old = get_allocation(&mut *conn, id).await?;
    sqlx::query("UPDATE hostel_allocations SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1")
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
        "hostel_allocation",
        Some(id),
        Some(snapshot(&old.allocation)),
        None,
    )
    .await?;
    Ok(())
}

/// A room on the occupancy board: every bed with its resident or empty.
#[derive(Debug, Clone, Serialize)]
pub struct BoardRoom {
    pub room: RoomView,
    pub beds: Vec<BoardBed>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BoardBed {
    pub bed_no: i32,
    pub resident: Option<AllocationView>,
}

pub async fn board(
    conn: &mut PgConnection,
    school_id: Uuid,
    hostel_id: Uuid,
    academic_year_id: Uuid,
) -> DbResult<Vec<BoardRoom>> {
    let rooms = list_rooms(&mut *conn, hostel_id).await?;
    let residents = list_allocations(
        conn,
        school_id,
        &AllocationFilter {
            academic_year_id: Some(academic_year_id),
            hostel_id: Some(hostel_id),
            open_only: true,
            ..Default::default()
        },
    )
    .await?;
    Ok(rooms
        .into_iter()
        .map(|room| {
            let beds = (1..=room.room.beds)
                .map(|n| BoardBed {
                    bed_no: n,
                    resident: residents
                        .iter()
                        .find(|a| a.allocation.room_id == room.room.id && a.allocation.bed_no == n)
                        .cloned(),
                })
                .collect();
            BoardRoom { room, beds }
        })
        .collect())
}

/// The hostel line of a student's voucher for a billing month: the hostel's fee head and
/// the amount, when the student has an allocation covering the month in a hostel with a
/// fee head.
pub async fn monthly_charge(
    conn: &mut PgConnection,
    student_id: Uuid,
    billing_month: NaiveDate,
) -> DbResult<Option<(Uuid, Decimal)>> {
    #[derive(sqlx::FromRow)]
    struct ChargeRow {
        fee_head_id: Option<Uuid>,
        amount: Option<Decimal>,
        room_fee: Option<Decimal>,
        hostel_fee: Decimal,
    }
    let (first, last) = rules::month_bounds(billing_month);
    let row: Option<ChargeRow> = sqlx::query_as(
        "SELECT h.fee_head_id, a.amount, r.monthly_fee AS room_fee, h.monthly_fee AS hostel_fee FROM hostel_allocations a
         JOIN hostel_rooms r ON r.id = a.room_id
         JOIN hostels h ON h.id = a.hostel_id
         WHERE a.student_id = $1 AND a.deleted_at IS NULL
           AND a.from_date <= $3 AND (a.to_date IS NULL OR a.to_date >= $2)
         ORDER BY a.from_date DESC LIMIT 1",
    )
    .bind(student_id)
    .bind(first)
    .bind(last)
    .fetch_optional(conn)
    .await?;
    Ok(row.and_then(|c| {
        let head = c.fee_head_id?;
        let amount = rules::monthly_charge(c.amount, c.room_fee, c.hostel_fee);
        (amount > Decimal::ZERO).then_some((head, amount))
    }))
}

/// Residents of the year with fees outstanding (any voucher, unpaid or partial).
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct OutstandingRow {
    pub student_id: Uuid,
    pub student_number: String,
    pub student_name: String,
    pub class_name: String,
    pub section_name: String,
    pub hostel_name: String,
    pub room_name: String,
    pub guardian_phone: String,
    pub vouchers: i64,
    pub balance: Decimal,
}

pub async fn outstanding(
    conn: &mut PgConnection,
    school_id: Uuid,
    academic_year_id: Uuid,
    hostel_id: Option<Uuid>,
) -> DbResult<Vec<OutstandingRow>> {
    Ok(sqlx::query_as::<_, OutstandingRow>(
        "SELECT st.id AS student_id, st.student_number, st.full_name AS student_name,
                coalesce(c.name, '') AS class_name, coalesce(sec.name, '') AS section_name,
                h.name AS hostel_name, r.name AS room_name,
                coalesce((SELECT g.phone FROM student_guardians sg JOIN guardians g ON g.id = sg.guardian_id
                          WHERE sg.student_id = st.id AND sg.deleted_at IS NULL AND g.deleted_at IS NULL
                          ORDER BY sg.is_primary DESC, sg.created_at LIMIT 1), '') AS guardian_phone,
                count(v.id) AS vouchers,
                coalesce(sum(v.total_due + v.late_fee - v.net_paid), 0)::numeric(14,2) AS balance
         FROM hostel_allocations a
         JOIN students st ON st.id = a.student_id
         JOIN hostels h ON h.id = a.hostel_id
         JOIN hostel_rooms r ON r.id = a.room_id
         LEFT JOIN enrollments e ON e.student_id = a.student_id AND e.academic_year_id = a.academic_year_id AND e.deleted_at IS NULL
         LEFT JOIN sections sec ON sec.id = e.section_id
         LEFT JOIN classes c ON c.id = sec.class_id
         JOIN fee_vouchers v ON v.student_id = st.id AND v.status IN ('unpaid', 'partial') AND v.deleted_at IS NULL
         WHERE a.school_id = $1 AND a.academic_year_id = $2 AND a.to_date IS NULL AND a.deleted_at IS NULL
           AND ($3::uuid IS NULL OR a.hostel_id = $3)
         GROUP BY st.id, st.student_number, st.full_name, c.name, sec.name, h.name, r.name
         HAVING sum(v.total_due + v.late_fee - v.net_paid) > 0
         ORDER BY h.name, r.name, st.full_name",
    )
    .bind(school_id)
    .bind(academic_year_id)
    .bind(hostel_id)
    .fetch_all(conn)
    .await?)
}

// ---- register -------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RegisterEntry {
    pub id: Uuid,
    pub school_id: Uuid,
    pub hostel_id: Uuid,
    pub student_id: Uuid,
    pub kind: String,
    pub started_at: DateTime<Utc>,
    pub expected_back: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub visitor_name: String,
    pub visitor_relationship: String,
    pub visitor_phone: String,
    pub guardian_consent: bool,
    pub consent_note: String,
    pub recorded_by: Option<Uuid>,
    pub closed_by: Option<Uuid>,
    pub notes: String,
    pub hlc: String,
}

/// Mirrored by `RegisterView` in the UI.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RegisterView {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub entry: RegisterEntry,
    pub student_number: String,
    pub student_name: String,
    pub hostel_name: String,
    pub room_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegisterInput {
    pub student_id: Uuid,
    pub kind: String,
    pub started_at: Option<DateTime<Utc>>,
    pub expected_back: Option<DateTime<Utc>>,
    #[serde(default)]
    pub visitor_name: String,
    #[serde(default)]
    pub visitor_relationship: String,
    #[serde(default)]
    pub visitor_phone: String,
    #[serde(default)]
    pub guardian_consent: bool,
    #[serde(default)]
    pub consent_note: String,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Default)]
pub struct RegisterFilter {
    pub hostel_id: Option<Uuid>,
    pub student_id: Option<Uuid>,
    pub kind: Option<String>,
    pub open_only: bool,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

const REG_COLS: &str = "x.id, x.school_id, x.hostel_id, x.student_id, x.kind, x.started_at, x.expected_back, x.ended_at, x.visitor_name,
    x.visitor_relationship, x.visitor_phone, x.guardian_consent, x.consent_note, x.recorded_by, x.closed_by, x.notes, x.hlc";
const REG_EXTRA: &str = "st.student_number, st.full_name AS student_name, h.name AS hostel_name,
    coalesce((SELECT r.name FROM hostel_allocations a JOIN hostel_rooms r ON r.id = a.room_id
              WHERE a.student_id = x.student_id AND a.to_date IS NULL AND a.deleted_at IS NULL LIMIT 1), '') AS room_name";
const REG_FROM: &str = "FROM hostel_register x JOIN students st ON st.id = x.student_id JOIN hostels h ON h.id = x.hostel_id";

pub async fn list_register(
    conn: &mut PgConnection,
    school_id: Uuid,
    f: &RegisterFilter,
) -> DbResult<Vec<RegisterView>> {
    Ok(
        sqlx::query_as::<_, RegisterView>(sqlx::AssertSqlSafe(format!(
            "SELECT {REG_COLS}, {REG_EXTRA} {REG_FROM}
         WHERE x.school_id = $1 AND x.deleted_at IS NULL
           AND ($2::uuid IS NULL OR x.hostel_id = $2)
           AND ($3::uuid IS NULL OR x.student_id = $3)
           AND ($4::text IS NULL OR x.kind = $4)
           AND (NOT $5 OR x.ended_at IS NULL)
           AND ($6::date IS NULL OR x.started_at::date >= $6)
           AND ($7::date IS NULL OR x.started_at::date <= $7)
         ORDER BY x.ended_at IS NOT NULL, x.started_at DESC"
        )))
        .bind(school_id)
        .bind(f.hostel_id)
        .bind(f.student_id)
        .bind(f.kind.as_deref().filter(|s| !s.is_empty()))
        .bind(f.open_only)
        .bind(f.from)
        .bind(f.to)
        .fetch_all(conn)
        .await?,
    )
}

pub async fn get_register(conn: &mut PgConnection, id: Uuid) -> DbResult<RegisterView> {
    sqlx::query_as::<_, RegisterView>(sqlx::AssertSqlSafe(format!(
        "SELECT {REG_COLS}, {REG_EXTRA} {REG_FROM} WHERE x.id = $1 AND x.deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("register entry".into()))
}

/// Record a resident going out (leave-out, consent checked against the hostel's rule) or a
/// visitor arriving. The hostel is the resident's current one.
pub async fn open_register(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    input: &RegisterInput,
    now: DateTime<Utc>,
) -> DbResult<RegisterView> {
    if !["leave_out", "visitor"].contains(&input.kind.as_str()) {
        return Err(DbError::Conflict("kind is leave_out or visitor".into()));
    }
    let current = list_allocations(
        &mut *conn,
        school_id,
        &AllocationFilter {
            student_id: Some(input.student_id),
            open_only: true,
            ..Default::default()
        },
    )
    .await?
    .into_iter()
    .next()
    .ok_or_else(|| DbError::Conflict("the student is not a hostel resident".into()))?;
    let hostel = get_hostel(&mut *conn, current.allocation.hostel_id).await?;
    if input.kind == "leave_out" {
        rules::leave_out_allowed(hostel.hostel.requires_consent, input.guardian_consent)
            .map_err(|m| DbError::Conflict(m.into()))?;
        let out: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM hostel_register WHERE student_id = $1 AND kind = 'leave_out' AND ended_at IS NULL AND deleted_at IS NULL)",
        )
        .bind(input.student_id)
        .fetch_one(&mut *conn)
        .await?;
        if out {
            return Err(DbError::Conflict(
                "the resident is already out; record the return first".into(),
            ));
        }
    } else if input.visitor_name.trim().is_empty() {
        return Err(DbError::Conflict("the visitor needs a name".into()));
    }
    let id = new_id();
    sqlx::query(
        "INSERT INTO hostel_register (id, school_id, hostel_id, student_id, kind, started_at, expected_back, visitor_name, visitor_relationship,
                visitor_phone, guardian_consent, consent_note, recorded_by, notes, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $13, $15)",
    )
    .bind(id)
    .bind(school_id)
    .bind(hostel.hostel.id)
    .bind(input.student_id)
    .bind(&input.kind)
    .bind(input.started_at.unwrap_or(now))
    .bind(input.expected_back)
    .bind(input.visitor_name.trim())
    .bind(input.visitor_relationship.trim())
    .bind(input.visitor_phone.trim())
    .bind(input.guardian_consent)
    .bind(input.consent_note.trim())
    .bind(actor.user_id)
    .bind(input.notes.trim())
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await?;
    let view = get_register(&mut *conn, id).await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "hostel_register",
        Some(id),
        None,
        Some(snapshot(&view.entry)),
    )
    .await?;
    Ok(view)
}

/// The resident is back (or the visitor has left).
pub async fn close_register(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    at: DateTime<Utc>,
    notes: Option<&str>,
) -> DbResult<RegisterView> {
    let old = get_register(&mut *conn, id).await?;
    if old.entry.ended_at.is_some() {
        return Err(DbError::Conflict("the entry is already closed".into()));
    }
    if at < old.entry.started_at {
        return Err(DbError::Conflict(
            "the return is before the departure".into(),
        ));
    }
    sqlx::query(
        "UPDATE hostel_register SET ended_at = $2, closed_by = $3, notes = $4, updated_at = now(), updated_by = $3, hlc = $5 WHERE id = $1",
    )
    .bind(id)
    .bind(at)
    .bind(actor.user_id)
    .bind(notes.map(str::trim).unwrap_or(&old.entry.notes))
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await?;
    let view = get_register(&mut *conn, id).await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "hostel_register",
        Some(id),
        Some(snapshot(&old.entry)),
        Some(snapshot(&view.entry)),
    )
    .await?;
    Ok(view)
}
