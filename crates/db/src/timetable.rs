//! Timetable (Standard stage 1): shifts, rooms, lessons, per-year rules, generated
//! timetables with their cards, and substitutions (absences and covers). The scheduling
//! engine lives in `madrasa_core::timetable`; [`setup_for_year`] maps Madrasa's entities
//! (sections, staff, subjects, rooms) onto its `SchoolSetup`, with UUID strings as ids.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, NaiveDate, Utc};
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use madrasa_core::timetable as engine;
use madrasa_core::timetable::lessons::Lesson as EngineLesson;
use madrasa_core::timetable::substitutions as subs;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::audit::{self, snapshot, Action};
use crate::models::{AcademicYear, Section, Staff, Subject};
use crate::{academics, map_unique, staff as staff_repo, Actor, DbError, DbResult};

// ---- JSON shapes -----------------------------------------------------------------------------

/// One weekday of a shift (seven per shift, Monday first).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShiftDay {
    pub working: bool,
    /// How many of the shift's periods run on this day (a short Friday has fewer).
    pub periods: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PeriodTime {
    /// "HH:MM"
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Break {
    pub after_period: u32,
    pub name: String,
    /// A double period may not straddle this break.
    #[serde(default = "yes")]
    pub cannot_cross: bool,
}

fn yes() -> bool {
    true
}

/// A cell of a time-off grid: 1 = avoid if possible, 2 = never.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimeOff {
    pub shift_id: Uuid,
    /// 0 = Monday.
    pub day: u8,
    /// 1-based.
    pub period: u8,
    pub state: u8,
}

/// Limits of one teacher (`timetable_rules.limits` for kind `teacher`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TeacherLimits {
    #[serde(default)]
    pub max_per_day: Option<u32>,
    #[serde(default)]
    pub max_per_week: Option<u32>,
    #[serde(default)]
    pub max_consecutive: Option<u32>,
    #[serde(default)]
    pub max_days_per_week: Option<u32>,
}

/// School-wide solver settings (`timetable_rules.limits` for kind `school`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchoolRules {
    #[serde(default)]
    pub default_max_per_day: Option<u32>,
    #[serde(default)]
    pub default_max_per_week: Option<u32>,
    /// How often one subject may be taught to a section on one day.
    pub max_same_subject_per_day: u32,
    /// Let the generator bend soft limits rather than leave lessons unplaced.
    pub allow_relaxation: bool,
    /// Seconds spent improving quality after every lesson is placed.
    pub improve_seconds: u32,
}

impl Default for SchoolRules {
    fn default() -> Self {
        let s = engine::Settings::default();
        SchoolRules {
            default_max_per_day: s.default_max_per_day,
            default_max_per_week: s.default_max_per_week,
            max_same_subject_per_day: s.max_same_subject_per_day,
            allow_relaxation: s.allow_relaxation,
            improve_seconds: s.improve_seconds,
        }
    }
}

/// A period an absence covers (empty list = whole day).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PeriodRef {
    pub shift_id: Uuid,
    pub period: u8,
}

fn from_json<T: serde::de::DeserializeOwned + Default>(v: &Value) -> T {
    serde_json::from_value(v.clone()).unwrap_or_default()
}

// ---- shifts ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Shift {
    pub id: Uuid,
    pub school_id: Uuid,
    pub academic_year_id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub days: Value,
    pub period_times: Value,
    pub breaks: Value,
    pub position: i32,
    pub hlc: String,
}

impl Shift {
    pub fn days(&self) -> Vec<ShiftDay> {
        from_json(&self.days)
    }
    pub fn period_times(&self) -> Vec<PeriodTime> {
        from_json(&self.period_times)
    }
    pub fn breaks(&self) -> Vec<Break> {
        from_json(&self.breaks)
    }
}

#[derive(Debug, Clone)]
pub struct ShiftInput {
    pub name: String,
    pub enabled: bool,
    pub days: Vec<ShiftDay>,
    pub period_times: Vec<PeriodTime>,
    pub breaks: Vec<Break>,
    pub position: i32,
}

/// The default morning shift: Monday–Thursday 7 periods from 08:00, Friday 5, a break after
/// the third period.
pub fn default_shift_input(name: &str) -> ShiftInput {
    let mut period_times = Vec::new();
    let mut t = 8 * 60;
    for p in 1..=7u32 {
        period_times.push(PeriodTime {
            start: format!("{:02}:{:02}", t / 60, t % 60),
            end: format!("{:02}:{:02}", (t + 40) / 60, (t + 40) % 60),
        });
        t += 40;
        if p == 3 {
            t += 20;
        }
    }
    ShiftInput {
        name: name.into(),
        enabled: true,
        days: (0..7)
            .map(|d| ShiftDay {
                working: d < 5,
                periods: match d {
                    0..=3 => 7,
                    4 => 5,
                    _ => 0,
                },
            })
            .collect(),
        period_times,
        breaks: vec![Break {
            after_period: 3,
            name: "Break".into(),
            cannot_cross: true,
        }],
        position: 0,
    }
}

fn check_shift(input: &ShiftInput) -> DbResult<()> {
    if input.name.trim().is_empty() {
        return Err(DbError::Conflict("the shift needs a name".into()));
    }
    if input.days.len() != 7 {
        return Err(DbError::Conflict("a shift lists all seven days".into()));
    }
    if input.period_times.is_empty() {
        return Err(DbError::Conflict("a shift needs at least one period".into()));
    }
    for p in &input.period_times {
        let ok = |s: &str| engine::model::parse_time(s).is_some();
        if !ok(&p.start) || !ok(&p.end) {
            return Err(DbError::Conflict(
                "period times are written as HH:MM, for example 08:40".into(),
            ));
        }
    }
    let n = input.period_times.len() as u32;
    if input
        .days
        .iter()
        .any(|d| d.working && (d.periods == 0 || d.periods > n))
    {
        return Err(DbError::Conflict(format!(
            "a working day has between 1 and {n} periods"
        )));
    }
    if input.breaks.iter().any(|b| b.after_period == 0 || b.after_period >= n) {
        return Err(DbError::Conflict(
            "a break comes after a period that exists, and not after the last one".into(),
        ));
    }
    Ok(())
}

const SHIFT_COLS: &str =
    "id, school_id, academic_year_id, name, enabled, days, period_times, breaks, position, hlc";

pub async fn list_shifts(conn: &mut PgConnection, year_id: Uuid) -> DbResult<Vec<Shift>> {
    Ok(sqlx::query_as::<_, Shift>(sqlx::AssertSqlSafe(format!(
        "SELECT {SHIFT_COLS} FROM timetable_shifts WHERE academic_year_id = $1 AND deleted_at IS NULL
         ORDER BY position, name"
    )))
    .bind(year_id)
    .fetch_all(conn)
    .await?)
}

