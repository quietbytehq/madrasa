//! Admissions reports (Standard stage 3): the funnel per session, applications by class,
//! pending tests, and the offer letter typeset with the PDF engine (en/ur).

use madrasa_core::admissions::StageKind;
use madrasa_db::admissions as repo;
use madrasa_db::admissions::ApplicationView;
use madrasa_db::models::{AcademicYear, School};
use rust_decimal::Decimal;
use sqlx::PgConnection;

use super::pdf::{compile, lit};
use super::text::tr;
use super::{format_date, money, Column, Context, Group, Lang, Params, Report};
use crate::ApiError;

/// Applications that reached and sit in each stage of the session.
pub async fn funnel(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    _params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "admissions.funnel",
        lang,
        &ctx.school.name,
        tr(lang, "report.admissions.funnel"),
    );
    r.param("param.academic_year", ctx.year.name.clone());
    r.columns = vec![
        Column::wide(tr(lang, "adm.col.stage")),
        Column::number(tr(lang, "adm.col.reached")),
        Column::number(tr(lang, "adm.col.current")),
        Column::number(tr(lang, "adm.col.percent")),
    ];
    let rows = repo::funnel(conn, ctx.school.id, ctx.year.id).await?;
    let total = rows.first().map(|(_, reached, _)| *reached).unwrap_or(0);
    let mut g = Group::default();
    for (stage, reached, current) in rows {
        let pct = if total > 0 {
            format!("{:.0}%", reached as f64 * 100.0 / total as f64)
        } else {
            "—".into()
        };
        g.rows.push(vec![
            stage.name,
            reached.to_string(),
            current.to_string(),
            pct,
        ]);
    }
    r.push_group(g);
    Ok(r)
}

/// Applications per class applied for, with how many were admitted.
pub async fn by_class(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    _params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "admissions.by_class",
        lang,
        &ctx.school.name,
        tr(lang, "report.admissions.by_class"),
    );
    r.param("param.academic_year", ctx.year.name.clone());
    r.columns = vec![
        Column::wide(tr(lang, "param.class")),
        Column::number(tr(lang, "adm.col.applications")),
        Column::number(tr(lang, "adm.col.open")),
        Column::number(tr(lang, "adm.col.admitted")),
        Column::number(tr(lang, "adm.col.rejected")),
        Column::number(tr(lang, "adm.col.converted")),
    ];
    let apps = repo::list(
        conn,
        ctx.school.id,
        &repo::ApplicationFilter {
            academic_year_id: Some(ctx.year.id),
            ..Default::default()
        },
    )
    .await?;
    let mut classes: Vec<String> = apps
        .iter()
        .map(|a| a.class_name.clone().unwrap_or_default())
        .collect();
    classes.sort();
    classes.dedup();
    let mut g = Group::default();
    let mut totals = [0usize; 5];
    for class in classes {
        let mine: Vec<&ApplicationView> = apps
            .iter()
            .filter(|a| a.class_name.clone().unwrap_or_default() == class)
            .collect();
        let count = |k: &str| mine.iter().filter(|a| a.stage_kind == k).count();
        let row = [
            mine.len(),
            count(StageKind::Open.as_str()),
            count(StageKind::Admitted.as_str()),
            count(StageKind::Rejected.as_str()) + count(StageKind::Withdrawn.as_str()),
            mine.iter()
                .filter(|a| a.application.student_id.is_some())
                .count(),
        ];
        for (t, v) in totals.iter_mut().zip(row) {
            *t += v;
        }
        let mut cells = vec![if class.is_empty() {
            tr(lang, "adm.no_class").to_string()
        } else {
            class
        }];
        cells.extend(row.iter().map(usize::to_string));
        g.rows.push(cells);
    }
    r.push_group(g);
    let mut total = vec![tr(lang, "tt.total").to_string()];
    total.extend(totals.iter().map(usize::to_string));
    r.total = Some(total);
    Ok(r)
}

