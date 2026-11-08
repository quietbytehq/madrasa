//! Inventory (Standard stage 7): categories, locations, items with stock on hand, movements
//! (purchases in, issues to staff, students or rooms, returns, write-offs, adjustments,
//! transfers) and asset tags. Every route needs the `inventory` feature and
//! `inventory.read`; changes need `inventory.manage`.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::NaiveDate;
use madrasa_core::edition::Feature;
use madrasa_core::inventory::RecipientKind;
use madrasa_core::permissions::Permission;
use madrasa_db::inventory as repo;
use madrasa_db::inventory::{
    Asset, AssetInput, AssetView, Category, CategoryInput, ItemInput, ItemView, Location,
    LocationInput, MovementInput, MovementView, StockRow,
};
use madrasa_db::{staff as staff_repo, students, timetable};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::{ApiError, ApiResult, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/inventory/categories",
            get(list_categories).post(create_category),
        )
        .route(
            "/inventory/categories/{id}",
            put(update_category).delete(delete_category),
        )
        .route(
            "/inventory/locations",
            get(list_locations).post(create_location),
        )
        .route(
            "/inventory/locations/{id}",
            put(update_location).delete(delete_location),
        )
        .route("/inventory/items", get(list_items).post(create_item))
        .route(
            "/inventory/items/{id}",
            get(get_item).put(update_item).delete(delete_item),
        )
        .route("/inventory/stock", get(stock))
        .route(
            "/inventory/movements",
            get(list_movements).post(record_movement),
        )
        .route(
            "/inventory/movements/{id}",
            axum::routing::delete(delete_movement),
        )
        .route("/inventory/transfers", post(transfer))
        .route("/inventory/recipients", get(recipients))
        .route("/inventory/assets", get(list_assets).post(create_asset))
        .route(
            "/inventory/assets/{id}",
            put(update_asset).delete(delete_asset),
        )
}

fn gate(state: &AppState, user: &CurrentUser, manage: bool) -> ApiResult<()> {
    state.require_feature(Feature::Inventory)?;
    user.require(if manage {
        Permission::InventoryManage
    } else {
        Permission::InventoryRead
    })
}

fn today() -> NaiveDate {
    crate::clock::today()
}

// ---- categories and locations ---------------------------------------------------------------

async fn list_categories(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<Category>>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        repo::list_categories(&mut c, user.user.school_id).await?,
    ))
}

async fn create_category(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<CategoryInput>,
) -> ApiResult<(StatusCode, Json<Category>)> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    let c = repo::save_category(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        None,
        &body,
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(c)))
}

async fn update_category(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<CategoryInput>,
) -> ApiResult<Json<Category>> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    if repo::get_category(&mut tx, id).await?.school_id != user.user.school_id {
        return Err(ApiError::not_found("category"));
    }
    let c = repo::save_category(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        Some(id),
        &body,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(c))
}

async fn delete_category(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    if repo::get_category(&mut tx, id).await?.school_id != user.user.school_id {
        return Err(ApiError::not_found("category"));
    }
    repo::delete_category(&mut tx, &state.hlc, user.actor(), id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_locations(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<Location>>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        repo::list_locations(&mut c, user.user.school_id).await?,
    ))
}

async fn create_location(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<LocationInput>,
) -> ApiResult<(StatusCode, Json<Location>)> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    let l = repo::save_location(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        None,
        &body,
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(l)))
}

async fn update_location(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<LocationInput>,
) -> ApiResult<Json<Location>> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    if repo::get_location(&mut tx, id).await?.school_id != user.user.school_id {
        return Err(ApiError::not_found("location"));
    }
    let l = repo::save_location(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        Some(id),
        &body,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(l))
}

async fn delete_location(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    if repo::get_location(&mut tx, id).await?.school_id != user.user.school_id {
        return Err(ApiError::not_found("location"));
    }
    repo::delete_location(&mut tx, &state.hlc, user.actor(), id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- items ----------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ItemQuery {
    pub q: Option<String>,
    pub category_id: Option<Uuid>,
    #[serde(default)]
    pub below_reorder: bool,
    #[serde(default)]
    pub active: bool,
}

async fn list_items(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<ItemQuery>,
) -> ApiResult<Json<Vec<ItemView>>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        repo::list_items(
            &mut c,
            user.user.school_id,
            &repo::ItemFilter {
                q: q.q,
                category_id: q.category_id,
                below_reorder: q.below_reorder,
                active_only: q.active,
            },
        )
        .await?,
    ))
}

