//! Leave reports (Standard stage 2): leave balances, the staff leave register and the
//! student leave register.

use chrono::NaiveDate;
use madrasa_core::leave::RequestStatus;
use madrasa_db::leave as repo;
use rust_decimal::Decimal;
use sqlx::PgConnection;

use super::students::scope_params;
use super::text::tr;
use super::{format_date, Column, Context, Group, Lang, Params, Report};
use crate::ApiError;

fn days(d: Decimal) -> String {
    d.normalize().to_string()
}

fn status(lang: Lang, s: &str) -> String {
    tr(lang, &format!("leave.status.{s}")).into()
}

fn range(
    params: &Params,
    year_from: NaiveDate,
    year_to: NaiveDate,
) -> Result<(NaiveDate, NaiveDate), ApiError> {
    let from = params.date("from")?.unwrap_or(year_from);
    let to = params.date("to")?.unwrap_or(year_to);
    if to < from {
        return Err(ApiError::validation("The range ends before it starts."));
    }
    Ok((from, to))
}

/// Every staff member's balance per leave type for the year.
pub async fn balances(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    _params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "leave.balances",
        lang,
        &ctx.school.name,
        tr(lang, "report.leave.balances"),
    );
    r.param("param.academic_year", ctx.year.name.clone());
    r.columns = vec![
        Column::wide(tr(lang, "col.name")),
        Column::text(tr(lang, "leave.col.type")),
        Column::number(tr(lang, "leave.col.entitlement")),
        Column::number(tr(lang, "leave.col.carried")),
        Column::number(tr(lang, "leave.col.adjustment")),
        Column::number(tr(lang, "leave.col.used")),
        Column::number(tr(lang, "leave.col.pending")),
        Column::number(tr(lang, "leave.col.available")),
    ];
    let rows = repo::balances(conn, ctx.school.id, ctx.year.id, None).await?;
    let mut g = Group::default();
    for b in rows {
        let bal = b.balance();
        g.rows.push(vec![
            b.staff_name,
            b.type_name,
            days(b.entitlement),
            days(b.carried_over),
            days(b.adjustment),
            days(b.used),
            days(b.pending),
            days(bal.available()),
        ]);
    }
    r.push_group(g);
    Ok(r)
}

/// Staff leave requests in a date range (default: the year), with their decision.
pub async fn register(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "leave.register",
        lang,
        &ctx.school.name,
        tr(lang, "report.leave.register"),
    );
    r.landscape = true;
    let (from, to) = range(params, ctx.year.start_date, ctx.year.end_date)?;
    r.param("param.academic_year", ctx.year.name.clone());
    r.param("param.from", format_date(from));
    r.param("param.to", format_date(to));
    let status_filter: Option<RequestStatus> = match params.get("status") {
        None => None,
        Some(s) => Some(s.parse().map_err(|_| {
            ApiError::validation("status is pending, approved, rejected or withdrawn.")
        })?),
    };
    r.columns = vec![
        Column::text(tr(lang, "col.staff_number")),
        Column::wide(tr(lang, "col.name")),
        Column::text(tr(lang, "leave.col.type")),
        Column::text(tr(lang, "param.from")),
        Column::text(tr(lang, "param.to")),
        Column::number(tr(lang, "leave.col.days")),
        Column::text(tr(lang, "col.status")),
        Column::wide(tr(lang, "leave.col.reason")),
        Column::text(tr(lang, "leave.col.decided_by")),
    ];
    let rows = repo::list_requests(
        conn,
        ctx.school.id,
        &repo::RequestFilter {
            academic_year_id: Some(ctx.year.id),
            staff_id: None,
            status: status_filter,
            from: Some(from),
            to: Some(to),
        },
    )
    .await?;
    let mut g = Group::default();
    let mut total = Decimal::ZERO;
    for v in rows {
        let q = &v.request;
        if q.status == "approved" {
            total += q.days;
        }
        g.rows.push(vec![
            v.staff_number,
            v.staff_name,
            v.type_name,
            format_date(q.from_date),
            format_date(q.to_date),
            days(q.days),
            status(lang, &q.status),
            q.reason.clone(),
            v.decided_by_name.unwrap_or_default(),
        ]);
    }
    r.push_group(g);
    r.total = Some(vec![
        tr(lang, "leave.col.approved_days").into(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        days(total),
        String::new(),
        String::new(),
        String::new(),
    ]);
    Ok(r)
}

/// Student leave requests of a class or section in a date range.
pub async fn students(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "leave.students",
        lang,
        &ctx.school.name,
        tr(lang, "report.leave.students"),
    );
    r.landscape = true;
    let (from, to) = range(params, ctx.year.start_date, ctx.year.end_date)?;
    let (class_id, section_id) = scope_params(conn, ctx, params, &mut r, false).await?;
    r.param("param.from", format_date(from));
    r.param("param.to", format_date(to));
    r.columns = vec![
        Column::text(tr(lang, "col.student_number")),
        Column::wide(tr(lang, "col.name")),
        Column::text(tr(lang, "col.class")),
        Column::text(tr(lang, "param.from")),
        Column::text(tr(lang, "param.to")),
        Column::number(tr(lang, "leave.col.days")),
        Column::text(tr(lang, "col.status")),
        Column::wide(tr(lang, "leave.col.reason")),
        Column::text(tr(lang, "leave.col.guardian")),
    ];
    let section_ids = match (section_id, class_id, &ctx.section_scope) {
        (Some(s), _, _) => Some(vec![s]),
        (None, Some(c), _) => Some(
            madrasa_db::academics::list_sections(conn, ctx.school.id, Some(ctx.year.id), Some(c))
                .await?
                .into_iter()
                .map(|s| s.id)
                .filter(|s| ctx.allows(*s))
                .collect(),
        ),
        (None, None, Some(scope)) => Some(scope.clone()),
        (None, None, None) => None,
    };
    let rows = repo::list_student_requests(
        conn,
        ctx.school.id,
        &repo::StudentFilter {
            academic_year_id: Some(ctx.year.id),
            student_id: None,
            section_ids,
            status: None,
            from: Some(from),
            to: Some(to),
        },
    )
    .await?;
    let mut g = Group::default();
    for v in rows {
        let q = &v.request;
        g.rows.push(vec![
            v.student_number,
            v.full_name,
            format!("{}-{}", v.class_name, v.section_name),
            format_date(q.from_date),
            format_date(q.to_date),
            v.days.to_string(),
            status(lang, &q.status),
            q.reason.clone(),
            v.guardian_name.unwrap_or_default(),
        ]);
    }
    r.push_group(g);
    Ok(r)
}
