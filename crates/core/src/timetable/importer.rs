//! Import (milestone 4): fill subjects, teachers, classes, rooms and lessons
//! from Excel or CSV files. Files are read by column heading, in any order.
use crate::timetable::lessons::{Lesson, LessonPart};
use crate::timetable::master::{Building, Class, Room, Subject, Teacher};
use crate::timetable::model::SchoolSetup;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

pub const KINDS: [&str; 5] = ["subjects", "teachers", "rooms", "classes", "lessons"];

fn sheet_title(kind: &str) -> &'static str {
    match kind {
        "subjects" => "Subjects",
        "teachers" => "Teachers",
        "classes" => "Classes",
        "rooms" => "Rooms",
        _ => "Lessons",
    }
}

pub fn headers(kind: &str) -> &'static [&'static str] {
    match kind {
        "subjects" => &["Name", "Short name", "Colour"],
        "teachers" => &["Name", "Short name", "Email", "Shifts", "Most periods per day", "Most periods in a row", "Most days per week", "Most periods per week"],
        "classes" => &["Name", "Short name", "Shift", "Class teacher", "Home classroom"],
        "rooms" => &["Name", "Short name", "Building", "Capacity"],
        _ => &["Subject", "Teachers", "Classes", "Periods per session", "Sessions per week", "Rooms"],
    }
}


#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub sheet: String,
    /// Row number as shown in Excel (the heading is row 1).
    pub row: usize,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub subjects: u32,
    pub teachers: u32,
    pub classes: u32,
    pub rooms: u32,
    pub lessons: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub setup: SchoolSetup,
    pub added: Counts,
    /// Rows left out because the same name already exists.
    pub skipped: u32,
    pub problems: Vec<Problem>,
}

pub type Table = Vec<Vec<String>>;

fn norm(s: &str) -> String {
    s.trim().to_lowercase()
}

fn split_list(s: &str) -> Vec<String> {
    s.split([',', ';']).map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()
}

/// Splits a cell that lists names ("Ms Ana, BOB"). A name may itself contain a comma
/// ("Khan, Ayesha"), so at each point the longest run of pieces that is a known name wins;
/// anything unknown falls back to a single piece (and is reported as not found later).
/// A semicolon always separates names.
fn split_names(s: &str, known: impl Fn(&str) -> bool) -> Vec<String> {
    let mut out = Vec::new();
    for segment in s.split(';') {
        let pieces: Vec<&str> = segment.split(',').map(str::trim).collect();
        let mut i = 0;
        while i < pieces.len() {
            let mut take = 1;
            for j in (i + 2..=pieces.len()).rev() {
                if known(&pieces[i..j].join(", ")) {
                    take = j - i;
                    break;
                }
            }
            let name = pieces[i..i + take].join(", ");
            if !name.is_empty() {
                out.push(name);
            }
            i += take;
        }
    }
    out
}

fn valid_colour(c: &str) -> bool {
    let c = c.trim();
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit())
}

const PALETTE: [&str; 12] = [
    "#1F6F6B", "#E8A33D", "#C0504D", "#4C78A8", "#7B5EA7", "#6A994E", "#D17A5B", "#3C8DA3", "#A68A3E", "#8C5A6B", "#5B7F5E", "#B0616F",
];

struct Rows<'a> {
    cols: HashMap<String, usize>,
    rows: &'a [Vec<String>],
}

impl<'a> Rows<'a> {
    fn new(t: &'a Table) -> Self {
        let cols = t
            .first()
            .map(|h| h.iter().enumerate().map(|(i, c)| (norm(c), i)).collect())
            .unwrap_or_default();
        Rows { cols, rows: t.get(1..).unwrap_or(&[]) }
    }
    fn get(&self, row: &[String], col: &str) -> String {
        self.cols.get(&norm(col)).and_then(|&i| row.get(i)).map(|s| s.trim().to_string()).unwrap_or_default()
    }
    fn missing(&self, kind: &str) -> Vec<&'static str> {
        let required: &[&str] = match kind {
            "subjects" | "teachers" | "classes" | "rooms" => &["Name", "Short name"],
            _ => &["Subject", "Classes", "Sessions per week"],
        };
        required.iter().copied().filter(|c| !self.cols.contains_key(&norm(c))).collect()
    }
}

fn find<'a, T>(items: &'a [T], key: &str, names: impl Fn(&T) -> (&str, &str)) -> Option<&'a T> {
    let k = norm(key);
    items.iter().find(|x| {
        let (n, s) = names(x);
        norm(n) == k || norm(s) == k
    })
}