pub async fn get_shift(conn: &mut PgConnection, id: Uuid) -> DbResult<Shift> {
    sqlx::query_as::<_, Shift>(sqlx::AssertSqlSafe(format!(
        "SELECT {SHIFT_COLS} FROM timetable_shifts WHERE id = $1 AND deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("shift".into()))
}

pub async fn create_shift(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    year_id: Uuid,
    input: &ShiftInput,
) -> DbResult<Shift> {
    check_shift(input)?;
    let row = sqlx::query_as::<_, Shift>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO timetable_shifts (id, school_id, academic_year_id, name, enabled, days, period_times, breaks, position, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) RETURNING {SHIFT_COLS}"
    )))
    .bind(new_id())
    .bind(school_id)
    .bind(year_id)
    .bind(input.name.trim())
    .bind(input.enabled)
    .bind(serde_json::to_value(&input.days).unwrap_or(Value::Null))
    .bind(serde_json::to_value(&input.period_times).unwrap_or(Value::Null))
    .bind(serde_json::to_value(&input.breaks).unwrap_or(Value::Null))
    .bind(input.position)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a shift with this name"))?;
    audit::record(conn, hlc, actor, Action::Create, "timetable_shift", Some(row.id), None, Some(snapshot(&row))).await?;
    Ok(row)
}

pub async fn update_shift(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    input: &ShiftInput,
) -> DbResult<Shift> {
    check_shift(input)?;
    let old = get_shift(&mut *conn, id).await?;
    let row = sqlx::query_as::<_, Shift>(sqlx::AssertSqlSafe(format!(
        "UPDATE timetable_shifts SET name = $2, enabled = $3, days = $4, period_times = $5, breaks = $6, position = $7,
                updated_at = now(), updated_by = $8, hlc = $9
         WHERE id = $1 AND deleted_at IS NULL RETURNING {SHIFT_COLS}"
    )))
    .bind(id)
    .bind(input.name.trim())
    .bind(input.enabled)
    .bind(serde_json::to_value(&input.days).unwrap_or(Value::Null))
    .bind(serde_json::to_value(&input.period_times).unwrap_or(Value::Null))
    .bind(serde_json::to_value(&input.breaks).unwrap_or(Value::Null))
    .bind(input.position)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a shift with this name"))?;
    audit::record(conn, hlc, actor, Action::Update, "timetable_shift", Some(id), Some(snapshot(&old)), Some(snapshot(&row))).await?;
    Ok(row)
}

pub async fn delete_shift(conn: &mut PgConnection, hlc: &Hlc, actor: Actor, id: Uuid) -> DbResult<()> {
    let old = get_shift(&mut *conn, id).await?;
    soft_delete(&mut *conn, hlc, actor, "timetable_shifts", id).await?;
    audit::record(conn, hlc, actor, Action::Delete, "timetable_shift", Some(id), Some(snapshot(&old)), None).await?;
    Ok(())
}

async fn soft_delete(conn: &mut PgConnection, hlc: &Hlc, actor: Actor, table: &str, id: Uuid) -> DbResult<()> {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE {table} SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1 AND deleted_at IS NULL"
    )))
    .bind(id)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .execute(conn)
    .await?;
    Ok(())
}

// ---- rooms ----------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Room {
    pub id: Uuid,
    pub school_id: Uuid,
    pub name: String,
    pub short_name: String,
    pub building: String,
    pub capacity: Option<i32>,
    pub hlc: String,
}

#[derive(Debug, Clone)]
pub struct RoomInput {
    pub name: String,
    pub short_name: String,
    pub building: String,
    pub capacity: Option<i32>,
}

fn check_room(input: &RoomInput) -> DbResult<()> {
    if input.name.trim().is_empty() {
        return Err(DbError::Conflict("the room needs a name".into()));
    }
    if input.capacity.is_some_and(|c| c < 0) {
        return Err(DbError::Conflict("capacity cannot be negative".into()));
    }
    Ok(())
}

fn short_or(name: &str, short: &str) -> String {
    let s = short.trim();
    if s.is_empty() {
        name.trim().chars().take(8).collect()
    } else {
        s.to_string()
    }
}

const ROOM_COLS: &str = "id, school_id, name, short_name, building, capacity, hlc";

pub async fn list_rooms(conn: &mut PgConnection, school_id: Uuid) -> DbResult<Vec<Room>> {
    Ok(sqlx::query_as::<_, Room>(sqlx::AssertSqlSafe(format!(
        "SELECT {ROOM_COLS} FROM rooms WHERE school_id = $1 AND deleted_at IS NULL ORDER BY building, name"
    )))
    .bind(school_id)
    .fetch_all(conn)
    .await?)
}

pub async fn get_room(conn: &mut PgConnection, id: Uuid) -> DbResult<Room> {
    sqlx::query_as::<_, Room>(sqlx::AssertSqlSafe(format!(
        "SELECT {ROOM_COLS} FROM rooms WHERE id = $1 AND deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("room".into()))
}

pub async fn create_room(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    input: &RoomInput,
) -> DbResult<Room> {
    check_room(input)?;
    let row = sqlx::query_as::<_, Room>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO rooms (id, school_id, name, short_name, building, capacity, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING {ROOM_COLS}"
    )))
    .bind(new_id())
    .bind(school_id)
    .bind(input.name.trim())
    .bind(short_or(&input.name, &input.short_name))
    .bind(input.building.trim())
    .bind(input.capacity)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a room with this name"))?;
    audit::record(conn, hlc, actor, Action::Create, "room", Some(row.id), None, Some(snapshot(&row))).await?;
    Ok(row)
}

pub async fn update_room(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    input: &RoomInput,
) -> DbResult<Room> {
    check_room(input)?;
    let old = get_room(&mut *conn, id).await?;
    let row = sqlx::query_as::<_, Room>(sqlx::AssertSqlSafe(format!(
        "UPDATE rooms SET name = $2, short_name = $3, building = $4, capacity = $5, updated_at = now(), updated_by = $6, hlc = $7
         WHERE id = $1 AND deleted_at IS NULL RETURNING {ROOM_COLS}"
    )))
    .bind(id)
    .bind(input.name.trim())
    .bind(short_or(&input.name, &input.short_name))
    .bind(input.building.trim())
    .bind(input.capacity)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a room with this name"))?;
    audit::record(conn, hlc, actor, Action::Update, "room", Some(id), Some(snapshot(&old)), Some(snapshot(&row))).await?;
    Ok(row)
}

pub async fn delete_room(conn: &mut PgConnection, hlc: &Hlc, actor: Actor, id: Uuid) -> DbResult<()> {
    let old = get_room(&mut *conn, id).await?;
    soft_delete(&mut *conn, hlc, actor, "rooms", id).await?;
    audit::record(conn, hlc, actor, Action::Delete, "room", Some(id), Some(snapshot(&old)), None).await?;
    Ok(())
}

// ---- lessons --------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Lesson {
    pub id: Uuid,
    pub school_id: Uuid,
    pub academic_year_id: Uuid,
    pub subject_id: Uuid,
    pub teacher_ids: Vec<Uuid>,
    pub section_ids: Vec<Uuid>,
    pub room_ids: Vec<Uuid>,
    pub periods_per_card: i32,
    pub cards_per_week: i32,
    pub hlc: String,
}

