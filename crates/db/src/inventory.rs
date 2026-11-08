//! Inventory (Standard stage 7): categories, items, stock locations, movements (stock on
//! hand is their sum per item and location, never negative) and asset tags.

use chrono::NaiveDate;
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use madrasa_core::inventory::{self as rules, MovementKind, RecipientKind};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::audit::{self, snapshot, Action};
use crate::{map_unique, Actor, DbError, DbResult};

fn yes() -> bool {
    true
}

// ---- categories and locations ---------------------------------------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Category {
    pub id: Uuid,
    pub school_id: Uuid,
    pub name: String,
    pub position: i32,
    pub hlc: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CategoryInput {
    pub name: String,
    #[serde(default)]
    pub position: i32,
}

pub async fn list_categories(conn: &mut PgConnection, school_id: Uuid) -> DbResult<Vec<Category>> {
    Ok(sqlx::query_as::<_, Category>(
        "SELECT id, school_id, name, position, hlc FROM inventory_categories WHERE school_id = $1 AND deleted_at IS NULL ORDER BY position, name",
    )
    .bind(school_id)
    .fetch_all(conn)
    .await?)
}

pub async fn get_category(conn: &mut PgConnection, id: Uuid) -> DbResult<Category> {
    sqlx::query_as::<_, Category>(
        "SELECT id, school_id, name, position, hlc FROM inventory_categories WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("category".into()))
}

pub async fn save_category(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    id: Option<Uuid>,
    input: &CategoryInput,
) -> DbResult<Category> {
    if input.name.trim().is_empty() {
        return Err(DbError::Conflict("the category needs a name".into()));
    }
    let (id, old) = match id {
        Some(id) => (id, Some(get_category(&mut *conn, id).await?)),
        None => (new_id(), None),
    };
    let row = sqlx::query_as::<_, Category>(
        "INSERT INTO inventory_categories (id, school_id, name, position, updated_by, hlc) VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, position = EXCLUDED.position, updated_at = now(), updated_by = EXCLUDED.updated_by, hlc = EXCLUDED.hlc
         RETURNING id, school_id, name, position, hlc",
    )
    .bind(id)
    .bind(school_id)
    .bind(input.name.trim())
    .bind(input.position)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a category with that name"))?;
    audit::record(
        conn,
        hlc,
        actor,
        if old.is_some() {
            Action::Update
        } else {
            Action::Create
        },
        "inventory_category",
        Some(id),
        old.map(|o| snapshot(&o)),
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

pub async fn delete_category(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
) -> DbResult<()> {
    let old = get_category(&mut *conn, id).await?;
    let used: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM inventory_items WHERE category_id = $1 AND deleted_at IS NULL)",
    )
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    if used {
        return Err(DbError::Conflict("items still use this category".into()));
    }
    sqlx::query("UPDATE inventory_categories SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1")
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
        "inventory_category",
        Some(id),
        Some(snapshot(&old)),
        None,
    )
    .await?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Location {
    pub id: Uuid,
    pub school_id: Uuid,
    pub name: String,
    pub kind: String,
    pub active: bool,
    pub notes: String,
    pub hlc: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LocationInput {
    pub name: String,
    #[serde(default = "store")]
    pub kind: String,
    #[serde(default = "yes")]
    pub active: bool,
    #[serde(default)]
    pub notes: String,
}

fn store() -> String {
    "store".into()
}

const LOCATION_COLS: &str = "id, school_id, name, kind, active, notes, hlc";

pub async fn list_locations(conn: &mut PgConnection, school_id: Uuid) -> DbResult<Vec<Location>> {
    Ok(sqlx::query_as::<_, Location>(sqlx::AssertSqlSafe(format!(
        "SELECT {LOCATION_COLS} FROM inventory_locations WHERE school_id = $1 AND deleted_at IS NULL ORDER BY name"
    )))
    .bind(school_id)
    .fetch_all(conn)
    .await?)
}

pub async fn get_location(conn: &mut PgConnection, id: Uuid) -> DbResult<Location> {
    sqlx::query_as::<_, Location>(sqlx::AssertSqlSafe(format!(
        "SELECT {LOCATION_COLS} FROM inventory_locations WHERE id = $1 AND deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("location".into()))
}

pub async fn save_location(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    id: Option<Uuid>,
    input: &LocationInput,
) -> DbResult<Location> {
    if input.name.trim().is_empty() {
        return Err(DbError::Conflict("the location needs a name".into()));
    }
    if !["store", "lab", "room", "office", "other"].contains(&input.kind.as_str()) {
        return Err(DbError::Conflict(
            "kind is store, lab, room, office or other".into(),
        ));
    }
    let (id, old) = match id {
        Some(id) => (id, Some(get_location(&mut *conn, id).await?)),
        None => (new_id(), None),
    };
    let row = sqlx::query_as::<_, Location>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO inventory_locations (id, school_id, name, kind, active, notes, updated_by, hlc) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, kind = EXCLUDED.kind, active = EXCLUDED.active, notes = EXCLUDED.notes,
             updated_at = now(), updated_by = EXCLUDED.updated_by, hlc = EXCLUDED.hlc
         RETURNING {LOCATION_COLS}"
    )))
    .bind(id)
    .bind(school_id)
    .bind(input.name.trim())
    .bind(&input.kind)
    .bind(input.active)
    .bind(input.notes.trim())
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a location with that name"))?;
    audit::record(
        conn,
        hlc,
        actor,
        if old.is_some() {
            Action::Update
        } else {
            Action::Create
        },
        "inventory_location",
        Some(id),
        old.map(|o| snapshot(&o)),
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

/// Remove a location; refused while stock is on hand there.
pub async fn delete_location(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
) -> DbResult<()> {
    let old = get_location(&mut *conn, id).await?;
    let stocked: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT item_id FROM inventory_movements WHERE location_id = $1 AND deleted_at IS NULL GROUP BY item_id
         HAVING sum(CASE WHEN kind IN ('purchase_in', 'return', 'adjust_in', 'transfer_in') THEN quantity ELSE -quantity END) > 0)",
    )
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    if stocked {
        return Err(DbError::Conflict(
            "stock is still on hand at this location; transfer it first".into(),
        ));
    }
    sqlx::query("UPDATE inventory_locations SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1")
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
        "inventory_location",
        Some(id),
        Some(snapshot(&old)),
        None,
    )
    .await?;
    Ok(())
}

// ---- items ----------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Item {
    pub id: Uuid,
    pub school_id: Uuid,
    pub category_id: Option<Uuid>,
    pub name: String,
    pub code: String,
    pub unit: String,
    pub reorder_level: Option<Decimal>,
    pub is_asset: bool,
    pub active: bool,
    pub notes: String,
    pub custom: Value,
    pub hlc: String,
}

