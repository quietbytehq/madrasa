//! Student domain rules: numbering, identifier normalisation, duplicate matching, statuses.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Student lifecycle status. There is no hard delete.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StudentStatus {
    Active,
    Left,
    Graduated,
}

impl StudentStatus {
    pub const ALL: &'static [StudentStatus] = &[
        StudentStatus::Active,
        StudentStatus::Left,
        StudentStatus::Graduated,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            StudentStatus::Active => "active",
            StudentStatus::Left => "left",
            StudentStatus::Graduated => "graduated",
        }
    }
}

impl fmt::Display for StudentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for StudentStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        StudentStatus::ALL
            .iter()
            .copied()
            .find(|v| v.as_str() == s)
            .ok_or_else(|| format!("unknown student status: {s}"))
    }
}

/// Guardian relationship to a student.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Relationship {
    Father,
    Mother,
    Guardian,
    Other,
}

impl Relationship {
    pub const ALL: &'static [Relationship] = &[
        Relationship::Father,
        Relationship::Mother,
        Relationship::Guardian,
        Relationship::Other,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Relationship::Father => "father",
            Relationship::Mother => "mother",
            Relationship::Guardian => "guardian",
            Relationship::Other => "other",
        }
    }
}

impl FromStr for Relationship {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Relationship::ALL
            .iter()
            .copied()
            .find(|v| v.as_str() == s)
            .ok_or_else(|| format!("unknown relationship: {s}"))
    }
}

/// Maximum photo size stored in the database.
pub const MAX_PHOTO_BYTES: usize = 512 * 1024;

/// Format a student number: `<prefix>-<year>-<sequence padded to 5>`, e.g. `KHI-MC-2026-00417`.
pub fn format_student_number(prefix: &str, year: i32, sequence: u32) -> String {
    format!("{}-{year}-{sequence:05}", normalise_prefix(prefix))
}

/// Prefix rules: uppercase, letters/digits/hyphens only, no leading/trailing hyphen.
pub fn normalise_prefix(prefix: &str) -> String {
    let cleaned: String = prefix
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '-'
            }
        })
        .collect();
    let mut out = String::new();
    for c in cleaned.chars() {
        if c == '-' && (out.is_empty() || out.ends_with('-')) {
            continue;
        }
        out.push(c);
    }
    out.trim_end_matches('-').to_string()
}

/// Normalise a CNIC or B-Form number to `#####-#######-#`. Returns `None` if it is not 13 digits.
pub fn normalise_cnic(input: &str) -> Option<String> {
    let digits: String = input.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() != 13 {
        return None;
    }
    Some(format!(
        "{}-{}-{}",
        &digits[0..5],
        &digits[5..12],
        &digits[12..13]
    ))
}

/// Normalise a Pakistani phone number: keeps digits, maps `+92`/`92` prefixes to a leading `0`.
/// Returns `None` for anything that is not 10–11 digits after normalisation.
pub fn normalise_phone(input: &str) -> Option<String> {
    let mut digits: String = input.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.starts_with("0092") {
        digits = format!("0{}", &digits[4..]);
    } else if digits.starts_with("92") && digits.len() >= 12 {
        digits = format!("0{}", &digits[2..]);
    } else if !digits.starts_with('0') && digits.len() == 10 {
        digits = format!("0{digits}");
    }
    (digits.len() == 11 && digits.starts_with('0')).then_some(digits)
}

/// Collapse whitespace and case for name comparison.
pub fn name_key(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// The identity facts used for duplicate detection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateProbe {
    pub full_name: String,
    pub father_name: String,
    pub date_of_birth: chrono::NaiveDate,
    /// Normalised CNIC/B-Form, if any.
    pub cnic: Option<String>,
}

/// Why two records look like the same student.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DuplicateReason {
    SameCnic,
    SameNameDobFather,
}

