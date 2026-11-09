//! Hostel (Standard stage 6): hostels with rooms and beds, the occupancy board, residents
//! allocated per year, and the leave-out / visitor register. Every route needs the `hostel`
//! feature and `hostel.read`; changes need `hostel.manage`.

use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use madrasa_core::edition::Feature;
use madrasa_core::hostel::{occupancy, Occupancy};
use madrasa_core::permissions::Permission;
use madrasa_db::hostel as repo;
use madrasa_db::hostel::{
    AllocateInput, AllocationPatch, AllocationView, BoardRoom, HostelInput, HostelView,
    RegisterInput, RegisterView, RoomInput, RoomView,
};
use madrasa_db::{academics, fees};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::{ApiError, ApiResult, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/hostel/hostels", get(list_hostels).post(create_hostel))
        .route(
            "/hostel/hostels/{id}",
            get(get_hostel).put(update_hostel).delete(delete_hostel),
        )
        .route("/hostel/hostels/{id}/rooms", post(create_room))
        .route("/hostel/rooms/{id}", put(update_room).delete(delete_room))
        .route("/hostel/board", get(board))
        .route("/hostel/residents", get(residents))
        .route("/hostel/allocations", get(list_allocations).post(allocate))
        .route(
            "/hostel/allocations/{id}",
            put(update_allocation).delete(delete_allocation),
        )
        .route("/hostel/register", get(list_register).post(open_register))
        .route("/hostel/register/{id}/close", post(close_register))
}

fn gate(state: &AppState, user: &CurrentUser, manage: bool) -> ApiResult<()> {
    state.require_feature(Feature::Hostel)?;
    user.require(if manage {
        Permission::HostelManage
    } else {
        Permission::HostelRead
    })
}

fn now() -> DateTime<Utc> {
    Utc::now()
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

// ---- hostels and rooms ----------------------------------------------------------------------

/// A hostel with its rooms. Mirrored by `HostelDetail` in the UI.
#[derive(Debug, Serialize)]
pub struct HostelDetail {
    #[serde(flatten)]
    pub hostel: HostelView,
    pub room_list: Vec<RoomView>,
}

async fn list_hostels(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<HostelView>>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(repo::list_hostels(&mut c, user.user.school_id).await?))
}

async fn owned_hostel(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    id: Uuid,
) -> ApiResult<HostelView> {
    let h = repo::get_hostel(c, id).await?;
    if h.hostel.school_id != user.user.school_id {
        return Err(ApiError::not_found("hostel"));
    }
    Ok(h)
}

async fn check_fee_head(
    c: &mut sqlx::PgConnection,
    school: Uuid,
    head: Option<Uuid>,
) -> ApiResult<()> {
    if let Some(id) = head {
        let h = fees::get_head(c, id).await?;
        if h.school_id != school {
            return Err(ApiError::not_found("fee head"));
        }
    }
    Ok(())
}

async fn detail(c: &mut sqlx::PgConnection, hostel: HostelView) -> ApiResult<HostelDetail> {
    let room_list = repo::list_rooms(c, hostel.hostel.id).await?;
    Ok(HostelDetail { hostel, room_list })
}

async fn create_hostel(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<HostelInput>,
) -> ApiResult<(StatusCode, Json<HostelDetail>)> {
    gate(&state, &user, true)?;
    let school = user.user.school_id;
    let mut tx = state.pool.begin().await?;
    check_fee_head(&mut tx, school, body.fee_head_id).await?;
    let h = repo::create_hostel(&mut tx, &state.hlc, user.actor(), school, &body).await?;
    let d = detail(&mut tx, h).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(d)))
}

async fn get_hostel(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<HostelDetail>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    let h = owned_hostel(&mut c, &user, id).await?;
    Ok(Json(detail(&mut c, h).await?))
}