/// An item with its category name and stock on hand. Mirrored by `ItemView` in the UI.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ItemView {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub item: Item,
    pub category_name: Option<String>,
    pub on_hand: Decimal,
    pub below_reorder: bool,
    pub assets: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ItemInput {
    pub category_id: Option<Uuid>,
    pub name: String,
    #[serde(default)]
    pub code: String,
    #[serde(default = "pcs")]
    pub unit: String,
    pub reorder_level: Option<Decimal>,
    #[serde(default)]
    pub is_asset: bool,
    #[serde(default = "yes")]
    pub active: bool,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub custom: Value,
}

fn pcs() -> String {
    "pcs".into()
}

const ITEM_COLS: &str = "i.id, i.school_id, i.category_id, i.name, i.code, i.unit, i.reorder_level, i.is_asset, i.active, i.notes, i.custom, i.hlc";
const ON_HAND: &str = "coalesce((SELECT sum(CASE WHEN m.kind IN ('purchase_in', 'return', 'adjust_in', 'transfer_in') THEN m.quantity ELSE -m.quantity END)
    FROM inventory_movements m WHERE m.item_id = i.id AND m.deleted_at IS NULL), 0)::numeric(14,2)";
const ITEM_EXTRA: &str = "c.name AS category_name, {ON_HAND} AS on_hand,
    (i.reorder_level IS NOT NULL AND i.reorder_level > 0 AND {ON_HAND} <= i.reorder_level) AS below_reorder,
    (SELECT count(*) FROM inventory_assets a WHERE a.item_id = i.id AND a.deleted_at IS NULL AND a.status <> 'disposed') AS assets";
