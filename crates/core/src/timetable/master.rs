//! Master data (milestone 2): subjects, teachers, classes, buildings, rooms.
use crate::timetable::model::{Issue, Shift};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// One non-default cell of a time-off grid.
/// state: 1 = avoid if possible, 2 = forbidden. Cells not listed are available.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TimeOffCell {
    pub shift: String,
    pub day: u8,
    pub period: u8,
    pub state: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Subject {
    pub id: String,
    pub name: String,
    pub short_name: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Teacher {
    pub id: String,
    pub name: String,
    pub short_name: String,
    #[serde(default)]
    pub email: String,
    /// Ids of the shifts this teacher works in.
    #[serde(default)]
    pub shifts: Vec<String>,
    #[serde(default)]
    pub max_lessons_per_day: Option<u32>,
    /// Most periods in a week. Empty = the school's default.
    #[serde(default)]
    pub max_periods_per_week: Option<u32>,
    #[serde(default)]
    pub max_consecutive: Option<u32>,
    #[serde(default)]
    pub max_days_per_week: Option<u32>,
    #[serde(default)]
    pub time_off: Vec<TimeOffCell>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    pub name: String,
}

/// One way of splitting a class, e.g. "Boys / Girls" with two groups.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Division {
    pub id: String,
    pub name: String,
    pub groups: Vec<Group>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Class {
    pub id: String,
    pub name: String,
    pub short_name: String,
    pub shift: String,
    #[serde(default)]
    pub class_teacher: Option<String>,
    #[serde(default)]
    pub home_room: Option<String>,
    #[serde(default)]
    pub divisions: Vec<Division>,
    #[serde(default)]
    pub time_off: Vec<TimeOffCell>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Building {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Room {
    pub id: String,
    pub name: String,
    pub short_name: String,
    #[serde(default)]
    pub building: Option<String>,
    #[serde(default)]
    pub capacity: Option<u32>,
    #[serde(default)]
    pub time_off: Vec<TimeOffCell>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct MasterData {
    #[serde(default)]
    pub subjects: Vec<Subject>,
    #[serde(default)]
    pub teachers: Vec<Teacher>,
    #[serde(default)]
    pub classes: Vec<Class>,
    #[serde(default)]
    pub buildings: Vec<Building>,
    #[serde(default)]
    pub rooms: Vec<Room>,
}

fn check_names<'a>(
    out: &mut Vec<Issue>,
    what: &str,
    items: impl Iterator<Item = (&'a str, &'a str)>,
) {
    let mut names = HashSet::new();
    let mut shorts = HashSet::new();
    for (name, short) in items {
        let n = name.trim();
        if n.is_empty() {
            out.push(err(format!("A {what} has no name. Please enter one or delete it.")));
            continue;
        }
        if !names.insert(n.to_lowercase()) {
            out.push(err(format!("There are two {what}s called \"{n}\". Names must be different.")));
        }
        let s = short.trim();
        if s.is_empty() {
            out.push(err(format!("The {what} \"{n}\" needs a short name.")));
        } else if !shorts.insert(s.to_lowercase()) {
            out.push(err(format!("The short name \"{s}\" is used by more than one {what}.")));
        }
    }
}

fn err(message: String) -> Issue {
    Issue { level: "error".into(), message }
}

impl MasterData {
    pub fn validate(&self, shifts: &[Shift]) -> Vec<Issue> {
        let mut out = Vec::new();
        let enabled: HashSet<&str> = shifts.iter().filter(|s| s.enabled).map(|s| s.id.as_str()).collect();

        check_names(&mut out, "subject", self.subjects.iter().map(|x| (x.name.as_str(), x.short_name.as_str())));
        check_names(&mut out, "teacher", self.teachers.iter().map(|x| (x.name.as_str(), x.short_name.as_str())));
        check_names(&mut out, "class", self.classes.iter().map(|x| (x.name.as_str(), x.short_name.as_str())));
        check_names(&mut out, "room", self.rooms.iter().map(|x| (x.name.as_str(), x.short_name.as_str())));

        let mut b = HashSet::new();
        for x in &self.buildings {
            if x.name.trim().is_empty() {
                out.push(err("A building has no name. Please enter one or delete it.".into()));
            } else if !b.insert(x.name.trim().to_lowercase()) {
                out.push(err(format!("There are two buildings called \"{}\".", x.name.trim())));
            }
        }

        for t in &self.teachers {
            if t.shifts.is_empty() {
                out.push(err(format!("Teacher \"{}\" is not assigned to any shift.", t.name.trim())));
            } else if !t.shifts.iter().any(|s| enabled.contains(s.as_str())) {
                out.push(err(format!(
                    "Teacher \"{}\" only works in a shift that is switched off.",
                    t.name.trim()
                )));
            }
        }

        for c in &self.classes {
            if !enabled.contains(c.shift.as_str()) {
                out.push(err(format!(
                    "Class \"{}\" must belong to a shift that is switched on.",
                    c.name.trim()
                )));
            }
            if let Some(t) = &c.class_teacher {
                if !self.teachers.iter().any(|x| &x.id == t) {
                    out.push(err(format!("Class \"{}\" has a class teacher who no longer exists.", c.name.trim())));
                }
            }
            for d in &c.divisions {
                if d.name.trim().is_empty() {
                    out.push(err(format!("Class \"{}\" has a division with no name.", c.name.trim())));
                }
                if d.groups.len() < 2 {
                    out.push(err(format!(
                        "Division \"{}\" of class \"{}\" needs at least two groups.",
                        d.name.trim(),
                        c.name.trim()
                    )));
                }
                let mut g = HashSet::new();
                for grp in &d.groups {
                    if grp.name.trim().is_empty() || !g.insert(grp.name.trim().to_lowercase()) {
                        out.push(err(format!(
                            "Division \"{}\" of class \"{}\" has a group with a missing or repeated name.",
                            d.name.trim(),
                            c.name.trim()
                        )));
                        break;
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timetable::model::SchoolSetup;

    fn msgs(s: &SchoolSetup) -> Vec<String> {
        s.validate().into_iter().map(|i| i.message).collect()
    }

    #[test]
    fn catches_duplicates_and_missing_names() {
        let mut s = SchoolSetup::default_new("T");
        for (id, n) in [("a", "Maths"), ("b", "maths"), ("c", "")] {
            s.master.subjects.push(Subject { id: id.into(), name: n.into(), short_name: id.into(), color: "#000".into() });
        }
        let m = msgs(&s);
        assert!(m.iter().any(|x| x.contains("two subjects called")));
        assert!(m.iter().any(|x| x.contains("has no name")));
    }

    #[test]
    fn teacher_and_class_need_a_working_shift() {
        let mut s = SchoolSetup::default_new("T");
        s.master.teachers.push(Teacher { id: "t".into(), name: "A".into(), short_name: "A".into(), ..Default::default() });
        s.master.teachers.push(Teacher { id: "u".into(), name: "B".into(), short_name: "B".into(), shifts: vec!["afternoon".into()], ..Default::default() });
        s.master.classes.push(Class { id: "c".into(), name: "5A".into(), short_name: "5A".into(), shift: "afternoon".into(), ..Default::default() });
        let m = msgs(&s);
        assert!(m.iter().any(|x| x.contains("not assigned to any shift")));
        assert!(m.iter().any(|x| x.contains("switched off")));
        assert!(m.iter().any(|x| x.contains("switched on")));
    }

    #[test]
    fn division_needs_two_distinct_groups() {
        let mut s = SchoolSetup::default_new("T");
        s.master.classes.push(Class {
            id: "c".into(), name: "5A".into(), short_name: "5A".into(), shift: "morning".into(),
            divisions: vec![Division { id: "d".into(), name: "Split".into(), groups: vec![Group { id: "g".into(), name: "One".into() }] }],
            ..Default::default()
        });
        assert!(msgs(&s).iter().any(|x| x.contains("at least two groups")));
    }
}
