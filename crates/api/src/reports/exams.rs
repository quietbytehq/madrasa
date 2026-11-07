//! Exam reports (M2 stage 2), built on the result sheets and report cards the exams module
//! already computes, so figures match the screen exactly. Teachers get their own sections.

use std::collections::HashMap;

use madrasa_db::models::Section;
use rust_decimal::Decimal;
use sqlx::PgConnection;
use uuid::Uuid;

use super::students::opt;
use super::text::{tr, value};
use super::{pct, Column, Context, Group, Params, Report};
use crate::auth::CurrentUser;
use crate::routes::exams::{result_sheet_for, ReportCard, ResultSheet};
use crate::ApiError;

/// Sections of the class (or the one section) in scope, in name order.
async fn sections_in_scope(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
    report: &mut Report,
    with_section: bool,
) -> Result<(Vec<Section>, String), ApiError> {
    let class_id = params
        .uuid("class_id")?
        .ok_or_else(|| ApiError::validation("class_id is required."))?;
    let class = madrasa_db::academics::get_class(conn, class_id).await?;
    if class.school_id != ctx.school.id {
        return Err(ApiError::not_found("class"));
    }
    report.param("param.academic_year", ctx.year.name.clone());
    report.param("param.class", class.name.clone());
    let mut sections = madrasa_db::academics::list_sections(
        conn,
        ctx.school.id,
        Some(ctx.year.id),
        Some(class_id),
    )
    .await?;
    if with_section {
        if let Some(sid) = params.uuid("section_id")? {
            let section = sections
                .iter()
                .find(|s| s.id == sid)
                .cloned()
                .ok_or_else(|| ApiError::not_found("section"))?;
            if !ctx.allows(sid) {
                return Err(ApiError::forbidden());
            }
            report.param("param.section", section.name.clone());
            return Ok((vec![section], class.name));
        }
    }
    sections.retain(|s| ctx.allows(s.id));
    sections.sort_by(|a, b| a.name.cmp(&b.name));
    Ok((sections, class.name))
}

async fn assessment_param(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
    report: &mut Report,
) -> Result<Uuid, ApiError> {
    let id = params
        .uuid("assessment_id")?
        .ok_or_else(|| ApiError::validation("assessment_id is required."))?;
    let a = madrasa_db::exams::get_assessment(conn, id).await?;
    if a.school_id != ctx.school.id || a.academic_year_id != ctx.year.id {
        return Err(ApiError::not_found("assessment"));
    }
    report.param("param.assessment", a.name);
    Ok(id)
}

fn d(v: Decimal) -> String {
    let s = v.round_dp(2).normalize().to_string();
    if s == "-0" {
        "0".into()
    } else {
        s
    }
}

fn result_columns(lang: super::Lang, sheet: &ResultSheet) -> Vec<Column> {
    let mut cols = vec![
        Column::number(tr(lang, "col.roll_no")),
        Column::text(tr(lang, "col.student_number")),
        Column::wide(tr(lang, "col.name")),
    ];
    for s in &sheet.subjects {
        cols.push(Column::number(format!(
            "{} /{}",
            s.subject_name,
            d(s.max_marks)
        )));
    }
    cols.push(Column::number(tr(lang, "col.total")));
    cols.push(Column::number(tr(lang, "col.percent")));
    cols.push(Column::text(tr(lang, "col.grade")));
    cols.push(Column::text(tr(lang, "col.result")));
    cols.push(Column::number(tr(lang, "col.position")));
    cols
}

fn result_row(
    lang: super::Lang,
    sheet: &ResultSheet,
    row: &crate::routes::exams::ResultRow,
) -> Vec<String> {
    let mut out = vec![
        opt(row.roll_no),
        row.student_number.clone(),
        row.full_name.clone(),
    ];
    for s in &sheet.subjects {
        let cell = row
            .marks
            .iter()
            .find(|m| m.subject_id == s.subject_id)
            .map(|m| {
                if !m.takes {
                    "—".to_string()
                } else if m.is_absent {
                    tr(lang, "value.absent").to_string()
                } else {
                    m.obtained.map(d).unwrap_or_default()
                }
            })
            .unwrap_or_default();
        out.push(cell);
    }
    out.push(format!("{}/{}", d(row.total), d(row.max)));
    out.push(pct(Some(row.percent)));
    out.push(row.grade.clone());
    out.push(tr(lang, if row.pass { "value.pass" } else { "value.fail" }).into());
    out.push(row.position.to_string());
    out
}