const ITEM_FROM: &str = "FROM inventory_items i LEFT JOIN inventory_categories c ON c.id = i.category_id AND c.deleted_at IS NULL";

fn item_extra() -> String {
    ITEM_EXTRA.replace("{ON_HAND}", ON_HAND)
}

#[derive(Debug, Clone, Default)]
pub struct ItemFilter {
    pub q: Option<String>,
    pub category_id: Option<Uuid>,
    pub below_reorder: bool,
    pub active_only: bool,
}

pub async fn list_items(
    conn: &mut PgConnection,
    school_id: Uuid,
    f: &ItemFilter,
) -> DbResult<Vec<ItemView>> {
    Ok(sqlx::query_as::<_, ItemView>(sqlx::AssertSqlSafe(format!(
        "SELECT * FROM (SELECT {ITEM_COLS}, {} {ITEM_FROM}
         WHERE i.school_id = $1 AND i.deleted_at IS NULL
           AND ($2::text IS NULL OR i.name ILIKE '%' || $2 || '%' OR i.code ILIKE '%' || $2 || '%')
           AND ($3::uuid IS NULL OR i.category_id = $3)
           AND (NOT $5 OR i.active)) x
         WHERE (NOT $4 OR x.below_reorder)
         ORDER BY x.category_name NULLS LAST, x.name",
        item_extra()
    )))
    .bind(school_id)
    .bind(f.q.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(f.category_id)
    .bind(f.below_reorder)
    .bind(f.active_only)
    .fetch_all(conn)
    .await?)
}

pub async fn get_item(conn: &mut PgConnection, id: Uuid) -> DbResult<ItemView> {
    sqlx::query_as::<_, ItemView>(sqlx::AssertSqlSafe(format!(
        "SELECT {ITEM_COLS}, {} {ITEM_FROM} WHERE i.id = $1 AND i.deleted_at IS NULL",
        item_extra()
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("item".into()))
}

fn check_item(input: &ItemInput) -> DbResult<()> {
    if input.name.trim().is_empty() {
        return Err(DbError::Conflict("the item needs a name".into()));
    }
    if input.reorder_level.is_some_and(|r| r < Decimal::ZERO) {
        return Err(DbError::Conflict(
            "the reorder level cannot be negative".into(),
        ));
    }
    Ok(())
}

pub async fn save_item(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    id: Option<Uuid>,
    input: &ItemInput,
) -> DbResult<ItemView> {
    check_item(input)?;
    if let Some(c) = input.category_id {
        let cat = get_category(&mut *conn, c).await?;
        if cat.school_id != school_id {
            return Err(DbError::NotFound("category".into()));
        }
    }
    let (id, old) = match id {
        Some(id) => (id, Some(get_item(&mut *conn, id).await?)),
        None => (new_id(), None),
    };
    sqlx::query(
        "INSERT INTO inventory_items (id, school_id, category_id, name, code, unit, reorder_level, is_asset, active, notes, custom, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
         ON CONFLICT (id) DO UPDATE SET category_id = EXCLUDED.category_id, name = EXCLUDED.name, code = EXCLUDED.code, unit = EXCLUDED.unit,
             reorder_level = EXCLUDED.reorder_level, is_asset = EXCLUDED.is_asset, active = EXCLUDED.active, notes = EXCLUDED.notes,
             custom = EXCLUDED.custom, updated_at = now(), updated_by = EXCLUDED.updated_by, hlc = EXCLUDED.hlc",
    )
    .bind(id)
    .bind(school_id)
    .bind(input.category_id)
    .bind(input.name.trim())
    .bind(input.code.trim())
    .bind(if input.unit.trim().is_empty() { "pcs" } else { input.unit.trim() })
    .bind(input.reorder_level)
    .bind(input.is_asset)
    .bind(input.active)
    .bind(input.notes.trim())
    .bind(if input.custom.is_object() { input.custom.clone() } else { json!({}) })
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "an item with that name or code"))?;
    let view = get_item(&mut *conn, id).await?;
    audit::record(
        conn,
        hlc,
        actor,
        if old.is_some() {
            Action::Update
        } else {
            Action::Create
        },
        "inventory_item",
        Some(id),
        old.map(|o| snapshot(&o.item)),
        Some(snapshot(&view.item)),
    )
    .await?;
    Ok(view)
}

