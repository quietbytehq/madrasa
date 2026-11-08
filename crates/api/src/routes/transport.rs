//! Transport (Standard stage 5): vehicles, routes with stops, students assigned to stops
//! per year, rosters and the fee head of the monthly charge. Every route needs the
//! `transport` feature and `transport.read`; changes need `transport.manage`.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::NaiveDate;
use madrasa_core::edition::Feature;
use madrasa_core::permissions::Permission;
use madrasa_core::transport::{capacity_warning, seats_left, CapacityWarning};
use madrasa_db::transport as repo;
use madrasa_db::transport::{
    AssignInput, AssignmentPatch, AssignmentView, RosterStop, RouteInput, RouteView, Stop,
    StudentRow, Vehicle, VehicleInput,
};
use madrasa_db::{academics, fees};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::{ApiError, ApiResult, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/transport/vehicles",
            get(list_vehicles).post(create_vehicle),
        )
        .route(
            "/transport/vehicles/{id}",
            axum::routing::put(update_vehicle).delete(delete_vehicle),
        )
        .route("/transport/routes", get(list_routes).post(create_route))
        .route(
            "/transport/routes/{id}",
            get(get_route).put(update_route).delete(delete_route),
        )
        .route("/transport/routes/{id}/roster", get(roster))
        .route("/transport/students", get(students))
        .route("/transport/assignments", get(list_assignments).post(assign))
        .route("/transport/assignments/bulk", post(assign_bulk))
        .route(
            "/transport/assignments/{id}",
            axum::routing::put(update_assignment).delete(delete_assignment),
        )
        .route("/transport/settings", get(get_settings).put(put_settings))
}

fn gate(state: &AppState, user: &CurrentUser, manage: bool) -> ApiResult<()> {
    state.require_feature(Feature::Transport)?;
    user.require(if manage {
        Permission::TransportManage
    } else {
        Permission::TransportRead
    })
}

// ---- vehicles -------------------------------------------------------------------------------

async fn list_vehicles(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<Vehicle>>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        repo::list_vehicles(&mut c, user.user.school_id).await?,
    ))
}

async fn create_vehicle(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<VehicleInput>,
) -> ApiResult<(StatusCode, Json<Vehicle>)> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    let v = repo::create_vehicle(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        &body,
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(v)))
}

async fn owned_vehicle(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    id: Uuid,
) -> ApiResult<Vehicle> {
    let v = repo::get_vehicle(c, id).await?;
    if v.school_id != user.user.school_id {
        return Err(ApiError::not_found("vehicle"));
    }
    Ok(v)
}

async fn update_vehicle(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<VehicleInput>,
) -> ApiResult<Json<Vehicle>> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    owned_vehicle(&mut tx, &user, id).await?;
    let v = repo::update_vehicle(&mut tx, &state.hlc, user.actor(), id, &body).await?;
    tx.commit().await?;
    Ok(Json(v))
}

async fn delete_vehicle(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    owned_vehicle(&mut tx, &user, id).await?;
    repo::delete_vehicle(&mut tx, &state.hlc, user.actor(), id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- routes ---------------------------------------------------------------------------------

/// A route with its stops. Mirrored by `RouteDetail` in the UI.
#[derive(Debug, Serialize)]
pub struct RouteDetail {
    #[serde(flatten)]
    pub route: RouteView,
    pub stop_list: Vec<Stop>,
}

async fn list_routes(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<RouteView>>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(repo::list_routes(&mut c, user.user.school_id).await?))
}

async fn owned_route(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    id: Uuid,
) -> ApiResult<RouteView> {
    let r = repo::get_route(c, id).await?;
    if r.route.school_id != user.user.school_id {
        return Err(ApiError::not_found("route"));
    }
    Ok(r)
}

async fn detail(c: &mut sqlx::PgConnection, route: RouteView) -> ApiResult<RouteDetail> {
    let stop_list = repo::stops_of(c, route.route.id).await?;
    Ok(RouteDetail { route, stop_list })
}

async fn create_route(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<RouteInput>,
) -> ApiResult<(StatusCode, Json<RouteDetail>)> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    let r = repo::create_route(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        &body,
    )
    .await?;
    let d = detail(&mut tx, r).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(d)))
}

async fn get_route(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<RouteDetail>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    let r = owned_route(&mut c, &user, id).await?;
    Ok(Json(detail(&mut c, r).await?))
}

async fn update_route(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RouteInput>,
) -> ApiResult<Json<RouteDetail>> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    owned_route(&mut tx, &user, id).await?;
    let r = repo::update_route(&mut tx, &state.hlc, user.actor(), id, &body).await?;
    let d = detail(&mut tx, r).await?;
    tx.commit().await?;
    Ok(Json(d))
}

