use super::*;
use crate::timetable::lessons::{Lesson, LessonPart};
use crate::timetable::master::*;
use crate::timetable::model::{Break, PeriodTime, SchoolSetup};
use std::path::Path;

fn run(s: &SchoolSetup) -> SolveReport {
    run_with(s, &[], Options { time_limit: Duration::from_secs(60), ..Options::default() })
}

fn run_with(s: &SchoolSetup, locked: &[PlacedCard], o: Options) -> SolveReport {
    solve(s, locked, &o, &AtomicBool::new(false), &mut |_| {})
}

/// Solves, then insists on a complete timetable that passes the independent checker.
fn must_solve(s: &SchoolSetup) -> SolveReport {
    let r = run(s);
    assert!(r.unplaced.is_empty(), "unplaced: {:#?}", r.unplaced);
    assert_eq!(r.placed, r.total);
    let bad = verify::check(s, &r.timetable);
    assert!(bad.is_empty(), "{bad:#?}");
    r
}

fn teacher(id: &str) -> Teacher {
    Teacher { id: id.into(), name: id.into(), short_name: id.into(), shifts: vec!["morning".into()], ..Default::default() }
}
fn class(id: &str) -> Class {
    Class { id: id.into(), name: id.into(), short_name: id.into(), shift: "morning".into(), ..Default::default() }
}
fn room(id: &str) -> Room {
    Room { id: id.into(), name: id.into(), short_name: id.into(), ..Default::default() }
}
fn lesson(id: &str, t: &[&str], c: &[&str], len: u32, n: u32) -> Lesson {
    Lesson {
        id: id.into(), subject: "s".into(),
        teachers: t.iter().map(|x| x.to_string()).collect(),
        parts: c.iter().map(|x| LessonPart { class_id: x.to_string(), group_id: None }).collect(),
        periods_per_card: len, cards_per_week: n, rooms: vec![],
    }
}
fn off(day: u8, period: u8, state: u8) -> TimeOffCell {
    TimeOffCell { shift: "morning".into(), day, period, state }
}
/// Small school: Mon-Fri, `periods` periods a day, one morning shift, no breaks.
fn school(periods: usize, days: usize) -> SchoolSetup {
    let mut s = SchoolSetup::default_new("T");
    s.master.subjects.push(Subject { id: "s".into(), name: "Subj".into(), short_name: "S".into(), color: "#000".into() });
    for (i, d) in s.days.iter_mut().enumerate() {
        d.working = i < days;
    }
    for d in 0..7 {
        s.shifts[0].days[d] = (0..periods)
            .map(|i| PeriodTime { start: format!("{:02}:{:02}", 8 + i * 40 / 60, i * 40 % 60), end: format!("{:02}:{:02}", 8 + (i * 40 + 35) / 60, (i * 40 + 35) % 60) })
            .collect();
    }
    s.shifts[0].breaks.clear();
    s
}

// ---------- bitmask maths (blueprint section 4) ----------

fn grp(id: &str) -> Group {
    Group { id: id.into(), name: id.into() }
}
fn split_class() -> Class {
    let mut c = class("10A");
    c.divisions = vec![
        Division { id: "gender".into(), name: "Gender".into(), groups: vec![grp("boys"), grp("girls")] },
        Division { id: "lang".into(), name: "Language".into(), groups: vec![grp("french"), grp("german")] },
    ];
    c
}

#[test]
fn atomic_masks_match_the_blueprint() {
    let (atoms, g) = class_masks(&split_class()).unwrap();
    assert_eq!(atoms, 4);
    let get = |n: &str| g.iter().find(|(id, _)| id == n).unwrap().1;
    assert_eq!(get("boys"), 0b0101);
    assert_eq!(get("girls"), 0b1010);
    assert_eq!(get("french"), 0b0011);
    assert_eq!(get("german"), 0b1100);
    assert_eq!(get("boys") & get("girls"), 0, "same division never overlaps");
    assert_eq!(get("french") & get("german"), 0);
    assert_ne!(get("boys") & get("french"), 0, "different divisions always overlap");
    assert_eq!(get("boys") | get("girls"), 0b1111);
}

#[test]
fn class_without_divisions_has_one_bucket_and_huge_splits_are_refused() {
    assert_eq!(class_masks(&class("A")).unwrap().0, 1);
    let mut c = class("B");
    c.divisions = (0..7).map(|i| Division { id: format!("d{i}"), name: "d".into(), groups: vec![grp("a"), grp("b")] }).collect();
    assert!(class_masks(&c).is_none(), "2^7 = 128 buckets is too many");
}

#[test]
fn disjoint_groups_share_a_period_but_overlapping_ones_do_not() {
    // One period a day, one day: Boys+Girls PE can both go in it, Boys + French cannot.
    let mut s = school(1, 1);
    s.master.classes.push(split_class());
    for t in ["a", "b", "c"] {
        s.master.teachers.push(teacher(t));
    }
    let mut boys = lesson("boys", &["a"], &["10A"], 1, 1);
    boys.parts[0].group_id = Some("boys".into());
    let mut girls = lesson("girls", &["b"], &["10A"], 1, 1);
    girls.parts[0].group_id = Some("girls".into());
    s.lessons = vec![boys.clone(), girls];
    let r = must_solve(&s);
    assert_eq!(r.timetable.cards[0].period, r.timetable.cards[1].period);

    let mut french = lesson("french", &["c"], &["10A"], 1, 1);
    french.parts[0].group_id = Some("french".into());
    s.lessons = vec![boys, french];
    let r = run(&s);
    assert_eq!(r.placed, 1, "only one of them fits");
    assert_eq!(r.unplaced.len(), 1);
}

// ---------- hard constraints ----------

#[test]
fn teacher_is_never_in_two_places() {
    let mut s = school(6, 5);
    s.master.teachers.push(teacher("t"));
    for c in ["a", "b", "c", "d"] {
        s.master.classes.push(class(c));
        s.lessons.push(lesson(&format!("l{c}"), &["t"], &[c], 1, 5));
    }
    must_solve(&s); // 20 cards, 30 slots
}

#[test]
fn room_is_never_double_booked_and_forbidden_room_times_are_respected() {
    let mut s = school(4, 5);
    s.master.teachers.extend(["t1", "t2", "t3"].map(teacher));
    s.master.classes.extend(["a", "b", "c"].map(class));
    s.master.rooms.push(room("lab"));
    s.master.rooms[0].time_off = (1..=4).map(|p| off(0, p, 2)).collect(); // closed on Monday
    for (i, c) in ["a", "b", "c"].iter().enumerate() {
        let mut l = lesson(&format!("l{c}"), &[&format!("t{}", i + 1)], &[c], 1, 4);
        l.rooms = vec!["lab".into()];
        s.lessons.push(l);
    }
    let r = must_solve(&s);
    assert!(r.timetable.cards.iter().all(|c| c.day != 0 && c.room.as_deref() == Some("lab")));
}

#[test]
fn lesson_may_choose_between_rooms() {
    let mut s = school(1, 1);
    s.master.teachers.extend(["t1", "t2"].map(teacher));
    s.master.classes.extend(["a", "b"].map(class));
    s.master.rooms.extend(["r1", "r2"].map(room));
    for (t, c) in [("t1", "a"), ("t2", "b")] {
        let mut l = lesson(&format!("l{c}"), &[t], &[c], 1, 1);
        l.rooms = vec!["r1".into(), "r2".into()];
        s.lessons.push(l);
    }
    let r = must_solve(&s);
    assert_ne!(r.timetable.cards[0].room, r.timetable.cards[1].room);
}

#[test]
fn forbidden_time_off_is_never_used_by_teacher_class_or_room() {
    let mut s = school(5, 5);
    s.master.teachers.push(teacher("t"));
    s.master.classes.push(class("a"));
    s.master.teachers[0].time_off = (1..=5).map(|p| off(0, p, 2)).chain((1..=2).map(|p| off(1, p, 2))).collect();
    s.master.classes[0].time_off = vec![off(2, 1, 2), off(2, 2, 2), off(2, 3, 2)];
    s.lessons = vec![lesson("l", &["t"], &["a"], 1, 5)];
    let r = must_solve(&s);
    for c in &r.timetable.cards {
        assert!(c.day != 0 && !(c.day == 1 && c.period <= 2) && !(c.day == 2 && c.period <= 3), "{c:?}");
    }
}

#[test]
fn avoid_cells_are_used_only_when_needed() {
    let mut s = school(2, 1);
    s.master.teachers.push(teacher("t"));
    s.master.classes.push(class("a"));
    s.master.teachers[0].time_off = vec![off(0, 1, 1)]; // would rather not teach period 1
    s.lessons = vec![lesson("l", &["t"], &["a"], 1, 1)];
    let r = must_solve(&s);
    assert_eq!(r.timetable.cards[0].period, 2, "free choice avoids the 'avoid' cell");
    s.lessons = vec![lesson("l", &["t"], &["a"], 1, 2)];
    must_solve(&s); // both periods needed, so the avoid cell is allowed
}

#[test]
fn doubles_do_not_cross_a_break_but_may_when_allowed() {
    let mut s = school(4, 1);
    s.shifts[0].breaks = vec![Break { after_period: 2, name: "Break".into(), cannot_cross: true }];
    s.master.teachers.push(teacher("t"));
    s.master.classes.push(class("a"));
    s.lessons = vec![lesson("l", &["t"], &["a"], 2, 2)];
    let r = must_solve(&s);
    let mut ps: Vec<u8> = r.timetable.cards.iter().map(|c| c.period).collect();
    ps.sort();
    assert_eq!(ps, vec![1, 3], "the only two double slots avoid the break");
    // Three doubles cannot fit in 4 periods anyway; a triple cannot avoid the break.
    s.lessons = vec![lesson("l", &["t"], &["a"], 3, 1)];
    let r = run(&s);
    assert_eq!(r.placed, 0);
    assert!(r.unplaced[0].message.contains("no time when everyone"), "{}", r.unplaced[0].message);
    // Allow crossing: now it fits.
    s.shifts[0].breaks[0].cannot_cross = false;
    must_solve(&s);
}

#[test]
fn doubles_stay_inside_the_day_and_on_free_periods() {
    let mut s = school(5, 5);
    s.master.teachers.push(teacher("t"));
    s.master.classes.push(class("a"));
    s.master.teachers[0].time_off = vec![off(0, 3, 2), off(1, 3, 2)];
    s.lessons = vec![lesson("l", &["t"], &["a"], 2, 4), lesson("l2", &["t"], &["a"], 3, 1)];
    let r = must_solve(&s);
    for c in &r.timetable.cards {
        assert!(c.period as u32 + c.length - 1 <= 5);
    }
}

#[test]
fn sessions_of_one_lesson_land_on_different_days() {
    let mut s = school(8, 5);
    s.master.teachers.push(teacher("t"));
    s.master.classes.push(class("a"));
    s.lessons = vec![lesson("l", &["t"], &["a"], 1, 5)];
    let r = must_solve(&s);
    let mut days: Vec<u8> = r.timetable.cards.iter().map(|c| c.day).collect();
    days.sort();
    days.dedup();
    assert_eq!(days.len(), 5);
}

