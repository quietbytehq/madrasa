//! Pre-generation check (milestone 5, blueprint section 6).
//! Finds impossible input before the solver runs and explains it in plain English.
use crate::timetable::lessons::{class_used, free_slots, Lesson};
use crate::timetable::master::TimeOffCell;
use crate::timetable::model::{SchoolSetup, Shift};
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// "error" = the timetable cannot be built; "warning" = advice.
    pub level: String,
    /// setup | class_volume | teacher_volume | time_off | room_pool | no_lessons
    pub kind: String,
    pub title: String,
    pub message: String,
    pub fix: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CheckReport {
    pub errors: u32,
    pub warnings: u32,
    /// Lesson sessions to place (a double period is one session).
    pub cards: u32,
    /// Periods those sessions fill (a double period is two).
    pub periods: u32,
    pub findings: Vec<Finding>,
}

type Grid = Vec<Vec<bool>>; // [day][period index] -> usable

fn periods_in(shift: &Shift, di: usize) -> usize {
    shift.days.get(di).map_or(0, |p| p.len())
}

fn grid(s: &SchoolSetup, shift: &Shift, off: &[TimeOffCell], works_here: bool) -> Grid {
    (0..s.days.len())
        .map(|di| {
            let working = s.days[di].working && works_here;
            (0..periods_in(shift, di))
                .map(|pi| {
                    working
                        && !off.iter().any(|c| {
                            c.state == 2 && c.shift == shift.id && c.day as usize == di && c.period as usize == pi + 1
                        })
                })
                .collect()
        })
        .collect()
}

fn count(g: &Grid) -> u32 {
    g.iter().flatten().filter(|x| **x).count() as u32
}

fn and(a: &Grid, b: &Grid) -> Grid {
    a.iter()
        .zip(b)
        .map(|(x, y)| x.iter().zip(y).map(|(p, q)| *p && *q).collect())
        .collect()
}

fn or(a: &Grid, b: &Grid) -> Grid {
    a.iter()
        .zip(b)
        .map(|(x, y)| x.iter().zip(y).map(|(p, q)| *p || *q).collect())
        .collect()
}

/// Most sessions of `len` consecutive periods that fit in `dom` without overlapping
/// and without running across a "cannot be crossed" break.
fn max_sessions(dom: &Grid, shift: &Shift, len: usize) -> u32 {
    let mut total = 0;
    for day in dom {
        let mut p = 0;
        while p + len <= day.len() {
            let crosses = (p + 1..p + len).any(|k| shift.breaks.iter().any(|b| b.cannot_cross && b.after_period as usize == k));
            if day[p..p + len].iter().all(|x| *x) && !crosses {
                total += 1;
                p += len;
            } else {
                p += 1;
            }
        }
    }
    total
}

