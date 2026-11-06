//! Report library (M2 stage 1). A report is built once as a [`Report`] document and rendered
//! as JSON for the screen, Excel through the export module, or PDF typeset on the server
//! (`pdf.rs`) with embedded fonts so Urdu is shaped and right-to-left.

pub mod catalogue;
pub mod pdf;
pub mod students;
pub mod text;

use std::collections::HashMap;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ApiError;

/// Report language. The UI passes its current language; English is the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    En,
    Ur,
}

impl Lang {
    pub fn parse(s: Option<&str>) -> Lang {
        match s {
            Some("ur") => Lang::Ur,
            _ => Lang::En,
        }
    }
    pub fn is_rtl(self) -> bool {
        self == Lang::Ur
    }
}

/// Horizontal alignment of a column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    Start,
    End,
    Center,
}

#[derive(Debug, Clone, Serialize)]
pub struct Column {
    pub label: String,
    pub align: Align,
    /// Takes the remaining width (names, addresses); other columns size to content.
    pub stretch: bool,
}

impl Column {
    pub fn text(label: impl Into<String>) -> Column {
        Column {
            label: label.into(),
            align: Align::Start,
            stretch: false,
        }
    }
    pub fn wide(label: impl Into<String>) -> Column {
        Column {
            label: label.into(),
            align: Align::Start,
            stretch: true,
        }
    }
    pub fn number(label: impl Into<String>) -> Column {
        Column {
            label: label.into(),
            align: Align::End,
            stretch: false,
        }
    }
}

/// Rows under one heading (a class, a day…), with an optional subtotal row.
#[derive(Debug, Clone, Serialize, Default)]
pub struct Group {
    pub heading: Option<String>,
    pub rows: Vec<Vec<String>>,
    pub subtotal: Option<Vec<String>>,
}

/// A parameter as shown in the report header, e.g. ("Class", "9").
#[derive(Debug, Clone, Serialize)]
pub struct ShownParam {
    pub label: String,
    pub value: String,
}

/// The report document. Every renderer reads only this.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub code: String,
    pub lang: Lang,
    pub school_name: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub params: Vec<ShownParam>,
    pub columns: Vec<Column>,
    pub groups: Vec<Group>,
    pub total: Option<Vec<String>>,
    pub notes: Vec<String>,
    pub landscape: bool,
    /// Local date of the run, already formatted.
    pub generated_on: String,
    pub row_count: usize,
}

impl Report {
    pub fn new(code: &str, lang: Lang, school_name: &str, title: &str) -> Report {
        Report {
            code: code.into(),
            lang,
            school_name: school_name.into(),
            title: title.into(),
            subtitle: None,
            params: Vec::new(),
            columns: Vec::new(),
            groups: Vec::new(),
            total: None,
            notes: Vec::new(),
            landscape: false,
            generated_on: format_date(crate::clock::today()),
            row_count: 0,
        }
    }

    pub fn param(&mut self, label_key: &str, value: impl Into<String>) {
        self.params.push(ShownParam {
            label: text::tr(self.lang, label_key).into(),
            value: value.into(),
        });
    }

    /// Append rows as one group and keep the row count current.
    pub fn push_group(&mut self, group: Group) {
        self.row_count += group.rows.len();
        self.groups.push(group);
    }

    /// Headers and flat rows for Excel: group headings and subtotals become rows of their
    /// own so the sheet reads like the PDF.
    pub fn flat(&self) -> (Vec<String>, Vec<Vec<String>>) {
        let headers = self.columns.iter().map(|c| c.label.clone()).collect();
        let mut rows = Vec::new();
        let width = self.columns.len();
        for g in &self.groups {
            if let Some(h) = &g.heading {
                let mut row = vec![h.clone()];
                row.resize(width, String::new());
                rows.push(row);
            }
            rows.extend(g.rows.iter().cloned());
            if let Some(s) = &g.subtotal {
                rows.push(s.clone());
            }
        }
        if let Some(t) = &self.total {
            rows.push(t.clone());
        }
        (headers, rows)
    }

    /// File name stem for downloads: code plus the parameters that narrow it.
    pub fn file_stem(&self) -> String {
        let mut stem = self.code.replace('.', "-");
        for p in &self.params {
            let v: String = p
                .value
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '-')
                .take(24)
                .collect();
            if !v.is_empty() && v.len() <= 24 {
                stem.push('-');
                stem.push_str(&v);
            }
        }
        stem
    }
}

/// Dates in reports are `DD-MM-YYYY` (the convention on Pakistani school forms).
pub fn format_date(d: NaiveDate) -> String {
    d.format("%d-%m-%Y").to_string()
}

/// Raw query parameters of a report run, with typed accessors that produce plain-language
/// validation errors.
#[derive(Debug, Clone, Default)]
pub struct Params(pub HashMap<String, String>);

impl Params {
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(|s| s.trim()).filter(|s| !s.is_empty())
    }
    pub fn uuid(&self, name: &str) -> Result<Option<Uuid>, ApiError> {
        match self.get(name) {
            None => Ok(None),
            Some(s) => s
                .parse()
                .map(Some)
                .map_err(|_| ApiError::validation(format!("{name} is not a valid id."))),
        }
    }
    pub fn date(&self, name: &str) -> Result<Option<NaiveDate>, ApiError> {
        match self.get(name) {
            None => Ok(None),
            Some(s) => NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .map(Some)
                .map_err(|_| ApiError::validation(format!("{name} must be a date (YYYY-MM-DD)."))),
        }
    }
    /// `YYYY-MM` → (year, month).
    pub fn month(&self, name: &str) -> Result<Option<(i32, u32)>, ApiError> {
        match self.get(name) {
            None => Ok(None),
            Some(s) => {
                let parts: Vec<&str> = s.split('-').collect();
                let parsed = (parts.len() == 2)
                    .then(|| Some((parts[0].parse::<i32>().ok()?, parts[1].parse::<u32>().ok()?)))
                    .flatten()
                    .filter(|(_, m)| (1..=12).contains(m));
                parsed.map(Some).ok_or_else(|| {
                    ApiError::validation(format!("{name} must be a month (YYYY-MM)."))
                })
            }
        }
    }
    /// Everything, for the audit log.
    pub fn as_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.0).unwrap_or(serde_json::Value::Null)
    }
}
