//! Independent checker for the hard rules of blueprint section 5.1 and the school's own
//! limits. It shares no placement code with the solver, so tests can trust it; only the count of usable days per lesson comes from the solver's model, so both always agree on it.
use super::Timetable;
use crate::timetable::master::TimeOffCell;
use crate::timetable::model::SchoolSetup;
use std::collections::HashMap;

fn state(cells: &[TimeOffCell], shift: &str, day: usize, period: usize) -> u8 {
    cells.iter().find(|c| c.shift == shift && c.day as usize == day && c.period as usize == period).map_or(0, |c| c.state)
}

/// Which bendable rules are enforced. Everything else is always checked.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Enforce {
    pub same_day: bool,
    pub subject_day: bool,
    pub consecutive: bool,
    pub per_day: bool,
}

impl Enforce {
    pub const ALL: Enforce = Enforce { same_day: true, subject_day: true, consecutive: true, per_day: true };
}

/// Returns a list of broken rules (empty = the timetable is valid).
pub fn check(s: &SchoolSetup, t: &Timetable) -> Vec<String> {
    check_with(s, t, Enforce::ALL)
}

pub fn check_with(s: &SchoolSetup, t: &Timetable, enforce: Enforce) -> Vec<String> {
    let mut bad = Vec::new();
    let m = &s.master;
    // On how many days each lesson can really happen: taken from the solver's model so the two never disagree.
    let usable_days = super::model::usable_days_by_lesson(s);
    let mut teacher_at: HashMap<(String, String, usize, usize), String> = HashMap::new();
    let mut room_at: HashMap<(String, String, usize, usize), String> = HashMap::new();
    let mut class_at: HashMap<(String, String, usize, usize), Vec<(u64, String)>> = HashMap::new();
    let mut lesson_days: HashMap<(String, usize), usize> = HashMap::new();
    let mut seen: HashMap<(String, u32), usize> = HashMap::new();
    let mut teacher_day: HashMap<(String, usize), Vec<usize>> = HashMap::new();
    let mut at: HashMap<(String, u32), (usize, usize)> = HashMap::new();
    // (class, bucket bit, subject, day) -> sessions
    let mut subj_day: HashMap<(String, u32, String, usize), u32> = HashMap::new();

    for pc in &t.cards {
        let tag = format!("{} #{}", pc.lesson_id, pc.number);
        *seen.entry((pc.lesson_id.clone(), pc.number)).or_default() += 1;
        let Some(l) = s.lessons.iter().find(|l| l.id == pc.lesson_id) else {
            bad.push(format!("{tag}: unknown lesson"));
            continue;
        };
        let Some(sh) = s.shifts.iter().find(|x| x.id == pc.shift) else {
            bad.push(format!("{tag}: unknown shift"));
            continue;
        };
        let (d, p0) = (pc.day as usize, pc.period as usize);
        at.insert((l.id.clone(), pc.number), (d, p0));
        let nper = sh.days.get(d).map_or(0, |x| x.len());
        if !s.days.get(d).map_or(false, |x| x.working) {
            bad.push(format!("{tag}: placed on a non-working day"));
        }
        if pc.length != l.periods_per_card {
            bad.push(format!("{tag}: wrong length"));
        }
        if p0 < 1 || p0 - 1 + pc.length as usize > nper {
            bad.push(format!("{tag}: runs past the end of the day"));
            continue;
        }
        for k in 1..pc.length as usize {
            if sh.breaks.iter().any(|b| b.cannot_cross && b.after_period as usize == p0 - 1 + k) {
                bad.push(format!("{tag}: crosses a break that cannot be crossed"));
            }
        }
        *lesson_days.entry((l.id.clone(), d)).or_default() += 1;
        for k in 0..pc.length as usize {
            let p = p0 + k; // 1-based
            for tid in &l.teachers {
                if let Some(te) = m.teachers.iter().find(|x| &x.id == tid) {
                    if state(&te.time_off, &sh.id, d, p) == 2 {
                        bad.push(format!("{tag}: teacher {} is off at day {d} period {p}", te.name));
                    }
                    if !te.shifts.contains(&sh.id) {
                        bad.push(format!("{tag}: teacher {} does not work in shift {}", te.name, sh.id));
                    }
                }
                if let Some(prev) = teacher_at.insert((tid.clone(), sh.id.clone(), d, p), tag.clone()) {
                    bad.push(format!("{tag}: teacher {tid} double-booked with {prev} at day {d} period {p}"));
                }
            }
            if let Some(r) = &pc.room {
                if !l.rooms.contains(r) {
                    bad.push(format!("{tag}: room {r} is not in the lesson's room pool"));
                }
                if let Some(ro) = m.rooms.iter().find(|x| &x.id == r) {
                    if state(&ro.time_off, &sh.id, d, p) == 2 {
                        bad.push(format!("{tag}: room {r} is off at day {d} period {p}"));
                    }
                }
                if let Some(prev) = room_at.insert((r.clone(), sh.id.clone(), d, p), tag.clone()) {
                    bad.push(format!("{tag}: room {r} double-booked with {prev} at day {d} period {p}"));
                }
            }
            for part in &l.parts {
                let Some(c) = m.classes.iter().find(|x| x.id == part.class_id) else { continue };
                if state(&c.time_off, &sh.id, d, p) == 2 {
                    bad.push(format!("{tag}: class {} is off at day {d} period {p}", c.name));
                }
                let mask = part_mask(c, part.group_id.as_deref());
                let e = class_at.entry((c.id.clone(), sh.id.clone(), d, p)).or_default();
                if let Some((_, prev)) = e.iter().find(|(m2, _)| m2 & mask != 0) {
                    bad.push(format!("{tag}: class {} overlaps {prev} at day {d} period {p}", c.name));
                }
                e.push((mask, tag.clone()));
            }
        }
        // sessions per subject per bucket per day (once per session, not per period)
        if !l.subject.is_empty() {
            for part in &l.parts {
                let Some(c) = m.classes.iter().find(|x| x.id == part.class_id) else { continue };
                let mask = part_mask(c, part.group_id.as_deref());
                for b in 0..64u32 {
                    if mask >> b & 1 == 1 {
                        *subj_day.entry((c.id.clone(), b, l.subject.clone(), d)).or_default() += 1;
                    }
                }
            }
        }
        for tid in &l.teachers {
            teacher_day.entry((tid.clone(), d)).or_default().extend((0..pc.length as usize).map(|k| p0 + k + 100 * s.shifts.iter().position(|x| x.id == sh.id).unwrap_or(0)));
        }
    }

    // every card at most once; same-lesson-per-day rule
    for l in &s.lessons {
        for n in 1..=l.cards_per_week {
            if let Some(k) = seen.get(&(l.id.clone(), n)).filter(|k| **k != 1) {
                bad.push(format!("{} #{n}: placed {k} times", l.id));
            }
        }
        if !enforce.same_day {
            continue;
        }
        if l.cards_per_week <= usable_days.get(&l.id).copied().unwrap_or(0) {
            for d in 0..s.days.len() {
                if lesson_days.get(&(l.id.clone(), d)).copied().unwrap_or(0) > 1 {
                    bad.push(format!("{}: two sessions on day {d}", l.id));
                }
            }
        }
    }

    // linked lessons: same day and period
    for k in &s.links {
        let cpw = k.lessons.iter().filter_map(|id| s.lessons.iter().find(|l| &l.id == id)).map(|l| l.cards_per_week).max().unwrap_or(0);
        for n in 1..=cpw {
            let spots: Vec<Option<&(usize, usize)>> = k.lessons.iter().map(|id| at.get(&(id.clone(), n))).collect();
            let first = spots.iter().flatten().next();
            if let Some(f) = first {
                if spots.iter().any(|x| x.map_or(true, |y| y != *f)) {
                    bad.push(format!("linked lessons {:?} session {n} are not at the same time", k.lessons));
                }
            }
        }
    }

    // most sessions of one subject per class bucket per day
    let mut totals: HashMap<(String, u32, String), u32> = HashMap::new();
    let mut usable_min: HashMap<(String, String), u32> = HashMap::new();
    for l in &s.lessons {
        if l.subject.is_empty() {
            continue;
        }
        let u = usable_days.get(&l.id).copied().unwrap_or(0);
        for part in &l.parts {
            let Some(c) = m.classes.iter().find(|x| x.id == part.class_id) else { continue };
            let e = usable_min.entry((c.id.clone(), l.subject.clone())).or_insert(u);
            *e = (*e).min(u);
            let mask = part_mask(c, part.group_id.as_deref());
            for b in 0..64u32 {
                if mask >> b & 1 == 1 {
                    *totals.entry((c.id.clone(), b, l.subject.clone())).or_default() += l.cards_per_week;
                }
            }
        }
    }
    if enforce.subject_day {
        for ((cid, bit, subj, d), n) in &subj_day {
            let c = m.classes.iter().find(|x| &x.id == cid).unwrap();
            let days = s.shifts.iter().find(|x| x.id == c.shift).map_or(1, |sh| {
                (0..s.days.len()).filter(|&i| s.days[i].working && sh.days.get(i).map_or(false, |p| !p.is_empty())).count().max(1)
            }) as u32;
            let days = days.min(usable_min.get(&(cid.clone(), subj.clone())).copied().unwrap_or(u32::MAX)).max(1);
            let total = totals.get(&(cid.clone(), *bit, subj.clone())).copied().unwrap_or(0);
            let limit = s.settings.max_same_subject_per_day.max(1).max((total + days - 1) / days);
            if *n > limit {
                bad.push(format!("class {} has {n} sessions of {subj} on day {d} (max {limit})", c.name));
            }
        }
    }

    // teacher limits
    for te in &m.teachers {
        let mut days_used = 0;
        let mut week = 0u32;
        for d in 0..s.days.len() {
            let Some(ps) = teacher_day.get_mut(&(te.id.clone(), d)) else { continue };
            ps.sort_unstable();
            days_used += 1;
            week += ps.len() as u32;
            if enforce.per_day {
                if let Some(max) = s.settings.max_per_day(te) {
                    if ps.len() as u32 > max {
                        bad.push(format!("{}: {} periods on day {d} (max {max})", te.name, ps.len()));
                    }
                }
            }
            if enforce.consecutive {
                if let Some(max) = te.max_consecutive {
                    let (mut run, mut best) = (1u32, 1u32);
                    for w in ps.windows(2) {
                        run = if w[1] == w[0] + 1 { run + 1 } else { 1 };
                        best = best.max(run);
                    }
                    if best > max {
                        bad.push(format!("{}: {best} lessons in a row on day {d} (max {max})", te.name));
                    }
                }
            }
        }
        if let Some(max) = s.settings.max_per_week(te) {
            if week > max {
                bad.push(format!("{}: {week} periods in the week (max {max})", te.name));
            }
        }
        if let Some(max) = te.max_days_per_week {
            if days_used > max {
                bad.push(format!("{}: works {days_used} days (max {max})", te.name));
            }
        }
    }
    bad
}


fn part_mask(c: &crate::timetable::master::Class, group: Option<&str>) -> u64 {
    match super::class_masks(c) {
        Some((atoms, groups)) => match group {
            None => if atoms >= 64 { u64::MAX } else { (1u64 << atoms) - 1 },
            Some(g) => groups.iter().find(|(id, _)| id == g).map_or(0, |x| x.1),
        },
        None => u64::MAX,
    }
}
