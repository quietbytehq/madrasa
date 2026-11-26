//! Import wizard service: read .xlsx/.csv, map columns, clean and validate rows against the
//! database (classes, sections, duplicates, existing students), and apply a batch that can be
//! rolled back. Row cleanup rules live in `madrasa_core::imports`.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use calamine::{Data, Reader};
use chrono::NaiveDate;
use madrasa_core::imports::{self as rules, CleanRow, ImportKind, Issue};
use madrasa_db::models::{Class, FeeHead, FeeStructure, ImportBatch, Section, Student};
use madrasa_db::{
    academics, fees, guardians, imports as repo, schools, settings, staff, students, Actor,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Cursor;
use uuid::Uuid;

use crate::{ApiError, AppState};

pub const MAX_ROWS: usize = 5000;

// What an uploaded spreadsheet may cost to open, checked before a spreadsheet library sees
// it. A workbook is a zip file: a few kilobytes can unpack to gigabytes, and a sheet with
// one cell in its far corner makes a reader lay out billions of empty cells. Either would
// stop the school's server; an import file comes from outside the school more often than
// anything else Madrasa reads.
/// The file itself.
pub const MAX_FILE_BYTES: usize = 20 * 1024 * 1024;
/// Everything in the workbook, unpacked.
const MAX_UNPACKED_BYTES: u64 = 300 * 1024 * 1024;
/// Files inside the workbook.
const MAX_ZIP_ENTRIES: usize = 2000;
/// Columns a sheet may span, and the furthest row it may reach.
const MAX_SHEET_COLUMNS: u32 = 500;
const MAX_SHEET_ROWS: u32 = 200_000;
/// Cells of the rectangle a sheet spans (the reader keeps one slot for each).
const MAX_SHEET_CELLS: u64 = 3_000_000;

/// A workbook that will not open: what to do about it, not only that it failed.
const UNREADABLE_SHEET: &str = "The sheet could not be read. Open the file in Excel, save it again as an Excel workbook (.xlsx) and choose it again.";

fn too_big() -> ApiError {
    ApiError::validation(
        "This file is too large to import. Keep one sheet with the columns you need and at most 5,000 rows, save it as .xlsx or .csv, and try again.",
    )
}

/// Unpack every file of a zip-based workbook into nothing, counting: refuse when it holds
/// too many files or more data than a real import ever would. What the zip's own headers
/// claim is not trusted; the bytes are counted as they come out.
fn check_unpacked_size(bytes: &[u8]) -> Result<(), ApiError> {
    use std::io::Read;
    let unreadable = || {
        ApiError::validation(
            "This is not a spreadsheet Madrasa can read. Save it as .xlsx or .csv.",
        )
    };
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| unreadable())?;
    if archive.len() > MAX_ZIP_ENTRIES {
        return Err(too_big());
    }
    let mut left = MAX_UNPACKED_BYTES;
    let mut sink = [0u8; 64 * 1024];
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|_| unreadable())?;
        loop {
            let n = entry.read(&mut sink).map_err(|_| unreadable())?;
            if n == 0 {
                break;
            }
            left = left.checked_sub(n as u64).ok_or_else(too_big)?;
        }
    }
    Ok(())
}

/// Read the cells of one `.xlsx` sheet as a stream and refuse a sheet that spans more than
/// a real list does, before anything lays it out as a grid.
fn check_sheet_span(bytes: &[u8], sheet: &str) -> Result<(), ApiError> {
    let unreadable = || ApiError::validation(UNREADABLE_SHEET);
    let mut wb: calamine::Xlsx<_> =
        calamine::open_workbook_from_rs(Cursor::new(bytes)).map_err(|_| unreadable())?;
    let mut cells = wb.worksheet_cells_reader(sheet).map_err(|_| unreadable())?;
    let (mut first_row, mut last_row, mut first_col, mut last_col) =
        (u32::MAX, 0u32, u32::MAX, 0u32);
    while let Some(cell) = cells.next_cell().map_err(|_| unreadable())? {
        let (row, col) = cell.get_position();
        first_row = first_row.min(row);
        last_row = last_row.max(row);
        first_col = first_col.min(col);
        last_col = last_col.max(col);
        if last_row >= MAX_SHEET_ROWS || last_col >= MAX_SHEET_COLUMNS {
            return Err(too_big());
        }
    }
    if first_row == u32::MAX {
        return Ok(());
    }
    let span = u64::from(last_row - first_row + 1) * u64::from(last_col - first_col + 1);
    if span > MAX_SHEET_CELLS {
        return Err(too_big());
    }
    Ok(())
}

// ---- reading files ------------------------------------------------------------------------

/// Mirrored by `ParsedFile` in the UI.
#[derive(Debug, Serialize)]
pub struct ParsedFile {
    pub file_name: String,
    pub sheets: Vec<String>,
    pub sheet: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// Field key → column index guessed from the headers, or the saved mapping when the
    /// headers match a previous import.
    pub mapping: BTreeMap<String, usize>,
    pub mapping_saved: bool,
}

fn cell_to_string(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(s) => s.clone(),
        Data::Float(f) => {
            if f.fract() == 0.0 && f.abs() < 1e15 {
                format!("{}", *f as i64)
            } else {
                f.to_string()
            }
        }
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(dt) => dt
            .as_datetime()
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| dt.as_f64().to_string()),
        Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
        Data::Error(e) => format!("{e:?}"),
    }
}

/// (sheet names, chosen sheet, headers, rows)
pub type Parsed = (Vec<String>, Vec<String>, Vec<String>, Vec<Vec<String>>);

/// Parse an uploaded file (base64) into headers and string cells.
/// The sheet to read when the user has not chosen one: a sheet named after the import kind
/// ("Students", "Staff", "Opening balances"…) first, then the first sheet that is not a
/// read-me or instructions sheet, then the first sheet.
pub fn pick_sheet(kind: ImportKind, names: &[String]) -> Option<&str> {
    let norm = |s: &str| {
        s.to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
    };
    let wanted: &[&str] = match kind {
        ImportKind::Students => &["students", "student", "pupils", "learners", "studentdata"],
        ImportKind::Staff => &["staff", "employees", "teachers", "staffdata"],
        ImportKind::OpeningBalances => {
            &["openingbalances", "balances", "opening", "fees", "arrears"]
        }
        ImportKind::Enquiries => &[
            "enquiries",
            "inquiries",
            "admissions",
            "applications",
            "leads",
        ],
        ImportKind::Guardians => &["guardians", "guardian", "parents", "guardiandata"],
        ImportKind::FeeStructure => &["feestructure", "structure", "fees", "feeheads"],
    };
    let skip = [
        "ہدایات",
        "readme",
        "read",
        "instructions",
        "notes",
        "about",
        "help",
        "info",
        "guide",
    ];
    names
        .iter()
        .find(|n| wanted.contains(&norm(n).as_str()))
        .or_else(|| {
            names
                .iter()
                .find(|n| !skip.iter().any(|s| norm(n).starts_with(s)))
        })
        .or_else(|| names.first())
        .map(String::as_str)
}

pub fn parse_file(
    kind: ImportKind,
    file_name: &str,
    content_base64: &str,
    sheet: Option<&str>,
) -> Result<Parsed, ApiError> {
    let bytes = STANDARD
        .decode(content_base64.trim())
        .map_err(|_| ApiError::validation("The file could not be read."))?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(ApiError::validation(
            "The file is too large (20 MB at most).",
        ));
    }
    let lower = file_name.to_lowercase();
    if lower.ends_with(".xlsx")
        || lower.ends_with(".xlsm")
        || lower.ends_with(".xls")
        || lower.ends_with(".ods")
    {
        // Zip-based workbooks are measured before they are opened (`.xls` is not a zip: it
        // cannot hold more than the file itself).
        let zipped = bytes.starts_with(b"PK");
        if zipped {
            check_unpacked_size(&bytes)?;
        }
        let mut wb =
            calamine::open_workbook_auto_from_rs(Cursor::new(bytes.as_slice())).map_err(|_| {
                ApiError::validation(
                    "This is not a spreadsheet Madrasa can read. Save it as .xlsx or .csv.",
                )
            })?;
        let names = wb.sheet_names().to_vec();
        let chosen = sheet
            .filter(|s| names.iter().any(|n| n == s))
            .or_else(|| pick_sheet(kind, &names))
            .map(str::to_owned)
            .ok_or_else(|| ApiError::validation("The workbook has no sheets."))?;
        if matches!(wb, calamine::Sheets::Xlsx(_)) {
            check_sheet_span(&bytes, &chosen)?;
        }
        let range = wb
            .worksheet_range(&chosen)
            .map_err(|_| ApiError::validation(UNREADABLE_SHEET))?;
        let mut rows_iter = range.rows();
        let headers: Vec<String> = rows_iter
            .next()
            .map(|r| r.iter().map(cell_to_string).collect())
            .unwrap_or_default();
        let mut rows = Vec::new();
        for r in rows_iter {
            let cells: Vec<String> = r.iter().map(cell_to_string).collect();
            if cells.iter().all(|c| c.trim().is_empty()) {
                continue;
            }
            rows.push(cells);
            if rows.len() > MAX_ROWS {
                return Err(ApiError::validation(format!(
                    "Import at most {MAX_ROWS} rows at a time."
                )));
            }
        }
        Ok((names, vec![chosen], headers, rows))
    } else {
        let text = String::from_utf8_lossy(&bytes);
        let text = text.trim_start_matches('\u{feff}');
        let delimiter = if text.lines().next().unwrap_or("").matches('\t').count()
            > text.lines().next().unwrap_or("").matches(',').count()
        {
            b'\t'
        } else if text.lines().next().unwrap_or("").matches(';').count()
            > text.lines().next().unwrap_or("").matches(',').count()
        {
            b';'
        } else {
            b','
        };
        let mut reader = csv::ReaderBuilder::new()
            .delimiter(delimiter)
            .flexible(true)
            .has_headers(false)
            .trim(csv::Trim::All)
            .from_reader(text.as_bytes());
        let mut headers = Vec::new();
        let mut rows = Vec::new();
        for (i, rec) in reader.records().enumerate() {
            let rec = rec.map_err(|_| {
                ApiError::validation(
                    "The CSV file could not be read. Save it again from Excel as \"CSV UTF-8\" and choose it again.",
                )
            })?;
            let cells: Vec<String> = rec.iter().map(str::to_owned).collect();
            if i == 0 {
                headers = cells;
                continue;
            }
            if cells.iter().all(|c| c.trim().is_empty()) {
                continue;
            }
            rows.push(cells);
            if rows.len() > MAX_ROWS {
                return Err(ApiError::validation(format!(
                    "Import at most {MAX_ROWS} rows at a time."
                )));
            }
        }
        Ok((vec!["csv".into()], vec!["csv".into()], headers, rows))
    }
}

