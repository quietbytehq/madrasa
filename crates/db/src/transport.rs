//! Transport (Standard stage 5): vehicles, routes with ordered stops, and the students
//! assigned to a stop per academic year. The monthly charge joins the fee voucher through
//! `monthly_charge` when the setting `transport.fee_head_id` names a fee head.

use chrono::{NaiveDate, NaiveTime};
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use madrasa_core::transport as rules;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::audit::{self, snapshot, Action};
use crate::{map_unique, settings, Actor, DbError, DbResult};

/// Fee head the monthly transport charge is posted under (empty = not charged).
pub const KEY_FEE_HEAD: &str = "transport.fee_head_id";

pub async fn fee_head(conn: &mut PgConnection, school_id: Uuid) -> DbResult<Option<Uuid>> {
    Ok(settings::get(conn, Some(school_id), KEY_FEE_HEAD)
        .await?
        .and_then(|v| v.as_str().and_then(|s| s.parse().ok())))
}

pub async fn set_fee_head(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    head: Option<Uuid>,
) -> DbResult<()> {
    settings::set(
        conn,
        hlc,
        actor,
        Some(school_id),
        KEY_FEE_HEAD,
        match head {
            Some(id) => json!(id.to_string()),
            None => Value::Null,
        },
    )
    .await?;
    Ok(())
}

// ---- vehicles -------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Vehicle {
    pub id: Uuid,
    pub school_id: Uuid,
    pub registration_no: String,
    pub make_model: String,
    pub capacity: Option<i32>,
    pub driver_name: String,
    pub driver_phone: String,
    pub helper_name: String,
    pub helper_phone: String,
    pub active: bool,
    pub notes: String,
    pub custom: Value,
    pub hlc: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct VehicleInput {
    pub registration_no: String,
    pub make_model: String,
    pub capacity: Option<i32>,
    pub driver_name: String,
    pub driver_phone: String,
    pub helper_name: String,
    pub helper_phone: String,
    pub active: bool,
    pub notes: String,
    pub custom: Value,
}

const VEHICLE_COLS: &str = "v.id, v.school_id, v.registration_no, v.make_model, v.capacity, v.driver_name, v.driver_phone, v.helper_name, v.helper_phone, v.active, v.notes, v.custom, v.hlc";

pub async fn list_vehicles(conn: &mut PgConnection, school_id: Uuid) -> DbResult<Vec<Vehicle>> {
    Ok(sqlx::query_as::<_, Vehicle>(sqlx::AssertSqlSafe(format!(
        "SELECT {VEHICLE_COLS} FROM transport_vehicles v WHERE v.school_id = $1 AND v.deleted_at IS NULL ORDER BY v.registration_no"
    )))
    .bind(school_id)
    .fetch_all(conn)
    .await?)
}