/// Remove an item; refused while stock is on hand or assets are tagged.
pub async fn delete_item(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
) -> DbResult<()> {
    let old = get_item(&mut *conn, id).await?;
    if old.on_hand > Decimal::ZERO || old.assets > 0 {
        return Err(DbError::Conflict(
            "the item still has stock or tagged assets; write them off first".into(),
        ));
    }
    sqlx::query("UPDATE inventory_items SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1")
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
        "inventory_item",
        Some(id),
        Some(snapshot(&old.item)),
        None,
    )
    .await?;
    Ok(())
}

// ---- stock and movements --------------------------------------------------------------------

/// Stock of one item at one location. Mirrored by `StockRow` in the UI.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct StockRow {
    pub item_id: Uuid,
    pub item_name: String,
    pub code: String,
    pub unit: String,
    pub category_name: Option<String>,
    pub location_id: Uuid,
    pub location_name: String,
    pub on_hand: Decimal,
}

pub async fn stock(
    conn: &mut PgConnection,
    school_id: Uuid,
    item_id: Option<Uuid>,
    location_id: Option<Uuid>,
) -> DbResult<Vec<StockRow>> {
    Ok(sqlx::query_as::<_, StockRow>(
        "SELECT i.id AS item_id, i.name AS item_name, i.code, i.unit, c.name AS category_name, l.id AS location_id, l.name AS location_name,
                sum(CASE WHEN m.kind IN ('purchase_in', 'return', 'adjust_in', 'transfer_in') THEN m.quantity ELSE -m.quantity END)::numeric(14,2) AS on_hand
         FROM inventory_movements m
         JOIN inventory_items i ON i.id = m.item_id AND i.deleted_at IS NULL
         LEFT JOIN inventory_categories c ON c.id = i.category_id AND c.deleted_at IS NULL
         JOIN inventory_locations l ON l.id = m.location_id
         WHERE m.school_id = $1 AND m.deleted_at IS NULL
           AND ($2::uuid IS NULL OR m.item_id = $2)
           AND ($3::uuid IS NULL OR m.location_id = $3)
         GROUP BY i.id, i.name, i.code, i.unit, c.name, l.id, l.name
         HAVING sum(CASE WHEN m.kind IN ('purchase_in', 'return', 'adjust_in', 'transfer_in') THEN m.quantity ELSE -m.quantity END) <> 0
         ORDER BY c.name NULLS LAST, i.name, l.name",
    )
    .bind(school_id)
    .bind(item_id)
    .bind(location_id)
    .fetch_all(conn)
    .await?)
}

/// Stock of one item at one location, with the item row locked so two movements cannot
/// both pass the balance check.
async fn on_hand_locked(
    conn: &mut PgConnection,
    item_id: Uuid,
    location_id: Uuid,
) -> DbResult<Decimal> {
    sqlx::query("SELECT id FROM inventory_items WHERE id = $1 FOR UPDATE")
        .bind(item_id)
        .execute(&mut *conn)
        .await?;
    let v: Option<Decimal> = sqlx::query_scalar(
        "SELECT sum(CASE WHEN kind IN ('purchase_in', 'return', 'adjust_in', 'transfer_in') THEN quantity ELSE -quantity END)::numeric(14,2)
         FROM inventory_movements WHERE item_id = $1 AND location_id = $2 AND deleted_at IS NULL",
    )
    .bind(item_id)
    .bind(location_id)
    .fetch_one(conn)
    .await?;
    Ok(v.unwrap_or(Decimal::ZERO))
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Movement {
    pub id: Uuid,
    pub school_id: Uuid,
    pub item_id: Uuid,
    pub location_id: Uuid,
    pub kind: String,
    pub quantity: Decimal,
    pub unit_cost: Option<Decimal>,
    pub total_cost: Option<Decimal>,
    pub moved_on: NaiveDate,
    pub recipient_kind: Option<String>,
    pub recipient_id: Option<Uuid>,
    pub recipient_name: String,
    pub reference: String,
    pub transfer_id: Option<Uuid>,
    pub notes: String,
    pub recorded_by: Option<Uuid>,
    pub hlc: String,
}

/// Mirrored by `MovementView` in the UI.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MovementView {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub movement: Movement,
    pub item_name: String,
    pub unit: String,
    pub location_name: String,
    pub recorded_by_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MovementInput {
    pub item_id: Uuid,
    pub location_id: Uuid,
    pub kind: MovementKind,
    pub quantity: Decimal,
    pub unit_cost: Option<Decimal>,
    pub moved_on: Option<NaiveDate>,
    pub recipient_kind: Option<RecipientKind>,
    pub recipient_id: Option<Uuid>,
    #[serde(default)]
    pub recipient_name: String,
    #[serde(default)]
    pub reference: String,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Default)]