pub fn mapping_key(kind: ImportKind) -> String {
    format!("import.mapping.{}", kind.as_str())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedMapping {
    pub headers: Vec<String>,
    pub mapping: BTreeMap<String, usize>,
}

pub async fn saved_mapping(
    conn: &mut sqlx::PgConnection,
    school_id: Uuid,
    kind: ImportKind,
) -> Result<Option<SavedMapping>, ApiError> {
    Ok(settings::get(conn, Some(school_id), &mapping_key(kind))
        .await?
        .and_then(|v| serde_json::from_value(v).ok()))
}

pub async fn save_mapping(
    state: &AppState,
    actor: Actor,
    school_id: Uuid,
    kind: ImportKind,
    m: &SavedMapping,
) -> Result<(), ApiError> {
    let mut c = state.pool.acquire().await?;
    settings::set(
        &mut c,
        &state.hlc,
        actor,
        Some(school_id),
        &mapping_key(kind),
        serde_json::to_value(m).map_err(|_| ApiError::internal())?,
    )
    .await?;
    Ok(())
}

fn same_headers(a: &[String], b: &[String]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| rules::normalize_header(x) == rules::normalize_header(y))
}

pub async fn parsed(
    conn: &mut sqlx::PgConnection,
    school_id: Uuid,
    kind: ImportKind,
    file_name: &str,
    content_base64: &str,
    sheet: Option<&str>,
) -> Result<ParsedFile, ApiError> {
    let (sheets, chosen, headers, rows) = parse_file(kind, file_name, content_base64, sheet)?;
    if headers.is_empty() {
        return Err(ApiError::validation(
            "The first row must hold the column headings.",
        ));
    }
    let saved = saved_mapping(conn, school_id, kind).await?;
    let (mapping, mapping_saved) = match saved {
        Some(s) if same_headers(&s.headers, &headers) => (s.mapping, true),
        _ => (rules::auto_map(kind, &headers), false),
    };
    Ok(ParsedFile {
        file_name: file_name.to_string(),
        sheets,
        sheet: chosen.into_iter().next().unwrap_or_default(),
        headers,
        rows,
        mapping,
        mapping_saved,
    })
}

// ---- templates ----------------------------------------------------------------------------

/// The name of the sheet a template's records go on (and the first name `pick_sheet` looks
/// for when the filled file comes back).
pub fn data_sheet_name(kind: ImportKind) -> &'static str {
    match kind {
        ImportKind::Students => "Students",
        ImportKind::Staff => "Staff",
        ImportKind::OpeningBalances => "Opening balances",
        ImportKind::Enquiries => "Enquiries",
        ImportKind::Guardians => "Guardians",
        ImportKind::FeeStructure => "Fee structure",
    }
}

/// The template for one kind of import. As a workbook it has three sheets: the records
/// (field labels as headings and one example row), "Read me" in English, and the same
/// instructions in Urdu. As CSV it is the headings and the example only.
pub fn template(kind: ImportKind, xlsx: bool) -> Result<Vec<u8>, ApiError> {
    let fields = rules::fields(kind);
    let headers: Vec<String> = fields.iter().map(|f| f.label.to_string()).collect();
    let example: Vec<String> = fields.iter().map(|f| f.example.to_string()).collect();
    if !xlsx {
        return Ok(crate::export::csv(&headers, &[example]));
    }
    use rust_xlsxwriter::{Format, FormatAlign, Workbook};
    let fail = |_| ApiError::internal();
    let mut wb = Workbook::new();
    let bold = Format::new().set_bold();
    let wrap = Format::new().set_text_wrap().set_align(FormatAlign::Top);
    let title = Format::new().set_bold().set_font_size(14);

    let ws = wb.add_worksheet();
    ws.set_name(data_sheet_name(kind)).map_err(fail)?;
    for (c, f) in fields.iter().enumerate() {
        ws.write_string_with_format(0, c as u16, f.label, &bold)
            .map_err(fail)?;
        // The example is text even when it looks like a number: a phone keeps its zero.
        ws.write_string(1, c as u16, f.example).map_err(fail)?;
        let width = f.label.chars().count().max(f.example.chars().count());
        let _ = ws.set_column_width(c as u16, width.clamp(10, 40) as f64 + 2.0);
    }
    let _ = ws.set_freeze_panes(1, 0);

    let (own, common) = rules::template_notes(kind);
    // (sheet, right-to-left, title, notes, table headings, table rows)
    let english: Vec<[String; 4]> = fields
        .iter()
        .map(|f| {
            [
                f.label.to_string(),
                if f.required { "Yes" } else { "" }.to_string(),
                f.example.to_string(),
                rules::urdu_label(f).unwrap_or("").to_string(),
            ]
        })
        .collect();
    let urdu: Vec<[String; 4]> = fields
        .iter()
        .map(|f| {
            [
                rules::urdu_label(f).unwrap_or(f.label).to_string(),
                if f.required { "ہاں" } else { "" }.to_string(),
                f.example.to_string(),
                f.label.to_string(),
            ]
        })
        .collect();
    /// An instructions sheet: its name, whether it reads right to left, its title, the
    /// notes, and the table of columns (headings and rows).
    type Notes<'a> = (
        &'a str,
        bool,
        &'a str,
        Vec<&'a str>,
        [&'a str; 4],
        &'a Vec<[String; 4]>,
    );
    let sheets: [Notes; 2] = [
        (
            "Read me",
            false,
            "How to fill this file",
            own.en.iter().chain(common.en).copied().collect(),
            [
                "Column",
                "Required",
                "Example",
                "Heading in Urdu (also accepted)",
            ],
            &english,
        ),
        (
            "ہدایات",
            true,
            "یہ فائل کیسے بھریں",
            own.ur.iter().chain(common.ur).copied().collect(),
            [
                "کالم",
                "لازمی",
                "مثال",
                "انگریزی سرخی (فائل میں یہی رہنے دیں)",
            ],
            &urdu,
        ),
    ];
    for (name, rtl, heading, notes, table, rows) in sheets {
        let ws = wb.add_worksheet();
        ws.set_name(name).map_err(fail)?;
        if rtl {
            ws.set_right_to_left(true);
        }
        let _ = ws.set_column_width(0, 44.0);
        let _ = ws.set_column_width(1, 12.0);
        let _ = ws.set_column_width(2, 28.0);
        let _ = ws.set_column_width(3, 40.0);
        ws.write_string_with_format(0, 0, heading, &title)
            .map_err(fail)?;
        let mut r = 2u32;
        for note in notes {
            // One note across the four columns, wrapped, tall enough to be read whole.
            ws.merge_range(r, 0, r, 3, note, &wrap).map_err(fail)?;
            let lines = note.chars().count() / 95 + 1;
            let _ = ws.set_row_height(r, 16.0 * lines as f64 + if rtl { 10.0 } else { 2.0 });
            r += 1;
        }
        r += 1;
        for (c, h) in table.iter().enumerate() {
            ws.write_string_with_format(r, c as u16, *h, &bold)
                .map_err(fail)?;
        }
        for row in rows {
            r += 1;
            for (c, v) in row.iter().enumerate() {
                ws.write_string(r, c as u16, v).map_err(fail)?;
            }
        }
    }
    wb.save_to_buffer().map_err(fail)
}

// ---- preview and apply --------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DuplicatePolicy {
    /// Leave existing records alone and skip the row.
    #[default]
    Skip,
    /// Overwrite the existing record with every non-empty value of the row.
    Update,
    /// Fill only the fields that are empty on the existing record.
    Merge,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ImportOptions {
    #[serde(default)]
    pub duplicates: DuplicatePolicy,
}

