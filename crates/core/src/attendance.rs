//! Attendance rules: statuses, the school calendar (weekends and holidays) and summaries.
//! Pure; the database layer stores sheets and entries.
//!
//! Rules:
//! - Everyone is present by default. A teacher records only the exceptions (absent, late,
//!   leave); the stored sheet still holds one entry per enrolled student so history and
//!   corrections are exact.
//! - A sheet is submitted when it is first saved. After that only a user with
//!   `attendance.correct` may change it, and every changed entry is audit-logged.
//! - No attendance is taken on weekends or holidays.
//! - Attendance percentage = (present + late) / (present + absent + late + leave).

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// A student's status for one day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AttendanceStatus {
    #[default]
    Present,
    Absent,
    Late,
    Leave,
}

impl AttendanceStatus {
    pub const ALL: &'static [AttendanceStatus] = &[
        AttendanceStatus::Present,
        AttendanceStatus::Absent,
        AttendanceStatus::Late,
        AttendanceStatus::Leave,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            AttendanceStatus::Present => "present",
            AttendanceStatus::Absent => "absent",
            AttendanceStatus::Late => "late",
            AttendanceStatus::Leave => "leave",
        }
    }

    /// Late students were in school; they count towards attendance.
    pub fn counts_as_present(self) -> bool {
        matches!(self, AttendanceStatus::Present | AttendanceStatus::Late)
    }
}

impl fmt::Display for AttendanceStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AttendanceStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        AttendanceStatus::ALL
            .iter()
            .copied()
            .find(|v| v.as_str() == s)
            .ok_or_else(|| format!("unknown attendance status: {s}"))
    }
}

/// A staff member's status for one day (simpler than students: no "late").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StaffStatus {
    #[default]
    Present,
    Absent,
    Leave,
}

impl StaffStatus {
    pub const ALL: &'static [StaffStatus] = &[
        StaffStatus::Present,
        StaffStatus::Absent,
        StaffStatus::Leave,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            StaffStatus::Present => "present",
            StaffStatus::Absent => "absent",
            StaffStatus::Leave => "leave",
        }
    }
}

impl FromStr for StaffStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        StaffStatus::ALL
            .iter()
            .copied()
            .find(|v| v.as_str() == s)
            .ok_or_else(|| format!("unknown staff attendance status: {s}"))
    }
}

/// Counts for a set of entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AttendanceSummary {
    pub present: u32,
    pub absent: u32,
    pub late: u32,
    pub leave: u32,
}

impl AttendanceSummary {
    pub fn add(&mut self, status: AttendanceStatus) {
        match status {
            AttendanceStatus::Present => self.present += 1,
            AttendanceStatus::Absent => self.absent += 1,
            AttendanceStatus::Late => self.late += 1,
            AttendanceStatus::Leave => self.leave += 1,
        }
    }

    pub fn total(&self) -> u32 {
        self.present + self.absent + self.late + self.leave
    }

    /// Attendance percentage with one decimal, or `None` when nothing was marked.
    pub fn percent(&self) -> Option<f64> {
        let total = self.total();
        if total == 0 {
            return None;
        }
        let attended = f64::from(self.present + self.late);
        Some((attended * 1000.0 / f64::from(total)).round() / 10.0)
    }
}

pub fn summarize(statuses: impl IntoIterator<Item = AttendanceStatus>) -> AttendanceSummary {
    let mut s = AttendanceSummary::default();
    for st in statuses {
        s.add(st);
    }
    s
}

/// A holiday or closure: an inclusive date range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HolidaySpan {
    pub name: String,
    pub starts_on: NaiveDate,
    pub ends_on: NaiveDate,
}

impl HolidaySpan {
    pub fn contains(&self, date: NaiveDate) -> bool {
        self.starts_on <= date && date <= self.ends_on
    }
}

/// Weekly off days as ISO weekday numbers (1 = Monday … 7 = Sunday). Default: Sunday.
pub fn default_weekend() -> Vec<u32> {
    vec![7]
}

pub fn weekday_number(d: NaiveDate) -> u32 {
    d.weekday().number_from_monday()
}

pub fn weekday_from_number(n: u32) -> Option<Weekday> {
    match n {
        1 => Some(Weekday::Mon),
        2 => Some(Weekday::Tue),
        3 => Some(Weekday::Wed),
        4 => Some(Weekday::Thu),
        5 => Some(Weekday::Fri),
        6 => Some(Weekday::Sat),
        7 => Some(Weekday::Sun),
        _ => None,
    }
}

/// What kind of day a date is for the school.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DayKind {
    School,
    Weekend,
    Holiday { name: String },
}

impl DayKind {
    pub fn is_school_day(&self) -> bool {
        matches!(self, DayKind::School)
    }
}

/// Classify a date. A holiday wins over a weekend so the reason shown is the specific one.
pub fn day_kind(date: NaiveDate, weekend: &[u32], holidays: &[HolidaySpan]) -> DayKind {
    if let Some(h) = holidays.iter().find(|h| h.contains(date)) {
        return DayKind::Holiday {
            name: h.name.clone(),
        };
    }
    if weekend.contains(&weekday_number(date)) {
        return DayKind::Weekend;
    }
    DayKind::School
}

