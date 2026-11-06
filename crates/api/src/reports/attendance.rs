//! Attendance reports (M2 stage 2). Teachers run them for their own sections only.

use chrono::{Datelike, NaiveDate};
use madrasa_core::attendance as core_att;
use madrasa_db::reports as q;
use rust_decimal::Decimal;
use sqlx::PgConnection;

use super::students::{class_heading, opt, scope_params};
use super::text::{tr, value};
use super::{format_date, pct, Column, Context, Group, Params, Report};
use crate::ApiError;

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

fn percent(present: i64, late: i64, marked: i64) -> Option<Decimal> {
    (marked > 0).then(|| {
        (Decimal::from(present + late) / Decimal::from(marked) * Decimal::from(100)).round_dp(1)
    })
}

/// One section, one month: a column per day.
pub async fn register(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "attendance.register",
        lang,
        &ctx.school.name,
        tr(lang, "report.attendance.register"),
    );
    r.landscape = true;
    let (y, m) = params
        .month("month")?
        .ok_or_else(|| ApiError::validation("month is required."))?;
    let first = NaiveDate::from_ymd_opt(y, m, 1)
        .ok_or_else(|| ApiError::validation("month is invalid."))?;
    let (_, section_id) = scope_params(conn, ctx, params, &mut r, true).await?;
    let section_id = section_id
        .filter(|s| !s.is_nil())
        .ok_or_else(|| ApiError::validation("section_id is required."))?;
    r.param("param.month", format!("{y}-{m:02}"));
    let (weekend, holidays) = madrasa_db::attendance::calendar(conn, ctx.school.id).await?;
    let days = core_att::month_days(first);
    let last = *days.last().unwrap_or(&first);
    let entries = madrasa_db::attendance::section_entries(conn, section_id, first, last).await?;
    let roster = madrasa_db::attendance::roster(conn, section_id, first).await?;
    let kinds: Vec<core_att::DayKind> = days
        .iter()
        .map(|d| core_att::day_kind(*d, &weekend, &holidays))
        .collect();
    let marked: std::collections::HashSet<NaiveDate> = entries.iter().map(|e| e.date).collect();
    r.columns = vec![
        Column::number(tr(lang, "col.roll_no")),
        Column::wide(tr(lang, "col.name")),
    ];
    for d in &days {
        r.columns.push(Column {
            label: d.day().to_string(),
            align: super::Align::Center,
            stretch: false,
        });
    }
    r.columns.push(Column::number(tr(lang, "col.present")));
    r.columns.push(Column::number(tr(lang, "col.absent")));
    r.columns.push(Column::number(tr(lang, "col.late")));
    r.columns.push(Column::number(tr(lang, "col.leave")));
    r.columns
        .push(Column::number(tr(lang, "col.attendance_percent")));
    let mut by_student: std::collections::HashMap<uuid::Uuid, Vec<Option<String>>> = roster
        .iter()
        .map(|s| (s.student_id, vec![None; days.len()]))
        .collect();
    for e in entries {
        let idx = (e.date - first).num_days();
        if idx >= 0 {
            if let Some(row) = by_student.get_mut(&e.student_id) {
                if let Some(cell) = row.get_mut(idx as usize) {
                    *cell = Some(e.status);
                }
            }
        }
    }
    let mut g = Group::default();
    let mut day_present = vec![0i64; days.len()];
    for s in &roster {
        let statuses = by_student.remove(&s.student_id).unwrap_or_default();
        let (mut p, mut a, mut l, mut lv, mut marked_days) = (0i64, 0i64, 0i64, 0i64, 0i64);
        let mut row = vec![opt(s.roll_no), s.full_name.clone()];
        for (i, st) in statuses.iter().enumerate() {
            let cell = match st.as_deref() {
                Some("present") => {
                    p += 1;
                    marked_days += 1;
                    day_present[i] += 1;
                    "P"
                }
                Some("absent") => {
                    a += 1;
                    marked_days += 1;
                    "A"
                }
                Some("late") => {
                    l += 1;
                    marked_days += 1;
                    day_present[i] += 1;
                    "L"
                }
                Some("leave") => {
                    lv += 1;
                    marked_days += 1;
                    "Lv"
                }
                Some(_) => "?",
                None if !kinds[i].is_school_day() => "H",
                None if marked.contains(&days[i]) => "—",
                None => "",
            };
            row.push(cell.into());
        }
        row.push(p.to_string());
        row.push(a.to_string());
        row.push(l.to_string());
        row.push(lv.to_string());
        row.push(pct(percent(p, l, marked_days)));
        g.rows.push(row);
    }
    r.push_group(g);
    let mut t = vec![String::new(); r.columns.len()];
    t[1] = tr(lang, "col.present").into();
    for (i, n) in day_present.iter().enumerate() {
        t[2 + i] = if marked.contains(&days[i]) {
            n.to_string()
        } else {
            String::new()
        };
    }
    r.total = Some(t);
    r.notes.push(tr(lang, "note.register_legend").into());
    r.notes.push(tr(lang, "note.attendance_percent").into());
    Ok(r)
}