fn number(s: &str) -> Result<Option<u32>, ()> {
    if s.is_empty() {
        return Ok(None);
    }
    s.parse::<f64>().ok().filter(|f| *f >= 0.0 && f.fract() == 0.0).map(|f| Some(f as u32)).ok_or(())
}

pub fn apply(mut setup: SchoolSetup, tables: Vec<(String, Table)>) -> ImportReport {
    let mut added = Counts::default();
    let mut skipped = 0;
    let mut problems = Vec::new();
    let mut id = 0u32;
    let mut new_id = |kind: &str| {
        id += 1;
        format!("{kind}-{}-{id}", std::process::id() ^ (setup_nonce()))
    };
    let enabled: Vec<(String, String)> =
        setup.shifts.iter().filter(|s| s.enabled).map(|s| (s.id.clone(), s.name.clone())).collect();
    let shift_of = |name: &str| enabled.iter().find(|(i, n)| norm(i) == norm(name) || norm(n) == norm(name)).map(|(i, _)| i.clone());

    for kind in KINDS {
        let Some((_, table)) = tables.iter().find(|(k, _)| k == kind) else { continue };
        let sheet = sheet_title(kind).to_string();
        let rows = Rows::new(table);
        if table.is_empty() {
            continue;
        }
        let miss = rows.missing(kind);
        if !miss.is_empty() {
            problems.push(Problem { sheet, row: 1, message: format!("The heading row is missing: {}.", miss.join(", ")) });
            continue;
        }
        for (ri, row) in rows.rows.iter().enumerate() {
            if row.iter().all(|c| c.trim().is_empty()) {
                continue;
            }
            let rn = ri + 2;
            let mut bad = |m: String| problems.push(Problem { sheet: sheet.clone(), row: rn, message: m });
            let m = &mut setup.master;
            match kind {
                "subjects" => {
                    let (name, short) = (rows.get(row, "Name"), rows.get(row, "Short name"));
                    if name.is_empty() || short.is_empty() {
                        bad("Name and Short name are both required.".into());
                    } else if find(&m.subjects, &name, |x| (&x.name, &x.short_name)).is_some() {
                        skipped += 1;
                    } else {
                        let c = rows.get(row, "Colour");
                        let color = if valid_colour(&c) { c.to_uppercase() } else { PALETTE[m.subjects.len() % 12].to_string() };
                        m.subjects.push(Subject { id: new_id("sub"), name, short_name: short, color });
                        added.subjects += 1;
                    }
                }
                "teachers" => {
                    let (name, short) = (rows.get(row, "Name"), rows.get(row, "Short name"));
                    if name.is_empty() || short.is_empty() {
                        bad("Name and Short name are both required.".into());
                        continue;
                    }
                    if find(&m.teachers, &name, |x| (&x.name, &x.short_name)).is_some() {
                        skipped += 1;
                        continue;
                    }
                    let mut shifts = Vec::new();
                    let listed = split_list(&rows.get(row, "Shifts"));
                    let mut ok = true;
                    for s in &listed {
                        match shift_of(s) {
                            Some(i) => shifts.push(i),
                            None => {
                                bad(format!("Shift \"{s}\" is not one of the school's shifts."));
                                ok = false;
                            }
                        }
                    }
                    let mut lim = [None; 4];
                    for (i, (h, old)) in [("Most periods per day", "Most lessons per day"), ("Most periods in a row", "Most lessons in a row"), ("Most days per week", "Most days per week"), ("Most periods per week", "Most lessons per week")].iter().enumerate() {
                        // files made with the older column names still work
                        let text = Some(rows.get(row, h)).filter(|v| !v.is_empty()).unwrap_or_else(|| rows.get(row, old));
                        match number(&text) {
                            Ok(v) => lim[i] = v,
                            Err(()) => {
                                bad(format!("\"{h}\" must be a whole number."));
                                ok = false;
                            }
                        }
                    }
                    if !ok {
                        continue;
                    }
                    if listed.is_empty() {
                        shifts = enabled.iter().map(|(i, _)| i.clone()).collect();
                    }
                    m.teachers.push(Teacher {
                        id: new_id("tea"), name, short_name: short, email: rows.get(row, "Email"), shifts,
                        max_lessons_per_day: lim[0], max_consecutive: lim[1], max_days_per_week: lim[2], max_periods_per_week: lim[3], time_off: vec![],
                    });
                    added.teachers += 1;
                }
                "rooms" => {
                    let (name, short) = (rows.get(row, "Name"), rows.get(row, "Short name"));
                    if name.is_empty() || short.is_empty() {
                        bad("Name and Short name are both required.".into());
                        continue;
                    }
                    if find(&m.rooms, &name, |x| (&x.name, &x.short_name)).is_some() {
                        skipped += 1;
                        continue;
                    }
                    let Ok(capacity) = number(&rows.get(row, "Capacity")) else {
                        bad("\"Capacity\" must be a whole number.".into());
                        continue;
                    };
                    let b = rows.get(row, "Building");
                    let building = if b.is_empty() {
                        None
                    } else if let Some(x) = m.buildings.iter().find(|x| norm(&x.name) == norm(&b)) {
                        Some(x.id.clone())
                    } else {
                        let bid = new_id("bld");
                        m.buildings.push(Building { id: bid.clone(), name: b });
                        Some(bid)
                    };
                    m.rooms.push(Room { id: new_id("room"), name, short_name: short, building, capacity, time_off: vec![] });
                    added.rooms += 1;
                }
                "classes" => {
                    let (name, short) = (rows.get(row, "Name"), rows.get(row, "Short name"));
                    if name.is_empty() || short.is_empty() {
                        bad("Name and Short name are both required.".into());
                        continue;
                    }
                    if find(&m.classes, &name, |x| (&x.name, &x.short_name)).is_some() {
                        skipped += 1;
                        continue;
                    }
                    let sh = rows.get(row, "Shift");
                    let shift = if sh.is_empty() && enabled.len() == 1 {
                        enabled[0].0.clone()
                    } else if let Some(i) = shift_of(&sh) {
                        i
                    } else {
                        bad(if sh.is_empty() { "Please give the class a shift.".into() } else { format!("Shift \"{sh}\" is not one of the school's shifts.") });
                        continue;
                    };
                    let mut ok = true;
                    let t = rows.get(row, "Class teacher");
                    let class_teacher = if t.is_empty() {
                        None
                    } else if let Some(x) = find(&m.teachers, &t, |x| (&x.name, &x.short_name)) {
                        Some(x.id.clone())
                    } else {
                        bad(format!("Class teacher \"{t}\" was not found. Import the teachers first."));
                        ok = false;
                        None
                    };
                    let r = rows.get(row, "Home classroom");
                    let home_room = if r.is_empty() {
                        None
                    } else if let Some(x) = find(&m.rooms, &r, |x| (&x.name, &x.short_name)) {
                        Some(x.id.clone())
                    } else {
                        bad(format!("Home classroom \"{r}\" was not found. Import the rooms first."));
                        ok = false;
                        None
                    };
                    if ok {
                        m.classes.push(Class {
                            id: new_id("cls"), name, short_name: short, shift, class_teacher, home_room,
                            divisions: vec![], time_off: vec![],
                        });
                        added.classes += 1;
                    }
                }
                _ => {
                    let mut ok = true;
                    let sname = rows.get(row, "Subject");
                    let subject = match find(&m.subjects, &sname, |x| (&x.name, &x.short_name)) {
                        Some(x) => x.id.clone(),
                        None => {
                            bad(format!("Subject \"{sname}\" was not found."));
                            ok = false;
                            String::new()
                        }
                    };
                    let mut teachers = Vec::new();
                    let known_teacher = |n: &str| find(&m.teachers, n, |x| (&x.name, &x.short_name)).is_some();
                    for t in split_names(&rows.get(row, "Teachers"), known_teacher) {
                        match find(&m.teachers, &t, |x| (&x.name, &x.short_name)) {
                            Some(x) => teachers.push(x.id.clone()),
                            None => {
                                bad(format!("Teacher \"{t}\" was not found."));
                                ok = false;
                            }
                        }
                    }
                    let mut rooms = Vec::new();
                    let known_room = |n: &str| find(&m.rooms, n, |x| (&x.name, &x.short_name)).is_some();
                    for r in split_names(&rows.get(row, "Rooms"), known_room) {
                        match find(&m.rooms, &r, |x| (&x.name, &x.short_name)) {
                            Some(x) => rooms.push(x.id.clone()),
                            None => {
                                bad(format!("Room \"{r}\" was not found."));
                                ok = false;
                            }
                        }
                    }
                    let mut parts = Vec::new();
                    let known_class = |n: &str| find(&m.classes, n.split(':').next().unwrap_or(n), |x| (&x.name, &x.short_name)).is_some();
                    for item in split_names(&rows.get(row, "Classes"), known_class) {
                        let (cn, gn) = match item.split_once(':') {
                            Some((c, g)) => (c.trim(), Some(g.trim())),
                            None => (item.as_str(), None),
                        };
                        let Some(c) = find(&m.classes, cn, |x| (&x.name, &x.short_name)) else {
                            bad(format!("Class \"{cn}\" was not found."));
                            ok = false;
                            continue;
                        };
                        let group_id = match gn {
                            None => None,
                            Some(g) => match c.divisions.iter().flat_map(|d| &d.groups).find(|x| norm(&x.name) == norm(g)) {
                                Some(x) => Some(x.id.clone()),
                                None => {
                                    bad(format!("Class \"{}\" has no group called \"{g}\". Add the group in Classes first.", c.name));
                                    ok = false;
                                    continue;
                                }
                            },
                        };
                        parts.push(LessonPart { class_id: c.id.clone(), group_id });
                    }
                    if parts.is_empty() && ok {
                        bad("Please give at least one class.".into());
                        ok = false;
                    }
                    let ppc = number(&rows.get(row, "Periods per session")).map(|v| v.unwrap_or(1));
                    let cpw = number(&rows.get(row, "Sessions per week")).map(|v| v.unwrap_or(0));
                    match (ppc, cpw) {
                        (Ok(p), Ok(c)) if (1..=3).contains(&p) && c >= 1 => {
                            // The same lesson twice (importing a file again) is left alone.
                            let same = |x: &Lesson| {
                                let eq = |a: &[String], b: &[String]| a.len() == b.len() && a.iter().all(|i| b.contains(i));
                                x.subject == subject && x.periods_per_card == p && x.cards_per_week == c
                                    && eq(&x.teachers, &teachers) && eq(&x.rooms, &rooms)
                                    && x.parts.len() == parts.len() && parts.iter().all(|q| x.parts.contains(q))
                            };
                            if ok && setup.lessons.iter().any(same) {
                                skipped += 1;
                            } else if ok {
                                m_push_lesson(&mut setup, Lesson {
                                    id: new_id("les"), subject, teachers, parts, periods_per_card: p, cards_per_week: c, rooms,
                                });
                                added.lessons += 1;
                            }
                        }
                        (Ok(p), Ok(_)) if !(1..=3).contains(&p) => bad("Periods per session must be 1, 2 or 3.".into()),
                        _ => bad("Sessions per week must be a whole number, 1 or more.".into()),
                    }
                }
            }
        }
    }
    ImportReport { setup, added, skipped, problems }
}