async fn update_hostel(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<HostelInput>,
) -> ApiResult<Json<HostelDetail>> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    owned_hostel(&mut tx, &user, id).await?;
    check_fee_head(&mut tx, user.user.school_id, body.fee_head_id).await?;
    let h = repo::update_hostel(&mut tx, &state.hlc, user.actor(), id, &body).await?;
    let d = detail(&mut tx, h).await?;
    tx.commit().await?;
    Ok(Json(d))
}

async fn delete_hostel(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    owned_hostel(&mut tx, &user, id).await?;
    repo::delete_hostel(&mut tx, &state.hlc, user.actor(), id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn create_room(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RoomInput>,
) -> ApiResult<(StatusCode, Json<RoomView>)> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    owned_hostel(&mut tx, &user, id).await?;
    let r = repo::create_room(&mut tx, &state.hlc, user.actor(), id, &body).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(r)))
}

async fn owned_room(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    id: Uuid,
) -> ApiResult<RoomView> {
    let r = repo::get_room(c, id).await?;
    if r.room.school_id != user.user.school_id {
        return Err(ApiError::not_found("room"));
    }
    Ok(r)
}

async fn update_room(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<RoomInput>,
) -> ApiResult<Json<RoomView>> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    owned_room(&mut tx, &user, id).await?;
    let r = repo::update_room(&mut tx, &state.hlc, user.actor(), id, &body).await?;
    tx.commit().await?;
    Ok(Json(r))
}

async fn delete_room(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    owned_room(&mut tx, &user, id).await?;
    repo::delete_room(&mut tx, &state.hlc, user.actor(), id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- board and residents --------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct BoardQuery {
    pub hostel_id: Uuid,
    pub academic_year_id: Option<Uuid>,
}

/// Mirrored by `HostelBoard` in the UI.
#[derive(Debug, Serialize)]
pub struct Board {
    #[serde(flatten)]
    pub hostel: HostelView,
    pub academic_year_id: Uuid,
    /// Rooms in order with their beds (`rooms` on the flattened hostel is the count).
    pub by_room: Vec<BoardRoom>,
    pub occupancy: Occupancy,
}

async fn board(
    State(state): State<AppState>,
    user: CurrentUser,
    // The edition check runs before the query is parsed (see `gate`).
    q: Result<Query<BoardQuery>, QueryRejection>,
) -> ApiResult<Json<Board>> {
    gate(&state, &user, false)?;
    let Query(q) = q?;
    let mut c = state.pool.acquire().await?;
    let school = user.user.school_id;
    let hostel = owned_hostel(&mut c, &user, q.hostel_id).await?;
    let year = year_or_active(&mut c, school, q.academic_year_id).await?;
    let rooms = repo::board(&mut c, school, q.hostel_id, year).await?;
    let beds: i64 = rooms
        .iter()
        .filter(|r| r.room.room.active)
        .map(|r| i64::from(r.room.room.beds))
        .sum();
    let occupied: i64 = rooms
        .iter()
        .map(|r| r.beds.iter().filter(|b| b.resident.is_some()).count() as i64)
        .sum();
    Ok(Json(Board {
        hostel,
        academic_year_id: year,
        by_room: rooms,
        occupancy: occupancy(beds, occupied),
    }))
}

#[derive(Debug, Deserialize)]
pub struct ResidentsQuery {
    pub academic_year_id: Option<Uuid>,
    pub hostel_id: Option<Uuid>,
    pub class_id: Option<Uuid>,
    pub q: Option<String>,
}

/// Residents of the year (open allocations).
async fn residents(
    State(state): State<AppState>,
    user: CurrentUser,
    // The edition check runs before the query is parsed (see `gate`).
    q: Result<Query<ResidentsQuery>, QueryRejection>,
) -> ApiResult<Json<Vec<AllocationView>>> {
    gate(&state, &user, false)?;
    let Query(q) = q?;
    let mut c = state.pool.acquire().await?;
    let school = user.user.school_id;
    let year = year_or_active(&mut c, school, q.academic_year_id).await?;
    Ok(Json(
        repo::list_allocations(
            &mut c,
            school,
            &repo::AllocationFilter {
                academic_year_id: Some(year),
                hostel_id: q.hostel_id,
                class_id: q.class_id,
                q: q.q,
                open_only: true,
                ..Default::default()
            },
        )
        .await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct AllocationQuery {
    pub academic_year_id: Option<Uuid>,
    pub hostel_id: Option<Uuid>,
    pub room_id: Option<Uuid>,
    pub student_id: Option<Uuid>,
    #[serde(default)]
    pub open: bool,
}

async fn list_allocations(
    State(state): State<AppState>,
    user: CurrentUser,
    // The edition check runs before the query is parsed (see `gate`).
    q: Result<Query<AllocationQuery>, QueryRejection>,
) -> ApiResult<Json<Vec<AllocationView>>> {
    gate(&state, &user, false)?;
    let Query(q) = q?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        repo::list_allocations(
            &mut c,
            user.user.school_id,
            &repo::AllocationFilter {
                academic_year_id: q.academic_year_id,
                hostel_id: q.hostel_id,
                room_id: q.room_id,
                student_id: q.student_id,
                open_only: q.open,
                ..Default::default()
            },
        )
        .await?,
    ))
}

async fn allocate(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<AllocateInput>,
) -> ApiResult<(StatusCode, Json<AllocationView>)> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    let a = repo::allocate(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        &body,
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(a)))
}

async fn owned_allocation(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    id: Uuid,
) -> ApiResult<AllocationView> {
    let a = repo::get_allocation(c, id).await?;
    if a.allocation.school_id != user.user.school_id {
        return Err(ApiError::not_found("allocation"));
    }
    Ok(a)
}

async fn update_allocation(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AllocationPatch>,
) -> ApiResult<Json<AllocationView>> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    owned_allocation(&mut tx, &user, id).await?;
    let a = repo::update_allocation(&mut tx, &state.hlc, user.actor(), id, &body).await?;
    tx.commit().await?;
    Ok(Json(a))
}

