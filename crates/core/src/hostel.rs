//! Hostel rules (Standard stage 6): the monthly charge of an allocation, bed numbers,
//! occupancy and the leave-out consent rule. Pure; the database layer applies them.

use chrono::NaiveDateTime;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

pub use crate::periods::{covers_month, month_bounds};

/// The monthly charge: the allocation's own amount, else the room's, else the hostel's.
pub fn monthly_charge(
    allocation: Option<Decimal>,
    room: Option<Decimal>,
    hostel: Decimal,
) -> Decimal {
    allocation.or(room).unwrap_or(hostel).max(Decimal::ZERO)
}

/// Beds are numbered 1..=beds within a room.
pub fn bed_is_valid(bed_no: i32, beds: i32) -> bool {
    bed_no >= 1 && bed_no <= beds
}

/// How full a hostel or room is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Occupancy {
    pub beds: i64,
    pub occupied: i64,
    pub free: i64,
}

pub fn occupancy(beds: i64, occupied: i64) -> Occupancy {
    Occupancy {
        beds,
        occupied,
        free: (beds - occupied).max(0),
    }
}

/// A leave-out needs the guardian's consent when the hostel asks for it.
pub fn leave_out_allowed(
    requires_consent: bool,
    guardian_consent: bool,
) -> Result<(), &'static str> {
    if requires_consent && !guardian_consent {
        Err("the guardian's consent is required before a resident leaves the hostel")
    } else {
        Ok(())
    }
}

/// A resident who is out past the time they were expected back and has not returned.
pub fn is_overdue(
    expected_back: Option<NaiveDateTime>,
    returned_at: Option<NaiveDateTime>,
    now: NaiveDateTime,
) -> bool {
    returned_at.is_none() && expected_back.is_some_and(|e| e < now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn dt(d: u32, h: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 4, d)
            .unwrap()
            .and_hms_opt(h, 0, 0)
            .unwrap()
    }

    #[test]
    fn the_most_specific_amount_wins() {
        let h = Decimal::from(8000);
        assert_eq!(monthly_charge(None, None, h), h);
        assert_eq!(
            monthly_charge(None, Some(Decimal::from(9000)), h),
            Decimal::from(9000)
        );
        assert_eq!(
            monthly_charge(Some(Decimal::from(5000)), Some(Decimal::from(9000)), h),
            Decimal::from(5000)
        );
        assert_eq!(
            monthly_charge(Some(Decimal::from(-1)), None, h),
            Decimal::ZERO
        );
    }

    #[test]
    fn beds_and_occupancy() {
        assert!(bed_is_valid(1, 4));
        assert!(bed_is_valid(4, 4));
        assert!(!bed_is_valid(0, 4));
        assert!(!bed_is_valid(5, 4));
        assert_eq!(
            occupancy(40, 37),
            Occupancy {
                beds: 40,
                occupied: 37,
                free: 3
            }
        );
        assert_eq!(occupancy(2, 3).free, 0);
    }

    #[test]
    fn consent_and_overdue() {
        assert!(leave_out_allowed(true, false).is_err());
        assert!(leave_out_allowed(true, true).is_ok());
        assert!(leave_out_allowed(false, false).is_ok());
        assert!(is_overdue(Some(dt(10, 18)), None, dt(10, 19)));
        assert!(!is_overdue(Some(dt(10, 18)), None, dt(10, 17)));
        assert!(!is_overdue(Some(dt(10, 18)), Some(dt(10, 17)), dt(11, 9)));
        assert!(!is_overdue(None, None, dt(11, 9)));
    }
}
