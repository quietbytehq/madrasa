//! Timetable reports (Standard stage 1): a section, teacher or room timetable as a grid,
//! the teacher load, free rooms on a day, and the cover sheet. All printouts go through the
//! report PDF engine, English and Urdu.

use chrono::NaiveDate;
use madrasa_core::timetable::lessons::summarize;
use madrasa_core::timetable::substitutions::{self as subs, SheetDay};
use madrasa_core::timetable::SchoolSetup;
use madrasa_db::models::AcademicYear;
use madrasa_db::timetable as repo;
use sqlx::PgConnection;

use super::text::tr;
use super::{format_date, Column, Context, Group, Lang, Params, Report};
use crate::ApiError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridKind {
    Section,
    Teacher,
    Room,
}

impl GridKind {
    pub fn parse(s: &str) -> Option<GridKind> {
        match s {
            "section" => Some(GridKind::Section),
            "teacher" => Some(GridKind::Teacher),
            "room" => Some(GridKind::Room),
            _ => None,
        }
    }
}

fn day_name(lang: Lang, day: usize) -> String {
    tr(lang, &format!("tt.day.{day}")).into()
}

struct Names<'a> {
    s: &'a SchoolSetup,
}

impl Names<'_> {
    fn subject(&self, id: &str) -> String {
        self.s
            .master
            .subjects
            .iter()
            .find(|x| x.id == id)
            .map(|x| x.name.clone())
            .unwrap_or_default()
    }
    fn teacher(&self, id: &str) -> String {
        self.s
            .master
            .teachers
            .iter()
            .find(|x| x.id == id)
            .map(|x| x.name.clone())
            .unwrap_or_default()
    }
    fn class(&self, id: &str) -> String {
        self.s
            .master
            .classes
            .iter()
            .find(|x| x.id == id)
            .map(|x| x.name.clone())
            .unwrap_or_default()
    }
    fn room(&self, id: &str) -> String {
        self.s
            .master
            .rooms
            .iter()
            .find(|x| x.id == id)
            .map(|x| x.name.clone())
            .unwrap_or_default()
    }
}

