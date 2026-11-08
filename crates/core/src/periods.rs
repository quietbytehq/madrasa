//! Calendar helpers shared by the modules that charge per month (transport, hostel).

use chrono::{Datelike, NaiveDate};

/// The first and last day of the month that holds `day`.
pub fn month_bounds(day: NaiveDate) -> (NaiveDate, NaiveDate) {
    let first = NaiveDate::from_ymd_opt(day.year(), day.month(), 1).unwrap_or(day);
    let next = if day.month() == 12 {
        NaiveDate::from_ymd_opt(day.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(day.year(), day.month() + 1, 1)
    };
    let last = next.and_then(|n| n.pred_opt()).unwrap_or(first);
    (first, last)
}

/// A dated span is charged for a month when any day of the month falls between its from
/// and to dates (an open span has no to date).
pub fn covers_month(from: NaiveDate, to: Option<NaiveDate>, month: NaiveDate) -> bool {
    let (first, last) = month_bounds(month);
    from <= last && to.is_none_or(|t| t >= first)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn month_bounds_handle_year_end() {
        assert_eq!(
            month_bounds(d(2026, 12, 15)),
            (d(2026, 12, 1), d(2026, 12, 31))
        );
        assert_eq!(month_bounds(d(2026, 2, 3)), (d(2026, 2, 1), d(2026, 2, 28)));
    }

    #[test]
    fn a_span_is_charged_for_every_month_it_touches() {
        assert!(covers_month(d(2026, 4, 10), None, d(2026, 4, 1)));
        assert!(covers_month(d(2026, 4, 10), None, d(2026, 9, 1)));
        assert!(!covers_month(d(2026, 5, 1), None, d(2026, 4, 1)));
        assert!(covers_month(
            d(2026, 4, 10),
            Some(d(2026, 6, 1)),
            d(2026, 6, 1)
        ));
        assert!(!covers_month(
            d(2026, 4, 10),
            Some(d(2026, 5, 31)),
            d(2026, 6, 1)
        ));
    }
}
