//! Consent reports (M2 stage 8): missing consents by section and type, consents that expire
//! with the year, and the consent form list for a section.

use std::collections::HashMap;

use madrasa_core::consents::{self as rules, Current, Renewal};
use madrasa_db::consents as repo;
use madrasa_db::consents::ConsentType;
use madrasa_db::reports::{self as q, StudentReportFilter, StudentReportRow};
use madrasa_db::students;
use sqlx::PgConnection;
use uuid::Uuid;

use super::students::scope_params;
use super::text::tr;
use super::{format_date, Column, Context, Group, Params, Report};
use crate::ApiError;

fn opt(v: Option<i32>) -> String {
    v.map(|n| n.to_string()).unwrap_or_default()
}

struct Base {
    rows: Vec<StudentReportRow>,
    types: Vec<ConsentType>,
    /// (student_number → student id)
    ids: HashMap<String, Uuid>,
    latest: Vec<repo::ConsentView>,
}

/// Students in scope (optionally one consent type), their ids and the latest consent rows.
async fn base(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
    r: &mut Report,
) -> Result<Base, ApiError> {
    let (class_id, section_id) = scope_params(conn, ctx, params, r, true).await?;
    let mut types = repo::list_types(conn, ctx.school.id).await?;
    if let Some(type_id) = params.uuid("consent_type_id")? {
        types.retain(|t| t.id == type_id);
        if let Some(t) = types.first() {
            r.param("param.consent_type", t.name.clone());
        }
    }
    let rows = q::student_list(
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
    let mut ids = HashMap::new();
    for row in &rows {
        if let Some(s) = students::find_by_number(conn, ctx.school.id, &row.student_number).await? {
            ids.insert(row.student_number.clone(), s.id);
        }
    }
    let all_ids: Vec<Uuid> = ids.values().copied().collect();
    let latest = repo::latest_for_students(conn, &all_ids).await?;
    Ok(Base {
        rows,
        types,
        ids,
        latest,
    })
}

fn state_of<'a>(
    b: &'a Base,
    student_id: Uuid,
    t: &ConsentType,
    year_id: Uuid,
) -> (Current, Option<&'a repo::ConsentView>) {
    let mine = b
        .latest
        .iter()
        .find(|v| v.consent.student_id == student_id && v.consent.consent_type_id == t.id);
    let current = rules::current(
        &t.rule(),
        mine.and_then(|v| {
            rules::Status::parse(&v.consent.status).map(|status| rules::Latest {
                status,
                academic_year_id: v.consent.academic_year_id,
                recorded_on: v.consent.recorded_on,
            })
        }),
        year_id,
    );
    (current, mine)
}

fn current_label(lang: super::Lang, c: Current) -> &'static str {
    match c {
        Current::Given => tr(lang, "consent.given"),
        Current::Refused => tr(lang, "consent.refused"),
        Current::Withdrawn => tr(lang, "consent.withdrawn"),
        Current::Expired => tr(lang, "consent.expired"),
        Current::Missing => tr(lang, "consent.missing"),
    }
}

/// Missing consents: one row per student and required consent that is not on file, grouped
/// by section.
pub async fn missing(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "students.consents_missing",
        lang,
        &ctx.school.name,
        tr(lang, "report.students.consents_missing"),
    );
    let b = base(conn, ctx, params, &mut r).await?;
    r.columns = vec![
        Column::number(tr(lang, "col.roll_no")),
        Column::text(tr(lang, "col.student_number")),
        Column::wide(tr(lang, "col.name")),
        Column::wide(tr(lang, "col.guardian")),
        Column::text(tr(lang, "col.phone")),
        Column::wide(tr(lang, "col.consent")),
        Column::text(tr(lang, "col.status")),
    ];
    let mut current_heading: Option<String> = None;
    let mut g = Group::default();
    for row in &b.rows {
        let heading = match (&row.class_name, &row.section_name) {
            (Some(c), Some(s)) => format!("{c} {s}"),
            _ => tr(lang, "common.not_enrolled").to_string(),
        };
        if current_heading.as_deref() != Some(&heading) {
            if current_heading.is_some() && !g.rows.is_empty() {
                let count = g.rows.len();
                g.subtotal = Some(vec![
                    tr(lang, "total.missing").replace("{n}", &count.to_string())
                ]);
                r.push_group(std::mem::take(&mut g));
            }
            g = Group::default();
            g.heading = Some(heading.clone());
            current_heading = Some(heading);
        }
        let Some(student_id) = b.ids.get(&row.student_number) else {
            continue;
        };
        for t in &b.types {
            if !t.rule().is_required_for(row.class_level) {
                continue;
            }
            let (current, _) = state_of(&b, *student_id, t, ctx.year.id);
            if !current.needs_collecting() {
                continue;
            }
            g.rows.push(vec![
                opt(row.roll_no),
                row.student_number.clone(),
                row.full_name.clone(),
                row.guardian_name.clone().unwrap_or_default(),
                row.guardian_phone.clone().unwrap_or_default(),
                t.name.clone(),
                current_label(lang, current).to_string(),
            ]);
        }
    }
    if current_heading.is_some() && !g.rows.is_empty() {
        let count = g.rows.len();
        g.subtotal = Some(vec![
            tr(lang, "total.missing").replace("{n}", &count.to_string())
        ]);
        r.push_group(g);
    }
    Ok(r)
}

