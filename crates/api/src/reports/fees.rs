//! Fee reports (M2 stage 2). Money columns carry the school's currency code in the label.

use chrono::{Datelike, NaiveDate};
use madrasa_db::reports as q;
use rust_decimal::Decimal;
use sqlx::PgConnection;

use super::students::{class_heading, opt, scope_params};
use super::text::{tr, value};
use super::{format_date, money, Column, Context, Group, Params, Report};
use crate::ApiError;

fn amount_col(ctx: &Context<'_>, key: &str) -> Column {
    Column::number(format!("{} ({})", tr(ctx.lang, key), ctx.school.currency))
}

fn range(params: &Params) -> Result<(NaiveDate, NaiveDate), ApiError> {
    let from = params
        .date("from")?
        .ok_or_else(|| ApiError::validation("from is required."))?;
    let to = params
        .date("to")?
        .ok_or_else(|| ApiError::validation("to is required."))?;
    if to < from {
        return Err(ApiError::validation("to must not be before from."));
    }
    Ok((from, to))
}

fn month_label(d: NaiveDate) -> String {
    format!("{}-{:02}", d.year(), d.month())
}

/// Every receipt in a period, grouped by day with a daily subtotal.
pub async fn collection(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "fees.collection",
        lang,
        &ctx.school.name,
        tr(lang, "report.fees.collection"),
    );
    r.landscape = true;
    let (from, to) = range(params)?;
    let class_id = params.uuid("class_id")?;
    r.param("param.from", format_date(from));
    r.param("param.to", format_date(to));
    if let Some(cid) = class_id {
        let class = madrasa_db::academics::get_class(conn, cid).await?;
        r.param("param.class", class.name);
    }
    let rows = q::receipts_between(conn, ctx.school.id, from, to, class_id).await?;
    r.columns = vec![
        Column::text(tr(lang, "col.date")),
        Column::text(tr(lang, "col.receipt_no")),
        Column::text(tr(lang, "col.student_number")),
        Column::wide(tr(lang, "col.name")),
        Column::text(tr(lang, "col.class")),
        Column::text(tr(lang, "col.voucher_no")),
        Column::text(tr(lang, "col.billing_month")),
        Column::text(tr(lang, "col.method")),
        Column::text(tr(lang, "col.reference")),
        amount_col(ctx, "col.amount"),
        Column::text(tr(lang, "col.received_by")),
    ];
    let mut groups: Vec<(NaiveDate, Group, Decimal)> = Vec::new();
    let mut total = Decimal::ZERO;
    for row in rows {
        if groups.last().map(|g| g.0) != Some(row.paid_on) {
            groups.push((
                row.paid_on,
                Group {
                    heading: Some(format_date(row.paid_on)),
                    ..Default::default()
                },
                Decimal::ZERO,
            ));
        }
        let g = groups.last_mut().expect("group");
        g.2 += row.amount;
        total += row.amount;
        let class = match (&row.class_name, &row.section_name) {
            (Some(c), Some(s)) => format!("{c}-{s}"),
            _ => String::new(),
        };
        g.1.rows.push(vec![
            format_date(row.paid_on),
            row.receipt_no,
            row.student_number,
            row.student_name,
            class,
            row.voucher_no,
            month_label(row.billing_month),
            if row.is_reversal {
                tr(lang, "value.reversal").into()
            } else {
                value(lang, &row.method)
            },
            row.reference.unwrap_or_default(),
            money(row.amount),
            row.received_by,
        ]);
    }
    for (_, mut g, sum) in groups {
        let mut sub = vec![String::new(); r.columns.len()];
        sub[1] = tr(lang, "subtotal").into();
        sub[2] = g.rows.len().to_string();
        sub[9] = money(sum);
        g.subtotal = Some(sub);
        r.push_group(g);
    }
    let mut t = vec![String::new(); r.columns.len()];
    t[1] = tr(lang, "common.total").into();
    t[2] = r.row_count.to_string();
    t[9] = money(total);
    r.total = Some(t);
    Ok(r)
}