/// An item with its stock by location, recent movements and assets. Mirrored by
/// `ItemDetail` in the UI.
#[derive(Debug, Serialize)]
pub struct ItemDetail {
    #[serde(flatten)]
    pub item: ItemView,
    pub by_location: Vec<StockRow>,
    pub recent: Vec<MovementView>,
    pub asset_list: Vec<AssetView>,
}

async fn owned_item(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    id: Uuid,
) -> ApiResult<ItemView> {
    let i = repo::get_item(c, id).await?;
    if i.item.school_id != user.user.school_id {
        return Err(ApiError::not_found("item"));
    }
    Ok(i)
}

async fn detail(c: &mut sqlx::PgConnection, school: Uuid, item: ItemView) -> ApiResult<ItemDetail> {
    let id = item.item.id;
    let by_location = repo::stock(c, school, Some(id), None).await?;
    let recent = repo::list_movements(
        c,
        school,
        &repo::MovementFilter {
            item_id: Some(id),
            ..Default::default()
        },
        25,
    )
    .await?;
    let asset_list = repo::list_assets(
        c,
        school,
        &repo::AssetFilter {
            item_id: Some(id),
            ..Default::default()
        },
    )
    .await?;
    Ok(ItemDetail {
        item,
        by_location,
        recent,
        asset_list,
    })
}

async fn create_item(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<ItemInput>,
) -> ApiResult<(StatusCode, Json<ItemDetail>)> {
    gate(&state, &user, true)?;
    let school = user.user.school_id;
    let mut tx = state.pool.begin().await?;
    let i = repo::save_item(&mut tx, &state.hlc, user.actor(), school, None, &body).await?;
    let d = detail(&mut tx, school, i).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(d)))
}

async fn get_item(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ItemDetail>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    let i = owned_item(&mut c, &user, id).await?;
    Ok(Json(detail(&mut c, user.user.school_id, i).await?))
}

async fn update_item(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<ItemInput>,
) -> ApiResult<Json<ItemDetail>> {
    gate(&state, &user, true)?;
    let school = user.user.school_id;
    let mut tx = state.pool.begin().await?;
    owned_item(&mut tx, &user, id).await?;
    let i = repo::save_item(&mut tx, &state.hlc, user.actor(), school, Some(id), &body).await?;
    let d = detail(&mut tx, school, i).await?;
    tx.commit().await?;
    Ok(Json(d))
}

async fn delete_item(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    owned_item(&mut tx, &user, id).await?;
    repo::delete_item(&mut tx, &state.hlc, user.actor(), id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- stock and movements --------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct StockQuery {
    pub item_id: Option<Uuid>,
    pub location_id: Option<Uuid>,
}

async fn stock(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<StockQuery>,
) -> ApiResult<Json<Vec<StockRow>>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        repo::stock(&mut c, user.user.school_id, q.item_id, q.location_id).await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct MovementQuery {
    pub item_id: Option<Uuid>,
    pub location_id: Option<Uuid>,
    pub kind: Option<String>,
    pub recipient_kind: Option<String>,
    pub recipient_id: Option<Uuid>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub q: Option<String>,
    pub limit: Option<i64>,
}

async fn list_movements(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<MovementQuery>,
) -> ApiResult<Json<Vec<MovementView>>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        repo::list_movements(
            &mut c,
            user.user.school_id,
            &repo::MovementFilter {
                item_id: q.item_id,
                location_id: q.location_id,
                kind: q.kind,
                recipient_kind: q.recipient_kind,
                recipient_id: q.recipient_id,
                from: q.from,
                to: q.to,
                q: q.q,
            },
            q.limit.unwrap_or(500).clamp(1, 5000),
        )
        .await?,
    ))
}

/// Mirrored by `MovementResult` in the UI: the movement and the item's stock after it.
#[derive(Debug, Serialize)]
pub struct MovementResult {
    pub movement: MovementView,
    pub on_hand: Decimal,
    pub below_reorder: bool,
}

async fn record_movement(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<MovementInput>,
) -> ApiResult<(StatusCode, Json<MovementResult>)> {
    gate(&state, &user, true)?;
    let school = user.user.school_id;
    let mut tx = state.pool.begin().await?;
    let m = repo::record(
        &mut tx,
        &state.hlc,
        user.actor(),
        school,
        &body,
        today(),
        None,
    )
    .await?;
    let item = repo::get_item(&mut tx, body.item_id).await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(MovementResult {
            movement: m,
            on_hand: item.on_hand,
            below_reorder: item.below_reorder,
        }),
    ))
}