fn plural(n: u32, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn finding(level: &str, kind: &str, title: String, message: String, fix: &str) -> Finding {
    Finding { level: level.into(), kind: kind.into(), title, message, fix: fix.into() }
}

pub fn run(s: &SchoolSetup) -> CheckReport {
    let m = &s.master;
    let mut out: Vec<Finding> = Vec::new();

    // 0. Basic setup problems come first: the other checks assume they are fixed.
    for i in s.validate() {
        let title = if i.level == "error" { "Setup problem" } else { "Setup note" };
        out.push(finding(&i.level, "setup", title.into(), i.message, "Fix this in the step it belongs to."));
    }

    // 1 + 2. Class and teacher volume.
    for c in &m.classes {
        let Some(sh) = s.shifts.iter().find(|x| x.id == c.shift) else { continue };
        let used = class_used(c, &s.lessons);
        let avail = free_slots(s, sh, &c.time_off);
        if used > avail {
            out.push(finding(
                "error", "class_volume",
                format!("Class {} has too many lessons", c.name.trim()),
                format!("Class {} has {} of lessons each week, but only {} are available in the {} shift (after time off).",
                    c.name.trim(), plural(used, "period", "periods"), avail, sh.name.to_lowercase()),
                "Remove or shorten some of its lessons, or free up time-off cells for this class.",
            ));
        } else if used == 0 && !s.lessons.is_empty() {
            out.push(finding("warning", "no_lessons", format!("Class {} has no lessons", c.name.trim()),
                format!("No lesson is given to class {} yet.", c.name.trim()), "Add its lessons on the Lessons page."));
        }
    }
    for t in &m.teachers {
        let used: u32 = s.lessons.iter().filter(|l| l.teachers.contains(&t.id)).map(|l| l.periods_per_card * l.cards_per_week).sum();
        if used == 0 {
            continue;
        }
        let avail: u32 = s.shifts.iter().filter(|sh| sh.enabled && t.shifts.contains(&sh.id)).map(|sh| free_slots(s, sh, &t.time_off)).sum();
        let name = t.name.trim();
        if used > avail {
            out.push(finding(
                "error", "teacher_volume",
                format!("{name} has too many lessons"),
                format!("{name} is given {} each week, but is only free for {}.", plural(used, "period", "periods"), plural(avail, "period", "periods")),
                "Give some lessons to another teacher, or free up some of their time-off cells.",
            ));
        }
        if let Some(max) = s.settings.max_per_week(t) {
            if used > max {
                out.push(finding(
                    "error", "teacher_week",
                    format!("{name} has more lessons than their weekly maximum"),
                    format!("{name} is given {} each week, but the most they may teach is {max}.", plural(used, "period", "periods")),
                    "Give some lessons to another teacher, or raise their weekly maximum (Teachers, or Rules & limits for the school default).",
                ));
            }
        }
        if let Some(max) = s.settings.max_per_day(t) {
            let days = working_days(s, t);
            if days > 0 && used > max * days {
                out.push(finding(
                    "error", "teacher_day",
                    format!("{name} cannot fit their lessons into the daily maximum"),
                    format!("{name} is given {} each week. With at most {max} a day over {} that is only {} possible.",
                        plural(used, "period", "periods"), plural(days, "working day", "working days"), max * days),
                    "Give some lessons to another teacher, or raise their daily maximum.",
                ));
            }
        }
    }

    // 2b. Lessons that must happen at the same time.
    for (i, link) in s.links.iter().enumerate() {
        if let Some(f) = link_bottleneck(s, link, i) {
            out.push(f);
        }
    }

    // 3. Time-off bottleneck for every lesson.
    for (i, l) in s.lessons.iter().enumerate() {
        if let Some(f) = lesson_bottleneck(s, l, i) {
            out.push(f);
        }
    }

    // 4. Room pool saturation.
    room_pools(s, &mut out);

    let errors = out.iter().filter(|f| f.level == "error").count() as u32;
    let warnings = out.len() as u32 - errors;
    CheckReport { errors, warnings, cards: crate::timetable::lessons::generate_cards(&s.lessons).len() as u32, periods: s.lessons.iter().map(|l| l.periods_per_card * l.cards_per_week).sum(), findings: out }
}

/// Working days a teacher can teach on (days with periods in a shift of theirs), capped by their day limit.
fn working_days(s: &SchoolSetup, t: &crate::timetable::master::Teacher) -> u32 {
    let days = (0..s.days.len())
        .filter(|&d| s.days[d].working && s.shifts.iter().any(|sh| sh.enabled && t.shifts.contains(&sh.id) && sh.days.get(d).map_or(false, |p| !p.is_empty())))
        .count() as u32;
    t.max_days_per_week.map_or(days, |m| m.min(days))
}

/// Can the lessons of a "same time" link ever meet? Everyone in them must be free together.
fn link_bottleneck(s: &SchoolSetup, link: &crate::timetable::links::Link, idx: usize) -> Option<Finding> {
    let m = &s.master;
    if let Some(p) = crate::timetable::links::problem(s, &link.lessons) {
        return Some(finding("error", "link", format!("Linked lessons {} cannot work", idx + 1),
            format!("These lessons are set to happen at the same time, but cannot: {p}."), "Fix or remove the link in the lesson editor."));
    }
    let ls: Vec<&Lesson> = link.lessons.iter().filter_map(|id| s.lessons.iter().find(|l| &l.id == id)).collect();
    let first = ls.first()?;
    let shift = s.shifts.iter().find(|x| m.classes.iter().find(|c| Some(&c.id) == first.parts.first().map(|p| &p.class_id)).map(|c| &c.shift) == Some(&x.id))?;
    let mut dom = grid(s, shift, &[], true);
    let mut names = Vec::new();
    for l in &ls {
        names.push(crate::timetable::links::lesson_title(s, &l.id));
        for t in l.teachers.iter().filter_map(|id| m.teachers.iter().find(|t| &t.id == id)) {
            dom = and(&dom, &grid(s, shift, &t.time_off, true));
        }
        for c in l.parts.iter().filter_map(|p| m.classes.iter().find(|c| c.id == p.class_id)) {
            dom = and(&dom, &grid(s, shift, &c.time_off, true));
        }
        let rooms: Vec<_> = l.rooms.iter().filter_map(|id| m.rooms.iter().find(|r| &r.id == id)).collect();
        if !rooms.is_empty() {
            let mut any = grid(s, shift, &[], false);
            for r in &rooms {
                any = or(&any, &grid(s, shift, &r.time_off, true));
            }
            dom = and(&dom, &any);
        }
    }
    let need = first.cards_per_week * first.periods_per_card;
    let free = count(&dom);
    let fit = if first.periods_per_card > 1 { max_sessions(&dom, shift, first.periods_per_card as usize) } else { free };
    if free < need || fit < first.cards_per_week {
        return Some(finding("error", "link", format!("Linked lessons {} cannot all meet", idx + 1),
            format!("{} must happen at the same time, needing {} a week, but everyone involved is free together for only {}.",
                names.join(" + "), plural(need, "period", "periods"), plural(if first.periods_per_card > 1 { fit * first.periods_per_card } else { free }, "period", "periods")),
            "Free up time-off cells for the teachers or classes involved, or reduce the sessions."));
    }
    None
}

fn lesson_bottleneck(s: &SchoolSetup, l: &Lesson, idx: usize) -> Option<Finding> {
    let m = &s.master;
    let classes: Vec<_> = l.parts.iter().filter_map(|p| m.classes.iter().find(|c| c.id == p.class_id)).collect();
    let first = classes.first()?;
    let shift = s.shifts.iter().find(|x| x.id == first.shift)?;
    let need = l.periods_per_card * l.cards_per_week;
    if need == 0 || !(1..=3).contains(&l.periods_per_card) {
        return None;
    }
    let subject = m.subjects.iter().find(|x| x.id == l.subject).map_or("a lesson", |x| x.name.trim());
    let who = classes.iter().map(|c| c.name.trim().to_string()).collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>().join(", ");
    let label = format!("Lesson {} ({subject}, {who})", idx + 1);
    let title = format!("{subject} for {who} cannot fit");

    // Everyone who must be there at once: (name, grid).
    let mut people: Vec<(String, Grid)> = Vec::new();
    for t in l.teachers.iter().filter_map(|id| m.teachers.iter().find(|t| &t.id == id)) {
        let here = t.shifts.contains(&shift.id);
        if !here {
            return Some(finding("error", "time_off", title,
                format!("{label}: {} does not work in the {} shift, but the class does.", t.name.trim(), shift.name.to_lowercase()),
                "Add that shift to the teacher, or choose another teacher."));
        }
        people.push((t.name.trim().to_string(), grid(s, shift, &t.time_off, true)));
    }
    for c in &classes {
        people.push((format!("class {}", c.name.trim()), grid(s, shift, &c.time_off, true)));
    }
    let mut dom = grid(s, shift, &[], true);
    for (_, g) in &people {
        dom = and(&dom, g);
    }
    // At least one candidate room must be free.
    let rooms: Vec<_> = l.rooms.iter().filter_map(|id| m.rooms.iter().find(|r| &r.id == id)).collect();
    if !rooms.is_empty() {
        let mut any = grid(s, shift, &[], false);
        for r in &rooms {
            any = or(&any, &grid(s, shift, &r.time_off, true));
        }
        dom = and(&dom, &any);
    }

    let free = count(&dom);
    let blocker = || {
        people
            .iter()
            .min_by_key(|(_, g)| count(g))
            .map(|(n, g)| format!("{n} has the fewest free periods ({}).", count(g)))
            .unwrap_or_default()
    };
    if free < need {
        return Some(finding("error", "time_off", title,
            format!("{label} needs {} a week, but only {} free at the same time for everyone involved. {}",
                plural(need, "period", "periods"), plural(free, "period is", "periods are"), blocker()),
            "Free up some time-off cells for the teacher, class or room named, or reduce the lesson's periods."));
    }
    if l.periods_per_card > 1 {
        let fit = max_sessions(&dom, shift, l.periods_per_card as usize);
        if fit < l.cards_per_week {
            let kind = if l.periods_per_card == 2 { "double" } else { "triple" };
            return Some(finding("error", "time_off", title,
                format!("{label} needs {} {kind} periods a week, but only {fit} will fit side by side (breaks cannot be crossed, and time off splits the free periods). {}",
                    l.cards_per_week, blocker()),
                "Free up neighbouring periods, allow the break to be crossed, or make some sessions single periods."));
        }
    }
    None
}

fn room_pools(s: &SchoolSetup, out: &mut Vec<Finding>) {
    let m = &s.master;
    let mut seen: BTreeSet<(String, Vec<String>)> = BTreeSet::new();
    let shift_of = |l: &Lesson| {
        l.parts.first().and_then(|p| m.classes.iter().find(|c| c.id == p.class_id)).and_then(|c| s.shifts.iter().find(|x| x.id == c.shift))
    };
    for l in s.lessons.iter().filter(|l| !l.rooms.is_empty()) {
        let Some(sh) = shift_of(l) else { continue };
        let pool: Vec<String> = l.rooms.iter().cloned().collect::<BTreeSet<_>>().into_iter().collect();
        if !seen.insert((sh.id.clone(), pool.clone())) {
            continue;
        }
        // Every lesson that can only use rooms from this pool competes for it.
        let demand: u32 = s
            .lessons
            .iter()
            .filter(|o| !o.rooms.is_empty() && o.rooms.iter().all(|r| pool.contains(r)) && shift_of(o).map(|x| &x.id) == Some(&sh.id))
            .map(|o| o.periods_per_card * o.cards_per_week)
            .sum();
        let rooms: Vec<_> = pool.iter().filter_map(|id| m.rooms.iter().find(|r| &r.id == id)).collect();
        let capacity: u32 = rooms.iter().map(|r| free_slots(s, sh, &r.time_off)).sum();
        if demand > capacity {
            let names = rooms.iter().map(|r| r.name.trim().to_string()).collect::<Vec<_>>().join(", ");
            out.push(finding("error", "room_pool", format!("Not enough room time: {names}"),
                format!("Lessons that can only use {names} need {} a week, but these rooms offer {} in the {} shift.",
                    plural(demand, "period", "periods"), plural(capacity, "period", "periods"), sh.name.to_lowercase()),
                "Allow more rooms for these lessons, add a room, or reduce the periods."));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timetable::lessons::LessonPart;
    use crate::timetable::master::{Class, Room, Subject, Teacher};

    fn school() -> SchoolSetup {
        let mut s = SchoolSetup::default_new("T");
        s.master.subjects.push(Subject { id: "m".into(), name: "Maths".into(), short_name: "MA".into(), color: "#000".into() });
        s.master.teachers.push(Teacher { id: "t".into(), name: "Mr Ali".into(), short_name: "AL".into(), shifts: vec!["morning".into()], ..Default::default() });
        s.master.classes.push(Class { id: "c".into(), name: "5A".into(), short_name: "5A".into(), shift: "morning".into(), ..Default::default() });
        s.master.rooms.push(Room { id: "r".into(), name: "Lab".into(), short_name: "LB".into(), ..Default::default() });
        s
    }
    fn lesson(ppc: u32, cpw: u32) -> Lesson {
        Lesson { id: "l".into(), subject: "m".into(), teachers: vec!["t".into()],
            parts: vec![LessonPart { class_id: "c".into(), group_id: None }], periods_per_card: ppc, cards_per_week: cpw, rooms: vec![] }
    }
    fn off(day: u8, period: u8) -> TimeOffCell {
        TimeOffCell { shift: "morning".into(), day, period, state: 2 }
    }
    fn kinds(r: &CheckReport) -> Vec<&str> {
        r.findings.iter().map(|f| f.kind.as_str()).collect()
    }

    #[test]
    fn healthy_school_has_no_findings() {
        let mut s = school();
        s.lessons = vec![lesson(1, 5)];
        let r = run(&s);
        assert_eq!((r.errors, r.warnings), (0, 0), "{:?}", r.findings);
    }

    #[test]
    fn overloaded_class_is_an_error() {
        let mut s = school();
        s.lessons = vec![lesson(1, 34)]; // 33 available
        let r = run(&s);
        assert!(kinds(&r).contains(&"class_volume"));
        assert!(r.findings.iter().any(|f| f.message.contains("34 periods")));
    }

    #[test]
    fn overloaded_teacher_names_the_teacher() {
        let mut s = school();
        s.master.teachers[0].time_off = (0..5).flat_map(|d| (1..=7).map(move |p| off(d, p))).collect();
        s.lessons = vec![lesson(1, 2)];
        let r = run(&s);
        let f = r.findings.iter().find(|f| f.kind == "teacher_volume").expect("teacher finding");
        assert!(f.title.contains("Mr Ali"));
        assert!(f.message.contains("only free for 0 periods"));
    }

    #[test]
    fn time_off_overlap_bottleneck() {
        let mut s = school();
        // Teacher free only Monday periods 1-2; class free only Tuesday: nothing shared.
        s.master.teachers[0].time_off = (0..5).flat_map(|d| (1..=7).filter(move |p| !(d == 0 && *p <= 2)).map(move |p| off(d, p))).collect();
        s.master.classes[0].time_off = (0..5).flat_map(|d| (1..=7).filter(move |_| d != 1).map(move |p| off(d, p))).collect();
        s.lessons = vec![lesson(1, 1)];
        let r = run(&s);
        let f = r.findings.iter().find(|f| f.kind == "time_off").expect("bottleneck");
        assert!(f.message.contains("only 0 periods are free"));
    }

    #[test]
    fn doubles_cannot_cross_break_or_gaps() {
        let mut s = school();
        // Only periods 3 and 4 open, but a no-cross break sits between them.
        let all: Vec<_> = (0..5).flat_map(|d| (1..=7).filter(|p| *p != 3 && *p != 4).map(move |p| off(d, p))).collect();
        s.master.classes[0].time_off = all;
        s.lessons = vec![lesson(2, 1)];
        let r = run(&s);
        assert!(r.findings.iter().any(|f| f.message.contains("double periods")), "{:?}", r.findings);
    }

    #[test]
    fn room_pool_saturation() {
        let mut s = school();
        s.master.rooms[0].time_off = (0..5).flat_map(|d| (1..=7).filter(move |p| !(d == 0 && *p <= 2)).map(move |p| off(d, p))).collect();
        let mut l = lesson(1, 3);
        l.rooms = vec!["r".into()];
        s.lessons = vec![l];
        let r = run(&s);
        assert!(kinds(&r).contains(&"room_pool"));
    }

    #[test]
    fn teacher_in_wrong_shift_is_explained() {
        let mut s = school();
        s.master.teachers[0].shifts = vec!["afternoon".into()];
        s.lessons = vec![lesson(1, 2)];
        let r = run(&s);
        assert!(r.findings.iter().any(|f| f.message.contains("does not work in the morning shift")));
    }
}