/// Net collection per calendar month.
pub async fn monthly(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "fees.monthly",
        lang,
        &ctx.school.name,
        tr(lang, "report.fees.monthly"),
    );
    let (from, to) = range(params)?;
    r.param("param.from", format_date(from));
    r.param("param.to", format_date(to));
    let rows = q::collection_by_month(conn, ctx.school.id, from, to).await?;
    r.columns = vec![
        Column::text(tr(lang, "col.month")),
        Column::number(tr(lang, "col.receipts")),
        Column::number(tr(lang, "col.reversals")),
        amount_col(ctx, "col.amount"),
    ];
    let mut g = Group::default();
    let (mut receipts, mut reversals, mut total) = (0i64, 0i64, Decimal::ZERO);
    for row in rows {
        receipts += row.receipts;
        reversals += row.reversals;
        total += row.amount;
        g.rows.push(vec![
            month_label(row.month),
            row.receipts.to_string(),
            row.reversals.to_string(),
            money(row.amount),
        ]);
    }
    r.push_group(g);
    r.total = Some(vec![
        tr(lang, "common.total").into(),
        receipts.to_string(),
        reversals.to_string(),
        money(total),
    ]);
    Ok(r)
}

fn bucket(days: i32) -> &'static str {
    match days {
        d if d <= 30 => "bucket.1_30",
        d if d <= 60 => "bucket.31_60",
        d if d <= 90 => "bucket.61_90",
        _ => "bucket.90_plus",
    }
}

/// Defaulters grouped by class, with an ageing column and bucket totals in the notes.
pub async fn defaulters(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "fees.defaulters",
        lang,
        &ctx.school.name,
        tr(lang, "report.fees.defaulters"),
    );
    r.landscape = true;
    let as_of = params.date("as_of")?.unwrap_or_else(crate::clock::today);
    let min_days = params.number("min_days", 1)?.max(1) as i32;
    let class_id = params.uuid("class_id")?;
    r.param("param.as_of", format_date(as_of));
    r.param("param.min_days", min_days.to_string());
    if let Some(cid) = class_id {
        let class = madrasa_db::academics::get_class(conn, cid).await?;
        r.param("param.class", class.name);
    } else {
        r.param("param.class", tr(lang, "param.all"));
    }
    let mut rows =
        madrasa_db::fees::defaulters(conn, ctx.school.id, as_of, class_id, min_days).await?;
    rows.sort_by(|a, b| {
        a.class_name
            .cmp(&b.class_name)
            .then(a.section_name.cmp(&b.section_name))
            .then(b.days_overdue.cmp(&a.days_overdue))
    });
    r.columns = vec![
        Column::text(tr(lang, "col.student_number")),
        Column::wide(tr(lang, "col.name")),
        Column::wide(tr(lang, "col.father_name")),
        Column::text(tr(lang, "col.section")),
        Column::text(tr(lang, "col.phone")),
        Column::text(tr(lang, "col.oldest_due")),
        Column::number(tr(lang, "col.days_overdue")),
        Column::text(tr(lang, "col.kind")),
        Column::number(tr(lang, "col.unpaid_vouchers")),
        amount_col(ctx, "col.balance"),
    ];
    let mut buckets: [(usize, Decimal); 4] = [(0, Decimal::ZERO); 4];
    let mut groups: Vec<Group> = Vec::new();
    let mut total = Decimal::ZERO;
    for row in rows {
        let heading = class_heading(lang, row.class_name.as_deref(), None);
        if groups.last().map(|g| g.heading.as_deref()) != Some(Some(heading.as_str())) {
            groups.push(Group {
                heading: Some(heading),
                ..Default::default()
            });
        }
        let b = bucket(row.days_overdue);
        let idx = [
            "bucket.1_30",
            "bucket.31_60",
            "bucket.61_90",
            "bucket.90_plus",
        ]
        .iter()
        .position(|k| *k == b)
        .unwrap_or(3);
        buckets[idx].0 += 1;
        buckets[idx].1 += row.balance;
        total += row.balance;
        groups.last_mut().expect("group").rows.push(vec![
            row.student_number,
            row.student_name,
            row.father_name,
            row.section_name.unwrap_or_default(),
            row.guardian_phone.unwrap_or_default(),
            format_date(row.oldest_due_date),
            row.days_overdue.to_string(),
            tr(lang, b).into(),
            row.unpaid_vouchers.to_string(),
            money(row.balance),
        ]);
    }
    for mut g in groups {
        let sum: Decimal = g
            .rows
            .iter()
            .filter_map(|r| r[9].replace(',', "").parse::<Decimal>().ok())
            .sum();
        let mut sub = vec![String::new(); r.columns.len()];
        sub[0] = tr(lang, "subtotal").into();
        sub[1] = g.rows.len().to_string();
        sub[9] = money(sum);
        g.subtotal = Some(sub);
        r.push_group(g);
    }
    let mut t = vec![String::new(); r.columns.len()];
    t[0] = tr(lang, "common.total").into();
    t[1] = r.row_count.to_string();
    t[9] = money(total);
    r.total = Some(t);
    for (i, key) in [
        "bucket.1_30",
        "bucket.31_60",
        "bucket.61_90",
        "bucket.90_plus",
    ]
    .iter()
    .enumerate()
    {
        r.notes.push(format!(
            "{}: {} · {} {}",
            tr(lang, key),
            buckets[i].0,
            money(buckets[i].1),
            ctx.school.currency
        ));
    }
    Ok(r)
}

