//! Import wizard rules: the three import kinds and their fields, header auto-mapping, cell
//! cleanup (trim, dates in several formats, phone/CNIC, gender, class names, Urdu digits)
//! and row validation. Pure; the API reads the files and talks to the database.

use crate::students::{normalise_cnic, normalise_phone};
use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportKind {
    Students,
    Staff,
    OpeningBalances,
}

impl ImportKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ImportKind::Students => "students",
            ImportKind::Staff => "staff",
            ImportKind::OpeningBalances => "opening_balances",
        }
    }
}

impl std::str::FromStr for ImportKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "students" => Ok(ImportKind::Students),
            "staff" => Ok(ImportKind::Staff),
            "opening_balances" => Ok(ImportKind::OpeningBalances),
            other => Err(format!("unknown import kind: {other}")),
        }
    }
}

/// One importable field.
#[derive(Debug, Clone, Serialize)]
pub struct FieldDef {
    pub key: &'static str,
    pub label: &'static str,
    pub required: bool,
    /// Header spellings (lower-case, English and Urdu) that map to this field automatically.
    pub aliases: &'static [&'static str],
    pub example: &'static str,
}

const fn f(
    key: &'static str,
    label: &'static str,
    required: bool,
    aliases: &'static [&'static str],
    example: &'static str,
) -> FieldDef {
    FieldDef {
        key,
        label,
        required,
        aliases,
        example,
    }
}

pub const STUDENT_FIELDS: &[FieldDef] = &[
    f(
        "student_number",
        "Student number",
        false,
        &[
            "student number",
            "student no",
            "student id",
            "admission no",
            "admission number",
            "gr no",
            "gr number",
            "طالب علم نمبر",
            "داخلہ نمبر",
        ],
        "KHI-MC-2026-00417",
    ),
    f(
        "legacy_id",
        "Legacy ID",
        false,
        &[
            "legacy id",
            "old id",
            "old number",
            "roll number old",
            "پرانا نمبر",
        ],
        "S-417",
    ),
    f(
        "full_name",
        "Student name",
        true,
        &[
            "name",
            "student name",
            "full name",
            "student",
            "نام",
            "طالب علم کا نام",
        ],
        "Ali Khan",
    ),
    f(
        "father_name",
        "Father's name",
        true,
        &[
            "father name",
            "father's name",
            "father",
            "guardian name father",
            "والد کا نام",
            "والد",
        ],
        "Ahmed Khan",
    ),
    f(
        "date_of_birth",
        "Date of birth",
        true,
        &[
            "date of birth",
            "dob",
            "birth date",
            "birthday",
            "تاریخ پیدائش",
        ],
        "2015-03-04",
    ),
    f("gender", "Gender", true, &["gender", "sex", "جنس"], "male"),
    f(
        "cnic",
        "B-Form / CNIC",
        false,
        &[
            "cnic",
            "b-form",
            "b form",
            "bform",
            "b-form no",
            "cnic/b-form",
            "شناختی کارڈ",
            "ب فارم",
        ],
        "42101-1234567-1",
    ),
    f(
        "address",
        "Address",
        false,
        &["address", "home address", "پتہ"],
        "House 12, Gulshan",
    ),
    f(
        "admission_date",
        "Admission date",
        false,
        &[
            "admission date",
            "date of admission",
            "joined",
            "تاریخ داخلہ",
        ],
        "2026-04-01",
    ),
    f(
        "class",
        "Class",
        true,
        &["class", "grade", "class name", "standard", "جماعت", "کلاس"],
        "1",
    ),
    f(
        "section",
        "Section",
        false,
        &["section", "sec", "سیکشن"],
        "A",
    ),
    f(
        "roll_no",
        "Roll no.",
        false,
        &["roll no", "roll number", "roll", "رول نمبر"],
        "7",
    ),
    f(
        "board_roll_no",
        "Board roll no.",
        false,
        &["board roll no", "board roll number", "بورڈ رول نمبر"],
        "",
    ),
    f(
        "guardian_name",
        "Guardian name",
        false,
        &[
            "guardian name",
            "guardian",
            "parent name",
            "mother name",
            "سرپرست کا نام",
            "سرپرست",
        ],
        "Ahmed Khan",
    ),
    f(
        "guardian_relationship",
        "Guardian relationship",
        false,
        &["relationship", "relation", "guardian relationship", "رشتہ"],
        "father",
    ),
    f(
        "guardian_phone",
        "Guardian phone",
        false,
        &[
            "phone",
            "mobile",
            "contact",
            "guardian phone",
            "father phone",
            "parent phone",
            "phone number",
            "فون",
            "موبائل",
            "رابطہ نمبر",
        ],
        "0300-1234567",
    ),
    f(
        "guardian_cnic",
        "Guardian CNIC",
        false,
        &[
            "guardian cnic",
            "father cnic",
            "parent cnic",
            "سرپرست شناختی کارڈ",
        ],
        "42101-7654321-1",
    ),
    f(
        "guardian_occupation",
        "Guardian occupation",
        false,
        &["occupation", "profession", "guardian occupation", "پیشہ"],
        "Trader",
    ),
];