pub struct MovementFilter {
    pub item_id: Option<Uuid>,
    pub location_id: Option<Uuid>,
    pub kind: Option<String>,
    pub recipient_kind: Option<String>,
    pub recipient_id: Option<Uuid>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub q: Option<String>,
}

const MOVE_COLS: &str = "m.id, m.school_id, m.item_id, m.location_id, m.kind, m.quantity, m.unit_cost, m.total_cost, m.moved_on, m.recipient_kind,
    m.recipient_id, m.recipient_name, m.reference, m.transfer_id, m.notes, m.recorded_by, m.hlc";
const MOVE_EXTRA: &str =
    "i.name AS item_name, i.unit, l.name AS location_name, u.full_name AS recorded_by_name";
const MOVE_FROM: &str = "FROM inventory_movements m JOIN inventory_items i ON i.id = m.item_id JOIN inventory_locations l ON l.id = m.location_id
    LEFT JOIN users u ON u.id = m.recorded_by";

pub async fn list_movements(
    conn: &mut PgConnection,
    school_id: Uuid,
    f: &MovementFilter,
    limit: i64,
) -> DbResult<Vec<MovementView>> {
    Ok(sqlx::query_as::<_, MovementView>(sqlx::AssertSqlSafe(format!(
        "SELECT {MOVE_COLS}, {MOVE_EXTRA} {MOVE_FROM}
         WHERE m.school_id = $1 AND m.deleted_at IS NULL
           AND ($2::uuid IS NULL OR m.item_id = $2)
           AND ($3::uuid IS NULL OR m.location_id = $3)
           AND ($4::text IS NULL OR m.kind = $4)
           AND ($5::text IS NULL OR m.recipient_kind = $5)
           AND ($6::uuid IS NULL OR m.recipient_id = $6)
           AND ($7::date IS NULL OR m.moved_on >= $7)
           AND ($8::date IS NULL OR m.moved_on <= $8)
           AND ($9::text IS NULL OR i.name ILIKE '%' || $9 || '%' OR m.recipient_name ILIKE '%' || $9 || '%' OR m.reference ILIKE '%' || $9 || '%')
         ORDER BY m.moved_on DESC, m.created_at DESC LIMIT $10"
    )))
    .bind(school_id)
    .bind(f.item_id)
    .bind(f.location_id)
    .bind(f.kind.as_deref().filter(|s| !s.is_empty()))
    .bind(f.recipient_kind.as_deref().filter(|s| !s.is_empty()))
    .bind(f.recipient_id)
    .bind(f.from)
    .bind(f.to)
    .bind(f.q.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(limit)
    .fetch_all(conn)
    .await?)
}

pub async fn get_movement(conn: &mut PgConnection, id: Uuid) -> DbResult<MovementView> {
    sqlx::query_as::<_, MovementView>(sqlx::AssertSqlSafe(format!(
        "SELECT {MOVE_COLS}, {MOVE_EXTRA} {MOVE_FROM} WHERE m.id = $1 AND m.deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("movement".into()))
}

/// Record one movement after checking the balance at its location.
pub async fn record(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    input: &MovementInput,
    today: NaiveDate,
    transfer_id: Option<Uuid>,
) -> DbResult<MovementView> {
    let item = get_item(&mut *conn, input.item_id).await?;
    if item.item.school_id != school_id {
        return Err(DbError::NotFound("item".into()));
    }
    let location = get_location(&mut *conn, input.location_id).await?;
    if location.school_id != school_id {
        return Err(DbError::NotFound("location".into()));
    }
    if !location.active {
        return Err(DbError::Conflict("the location is not in use".into()));
    }
    if input.kind.needs_recipient() && input.recipient_name.trim().is_empty() {
        return Err(DbError::Conflict(
            "an issue or return names who received or brought back the goods".into(),
        ));
    }
    if input.unit_cost.is_some_and(|c| c < Decimal::ZERO) {
        return Err(DbError::Conflict("the unit cost cannot be negative".into()));
    }
    let on_hand = on_hand_locked(&mut *conn, input.item_id, input.location_id).await?;
    rules::apply(on_hand, input.kind, input.quantity).map_err(|m| DbError::Conflict(m.into()))?;
    let id = new_id();
    sqlx::query(
        "INSERT INTO inventory_movements (id, school_id, item_id, location_id, kind, quantity, unit_cost, total_cost, moved_on, recipient_kind,
                recipient_id, recipient_name, reference, transfer_id, notes, recorded_by, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $16, $17)",
    )
    .bind(id)
    .bind(school_id)
    .bind(input.item_id)
    .bind(input.location_id)
    .bind(input.kind.as_str())
    .bind(input.quantity)
    .bind(input.unit_cost)
    .bind(rules::total_cost(input.quantity, input.unit_cost))
    .bind(input.moved_on.unwrap_or(today))
    .bind(input.recipient_kind.map(|k| k.as_str()))
    .bind(input.recipient_id)
    .bind(input.recipient_name.trim())
    .bind(input.reference.trim())
    .bind(transfer_id)
    .bind(input.notes.trim())
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await?;
    let view = get_movement(&mut *conn, id).await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "inventory_movement",
        Some(id),
        None,
        Some(snapshot(&view.movement)),
    )
    .await?;
    Ok(view)
}

/// Move stock between two locations: a `transfer_out` and a `transfer_in` sharing an id.
#[allow(clippy::too_many_arguments)]
pub async fn transfer(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    item_id: Uuid,
    from: Uuid,
    to: Uuid,
    quantity: Decimal,
    moved_on: Option<NaiveDate>,
    notes: &str,
    today: NaiveDate,
) -> DbResult<(MovementView, MovementView)> {
    if from == to {
        return Err(DbError::Conflict("pick two different locations".into()));
    }
    let transfer_id = new_id();
    let base = MovementInput {
        item_id,
        location_id: from,
        kind: MovementKind::TransferOut,
        quantity,
        unit_cost: None,
        moved_on,
        recipient_kind: None,
        recipient_id: None,
        recipient_name: String::new(),
        reference: String::new(),
        notes: notes.into(),
    };
    let out = record(
        &mut *conn,
        hlc,
        actor,
        school_id,
        &base,
        today,
        Some(transfer_id),
    )
    .await?;
    let inn = record(
        conn,
        hlc,
        actor,
        school_id,
        &MovementInput {
            location_id: to,
            kind: MovementKind::TransferIn,
            ..base
        },
        today,
        Some(transfer_id),
    )
    .await?;
    Ok((out, inn))
}

/// Remove a movement (a wrong entry); refused when stock at the location would go negative.
pub async fn delete_movement(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
) -> DbResult<()> {
    let old = get_movement(&mut *conn, id).await?;
    let m = &old.movement;
    let kind =
        MovementKind::parse(&m.kind).ok_or_else(|| DbError::Conflict("unknown kind".into()))?;
    if kind.adds_stock() {
        let on_hand = on_hand_locked(&mut *conn, m.item_id, m.location_id).await?;
        if on_hand - m.quantity < Decimal::ZERO {
            return Err(DbError::Conflict(
                "the stock has been used since; record a write-off or adjustment instead".into(),
            ));
        }
    }
    let now = hlc.now().to_string();
    sqlx::query("UPDATE inventory_movements SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1 OR (transfer_id IS NOT NULL AND transfer_id = $4)")
        .bind(id)
        .bind(actor.user_id)
        .bind(&now)
        .bind(m.transfer_id)
        .execute(&mut *conn)
        .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Delete,
        "inventory_movement",
        Some(id),
        Some(snapshot(&old.movement)),
        None,
    )
    .await?;
    Ok(())
}

/// Movements in a period, summed per item and kind for the movements report.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MovementSummary {
    pub item_id: Uuid,
    pub item_name: String,
    pub unit: String,
    pub category_name: Option<String>,
    pub received: Decimal,
    pub issued: Decimal,
    pub returned: Decimal,
    pub written_off: Decimal,
    pub adjusted: Decimal,
    pub cost_in: Decimal,
}

