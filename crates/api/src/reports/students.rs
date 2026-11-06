//! Student reports (M2 stage 1).

use chrono::Datelike;
use madrasa_db::models::{AcademicYear, School};
use madrasa_db::reports as q;
use madrasa_db::reports::StudentReportFilter;
use sqlx::PgConnection;
use uuid::Uuid;

use super::text::{tr, value};
use super::{format_date, Column, Group, Lang, Params, Report};
use crate::ApiError;

/// What every student report starts from: the school, the year, the scope and the common
/// parameters shown in the header.
pub struct Context<'a> {
    pub school: &'a School,
    pub year: &'a AcademicYear,
    pub lang: Lang,
    /// Teachers: only these sections.
    pub section_scope: Option<Vec<Uuid>>,
}

/// Resolve class and section names for the header and the filter.
async fn scope_params(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
    report: &mut Report,
    with_section: bool,
) -> Result<(Option<Uuid>, Option<Uuid>), ApiError> {
    report.param("param.academic_year", ctx.year.name.clone());
    let class_id = params.uuid("class_id")?;
    let mut section_id = if with_section {
        params.uuid("section_id")?
    } else {
        None
    };
    let mut class_id = class_id;
    if let Some(sid) = section_id {
        let section = madrasa_db::academics::get_section(conn, sid).await?;
        if section.school_id != ctx.school.id {
            return Err(ApiError::not_found("section"));
        }
        class_id = Some(section.class_id);
        let class = madrasa_db::academics::get_class(conn, section.class_id).await?;
        report.param("param.class", class.name);
        report.param("param.section", section.name);
    } else if let Some(cid) = class_id {
        let class = madrasa_db::academics::get_class(conn, cid).await?;
        if class.school_id != ctx.school.id {
            return Err(ApiError::not_found("class"));
        }
        report.param("param.class", class.name);
        if with_section {
            report.param("param.section", tr(ctx.lang, "param.all"));
        }
    } else {
        report.param("param.class", tr(ctx.lang, "param.all"));
    }
    if let Some(scope) = &ctx.section_scope {
        if let Some(sid) = section_id {
            if !scope.contains(&sid) {
                return Err(ApiError::forbidden());
            }
        } else if scope.is_empty() {
            section_id = Some(Uuid::nil());
        }
    }
    Ok((class_id, section_id))
}

fn class_heading(lang: Lang, class: Option<&str>, section: Option<&str>) -> String {
    match (class, section) {
        (Some(c), Some(s)) => format!("{} {c} · {s}", tr(lang, "group.class")),
        (Some(c), None) => format!("{} {c}", tr(lang, "group.class")),
        _ => tr(lang, "common.not_enrolled").to_string(),
    }
}

fn opt<T: ToString>(v: Option<T>) -> String {
    v.map(|x| x.to_string()).unwrap_or_default()
}

pub async fn list(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "students.list",
        lang,
        &ctx.school.name,
        tr(lang, "report.students.list"),
    );
    r.landscape = true;
    let (class_id, section_id) = scope_params(conn, ctx, params, &mut r, true).await?;
    let status = params.get("status").map(str::to_string);
    if let Some(s) = &status {
        if !["active", "left", "graduated"].contains(&s.as_str()) {
            return Err(ApiError::validation(
                "status must be active, left or graduated.",
            ));
        }
    }
    r.param(
        "param.status",
        value(lang, status.as_deref().unwrap_or("active")),
    );
    let rows = q::student_list(
        conn,
        ctx.school.id,
        &StudentReportFilter {
            academic_year_id: ctx.year.id,
            class_id,
            section_id,
            status,
            section_scope: ctx.section_scope.clone(),
        },
    )
    .await?;
    r.columns = vec![
        Column::number(tr(lang, "col.no")),
        Column::text(tr(lang, "col.student_number")),
        Column::wide(tr(lang, "col.name")),
        Column::wide(tr(lang, "col.father_name")),
        Column::text(tr(lang, "col.gender")),
        Column::text(tr(lang, "col.date_of_birth")),
        Column::number(tr(lang, "col.roll_no")),
        Column::text(tr(lang, "col.guardian")),
        Column::text(tr(lang, "col.phone")),
        Column::text(tr(lang, "col.admission_date")),
    ];
    let mut groups: Vec<Group> = Vec::new();
    for row in rows {
        let heading = class_heading(lang, row.class_name.as_deref(), row.section_name.as_deref());
        if groups.last().map(|g| g.heading.as_deref()) != Some(Some(heading.as_str())) {
            groups.push(Group {
                heading: Some(heading),
                ..Default::default()
            });
        }
        let g = groups.last_mut().expect("group");
        let n = g.rows.len() + 1;
        g.rows.push(vec![
            n.to_string(),
            row.student_number,
            row.full_name,
            row.father_name,
            value(lang, &row.gender),
            format_date(row.date_of_birth),
            opt(row.roll_no),
            row.guardian_name.unwrap_or_default(),
            row.guardian_phone.unwrap_or_default(),
            format_date(row.admission_date),
        ]);
    }
    for mut g in groups {
        let mut sub = vec![String::new(); r.columns.len()];
        sub[1] = tr(lang, "subtotal").into();
        sub[2] = g.rows.len().to_string();
        g.subtotal = Some(sub);
        r.push_group(g);
    }
    let mut total = vec![String::new(); r.columns.len()];
    total[1] = tr(lang, "common.total").into();
    total[2] = r.row_count.to_string();
    r.total = Some(total);
    Ok(r)
}

