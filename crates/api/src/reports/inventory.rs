//! Inventory reports (Standard stage 7): stock on hand, items below their reorder level
//! and movements in a period.

use madrasa_db::inventory as repo;
use rust_decimal::Decimal;
use sqlx::PgConnection;
use std::collections::BTreeMap;

use super::text::tr;
use super::{format_date, money, Column, Context, Group, Params, Report};
use crate::ApiError;

fn qty(d: Decimal) -> String {
    d.normalize().to_string()
}

/// Stock on hand per item and location, grouped by category.
pub async fn stock(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    _params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "inventory.stock",
        lang,
        &ctx.school.name,
        tr(lang, "report.inventory.stock"),
    );
    r.columns = vec![
        Column::wide(tr(lang, "inv.col.item")),
        Column::text(tr(lang, "inv.col.code")),
        Column::text(tr(lang, "inv.col.location")),
        Column::number(tr(lang, "inv.col.on_hand")),
        Column::text(tr(lang, "inv.col.unit")),
    ];
    let rows = repo::stock(conn, ctx.school.id, None, None).await?;
    let mut by_cat: BTreeMap<String, Vec<repo::StockRow>> = BTreeMap::new();
    for row in rows {
        by_cat
            .entry(
                row.category_name
                    .clone()
                    .unwrap_or_else(|| tr(lang, "inv.no_category").into()),
            )
            .or_default()
            .push(row);
    }
    for (cat, list) in by_cat {
        let mut g = Group {
            heading: Some(cat),
            ..Default::default()
        };
        for row in list {
            g.rows.push(vec![
                row.item_name,
                row.code,
                row.location_name,
                qty(row.on_hand),
                row.unit,
            ]);
        }
        r.push_group(g);
    }
    Ok(r)
}

/// Items at or below their reorder level with the shortfall.
pub async fn reorder(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    _params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "inventory.reorder",
        lang,
        &ctx.school.name,
        tr(lang, "report.inventory.reorder"),
    );
    r.columns = vec![
        Column::wide(tr(lang, "inv.col.item")),
        Column::text(tr(lang, "inv.col.category")),
        Column::number(tr(lang, "inv.col.on_hand")),
        Column::number(tr(lang, "inv.col.reorder_level")),
        Column::number(tr(lang, "inv.col.shortfall")),
        Column::text(tr(lang, "inv.col.unit")),
    ];
    let items = repo::list_items(
        conn,
        ctx.school.id,
        &repo::ItemFilter {
            below_reorder: true,
            active_only: true,
            ..Default::default()
        },
    )
    .await?;
    let mut g = Group::default();
    for i in items {
        let level = i.item.reorder_level.unwrap_or_default();
        g.rows.push(vec![
            i.item.name,
            i.category_name.unwrap_or_default(),
            qty(i.on_hand),
            qty(level),
            qty((level - i.on_hand).max(Decimal::ZERO)),
            i.item.unit,
        ]);
    }
    let n = g.rows.len();
    r.push_group(g);
    r.total = Some(vec![
        tr(lang, "tt.total").into(),
        n.to_string(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    ]);
    Ok(r)
}

/// Movements in a date range summed per item: received, issued, returned, written off,
/// adjusted, and the purchase cost.
pub async fn movements(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "inventory.movements",
        lang,
        &ctx.school.name,
        tr(lang, "report.inventory.movements"),
    );
    let from = params.date("from")?.unwrap_or(ctx.year.start_date);
    let to = params.date("to")?.unwrap_or_else(crate::clock::today);
    r.param("param.from", format_date(from));
    r.param("param.to", format_date(to));
    r.columns = vec![
        Column::wide(tr(lang, "inv.col.item")),
        Column::text(tr(lang, "inv.col.unit")),
        Column::number(tr(lang, "inv.col.received")),
        Column::number(tr(lang, "inv.col.issued")),
        Column::number(tr(lang, "inv.col.returned")),
        Column::number(tr(lang, "inv.col.written_off")),
        Column::number(tr(lang, "inv.col.adjusted")),
        Column::number(tr(lang, "inv.col.cost_in")),
    ];
    let rows = repo::movement_summary(conn, ctx.school.id, from, to).await?;
    let mut by_cat: BTreeMap<String, Vec<repo::MovementSummary>> = BTreeMap::new();
    for row in rows {
        by_cat
            .entry(
                row.category_name
                    .clone()
                    .unwrap_or_else(|| tr(lang, "inv.no_category").into()),
            )
            .or_default()
            .push(row);
    }
    let mut cost = Decimal::ZERO;
    for (cat, list) in by_cat {
        let mut g = Group {
            heading: Some(cat),
            ..Default::default()
        };
        for row in list {
            cost += row.cost_in;
            g.rows.push(vec![
                row.item_name,
                row.unit,
                qty(row.received),
                qty(row.issued),
                qty(row.returned),
                qty(row.written_off),
                qty(row.adjusted),
                money(row.cost_in),
            ]);
        }
        r.push_group(g);
    }
    let mut t = vec![tr(lang, "tt.total").to_string()];
    t.resize(7, String::new());
    t.push(money(cost));
    r.total = Some(t);
    Ok(r)
}
