use crate::timetable::master::MasterData;
use serde::{Deserialize, Serialize};

pub const DAY_NAMES: [&str; 7] = [
    "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Day {
    pub name: String,
    pub working: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PeriodTime {
    pub start: String, // "HH:MM"
    pub end: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Break {
    /// The break comes after this period number (1-based).
    pub after_period: u32,
    pub name: String,
    /// Double periods may not run across this break.
    pub cannot_cross: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Shift {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// Always 7 entries (Monday..Sunday): the periods of that day.
    pub days: Vec<Vec<PeriodTime>>,
    pub breaks: Vec<Break>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SchoolSetup {
    pub name: String,
    /// primary | middle | high | higher_secondary
    pub school_type: String,
    pub academic_year: String,
    /// Always 7 entries (Monday..Sunday).
    pub days: Vec<Day>,
    pub shifts: Vec<Shift>,
    #[serde(default)]
    pub master: MasterData,
    #[serde(default)]
    pub lessons: Vec<crate::timetable::lessons::Lesson>,
    /// The school's own logo as a picture data URL (shown on printouts and exports).
    #[serde(default)]
    pub logo: Option<String>,
    /// "Must happen at the same time" groups of lessons.
    #[serde(default)]
    pub links: Vec<crate::timetable::links::Link>,
    /// School-wide limits and preferences.
    #[serde(default)]
    pub settings: crate::timetable::settings::Settings,
    /// Absences and cover teachers (kept apart from the timetable, which they never change).
    #[serde(default)]
    pub substitutions: crate::timetable::substitutions::Substitutions,
    /// The generated timetable, if any.
    #[serde(default)]
    pub timetable: Option<crate::timetable::solver::Timetable>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    /// "error" blocks timetable generation later; "warning" is advice.
    pub level: String,
    pub message: String,
}

pub fn parse_time(s: &str) -> Option<u32> {
    let (h, m) = s.split_once(':')?;
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

fn fmt_time(mins: u32) -> String {
    format!("{:02}:{:02}", mins / 60, mins % 60)
}

fn periods(start: u32, count: usize, len: u32, gaps: &[(usize, u32)]) -> Vec<PeriodTime> {
    let mut t = start;
    let mut out = Vec::new();
    for i in 0..count {
        out.push(PeriodTime { start: fmt_time(t), end: fmt_time(t + len) });
        t += len;
        if let Some((_, g)) = gaps.iter().find(|(after, _)| *after == i + 1) {
            t += g;
        }
    }
    out
}

impl SchoolSetup {
    pub fn default_new(name: &str) -> Self {
        let days = DAY_NAMES
            .iter()
            .enumerate()
            .map(|(i, n)| Day { name: n.to_string(), working: i < 5 })
            .collect();
        let work = |p: Vec<PeriodTime>, fri: Vec<PeriodTime>| -> Vec<Vec<PeriodTime>> {
            (0..7)
                .map(|d| match d {
                    0..=3 => p.clone(),
                    4 => fri.clone(),
                    _ => vec![],
                })
                .collect()
        };
        let morning = Shift {
            id: "morning".into(),
            name: "Morning".into(),
            enabled: true,
            days: work(
                periods(8 * 60, 7, 40, &[(3, 20)]),
                periods(8 * 60, 5, 40, &[(3, 20)]),
            ),
            breaks: vec![Break { after_period: 3, name: "Break".into(), cannot_cross: true }],
        };
        let afternoon = Shift {
            id: "afternoon".into(),
            name: "Afternoon".into(),
            enabled: false,
            days: work(
                periods(13 * 60, 6, 40, &[(3, 15)]),
                periods(14 * 60, 6, 40, &[(3, 15)]),
            ),
            breaks: vec![Break { after_period: 3, name: "Break".into(), cannot_cross: true }],
        };
        SchoolSetup {
            name: name.to_string(),
            school_type: "primary".into(),
            academic_year: String::new(),
            days,
            shifts: vec![morning, afternoon],
            master: MasterData::default(),
            lessons: vec![],
            logo: None,
            links: vec![],
            settings: Default::default(),
            substitutions: Default::default(),
            timetable: None,
        }
    }

    /// Checks the setup and returns messages written for school staff.
    pub fn validate(&self) -> Vec<Issue> {
        let mut out = Vec::new();
        let mut err = |m: String| out.push(Issue { level: "error".into(), message: m });
        let mut warn_list: Vec<String> = Vec::new();

        if self.name.trim().is_empty() {
            err("Please enter the school name.".into());
        }
        if self.days.len() != 7 || self.shifts.iter().any(|s| s.days.len() != 7) {
            err("The weekly schedule is incomplete.".into());
            return out;
        }
        if !self.days.iter().any(|d| d.working) {
            err("Choose at least one working day.".into());
        }
        let enabled: Vec<&Shift> = self.shifts.iter().filter(|s| s.enabled).collect();
        if enabled.is_empty() {
            err("Turn on at least one shift (morning or afternoon).".into());
        }

        for shift in &enabled {
            for (di, day) in self.days.iter().enumerate() {
                if !day.working {
                    continue;
                }
                let ps = &shift.days[di];
                if ps.is_empty() {
                    err(format!("{} shift has no periods on {}.", shift.name, day.name));
                    continue;
                }
                let mut prev_end: Option<u32> = None;
                for (pi, p) in ps.iter().enumerate() {
                    let n = pi + 1;
                    let (Some(s), Some(e)) = (parse_time(&p.start), parse_time(&p.end)) else {
                        err(format!("{} shift, {}: period {} has a time that is not valid.", shift.name, day.name, n));
                        continue;
                    };
                    if e <= s {
                        err(format!("{} shift, {}: period {} must end after it starts.", shift.name, day.name, n));
                    }
                    if let Some(pe) = prev_end {
                        if s < pe {
                            err(format!("{} shift, {}: period {} starts before period {} ends.", shift.name, day.name, n, n - 1));
                        }
                    }
                    prev_end = Some(e);
                }
                for b in &shift.breaks {
                    let a = b.after_period as usize;
                    if a >= 1 && a < ps.len() {
                        if let (Some(e), Some(s)) = (parse_time(&ps[a - 1].end), parse_time(&ps[a].start)) {
                            if s <= e {
                                warn_list.push(format!(
                                    "{} shift, {}: \"{}\" after period {} has no time gap between the periods.",
                                    shift.name, day.name, b.name, b.after_period
                                ));
                            }
                        }
                    }
                }
            }
            for b in &shift.breaks {
                if b.after_period == 0 {
                    err(format!("{} shift: the break \"{}\" must come after period 1 or later.", shift.name, b.name));
                }
            }
        }

        // Overlap between shifts on the same day.
        if enabled.len() == 2 {
            for (di, day) in self.days.iter().enumerate().filter(|(_, d)| d.working) {
                let span = |s: &Shift| {
                    let v = &s.days[di];
                    Some((parse_time(&v.first()?.start)?, parse_time(&v.last()?.end)?))
                };
                if let (Some(a), Some(b)) = (span(enabled[0]), span(enabled[1])) {
                    if a.0 < b.1 && b.0 < a.1 {
                        warn_list.push(format!(
                            "On {} the {} and {} shifts overlap in time.",
                            day.name, enabled[0].name, enabled[1].name
                        ));
                    }
                }
            }
        }

        out.extend(self.master.validate(&self.shifts));
        out.extend(crate::timetable::lessons::validate(&self.lessons, &self.master));
        out.extend(crate::timetable::links::validate(self));

        for m in warn_list {
            out.push(Issue { level: "warning".into(), message: m });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_valid() {
        let s = SchoolSetup::default_new("Test School");
        assert!(s.validate().is_empty(), "{:?}", s.validate());
    }

    #[test]
    fn friday_is_shorter_by_default() {
        let s = SchoolSetup::default_new("T");
        assert_eq!(s.shifts[0].days[0].len(), 7);
        assert_eq!(s.shifts[0].days[4].len(), 5);
    }

    #[test]
    fn catches_bad_times_and_missing_name() {
        let mut s = SchoolSetup::default_new(" ");
        s.shifts[0].days[1][2].end = "08:00".into();
        let issues = s.validate();
        assert!(issues.iter().any(|i| i.message.contains("school name")));
        assert!(issues.iter().any(|i| i.message.contains("must end after")));
    }

    #[test]
    fn needs_a_shift_and_a_day() {
        let mut s = SchoolSetup::default_new("T");
        s.shifts[0].enabled = false;
        for d in &mut s.days {
            d.working = false;
        }
        let m: Vec<_> = s.validate().into_iter().map(|i| i.message).collect();
        assert!(m.iter().any(|x| x.contains("working day")));
        assert!(m.iter().any(|x| x.contains("at least one shift")));
    }

    #[test]
    fn warns_when_shifts_overlap() {
        let mut s = SchoolSetup::default_new("T");
        s.shifts[1].enabled = true;
        s.shifts[1].days[0] = periods(9 * 60, 3, 40, &[]);
        assert!(s.validate().iter().any(|i| i.level == "warning" && i.message.contains("overlap")));
    }

    #[test]
    fn saturday_working_needs_periods() {
        let mut s = SchoolSetup::default_new("T");
        s.days[5].working = true;
        assert!(s.validate().iter().any(|i| i.message.contains("Saturday")));
    }
}
