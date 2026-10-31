//! The school's calendar date. Every "today" in the API (dashboard, defaulters, attendance
//! limits, default dates, the late fee pass) comes from here so they all agree.
//!
//! The zone is the school's time zone setting (see `AppState::reload_profile`), set once at
//! start and again when the profile changes. Before setup, or if the stored name is invalid,
//! the machine's local zone is used. One process serves one school in Lite; Standard will
//! carry the zone per request.

use chrono::{Local, NaiveDate, Utc};
use chrono_tz::Tz;
use std::sync::RwLock;

static ZONE: RwLock<Option<Tz>> = RwLock::new(None);

/// Use this zone from now on (`None`: the machine's local zone).
pub fn set_time_zone(tz: Option<Tz>) {
    *ZONE.write().unwrap_or_else(|e| e.into_inner()) = tz;
}

/// The zone in force, if a school zone is set.
pub fn time_zone() -> Option<Tz> {
    *ZONE.read().unwrap_or_else(|e| e.into_inner())
}

/// Today in the school's time zone.
pub fn today() -> NaiveDate {
    match time_zone() {
        Some(tz) => Utc::now().with_timezone(&tz).date_naive(),
        None => Local::now().date_naive(),
    }
}

/// Today in a given zone (what [`today`] returns once that zone is set).
pub fn today_in(tz: Tz) -> NaiveDate {
    Utc::now().with_timezone(&tz).date_naive()
}
