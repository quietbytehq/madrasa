//! School-wide rules and preferences (milestone 8).
use crate::timetable::master::Teacher;
use serde::{Deserialize, Serialize};

/// How much each "nice to have" matters, 0 (ignore) to 10 (very important).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Weights {
    /// Free periods in the middle of a teacher's day.
    pub teacher_gaps: u32,
    /// Free periods in the middle of a class's day, and late starts.
    pub class_gaps: u32,
    /// Sessions of one lesson spread evenly across the week.
    pub spread: u32,
    /// Teachers' and classes' "avoid if possible" cells.
    pub avoid: u32,
    /// Similar number of periods on each day for a class.
    pub balance: u32,
    /// Same room for every session of a lesson.
    pub room_consistency: u32,
}

impl Default for Weights {
    fn default() -> Self {
        Weights { teacher_gaps: 6, class_gaps: 8, spread: 5, avoid: 7, balance: 3, room_consistency: 3 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Default for every teacher who has no limit of their own.
    pub default_max_per_day: Option<u32>,
    pub default_max_per_week: Option<u32>,
    /// Most sessions of one subject a class (or group) may have on one day.
    pub max_same_subject_per_day: u32,
    pub weights: Weights,
    /// If a full timetable is impossible, bend a few rules and list which.
    pub allow_relaxation: bool,
    /// Seconds spent improving quality after the lessons are placed.
    pub improve_seconds: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            default_max_per_day: None,
            default_max_per_week: None,
            max_same_subject_per_day: 1,
            weights: Weights::default(),
            allow_relaxation: false,
            improve_seconds: 20,
        }
    }
}

impl Settings {
    /// A teacher's own limit if they have one, otherwise the school default.
    pub fn max_per_day(&self, t: &Teacher) -> Option<u32> {
        t.max_lessons_per_day.or(self.default_max_per_day)
    }
    pub fn max_per_week(&self, t: &Teacher) -> Option<u32> {
        t.max_periods_per_week.or(self.default_max_per_week)
    }
}
