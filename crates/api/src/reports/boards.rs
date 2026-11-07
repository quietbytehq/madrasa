//! Board result reports (M2 stage 4), built on the boards module's computed sheet.

use madrasa_core::boards::{CandidateStatus, SubjectStatus};
use rust_decimal::Decimal;
use sqlx::PgConnection;
use uuid::Uuid;

use super::students::opt;
use super::text::tr;
use super::{pct, Column, Context, Group, Lang, Params, Report};
use crate::auth::CurrentUser;
use crate::routes::boards::{sheet_for, ResultSheet, SheetRow};
use crate::ApiError;

fn d(v: Decimal) -> String {
    v.round_dp(2).normalize().to_string()
}

fn status_text(lang: Lang, s: CandidateStatus) -> String {
    tr(
        lang,
        match s {
            CandidateStatus::Pass => "value.pass",
            CandidateStatus::Fail => "value.fail",
            CandidateStatus::Absent => "value.absent",
            CandidateStatus::Withheld => "value.withheld",
            CandidateStatus::Pending => "value.pending",
        },
    )
    .into()
}

fn subject_cell(lang: Lang, cell: &crate::routes::boards::SubjectCell) -> String {
    match cell.status {
        None => String::new(),
        Some(SubjectStatus::Absent) => tr(lang, "value.absent").into(),
        Some(SubjectStatus::Withheld) => tr(lang, "value.withheld").into(),
        Some(_) => {
            let marks = cell.obtained.map(d).unwrap_or_default();
            match &cell.grade {
                Some(g) if !marks.is_empty() => format!("{marks} ({g})"),
                Some(g) => g.clone(),
                None => marks,
            }
        }
    }
}

async fn load(
    conn: &mut PgConnection,
    user: &CurrentUser,
    ctx: &Context<'_>,
    params: &Params,
    report: &mut Report,
) -> Result<(ResultSheet, Option<Uuid>), ApiError> {
    let exam_id = params
        .uuid("board_exam_id")?
        .ok_or_else(|| ApiError::validation("board_exam_id is required."))?;
    let sheet = sheet_for(conn, user, exam_id).await?;
    if sheet.exam.exam.academic_year_id != ctx.year.id {
        return Err(ApiError::validation(
            "The exam belongs to another academic year.",
        ));
    }
    report.param("param.academic_year", ctx.year.name.clone());
    report.param("param.board", sheet.board.name.clone());
    report.param("param.board_exam", sheet.exam.exam.name.clone());
    let class_id = params.uuid("class_id")?;
    if let Some(cid) = class_id {
        let class = madrasa_db::academics::get_class(conn, cid).await?;
        report.param("param.class", class.name);
    }
    Ok((sheet, class_id))
}

fn rows_in_scope<'a>(
    sheet: &'a ResultSheet,
    ctx: &Context<'_>,
    class_name: Option<&str>,
) -> Vec<&'a SheetRow> {
    sheet
        .rows
        .iter()
        .filter(|r| class_name.is_none_or(|c| r.candidate.class_name.as_deref() == Some(c)))
        .filter(|r| r.candidate.section_id.is_none_or(|s| ctx.allows(s)))
        .collect()
}

async fn class_name(
    conn: &mut PgConnection,
    class_id: Option<Uuid>,
) -> Result<Option<String>, ApiError> {
    Ok(match class_id {
        Some(id) => Some(madrasa_db::academics::get_class(conn, id).await?.name),
        None => None,
    })
}

pub async fn sheet(
    conn: &mut PgConnection,
    user: &CurrentUser,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "exams.board_sheet",
        lang,
        &ctx.school.name,
        tr(lang, "report.exams.board_sheet"),
    );
    r.landscape = true;
    let (sheet, class_id) = load(conn, user, ctx, params, &mut r).await?;
    let cname = class_name(conn, class_id).await?;
    r.subtitle = Some(format!("{} · {}", sheet.board.name, sheet.exam.exam.name));
    r.columns = vec![
        Column::text(tr(lang, "col.board_roll_no")),
        Column::text(tr(lang, "col.student_number")),
        Column::wide(tr(lang, "col.name")),
        Column::text(tr(lang, "col.section")),
    ];
    for s in &sheet.exam.subjects {
        r.columns
            .push(Column::number(format!("{} /{}", s.name, d(s.max_marks))));
    }
    r.columns.push(Column::number(tr(lang, "col.total")));
    r.columns.push(Column::number(tr(lang, "col.percent")));
    r.columns.push(Column::text(tr(lang, "col.grade")));
    r.columns.push(Column::text(tr(lang, "col.result")));
    r.columns.push(Column::number(tr(lang, "col.position")));
    let mut g = Group::default();
    for row in rows_in_scope(&sheet, ctx, cname.as_deref()) {
        let mut cells = vec![
            row.candidate.roll_no.clone().unwrap_or_default(),
            row.candidate.student_number.clone(),
            row.candidate.full_name.clone(),
            match (&row.candidate.class_name, &row.candidate.section_name) {
                (Some(c), Some(s)) => format!("{c}-{s}"),
                _ => String::new(),
            },
        ];
        for c in &row.cells {
            cells.push(subject_cell(lang, c));
        }
        cells.push(if row.status == CandidateStatus::Pending {
            String::new()
        } else {
            format!("{}/{}", d(row.total), d(row.max))
        });
        cells.push(pct(row.percent));
        cells.push(row.grade.clone().unwrap_or_default());
        cells.push(status_text(lang, row.status));
        cells.push(opt(row.position));
        g.rows.push(cells);
    }
    r.push_group(g);
    r.notes.push(format!(
        "{}: {} · {}: {} · {}: {}",
        tr(lang, "col.students"),
        sheet.candidates,
        tr(lang, "col.passed"),
        sheet.passed,
        tr(lang, "col.pass_percent"),
        pct(sheet.pass_percent)
    ));
    Ok(r)
}