async fn delete_allocation(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    owned_allocation(&mut tx, &user, id).await?;
    repo::delete_allocation(&mut tx, &state.hlc, user.actor(), id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- register -------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct RegisterQuery {
    pub hostel_id: Option<Uuid>,
    pub student_id: Option<Uuid>,
    pub kind: Option<String>,
    #[serde(default)]
    pub open: bool,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

async fn list_register(
    State(state): State<AppState>,
    user: CurrentUser,
    // The edition check runs before the query is parsed (see `gate`).
    q: Result<Query<RegisterQuery>, QueryRejection>,
) -> ApiResult<Json<Vec<RegisterView>>> {
    gate(&state, &user, false)?;
    let Query(q) = q?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        repo::list_register(
            &mut c,
            user.user.school_id,
            &repo::RegisterFilter {
                hostel_id: q.hostel_id,
                student_id: q.student_id,
                kind: q.kind,
                open_only: q.open,
                from: q.from,
                to: q.to,
            },
        )
        .await?,
    ))
}

async fn open_register(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<RegisterInput>,
) -> ApiResult<(StatusCode, Json<RegisterView>)> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    let e = repo::open_register(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        &body,
        now(),
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(e)))
}

#[derive(Debug, Default, Deserialize)]
pub struct CloseBody {
    pub at: Option<DateTime<Utc>>,
    pub notes: Option<String>,
}

async fn close_register(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: Option<Json<CloseBody>>,
) -> ApiResult<Json<RegisterView>> {
    gate(&state, &user, true)?;
    let body = body.map(|b| b.0).unwrap_or_default();
    let mut tx = state.pool.begin().await?;
    let e = repo::get_register(&mut tx, id).await?;
    if e.entry.school_id != user.user.school_id {
        return Err(ApiError::not_found("register entry"));
    }
    let e = repo::close_register(
        &mut tx,
        &state.hlc,
        user.actor(),
        id,
        body.at.unwrap_or_else(now),
        body.notes.as_deref(),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(e))
}
