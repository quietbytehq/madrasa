//! Lessons (milestone 3): who teaches what to whom, how often, and where.
use crate::timetable::master::{Class, MasterData, TimeOffCell};
use crate::timetable::model::{Issue, SchoolSetup, Shift};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// One class taking part in a lesson, either as a whole or one group of it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct LessonPart {
    pub class_id: String,
    /// None = the whole class.
    #[serde(default)]
    pub group_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Lesson {
    pub id: String,
    pub subject: String,
    #[serde(default)]
    pub teachers: Vec<String>,
    #[serde(default)]
    pub parts: Vec<LessonPart>,
    /// Periods in a row per session: 1 = single, 2 = double, 3 = triple.
    pub periods_per_card: u32,
    /// Sessions per week.
    pub cards_per_week: u32,
    /// Rooms this lesson may use. Empty = the class's home room / any room.
    #[serde(default)]
    pub rooms: Vec<String>,
}

/// One session of a lesson that will later be placed on the timetable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    pub lesson_id: String,
    /// 1-based number of this session within the week.
    pub number: u32,
    pub length: u32,
}

pub fn generate_cards(lessons: &[Lesson]) -> Vec<Card> {
    lessons
        .iter()
        .flat_map(|l| {
            (1..=l.cards_per_week).map(move |n| Card { lesson_id: l.id.clone(), number: n, length: l.periods_per_card })
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Load {
    pub id: String,
    pub name: String,
    pub used: u32,
    pub available: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct LoadSummary {
    pub cards: u32,
    pub classes: Vec<Load>,
    pub teachers: Vec<Load>,
}

/// Periods in a shift on working days that are not forbidden by `off`.
pub(crate) fn free_slots(s: &SchoolSetup, shift: &Shift, off: &[TimeOffCell]) -> u32 {
    let mut n = 0;
    for (di, day) in s.days.iter().enumerate() {
        if !day.working {
            continue;
        }
        for pi in 0..shift.days.get(di).map_or(0, |p| p.len()) {
            let forbidden = off.iter().any(|c| {
                c.state == 2 && c.shift == shift.id && c.day as usize == di && c.period as usize == pi + 1
            });
            if !forbidden {
                n += 1;
            }
        }
    }
    n
}

pub(crate) fn class_used(c: &Class, lessons: &[Lesson]) -> u32 {
    let mut whole = 0;
    // Groups of one split can run side by side, so only the busiest group counts.
    let mut per_group: HashMap<&str, u32> = HashMap::new();
    for l in lessons {
        for p in l.parts.iter().filter(|p| p.class_id == c.id) {
            let n = l.periods_per_card * l.cards_per_week;
            match &p.group_id {
                None => whole += n,
                Some(g) => *per_group.entry(g.as_str()).or_default() += n,
            }
        }
    }
    let split: u32 = c
        .divisions
        .iter()
        .map(|d| d.groups.iter().map(|g| per_group.get(g.id.as_str()).copied().unwrap_or(0)).max().unwrap_or(0))
        .sum();
    whole + split
}

pub fn summarize(s: &SchoolSetup) -> LoadSummary {
    let m = &s.master;
    let classes = m
        .classes
        .iter()
        .map(|c| Load {
            id: c.id.clone(),
            name: c.name.clone(),
            used: class_used(c, &s.lessons),
            available: s.shifts.iter().find(|x| x.id == c.shift).map_or(0, |sh| free_slots(s, sh, &c.time_off)),
        })
        .collect();
    let teachers = m
        .teachers
        .iter()
        .map(|t| Load {
            id: t.id.clone(),
            name: t.name.clone(),
            used: s
                .lessons
                .iter()
                .filter(|l| l.teachers.contains(&t.id))
                .map(|l| l.periods_per_card * l.cards_per_week)
                .sum(),
            available: {
                let free: u32 = s
                    .shifts
                    .iter()
                    .filter(|sh| sh.enabled && t.shifts.contains(&sh.id))
                    .map(|sh| free_slots(s, sh, &t.time_off))
                    .sum();
                s.settings.max_per_week(t).map_or(free, |max| free.min(max))
            },
        })
        .collect();
    LoadSummary { cards: generate_cards(&s.lessons).len() as u32, classes, teachers }
}

fn err(message: String) -> Issue {
    Issue { level: "error".into(), message }
}

pub fn validate(lessons: &[Lesson], m: &MasterData) -> Vec<Issue> {
    let mut out = Vec::new();
    let subj = |id: &str| m.subjects.iter().find(|x| x.id == id);
    for (i, l) in lessons.iter().enumerate() {
        let label = format!(
            "Lesson {} ({})",
            i + 1,
            subj(&l.subject).map_or("no subject", |s| s.name.trim())
        );
        if subj(&l.subject).is_none() {
            out.push(err(format!("{label} has no subject.")));
        }
        if l.parts.is_empty() {
            out.push(err(format!("{label} has no class. Choose who attends it.")));
        }
        if l.teachers.is_empty() {
            out.push(Issue { level: "warning".into(), message: format!("{label} has no teacher yet.") });
        }
        if l.cards_per_week == 0 {
            out.push(err(format!("{label} must be held at least once a week.")));
        }
        if !(1..=3).contains(&l.periods_per_card) {
            out.push(err(format!("{label}: a session can be 1, 2 or 3 periods long.")));
        }
        for t in &l.teachers {
            if !m.teachers.iter().any(|x| &x.id == t) {
                out.push(err(format!("{label} uses a teacher who no longer exists.")));
            }
        }
        for r in &l.rooms {
            if !m.rooms.iter().any(|x| &x.id == r) {
                out.push(err(format!("{label} uses a room that no longer exists.")));
            }
        }
        let mut seen = HashSet::new();
        let mut shifts = HashSet::new();
        for p in &l.parts {
            let Some(c) = m.classes.iter().find(|c| c.id == p.class_id) else {
                out.push(err(format!("{label} uses a class that no longer exists.")));
                continue;
            };
            shifts.insert(c.shift.as_str());
            if let Some(g) = &p.group_id {
                if !c.divisions.iter().any(|d| d.groups.iter().any(|x| &x.id == g)) {
                    out.push(err(format!("{label}: a group of class \"{}\" no longer exists.", c.name.trim())));
                }
            }
            if !seen.insert((p.class_id.as_str(), p.group_id.as_deref())) {
                out.push(err(format!("{label} lists class \"{}\" more than once.", c.name.trim())));
            }
        }
        if shifts.len() > 1 {
            out.push(err(format!("{label} mixes classes from different shifts, so they can never meet.")));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timetable::master::{Division, Group, Subject, Teacher};

    fn school() -> SchoolSetup {
        let mut s = SchoolSetup::default_new("T");
        s.master.subjects.push(Subject { id: "m".into(), name: "Maths".into(), short_name: "MA".into(), color: "#000".into() });
        s.master.teachers.push(Teacher { id: "t".into(), name: "A".into(), short_name: "A".into(), shifts: vec!["morning".into()], ..Default::default() });
        s.master.classes.push(Class {
            id: "c".into(), name: "5A".into(), short_name: "5A".into(), shift: "morning".into(),
            divisions: vec![Division { id: "d".into(), name: "Split".into(),
                groups: vec![Group { id: "g1".into(), name: "B".into() }, Group { id: "g2".into(), name: "G".into() }] }],
            ..Default::default()
        });
        s
    }
    fn lesson(parts: Vec<LessonPart>, ppc: u32, cpw: u32) -> Lesson {
        Lesson { id: "l".into(), subject: "m".into(), teachers: vec!["t".into()], parts, periods_per_card: ppc, cards_per_week: cpw, rooms: vec![] }
    }
    fn part(g: Option<&str>) -> LessonPart {
        LessonPart { class_id: "c".into(), group_id: g.map(Into::into) }
    }

    #[test]
    fn cards_are_made_per_session() {
        let c = generate_cards(&[lesson(vec![part(None)], 2, 3)]);
        assert_eq!(c.len(), 3);
        assert!(c.iter().all(|x| x.length == 2));
        assert_eq!(c[2].number, 3);
    }

    #[test]
    fn load_counts_periods_and_parallel_groups() {
        let mut s = school();
        let mut a = lesson(vec![part(None)], 1, 4);
        a.id = "a".into();
        let mut b = lesson(vec![part(Some("g1"))], 2, 2);
        b.id = "b".into();
        let mut c = lesson(vec![part(Some("g2"))], 1, 3);
        c.id = "c".into();
        s.lessons = vec![a, b, c];
        let sum = summarize(&s);
        assert_eq!(sum.cards, 9);
        // 4 whole-class + busiest group (4 periods)
        assert_eq!(sum.classes[0].used, 8);
        // 7 periods Mon-Thu + 5 on Friday = 33 in the default morning shift
        assert_eq!(sum.classes[0].available, 33);
        assert_eq!(sum.teachers[0].used, 11);
    }

    #[test]
    fn forbidden_slots_reduce_availability() {
        let mut s = school();
        s.master.teachers[0].time_off.push(TimeOffCell { shift: "morning".into(), day: 0, period: 1, state: 2 });
        s.master.teachers[0].time_off.push(TimeOffCell { shift: "morning".into(), day: 0, period: 2, state: 1 });
        assert_eq!(summarize(&s).teachers[0].available, 32);
    }

    #[test]
    fn catches_bad_lessons() {
        let s = school();
        let mut l = lesson(vec![], 4, 0);
        l.subject = "x".into();
        let m: Vec<String> = validate(&[l], &s.master).into_iter().map(|i| i.message).collect();
        assert!(m.iter().any(|x| x.contains("no subject")));
        assert!(m.iter().any(|x| x.contains("no class")));
        assert!(m.iter().any(|x| x.contains("at least once")));
        assert!(m.iter().any(|x| x.contains("1, 2 or 3")));
    }

    #[test]
    fn valid_lesson_is_clean() {
        let s = school();
        assert!(validate(&[lesson(vec![part(Some("g1"))], 1, 3)], &s.master).is_empty());
    }
}