pub async fn percent_by_section(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "attendance.percent",
        lang,
        &ctx.school.name,
        tr(lang, "report.attendance.percent"),
    );
    let (from, to) = range(params)?;
    let (class_id, _) = scope_params(conn, ctx, params, &mut r, false).await?;
    r.param("param.from", format_date(from));
    r.param("param.to", format_date(to));
    let rows = q::attendance_by_section(
        conn,
        ctx.school.id,
        ctx.year.id,
        from,
        to,
        class_id,
        ctx.section_scope.as_deref(),
    )
    .await?;
    r.columns = vec![
        Column::text(tr(lang, "col.class")),
        Column::text(tr(lang, "col.section")),
        Column::number(tr(lang, "col.enrolled")),
        Column::number(tr(lang, "col.days_marked")),
        Column::number(tr(lang, "col.present")),
        Column::number(tr(lang, "col.absent")),
        Column::number(tr(lang, "col.late")),
        Column::number(tr(lang, "col.leave")),
        Column::number(tr(lang, "col.attendance_percent")),
    ];
    let mut g = Group::default();
    let mut sums = [0i64; 6];
    for row in rows {
        let marked = row.present + row.absent + row.late + row.leave;
        for (s, v) in sums.iter_mut().zip([
            row.enrolled,
            row.days_marked,
            row.present,
            row.absent,
            row.late,
            row.leave,
        ]) {
            *s += v;
        }
        g.rows.push(vec![
            row.class_name,
            row.section_name,
            row.enrolled.to_string(),
            row.days_marked.to_string(),
            row.present.to_string(),
            row.absent.to_string(),
            row.late.to_string(),
            row.leave.to_string(),
            pct(percent(row.present, row.late, marked)),
        ]);
    }
    r.push_group(g);
    let marked = sums[2] + sums[3] + sums[4] + sums[5];
    r.total = Some(vec![
        tr(lang, "common.total").into(),
        String::new(),
        sums[0].to_string(),
        String::new(),
        sums[2].to_string(),
        sums[3].to_string(),
        sums[4].to_string(),
        sums[5].to_string(),
        pct(percent(sums[2], sums[4], marked)),
    ]);
    r.notes.push(tr(lang, "note.attendance_percent").into());
    Ok(r)
}

pub async fn absentees(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "attendance.absentees",
        lang,
        &ctx.school.name,
        tr(lang, "report.attendance.absentees"),
    );
    r.landscape = true;
    let (from, to) = range(params)?;
    let (class_id, section_id) = scope_params(conn, ctx, params, &mut r, true).await?;
    r.param("param.from", format_date(from));
    r.param("param.to", format_date(to));
    let rows = q::absentees_between(
        conn,
        ctx.school.id,
        from,
        to,
        class_id,
        section_id,
        ctx.section_scope.as_deref(),
    )
    .await?;
    r.columns = vec![
        Column::text(tr(lang, "col.date")),
        Column::text(tr(lang, "col.student_number")),
        Column::wide(tr(lang, "col.name")),
        Column::wide(tr(lang, "col.father_name")),
        Column::text(tr(lang, "col.class")),
        Column::number(tr(lang, "col.roll_no")),
        Column::text(tr(lang, "col.status")),
        Column::wide(tr(lang, "col.note")),
        Column::text(tr(lang, "col.phone")),
    ];
    let mut groups: Vec<Group> = Vec::new();
    for row in rows {
        let heading = format_date(row.date);
        if groups.last().map(|g| g.heading.as_deref()) != Some(Some(heading.as_str())) {
            groups.push(Group {
                heading: Some(heading),
                ..Default::default()
            });
        }
        groups.last_mut().expect("group").rows.push(vec![
            format_date(row.date),
            row.student_number,
            row.full_name,
            row.father_name,
            format!("{}-{}", row.class_name, row.section_name),
            opt(row.roll_no),
            value(lang, &row.status),
            row.note.unwrap_or_default(),
            row.guardian_phone.unwrap_or_default(),
        ]);
    }
    for mut g in groups {
        let mut sub = vec![String::new(); r.columns.len()];
        sub[1] = tr(lang, "subtotal").into();
        sub[2] = g.rows.len().to_string();
        g.subtotal = Some(sub);
        r.push_group(g);
    }
    Ok(r)
}

