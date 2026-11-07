//! Leave rules (Standard stage 2): working days of a request, half days, balances with
//! carry-over, request states and overlap checks. Pure; the database layer applies them.

use std::str::FromStr;

use chrono::{Datelike, NaiveDate};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// The life of a request: pending → approved | rejected | withdrawn; approved → withdrawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestStatus {
    Pending,
    Approved,
    Rejected,
    Withdrawn,
}

impl RequestStatus {
    pub const ALL: &'static [RequestStatus] = &[
        RequestStatus::Pending,
        RequestStatus::Approved,
        RequestStatus::Rejected,
        RequestStatus::Withdrawn,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            RequestStatus::Pending => "pending",
            RequestStatus::Approved => "approved",
            RequestStatus::Rejected => "rejected",
            RequestStatus::Withdrawn => "withdrawn",
        }
    }

    /// Whether a request may move from `self` to `to`.
    pub fn can_become(self, to: RequestStatus) -> bool {
        matches!(
            (self, to),
            (RequestStatus::Pending, RequestStatus::Approved)
                | (RequestStatus::Pending, RequestStatus::Rejected)
                | (RequestStatus::Pending, RequestStatus::Withdrawn)
                | (RequestStatus::Approved, RequestStatus::Withdrawn)
        )
    }

    /// Pending and approved requests take days and block overlapping requests.
    pub fn is_live(self) -> bool {
        matches!(self, RequestStatus::Pending | RequestStatus::Approved)
    }
}

impl FromStr for RequestStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        RequestStatus::ALL
            .iter()
            .copied()
            .find(|x| x.as_str() == s)
            .ok_or_else(|| format!("unknown request status: {s}"))
    }
}

/// A half-day request: the morning or the afternoon of one day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HalfDay {
    Am,
    Pm,
}

impl HalfDay {
    pub fn as_str(self) -> &'static str {
        match self {
            HalfDay::Am => "am",
            HalfDay::Pm => "pm",
        }
    }
    pub fn parse(s: &str) -> Option<HalfDay> {
        match s {
            "am" => Some(HalfDay::Am),
            "pm" => Some(HalfDay::Pm),
            _ => None,
        }
    }
}

/// The school calendar as far as leave is concerned.
#[derive(Debug, Clone, Default)]
pub struct Calendar {
    /// ISO weekday numbers (1 = Monday … 7 = Sunday) that are off.
    pub weekend: Vec<u32>,
    /// Holiday ranges (inclusive).
    pub holidays: Vec<(NaiveDate, NaiveDate)>,
}

impl Calendar {
    pub fn is_working(&self, d: NaiveDate) -> bool {
        !self.weekend.contains(&d.weekday().number_from_monday())
            && !self.holidays.iter().any(|(a, b)| *a <= d && d <= *b)
    }
}

/// Working days from `from` to `to` inclusive (empty when `to` is before `from`).
pub fn working_days(from: NaiveDate, to: NaiveDate, cal: &Calendar) -> Vec<NaiveDate> {
    let mut out = Vec::new();
    let mut d = from;
    while d <= to {
        if cal.is_working(d) {
            out.push(d);
        }
        d = match d.succ_opt() {
            Some(n) => n,
            None => break,
        };
    }
    out
}

/// Days a request takes: one per working day, or half a day for a single half-day request.
pub fn leave_days(working: usize, half: Option<HalfDay>) -> Decimal {
    match half {
        Some(_) if working == 1 => Decimal::new(5, 1),
        _ => Decimal::from(working as u64),
    }
}

/// Problems with a request's dates, in plain language.
pub fn check_dates(
    from: NaiveDate,
    to: NaiveDate,
    half: Option<HalfDay>,
    working: usize,
) -> Result<(), String> {
    if to < from {
        return Err("The leave ends before it starts.".into());
    }
    if working == 0 {
        return Err("The dates fall on weekends or holidays; no leave is needed.".into());
    }
    if half.is_some() && from != to {
        return Err("A half day is a single date.".into());
    }
    Ok(())
}

/// Inclusive date ranges that share at least one day.
pub fn overlaps(a: (NaiveDate, NaiveDate), b: (NaiveDate, NaiveDate)) -> bool {
    a.0 <= b.1 && b.0 <= a.1
}

/// One staff member's balance of one leave type for a year.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Balance {
    pub entitlement: Decimal,
    pub carried_over: Decimal,
    pub adjustment: Decimal,
    /// Approved days this year.
    pub used: Decimal,
    /// Days of requests still awaiting a decision.
    pub pending: Decimal,
}