async fn delete_movement(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    if repo::get_movement(&mut tx, id).await?.movement.school_id != user.user.school_id {
        return Err(ApiError::not_found("movement"));
    }
    repo::delete_movement(&mut tx, &state.hlc, user.actor(), id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct TransferBody {
    pub item_id: Uuid,
    pub from_location_id: Uuid,
    pub to_location_id: Uuid,
    pub quantity: Decimal,
    pub moved_on: Option<NaiveDate>,
    #[serde(default)]
    pub notes: String,
}

async fn transfer(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<TransferBody>,
) -> ApiResult<(StatusCode, Json<Vec<MovementView>>)> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    let (out, inn) = repo::transfer(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        body.item_id,
        body.from_location_id,
        body.to_location_id,
        body.quantity,
        body.moved_on,
        &body.notes,
        today(),
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(vec![out, inn])))
}

/// Someone an issue can go to. Mirrored by `Recipient` in the UI.
#[derive(Debug, Serialize)]
pub struct Recipient {
    pub kind: RecipientKind,
    pub id: Uuid,
    pub name: String,
    pub detail: String,
}

#[derive(Debug, Deserialize)]
pub struct RecipientQuery {
    pub kind: Option<RecipientKind>,
    pub q: Option<String>,
}

/// Staff, students and rooms by name, for the issue form.
async fn recipients(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<RecipientQuery>,
) -> ApiResult<Json<Vec<Recipient>>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    let school = user.user.school_id;
    let needle = q.q.as_deref().map(str::trim).unwrap_or("").to_lowercase();
    let mut out = Vec::new();
    if q.kind.is_none_or(|k| k == RecipientKind::Staff) {
        out.extend(
            staff_repo::list(&mut c, school, q.q.as_deref())
                .await?
                .into_iter()
                .filter(|s| s.status == "active")
                .take(10)
                .map(|s| Recipient {
                    kind: RecipientKind::Staff,
                    id: s.id,
                    name: s.full_name,
                    detail: s.designation,
                }),
        );
    }
    if q.kind.is_none_or(|k| k == RecipientKind::Student) && !needle.is_empty() {
        let page = students::list(
            &mut c,
            school,
            &students::StudentFilter {
                q: q.q.clone(),
                class_id: None,
                section_id: None,
                status: Some("active".into()),
                section_scope: None,
            },
            students::StudentSort::Name,
            false,
            1,
            10,
        )
        .await?;
        out.extend(page.items.into_iter().map(|s| Recipient {
            kind: RecipientKind::Student,
            id: s.id,
            name: s.full_name,
            detail: match (s.class_name, s.section_name) {
                (Some(c), Some(sec)) => format!("{c}-{sec}"),
                (Some(c), None) => c,
                _ => s.student_number,
            },
        }));
    }
    if q.kind.is_none_or(|k| k == RecipientKind::Room) {
        out.extend(
            timetable::list_rooms(&mut c, school)
                .await?
                .into_iter()
                .filter(|r| needle.is_empty() || r.name.to_lowercase().contains(&needle))
                .take(10)
                .map(|r| Recipient {
                    kind: RecipientKind::Room,
                    id: r.id,
                    name: r.name,
                    detail: r.building,
                }),
        );
    }
    Ok(Json(out))
}

// ---- assets ---------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct AssetQuery {
    pub item_id: Option<Uuid>,
    pub status: Option<String>,
    pub location_id: Option<Uuid>,
    pub q: Option<String>,
}

async fn list_assets(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<AssetQuery>,
) -> ApiResult<Json<Vec<AssetView>>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        repo::list_assets(
            &mut c,
            user.user.school_id,
            &repo::AssetFilter {
                item_id: q.item_id,
                status: q.status,
                location_id: q.location_id,
                q: q.q,
            },
        )
        .await?,
    ))
}

async fn create_asset(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<AssetInput>,
) -> ApiResult<(StatusCode, Json<AssetView>)> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    let a = repo::save_asset(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        None,
        &body,
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(a)))
}

async fn owned_asset(c: &mut sqlx::PgConnection, user: &CurrentUser, id: Uuid) -> ApiResult<Asset> {
    let a = repo::get_asset(c, id).await?;
    if a.asset.school_id != user.user.school_id {
        return Err(ApiError::not_found("asset"));
    }
    Ok(a.asset)
}

async fn update_asset(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AssetInput>,
) -> ApiResult<Json<AssetView>> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    owned_asset(&mut tx, &user, id).await?;
    let a = repo::save_asset(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        Some(id),
        &body,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(a))
}

async fn delete_asset(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    owned_asset(&mut tx, &user, id).await?;
    repo::delete_asset(&mut tx, &state.hlc, user.actor(), id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