pub async fn below(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "attendance.below",
        lang,
        &ctx.school.name,
        tr(lang, "report.attendance.below"),
    );
    r.landscape = true;
    let (from, to) = range(params)?;
    let threshold = params.number("threshold", 75)?;
    if threshold > 100 {
        return Err(ApiError::validation("threshold must be between 0 and 100."));
    }
    let (class_id, section_id) = scope_params(conn, ctx, params, &mut r, true).await?;
    r.param("param.from", format_date(from));
    r.param("param.to", format_date(to));
    r.param("param.threshold", threshold.to_string());
    let rows = q::student_attendance_totals(
        conn,
        ctx.school.id,
        ctx.year.id,
        from,
        to,
        class_id,
        section_id,
        ctx.section_scope.as_deref(),
    )
    .await?;
    r.columns = vec![
        Column::text(tr(lang, "col.student_number")),
        Column::wide(tr(lang, "col.name")),
        Column::wide(tr(lang, "col.father_name")),
        Column::number(tr(lang, "col.roll_no")),
        Column::number(tr(lang, "col.present")),
        Column::number(tr(lang, "col.absent")),
        Column::number(tr(lang, "col.late")),
        Column::number(tr(lang, "col.leave")),
        Column::number(tr(lang, "col.attendance_percent")),
        Column::text(tr(lang, "col.phone")),
    ];
    let limit = Decimal::from(threshold);
    let mut groups: Vec<Group> = Vec::new();
    for row in rows {
        let marked = row.present + row.absent + row.late + row.leave;
        let Some(p) = percent(row.present, row.late, marked) else {
            continue;
        };
        if p >= limit {
            continue;
        }
        let heading = class_heading(lang, Some(&row.class_name), Some(&row.section_name));
        if groups.last().map(|g| g.heading.as_deref()) != Some(Some(heading.as_str())) {
            groups.push(Group {
                heading: Some(heading),
                ..Default::default()
            });
        }
        groups.last_mut().expect("group").rows.push(vec![
            row.student_number,
            row.full_name,
            row.father_name,
            opt(row.roll_no),
            row.present.to_string(),
            row.absent.to_string(),
            row.late.to_string(),
            row.leave.to_string(),
            pct(Some(p)),
            row.guardian_phone.unwrap_or_default(),
        ]);
    }
    for g in groups {
        r.push_group(g);
    }
    r.notes.push(tr(lang, "note.attendance_percent").into());
    Ok(r)
}

pub async fn staff(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "attendance.staff",
        lang,
        &ctx.school.name,
        tr(lang, "report.attendance.staff"),
    );
    let (y, m) = params
        .month("month")?
        .ok_or_else(|| ApiError::validation("month is required."))?;
    let first = NaiveDate::from_ymd_opt(y, m, 1)
        .ok_or_else(|| ApiError::validation("month is invalid."))?;
    let days = core_att::month_days(first);
    let last = *days.last().unwrap_or(&first);
    r.param("param.month", format!("{y}-{m:02}"));
    let rows = q::staff_attendance_totals(conn, ctx.school.id, first, last).await?;
    r.columns = vec![
        Column::text(tr(lang, "col.staff_number")),
        Column::wide(tr(lang, "col.name")),
        Column::text(tr(lang, "col.designation")),
        Column::number(tr(lang, "col.present")),
        Column::number(tr(lang, "col.absent")),
        Column::number(tr(lang, "col.late")),
        Column::number(tr(lang, "col.leave")),
        Column::number(tr(lang, "col.attendance_percent")),
    ];
    let mut g = Group::default();
    for row in rows {
        let marked = row.present + row.absent + row.late + row.leave;
        g.rows.push(vec![
            row.staff_number,
            row.full_name,
            row.designation,
            row.present.to_string(),
            row.absent.to_string(),
            row.late.to_string(),
            row.leave.to_string(),
            pct(percent(row.present, row.late, marked)),
        ]);
    }
    r.push_group(g);
    Ok(r)
}