#[test]
fn more_sessions_than_days_is_allowed_to_share_a_day() {
    let mut s = school(8, 2);
    s.master.teachers.push(teacher("t"));
    s.master.classes.push(class("a"));
    s.lessons = vec![lesson("l", &["t"], &["a"], 1, 4)];
    must_solve(&s);
}

#[test]
fn teacher_limits_are_hard() {
    let mut s = school(8, 5);
    s.master.classes.extend(["a", "b", "c"].map(class));
    let mut t = teacher("t");
    t.max_lessons_per_day = Some(3);
    t.max_consecutive = Some(2);
    t.max_days_per_week = Some(3);
    s.master.teachers.push(t);
    for c in ["a", "b", "c"] {
        s.lessons.push(lesson(&format!("l{c}"), &["t"], &[c], 1, 3));
    }
    let r = must_solve(&s); // 9 lessons: exactly 3 days x 3
    let days: std::collections::BTreeSet<u8> = r.timetable.cards.iter().map(|c| c.day).collect();
    assert_eq!(days.len(), 3);
}

#[test]
fn impossible_limits_are_reported_not_broken() {
    let mut s = school(8, 5);
    s.master.classes.push(class("a"));
    let mut t = teacher("t");
    t.max_days_per_week = Some(1);
    t.max_lessons_per_day = Some(2);
    s.master.teachers.push(t);
    s.lessons = vec![lesson("l", &["t"], &["a"], 1, 3)];
    let r = run(&s);
    assert_eq!(r.placed, 2);
    assert_eq!(r.unplaced.len(), 1);
    assert!(verify::check(&s, &r.timetable).is_empty());
    assert!(r.unplaced[0].message.contains("daily limits") || r.unplaced[0].message.contains("t"), "{}", r.unplaced[0].message);
}

#[test]
fn co_teaching_and_merged_classes_need_everyone_free() {
    let mut s = school(3, 1);
    s.master.teachers.extend(["t1", "t2"].map(teacher));
    s.master.classes.extend(["a", "b"].map(class));
    s.lessons = vec![
        lesson("joint", &["t1", "t2"], &["a", "b"], 1, 1),
        lesson("onlya", &["t1"], &["a"], 1, 1),
        lesson("onlyb", &["t2"], &["b"], 1, 1),
    ];
    let r = must_solve(&s);
    let p = |id: &str| r.timetable.cards.iter().find(|c| c.lesson_id == id).unwrap().period;
    assert!(p("joint") != p("onlya") && p("joint") != p("onlyb"));
}

#[test]
fn double_shift_teacher_can_work_both_shifts_with_one_day_limit() {
    let mut s = school(3, 1);
    s.shifts[1].enabled = true;
    for d in 0..7 {
        s.shifts[1].days[d] = s.shifts[1].days[d].iter().take(3).cloned().collect();
    }
    s.shifts[1].breaks.clear();
    let mut t = teacher("t");
    t.shifts = vec!["morning".into(), "afternoon".into()];
    t.max_lessons_per_day = Some(5);
    s.master.teachers.push(t);
    s.master.classes.push(class("am"));
    let mut pm = class("pm");
    pm.shift = "afternoon".into();
    s.master.classes.push(pm);
    s.lessons = vec![lesson("x", &["t"], &["am"], 1, 3), lesson("y", &["t"], &["pm"], 1, 2)];
    let r = must_solve(&s);
    assert_eq!(r.timetable.cards.iter().filter(|c| c.shift == "afternoon").count(), 2);
    // a 6th period in the same day exceeds the daily limit
    s.lessons[1].cards_per_week = 3;
    s.lessons[1].parts[0].group_id = None;
    s.master.teachers[0].max_lessons_per_day = Some(5);
    let r = run(&s);
    assert_eq!(r.placed, 5);
    assert_eq!(r.unplaced.len(), 1);
}

#[test]
fn friday_has_fewer_periods() {
    let mut s = SchoolSetup::default_new("T"); // Mon-Thu 7 periods, Fri 5
    s.master.subjects.push(Subject { id: "s".into(), name: "S".into(), short_name: "S".into(), color: "#000".into() });
    s.master.teachers.push(teacher("t"));
    s.master.classes.push(class("a"));
    s.lessons = vec![lesson("l", &["t"], &["a"], 1, 5), lesson("m", &["t"], &["a"], 2, 5)];
    // 15 periods; Friday has only 5
    let r = must_solve(&s);
    for c in &r.timetable.cards {
        if c.day == 4 {
            assert!(c.period as u32 + c.length - 1 <= 5);
        }
    }
}

// ---------- search power ----------

/// n classes x n teachers, every pair meets once, in a week with n periods on one day:
/// a Latin square with no free slot at all. Greedy placement gets stuck; chains must fix it.
fn latin(n: usize) -> SchoolSetup {
    let mut s = school(n, 1);
    for i in 0..n {
        s.master.teachers.push(teacher(&format!("t{i}")));
        s.master.classes.push(class(&format!("c{i}")));
    }
    for i in 0..n {
        for j in 0..n {
            s.lessons.push(lesson(&format!("l{i}_{j}"), &[&format!("t{i}")], &[&format!("c{j}")], 1, 1));
        }
    }
    s
}

#[test]
fn completely_full_latin_squares_are_solved() {
    for n in [5, 8, 10] {
        let s = latin(n);
        let r = must_solve(&s);
        assert_eq!(r.placed as usize, n * n);
    }
}

#[test]
fn different_seeds_give_valid_timetables() {
    let s = latin(7);
    for seed in 1..=5 {
        let r = run_with(&s, &[], Options { seed, ..Options::default() });
        assert!(r.unplaced.is_empty());
        assert!(verify::check(&s, &r.timetable).is_empty());
    }
}

// ---------- locked cards, cancel, reports ----------

#[test]
fn locked_cards_stay_exactly_where_they_are() {
    let mut s = school(5, 5);
    s.master.teachers.push(teacher("t"));
    s.master.classes.push(class("a"));
    s.lessons = vec![lesson("l", &["t"], &["a"], 1, 5), lesson("m", &["t"], &["a"], 1, 5)];
    let lock = PlacedCard { lesson_id: "l".into(), number: 1, shift: "morning".into(), day: 3, period: 4, length: 1, room: None, locked: true };
    let r = run_with(&s, std::slice::from_ref(&lock), Options::default());
    assert!(r.unplaced.is_empty());
    assert!(r.timetable.cards.iter().any(|c| c.lesson_id == "l" && c.number == 1 && c.day == 3 && c.period == 4 && c.locked));
    assert!(verify::check(&s, &r.timetable).is_empty());
}

#[test]
fn a_bad_locked_card_is_reported_in_plain_english() {
    let mut s = school(5, 5);
    s.master.teachers.push(teacher("t"));
    s.master.classes.push(class("a"));
    s.master.teachers[0].time_off = vec![off(0, 1, 2)];
    s.lessons = vec![lesson("l", &["t"], &["a"], 1, 1)];
    let lock = PlacedCard { lesson_id: "l".into(), number: 1, shift: "morning".into(), day: 0, period: 1, length: 1, room: None, locked: true };
    let r = run_with(&s, &[lock], Options::default());
    assert_eq!(r.unplaced.len(), 1);
    assert!(r.unplaced[0].message.contains("locked"));
}

fn lock_at(lesson: &str, day: u8, period: u8) -> PlacedCard {
    PlacedCard { lesson_id: lesson.into(), number: 1, shift: "morning".into(), day, period, length: 1, room: None, locked: true }
}

#[test]
fn two_locked_lessons_that_clash_name_each_other() {
    let mut s = school(5, 5);
    s.master.teachers.extend(["t", "u"].map(teacher));
    s.master.classes.push(class("a"));
    s.lessons = vec![lesson("l", &["t"], &["a"], 1, 1), lesson("m", &["u"], &["a"], 1, 1)];
    // both locked on Monday period 1, for the same class
    let r = run_with(&s, &[lock_at("l", 0, 1), lock_at("m", 0, 1)], Options::default());
    assert_eq!(r.unplaced.len(), 1);
    let msg = &r.unplaced[0].message;
    assert!(msg.contains("cannot stay where it is") && msg.contains("clashes with the locked lesson") && msg.contains("Monday period 1"), "{msg}");
    assert_eq!(r.locks_in_the_way.len(), 1);
    assert_eq!(r.locks_in_the_way[0].when, "Monday period 1");
    assert_eq!(r.locks_in_the_way[0].blocks, 1);
}

#[test]
fn when_locks_alone_make_a_lesson_impossible_they_are_named() {
    // One teacher, one day of two periods: two locked lessons take both, a third cannot go anywhere.
    let mut s = school(2, 1);
    s.master.teachers.push(teacher("t"));
    s.master.classes.extend(["a", "b", "c"].map(class));
    s.lessons = vec![lesson("p", &["t"], &["a"], 1, 1), lesson("q", &["t"], &["b"], 1, 1), lesson("r", &["t"], &["c"], 1, 1)];
    let r = run_with(&s, &[lock_at("p", 0, 1), lock_at("q", 0, 2)], Options { time_limit: Duration::from_secs(2), ..Options::default() });
    assert_eq!(r.unplaced.len(), 1);
    assert_eq!(r.unplaced[0].lesson_id, "r");
    let msg = &r.unplaced[0].message;
    assert!(msg.contains("the locked lessons already take every time it could use"), "{msg}");
    assert!(msg.contains("Monday period 1") && msg.contains("Monday period 2"), "{msg}");
    assert!(r.unplaced[0].fix.contains("Unlock"));
    let mut named: Vec<&str> = r.locks_in_the_way.iter().map(|b| b.sessions[0].lesson_id.as_str()).collect();
    named.sort();
    assert_eq!(named, ["p", "q"]);
    assert!(r.locks_in_the_way.iter().all(|b| b.blocks == 1 && b.sessions[0].number == 1));
    // unlocking them makes room: without locks only one of the three cannot fit, and no lock is blamed
    let r = run_with(&s, &[], Options { time_limit: Duration::from_secs(2), ..Options::default() });
    assert_eq!(r.unplaced.len(), 1);
    assert!(r.locks_in_the_way.is_empty());
    assert!(!r.unplaced[0].message.contains("ocked"));
}

#[test]
fn a_lock_that_is_part_of_the_problem_is_mentioned() {
    // The lock takes period 1; period 2 is free for only one of the two others.
    let mut s = school(2, 1);
    s.master.teachers.push(teacher("t"));
    s.master.classes.extend(["a", "b", "c"].map(class));
    s.lessons = vec![lesson("p", &["t"], &["a"], 1, 1), lesson("q", &["t"], &["b"], 1, 1), lesson("r", &["t"], &["c"], 1, 1)];
    let r = run_with(&s, &[lock_at("p", 0, 1)], Options { time_limit: Duration::from_secs(2), ..Options::default() });
    assert_eq!(r.unplaced.len(), 1);
    let msg = &r.unplaced[0].message;
    assert!(msg.contains("could not be fitted in") && msg.contains("Locked lessons in the way") && msg.contains("Monday period 1"), "{msg}");
    assert_eq!(r.locks_in_the_way.len(), 1);
    assert_eq!(r.locks_in_the_way[0].sessions[0].lesson_id, "p");
}