pub async fn movement_summary(
    conn: &mut PgConnection,
    school_id: Uuid,
    from: NaiveDate,
    to: NaiveDate,
) -> DbResult<Vec<MovementSummary>> {
    Ok(sqlx::query_as::<_, MovementSummary>(
        "SELECT i.id AS item_id, i.name AS item_name, i.unit, c.name AS category_name,
                coalesce(sum(m.quantity) FILTER (WHERE m.kind = 'purchase_in'), 0)::numeric(14,2) AS received,
                coalesce(sum(m.quantity) FILTER (WHERE m.kind = 'issue'), 0)::numeric(14,2) AS issued,
                coalesce(sum(m.quantity) FILTER (WHERE m.kind = 'return'), 0)::numeric(14,2) AS returned,
                coalesce(sum(m.quantity) FILTER (WHERE m.kind = 'write_off'), 0)::numeric(14,2) AS written_off,
                coalesce(sum(CASE WHEN m.kind = 'adjust_in' THEN m.quantity WHEN m.kind = 'adjust_out' THEN -m.quantity ELSE 0 END), 0)::numeric(14,2) AS adjusted,
                coalesce(sum(m.total_cost) FILTER (WHERE m.kind = 'purchase_in'), 0)::numeric(14,2) AS cost_in
         FROM inventory_movements m
         JOIN inventory_items i ON i.id = m.item_id
         LEFT JOIN inventory_categories c ON c.id = i.category_id AND c.deleted_at IS NULL
         WHERE m.school_id = $1 AND m.deleted_at IS NULL AND m.moved_on BETWEEN $2 AND $3
         GROUP BY i.id, i.name, i.unit, c.name ORDER BY c.name NULLS LAST, i.name",
    )
    .bind(school_id)
    .bind(from)
    .bind(to)
    .fetch_all(conn)
    .await?)
}