#[derive(Debug, Clone)]
pub struct LessonInput {
    pub subject_id: Uuid,
    pub teacher_ids: Vec<Uuid>,
    pub section_ids: Vec<Uuid>,
    pub room_ids: Vec<Uuid>,
    pub periods_per_card: i32,
    pub cards_per_week: i32,
}

fn check_lesson(input: &LessonInput) -> DbResult<()> {
    if input.section_ids.is_empty() {
        return Err(DbError::Conflict("a lesson needs at least one section".into()));
    }
    if !(1..=3).contains(&input.periods_per_card) {
        return Err(DbError::Conflict("a session is 1, 2 or 3 periods long".into()));
    }
    if !(1..=40).contains(&input.cards_per_week) {
        return Err(DbError::Conflict("a lesson is held 1 to 40 times a week".into()));
    }
    Ok(())
}

fn dedup(v: &[Uuid]) -> Vec<Uuid> {
    let mut seen = HashSet::new();
    v.iter().copied().filter(|x| seen.insert(*x)).collect()
}

const LESSON_COLS: &str = "id, school_id, academic_year_id, subject_id, teacher_ids, section_ids, room_ids, periods_per_card, cards_per_week, hlc";

pub async fn list_lessons(conn: &mut PgConnection, year_id: Uuid) -> DbResult<Vec<Lesson>> {
    Ok(sqlx::query_as::<_, Lesson>(sqlx::AssertSqlSafe(format!(
        "SELECT {LESSON_COLS} FROM timetable_lessons WHERE academic_year_id = $1 AND deleted_at IS NULL ORDER BY created_at"
    )))
    .bind(year_id)
    .fetch_all(conn)
    .await?)
}

pub async fn get_lesson(conn: &mut PgConnection, id: Uuid) -> DbResult<Lesson> {
    sqlx::query_as::<_, Lesson>(sqlx::AssertSqlSafe(format!(
        "SELECT {LESSON_COLS} FROM timetable_lessons WHERE id = $1 AND deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("lesson".into()))
}

pub async fn create_lesson(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    year_id: Uuid,
    input: &LessonInput,
) -> DbResult<Lesson> {
    check_lesson(input)?;
    let row = sqlx::query_as::<_, Lesson>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO timetable_lessons (id, school_id, academic_year_id, subject_id, teacher_ids, section_ids, room_ids, periods_per_card, cards_per_week, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) RETURNING {LESSON_COLS}"
    )))
    .bind(new_id())
    .bind(school_id)
    .bind(year_id)
    .bind(input.subject_id)
    .bind(dedup(&input.teacher_ids))
    .bind(dedup(&input.section_ids))
    .bind(dedup(&input.room_ids))
    .bind(input.periods_per_card)
    .bind(input.cards_per_week)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(conn, hlc, actor, Action::Create, "timetable_lesson", Some(row.id), None, Some(snapshot(&row))).await?;
    Ok(row)
}

pub async fn update_lesson(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    input: &LessonInput,
) -> DbResult<Lesson> {
    check_lesson(input)?;
    let old = get_lesson(&mut *conn, id).await?;
    let row = sqlx::query_as::<_, Lesson>(sqlx::AssertSqlSafe(format!(
        "UPDATE timetable_lessons SET subject_id = $2, teacher_ids = $3, section_ids = $4, room_ids = $5, periods_per_card = $6,
                cards_per_week = $7, updated_at = now(), updated_by = $8, hlc = $9
         WHERE id = $1 AND deleted_at IS NULL RETURNING {LESSON_COLS}"
    )))
    .bind(id)
    .bind(input.subject_id)
    .bind(dedup(&input.teacher_ids))
    .bind(dedup(&input.section_ids))
    .bind(dedup(&input.room_ids))
    .bind(input.periods_per_card)
    .bind(input.cards_per_week)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(conn, hlc, actor, Action::Update, "timetable_lesson", Some(id), Some(snapshot(&old)), Some(snapshot(&row))).await?;
    Ok(row)
}

/// Soft-delete a lesson and the cards placed for it in every timetable of its year.
pub async fn delete_lesson(conn: &mut PgConnection, hlc: &Hlc, actor: Actor, id: Uuid) -> DbResult<()> {
    let old = get_lesson(&mut *conn, id).await?;
    soft_delete(&mut *conn, hlc, actor, "timetable_lessons", id).await?;
    sqlx::query(
        "UPDATE timetable_cards SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3
         WHERE lesson_id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await?;
    audit::record(conn, hlc, actor, Action::Delete, "timetable_lesson", Some(id), Some(snapshot(&old)), None).await?;
    Ok(())
}

/// Seed lessons from the teacher allocation grid (`section_teachers` rows with a subject):
/// one lesson per (section, subject) that has none yet, with the allocated teachers and the
/// default number of sessions. Returns how many lessons were created.
pub async fn seed_lessons_from_allocations(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    year_id: Uuid,
    cards_per_week: i32,
) -> DbResult<usize> {
    #[derive(sqlx::FromRow)]
    struct Row {
        section_id: Uuid,
        subject_id: Uuid,
        teacher_ids: Vec<Uuid>,
    }
    let rows = sqlx::query_as::<_, Row>(
        "SELECT st.section_id, st.subject_id, array_agg(st.staff_id ORDER BY st.staff_id) AS teacher_ids
         FROM section_teachers st
         JOIN sections s ON s.id = st.section_id AND s.deleted_at IS NULL
         JOIN staff t ON t.id = st.staff_id AND t.deleted_at IS NULL
         WHERE st.school_id = $1 AND s.academic_year_id = $2 AND st.subject_id IS NOT NULL AND st.deleted_at IS NULL
         GROUP BY st.section_id, st.subject_id",
    )
    .bind(school_id)
    .bind(year_id)
    .fetch_all(&mut *conn)
    .await?;
    let existing = list_lessons(&mut *conn, year_id).await?;
    let mut have: HashSet<(Uuid, Uuid)> = HashSet::new();
    for l in &existing {
        for s in &l.section_ids {
            have.insert((*s, l.subject_id));
        }
    }
    let mut n = 0;
    for r in rows {
        if have.contains(&(r.section_id, r.subject_id)) {
            continue;
        }
        create_lesson(
            &mut *conn,
            hlc,
            actor,
            school_id,
            year_id,
            &LessonInput {
                subject_id: r.subject_id,
                teacher_ids: r.teacher_ids,
                section_ids: vec![r.section_id],
                room_ids: vec![],
                periods_per_card: 1,
                cards_per_week,
            },
        )
        .await?;
        n += 1;
    }
    Ok(n)
}

// ---- rules ----------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Rule {
    pub id: Uuid,
    pub school_id: Uuid,
    pub academic_year_id: Uuid,
    /// `school`, `teacher`, `section` or `room`.
    pub kind: String,
    pub entity_id: Option<Uuid>,
    pub shift_ids: Vec<Uuid>,
    pub home_room_id: Option<Uuid>,
    pub time_off: Value,
    pub limits: Value,
    pub hlc: String,
}