/// Mirrored by `ImportRequest` in the UI. Rows are the raw cells of the file.
#[derive(Debug, Clone, Deserialize)]
pub struct ImportRequest {
    pub kind: ImportKind,
    pub file_name: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub mapping: BTreeMap<String, usize>,
    #[serde(default)]
    pub options: ImportOptions,
    /// Remember this mapping for files with the same headings.
    #[serde(default)]
    pub save_mapping: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowAction {
    Create,
    Update,
    Skip,
    Error,
}

/// Mirrored by `RowOutcome` in the UI.
#[derive(Debug, Clone, Serialize)]
pub struct RowOutcome {
    pub row_no: usize,
    pub values: BTreeMap<String, String>,
    pub errors: Vec<Issue>,
    pub warnings: Vec<Issue>,
    pub action: RowAction,
    /// The existing record the row matched, if any.
    pub matched: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ImportSummary {
    pub total: usize,
    pub create: usize,
    pub update: usize,
    pub skip: usize,
    pub error: usize,
}

/// Why some rows will not be imported, said once for all the rows it applies to.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Reason {
    /// `skip` (left alone on purpose) or `error` (cannot be imported as it is).
    pub action: RowAction,
    pub reason: String,
    pub count: usize,
    /// The file's row numbers, the first fifty.
    pub rows: Vec<usize>,
}

/// A class or section the file names and the school has not set up.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Missing {
    pub name: String,
    pub rows: Vec<usize>,
}

/// A row that is not the same as an existing record but looks like one.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Lookalike {
    pub row_no: usize,
    pub name: String,
    pub looks_like: String,
}

/// The dry run in words: what a person needs to read before confirming. Mirrored by
/// `DryRun` in the UI.
#[derive(Debug, Clone, Serialize, Default)]
pub struct DryRun {
    pub reasons: Vec<Reason>,
    pub missing_classes: Vec<Missing>,
    pub missing_sections: Vec<Missing>,
    pub likely_duplicates: Vec<Lookalike>,
}

/// Mirrored by `ImportPreview` in the UI.
#[derive(Debug, Serialize)]
pub struct ImportPreview {
    pub rows: Vec<RowOutcome>,
    pub summary: ImportSummary,
    /// File columns not mapped to any field: shown before import so nothing is dropped unseen.
    pub unmapped: Vec<String>,
    pub report: DryRun,
}

#[derive(Debug, Serialize)]
pub struct ImportResult {
    pub batch: ImportBatch,
    pub summary: ImportSummary,
    pub rows: Vec<RowOutcome>,
}

struct Ctx {
    school_id: Uuid,
    rules: rules::RowRules,
    year_id: Option<Uuid>,
    year_start: Option<NaiveDate>,
    classes: Vec<Class>,
    sections: Vec<Section>,
    /// Fee heads and this year's amounts (fee structure imports).
    heads: Vec<FeeHead>,
    structure: Vec<FeeStructure>,
    /// Students on file by name, for rows that look like one of them (student imports).
    identities: HashMap<String, Vec<students::Identity>>,
}

async fn ctx(
    conn: &mut sqlx::PgConnection,
    school_id: Uuid,
    kind: ImportKind,
) -> Result<Ctx, ApiError> {
    let year = academics::active_year(conn, school_id).await?;
    let classes = academics::list_classes(conn, school_id).await?;
    let sections = match &year {
        Some(y) => academics::list_sections(conn, school_id, Some(y.id), None).await?,
        None => Vec::new(),
    };
    let school = schools::get(conn, school_id).await?;
    let (heads, structure) = match (kind, &year) {
        (ImportKind::FeeStructure, Some(y)) => (
            fees::list_heads(conn, school_id).await?,
            fees::list_structure(conn, y.id, None).await?,
        ),
        (ImportKind::FeeStructure, None) => (fees::list_heads(conn, school_id).await?, Vec::new()),
        _ => (Vec::new(), Vec::new()),
    };
    let mut identities: HashMap<String, Vec<students::Identity>> = HashMap::new();
    if kind == ImportKind::Students {
        for s in students::identities(conn, school_id).await? {
            identities
                .entry(madrasa_core::students::name_key(&s.full_name))
                .or_default()
                .push(s);
        }
    }
    Ok(Ctx {
        school_id,
        rules: rules::RowRules::new(&school.country, crate::clock::today()),
        year_id: year.as_ref().map(|y| y.id),
        year_start: year.as_ref().map(|y| y.start_date),
        classes,
        sections,
        heads,
        structure,
        identities,
    })
}

impl Ctx {
    fn class(&self, key: &str) -> Option<&Class> {
        self.classes
            .iter()
            .find(|c| rules::class_key(&c.name) == key)
    }
    fn section(&self, class_id: Uuid, name: &str) -> Option<&Section> {
        self.sections
            .iter()
            .find(|s| s.class_id == class_id && s.name.eq_ignore_ascii_case(name))
    }
}

fn summarize(rows: &[RowOutcome]) -> ImportSummary {
    let mut s = ImportSummary {
        total: rows.len(),
        ..Default::default()
    };
    for r in rows {
        match r.action {
            RowAction::Create => s.create += 1,
            RowAction::Update => s.update += 1,
            RowAction::Skip => s.skip += 1,
            RowAction::Error => s.error += 1,
        }
    }
    s
}

fn outcome(row: CleanRow, action: RowAction, matched: Option<String>) -> RowOutcome {
    RowOutcome {
        row_no: row.row_no,
        values: row.values,
        errors: row.errors,
        warnings: row.warnings,
        action,
        matched,
    }
}

/// What a row would do, decided against the database. Shared by preview and apply.
struct Plan {
    row: CleanRow,
    action: RowAction,
    matched: Option<String>,
    existing_student: Option<Student>,
    existing_staff: Option<madrasa_db::models::Staff>,
    section_id: Option<Uuid>,
    target_student: Option<Student>,
    /// For the dry-run report.
    missing_class: Option<String>,
    missing_section: Option<String>,
    lookalike: Option<String>,
    /// A fee structure row, resolved.
    fee: Option<FeePlan>,
}

/// One line of the fee structure: the class, the head (to be created when `head_id` is
/// none) and the amount, with what is set now.
#[derive(Debug, Clone)]
struct FeePlan {
    class_id: Uuid,
    head_id: Option<Uuid>,
    head_name: String,
    amount: Decimal,
    old: Option<Decimal>,
}

