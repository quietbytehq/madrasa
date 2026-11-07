//! Teacher substitutions (0.2.0 stage 5; the plan is in docs/stage5-plan.md).
//!
//! Absences and covers are kept apart from the timetable: everything here only *reads* the base
//! timetable and only the person's choices (absences, covers) are stored. A cover keeps a copy of
//! where the lesson was when the decision was made, so the record keeps its meaning if the
//! timetable is built again later.
use crate::timetable::model::SchoolSetup;
use crate::timetable::solver::PlacedCard;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// One period of one shift.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct PeriodRef {
    pub shift: String,
    /// 1-based
    pub period: u8,
}

/// A teacher is away from `from` to `to` (inclusive, "YYYY-MM-DD"). With `periods` empty, for
/// whole days; otherwise only for those periods, on each day of the range (part-day).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Absence {
    pub id: String,
    pub teacher_id: String,
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub periods: Vec<PeriodRef>,
    #[serde(default)]
    pub note: String,
}

/// The decision for one lesson session on one date: who covers it, or that no cover is needed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Cover {
    pub absence_id: String,
    pub date: String,
    pub lesson_id: String,
    /// Which session of the lesson (1-based), as in the timetable.
    pub number: u32,
    pub absent_teacher: String,
    #[serde(default)]
    pub cover_teacher: Option<String>,
    #[serde(default)]
    pub no_cover: bool,
    /// Where the session was when the decision was made.
    pub shift: String,
    pub period: u8,
    pub length: u32,
    /// Decided before the counts were last reset: kept on the cover sheets, no longer counted.
    #[serde(default)]
    pub archived: bool,
}

