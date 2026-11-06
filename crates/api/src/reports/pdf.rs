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
    fn file(&self, _id: FileId) -> FileResult<Bytes> {
        Err(FileError::NotFound("report.typ".into()))
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
fn lit(s: &str) -> String {
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

/// Typeset the report. CPU-bound: call from `spawn_blocking`.
pub fn render(report: &Report) -> Result<Vec<u8>, ApiError> {
    let world = ReportWorld {
        main: Source::new(*MAIN_ID, source(report)),
    };
    let doc = typst::compile::<PagedDocument>(&world)
        .output
        .map_err(|errors| {
            for e in errors.iter() {
                tracing::error!(message = %e.message, "report typesetting failed");
            }
            ApiError::internal()
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
        main: Source::new(*MAIN_ID, source(report)),
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
