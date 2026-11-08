//! Library reports (Standard stage 4): overdue books, fines collected, most borrowed and
//! stock by category.

use madrasa_core::library::MemberKind;
use madrasa_db::library as repo;
use rust_decimal::Decimal;
use sqlx::PgConnection;

use super::text::tr;
use super::{format_date, money, Column, Context, Group, Params, Report};
use crate::ApiError;

fn member_kind(lang: super::Lang, k: &str) -> String {
    tr(
        lang,
        if k == MemberKind::Staff.as_str() {
            "lib.member.staff"
        } else {
            "lib.member.student"
        },
    )
    .into()
}

/// Copies out past their due date, most overdue first.
pub async fn overdue(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "library.overdue",
        lang,
        &ctx.school.name,
        tr(lang, "report.library.overdue"),
    );
    let as_of = params.date("as_of")?.unwrap_or_else(crate::clock::today);
    r.param("param.as_of", format_date(as_of));
    r.columns = vec![
        Column::text(tr(lang, "lib.col.accession")),
        Column::wide(tr(lang, "lib.col.title")),
        Column::text(tr(lang, "lib.col.member_kind")),
        Column::wide(tr(lang, "col.name")),
        Column::text(tr(lang, "lib.col.detail")),
        Column::text(tr(lang, "lib.col.due")),
        Column::number(tr(lang, "lib.col.days_late")),
    ];
    let loans = repo::list_loans(
        conn,
        ctx.school.id,
        &repo::LoanFilter {
            state: Some("overdue".into()),
            ..Default::default()
        },
        as_of,
    )
    .await?;
    let mut g = Group::default();
    for v in loans {
        let l = &v.loan;
        g.rows.push(vec![
            v.accession_no,
            v.title,
            member_kind(lang, &l.member_kind),
            v.member_name,
            v.member_detail,
            format_date(l.due_on),
            madrasa_core::library::days_late(l.due_on, as_of).to_string(),
        ]);
    }
    r.push_group(g);
    Ok(r)
}

/// Fines settled (collected at the desk or posted to a voucher) in a date range.
pub async fn fines(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "library.fines",
        lang,
        &ctx.school.name,
        tr(lang, "report.library.fines"),
    );
    let from = params.date("from")?.unwrap_or(ctx.year.start_date);
    let to = params.date("to")?.unwrap_or(ctx.year.end_date);
    r.param("param.academic_year", ctx.year.name.clone());
    r.param("param.from", format_date(from));
    r.param("param.to", format_date(to));
    r.columns = vec![
        Column::text(tr(lang, "lib.col.returned")),
        Column::text(tr(lang, "lib.col.accession")),
        Column::wide(tr(lang, "lib.col.title")),
        Column::wide(tr(lang, "col.name")),
        Column::number(tr(lang, "lib.col.fine")),
        Column::number(tr(lang, "lib.col.waived")),
        Column::number(tr(lang, "lib.col.collected")),
        Column::text(tr(lang, "lib.col.how")),
    ];
    let loans = repo::list_loans(
        conn,
        ctx.school.id,
        &repo::LoanFilter {
            state: Some("returned".into()),
            ..Default::default()
        },
        to,
    )
    .await?;
    let mut g = Group::default();
    let mut total = Decimal::ZERO;
    for v in loans {
        let l = &v.loan;
        let settled = l.fine_settled_on.is_some() || l.fine_voucher_id.is_some();
        let Some(ret) = l.returned_on else { continue };
        if l.fine <= Decimal::ZERO || !settled || ret < from || ret > to {
            continue;
        }
        let collected = (l.fine - l.fine_waived).max(Decimal::ZERO);
        total += collected;
        g.rows.push(vec![
            format_date(ret),
            v.accession_no,
            v.title,
            v.member_name,
            money(l.fine),
            money(l.fine_waived),
            money(collected),
            tr(
                lang,
                if l.fine_voucher_id.is_some() {
                    "lib.how.voucher"
                } else {
                    "lib.how.desk"
                },
            )
            .into(),
        ]);
    }
    r.push_group(g);
    r.total = Some(vec![
        tr(lang, "tt.total").into(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        money(total),
        String::new(),
    ]);
    Ok(r)
}

/// Titles by loans issued in a date range.
pub async fn most_borrowed(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "library.most_borrowed",
        lang,
        &ctx.school.name,
        tr(lang, "report.library.most_borrowed"),
    );
    let from = params.date("from")?.unwrap_or(ctx.year.start_date);
    let to = params.date("to")?.unwrap_or(ctx.year.end_date);
    let top = params.number("top", 20)?.max(1);
    r.param("param.from", format_date(from));
    r.param("param.to", format_date(to));
    r.columns = vec![
        Column::wide(tr(lang, "lib.col.title")),
        Column::text(tr(lang, "lib.col.author")),
        Column::text(tr(lang, "lib.col.category")),
        Column::number(tr(lang, "lib.col.loans")),
    ];
    let rows = repo::most_borrowed(conn, ctx.school.id, from, to, i64::from(top)).await?;
    let mut g = Group::default();
    for row in rows {
        g.rows.push(vec![
            row.title,
            row.author,
            row.category,
            row.loans.to_string(),
        ]);
    }
    r.push_group(g);
    Ok(r)
}

/// Copies by category and status.
pub async fn stock(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    _params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "library.stock",
        lang,
        &ctx.school.name,
        tr(lang, "report.library.stock"),
    );
    r.columns = vec![
        Column::wide(tr(lang, "lib.col.category")),
        Column::number(tr(lang, "lib.col.titles")),
        Column::number(tr(lang, "lib.col.copies")),
        Column::number(tr(lang, "lib.col.available")),
        Column::number(tr(lang, "lib.col.on_loan")),
        Column::number(tr(lang, "lib.col.lost")),
        Column::number(tr(lang, "lib.col.withdrawn")),
    ];
    let rows = repo::stock(conn, ctx.school.id).await?;
    let mut g = Group::default();
    let mut t = [0i64; 6];
    for row in rows {
        let cells = [
            row.titles,
            row.copies,
            row.available,
            row.on_loan,
            row.lost,
            row.withdrawn,
        ];
        for (a, b) in t.iter_mut().zip(cells) {
            *a += b;
        }
        let mut line = vec![if row.category.is_empty() {
            tr(lang, "lib.no_category").to_string()
        } else {
            row.category
        }];
        line.extend(cells.iter().map(i64::to_string));
        g.rows.push(line);
    }
    r.push_group(g);
    let mut total = vec![tr(lang, "tt.total").to_string()];
    total.extend(t.iter().map(i64::to_string));
    r.total = Some(total);
    Ok(r)
}
