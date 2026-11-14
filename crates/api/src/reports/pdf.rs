//! PDF rendering of a [`Report`] with typst, using fonts compiled into the executable so a
//! school PC needs nothing installed. Urdu reports are typeset in Noto Nastaliq Urdu,
//! right-to-left; English ones in Inter with Nastaliq as the fallback for Urdu names.

use std::sync::LazyLock;

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_layout::PagedDocument;

use super::{Align, Lang, Report};
use crate::ApiError;

const FONT_FILES: &[&[u8]] = &[
    include_bytes!("../../fonts/Inter-Regular.ttf"),
    include_bytes!("../../fonts/Inter-Bold.ttf"),
    include_bytes!("../../fonts/NotoNastaliqUrdu-Regular.ttf"),
    include_bytes!("../../fonts/NotoNastaliqUrdu-Bold.ttf"),
];

/// Madrasa's emblem (assets/brand/madrasa-mark.svg) for the header of vouchers, report
/// cards and offer letters: markup reads it as `image("/madrasa-mark.svg")`. The blue-page
/// version, made for white paper; printouts are always light.
const MARK_SVG: &[u8] = include_bytes!("../../../../assets/brand/madrasa-mark.svg");
const MARK_PATH: &str = "madrasa-mark.svg";

/// The emblem at `height`, as typst markup.
pub fn mark(height: &str) -> String {
    format!("image(\"/{MARK_PATH}\", height: {height}, alt: \"Madrasa\")")
}

struct Fonts {
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
}

static FONTS: LazyLock<Fonts> = LazyLock::new(|| {
    let mut book = FontBook::new();
    let mut fonts = Vec::new();
    for data in FONT_FILES {
        for font in Font::iter(Bytes::new(*data)) {
            book.push(font.info().clone());
            fonts.push(font);
        }
    }
    Fonts {
        book: LazyHash::new(book),
        fonts,
    }
});

static MARK_ID: LazyLock<FileId> = LazyLock::new(|| {
    FileId::new(RootedPath::new(
        VirtualRoot::Project,
        VirtualPath::new(MARK_PATH).expect("static path"),
    ))
});

static LIBRARY: LazyLock<LazyHash<Library>> = LazyLock::new(|| LazyHash::new(Library::default()));
static MAIN_ID: LazyLock<FileId> = LazyLock::new(|| {
    FileId::new(RootedPath::new(
        VirtualRoot::Project,
        VirtualPath::new("report.typ").expect("static path"),
    ))
});

struct ReportWorld {
    main: Source,
}

impl World for ReportWorld {
    fn library(&self) -> &LazyHash<Library> {
        &LIBRARY
    }
    fn book(&self) -> &LazyHash<FontBook> {
        &FONTS.book
    }
    fn main(&self) -> FileId {
        self.main.id()
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            Ok(self.main.clone())
        } else {
            Err(FileError::NotFound("report.typ".into()))
        }
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == *MARK_ID {
            Ok(Bytes::new(MARK_SVG))
        } else {
            Err(FileError::NotFound(MARK_PATH.into()))
        }
    }
    fn font(&self, index: usize) -> Option<Font> {
        FONTS.fonts.get(index).cloned()
    }
    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