pub const STAFF_FIELDS: &[FieldDef] = &[
    f(
        "staff_number",
        "Staff number",
        false,
        &[
            "staff number",
            "staff no",
            "staff id",
            "employee id",
            "employee no",
            "عملہ نمبر",
            "ملازم نمبر",
        ],
        "S-0012",
    ),
    f(
        "full_name",
        "Name",
        true,
        &["name", "full name", "staff name", "employee name", "نام"],
        "Sana Malik",
    ),
    f(
        "father_name",
        "Father's name",
        false,
        &["father name", "father's name", "father", "والد کا نام"],
        "Tariq Malik",
    ),
    f(
        "gender",
        "Gender",
        false,
        &["gender", "sex", "جنس"],
        "female",
    ),
    f(
        "phone",
        "Phone",
        false,
        &[
            "phone",
            "mobile",
            "contact",
            "phone number",
            "فون",
            "موبائل",
        ],
        "0300-1234567",
    ),
    f(
        "email",
        "Email",
        false,
        &["email", "e-mail", "ای میل"],
        "sana@example.com",
    ),
    f(
        "cnic",
        "CNIC",
        false,
        &["cnic", "id card", "شناختی کارڈ"],
        "42101-1234567-1",
    ),
    f(
        "designation",
        "Designation",
        false,
        &["designation", "role", "position", "title", "عہدہ"],
        "Teacher",
    ),
    f(
        "joined_on",
        "Joined on",
        false,
        &[
            "joined on",
            "joining date",
            "date of joining",
            "joined",
            "تاریخ تقرری",
        ],
        "2020-08-01",
    ),
];

pub const BALANCE_FIELDS: &[FieldDef] = &[
    f(
        "student_number",
        "Student number",
        false,
        &[
            "student number",
            "student no",
            "student id",
            "admission no",
            "gr no",
            "طالب علم نمبر",
        ],
        "KHI-MC-2026-00417",
    ),
    f(
        "legacy_id",
        "Legacy ID",
        false,
        &["legacy id", "old id", "old number", "پرانا نمبر"],
        "S-417",
    ),
    f(
        "amount",
        "Opening balance",
        true,
        &[
            "amount",
            "balance",
            "opening balance",
            "outstanding",
            "dues",
            "arrears",
            "بقایا",
            "رقم",
        ],
        "4500",
    ),
    f(
        "note",
        "Note",
        false,
        &["note", "remarks", "نوٹ"],
        "Carried from old register",
    ),
];