pub async fn strength(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "students.strength",
        lang,
        &ctx.school.name,
        tr(lang, "report.students.strength"),
    );
    let (class_id, _) = scope_params(conn, ctx, params, &mut r, false).await?;
    let rows = q::class_strength(conn, ctx.school.id, ctx.year.id, class_id).await?;
    r.columns = vec![
        Column::text(tr(lang, "col.class")),
        Column::text(tr(lang, "col.section")),
        Column::wide(tr(lang, "col.class_teacher")),
        Column::number(tr(lang, "col.boys")),
        Column::number(tr(lang, "col.girls")),
        Column::number(tr(lang, "col.other")),
        Column::number(tr(lang, "col.total")),
        Column::number(tr(lang, "col.capacity")),
    ];
    let (mut boys, mut girls, mut other, mut total) = (0i64, 0i64, 0i64, 0i64);
    let mut g = Group::default();
    for row in rows {
        boys += row.boys;
        girls += row.girls;
        other += row.other;
        total += row.total;
        g.rows.push(vec![
            row.class_name,
            row.section_name,
            row.class_teacher.unwrap_or_default(),
            row.boys.to_string(),
            row.girls.to_string(),
            row.other.to_string(),
            row.total.to_string(),
            opt(row.capacity),
        ]);
    }
    r.push_group(g);
    r.total = Some(vec![
        tr(lang, "common.total").into(),
        String::new(),
        String::new(),
        boys.to_string(),
        girls.to_string(),
        other.to_string(),
        total.to_string(),
        String::new(),
    ]);
    Ok(r)
}

pub async fn movements(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "students.movements",
        lang,
        &ctx.school.name,
        tr(lang, "report.students.movements"),
    );
    let from = params
        .date("from")?
        .ok_or_else(|| ApiError::validation("from is required."))?;
    let to = params
        .date("to")?
        .ok_or_else(|| ApiError::validation("to is required."))?;
    if to < from {
        return Err(ApiError::validation("to must not be before from."));
    }
    let (class_id, _) = scope_params(conn, ctx, params, &mut r, false).await?;
    r.param("param.from", format_date(from));
    r.param("param.to", format_date(to));
    r.columns = vec![
        Column::text(tr(lang, "col.kind")),
        Column::text(tr(lang, "col.date")),
        Column::text(tr(lang, "col.student_number")),
        Column::wide(tr(lang, "col.name")),
        Column::wide(tr(lang, "col.father_name")),
        Column::text(tr(lang, "col.class")),
        Column::text(tr(lang, "col.section")),
        Column::text(tr(lang, "col.status")),
        Column::wide(tr(lang, "col.reason")),
    ];
    let admissions = q::admissions(conn, ctx.school.id, ctx.year.id, from, to, class_id).await?;
    let withdrawals = q::withdrawals(conn, ctx.school.id, ctx.year.id, from, to, class_id).await?;
    for (key, rows) in [
        ("value.admission", admissions),
        ("value.withdrawal", withdrawals),
    ] {
        let mut g = Group {
            heading: Some(format!("{} ({})", tr(lang, key), rows.len())),
            ..Default::default()
        };
        for row in rows {
            g.rows.push(vec![
                tr(lang, key).into(),
                format_date(row.on_date),
                row.student_number,
                row.full_name,
                row.father_name,
                row.class_name.unwrap_or_default(),
                row.section_name.unwrap_or_default(),
                value(lang, &row.status),
                row.reason.unwrap_or_default(),
            ]);
        }
        r.push_group(g);
    }
    Ok(r)
}