async fn delete_route(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    owned_route(&mut tx, &user, id).await?;
    repo::delete_route(&mut tx, &state.hlc, user.actor(), id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- roster ---------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct YearQuery {
    pub academic_year_id: Option<Uuid>,
}

async fn year_or_active(
    c: &mut sqlx::PgConnection,
    school_id: Uuid,
    year: Option<Uuid>,
) -> ApiResult<Uuid> {
    match year {
        Some(y) => Ok(y),
        None => academics::active_year(c, school_id)
            .await?
            .map(|y| y.id)
            .ok_or_else(|| ApiError::validation("No active academic year.")),
    }
}

/// Mirrored by `RouteRoster` in the UI.
#[derive(Debug, Serialize)]
pub struct Roster {
    #[serde(flatten)]
    pub route: RouteView,
    pub academic_year_id: Uuid,
    /// Stops in order with their students (`stops` on the flattened route is the count).
    pub by_stop: Vec<RosterStop>,
    pub total: usize,
    pub seats_left: Option<i64>,
    pub warning: Option<CapacityWarning>,
}

async fn roster(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Query(q): Query<YearQuery>,
) -> ApiResult<Json<Roster>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    let school = user.user.school_id;
    let route = owned_route(&mut c, &user, id).await?;
    let year = year_or_active(&mut c, school, q.academic_year_id).await?;
    let stops = repo::roster(&mut c, school, id, year).await?;
    let total: usize = stops.iter().map(|s| s.students.len()).sum();
    Ok(Json(Roster {
        seats_left: seats_left(route.vehicle_capacity, total as i64),
        warning: capacity_warning(route.vehicle_capacity, total as i64),
        route,
        academic_year_id: year,
        by_stop: stops,
        total,
    }))
}

// ---- students and assignments ---------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct StudentsQuery {
    pub academic_year_id: Option<Uuid>,
    pub class_id: Option<Uuid>,
    pub section_id: Option<Uuid>,
    pub q: Option<String>,
    #[serde(default)]
    pub without: bool,
}

async fn students(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<StudentsQuery>,
) -> ApiResult<Json<Vec<StudentRow>>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    let school = user.user.school_id;
    let year = year_or_active(&mut c, school, q.academic_year_id).await?;
    Ok(Json(
        repo::students(
            &mut c,
            school,
            year,
            q.class_id,
            q.section_id,
            q.q.as_deref(),
            q.without,
        )
        .await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct AssignmentQuery {
    pub academic_year_id: Option<Uuid>,
    pub route_id: Option<Uuid>,
    pub stop_id: Option<Uuid>,
    pub section_id: Option<Uuid>,
    pub student_id: Option<Uuid>,
    #[serde(default)]
    pub open: bool,
}

async fn list_assignments(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<AssignmentQuery>,
) -> ApiResult<Json<Vec<AssignmentView>>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        repo::list_assignments(
            &mut c,
            user.user.school_id,
            &repo::AssignmentFilter {
                academic_year_id: q.academic_year_id,
                route_id: q.route_id,
                stop_id: q.stop_id,
                section_id: q.section_id,
                student_id: q.student_id,
                open_only: q.open,
            },
        )
        .await?,
    ))
}

/// Mirrored by `AssignResult` in the UI: the assignment and how full the route is now.
#[derive(Debug, Serialize)]
pub struct AssignResult {
    pub assignment: AssignmentView,
    pub route_students: i64,
    pub seats_left: Option<i64>,
    pub warning: Option<CapacityWarning>,
}

async fn load_capacity(
    c: &mut sqlx::PgConnection,
    school: Uuid,
    route_id: Uuid,
    year: Uuid,
) -> ApiResult<(i64, Option<i64>, Option<CapacityWarning>)> {
    let route = repo::get_route(c, route_id).await?;
    let n = repo::list_assignments(
        c,
        school,
        &repo::AssignmentFilter {
            academic_year_id: Some(year),
            route_id: Some(route_id),
            open_only: true,
            ..Default::default()
        },
    )
    .await?
    .len() as i64;
    Ok((
        n,
        seats_left(route.vehicle_capacity, n),
        capacity_warning(route.vehicle_capacity, n),
    ))
}