fn m_push_lesson(s: &mut SchoolSetup, l: Lesson) {
    s.lessons.push(l);
}

fn setup_nonce() -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.subsec_nanos())
}

// ---------- reading files ----------

/// Turns CSV bytes into text. UTF-8 is expected; files saved by older Excel / Notepad on Windows
/// use Windows-1252, so anything that is not valid UTF-8 is read that way instead.
pub fn decode_csv_bytes(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(t) => t.to_string(),
        Err(_) => {
            const HIGH: [char; 32] = [
                '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}',
                '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
            ];
            bytes
                .iter()
                .map(|&b| match b {
                    0x80..=0x9f => HIGH[(b - 0x80) as usize],
                    _ => b as char, // 0x00-0x7f and 0xa0-0xff match Unicode directly
                })
                .collect()
        }
    }
}

pub fn read_csv_text(text: &str) -> Result<Table, String> {
    let text = text.trim_start_matches('\u{feff}');
    let first = text.lines().next().unwrap_or("");
    let delim = if first.contains(';') && !first.contains(',') { b';' } else { b',' };
    let mut rd = csv::ReaderBuilder::new().has_headers(false).flexible(true).delimiter(delim).from_reader(text.as_bytes());
    let mut out = Vec::new();
    for r in rd.records() {
        let r = r.map_err(|e| format!("This file could not be read as CSV: {e}"))?;
        out.push(r.iter().map(|c| c.to_string()).collect());
    }
    Ok(out)
}

