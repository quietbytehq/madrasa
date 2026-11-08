//! Hostel reports (Standard stage 6): occupancy by hostel and room, residents by class
//! and residents with fees outstanding.

use madrasa_db::hostel as repo;
use rust_decimal::Decimal;
use sqlx::PgConnection;
use std::collections::BTreeMap;

use super::text::tr;
use super::{money, Column, Context, Group, Params, Report};
use crate::ApiError;

/// Beds, residents and free beds per room, grouped by hostel.
pub async fn occupancy(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    _params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "hostel.occupancy",
        lang,
        &ctx.school.name,
        tr(lang, "report.hostel.occupancy"),
    );
    r.param("param.academic_year", ctx.year.name.clone());
    r.columns = vec![
        Column::text(tr(lang, "hst.col.floor")),
        Column::wide(tr(lang, "hst.col.room")),
        Column::number(tr(lang, "hst.col.beds")),
        Column::number(tr(lang, "hst.col.occupied")),
        Column::number(tr(lang, "hst.col.free")),
    ];
    let mut t = [0i64; 3];
    for hostel in repo::list_hostels(conn, ctx.school.id).await? {
        let rooms = repo::board(conn, ctx.school.id, hostel.hostel.id, ctx.year.id).await?;
        let mut g = Group {
            heading: Some(hostel.hostel.name.clone()),
            ..Default::default()
        };
        let mut sub = [0i64; 3];
        for room in rooms {
            let beds = i64::from(room.room.room.beds);
            let occupied = room.beds.iter().filter(|b| b.resident.is_some()).count() as i64;
            let free = (beds - occupied).max(0);
            sub[0] += beds;
            sub[1] += occupied;
            sub[2] += free;
            g.rows.push(vec![
                room.room.room.floor,
                room.room.room.name,
                beds.to_string(),
                occupied.to_string(),
                free.to_string(),
            ]);
        }
        for (a, b) in t.iter_mut().zip(sub) {
            *a += b;
        }
        g.subtotal = Some(vec![
            tr(lang, "tt.total").into(),
            String::new(),
            sub[0].to_string(),
            sub[1].to_string(),
            sub[2].to_string(),
        ]);
        r.push_group(g);
    }
    r.total = Some(vec![
        tr(lang, "tt.total").into(),
        String::new(),
        t[0].to_string(),
        t[1].to_string(),
        t[2].to_string(),
    ]);
    Ok(r)
}

/// Residents grouped by class, with hostel, room and bed.
pub async fn residents(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "hostel.residents",
        lang,
        &ctx.school.name,
        tr(lang, "report.hostel.residents"),
    );
    r.param("param.academic_year", ctx.year.name.clone());
    let class_id = params.uuid("class_id")?;
    r.columns = vec![
        Column::text(tr(lang, "col.student_number")),
        Column::wide(tr(lang, "col.name")),
        Column::text(tr(lang, "hst.col.hostel")),
        Column::text(tr(lang, "hst.col.room")),
        Column::number(tr(lang, "hst.col.bed")),
        Column::text(tr(lang, "hst.col.guardian_phone")),
    ];
    let rows = repo::list_allocations(
        conn,
        ctx.school.id,
        &repo::AllocationFilter {
            academic_year_id: Some(ctx.year.id),
            class_id,
            open_only: true,
            ..Default::default()
        },
    )
    .await?;
    let mut by_class: BTreeMap<String, Vec<repo::AllocationView>> = BTreeMap::new();
    for a in rows {
        let key = if a.class_name.is_empty() {
            tr(lang, "hst.no_class").to_string()
        } else {
            format!("{}-{}", a.class_name, a.section_name)
        };
        by_class.entry(key).or_default().push(a);
    }
    let mut n = 0;
    for (class, list) in by_class {
        let mut g = Group {
            heading: Some(class),
            ..Default::default()
        };
        for a in list {
            n += 1;
            g.rows.push(vec![
                a.student_number,
                a.student_name,
                a.hostel_name,
                a.room_name,
                a.allocation.bed_no.to_string(),
                a.guardian_phone,
            ]);
        }
        r.push_group(g);
    }
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

/// Residents with any fee balance outstanding, by hostel.
pub async fn outstanding(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    _params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "hostel.outstanding",
        lang,
        &ctx.school.name,
        tr(lang, "report.hostel.outstanding"),
    );
    r.param("param.academic_year", ctx.year.name.clone());
    r.columns = vec![
        Column::text(tr(lang, "col.student_number")),
        Column::wide(tr(lang, "col.name")),
        Column::text(tr(lang, "col.class")),
        Column::text(tr(lang, "hst.col.room")),
        Column::text(tr(lang, "hst.col.guardian_phone")),
        Column::number(tr(lang, "hst.col.vouchers")),
        Column::number(tr(lang, "hst.col.balance")),
    ];
    let rows = repo::outstanding(conn, ctx.school.id, ctx.year.id, None).await?;
    let mut by_hostel: BTreeMap<String, Vec<repo::OutstandingRow>> = BTreeMap::new();
    for row in rows {
        by_hostel
            .entry(row.hostel_name.clone())
            .or_default()
            .push(row);
    }
    let mut total = Decimal::ZERO;
    for (hostel, list) in by_hostel {
        let mut g = Group {
            heading: Some(hostel),
            ..Default::default()
        };
        let mut sub = Decimal::ZERO;
        for row in list {
            sub += row.balance;
            g.rows.push(vec![
                row.student_number,
                row.student_name,
                format!("{}-{}", row.class_name, row.section_name),
                row.room_name,
                row.guardian_phone,
                row.vouchers.to_string(),
                money(row.balance),
            ]);
        }
        total += sub;
        let mut s = vec![tr(lang, "tt.total").to_string()];
        s.resize(6, String::new());
        s.push(money(sub));
        g.subtotal = Some(s);
        r.push_group(g);
    }
    let mut t = vec![tr(lang, "tt.total").to_string()];
    t.resize(6, String::new());
    t.push(money(total));
    r.total = Some(t);
    Ok(r)
}