pub async fn get_vehicle(conn: &mut PgConnection, id: Uuid) -> DbResult<Vehicle> {
    sqlx::query_as::<_, Vehicle>(sqlx::AssertSqlSafe(format!(
        "SELECT {VEHICLE_COLS} FROM transport_vehicles v WHERE v.id = $1 AND v.deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("vehicle".into()))
}

fn check_vehicle(input: &VehicleInput) -> DbResult<()> {
    if input.registration_no.trim().is_empty() {
        return Err(DbError::Conflict(
            "the vehicle needs a registration number".into(),
        ));
    }
    if input.capacity.is_some_and(|c| c < 0) {
        return Err(DbError::Conflict("capacity cannot be negative".into()));
    }
    Ok(())
}

pub async fn create_vehicle(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    input: &VehicleInput,
) -> DbResult<Vehicle> {
    check_vehicle(input)?;
    let row = sqlx::query_as::<_, Vehicle>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO transport_vehicles AS v (id, school_id, registration_no, make_model, capacity, driver_name, driver_phone, helper_name, helper_phone, active, notes, custom, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14) RETURNING {VEHICLE_COLS}"
    )))
    .bind(new_id())
    .bind(school_id)
    .bind(input.registration_no.trim())
    .bind(input.make_model.trim())
    .bind(input.capacity)
    .bind(input.driver_name.trim())
    .bind(input.driver_phone.trim())
    .bind(input.helper_name.trim())
    .bind(input.helper_phone.trim())
    .bind(input.active)
    .bind(input.notes.trim())
    .bind(if input.custom.is_object() { input.custom.clone() } else { json!({}) })
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a vehicle with that registration number"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "transport_vehicle",
        Some(row.id),
        None,
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

pub async fn update_vehicle(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    input: &VehicleInput,
) -> DbResult<Vehicle> {
    check_vehicle(input)?;
    let old = get_vehicle(&mut *conn, id).await?;
    let row = sqlx::query_as::<_, Vehicle>(sqlx::AssertSqlSafe(format!(
        "UPDATE transport_vehicles AS v SET registration_no = $2, make_model = $3, capacity = $4, driver_name = $5, driver_phone = $6,
                helper_name = $7, helper_phone = $8, active = $9, notes = $10, custom = $11, updated_at = now(), updated_by = $12, hlc = $13
         WHERE v.id = $1 AND v.deleted_at IS NULL RETURNING {VEHICLE_COLS}"
    )))
    .bind(id)
    .bind(input.registration_no.trim())
    .bind(input.make_model.trim())
    .bind(input.capacity)
    .bind(input.driver_name.trim())
    .bind(input.driver_phone.trim())
    .bind(input.helper_name.trim())
    .bind(input.helper_phone.trim())
    .bind(input.active)
    .bind(input.notes.trim())
    .bind(if input.custom.is_object() { input.custom.clone() } else { json!({}) })
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a vehicle with that registration number"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "transport_vehicle",
        Some(id),
        Some(snapshot(&old)),
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

/// Remove a vehicle; refused while a route uses it.
pub async fn delete_vehicle(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
) -> DbResult<()> {
    let old = get_vehicle(&mut *conn, id).await?;
    let used: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM transport_routes WHERE vehicle_id = $1 AND deleted_at IS NULL)",
    )
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    if used {
        return Err(DbError::Conflict(
            "the vehicle is assigned to a route; move the route to another vehicle first".into(),
        ));
    }
    sqlx::query("UPDATE transport_vehicles SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1")
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
        "transport_vehicle",
        Some(id),
        Some(snapshot(&old)),
        None,
    )
    .await?;
    Ok(())
}

// ---- routes and stops -----------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Route {
    pub id: Uuid,
    pub school_id: Uuid,
    pub name: String,
    pub code: String,
    pub vehicle_id: Option<Uuid>,
    pub monthly_fee: Decimal,
    pub active: bool,
    pub notes: String,
    pub hlc: String,
}

