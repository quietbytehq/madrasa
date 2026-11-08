//! Transport reports (Standard stage 5): route manifests, students without a stop and
//! the monthly fee per route.

use chrono::NaiveDate;
use madrasa_db::transport as repo;
use madrasa_db::{academics, students};
use rust_decimal::Decimal;
use sqlx::PgConnection;

use super::text::tr;
use super::{format_date, money, Column, Context, Group, Params, Report};
use crate::ApiError;

fn time(t: Option<chrono::NaiveTime>) -> String {
    t.map(|t| t.format("%H:%M").to_string()).unwrap_or_default()
}

/// Every route with its stops and the students on them, for the driver's clipboard.
pub async fn manifest(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    _params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "transport.manifest",
        lang,
        &ctx.school.name,
        tr(lang, "report.transport.manifest"),
    );
    r.param("param.academic_year", ctx.year.name.clone());
    r.columns = vec![
        Column::text(tr(lang, "trp.col.stop")),
        Column::text(tr(lang, "trp.col.pickup")),
        Column::text(tr(lang, "col.student_number")),
        Column::wide(tr(lang, "col.name")),
        Column::text(tr(lang, "col.class")),
        Column::text(tr(lang, "trp.col.guardian_phone")),
    ];
    let routes = repo::list_routes(conn, ctx.school.id).await?;
    for route in routes.into_iter().filter(|r| r.route.active) {
        let stops = repo::roster(conn, ctx.school.id, route.route.id, ctx.year.id).await?;
        let mut heading = route.route.name.clone();
        if let Some(reg) = &route.vehicle_registration {
            heading.push_str(&format!(" · {reg}"));
        }
        if let Some(d) = route.driver_name.as_deref().filter(|d| !d.is_empty()) {
            heading.push_str(&format!(" · {d}"));
        }
        let mut g = Group {
            heading: Some(heading),
            ..Default::default()
        };
        let mut n = 0;
        for s in stops {
            if s.students.is_empty() {
                g.rows.push(vec![
                    s.stop.name.clone(),
                    time(s.stop.pickup_time),
                    String::new(),
                    tr(lang, "trp.no_students").into(),
                    String::new(),
                    String::new(),
                ]);
            }
            for a in s.students {
                n += 1;
                g.rows.push(vec![
                    s.stop.name.clone(),
                    time(s.stop.pickup_time),
                    a.student_number,
                    a.student_name,
                    format!("{}-{}", a.class_name, a.section_name),
                    a.guardian_phone,
                ]);
            }
        }
        let mut sub = vec![tr(lang, "trp.students").to_string(), n.to_string()];
        sub.resize(6, String::new());
        g.subtotal = Some(sub);
        r.push_group(g);
    }
    Ok(r)
}

/// Active students of the year (one class or section, or all) with no transport.
pub async fn without_stop(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "transport.without_stop",
        lang,
        &ctx.school.name,
        tr(lang, "report.transport.without_stop"),
    );
    r.param("param.academic_year", ctx.year.name.clone());
    let class_id = params.uuid("class_id")?;
    let section_id = params.uuid("section_id")?;
    if let Some(c) = class_id {
        r.param("param.class", academics::get_class(conn, c).await?.name);
    }
    if let Some(s) = section_id {
        r.param("param.section", academics::get_section(conn, s).await?.name);
    }
    r.columns = vec![
        Column::text(tr(lang, "col.student_number")),
        Column::wide(tr(lang, "col.name")),
        Column::text(tr(lang, "col.class")),
        Column::number(tr(lang, "col.roll_no")),
    ];
    let rows = repo::students(
        conn,
        ctx.school.id,
        ctx.year.id,
        class_id,
        section_id,
        None,
        true,
    )
    .await?;
    let mut g = Group::default();
    for s in rows {
        g.rows.push(vec![
            s.student_number,
            s.full_name,
            format!("{}-{}", s.class_name, s.section_name),
            s.roll_no.map(|n| n.to_string()).unwrap_or_default(),
        ]);
    }
    let n = g.rows.len();
    r.push_group(g);
    r.total = Some(vec![
        tr(lang, "tt.total").into(),
        n.to_string(),
        String::new(),
        String::new(),
    ]);
    Ok(r)
}

/// Students and the monthly charge per route for a billing month.
pub async fn fee_by_route(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "transport.fee_by_route",
        lang,
        &ctx.school.name,
        tr(lang, "report.transport.fee_by_route"),
    );
    let (y, m) = params
        .month("month")?
        .ok_or_else(|| ApiError::validation("month is required (YYYY-MM)."))?;
    let month = NaiveDate::from_ymd_opt(y, m, 1)
        .ok_or_else(|| ApiError::validation("month must be a month (YYYY-MM)."))?;
    r.param("param.academic_year", ctx.year.name.clone());
    r.param("param.month", format_date(month));
    r.columns = vec![
        Column::wide(tr(lang, "trp.col.route")),
        Column::text(tr(lang, "trp.col.vehicle")),
        Column::number(tr(lang, "trp.students")),
        Column::number(tr(lang, "trp.col.monthly_total")),
    ];
    let rows = repo::fee_by_route(conn, ctx.school.id, ctx.year.id, month).await?;
    let mut g = Group::default();
    let mut students = 0;
    let mut total = Decimal::ZERO;
    for row in rows {
        students += row.students;
        total += row.total;
        g.rows.push(vec![
            row.route_name,
            row.vehicle_registration.unwrap_or_default(),
            row.students.to_string(),
            money(row.total),
        ]);
    }
    r.push_group(g);
    r.total = Some(vec![
        tr(lang, "tt.total").into(),
        String::new(),
        students.to_string(),
        money(total),
    ]);
    let _ = students::StudentSort::Name;
    Ok(r)
}