#[test]
fn amber_slots_name_the_nice_to_have_rule_they_break() {
    let mut s = school(5, 5);
    s.master.teachers.extend(["t", "u"].map(teacher));
    s.master.classes.extend(["a", "b", "c"].map(class));
    s.lessons = vec![lesson("x", &["t"], &["a"], 1, 1), lesson("y", &["t"], &["b"], 1, 1), lesson("z", &["u"], &["c"], 1, 2)];
    let at = |l: &str, number: u32, day: u8, period: u8| PlacedCard { lesson_id: l.into(), number, shift: "morning".into(), day, period, length: 1, room: None, locked: false };
    s.timetable = Some(Timetable { cards: vec![at("x", 1, 0, 1), at("z", 1, 2, 1)] });
    let slot = |s: &SchoolSetup, l: &str, n: u32, day: u8, period: u8| {
        edit::slot_options(s, l, n).unwrap().into_iter().find(|o| o.day == day && o.period == period).unwrap()
    };
    // t teaches Monday period 1: period 2 is fine, period 3 leaves a free period in between
    assert_eq!(slot(&s, "y", 1, 0, 2).status, "ok");
    let o = slot(&s, "y", 1, 0, 3);
    assert_eq!(o.status, "soft");
    assert!(o.reasons.iter().any(|r| r == "t would have a free period in the middle of the day."), "{:?}", o.reasons);
    // the rule switched off in Rules & limits: no longer amber
    let mut off = s.clone();
    off.settings.weights.teacher_gaps = 0;
    assert_eq!(slot(&off, "y", 1, 0, 3).status, "ok");
    // a second session of z the day before or after the first (Wednesday) is amber, Monday is fine
    let o = slot(&s, "z", 2, 1, 1);
    assert_eq!(o.status, "soft");
    assert!(o.reasons.iter().any(|r| r.contains("is on Wednesday")), "{:?}", o.reasons);
    assert_eq!(slot(&s, "z", 2, 3, 1).status, "soft");
    assert_eq!(slot(&s, "z", 2, 0, 1).status, "ok");
    // a clash stays red, with its reason
    let o = slot(&s, "y", 1, 0, 1);
    assert_eq!(o.status, "blocked");
    assert!(o.reasons[0].contains("t is already teaching"), "{:?}", o.reasons);
}

#[test]
fn cancel_stops_quickly_and_returns_what_was_built() {
    let s = latin(10);
    let cancel = AtomicBool::new(true);
    let r = solve(&s, &[], &Options::default(), &cancel, &mut |_| {});
    assert!(r.cancelled);
    assert!(verify::check(&s, &r.timetable).is_empty());
}

#[test]
fn time_limit_is_respected() {
    // Truly impossible (two classes, one teacher, one period) must not spin forever.
    let mut s = school(1, 1);
    s.master.teachers.push(teacher("t"));
    s.master.classes.extend(["a", "b"].map(class));
    s.lessons = vec![lesson("x", &["t"], &["a"], 1, 1), lesson("y", &["t"], &["b"], 1, 1)];
    let r = run_with(&s, &[], Options { time_limit: Duration::from_millis(300), ..Options::default() });
    assert_eq!(r.placed, 1);
    assert!(r.seconds < 5.0);
    assert!(r.unplaced[0].message.contains("t") && r.unplaced[0].fix.len() > 10);
}

#[test]
fn unplaced_report_names_the_blocking_teacher() {
    let mut s = school(2, 1);
    s.master.teachers.push(teacher("Busy"));
    s.master.classes.extend(["a", "b", "c"].map(class));
    s.lessons = (["a", "b", "c"]).iter().map(|c| lesson(&format!("l{c}"), &["Busy"], &[c], 1, 1)).collect();
    let r = run_with(&s, &[], Options { time_limit: Duration::from_secs(1), ..Options::default() });
    assert_eq!(r.unplaced.len(), 1);
    assert!(r.unplaced[0].message.contains("Busy"), "{}", r.unplaced[0].message);
}

#[test]
fn time_off_dead_end_is_explained() {
    let mut s = school(3, 1);
    s.master.teachers.push(teacher("Ms Zed"));
    s.master.classes.push(class("a"));
    s.master.teachers[0].time_off = (1..=3).map(|p| off(0, p, 2)).collect();
    s.lessons = vec![lesson("l", &["Ms Zed"], &["a"], 1, 1)];
    let r = run(&s);
    assert!(r.unplaced[0].message.contains("Ms Zed has only 0 possible start"), "{}", r.unplaced[0].message);
}

#[test]
fn broken_data_is_reported_not_a_crash() {
    let mut s = school(3, 1);
    s.master.classes.push(class("a"));
    s.lessons = vec![lesson("l", &["ghost"], &["a"], 1, 1), lesson("k", &[], &["nobody"], 1, 1)];
    let r = run(&s);
    assert_eq!(r.unplaced.len(), 2);
    let mut s2 = school(3, 1);
    s2.master.teachers.push(teacher("t"));
    s2.master.classes.push(class("a"));
    s2.lessons = vec![lesson("l", &["t"], &["a"], 5, 1)];
    assert_eq!(run(&s2).unplaced.len(), 1);
}

#[test]
fn progress_reaches_the_total() {
    let s = latin(6);
    let mut last = (0, 0);
    let r = solve(&s, &[], &Options::default(), &AtomicBool::new(false), &mut |p| last = (p.placed, p.total));
    assert_eq!(last, (r.placed as usize, 36));
}


// ---------- the sample school and a big school ----------

fn sample() -> SchoolSetup {
    use crate::timetable::importer::*;
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/timetable/samples");
    let tables = KINDS.iter().map(|k| read_file(&dir.join(format!("{k}.csv")), Some(k)).unwrap().remove(0)).collect();
    apply(SchoolSetup::default_new("Sample School"), tables).setup
}

#[test]
fn sample_school_generates_fully() {
    let s = sample();
    assert_eq!(crate::timetable::check::run(&s).errors, 0);
    let r = must_solve(&s);
    assert_eq!((r.total, r.placed), (108, 108)); // 27 sessions for each of the 4 classes
}

#[test]
fn sample_school_works_for_many_seeds() {
    let s = sample();
    for seed in 1..=10 {
        let r = run_with(&s, &[], Options { seed, ..Options::default() });
        assert!(r.unplaced.is_empty(), "seed {seed}: {:?}", r.unplaced);
        assert!(verify::check(&s, &r.timetable).is_empty(), "seed {seed}");
    }
}

#[test]
fn sample_school_respects_its_special_rules() {
    let s = sample();
    let r = must_solve(&s);
    // Computer Studies doubles sit in the lab, PE is on the playground, nobody shares either.
    for c in &r.timetable.cards {
        let l = s.lessons.iter().find(|l| l.id == c.lesson_id).unwrap();
        if l.periods_per_card == 2 {
            assert_eq!(c.room.as_deref(), l.rooms.first().map(|x| x.as_str()));
        }
    }
    // Hamza Noor works at most 4 days and 5 lessons a day.
    let hno = s.master.teachers.iter().find(|t| t.short_name == "HNO").unwrap();
    let mut per_day = std::collections::HashMap::<u8, u32>::new();
    for c in &r.timetable.cards {
        if s.lessons.iter().find(|l| l.id == c.lesson_id).unwrap().teachers.contains(&hno.id) {
            *per_day.entry(c.day).or_default() += c.length;
        }
    }
    assert!(per_day.len() <= 4 && per_day.values().all(|&n| n <= 5), "{per_day:?}");
}

/// 60 classes, ~128 teachers, 8 periods x 6 days (CLAUDE.md performance target).
fn big_school() -> SchoolSetup {
    let mut s = school(8, 6);
    let subjects = 12;
    for i in 0..128 {
        let mut t = teacher(&format!("t{i}"));
        t.max_consecutive = Some(4);
        t.max_lessons_per_day = Some(7);
        s.master.teachers.push(t);
    }
    for i in 0..60 {
        s.master.classes.push(class(&format!("c{i}")));
    }
    for r in 0..14 {
        s.master.rooms.push(room(&format!("lab{r}")));
    }
    for c in 0..60 {
        let grade = c / 4; // 15 grades x 4 sections
        for sub in 0..subjects {
            // each grade has its own 8-teacher pool (grades share teachers in pairs)
            let t = (grade / 2) * 16 + (sub % 8) * 2 + (c % 2);
            let (len, n) = match sub {
                0..=3 => (1, 5),
                4..=6 => (1, 3),
                7 => (2, 1),
                _ => (1, 3),
            };
            let mut l = lesson(&format!("l{c}_{sub}"), &[&format!("t{t}")], &[&format!("c{c}")], len, n);
            if sub == 7 {
                l.rooms = (0..14).map(|r| format!("lab{r}")).collect();
            }
            s.lessons.push(l);
        }
    }
    s
}

#[test]
fn big_school_finishes_well_inside_two_minutes() {
    let s = big_school();
    let chk = crate::timetable::check::run(&s);
    assert_eq!(chk.errors, 0, "{:?}", chk.findings.iter().filter(|f| f.level == "error").map(|f| &f.message).collect::<Vec<_>>());
    let r = run_with(&s, &[], Options { time_limit: Duration::from_secs(110), ..Options::default() });
    assert!(r.unplaced.is_empty(), "{} unplaced of {}: {:#?}", r.unplaced.len(), r.total, r.unplaced.iter().take(3).collect::<Vec<_>>());
    assert!(verify::check(&s, &r.timetable).is_empty());
    assert!(r.seconds < 110.0, "{}", r.seconds);
    eprintln!("big school: {} cards in {:.2}s", r.total, r.seconds);
}


#[test]
fn part_time_day_limit_lets_sessions_share_a_day_instead_of_failing() {
    let mut s = school(6, 5);
    s.master.classes.push(class("a"));
    let mut t = teacher("t");
    t.max_days_per_week = Some(4);
    s.master.teachers.push(t);
    s.lessons = vec![lesson("l", &["t"], &["a"], 1, 5)]; // 5 sessions but only 4 days allowed
    must_solve(&s);
}

/// Every lesson gets a subject of its own (so the "same subject per day" rule stays out of the way).
fn give_own_subjects(s: &mut SchoolSetup) {
    for l in &mut s.lessons {
        let id = format!("sub-{}", l.id);
        if !s.master.subjects.iter().any(|x| x.id == id) {
            s.master.subjects.push(Subject { id: id.clone(), name: id.clone(), short_name: id.clone(), color: "#000".into() });
        }
        l.subject = id;
    }
}