/// The weekly grid of one section, teacher or room: one group per shift, a row per period,
/// a column per working day.
pub fn grid(
    lang: Lang,
    school_name: &str,
    year: &AcademicYear,
    setup: &SchoolSetup,
    kind: GridKind,
    id: &str,
) -> Result<Report, ApiError> {
    let names = Names { s: setup };
    let (code, title_key, who) = match kind {
        GridKind::Section => (
            "timetable.section",
            "report.timetable.section",
            names.class(id),
        ),
        GridKind::Teacher => (
            "timetable.teacher",
            "report.timetable.teacher",
            names.teacher(id),
        ),
        GridKind::Room => ("timetable.room", "report.timetable.room", names.room(id)),
    };
    if who.is_empty() {
        return Err(ApiError::not_found("timetable entity"));
    }
    let mut r = Report::new(code, lang, school_name, tr(lang, title_key));
    r.subtitle = Some(who.clone());
    r.landscape = true;
    r.param("param.academic_year", year.name.clone());
    r.param(
        match kind {
            GridKind::Section => "param.section",
            GridKind::Teacher => "tt.teacher",
            GridKind::Room => "tt.room",
        },
        who,
    );
    let working: Vec<usize> = setup
        .days
        .iter()
        .enumerate()
        .filter(|(_, d)| d.working)
        .map(|(i, _)| i)
        .collect();
    r.columns.push(Column::text(tr(lang, "tt.period")));
    r.columns.push(Column::text(tr(lang, "tt.time")));
    for d in &working {
        r.columns.push(Column::wide(day_name(lang, *d)));
    }
    let cards: Vec<_> = setup
        .timetable
        .as_ref()
        .map(|t| t.cards.clone())
        .unwrap_or_default();
    let lesson_of = |lid: &str| setup.lessons.iter().find(|l| l.id == lid);
    let mine = |lid: &str, room: Option<&str>| -> bool {
        let Some(l) = lesson_of(lid) else {
            return false;
        };
        match kind {
            GridKind::Section => l.parts.iter().any(|p| p.class_id == id),
            GridKind::Teacher => l.teachers.iter().any(|t| t == id),
            GridKind::Room => room == Some(id),
        }
    };
    let shifts: Vec<_> = match kind {
        GridKind::Section => {
            let sh = setup
                .master
                .classes
                .iter()
                .find(|c| c.id == id)
                .map(|c| c.shift.clone())
                .unwrap_or_default();
            setup
                .shifts
                .iter()
                .filter(|s| s.enabled && s.id == sh)
                .collect()
        }
        GridKind::Teacher => {
            let t = setup.master.teachers.iter().find(|t| t.id == id);
            setup
                .shifts
                .iter()
                .filter(|s| {
                    s.enabled && t.is_none_or(|t| t.shifts.is_empty() || t.shifts.contains(&s.id))
                })
                .collect()
        }
        GridKind::Room => setup.shifts.iter().filter(|s| s.enabled).collect(),
    };
    for shift in shifts {
        let periods = shift.days.iter().map(Vec::len).max().unwrap_or(0);
        let mut g = Group {
            heading: if setup.shifts.iter().filter(|s| s.enabled).count() > 1 {
                Some(shift.name.clone())
            } else {
                None
            },
            ..Default::default()
        };
        for p in 1..=periods {
            let time = working
                .iter()
                .find_map(|d| shift.days.get(*d).and_then(|x| x.get(p - 1)))
                .map(|t| format!("{}–{}", t.start, t.end))
                .unwrap_or_default();
            let mut row = vec![p.to_string(), time];
            for d in &working {
                let cell = cards
                    .iter()
                    .find(|c| {
                        c.shift == shift.id
                            && c.day as usize == *d
                            && (c.period as usize..c.period as usize + c.length as usize)
                                .contains(&p)
                            && mine(&c.lesson_id, c.room.as_deref())
                    })
                    .and_then(|c| lesson_of(&c.lesson_id).map(|l| (c, l)))
                    .map(|(c, l)| {
                        let subject = names.subject(&l.subject);
                        let teachers = l
                            .teachers
                            .iter()
                            .map(|t| names.teacher(t))
                            .collect::<Vec<_>>()
                            .join(", ");
                        let classes = l
                            .parts
                            .iter()
                            .map(|p| names.class(&p.class_id))
                            .collect::<Vec<_>>()
                            .join(", ");
                        let room = c.room.as_deref().map(|r| names.room(r)).unwrap_or_default();
                        let parts: Vec<String> = match kind {
                            GridKind::Section => vec![subject, teachers, room],
                            GridKind::Teacher => vec![subject, classes, room],
                            GridKind::Room => vec![subject, classes, teachers],
                        };
                        parts
                            .into_iter()
                            .filter(|s| !s.is_empty())
                            .collect::<Vec<_>>()
                            .join(" · ")
                    })
                    .unwrap_or_default();
                row.push(cell);
            }
            g.rows.push(row);
        }
        if let Some(b) = shift.breaks.first() {
            r.notes.push(format!(
                "{}: {} {}",
                b.name,
                tr(lang, "tt.after_period"),
                b.after_period
            ));
        }
        r.push_group(g);
    }
    if setup.timetable.is_none() {
        r.notes.push(tr(lang, "tt.no_timetable").into());
    }
    Ok(r)
}