/// Compare a probe against an existing record.
pub fn duplicate_reason(
    new: &DuplicateProbe,
    existing: &DuplicateProbe,
) -> Option<DuplicateReason> {
    if let (Some(a), Some(b)) = (&new.cnic, &existing.cnic) {
        if a == b {
            return Some(DuplicateReason::SameCnic);
        }
    }
    if name_key(&new.full_name) == name_key(&existing.full_name)
        && name_key(&new.father_name) == name_key(&existing.father_name)
        && new.date_of_birth == existing.date_of_birth
    {
        return Some(DuplicateReason::SameNameDobFather);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn student_number_format() {
        assert_eq!(
            format_student_number("KHI-MC", 2026, 417),
            "KHI-MC-2026-00417"
        );
        assert_eq!(
            format_student_number(" khi mc ", 2026, 1),
            "KHI-MC-2026-00001"
        );
        assert_eq!(
            format_student_number("ghs01", 2027, 123456),
            "GHS01-2027-123456"
        );
        assert_eq!(normalise_prefix("--a__b--"), "A-B");
    }

    #[test]
    fn cnic_normalisation() {
        assert_eq!(
            normalise_cnic("42101-1234567-1").unwrap(),
            "42101-1234567-1"
        );
        assert_eq!(normalise_cnic("4210112345671").unwrap(), "42101-1234567-1");
        assert_eq!(
            normalise_cnic(" 42101 1234567 1 ").unwrap(),
            "42101-1234567-1"
        );
        assert!(normalise_cnic("12345").is_none());
        assert!(normalise_cnic("").is_none());
    }

    #[test]
    fn phone_normalisation() {
        assert_eq!(normalise_phone("0300-1234567").unwrap(), "03001234567");
        assert_eq!(normalise_phone("+92 300 1234567").unwrap(), "03001234567");
        assert_eq!(normalise_phone("00923001234567").unwrap(), "03001234567");
        assert_eq!(normalise_phone("3001234567").unwrap(), "03001234567");
        assert!(normalise_phone("12345").is_none());
        assert!(normalise_phone("abc").is_none());
    }

    #[test]
    fn duplicate_rules() {
        let dob = NaiveDate::from_ymd_opt(2015, 3, 4).unwrap();
        let a = DuplicateProbe {
            full_name: "Ali Khan".into(),
            father_name: "Ahmed Khan".into(),
            date_of_birth: dob,
            cnic: Some("42101-1234567-1".into()),
        };
        let same_cnic = DuplicateProbe {
            full_name: "Other".into(),
            father_name: "X".into(),
            ..a.clone()
        };
        assert_eq!(
            duplicate_reason(&a, &same_cnic),
            Some(DuplicateReason::SameCnic)
        );
        let same_identity = DuplicateProbe {
            full_name: "  ali   KHAN ".into(),
            father_name: "ahmed khan".into(),
            date_of_birth: dob,
            cnic: None,
        };
        assert_eq!(
            duplicate_reason(&a, &same_identity),
            Some(DuplicateReason::SameNameDobFather)
        );
        let different = DuplicateProbe {
            full_name: "Ali Khan".into(),
            father_name: "Ahmed Khan".into(),
            date_of_birth: NaiveDate::from_ymd_opt(2014, 3, 4).unwrap(),
            cnic: Some("42101-7654321-1".into()),
        };
        assert_eq!(duplicate_reason(&a, &different), None);
    }

    #[test]
    fn enums_round_trip() {
        for s in StudentStatus::ALL {
            assert_eq!(s.as_str().parse::<StudentStatus>().unwrap(), *s);
            assert_eq!(
                serde_json::to_string(s).unwrap(),
                format!("\"{}\"", s.as_str())
            );
        }
        for r in Relationship::ALL {
            assert_eq!(r.as_str().parse::<Relationship>().unwrap(), *r);
        }
        assert!("alumni".parse::<StudentStatus>().is_err());
    }
}
