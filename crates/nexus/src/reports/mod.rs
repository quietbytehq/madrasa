//! Network-wide reports (stage 5): the same question asked of every joined school, with the
//! school as a column.
//!
//! Each report runs a query in each active school's schema and puts the rows together. The
//! per-school queries are the ones a school's own reports use (`madrasa_db::reports`), so a
//! figure here is the figure the school sees, as of its last sync. Schools that left or
//! were removed are not included. Money is never added across currencies.

pub mod text;

use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, Months, NaiveDate, Utc};
use madrasa_db::reports as school_reports;
use madrasa_db::sync;
use rust_decimal::Decimal;
use serde::Serialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::tenants::{self, HubSchool};
use crate::{HubError, HubResult, HubState};
pub use text::{tr, Lang};

/// What kind of value a column holds: decides alignment on screen and the cell type in Excel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Text,
    Int,
    /// Exact decimal, sent as a string (`"2500.00"`): money never passes through a float.
    Money,
    Percent,
    Date,
    DateTime,
}

#[derive(Debug, Clone, Serialize)]
pub struct Column {
    pub key: &'static str,
    pub label: String,
    pub kind: Kind,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub code: &'static str,
    pub title: String,
    pub about: String,
    pub generated_at: DateTime<Utc>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    /// Schools included.
    pub schools: usize,
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<Value>>,
    /// Total rows, in the same columns.
    pub totals: Vec<Vec<Value>>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogueEntry {
    pub code: &'static str,
    pub title: String,
    pub about: String,
    /// The report takes `from` and `to`.
    pub takes_period: bool,
}

/// What a report is asked with.
#[derive(Debug, Clone, Default)]
pub struct Params {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    /// One school (its hub id) instead of all.
    pub school: Option<Uuid>,
    pub lang: Lang,
}

/// (code, takes a period).
const REPORTS: [(&str, bool); 5] = [
    ("summary", false),
    ("enrolment", false),
    ("fees", true),
    ("attendance", true),
    ("sync", false),
];

/// The longest period a report covers, in months.
const MAX_MONTHS: u32 = 24;

pub fn catalogue(lang: Lang) -> Vec<CatalogueEntry> {
    REPORTS
        .iter()
        .map(|(code, takes_period)| CatalogueEntry {
            code,
            title: tr(lang, &format!("{code}.title")),
            about: tr(lang, &format!("{code}.about")),
            takes_period: *takes_period,
        })
        .collect()
}

fn col(lang: Lang, key: &'static str, kind: Kind) -> Column {
    Column {
        key,
        label: tr(lang, &format!("col.{key}")),
        kind,
    }
}

fn money(v: Decimal) -> Value {
    Value::String(format!("{v:.2}"))
}

fn first_of_month(d: NaiveDate) -> NaiveDate {
    d.with_day(1).unwrap_or(d)
}

/// The period of a report: what was asked for, or the last twelve months to today. Refused
/// when it is backwards or longer than two years.
fn period(p: &Params) -> HubResult<(NaiveDate, NaiveDate)> {
    let today = Utc::now().date_naive();
    let to = p.to.unwrap_or(today);
    let from = p.from.unwrap_or_else(|| {
        first_of_month(to)
            .checked_sub_months(Months::new(11))
            .unwrap_or(to)
    });
    if from > to {
        return Err(HubError::validation("The period ends before it starts."));
    }
    let months =
        (to.year() - from.year()) as i64 * 12 + to.month() as i64 - from.month() as i64 + 1;
    if months > MAX_MONTHS as i64 {
        return Err(HubError::validation(
            "Choose a period of at most two years.",
        ));
    }
    Ok((from, to))
}

/// The schools a report covers: every joined school, or the one asked for.
async fn selected(state: &HubState, p: &Params) -> HubResult<Vec<HubSchool>> {
    let mut conn = state.pool.acquire().await?;
    let all = tenants::active(&mut conn).await?;
    match p.school {
        None => Ok(all),
        Some(id) => {
            let one: Vec<HubSchool> = all.into_iter().filter(|s| s.id == id).collect();
            if one.is_empty() {
                return Err(HubError::not_found("School"));
            }
            Ok(one)
        }
    }
}

fn base(code: &'static str, p: &Params, schools: usize, columns: Vec<Column>) -> Report {
    Report {
        code,
        title: tr(p.lang, &format!("{code}.title")),
        about: tr(p.lang, &format!("{code}.about")),
        generated_at: Utc::now(),
        from: None,
        to: None,
        schools,
        columns,
        rows: Vec::new(),
        totals: Vec::new(),
        notes: vec![tr(p.lang, "note.active_only"), tr(p.lang, "note.as_synced")],
    }
}

/// The active academic year of the school in this transaction, if it has one.
async fn active_year(conn: &mut sqlx::PgConnection, school_id: Uuid) -> HubResult<Option<Uuid>> {
    Ok(sqlx::query_scalar(
        "SELECT id FROM academic_years WHERE school_id = $1 AND is_active AND deleted_at IS NULL
         ORDER BY start_date DESC LIMIT 1",
    )
    .bind(school_id)
    .fetch_optional(conn)
    .await?)
}

/// Run a report.
pub async fn run(state: &HubState, code: &str, p: &Params) -> HubResult<Report> {
    match code {
        "summary" => summary(state, p).await,
        "enrolment" => enrolment(state, p).await,
        "fees" => fees(state, p).await,
        "attendance" => attendance(state, p).await,
        "sync" => sync_health(state, p).await,
        _ => Err(HubError::not_found("Report")),
    }
}

async fn summary(state: &HubState, p: &Params) -> HubResult<Report> {
    let schools = selected(state, p).await?;
    let l = p.lang;
    let mut r = base(
        "summary",
        p,
        schools.len(),
        vec![
            col(l, "school", Kind::Text),
            col(l, "code", Kind::Text),
            col(l, "edition", Kind::Text),
            col(l, "version", Kind::Text),
            col(l, "students", Kind::Int),
            col(l, "staff", Kind::Int),
            col(l, "sections", Kind::Int),
            col(l, "last_sync", Kind::DateTime),
        ],
    );
    let (mut students, mut staff, mut sections) = (0i64, 0i64, 0i64);
    for school in &schools {
        let mut tx = tenants::begin(&state.pool, school).await?;
        let (st, sf, sec): (i64, i64, i64) = sqlx::query_as(
            "SELECT
               (SELECT count(*) FROM students WHERE school_id = $1 AND status = 'active' AND deleted_at IS NULL),
               (SELECT count(*) FROM staff WHERE school_id = $1 AND status = 'active' AND deleted_at IS NULL),
               (SELECT count(*) FROM sections s JOIN academic_years y ON y.id = s.academic_year_id
                 WHERE s.school_id = $1 AND s.deleted_at IS NULL AND y.is_active AND y.deleted_at IS NULL)",
        )
        .bind(school.school_id)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        students += st;
        staff += sf;
        sections += sec;
        r.rows.push(vec![
            json!(school.name),
            json!(school.code),
            json!(school.edition),
            json!(school.school_version),
            json!(st),
            json!(sf),
            json!(sec),
            json!(school.last_sync_at),
        ]);
    }
    r.totals.push(vec![
        json!(tr(l, "row.total")),
        json!(""),
        json!(""),
        json!(""),
        json!(students),
        json!(staff),
        json!(sections),
        Value::Null,
    ]);
    Ok(r)
}

async fn enrolment(state: &HubState, p: &Params) -> HubResult<Report> {
    let schools = selected(state, p).await?;
    let l = p.lang;
    let mut r = base(
        "enrolment",
        p,
        schools.len(),
        vec![
            col(l, "school", Kind::Text),
            col(l, "class", Kind::Text),
            col(l, "boys", Kind::Int),
            col(l, "girls", Kind::Int),
            col(l, "other", Kind::Int),
            col(l, "total", Kind::Int),
        ],
    );
    let mut sum = [0i64; 4];
    for school in &schools {
        let mut tx = tenants::begin(&state.pool, school).await?;
        let rows = match active_year(&mut tx, school.school_id).await? {
            Some(year) => {
                school_reports::class_strength(&mut tx, school.school_id, year, None).await?
            }
            None => Vec::new(),
        };
        tx.commit().await?;
        // The school's report is per section; a network reads it per class.
        let mut classes: BTreeMap<(i32, String), [i64; 4]> = BTreeMap::new();
        for row in rows {
            let c = classes
                .entry((row.class_level, row.class_name))
                .or_default();
            c[0] += row.boys;
            c[1] += row.girls;
            c[2] += row.other;
            c[3] += row.total;
        }
        for ((_, name), c) in classes {
            for (total, value) in sum.iter_mut().zip(c) {
                *total += value;
            }
            r.rows.push(vec![
                json!(school.name),
                json!(name),
                json!(c[0]),
                json!(c[1]),
                json!(c[2]),
                json!(c[3]),
            ]);
        }
    }
    r.totals.push(vec![
        json!(tr(l, "row.total")),
        json!(""),
        json!(sum[0]),
        json!(sum[1]),
        json!(sum[2]),
        json!(sum[3]),
    ]);
    Ok(r)
}

/// One month of one school's fees.
#[derive(Default, Clone, Copy)]
struct FeeMonth {
    billed: Decimal,
    collected: Decimal,
    receipts: i64,
    outstanding: Decimal,
}

async fn fees(state: &HubState, p: &Params) -> HubResult<Report> {
    let (from, to) = period(p)?;
    let schools = selected(state, p).await?;
    let l = p.lang;
    let mut r = base(
        "fees",
        p,
        schools.len(),
        vec![
            col(l, "school", Kind::Text),
            col(l, "currency", Kind::Text),
            col(l, "month", Kind::Date),
            col(l, "billed", Kind::Money),
            col(l, "collected", Kind::Money),
            col(l, "receipts", Kind::Int),
            col(l, "outstanding", Kind::Money),
        ],
    );
    r.from = Some(from);
    r.to = Some(to);
    r.notes.push(tr(l, "note.billed"));
    r.notes.push(tr(l, "note.per_currency"));
    let mut by_currency: BTreeMap<String, FeeMonth> = BTreeMap::new();
    for school in &schools {
        let mut tx = tenants::begin(&state.pool, school).await?;
        let currency: String = sqlx::query_scalar("SELECT currency FROM schools WHERE id = $1")
            .bind(school.school_id)
            .fetch_optional(&mut *tx)
            .await?
            .unwrap_or_default();
        let billed: Vec<(NaiveDate, Decimal, Decimal)> = sqlx::query_as(
            "SELECT date_trunc('month', billing_month)::date,
                    COALESCE(sum(net_monthly + late_fee) FILTER (WHERE status <> 'cancelled'), 0)::numeric(14,2),
                    COALESCE(sum(total_due + late_fee - net_paid) FILTER (WHERE status IN ('unpaid', 'partial')), 0)::numeric(14,2)
             FROM fee_vouchers
             WHERE school_id = $1 AND deleted_at IS NULL AND billing_month BETWEEN $2 AND $3
             GROUP BY 1",
        )
        .bind(school.school_id)
        .bind(first_of_month(from))
        .bind(to)
        .fetch_all(&mut *tx)
        .await?;
        let collected =
            school_reports::collection_by_month(&mut tx, school.school_id, from, to).await?;
        tx.commit().await?;
        let mut months: BTreeMap<NaiveDate, FeeMonth> = BTreeMap::new();
        for (month, amount, outstanding) in billed {
            let m = months.entry(month).or_default();
            m.billed = amount;
            m.outstanding = outstanding;
        }
        for c in collected {
            let m = months.entry(c.month).or_default();
            m.collected = c.amount;
            m.receipts = c.receipts;
        }
        let total = by_currency.entry(currency.clone()).or_default();
        for (month, m) in months {
            total.billed += m.billed;
            total.collected += m.collected;
            total.receipts += m.receipts;
            total.outstanding += m.outstanding;
            r.rows.push(vec![
                json!(school.name),
                json!(currency),
                json!(month),
                money(m.billed),
                money(m.collected),
                json!(m.receipts),
                money(m.outstanding),
            ]);
        }
    }
    for (currency, t) in by_currency {
        r.totals.push(vec![
            json!(tr(l, "row.total_currency")),
            json!(currency),
            Value::Null,
            money(t.billed),
            money(t.collected),
            json!(t.receipts),
            money(t.outstanding),
        ]);
    }
    Ok(r)
}

fn percent(part: i64, whole: i64) -> Value {
    if whole == 0 {
        Value::Null
    } else {
        json!((part as f64 * 1000.0 / whole as f64).round() / 10.0)
    }
}

async fn attendance(state: &HubState, p: &Params) -> HubResult<Report> {
    let (from, to) = period(p)?;
    let schools = selected(state, p).await?;
    let l = p.lang;
    let mut r = base(
        "attendance",
        p,
        schools.len(),
        vec![
            col(l, "school", Kind::Text),
            col(l, "month", Kind::Date),
            col(l, "marked", Kind::Int),
            col(l, "present", Kind::Int),
            col(l, "late", Kind::Int),
            col(l, "absent", Kind::Int),
            col(l, "leave", Kind::Int),
            col(l, "present_pct", Kind::Percent),
        ],
    );
    r.from = Some(from);
    r.to = Some(to);
    r.notes.push(tr(l, "note.present_pct"));
    let mut sum = [0i64; 4];
    for school in &schools {
        let mut tx = tenants::begin(&state.pool, school).await?;
        // By the day the mark is for, whichever academic year its sheet belongs to.
        let months: Vec<(NaiveDate, i64, i64, i64, i64)> = sqlx::query_as(
            "SELECT date_trunc('month', e.date)::date,
                    count(*) FILTER (WHERE e.status = 'present'),
                    count(*) FILTER (WHERE e.status = 'late'),
                    count(*) FILTER (WHERE e.status = 'absent'),
                    count(*) FILTER (WHERE e.status = 'leave')
             FROM attendance_entries e
             JOIN attendance_sheets s ON s.id = e.sheet_id AND s.deleted_at IS NULL
             WHERE e.school_id = $1 AND e.deleted_at IS NULL AND e.date BETWEEN $2 AND $3
             GROUP BY 1 ORDER BY 1",
        )
        .bind(school.school_id)
        .bind(from)
        .bind(to)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        for (month, present, late, absent, leave) in months {
            let marked = present + late + absent + leave;
            for (total, value) in sum.iter_mut().zip([present, late, absent, leave]) {
                *total += value;
            }
            r.rows.push(vec![
                json!(school.name),
                json!(month),
                json!(marked),
                json!(present),
                json!(late),
                json!(absent),
                json!(leave),
                percent(present + late, marked),
            ]);
        }
    }
    let marked: i64 = sum.iter().sum();
    r.totals.push(vec![
        json!(tr(l, "row.total")),
        Value::Null,
        json!(marked),
        json!(sum[0]),
        json!(sum[1]),
        json!(sum[2]),
        json!(sum[3]),
        percent(sum[0] + sum[1], marked),
    ]);
    Ok(r)
}

/// Every school in the register, joined or not, with what is waiting. The only report that
/// lists schools that left: head office needs to see them to purge them.
async fn sync_health(state: &HubState, p: &Params) -> HubResult<Report> {
    let l = p.lang;
    let schools: Vec<HubSchool> = {
        let mut conn = state.pool.acquire().await?;
        tenants::list(&mut conn)
            .await?
            .into_iter()
            .filter(|s| p.school.is_none_or(|id| id == s.id))
            .collect()
    };
    let mut r = base(
        "sync",
        p,
        schools.len(),
        vec![
            col(l, "school", Kind::Text),
            col(l, "code", Kind::Text),
            col(l, "status", Kind::Text),
            col(l, "joined", Kind::DateTime),
            col(l, "last_sync", Kind::DateTime),
            col(l, "pending", Kind::Int),
            col(l, "parked", Kind::Int),
            col(l, "overwrites", Kind::Int),
            col(l, "version", Kind::Text),
        ],
    );
    r.notes.clear();
    for school in &schools {
        let status = if tenants::is_schema_name(&school.schema_name) {
            let mut tx = tenants::begin(&state.pool, school).await?;
            let st = sync::status(&mut tx).await?;
            tx.commit().await?;
            Some(st)
        } else {
            None
        };
        r.rows.push(vec![
            json!(school.name),
            json!(school.code),
            json!(tr(l, &format!("status.{}", school.status))),
            json!(school.joined_at),
            json!(school.last_sync_at),
            json!(status.as_ref().map(|s| s.pending)),
            json!(status.as_ref().map(|s| s.parked)),
            json!(status.as_ref().map(|s| s.overwrites)),
            json!(school.school_version),
        ]);
    }
    Ok(r)
}

// ---- downloads --------------------------------------------------------------------------------

fn cell_text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// The report as CSV: headings, rows, then the total rows. With a BOM so Excel opens Urdu
/// text correctly.
pub fn to_csv(r: &Report) -> Vec<u8> {
    let mut w = csv::Writer::from_writer(Vec::new());
    let _ = w.write_record(r.columns.iter().map(|c| c.label.as_str()));
    for row in r.rows.iter().chain(r.totals.iter()) {
        let _ = w.write_record(row.iter().map(cell_text));
    }
    let mut out = vec![0xEF, 0xBB, 0xBF];
    out.extend(w.into_inner().unwrap_or_default());
    out
}

/// The report as a one-sheet workbook: bold headings, numbers as numbers, bold totals.
pub fn to_xlsx(r: &Report) -> Result<Vec<u8>, rust_xlsxwriter::XlsxError> {
    use rust_xlsxwriter::{Format, Workbook};
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    let name: String = r.title.chars().take(31).collect();
    let _ = ws.set_name(&name);
    let bold = Format::new().set_bold();
    let two_dp = Format::new().set_num_format("#,##0.00");
    let bold_two_dp = Format::new().set_bold().set_num_format("#,##0.00");
    for (c, column) in r.columns.iter().enumerate() {
        ws.write_string_with_format(0, c as u16, &column.label, &bold)?;
        let width = r
            .rows
            .iter()
            .filter_map(|row| row.get(c))
            .map(|v| cell_text(v).chars().count())
            .chain(std::iter::once(column.label.chars().count()))
            .max()
            .unwrap_or(10)
            .clamp(8, 48);
        let _ = ws.set_column_width(c as u16, width as f64 + 2.0);
    }
    let total_from = r.rows.len();
    for (i, row) in r.rows.iter().chain(r.totals.iter()).enumerate() {
        let is_total = i >= total_from;
        for (c, v) in row.iter().enumerate() {
            let (line, column) = ((i + 1) as u32, c as u16);
            let kind = r.columns.get(c).map(|k| k.kind).unwrap_or(Kind::Text);
            let text = cell_text(v);
            let number = match kind {
                Kind::Int | Kind::Money | Kind::Percent => text.parse::<f64>().ok(),
                _ => None,
            };
            match (number, kind == Kind::Money, is_total) {
                (Some(n), true, true) => {
                    ws.write_number_with_format(line, column, n, &bold_two_dp)?
                }
                (Some(n), true, false) => ws.write_number_with_format(line, column, n, &two_dp)?,
                (Some(n), false, true) => ws.write_number_with_format(line, column, n, &bold)?,
                (Some(n), false, false) => ws.write_number(line, column, n)?,
                (None, _, true) => ws.write_string_with_format(line, column, &text, &bold)?,
                (None, _, false) => ws.write_string(line, column, &text)?,
            };
        }
    }
    let _ = ws.set_freeze_panes(1, 0);
    wb.save_to_buffer()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Report {
        Report {
            code: "fees",
            title: "Fees".into(),
            about: String::new(),
            generated_at: Utc::now(),
            from: None,
            to: None,
            schools: 1,
            columns: vec![
                Column {
                    key: "school",
                    label: "School".into(),
                    kind: Kind::Text,
                },
                Column {
                    key: "billed",
                    label: "Billed".into(),
                    kind: Kind::Money,
                },
                Column {
                    key: "receipts",
                    label: "Receipts".into(),
                    kind: Kind::Int,
                },
            ],
            rows: vec![vec![json!("Gulshan, \"High\""), json!("2500.50"), json!(3)]],
            totals: vec![vec![json!("Total"), json!("2500.50"), json!(3)]],
            notes: vec![],
        }
    }

    #[test]
    fn the_catalogue_lists_every_report_in_both_languages() {
        let en = catalogue(Lang::En);
        let ur = catalogue(Lang::Ur);
        assert_eq!(en.len(), 5);
        for (a, b) in en.iter().zip(&ur) {
            assert_eq!(a.code, b.code);
            assert!(
                !a.title.contains(".title"),
                "{} has no English title",
                a.code
            );
            assert_ne!(a.title, b.title, "{} is not translated", a.code);
        }
        assert_eq!(
            en.iter()
                .filter(|e| e.takes_period)
                .map(|e| e.code)
                .collect::<Vec<_>>(),
            ["fees", "attendance"]
        );
    }

    #[test]
    fn the_default_period_is_the_last_twelve_months_and_bad_periods_are_refused() {
        let d = |y, m, day| NaiveDate::from_ymd_opt(y, m, day).unwrap();
        let p = Params {
            to: Some(d(2026, 11, 11)),
            ..Default::default()
        };
        assert_eq!(period(&p).unwrap(), (d(2025, 12, 1), d(2026, 11, 11)));
        let backwards = Params {
            from: Some(d(2026, 5, 1)),
            to: Some(d(2026, 4, 1)),
            ..Default::default()
        };
        assert!(period(&backwards).is_err());
        let long = Params {
            from: Some(d(2024, 1, 1)),
            to: Some(d(2026, 2, 1)),
            ..Default::default()
        };
        assert!(period(&long).is_err());
        let two_years = Params {
            from: Some(d(2024, 12, 1)),
            to: Some(d(2026, 11, 30)),
            ..Default::default()
        };
        assert!(period(&two_years).is_ok());
    }

    #[test]
    fn money_keeps_two_decimals_and_percentages_one() {
        assert_eq!(money(Decimal::new(25005, 1)), json!("2500.50"));
        assert_eq!(money(Decimal::ZERO), json!("0.00"));
        assert_eq!(percent(2, 3), json!(66.7));
        assert_eq!(percent(0, 0), Value::Null);
    }

    #[test]
    fn csv_has_a_bom_headings_rows_and_totals() {
        let bytes = to_csv(&sample());
        assert_eq!(&bytes[..3], [0xEF, 0xBB, 0xBF]);
        let text = String::from_utf8(bytes[3..].to_vec()).unwrap();
        assert_eq!(
            text,
            "School,Billed,Receipts\n\"Gulshan, \"\"High\"\"\",2500.50,3\nTotal,2500.50,3\n"
        );
    }

    #[test]
    fn xlsx_is_a_workbook() {
        let bytes = to_xlsx(&sample()).unwrap();
        assert_eq!(&bytes[..2], b"PK", "an xlsx file is a zip archive");
        assert!(bytes.len() > 1000);
    }
}