/// Periods per week each teacher teaches, against what they could.
pub fn teacher_load_report(
    lang: Lang,
    school_name: &str,
    year: &AcademicYear,
    setup: &SchoolSetup,
) -> Report {
    let mut r = Report::new(
        "timetable.teacher_load",
        lang,
        school_name,
        tr(lang, "report.timetable.teacher_load"),
    );
    r.param("param.academic_year", year.name.clone());
    r.columns = vec![
        Column::wide(tr(lang, "tt.teacher")),
        Column::number(tr(lang, "tt.lessons")),
        Column::number(tr(lang, "tt.sections")),
        Column::number(tr(lang, "tt.periods_used")),
        Column::number(tr(lang, "tt.periods_available")),
        Column::number(tr(lang, "tt.free")),
    ];
    let load = summarize(setup);
    let mut g = Group::default();
    let (mut used, mut avail) = (0u32, 0u32);
    for t in &load.teachers {
        let lessons: Vec<_> = setup
            .lessons
            .iter()
            .filter(|l| l.teachers.contains(&t.id))
            .collect();
        let mut sections: Vec<&str> = lessons
            .iter()
            .flat_map(|l| l.parts.iter().map(|p| p.class_id.as_str()))
            .collect();
        sections.sort_unstable();
        sections.dedup();
        used += t.used;
        avail += t.available;
        g.rows.push(vec![
            t.name.clone(),
            lessons.len().to_string(),
            sections.len().to_string(),
            t.used.to_string(),
            t.available.to_string(),
            t.available.saturating_sub(t.used).to_string(),
        ]);
    }
    r.push_group(g);
    r.total = Some(vec![
        tr(lang, "tt.total").into(),
        setup.lessons.len().to_string(),
        String::new(),
        used.to_string(),
        avail.to_string(),
        avail.saturating_sub(used).to_string(),
    ]);
    r
}