/// Yearly consents given in this academic year: they lapse when the year ends.
pub async fn expiring(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "students.consents_expiring",
        lang,
        &ctx.school.name,
        tr(lang, "report.students.consents_expiring"),
    );
    let b = base(conn, ctx, params, &mut r).await?;
    r.columns = vec![
        Column::text(tr(lang, "col.class")),
        Column::text(tr(lang, "col.section")),
        Column::number(tr(lang, "col.roll_no")),
        Column::wide(tr(lang, "col.name")),
        Column::wide(tr(lang, "col.consent")),
        Column::wide(tr(lang, "col.given_by")),
        Column::text(tr(lang, "col.date")),
    ];
    let mut g = Group::default();
    for row in &b.rows {
        let Some(student_id) = b.ids.get(&row.student_number) else {
            continue;
        };
        for t in b
            .types
            .iter()
            .filter(|t| t.rule().renewal == Renewal::Yearly)
        {
            let (current, latest) = state_of(&b, *student_id, t, ctx.year.id);
            let Some(l) = latest else { continue };
            if current != Current::Given {
                continue;
            }
            g.rows.push(vec![
                row.class_name.clone().unwrap_or_default(),
                row.section_name.clone().unwrap_or_default(),
                opt(row.roll_no),
                row.full_name.clone(),
                t.name.clone(),
                l.guardian_name.clone().unwrap_or_default(),
                format_date(l.consent.recorded_on),
            ]);
        }
    }
    r.notes
        .push(tr(lang, "note.consents_expiring").replace("{year}", &ctx.year.name));
    r.push_group(g);
    Ok(r)
}

/// Consent form list: every student of the scope with the consents still to collect, so
/// the office can send the forms home and tick them off.
pub async fn forms(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "students.consent_forms",
        lang,
        &ctx.school.name,
        tr(lang, "report.students.consent_forms"),
    );
    r.landscape = true;
    let b = base(conn, ctx, params, &mut r).await?;
    r.columns = vec![
        Column::number(tr(lang, "col.roll_no")),
        Column::text(tr(lang, "col.student_number")),
        Column::wide(tr(lang, "col.name")),
        Column::wide(tr(lang, "col.father_name")),
        Column::wide(tr(lang, "col.guardian")),
        Column::text(tr(lang, "col.phone")),
        Column::wide(tr(lang, "col.to_collect")),
        Column::text(tr(lang, "col.signature")),
    ];
    let mut current_heading: Option<String> = None;
    let mut g = Group::default();
    for row in &b.rows {
        let heading = match (&row.class_name, &row.section_name) {
            (Some(c), Some(s)) => format!("{c} {s}"),
            _ => tr(lang, "common.not_enrolled").to_string(),
        };
        if current_heading.as_deref() != Some(&heading) {
            if current_heading.is_some() && !g.rows.is_empty() {
                r.push_group(std::mem::take(&mut g));
            }
            g = Group::default();
            g.heading = Some(heading.clone());
            current_heading = Some(heading);
        }
        let Some(student_id) = b.ids.get(&row.student_number) else {
            continue;
        };
        let to_collect: Vec<String> = b
            .types
            .iter()
            .filter(|t| t.rule().is_required_for(row.class_level))
            .filter(|t| {
                state_of(&b, *student_id, t, ctx.year.id)
                    .0
                    .needs_collecting()
            })
            .map(|t| t.name.clone())
            .collect();
        if to_collect.is_empty() {
            continue;
        }
        g.rows.push(vec![
            opt(row.roll_no),
            row.student_number.clone(),
            row.full_name.clone(),
            row.father_name.clone(),
            row.guardian_name.clone().unwrap_or_default(),
            row.guardian_phone.clone().unwrap_or_default(),
            to_collect.join(", "),
            String::new(),
        ]);
    }
    if current_heading.is_some() && !g.rows.is_empty() {
        r.push_group(g);
    }
    Ok(r)
}