impl Cover {
    /// The same session for the same absent teacher on the same date.
    pub fn key(&self) -> String {
        format!("{}|{}|{}#{}|{}", self.absence_id, self.date, self.lesson_id, self.number, self.absent_teacher)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Substitutions {
    pub absences: Vec<Absence>,
    pub covers: Vec<Cover>,
    /// The day the counts were last reset (covers decided before then are `archived`).
    pub counts_from: Option<String>,
    /// The school year shown with the counts (the academic year at the last reset).
    pub counts_year: Option<String>,
}

impl Substitutions {
    /// No absences and no covers (the file can stay in the old format).
    pub fn has_records(&self) -> bool {
        !self.absences.is_empty() || !self.covers.is_empty()
    }
}

/// The part of `Substitutions` saved as one row in the school file.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct CountSettings {
    pub counts_from: Option<String>,
    pub counts_year: Option<String>,
}

// ---------- dates (no date library: days since 1970-01-01, the proleptic Gregorian calendar) ----------

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// "2026-10-26" as a day number, or `None` if it is not a real date.
pub fn parse_date(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, m, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let n = days_from_civil(y, m, d);
    (civil_from_days(n) == (y, m, d)).then_some(n)
}

pub fn format_date(n: i64) -> String {
    let (y, m, d) = civil_from_days(n);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Monday = 0 ... Sunday = 6, like the school's list of days.
pub fn weekday(n: i64) -> usize {
    (n + 3).rem_euclid(7) as usize // 1970-01-01 was a Thursday
}

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/// "Monday 26 October 2026"
pub fn long_date(setup: &SchoolSetup, n: i64) -> String {
    let (y, m, d) = civil_from_days(n);
    let day = setup.days.get(weekday(n)).map_or("", |d| d.name.as_str());
    format!("{day} {d} {} {y}", MONTHS[(m - 1) as usize])
}

fn date_or_err(s: &str) -> Result<i64, String> {
    parse_date(s).ok_or_else(|| format!("\"{s}\" is not a date. Please choose a date."))
}

// ---------- the school's timetable, looked up quickly ----------

struct Ctx<'a> {
    s: &'a SchoolSetup,
    lessons: HashMap<&'a str, &'a crate::timetable::lessons::Lesson>,
    /// (teacher, shift, day, period) -> the placed session there
    busy: HashMap<(&'a str, &'a str, u8, u8), &'a PlacedCard>,
    /// periods each teacher teaches in a normal week
    week_load: HashMap<&'a str, u32>,
    /// periods each teacher teaches on each weekday
    day_load: HashMap<(&'a str, u8), u32>,
    /// (teacher, subject) pairs that occur in some lesson
    teaches: HashSet<(&'a str, &'a str)>,
    cards: &'a [PlacedCard],
}

impl<'a> Ctx<'a> {
    fn new(s: &'a SchoolSetup) -> Self {
        let lessons: HashMap<&str, &crate::timetable::lessons::Lesson> = s.lessons.iter().map(|l| (l.id.as_str(), l)).collect();
        let cards: &[PlacedCard] = s.timetable.as_ref().map_or(&[], |t| t.cards.as_slice());
        let (mut busy, mut week_load, mut day_load) = (HashMap::new(), HashMap::new(), HashMap::new());
        for c in cards {
            let Some(l) = lessons.get(c.lesson_id.as_str()) else { continue };
            for t in &l.teachers {
                *week_load.entry(t.as_str()).or_insert(0) += c.length;
                *day_load.entry((t.as_str(), c.day)).or_insert(0) += c.length;
                for k in 0..c.length.min(24) as u8 {
                    busy.insert((t.as_str(), c.shift.as_str(), c.day, c.period + k), c);
                }
            }
        }
        let teaches = s.lessons.iter().flat_map(|l| l.teachers.iter().map(move |t| (t.as_str(), l.subject.as_str()))).collect();
        Ctx { s, lessons, busy, week_load, day_load, teaches, cards }
    }

    fn teacher_name(&self, id: &str) -> String {
        self.s.master.teachers.iter().find(|t| t.id == id).map_or_else(|| "(removed teacher)".into(), |t| t.name.clone())
    }
    fn subject_name(&self, lesson: &str) -> String {
        let sub = self.lessons.get(lesson).map(|l| l.subject.as_str()).unwrap_or("");
        self.s.master.subjects.iter().find(|x| x.id == sub).map_or_else(|| "A lesson".into(), |x| x.name.clone())
    }
    /// "Grade 6 A, Grade 6 B (Boys)"
    fn class_names(&self, lesson: &str) -> String {
        let Some(l) = self.lessons.get(lesson) else { return String::new() };
        l.parts
            .iter()
            .filter_map(|p| {
                let c = self.s.master.classes.iter().find(|c| c.id == p.class_id)?;
                let g = p.group_id.as_ref().and_then(|g| c.divisions.iter().flat_map(|d| &d.groups).find(|x| &x.id == g));
                Some(match g {
                    Some(g) => format!("{} ({})", c.name, g.name),
                    None => c.name.clone(),
                })
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
    /// The session's room, or else the home rooms of its classes (where the cover teacher goes).
    fn room_name(&self, c: &PlacedCard) -> String {
        let name = |id: &String| self.s.master.rooms.iter().find(|x| &x.id == id).map(|r| r.name.clone());
        if let Some(r) = c.room.as_ref().and_then(name) {
            return r;
        }
        let Some(l) = self.lessons.get(c.lesson_id.as_str()) else { return String::new() };
        let mut out: Vec<String> = Vec::new();
        for p in &l.parts {
            let home = self.s.master.classes.iter().find(|x| x.id == p.class_id).and_then(|x| x.home_room.as_ref()).and_then(name);
            if let Some(h) = home.filter(|h| !out.contains(h)) {
                out.push(h);
            }
        }
        out.join(", ")
    }
    /// Start and end time of periods `period .. period + length - 1` on `day`.
    fn times(&self, shift: &str, day: u8, period: u8, length: u32) -> (String, String) {
        let Some(sh) = self.s.shifts.iter().find(|x| x.id == shift) else { return Default::default() };
        let Some(ps) = sh.days.get(day as usize) else { return Default::default() };
        let start = ps.get((period as usize).saturating_sub(1)).map(|p| p.start.clone()).unwrap_or_default();
        let end = ps.get((period as usize + length as usize).saturating_sub(2)).map(|p| p.end.clone()).unwrap_or_default();
        (start, end)
    }
    fn shift_name(&self, shift: &str) -> String {
        self.s.shifts.iter().find(|x| x.id == shift).map_or_else(|| shift.to_string(), |x| x.name.clone())
    }
}

fn overlaps(p: u8, len: u32, q: u8, qlen: u32) -> bool {
    (p as u32) < q as u32 + qlen && (q as u32) < p as u32 + len
}

/// Is `a` an absence on day `n` for (part of) periods `p .. p+len-1` of `shift`?
fn absence_hits(a: &Absence, n: i64, shift: &str, p: u8, len: u32) -> bool {
    let (Some(from), Some(to)) = (parse_date(&a.from), parse_date(&a.to)) else { return false };
    if n < from || n > to {
        return false;
    }
    a.periods.is_empty() || a.periods.iter().any(|x| x.shift == shift && overlaps(p, len, x.period, 1))
}

/// Whole-day absences, or part-day ones that cover at least one period: is the teacher away then?
fn away(s: &SchoolSetup, teacher: &str, n: i64, shift: &str, p: u8, len: u32) -> bool {
    s.substitutions.absences.iter().any(|a| a.teacher_id == teacher && absence_hits(a, n, shift, p, len))
}

/// Covers that count: their absence still exists, they name a teacher, and they were decided
/// after the counts were last reset (whatever the date of the lesson).
fn counted<'a>(s: &'a SchoolSetup) -> impl Iterator<Item = &'a Cover> + 'a {
    let ids: HashSet<&str> = s.substitutions.absences.iter().map(|a| a.id.as_str()).collect();
    s.substitutions.covers.iter().filter(move |c| c.cover_teacher.is_some() && !c.archived && ids.contains(c.absence_id.as_str()))
}

/// "Reset counts": every cover so far stops counting; the counts start again from `today`.
pub fn reset_counts(sub: &mut Substitutions, today: &str, year: &str) {
    for c in &mut sub.covers {
        c.archived = true;
    }
    sub.counts_from = Some(today.to_string());
    sub.counts_year = (!year.trim().is_empty()).then(|| year.to_string());
}

fn cover_counts_by_teacher(s: &SchoolSetup) -> HashMap<String, (u32, u32)> {
    let mut out: HashMap<String, (u32, u32)> = HashMap::new();
    for c in counted(s) {
        let e = out.entry(c.cover_teacher.clone().unwrap_or_default()).or_default();
        e.0 += 1;
        e.1 += c.length;
    }
    out
}

// ---------- the day view ----------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub teacher_id: String,
    pub name: String,
    /// Teaches the same subject (in some lesson).
    pub same_subject: bool,
    /// Covers counted so far this school year.
    pub covers: u32,
    /// Keeps every rule. When false, `reasons` says which rules it breaks.
    pub suitable: bool,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Need {
    pub absence_id: String,
    pub date: String,
    pub lesson_id: String,
    pub number: u32,
    pub absent_teacher: String,
    pub absent_name: String,
    pub shift: String,
    pub shift_name: String,
    pub period: u8,
    pub length: u32,
    pub start: String,
    pub end: String,
    pub classes: String,
    pub subject: String,
    pub room: String,
    pub note: String,
    /// The decision so far.
    pub cover_teacher: Option<String>,
    pub cover_name: Option<String>,
    pub no_cover: bool,
    /// When the chosen teacher breaks a rule: what is wrong.
    pub warning: Option<String>,
    /// Every teacher who could be asked, best first (suitable ones before the others).
    pub candidates: Vec<Candidate>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StaleCover {
    pub cover: Cover,
    pub title: String,
    pub cover_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CoverDay {
    pub date: String,
    /// "Monday 26 October 2026"
    pub title: String,
    pub working: bool,
    pub needs: Vec<Need>,
    /// Saved covers for this date whose lesson is no longer at that time in the timetable.
    pub stale: Vec<StaleCover>,
    pub has_timetable: bool,
}

/// The sessions the absent teachers would teach on day `n`: (absence, session).
fn needs_on<'a>(ctx: &Ctx<'a>, n: i64) -> Vec<(&'a Absence, &'a PlacedCard)> {
    let s = ctx.s;
    let w = weekday(n);
    if !s.days.get(w).is_some_and(|d| d.working) {
        return vec![];
    }
    let mut seen: HashSet<(&str, &str, u32)> = HashSet::new();
    let mut out = Vec::new();
    for a in &s.substitutions.absences {
        if !s.master.teachers.iter().any(|t| t.id == a.teacher_id) {
            continue;
        }
        for c in ctx.cards.iter().filter(|c| c.day as usize == w) {
            let Some(l) = ctx.lessons.get(c.lesson_id.as_str()) else { continue };
            if !l.teachers.contains(&a.teacher_id) || !absence_hits(a, n, &c.shift, c.period, c.length) {
                continue;
            }
            if seen.insert((a.teacher_id.as_str(), c.lesson_id.as_str(), c.number)) {
                out.push((a, c));
            }
        }
    }
    out.sort_by(|x, y| {
        let sh = |id: &str| s.shifts.iter().position(|z| z.id == id).unwrap_or(99);
        (sh(&x.1.shift), x.1.period, ctx.class_names(&x.1.lesson_id), ctx.teacher_name(&x.0.teacher_id))
            .cmp(&(sh(&y.1.shift), y.1.period, ctx.class_names(&y.1.lesson_id), ctx.teacher_name(&y.0.teacher_id)))
    });
    out
}

fn monday_of(n: i64) -> i64 {
    n - weekday(n) as i64
}

/// Which rules teacher `t` would break by covering session `c` on day `n` (empty = suitable).
/// `key` is the cover decision being made, which does not count against itself.
fn rule_problems(ctx: &Ctx, t: &crate::timetable::master::Teacher, n: i64, c: &PlacedCard, key: &str) -> Vec<String> {
    let s = ctx.s;
    let name = &t.name;
    let mut out = Vec::new();
    if !t.shifts.contains(&c.shift) {
        out.push(format!("{name} does not work in the {} shift.", ctx.shift_name(&c.shift).to_lowercase()));
    }
    let span = c.period..c.period + c.length.min(24) as u8;
    if let Some(o) = span.clone().find_map(|p| ctx.busy.get(&(t.id.as_str(), c.shift.as_str(), c.day, p))) {
        out.push(format!("{name} is teaching {} ({}) then.", ctx.subject_name(&o.lesson_id), ctx.class_names(&o.lesson_id)));
    }
    if t.time_off.iter().any(|x| x.state == 2 && x.shift == c.shift && x.day == c.day && span.contains(&x.period)) {
        out.push(format!("{name} is not available then (time off)."));
    }
    if away(s, &t.id, n, &c.shift, c.period, c.length) {
        out.push(format!("{name} is away then."));
    }
    let date = format_date(n);
    if let Some(o) = s.substitutions.covers.iter().find(|o| {
        o.cover_teacher.as_deref() == Some(t.id.as_str()) && o.date == date && o.shift == c.shift && overlaps(o.period, o.length, c.period, c.length) && o.key() != key
    }) {
        out.push(format!("{name} is already covering {} ({}) then.", ctx.subject_name(&o.lesson_id), ctx.class_names(&o.lesson_id)));
    }
    if let Some(max) = s.settings.max_per_week(t) {
        let week = monday_of(n);
        let covers: u32 = s
            .substitutions
            .covers
            .iter()
            .filter(|o| o.cover_teacher.as_deref() == Some(t.id.as_str()) && o.key() != key && parse_date(&o.date).is_some_and(|d| monday_of(d) == week))
            .map(|o| o.length)
            .sum();
        let total = ctx.week_load.get(t.id.as_str()).copied().unwrap_or(0) + covers + c.length;
        if total > max {
            out.push(format!("{name} would teach {total} periods that week, more than their weekly maximum of {max}."));
        }
    }
    out
}

fn need_key(a: &Absence, date: &str, c: &PlacedCard) -> String {
    format!("{}|{}|{}#{}|{}", a.id, date, c.lesson_id, c.number, a.teacher_id)
}

/// Everything the Substitutions page shows for one date.
pub fn cover_day(s: &SchoolSetup, date: &str) -> Result<CoverDay, String> {
    let n = date_or_err(date)?;
    let ctx = Ctx::new(s);
    let date = format_date(n);
    let counts = cover_counts_by_teacher(s);
    let covers: HashMap<String, &Cover> = s.substitutions.covers.iter().filter(|c| c.date == date).map(|c| (c.key(), c)).collect();
    let mut needs = Vec::new();
    let mut used: HashSet<String> = HashSet::new();
    for (a, c) in needs_on(&ctx, n) {
        let key = need_key(a, &date, c);
        let lesson = ctx.lessons.get(c.lesson_id.as_str()).copied();
        let subject = lesson.map(|l| l.subject.as_str()).unwrap_or("");
        let mut candidates: Vec<Candidate> = s
            .master
            .teachers
            .iter()
            .filter(|t| t.id != a.teacher_id && !lesson.is_some_and(|l| l.teachers.contains(&t.id)))
            .map(|t| {
                let reasons = rule_problems(&ctx, t, n, c, &key);
                Candidate {
                    teacher_id: t.id.clone(),
                    name: t.name.clone(),
                    same_subject: ctx.teaches.contains(&(t.id.as_str(), subject)),
                    covers: counts.get(&t.id).map_or(0, |x| x.0),
                    suitable: reasons.is_empty(),
                    reasons,
                }
            })
            .collect();
        candidates.sort_by(|x, y| {
            let load = |id: &str| ctx.day_load.get(&(id, c.day)).copied().unwrap_or(0);
            (!x.suitable, !x.same_subject, x.covers, load(&x.teacher_id), &x.name).cmp(&(!y.suitable, !y.same_subject, y.covers, load(&y.teacher_id), &y.name))
        });
        let cur = covers.get(&key).copied();
        used.insert(key);
        let cover_teacher = cur.and_then(|x| x.cover_teacher.clone());
        let warning = cover_teacher.as_ref().and_then(|id| {
            let cand = candidates.iter().find(|x| &x.teacher_id == id)?;
            (!cand.suitable).then(|| cand.reasons.join(" "))
        });
        let (start, end) = ctx.times(&c.shift, c.day, c.period, c.length);
        needs.push(Need {
            absence_id: a.id.clone(),
            date: date.clone(),
            lesson_id: c.lesson_id.clone(),
            number: c.number,
            absent_teacher: a.teacher_id.clone(),
            absent_name: ctx.teacher_name(&a.teacher_id),
            shift: c.shift.clone(),
            shift_name: ctx.shift_name(&c.shift),
            period: c.period,
            length: c.length,
            start,
            end,
            classes: ctx.class_names(&c.lesson_id),
            subject: ctx.subject_name(&c.lesson_id),
            room: ctx.room_name(c),
            note: a.note.clone(),
            cover_name: cover_teacher.as_deref().map(|t| ctx.teacher_name(t)),
            cover_teacher,
            no_cover: cur.is_some_and(|x| x.no_cover),
            warning,
            candidates,
        });
    }
    let stale = covers
        .iter()
        .filter(|(k, _)| !used.contains(*k))
        .map(|(_, c)| StaleCover {
            title: format!("{} ({}), period {}, for {}", ctx.subject_name(&c.lesson_id), ctx.class_names(&c.lesson_id), c.period, ctx.teacher_name(&c.absent_teacher)),
            cover_name: c.cover_teacher.as_deref().map(|t| ctx.teacher_name(t)),
            cover: (*c).clone(),
        })
        .collect();
    Ok(CoverDay {
        title: long_date(s, n),
        working: s.days.get(weekday(n)).is_some_and(|d| d.working),
        date,
        needs,
        stale,
        has_timetable: !ctx.cards.is_empty(),
    })
}

// ---------- counts ----------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TeacherCount {
    pub teacher_id: String,
    pub name: String,
    pub covers: u32,
    pub periods: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CoverCounts {
    /// The school year the counts belong to.
    pub year: String,
    /// Counted from this date (the last reset), or from the first cover.
    pub since: Option<String>,
    pub total: u32,
    pub total_periods: u32,
    /// Every teacher, most covers first.
    pub teachers: Vec<TeacherCount>,
    /// The academic year on the School page is not the one the counts were reset for.
    pub new_year: bool,
}

pub fn cover_counts(s: &SchoolSetup) -> CoverCounts {
    let by = cover_counts_by_teacher(s);
    let mut teachers: Vec<TeacherCount> = s
        .master
        .teachers
        .iter()
        .map(|t| {
            let (covers, periods) = by.get(&t.id).copied().unwrap_or((0, 0));
            TeacherCount { teacher_id: t.id.clone(), name: t.name.clone(), covers, periods }
        })
        .collect();
    teachers.sort_by(|a, b| b.covers.cmp(&a.covers).then(b.periods.cmp(&a.periods)).then(a.name.cmp(&b.name)));
    let sub = &s.substitutions;
    let year = sub.counts_year.clone().filter(|y| !y.is_empty()).unwrap_or_else(|| s.academic_year.clone());
    CoverCounts {
        since: sub.counts_from.clone(),
        new_year: sub.counts_year.as_ref().is_some_and(|y| !s.academic_year.trim().is_empty() && y != &s.academic_year),
        year,
        total: teachers.iter().map(|t| t.covers).sum(),
        total_periods: teachers.iter().map(|t| t.periods).sum(),
        teachers,
    }
}

// ---------- the cover sheet ----------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SheetRow {
    pub shift: String,
    pub period: String,
    pub start: String,
    pub end: String,
    pub classes: String,
    pub subject: String,
    pub absent: String,
    /// The cover teacher, "No cover needed", or "NOT COVERED YET".
    pub cover: String,
    pub covered: bool,
    pub room: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SheetDay {
    pub date: String,
    pub title: String,
    pub rows: Vec<SheetRow>,
}

pub const NOT_COVERED: &str = "NOT COVERED YET";
pub const NO_COVER: &str = "No cover needed";

/// One entry per date (from `from` to `to`) with at least one session needing cover.
pub fn cover_sheet(s: &SchoolSetup, from: &str, to: &str) -> Result<Vec<SheetDay>, String> {
    let (a, b) = (date_or_err(from)?, date_or_err(to)?);
    if b < a {
        return Err("The last date is before the first date.".into());
    }
    if b - a > 366 {
        return Err("Please choose at most one year at a time.".into());
    }
    let mut out = Vec::new();
    for n in a..=b {
        let day = cover_day(s, &format_date(n))?;
        if day.needs.is_empty() {
            continue;
        }
        let rows = day
            .needs
            .iter()
            .map(|x| SheetRow {
                shift: x.shift_name.clone(),
                period: if x.length > 1 { format!("{}-{}", x.period, x.period as u32 + x.length - 1) } else { x.period.to_string() },
                start: x.start.clone(),
                end: x.end.clone(),
                classes: x.classes.clone(),
                subject: x.subject.clone(),
                absent: x.absent_name.clone(),
                cover: if x.no_cover { NO_COVER.into() } else { x.cover_name.clone().unwrap_or_else(|| NOT_COVERED.into()) },
                covered: x.no_cover || x.cover_name.is_some(),
                room: x.room.clone(),
                note: x.note.clone(),
            })
            .collect();
        out.push(SheetDay { date: day.date, title: day.title, rows });
    }
    Ok(out)
}

pub fn cover_sheet_csv(s: &SchoolSetup, from: &str, to: &str) -> Result<String, String> {
    let days = cover_sheet(s, from, to)?;
    if days.is_empty() {
        return Err("No lessons need cover on these dates.".into());
    }
    let mut w = csv::Writer::from_writer(Vec::new());
    w.write_record(["Date", "Day", "Shift", "Period", "Start", "End", "Class", "Subject", "Absent teacher", "Cover teacher", "Room", "Note"])
        .map_err(|e| e.to_string())?;
    for d in &days {
        let n = parse_date(&d.date).unwrap_or(0);
        let day = s.days.get(weekday(n)).map_or("", |x| x.name.as_str());
        for r in &d.rows {
            w.write_record([&d.date, day, &r.shift, &r.period, &r.start, &r.end, &r.classes, &r.subject, &r.absent, &r.cover, &r.room, &r.note])
                .map_err(|e| e.to_string())?;
        }
    }
    let bytes = w.into_inner().map_err(|e| e.to_string())?;
    Ok(format!("\u{FEFF}{}", String::from_utf8_lossy(&bytes)))
}

/// Checks a newly entered absence (the page asks before adding it).
pub fn check_absence(s: &SchoolSetup, a: &Absence) -> Result<(), String> {
    if !s.master.teachers.iter().any(|t| t.id == a.teacher_id) {
        return Err("Please choose a teacher.".into());
    }
    let (from, to) = (date_or_err(&a.from)?, date_or_err(&a.to)?);
    if to < from {
        return Err("The last day of the absence is before the first day.".into());
    }
    if to - from > 366 {
        return Err("An absence can be at most one year long. Please add a longer one in parts.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timetable::lessons::{Lesson, LessonPart};
    use crate::timetable::master::*;
    use crate::timetable::solver::Timetable;

    const MON: &str = "2026-10-26";

    fn teacher(id: &str, shifts: &[&str]) -> Teacher {
        Teacher { id: id.into(), name: format!("Teacher {id}"), short_name: id.into(), shifts: shifts.iter().map(|s| s.to_string()).collect(), ..Default::default() }
    }
    fn lesson(id: &str, subject: &str, t: &str, class: &str, n: u32) -> Lesson {
        Lesson { id: id.into(), subject: subject.into(), teachers: vec![t.into()], parts: vec![LessonPart { class_id: class.into(), group_id: None }], periods_per_card: 1, cards_per_week: n, rooms: vec![] }
    }
    fn at(l: &str, number: u32, day: u8, period: u8) -> PlacedCard {
        PlacedCard { lesson_id: l.into(), number, shift: "morning".into(), day, period, length: 1, room: None, locked: false }
    }
    /// Maths teachers A, B, E; English teachers C and D (D works afternoons only).
    fn school() -> SchoolSetup {
        let mut s = SchoolSetup::default_new("Demo");
        s.academic_year = "2026-27".into();
        s.master.subjects = vec![
            Subject { id: "math".into(), name: "Mathematics".into(), short_name: "MA".into(), color: "#1F6F6B".into() },
            Subject { id: "eng".into(), name: "English".into(), short_name: "EN".into(), color: "#4C78A8".into() },
        ];
        s.master.teachers = vec![teacher("A", &["morning"]), teacher("B", &["morning"]), teacher("C", &["morning"]), teacher("D", &["afternoon"]), teacher("E", &["morning"])];
        s.master.classes = ["6A", "6B"].iter().map(|c| Class { id: c.to_string(), name: format!("Grade {c}"), short_name: c.to_string(), shift: "morning".into(), ..Default::default() }).collect();
        s.master.rooms = vec![Room { id: "r1".into(), name: "Room 1".into(), short_name: "R1".into(), ..Default::default() }];
        s.lessons = vec![lesson("L1", "math", "A", "6A", 2), lesson("L2", "math", "B", "6B", 1), lesson("L3", "eng", "C", "6B", 1), lesson("L4", "math", "E", "6B", 1), lesson("L5", "eng", "D", "6A", 1)];
        let mut l1 = at("L1", 1, 0, 1);
        l1.room = Some("r1".into());
        s.timetable = Some(Timetable { cards: vec![l1, at("L1", 2, 1, 1), at("L2", 1, 0, 1), at("L3", 1, 0, 2), at("L4", 1, 1, 3)] });
        s
    }
    fn absent(s: &mut SchoolSetup, id: &str, t: &str, from: &str, to: &str, periods: &[u8]) {
        s.substitutions.absences.push(Absence {
            id: id.into(), teacher_id: t.into(), from: from.into(), to: to.into(),
            periods: periods.iter().map(|p| PeriodRef { shift: "morning".into(), period: *p }).collect(), note: String::new(),
        });
    }
    fn cover(need: &Need, t: Option<&str>) -> Cover {
        Cover {
            absence_id: need.absence_id.clone(), date: need.date.clone(), lesson_id: need.lesson_id.clone(), number: need.number,
            absent_teacher: need.absent_teacher.clone(), cover_teacher: t.map(String::from), no_cover: t.is_none(),
            shift: need.shift.clone(), period: need.period, length: need.length, archived: false,
        }
    }
    fn order(n: &Need) -> Vec<(&str, bool)> {
        n.candidates.iter().map(|c| (c.teacher_id.as_str(), c.suitable)).collect()
    }
    fn reasons<'a>(n: &'a Need, t: &str) -> &'a [String] {
        &n.candidates.iter().find(|c| c.teacher_id == t).unwrap().reasons
    }

    #[test]
    fn dates_and_weekdays() {
        assert_eq!(weekday(parse_date(MON).unwrap()), 0);
        assert_eq!(weekday(parse_date("2026-11-01").unwrap()), 6);
        assert_eq!(weekday(parse_date("2000-02-29").unwrap()), 1);
        for bad in ["2026-02-29", "2026-13-01", "2026-00-10", "2026-10-32", "26-10-2026", "", "2026-1-05", "abcd-ef-gh"] {
            assert_eq!(parse_date(bad), None, "{bad}");
        }
        for d in ["1999-12-31", "2024-02-29", "2026-10-26", "2100-03-01"] {
            assert_eq!(format_date(parse_date(d).unwrap()), d);
        }
        assert_eq!(format_date(parse_date("2026-12-31").unwrap() + 1), "2027-01-01");
        assert_eq!(long_date(&school(), parse_date(MON).unwrap()), "Monday 26 October 2026");
        assert_eq!(format_date(monday_of(parse_date("2026-10-29").unwrap())), MON);
    }

    #[test]
    fn whole_day_absence_lists_the_sessions_and_ranks_same_subject_first() {
        let mut s = school();
        absent(&mut s, "a1", "A", MON, MON, &[]);
        let day = cover_day(&s, MON).unwrap();
        assert_eq!(day.title, "Monday 26 October 2026");
        assert_eq!(day.needs.len(), 1, "A teaches once on Monday");
        let n = &day.needs[0];
        assert_eq!((n.lesson_id.as_str(), n.number, n.period), ("L1", 1, 1));
        assert_eq!((n.classes.as_str(), n.subject.as_str(), n.room.as_str(), n.absent_name.as_str()), ("Grade 6A", "Mathematics", "Room 1", "Teacher A"));
        // no room of its own: the class's home room
        let mut h = s.clone();
        h.timetable.as_mut().unwrap().cards[0].room = None;
        assert_eq!(cover_day(&h, MON).unwrap().needs[0].room, "");
        h.master.classes[0].home_room = Some("r1".into());
        assert_eq!(cover_day(&h, MON).unwrap().needs[0].room, "Room 1");
        assert_eq!((n.start.as_str(), n.end.as_str()), ("08:00", "08:40"));
        // E (maths, free) first, then C (English, free); B is teaching then, D works afternoons
        assert_eq!(order(n), [("E", true), ("C", true), ("B", false), ("D", false)]);
        assert!(n.candidates[0].same_subject && !n.candidates[1].same_subject);
        assert_eq!(reasons(n, "B"), ["Teacher B is teaching Mathematics (Grade 6B) then."]);
        assert_eq!(reasons(n, "D"), ["Teacher D does not work in the morning shift."]);
        assert!(n.candidates.iter().all(|c| c.teacher_id != "A"), "the absent teacher is not offered");
        // Tuesday: one session; Saturday: not a working day
        assert_eq!(cover_day(&s, "2026-10-27").unwrap().needs.len(), 0, "the absence is Monday only");
        let sat = cover_day(&s, "2026-10-31").unwrap();
        assert!(!sat.working && sat.needs.is_empty());
    }

    #[test]
    fn a_date_range_and_part_day_absences() {
        let mut s = school();
        absent(&mut s, "a1", "A", MON, "2026-10-27", &[]);
        assert_eq!(cover_day(&s, "2026-10-27").unwrap().needs.len(), 1, "Tuesday is in the range");
        s.substitutions.absences.clear();
        // away only in period 2: the period-1 lesson needs no cover
        absent(&mut s, "a1", "A", MON, MON, &[2]);
        assert!(cover_day(&s, MON).unwrap().needs.is_empty());
        s.substitutions.absences[0].periods = vec![PeriodRef { shift: "morning".into(), period: 1 }];
        assert_eq!(cover_day(&s, MON).unwrap().needs.len(), 1);
        // a double period counts when either half is in the absence
        s.timetable.as_mut().unwrap().cards[0].length = 2;
        s.substitutions.absences[0].periods = vec![PeriodRef { shift: "morning".into(), period: 2 }];
        let day = cover_day(&s, MON).unwrap();
        assert_eq!(day.needs.len(), 1);
        assert_eq!(day.needs[0].end, "09:20");
    }

    #[test]
    fn absent_time_off_and_already_covering_teachers_are_not_suitable() {
        let mut s = school();
        absent(&mut s, "a1", "A", MON, MON, &[]);
        // C is away in period 1 only
        absent(&mut s, "c1", "C", MON, MON, &[1]);
        let n = cover_day(&s, MON).unwrap().needs.into_iter().find(|n| n.absent_teacher == "A").unwrap();
        assert_eq!(reasons(&n, "C"), ["Teacher C is away then."]);
        s.substitutions.absences.pop();
        // E has time off in period 1
        s.master.teachers[4].time_off = vec![TimeOffCell { shift: "morning".into(), day: 0, period: 1, state: 2 }];
        let n = &cover_day(&s, MON).unwrap().needs[0];
        assert_eq!(reasons(n, "E"), ["Teacher E is not available then (time off)."]);
        s.master.teachers[4].time_off.clear();
        // B is away too: C covers A's lesson, so C cannot also cover B's at the same time
        absent(&mut s, "b1", "B", MON, MON, &[]);
        let day = cover_day(&s, MON).unwrap();
        assert_eq!(day.needs.len(), 2);
        let for_a = day.needs.iter().find(|n| n.absent_teacher == "A").unwrap();
        s.substitutions.covers.push(cover(for_a, Some("C")));
        let day = cover_day(&s, MON).unwrap();
        let for_b = day.needs.iter().find(|n| n.absent_teacher == "B").unwrap();
        assert_eq!(reasons(for_b, "C"), ["Teacher C is already covering Mathematics (Grade 6A) then."]);
        // ... and the cover being made does not count against itself
        let for_a = day.needs.iter().find(|n| n.absent_teacher == "A").unwrap();
        assert!(for_a.candidates.iter().find(|c| c.teacher_id == "C").unwrap().suitable);
        assert_eq!(for_a.cover_name.as_deref(), Some("Teacher C"));
        assert_eq!(for_a.warning, None);
    }

    #[test]
    fn the_weekly_maximum_counts_lessons_and_covers_that_week() {
        let mut s = school();
        absent(&mut s, "a1", "A", MON, "2026-10-27", &[]);
        // E teaches 1 period a week; with a maximum of 2, one cover fits
        s.master.teachers[4].max_periods_per_week = Some(2);
        let n = cover_day(&s, MON).unwrap().needs.remove(0);
        assert!(n.candidates.iter().find(|c| c.teacher_id == "E").unwrap().suitable);
        s.substitutions.covers.push(cover(&n, Some("E")));
        // the second (Tuesday, same week) would make 3
        let tue = cover_day(&s, "2026-10-27").unwrap().needs.remove(0);
        assert_eq!(reasons(&tue, "E"), ["Teacher E would teach 3 periods that week, more than their weekly maximum of 2."]);
        // next week counts from zero again
        absent(&mut s, "a2", "A", "2026-11-02", "2026-11-02", &[]);
        let next = cover_day(&s, "2026-11-02").unwrap().needs.remove(0);
        assert!(next.candidates.iter().find(|c| c.teacher_id == "E").unwrap().suitable);
        // the school-wide default applies to teachers without their own
        s.master.teachers[4].max_periods_per_week = None;
        s.settings.default_max_per_week = Some(1);
        let n = cover_day(&s, MON).unwrap().needs.remove(0);
        assert!(!n.candidates.iter().find(|c| c.teacher_id == "E").unwrap().suitable);
    }

    #[test]
    fn fewest_covers_first_within_a_group_and_a_reset_starts_again() {
        let mut s = school();
        s.master.teachers.push(teacher("F", &["morning"]));
        s.lessons.push(lesson("L6", "math", "F", "6A", 0)); // F teaches maths too, nothing placed
        absent(&mut s, "a1", "A", MON, "2026-10-27", &[]);
        let n = cover_day(&s, MON).unwrap().needs.remove(0);
        assert_eq!(order(&n)[..3], [("E", true), ("F", true), ("C", true)], "same subject first; then by name");
        // E covered something earlier: F goes first now
        let tue = cover_day(&s, "2026-10-27").unwrap().needs.remove(0);
        s.substitutions.covers.push(cover(&tue, Some("E")));
        let n = cover_day(&s, MON).unwrap().needs.remove(0);
        assert_eq!(order(&n)[..3], [("F", true), ("E", true), ("C", true)]);
        assert_eq!(n.candidates[1].covers, 1);
        // counts reset after that cover: equal again
        reset_counts(&mut s.substitutions, "2026-10-28", "2026-27");
        let n = cover_day(&s, MON).unwrap().needs.remove(0);
        assert_eq!(order(&n)[..2], [("E", true), ("F", true)]);
        // among equals, less teaching that weekday goes first: on Tuesday B (free that day) and F
        // come before E, who teaches on Tuesday
        let tue = cover_day(&s, "2026-10-27").unwrap().needs.remove(0);
        assert_eq!(order(&tue)[..3], [("B", true), ("F", true), ("E", true)]);
    }

    #[test]
    fn a_chosen_teacher_who_breaks_a_rule_gets_a_warning() {
        let mut s = school();
        absent(&mut s, "a1", "A", MON, MON, &[]);
        let n = cover_day(&s, MON).unwrap().needs.remove(0);
        s.substitutions.covers.push(cover(&n, Some("B")));
        let n = cover_day(&s, MON).unwrap().needs.remove(0);
        assert_eq!(n.cover_teacher.as_deref(), Some("B"));
        assert_eq!(n.warning.as_deref(), Some("Teacher B is teaching Mathematics (Grade 6B) then."));
    }

    #[test]
    fn counts_per_school_year_with_reset() {
        let mut s = school();
        absent(&mut s, "a1", "A", MON, "2026-10-27", &[]);
        let mon = cover_day(&s, MON).unwrap().needs.remove(0);
        let tue = cover_day(&s, "2026-10-27").unwrap().needs.remove(0);
        s.substitutions.covers = vec![cover(&mon, Some("E")), cover(&tue, None)];
        let c = cover_counts(&s);
        assert_eq!((c.total, c.total_periods, c.year.as_str(), c.since.clone(), c.new_year), (1, 1, "2026-27", None, false));
        assert_eq!((c.teachers[0].teacher_id.as_str(), c.teachers[0].covers), ("E", 1));
        assert_eq!(c.teachers.len(), 5, "every teacher is listed");
        // "no cover needed" counts for nobody; a reset starts again at 0, even for covers of
        // lessons after the reset date that were decided before it
        reset_counts(&mut s.substitutions, "2026-10-20", "2026-27");
        let c = cover_counts(&s);
        assert_eq!((c.total, c.since.as_deref(), c.year.as_str()), (0, Some("2026-10-20"), "2026-27"));
        assert_eq!(s.substitutions.covers.len(), 2, "the covers themselves are kept");
        assert!(cover_sheet(&s, MON, MON).unwrap()[0].rows[0].covered);
        // a decision made after the reset counts
        s.substitutions.covers[0].archived = false;
        assert_eq!(cover_counts(&s).total, 1);
        s.substitutions.covers[0].archived = true;
        // the School page moves on to the next year: the page suggests a reset
        s.academic_year = "2027-28".into();
        let c = cover_counts(&s);
        assert!(c.new_year);
        assert_eq!(c.year, "2026-27");
    }

    #[test]
    fn removing_an_absence_takes_its_covers_out_of_the_counts() {
        let mut s = school();
        absent(&mut s, "a1", "A", MON, MON, &[]);
        let n = cover_day(&s, MON).unwrap().needs.remove(0);
        s.substitutions.covers.push(cover(&n, Some("E")));
        assert_eq!(cover_counts(&s).total, 1);
        s.substitutions.absences.clear(); // (the page also removes the covers themselves)
        assert_eq!(cover_counts(&s).total, 0);
        assert!(cover_day(&s, MON).unwrap().needs.is_empty());
    }

    #[test]
    fn a_cover_for_a_lesson_that_moved_is_shown_as_stale() {
        let mut s = school();
        absent(&mut s, "a1", "A", MON, MON, &[]);
        let n = cover_day(&s, MON).unwrap().needs.remove(0);
        s.substitutions.covers.push(cover(&n, Some("E")));
        // the timetable is built again and the lesson moves to Wednesday
        s.timetable.as_mut().unwrap().cards[0].day = 2;
        let day = cover_day(&s, MON).unwrap();
        assert!(day.needs.is_empty());
        assert_eq!(day.stale.len(), 1);
        assert_eq!(day.stale[0].cover_name.as_deref(), Some("Teacher E"));
        assert!(day.stale[0].title.contains("Mathematics (Grade 6A), period 1, for Teacher A"));
    }

    #[test]
    fn substitutions_never_change_the_timetable() {
        let mut s = school();
        let before = s.timetable.clone();
        absent(&mut s, "a1", "A", MON, "2026-10-30", &[]);
        for d in ["2026-10-26", "2026-10-27", "2026-10-28"] {
            for n in cover_day(&s, d).unwrap().needs {
                let best = n.candidates.first().map(|c| c.teacher_id.clone());
                s.substitutions.covers.push(cover(&n, best.as_deref()));
            }
        }
        cover_counts(&s);
        cover_sheet(&s, MON, "2026-10-30").unwrap();
        assert_eq!(s.timetable, before);
        assert_eq!(s.substitutions.covers.len(), 2);
    }

    #[test]
    fn cover_sheet_has_one_entry_per_day_and_a_csv() {
        let mut s = school();
        absent(&mut s, "a1", "A", MON, "2026-10-28", &[]);
        s.substitutions.absences[0].note = "Course".into();
        let mon = cover_day(&s, MON).unwrap().needs.remove(0);
        s.substitutions.covers.push(cover(&mon, Some("E")));
        let days = cover_sheet(&s, MON, "2026-10-28").unwrap();
        assert_eq!(days.len(), 2, "Wednesday: A teaches nothing, so no page");
        assert_eq!(days[0].title, "Monday 26 October 2026");
        assert_eq!((days[0].rows[0].cover.as_str(), days[0].rows[0].covered), ("Teacher E", true));
        assert_eq!((days[1].rows[0].cover.as_str(), days[1].rows[0].covered), (NOT_COVERED, false));
        let csv = cover_sheet_csv(&s, MON, "2026-10-28").unwrap();
        let lines: Vec<&str> = csv.trim_start_matches('\u{FEFF}').lines().collect();
        assert_eq!(lines[0], "Date,Day,Shift,Period,Start,End,Class,Subject,Absent teacher,Cover teacher,Room,Note");
        assert_eq!(lines[1], "2026-10-26,Monday,Morning,1,08:00,08:40,Grade 6A,Mathematics,Teacher A,Teacher E,Room 1,Course");
        assert_eq!(lines[2], "2026-10-27,Tuesday,Morning,1,08:00,08:40,Grade 6A,Mathematics,Teacher A,NOT COVERED YET,,Course");
        assert_eq!(lines.len(), 3);
        // no cover needed
        s.substitutions.covers[0] = cover(&mon, None);
        assert_eq!(cover_sheet(&s, MON, MON).unwrap()[0].rows[0].cover, NO_COVER);
        // bad ranges
        assert!(cover_sheet(&s, "2026-10-28", MON).unwrap_err().contains("before the first"));
        assert!(cover_sheet_csv(&s, "2026-11-02", "2026-11-03").unwrap_err().contains("No lessons need cover"));
        assert!(cover_sheet(&s, "x", MON).unwrap_err().contains("not a date"));
    }

    #[test]
    fn new_absences_are_checked() {
        let s = school();
        let a = |t: &str, from: &str, to: &str| Absence { id: "x".into(), teacher_id: t.into(), from: from.into(), to: to.into(), periods: vec![], note: String::new() };
        assert!(check_absence(&s, &a("A", MON, MON)).is_ok());
        assert!(check_absence(&s, &a("nobody", MON, MON)).unwrap_err().contains("choose a teacher"));
        assert!(check_absence(&s, &a("A", MON, "2026-10-01")).unwrap_err().contains("before the first day"));
        assert!(check_absence(&s, &a("A", "2026-02-30", MON)).unwrap_err().contains("not a date"));
        assert!(check_absence(&s, &a("A", "2026-01-01", "2027-06-01")).unwrap_err().contains("at most one year"));
    }
}