/// One student's vouchers and receipts with a running balance.
pub async fn ledger(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "fees.ledger",
        lang,
        &ctx.school.name,
        tr(lang, "report.fees.ledger"),
    );
    r.landscape = true;
    let number = params
        .get("student_number")
        .ok_or_else(|| ApiError::validation("student_number is required."))?;
    let student = madrasa_db::students::find_by_number(conn, ctx.school.id, number)
        .await?
        .ok_or_else(|| ApiError::not_found("student"))?;
    r.subtitle = Some(format!(
        "{} · {} · {}",
        student.student_number, student.full_name, student.father_name
    ));
    r.param("param.student_number", student.student_number.clone());
    let vouchers = madrasa_db::fees::vouchers_for_student(conn, student.id).await?;
    let receipts = madrasa_db::fees::receipts_for_student(conn, student.id).await?;
    r.columns = vec![
        Column::text(tr(lang, "col.date")),
        Column::wide(tr(lang, "col.kind")),
        Column::text(tr(lang, "col.voucher_no")),
        Column::text(tr(lang, "col.status")),
        amount_col(ctx, "col.billed"),
        amount_col(ctx, "col.discounts"),
        amount_col(ctx, "col.late_fee"),
        amount_col(ctx, "col.paid"),
        amount_col(ctx, "col.balance"),
    ];
    // Entries in date order: vouchers on their issue date, receipts on their payment date.
    enum Entry<'a> {
        V(&'a madrasa_db::models::FeeVoucher),
        R(&'a madrasa_db::models::FeeReceipt),
    }
    let mut entries: Vec<(NaiveDate, usize, Entry<'_>)> = Vec::new();
    for (i, v) in vouchers
        .iter()
        .filter(|v| v.status != "cancelled")
        .enumerate()
    {
        entries.push((v.issue_date, i, Entry::V(v)));
    }
    for (i, rc) in receipts.iter().enumerate() {
        entries.push((rc.paid_on, 10_000 + i, Entry::R(rc)));
    }
    entries.sort_by_key(|(d, i, _)| (*d, *i));
    let mut balance = Decimal::ZERO;
    let (mut billed, mut discounts, mut late, mut paid) =
        (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO, Decimal::ZERO);
    let by_id: std::collections::HashMap<_, _> = vouchers.iter().map(|v| (v.id, v)).collect();
    let mut g = Group::default();
    for (date, _, e) in entries {
        match e {
            Entry::V(v) => {
                let disc = v.sibling_discount + v.concession_discount;
                // Arrears are already in the earlier vouchers' balances; count the month's own charge.
                let charge = v.net_monthly + v.late_fee;
                balance += charge;
                billed += v.gross;
                discounts += disc;
                late += v.late_fee;
                g.rows.push(vec![
                    format_date(date),
                    format!(
                        "{} {}",
                        tr(lang, "col.billing_month"),
                        month_label(v.billing_month)
                    ),
                    v.voucher_no.clone(),
                    value(lang, &v.status),
                    money(v.gross),
                    money(disc),
                    money(v.late_fee),
                    String::new(),
                    money(balance),
                ]);
            }
            Entry::R(rc) => {
                balance -= rc.amount;
                paid += rc.amount;
                let vno = by_id
                    .get(&rc.voucher_id)
                    .map(|v| v.voucher_no.clone())
                    .unwrap_or_default();
                g.rows.push(vec![
                    format_date(date),
                    if rc.reverses_id.is_some() {
                        format!("{} {}", tr(lang, "value.reversal"), rc.receipt_no)
                    } else {
                        format!(
                            "{} {} · {}",
                            tr(lang, "col.receipt_no"),
                            rc.receipt_no,
                            value(lang, &rc.method)
                        )
                    },
                    vno,
                    String::new(),
                    String::new(),
                    String::new(),
                    String::new(),
                    money(rc.amount),
                    money(balance),
                ]);
            }
        }
    }
    r.push_group(g);
    r.total = Some(vec![
        tr(lang, "common.total").into(),
        String::new(),
        String::new(),
        String::new(),
        money(billed),
        money(discounts),
        money(late),
        money(paid),
        money(balance),
    ]);
    Ok(r)
}

pub async fn concessions(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "fees.concessions",
        lang,
        &ctx.school.name,
        tr(lang, "report.fees.concessions"),
    );
    r.landscape = true;
    let (from, to) = range(params)?;
    let (class_id, _) = scope_params(conn, ctx, params, &mut r, false).await?;
    r.param("param.from", format_date(from));
    r.param("param.to", format_date(to));
    let rows =
        q::concessions_register(conn, ctx.school.id, ctx.year.id, from, to, class_id).await?;
    r.columns = vec![
        Column::text(tr(lang, "col.student_number")),
        Column::wide(tr(lang, "col.name")),
        Column::text(tr(lang, "col.class")),
        Column::text(tr(lang, "col.concession")),
        Column::number(tr(lang, "col.percent")),
        amount_col(ctx, "col.amount"),
        Column::wide(tr(lang, "col.reason")),
        Column::text(tr(lang, "col.approved_by")),
        Column::text(tr(lang, "col.valid_from")),
        Column::text(tr(lang, "col.valid_to")),
    ];
    let mut g = Group::default();
    for row in rows {
        g.rows.push(vec![
            row.student_number,
            row.student_name,
            match (&row.class_name, &row.section_name) {
                (Some(c), Some(s)) => format!("{c}-{s}"),
                _ => String::new(),
            },
            value(lang, &row.kind),
            row.percent.map(|p| format!("{:.0}", p)).unwrap_or_default(),
            row.amount.map(money).unwrap_or_default(),
            row.reason,
            row.approved_by,
            format_date(row.valid_from),
            opt(row.valid_to.map(format_date)),
        ]);
    }
    r.push_group(g);
    Ok(r)
}