impl Balance {
    /// Days granted for the year.
    pub fn granted(&self) -> Decimal {
        self.entitlement + self.carried_over + self.adjustment
    }
    /// Days left once approved leave is taken off.
    pub fn available(&self) -> Decimal {
        self.granted() - self.used
    }
    /// Days left if every pending request were approved too.
    pub fn remaining(&self) -> Decimal {
        self.available() - self.pending
    }
}

/// Days carried into the next year: what was left, never negative, capped by the type.
pub fn carry_over(available: Decimal, max_carry: Decimal) -> Decimal {
    available
        .max(Decimal::ZERO)
        .min(max_carry.max(Decimal::ZERO))
}

/// Refuse a request that would take more than is left (counting pending requests too).
pub fn check_balance(days: Decimal, balance: &Balance) -> Result<(), String> {
    if days > balance.remaining() {
        return Err(format!(
            "Only {} days are left for this leave type (including requests awaiting a decision).",
            balance.remaining().normalize()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn cal() -> Calendar {
        Calendar {
            weekend: vec![6, 7],
            holidays: vec![(d(2026, 4, 8), d(2026, 4, 8))],
        }
    }

    #[test]
    fn working_days_skip_weekends_and_holidays() {
        // Mon 6 Apr – Mon 13 Apr 2026: Wed 8 is a holiday, 11–12 the weekend.
        let days = working_days(d(2026, 4, 6), d(2026, 4, 13), &cal());
        assert_eq!(
            days,
            vec![
                d(2026, 4, 6),
                d(2026, 4, 7),
                d(2026, 4, 9),
                d(2026, 4, 10),
                d(2026, 4, 13)
            ]
        );
        assert!(working_days(d(2026, 4, 11), d(2026, 4, 12), &cal()).is_empty());
        assert!(working_days(d(2026, 4, 10), d(2026, 4, 9), &cal()).is_empty());
    }

    #[test]
    fn half_days_count_half_only_for_one_day() {
        assert_eq!(leave_days(1, Some(HalfDay::Am)), Decimal::new(5, 1));
        assert_eq!(leave_days(3, None), Decimal::from(3));
        assert_eq!(leave_days(3, Some(HalfDay::Pm)), Decimal::from(3));
        assert!(check_dates(d(2026, 4, 6), d(2026, 4, 7), Some(HalfDay::Am), 2).is_err());
        assert!(check_dates(d(2026, 4, 7), d(2026, 4, 6), None, 0).is_err());
        assert!(check_dates(d(2026, 4, 11), d(2026, 4, 12), None, 0).is_err());
        assert!(check_dates(d(2026, 4, 6), d(2026, 4, 6), Some(HalfDay::Pm), 1).is_ok());
    }

    #[test]
    fn balances_carry_over_within_the_cap() {
        let b = Balance {
            entitlement: Decimal::from(20),
            carried_over: Decimal::from(3),
            adjustment: Decimal::from(-1),
            used: Decimal::from(10),
            pending: Decimal::new(25, 1),
        };
        assert_eq!(b.granted(), Decimal::from(22));
        assert_eq!(b.available(), Decimal::from(12));
        assert_eq!(b.remaining(), Decimal::new(95, 1));
        assert_eq!(
            carry_over(b.available(), Decimal::from(5)),
            Decimal::from(5)
        );
        assert_eq!(
            carry_over(Decimal::from(2), Decimal::from(5)),
            Decimal::from(2)
        );
        assert_eq!(
            carry_over(Decimal::from(-3), Decimal::from(5)),
            Decimal::ZERO
        );
        assert!(check_balance(Decimal::from(10), &b).is_err());
        assert!(check_balance(Decimal::new(95, 1), &b).is_ok());
    }

    #[test]
    fn states_and_overlaps() {
        use RequestStatus::*;
        assert!(Pending.can_become(Approved));
        assert!(Pending.can_become(Rejected));
        assert!(Approved.can_become(Withdrawn));
        assert!(!Approved.can_become(Rejected));
        assert!(!Rejected.can_become(Approved));
        assert!(!Withdrawn.can_become(Pending));
        assert!(Pending.is_live() && Approved.is_live() && !Rejected.is_live());
        assert_eq!("approved".parse::<RequestStatus>().unwrap(), Approved);
        assert!(overlaps(
            (d(2026, 4, 6), d(2026, 4, 8)),
            (d(2026, 4, 8), d(2026, 4, 10))
        ));
        assert!(!overlaps(
            (d(2026, 4, 6), d(2026, 4, 8)),
            (d(2026, 4, 9), d(2026, 4, 10))
        ));
    }
}