/// A route with its vehicle and counts. Mirrored by `RouteView` in the UI.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RouteView {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub route: Route,
    pub vehicle_registration: Option<String>,
    pub vehicle_capacity: Option<i32>,
    pub driver_name: Option<String>,
    pub stops: i64,
    /// Students with an open assignment on the route (any year).
    pub students: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Stop {
    pub id: Uuid,
    pub school_id: Uuid,
    pub route_id: Uuid,
    pub name: String,
    pub position: i32,
    pub pickup_time: Option<NaiveTime>,
    pub drop_time: Option<NaiveTime>,
    pub monthly_fee: Option<Decimal>,
    pub hlc: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StopInput {
    /// An existing stop to keep (renamed or moved) or `None` for a new one.
    pub id: Option<Uuid>,
    pub name: String,
    pub pickup_time: Option<NaiveTime>,
    pub drop_time: Option<NaiveTime>,
    pub monthly_fee: Option<Decimal>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct RouteInput {
    pub name: String,
    #[serde(default)]
    pub code: String,
    pub vehicle_id: Option<Uuid>,
    #[serde(default)]
    pub monthly_fee: Decimal,
    #[serde(default = "yes")]
    pub active: bool,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub stops: Vec<StopInput>,
}

const ROUTE_COLS: &str =
    "r.id, r.school_id, r.name, r.code, r.vehicle_id, r.monthly_fee, r.active, r.notes, r.hlc";
const ROUTE_EXTRA: &str = "v.registration_no AS vehicle_registration, v.capacity AS vehicle_capacity, v.driver_name,
    (SELECT count(*) FROM transport_stops s WHERE s.route_id = r.id AND s.deleted_at IS NULL) AS stops,
    (SELECT count(*) FROM transport_assignments a WHERE a.route_id = r.id AND a.to_date IS NULL AND a.deleted_at IS NULL) AS students";
const STOP_COLS: &str = "s.id, s.school_id, s.route_id, s.name, s.position, s.pickup_time, s.drop_time, s.monthly_fee, s.hlc";

pub async fn list_routes(conn: &mut PgConnection, school_id: Uuid) -> DbResult<Vec<RouteView>> {
    Ok(sqlx::query_as::<_, RouteView>(sqlx::AssertSqlSafe(format!(
        "SELECT {ROUTE_COLS}, {ROUTE_EXTRA} FROM transport_routes r
         LEFT JOIN transport_vehicles v ON v.id = r.vehicle_id AND v.deleted_at IS NULL
         WHERE r.school_id = $1 AND r.deleted_at IS NULL ORDER BY r.name"
    )))
    .bind(school_id)
    .fetch_all(conn)
    .await?)
}

pub async fn get_route(conn: &mut PgConnection, id: Uuid) -> DbResult<RouteView> {
    sqlx::query_as::<_, RouteView>(sqlx::AssertSqlSafe(format!(
        "SELECT {ROUTE_COLS}, {ROUTE_EXTRA} FROM transport_routes r
         LEFT JOIN transport_vehicles v ON v.id = r.vehicle_id AND v.deleted_at IS NULL
         WHERE r.id = $1 AND r.deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("route".into()))
}

pub async fn stops_of(conn: &mut PgConnection, route_id: Uuid) -> DbResult<Vec<Stop>> {
    Ok(sqlx::query_as::<_, Stop>(sqlx::AssertSqlSafe(format!(
        "SELECT {STOP_COLS} FROM transport_stops s WHERE s.route_id = $1 AND s.deleted_at IS NULL ORDER BY s.position, s.name"
    )))
    .bind(route_id)
    .fetch_all(conn)
    .await?)
}

pub async fn get_stop(conn: &mut PgConnection, id: Uuid) -> DbResult<Stop> {
    sqlx::query_as::<_, Stop>(sqlx::AssertSqlSafe(format!(
        "SELECT {STOP_COLS} FROM transport_stops s WHERE s.id = $1 AND s.deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("stop".into()))
}

fn check_route(input: &RouteInput) -> DbResult<()> {
    if input.name.trim().is_empty() {
        return Err(DbError::Conflict("the route needs a name".into()));
    }
    if input.monthly_fee < Decimal::ZERO {
        return Err(DbError::Conflict(
            "the monthly fee cannot be negative".into(),
        ));
    }
    if input.stops.iter().any(|s| s.name.trim().is_empty()) {
        return Err(DbError::Conflict("every stop needs a name".into()));
    }
    if input
        .stops
        .iter()
        .any(|s| s.monthly_fee.is_some_and(|f| f < Decimal::ZERO))
    {
        return Err(DbError::Conflict(
            "a stop's monthly fee cannot be negative".into(),
        ));
    }
    Ok(())
}

async fn check_vehicle_of(
    conn: &mut PgConnection,
    school_id: Uuid,
    vehicle_id: Option<Uuid>,
) -> DbResult<()> {
    if let Some(v) = vehicle_id {
        let vehicle = get_vehicle(conn, v).await?;
        if vehicle.school_id != school_id {
            return Err(DbError::NotFound("vehicle".into()));
        }
    }
    Ok(())
}

pub async fn create_route(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    input: &RouteInput,
) -> DbResult<RouteView> {
    check_route(input)?;
    check_vehicle_of(&mut *conn, school_id, input.vehicle_id).await?;
    let id = new_id();
    sqlx::query(
        "INSERT INTO transport_routes (id, school_id, name, code, vehicle_id, monthly_fee, active, notes, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(id)
    .bind(school_id)
    .bind(input.name.trim())
    .bind(input.code.trim())
    .bind(input.vehicle_id)
    .bind(input.monthly_fee)
    .bind(input.active)
    .bind(input.notes.trim())
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a route with that name"))?;
    save_stops(&mut *conn, hlc, actor, school_id, id, &input.stops).await?;
    let view = get_route(&mut *conn, id).await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "transport_route",
        Some(id),
        None,
        Some(snapshot(&view.route)),
    )
    .await?;
    Ok(view)
}

pub async fn update_route(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    input: &RouteInput,
) -> DbResult<RouteView> {
    check_route(input)?;
    let old = get_route(&mut *conn, id).await?;
    check_vehicle_of(&mut *conn, old.route.school_id, input.vehicle_id).await?;
    sqlx::query(
        "UPDATE transport_routes SET name = $2, code = $3, vehicle_id = $4, monthly_fee = $5, active = $6, notes = $7,
                updated_at = now(), updated_by = $8, hlc = $9
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(input.name.trim())
    .bind(input.code.trim())
    .bind(input.vehicle_id)
    .bind(input.monthly_fee)
    .bind(input.active)
    .bind(input.notes.trim())
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a route with that name"))?;
    save_stops(
        &mut *conn,
        hlc,
        actor,
        old.route.school_id,
        id,
        &input.stops,
    )
    .await?;
    let view = get_route(&mut *conn, id).await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "transport_route",
        Some(id),
        Some(snapshot(&old.route)),
        Some(snapshot(&view.route)),
    )
    .await?;
    Ok(view)
}

/// Replace the stops of a route with the given list in order: existing ids are kept (and
/// renamed or moved), new ones added, missing ones removed unless a student is still
/// assigned to them.
async fn save_stops(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    route_id: Uuid,
    stops: &[StopInput],
) -> DbResult<()> {
    let existing = stops_of(&mut *conn, route_id).await?;
    let keep: Vec<Uuid> = stops.iter().filter_map(|s| s.id).collect();
    for e in &existing {
        if keep.contains(&e.id) {
            continue;
        }
        let used: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM transport_assignments WHERE stop_id = $1 AND to_date IS NULL AND deleted_at IS NULL)",
        )
        .bind(e.id)
        .fetch_one(&mut *conn)
        .await?;
        if used {
            return Err(DbError::Conflict(format!(
                "stop \"{}\" still has students assigned; move them first",
                e.name
            )));
        }
        sqlx::query("UPDATE transport_stops SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1")
            .bind(e.id)
            .bind(actor.user_id)
            .bind(hlc.now().to_string())
            .execute(&mut *conn)
            .await?;
    }
    for (i, s) in stops.iter().enumerate() {
        match s.id.filter(|id| existing.iter().any(|e| e.id == *id)) {
            Some(id) => {
                sqlx::query(
                    "UPDATE transport_stops SET name = $2, position = $3, pickup_time = $4, drop_time = $5, monthly_fee = $6,
                            updated_at = now(), updated_by = $7, hlc = $8 WHERE id = $1",
                )
                .bind(id)
                .bind(s.name.trim())
                .bind(i as i32)
                .bind(s.pickup_time)
                .bind(s.drop_time)
                .bind(s.monthly_fee)
                .bind(actor.user_id)
                .bind(hlc.now().to_string())
                .execute(&mut *conn)
                .await?;
            }
            None => {
                sqlx::query(
                    "INSERT INTO transport_stops (id, school_id, route_id, name, position, pickup_time, drop_time, monthly_fee, updated_by, hlc)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
                )
                .bind(new_id())
                .bind(school_id)
                .bind(route_id)
                .bind(s.name.trim())
                .bind(i as i32)
                .bind(s.pickup_time)
                .bind(s.drop_time)
                .bind(s.monthly_fee)
                .bind(actor.user_id)
                .bind(hlc.now().to_string())
                .execute(&mut *conn)
                .await?;
            }
        }
    }
    Ok(())
}

/// Remove a route and its stops; refused while a student is assigned to it.
pub async fn delete_route(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
) -> DbResult<()> {
    let old = get_route(&mut *conn, id).await?;
    if old.students > 0 {
        return Err(DbError::Conflict(
            "students are still assigned to this route; move them first".into(),
        ));
    }
    let now = hlc.now().to_string();
    sqlx::query("UPDATE transport_stops SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE route_id = $1 AND deleted_at IS NULL")
        .bind(id)
        .bind(actor.user_id)
        .bind(&now)
        .execute(&mut *conn)
        .await?;
    sqlx::query("UPDATE transport_routes SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1")
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
        "transport_route",
        Some(id),
        Some(snapshot(&old.route)),
        None,
    )
    .await?;
    Ok(())
}

// ---- assignments ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Assignment {
    pub id: Uuid,
    pub school_id: Uuid,
    pub student_id: Uuid,
    pub academic_year_id: Uuid,
    pub route_id: Uuid,
    pub stop_id: Uuid,
    pub from_date: NaiveDate,
    pub to_date: Option<NaiveDate>,
    pub amount: Option<Decimal>,
    pub notes: String,
    pub hlc: String,
}

/// An assignment with the student, route and stop named and the charge that applies.
/// Mirrored by `AssignmentView` in the UI.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AssignmentView {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub assignment: Assignment,
    pub student_number: String,
    pub student_name: String,
    pub class_name: String,
    pub section_name: String,
    pub section_id: Uuid,
    pub roll_no: Option<i32>,
    pub guardian_phone: String,
    pub route_name: String,
    pub stop_name: String,
    pub stop_position: i32,
    pub pickup_time: Option<NaiveTime>,
    /// Monthly charge after the assignment, stop and route defaults.
    pub charge: Decimal,
}

const ASSIGN_COLS: &str = "a.id, a.school_id, a.student_id, a.academic_year_id, a.route_id, a.stop_id, a.from_date, a.to_date, a.amount, a.notes, a.hlc";
const ASSIGN_EXTRA: &str = "st.student_number, st.full_name AS student_name,
    coalesce(c.name, '') AS class_name, coalesce(sec.name, '') AS section_name, e.section_id, e.roll_no,
    coalesce((SELECT g.phone FROM student_guardians sg JOIN guardians g ON g.id = sg.guardian_id
              WHERE sg.student_id = st.id AND sg.deleted_at IS NULL AND g.deleted_at IS NULL
              ORDER BY sg.is_primary DESC, sg.created_at LIMIT 1), '') AS guardian_phone,
    r.name AS route_name, s.name AS stop_name, s.position AS stop_position, s.pickup_time,
    coalesce(a.amount, s.monthly_fee, r.monthly_fee) AS charge";
const ASSIGN_FROM: &str = "FROM transport_assignments a
    JOIN students st ON st.id = a.student_id
    JOIN transport_routes r ON r.id = a.route_id
    JOIN transport_stops s ON s.id = a.stop_id
    JOIN enrollments e ON e.student_id = a.student_id AND e.academic_year_id = a.academic_year_id AND e.deleted_at IS NULL
    JOIN sections sec ON sec.id = e.section_id
    JOIN classes c ON c.id = sec.class_id";

#[derive(Debug, Clone, Default)]
pub struct AssignmentFilter {
    pub academic_year_id: Option<Uuid>,
    pub route_id: Option<Uuid>,
    pub stop_id: Option<Uuid>,
    pub section_id: Option<Uuid>,
    pub student_id: Option<Uuid>,
    /// Only assignments without a to date.
    pub open_only: bool,
}

pub async fn list_assignments(
    conn: &mut PgConnection,
    school_id: Uuid,
    f: &AssignmentFilter,
) -> DbResult<Vec<AssignmentView>> {
    Ok(
        sqlx::query_as::<_, AssignmentView>(sqlx::AssertSqlSafe(format!(
            "SELECT {ASSIGN_COLS}, {ASSIGN_EXTRA} {ASSIGN_FROM}
         WHERE a.school_id = $1 AND a.deleted_at IS NULL
           AND ($2::uuid IS NULL OR a.academic_year_id = $2)
           AND ($3::uuid IS NULL OR a.route_id = $3)
           AND ($4::uuid IS NULL OR a.stop_id = $4)
           AND ($5::uuid IS NULL OR e.section_id = $5)
           AND ($6::uuid IS NULL OR a.student_id = $6)
           AND (NOT $7 OR a.to_date IS NULL)
         ORDER BY r.name, s.position, c.level, c.name, sec.name, e.roll_no, lower(st.full_name)"
        )))
        .bind(school_id)
        .bind(f.academic_year_id)
        .bind(f.route_id)
        .bind(f.stop_id)
        .bind(f.section_id)
        .bind(f.student_id)
        .bind(f.open_only)
        .fetch_all(conn)
        .await?,
    )
}

pub async fn get_assignment(conn: &mut PgConnection, id: Uuid) -> DbResult<AssignmentView> {
    sqlx::query_as::<_, AssignmentView>(sqlx::AssertSqlSafe(format!(
        "SELECT {ASSIGN_COLS}, {ASSIGN_EXTRA} {ASSIGN_FROM} WHERE a.id = $1 AND a.deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("assignment".into()))
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssignInput {
    pub student_id: Uuid,
    pub academic_year_id: Uuid,
    pub stop_id: Uuid,
    pub from_date: NaiveDate,
    pub amount: Option<Decimal>,
    #[serde(default)]
    pub notes: String,
}

/// Put a student on a stop from a date. An open assignment of the same year ends the day
/// before (or is removed when it had not started yet); the stop's route is taken from the
/// stop. Returns the new assignment.
pub async fn assign(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    input: &AssignInput,
) -> DbResult<AssignmentView> {
    let stop = get_stop(&mut *conn, input.stop_id).await?;
    if stop.school_id != school_id {
        return Err(DbError::NotFound("stop".into()));
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
    let open = sqlx::query_as::<_, Assignment>(sqlx::AssertSqlSafe(format!(
        "SELECT {ASSIGN_COLS} FROM transport_assignments a
         WHERE a.student_id = $1 AND a.academic_year_id = $2 AND a.to_date IS NULL AND a.deleted_at IS NULL"
    )))
    .bind(input.student_id)
    .bind(input.academic_year_id)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(prev) = open {
        if prev.stop_id == input.stop_id && prev.amount == input.amount {
            return get_assignment(conn, prev.id).await;
        }
        let now = hlc.now().to_string();
        if prev.from_date >= input.from_date {
            sqlx::query("UPDATE transport_assignments SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1")
                .bind(prev.id)
                .bind(actor.user_id)
                .bind(&now)
                .execute(&mut *conn)
                .await?;
        } else {
            sqlx::query("UPDATE transport_assignments SET to_date = $2, updated_at = now(), updated_by = $3, hlc = $4 WHERE id = $1")
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
            "transport_assignment",
            Some(prev.id),
            Some(snapshot(&prev)),
            Some(json!({ "ended_for": input.stop_id })),
        )
        .await?;
    }
    let id = new_id();
    sqlx::query(
        "INSERT INTO transport_assignments (id, school_id, student_id, academic_year_id, route_id, stop_id, from_date, amount, notes, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(id)
    .bind(school_id)
    .bind(input.student_id)
    .bind(input.academic_year_id)
    .bind(stop.route_id)
    .bind(stop.id)
    .bind(input.from_date)
    .bind(input.amount)
    .bind(input.notes.trim())
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "an open transport assignment for the student"))?;
    let view = get_assignment(&mut *conn, id).await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "transport_assignment",
        Some(id),
        None,
        Some(snapshot(&view.assignment)),
    )
    .await?;
    Ok(view)
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssignmentPatch {
    pub stop_id: Option<Uuid>,
    /// `Some(None)` reopens; `Some(Some(d))` ends on `d`.
    pub to_date: Option<Option<NaiveDate>>,
    pub amount: Option<Option<Decimal>>,
    pub notes: Option<String>,
}

pub async fn update_assignment(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    patch: &AssignmentPatch,
) -> DbResult<AssignmentView> {
    let old = get_assignment(&mut *conn, id).await?;
    let a = &old.assignment;
    let (route_id, stop_id) = match patch.stop_id {
        Some(s) => {
            let stop = get_stop(&mut *conn, s).await?;
            if stop.school_id != a.school_id {
                return Err(DbError::NotFound("stop".into()));
            }
            (stop.route_id, stop.id)
        }
        None => (a.route_id, a.stop_id),
    };
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
        "UPDATE transport_assignments SET route_id = $2, stop_id = $3, to_date = $4, amount = $5, notes = $6,
                updated_at = now(), updated_by = $7, hlc = $8 WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(route_id)
    .bind(stop_id)
    .bind(to_date)
    .bind(amount)
    .bind(patch.notes.as_deref().map(str::trim).unwrap_or(&a.notes))
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "an open transport assignment for the student"))?;
    let view = get_assignment(&mut *conn, id).await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "transport_assignment",
        Some(id),
        Some(snapshot(&old.assignment)),
        Some(snapshot(&view.assignment)),
    )
    .await?;
    Ok(view)
}

pub async fn delete_assignment(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
) -> DbResult<()> {
    let old = get_assignment(&mut *conn, id).await?;
    sqlx::query("UPDATE transport_assignments SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1")
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
        "transport_assignment",
        Some(id),
        Some(snapshot(&old.assignment)),
        None,
    )
    .await?;
    Ok(())
}

// ---- students, rosters and fees -------------------------------------------------------------

/// A student of a section with the transport they have now, if any. Mirrored by
/// `TransportStudent` in the UI.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct StudentRow {
    pub student_id: Uuid,
    pub student_number: String,
    pub full_name: String,
    pub class_name: String,
    pub section_name: String,
    pub section_id: Uuid,
    pub roll_no: Option<i32>,
    pub assignment_id: Option<Uuid>,
    pub route_id: Option<Uuid>,
    pub route_name: Option<String>,
    pub stop_id: Option<Uuid>,
    pub stop_name: Option<String>,
    pub charge: Option<Decimal>,
}

/// Active students enrolled in the year (one section, or every section), each with their
/// open assignment. `without_only` keeps the ones without transport.
pub async fn students(
    conn: &mut PgConnection,
    school_id: Uuid,
    academic_year_id: Uuid,
    class_id: Option<Uuid>,
    section_id: Option<Uuid>,
    q: Option<&str>,
    without_only: bool,
) -> DbResult<Vec<StudentRow>> {
    Ok(sqlx::query_as::<_, StudentRow>(
        "SELECT st.id AS student_id, st.student_number, st.full_name, c.name AS class_name, sec.name AS section_name,
                e.section_id, e.roll_no, a.id AS assignment_id, a.route_id, r.name AS route_name, a.stop_id, s.name AS stop_name,
                coalesce(a.amount, s.monthly_fee, r.monthly_fee) AS charge
         FROM enrollments e
         JOIN students st ON st.id = e.student_id AND st.deleted_at IS NULL AND st.status = 'active'
         JOIN sections sec ON sec.id = e.section_id
         JOIN classes c ON c.id = sec.class_id
         LEFT JOIN transport_assignments a ON a.student_id = st.id AND a.academic_year_id = e.academic_year_id
              AND a.to_date IS NULL AND a.deleted_at IS NULL
         LEFT JOIN transport_routes r ON r.id = a.route_id
         LEFT JOIN transport_stops s ON s.id = a.stop_id
         WHERE st.school_id = $1 AND e.academic_year_id = $2 AND e.deleted_at IS NULL
           AND ($3::uuid IS NULL OR sec.class_id = $3)
           AND ($4::uuid IS NULL OR e.section_id = $4)
           AND ($5::text IS NULL OR st.full_name ILIKE '%' || $5 || '%' OR st.student_number ILIKE '%' || $5 || '%')
           AND (NOT $6 OR a.id IS NULL)
         ORDER BY c.level, c.name, sec.name, e.roll_no, lower(st.full_name)",
    )
    .bind(school_id)
    .bind(academic_year_id)
    .bind(class_id)
    .bind(section_id)
    .bind(q.map(str::trim).filter(|s| !s.is_empty()))
    .bind(without_only)
    .fetch_all(conn)
    .await?)
}

/// One stop of a roster with its students.
#[derive(Debug, Clone, Serialize)]
pub struct RosterStop {
    pub stop: Stop,
    pub students: Vec<AssignmentView>,
}

/// The students on a route in a year, by stop in order (empty stops included).
pub async fn roster(
    conn: &mut PgConnection,
    school_id: Uuid,
    route_id: Uuid,
    academic_year_id: Uuid,
) -> DbResult<Vec<RosterStop>> {
    let stops = stops_of(&mut *conn, route_id).await?;
    let rows = list_assignments(
        conn,
        school_id,
        &AssignmentFilter {
            academic_year_id: Some(academic_year_id),
            route_id: Some(route_id),
            open_only: true,
            ..Default::default()
        },
    )
    .await?;
    Ok(stops
        .into_iter()
        .map(|stop| RosterStop {
            students: rows
                .iter()
                .filter(|r| r.assignment.stop_id == stop.id)
                .cloned()
                .collect(),
            stop,
        })
        .collect())
}

/// The transport line of a student's voucher for a billing month: the fee head and the
/// amount, when the student has an assignment covering the month and a fee head is set.
pub async fn monthly_charge(
    conn: &mut PgConnection,
    school_id: Uuid,
    student_id: Uuid,
    billing_month: NaiveDate,
) -> DbResult<Option<(Uuid, Decimal)>> {
    let Some(head) = fee_head(&mut *conn, school_id).await? else {
        return Ok(None);
    };
    let (first, last) = rules::month_bounds(billing_month);
    // The assignment in force on the last day of the month that overlaps it, else the latest
    // one that overlaps it (a route change mid-month charges the new route).
    let row: Option<(Option<Decimal>, Option<Decimal>, Decimal)> = sqlx::query_as(
        "SELECT a.amount, s.monthly_fee, r.monthly_fee FROM transport_assignments a
         JOIN transport_stops s ON s.id = a.stop_id
         JOIN transport_routes r ON r.id = a.route_id
         WHERE a.student_id = $1 AND a.deleted_at IS NULL
           AND a.from_date <= $3 AND (a.to_date IS NULL OR a.to_date >= $2)
         ORDER BY a.from_date DESC LIMIT 1",
    )
    .bind(student_id)
    .bind(first)
    .bind(last)
    .fetch_optional(conn)
    .await?;
    Ok(row.and_then(|(a, s, r)| {
        let amount = rules::monthly_charge(a, s, r);
        (amount > Decimal::ZERO).then_some((head, amount))
    }))
}

/// Per route: students assigned in the month and the sum of their monthly charges.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RouteFeeRow {
    pub route_id: Uuid,
    pub route_name: String,
    pub vehicle_registration: Option<String>,
    pub students: i64,
    pub total: Decimal,
}