// ---------- manual editing (milestone 7) ----------

use super::edit::{is_consistent, move_card, slot_options};

fn status_at(opts: &[edit::SlotInfo], day: u8, period: u8) -> &edit::SlotInfo {
    opts.iter().find(|o| o.day == day && o.period == period).unwrap()
}

/// Two teachers, two classes, 3 periods x 2 days; "x" for class a is placed on Mon P1.
fn small_edit_school() -> SchoolSetup {
    let mut s = school(3, 2);
    s.master.teachers.extend(["t1", "t2"].map(teacher));
    s.master.classes.extend(["a", "b"].map(class));
    s.lessons = vec![lesson("x", &["t1"], &["a"], 1, 1), lesson("y", &["t1"], &["b"], 1, 1), lesson("z", &["t2"], &["a"], 1, 1)];
    give_own_subjects(&mut s);
    s
}

#[test]
fn slot_colours_explain_themselves() {
    let mut s = small_edit_school();
    s.master.teachers[0].time_off = vec![off(1, 3, 2)]; // t1 off Tue P3
    s.master.classes[1].time_off = vec![off(1, 2, 1)]; // class b would rather avoid Tue P2
    s.timetable = Some(Timetable { cards: vec![
        PlacedCard { lesson_id: "x".into(), number: 1, shift: "morning".into(), day: 0, period: 1, length: 1, room: None, locked: true },
        PlacedCard { lesson_id: "z".into(), number: 1, shift: "morning".into(), day: 0, period: 2, length: 1, room: None, locked: false },
    ] });
    let o = slot_options(&s, "y", 1).unwrap();
    assert_eq!(o.len(), 6);
    let red = status_at(&o, 0, 1);
    assert_eq!(red.status, "blocked");
    assert!(red.reasons[0].contains("t1 is already teaching") && red.reasons[0].contains("(locked)"), "{:?}", red.reasons);
    assert_eq!(status_at(&o, 1, 3).status, "blocked");
    assert!(status_at(&o, 1, 3).reasons[0].contains("not available"));
    assert_eq!(status_at(&o, 1, 2).status, "soft");
    assert_eq!(status_at(&o, 0, 2).status, "ok");
    assert_eq!(status_at(&o, 1, 1).status, "ok");
}

#[test]
fn moving_checks_the_rules_and_respects_locks() {
    let mut s = small_edit_school();
    s.timetable = Some(Timetable { cards: vec![
        PlacedCard { lesson_id: "x".into(), number: 1, shift: "morning".into(), day: 0, period: 1, length: 1, room: None, locked: false },
    ] });
    // y (same teacher) cannot take x's slot; a free one works
    assert!(move_card(&s, "y", 1, 0, 1, None).unwrap_err().contains("t1 is already teaching"));
    s.timetable = Some(move_card(&s, "y", 1, 1, 2, None).unwrap());
    assert_eq!(s.timetable.as_ref().unwrap().cards.len(), 2);
    // moving x to class a's other lesson z's... z is unplaced, so any free slot is fine
    s.timetable = Some(move_card(&s, "x", 1, 0, 3, None).unwrap());
    assert_eq!(s.timetable.as_ref().unwrap().cards.iter().filter(|c| c.lesson_id == "x").count(), 1, "moved, not duplicated");
    assert!(is_consistent(&s));
    // lock it
    s.timetable.as_mut().unwrap().cards.iter_mut().find(|c| c.lesson_id == "x").unwrap().locked = true;
    assert!(move_card(&s, "x", 1, 0, 1, None).unwrap_err().contains("locked"));
    // unknown things give plain errors, not crashes
    assert!(move_card(&s, "nope", 1, 0, 1, None).is_err());
    assert!(move_card(&s, "x", 9, 0, 1, None).is_err());
    assert!(move_card(&s, "y", 1, 6, 1, None).is_err());
}

#[test]
fn room_choice_is_offered_and_enforced() {
    let mut s = school(2, 1);
    s.master.teachers.extend(["t1", "t2"].map(teacher));
    s.master.classes.extend(["a", "b"].map(class));
    s.master.rooms.extend(["r1", "r2"].map(room));
    let mut l1 = lesson("l1", &["t1"], &["a"], 1, 1);
    l1.rooms = vec!["r1".into(), "r2".into()];
    let mut l2 = lesson("l2", &["t2"], &["b"], 1, 1);
    l2.rooms = vec!["r1".into(), "r2".into()];
    s.lessons = vec![l1, l2];
    s.timetable = Some(move_card(&s, "l1", 1, 0, 1, Some("r2".into())).unwrap());
    let o = slot_options(&s, "l2", 1).unwrap();
    let rooms = &status_at(&o, 0, 1).rooms;
    assert_eq!(rooms.iter().map(|r| (r.id.as_str(), r.free)).collect::<Vec<_>>(), vec![("r1", true), ("r2", false)]);
    assert!(move_card(&s, "l2", 1, 0, 1, Some("r2".into())).unwrap_err().contains("in use"));
    assert!(move_card(&s, "l2", 1, 0, 1, Some("zzz".into())).unwrap_err().contains("not allowed"));
    let tt = move_card(&s, "l2", 1, 0, 1, None).unwrap(); // automatic pick = the free room
    assert_eq!(tt.cards.iter().find(|c| c.lesson_id == "l2").unwrap().room.as_deref(), Some("r1"));
}

#[test]
fn every_green_or_yellow_move_on_the_sample_school_keeps_the_timetable_valid() {
    let mut s = sample();
    s.timetable = Some(must_solve(&s).timetable);
    let mut rng = 12345u64;
    let mut next = |n: usize| {
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((rng >> 33) as usize) % n
    };
    let mut moved = 0;
    for _ in 0..150 {
        let tt = s.timetable.clone().unwrap();
        let c = &tt.cards[next(tt.cards.len())];
        let opts = slot_options(&s, &c.lesson_id, c.number).unwrap();
        // its own slot is always allowed
        assert_ne!(status_at(&opts, c.day, c.period).status, "blocked", "{c:?}");
        let ok: Vec<_> = opts.iter().filter(|o| o.status != "blocked").collect();
        let o = ok[next(ok.len())];
        s.timetable = Some(move_card(&s, &c.lesson_id, c.number, o.day, o.period, None).unwrap());
        moved += 1;
        assert_eq!(verify::check(&s, s.timetable.as_ref().unwrap()), Vec::<String>::new());
    }
    assert_eq!(moved, 150);
    // and a red slot is always refused
    let tt = s.timetable.clone().unwrap();
    let c = &tt.cards[0];
    let opts = slot_options(&s, &c.lesson_id, c.number).unwrap();
    for o in opts.iter().filter(|o| o.status == "blocked").take(20) {
        assert!(move_card(&s, &c.lesson_id, c.number, o.day, o.period, None).is_err());
    }
}

#[test]
fn unplaced_sessions_can_be_dropped_into_an_empty_timetable() {
    let mut s = small_edit_school();
    s.timetable = None;
    let tt = move_card(&s, "x", 1, 0, 1, None).unwrap();
    assert_eq!(tt.cards.len(), 1);
}

#[test]
fn stale_timetables_are_noticed() {
    let mut s = small_edit_school();
    s.timetable = Some(Timetable { cards: vec![
        PlacedCard { lesson_id: "x".into(), number: 1, shift: "morning".into(), day: 0, period: 1, length: 1, room: None, locked: false },
    ] });
    assert!(is_consistent(&s));
    s.master.teachers[0].time_off = vec![off(0, 1, 2)];
    assert!(!is_consistent(&s));
    s.lessons.clear();
    assert!(!is_consistent(&s));
}

// ---------- swapping ----------

use super::edit::{swap_cards, swap_options};

fn placed(id: &str, day: u8, period: u8) -> PlacedCard {
    PlacedCard { lesson_id: id.into(), number: 1, shift: "morning".into(), day, period, length: 1, room: None, locked: false }
}
fn at(tt: &Timetable, id: &str) -> Option<(u8, u8)> {
    tt.cards.iter().find(|c| c.lesson_id == id).map(|c| (c.day, c.period))
}

#[test]
fn two_lessons_swap_places() {
    let mut s = small_edit_school(); // x(t1,a) y(t1,b) z(t2,a)
    s.timetable = Some(Timetable { cards: vec![placed("x", 0, 1), placed("z", 0, 2)] });
    let tt = swap_cards(&s, "x", 1, "z", 1).unwrap();
    assert_eq!((at(&tt, "x"), at(&tt, "z")), (Some((0, 2)), Some((0, 1))));
    s.timetable = Some(tt);
    assert!(verify::check(&s, s.timetable.as_ref().unwrap()).is_empty());
}

#[test]
fn a_swap_that_breaks_a_rule_is_refused_with_the_reason() {
    let mut s = small_edit_school(); // x(t1,a) y(t1,b) z(t2,a)
    s.lessons.push(lesson("w", &["t1"], &["a"], 1, 1));
    give_own_subjects(&mut s);
    s.timetable = Some(Timetable { cards: vec![placed("x", 0, 1), placed("z", 0, 2), placed("w", 0, 3)] });
    assert!(swap_cards(&s, "z", 1, "w", 1).is_ok());
    // t2 is off on Monday P3, so z can no longer take w's place
    s.master.teachers[1].time_off = vec![off(0, 3, 2)];
    let e = swap_cards(&s, "z", 1, "w", 1).unwrap_err();
    assert!(e.contains("t2") && e.contains("not available"), "{e}");
    let o = swap_options(&s, "z", 1).unwrap();
    let w = o.iter().find(|x| x.lesson_id == "w").unwrap();
    assert!(!w.ok && !w.reasons.is_empty());
    assert!(o.iter().find(|x| x.lesson_id == "x").unwrap().ok);
}

#[test]
fn locked_lessons_cannot_be_swapped() {
    let mut s = small_edit_school();
    let mut z = placed("z", 0, 2);
    z.locked = true;
    s.timetable = Some(Timetable { cards: vec![placed("x", 0, 1), z] });
    assert!(swap_cards(&s, "x", 1, "z", 1).unwrap_err().contains("locked"));
    assert!(!swap_options(&s, "x", 1).unwrap()[0].ok);
}

#[test]
fn dropping_an_unplaced_lesson_on_a_placed_one_replaces_it() {
    let mut s = small_edit_school();
    s.timetable = Some(Timetable { cards: vec![placed("z", 0, 2)] });
    let tt = swap_cards(&s, "x", 1, "z", 1).unwrap();
    assert_eq!(at(&tt, "x"), Some((0, 2)));
    assert_eq!(at(&tt, "z"), None, "the other lesson goes back to 'not placed'");
}

