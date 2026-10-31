//! The school's calendar date. Every "today" in the API (dashboard, defaulters, attendance
//! limits, default dates, the late fee pass) comes from here so they all agree.
//!
//! Lite runs on the school's own PC, so the machine's local time zone is the school's. A
//! school time zone setting will replace the machine's zone here without touching the callers.

use chrono::{Local, NaiveDate};

/// Today in the school's time zone.
pub fn today() -> NaiveDate {
    Local::now().date_naive()
}