pub async fn contacts(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "students.contacts",
        lang,
        &ctx.school.name,
        tr(lang, "report.students.contacts"),
    );
    r.landscape = true;
    let (class_id, section_id) = scope_params(conn, ctx, params, &mut r, true).await?;
    let rows = q::guardian_contacts(
        conn,
        ctx.school.id,
        &StudentReportFilter {
            academic_year_id: ctx.year.id,
            class_id,
            section_id,
            status: None,
            section_scope: ctx.section_scope.clone(),
        },
    )
    .await?;
    r.columns = vec![
        Column::text(tr(lang, "col.student_number")),
        Column::wide(tr(lang, "col.name")),
        Column::number(tr(lang, "col.roll_no")),
        Column::wide(tr(lang, "col.guardian")),
        Column::text(tr(lang, "col.relationship")),
        Column::text(tr(lang, "col.primary")),
        Column::text(tr(lang, "col.phone")),
        Column::text(tr(lang, "col.email")),
        Column::text(tr(lang, "col.cnic")),
        Column::wide(tr(lang, "col.address")),
    ];
    let mut groups: Vec<Group> = Vec::new();
    for row in rows {
        let heading = class_heading(lang, row.class_name.as_deref(), row.section_name.as_deref());
        if groups.last().map(|g| g.heading.as_deref()) != Some(Some(heading.as_str())) {
            groups.push(Group {
                heading: Some(heading),
                ..Default::default()
            });
        }
        groups.last_mut().expect("group").rows.push(vec![
            row.student_number,
            row.full_name,
            opt(row.roll_no),
            row.guardian_name,
            value(lang, &row.relationship),
            if row.is_primary {
                tr(lang, "common.yes").into()
            } else {
                String::new()
            },
            row.phone.unwrap_or_default(),
            row.email.unwrap_or_default(),
            row.cnic.unwrap_or_default(),
            row.address,
        ]);
    }
    for g in groups {
        r.push_group(g);
    }
    Ok(r)
}

pub async fn birthdays(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "students.birthdays",
        lang,
        &ctx.school.name,
        tr(lang, "report.students.birthdays"),
    );
    let (year, month) = params
        .month("month")?
        .ok_or_else(|| ApiError::validation("month is required."))?;
    let (class_id, _) = scope_params(conn, ctx, params, &mut r, false).await?;
    r.param("param.month", format!("{year}-{month:02}"));
    let rows = q::birthdays(
        conn,
        ctx.school.id,
        ctx.year.id,
        month,
        class_id,
        ctx.section_scope.as_deref(),
    )
    .await?;
    r.columns = vec![
        Column::number(tr(lang, "col.day")),
        Column::text(tr(lang, "col.student_number")),
        Column::wide(tr(lang, "col.name")),
        Column::wide(tr(lang, "col.father_name")),
        Column::text(tr(lang, "col.class")),
        Column::text(tr(lang, "col.section")),
        Column::number(tr(lang, "col.roll_no")),
        Column::text(tr(lang, "col.date_of_birth")),
        Column::number(tr(lang, "col.age")),
    ];
    let mut g = Group::default();
    for row in rows {
        g.rows.push(vec![
            row.date_of_birth.day().to_string(),
            row.student_number,
            row.full_name,
            row.father_name,
            row.class_name.unwrap_or_default(),
            row.section_name.unwrap_or_default(),
            opt(row.roll_no),
            format_date(row.date_of_birth),
            (year - row.date_of_birth.year()).to_string(),
        ]);
    }
    r.push_group(g);
    Ok(r)
}

pub async fn missing(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "students.missing",
        lang,
        &ctx.school.name,
        tr(lang, "report.students.missing"),
    );
    r.landscape = true;
    let (class_id, section_id) = scope_params(conn, ctx, params, &mut r, true).await?;
    let rows = q::missing_data(
        conn,
        ctx.school.id,
        &StudentReportFilter {
            academic_year_id: ctx.year.id,
            class_id,
            section_id,
            status: None,
            section_scope: ctx.section_scope.clone(),
        },
    )
    .await?;
    r.columns = vec![
        Column::text(tr(lang, "col.student_number")),
        Column::wide(tr(lang, "col.name")),
        Column::wide(tr(lang, "col.father_name")),
        Column::text(tr(lang, "col.class")),
        Column::text(tr(lang, "col.section")),
        Column::number(tr(lang, "col.roll_no")),
        Column::text(tr(lang, "col.phone")),
        Column::wide(tr(lang, "col.missing")),
    ];
    let mut g = Group::default();
    for row in rows {
        let mut missing = Vec::new();
        if row.cnic.as_deref().unwrap_or("").is_empty() {
            missing.push(tr(lang, "missing.cnic"));
        }
        if !row.has_photo {
            missing.push(tr(lang, "missing.photo"));
        }
        if row.guardians == 0 {
            missing.push(tr(lang, "missing.guardian"));
        } else if row.guardian_phone.as_deref().unwrap_or("").is_empty() {
            missing.push(tr(lang, "missing.phone"));
        }
        if row.class_name.is_none() {
            missing.push(tr(lang, "missing.enrolment"));
        } else if row.roll_no.is_none() {
            missing.push(tr(lang, "missing.roll_no"));
        }
        if row.address.is_empty() {
            missing.push(tr(lang, "missing.address"));
        }
        g.rows.push(vec![
            row.student_number,
            row.full_name,
            row.father_name,
            row.class_name.unwrap_or_default(),
            row.section_name.unwrap_or_default(),
            opt(row.roll_no),
            row.guardian_phone.unwrap_or_default(),
            missing.join(", "),
        ]);
    }
    r.push_group(g);
    Ok(r)
}