/// All dates of the month that holds `first` (any day of the month may be passed).
pub fn month_days(first: NaiveDate) -> Vec<NaiveDate> {
    let start = NaiveDate::from_ymd_opt(first.year(), first.month(), 1).unwrap_or(first);
    let next = if start.month() == 12 {
        NaiveDate::from_ymd_opt(start.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(start.year(), start.month() + 1, 1)
    }
    .unwrap_or(start);
    let mut days = Vec::with_capacity(31);
    let mut d = start;
    while d < next {
        days.push(d);
        d += Duration::days(1);
    }
    days
}

/// School days between two dates (inclusive), given the calendar.
pub fn school_days(
    from: NaiveDate,
    to: NaiveDate,
    weekend: &[u32],
    holidays: &[HolidaySpan],
) -> Vec<NaiveDate> {
    let mut out = Vec::new();
    let mut d = from;
    while d <= to {
        if day_kind(d, weekend, holidays).is_school_day() {
            out.push(d);
        }
        d += Duration::days(1);
    }
    out
}

/// Validate a weekend setting: 0–6 distinct weekday numbers in 1..=7 (at least one school day).
pub fn validate_weekend(days: &[u32]) -> Result<Vec<u32>, String> {
    let mut v: Vec<u32> = days.to_vec();
    v.sort_unstable();
    v.dedup();
    if v.iter().any(|d| !(1..=7).contains(d)) {
        return Err("weekend days must be weekday numbers 1 (Monday) to 7 (Sunday)".into());
    }
    if v.len() >= 7 {
        return Err("at least one day of the week must be a school day".into());
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn statuses_round_trip_and_present_is_the_default() {
        assert_eq!(AttendanceStatus::default(), AttendanceStatus::Present);
        for s in AttendanceStatus::ALL {
            assert_eq!(s.as_str().parse::<AttendanceStatus>().unwrap(), *s);
            assert_eq!(
                serde_json::to_value(s).unwrap(),
                serde_json::Value::String(s.as_str().into())
            );
        }
        assert!("sick".parse::<AttendanceStatus>().is_err());
        assert!(AttendanceStatus::Late.counts_as_present());
        assert!(!AttendanceStatus::Leave.counts_as_present());
        assert_eq!("leave".parse::<StaffStatus>().unwrap(), StaffStatus::Leave);
        assert!("late".parse::<StaffStatus>().is_err());
    }

    #[test]
    fn summary_percent_counts_late_as_attended_and_handles_empty() {
        use AttendanceStatus::*;
        let s = summarize([Present, Present, Late, Absent, Leave, Present]);
        assert_eq!((s.present, s.absent, s.late, s.leave), (3, 1, 1, 1));
        assert_eq!(s.total(), 6);
        // 4 of 6 = 66.666… → 66.7
        assert_eq!(s.percent(), Some(66.7));
        assert_eq!(summarize([]).percent(), None);
        assert_eq!(summarize([Present; 40]).percent(), Some(100.0));
        assert_eq!(summarize([Absent]).percent(), Some(0.0));
    }

    #[test]
    fn day_kind_prefers_holiday_over_weekend_and_defaults_to_sunday_off() {
        let holidays = vec![HolidaySpan {
            name: "Eid".into(),
            starts_on: d(2026, 5, 27),
            ends_on: d(2026, 5, 30),
        }];
        let weekend = default_weekend();
        assert_eq!(weekday_number(d(2026, 5, 31)), 7, "31 May 2026 is a Sunday");
        assert_eq!(
            day_kind(d(2026, 5, 31), &weekend, &holidays),
            DayKind::Weekend
        );
        assert_eq!(
            day_kind(d(2026, 5, 28), &weekend, &holidays),
            DayKind::Holiday { name: "Eid".into() }
        );
        // A holiday that falls on a weekend is reported as the holiday.
        let sat = vec![HolidaySpan {
            name: "Closure".into(),
            starts_on: d(2026, 5, 31),
            ends_on: d(2026, 5, 31),
        }];
        assert_eq!(
            day_kind(d(2026, 5, 31), &weekend, &sat),
            DayKind::Holiday {
                name: "Closure".into()
            }
        );
        assert_eq!(
            day_kind(d(2026, 5, 26), &weekend, &holidays),
            DayKind::School
        );
        assert!(day_kind(d(2026, 5, 26), &[6, 7], &[]).is_school_day());
        assert!(!day_kind(d(2026, 5, 30), &[6, 7], &[]).is_school_day());
    }

    #[test]
    fn month_days_and_school_days_skip_weekends_and_holidays() {
        let days = month_days(d(2026, 2, 10));
        assert_eq!(days.len(), 28);
        assert_eq!(days[0], d(2026, 2, 1));
        assert_eq!(month_days(d(2026, 12, 31)).len(), 31);
        let holidays = vec![HolidaySpan {
            name: "Labour Day".into(),
            starts_on: d(2026, 5, 1),
            ends_on: d(2026, 5, 1),
        }];
        // May 2026: 31 days, 5 Sundays (3, 10, 17, 24, 31), 1 holiday on a Friday → 25.
        let sd = school_days(d(2026, 5, 1), d(2026, 5, 31), &default_weekend(), &holidays);
        assert_eq!(sd.len(), 25);
        assert_eq!(sd[0], d(2026, 5, 2));
        assert!(!sd.contains(&d(2026, 5, 3)));
    }

    #[test]
    fn weekend_validation() {
        assert_eq!(validate_weekend(&[7, 6, 7]).unwrap(), vec![6, 7]);
        assert_eq!(validate_weekend(&[]).unwrap(), Vec::<u32>::new());
        assert!(validate_weekend(&[0]).is_err());
        assert!(validate_weekend(&[8]).is_err());
        assert!(validate_weekend(&[1, 2, 3, 4, 5, 6, 7]).is_err());
        assert_eq!(weekday_from_number(5), Some(Weekday::Fri));
        assert_eq!(weekday_from_number(0), None);
    }
}