impl Rule {
    pub fn time_off(&self) -> Vec<TimeOff> {
        from_json(&self.time_off)
    }
    pub fn teacher_limits(&self) -> TeacherLimits {
        from_json(&self.limits)
    }
    pub fn school_rules(&self) -> SchoolRules {
        serde_json::from_value(self.limits.clone()).unwrap_or_default()
    }
}

#[derive(Debug, Clone)]
pub struct RuleInput {
    pub kind: String,
    pub entity_id: Option<Uuid>,
    pub shift_ids: Vec<Uuid>,
    pub home_room_id: Option<Uuid>,
    pub time_off: Vec<TimeOff>,
    pub limits: Value,
}

const RULE_COLS: &str = "id, school_id, academic_year_id, kind, entity_id, shift_ids, home_room_id, time_off, limits, hlc";

pub async fn list_rules(conn: &mut PgConnection, year_id: Uuid) -> DbResult<Vec<Rule>> {
    Ok(sqlx::query_as::<_, Rule>(sqlx::AssertSqlSafe(format!(
        "SELECT {RULE_COLS} FROM timetable_rules WHERE academic_year_id = $1 AND deleted_at IS NULL ORDER BY kind, entity_id"
    )))
    .bind(year_id)
    .fetch_all(conn)
    .await?)
}

/// Create or replace the rule of one entity (or the school) for the year; audited.
pub async fn set_rule(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    year_id: Uuid,
    input: &RuleInput,
) -> DbResult<Rule> {
    if !matches!(input.kind.as_str(), "school" | "teacher" | "section" | "room") {
        return Err(DbError::Conflict("unknown rule kind".into()));
    }
    if (input.kind == "school") != input.entity_id.is_none() {
        return Err(DbError::Conflict(
            "school rules have no entity; other rules name one".into(),
        ));
    }
    if input.time_off.iter().any(|c| c.day > 6 || c.period == 0 || !(1..=2).contains(&c.state)) {
        return Err(DbError::Conflict("a time-off cell names a day, a period and a state".into()));
    }
    let old = sqlx::query_as::<_, Rule>(sqlx::AssertSqlSafe(format!(
        "SELECT {RULE_COLS} FROM timetable_rules WHERE academic_year_id = $1 AND kind = $2 AND entity_id IS NOT DISTINCT FROM $3 AND deleted_at IS NULL"
    )))
    .bind(year_id)
    .bind(&input.kind)
    .bind(input.entity_id)
    .fetch_optional(&mut *conn)
    .await?;
    let row = match &old {
        Some(o) => {
            sqlx::query_as::<_, Rule>(sqlx::AssertSqlSafe(format!(
                "UPDATE timetable_rules SET shift_ids = $2, home_room_id = $3, time_off = $4, limits = $5, updated_at = now(), updated_by = $6, hlc = $7
                 WHERE id = $1 RETURNING {RULE_COLS}"
            )))
            .bind(o.id)
            .bind(dedup(&input.shift_ids))
            .bind(input.home_room_id)
            .bind(serde_json::to_value(&input.time_off).unwrap_or(Value::Null))
            .bind(&input.limits)
            .bind(actor.user_id)
            .bind(hlc.now().to_string())
            .fetch_one(&mut *conn)
            .await?
        }
        None => {
            sqlx::query_as::<_, Rule>(sqlx::AssertSqlSafe(format!(
                "INSERT INTO timetable_rules (id, school_id, academic_year_id, kind, entity_id, shift_ids, home_room_id, time_off, limits, updated_by, hlc)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) RETURNING {RULE_COLS}"
            )))
            .bind(new_id())
            .bind(school_id)
            .bind(year_id)
            .bind(&input.kind)
            .bind(input.entity_id)
            .bind(dedup(&input.shift_ids))
            .bind(input.home_room_id)
            .bind(serde_json::to_value(&input.time_off).unwrap_or(Value::Null))
            .bind(&input.limits)
            .bind(actor.user_id)
            .bind(hlc.now().to_string())
            .fetch_one(&mut *conn)
            .await?
        }
    };
    audit::record(
        conn,
        hlc,
        actor,
        if old.is_some() { Action::Update } else { Action::Create },
        "timetable_rule",
        Some(row.id),
        old.as_ref().map(snapshot),
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

// ---- timetables and cards ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Timetable {
    pub id: Uuid,
    pub school_id: Uuid,
    pub academic_year_id: Uuid,
    pub name: String,
    /// `draft` or `published`.
    pub status: String,
    pub report: Option<Value>,
    pub generated_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub hlc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, sqlx::FromRow)]
pub struct Card {
    pub id: Uuid,
    pub timetable_id: Uuid,
    pub lesson_id: Uuid,
    pub number: i32,
    pub shift_id: Uuid,
    pub day: i16,
    pub period: i16,
    pub length: i16,
    pub room_id: Option<Uuid>,
    pub locked: bool,
}

const TT_COLS: &str = "id, school_id, academic_year_id, name, status, report, generated_at, created_at, hlc";
const CARD_COLS: &str = "id, timetable_id, lesson_id, number, shift_id, day, period, length, room_id, locked";

pub async fn list_timetables(conn: &mut PgConnection, year_id: Uuid) -> DbResult<Vec<Timetable>> {
    Ok(sqlx::query_as::<_, Timetable>(sqlx::AssertSqlSafe(format!(
        "SELECT {TT_COLS} FROM timetables WHERE academic_year_id = $1 AND deleted_at IS NULL ORDER BY status DESC, created_at DESC"
    )))
    .bind(year_id)
    .fetch_all(conn)
    .await?)
}