// ---- assets ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Asset {
    pub id: Uuid,
    pub school_id: Uuid,
    pub item_id: Uuid,
    pub tag: String,
    pub serial_no: String,
    pub location_id: Option<Uuid>,
    pub status: String,
    pub assigned_kind: Option<String>,
    pub assigned_id: Option<Uuid>,
    pub assigned_name: String,
    pub purchased_on: Option<NaiveDate>,
    pub cost: Option<Decimal>,
    pub notes: String,
    pub hlc: String,
}

/// Mirrored by `AssetView` in the UI.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AssetView {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub asset: Asset,
    pub item_name: String,
    pub location_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetInput {
    pub item_id: Uuid,
    pub tag: String,
    #[serde(default)]
    pub serial_no: String,
    pub location_id: Option<Uuid>,
    #[serde(default = "in_store")]
    pub status: String,
    pub assigned_kind: Option<RecipientKind>,
    pub assigned_id: Option<Uuid>,
    #[serde(default)]
    pub assigned_name: String,
    pub purchased_on: Option<NaiveDate>,
    pub cost: Option<Decimal>,
    #[serde(default)]
    pub notes: String,
}

fn in_store() -> String {
    "in_store".into()
}

const ASSET_COLS: &str = "a.id, a.school_id, a.item_id, a.tag, a.serial_no, a.location_id, a.status, a.assigned_kind, a.assigned_id, a.assigned_name, a.purchased_on, a.cost, a.notes, a.hlc";
const ASSET_FROM: &str = "FROM inventory_assets a JOIN inventory_items i ON i.id = a.item_id LEFT JOIN inventory_locations l ON l.id = a.location_id";

#[derive(Debug, Clone, Default)]
pub struct AssetFilter {
    pub item_id: Option<Uuid>,
    pub status: Option<String>,
    pub location_id: Option<Uuid>,
    pub q: Option<String>,
}

