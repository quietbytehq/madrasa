//! "Must happen at the same time" links between lessons (milestone 8).
//! Session n of every linked lesson is placed on the same day and period.
use crate::timetable::model::{Issue, SchoolSetup};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Link {
    pub id: String,
    pub lessons: Vec<String>,
}

fn err(message: String) -> Issue {
    Issue { level: "error".into(), message }
}

/// "Maths for 5A, 5B"
pub fn lesson_title(s: &SchoolSetup, lesson_id: &str) -> String {
    let m = &s.master;
    let Some(l) = s.lessons.iter().find(|l| l.id == lesson_id) else { return "a lesson that no longer exists".into() };
    let subj = m.subjects.iter().find(|x| x.id == l.subject).map_or("A lesson", |x| x.name.trim());
    let mut classes: Vec<String> = Vec::new();
    for p in &l.parts {
        let Some(c) = m.classes.iter().find(|c| c.id == p.class_id) else { continue };
        let g = p.group_id.as_ref().and_then(|g| c.divisions.iter().flat_map(|d| d.groups.iter()).find(|x| &x.id == g));
        let name = match g {
            Some(g) => format!("{} {}", c.name.trim(), g.name.trim()),
            None => c.name.trim().to_string(),
        };
        if !classes.contains(&name) {
            classes.push(name);
        }
    }
    format!("{subj} for {}", classes.join(", "))
}

/// Names of the classes (with the group, if only a group attends) of a lesson.
pub fn lesson_classes(s: &SchoolSetup, lesson_id: &str) -> Vec<String> {
    let m = &s.master;
    let Some(l) = s.lessons.iter().find(|l| l.id == lesson_id) else { return vec![] };
    l.parts
        .iter()
        .filter_map(|p| {
            let c = m.classes.iter().find(|c| c.id == p.class_id)?;
            let g = p.group_id.as_ref().and_then(|g| c.divisions.iter().flat_map(|d| d.groups.iter()).find(|x| &x.id == g));
            Some(match g {
                Some(g) => format!("{} {}", c.name.trim(), g.name.trim()),
                None => c.name.trim().to_string(),
            })
        })
        .collect()
}

/// Why a set of lessons cannot all happen at the same moment, if they cannot.
pub fn problem(s: &SchoolSetup, ids: &[String]) -> Option<String> {
    let m = &s.master;
    let ls: Vec<_> = ids.iter().filter_map(|id| s.lessons.iter().find(|l| &l.id == id)).collect();
    if ls.len() != ids.len() {
        return Some("a linked lesson no longer exists".into());
    }
    let first = ls.first()?;
    for l in &ls {
        if l.cards_per_week != first.cards_per_week || l.periods_per_card != first.periods_per_card {
            return Some(format!(
                "{} and {} do not have the same number or length of sessions, so session 1 cannot match session 1",
                lesson_title(s, &first.id), lesson_title(s, &l.id)
            ));
        }
    }
    let shift_of = |l: &crate::timetable::lessons::Lesson| l.parts.first().and_then(|p| m.classes.iter().find(|c| c.id == p.class_id)).map(|c| c.shift.clone());
    if ls.iter().any(|l| shift_of(l) != shift_of(first)) {
        return Some("they are in different shifts".into());
    }
    for (i, a) in ls.iter().enumerate() {
        for b in &ls[i + 1..] {
            if let Some(t) = a.teachers.iter().find(|t| b.teachers.contains(t)) {
                let name = m.teachers.iter().find(|x| &x.id == t).map_or("A teacher", |x| x.name.trim());
                return Some(format!("{name} teaches both {} and {}", lesson_title(s, &a.id), lesson_title(s, &b.id)));
            }
            for pa in &a.parts {
                for pb in &b.parts {
                    if pa.class_id != pb.class_id {
                        continue;
                    }
                    let Some(c) = m.classes.iter().find(|c| c.id == pa.class_id) else { continue };
                    let masks = crate::timetable::solver::class_masks(c);
                    let mask = |g: &Option<String>| match (g, &masks) {
                        (None, Some((atoms, _))) => if *atoms >= 64 { u64::MAX } else { (1u64 << atoms) - 1 },
                        (Some(g), Some((_, groups))) => groups.iter().find(|(id, _)| id == g).map_or(0, |x| x.1),
                        _ => u64::MAX,
                    };
                    if mask(&pa.group_id) & mask(&pb.group_id) != 0 {
                        return Some(format!(
                            "class {} would need to be in both {} and {} at once. Link lessons for different groups of the class",
                            c.name.trim(), lesson_title(s, &a.id), lesson_title(s, &b.id)
                        ));
                    }
                }
            }
        }
    }
    None
}