pub async fn get_timetable(conn: &mut PgConnection, id: Uuid) -> DbResult<Timetable> {
    sqlx::query_as::<_, Timetable>(sqlx::AssertSqlSafe(format!(
        "SELECT {TT_COLS} FROM timetables WHERE id = $1 AND deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("timetable".into()))
}

pub async fn published_timetable(conn: &mut PgConnection, year_id: Uuid) -> DbResult<Option<Timetable>> {
    Ok(sqlx::query_as::<_, Timetable>(sqlx::AssertSqlSafe(format!(
        "SELECT {TT_COLS} FROM timetables WHERE academic_year_id = $1 AND status = 'published' AND deleted_at IS NULL"
    )))
    .bind(year_id)
    .fetch_optional(conn)
    .await?)
}

pub async fn create_timetable(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    year_id: Uuid,
    name: &str,
) -> DbResult<Timetable> {
    let name = name.trim();
    if name.is_empty() {
        return Err(DbError::Conflict("the timetable needs a name".into()));
    }
    let row = sqlx::query_as::<_, Timetable>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO timetables (id, school_id, academic_year_id, name, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING {TT_COLS}"
    )))
    .bind(new_id())
    .bind(school_id)
    .bind(year_id)
    .bind(name)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(conn, hlc, actor, Action::Create, "timetable", Some(row.id), None, Some(snapshot(&row))).await?;
    Ok(row)
}

pub async fn rename_timetable(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    name: &str,
) -> DbResult<Timetable> {
    let name = name.trim();
    if name.is_empty() {
        return Err(DbError::Conflict("the timetable needs a name".into()));
    }
    let old = get_timetable(&mut *conn, id).await?;
    let row = sqlx::query_as::<_, Timetable>(sqlx::AssertSqlSafe(format!(
        "UPDATE timetables SET name = $2, updated_at = now(), updated_by = $3, hlc = $4 WHERE id = $1 RETURNING {TT_COLS}"
    )))
    .bind(id)
    .bind(name)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(conn, hlc, actor, Action::Update, "timetable", Some(id), Some(snapshot(&old)), Some(snapshot(&row))).await?;
    Ok(row)
}

/// Publish a timetable: it becomes the one the school runs on; a previously published one
/// of the year goes back to draft.
pub async fn publish_timetable(conn: &mut PgConnection, hlc: &Hlc, actor: Actor, id: Uuid) -> DbResult<Timetable> {
    let old = get_timetable(&mut *conn, id).await?;
    if old.status == "published" {
        return Ok(old);
    }
    sqlx::query(
        "UPDATE timetables SET status = 'draft', updated_at = now(), updated_by = $2, hlc = $3
         WHERE academic_year_id = $1 AND status = 'published' AND deleted_at IS NULL",
    )
    .bind(old.academic_year_id)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await?;
    let row = sqlx::query_as::<_, Timetable>(sqlx::AssertSqlSafe(format!(
        "UPDATE timetables SET status = 'published', updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1 RETURNING {TT_COLS}"
    )))
    .bind(id)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(conn, hlc, actor, Action::Update, "timetable", Some(id), Some(snapshot(&old)), Some(snapshot(&row))).await?;
    Ok(row)
}

pub async fn delete_timetable(conn: &mut PgConnection, hlc: &Hlc, actor: Actor, id: Uuid) -> DbResult<()> {
    let old = get_timetable(&mut *conn, id).await?;
    soft_delete(&mut *conn, hlc, actor, "timetables", id).await?;
    sqlx::query(
        "UPDATE timetable_cards SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE timetable_id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await?;
    audit::record(conn, hlc, actor, Action::Delete, "timetable", Some(id), Some(snapshot(&old)), None).await?;
    Ok(())
}

pub async fn cards(conn: &mut PgConnection, timetable_id: Uuid) -> DbResult<Vec<Card>> {
    Ok(sqlx::query_as::<_, Card>(sqlx::AssertSqlSafe(format!(
        "SELECT {CARD_COLS} FROM timetable_cards WHERE timetable_id = $1 AND deleted_at IS NULL ORDER BY day, period, lesson_id, number"
    )))
    .bind(timetable_id)
    .fetch_all(conn)
    .await?)
}

/// A placed card as the engine sees it, converted back to Madrasa ids.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedInput {
    pub lesson_id: Uuid,
    pub number: i32,
    pub shift_id: Uuid,
    pub day: i16,
    pub period: i16,
    pub length: i16,
    pub room_id: Option<Uuid>,
    pub locked: bool,
}

impl PlacedInput {
    pub fn from_engine(c: &engine::PlacedCard) -> Option<PlacedInput> {
        Some(PlacedInput {
            lesson_id: c.lesson_id.parse().ok()?,
            number: c.number as i32,
            shift_id: c.shift.parse().ok()?,
            day: c.day as i16,
            period: c.period as i16,
            length: c.length as i16,
            room_id: c.room.as_deref().and_then(|r| r.parse().ok()),
            locked: c.locked,
        })
    }
}

/// Replace every card of a timetable with the given placements and store the generator's
/// report; one audit entry for the whole change.
pub async fn replace_cards(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    timetable_id: Uuid,
    placed: &[PlacedInput],
    report: Option<Value>,
) -> DbResult<usize> {
    let old = cards(&mut *conn, timetable_id).await?;
    sqlx::query(
        "UPDATE timetable_cards SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE timetable_id = $1 AND deleted_at IS NULL",
    )
    .bind(timetable_id)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await?;
    for p in placed {
        sqlx::query(
            "INSERT INTO timetable_cards (id, school_id, timetable_id, lesson_id, number, shift_id, day, period, length, room_id, locked, updated_by, hlc)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        )
        .bind(new_id())
        .bind(school_id)
        .bind(timetable_id)
        .bind(p.lesson_id)
        .bind(p.number)
        .bind(p.shift_id)
        .bind(p.day)
        .bind(p.period)
        .bind(p.length)
        .bind(p.room_id)
        .bind(p.locked)
        .bind(actor.user_id)
        .bind(hlc.now().to_string())
        .execute(&mut *conn)
        .await?;
    }
    sqlx::query(
        "UPDATE timetables SET report = COALESCE($2, report), generated_at = CASE WHEN $2 IS NULL THEN generated_at ELSE now() END,
                updated_at = now(), updated_by = $3, hlc = $4 WHERE id = $1",
    )
    .bind(timetable_id)
    .bind(&report)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "timetable_cards",
        Some(timetable_id),
        Some(serde_json::json!({ "cards": old.len() })),
        Some(serde_json::json!({ "cards": placed.len(), "report": report.is_some() })),
    )
    .await?;
    Ok(placed.len())
}

/// Lock or unlock one card.
pub async fn set_card_locked(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    timetable_id: Uuid,
    lesson_id: Uuid,
    number: i32,
    locked: bool,
) -> DbResult<Card> {
    let row = sqlx::query_as::<_, Card>(sqlx::AssertSqlSafe(format!(
        "UPDATE timetable_cards SET locked = $4, updated_at = now(), updated_by = $5, hlc = $6
         WHERE timetable_id = $1 AND lesson_id = $2 AND number = $3 AND deleted_at IS NULL RETURNING {CARD_COLS}"
    )))
    .bind(timetable_id)
    .bind(lesson_id)
    .bind(number)
    .bind(locked)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| DbError::NotFound("timetable card".into()))?;
    audit::record(conn, hlc, actor, Action::Update, "timetable_card", Some(row.id), None, Some(snapshot(&row))).await?;
    Ok(row)
}