pub async fn merit(
    conn: &mut PgConnection,
    user: &CurrentUser,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "exams.board_merit",
        lang,
        &ctx.school.name,
        tr(lang, "report.exams.board_merit"),
    );
    let (sheet, class_id) = load(conn, user, ctx, params, &mut r).await?;
    let cname = class_name(conn, class_id).await?;
    let top = params.number("top", 10)?.max(1) as usize;
    r.param("param.top_n", top.to_string());
    r.subtitle = Some(format!("{} · {}", sheet.board.name, sheet.exam.exam.name));
    let mut rows: Vec<&SheetRow> = rows_in_scope(&sheet, ctx, cname.as_deref())
        .into_iter()
        .filter(|row| row.percent.is_some() && row.status != CandidateStatus::Pending)
        .collect();
    rows.sort_by(|a, b| {
        b.percent
            .cmp(&a.percent)
            .then(a.candidate.full_name.cmp(&b.candidate.full_name))
    });
    r.columns = vec![
        Column::number(tr(lang, "col.position")),
        Column::text(tr(lang, "col.board_roll_no")),
        Column::wide(tr(lang, "col.name")),
        Column::wide(tr(lang, "col.father_name")),
        Column::text(tr(lang, "col.section")),
        Column::number(tr(lang, "col.total")),
        Column::number(tr(lang, "col.percent")),
        Column::text(tr(lang, "col.grade")),
        Column::text(tr(lang, "col.result")),
    ];
    let mut g = Group::default();
    let mut position = 0usize;
    let mut last: Option<Decimal> = None;
    for (i, row) in rows.iter().enumerate() {
        if last != row.percent {
            position = i + 1;
            last = row.percent;
        }
        if position > top {
            break;
        }
        g.rows.push(vec![
            position.to_string(),
            row.candidate.roll_no.clone().unwrap_or_default(),
            row.candidate.full_name.clone(),
            row.candidate.father_name.clone(),
            match (&row.candidate.class_name, &row.candidate.section_name) {
                (Some(c), Some(s)) => format!("{c}-{s}"),
                _ => String::new(),
            },
            format!("{}/{}", d(row.total), d(row.max)),
            pct(row.percent),
            row.grade.clone().unwrap_or_default(),
            status_text(lang, row.status),
        ]);
    }
    r.push_group(g);
    Ok(r)
}