pub async fn vouchers(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "fees.vouchers",
        lang,
        &ctx.school.name,
        tr(lang, "report.fees.vouchers"),
    );
    r.landscape = true;
    let (y, m) = params
        .month("month")?
        .ok_or_else(|| ApiError::validation("month is required."))?;
    let month = NaiveDate::from_ymd_opt(y, m, 1)
        .ok_or_else(|| ApiError::validation("month is invalid."))?;
    let (class_id, section_id) = scope_params(conn, ctx, params, &mut r, true).await?;
    r.param("param.month", month_label(month));
    let rows = madrasa_db::fees::list_vouchers(
        conn,
        ctx.school.id,
        &madrasa_db::fees::VoucherFilter {
            billing_month: Some(month),
            class_id,
            section_id,
            status: None,
            student_id: None,
            q: None,
        },
        100_000,
    )
    .await?;
    r.columns = vec![
        Column::text(tr(lang, "col.voucher_no")),
        Column::text(tr(lang, "col.student_number")),
        Column::wide(tr(lang, "col.name")),
        Column::text(tr(lang, "col.due_date")),
        amount_col(ctx, "col.gross"),
        amount_col(ctx, "col.discounts"),
        amount_col(ctx, "col.arrears"),
        amount_col(ctx, "col.total_due"),
        amount_col(ctx, "col.late_fee"),
        amount_col(ctx, "col.paid"),
        amount_col(ctx, "col.balance"),
        Column::text(tr(lang, "col.status")),
    ];
    let mut groups: Vec<Group> = Vec::new();
    let mut sums = [Decimal::ZERO; 7];
    let mut rows_sorted = rows;
    rows_sorted.sort_by(|a, b| {
        a.class_name
            .cmp(&b.class_name)
            .then(a.section_name.cmp(&b.section_name))
            .then(a.student_name.cmp(&b.student_name))
    });
    for row in rows_sorted {
        let heading = class_heading(lang, row.class_name.as_deref(), row.section_name.as_deref());
        if groups.last().map(|g| g.heading.as_deref()) != Some(Some(heading.as_str())) {
            groups.push(Group {
                heading: Some(heading),
                ..Default::default()
            });
        }
        let disc = row.sibling_discount + row.concession_discount;
        let balance = if row.status == "cancelled" {
            Decimal::ZERO
        } else {
            row.total_due + row.late_fee - row.net_paid
        };
        let vals = [
            row.gross,
            disc,
            row.arrears,
            row.total_due,
            row.late_fee,
            row.net_paid,
            balance,
        ];
        if row.status != "cancelled" {
            for (s, v) in sums.iter_mut().zip(vals) {
                *s += v;
            }
        }
        groups.last_mut().expect("group").rows.push(vec![
            row.voucher_no,
            row.student_number,
            row.student_name,
            format_date(row.due_date),
            money(row.gross),
            money(disc),
            money(row.arrears),
            money(row.total_due),
            money(row.late_fee),
            money(row.net_paid),
            money(balance),
            value(lang, &row.status),
        ]);
    }
    for g in groups {
        r.push_group(g);
    }
    let mut t = vec![String::new(); r.columns.len()];
    t[0] = tr(lang, "common.total").into();
    t[1] = r.row_count.to_string();
    for (i, s) in sums.iter().enumerate() {
        t[4 + i] = money(*s);
    }
    r.total = Some(t);
    Ok(r)
}