async fn plan_rows(
    conn: &mut sqlx::PgConnection,
    c: &Ctx,
    req: &ImportRequest,
) -> Result<Vec<Plan>, ApiError> {
    let mut plans = Vec::with_capacity(req.rows.len());
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut seen_numbers: HashMap<String, usize> = HashMap::new();
    // Guardian names already seen under each phone in this file (by family id when given,
    // so one family's rows never warn against each other).
    let mut seen_phones: HashMap<String, Vec<String>> = HashMap::new();
    // Name and father's name of the rows so far (rows that look alike inside the file).
    let mut seen_names: HashMap<(String, String), usize> = HashMap::new();
    for (i, raw) in req.rows.iter().enumerate() {
        let row_no = i + 2; // 1 is the heading row
        let mut row = rules::clean_row(req.kind, row_no, raw, &req.mapping, &c.rules);
        let mut matched = None;
        let mut existing_student = None;
        let mut existing_staff = None;
        let mut section_id = None;
        let mut target_student = None;
        let mut missing_class = None;
        let mut missing_section = None;
        let mut lookalike = None;
        let mut fee: Option<FeePlan> = None;
        // The row is left alone on purpose, and why.
        let mut skip_reason: Option<String> = None;
        match req.kind {
            ImportKind::Students => {
                // Class and section must exist in the active year.
                if let Some(class_key) = row.get("class").map(str::to_owned) {
                    match c.class(&class_key) {
                        None => {
                            missing_class = Some(row.values["class"].clone());
                            row.errors.push(Issue {
                                field: "class".into(),
                                message: format!(
                                    "Class \"{}\" is not set up in Settings",
                                    row.values["class"]
                                ),
                            })
                        }
                        Some(class) => {
                            if c.year_id.is_none() {
                                row.errors.push(Issue {
                                    field: "class".into(),
                                    message: "No active academic year".into(),
                                });
                            } else if let Some(sec) = row.get("section").map(str::to_owned) {
                                match c.section(class.id, &sec) {
                                    Some(s) => section_id = Some(s.id),
                                    None => {
                                        missing_section = Some(format!("{}-{sec}", class.name));
                                        row.errors.push(Issue {
                                            field: "section".into(),
                                            message: format!(
                                                "Section \"{sec}\" of class {} is not set up",
                                                class.name
                                            ),
                                        })
                                    }
                                }
                            } else {
                                let mut secs = c.sections.iter().filter(|s| s.class_id == class.id);
                                match (secs.next(), secs.next()) {
                                    (Some(only), None) => {
                                        section_id = Some(only.id);
                                        row.warnings.push(Issue {
                                            field: "section".into(),
                                            message: format!(
                                                "Section not given; using {}",
                                                only.name
                                            ),
                                        });
                                    }
                                    (Some(_), Some(_)) => row.errors.push(Issue {
                                        field: "section".into(),
                                        message: format!(
                                            "Class {} has several sections; give the section",
                                            class.name
                                        ),
                                    }),
                                    (None, _) => row.errors.push(Issue {
                                        field: "section".into(),
                                        message: format!(
                                            "Class {} has no section in the active year",
                                            class.name
                                        ),
                                    }),
                                }
                            }
                        }
                    }
                }
                // A phone already on file under another name: a separate guardian will be
                // created (a phone alone never merges people), and the office should know.
                for slot in ["guardian", "guardian2"] {
                    let (Some(name), Some(phone)) = (
                        row.get(&format!("{slot}_name")),
                        row.get(&format!("{slot}_phone")),
                    ) else {
                        continue;
                    };
                    let mut others: Vec<String> =
                        guardians::find_by_phone(conn, c.school_id, phone)
                            .await?
                            .into_iter()
                            .filter(|g| !g.full_name.eq_ignore_ascii_case(name.trim()))
                            .map(|g| g.full_name)
                            .collect();
                    let in_file = seen_phones.entry(phone.to_string()).or_default();
                    for other in in_file
                        .iter()
                        .filter(|n| !n.eq_ignore_ascii_case(name.trim()))
                    {
                        if !others.iter().any(|o| o.eq_ignore_ascii_case(other)) {
                            others.push(other.clone());
                        }
                    }
                    if !in_file.iter().any(|n| n.eq_ignore_ascii_case(name.trim())) {
                        in_file.push(name.trim().to_string());
                    }
                    if !others.is_empty() {
                        row.warnings.push(Issue {
                            field: format!("{slot}_phone"),
                            message: format!(
                                "Phone {phone} is already recorded for {}; {name} will be a separate guardian",
                                others.join(", ")
                            ),
                        });
                    }
                }
                // Duplicates inside the file.
                if let Some(key) = rules::student_file_key(&row) {
                    if let Some(prev) = seen.get(&key) {
                        row.errors.push(Issue {
                            field: "full_name".into(),
                            message: format!("Same student as row {prev} in this file"),
                        });
                    } else {
                        seen.insert(key, row_no);
                    }
                }
                if let Some(n) = row.get("student_number").map(str::to_uppercase) {
                    if let Some(prev) = seen_numbers.get(&n) {
                        row.errors.push(Issue {
                            field: "student_number".into(),
                            message: format!("Same student number as row {prev} in this file"),
                        });
                    } else {
                        seen_numbers.insert(n, row_no);
                    }
                }
                // Existing records: by student number, then by CNIC / name + DOB + father.
                if row.errors.is_empty() {
                    if let Some(n) = row.get("student_number") {
                        existing_student = students::find_by_number(conn, c.school_id, n).await?;
                    }
                    if existing_student.is_none() {
                        if let (Some(name), Some(father), Some(dob)) = (
                            row.get("full_name"),
                            row.get("father_name"),
                            row.get("date_of_birth"),
                        ) {
                            let dob: NaiveDate = dob.parse().map_err(|_| ApiError::internal())?;
                            let probe = madrasa_core::students::DuplicateProbe {
                                full_name: name.to_string(),
                                father_name: father.to_string(),
                                date_of_birth: dob,
                                cnic: row.get("cnic").map(str::to_owned),
                            };
                            existing_student =
                                students::find_duplicates(conn, c.school_id, &probe, None)
                                    .await?
                                    .into_iter()
                                    .next()
                                    .map(|m| m.student);
                        }
                    }
                    if let Some(s) = &existing_student {
                        matched = Some(format!("{} ({})", s.full_name, s.student_number));
                    }
                }
                // Not the same record, but close: the same name with the same father or the
                // same birthday. Imported all the same; the office is told to look.
                if row.errors.is_empty() && existing_student.is_none() {
                    if let (Some(name), Some(father)) =
                        (row.get("full_name"), row.get("father_name"))
                    {
                        let key = madrasa_core::students::name_key(name);
                        let father_key = madrasa_core::students::name_key(father);
                        let dob = row
                            .get("date_of_birth")
                            .and_then(|d| d.parse::<NaiveDate>().ok());
                        let on_file = c.identities.get(&key).and_then(|same_name| {
                            same_name.iter().find_map(|s| {
                                let same_father =
                                    madrasa_core::students::name_key(&s.father_name) == father_key;
                                let same_birthday = dob == Some(s.date_of_birth);
                                match (same_father, same_birthday) {
                                    (true, false) => Some(format!(
                                        "{} ({}), same father's name, born {}",
                                        s.full_name, s.student_number, s.date_of_birth
                                    )),
                                    (false, true) => Some(format!(
                                        "{} ({}), same date of birth, father {}",
                                        s.full_name, s.student_number, s.father_name
                                    )),
                                    _ => None,
                                }
                            })
                        });
                        let in_file = seen_names
                            .get(&(key.clone(), father_key.clone()))
                            .map(|prev| format!("row {prev} of this file (another date of birth)"));
                        seen_names.entry((key, father_key)).or_insert(row_no);
                        if let Some(like) = on_file.or(in_file) {
                            row.warnings.push(Issue {
                                field: "full_name".into(),
                                message: format!(
                                    "Looks like {like}. Check that this is another child before importing"
                                ),
                            });
                            lookalike = Some(like);
                        }
                    }
                }
            }
            ImportKind::Guardians => {
                if row.errors.is_empty() {
                    let found = match (row.get("student_number"), row.get("legacy_id")) {
                        (Some(n), _) => students::find_by_number(conn, c.school_id, n).await?,
                        (None, Some(l)) => {
                            students::find_by_legacy_id(conn, c.school_id, l).await?
                        }
                        _ => None,
                    };
                    match found {
                        None => row.errors.push(Issue {
                            field: "student_number".into(),
                            message: "No student with this number or legacy ID".into(),
                        }),
                        Some(s) => {
                            let name = row.get("guardian_name").unwrap_or("").to_string();
                            let key =
                                format!("{}|{}", s.id, madrasa_core::students::name_key(&name));
                            if let Some(prev) = seen.get(&key) {
                                row.errors.push(Issue {
                                    field: "guardian_name".into(),
                                    message: format!(
                                        "Same guardian and student as row {prev} in this file"
                                    ),
                                });
                            } else {
                                seen.insert(key, row_no);
                            }
                            matched = Some(format!("{} ({})", s.full_name, s.student_number));
                            // Known already (same ID number, or same phone and name)?
                            let known = guardians::find_matching(
                                conn,
                                c.school_id,
                                row.get("guardian_cnic"),
                                row.get("guardian_phone"),
                                &name,
                            )
                            .await?;
                            if let Some(g) = known {
                                if guardians::link_between(conn, s.id, g.id).await?.is_some() {
                                    skip_reason = Some(format!(
                                        "{} is already a guardian of this student",
                                        g.full_name
                                    ));
                                } else {
                                    row.warnings.push(Issue {
                                        field: "guardian_name".into(),
                                        message: format!(
                                            "{} is already on file and will be linked to this student, not created again",
                                            g.full_name
                                        ),
                                    });
                                }
                            }
                            target_student = Some(s);
                        }
                    }
                }
            }
            ImportKind::FeeStructure => {
                let class = row.get("class").map(str::to_owned).and_then(|key| {
                    let found = c.class(&key).cloned();
                    if found.is_none() {
                        missing_class = Some(row.values["class"].clone());
                    }
                    found
                });
                if let Some(name) = &missing_class {
                    row.errors.push(Issue {
                        field: "class".into(),
                        message: format!("Class \"{name}\" is not set up in Settings"),
                    });
                }
                if c.year_id.is_none() {
                    row.errors.push(Issue {
                        field: "amount".into(),
                        message: "No active academic year".into(),
                    });
                }
                if let (true, Some(class), Some(head_name), Some(amount)) = (
                    row.errors.is_empty(),
                    class,
                    row.get("fee_head").map(str::to_owned),
                    row.get("amount").and_then(|a| a.parse::<Decimal>().ok()),
                ) {
                    let key = format!("{}|{}", class.id, head_name.to_lowercase());
                    if let Some(prev) = seen.get(&key) {
                        row.errors.push(Issue {
                            field: "fee_head".into(),
                            message: format!("Same class and fee head as row {prev} in this file"),
                        });
                    } else {
                        seen.insert(key, row_no);
                        let head = c.heads.iter().find(|h| {
                            h.name.eq_ignore_ascii_case(&head_name)
                                || h.code.eq_ignore_ascii_case(&head_name)
                        });
                        let old = head.and_then(|h| {
                            c.structure
                                .iter()
                                .find(|s| s.class_id == class.id && s.fee_head_id == h.id)
                                .map(|s| s.amount)
                        });
                        match (head, old) {
                            (_, Some(now)) if now == amount => {
                                skip_reason = Some("Already set to this amount".into());
                            }
                            (_, None) if amount.is_zero() => {
                                skip_reason =
                                    Some("Amount is 0 and nothing is set: nothing to remove".into());
                            }
                            (None, _) => row.warnings.push(Issue {
                                field: "fee_head".into(),
                                message: format!(
                                    "Fee head \"{head_name}\" does not exist yet and will be created"
                                ),
                            }),
                            _ => {}
                        }
                        if let Some(now) = old.filter(|now| *now != amount) {
                            matched =
                                Some(format!("{} for class {}: now {now}", head_name, class.name));
                        }
                        fee = Some(FeePlan {
                            class_id: class.id,
                            head_id: head.map(|h| h.id),
                            head_name,
                            amount,
                            old,
                        });
                    }
                }
            }
            ImportKind::Staff => {
                if row.errors.is_empty() {
                    if let Some(n) = row.get("staff_number").map(str::to_owned) {
                        if let Some(prev) = seen_numbers.get(&n.to_uppercase()) {
                            row.errors.push(Issue {
                                field: "staff_number".into(),
                                message: format!("Same staff number as row {prev} in this file"),
                            });
                        } else {
                            seen_numbers.insert(n.to_uppercase(), row_no);
                        }
                        existing_staff = staff::find_by_number(conn, c.school_id, &n).await?;
                    }
                    if existing_staff.is_none() {
                        if let Some(cnic) = row.get("cnic") {
                            existing_staff = staff::find_by_cnic(conn, c.school_id, cnic).await?;
                        }
                    }
                    if let Some(s) = &existing_staff {
                        matched = Some(format!("{} ({})", s.full_name, s.staff_number));
                    }
                }
            }
            ImportKind::Enquiries => {
                // The class applied for must exist when given; the rest is free text.
                if let Some(class_key) = row.get("class").map(str::to_owned) {
                    if c.class(&class_key).is_none() {
                        row.errors.push(Issue {
                            field: "class".into(),
                            message: format!(
                                "Class \"{}\" is not set up in Settings",
                                row.values["class"]
                            ),
                        });
                    }
                }
            }
            ImportKind::OpeningBalances => {
                if row.errors.is_empty() {
                    let found = match (row.get("student_number"), row.get("legacy_id")) {
                        (Some(n), _) => students::find_by_number(conn, c.school_id, n).await?,
                        (None, Some(l)) => {
                            students::find_by_legacy_id(conn, c.school_id, l).await?
                        }
                        _ => None,
                    };
                    match found {
                        Some(s) => {
                            if c.year_id.is_none() {
                                row.errors.push(Issue {
                                    field: "amount".into(),
                                    message: "No active academic year".into(),
                                });
                            }
                            let key = s.id.to_string();
                            if let Some(prev) = seen.get(&key) {
                                row.errors.push(Issue {
                                    field: "student_number".into(),
                                    message: format!("Same student as row {prev} in this file"),
                                });
                            } else {
                                seen.insert(key, row_no);
                            }
                            matched = Some(format!("{} ({})", s.full_name, s.student_number));
                            target_student = Some(s);
                        }
                        None => row.errors.push(Issue {
                            field: "student_number".into(),
                            message: "No student with this number or legacy ID".into(),
                        }),
                    }
                }
            }
        }
        let already_set = fee.as_ref().is_some_and(|f| f.old.is_some());
        let action = if !row.errors.is_empty() {
            RowAction::Error
        } else if let Some(reason) = skip_reason {
            row.warnings.push(Issue {
                field: rules::fields(req.kind)[0].key.into(),
                message: format!("{reason}; skipped"),
            });
            RowAction::Skip
        } else if existing_student.is_some() || existing_staff.is_some() || already_set {
            match req.options.duplicates {
                DuplicatePolicy::Skip => {
                    row.warnings.push(Issue {
                        field: if already_set { "amount" } else { "full_name" }.into(),
                        message: if already_set {
                            "An amount is already set; skipped (choose \"update\" to replace it)"
                        } else {
                            "Already exists; skipped"
                        }
                        .into(),
                    });
                    RowAction::Skip
                }
                _ => RowAction::Update,
            }
        } else {
            RowAction::Create
        };
        plans.push(Plan {
            row,
            action,
            matched,
            existing_student,
            existing_staff,
            section_id,
            target_student,
            missing_class,
            missing_section,
            lookalike,
            fee,
        });
    }
    Ok(plans)
}

