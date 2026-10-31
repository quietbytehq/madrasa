//! Import wizard service: read .xlsx/.csv, map columns, clean and validate rows against the
//! database (classes, sections, duplicates, existing students), and apply a batch that can be
//! rolled back. Row cleanup rules live in `madrasa_core::imports`.

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use calamine::{Data, Reader};
use chrono::NaiveDate;
use madrasa_core::imports::{self as rules, CleanRow, ImportKind, Issue};
use madrasa_db::models::{Class, ImportBatch, Section, Student};
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
pub fn parse_file(
    file_name: &str,
    content_base64: &str,
    sheet: Option<&str>,
) -> Result<Parsed, ApiError> {
    let bytes = STANDARD
        .decode(content_base64.trim())
        .map_err(|_| ApiError::validation("The file could not be read."))?;
    let lower = file_name.to_lowercase();
    if lower.ends_with(".xlsx")
        || lower.ends_with(".xlsm")
        || lower.ends_with(".xls")
        || lower.ends_with(".ods")
    {
        let mut wb = calamine::open_workbook_auto_from_rs(Cursor::new(bytes)).map_err(|_| {
            ApiError::validation(
                "This is not a spreadsheet Madrasa can read. Save it as .xlsx or .csv.",
            )
        })?;
        let names = wb.sheet_names().to_vec();
        let chosen = sheet
            .filter(|s| names.iter().any(|n| n == s))
            .map(str::to_owned)
            .or_else(|| names.first().cloned())
            .ok_or_else(|| ApiError::validation("The workbook has no sheets."))?;
        let range = wb
            .worksheet_range(&chosen)
            .map_err(|_| ApiError::validation("The sheet could not be read."))?;
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
            let rec = rec.map_err(|_| ApiError::validation("The CSV file could not be read."))?;
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
    let (sheets, chosen, headers, rows) = parse_file(file_name, content_base64, sheet)?;
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

/// Template with the field labels as headings and one example row.
pub fn template(kind: ImportKind, xlsx: bool) -> Result<Vec<u8>, ApiError> {
    let fields = rules::fields(kind);
    let headers: Vec<&str> = fields.iter().map(|f| f.label).collect();
    let example: Vec<&str> = fields.iter().map(|f| f.example).collect();
    if xlsx {
        crate::export::xlsx(
            kind.as_str(),
            &headers.iter().map(|h| h.to_string()).collect::<Vec<_>>(),
            &[example.iter().map(|e| e.to_string()).collect()],
        )
    } else {
        Ok(crate::export::csv(
            &headers.iter().map(|h| h.to_string()).collect::<Vec<_>>(),
            &[example.iter().map(|e| e.to_string()).collect()],
        ))
    }
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

/// Mirrored by `ImportPreview` in the UI.
#[derive(Debug, Serialize)]
pub struct ImportPreview {
    pub rows: Vec<RowOutcome>,
    pub summary: ImportSummary,
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
}

async fn ctx(conn: &mut sqlx::PgConnection, school_id: Uuid) -> Result<Ctx, ApiError> {
    let year = academics::active_year(conn, school_id).await?;
    let classes = academics::list_classes(conn, school_id).await?;
    let sections = match &year {
        Some(y) => academics::list_sections(conn, school_id, Some(y.id), None).await?,
        None => Vec::new(),
    };
    let school = schools::get(conn, school_id).await?;
    Ok(Ctx {
        school_id,
        rules: rules::RowRules::new(&school.country, crate::clock::today()),
        year_id: year.as_ref().map(|y| y.id),
        year_start: year.as_ref().map(|y| y.start_date),
        classes,
        sections,
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
}

async fn plan_rows(
    conn: &mut sqlx::PgConnection,
    c: &Ctx,
    req: &ImportRequest,
) -> Result<Vec<Plan>, ApiError> {
    let mut plans = Vec::with_capacity(req.rows.len());
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut seen_numbers: HashMap<String, usize> = HashMap::new();
    for (i, raw) in req.rows.iter().enumerate() {
        let row_no = i + 2; // 1 is the heading row
        let mut row = rules::clean_row(req.kind, row_no, raw, &req.mapping, &c.rules);
        let mut matched = None;
        let mut existing_student = None;
        let mut existing_staff = None;
        let mut section_id = None;
        let mut target_student = None;
        match req.kind {
            ImportKind::Students => {
                // Class and section must exist in the active year.
                if let Some(class_key) = row.get("class").map(str::to_owned) {
                    match c.class(&class_key) {
                        None => row.errors.push(Issue {
                            field: "class".into(),
                            message: format!(
                                "Class \"{}\" is not set up in Settings",
                                row.values["class"]
                            ),
                        }),
                        Some(class) => {
                            if c.year_id.is_none() {
                                row.errors.push(Issue {
                                    field: "class".into(),
                                    message: "No active academic year".into(),
                                });
                            } else if let Some(sec) = row.get("section").map(str::to_owned) {
                                match c.section(class.id, &sec) {
                                    Some(s) => section_id = Some(s.id),
                                    None => row.errors.push(Issue {
                                        field: "section".into(),
                                        message: format!(
                                            "Section \"{sec}\" of class {} is not set up",
                                            class.name
                                        ),
                                    }),
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
        let action = if !row.errors.is_empty() {
            RowAction::Error
        } else if existing_student.is_some() || existing_staff.is_some() {
            match req.options.duplicates {
                DuplicatePolicy::Skip => {
                    row.warnings.push(Issue {
                        field: "full_name".into(),
                        message: "Already exists; skipped".into(),
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
    let ctx = ctx(&mut c, school_id).await?;
    let plans = plan_rows(&mut c, &ctx, req).await?;
    let rows: Vec<RowOutcome> = plans
        .into_iter()
        .map(|p| outcome(p.row, p.action, p.matched))
        .collect();
    let summary = summarize(&rows);
    Ok(ImportPreview { rows, summary })
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
    let ctx = ctx(&mut tx, school_id).await?;
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
                        custom: serde_json::json!({ "imported": true }),
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
                    custom: None,
                };
                students::update(&mut tx, &state.hlc, actor, s.id, upd).await?;
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
                let month = ctx.year_start.unwrap_or_else(crate::clock::today);
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
    let Some(name) = r.get("guardian_name") else {
        return Ok(());
    };
    // Already normalised for the school's country by `clean_row`.
    let cnic = pick(r, "guardian_cnic");
    let phone = pick(r, "guardian_phone");
    let existing =
        guardians::find_matching(tx, school_id, cnic.as_deref(), phone.as_deref()).await?;
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
                    email: pick(r, "guardian_email"),
                    cnic,
                    occupation: r.get("guardian_occupation").unwrap_or("").into(),
                    address: r.get("address").unwrap_or("").into(),
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
    let relationship = r.get("guardian_relationship").unwrap_or("father");
    let link = guardians::link(
        tx,
        &state.hlc,
        actor,
        student_id,
        guardian_id,
        relationship,
        true,
    )
    .await?;
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