pub fn fields(kind: ImportKind) -> &'static [FieldDef] {
    match kind {
        ImportKind::Students => STUDENT_FIELDS,
        ImportKind::Staff => STAFF_FIELDS,
        ImportKind::OpeningBalances => BALANCE_FIELDS,
    }
}

/// Lower-case, Latin digits, single spaces, no punctuation except hyphen and slash.
pub fn normalize_header(h: &str) -> String {
    let s = clean_text(h).to_lowercase();
    let s: String = s
        .chars()
        .map(|c| match c {
            '_' | '.' | ':' | '(' | ')' | '*' | '#' => ' ',
            c => c,
        })
        .collect();
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Field key → column index, matched by alias or by the field key itself.
pub fn auto_map(kind: ImportKind, headers: &[String]) -> BTreeMap<String, usize> {
    let normalized: Vec<String> = headers.iter().map(|h| normalize_header(h)).collect();
    let mut map = BTreeMap::new();
    let mut used = vec![false; headers.len()];
    for field in fields(kind) {
        let key_alias = field.key.replace('_', " ");
        let hit = normalized.iter().enumerate().find(|(i, h)| {
            !used[*i] && (h.as_str() == key_alias || field.aliases.contains(&h.as_str()))
        });
        if let Some((i, _)) = hit {
            used[i] = true;
            map.insert(field.key.to_string(), i);
        }
    }
    map
}

// ---- cleanup ------------------------------------------------------------------------------

/// Trim, collapse whitespace, strip control characters and replace Urdu/Arabic-Indic digits
/// with Latin ones. Urdu letters are kept as typed.
pub fn clean_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let mapped = match c {
            '\u{0660}'..='\u{0669}' => char::from(b'0' + (c as u32 - 0x0660) as u8),
            '\u{06F0}'..='\u{06F9}' => char::from(b'0' + (c as u32 - 0x06F0) as u8),
            '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{FEFF}' => continue,
            c if c.is_control() => ' ',
            c => c,
        };
        out.push(mapped);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Accepts `2015-03-04`, `04/03/2015`, `04-03-2015`, `4.3.2015`, `04 Mar 2015`,
/// `March 4, 2015` and Excel serial numbers (e.g. `42067`). Day-first for numeric dates.
pub fn parse_date(input: &str) -> Option<NaiveDate> {
    let s = clean_text(input);
    if s.is_empty() {
        return None;
    }
    if let Ok(serial) = s.parse::<f64>() {
        if (20000.0..80000.0).contains(&serial) {
            let base = NaiveDate::from_ymd_opt(1899, 12, 30)?;
            return base.checked_add_signed(chrono::Duration::days(serial as i64));
        }
    }
    let formats = [
        "%Y-%m-%d",
        "%Y/%m/%d",
        "%d/%m/%Y",
        "%d-%m-%Y",
        "%d.%m.%Y",
        "%d/%m/%y",
        "%d-%m-%y",
        "%d %b %Y",
        "%d %B %Y",
        "%b %d, %Y",
        "%B %d, %Y",
        "%d-%b-%Y",
        "%d-%b-%y",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
    ];
    for fmt in formats {
        if let Ok(d) = NaiveDate::parse_from_str(&s, fmt) {
            return Some(d);
        }
    }
    None
}

pub fn parse_gender(input: &str) -> Option<&'static str> {
    match clean_text(input).to_lowercase().as_str() {
        "m" | "male" | "boy" | "b" | "مرد" | "لڑکا" | "ذکر" => Some("male"),
        "f" | "female" | "girl" | "g" | "عورت" | "لڑکی" | "مؤنث" | "خاتون" => {
            Some("female")
        }
        "other" | "o" | "دیگر" => Some("other"),
        _ => None,
    }
}