pub async fn preview(
    state: &AppState,
    school_id: Uuid,
    req: &ImportRequest,
) -> Result<ImportPreview, ApiError> {
    if req.rows.len() > MAX_ROWS {
        return Err(ApiError::validation(format!(
            "Import at most {MAX_ROWS} rows at a time."
        )));
    }
    let mut c = state.pool.acquire().await?;
    let ctx = ctx(&mut c, school_id, req.kind).await?;
    let plans = plan_rows(&mut c, &ctx, req).await?;
    let report = dry_run(&plans);
    let rows: Vec<RowOutcome> = plans
        .into_iter()
        .map(|p| outcome(p.row, p.action, p.matched))
        .collect();
    let summary = summarize(&rows);
    let unmapped = req
        .headers
        .iter()
        .enumerate()
        .filter(|(i, h)| !h.trim().is_empty() && !req.mapping.values().any(|v| v == i))
        .map(|(_, h)| h.clone())
        .collect();
    Ok(ImportPreview {
        rows,
        summary,
        unmapped,
        report,
    })
}

/// The same reason said about different rows is one reason: "row 12" becomes "an earlier
/// row", so forty repeats are one line with forty row numbers.
fn general(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let mut words = message.split(' ').peekable();
    while let Some(w) = words.next() {
        if !out.is_empty() {
            out.push(' ');
        }
        let number_next = words
            .peek()
            .is_some_and(|n| n.chars().next().is_some_and(|c| c.is_ascii_digit()));
        if w == "row" && number_next {
            out.push_str("an earlier row");
            words.next();
        } else {
            out.push_str(w);
        }
    }
    out
}

/// The dry run in words, from the planned rows.
fn dry_run(plans: &[Plan]) -> DryRun {
    const ROWS_SHOWN: usize = 50;
    fn note(list: &mut Vec<Missing>, name: &str, row_no: usize) {
        match list.iter_mut().find(|m| m.name.eq_ignore_ascii_case(name)) {
            Some(m) => m.rows.push(row_no),
            None => list.push(Missing {
                name: name.to_string(),
                rows: vec![row_no],
            }),
        }
    }
    let mut report = DryRun::default();
    for p in plans {
        let row_no = p.row.row_no;
        if let Some(c) = &p.missing_class {
            note(&mut report.missing_classes, c, row_no);
        }
        if let Some(s) = &p.missing_section {
            note(&mut report.missing_sections, s, row_no);
        }
        if let Some(like) = &p.lookalike {
            report.likely_duplicates.push(Lookalike {
                row_no,
                name: p
                    .row
                    .get("full_name")
                    .or(p.row.get("guardian_name"))
                    .unwrap_or("")
                    .to_string(),
                looks_like: like.clone(),
            });
        }
        let why: Vec<String> = match p.action {
            RowAction::Error => p.row.errors.iter().map(|e| general(&e.message)).collect(),
            // The warning that says why it is skipped is the last one added.
            RowAction::Skip => p
                .row
                .warnings
                .last()
                .map(|w| general(&w.message))
                .into_iter()
                .collect(),
            _ => Vec::new(),
        };
        for reason in why {
            match report
                .reasons
                .iter_mut()
                .find(|r| r.action == p.action && r.reason == reason)
            {
                Some(r) => {
                    r.count += 1;
                    if r.rows.len() < ROWS_SHOWN {
                        r.rows.push(row_no);
                    }
                }
                None => report.reasons.push(Reason {
                    action: p.action,
                    reason,
                    count: 1,
                    rows: vec![row_no],
                }),
            }
        }
    }
    report.reasons.sort_by_key(|r| std::cmp::Reverse(r.count));
    report
}