pub fn read_file(path: &Path, csv_kind: Option<&str>) -> Result<Vec<(String, Table)>, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if ext == "csv" {
        let kind = csv_kind.filter(|k| KINDS.contains(k)).ok_or("Please say what this CSV file contains.")?;
        let bytes = std::fs::read(path).map_err(|e| format!("Could not open the file: {e}"))?;
        let text = decode_csv_bytes(&bytes);
        return Ok(vec![(kind.to_string(), read_csv_text(&text)?)]);
    }
    // Spreadsheets are read by the API crate (which carries the Excel readers); the core
    // engine imports CSV only.
    Err("Only .csv files are read here; spreadsheets go through the import wizard.".into())
}

// ---------- templates ----------

pub fn csv_template(kind: &str) -> String {
    format!("{}\n", headers(kind).join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn csv(kind: &str, text: &str) -> (String, Table) {
        (kind.to_string(), read_csv_text(text).unwrap())
    }
    fn base() -> SchoolSetup {
        SchoolSetup::default_new("T")
    }

    #[test]
    fn reads_windows_1252_csv_files() {
        // "Café" and "Zoë – Ali" saved by Notepad/Excel as ANSI
        assert_eq!(decode_csv_bytes(b"Caf\xe9"), "Café");
        assert_eq!(decode_csv_bytes(b"Zo\xeb \x96 Ali"), "Zoë – Ali");
        assert_eq!(decode_csv_bytes("Café".as_bytes()), "Café");
    }

    #[test]
    fn names_with_commas_survive_import() {
        // "Khan, Ayesha" is how the Excel export writes a teacher; the lessons list must find her again.
        let r = apply(base(), vec![
            csv("lessons", "Subject,Teachers,Classes,Periods per session,Sessions per week,Rooms\nMaths,\"Khan, Ayesha\",\"Grade 6, A\",1,5,\"Lab, North\"\nMaths,\"Khan, Ayesha; BOB\",\"Grade 6, A\",1,2,\n"),
            csv("classes", "Name,Short name,Shift,Class teacher,Home classroom\n\"Grade 6, A\",6A,Morning,,\n"),
            csv("rooms", "Name,Short name,Building,Capacity\n\"Lab, North\",LN,Main,30\n"),
            csv("teachers", "Name,Short name,Email,Shifts,Most lessons per day\n\"Khan, Ayesha\",AKH,,Morning,6\nBob,BOB,,,\n"),
            csv("subjects", "Name,Short name,Colour\nMaths,MA,#123456\n"),
        ]);
        assert!(r.problems.is_empty(), "{:?}", r.problems);
        assert_eq!(r.added.lessons, 2);
        assert_eq!(r.setup.lessons[0].teachers.len(), 1);
        assert_eq!(r.setup.lessons[0].rooms.len(), 1);
        assert_eq!(r.setup.lessons[1].teachers.len(), 2);
    }

    #[test]
    fn split_names_still_splits_plain_lists() {
        let known = |n: &str| n == "Khan, Ayesha";
        assert_eq!(split_names("Ms Ana, BOB; Cy", known), vec!["Ms Ana", "BOB", "Cy"]);
        assert_eq!(split_names("Khan, Ayesha, BOB", known), vec!["Khan, Ayesha", "BOB"]);
        assert_eq!(split_names("Khan, Zed", known), vec!["Khan", "Zed"]);
        assert!(split_names(" , ", known).is_empty());
    }

    #[test]
    fn imports_everything_in_order_and_links_by_name() {
        let r = apply(base(), vec![
            csv("lessons", "Subject,Teachers,Classes,Periods per session,Sessions per week,Rooms\nMaths,\"Ms Ana, BOB\",5A,1,5,Lab\n"),
            csv("classes", "Name,Short name,Shift,Class teacher,Home classroom\nGrade 5A,5A,Morning,BOB,Lab\n"),
            csv("rooms", "Name,Short name,Building,Capacity\nScience Lab,Lab,Main,30\n"),
            csv("teachers", "Name,Short name,Email,Shifts,Most lessons per day\nMs Ana,ANA,,Morning,6\nBob Khan,BOB,,,\n"),
            csv("subjects", "Name,Short name,Colour\nMaths,MA,#123456\n"),
        ]);
        assert!(r.problems.is_empty(), "{:?}", r.problems);
        assert_eq!(r.added, Counts { subjects: 1, teachers: 2, classes: 1, rooms: 1, lessons: 1 });
        let s = &r.setup;
        assert_eq!(s.master.subjects[0].color, "#123456");
        assert_eq!(s.master.teachers[0].max_lessons_per_day, Some(6));
        assert_eq!(s.master.teachers[1].shifts, vec!["morning".to_string()]);
        assert_eq!(s.master.buildings.len(), 1);
        assert_eq!(s.lessons[0].teachers.len(), 2);
        assert_eq!(s.lessons[0].cards_per_week, 5);
        assert!(s.validate().is_empty(), "{:?}", s.validate());
    }

    #[test]
    fn teacher_limits_accept_the_new_and_the_older_column_names() {
        let first = apply(base(), vec![csv("teachers", "Name,Short name,Shifts,Most periods per day,Most periods per week\nMs Ana,ANA,,6,25\n")]);
        let r = apply(first.setup, vec![csv("teachers", "Name,Short name,Shifts,Most lessons per day,Most lessons in a row,Most lessons per week\nBob Khan,BOB,,5,3,20\n")]);
        assert!(first.problems.is_empty() && r.problems.is_empty(), "{:?} {:?}", first.problems, r.problems);
        let t = &r.setup.master.teachers;
        assert_eq!((t[0].max_lessons_per_day, t[0].max_periods_per_week), (Some(6), Some(25)));
        assert_eq!((t[1].max_lessons_per_day, t[1].max_consecutive, t[1].max_periods_per_week), (Some(5), Some(3), Some(20)));
    }

    #[test]
    fn reports_row_and_problem_and_skips_bad_rows() {
        let r = apply(base(), vec![
            csv("subjects", "Name,Short name\nMaths,MA\n,XX\nMaths,M2\nArt,AR\n"),
            csv("lessons", "Subject,Teachers,Classes,Periods per session,Sessions per week\nPhysics,,5A,1,2\nArt,,5A,4,2\nArt,,,1,1\n"),
        ]);
        assert_eq!(r.added.subjects, 2);
        assert_eq!(r.skipped, 1);
        assert_eq!(r.added.lessons, 0);
        let p: Vec<_> = r.problems.iter().map(|p| (p.sheet.as_str(), p.row, p.message.as_str())).collect();
        assert!(p.iter().any(|(s, row, m)| *s == "Subjects" && *row == 3 && m.contains("required")));
        assert!(p.iter().any(|(s, row, m)| *s == "Lessons" && *row == 2 && m.contains("Physics")));
        assert!(p.iter().any(|(_, row, m)| *row == 3 && m.contains("1, 2 or 3")));
    }

    #[test]
    fn missing_heading_is_explained() {
        let r = apply(base(), vec![csv("subjects", "Title,Code\nMaths,MA\n")]);
        assert!(r.problems[0].message.contains("heading"));
    }

    #[test]
    fn semicolon_csv_with_bom_works() {
        let t = read_csv_text("\u{feff}Name;Short name\nMaths;MA\n").unwrap();
        assert_eq!(t[1], vec!["Maths", "MA"]);
    }

}

#[cfg(test)]
mod sample_tests {
    use super::*;

    #[test]
    fn sample_school_imports_cleanly() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/timetable/samples");
        let tables = KINDS
            .iter()
            .map(|k| {
                let p = dir.join(format!("{k}.csv"));
                read_file(&p, Some(k)).unwrap().remove(0)
            })
            .collect();
        let r = apply(SchoolSetup::default_new("Sample School"), tables);
        assert!(r.problems.is_empty(), "{:?}", r.problems);
        assert_eq!(r.added, Counts { subjects: 9, teachers: 9, classes: 4, rooms: 7, lessons: 36 });
        assert!(r.setup.validate().is_empty(), "{:?}", r.setup.validate());
        let sum = crate::timetable::lessons::summarize(&r.setup);
        assert!(sum.classes.iter().all(|c| c.used <= c.available), "{:?}", sum.classes);
        assert!(sum.teachers.iter().all(|t| t.used <= t.available), "{:?}", sum.teachers);
        let check = crate::timetable::check::run(&r.setup);
        assert_eq!(check.errors, 0, "{:?}", check.findings);
    }

    #[test]
    fn importing_the_sample_twice_adds_nothing_the_second_time() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/timetable/samples");
        let load = || -> Vec<(String, Table)> {
            KINDS.iter().map(|k| read_file(&dir.join(format!("{k}.csv")), Some(k)).unwrap().remove(0)).collect()
        };
        let first = apply(SchoolSetup::default_new("S"), load());
        let second = apply(first.setup.clone(), load());
        assert_eq!(second.added, Counts::default());
        assert_eq!(second.skipped, 9 + 9 + 7 + 4 + 36);
        assert_eq!(second.setup.lessons.len(), 36);
    }
}