/// Which rooms are free in each period of one weekday (0 = Monday).
pub fn free_rooms_report(
    lang: Lang,
    school_name: &str,
    year: &AcademicYear,
    setup: &SchoolSetup,
    day: usize,
) -> Report {
    let names = Names { s: setup };
    let mut r = Report::new(
        "timetable.free_rooms",
        lang,
        school_name,
        tr(lang, "report.timetable.free_rooms"),
    );
    r.landscape = true;
    r.param("param.academic_year", year.name.clone());
    r.param("param.day", day_name(lang, day.min(6)));
    let cards: Vec<_> = setup
        .timetable
        .as_ref()
        .map(|t| t.cards.clone())
        .unwrap_or_default();
    let periods = setup
        .shifts
        .iter()
        .filter(|s| s.enabled)
        .flat_map(|s| s.days.iter().map(Vec::len))
        .max()
        .unwrap_or(0);
    r.columns.push(Column::wide(tr(lang, "tt.room")));
    for p in 1..=periods {
        r.columns
            .push(Column::text(format!("{} {p}", tr(lang, "tt.period"))));
    }
    for shift in setup.shifts.iter().filter(|s| s.enabled) {
        let mut g = Group {
            heading: Some(shift.name.clone()),
            ..Default::default()
        };
        let n = shift.days.get(day).map_or(0, Vec::len);
        for room in &setup.master.rooms {
            let mut row = vec![room.name.clone()];
            for p in 1..=periods {
                if p > n {
                    row.push("—".into());
                    continue;
                }
                let taken = cards.iter().find(|c| {
                    c.shift == shift.id
                        && c.day as usize == day
                        && c.room.as_deref() == Some(room.id.as_str())
                        && (c.period as usize..c.period as usize + c.length as usize).contains(&p)
                });
                row.push(match taken {
                    Some(c) => setup
                        .lessons
                        .iter()
                        .find(|l| l.id == c.lesson_id)
                        .map(|l| {
                            l.parts
                                .iter()
                                .map(|x| names.class(&x.class_id))
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_default(),
                    None => tr(lang, "tt.free").into(),
                });
            }
            g.rows.push(row);
        }
        r.push_group(g);
    }
    if setup.master.rooms.is_empty() {
        r.notes.push(tr(lang, "tt.no_rooms").into());
    }
    r
}

/// The cover sheet for a date range: one group per day, a row per session needing cover.
pub fn cover_sheet_report(
    lang: Lang,
    school_name: &str,
    year: &AcademicYear,
    from: NaiveDate,
    to: NaiveDate,
    days: &[SheetDay],
) -> Report {
    let mut r = Report::new(
        "timetable.cover_sheet",
        lang,
        school_name,
        tr(lang, "report.timetable.cover_sheet"),
    );
    r.landscape = true;
    r.param("param.academic_year", year.name.clone());
    r.param("param.from", format_date(from));
    r.param("param.to", format_date(to));
    r.columns = vec![
        Column::text(tr(lang, "tt.period")),
        Column::text(tr(lang, "tt.time")),
        Column::text(tr(lang, "param.section")),
        Column::text(tr(lang, "tt.subject")),
        Column::wide(tr(lang, "tt.absent")),
        Column::wide(tr(lang, "tt.cover")),
        Column::text(tr(lang, "tt.room")),
        Column::text(tr(lang, "tt.note")),
    ];
    for d in days {
        let mut g = Group {
            heading: Some(format!("{} ({})", d.title, d.date)),
            ..Default::default()
        };
        for row in &d.rows {
            let cover = if row.cover == subs::NOT_COVERED {
                tr(lang, "tt.not_covered").to_string()
            } else if row.cover == subs::NO_COVER {
                tr(lang, "tt.no_cover").to_string()
            } else {
                row.cover.clone()
            };
            g.rows.push(vec![
                if row.shift.is_empty() {
                    row.period.clone()
                } else {
                    format!("{} {}", row.shift, row.period)
                },
                format!("{}–{}", row.start, row.end),
                row.classes.clone(),
                row.subject.clone(),
                row.absent.clone(),
                cover,
                row.room.clone(),
                row.note.clone(),
            ]);
        }
        r.push_group(g);
    }
    if days.is_empty() {
        r.notes.push(tr(lang, "tt.nothing_to_cover").into());
    }
    r
}

// ---- catalogue entry points -------------------------------------------------------------------

async fn load(conn: &mut PgConnection, ctx: &Context<'_>) -> Result<SchoolSetup, ApiError> {
    Ok(repo::setup_for_year(conn, ctx.school.id, ctx.year, None)
        .await?
        .setup)
}

pub async fn section(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let section_id = params
        .uuid("section_id")?
        .ok_or_else(|| ApiError::validation("Choose a section."))?;
    if !ctx.allows(section_id) {
        return Err(ApiError::forbidden());
    }
    let setup = load(conn, ctx).await?;
    grid(
        ctx.lang,
        &ctx.school.name,
        ctx.year,
        &setup,
        GridKind::Section,
        &section_id.to_string(),
    )
}

pub async fn teacher_load(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    _params: &Params,
) -> Result<Report, ApiError> {
    let setup = load(conn, ctx).await?;
    Ok(teacher_load_report(
        ctx.lang,
        &ctx.school.name,
        ctx.year,
        &setup,
    ))
}

pub async fn free_rooms(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let day = params.number("day", 0)?;
    if day > 6 {
        return Err(ApiError::validation(
            "The day is a number from 0 (Monday) to 6 (Sunday).",
        ));
    }
    let setup = load(conn, ctx).await?;
    Ok(free_rooms_report(
        ctx.lang,
        &ctx.school.name,
        ctx.year,
        &setup,
        day as usize,
    ))
}

pub async fn cover_sheet(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    params: &Params,
) -> Result<Report, ApiError> {
    let from = params
        .date("from")?
        .ok_or_else(|| ApiError::validation("Give the first date."))?;
    let to = params.date("to")?.unwrap_or(from);
    if to < from || (to - from).num_days() > 62 {
        return Err(ApiError::validation(
            "The range is at most two months and ends after it starts.",
        ));
    }
    let setup = load(conn, ctx).await?;
    let days = subs::cover_sheet(
        &setup,
        &from.format("%Y-%m-%d").to_string(),
        &to.format("%Y-%m-%d").to_string(),
    )
    .map_err(ApiError::validation)?;
    Ok(cover_sheet_report(
        ctx.lang,
        &ctx.school.name,
        ctx.year,
        from,
        to,
        &days,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use madrasa_core::timetable::lessons::{Lesson, LessonPart};
    use madrasa_core::timetable::{Class, PlacedCard, Subject, Teacher, Timetable};

    fn year() -> AcademicYear {
        AcademicYear {
            id: uuid::Uuid::nil(),
            school_id: uuid::Uuid::nil(),
            name: "2026-27".into(),
            start_date: NaiveDate::from_ymd_opt(2026, 4, 1).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2027, 3, 31).unwrap(),
            start_month: 4,
            is_active: true,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            updated_by: None,
            deleted_at: None,
            hlc: String::new(),
        }
    }

    fn setup() -> SchoolSetup {
        let mut s = SchoolSetup::default_new("Test");
        s.master.subjects.push(Subject {
            id: "m".into(),
            name: "Maths".into(),
            short_name: "MA".into(),
            color: "#000".into(),
        });
        s.master.teachers.push(Teacher {
            id: "t".into(),
            name: "Ayesha Khan".into(),
            short_name: "AK".into(),
            shifts: vec!["morning".into()],
            ..Default::default()
        });
        s.master.classes.push(Class {
            id: "c".into(),
            name: "9 A".into(),
            short_name: "9A".into(),
            shift: "morning".into(),
            ..Default::default()
        });
        s.lessons.push(Lesson {
            id: "l".into(),
            subject: "m".into(),
            teachers: vec!["t".into()],
            parts: vec![LessonPart {
                class_id: "c".into(),
                group_id: None,
            }],
            periods_per_card: 2,
            cards_per_week: 1,
            rooms: vec![],
        });
        s.timetable = Some(Timetable {
            cards: vec![PlacedCard {
                lesson_id: "l".into(),
                number: 1,
                shift: "morning".into(),
                day: 1,
                period: 2,
                length: 2,
                room: None,
                locked: false,
            }],
        });
        s
    }

    #[test]
    fn section_grid_shows_the_double_period_on_both_rows() {
        let s = setup();
        let r = grid(Lang::En, "Test", &year(), &s, GridKind::Section, "c").unwrap();
        // Columns: period, time, Mon..Fri.
        assert_eq!(r.columns.len(), 7);
        let rows = &r.groups[0].rows;
        assert_eq!(rows.len(), 7);
        assert_eq!(rows[1][3], "Maths · Ayesha Khan", "Tuesday, period 2");
        assert_eq!(
            rows[2][3], "Maths · Ayesha Khan",
            "Tuesday, period 3 (second half)"
        );
        assert_eq!(rows[0][3], "");
        let t = grid(Lang::Ur, "Test", &year(), &s, GridKind::Teacher, "t").unwrap();
        assert_eq!(t.groups[0].rows[1][3], "Maths · 9 A");
        assert!(grid(Lang::En, "Test", &year(), &s, GridKind::Room, "nope").is_err());
    }

    #[test]
    fn teacher_load_counts_periods() {
        let s = setup();
        let r = teacher_load_report(Lang::En, "Test", &year(), &s);
        assert_eq!(r.groups[0].rows[0][0], "Ayesha Khan");
        assert_eq!(r.groups[0].rows[0][3], "2");
        assert_eq!(r.groups[0].rows[0][4], "33");
    }

    #[test]
    fn free_rooms_marks_taken_periods() {
        let mut s = setup();
        s.master.rooms.push(madrasa_core::timetable::Room {
            id: "r".into(),
            name: "Lab".into(),
            short_name: "Lab".into(),
            ..Default::default()
        });
        s.timetable.as_mut().unwrap().cards[0].room = Some("r".into());
        let r = free_rooms_report(Lang::En, "Test", &year(), &s, 1);
        let row = &r.groups[0].rows[0];
        assert_eq!(row[0], "Lab");
        assert_eq!(row[1], "Free");
        assert_eq!(row[2], "9 A");
        assert_eq!(row[3], "9 A");
        let fri = free_rooms_report(Lang::En, "Test", &year(), &s, 4);
        assert_eq!(fri.groups[0].rows[0][7], "—", "Friday has five periods");
    }
}
