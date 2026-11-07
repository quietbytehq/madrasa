//! Timetable engine (Standard alpha, stage 1), ported from quietbytehq/timewizer
//! (Apache-2.0, same owner): a school's days, shifts (periods and breaks), master data,
//! lessons, time-off grids and rules compile into a compact model that the solver fills by
//! difficulty order with ejection chains, repair and relaxation, then verifies. The engine
//! works on its own ids (strings); `crates/db`/`crates/api` map Madrasa's sections, subjects,
//! staff and rooms onto them and store the result.
//!
//! Kept as close to the original as practical so fixes can be carried across; only the
//! module paths changed.

// The engine is vendored from quietbytehq/timewizer and kept close to upstream so fixes can
// be carried over; its style differs from the rest of the crate in these respects.
#![allow(
    clippy::unnecessary_map_or,
    clippy::single_match,
    clippy::len_zero,
    clippy::type_complexity,
    clippy::needless_range_loop,
    clippy::manual_div_ceil,
    clippy::redundant_locals,
    clippy::unnecessary_sort_by
)]

pub mod check;
pub mod lessons;
pub mod links;
pub mod master;
pub mod model;
pub mod settings;
pub mod solver;
pub mod substitutions;
pub mod importer;

pub use lessons::{Lesson, LessonPart};
pub use master::{Class, Division, Group, MasterData, Room, Subject, Teacher, TimeOffCell};
pub use model::{Break, Day, PeriodTime, SchoolSetup, Shift};
pub use settings::{Settings, Weights};
pub use solver::{solve, Options, PlacedCard, SolveReport, Timetable};
