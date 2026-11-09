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

/// Teachers get a friendlier check than the other lists: a clashing short name comes with
/// a suggested unique one, and two teachers who share a full name are only a warning when
/// their short names already tell them apart (initials collide often in a large staff room).
fn check_teacher_names<'a>(out: &mut Vec<Issue>, items: impl Iterator<Item = (&'a str, &'a str)> + Clone) {
    let mut shorts: HashSet<String> = HashSet::new();
    let mut seen_names: Vec<(String, String)> = Vec::new();
    let all_shorts: HashSet<String> = items.clone().map(|(_, s)| s.trim().to_lowercase()).collect();
    for (name, short) in items {
        let n = name.trim();
        if n.is_empty() {
            out.push(err("A teacher has no name. Please enter one or delete it.".into()));
            continue;
        }
        let s = short.trim();
        if s.is_empty() {
            out.push(err(format!("The teacher \"{n}\" needs a short name.")));
        } else if !shorts.insert(s.to_lowercase()) {
            let suggestion = suggest_short_name(n, &all_shorts);
            out.push(err(format!(
                "The short name \"{s}\" is used by more than one teacher. Try \"{suggestion}\" for {n}."
            )));
        }
        if let Some((_, other)) = seen_names.iter().find(|(k, _)| k == &n.to_lowercase()) {
            if s.is_empty() || other.eq_ignore_ascii_case(s) {
                out.push(err(format!("There are two teachers called \"{n}\". Names must be different.")));
            } else {
                out.push(Issue {
                    level: "warning".into(),
                    message: format!(
                        "There are two teachers called \"{n}\" ({other} and {s}). Their short names tell them apart; check that both are really different people."
                    ),
                });
            }
        }
        seen_names.push((n.to_lowercase(), s.to_string()));
    }
}

/// A short name for `name` that no one in `taken` (lower-cased) uses yet: the initials, then
/// the initials with more letters of the last word ("AH" → "AHu" → "AHus"), then a number.
pub fn suggest_short_name(name: &str, taken: &HashSet<String>) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    let initials: String = words.iter().filter_map(|w| w.chars().next()).take(3).collect::<String>().to_uppercase();
    let base = if initials.is_empty() { name.chars().take(3).collect::<String>().to_uppercase() } else { initials };
    let free = |c: &str| !taken.contains(&c.to_lowercase());
    if free(&base) {
        return base;
    }
    if let Some(last) = words.last() {
        let prefix: String = words[..words.len() - 1].iter().filter_map(|w| w.chars().next()).take(2).collect::<String>().to_uppercase();
        let rest: Vec<char> = last.chars().collect();
        for n in 2..=rest.len().min(4) {
            let mut c = prefix.clone();
            c.extend(rest[..n].iter());
            if free(&c) {
                return c;
            }
        }
    }
    (2..1000).map(|n| format!("{base}{n}")).find(|c| free(c)).unwrap_or(base)
}

fn err(message: String) -> Issue {
    Issue { level: "error".into(), message }
}

impl MasterData {
    pub fn validate(&self, shifts: &[Shift]) -> Vec<Issue> {
        let mut out = Vec::new();
        let enabled: HashSet<&str> = shifts.iter().filter(|s| s.enabled).map(|s| s.id.as_str()).collect();

        check_names(&mut out, "subject", self.subjects.iter().map(|x| (x.name.as_str(), x.short_name.as_str())));
        check_teacher_names(&mut out, self.teachers.iter().map(|x| (x.name.as_str(), x.short_name.as_str())));
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
    fn clashing_teacher_initials_get_a_suggestion_and_same_names_warn_when_codes_differ() {
        let mut s = SchoolSetup::default_new("T");
        for (id, n, c) in [("a", "Ayesha Hussain", "AH"), ("b", "Ali Hassan", "AH"), ("c", "Danish Khan", "DK"), ("d", "Danish Khan", "DKh")] {
            s.master.teachers.push(Teacher { id: id.into(), name: n.into(), short_name: c.into(), shifts: vec!["morning".into()], ..Default::default() });
        }
        let issues = s.validate();
        let errors: Vec<&str> = issues.iter().filter(|i| i.level == "error").map(|i| i.message.as_str()).collect();
        let warnings: Vec<&str> = issues.iter().filter(|i| i.level == "warning").map(|i| i.message.as_str()).collect();
        assert_eq!(errors, ["The short name \"AH\" is used by more than one teacher. Try \"AHa\" for Ali Hassan."]);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("two teachers called \"Danish Khan\" (DK and DKh)"), "{}", warnings[0]);

        // Same name and same code is still an error.
        s.master.teachers[3].short_name = "DK".into();
        let m = msgs(&s);
        assert!(m.iter().any(|x| x == "There are two teachers called \"Danish Khan\". Names must be different."), "{m:?}");
    }

    #[test]
    fn suggested_short_names_are_unique() {
        let taken: HashSet<String> = ["ah", "ahu", "ahus", "ahuss", "dk"].iter().map(|s| s.to_string()).collect();
        assert_eq!(suggest_short_name("Ayesha Hussain", &taken), "AH2");
        assert_eq!(suggest_short_name("Ali Hassan", &taken), "AHa");
        assert_eq!(suggest_short_name("Danish Khan", &taken), "DKh");
        assert_eq!(suggest_short_name("Sana", &taken), "S");
        assert_eq!(suggest_short_name("Muhammad Ali Raza Shah", &taken), "MAR");
        assert_eq!(suggest_short_name("Ayesha Hussain", &HashSet::new()), "AH");
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