#[test]
fn swapping_a_double_with_a_single_keeps_lengths() {
    let mut s = school(4, 1);
    s.master.teachers.extend(["t1", "t2"].map(teacher));
    s.master.classes.extend(["a", "b"].map(class));
    s.lessons = vec![lesson("d", &["t1"], &["a"], 2, 1), lesson("e", &["t2"], &["b"], 1, 1)];
    let mut d = placed("d", 0, 1);
    d.length = 2;
    s.timetable = Some(Timetable { cards: vec![d, placed("e", 0, 4)] });
    let tt = swap_cards(&s, "d", 1, "e", 1).unwrap_err(); // double at P4 would run past the day
    assert!(tt.contains("past the end"), "{tt}");
    s.timetable.as_mut().unwrap().cards[1].period = 3;
    let tt = swap_cards(&s, "d", 1, "e", 1).unwrap();
    assert_eq!((at(&tt, "d"), at(&tt, "e")), (Some((0, 3)), Some((0, 1))));
    assert_eq!(tt.cards.iter().find(|c| c.lesson_id == "d").unwrap().length, 2);
}

#[test]
fn swapping_keeps_or_finds_rooms() {
    let mut s = school(2, 1);
    s.master.teachers.extend(["t1", "t2"].map(teacher));
    s.master.classes.extend(["a", "b"].map(class));
    s.master.rooms.extend(["r1", "r2"].map(room));
    let mut l1 = lesson("l1", &["t1"], &["a"], 1, 1);
    l1.rooms = vec!["r1".into(), "r2".into()];
    let mut l2 = lesson("l2", &["t2"], &["b"], 1, 1);
    l2.rooms = vec!["r1".into()];
    s.lessons = vec![l1, l2];
    let mut a = placed("l1", 0, 1);
    a.room = Some("r1".into());
    let mut b = placed("l2", 0, 2);
    b.room = Some("r1".into());
    s.timetable = Some(Timetable { cards: vec![a, b] });
    let tt = swap_cards(&s, "l1", 1, "l2", 1).unwrap();
    s.timetable = Some(tt);
    assert!(verify::check(&s, s.timetable.as_ref().unwrap()).is_empty());
    assert_eq!(s.timetable.as_ref().unwrap().cards.iter().find(|c| c.lesson_id == "l2").unwrap().room.as_deref(), Some("r1"));
}

#[test]
fn swap_options_agree_with_swapping_on_the_sample_school() {
    let mut s = sample();
    s.timetable = Some(must_solve(&s).timetable);
    let mut rng = 99u64;
    let mut next = |n: usize| {
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((rng >> 33) as usize) % n
    };
    let (mut done, mut refused) = (0, 0);
    for _ in 0..40 {
        let tt = s.timetable.clone().unwrap();
        let a = tt.cards[next(tt.cards.len())].clone();
        for o in swap_options(&s, &a.lesson_id, a.number).unwrap() {
            let r = swap_cards(&s, &a.lesson_id, a.number, &o.lesson_id, o.number);
            assert_eq!(r.is_ok(), o.ok, "{a:?} with {o:?}: {r:?}");
        }
        let ok: Vec<_> = swap_options(&s, &a.lesson_id, a.number).unwrap().into_iter().filter(|o| o.ok).collect();
        if ok.is_empty() { refused += 1; continue; }
        let o = &ok[next(ok.len())];
        s.timetable = Some(swap_cards(&s, &a.lesson_id, a.number, &o.lesson_id, o.number).unwrap());
        done += 1;
        assert_eq!(verify::check(&s, s.timetable.as_ref().unwrap()), Vec::<String>::new());
    }
    assert!(done >= 20, "done {done}, refused {refused}");
}

// ---------- milestone 8: school limits, subject-per-day, links ----------

use crate::timetable::links::Link;

fn subject(s: &mut SchoolSetup, id: &str) {
    s.master.subjects.push(Subject { id: id.into(), name: id.into(), short_name: id.into(), color: "#000".into() });
}
fn with_subject(mut l: Lesson, subj: &str) -> Lesson {
    l.subject = subj.into();
    l
}

#[test]
fn weekly_maximum_is_a_hard_limit_and_unplaced_lessons_are_explained() {
    let mut s = school(8, 5);
    s.master.classes.extend(["a", "b", "c"].map(class));
    s.master.teachers.push(teacher("t"));
    s.settings.default_max_per_week = Some(8);
    for c in ["a", "b", "c"] {
        s.lessons.push(lesson(&format!("l{c}"), &["t"], &[c], 1, 3)); // 9 periods wanted
    }
    give_own_subjects(&mut s);
    let r = run(&s);
    assert_eq!(r.placed, 8);
    assert_eq!(r.unplaced.len(), 1);
    assert!(r.unplaced[0].message.contains("t's limits"), "{}", r.unplaced[0].message);
    assert!(verify::check(&s, &r.timetable).is_empty());
}

#[test]
fn a_teachers_own_limit_beats_the_school_default() {
    let mut s = school(8, 5);
    s.master.classes.extend(["a", "b"].map(class));
    let mut t = teacher("t");
    t.max_periods_per_week = Some(6); // default says 4
    s.master.teachers.push(t);
    s.settings.default_max_per_week = Some(4);
    s.lessons = vec![lesson("la", &["t"], &["a"], 1, 3), lesson("lb", &["t"], &["b"], 1, 3)];
    give_own_subjects(&mut s);
    must_solve(&s); // 6 periods are allowed because of the override
    s.master.teachers[0].max_periods_per_week = None;
    assert_eq!(run(&s).placed, 4, "the default of 4 applies to a teacher without their own limit");
}

#[test]
fn default_daily_maximum_applies_to_everyone_without_their_own() {
    let mut s = school(8, 5);
    s.master.classes.extend(["a", "b"].map(class));
    s.master.teachers.extend(["t", "u"].map(teacher));
    s.master.teachers[1].max_lessons_per_day = Some(5);
    s.settings.default_max_per_day = Some(2);
    s.lessons = vec![lesson("la", &["t"], &["a"], 1, 5), lesson("lb", &["u"], &["b"], 1, 5)];
    give_own_subjects(&mut s);
    let r = must_solve(&s);
    for d in 0..5u8 {
        assert!(r.timetable.cards.iter().filter(|c| c.day == d && c.lesson_id == "la").count() <= 2);
    }
    // one-per-day lessons never reach the limit; make a heavy teacher to prove it bites
    s.lessons = vec![with_subject(lesson("big", &["t"], &["a"], 1, 5), "x"), with_subject(lesson("big2", &["t"], &["b"], 1, 5), "y")];
    subject(&mut s, "x");
    subject(&mut s, "y");
    let r = must_solve(&s);
    for d in 0..5u8 {
        assert!(r.timetable.cards.iter().filter(|c| c.day == d).count() <= 2, "day {d}");
    }
}

#[test]
fn only_one_session_of_a_subject_per_class_per_day_by_default() {
    let mut s = school(8, 5);
    s.master.classes.push(class("a"));
    s.master.teachers.extend(["t", "u"].map(teacher));
    subject(&mut s, "maths");
    // two different lessons, same subject, same class (e.g. two teachers sharing a subject)
    s.lessons = vec![with_subject(lesson("m1", &["t"], &["a"], 1, 3), "maths"), with_subject(lesson("m2", &["u"], &["a"], 1, 2), "maths")];
    let r = must_solve(&s);
    let mut days: Vec<u8> = r.timetable.cards.iter().map(|c| c.day).collect();
    days.sort();
    days.dedup();
    assert_eq!(days.len(), 5, "five sessions of maths fill five different days");
    // allowing two per day lets them share
    s.settings.max_same_subject_per_day = 2;
    s.lessons[0].cards_per_week = 4;
    s.lessons[1].cards_per_week = 4;
    must_solve(&s); // 8 sessions can only fit with up to 2 a day anyway
}

#[test]
fn the_subject_limit_relaxes_itself_when_there_are_more_sessions_than_days() {
    let mut s = school(8, 5);
    s.master.classes.push(class("a"));
    s.master.teachers.push(teacher("t"));
    subject(&mut s, "maths");
    s.lessons = vec![with_subject(lesson("m", &["t"], &["a"], 1, 7), "maths")]; // 7 sessions, 5 days
    let r = must_solve(&s);
    assert_eq!(r.placed, 7);
}

#[test]
fn groups_of_a_split_class_may_have_the_same_subject_at_once() {
    let mut s = school(2, 1);
    s.master.classes.push(split_class());
    s.master.teachers.extend(["t1", "t2"].map(teacher));
    subject(&mut s, "pe");
    let mut boys = with_subject(lesson("b", &["t1"], &["10A"], 1, 1), "pe");
    boys.parts[0].group_id = Some("boys".into());
    let mut girls = with_subject(lesson("g", &["t2"], &["10A"], 1, 1), "pe");
    girls.parts[0].group_id = Some("girls".into());
    s.lessons = vec![boys, girls];
    let r = must_solve(&s);
    assert_eq!(r.timetable.cards.len(), 2);
}

/// 7A is split in halves. ICT (lab) and Library are taught in parallel and swap halves.
fn ict_library_school() -> SchoolSetup {
    let mut s = school(6, 5);
    let mut c = class("7A");
    c.divisions = vec![Division { id: "h".into(), name: "Halves".into(), groups: vec![grp("g1"), grp("g2")] }];
    s.master.classes.push(c);
    s.master.teachers.extend(["ict", "lib"].map(teacher));
    s.master.rooms.extend(["lab", "library"].map(room));
    for x in ["ict", "library"] {
        subject(&mut s, x);
    }
    let mk = |id: &str, t: &str, subj: &str, g: &str, r: &str| {
        let mut l = with_subject(lesson(id, &[t], &["7A"], 1, 1), subj);
        l.parts[0].group_id = Some(g.into());
        l.rooms = vec![r.into()];
        l
    };
    s.lessons = vec![mk("ict1", "ict", "ict", "g1", "lab"), mk("lib2", "lib", "library", "g2", "library"), mk("ict2", "ict", "ict", "g2", "lab"), mk("lib1", "lib", "library", "g1", "library")];
    s.links = vec![
        Link { id: "k1".into(), lessons: vec!["ict1".into(), "lib2".into()] },
        Link { id: "k2".into(), lessons: vec!["ict2".into(), "lib1".into()] },
    ];
    s
}

#[test]
fn linked_lessons_always_happen_at_the_same_time() {
    let s = ict_library_school();
    assert!(s.validate().iter().all(|i| i.level != "error"), "{:?}", s.validate());
    let r = must_solve(&s);
    assert_eq!(r.total, 4, "four lesson sessions, shared in pairs");
    assert_eq!(r.placed, 4);
    assert_eq!(r.timetable.cards.len(), 4);
    let at = |id: &str| { let c = r.timetable.cards.iter().find(|c| c.lesson_id == id).unwrap(); (c.day, c.period) };
    assert_eq!(at("ict1"), at("lib2"));
    assert_eq!(at("ict2"), at("lib1"));
    assert_ne!(at("ict1"), at("ict2"), "the halves swap: one teacher cannot be in both");
    let room = |id: &str| r.timetable.cards.iter().find(|c| c.lesson_id == id).unwrap().room.clone();
    assert_eq!((room("ict1").as_deref(), room("lib2").as_deref()), (Some("lab"), Some("library")));
}