// ---- substitutions ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Absence {
    pub id: Uuid,
    pub school_id: Uuid,
    pub academic_year_id: Uuid,
    pub staff_id: Uuid,
    pub from_date: NaiveDate,
    pub to_date: NaiveDate,
    pub periods: Value,
    pub note: String,
    pub hlc: String,
}

impl Absence {
    pub fn periods(&self) -> Vec<PeriodRef> {
        from_json(&self.periods)
    }
}

#[derive(Debug, Clone)]
pub struct AbsenceInput {
    pub staff_id: Uuid,
    pub from_date: NaiveDate,
    pub to_date: NaiveDate,
    pub periods: Vec<PeriodRef>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Cover {
    pub id: Uuid,
    pub school_id: Uuid,
    pub academic_year_id: Uuid,
    pub absence_id: Uuid,
    pub date: NaiveDate,
    pub lesson_id: Uuid,
    pub number: i32,
    pub absent_staff_id: Uuid,
    pub cover_staff_id: Option<Uuid>,
    pub shift_id: Uuid,
    pub period: i16,
    pub length: i16,
    pub note: String,
    pub hlc: String,
}

#[derive(Debug, Clone)]
pub struct CoverInput {
    pub absence_id: Uuid,
    pub date: NaiveDate,
    pub lesson_id: Uuid,
    pub number: i32,
    pub absent_staff_id: Uuid,
    /// `None` = no cover needed (the class is free or merged).
    pub cover_staff_id: Option<Uuid>,
    pub shift_id: Uuid,
    pub period: i16,
    pub length: i16,
    pub note: String,
}

const ABSENCE_COLS: &str = "id, school_id, academic_year_id, staff_id, from_date, to_date, periods, note, hlc";
const COVER_COLS: &str = "id, school_id, academic_year_id, absence_id, date, lesson_id, number, absent_staff_id, cover_staff_id, shift_id, period, length, note, hlc";

pub async fn list_absences(conn: &mut PgConnection, year_id: Uuid) -> DbResult<Vec<Absence>> {
    Ok(sqlx::query_as::<_, Absence>(sqlx::AssertSqlSafe(format!(
        "SELECT {ABSENCE_COLS} FROM timetable_absences WHERE academic_year_id = $1 AND deleted_at IS NULL ORDER BY from_date DESC, created_at DESC"
    )))
    .bind(year_id)
    .fetch_all(conn)
    .await?)
}

pub async fn get_absence(conn: &mut PgConnection, id: Uuid) -> DbResult<Absence> {
    sqlx::query_as::<_, Absence>(sqlx::AssertSqlSafe(format!(
        "SELECT {ABSENCE_COLS} FROM timetable_absences WHERE id = $1 AND deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("absence".into()))
}

pub async fn create_absence(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    year_id: Uuid,
    input: &AbsenceInput,
) -> DbResult<Absence> {
    if input.to_date < input.from_date {
        return Err(DbError::Conflict("the absence ends before it starts".into()));
    }
    let row = sqlx::query_as::<_, Absence>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO timetable_absences (id, school_id, academic_year_id, staff_id, from_date, to_date, periods, note, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING {ABSENCE_COLS}"
    )))
    .bind(new_id())
    .bind(school_id)
    .bind(year_id)
    .bind(input.staff_id)
    .bind(input.from_date)
    .bind(input.to_date)
    .bind(serde_json::to_value(&input.periods).unwrap_or(Value::Null))
    .bind(input.note.trim())
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(conn, hlc, actor, Action::Create, "timetable_absence", Some(row.id), None, Some(snapshot(&row))).await?;
    Ok(row)
}

/// Remove an absence and the covers decided for it.
pub async fn delete_absence(conn: &mut PgConnection, hlc: &Hlc, actor: Actor, id: Uuid) -> DbResult<()> {
    let old = get_absence(&mut *conn, id).await?;
    soft_delete(&mut *conn, hlc, actor, "timetable_absences", id).await?;
    sqlx::query(
        "UPDATE timetable_covers SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE absence_id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await?;
    audit::record(conn, hlc, actor, Action::Delete, "timetable_absence", Some(id), Some(snapshot(&old)), None).await?;
    Ok(())
}

pub async fn list_covers(conn: &mut PgConnection, year_id: Uuid) -> DbResult<Vec<Cover>> {
    Ok(sqlx::query_as::<_, Cover>(sqlx::AssertSqlSafe(format!(
        "SELECT {COVER_COLS} FROM timetable_covers WHERE academic_year_id = $1 AND deleted_at IS NULL ORDER BY date, period"
    )))
    .bind(year_id)
    .fetch_all(conn)
    .await?)
}

/// Decide (or change) who covers one session on one date.
pub async fn set_cover(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    year_id: Uuid,
    input: &CoverInput,
) -> DbResult<Cover> {
    let old = sqlx::query_as::<_, Cover>(sqlx::AssertSqlSafe(format!(
        "SELECT {COVER_COLS} FROM timetable_covers WHERE absence_id = $1 AND date = $2 AND lesson_id = $3 AND number = $4 AND deleted_at IS NULL"
    )))
    .bind(input.absence_id)
    .bind(input.date)
    .bind(input.lesson_id)
    .bind(input.number)
    .fetch_optional(&mut *conn)
    .await?;
    let row = match &old {
        Some(o) => {
            sqlx::query_as::<_, Cover>(sqlx::AssertSqlSafe(format!(
                "UPDATE timetable_covers SET cover_staff_id = $2, shift_id = $3, period = $4, length = $5, note = $6, updated_at = now(), updated_by = $7, hlc = $8
                 WHERE id = $1 RETURNING {COVER_COLS}"
            )))
            .bind(o.id)
            .bind(input.cover_staff_id)
            .bind(input.shift_id)
            .bind(input.period)
            .bind(input.length)
            .bind(input.note.trim())
            .bind(actor.user_id)
            .bind(hlc.now().to_string())
            .fetch_one(&mut *conn)
            .await?
        }
        None => {
            sqlx::query_as::<_, Cover>(sqlx::AssertSqlSafe(format!(
                "INSERT INTO timetable_covers (id, school_id, academic_year_id, absence_id, date, lesson_id, number, absent_staff_id, cover_staff_id, shift_id, period, length, note, updated_by, hlc)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15) RETURNING {COVER_COLS}"
            )))
            .bind(new_id())
            .bind(school_id)
            .bind(year_id)
            .bind(input.absence_id)
            .bind(input.date)
            .bind(input.lesson_id)
            .bind(input.number)
            .bind(input.absent_staff_id)
            .bind(input.cover_staff_id)
            .bind(input.shift_id)
            .bind(input.period)
            .bind(input.length)
            .bind(input.note.trim())
            .bind(actor.user_id)
            .bind(hlc.now().to_string())
            .fetch_one(&mut *conn)
            .await?
        }
    };
    audit::record(
        conn,
        hlc,
        actor,
        if old.is_some() { Action::Update } else { Action::Create },
        "timetable_cover",
        Some(row.id),
        old.as_ref().map(snapshot),
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

/// Withdraw a cover decision (the session needs cover again).
pub async fn clear_cover(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    absence_id: Uuid,
    date: NaiveDate,
    lesson_id: Uuid,
    number: i32,
) -> DbResult<bool> {
    let old = sqlx::query_as::<_, Cover>(sqlx::AssertSqlSafe(format!(
        "SELECT {COVER_COLS} FROM timetable_covers WHERE absence_id = $1 AND date = $2 AND lesson_id = $3 AND number = $4 AND deleted_at IS NULL"
    )))
    .bind(absence_id)
    .bind(date)
    .bind(lesson_id)
    .bind(number)
    .fetch_optional(&mut *conn)
    .await?;
    let Some(o) = old else { return Ok(false) };
    soft_delete(&mut *conn, hlc, actor, "timetable_covers", o.id).await?;
    audit::record(conn, hlc, actor, Action::Delete, "timetable_cover", Some(o.id), Some(snapshot(&o)), None).await?;
    Ok(true)
}

// ---- mapping to the engine --------------------------------------------------------------------

/// Everything the engine and the screens need for one academic year.
pub struct YearSetup {
    pub setup: engine::SchoolSetup,
    pub shifts: Vec<Shift>,
    pub rooms: Vec<Room>,
    pub lessons: Vec<Lesson>,
    pub sections: Vec<Section>,
    pub staff: Vec<Staff>,
    pub subjects: Vec<Subject>,
    pub rules: Vec<Rule>,
    /// The timetable whose cards were loaded (`None` = no cards).
    pub timetable: Option<Timetable>,
}

const COLORS: &[&str] = &[
    "#2864C5", "#19835A", "#D99A27", "#8E5AD1", "#C44D58", "#0E8C9E", "#B5651D", "#5B6B7F",
    "#2E9E6B", "#C2366B", "#3B7DD8", "#7A8A1F",
];

fn initials(name: &str) -> String {
    let s: String = name
        .split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(3)
        .collect();
    if s.is_empty() {
        name.chars().take(3).collect()
    } else {
        s.to_uppercase()
    }
}

fn time_off_cells(cells: &[TimeOff]) -> Vec<engine::TimeOffCell> {
    cells
        .iter()
        .map(|c| engine::TimeOffCell {
            shift: c.shift_id.to_string(),
            day: c.day,
            period: c.period,
            state: c.state,
        })
        .collect()
}

/// Load the year's setup and, when given, the cards of one timetable (otherwise the
/// published one, if any).
pub async fn setup_for_year(
    conn: &mut PgConnection,
    school_id: Uuid,
    year: &AcademicYear,
    timetable_id: Option<Uuid>,
) -> DbResult<YearSetup> {
    let school = crate::schools::get(&mut *conn, school_id).await?;
    let shifts = list_shifts(&mut *conn, year.id).await?;
    let rooms = list_rooms(&mut *conn, school_id).await?;
    let lessons = list_lessons(&mut *conn, year.id).await?;
    let sections = academics::list_sections(&mut *conn, school_id, Some(year.id), None).await?;
    let classes = academics::list_classes(&mut *conn, school_id).await?;
    let staff = staff_repo::list(&mut *conn, school_id, None).await?;
    let subjects = academics::list_subjects(&mut *conn, school_id).await?;
    let rules = list_rules(&mut *conn, year.id).await?;
    let timetable = match timetable_id {
        Some(id) => Some(get_timetable(&mut *conn, id).await?),
        None => published_timetable(&mut *conn, year.id).await?,
    };
    let placed = match &timetable {
        Some(t) => cards(&mut *conn, t.id).await?,
        None => Vec::new(),
    };
    let absences = list_absences(&mut *conn, year.id).await?;
    let covers = list_covers(&mut *conn, year.id).await?;

    let rule_of = |kind: &str, id: Option<Uuid>| rules.iter().find(|r| r.kind == kind && r.entity_id == id);
    let enabled: Vec<&Shift> = shifts.iter().filter(|s| s.enabled).collect();
    let enabled_ids: Vec<String> = enabled.iter().map(|s| s.id.to_string()).collect();

    let mut setup = engine::SchoolSetup::default_new(&school.name);
    setup.academic_year = year.name.clone();
    let mut working = [false; 7];
    for s in &enabled {
        for (i, d) in s.days().iter().enumerate().take(7) {
            working[i] |= d.working && d.periods > 0;
        }
    }
    for (i, d) in setup.days.iter_mut().enumerate() {
        d.working = working[i];
    }
    setup.shifts = shifts
        .iter()
        .map(|s| {
            let times = s.period_times();
            let days = s.days();
            engine::Shift {
                id: s.id.to_string(),
                name: s.name.clone(),
                enabled: s.enabled,
                days: (0..7)
                    .map(|i| match days.get(i) {
                        Some(d) if d.working => times
                            .iter()
                            .take(d.periods as usize)
                            .map(|p| engine::PeriodTime { start: p.start.clone(), end: p.end.clone() })
                            .collect(),
                        _ => vec![],
                    })
                    .collect(),
                breaks: s
                    .breaks()
                    .iter()
                    .map(|b| engine::Break { after_period: b.after_period, name: b.name.clone(), cannot_cross: b.cannot_cross })
                    .collect(),
            }
        })
        .collect();

    let class_name: HashMap<Uuid, &str> = classes.iter().map(|c| (c.id, c.name.as_str())).collect();
    let default_shift = enabled.first().map(|s| s.id.to_string()).unwrap_or_default();
    setup.master.subjects = subjects
        .iter()
        .enumerate()
        .map(|(i, s)| engine::Subject {
            id: s.id.to_string(),
            name: s.name.clone(),
            short_name: if s.code.trim().is_empty() { initials(&s.name) } else { s.code.clone() },
            color: COLORS[i % COLORS.len()].into(),
        })
        .collect();
    // Active staff plus anyone a lesson still names.
    let named: HashSet<Uuid> = lessons.iter().flat_map(|l| l.teacher_ids.iter().copied()).collect();
    setup.master.teachers = staff
        .iter()
        .filter(|t| t.status == "active" || named.contains(&t.id))
        .map(|t| {
            let rule = rule_of("teacher", Some(t.id));
            let limits = rule.map(Rule::teacher_limits).unwrap_or_default();
            let mut own: Vec<String> = rule
                .map(|r| r.shift_ids.iter().map(ToString::to_string).collect())
                .unwrap_or_default();
            own.retain(|s| enabled_ids.contains(s));
            engine::Teacher {
                id: t.id.to_string(),
                name: t.full_name.clone(),
                short_name: initials(&t.full_name),
                email: t.email.clone().unwrap_or_default(),
                shifts: if own.is_empty() { enabled_ids.clone() } else { own },
                max_lessons_per_day: limits.max_per_day,
                max_periods_per_week: limits.max_per_week,
                max_consecutive: limits.max_consecutive,
                max_days_per_week: limits.max_days_per_week,
                time_off: rule.map(|r| time_off_cells(&r.time_off())).unwrap_or_default(),
            }
        })
        .collect();
    setup.master.classes = sections
        .iter()
        .map(|s| {
            let rule = rule_of("section", Some(s.id));
            let shift = rule
                .and_then(|r| r.shift_ids.first())
                .map(ToString::to_string)
                .filter(|id| enabled_ids.contains(id))
                .unwrap_or_else(|| default_shift.clone());
            let name = format!("{} {}", class_name.get(&s.class_id).copied().unwrap_or(""), s.name).trim().to_string();
            engine::Class {
                id: s.id.to_string(),
                name: name.clone(),
                short_name: name,
                shift,
                class_teacher: s.class_teacher_id.map(|t| t.to_string()),
                home_room: rule.and_then(|r| r.home_room_id).map(|r| r.to_string()),
                divisions: vec![],
                time_off: rule.map(|r| time_off_cells(&r.time_off())).unwrap_or_default(),
            }
        })
        .collect();
    let mut buildings: Vec<String> = rooms.iter().map(|r| r.building.clone()).filter(|b| !b.is_empty()).collect();
    buildings.sort();
    buildings.dedup();
    setup.master.buildings = buildings
        .iter()
        .map(|b| engine::master::Building { id: b.clone(), name: b.clone() })
        .collect();
    setup.master.rooms = rooms
        .iter()
        .map(|r| engine::Room {
            id: r.id.to_string(),
            name: r.name.clone(),
            short_name: r.short_name.clone(),
            building: if r.building.is_empty() { None } else { Some(r.building.clone()) },
            capacity: r.capacity.map(|c| c as u32),
            time_off: rule_of("room", Some(r.id)).map(|x| time_off_cells(&x.time_off())).unwrap_or_default(),
        })
        .collect();
    setup.lessons = lessons
        .iter()
        .map(|l| EngineLesson {
            id: l.id.to_string(),
            subject: l.subject_id.to_string(),
            teachers: l.teacher_ids.iter().map(ToString::to_string).collect(),
            parts: l
                .section_ids
                .iter()
                .map(|s| engine::LessonPart { class_id: s.to_string(), group_id: None })
                .collect(),
            periods_per_card: l.periods_per_card as u32,
            cards_per_week: l.cards_per_week as u32,
            rooms: l.room_ids.iter().map(ToString::to_string).collect(),
        })
        .collect();
    let school_rules = rule_of("school", None).map(Rule::school_rules).unwrap_or_default();
    setup.settings = engine::Settings {
        default_max_per_day: school_rules.default_max_per_day,
        default_max_per_week: school_rules.default_max_per_week,
        max_same_subject_per_day: school_rules.max_same_subject_per_day,
        allow_relaxation: school_rules.allow_relaxation,
        improve_seconds: school_rules.improve_seconds,
        ..Default::default()
    };
    setup.timetable = timetable.as_ref().map(|_| engine::Timetable {
        cards: placed
            .iter()
            .map(|c| engine::PlacedCard {
                lesson_id: c.lesson_id.to_string(),
                number: c.number as u32,
                shift: c.shift_id.to_string(),
                day: c.day as u8,
                period: c.period as u8,
                length: c.length as u32,
                room: c.room_id.map(|r| r.to_string()),
                locked: c.locked,
            })
            .collect(),
    });
    setup.substitutions = subs::Substitutions {
        absences: absences
            .iter()
            .map(|a| subs::Absence {
                id: a.id.to_string(),
                teacher_id: a.staff_id.to_string(),
                from: a.from_date.format("%Y-%m-%d").to_string(),
                to: a.to_date.format("%Y-%m-%d").to_string(),
                periods: a
                    .periods()
                    .iter()
                    .map(|p| subs::PeriodRef { shift: p.shift_id.to_string(), period: p.period })
                    .collect(),
                note: a.note.clone(),
            })
            .collect(),
        covers: covers
            .iter()
            .map(|c| subs::Cover {
                absence_id: c.absence_id.to_string(),
                date: c.date.format("%Y-%m-%d").to_string(),
                lesson_id: c.lesson_id.to_string(),
                number: c.number as u32,
                absent_teacher: c.absent_staff_id.to_string(),
                cover_teacher: c.cover_staff_id.map(|t| t.to_string()),
                no_cover: c.cover_staff_id.is_none(),
                shift: c.shift_id.to_string(),
                period: c.period as u8,
                length: c.length as u32,
                archived: false,
            })
            .collect(),
        counts_from: None,
        counts_year: Some(year.name.clone()),
    };
    Ok(YearSetup {
        setup,
        shifts,
        rooms,
        lessons,
        sections,
        staff,
        subjects,
        rules,
        timetable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_shift_is_a_pakistani_school_week() {
        let s = default_shift_input("Morning");
        assert!(check_shift(&s).is_ok());
        assert_eq!(s.period_times.len(), 7);
        assert_eq!(s.period_times[0].start, "08:00");
        assert_eq!(s.period_times[3].start, "10:20", "the break is after the third period");
        assert_eq!(s.days[4].periods, 5, "Friday is short");
        assert!(!s.days[5].working && !s.days[6].working);
    }

    #[test]
    fn shift_checks_catch_bad_input() {
        let mut s = default_shift_input("Morning");
        s.days[0].periods = 9;
        assert!(check_shift(&s).is_err());
        let mut s = default_shift_input("Morning");
        s.breaks[0].after_period = 7;
        assert!(check_shift(&s).is_err());
        let mut s = default_shift_input("Morning");
        s.period_times[0].start = "8am".into();
        assert!(check_shift(&s).is_err());
    }

    #[test]
    fn lesson_checks() {
        let ok = LessonInput {
            subject_id: Uuid::nil(),
            teacher_ids: vec![],
            section_ids: vec![Uuid::nil()],
            room_ids: vec![],
            periods_per_card: 2,
            cards_per_week: 3,
        };
        assert!(check_lesson(&ok).is_ok());
        assert!(check_lesson(&LessonInput { section_ids: vec![], ..ok.clone() }).is_err());
        assert!(check_lesson(&LessonInput { periods_per_card: 4, ..ok.clone() }).is_err());
        assert!(check_lesson(&LessonInput { cards_per_week: 0, ..ok }).is_err());
    }

    #[test]
    fn initials_and_short_names() {
        assert_eq!(initials("Ayesha Khan"), "AK");
        assert_eq!(initials("Muhammad Ali Raza Shah"), "MAR");
        assert_eq!(short_or("Room 12", ""), "Room 12");
        assert_eq!(short_or("Room 12", " R12 "), "R12");
    }
}