pub fn parse_relationship(input: &str) -> Option<&'static str> {
    match clean_text(input).to_lowercase().as_str() {
        "" | "father" | "dad" | "abu" | "walid" | "والد" | "ابو" => Some("father"),
        "mother" | "mom" | "mum" | "ami" | "walida" | "والدہ" | "امی" => Some("mother"),
        "guardian" | "uncle" | "aunt" | "grandfather" | "grandmother" | "brother" | "sister"
        | "سرپرست" | "چچا" | "ماموں" | "دادا" | "نانا" | "بھائی" | "بہن" => {
            Some("guardian")
        }
        "other" | "دیگر" => Some("other"),
        _ => None,
    }
}

/// Money: strips `Rs`, commas and spaces; at most two decimals; not negative.
pub fn parse_money(input: &str) -> Option<Decimal> {
    let s: String = clean_text(input)
        .to_lowercase()
        .replace("rs.", "")
        .replace("rs", "")
        .replace("pkr", "")
        .replace([',', ' '], "");
    if s.is_empty() {
        return None;
    }
    let d: Decimal = s.parse().ok()?;
    if d < Decimal::ZERO || d.round_dp(2) != d {
        return None;
    }
    Some(d)
}

/// Normalise a class name for matching: `Class 1`, `Grade 1`, `1st`, `I`, `One` → `1`;
/// `KG`, `K.G.`, `Kindergarten` → `kg`; `Nursery`/`Prep`/`Playgroup` keep their word.
pub fn class_key(input: &str) -> String {
    let s = clean_text(input).to_lowercase();
    let s = s
        .replace("class", " ")
        .replace("grade", " ")
        .replace("standard", " ")
        .replace("std", " ")
        .replace("جماعت", " ")
        .replace("کلاس", " ")
        .replace(['.', '-', '_'], " ");
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let word = match s.as_str() {
        "k g" | "kg" | "kindergarten" | "kgarten" | "ک جی" => return "kg".into(),
        "nursery" | "nur" | "نرسری" => return "nursery".into(),
        "prep" | "preparatory" | "پریپ" => return "prep".into(),
        "playgroup" | "play group" | "pg" => return "playgroup".into(),
        other => other.to_string(),
    };
    let stripped = word
        .trim_end_matches("st")
        .trim_end_matches("nd")
        .trim_end_matches("rd")
        .trim_end_matches("th")
        .trim();
    if let Ok(n) = stripped.parse::<u32>() {
        return n.to_string();
    }
    let roman = [
        ("i", 1),
        ("ii", 2),
        ("iii", 3),
        ("iv", 4),
        ("v", 5),
        ("vi", 6),
        ("vii", 7),
        ("viii", 8),
        ("ix", 9),
        ("x", 10),
        ("xi", 11),
        ("xii", 12),
    ];
    let words = [
        ("one", 1),
        ("two", 2),
        ("three", 3),
        ("four", 4),
        ("five", 5),
        ("six", 6),
        ("seven", 7),
        ("eight", 8),
        ("nine", 9),
        ("ten", 10),
        ("eleven", 11),
        ("twelve", 12),
        ("ایک", 1),
        ("دو", 2),
        ("تین", 3),
        ("چار", 4),
        ("پانچ", 5),
        ("چھ", 6),
        ("سات", 7),
        ("آٹھ", 8),
        ("نو", 9),
        ("دس", 10),
    ];
    for (k, n) in roman.iter().chain(words.iter()) {
        if *k == word {
            return n.to_string();
        }
    }
    word
}

/// Split a combined class value such as `1-A`, `1 A`, `1/A`, `Class 1 (A)` into class and
/// section. Returns the class key and the upper-case section when present.
pub fn split_class_section(input: &str) -> (String, Option<String>) {
    let s = clean_text(input).replace(['(', ')'], " ");
    let s = s.trim();
    let parts: Vec<&str> = s
        .split(|c: char| c == '-' || c == '/' || c.is_whitespace())
        .filter(|p| !p.is_empty())
        .collect();
    if parts.len() >= 2 {
        let last = parts[parts.len() - 1];
        let is_section = last.chars().count() == 1 && last.chars().all(|c| c.is_alphabetic());
        if is_section {
            let class = parts[..parts.len() - 1].join(" ");
            return (class_key(&class), Some(last.to_uppercase()));
        }
    }
    (class_key(s), None)
}