#[test]
fn electives_across_sections_run_in_parallel_with_their_own_rooms() {
    let mut s = school(6, 5);
    s.master.classes.extend(["7A", "7B", "7C"].map(class));
    s.master.teachers.extend(["t1", "t2", "t3"].map(teacher));
    s.master.rooms.extend(["r1", "r2", "r3"].map(room));
    for x in ["urdu", "french", "art"] {
        subject(&mut s, x);
    }
    let pool = |mut l: Lesson| { l.rooms = vec!["r1".into(), "r2".into(), "r3".into()]; l };
    s.lessons = vec![
        pool(with_subject(lesson("u", &["t1"], &["7A"], 2, 2), "urdu")),
        pool(with_subject(lesson("f", &["t2"], &["7B"], 2, 2), "french")),
        pool(with_subject(lesson("a", &["t3"], &["7C"], 2, 2), "art")),
    ];
    s.links = vec![Link { id: "k".into(), lessons: vec!["u".into(), "f".into(), "a".into()] }];
    let r = must_solve(&s);
    let rooms: std::collections::HashSet<_> = r.timetable.cards.iter().filter(|c| c.number == 1).map(|c| c.room.clone().unwrap()).collect();
    assert_eq!(rooms.len(), 3, "three lessons at once need three different rooms");
    for n in 1..=2 {
        let spots: std::collections::HashSet<_> = r.timetable.cards.iter().filter(|c| c.number == n).map(|c| (c.day, c.period)).collect();
        assert_eq!(spots.len(), 1);
    }
}

#[test]
fn linked_lessons_that_cannot_meet_are_reported_not_crashed() {
    let mut s = ict_library_school();
    s.master.teachers[1].time_off = (1..=6).flat_map(|p| (0..5).map(move |d| off(d, p, 2))).collect(); // library teacher never free
    let r = run(&s);
    assert!(r.unplaced.len() >= 1);
    assert!(r.unplaced.iter().any(|u| u.message.contains("no time when everyone")), "{:?}", r.unplaced);
    assert!(verify::check(&s, &r.timetable).is_empty());
    // a link that is simply wrong (different session counts) says why
    let mut s2 = ict_library_school();
    s2.lessons[0].cards_per_week = 3;
    let r2 = run(&s2);
    assert!(r2.unplaced.iter().any(|u| u.message.contains("linked to happen at the same time")), "{:?}", r2.unplaced);
}

#[test]
fn moving_one_lesson_of_a_link_moves_all_of_them() {
    let mut s = ict_library_school();
    s.timetable = Some(must_solve(&s).timetable);
    let c = s.timetable.as_ref().unwrap().cards.iter().find(|c| c.lesson_id == "ict1").unwrap().clone();
    let opts = slot_options(&s, "ict1", 1).unwrap();
    let target = opts.iter().find(|o| o.status != "blocked" && (o.day, o.period) != (c.day, c.period)).unwrap();
    let tt = move_card(&s, "ict1", 1, target.day, target.period, None).unwrap();
    let pos = |id: &str| { let x = tt.cards.iter().find(|c| c.lesson_id == id).unwrap(); (x.day, x.period) };
    assert_eq!(pos("ict1"), (target.day, target.period));
    assert_eq!(pos("lib2"), pos("ict1"));
    assert_eq!(tt.cards.len(), 4);
    s.timetable = Some(tt);
    assert!(verify::check(&s, s.timetable.as_ref().unwrap()).is_empty());
    // locking one partner locks the pair
    s.timetable.as_mut().unwrap().cards.iter_mut().find(|c| c.lesson_id == "lib2").unwrap().locked = true;
    assert!(move_card(&s, "ict1", 1, c.day, c.period, None).unwrap_err().contains("locked"));
}

#[test]
fn linked_pairs_can_be_swapped_with_each_other() {
    let mut s = ict_library_school();
    s.timetable = Some(must_solve(&s).timetable);
    let before = s.timetable.clone().unwrap();
    let tt = swap_cards(&s, "ict1", 1, "ict2", 1).unwrap();
    let pos = |t: &Timetable, id: &str| { let x = t.cards.iter().find(|c| c.lesson_id == id).unwrap(); (x.day, x.period) };
    assert_eq!(pos(&tt, "ict1"), pos(&before, "ict2"));
    assert_eq!(pos(&tt, "lib1"), pos(&before, "ict1"));
    s.timetable = Some(tt);
    assert!(verify::check(&s, s.timetable.as_ref().unwrap()).is_empty());
}

#[test]
fn locked_linked_lessons_stay_together_when_rebuilding() {
    let s = ict_library_school();
    let lock: Vec<PlacedCard> = ["ict1", "lib2"].iter().map(|id| PlacedCard {
        lesson_id: id.to_string(), number: 1, shift: "morning".into(), day: 2, period: 3, length: 1,
        room: Some(if *id == "ict1" { "lab" } else { "library" }.into()), locked: true,
    }).collect();
    let r = run_with(&s, &lock, Options::default());
    assert!(r.unplaced.is_empty());
    for id in ["ict1", "lib2"] {
        let c = r.timetable.cards.iter().find(|c| c.lesson_id == id).unwrap();
        assert_eq!((c.day, c.period, c.locked), (2, 3, true));
    }
    assert!(verify::check(&s, &r.timetable).is_empty());
}


#[test]
fn pre_check_explains_weekly_limits_and_impossible_links() {
    let mut s = school(8, 5);
    s.master.classes.push(class("a"));
    s.master.teachers.push(teacher("Mr Busy"));
    s.settings.default_max_per_week = Some(5);
    s.lessons = vec![lesson("l", &["Mr Busy"], &["a"], 1, 7)];
    give_own_subjects(&mut s);
    let r = crate::timetable::check::run(&s);
    assert!(r.findings.iter().any(|f| f.kind == "teacher_week" && f.message.contains("most they may teach is 5")), "{:?}", r.findings);
    s.settings.default_max_per_week = None;
    s.settings.default_max_per_day = Some(1);
    let r = crate::timetable::check::run(&s);
    assert!(r.findings.iter().any(|f| f.kind == "teacher_day"), "{:?}", r.findings);

    let mut l = ict_library_school();
    l.master.teachers[1].time_off = (1..=6).flat_map(|p| (0..5).map(move |d| off(d, p, 2))).collect();
    let r = crate::timetable::check::run(&l);
    assert!(r.findings.iter().any(|f| f.kind == "link"), "{:?}", r.findings);
}


// ---------- milestone 8: quality and improving ----------

use super::soft;
use crate::timetable::settings::Weights;

fn state_of(s: &SchoolSetup) -> (super::model::Model, Vec<Option<super::engine::Pl>>) {
    let m = super::model::compile(s);
    let tt = s.timetable.clone().unwrap();
    let st = super::state_from(&m, &tt, &[]);
    let places = st.place.clone();
    (m, places)
}

fn terms(s: &SchoolSetup) -> soft::Terms {
    let (m, places) = state_of(s);
    let st = super::state_from_places(&m, &places, &vec![false; m.cards.len()]);
    soft::measure(&st)
}

fn put(id: &str, n: u32, day: u8, period: u8, room: Option<&str>) -> PlacedCard {
    PlacedCard { lesson_id: id.into(), number: n, shift: "morning".into(), day, period, length: 1, room: room.map(Into::into), locked: false }
}

#[test]
fn quality_counts_gaps_late_starts_avoid_spread_and_room_changes() {
    let mut s = school(6, 5);
    s.master.classes.extend(["a", "b"].map(class));
    s.master.teachers.push(teacher("t"));
    s.master.rooms.extend(["r1", "r2"].map(room));
    s.master.teachers[0].time_off = vec![off(0, 3, 1)]; // avoid Mon P3
    let mut l1 = lesson("l1", &["t"], &["a"], 1, 2);
    l1.rooms = vec!["r1".into(), "r2".into()];
    s.lessons = vec![l1, lesson("l2", &["t"], &["b"], 1, 1)];
    give_own_subjects(&mut s);
    // Teacher: Mon P1 (l1#1), Mon P3 (l2): one gap. l1#1 in r1, l1#2 on Tue in r2: one room change,
    // bunched on neighbouring days. Class a: Mon P1 only, Tue P2 (late start by one).
    s.timetable = Some(Timetable { cards: vec![put("l1", 1, 0, 1, Some("r1")), put("l1", 2, 1, 2, Some("r2")), put("l2", 1, 0, 3, None)] });
    let t = terms(&s);
    assert_eq!(t.teacher_gaps, 1.0);
    assert_eq!(t.class_gaps, 3.0, "class a starts at P2 on Tuesday (1); class b starts at P3 on Monday (2)");
}

#[test]
fn quality_terms_one_at_a_time() {
    let mut s = school(6, 5);
    s.master.classes.push(class("a"));
    s.master.teachers.push(teacher("t"));
    s.master.rooms.extend(["r1", "r2"].map(room));
    let mut l = lesson("l", &["t"], &["a"], 1, 2);
    l.rooms = vec!["r1".into(), "r2".into()];
    s.lessons = vec![l];
    give_own_subjects(&mut s);
    let mk = |d2: u8, r2: &str, avoid: bool| {
        let mut s = s.clone();
        if avoid {
            s.master.teachers[0].time_off = vec![off(d2, 1, 1)];
        }
        s.timetable = Some(Timetable { cards: vec![put("l", 1, 0, 1, Some("r1")), put("l", 2, d2, 1, Some(r2))] });
        terms(&s)
    };
    assert_eq!(mk(4, "r1", false).rooms, 0.0);
    assert_eq!(mk(4, "r2", false).rooms, 1.0, "two rooms = one change");
    assert!(mk(1, "r1", false).spread > mk(3, "r1", false).spread, "neighbouring days are worse than spaced ones");
    assert_eq!(mk(3, "r1", false).avoid, 0.0);
    assert_eq!(mk(3, "r1", true).avoid, 1.0);
    // balance: two different lessons for class a, both on Monday vs Monday and Thursday
    let mut s2 = s.clone();
    s2.master.teachers.push(teacher("u"));
    s2.lessons.push(lesson("m", &["u"], &["a"], 1, 1));
    give_own_subjects(&mut s2);
    let place = |day2: u8, period2: u8| {
        let mut s3 = s2.clone();
        s3.timetable = Some(Timetable { cards: vec![put("l", 1, 0, 1, Some("r1")), put("m", 1, day2, period2, None)] });
        terms(&s3)
    };
    assert!(place(0, 2).balance > place(3, 1).balance);
}