pub fn validate(s: &SchoolSetup) -> Vec<Issue> {
    let mut out = Vec::new();
    let mut used: Vec<&str> = Vec::new();
    for (i, link) in s.links.iter().enumerate() {
        if link.lessons.len() < 2 {
            out.push(err(format!("Link {} needs at least two lessons. Add another or remove the link.", i + 1)));
            continue;
        }
        for id in &link.lessons {
            if used.contains(&id.as_str()) {
                out.push(err(format!("{} is in more than one \"same time\" link. A lesson can only be in one.", lesson_title(s, id))));
            }
            used.push(id);
        }
        if let Some(p) = problem(s, &link.lessons) {
            out.push(err(format!("These lessons cannot happen at the same time: {p}.")));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timetable::lessons::{Lesson, LessonPart};
    use crate::timetable::master::*;

    fn school() -> SchoolSetup {
        let mut s = SchoolSetup::default_new("T");
        s.master.subjects.push(Subject { id: "s".into(), name: "ICT".into(), short_name: "I".into(), color: "#000".into() });
        for t in ["t1", "t2"] {
            s.master.teachers.push(Teacher { id: t.into(), name: t.into(), short_name: t.into(), shifts: vec!["morning".into()], ..Default::default() });
        }
        s.master.classes.push(Class {
            id: "c".into(), name: "7A".into(), short_name: "7A".into(), shift: "morning".into(),
            divisions: vec![Division { id: "d".into(), name: "Halves".into(), groups: vec![Group { id: "g1".into(), name: "G1".into() }, Group { id: "g2".into(), name: "G2".into() }] }],
            ..Default::default()
        });
        let mk = |id: &str, t: &str, g: Option<&str>| Lesson {
            id: id.into(), subject: "s".into(), teachers: vec![t.into()],
            parts: vec![LessonPart { class_id: "c".into(), group_id: g.map(Into::into) }],
            periods_per_card: 1, cards_per_week: 2, rooms: vec![],
        };
        s.lessons = vec![mk("a", "t1", Some("g1")), mk("b", "t2", Some("g2")), mk("w", "t2", None)];
        s
    }
    fn link(ids: &[&str]) -> Link {
        Link { id: "k".into(), lessons: ids.iter().map(|x| x.to_string()).collect() }
    }

    #[test]
    fn a_good_link_has_no_problems() {
        let mut s = school();
        s.links = vec![link(&["a", "b"])];
        assert!(validate(&s).is_empty(), "{:?}", validate(&s));
    }

    #[test]
    fn explains_why_lessons_cannot_be_linked() {
        let mut s = school();
        s.links = vec![link(&["a", "w"])]; // whole class overlaps group 1
        assert!(validate(&s)[0].message.contains("would need to be in both"), "{:?}", validate(&s));
        s.links = vec![link(&["b", "w"])]; // same teacher t2
        assert!(validate(&s)[0].message.contains("t2 teaches both"), "{:?}", validate(&s));
        s.links = vec![link(&["a"])];
        assert!(validate(&s)[0].message.contains("at least two"));
        s.links = vec![link(&["a", "b"]), link(&["b", "w"])];
        assert!(validate(&s).iter().any(|i| i.message.contains("more than one")));
        s.links = vec![link(&["a", "b"])];
        s.lessons[1].cards_per_week = 3;
        assert!(validate(&s)[0].message.contains("same number or length"));
        s.lessons[1].cards_per_week = 2;
        s.lessons[1].parts[0].class_id = "gone".into();
        assert!(!validate(&s).is_empty());
    }
}