pub async fn list_assets(
    conn: &mut PgConnection,
    school_id: Uuid,
    f: &AssetFilter,
) -> DbResult<Vec<AssetView>> {
    Ok(sqlx::query_as::<_, AssetView>(sqlx::AssertSqlSafe(format!(
        "SELECT {ASSET_COLS}, i.name AS item_name, l.name AS location_name {ASSET_FROM}
         WHERE a.school_id = $1 AND a.deleted_at IS NULL
           AND ($2::uuid IS NULL OR a.item_id = $2)
           AND ($3::text IS NULL OR a.status = $3)
           AND ($4::uuid IS NULL OR a.location_id = $4)
           AND ($5::text IS NULL OR a.tag ILIKE '%' || $5 || '%' OR a.serial_no ILIKE '%' || $5 || '%' OR i.name ILIKE '%' || $5 || '%' OR a.assigned_name ILIKE '%' || $5 || '%')
         ORDER BY i.name, a.tag"
    )))
    .bind(school_id)
    .bind(f.item_id)
    .bind(f.status.as_deref().filter(|s| !s.is_empty()))
    .bind(f.location_id)
    .bind(f.q.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .fetch_all(conn)
    .await?)
}

pub async fn get_asset(conn: &mut PgConnection, id: Uuid) -> DbResult<AssetView> {
    sqlx::query_as::<_, AssetView>(sqlx::AssertSqlSafe(format!(
        "SELECT {ASSET_COLS}, i.name AS item_name, l.name AS location_name {ASSET_FROM} WHERE a.id = $1 AND a.deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("asset".into()))
}

pub async fn save_asset(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    id: Option<Uuid>,
    input: &AssetInput,
) -> DbResult<AssetView> {
    if input.tag.trim().is_empty() {
        return Err(DbError::Conflict("the asset needs a tag".into()));
    }
    if !["in_store", "in_use", "under_repair", "disposed"].contains(&input.status.as_str()) {
        return Err(DbError::Conflict(
            "status is in_store, in_use, under_repair or disposed".into(),
        ));
    }
    if input.cost.is_some_and(|c| c < Decimal::ZERO) {
        return Err(DbError::Conflict("the cost cannot be negative".into()));
    }
    let item = get_item(&mut *conn, input.item_id).await?;
    if item.item.school_id != school_id {
        return Err(DbError::NotFound("item".into()));
    }
    if let Some(l) = input.location_id {
        if get_location(&mut *conn, l).await?.school_id != school_id {
            return Err(DbError::NotFound("location".into()));
        }
    }
    let (id, old) = match id {
        Some(id) => (id, Some(get_asset(&mut *conn, id).await?)),
        None => (new_id(), None),
    };
    sqlx::query(
        "INSERT INTO inventory_assets (id, school_id, item_id, tag, serial_no, location_id, status, assigned_kind, assigned_id, assigned_name, purchased_on, cost, notes, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
         ON CONFLICT (id) DO UPDATE SET item_id = EXCLUDED.item_id, tag = EXCLUDED.tag, serial_no = EXCLUDED.serial_no, location_id = EXCLUDED.location_id,
             status = EXCLUDED.status, assigned_kind = EXCLUDED.assigned_kind, assigned_id = EXCLUDED.assigned_id, assigned_name = EXCLUDED.assigned_name,
             purchased_on = EXCLUDED.purchased_on, cost = EXCLUDED.cost, notes = EXCLUDED.notes, updated_at = now(), updated_by = EXCLUDED.updated_by, hlc = EXCLUDED.hlc",
    )
    .bind(id)
    .bind(school_id)
    .bind(input.item_id)
    .bind(input.tag.trim())
    .bind(input.serial_no.trim())
    .bind(input.location_id)
    .bind(&input.status)
    .bind(input.assigned_kind.map(|k| k.as_str()))
    .bind(input.assigned_id)
    .bind(input.assigned_name.trim())
    .bind(input.purchased_on)
    .bind(input.cost)
    .bind(input.notes.trim())
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "an asset with that tag"))?;
    let view = get_asset(&mut *conn, id).await?;
    audit::record(
        conn,
        hlc,
        actor,
        if old.is_some() {
            Action::Update
        } else {
            Action::Create
        },
        "inventory_asset",
        Some(id),
        old.map(|o| snapshot(&o.asset)),
        Some(snapshot(&view.asset)),
    )
    .await?;
    Ok(view)
}

pub async fn delete_asset(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
) -> DbResult<()> {
    let old = get_asset(&mut *conn, id).await?;
    sqlx::query("UPDATE inventory_assets SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1")
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
        "inventory_asset",
        Some(id),
        Some(snapshot(&old.asset)),
        None,
    )
    .await?;
    Ok(())
}