#[test]
fn improving_never_breaks_a_rule_never_gets_worse_and_the_bookkeeping_is_exact() {
    let mut s = sample();
    s.timetable = Some(must_solve(&s).timetable);
    let w = Weights::default();
    let (m, _) = state_of(&s);
    let mut st = super::state_from(&m, s.timetable.as_ref().unwrap(), &[]);
    let before = soft::measure(&st).weighted(&w);
    let res = soft::improve(&mut st, &w, 3, Instant::now() + Duration::from_secs(3), &AtomicBool::new(false), &mut |_| {});
    assert!(res.moves > 1000 && res.accepted > 0, "{} moves, {} accepted", res.moves, res.accepted);
    // the energy tracked move by move equals a full recount of where the search ended
    let now = soft::measure(&st).weighted(&w);
    assert!((now - res.final_energy).abs() < 1e-6, "tracked {} vs real {}", res.final_energy, now);
    let best = super::state_from_places(&m, &res.best, &vec![false; m.cards.len()]);
    let after = soft::measure(&best).weighted(&w);
    assert!(after <= before + 1e-9 && after < before * 0.9, "before {before} after {after}");
    // and the improved timetable obeys every hard rule
    let r = super::improve_timetable(&s, Duration::from_secs(2), 5, &AtomicBool::new(false), &mut |_| {});
    s.timetable = Some(r.timetable);
    assert_eq!(verify::check(&s, s.timetable.as_ref().unwrap()), Vec::<String>::new());
    assert!(r.quality_after.penalty <= r.quality_before.penalty);
}

#[test]
fn improving_keeps_locked_sessions_and_linked_pairs_together() {
    let mut s = ict_library_school();
    let mut tt = must_solve(&s).timetable;
    for c in tt.cards.iter_mut().filter(|c| c.lesson_id == "ict1" || c.lesson_id == "lib2") {
        c.locked = true;
    }
    let before: Vec<_> = tt.cards.iter().filter(|c| c.locked).map(|c| (c.lesson_id.clone(), c.day, c.period)).collect();
    s.timetable = Some(tt);
    let r = super::improve_timetable(&s, Duration::from_secs(1), 9, &AtomicBool::new(false), &mut |_| {});
    s.timetable = Some(r.timetable);
    let after: Vec<_> = s.timetable.as_ref().unwrap().cards.iter().filter(|c| c.locked).map(|c| (c.lesson_id.clone(), c.day, c.period)).collect();
    assert_eq!(before, after);
    assert_eq!(verify::check(&s, s.timetable.as_ref().unwrap()), Vec::<String>::new());
}

#[test]
fn heavier_weights_get_what_they_ask_for() {
    // The same school, once caring only about teacher gaps and once caring only about class gaps.
    let mut s = sample();
    s.timetable = Some(must_solve(&s).timetable);
    let measure_after = |w: Weights| {
        let mut s2 = s.clone();
        s2.settings.weights = w;
        let r = super::improve_timetable(&s2, Duration::from_secs(2), 11, &AtomicBool::new(false), &mut |_| {});
        s2.timetable = Some(r.timetable);
        terms(&s2)
    };
    let zero = Weights { teacher_gaps: 0, class_gaps: 0, spread: 0, avoid: 0, balance: 0, room_consistency: 0 };
    let base = terms(&s);
    let teachers_first = measure_after(Weights { teacher_gaps: 10, ..zero.clone() });
    let classes_first = measure_after(Weights { class_gaps: 10, ..zero.clone() });
    assert!(teachers_first.teacher_gaps <= base.teacher_gaps);
    assert!(classes_first.class_gaps <= base.class_gaps);
    assert!(teachers_first.teacher_gaps <= classes_first.teacher_gaps, "{teachers_first:?} {classes_first:?}");
    assert!(classes_first.class_gaps <= teachers_first.class_gaps, "{teachers_first:?} {classes_first:?}");
}

#[test]
fn solve_can_improve_in_the_same_run_and_reports_before_and_after() {
    let s = sample();
    let r = run_with(&s, &[], Options { improve_time: Duration::from_secs(2), ..Options::default() });
    assert!(r.unplaced.is_empty());
    assert!(r.quality_after.penalty <= r.quality_before.penalty);
    assert!(r.quality_before.items.len() == 6 && r.quality_before.items[0].label.contains("teachers"));
    assert!(verify::check(&s, &r.timetable).is_empty());
    let mut phases = Vec::new();
    let _ = solve(&s, &[], &Options { improve_time: Duration::from_millis(300), ..Options::default() }, &AtomicBool::new(false), &mut |p| {
        if phases.last() != Some(&p.phase) { phases.push(p.phase); }
    });
    assert_eq!(phases, vec![super::Phase::Placing, super::Phase::Improving, super::Phase::Placing]);
}

#[test]
fn cancel_stops_improving_quickly_and_keeps_a_valid_timetable() {
    let mut s = sample();
    s.timetable = Some(must_solve(&s).timetable);
    let cancel = AtomicBool::new(true);
    let t0 = Instant::now();
    let r = super::improve_timetable(&s, Duration::from_secs(30), 1, &cancel, &mut |_| {});
    assert!(t0.elapsed() < Duration::from_secs(5));
    s.timetable = Some(r.timetable);
    assert!(verify::check(&s, s.timetable.as_ref().unwrap()).is_empty());
}

// ---------- milestone 8: relaxation ----------

/// One teacher, one day of four periods, four lessons for four classes. With "at most 2 in a row"
/// it cannot be done without bending that rule.
fn four_in_a_row() -> SchoolSetup {
    let mut s = school(4, 1);
    s.master.classes.extend(["a", "b", "c", "d"].map(class));
    let mut t = teacher("Mr Row");
    t.max_consecutive = Some(2);
    s.master.teachers.push(t);
    s.lessons = ["a", "b", "c", "d"].iter().map(|c| lesson(&format!("l{c}"), &["Mr Row"], &[c], 1, 1)).collect();
    give_own_subjects(&mut s);
    s
}

fn bendable(consec: bool, per_day: bool, same_day: bool, subject_day: bool) -> verify::Enforce {
    verify::Enforce { same_day, subject_day, consecutive: consec, per_day }
}

#[test]
fn without_permission_no_rule_is_bent() {
    let s = four_in_a_row();
    let r = run_with(&s, &[], Options { time_limit: Duration::from_secs(5), ..Options::default() });
    assert_eq!(r.unplaced.len(), 1);
    assert!(r.relaxed.is_empty());
    assert!(verify::check(&s, &r.timetable).is_empty());
}

#[test]
fn with_permission_the_rule_is_bent_and_reported() {
    let mut s = four_in_a_row();
    s.settings.allow_relaxation = true;
    let r = run(&s);
    assert!(r.unplaced.is_empty(), "{:?}", r.unplaced);
    assert_eq!(r.relaxed.len(), 1, "{:?}", r.relaxed);
    assert_eq!(r.relaxed[0].kind, "consecutive");
    assert!(r.relaxed[0].message.contains("Mr Row has 4 lessons in a row on Monday (the limit is 2)"), "{}", r.relaxed[0].message);
    // everything except the bent rule still holds
    assert!(verify::check_with(&s, &r.timetable, bendable(false, true, true, true)).is_empty());
    assert!(!verify::check(&s, &r.timetable).is_empty());
    // the edit page accepts such a timetable instead of calling it stale
    s.timetable = Some(r.timetable);
    assert!(super::edit::is_consistent(&s));
    assert_eq!(slot_options(&s, "la", 1).unwrap().len(), 4);
}

#[test]
fn daily_maximum_can_be_bent_too() {
    let mut s = four_in_a_row();
    s.master.teachers[0].max_consecutive = None;
    s.settings.default_max_per_day = Some(2);
    s.settings.allow_relaxation = true;
    let r = run(&s);
    assert!(r.unplaced.is_empty());
    assert!(r.relaxed.iter().any(|x| x.kind == "perDay" && x.message.contains("4 lessons on Monday (the limit is 2)")), "{:?}", r.relaxed);
}

#[test]
fn clashes_time_off_weekly_limits_and_links_are_never_bent() {
    let mut s = four_in_a_row();
    s.settings.allow_relaxation = true;
    s.settings.default_max_per_week = Some(3);
    let r = run_with(&s, &[], Options { time_limit: Duration::from_secs(5), ..Options::default() });
    assert_eq!(r.unplaced.len(), 1, "the weekly maximum of 3 still holds");
    assert!(verify::check_with(&s, &r.timetable, bendable(false, false, false, false)).is_empty());

    let mut s = ict_library_school();
    s.settings.allow_relaxation = true;
    s.master.teachers[1].time_off = (1..=6).flat_map(|p| (0..5).map(move |d| off(d, p, 2))).collect();
    let r = run_with(&s, &[], Options { time_limit: Duration::from_secs(5), ..Options::default() });
    assert!(!r.unplaced.is_empty(), "time off is never bent");
    assert!(verify::check_with(&s, &r.timetable, bendable(false, false, false, false)).is_empty());
}

#[test]
fn only_what_is_needed_gets_bent() {
    // Lots of room everywhere; relaxation allowed, but nothing needs bending.
    let mut s = school(8, 5);
    s.master.classes.extend(["a", "b"].map(class));
    let mut t = teacher("t");
    t.max_consecutive = Some(2);
    s.master.teachers.push(t);
    s.settings.allow_relaxation = true;
    s.lessons = vec![lesson("la", &["t"], &["a"], 1, 5), lesson("lb", &["t"], &["b"], 1, 5)];
    give_own_subjects(&mut s);
    let r = must_solve(&s);
    assert!(r.relaxed.is_empty());
}

#[test]
fn locked_lessons_that_break_a_bendable_rule_are_kept_when_relaxation_is_on() {
    let mut s = school(4, 3);
    s.master.classes.push(class("a"));
    s.master.teachers.push(teacher("t"));
    s.lessons = vec![lesson("l", &["t"], &["a"], 1, 2)];
    give_own_subjects(&mut s);
    let lock: Vec<PlacedCard> = (1..=2).map(|n| PlacedCard { lesson_id: "l".into(), number: n, shift: "morning".into(), day: 0, period: n as u8, length: 1, room: None, locked: true }).collect();
    let r = run_with(&s, &lock, Options::default());
    assert_eq!(r.unplaced.len(), 1, "the second locked session on the same day is refused by default");
    s.settings.allow_relaxation = true;
    let r = run_with(&s, &lock, Options::default());
    assert!(r.unplaced.is_empty());
    assert!(r.relaxed.iter().any(|x| x.kind == "sameDay"), "{:?}", r.relaxed);
}

#[test]
fn improving_a_relaxed_timetable_keeps_all_other_rules() {
    let mut s = four_in_a_row();
    s.settings.allow_relaxation = true;
    let r = run(&s);
    s.timetable = Some(r.timetable);
    let imp = super::improve_timetable(&s, Duration::from_secs(1), 1, &AtomicBool::new(false), &mut |_| {});
    s.timetable = Some(imp.timetable);
    assert_eq!(s.timetable.as_ref().unwrap().cards.len(), 4);
    assert!(verify::check_with(&s, s.timetable.as_ref().unwrap(), bendable(false, true, true, true)).is_empty());
}