pub async fn result_sheet(
    conn: &mut PgConnection,
    user: &CurrentUser,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "exams.result_sheet",
        lang,
        &ctx.school.name,
        tr(lang, "report.exams.result_sheet"),
    );
    r.landscape = true;
    let assessment_id = assessment_param(conn, ctx, params, &mut r).await?;
    let (sections, _) = sections_in_scope(conn, ctx, params, &mut r, true).await?;
    let section = sections
        .first()
        .filter(|_| params.get("section_id").is_some())
        .ok_or_else(|| ApiError::validation("section_id is required."))?;
    let sheet = result_sheet_for(conn, user, assessment_id, section.id).await?;
    r.columns = result_columns(lang, &sheet);
    let mut g = Group::default();
    for row in &sheet.rows {
        g.rows.push(result_row(lang, &sheet, row));
    }
    r.push_group(g);
    if !sheet.complete {
        r.notes.push(tr(lang, "note.not_complete").into());
    }
    Ok(r)
}

/// Result rows of every section in scope, with the section name, sorted by percentage.
async fn class_rows(
    conn: &mut PgConnection,
    user: &CurrentUser,
    assessment_id: Uuid,
    sections: &[Section],
) -> Result<Vec<(String, ResultSheet)>, ApiError> {
    let mut out = Vec::new();
    for s in sections {
        let sheet = result_sheet_for(conn, user, assessment_id, s.id).await?;
        out.push((s.name.clone(), sheet));
    }
    Ok(out)
}

pub async fn merit(
    conn: &mut PgConnection,
    user: &CurrentUser,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "exams.merit",
        lang,
        &ctx.school.name,
        tr(lang, "report.exams.merit"),
    );
    let assessment_id = assessment_param(conn, ctx, params, &mut r).await?;
    let (sections, _) = sections_in_scope(conn, ctx, params, &mut r, false).await?;
    let top = params.number("top", 10)?.max(1) as usize;
    r.param("param.top_n", top.to_string());
    let sheets = class_rows(conn, user, assessment_id, &sections).await?;
    let mut all: Vec<(String, &crate::routes::exams::ResultRow)> = sheets
        .iter()
        .flat_map(|(name, sheet)| sheet.rows.iter().map(move |row| (name.clone(), row)))
        .filter(|(_, row)| row.max > Decimal::ZERO)
        .collect();
    all.sort_by(|a, b| {
        b.1.percent
            .cmp(&a.1.percent)
            .then(a.1.full_name.cmp(&b.1.full_name))
    });
    r.columns = vec![
        Column::number(tr(lang, "col.position")),
        Column::text(tr(lang, "col.student_number")),
        Column::wide(tr(lang, "col.name")),
        Column::text(tr(lang, "col.section")),
        Column::number(tr(lang, "col.roll_no")),
        Column::number(tr(lang, "col.total")),
        Column::number(tr(lang, "col.percent")),
        Column::text(tr(lang, "col.grade")),
    ];
    let mut g = Group::default();
    let mut position = 0usize;
    let mut last: Option<Decimal> = None;
    for (i, (section, row)) in all.iter().enumerate() {
        if last != Some(row.percent) {
            position = i + 1;
            last = Some(row.percent);
        }
        if position > top {
            break;
        }
        g.rows.push(vec![
            position.to_string(),
            row.student_number.clone(),
            row.full_name.clone(),
            section.clone(),
            opt(row.roll_no),
            format!("{}/{}", d(row.total), d(row.max)),
            pct(Some(row.percent)),
            row.grade.clone(),
        ]);
    }
    r.push_group(g);
    if sheets.iter().any(|(_, s)| !s.complete) {
        r.notes.push(tr(lang, "note.not_complete").into());
    }
    Ok(r)
}

