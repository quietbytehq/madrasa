//! Library rules (Standard stage 4): loan rules per member type, due dates, renewals and
//! fines. Pure; the database layer applies them.

use chrono::{Duration, NaiveDate};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// Who borrows: a student or a staff member. Each kind has its own rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberKind {
    Student,
    Staff,
}

impl MemberKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MemberKind::Student => "student",
            MemberKind::Staff => "staff",
        }
    }
    pub fn parse(s: &str) -> Option<MemberKind> {
        match s {
            "student" => Some(MemberKind::Student),
            "staff" => Some(MemberKind::Staff),
            _ => None,
        }
    }
}

/// The loan rules of one member kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoanRules {
    /// Books a member may have out at once.
    pub max_books: u32,
    /// Days from issue to due date.
    pub loan_days: u32,
    /// Fine per day late (the school's currency).
    pub fine_per_day: Decimal,
    /// Renewals allowed per loan.
    pub max_renewals: u32,
}

impl Default for LoanRules {
    fn default() -> Self {
        LoanRules {
            max_books: 2,
            loan_days: 14,
            fine_per_day: Decimal::from(5),
            max_renewals: 1,
        }
    }
}

/// Rules for both member kinds, as stored in the school's settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryRules {
    pub student: LoanRules,
    pub staff: LoanRules,
}

impl Default for LibraryRules {
    fn default() -> Self {
        LibraryRules {
            student: LoanRules::default(),
            staff: LoanRules {
                max_books: 5,
                loan_days: 30,
                fine_per_day: Decimal::ZERO,
                max_renewals: 2,
            },
        }
    }
}

impl LibraryRules {
    pub fn for_kind(&self, kind: MemberKind) -> &LoanRules {
        match kind {
            MemberKind::Student => &self.student,
            MemberKind::Staff => &self.staff,
        }
    }

    pub fn check(&self) -> Result<(), String> {
        for r in [&self.student, &self.staff] {
            if r.loan_days == 0 {
                return Err("The loan period must be at least one day.".into());
            }
            if r.max_books == 0 {
                return Err("Members must be allowed at least one book.".into());
            }
            if r.fine_per_day < Decimal::ZERO {
                return Err("The fine per day cannot be negative.".into());
            }
        }
        Ok(())
    }
}

pub fn due_date(issued_on: NaiveDate, rules: &LoanRules) -> NaiveDate {
    issued_on + Duration::days(i64::from(rules.loan_days))
}

/// Whole days late (0 when returned on or before the due date).
pub fn days_late(due_on: NaiveDate, returned_on: NaiveDate) -> u32 {
    (returned_on - due_on).num_days().max(0) as u32
}

/// The fine for a return: days late × fine per day, two decimals.
pub fn fine(due_on: NaiveDate, returned_on: NaiveDate, rules: &LoanRules) -> Decimal {
    (Decimal::from(days_late(due_on, returned_on)) * rules.fine_per_day).round_dp(2)
}

/// Whether a member may take another book.
pub fn can_issue(active_loans: u32, overdue_loans: u32, rules: &LoanRules) -> Result<(), String> {
    if overdue_loans > 0 {
        return Err("This member has an overdue book; return it first.".into());
    }
    if active_loans >= rules.max_books {
        return Err(format!(
            "This member already has {} books out (the limit is {}).",
            active_loans, rules.max_books
        ));
    }
    Ok(())
}

/// Whether a loan may be renewed today; returns the new due date.
pub fn renew(
    due_on: NaiveDate,
    renewals: u32,
    today: NaiveDate,
    rules: &LoanRules,
) -> Result<NaiveDate, String> {
    if renewals >= rules.max_renewals {
        return Err(format!(
            "This loan was renewed {} times already (the limit is {}).",
            renewals, rules.max_renewals
        ));
    }
    if today > due_on {
        return Err("An overdue book cannot be renewed; return it and pay the fine.".into());
    }
    Ok(due_date(today, rules))
}

/// What a waiver leaves to pay: never below zero, never more than the fine.
pub fn waive(fine: Decimal, waived: Decimal) -> Result<Decimal, String> {
    if waived < Decimal::ZERO {
        return Err("The waived amount cannot be negative.".into());
    }
    if waived > fine {
        return Err("The waiver cannot exceed the fine.".into());
    }
    Ok((fine - waived).round_dp(2))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn due_dates_and_fines() {
        let r = LoanRules::default();
        assert_eq!(due_date(d(2026, 4, 1), &r), d(2026, 4, 15));
        assert_eq!(days_late(d(2026, 4, 15), d(2026, 4, 10)), 0);
        assert_eq!(days_late(d(2026, 4, 15), d(2026, 4, 18)), 3);
        assert_eq!(fine(d(2026, 4, 15), d(2026, 4, 18), &r), Decimal::from(15));
        assert_eq!(fine(d(2026, 4, 15), d(2026, 4, 15), &r), Decimal::ZERO);
        let staff = LibraryRules::default().staff;
        assert_eq!(fine(d(2026, 4, 15), d(2026, 5, 18), &staff), Decimal::ZERO);
    }

    #[test]
    fn issue_limits_and_overdues() {
        let r = LoanRules::default();
        assert!(can_issue(0, 0, &r).is_ok());
        assert!(can_issue(1, 0, &r).is_ok());
        assert!(can_issue(2, 0, &r).is_err());
        assert!(can_issue(0, 1, &r).is_err());
    }

    #[test]
    fn renewals_are_limited_and_not_overdue() {
        let r = LoanRules::default();
        assert_eq!(
            renew(d(2026, 4, 15), 0, d(2026, 4, 14), &r).unwrap(),
            d(2026, 4, 28)
        );
        assert!(renew(d(2026, 4, 15), 1, d(2026, 4, 14), &r).is_err());
        assert!(renew(d(2026, 4, 15), 0, d(2026, 4, 16), &r).is_err());
    }

    #[test]
    fn waivers_stay_within_the_fine() {
        assert_eq!(
            waive(Decimal::from(15), Decimal::from(5)).unwrap(),
            Decimal::from(10)
        );
        assert!(waive(Decimal::from(15), Decimal::from(20)).is_err());
        assert!(waive(Decimal::from(15), Decimal::from(-1)).is_err());
        let mut rules = LibraryRules::default();
        assert!(rules.check().is_ok());
        rules.student.loan_days = 0;
        assert!(rules.check().is_err());
    }
}