/// Escape for a typst string literal; cells are emitted as `[#"…"]` so only two characters
/// matter and nothing in the data is ever read as markup.
pub(super) fn lit(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn cell(s: &str) -> String {
    format!("[#{}]", lit(s))
}

fn bold(s: &str) -> String {
    format!("[#text(weight: \"bold\")[#{}]]", lit(s))
}

/// The typst source for a report. Public so tests can inspect it.
pub fn source(report: &Report) -> String {
    let rtl = report.lang.is_rtl();
    let (font, size, lang, dir) = match report.lang {
        Lang::En => ("(\"Inter\", \"Noto Nastaliq Urdu\")", "9pt", "en", "ltr"),
        Lang::Ur => ("(\"Noto Nastaliq Urdu\", \"Inter\")", "9.5pt", "ur", "rtl"),
    };
    let ncols = report.columns.len().max(1);
    let widths = report
        .columns
        .iter()
        .map(|c| if c.stretch { "1fr" } else { "auto" })
        .collect::<Vec<_>>()
        .join(", ");
    let aligns = report
        .columns
        .iter()
        .map(|c| match c.align {
            Align::Start => "start + horizon",
            Align::End => "end + horizon",
            Align::Center => "center + horizon",
        })
        .collect::<Vec<_>>()
        .join(", ");
    let page_label = super::text::tr(report.lang, "common.page");
    let mut s = String::new();
    s.push_str(&format!(
        "#set page(paper: \"a4\", flipped: {flipped}, margin: (x: 1.4cm, top: 1.4cm, bottom: 1.6cm),\n  footer: context [#set text(size: 7.5pt, fill: luma(90)); #align(center)[#{page} #counter(page).display(\"1 / 1\", both: true)]])\n",
        flipped = report.landscape,
        page = lit(page_label),
    ));
    s.push_str(&format!(
        "#set text(font: {font}, size: {size}, lang: \"{lang}\", dir: {dir})\n#set par(leading: {leading})\n#set table(stroke: 0.4pt + luma(170), inset: (x: 4pt, y: {inset}))\n#set table.header(repeat: true)\n",
        leading = if rtl { "0.8em" } else { "0.55em" },
        inset = if rtl { "3pt" } else { "3.5pt" },
    ));
    // Header: school name, title, parameters, run date.
    s.push_str(&format!(
        "#block(width: 100%)[#text(size: 13pt, weight: \"bold\")[#{school}] #h(1fr) #text(size: 8pt, fill: luma(90))[#{gen}: #{date}]]\n",
        school = lit(&report.school_name),
        gen = lit(super::text::tr(report.lang, "common.generated")),
        date = lit(&report.generated_on),
    ));
    s.push_str(&format!(
        "#text(size: 12pt, weight: \"bold\")[#{}]\n",
        lit(&report.title)
    ));
    if let Some(sub) = &report.subtitle {
        s.push_str(&format!(
            "#linebreak()#text(size: 9pt, fill: luma(60))[#{}]\n",
            lit(sub)
        ));
    }
    if !report.params.is_empty() {
        let parts: Vec<String> = report
            .params
            .iter()
            .map(|p| {
                format!(
                    "#{}: #text(weight: \"bold\")[#{}]",
                    lit(&p.label),
                    lit(&p.value)
                )
            })
            .collect();
        s.push_str(&format!(
            "#block(above: 4pt, below: 6pt)[#text(size: 8.5pt, fill: luma(60))[{}]]\n",
            parts.join(" #h(1em) ")
        ));
    } else {
        s.push_str("#v(6pt)\n");
    }
    // Table.
    s.push_str(&format!(
        "#table(columns: ({widths}), align: ({aligns}),\n  table.header({}),\n",
        report
            .columns
            .iter()
            .map(|c| format!(
                "table.cell(fill: luma(235))[#text(weight: \"bold\")[#{}]]",
                lit(&c.label)
            ))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    for g in &report.groups {
        if let Some(h) = &g.heading {
            s.push_str(&format!(
                "  table.cell(colspan: {ncols}, fill: luma(245), align: start)[#text(weight: \"bold\")[#{}]],\n",
                lit(h)
            ));
        }
        for row in &g.rows {
            let cells: Vec<String> = (0..ncols)
                .map(|i| cell(row.get(i).map(String::as_str).unwrap_or("")))
                .collect();
            s.push_str("  ");
            s.push_str(&cells.join(", "));
            s.push_str(",\n");
        }
        if let Some(sub) = &g.subtotal {
            let cells: Vec<String> = (0..ncols)
                .map(|i| {
                    format!(
                        "table.cell(fill: luma(248)){}",
                        bold(sub.get(i).map(String::as_str).unwrap_or(""))
                    )
                })
                .collect();
            s.push_str("  ");
            s.push_str(&cells.join(", "));
            s.push_str(",\n");
        }
    }
    if let Some(total) = &report.total {
        let cells: Vec<String> = (0..ncols)
            .map(|i| {
                format!(
                    "table.cell(fill: luma(235))

{}",
                    bold(total.get(i).map(String::as_str).unwrap_or(""))
                )
                .replace("\n\n", "")
            })
            .collect();
        s.push_str("  ");
        s.push_str(&cells.join(", "));
        s.push_str(",\n");
    }
    s.push_str(")\n");
    if report.row_count == 0 {
        s.push_str(&format!(
            "#text(fill: luma(90))[#{}]\n",
            lit(super::text::tr(report.lang, "common.none"))
        ));
    }
    for n in &report.notes {
        s.push_str(&format!(
            "#block(above: 6pt)[#text(size: 8pt, fill: luma(60))[#{}]]\n",
            lit(n)
        ));
    }
    s
}

/// One report card per page: header, subject table, assessment totals, overall result,
/// attendance and signature lines. Same content as the on-screen card.
pub fn cards_source(report: &Report) -> String {
    use super::text::tr;
    let lang = report.lang;
    let rtl = lang.is_rtl();
    let (font, size, lang_code, dir) = match lang {
        Lang::En => ("(\"Inter\", \"Noto Nastaliq Urdu\")", "9.5pt", "en", "ltr"),
        Lang::Ur => ("(\"Noto Nastaliq Urdu\", \"Inter\")", "10pt", "ur", "rtl"),
    };
    let mut s = String::new();
    s.push_str("#set page(paper: \"a4\", margin: (x: 1.6cm, y: 1.6cm))\n");
    s.push_str(&format!(
        "#set text(font: {font}, size: {size}, lang: \"{lang_code}\", dir: {dir})\n#set par(leading: {leading})\n#set table(stroke: 0.4pt + luma(170), inset: (x: 4pt, y: 3.5pt))\n",
        leading = if rtl { "0.8em" } else { "0.55em" },
    ));
    let d = |v: rust_decimal::Decimal| v.round_dp(2).normalize().to_string();
    for (i, c) in report.cards.iter().enumerate() {
        if i > 0 {
            s.push_str("#pagebreak()\n");
        }
        s.push_str(&format!(
            "#place(top + start, dy: -2pt)[#{mark}]\n#align(center)[#text(size: 15pt, weight: \"bold\")[#{school}]\n#linebreak()#text(size: 9pt, fill: luma(80))[#{city}]\n#v(2pt)#text(size: 12pt, weight: \"bold\")[#{title} · #{year}]]\n#v(6pt)\n",
            mark = mark("1.35cm"),
            school = lit(&c.school.name),
            city = lit(&c.school.city),
            title = lit(tr(lang, "card.title")),
            year = lit(&c.academic_year),
        ));
        let class = match (&c.class_name, &c.section_name) {
            (Some(cl), Some(se)) => format!("{cl} · {se}"),
            (Some(cl), None) => cl.clone(),
            _ => String::new(),
        };
        s.push_str(&format!(
            "#grid(columns: (auto, 1fr, auto, 1fr), column-gutter: 8pt, row-gutter: 4pt,\n  [#text(fill: luma(90))[#{l1}]], [#text(weight: \"bold\")[#{v1}] #h(6pt) #text(fill: luma(90))[#{num}]],\n  [#text(fill: luma(90))[#{l2}]], [#{v2}],\n  [#text(fill: luma(90))[#{l3}]], [#{v3}],\n  [#text(fill: luma(90))[#{l4}]], [#{v4}])\n#v(8pt)\n",
            l1 = lit(tr(lang, "card.student")), v1 = lit(&c.full_name), num = lit(&c.student_number),
            l2 = lit(tr(lang, "card.father")), v2 = lit(&c.father_name),
            l3 = lit(tr(lang, "card.class")), v3 = lit(&class),
            l4 = lit(tr(lang, "card.roll_no")), v4 = lit(&c.roll_no.map(|r| r.to_string()).unwrap_or_default()),
        ));
        if c.subjects.is_empty() {
            s.push_str(&format!(
                "#text(fill: luma(90))[#{}]\n",
                lit(tr(lang, "card.empty"))
            ));
            continue;
        }
        let ncols = 1 + c.assessments.len() + 2;
        let mut widths = vec!["1fr".to_string()];
        widths.extend(std::iter::repeat_n(
            "auto".to_string(),
            c.assessments.len() + 2,
        ));
        let mut aligns = vec!["start + horizon".to_string()];
        aligns.extend(std::iter::repeat_n(
            "end + horizon".to_string(),
            c.assessments.len() + 2,
        ));
        s.push_str(&format!(
            "#table(columns: ({}), align: ({}),\n  table.header(",
            widths.join(", "),
            aligns.join(", ")
        ));
        let mut header = Vec::new();
        if c.terms.len() > 1 {
            // A first header row with each term spanning its components.
            header.push("table.cell(fill: luma(225))[]".to_string());
            for t in &c.terms {
                let span = c
                    .assessments
                    .iter()
                    .filter(|a| a.term_id == Some(t.id))
                    .count()
                    .max(1);
                header.push(format!(
                    "table.cell(colspan: {span}, fill: luma(225), align: center)[#text(weight: \"bold\")[#{}]#linebreak()#text(size: 7.5pt, fill: luma(90))[{}%]]",
                    lit(&t.name),
                    d(t.weight)
                ));
            }
            header.push("table.cell(colspan: 2, fill: luma(225))[]".to_string());
        }
        header.push(format!(
            "table.cell(fill: luma(235))[#text(weight: \"bold\")[#{}]]",
            lit(tr(lang, "col.subject"))
        ));
        for a in &c.assessments {
            header.push(format!(
                "table.cell(fill: luma(235))[#text(weight: \"bold\")[#{}]#linebreak()#text(size: 7.5pt, fill: luma(90))[{}%]]",
                lit(&a.name),
                d(a.weight)
            ));
        }
        header.push(format!(
            "table.cell(fill: luma(235))[#text(weight: \"bold\")[#{}]]",
            lit(tr(lang, "card.weighted"))
        ));
        header.push(format!(
            "table.cell(fill: luma(235))[#text(weight: \"bold\")[#{}]]",
            lit(tr(lang, "col.grade"))
        ));
        s.push_str(&header.join(", "));
        s.push_str("),\n");
        for sub in &c.subjects {
            let mut row = vec![cell(&sub.subject_name)];
            for (ai, cellv) in sub.cells.iter().enumerate() {
                let _ = ai;
                let text = match (cellv.max_marks, cellv.obtained, cellv.is_absent) {
                    (Some(max), _, true) => format!("{} /{}", tr(lang, "value.absent"), d(max)),
                    (Some(max), Some(ob), false) => format!("{} /{}", d(ob), d(max)),
                    (Some(max), None, false) => format!("— /{}", d(max)),
                    (None, _, _) => "—".into(),
                };
                row.push(cell(&text));
            }
            row.push(cell(
                &sub.weighted_percent
                    .map(|p| format!("{:.1}", p.round_dp(1)))
                    .unwrap_or_else(|| "—".into()),
            ));
            row.push(cell(sub.grade.as_deref().unwrap_or("—")));
            s.push_str("  ");
            s.push_str(&row.join(", "));
            s.push_str(",\n");
        }
        // Totals row per assessment.
        let mut tot = vec![format!(
            "table.cell(fill: luma(245)){}",
            bold(tr(lang, "card.total"))
        )];
        for a in &c.assessments {
            let t = c.totals.iter().find(|t| t.assessment_id == a.id);
            let text = match t {
                Some(t) if t.max > rust_decimal::Decimal::ZERO => {
                    let mut parts = vec![format!("{} /{}", d(t.total), d(t.max))];
                    if let Some(p) = t.percent {
                        parts.push(format!("{:.1}%", p.round_dp(1)));
                    }
                    if let Some(g) = &t.grade {
                        parts.push(g.clone());
                    }
                    if let Some(pos) = t.position {
                        parts.push(format!("#{pos}/{}", t.students));
                    }
                    parts.join(" · ")
                }
                _ => tr(lang, "value.not_held").to_string(),
            };
            tot.push(format!("table.cell(fill: luma(245)){}", bold(&text)));
        }
        tot.push(format!(
            "table.cell(fill: luma(245)){}",
            bold(
                &c.overall_percent
                    .map(|p| format!("{:.1}", p.round_dp(1)))
                    .unwrap_or_else(|| "—".into())
            )
        ));
        tot.push(format!(
            "table.cell(fill: luma(245)){}",
            bold(c.overall_grade.as_deref().unwrap_or("—"))
        ));
        s.push_str("  ");
        s.push_str(&tot.join(", "));
        s.push_str(",\n");
        if c.terms.len() > 1 {
            // Term results under their components.
            let mut row = vec![format!(
                "table.cell(fill: luma(245)){}",
                bold(tr(lang, "card.term_result"))
            )];
            for t in &c.terms {
                let span = c
                    .assessments
                    .iter()
                    .filter(|a| a.term_id == Some(t.id))
                    .count()
                    .max(1);
                let text = match (t.percent, &t.grade) {
                    (Some(p), Some(g)) => format!("{:.1}% · {g}", p.round_dp(1)),
                    (Some(p), None) => format!("{:.1}%", p.round_dp(1)),
                    _ => tr(lang, "value.not_held").to_string(),
                };
                row.push(format!(
                    "table.cell(colspan: {span}, fill: luma(245), align: center){}",
                    bold(&text)
                ));
            }
            row.push("table.cell(colspan: 2, fill: luma(245))[]".to_string());
            s.push_str("  ");
            s.push_str(&row.join(", "));
            s.push_str(",\n");
        }
        s.push_str(")\n");
        let _ = ncols;
        let result = match c.pass {
            Some(true) => tr(lang, "value.pass"),
            Some(false) => tr(lang, "value.fail"),
            None => "—",
        };
        let attendance = match c.attendance.percent {
            Some(p) => format!("{}/{} ({p:.1}%)", c.attendance.present, c.attendance.total),
            None => format!("{}/{}", c.attendance.present, c.attendance.total),
        };
        s.push_str(&format!(
            "#v(8pt)#grid(columns: (auto, 1fr), column-gutter: 8pt, row-gutter: 5pt,\n  [#text(fill: luma(90))[#{l1}]], [#text(weight: \"bold\")[#{v1}]],\n  [#text(fill: luma(90))[#{l2}]], [#{v2}],\n  [#text(fill: luma(90))[#{l3}]], [#{v3}])\n",
            l1 = lit(tr(lang, "card.result")), v1 = lit(result),
            l2 = lit(tr(lang, "card.remarks")), v2 = lit(c.overall_remarks.as_deref().unwrap_or("—")),
            l3 = lit(tr(lang, "card.attendance")), v3 = lit(&attendance),
        ));
        s.push_str(&format!(
            "#v(1fr)#grid(columns: (1fr, 1fr, 1fr), column-gutter: 24pt,\n  [#line(length: 100%, stroke: 0.5pt) #text(size: 8pt, fill: luma(90))[#{}]],\n  [#line(length: 100%, stroke: 0.5pt) #text(size: 8pt, fill: luma(90))[#{}]],\n  [#line(length: 100%, stroke: 0.5pt) #text(size: 8pt, fill: luma(90))[#{}]])\n",
            lit(tr(lang, "card.signature.teacher")),
            lit(tr(lang, "card.signature.principal")),
            lit(tr(lang, "card.signature.parent")),
        ));
    }
    if report.cards.is_empty() {
        s.push_str(&format!(
            "#text(fill: luma(90))[#{}]\n",
            lit(tr(lang, "card.no_students"))
        ));
    }
    s
}

fn markup(report: &Report) -> String {
    if report.cards.is_empty() {
        source(report)
    } else {
        cards_source(report)
    }
}

/// Typeset the report. CPU-bound: call from `spawn_blocking`.
pub fn render(report: &Report) -> Result<Vec<u8>, ApiError> {
    compile(&markup(report))
}

/// Typeset any typst markup with the bundled fonts (reports, vouchers). CPU-bound.
pub fn compile(markup: &str) -> Result<Vec<u8>, ApiError> {
    let world = ReportWorld {
        main: Source::new(*MAIN_ID, markup.to_string()),
    };
    let doc = typst::compile::<PagedDocument>(&world)
        .output
        .map_err(|errors| {
            let messages: Vec<String> = errors.iter().map(|e| e.message.to_string()).collect();
            for m in &messages {
                tracing::error!(message = %m, "report typesetting failed");
            }
            let mut err = ApiError::internal();
            err.details = Some(serde_json::json!({ "typst": messages }));
            err
        })?;
    typst_pdf::pdf(&doc, &typst_pdf::PdfOptions::default()).map_err(|errors| {
        for e in errors.iter() {
            tracing::error!(message = %e.message, "report pdf export failed");
        }
        ApiError::internal()
    })
}

/// Number of pages the report would take (tests).
pub fn page_count(report: &Report) -> Result<usize, ApiError> {
    let world = ReportWorld {
        main: Source::new(*MAIN_ID, markup(report)),
    };
    typst::compile::<PagedDocument>(&world)
        .output
        .map(|d| d.pages().len())
        .map_err(|_| ApiError::internal())
}

#[cfg(test)]
mod tests {
    use super::super::{Column, Group};
    use super::*;

    fn sample(lang: Lang) -> Report {
        let mut r = Report::new(
            "students.list",
            lang,
            "Gulshan High \"East\"",
            "Student list",
        );
        r.param("param.class", "9");
        r.columns = vec![
            Column::number("No."),
            Column::wide("Name"),
            Column::text("Phone"),
        ];
        let mut g = Group {
            heading: Some("Class 9 · A".into()),
            ..Default::default()
        };
        for i in 1..=60 {
            g.rows.push(vec![
                i.to_string(),
                format!("Student #{i} [x] \\ *bold*"),
                "0300-1234567".into(),
            ]);
        }
        g.subtotal = Some(vec!["".into(), "Subtotal".into(), "60".into()]);
        r.push_group(g);
        r.total = Some(vec!["".into(), "Total".into(), "60".into()]);
        r
    }

    #[test]
    fn the_madrasa_emblem_is_embedded_and_nothing_else_is_readable() {
        let pdf = compile(&format!("#{}", mark("1cm"))).expect("the emblem typesets");
        assert!(pdf.starts_with(b"%PDF"));
        // An image other than the emblem is not found: the world serves no other file.
        assert!(compile("#image(\"/etc/passwd\")").is_err());
    }

    #[test]
    fn data_is_never_markup_and_long_reports_paginate() {
        let r = sample(Lang::En);
        let src = source(&r);
        assert!(src.contains("Student #1 [x] \\\\ *bold*"), "{src}");
        let pages = page_count(&r).unwrap();
        assert!(pages >= 2, "60 rows should span pages, got {pages}");
        let pdf = render(&r).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
    }

    #[test]
    fn report_cards_take_one_page_each_in_both_languages() {
        use crate::routes::exams::{
            AssessmentHeader, AttendanceLine, CardAssessmentTotal, CardCell, CardSubject,
            ReportCard, SchoolHeader,
        };
        use madrasa_core::exams::GradingScheme;
        use rust_decimal::Decimal;
        let aid = uuid::Uuid::now_v7();
        let card = |name: &str| ReportCard {
            school: SchoolHeader {
                name: "Gulshan High".into(),
                code: "KHI-MC".into(),
                city: "Karachi".into(),
            },
            academic_year: "2026-27".into(),
            student_id: uuid::Uuid::now_v7(),
            student_number: "KHI-MC-2026-00001".into(),
            full_name: name.into(),
            father_name: "Ahmed Khan".into(),
            class_name: Some("9".into()),
            section_name: Some("A".into()),
            roll_no: Some(3),
            assessments: vec![AssessmentHeader {
                id: aid,
                name: "Mid-year".into(),
                weight: Decimal::from(30),
                held: true,
                term_id: None,
                kind: "entered".into(),
            }],
            terms: Vec::new(),
            subjects: vec![CardSubject {
                subject_id: uuid::Uuid::now_v7(),
                subject_name: "English".into(),
                cells: vec![CardCell {
                    max_marks: Some(Decimal::from(50)),
                    obtained: Some(Decimal::from(45)),
                    is_absent: false,
                    percent: Some(Decimal::from(90)),
                    approved: true,
                }],
                term_percents: Vec::new(),
                weighted_percent: Some(Decimal::from(90)),
                grade: Some("A+".into()),
            }],
            totals: vec![CardAssessmentTotal {
                assessment_id: aid,
                total: Decimal::from(45),
                max: Decimal::from(50),
                percent: Some(Decimal::from(90)),
                grade: Some("A+".into()),
                position: Some(1),
                students: 30,
            }],
            overall_percent: Some(Decimal::from(90)),
            overall_grade: Some("A+".into()),
            overall_remarks: Some("Outstanding".into()),
            pass: Some(true),
            attendance: AttendanceLine {
                present: 180,
                total: 200,
                percent: Some(90.0),
            },
            grading: GradingScheme::default(),
        };
        for lang in [Lang::En, Lang::Ur] {
            let mut r = Report::new("exams.report_cards", lang, "Gulshan High", "Report cards");
            r.cards = vec![
                card("Ali Khan"),
                card("سارہ خان"),
                card("Bilal \"B\" Ahmed"),
            ];
            assert_eq!(page_count(&r).unwrap(), 3, "{lang:?}");
            assert!(render(&r).unwrap().starts_with(b"%PDF"));
            // Each card's header carries the Madrasa emblem.
            assert_eq!(cards_source(&r).matches(&mark("1.35cm")).count(), 3);
        }
        let mut empty = Report::new(
            "exams.report_cards",
            Lang::En,
            "Gulshan High",
            "Report cards",
        );
        empty.cards = Vec::new();
        assert!(cards_source(&empty).contains("No students enrolled."));
    }

    #[test]
    fn urdu_report_is_right_to_left_in_nastaliq() {
        let mut r = sample(Lang::Ur);
        r.title = "طلبہ کی فہرست".into();
        r.columns[1].label = "نام".into();
        let src = source(&r);
        assert!(src.contains("dir: rtl") && src.contains("lang: \"ur\""));
        assert!(src.contains("\"Noto Nastaliq Urdu\", \"Inter\""));
        let pdf = render(&r).unwrap();
        assert!(pdf.len() > 10_000, "fonts embedded");
    }
}