pub async fn analysis(
    conn: &mut PgConnection,
    user: &CurrentUser,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "exams.analysis",
        lang,
        &ctx.school.name,
        tr(lang, "report.exams.analysis"),
    );
    r.landscape = true;
    let assessment_id = assessment_param(conn, ctx, params, &mut r).await?;
    let (sections, _) = sections_in_scope(conn, ctx, params, &mut r, true).await?;
    let sheets = class_rows(conn, user, assessment_id, &sections).await?;
    r.columns = vec![
        Column::wide(tr(lang, "col.subject")),
        Column::number(tr(lang, "col.max_marks")),
        Column::number(tr(lang, "col.appeared")),
        Column::number(tr(lang, "col.absent")),
        Column::number(tr(lang, "col.average")),
        Column::number(tr(lang, "col.highest")),
        Column::number(tr(lang, "col.lowest")),
        Column::number(tr(lang, "col.passed")),
        Column::number(tr(lang, "col.pass_percent")),
    ];
    for (name, sheet) in &sheets {
        let mut g = Group {
            heading: Some(format!("{} {}", tr(lang, "col.section"), name)),
            ..Default::default()
        };
        for a in &sheet.analysis {
            let pass_pct = (a.appeared > 0).then(|| {
                (Decimal::from(a.passed) / Decimal::from(a.appeared) * Decimal::from(100))
                    .round_dp(1)
            });
            g.rows.push(vec![
                a.subject_name.clone(),
                d(a.max_marks),
                a.appeared.to_string(),
                a.absent.to_string(),
                pct(a.average_percent),
                a.highest.map(d).unwrap_or_default(),
                a.lowest.map(d).unwrap_or_default(),
                a.passed.to_string(),
                pct(pass_pct),
            ]);
        }
        r.push_group(g);
    }
    if sheets.iter().any(|(_, s)| !s.complete) {
        r.notes.push(tr(lang, "note.not_complete").into());
    }
    Ok(r)
}

pub async fn grades(
    conn: &mut PgConnection,
    user: &CurrentUser,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "exams.grades",
        lang,
        &ctx.school.name,
        tr(lang, "report.exams.grades"),
    );
    let assessment_id = assessment_param(conn, ctx, params, &mut r).await?;
    let (sections, _) = sections_in_scope(conn, ctx, params, &mut r, false).await?;
    let sheets = class_rows(conn, user, assessment_id, &sections).await?;
    let grading = madrasa_db::exams::grading(conn, ctx.school.id).await?;
    let mut bands: Vec<String> = grading.bands.iter().map(|b| b.grade.clone()).collect();
    bands.dedup();
    r.columns = vec![
        Column::text(tr(lang, "col.section")),
        Column::number(tr(lang, "col.students")),
    ];
    for b in &bands {
        r.columns.push(Column::number(b.clone()));
    }
    r.columns.push(Column::number(tr(lang, "col.passed")));
    r.columns.push(Column::number(tr(lang, "col.failed")));
    r.columns.push(Column::number(tr(lang, "col.pass_percent")));
    let mut g = Group::default();
    let mut totals: HashMap<String, usize> = HashMap::new();
    let (mut students, mut passed) = (0usize, 0usize);
    for (name, sheet) in &sheets {
        let graded: Vec<_> = sheet
            .rows
            .iter()
            .filter(|r| r.max > Decimal::ZERO)
            .collect();
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for row in &graded {
            *counts.entry(row.grade.as_str()).or_default() += 1;
        }
        let p = graded.iter().filter(|r| r.pass).count();
        students += graded.len();
        passed += p;
        let mut row = vec![name.clone(), graded.len().to_string()];
        for b in &bands {
            let n = counts.get(b.as_str()).copied().unwrap_or(0);
            *totals.entry(b.clone()).or_default() += n;
            row.push(n.to_string());
        }
        row.push(p.to_string());
        row.push((graded.len() - p).to_string());
        row.push(pct((!graded.is_empty()).then(|| {
            (Decimal::from(p as u32) / Decimal::from(graded.len() as u32) * Decimal::from(100))
                .round_dp(1)
        })));
        g.rows.push(row);
    }
    r.push_group(g);
    let mut t = vec![tr(lang, "common.total").to_string(), students.to_string()];
    for b in &bands {
        t.push(totals.get(b).copied().unwrap_or(0).to_string());
    }
    t.push(passed.to_string());
    t.push((students - passed).to_string());
    t.push(pct((students > 0).then(|| {
        (Decimal::from(passed as u32) / Decimal::from(students as u32) * Decimal::from(100))
            .round_dp(1)
    })));
    r.total = Some(t);
    Ok(r)
}