pub async fn late_fees(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "fees.late_fees",
        lang,
        &ctx.school.name,
        tr(lang, "report.fees.late_fees"),
    );
    r.landscape = true;
    let (from, to) = range(params)?;
    let from_month = NaiveDate::from_ymd_opt(from.year(), from.month(), 1).expect("first of month");
    let to_month = NaiveDate::from_ymd_opt(to.year(), to.month(), 1).expect("first of month");
    let class_id = params.uuid("class_id")?;
    r.param("param.from", month_label(from_month));
    r.param("param.to", month_label(to_month));
    if let Some(cid) = class_id {
        let class = madrasa_db::academics::get_class(conn, cid).await?;
        r.param("param.class", class.name);
    }
    let rows = q::late_fees(conn, ctx.school.id, from_month, to_month, class_id).await?;
    r.columns = vec![
        Column::text(tr(lang, "col.billing_month")),
        Column::text(tr(lang, "col.voucher_no")),
        Column::text(tr(lang, "col.student_number")),
        Column::wide(tr(lang, "col.name")),
        Column::text(tr(lang, "col.class")),
        Column::text(tr(lang, "col.due_date")),
        amount_col(ctx, "col.late_fee"),
        Column::text(tr(lang, "col.waived")),
        Column::text(tr(lang, "col.waived_by")),
        Column::wide(tr(lang, "col.reason")),
        Column::text(tr(lang, "col.status")),
    ];
    let mut g = Group::default();
    let (mut assessed, mut waived) = (Decimal::ZERO, Decimal::ZERO);
    for row in rows {
        if row.late_fee_waived {
            waived += row.late_fee;
        } else {
            assessed += row.late_fee;
        }
        g.rows.push(vec![
            month_label(row.billing_month),
            row.voucher_no,
            row.student_number,
            row.student_name,
            match (&row.class_name, &row.section_name) {
                (Some(c), Some(s)) => format!("{c}-{s}"),
                _ => String::new(),
            },
            format_date(row.due_date),
            money(row.late_fee),
            if row.late_fee_waived {
                tr(lang, "common.yes").into()
            } else {
                String::new()
            },
            row.waived_by.unwrap_or_default(),
            row.waiver_reason.unwrap_or_default(),
            value(lang, &row.status),
        ]);
    }
    r.push_group(g);
    let mut t = vec![String::new(); r.columns.len()];
    t[0] = tr(lang, "common.total").into();
    t[1] = r.row_count.to_string();
    t[6] = money(assessed);
    t[7] = money(waived);
    r.total = Some(t);
    Ok(r)
}

pub async fn heads(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "fees.heads",
        lang,
        &ctx.school.name,
        tr(lang, "report.fees.heads"),
    );
    let _ = params;
    r.param("param.academic_year", ctx.year.name.clone());
    let rows = q::head_wise(conn, ctx.school.id, ctx.year.id).await?;
    r.columns = vec![
        Column::wide(tr(lang, "col.fee_head")),
        Column::number(tr(lang, "col.vouchers")),
        amount_col(ctx, "col.billed"),
        amount_col(ctx, "col.collected"),
        amount_col(ctx, "col.outstanding"),
        Column::number(tr(lang, "col.percent")),
    ];
    let mut g = Group::default();
    let (mut billed, mut collected) = (Decimal::ZERO, Decimal::ZERO);
    for row in rows {
        billed += row.billed;
        collected += row.collected;
        let pct = if row.billed > Decimal::ZERO {
            format!(
                "{:.1}",
                (row.collected / row.billed * Decimal::from(100)).round_dp(1)
            )
        } else {
            "—".into()
        };
        g.rows.push(vec![
            row.head,
            row.vouchers.to_string(),
            money(row.billed),
            money(row.collected),
            money(row.billed - row.collected),
            pct,
        ]);
    }
    r.push_group(g);
    let pct = if billed > Decimal::ZERO {
        format!(
            "{:.1}",
            (collected / billed * Decimal::from(100)).round_dp(1)
        )
    } else {
        "—".into()
    };
    r.total = Some(vec![
        tr(lang, "common.total").into(),
        String::new(),
        money(billed),
        money(collected),
        money(billed - collected),
        pct,
    ]);
    r.notes.push(tr(lang, "note.head_split").into());
    Ok(r)
}