// ---- rows ---------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    pub field: String,
    pub message: String,
}

/// A cleaned row: canonical string values per field, plus what is wrong with it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CleanRow {
    pub row_no: usize,
    pub values: BTreeMap<String, String>,
    pub errors: Vec<Issue>,
    pub warnings: Vec<Issue>,
}

impl CleanRow {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values
            .get(key)
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    }
    fn err(&mut self, field: &str, message: impl Into<String>) {
        self.errors.push(Issue {
            field: field.into(),
            message: message.into(),
        });
    }
    fn warn(&mut self, field: &str, message: impl Into<String>) {
        self.warnings.push(Issue {
            field: field.into(),
            message: message.into(),
        });
    }
}

/// Pick the mapped cells of a raw row, clean them and validate the formats. Database checks
/// (class exists, duplicates, student found) happen in the API on top of this.
pub fn clean_row(
    kind: ImportKind,
    row_no: usize,
    raw: &[String],
    mapping: &BTreeMap<String, usize>,
) -> CleanRow {
    let mut row = CleanRow {
        row_no,
        ..Default::default()
    };
    for field in fields(kind) {
        let value = mapping
            .get(field.key)
            .and_then(|i| raw.get(*i))
            .map(|v| clean_text(v))
            .unwrap_or_default();
        row.values.insert(field.key.to_string(), value);
    }
    for field in fields(kind) {
        let value = row.values[field.key].clone();
        if value.is_empty() {
            if field.required {
                row.err(field.key, format!("{} is required", field.label));
            }
            continue;
        }
        match field.key {
            "date_of_birth" | "admission_date" | "joined_on" => match parse_date(&value) {
                Some(d) => {
                    if d > chrono::Utc::now().date_naive() {
                        row.err(field.key, format!("{} is in the future", field.label));
                    } else if field.key == "date_of_birth" && d.year() < 1990 {
                        row.warn(field.key, "Date of birth looks too old for a student");
                    }
                    row.values
                        .insert(field.key.into(), d.format("%Y-%m-%d").to_string());
                }
                None => row.err(
                    field.key,
                    format!(
                        "{} is not a date (use YYYY-MM-DD or DD/MM/YYYY)",
                        field.label
                    ),
                ),
            },
            "gender" => match parse_gender(&value) {
                Some(g) => {
                    row.values.insert("gender".into(), g.into());
                }
                None => row.err("gender", "Gender must be male or female"),
            },
            "cnic" | "guardian_cnic" => match normalise_cnic(&value) {
                Some(c) => {
                    row.values.insert(field.key.into(), c);
                }
                None => row.err(field.key, format!("{} must have 13 digits", field.label)),
            },
            "phone" | "guardian_phone" => match normalise_phone(&value) {
                Some(p) => {
                    row.values.insert(field.key.into(), p);
                }
                None => row.err(
                    field.key,
                    format!("{} must have 11 digits, e.g. 0300-1234567", field.label),
                ),
            },
            "guardian_relationship" => match parse_relationship(&value) {
                Some(r) => {
                    row.values.insert(field.key.into(), r.into());
                }
                None => row.warn(
                    field.key,
                    "Relationship not recognised; recorded as guardian",
                ),
            },
            "roll_no" => {
                if value.parse::<u32>().is_err() {
                    row.err("roll_no", "Roll no. must be a whole number");
                }
            }
            "amount" => match parse_money(&value) {
                Some(d) => {
                    row.values.insert("amount".into(), d.to_string());
                }
                None => row.err(
                    "amount",
                    "Amount must be a positive number with at most two decimals",
                ),
            },
            "class" => {
                let (class, section) = split_class_section(&value);
                row.values.insert("class".into(), class);
                if let Some(sec) = section {
                    if row.get("section").is_none() {
                        row.values.insert("section".into(), sec);
                    }
                }
            }
            "section" => {
                row.values.insert("section".into(), value.to_uppercase());
            }
            "email" if !value.contains('@') => row.err("email", "Email looks wrong"),
            _ => {}
        }
    }
    // Phone or CNIC without a guardian name: the guardian is the father.
    if kind == ImportKind::Students
        && row.get("guardian_name").is_none()
        && (row.get("guardian_phone").is_some() || row.get("guardian_cnic").is_some())
    {
        if let Some(father) = row.get("father_name").map(str::to_owned) {
            row.values.insert("guardian_name".into(), father);
            row.values
                .insert("guardian_relationship".into(), "father".into());
        }
    }
    if kind == ImportKind::Students
        && row.get("guardian_relationship").is_none()
        && row.get("guardian_name").is_some()
    {
        row.values
            .insert("guardian_relationship".into(), "father".into());
    }
    if kind == ImportKind::OpeningBalances
        && row.get("student_number").is_none()
        && row.get("legacy_id").is_none()
    {
        row.err("student_number", "Give the student number or the legacy ID");
    }
    row
}