pub async fn failing(
    conn: &mut PgConnection,
    user: &CurrentUser,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "exams.failing",
        lang,
        &ctx.school.name,
        tr(lang, "report.exams.failing"),
    );
    let assessment_id = assessment_param(conn, ctx, params, &mut r).await?;
    let (sections, _) = sections_in_scope(conn, ctx, params, &mut r, true).await?;
    let sheets = class_rows(conn, user, assessment_id, &sections).await?;
    let pass = sheets
        .first()
        .map(|(_, s)| s.grading.pass_percent)
        .unwrap_or(Decimal::from(40));
    r.columns = vec![
        Column::text(tr(lang, "col.section")),
        Column::number(tr(lang, "col.roll_no")),
        Column::text(tr(lang, "col.student_number")),
        Column::wide(tr(lang, "col.name")),
        Column::number(tr(lang, "col.marks")),
        Column::number(tr(lang, "col.percent")),
    ];
    // One group per subject, across the sections.
    let mut by_subject: Vec<(String, Vec<Vec<String>>)> = Vec::new();
    for (name, sheet) in &sheets {
        for s in &sheet.subjects {
            for row in &sheet.rows {
                let Some(m) = row.marks.iter().find(|m| m.subject_id == s.subject_id) else {
                    continue;
                };
                if m.obtained.is_none() && !m.is_absent {
                    continue;
                }
                if m.percent >= pass {
                    continue;
                }
                let entry = match by_subject.iter_mut().find(|(n, _)| *n == s.subject_name) {
                    Some(e) => e,
                    None => {
                        by_subject.push((s.subject_name.clone(), Vec::new()));
                        by_subject.last_mut().expect("pushed")
                    }
                };
                entry.1.push(vec![
                    name.clone(),
                    opt(row.roll_no),
                    row.student_number.clone(),
                    row.full_name.clone(),
                    if m.is_absent {
                        value(lang, "absent")
                    } else {
                        format!(
                            "{}/{}",
                            m.obtained.map(d).unwrap_or_default(),
                            d(s.max_marks)
                        )
                    },
                    pct(Some(m.percent)),
                ]);
            }
        }
    }
    for (subject, rows) in by_subject {
        r.push_group(Group {
            heading: Some(format!("{} ({})", subject, rows.len())),
            rows,
            subtotal: None,
        });
    }
    r.notes
        .push(format!("{}: {}%", tr(lang, "col.pass_percent"), d(pass)));
    Ok(r)
}

/// The section's report cards: a summary table on screen and in Excel, one card per page in
/// the PDF (the renderer uses `Report::cards`).
pub async fn report_cards(
    conn: &mut PgConnection,
    user: &CurrentUser,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "exams.report_cards",
        lang,
        &ctx.school.name,
        tr(lang, "report.exams.report_cards"),
    );
    let (sections, _) = sections_in_scope(conn, ctx, params, &mut r, true).await?;
    let section = sections
        .first()
        .filter(|_| params.get("section_id").is_some())
        .ok_or_else(|| ApiError::validation("section_id is required."))?;
    let cards: Vec<ReportCard> =
        crate::routes::exams::section_cards_for(conn, user, ctx.year, section).await?;
    r.columns = vec![
        Column::number(tr(lang, "col.roll_no")),
        Column::text(tr(lang, "col.student_number")),
        Column::wide(tr(lang, "col.name")),
        Column::number(tr(lang, "col.overall")),
        Column::text(tr(lang, "col.grade")),
        Column::text(tr(lang, "col.result")),
        Column::number(tr(lang, "col.attendance_percent")),
    ];
    let mut g = Group::default();
    for c in &cards {
        g.rows.push(vec![
            opt(c.roll_no),
            c.student_number.clone(),
            c.full_name.clone(),
            pct(c.overall_percent),
            c.overall_grade.clone().unwrap_or_default(),
            match c.pass {
                Some(true) => tr(lang, "value.pass").into(),
                Some(false) => tr(lang, "value.fail").into(),
                None => String::new(),
            },
            c.attendance
                .percent
                .map(|p| format!("{p:.1}"))
                .unwrap_or_else(|| "—".into()),
        ]);
    }
    r.push_group(g);
    r.cards = cards;
    Ok(r)
}