/// Open applications with a test date on or after a day (default today), soonest first.
pub async fn pending_tests(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let mut r = Report::new(
        "admissions.pending_tests",
        lang,
        &ctx.school.name,
        tr(lang, "report.admissions.pending_tests"),
    );
    let from = params.date("as_of")?.unwrap_or_else(crate::clock::today);
    r.param("param.academic_year", ctx.year.name.clone());
    r.param("param.as_of", format_date(from));
    r.columns = vec![
        Column::text(tr(lang, "adm.col.test_date")),
        Column::text(tr(lang, "adm.col.application_no")),
        Column::wide(tr(lang, "col.name")),
        Column::text(tr(lang, "param.class")),
        Column::text(tr(lang, "adm.col.stage")),
        Column::text(tr(lang, "col.phone")),
    ];
    let mut apps = repo::list(
        conn,
        ctx.school.id,
        &repo::ApplicationFilter {
            academic_year_id: Some(ctx.year.id),
            ..Default::default()
        },
    )
    .await?;
    apps.retain(|a| {
        a.stage_kind == StageKind::Open.as_str()
            && a.application.test_date.is_some_and(|d| d >= from)
            && a.application.test_marks.is_none()
    });
    apps.sort_by_key(|a| a.application.test_date);
    let mut g = Group::default();
    for a in apps {
        let q = &a.application;
        g.rows.push(vec![
            q.test_date.map(format_date).unwrap_or_default(),
            q.application_no.clone(),
            q.full_name.clone(),
            a.class_name.clone().unwrap_or_default(),
            a.stage_name.clone(),
            q.guardian_phone
                .clone()
                .or(q.phone.clone())
                .unwrap_or_default(),
        ]);
    }
    r.push_group(g);
    Ok(r)
}

/// The offer letter for one application, one page, English or Urdu.
pub fn offer_letter(
    lang: Lang,
    school: &School,
    year: &AcademicYear,
    a: &ApplicationView,
    currency: &str,
) -> Result<Vec<u8>, ApiError> {
    let q = &a.application;
    let (font, dir, size) = match lang {
        Lang::Ur => ("\"Noto Nastaliq Urdu\"", "rtl", "11pt"),
        Lang::En => ("\"Inter\"", "ltr", "10.5pt"),
    };
    let date = q.offer_date.unwrap_or_else(crate::clock::today);
    let fee = q.offer_fee.filter(|f| *f > Decimal::ZERO);
    let class = a.class_name.clone().unwrap_or_default();
    let guardian = if q.guardian_name.is_empty() {
        tr(lang, "adm.letter.parent").to_string()
    } else {
        q.guardian_name.clone()
    };
    let fee_line = match fee {
        Some(f) => format!("{} {} {}", tr(lang, "adm.letter.fee"), currency, money(f)),
        None => String::new(),
    };
    let expires = q
        .offer_expires
        .map(|d| format!("{} {}", tr(lang, "adm.letter.expires"), format_date(d)))
        .unwrap_or_default();
    let body = tr(lang, "adm.letter.body")
        .replace("{name}", q.full_name.trim())
        .replace("{class}", class.trim())
        .replace("{year}", year.name.trim())
        .replace("{school}", school.name.trim());
    let markup = format!(
        "#set page(paper: \"a4\", margin: (x: 2.2cm, y: 2.2cm))\n#set text(font: {font}, size: {size}, lang: \"{lang_code}\", dir: {dir})\n#set par(leading: 0.8em, justify: false)\n\
         #align(center)[#text(size: 16pt, weight: \"bold\")[#{school}]]\n#align(center)[#text(size: 9pt, fill: luma(90))[#{city}]]\n#v(1.2em)\n\
         #grid(columns: (1fr, auto), [#{ref_label} #{application_no}], [#{date_label} #{date}])\n#v(1.5em)\n\
         #text(size: 13pt, weight: \"bold\")[#{title}]\n#v(0.8em)\n\
         #{greeting} #{guardian},\n#v(0.6em)\n#{body}\n#v(0.6em)\n#{fee_line}\n#{expires}\n#v(0.8em)\n#{closing}\n#v(3em)\n\
         #grid(columns: (1fr, 1fr), [#line(length: 60%, stroke: 0.5pt) #linebreak() #{principal}], [])\n",
        font = font,
        size = size,
        lang_code = if lang == Lang::Ur { "ur" } else { "en" },
        dir = dir,
        school = lit(&school.name),
        city = lit(&school.city),
        ref_label = lit(tr(lang, "adm.letter.ref")),
        application_no = lit(&q.application_no),
        date_label = lit(tr(lang, "adm.letter.date")),
        date = lit(&format_date(date)),
        title = lit(tr(lang, "adm.letter.title")),
        greeting = lit(tr(lang, "adm.letter.greeting")),
        guardian = lit(&guardian),
        body = lit(&body),
        fee_line = lit(&fee_line),
        expires = lit(&expires),
        closing = lit(tr(lang, "adm.letter.closing")),
        principal = lit(tr(lang, "adm.letter.principal")),
    );
    compile(&markup)
}