#[test]
fn two_sessions_may_share_a_day_when_no_other_day_can_hold_them() {
    // Doubles need two free periods in a row. Tuesday only has its last period free for the
    // teacher, so Tuesday cannot hold a double and both sessions have to be on Monday.
    // The solver allows that, and the independent checker must agree.
    let mut s = school(4, 2);
    let mut t = teacher("T");
    t.time_off = (1..=3).map(|p| off(1, p, 2)).collect();
    s.master.teachers.push(t);
    s.master.classes.push(class("C"));
    s.lessons.push(lesson("L", &["T"], &["C"], 2, 2));
    let r = must_solve(&s);
    assert!(r.timetable.cards.iter().all(|c| c.day == 0), "{:?}", r.timetable.cards);
    assert!(verify::check(&s, &r.timetable).is_empty());
}

#[test]
fn improve_returns_the_best_timetable_it_saw_even_when_time_runs_out_mid_search() {
    let mut s = sample();
    s.timetable = Some(must_solve(&s).timetable);
    let w = Weights::default();
    let (m, _) = state_of(&s);
    for seed in 1..=60u64 {
        let mut st = super::state_from(&m, s.timetable.as_ref().unwrap(), &[]);
        let ms = 5 + seed % 40; // stops at all sorts of moments
        let res = soft::improve(&mut st, &w, seed, Instant::now() + Duration::from_millis(ms), &AtomicBool::new(false), &mut |_| {});
        let best = super::state_from_places(&m, &res.best, &vec![false; m.cards.len()]);
        let real = soft::measure(&best).weighted(&w);
        assert!((real - res.best_energy).abs() < 1e-6, "seed {seed}: returned a timetable worth {real}, but the best seen was {}", res.best_energy);
    }
}

// ---------- fast lookups (a large school made these quadratic) ----------

#[test]
fn a_session_is_found_by_lesson_id_and_number_exactly() {
    let s = sample();
    let m = compile(&s);
    for (li, l) in s.lessons.iter().enumerate() {
        for n in 1..=l.cards_per_week {
            assert_eq!(card_of(&m, &l.id, n), Some(m.cards_of_lesson[li][n as usize - 1]), "{} #{n}", l.id);
        }
        assert_eq!(card_of(&m, &l.id, 0), None);
        assert_eq!(card_of(&m, &l.id, l.cards_per_week + 1), None);
    }
    assert_eq!(card_of(&m, "no-such-lesson", 1), None);
}

#[test]
fn grouping_sessions_by_card_matches_looking_at_each_one() {
    let mut s = sample();
    s.timetable = Some(must_solve(&s).timetable);
    let tt = s.timetable.clone().unwrap();
    let m = compile(&s);
    let groups = group_by_card(&m, &tt.cards);
    assert_eq!(groups.len(), m.cards.len());
    for (v, g) in groups.iter().enumerate() {
        let slow: Vec<&PlacedCard> = tt.cards.iter().filter(|c| card_of(&m, &c.lesson_id, c.number) == Some(v)).collect();
        assert_eq!(g.len(), slow.len());
        assert!(g.iter().zip(&slow).all(|(a, b)| std::ptr::eq(*a, *b)), "card {v}: same sessions, same order");
    }
    assert_eq!(groups.iter().map(|g| g.len()).sum::<usize>(), tt.cards.len());
}

#[test]
fn linked_lessons_share_one_swap_answer() {
    // two linked lessons occupy one card; asking what the dragged lesson may swap with answers for both
    let s = sample();
    let mut s = s;
    let r = must_solve(&s);
    s.timetable = Some(r.timetable.clone());
    let any = r.timetable.cards.iter().find(|c| !c.locked).unwrap();
    let list = edit::swap_options(&s, &any.lesson_id, any.number).unwrap();
    // every other placed session of the shift is answered exactly once
    let in_shift = r.timetable.cards.iter().filter(|c| c.shift == any.shift && !(c.lesson_id == any.lesson_id && c.number == any.number)).count();
    assert_eq!(list.len(), in_shift);
}

// ---------- repair ----------

fn placements(s: &SchoolSetup, tt: &Timetable) -> (Model, Vec<Option<Pl>>) {
    let m = compile(s);
    let places = state_from(&m, tt, &[]).place.clone();
    (m, places)
}

fn timetable_of(m: &Model, places: &[Option<Pl>], locked: &[bool]) -> Timetable {
    let mut cards = Vec::new();
    for v in 0..m.cards.len() {
        if let Some(p) = places[v] {
            cards.extend(emit(m, v, &p, locked[v]));
        }
    }
    Timetable { cards }
}

fn repair_now(m: &Model, places: Vec<Option<Pl>>, locked: &[bool], cancel: &AtomicBool) -> (Vec<Option<Pl>>, bool) {
    repair::repair(m, places, locked, Rules::STRICT, 8, 1, Instant::now() + Duration::from_secs(60), cancel, Duration::from_secs(1), &mut |_| {})
}

#[test]
fn repair_puts_back_sessions_taken_off_a_finished_timetable() {
    let s = sample();
    let (m, mut places) = placements(&s, &must_solve(&s).timetable);
    let locked = vec![false; m.cards.len()];
    let removed: Vec<usize> = (0..m.cards.len()).step_by(7).collect();
    assert!(removed.len() >= 8);
    for &v in &removed {
        places[v] = None;
    }
    let (fixed, cancelled) = repair_now(&m, places, &locked, &AtomicBool::new(false));
    assert!(!cancelled);
    assert!(fixed.iter().all(|p| p.is_some()), "every session is placed again");
    let bad = verify::check(&s, &timetable_of(&m, &fixed, &locked));
    assert!(bad.is_empty(), "{bad:#?}");
}

#[test]
fn repair_never_moves_a_locked_session() {
    let s = sample();
    let (m, mut places) = placements(&s, &must_solve(&s).timetable);
    let mut locked = vec![false; m.cards.len()];
    for v in (0..m.cards.len()).step_by(3) {
        locked[v] = true;
    }
    let before = places.clone();
    for v in (1..m.cards.len()).step_by(5).filter(|&v| !locked[v]) {
        places[v] = None;
    }
    let (fixed, _) = repair_now(&m, places, &locked, &AtomicBool::new(false));
    for v in 0..m.cards.len() {
        if locked[v] {
            assert_eq!(fixed[v], before[v], "locked card {v} must stay exactly where it was");
        }
    }
    assert!(verify::check(&s, &timetable_of(&m, &fixed, &locked)).is_empty());
}

#[test]
fn repair_stops_at_once_when_cancelled_and_changes_nothing() {
    let s = sample();
    let (m, mut places) = placements(&s, &must_solve(&s).timetable);
    places[0] = None;
    let before = places.clone();
    let (after, cancelled) = repair_now(&m, places, &vec![false; m.cards.len()], &AtomicBool::new(true));
    assert!(cancelled);
    assert_eq!(after, before);
}

#[test]
fn repair_does_nothing_to_a_complete_timetable() {
    let s = sample();
    let (m, places) = placements(&s, &must_solve(&s).timetable);
    let (after, cancelled) = repair_now(&m, places.clone(), &vec![false; m.cards.len()], &AtomicBool::new(false));
    assert!(!cancelled);
    assert_eq!(after, places);
}

#[test]
fn repair_effort_grows_with_the_school_but_has_a_ceiling() {
    let small = repair::limits(40, Duration::from_millis(100));
    let big = repair::limits(3200, Duration::from_secs(10));
    let huge = repair::limits(100_000, Duration::from_secs(600));
    assert!(small.0 < big.0 && big.0 <= huge.0);
    assert!(small.1 < big.1 && huge.1 <= Duration::from_secs(45));
    assert!(small.1 >= Duration::from_millis(1500));
}

#[test]
fn switching_repair_off_is_respected() {
    // an impossible weekly maximum leaves a session over; repair or not, the report is the same plain explanation
    let mut s = school(8, 5);
    s.master.classes.extend(["a", "b", "c"].map(class));
    s.master.teachers.push(teacher("t"));
    s.settings.default_max_per_week = Some(8);
    for c in ["a", "b", "c"] {
        s.lessons.push(lesson(&format!("l{c}"), &["t"], &[c], 1, 3));
    }
    give_own_subjects(&mut s);
    for repair in [false, true] {
        let r = run_with(&s, &[], Options { repair, time_limit: Duration::from_secs(10), ..Options::default() });
        assert_eq!((r.placed, r.unplaced.len()), (8, 1), "repair={repair}");
        assert!(verify::check(&s, &r.timetable).is_empty());
    }
}

// ---------- the large school (slow: run with `cargo test --release -- --ignored`) ----------



// ---------- "same subject on the same day" belongs to each group of students ----------

#[test]
fn the_same_subject_limit_is_worked_out_for_each_group_of_students() {
    // Whole-class English five times a week from a teacher who works only 3 days (so sessions must share
    // days), plus a reading lesson twice a week for Group 2 only.
    //   Group 1 has 5 English sessions over 3 days  -> at most 2 a day
    //   Group 2 has 7 English sessions over 3 days  -> at most 3 a day
    // The solver used to apply the larger limit (3) to the whole class, so Group 1 could get 3 in a day
    // and the independent checker rightly objected.
    let mut s = school(7, 5);
    let mut c = class("a");
    c.divisions = vec![Division { id: "d".into(), name: "Halves".into(), groups: vec![grp("g1"), grp("g2")] }];
    s.master.classes.push(c);
    let mut part_time = teacher("t");
    part_time.max_days_per_week = Some(3);
    s.master.teachers.extend([part_time, teacher("u")]);
    s.lessons.push(lesson("whole", &["t"], &["a"], 1, 5));
    let mut reading = lesson("reading", &["u"], &["a"], 1, 2);
    reading.parts[0].group_id = Some("g2".into());
    s.lessons.push(reading);

    let m = compile(&s);
    let whole: Vec<usize> = m.cards_of_lesson[0].clone();
    let read: Vec<usize> = m.cards_of_lesson[1].clone();
    let mut st = State::new(&m);
    st.place(whole[0], Pl::new(0, 0));
    st.place(whole[1], Pl::new(0, 1));
    let at = |period| model::Start { day: 0, period, avoid: 0 };

    // a third whole-class session that day would give Group 1 three: refused, and the reason names the subject
    let (blocked, _) = st.conflicts(whole[2], at(2));
    assert!(blocked.iter().any(|(_, b)| matches!(b, Blocker::SameSubject(..))), "{blocked:?}");
    // but a reading session for Group 2 only makes three for Group 2, whose limit is 3: allowed
    let (free, _) = st.conflicts(read[0], at(2));
    assert!(free.is_empty(), "{free:?}");
    // and the real search never breaks it, for many seeds
    for seed in 1..=8 {
        let r = run_with(&s, &[], Options { seed, ..Options::default() });
        assert_eq!(r.placed, r.total, "seed {seed}");
        let bad = verify::check(&s, &r.timetable);
        assert!(bad.is_empty(), "seed {seed}: {bad:?}");
    }
}