/// The dry run as a workbook to keep or pass on: a summary with the reasons, every row with
/// what would happen to it and why, what is missing, and the likely duplicates.
pub fn report_workbook(req: &ImportRequest, preview: &ImportPreview) -> Result<Vec<u8>, ApiError> {
    use crate::export::Sheet;
    let text = |a: RowAction| match a {
        RowAction::Create => "Will be created",
        RowAction::Update => "Will be updated",
        RowAction::Skip => "Will be skipped",
        RowAction::Error => "Cannot be imported",
    };
    let rows_of = |rows: &[usize]| {
        rows.iter()
            .map(|r| r.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let s = &preview.summary;
    let mut summary: Vec<Vec<String>> = vec![
        vec!["File".into(), req.file_name.clone()],
        vec!["Kind".into(), data_sheet_name(req.kind).into()],
        vec!["Checked on".into(), crate::clock::today().to_string()],
        vec!["Nothing has been saved".into(), "This is a dry run.".into()],
        vec![],
        vec!["Rows in the file".into(), s.total.to_string()],
        vec!["Will be created".into(), s.create.to_string()],
        vec!["Will be updated".into(), s.update.to_string()],
        vec!["Will be skipped".into(), s.skip.to_string()],
        vec!["Cannot be imported".into(), s.error.to_string()],
        vec![],
        vec!["Why".into(), "Rows".into(), "Row numbers".into()],
    ];
    for r in &preview.report.reasons {
        summary.push(vec![
            format!("{}: {}", text(r.action), r.reason),
            r.count.to_string(),
            rows_of(&r.rows),
        ]);
    }
    let fields = rules::fields(req.kind);
    let mut headers: Vec<String> = vec![
        "Row".into(),
        "What will happen".into(),
        "Why".into(),
        "Matches".into(),
    ];
    headers.extend(fields.iter().map(|f| f.label.to_string()));
    let rows: Vec<Vec<String>> = preview
        .rows
        .iter()
        .map(|r| {
            let why = r
                .errors
                .iter()
                .chain(&r.warnings)
                .map(|i| i.message.clone())
                .collect::<Vec<_>>()
                .join("; ");
            let mut line = vec![
                r.row_no.to_string(),
                text(r.action).to_string(),
                why,
                r.matched.clone().unwrap_or_default(),
            ];
            line.extend(
                fields
                    .iter()
                    .map(|f| r.values.get(f.key).cloned().unwrap_or_default()),
            );
            line
        })
        .collect();
    let missing: Vec<Vec<String>> = preview
        .report
        .missing_classes
        .iter()
        .map(|m| ("Class", m))
        .chain(
            preview
                .report
                .missing_sections
                .iter()
                .map(|m| ("Section", m)),
        )
        .map(|(what, m)| {
            vec![
                what.to_string(),
                m.name.clone(),
                m.rows.len().to_string(),
                rows_of(&m.rows),
            ]
        })
        .collect();
    let alike: Vec<Vec<String>> = preview
        .report
        .likely_duplicates
        .iter()
        .map(|l| vec![l.row_no.to_string(), l.name.clone(), l.looks_like.clone()])
        .collect();
    crate::export::workbook(&[
        Sheet::text("Summary", &["Dry run", ""], summary),
        Sheet::text(
            "Rows",
            &headers.iter().map(String::as_str).collect::<Vec<_>>(),
            rows,
        ),
        Sheet::text(
            "Missing classes, sections",
            &["Missing", "Name", "Rows", "Row numbers"],
            missing,
        ),
        Sheet::text(
            "Likely duplicates",
            &["Row", "Name in the file", "Looks like"],
            alike,
        ),
    ])
}

/// Extra student fields kept in `custom`: family id and address parts.
fn student_custom(r: &CleanRow, base: serde_json::Value) -> serde_json::Value {
    merge_custom(r, base, &["family_id", "city", "state", "postal_code"])
}

/// Extra staff fields kept in `custom`: staff type, subject/department, grades taught.
fn staff_custom(r: &CleanRow, base: serde_json::Value) -> serde_json::Value {
    merge_custom(r, base, &["staff_type", "department", "grades"])
}

fn merge_custom(r: &CleanRow, base: serde_json::Value, keys: &[&str]) -> serde_json::Value {
    let mut obj = match base {
        serde_json::Value::Object(o) => o,
        _ => serde_json::Map::new(),
    };
    for key in keys {
        if let Some(v) = r.get(key) {
            obj.insert((*key).into(), serde_json::Value::String(v.to_string()));
        }
    }
    serde_json::Value::Object(obj)
}

fn pick(row: &CleanRow, key: &str) -> Option<String> {
    row.get(key).map(str::to_owned)
}

fn merged(policy: DuplicatePolicy, current: &str, incoming: Option<String>) -> Option<String> {
    match (policy, incoming) {
        (_, None) => None,
        (DuplicatePolicy::Merge, Some(v)) => current.is_empty().then_some(v),
        (_, Some(v)) => (v != current).then_some(v),
    }
}

fn merged_opt(
    policy: DuplicatePolicy,
    current: Option<&str>,
    incoming: Option<String>,
) -> Option<Option<String>> {
    merged(policy, current.unwrap_or(""), incoming).map(Some)
}

/// Apply the import inside one transaction. Rows with errors are reported, not imported.
pub async fn apply(
    state: &AppState,
    actor: Actor,
    user_id: Uuid,
    school_id: Uuid,
    req: &ImportRequest,
) -> Result<ImportResult, ApiError> {
    if req.rows.len() > MAX_ROWS {
        return Err(ApiError::validation(format!(
            "Import at most {MAX_ROWS} rows at a time."
        )));
    }
    let mut tx = state.pool.begin().await?;
    let ctx = ctx(&mut tx, school_id, req.kind).await?;
    let plans = plan_rows(&mut tx, &ctx, req).await?;
    let batch = repo::create_batch(
        &mut tx,
        &state.hlc,
        actor,
        repo::NewBatch {
            school_id,
            kind: req.kind.as_str().into(),
            file_name: req.file_name.clone(),
            options: serde_json::to_value(&req.options).unwrap_or_default(),
            total_rows: req.rows.len() as i32,
            applied_by: user_id,
        },
    )
    .await?;
    let policy = req.options.duplicates;
    let mut outcomes = Vec::with_capacity(plans.len());
    let mut used_guardians: HashSet<Uuid> = HashSet::new();
    // Fee heads this batch has created, by lower-case name (two classes, one new head).
    let mut new_heads: HashMap<String, Uuid> = HashMap::new();
    for p in plans {
        let row_no = p.row.row_no as i32;
        match (p.action, req.kind) {
            (RowAction::Error, _) | (RowAction::Skip, _) => {
                if p.action == RowAction::Skip {
                    repo::add_row(
                        &mut tx,
                        &state.hlc,
                        actor,
                        &batch,
                        row_no,
                        "skipped",
                        req.kind.as_str(),
                        None,
                        None,
                    )
                    .await?;
                }
            }
            (RowAction::Create, ImportKind::Students) => {
                let r = &p.row;
                let student = students::create(
                    &mut tx,
                    &state.hlc,
                    actor,
                    students::NewStudent {
                        school_id,
                        student_number: pick(r, "student_number"),
                        legacy_id: pick(r, "legacy_id"),
                        full_name: r.get("full_name").unwrap_or("").into(),
                        father_name: r.get("father_name").unwrap_or("").into(),
                        date_of_birth: r
                            .get("date_of_birth")
                            .and_then(|d| d.parse().ok())
                            .unwrap_or_default(),
                        gender: r.get("gender").unwrap_or("other").into(),
                        cnic: pick(r, "cnic"),
                        address: r.get("address").unwrap_or("").into(),
                        admission_date: r
                            .get("admission_date")
                            .and_then(|d| d.parse().ok())
                            .or(ctx.year_start)
                            .unwrap_or_else(crate::clock::today),
                        board_roll_no: pick(r, "board_roll_no"),
                        board_registration_no: None,
                        custom: student_custom(r, serde_json::json!({ "imported": true })),
                    },
                )
                .await?;
                if let Some(status) = r.get("status").filter(|st| *st != "active") {
                    students::set_status(
                        &mut tx,
                        &state.hlc,
                        actor,
                        student.id,
                        status,
                        Some(crate::clock::today()),
                        Some("imported"),
                    )
                    .await?;
                }
                repo::add_row(
                    &mut tx,
                    &state.hlc,
                    actor,
                    &batch,
                    row_no,
                    "created",
                    "student",
                    Some(student.id),
                    None,
                )
                .await?;
                if let Some(sid) = p.section_id {
                    let roll = r.get("roll_no").and_then(|v| v.parse().ok());
                    let e =
                        students::enrol(&mut tx, &state.hlc, actor, student.id, sid, roll).await?;
                    repo::add_row(
                        &mut tx,
                        &state.hlc,
                        actor,
                        &batch,
                        row_no,
                        "created",
                        "enrollment",
                        Some(e.id),
                        None,
                    )
                    .await?;
                }
                attach_guardian(
                    &mut tx,
                    state,
                    actor,
                    &batch,
                    row_no,
                    school_id,
                    student.id,
                    r,
                    &mut used_guardians,
                )
                .await?;
            }
            (RowAction::Update, ImportKind::Students) => {
                let r = &p.row;
                let s = p.existing_student.clone().ok_or_else(ApiError::internal)?;
                let old = serde_json::json!({
                    "legacy_id": s.legacy_id, "full_name": s.full_name, "father_name": s.father_name,
                    "date_of_birth": s.date_of_birth, "gender": s.gender, "cnic": s.cnic, "address": s.address,
                    "admission_date": s.admission_date, "board_roll_no": s.board_roll_no,
                });
                let upd = students::StudentUpdate {
                    legacy_id: merged_opt(policy, s.legacy_id.as_deref(), pick(r, "legacy_id")),
                    full_name: merged(policy, &s.full_name, pick(r, "full_name")),
                    father_name: merged(policy, &s.father_name, pick(r, "father_name")),
                    date_of_birth: merged(
                        policy,
                        &s.date_of_birth.to_string(),
                        pick(r, "date_of_birth"),
                    )
                    .and_then(|d| d.parse().ok()),
                    gender: merged(policy, &s.gender, pick(r, "gender")),
                    cnic: merged_opt(policy, s.cnic.as_deref(), pick(r, "cnic")),
                    address: merged(policy, &s.address, pick(r, "address")),
                    admission_date: merged(
                        policy,
                        &s.admission_date.to_string(),
                        pick(r, "admission_date"),
                    )
                    .and_then(|d| d.parse().ok()),
                    board_roll_no: merged_opt(
                        policy,
                        s.board_roll_no.as_deref(),
                        pick(r, "board_roll_no"),
                    ),
                    board_registration_no: None,
                    custom: Some(student_custom(r, s.custom.clone())),
                };
                students::update(&mut tx, &state.hlc, actor, s.id, upd).await?;
                if let Some(status) = r.get("status").filter(|st| *st != s.status) {
                    students::set_status(
                        &mut tx,
                        &state.hlc,
                        actor,
                        s.id,
                        status,
                        Some(crate::clock::today()),
                        Some("imported"),
                    )
                    .await?;
                }
                repo::add_row(
                    &mut tx,
                    &state.hlc,
                    actor,
                    &batch,
                    row_no,
                    "updated",
                    "student",
                    Some(s.id),
                    Some(old),
                )
                .await?;
                if let (Some(sid), Some(year)) = (p.section_id, ctx.year_id) {
                    if students::section_in_year(&mut tx, s.id, year)
                        .await?
                        .is_none()
                    {
                        let roll = r.get("roll_no").and_then(|v| v.parse().ok());
                        let e =
                            students::enrol(&mut tx, &state.hlc, actor, s.id, sid, roll).await?;
                        repo::add_row(
                            &mut tx,
                            &state.hlc,
                            actor,
                            &batch,
                            row_no,
                            "created",
                            "enrollment",
                            Some(e.id),
                            None,
                        )
                        .await?;
                    }
                }
                if guardians::for_student(&mut tx, s.id).await?.is_empty() {
                    attach_guardian(
                        &mut tx,
                        state,
                        actor,
                        &batch,
                        row_no,
                        school_id,
                        s.id,
                        r,
                        &mut used_guardians,
                    )
                    .await?;
                }
            }
            (RowAction::Create, ImportKind::Staff) => {
                let r = &p.row;
                let number = match pick(r, "staff_number") {
                    Some(n) => n,
                    None => format!(
                        "S-{:04}",
                        students::next_sequence(&mut tx, &state.hlc, school_id, "staff_number")
                            .await?
                    ),
                };
                let member = staff::create(
                    &mut tx,
                    &state.hlc,
                    actor,
                    staff::NewStaff {
                        school_id,
                        user_id: None,
                        staff_number: number,
                        full_name: r.get("full_name").unwrap_or("").into(),
                        father_name: r.get("father_name").unwrap_or("").into(),
                        gender: r.get("gender").unwrap_or("other").into(),
                        phone: pick(r, "phone"),
                        email: pick(r, "email"),
                        cnic: pick(r, "cnic"),
                        designation: r.get("designation").unwrap_or("").into(),
                        joined_on: r.get("joined_on").and_then(|d| d.parse().ok()),
                        custom: staff_custom(r, serde_json::json!({ "imported": true })),
                    },
                )
                .await?;
                repo::add_row(
                    &mut tx,
                    &state.hlc,
                    actor,
                    &batch,
                    row_no,
                    "created",
                    "staff",
                    Some(member.id),
                    None,
                )
                .await?;
            }
            (RowAction::Update, ImportKind::Staff) => {
                let r = &p.row;
                let s = p.existing_staff.clone().ok_or_else(ApiError::internal)?;
                let old = serde_json::json!({
                    "full_name": s.full_name, "father_name": s.father_name, "gender": s.gender, "phone": s.phone,
                    "email": s.email, "cnic": s.cnic, "designation": s.designation, "joined_on": s.joined_on,
                });
                staff::update(
                    &mut tx,
                    &state.hlc,
                    actor,
                    s.id,
                    staff::StaffUpdate {
                        user_id: None,
                        full_name: merged(policy, &s.full_name, pick(r, "full_name")),
                        father_name: merged(policy, &s.father_name, pick(r, "father_name")),
                        gender: merged(policy, &s.gender, pick(r, "gender")),
                        phone: merged_opt(policy, s.phone.as_deref(), pick(r, "phone")),
                        email: merged_opt(policy, s.email.as_deref(), pick(r, "email")),
                        cnic: merged_opt(policy, s.cnic.as_deref(), pick(r, "cnic")),
                        designation: merged(policy, &s.designation, pick(r, "designation")),
                        joined_on: merged_opt(
                            policy,
                            s.joined_on.map(|d| d.to_string()).as_deref(),
                            pick(r, "joined_on"),
                        )
                        .map(|o| o.and_then(|d| d.parse().ok())),
                        status: None,
                        custom: Some(staff_custom(r, s.custom.clone())),
                    },
                )
                .await?;
                repo::add_row(
                    &mut tx,
                    &state.hlc,
                    actor,
                    &batch,
                    row_no,
                    "updated",
                    "staff",
                    Some(s.id),
                    Some(old),
                )
                .await?;
            }
            (RowAction::Create, ImportKind::OpeningBalances) => {
                let r = &p.row;
                let s = p.target_student.clone().ok_or_else(ApiError::internal)?;
                let amount: Decimal = r
                    .get("amount")
                    .and_then(|a| a.parse().ok())
                    .unwrap_or_default();
                // Billed to the month before the year begins: it then stands before every
                // monthly voucher, shows as arrears on the first one, and does not take the
                // first month's place (which left that month unbilled).
                let month = {
                    use chrono::Datelike;
                    let start = ctx.year_start.unwrap_or_else(crate::clock::today);
                    start
                        .with_day(1)
                        .and_then(|d| d.checked_sub_months(chrono::Months::new(1)))
                        .unwrap_or(start)
                };
                let v = fees::create_opening_balance(
                    &mut tx,
                    &state.hlc,
                    actor,
                    school_id,
                    s.id,
                    ctx.year_id
                        .ok_or_else(|| ApiError::validation("No active academic year."))?,
                    month,
                    amount,
                    r.get("note"),
                )
                .await?;
                repo::add_row(
                    &mut tx,
                    &state.hlc,
                    actor,
                    &batch,
                    row_no,
                    "created",
                    "fee_voucher",
                    Some(v.id),
                    None,
                )
                .await?;
            }
            (RowAction::Update, ImportKind::OpeningBalances) => {}
            (RowAction::Create, ImportKind::Enquiries) => {
                let r = &p.row;
                let year = ctx
                    .year_id
                    .ok_or_else(|| ApiError::validation("No active academic year."))?;
                let class_id = r.get("class").and_then(|k| ctx.class(k)).map(|cl| cl.id);
                let app = madrasa_db::admissions::create(
                    &mut tx,
                    &state.hlc,
                    actor,
                    school_id,
                    &madrasa_db::admissions::ApplicationInput {
                        academic_year_id: year,
                        class_id,
                        enquiry_date: r
                            .get("enquiry_date")
                            .and_then(|d| d.parse().ok())
                            .unwrap_or_else(crate::clock::today),
                        source: pick(r, "source").unwrap_or_else(|| "import".into()),
                        full_name: r.get("full_name").unwrap_or("").into(),
                        father_name: r.get("father_name").unwrap_or("").into(),
                        date_of_birth: r.get("date_of_birth").and_then(|d| d.parse().ok()),
                        gender: r.get("gender").unwrap_or("").into(),
                        cnic: None,
                        address: r.get("address").unwrap_or("").into(),
                        phone: pick(r, "phone"),
                        email: pick(r, "email"),
                        previous_school: r.get("previous_school").unwrap_or("").into(),
                        guardian_name: r.get("guardian_name").unwrap_or("").into(),
                        guardian_relationship: "guardian".into(),
                        guardian_phone: pick(r, "guardian_phone"),
                        guardian_cnic: pick(r, "guardian_cnic"),
                        guardian_email: None,
                        guardian_occupation: String::new(),
                        test_date: None,
                        test_marks: None,
                        test_total: None,
                        interview_date: None,
                        interview_notes: String::new(),
                        offer_date: None,
                        offer_fee: None,
                        offer_expires: None,
                        notes: r.get("notes").unwrap_or("").into(),
                        custom: serde_json::json!({ "imported": true }),
                    },
                    None,
                    actor.user_id,
                )
                .await?;
                repo::add_row(
                    &mut tx,
                    &state.hlc,
                    actor,
                    &batch,
                    row_no,
                    "created",
                    "admission_application",
                    Some(app.id),
                    None,
                )
                .await?;
            }
            (RowAction::Update, ImportKind::Enquiries) => {}
            (RowAction::Create | RowAction::Update, ImportKind::Guardians) => {
                let s = p.target_student.clone().ok_or_else(ApiError::internal)?;
                attach_guardian_slot(
                    &mut tx,
                    state,
                    actor,
                    &batch,
                    row_no,
                    school_id,
                    s.id,
                    &p.row,
                    &mut used_guardians,
                    "guardian",
                    p.row.get("primary") == Some("true"),
                )
                .await?;
            }
            (RowAction::Create | RowAction::Update, ImportKind::FeeStructure) => {
                let f = p.fee.clone().ok_or_else(ApiError::internal)?;
                let year = ctx
                    .year_id
                    .ok_or_else(|| ApiError::validation("No active academic year."))?;
                let head_id = match f
                    .head_id
                    .or_else(|| new_heads.get(&f.head_name.to_lowercase()).copied())
                {
                    Some(id) => id,
                    None => {
                        let taken: Vec<String> = fees::list_heads(&mut tx, school_id)
                            .await?
                            .into_iter()
                            .map(|h| h.code)
                            .collect();
                        let head = fees::create_head(
                            &mut tx,
                            &state.hlc,
                            actor,
                            school_id,
                            &f.head_name,
                            &head_code(&f.head_name, &taken),
                            true,
                            taken.len() as i32 + 1,
                        )
                        .await?;
                        repo::add_row(
                            &mut tx,
                            &state.hlc,
                            actor,
                            &batch,
                            row_no,
                            "created",
                            "fee_head",
                            Some(head.id),
                            None,
                        )
                        .await?;
                        new_heads.insert(f.head_name.to_lowercase(), head.id);
                        head.id
                    }
                };
                let set = fees::set_structure(
                    &mut tx, &state.hlc, actor, school_id, year, f.class_id, head_id, f.amount,
                )
                .await?;
                repo::add_row(
                    &mut tx,
                    &state.hlc,
                    actor,
                    &batch,
                    row_no,
                    "updated",
                    "fee_structure",
                    Some(set.map(|s| s.id).unwrap_or(head_id)),
                    Some(serde_json::json!({
                        "academic_year_id": year.to_string(),
                        "class_id": f.class_id.to_string(),
                        "fee_head_id": head_id.to_string(),
                        "amount": f.old.unwrap_or_default().to_string(),
                    })),
                )
                .await?;
            }
        }
        outcomes.push(outcome(p.row, p.action, p.matched));
    }
    let summary = summarize(&outcomes);
    let batch = repo::finish_batch(
        &mut tx,
        &state.hlc,
        actor,
        batch.id,
        summary.create as i32,
        summary.update as i32,
        (summary.skip + summary.error) as i32,
    )
    .await?;
    if req.save_mapping {
        settings::set(
            &mut tx,
            &state.hlc,
            actor,
            Some(school_id),
            &mapping_key(req.kind),
            serde_json::to_value(SavedMapping {
                headers: req.headers.clone(),
                mapping: req.mapping.clone(),
            })
            .map_err(|_| ApiError::internal())?,
        )
        .await?;
    }
    tx.commit().await?;
    Ok(ImportResult {
        batch,
        summary,
        rows: outcomes,
    })
}

/// A short code for a new fee head: the capitals and digits of its name ("Computer fee" →
/// "COMPUTER"), made different from the codes in use by a number at the end.
fn head_code(name: &str, taken: &[String]) -> String {
    let base: String = name
        .split_whitespace()
        .next()
        .unwrap_or("FEE")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(10)
        .collect::<String>()
        .to_uppercase();
    let base = if base.is_empty() {
        "FEE".to_string()
    } else {
        base
    };
    let is_taken = |code: &str| taken.iter().any(|t| t.eq_ignore_ascii_case(code));
    if !is_taken(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}{n}"))
        .find(|code| !is_taken(code))
        .unwrap_or(base)
}

/// Link the row's guardians: the first (primary) and, when the register has one, the second.
#[allow(clippy::too_many_arguments)]
async fn attach_guardian(
    tx: &mut sqlx::PgConnection,
    state: &AppState,
    actor: Actor,
    batch: &ImportBatch,
    row_no: i32,
    school_id: Uuid,
    student_id: Uuid,
    r: &CleanRow,
    used: &mut HashSet<Uuid>,
) -> Result<(), ApiError> {
    for (slot, primary) in [("guardian", true), ("guardian2", false)] {
        attach_guardian_slot(
            tx, state, actor, batch, row_no, school_id, student_id, r, used, slot, primary,
        )
        .await?;
    }
    Ok(())
}

/// One guardian column group (`guardian_*` or `guardian2_*`). An existing guardian is reused
/// when the family id and name match (siblings in one register), else when the ID number
/// or phone matches; otherwise one is created, carrying the family id for later rows.
#[allow(clippy::too_many_arguments)]
async fn attach_guardian_slot(
    tx: &mut sqlx::PgConnection,
    state: &AppState,
    actor: Actor,
    batch: &ImportBatch,
    row_no: i32,
    school_id: Uuid,
    student_id: Uuid,
    r: &CleanRow,
    used: &mut HashSet<Uuid>,
    slot: &str,
    primary: bool,
) -> Result<(), ApiError> {
    let key = |field: &str| format!("{slot}_{field}");
    let Some(name) = r.get(&key("name")) else {
        return Ok(());
    };
    let family_id = r.get("family_id");
    // Already normalised for the school's country by `clean_row`.
    let cnic = pick(r, &key("cnic"));
    let phone = pick(r, &key("phone"));
    let mut existing = match family_id {
        Some(f) => guardians::find_by_family(tx, school_id, f, name).await?,
        None => None,
    };
    if existing.is_none() && (cnic.is_some() || phone.is_some()) {
        existing = guardians::find_matching(tx, school_id, cnic.as_deref(), phone.as_deref(), name)
            .await?;
    }
    let guardian_id = match existing {
        Some(g) => g.id,
        None => {
            let g = guardians::create(
                tx,
                &state.hlc,
                actor,
                guardians::NewGuardian {
                    school_id,
                    full_name: name.to_string(),
                    phone,
                    email: pick(r, &key("email")),
                    cnic,
                    occupation: r.get(&key("occupation")).unwrap_or("").into(),
                    address: r.get("address").unwrap_or("").into(),
                    custom: match family_id {
                        Some(f) => serde_json::json!({ "family_id": f }),
                        None => serde_json::json!({}),
                    },
                },
            )
            .await?;
            repo::add_row(
                tx,
                &state.hlc,
                actor,
                batch,
                row_no,
                "created",
                "guardian",
                Some(g.id),
                None,
            )
            .await?;
            used.insert(g.id);
            g.id
        }
    };
    let relationship =
        r.get(&key("relationship"))
            .unwrap_or(if primary { "father" } else { "guardian" });
    // A link that already exists (an update import re-linking the same guardian) is recorded
    // as an update with its old values, so rolling the batch back restores it instead of
    // removing it (v0.2.0-alpha O3).
    let before = guardians::link_between(tx, student_id, guardian_id).await?;
    let link = guardians::link(
        tx,
        &state.hlc,
        actor,
        student_id,
        guardian_id,
        relationship,
        primary,
    )
    .await?;
    match before {
        Some(old) => {
            repo::add_row(
                tx,
                &state.hlc,
                actor,
                batch,
                row_no,
                "updated",
                "student_guardian",
                Some(link.id),
                Some(serde_json::json!({ "relationship": old.relationship, "is_primary": old.is_primary })),
            )
            .await?;
        }
        None => {
            repo::add_row(
                tx,
                &state.hlc,
                actor,
                batch,
                row_no,
                "created",
                "student_guardian",
                Some(link.id),
                None,
            )
            .await?;
        }
    }
    Ok(())
}

pub async fn rollback(
    state: &AppState,
    actor: Actor,
    school_id: Uuid,
    batch_id: Uuid,
) -> Result<usize, ApiError> {
    let mut tx = state.pool.begin().await?;
    let batch = repo::get_batch(&mut tx, batch_id).await?;
    if batch.school_id != school_id {
        return Err(ApiError::not_found("import"));
    }
    let n = repo::rollback(&mut tx, &state.hlc, actor, batch_id).await?;
    tx.commit().await?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn b64(bytes: &[u8]) -> String {
        STANDARD.encode(bytes)
    }

    fn workbook(cells: &[(u32, u16, &str)]) -> Vec<u8> {
        let mut wb = rust_xlsxwriter::Workbook::new();
        let ws = wb.add_worksheet();
        ws.set_name("Students").unwrap();
        for (row, col, text) in cells {
            ws.write_string(*row, *col, *text).unwrap();
        }
        wb.save_to_buffer().unwrap()
    }

    fn message(e: ApiError) -> String {
        format!("{e:?}")
    }

    #[test]
    fn an_ordinary_workbook_is_read_as_before() {
        let file = workbook(&[
            (0, 0, "Name"),
            (0, 1, "Father"),
            (1, 0, "Ali Khan"),
            (1, 1, "Ahmed Khan"),
            (2, 0, "=1+1"),
            (2, 1, "Sara"),
        ]);
        let (sheets, chosen, headers, rows) =
            parse_file(ImportKind::Students, "students.xlsx", &b64(&file), None).unwrap();
        assert_eq!(sheets, ["Students"]);
        assert_eq!(chosen, ["Students"]);
        assert_eq!(headers, ["Name", "Father"]);
        // Text that looks like a formula comes in as the text it is; nothing is evaluated.
        assert_eq!(rows, [["Ali Khan", "Ahmed Khan"], ["=1+1", "Sara"]]);
    }

    /// A three-kilobyte workbook with one cell in the far corner of the sheet: a reader that
    /// lays the sheet out as a grid would ask for seventeen thousand million cells and take
    /// the server down. Refused, with a message that says what to do.
    #[test]
    fn a_sheet_with_a_cell_in_its_far_corner_is_refused_before_it_is_laid_out() {
        let file = workbook(&[(0, 0, "Name"), (1_048_575, 16_383, "x")]);
        assert!(
            file.len() < 20_000,
            "the file itself is tiny: {}",
            file.len()
        );
        let err = parse_file(ImportKind::Students, "wide.xlsx", &b64(&file), None).unwrap_err();
        assert!(message(err).contains("too large to import"));
        // Far down but narrow, and wide but short, are refused too.
        for corner in [(250_000, 1), (10, 700)] {
            let file = workbook(&[(0, 0, "Name"), (corner.0, corner.1, "x")]);
            let err =
                parse_file(ImportKind::Students, "corner.xlsx", &b64(&file), None).unwrap_err();
            assert!(message(err).contains("too large to import"), "{corner:?}");
        }
    }

    /// A zip bomb: a workbook whose parts unpack to far more than any import would.
    #[test]
    fn a_workbook_that_unpacks_to_hundreds_of_megabytes_is_refused() {
        use std::io::Write;
        let mut out = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut out);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            zip.start_file("xl/worksheets/sheet1.xml", options).unwrap();
            let block = vec![b' '; 1024 * 1024];
            for _ in 0..(MAX_UNPACKED_BYTES / (1024 * 1024) + 8) {
                zip.write_all(&block).unwrap();
            }
            zip.finish().unwrap();
        }
        let file = out.into_inner();
        assert!(
            file.len() < 2 * 1024 * 1024,
            "it packs small: {}",
            file.len()
        );
        let err = parse_file(ImportKind::Students, "bomb.xlsx", &b64(&file), None).unwrap_err();
        assert!(message(err).contains("too large to import"));
    }

    #[test]
    fn a_file_over_the_size_limit_or_not_a_spreadsheet_is_refused_plainly() {
        let big = vec![b'a'; MAX_FILE_BYTES + 1];
        let err = parse_file(ImportKind::Students, "big.csv", &b64(&big), None).unwrap_err();
        assert!(message(err).contains("20 MB"));
        let err = parse_file(
            ImportKind::Students,
            "fake.xlsx",
            &b64(b"PK not a zip"),
            None,
        )
        .unwrap_err();
        assert!(message(err).contains("not a spreadsheet"));
    }

    #[test]
    fn sheet_named_after_the_kind_wins_over_a_read_me() {
        let wb = names(&["Read Me", "Students", "Staff"]);
        assert_eq!(pick_sheet(ImportKind::Students, &wb), Some("Students"));
        assert_eq!(pick_sheet(ImportKind::Staff, &wb), Some("Staff"));
        let wb = names(&["README", "Instructions", "Data"]);
        assert_eq!(pick_sheet(ImportKind::Students, &wb), Some("Data"));
        let wb = names(&["Read me"]);
        assert_eq!(pick_sheet(ImportKind::Students, &wb), Some("Read me"));
        assert_eq!(pick_sheet(ImportKind::Students, &[]), None);
        let wb = names(&["Sheet1", "Opening Balances"]);
        assert_eq!(
            pick_sheet(ImportKind::OpeningBalances, &wb),
            Some("Opening Balances")
        );
    }
}