/// Key that identifies a student row for duplicate checks inside one file.
pub fn student_file_key(row: &CleanRow) -> Option<String> {
    let bad = |field: &str| row.errors.iter().any(|e| e.field == field);
    if let Some(c) = row.get("cnic").filter(|_| !bad("cnic")) {
        return Some(format!("cnic:{c}"));
    }
    if bad("full_name") || bad("father_name") || bad("date_of_birth") {
        return None;
    }
    match (
        row.get("full_name"),
        row.get("father_name"),
        row.get("date_of_birth"),
    ) {
        (Some(n), Some(f), Some(d)) => Some(format!(
            "n:{}|{}|{}",
            crate::students::name_key(n),
            crate::students::name_key(f),
            d
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn headers_map_by_alias_in_english_and_urdu() {
        let headers: Vec<String> = [
            "Sr.",
            "Student Name",
            "Father's Name",
            "DOB",
            "Sex",
            "B-Form No",
            "Class",
            "Section",
            "Mobile",
            "والد کا نام",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let m = auto_map(ImportKind::Students, &headers);
        assert_eq!(m["full_name"], 1);
        assert_eq!(m["father_name"], 2, "first match wins");
        assert_eq!(m["date_of_birth"], 3);
        assert_eq!(m["gender"], 4);
        assert_eq!(m["cnic"], 5);
        assert_eq!(m["class"], 6);
        assert_eq!(m["section"], 7);
        assert_eq!(m["guardian_phone"], 8);
        assert!(!m.contains_key("student_number"));
        let ur: Vec<String> = ["نام", "تاریخ پیدائش", "جنس", "جماعت"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let m = auto_map(ImportKind::Students, &ur);
        assert_eq!(
            (m["full_name"], m["date_of_birth"], m["gender"], m["class"]),
            (0, 1, 2, 3)
        );
        let staff: Vec<String> = ["Employee ID", "Name", "Designation", "Joining Date"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let m = auto_map(ImportKind::Staff, &staff);
        assert_eq!(
            (m["staff_number"], m["designation"], m["joined_on"]),
            (0, 2, 3)
        );
    }

    #[test]
    fn cleanup_handles_messy_text_dates_phones_and_digits() {
        assert_eq!(clean_text("  Ali   Khan \u{200B}"), "Ali Khan");
        assert_eq!(clean_text("۰۳۰۰-۱۲۳۴۵۶۷"), "0300-1234567");
        assert_eq!(clean_text("٠٣٠٠١٢٣٤٥٦٧"), "03001234567");
        assert_eq!(parse_date("2015-03-04"), Some(d(2015, 3, 4)));
        assert_eq!(parse_date("04/03/2015"), Some(d(2015, 3, 4)), "day first");
        assert_eq!(parse_date("4.3.2015"), Some(d(2015, 3, 4)));
        assert_eq!(parse_date("04-Mar-2015"), Some(d(2015, 3, 4)));
        assert_eq!(parse_date("March 4, 2015"), Some(d(2015, 3, 4)));
        assert_eq!(parse_date("42067"), Some(d(2015, 3, 4)), "Excel serial");
        assert_eq!(parse_date("2015-03-04T00:00:00"), Some(d(2015, 3, 4)));
        assert_eq!(parse_date("not a date"), None);
        assert_eq!(parse_date("31/02/2015"), None);
        assert_eq!(parse_gender(" F "), Some("female"));
        assert_eq!(parse_gender("لڑکا"), Some("male"));
        assert_eq!(parse_gender("x"), None);
        assert_eq!(parse_relationship("Abu"), Some("father"));
        assert_eq!(parse_relationship("والدہ"), Some("mother"));
        assert_eq!(parse_relationship("Uncle"), Some("guardian"));
        assert_eq!(parse_money("Rs. 4,500"), Some(dec!(4500)));
        assert_eq!(parse_money("1250.50"), Some(dec!(1250.50)));
        assert_eq!(parse_money("-5"), None);
        assert_eq!(parse_money("12.345"), None);
    }

    #[test]
    fn class_names_match_tolerantly() {
        for (input, key) in [
            ("Class 1", "1"),
            ("Grade 1", "1"),
            ("1st", "1"),
            ("I", "1"),
            ("One", "1"),
            ("1", "1"),
            ("Class-10", "10"),
            ("X", "10"),
            ("Std. 5", "5"),
            ("جماعت 3", "3"),
            ("تین", "3"),
            ("K.G.", "kg"),
            ("Kindergarten", "kg"),
            ("Nursery", "nursery"),
            ("Prep", "prep"),
        ] {
            assert_eq!(class_key(input), key, "{input}");
        }
        assert_eq!(split_class_section("1-A"), ("1".into(), Some("A".into())));
        assert_eq!(
            split_class_section("Class 7 (b)"),
            ("7".into(), Some("B".into()))
        );
        assert_eq!(split_class_section("KG/C"), ("kg".into(), Some("C".into())));
        assert_eq!(split_class_section("Grade 2"), ("2".into(), None));
        assert_eq!(split_class_section("Nursery"), ("nursery".into(), None));
    }

    fn mapping(pairs: &[(&str, usize)]) -> BTreeMap<String, usize> {
        pairs.iter().map(|(k, i)| (k.to_string(), *i)).collect()
    }

    #[test]
    fn student_rows_are_cleaned_and_validated() {
        let m = mapping(&[
            ("full_name", 0),
            ("father_name", 1),
            ("date_of_birth", 2),
            ("gender", 3),
            ("cnic", 4),
            ("class", 5),
            ("guardian_phone", 6),
            ("roll_no", 7),
        ]);
        let raw = |cells: &[&str]| cells.iter().map(|c| c.to_string()).collect::<Vec<_>>();
        let good = clean_row(
            ImportKind::Students,
            2,
            &raw(&[
                " Ali  Khan",
                "Ahmed Khan",
                "04/03/2015",
                "M",
                "4210112345671",
                "Class 1-A",
                "0300 1234567",
                "7",
            ]),
            &m,
        );
        assert!(good.errors.is_empty(), "{:?}", good.errors);
        assert_eq!(good.get("full_name"), Some("Ali Khan"));
        assert_eq!(good.get("date_of_birth"), Some("2015-03-04"));
        assert_eq!(good.get("gender"), Some("male"));
        assert_eq!(good.get("cnic"), Some("42101-1234567-1"));
        assert_eq!(
            (good.get("class"), good.get("section")),
            (Some("1"), Some("A"))
        );
        assert_eq!(good.get("guardian_phone"), Some("03001234567"));
        assert_eq!(
            good.get("guardian_name"),
            Some("Ahmed Khan"),
            "phone without name → father"
        );
        assert_eq!(good.get("guardian_relationship"), Some("father"));
        assert_eq!(
            student_file_key(&good).as_deref(),
            Some("cnic:42101-1234567-1")
        );

        let bad = clean_row(
            ImportKind::Students,
            3,
            &raw(&["", "Ahmed", "2015-13-40", "?", "123", "", "12", "x"]),
            &m,
        );
        let fields: Vec<&str> = bad.errors.iter().map(|e| e.field.as_str()).collect();
        assert_eq!(
            fields,
            [
                "full_name",
                "date_of_birth",
                "gender",
                "cnic",
                "class",
                "roll_no",
                "guardian_phone"
            ]
        );
        assert!(student_file_key(&bad).is_none());

        let future = clean_row(
            ImportKind::Students,
            4,
            &raw(&["Sara", "Ahmed", "2099-01-01", "F", "", "KG", "", ""]),
            &m,
        );
        assert_eq!(future.errors.len(), 1);
        assert_eq!(future.errors[0].field, "date_of_birth");
        let old = clean_row(
            ImportKind::Students,
            5,
            &raw(&["Sara", "Ahmed", "1985-01-01", "F", "", "KG", "", ""]),
            &m,
        );
        assert!(old.errors.is_empty());
        assert_eq!(old.warnings[0].field, "date_of_birth");
        assert_eq!(
            student_file_key(&old).as_deref(),
            Some("n:sara|ahmed|1985-01-01")
        );
    }

    #[test]
    fn staff_and_balance_rows() {
        let m = mapping(&[
            ("full_name", 0),
            ("phone", 1),
            ("email", 2),
            ("joined_on", 3),
            ("gender", 4),
        ]);
        let row = clean_row(
            ImportKind::Staff,
            2,
            &[
                "Sana Malik".into(),
                "+92 300 1234567".into(),
                "sana@example.com".into(),
                "01/08/2020".into(),
                "Female".into(),
            ],
            &m,
        );
        assert!(row.errors.is_empty(), "{:?}", row.errors);
        assert_eq!(row.get("phone"), Some("03001234567"));
        assert_eq!(row.get("joined_on"), Some("2020-08-01"));
        let bad = clean_row(
            ImportKind::Staff,
            3,
            &["".into(), "12".into(), "nope".into(), "".into(), "".into()],
            &m,
        );
        assert_eq!(
            bad.errors
                .iter()
                .map(|e| e.field.as_str())
                .collect::<Vec<_>>(),
            ["full_name", "phone", "email"]
        );

        let m = mapping(&[("student_number", 0), ("amount", 1)]);
        let row = clean_row(
            ImportKind::OpeningBalances,
            2,
            &["KHI-MC-2026-00001".into(), "Rs 4,500.50".into()],
            &m,
        );
        assert!(row.errors.is_empty());
        assert_eq!(row.get("amount"), Some("4500.50"));
        let bad = clean_row(
            ImportKind::OpeningBalances,
            3,
            &["".into(), "abc".into()],
            &m,
        );
        assert_eq!(
            bad.errors
                .iter()
                .map(|e| e.field.as_str())
                .collect::<Vec<_>>(),
            ["amount", "student_number"]
        );
    }

    #[test]
    fn import_kind_round_trips() {
        for k in [
            ImportKind::Students,
            ImportKind::Staff,
            ImportKind::OpeningBalances,
        ] {
            assert_eq!(k.as_str().parse::<ImportKind>().unwrap(), k);
        }
        assert!("x".parse::<ImportKind>().is_err());
        assert!(
            fields(ImportKind::Students)
                .iter()
                .filter(|f| f.required)
                .count()
                >= 5
        );
    }
}
