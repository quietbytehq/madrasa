//! Manual editing helpers (milestone 7, blueprint section 8): which slots a card may be
//! dragged to, and applying a move or a swap. Uses the same collision logic as the solver.
//! Lessons linked as "same time" move together.
use super::engine::{Blocker, Pl, State};
use super::model::{compile, Model, MAXR};
use super::{card_of, emit, group_by_card, state_from, PlacedCard, Timetable};
use crate::timetable::model::SchoolSetup;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RoomOption {
    pub id: String,
    pub name: String,
    pub free: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SlotInfo {
    pub shift: String,
    pub day: u8,
    /// 1-based
    pub period: u8,
    /// "ok" (green) | "soft" (amber) | "blocked" (red)
    pub status: String,
    pub reasons: Vec<String>,
    pub rooms: Vec<RoomOption>,
}

fn find_card(m: &Model, lesson_id: &str, number: u32) -> Result<usize, String> {
    let v = card_of(m, lesson_id, number).ok_or("That lesson session no longer exists.")?;
    match &m.cards[v].broken {
        Some(b) => Err(b.clone()),
        None => Ok(v),
    }
}

fn why_not_in_domain(m: &Model, v: usize, d: usize, p: usize) -> String {
    let c = &m.cards[v];
    let per = m.periods[c.shift][d];
    if p + c.len > per {
        return format!("A {}-period lesson would run past the end of the day.", c.len);
    }
    if (p + 1..p + c.len).any(|k| !m.cross_ok[c.shift][k]) {
        return "It would run across a break that cannot be crossed.".into();
    }
    let span = |f: &dyn Fn(usize) -> bool| (0..c.len).any(|k| f(m.slot(c.shift, d, p + k)));
    for &t in &c.teachers {
        if span(&|sl| m.t_forb[t * m.slots + sl]) {
            return format!("{} is not available then (time off).", m.teacher_names[t]);
        }
    }
    for &(ci, _) in &c.classes {
        if span(&|sl| m.c_forb[ci * m.slots + sl]) {
            return format!("Class {} is not available then (time off).", m.class_names[ci]);
        }
    }
    if !c.pools.is_empty() {
        return "The allowed rooms are not all open then.".into();
    }
    "Not allowed at this time.".into()
}

fn blocker_text(m: &Model, st: &State, b: &Blocker, other: u32) -> String {
    let o = other as usize;
    let title = m.card_title(o);
    let locked = if st.locked[o] { " (locked)" } else { "" };
    match b {
        Blocker::Teacher(t) => format!("{} is already teaching {title}{locked}.", m.teacher_names[*t]),
        Blocker::Class(c) => format!("Class {} already has {title}{locked}.", m.class_names[*c]),
        Blocker::Room(r) => format!("Room {} is used by {title}{locked}.", m.room_names[*r]),
        Blocker::SameDay => format!("Another session of this lesson is already on that day{locked}."),
        Blocker::SameSubject(c, s) => format!("Class {} already has {} that day{locked}.", m.class_names[*c], m.subject_names[*s]),
    }
}

fn evaluate(m: &Model, st: &State, v: usize, d: usize, p: usize) -> (String, Vec<String>, Vec<RoomOption>) {
    let c = &m.cards[v];
    let Some(s) = c.domain.iter().copied().find(|s| s.day as usize == d && s.period as usize == p) else {
        return ("blocked".into(), vec![why_not_in_domain(m, v, d, p)], vec![]);
    };
    let (bl, _) = st.conflicts(v, s);
    if !bl.is_empty() {
        return ("blocked".into(), bl.iter().map(|(o, b)| blocker_text(m, st, b, *o)).collect(), vec![]);
    }
    if let Some(t) = st.limit_problem(v, s) {
        return ("blocked".into(), vec![format!("{} would go over their limits for the day or week.", m.teacher_names[t])], vec![]);
    }
    // A room list is offered when exactly one lesson of the card needs a room.
    let rooms = if c.pools.len() == 1 {
        c.pools[0]
            .iter()
            .map(|&r| RoomOption { id: m.room_ids[r].clone(), name: m.room_names[r].clone(), free: st.room_free(r, c.shift, s, c.len) })
            .collect()
    } else {
        vec![]
    };
    if s.avoid > 0 {
        let mut who = Vec::new();
        for &t in &c.teachers {
            if (0..c.len).any(|k| m.t_avoid[t * m.slots + m.slot(c.shift, d, p + k)]) {
                who.push(m.teacher_names[t].clone());
            }
        }
        for &(ci, _) in &c.classes {
            if (0..c.len).any(|k| m.c_avoid[ci * m.slots + m.slot(c.shift, d, p + k)]) {
                who.push(format!("class {}", m.class_names[ci]));
            }
        }
        return ("soft".into(), vec![format!("{} would rather avoid this time.", who.join(" and "))], rooms);
    }
    ("ok".into(), vec![], rooms)
}

/// Free periods between the first and last busy period of a day.
fn gaps(busy: impl Fn(usize) -> bool, per: usize) -> usize {
    let ps: Vec<usize> = (0..per).filter(|&p| busy(p)).collect();
    match (ps.first(), ps.last()) {
        (Some(a), Some(b)) => b - a + 1 - ps.len(),
        _ => 0,
    }
}

/// "Nice to have" rules (Rules & limits) that a move to (d, p) would break, beyond "avoid" cells:
/// a new free period in the middle of a teacher's or class's day, and a session the day before
/// or after another session of the same lesson. Only rules whose weight is above 0 count.
fn soft_reasons(m: &Model, st: &State, v: usize, d: usize, p: usize, w: &crate::timetable::settings::Weights) -> Vec<String> {
    let c = &m.cards[v];
    let per = m.periods[c.shift][d];
    let here = |q: usize| q >= p && q < p + c.len;
    let mut out = Vec::new();
    if w.teacher_gaps > 0 {
        for &t in &c.teachers {
            let busy = |q: usize| st.t_occ[t * m.slots + m.slot(c.shift, d, q)] > 0;
            if gaps(|q| busy(q) || here(q), per) > gaps(busy, per) {
                out.push(format!("{} would have a free period in the middle of the day.", m.teacher_names[t]));
            }
        }
    }
    if w.class_gaps > 0 {
        for &(ci, _) in &c.classes {
            let busy = |q: usize| st.c_mask[ci * m.slots + m.slot(c.shift, d, q)] != 0;
            if gaps(|q| busy(q) || here(q), per) > gaps(busy, per) {
                out.push(format!("Class {} would have a free period in the middle of the day.", m.class_names[ci]));
            }
        }
    }
    if w.spread > 0 {
        let days = (0..m.nd).filter(|&x| m.periods[c.shift][x] > 0).count();
        for mb in &c.members {
            let own = &m.cards_of_lesson[mb.lesson];
            // Only when the week has room to keep a day between sessions.
            if own.len() * 2 > days + 1 {
                continue;
            }
            let next_door = own.iter().filter(|&&o| o != v).filter_map(|&o| st.place[o]).find(|q| (q.day as usize).abs_diff(d) == 1);
            if let Some(q) = next_door {
                out.push(format!(
                    "Another session of {} is on {}; spread across the week is better.",
                    m.lesson_titles[mb.lesson],
                    m.day_names.get(q.day as usize).map_or("the next day", |n| n.as_str())
                ));
                break;
            }
        }
    }
    out
}

/// Every start cell for the card, coloured by whether it can go there right now:
/// "ok" (green), "soft" (amber: allowed, but breaks a "nice to have" rule) or "blocked" (red).
pub fn slot_options(setup: &SchoolSetup, lesson_id: &str, number: u32) -> Result<Vec<SlotInfo>, String> {
    let m = compile(setup);
    let v = find_card(&m, lesson_id, number)?;
    let tt = setup.timetable.clone().unwrap_or_default();
    let st = state_from(&m, &tt, &[v]);
    let sh = m.cards[v].shift;
    let w = &setup.settings.weights;
    let mut out = Vec::new();
    for d in 0..m.nd {
        for p in 0..m.periods[sh][d] {
            let (mut status, mut reasons, rooms) = evaluate(&m, &st, v, d, p);
            if status != "blocked" {
                let more = soft_reasons(&m, &st, v, d, p, w);
                if !more.is_empty() {
                    status = "soft".into();
                    reasons.extend(more);
                }
            }
            out.push(SlotInfo { shift: m.shift_ids[sh].clone(), day: d as u8, period: p as u8 + 1, status, reasons, rooms });
        }
    }
    Ok(out)
}

/// Rooms for card `v` starting at `start`, keeping `prefer` when the card needs one room only.
fn rooms_for(m: &Model, st: &State, v: usize, start: super::model::Start, prefer: Option<usize>) -> Result<[i16; MAXR], String> {
    let c = &m.cards[v];
    let (bl, mut rooms) = st.conflicts(v, start);
    if !bl.is_empty() {
        return Err("That time is no longer free.".into());
    }
    if c.pools.len() == 1 {
        if let Some(r) = prefer.filter(|r| c.pools[0].contains(r) && st.room_free(*r, c.shift, start, c.len)) {
            rooms[0] = r as i16;
        }
    }
    if rooms.iter().take(c.pools.len()).any(|r| *r < 0) {
        return Err(format!("No room is free for {} at that time.", m.card_title(v)));
    }
    Ok(rooms)
}

/// Moves (or places) one session. Fails with a plain-English reason if the target is blocked.
pub fn move_card(
    setup: &SchoolSetup,
    lesson_id: &str,
    number: u32,
    day: u8,
    period: u8,
    room: Option<String>,
) -> Result<Timetable, String> {
    let m = compile(setup);
    let v = find_card(&m, lesson_id, number)?;
    let mut tt = setup.timetable.clone().unwrap_or_default();
    let mine = |c: &PlacedCard| card_of(&m, &c.lesson_id, c.number) == Some(v);
    if group_by_card(&m, &tt.cards)[v].iter().any(|c| c.locked) {
        return Err("This lesson is locked. Unlock it first to move it.".into());
    }
    let st = state_from(&m, &tt, &[v]);
    let (d, p) = (day as usize, (period as usize).wrapping_sub(1));
    if d >= m.nd || p >= m.pm {
        return Err("That is not a time in the week.".into());
    }
    let (status, reasons, options) = evaluate(&m, &st, v, d, p);
    if status == "blocked" {
        return Err(reasons.join(" "));
    }
    let c = &m.cards[v];
    let start = *c.domain.iter().find(|s| s.day as usize == d && s.period as usize == p).ok_or("That time is not allowed.")?;
    let prefer = match room {
        Some(r) if c.pools.len() == 1 => match options.iter().find(|o| o.id == r) {
            Some(o) if o.free => m.room_ids.iter().position(|x| *x == r),
            Some(_) => return Err("That room is already in use at that time.".into()),
            None => return Err("That room is not allowed for this lesson.".into()),
        },
        _ => None,
    };
    let mut pl = Pl::new(day, period - 1);
    pl.rooms = rooms_for(&m, &st, v, start, prefer)?;
    tt.cards.retain(|c| !mine(c));
    tt.cards.extend(emit(&m, v, &pl, false));
    Ok(tt)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SwapInfo {
    pub lesson_id: String,
    pub number: u32,
    pub ok: bool,
    pub reasons: Vec<String>,
}

/// Can `a` take the place of the placed card `b`, with `b` going to where `a` was (`a_old`,
/// or off the timetable when `a` was not placed)? `st` must not contain `a`. State is restored.
fn try_swap(m: &Model, st: &mut State, a: usize, b: usize, a_old: Option<Pl>, a_pref: Option<usize>, b_pref: Option<usize>) -> Result<(Pl, Option<Pl>), Vec<String>> {
    let Some(pb) = st.place[b] else { return Err(vec!["That lesson is not on the timetable.".into()]) };
    if m.cards[a].shift != m.cards[b].shift {
        return Err(vec!["They are in different shifts.".into()]);
    }
    st.unplace(b);
    let result = (|| {
        let (d, p) = (pb.day as usize, pb.period as usize);
        let (status, reasons, _) = evaluate(m, st, a, d, p);
        if status == "blocked" {
            return Err(reasons);
        }
        let sa = *m.cards[a].domain.iter().find(|s| s.day as usize == d && s.period as usize == p).ok_or_else(|| vec!["Not allowed at that time.".to_string()])?;
        let mut new_a = Pl::new(pb.day, pb.period);
        new_a.rooms = rooms_for(m, st, a, sa, a_pref).map_err(|e| vec![e])?;
        let Some(old) = a_old else { return Ok((new_a, None)) };
        st.place(a, new_a);
        let r = (|| {
            let (status, reasons, _) = evaluate(m, st, b, old.day as usize, old.period as usize);
            if status == "blocked" {
                return Err(reasons);
            }
            let sb = *m.cards[b].domain.iter().find(|s| s.day == old.day && s.period == old.period).ok_or_else(|| vec!["Not allowed at that time.".to_string()])?;
            let mut new_b = Pl::new(old.day, old.period);
            new_b.rooms = rooms_for(m, st, b, sb, b_pref).map_err(|e| vec![e])?;
            Ok(new_b)
        })();
        st.unplace(a);
        Ok((new_a, Some(r?)))
    })();
    st.place(b, pb);
    st.commit();
    result
}

fn first_room(m: &Model, v: usize, pcs: &[&PlacedCard]) -> Option<usize> {
    let r = super::rooms_of(m, v, pcs)[0];
    (r >= 0).then_some(r as usize)
}

/// For every placed lesson in the same shift: could the dragged session swap places with it?
pub fn swap_options(setup: &SchoolSetup, lesson_id: &str, number: u32) -> Result<Vec<SwapInfo>, String> {
    let m = compile(setup);
    let a = find_card(&m, lesson_id, number)?;
    let tt = setup.timetable.clone().unwrap_or_default();
    let mut st = state_from(&m, &tt, &[a]);
    let groups = group_by_card(&m, &tt.cards);
    let a_pcs = &groups[a];
    let a_old = a_pcs.first().map(|c| Pl::new(c.day, c.period - 1));
    let a_pref = first_room(&m, a, a_pcs);
    // Linked lessons share one card, so the answer for a card is worked out once.
    let mut answers: HashMap<usize, (bool, Vec<String>)> = HashMap::new();
    let mut out = Vec::new();
    for pc in tt.cards.iter().filter(|c| c.shift == m.shift_ids[m.cards[a].shift]) {
        let Ok(b) = find_card(&m, &pc.lesson_id, pc.number) else { continue };
        if b == a {
            continue;
        }
        let (ok, reasons) = answers
            .entry(b)
            .or_insert_with(|| {
                let b_pcs = &groups[b];
                if b_pcs.iter().any(|c| c.locked) {
                    (false, vec!["This lesson is locked.".to_string()])
                } else {
                    match try_swap(&m, &mut st, a, b, a_old, a_pref, first_room(&m, b, b_pcs)) {
                        Ok(_) => (true, vec![]),
                        Err(r) => (false, r),
                    }
                }
            })
            .clone();
        out.push(SwapInfo { lesson_id: pc.lesson_id.clone(), number: pc.number, ok, reasons });
    }
    Ok(out)
}

/// Swaps the dragged session with another placed one. If the dragged session was not on the
/// timetable yet, the other one goes off it.
pub fn swap_cards(setup: &SchoolSetup, lesson_id: &str, number: u32, with_lesson: &str, with_number: u32) -> Result<Timetable, String> {
    let m = compile(setup);
    let a = find_card(&m, lesson_id, number)?;
    let b = find_card(&m, with_lesson, with_number)?;
    if a == b {
        return Err("Choose a different lesson to swap with.".into());
    }
    let mut tt = setup.timetable.clone().unwrap_or_default();
    let groups = group_by_card(&m, &tt.cards);
    let (pa, pb): (Vec<PlacedCard>, Vec<PlacedCard>) = (groups[a].iter().map(|c| (*c).clone()).collect(), groups[b].iter().map(|c| (*c).clone()).collect());
    if pb.is_empty() {
        return Err("That lesson is not on the timetable.".into());
    }
    if pa.iter().chain(pb.iter()).any(|c| c.locked) {
        return Err("A locked lesson cannot be swapped. Unlock it first.".into());
    }
    let mut st = state_from(&m, &tt, &[a]);
    let a_old = pa.first().map(|c| Pl::new(c.day, c.period - 1));
    let (ra, rb): (Vec<&PlacedCard>, Vec<&PlacedCard>) = (pa.iter().collect(), pb.iter().collect());
    let (new_a, new_b) = try_swap(&m, &mut st, a, b, a_old, first_room(&m, a, &ra), first_room(&m, b, &rb)).map_err(|r| r.join(" "))?;
    tt.cards.retain(|c| { let v = card_of(&m, &c.lesson_id, c.number); v != Some(a) && v != Some(b) });
    tt.cards.extend(emit(&m, a, &new_a, false));
    if let Some(nb) = new_b {
        tt.cards.extend(emit(&m, b, &nb, false));
    }
    Ok(tt)
}

/// True if the saved timetable still matches the setup (no clashes, nothing out of range).
pub fn is_consistent(setup: &SchoolSetup) -> bool {
    // A timetable built with "bend a few rules" may legitimately break those rules.
    let bend = setup.settings.allow_relaxation;
    let enforce = super::verify::Enforce { same_day: !bend, subject_day: !bend, consecutive: !bend, per_day: !bend };
    match &setup.timetable {
        Some(t) => super::verify::check_with(setup, t, enforce).is_empty(),
        None => true,
    }
}