pub async fn pass_rates(
    conn: &mut PgConnection,
    user: &CurrentUser,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "exams.board_pass",
        lang,
        &ctx.school.name,
        tr(lang, "report.exams.board_pass"),
    );
    let (sheet, class_id) = load(conn, user, ctx, params, &mut r).await?;
    let cname = class_name(conn, class_id).await?;
    r.subtitle = Some(format!("{} · {}", sheet.board.name, sheet.exam.exam.name));
    let rows = rows_in_scope(&sheet, ctx, cname.as_deref());
    r.columns = vec![
        Column::wide(tr(lang, "col.subject")),
        Column::number(tr(lang, "col.appeared")),
        Column::number(tr(lang, "col.passed")),
        Column::number(tr(lang, "col.failed")),
        Column::number(tr(lang, "col.pass_percent")),
        Column::number(tr(lang, "col.average")),
    ];
    // By subject.
    let mut g = Group {
        heading: Some(tr(lang, "col.subject").into()),
        ..Default::default()
    };
    for s in &sheet.exam.subjects {
        let cells: Vec<_> = rows
            .iter()
            .filter_map(|row| row.cells.iter().find(|c| c.board_subject_id == s.id))
            .filter(|c| c.status.is_some() && c.status != Some(SubjectStatus::Absent))
            .collect();
        let appeared = cells.len();
        let passed = cells
            .iter()
            .filter(|c| c.status == Some(SubjectStatus::Pass))
            .count();
        let pcts: Vec<Decimal> = cells
            .iter()
            .filter_map(|c| {
                c.obtained
                    .map(|o| madrasa_core::exams::percent(o, c.max_marks))
            })
            .collect();
        let avg = (!pcts.is_empty())
            .then(|| (pcts.iter().sum::<Decimal>() / Decimal::from(pcts.len() as u32)).round_dp(1));
        g.rows.push(vec![
            s.name.clone(),
            appeared.to_string(),
            passed.to_string(),
            (appeared - passed).to_string(),
            pct((appeared > 0).then(|| {
                (Decimal::from(passed as u32) * Decimal::from(100) / Decimal::from(appeared as u32))
                    .round_dp(1)
            })),
            pct(avg),
        ]);
    }
    r.push_group(g);
    // By section.
    let mut sections: Vec<String> = rows
        .iter()
        .map(
            |row| match (&row.candidate.class_name, &row.candidate.section_name) {
                (Some(c), Some(s)) => format!("{c}-{s}"),
                _ => tr(lang, "common.not_enrolled").into(),
            },
        )
        .collect();
    sections.sort();
    sections.dedup();
    let mut g = Group {
        heading: Some(tr(lang, "col.section").into()),
        ..Default::default()
    };
    for sec in sections {
        let members: Vec<&&SheetRow> = rows
            .iter()
            .filter(|row| {
                let key = match (&row.candidate.class_name, &row.candidate.section_name) {
                    (Some(c), Some(s)) => format!("{c}-{s}"),
                    _ => tr(lang, "common.not_enrolled").into(),
                };
                key == sec
            })
            .filter(|row| row.status != CandidateStatus::Pending)
            .collect();
        let appeared = members
            .iter()
            .filter(|m| m.status != CandidateStatus::Absent)
            .count();
        let passed = members
            .iter()
            .filter(|m| m.status == CandidateStatus::Pass)
            .count();
        let pcts: Vec<Decimal> = members.iter().filter_map(|m| m.percent).collect();
        let avg = (!pcts.is_empty())
            .then(|| (pcts.iter().sum::<Decimal>() / Decimal::from(pcts.len() as u32)).round_dp(1));
        g.rows.push(vec![
            sec,
            appeared.to_string(),
            passed.to_string(),
            (appeared - passed).to_string(),
            pct((appeared > 0).then(|| {
                (Decimal::from(passed as u32) * Decimal::from(100) / Decimal::from(appeared as u32))
                    .round_dp(1)
            })),
            pct(avg),
        ]);
    }
    r.push_group(g);
    r.total = Some(vec![
        tr(lang, "common.total").into(),
        sheet.with_results.to_string(),
        sheet.passed.to_string(),
        (sheet.with_results - sheet.passed).to_string(),
        pct(sheet.pass_percent),
        String::new(),
    ]);
    Ok(r)
}

/// One student's board result card: subjects down, marks, grade and status, then totals.
pub async fn card(
    conn: &mut PgConnection,
    user: &CurrentUser,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "exams.board_card",
        lang,
        &ctx.school.name,
        tr(lang, "report.exams.board_card"),
    );
    let (sheet, _) = load(conn, user, ctx, params, &mut r).await?;
    let number = params
        .get("student_number")
        .ok_or_else(|| ApiError::validation("student_number is required."))?;
    let row = sheet
        .rows
        .iter()
        .find(|row| row.candidate.student_number.eq_ignore_ascii_case(number))
        .ok_or_else(|| ApiError::not_found("candidate"))?;
    if let Some(sid) = row.candidate.section_id {
        if !ctx.allows(sid) {
            return Err(ApiError::forbidden());
        }
    }
    r.subtitle = Some(format!(
        "{} · {} · {} {}",
        row.candidate.full_name,
        row.candidate.father_name,
        tr(lang, "col.board_roll_no"),
        row.candidate.roll_no.clone().unwrap_or_default()
    ));
    r.param("param.student_number", row.candidate.student_number.clone());
    r.columns = vec![
        Column::wide(tr(lang, "col.subject")),
        Column::number(tr(lang, "col.max_marks")),
        Column::number(tr(lang, "col.marks")),
        Column::text(tr(lang, "col.grade")),
        Column::text(tr(lang, "col.result")),
    ];
    let mut g = Group::default();
    for (s, c) in sheet.exam.subjects.iter().zip(&row.cells) {
        g.rows.push(vec![
            s.name.clone(),
            d(c.max_marks),
            c.obtained.map(d).unwrap_or_default(),
            c.grade.clone().unwrap_or_default(),
            c.status
                .map(|st| {
                    tr(
                        lang,
                        match st {
                            SubjectStatus::Pass => "value.pass",
                            SubjectStatus::Fail => "value.fail",
                            SubjectStatus::Absent => "value.absent",
                            SubjectStatus::Withheld => "value.withheld",
                        },
                    )
                    .to_string()
                })
                .unwrap_or_default(),
        ]);
    }
    r.push_group(g);
    r.total = Some(vec![
        tr(lang, "common.total").into(),
        d(row.max),
        d(row.total),
        row.grade.clone().unwrap_or_default(),
        status_text(lang, row.status),
    ]);
    r.notes
        .push(format!("{}: {}", tr(lang, "col.percent"), pct(row.percent)));
    if let Some(p) = row.position {
        r.notes.push(format!(
            "{}: {} / {}",
            tr(lang, "col.position"),
            p,
            sheet.with_results
        ));
    }
    Ok(r)
}