pub async fn fee_by_route(
    conn: &mut PgConnection,
    school_id: Uuid,
    academic_year_id: Uuid,
    billing_month: NaiveDate,
) -> DbResult<Vec<RouteFeeRow>> {
    let (first, last) = rules::month_bounds(billing_month);
    Ok(sqlx::query_as::<_, RouteFeeRow>(
        "SELECT r.id AS route_id, r.name AS route_name, v.registration_no AS vehicle_registration,
                count(a.id) AS students, coalesce(sum(coalesce(a.amount, s.monthly_fee, r.monthly_fee)), 0)::numeric(14,2) AS total
         FROM transport_routes r
         LEFT JOIN transport_vehicles v ON v.id = r.vehicle_id AND v.deleted_at IS NULL
         LEFT JOIN transport_assignments a ON a.route_id = r.id AND a.academic_year_id = $2 AND a.deleted_at IS NULL
              AND a.from_date <= $4 AND (a.to_date IS NULL OR a.to_date >= $3)
         LEFT JOIN transport_stops s ON s.id = a.stop_id
         WHERE r.school_id = $1 AND r.deleted_at IS NULL
         GROUP BY r.id, r.name, v.registration_no ORDER BY r.name",
    )
    .bind(school_id)
    .bind(academic_year_id)
    .bind(first)
    .bind(last)
    .fetch_all(conn)
    .await?)
}
