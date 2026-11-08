//! Transport rules (Standard stage 5): which assignment is charged in a billing month, the
//! monthly charge of an assignment and the capacity warning of a route. Pure; the database
//! layer applies them.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

pub use crate::periods::{covers_month, month_bounds};

/// The monthly charge: the assignment's own amount, else the stop's, else the route's.
pub fn monthly_charge(
    assignment: Option<Decimal>,
    stop: Option<Decimal>,
    route: Decimal,
) -> Decimal {
    assignment.or(stop).unwrap_or(route).max(Decimal::ZERO)
}

/// How full a route is against its vehicle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "by")]
pub enum CapacityWarning {
    /// Every seat is taken.
    Full,
    /// More students than seats, by this many.
    Over(i64),
}

/// `None` when the route has no vehicle, a vehicle without a capacity, or seats to spare.
pub fn capacity_warning(capacity: Option<i32>, assigned: i64) -> Option<CapacityWarning> {
    let cap = i64::from(capacity?);
    if cap <= 0 {
        return None;
    }
    match assigned.cmp(&cap) {
        std::cmp::Ordering::Less => None,
        std::cmp::Ordering::Equal => Some(CapacityWarning::Full),
        std::cmp::Ordering::Greater => Some(CapacityWarning::Over(assigned - cap)),
    }
}

/// Seats still free, when the capacity is known.
pub fn seats_left(capacity: Option<i32>, assigned: i64) -> Option<i64> {
    capacity.map(|c| i64::from(c) - assigned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_most_specific_amount_wins() {
        let r = Decimal::from(1500);
        assert_eq!(monthly_charge(None, None, r), r);
        assert_eq!(
            monthly_charge(None, Some(Decimal::from(1200)), r),
            Decimal::from(1200)
        );
        assert_eq!(
            monthly_charge(Some(Decimal::from(900)), Some(Decimal::from(1200)), r),
            Decimal::from(900)
        );
        assert_eq!(
            monthly_charge(Some(Decimal::from(-5)), None, r),
            Decimal::ZERO
        );
    }

    #[test]
    fn capacity_warnings() {
        assert_eq!(capacity_warning(None, 50), None);
        assert_eq!(capacity_warning(Some(0), 3), None);
        assert_eq!(capacity_warning(Some(40), 39), None);
        assert_eq!(capacity_warning(Some(40), 40), Some(CapacityWarning::Full));
        assert_eq!(
            capacity_warning(Some(40), 43),
            Some(CapacityWarning::Over(3))
        );
        assert_eq!(seats_left(Some(40), 43), Some(-3));
        assert_eq!(seats_left(None, 1), None);
    }
}