async fn assign(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<AssignInput>,
) -> ApiResult<(StatusCode, Json<AssignResult>)> {
    gate(&state, &user, true)?;
    let school = user.user.school_id;
    let mut tx = state.pool.begin().await?;
    let a = repo::assign(&mut tx, &state.hlc, user.actor(), school, &body).await?;
    let (route_students, seats, warning) = load_capacity(
        &mut tx,
        school,
        a.assignment.route_id,
        a.assignment.academic_year_id,
    )
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(AssignResult {
            assignment: a,
            route_students,
            seats_left: seats,
            warning,
        }),
    ))
}

#[derive(Debug, Deserialize)]
pub struct BulkAssignBody {
    pub student_ids: Vec<Uuid>,
    pub academic_year_id: Uuid,
    pub stop_id: Uuid,
    pub from_date: NaiveDate,
    pub amount: Option<Decimal>,
}

/// Mirrored by `BulkAssignResult` in the UI.
#[derive(Debug, Serialize)]
pub struct BulkAssignResult {
    pub assigned: usize,
    /// `[student_id, reason]` for the ones refused.
    pub refused: Vec<(Uuid, String)>,
    pub route_students: i64,
    pub seats_left: Option<i64>,
    pub warning: Option<CapacityWarning>,
}

async fn assign_bulk(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<BulkAssignBody>,
) -> ApiResult<Json<BulkAssignResult>> {
    gate(&state, &user, true)?;
    let school = user.user.school_id;
    let mut c = state.pool.acquire().await?;
    let stop = repo::get_stop(&mut c, body.stop_id).await?;
    if stop.school_id != school {
        return Err(ApiError::not_found("stop"));
    }
    let mut assigned = 0;
    let mut refused = Vec::new();
    for sid in body.student_ids {
        let mut tx = state.pool.begin().await?;
        let r = repo::assign(
            &mut tx,
            &state.hlc,
            user.actor(),
            school,
            &AssignInput {
                student_id: sid,
                academic_year_id: body.academic_year_id,
                stop_id: body.stop_id,
                from_date: body.from_date,
                amount: body.amount,
                notes: String::new(),
            },
        )
        .await;
        match r {
            Ok(_) => {
                tx.commit().await?;
                assigned += 1;
            }
            Err(e) => {
                tx.rollback().await?;
                refused.push((sid, e.to_string()));
            }
        }
    }
    let (route_students, seats, warning) =
        load_capacity(&mut c, school, stop.route_id, body.academic_year_id).await?;
    Ok(Json(BulkAssignResult {
        assigned,
        refused,
        route_students,
        seats_left: seats,
        warning,
    }))
}

async fn owned_assignment(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    id: Uuid,
) -> ApiResult<AssignmentView> {
    let a = repo::get_assignment(c, id).await?;
    if a.assignment.school_id != user.user.school_id {
        return Err(ApiError::not_found("assignment"));
    }
    Ok(a)
}

async fn update_assignment(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AssignmentPatch>,
) -> ApiResult<Json<AssignmentView>> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    owned_assignment(&mut tx, &user, id).await?;
    let a = repo::update_assignment(&mut tx, &state.hlc, user.actor(), id, &body).await?;
    tx.commit().await?;
    Ok(Json(a))
}

async fn delete_assignment(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    owned_assignment(&mut tx, &user, id).await?;
    repo::delete_assignment(&mut tx, &state.hlc, user.actor(), id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- settings -------------------------------------------------------------------------------

/// Mirrored by `TransportSettings` in the UI.
#[derive(Debug, Serialize, Deserialize)]
pub struct TransportSettings {
    pub fee_head_id: Option<Uuid>,
    #[serde(skip_deserializing)]
    pub fee_head_name: Option<String>,
}

async fn settings_view(c: &mut sqlx::PgConnection, school: Uuid) -> ApiResult<TransportSettings> {
    let fee_head_id = repo::fee_head(c, school).await?;
    let fee_head_name = match fee_head_id {
        Some(id) => fees::get_head(c, id).await.ok().map(|h| h.name),
        None => None,
    };
    Ok(TransportSettings {
        fee_head_id,
        fee_head_name,
    })
}

async fn get_settings(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<TransportSettings>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(settings_view(&mut c, user.user.school_id).await?))
}

async fn put_settings(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<TransportSettings>,
) -> ApiResult<Json<TransportSettings>> {
    gate(&state, &user, true)?;
    let school = user.user.school_id;
    let mut tx = state.pool.begin().await?;
    if let Some(id) = body.fee_head_id {
        let head = fees::get_head(&mut tx, id).await?;
        if head.school_id != school {
            return Err(ApiError::not_found("fee head"));
        }
    }
    repo::set_fee_head(&mut tx, &state.hlc, user.actor(), school, body.fee_head_id).await?;
    let view = settings_view(&mut tx, school).await?;
    tx.commit().await?;
    Ok(Json(view))
}
