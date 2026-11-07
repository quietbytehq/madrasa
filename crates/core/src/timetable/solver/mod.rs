//! Timetable generator (milestone 6, blueprint sections 4, 5.1 and 7).
//!
//! Pipeline: compile the setup into small integer arrays + bitmasks (`model`), order cards
//! by difficulty, place them with ejection chains (`engine`), then explain anything left
//! over in plain English. `verify` is an independent checker of the hard rules.
pub mod edit;
mod engine;
mod repair;
pub mod soft;
mod model;
/// Independent rule checker; used by the tests now and by manual editing later.
#[allow(dead_code)]
pub mod verify;

#[cfg(test)]
mod tests;

pub use model::class_masks;

use crate::timetable::model::SchoolSetup;
use engine::{Blocker, Config, Engine, Pl, Rules, State, Violation};
use model::{compile, Model, MAXR};
use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

/// One lesson session on the timetable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlacedCard {
    pub lesson_id: String,
    /// 1-based session number within the lesson.
    pub number: u32,
    pub shift: String,
    /// 0 = Monday.
    pub day: u8,
    /// 1-based period at which the session starts.
    pub period: u8,
    pub length: u32,
    #[serde(default)]
    pub room: Option<String>,
    #[serde(default)]
    pub locked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Timetable {
    pub cards: Vec<PlacedCard>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UnplacedCard {
    pub lesson_id: String,
    pub number: u32,
    pub title: String,
    pub message: String,
    pub fix: String,
}

/// One lesson session: the lesson and which of its sessions (1-based).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionRef {
    pub lesson_id: String,
    pub number: u32,
}

/// A locked lesson that stops others from being placed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LockInTheWay {
    /// The sessions locked together (more than one for "same time" lessons).
    pub sessions: Vec<SessionRef>,
    pub title: String,
    /// "Monday period 2"
    pub when: String,
    /// How many unplaced lesson sessions it is in the way of.
    pub blocks: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SolveReport {
    pub total: u32,
    pub placed: u32,
    pub unplaced: Vec<UnplacedCard>,
    pub timetable: Timetable,
    pub seconds: f64,
    pub cancelled: bool,
    pub timed_out: bool,
    /// Quality right after the lessons were placed, and after improving.
    pub quality_before: soft::Quality,
    pub quality_after: soft::Quality,
    /// Rules that had to be bent to place every lesson (only when relaxation is allowed).
    pub relaxed: Vec<Relaxation>,
    /// Locked lessons that stop other sessions from being placed, the worst first.
    #[serde(default)]
    pub locks_in_the_way: Vec<LockInTheWay>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Relaxation {
    /// sameDay | subjectPerDay | consecutive | perDay
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct Options {
    pub time_limit: Duration,
    pub seed: u64,
    pub max_depth: usize,
    /// How long to spend improving quality once every lesson is placed (zero = skip).
    pub improve_time: Duration,
    /// When sessions are still left over, shake the timetable around them and try again.
    pub repair: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { time_limit: Duration::from_secs(120), seed: 1, max_depth: 8, improve_time: Duration::ZERO, repair: true }
    }
}

/// What the generator is doing right now.
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Placing,
    Improving,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub phase: Phase,
    pub placed: usize,
    pub total: usize,
    /// 0..1 through the improving phase
    pub fraction: f64,
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// Generates a timetable. `locked` cards are placed first and never moved.
/// `progress(placed, total)` is called now and then; set `cancel` to stop early
/// (the best timetable found so far is still returned).
pub fn solve(
    setup: &SchoolSetup,
    locked: &[PlacedCard],
    opts: &Options,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(Progress),
) -> SolveReport {
    let t0 = Instant::now();
    let m = compile(setup);
    let total = m.cards.len();
    let sessions_of = |places: &[Option<Pl>]| -> usize { places.iter().enumerate().filter(|(_, p)| p.is_some()).map(|(v, _)| m.cards[v].members.len()).sum() };
    let total_sessions: usize = m.cards.iter().map(|c| c.members.len()).sum();
    let deadline = t0 + opts.time_limit;
    // Cards that can never be placed (bad data, or no allowed time at all).
    let structural = m.cards.iter().filter(|c| c.broken.is_some() || c.domain.is_empty()).count();

    let mut best: Option<(usize, Vec<Option<Pl>>, Vec<bool>)> = None;
    let problems: Vec<(usize, String)>;
    let (cancelled, timed_out);
    let mut attempt = 0u64;
    loop {
        let stagnation = (opts.time_limit / 30).clamp(Duration::from_millis(500), Duration::from_secs(4));
        let cfg = Config { max_depth: opts.max_depth, stagnation, deadline, seed: opts.seed.wrapping_add(attempt * 7919), limit: None, node_budget: None };
        let mut eng = Engine::new(&m, &cfg);
        if setup.settings.allow_relaxation {
            eng.s.rules = Rules::RELAXED;
        }
        let probs = place_locked(&mut eng, &m, locked);
        eng.s.rules = Rules::STRICT;
        let queue: Vec<usize> = (0..total).filter(|&v| eng.s.place[v].is_none() && !probs.iter().any(|(p, _)| *p == v)).collect();
        let floor = best.as_ref().map_or(0, |b| sessions_of(&b.1));
        let out = eng.run(queue, &cfg, cancel, &mut |n| progress(Progress { phase: Phase::Placing, placed: n.max(floor), total: total_sessions, fraction: 0.0 }));
        let n = out.best.iter().flatten().count();
        if best.as_ref().map_or(true, |b| n > b.0) {
            best = Some((n, out.best, eng.s.locked.clone()));
        }
        attempt += 1;
        let done = best.as_ref().unwrap().0 + structural + probs.len() >= total;
        if done || out.cancelled || out.timed_out || Instant::now() >= deadline || t0.elapsed() > opts.time_limit.min(Duration::from_secs(15)) || attempt >= 12 {
            cancelled = out.cancelled;
            timed_out = out.timed_out;
            problems = probs;
            break;
        }
    }
    let (_, mut best_place, best_locked) = best.unwrap();
    let (mut cancelled, mut timed_out) = (cancelled, timed_out);

    // Relaxation: if lessons are still left over and the school allows it, bend a few rules.
    if setup.settings.allow_relaxation && !cancelled {
        let queue: Vec<usize> = (0..total)
            .filter(|&v| best_place[v].is_none() && m.cards[v].broken.is_none() && !m.cards[v].domain.is_empty() && !problems.iter().any(|(p, _)| *p == v))
            .collect();
        if !queue.is_empty() && Instant::now() < deadline {
            let stagnation = (opts.time_limit / 30).clamp(Duration::from_millis(500), Duration::from_secs(4));
            let cfg = Config { max_depth: opts.max_depth, stagnation, deadline, seed: opts.seed.wrapping_add(99_991), limit: None, node_budget: None };
            let mut eng = Engine::new(&m, &cfg);
            eng.s.rules = Rules::RELAXED;
            for (v, p) in best_place.iter().enumerate() {
                if let Some(p) = p {
                    eng.s.place(v, *p);
                    eng.s.locked[v] = best_locked[v];
                }
            }
            eng.s.commit();
            let floor = sessions_of(&best_place);
            let out = eng.run(queue, &cfg, cancel, &mut |n| progress(Progress { phase: Phase::Placing, placed: n.max(floor), total: total_sessions, fraction: 0.0 }));
            if out.best.iter().flatten().count() > floor {
                best_place = out.best;
            }
            cancelled = out.cancelled;
            timed_out = timed_out || out.timed_out;
        }
    }

    // Repair: when a few sessions are still left over, shake the timetable around them and try again.
    let mut best_place = best_place;
    let leftover = |places: &[Option<Pl>]| (0..m.cards.len()).filter(|&v| places[v].is_none() && m.cards[v].broken.is_none() && !m.cards[v].domain.is_empty() && !problems.iter().any(|(p, _)| *p == v)).count();
    if opts.repair && !cancelled && leftover(&best_place) > 0 && Instant::now() < deadline {
        let rules = if setup.settings.allow_relaxation { Rules::RELAXED } else { Rules::STRICT };
        let floor = sessions_of(&best_place);
        let report = &mut |n: usize| progress(Progress { phase: Phase::Placing, placed: n.max(floor), total: total_sessions, fraction: 0.0 });
        let (fixed, stopped) = repair::repair(&m, best_place, &best_locked, rules, opts.max_depth, opts.seed, deadline, cancel, t0.elapsed(), &mut |places| report(sessions_of(places)));
        best_place = fixed;
        cancelled = cancelled || stopped;
    }

    // Improve quality while every hard rule stays satisfied.
    let w = &setup.settings.weights;
    let mut fin = state_from_places(&m, &best_place, &best_locked);
    let quality_before = soft::quality_of(&soft::measure(&fin), w);
    let placed_now = sessions_of(&best_place);
    if !cancelled && !opts.improve_time.is_zero() && placed_now > 0 {
        let res = soft::improve(&mut fin, w, opts.seed, Instant::now() + opts.improve_time, cancel, &mut |f| {
            progress(Progress { phase: Phase::Improving, placed: placed_now, total: total_sessions, fraction: f })
        });
        best_place = res.best;
        fin = state_from_places(&m, &best_place, &best_locked);
    }
    let quality_after = soft::quality_of(&soft::measure(&fin), w);
    let relaxed = if setup.settings.allow_relaxation { describe_relaxed(&m, &best_place, &best_locked) } else { Vec::new() };
    // Locked lessons that stop others from being placed (only looked at when something is left over).
    let locks_only = (best_locked.iter().any(|&l| l) && best_place.iter().any(|p| p.is_none())).then(|| {
        let places: Vec<Option<Pl>> = best_place.iter().zip(&best_locked).map(|(p, &l)| if l { *p } else { None }).collect();
        state_from_places(&m, &places, &best_locked)
    });
    let mut blame: Vec<(usize, u32)> = Vec::new();
    let mut blame_on = |o: usize, sessions: u32| match blame.iter_mut().find(|b| b.0 == o) {
        Some(b) => b.1 += sessions,
        None => blame.push((o, sessions)),
    };
    let out = engine::Outcome { best: best_place, cancelled, timed_out };
    let locked_flags = best_locked;
    let mut cards = Vec::new();
    let mut unplaced = Vec::new();
    for v in 0..total {
        let c = &m.cards[v];
        match out.best[v] {
            Some(p) => cards.extend(emit(&m, v, &p, locked_flags[v])),
            None => {
                let sessions = c.members.len() as u32;
                let (mut message, mut fix) = match problems.iter().find(|(p, _)| *p == v) {
                    Some((_, msg)) => {
                        // the other locked lessons it clashes with are to blame
                        if let Some(p) = m.cards[v].domain.iter().find(|s| locked.iter().any(|lc| card_of(&m, &lc.lesson_id, lc.number) == Some(v) && lc.day == s.day && lc.period as usize == s.period as usize + 1)) {
                            for (o, _) in fin.conflicts(v, *p).0 {
                                if fin.locked[o as usize] {
                                    blame_on(o as usize, sessions);
                                }
                            }
                        }
                        (msg.clone(), "Unlock or move that lesson, then generate again.".to_string())
                    }
                    None => explain(&m, &fin, v),
                };
                if let (Some(lo), None, false) = (&locks_only, &c.broken, c.domain.is_empty() || problems.iter().any(|(p, _)| *p == v)) {
                    let (all, by) = locks_in_the_way(&m, &fin, lo, v);
                    if !by.is_empty() {
                        let shown: Vec<String> = by.iter().take(4).map(|&o| placed_title(&m, if all { lo } else { &fin }, o)).collect();
                        let more = if by.len() > 4 { format!(", and {} more", by.len() - 4) } else { String::new() };
                        if all {
                            message = format!(
                                "{} cannot be placed: the locked lessons already take every time it could use. Locked lessons in the way: {}{more}.",
                                m.card_title(v),
                                shown.join(", ")
                            );
                            fix = "Unlock some of these lessons (or press \"Unlock these\" below), then build again.".into();
                        } else {
                            message = format!("{message} Locked lessons in the way: {}{more}.", shown.join(", "));
                        }
                        for &o in &by {
                            blame_on(o, sessions);
                        }
                    }
                }
                // One entry per lesson session, so the counts match "placed of total".
                for mb in &c.members {
                    unplaced.push(UnplacedCard {
                        lesson_id: m.lesson_ids[mb.lesson].clone(),
                        number: mb.number,
                        title: format!("{} (session {} of {})", m.lesson_titles[mb.lesson], mb.number, m.cards_of_lesson[mb.lesson].len()),
                        message: message.clone(),
                        fix: fix.clone(),
                    });
                }
            }
        }
    }
    blame.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let locks_in_the_way = blame
        .into_iter()
        .filter_map(|(o, n)| {
            let p = out.best[o]?;
            Some(LockInTheWay {
                sessions: m.cards[o].members.iter().map(|mb| SessionRef { lesson_id: m.lesson_ids[mb.lesson].clone(), number: mb.number }).collect(),
                title: m.card_title(o),
                when: format!("{} period {}", m.day_names.get(p.day as usize).map_or("", |d| d.as_str()), p.period + 1),
                blocks: n,
            })
        })
        .collect();
    let placed = sessions_of(&out.best);
    progress(Progress { phase: Phase::Placing, placed, total: total_sessions, fraction: 1.0 });
    SolveReport {
        total: total_sessions as u32,
        placed: placed as u32,
        unplaced,
        timetable: Timetable { cards },
        seconds: t0.elapsed().as_secs_f64(),
        cancelled: out.cancelled,
        timed_out: out.timed_out,
        quality_before,
        quality_after,
        relaxed,
        locks_in_the_way,
    }
}

/// Plain-English reason why a card is still unplaced.
fn explain(m: &Model, st: &State, v: usize) -> (String, String) {
    let c = &m.cards[v];
    let title = &m.card_title(v);
    if let Some(b) = &c.broken {
        return (b.clone(), "Fix the lesson on the Lessons page.".into());
    }
    if c.domain.is_empty() {
        // Who has the fewest usable starts on their own?
        let mut worst: Option<(usize, String)> = None;
        let mut consider = |name: String, free: usize| {
            if worst.as_ref().map_or(true, |(f, _)| free < *f) {
                worst = Some((free, name));
            }
        };
        let starts = |forb: &dyn Fn(usize) -> bool| -> usize {
            let mut n = 0;
            for d in 0..m.nd {
                let per = m.periods[c.shift][d];
                for p in 0..per {
                    if p + c.len > per || (p + 1..p + c.len).any(|k| !m.cross_ok[c.shift][k]) {
                        continue;
                    }
                    if (0..c.len).all(|k| !forb(m.slot(c.shift, d, p + k))) {
                        n += 1;
                    }
                }
            }
            n
        };
        for &t in &c.teachers {
            consider(m.teacher_names[t].clone(), starts(&|sl| m.t_forb[t * m.slots + sl]));
        }
        for &(ci, _) in &c.classes {
            consider(format!("class {}", m.class_names[ci]), starts(&|sl| m.c_forb[ci * m.slots + sl]));
        }
        for pool in &c.pools {
            let free = starts(&|sl| pool.iter().all(|&r| m.r_forb[r * m.slots + sl]));
            consider("the allowed rooms".into(), free);
        }
        let who = worst.map(|(f, n)| format!(" {n} has only {} possible start {}.", f, if f == 1 { "time" } else { "times" })).unwrap_or_default();
        return (
            format!("{title} has no time when everyone it needs is free at once (time off and breaks remove every option).{who}"),
            "Free up some time-off cells, allow breaks to be crossed, or make the session shorter.".into(),
        );
    }
    // Count what blocks each possible start.
    let mut counts: Vec<(String, usize)> = Vec::new();
    let mut bump = |name: String| match counts.iter_mut().find(|(n, _)| *n == name) {
        Some(e) => e.1 += 1,
        None => counts.push((name, 1)),
    };
    for &s in &c.domain {
        let (bl, _) = st.conflicts(v, s);
        if bl.is_empty() {
            if let Some(t) = st.limit_problem(v, s) {
                bump(format!("{}'s limits", m.teacher_names[t]));
            }
        }
        for (_, b) in bl {
            bump(match b {
                Blocker::Teacher(t) => m.teacher_names[t].clone(),
                Blocker::Class(ci) => format!("class {}", m.class_names[ci]),
                Blocker::Room(r) => format!("room {}", m.room_names[r]),
                Blocker::SameDay => "another session of the same lesson on that day".into(),
                Blocker::SameSubject(ci, sub) => format!("class {} already having {} that day", m.class_names[ci], m.subject_names[sub]),
            });
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1));
    let top: Vec<String> = counts.iter().take(3).map(|(n, k)| format!("{n} ({k}×)")).collect();
    let n = c.domain.len();
    (
        format!(
            "{title} could not be fitted in. It has {} to choose from, and each was already taken. Most often in the way: {}.",
            plural(n, "possible time", "possible times"),
            top.join(", ")
        ),
        "Reduce what the busiest of these has to do, free up their time-off cells, add another room or teacher, or make a session shorter. You can also try generating again.".into(),
    )
}

/// The room of each pool, in order, as given by the placed cards of a card's members.
pub(super) fn rooms_of(m: &Model, v: usize, pcs: &[&PlacedCard]) -> [i16; MAXR] {
    let mut out = [-1i16; MAXR];
    let mut k = 0;
    for mb in &m.cards[v].members {
        if mb.pool.is_empty() {
            continue;
        }
        let id = &m.lesson_ids[mb.lesson];
        let room = pcs.iter().find(|c| &c.lesson_id == id && c.number == mb.number).and_then(|c| c.room.as_ref());
        if let Some(r) = room.and_then(|r| m.room_ids.iter().position(|x| x == r)) {
            out[k] = r as i16;
        }
        k += 1;
    }
    out
}

/// One `PlacedCard` per lesson of the card.
pub(super) fn emit(m: &Model, v: usize, p: &Pl, locked: bool) -> Vec<PlacedCard> {
    let c = &m.cards[v];
    let mut k = 0;
    c.members
        .iter()
        .map(|mb| {
            let room = if mb.pool.is_empty() {
                None
            } else {
                let r = p.rooms[k];
                k += 1;
                (r >= 0).then(|| m.room_ids[r as usize].clone())
            };
            PlacedCard {
                lesson_id: m.lesson_ids[mb.lesson].clone(),
                number: mb.number,
                shift: m.shift_ids[c.shift].clone(),
                day: p.day,
                period: p.period + 1,
                length: c.len as u32,
                room,
                locked,
            }
        })
        .collect()
}

/// The card that holds a given lesson session.
pub(super) fn card_of(m: &Model, lesson_id: &str, number: u32) -> Option<usize> {
    let li = *m.lesson_index.get(lesson_id)?;
    m.cards_of_lesson[li].get(number.wrapping_sub(1) as usize).copied()
}

/// Placed sessions grouped by the solver card they belong to (one pass over the timetable).
/// Entry `v` lists, in timetable order, every placed session of card `v` (more than one when
/// lessons are linked to happen at the same time).
pub(super) fn group_by_card<'a>(m: &Model, cards: &'a [PlacedCard]) -> Vec<Vec<&'a PlacedCard>> {
    let mut out: Vec<Vec<&PlacedCard>> = vec![Vec::new(); m.cards.len()];
    for c in cards {
        if let Some(v) = card_of(m, &c.lesson_id, c.number) {
            out[v].push(c);
        }
    }
    out
}

/// The current timetable as solver state, leaving out the cards in `skip`.
/// Sessions that no longer fit the setup (changed lessons, shorter day, ...) are ignored.
pub(super) fn state_from<'a>(m: &'a Model, tt: &Timetable, skip: &[usize]) -> State<'a> {
    let mut st = State::new(m);
    // A timetable built with relaxation may break bendable rules; it must still load completely.
    st.rules = Rules::RELAXED;
    let groups = group_by_card(m, &tt.cards);
    let mut done = vec![false; m.cards.len()];
    for &v in skip {
        if let Some(d) = done.get_mut(v) {
            *d = true;
        }
    }
    for pc in &tt.cards {
        let Some(v) = card_of(m, &pc.lesson_id, pc.number) else { continue };
        if done[v] || m.cards[v].broken.is_some() {
            continue;
        }
        done[v] = true;
        let mine = &groups[v];
        let p = (pc.period as usize).wrapping_sub(1);
        let Some(s) = m.cards[v].domain.iter().copied().find(|s| s.day == pc.day && s.period as usize == p) else { continue };
        let (bl, assigned) = st.conflicts(v, s);
        if !bl.is_empty() {
            continue;
        }
        let given = rooms_of(m, v, mine);
        let mut pl = Pl::new(s.day, s.period);
        for k in 0..MAXR {
            let g = given[k];
            pl.rooms[k] = if g >= 0 && st.room_free(g as usize, m.cards[v].shift, s, m.cards[v].len) { g } else { assigned[k] };
        }
        st.place(v, pl);
        st.locked[v] = mine.iter().any(|c| c.locked);
    }
    st.commit();
    st.rules = Rules::STRICT;
    st
}

/// Locked cards go in first, at depth 0. Returns the ones that cannot stay where they are.
fn place_locked(eng: &mut Engine, m: &Model, locked: &[PlacedCard]) -> Vec<(usize, String)> {
    let mut problems = Vec::new();
    let groups = group_by_card(m, locked);
    let mut done = vec![false; m.cards.len()];
    for lc in locked {
        let Some(v) = card_of(m, &lc.lesson_id, lc.number) else { continue };
        if done[v] {
            continue;
        }
        done[v] = true;
        let mine: &Vec<&PlacedCard> = &groups[v];
        let p = (lc.period as usize).wrapping_sub(1);
        let title = m.card_title(v);
        let Some(st) = m.cards[v].domain.iter().copied().find(|s| s.day == lc.day && s.period as usize == p) else {
            problems.push((v, format!("The locked lesson {title} cannot stay where it is: that time is not allowed (time off, a break, or the end of the day).")));
            continue;
        };
        let (bl, assigned) = eng.s.conflicts(v, st);
        if bl.is_empty() && eng.s.limit_problem(v, st).is_none() {
            let given = rooms_of(m, v, mine);
            let mut pl = Pl::new(st.day, st.period);
            for k in 0..MAXR {
                let g = given[k];
                pl.rooms[k] = if g >= 0 && eng.s.room_free(g as usize, m.cards[v].shift, st, m.cards[v].len) { g } else { assigned[k] };
            }
            eng.s.place(v, pl);
            eng.s.locked[v] = true;
        } else if !bl.is_empty() {
            let mut others: Vec<usize> = Vec::new();
            for (o, _) in &bl {
                if !others.contains(&(*o as usize)) {
                    others.push(*o as usize);
                }
            }
            let names: Vec<String> = others.iter().map(|&o| placed_title(m, &eng.s, o)).collect();
            problems.push((v, format!("The locked lesson {title} cannot stay where it is: it clashes with the locked lesson {}.", names.join(" and the locked lesson "))));
        } else {
            let who = eng.s.limit_problem(v, st).map(|t| m.teacher_names[t].clone()).unwrap_or_else(|| "a teacher".into());
            problems.push((v, format!("The locked lesson {title} cannot stay where it is: together with the other locked lessons, {who} would go over their limits for the day or week.")));
        }
    }
    eng.s.commit();
    problems
}

/// "Mathematics · 6A (Monday period 2)": a placed card and where it is.
fn placed_title(m: &Model, st: &State, o: usize) -> String {
    match st.place[o] {
        Some(p) => format!("{} ({} period {})", m.card_title(o), m.day_names.get(p.day as usize).map_or("", |d| d.as_str()), p.period + 1),
        None => m.card_title(o),
    }
}

/// Which locked cards stop card `v` from being placed. `all` is true when the locked cards alone
/// (with nothing else on the timetable) already take every time `v` could use: then the locks
/// are the cause. Otherwise the list holds the locked cards that are the only thing in the way of
/// some of its times in the final timetable.
fn locks_in_the_way(m: &Model, fin: &State, locks_only: &State, v: usize) -> (bool, Vec<usize>) {
    let mut out: Vec<usize> = Vec::new();
    let add = |o: usize, out: &mut Vec<usize>| {
        if !out.contains(&o) {
            out.push(o);
        }
    };
    let mut all = true;
    for &s in &m.cards[v].domain {
        let (bl, _) = locks_only.conflicts(v, s);
        if bl.is_empty() && locks_only.limit_problem(v, s).is_none() {
            all = false;
            break;
        }
        for (o, _) in bl {
            add(o as usize, &mut out);
        }
    }
    if all {
        return (true, out);
    }
    out.clear();
    for &s in &m.cards[v].domain {
        let (bl, _) = fin.conflicts(v, s);
        if !bl.is_empty() && bl.iter().all(|(o, _)| fin.locked[*o as usize]) {
            for (o, _) in bl {
                add(o as usize, &mut out);
            }
        }
    }
    (false, out)
}

/// Solver state holding exactly these placements.
pub(super) fn state_from_places<'a>(m: &'a Model, places: &[Option<Pl>], locked: &[bool]) -> State<'a> {
    let mut st = State::new(m);
    for (v, p) in places.iter().enumerate() {
        if let Some(p) = p {
            st.place(v, *p);
        }
    }
    st.locked = locked.to_vec();
    st.commit();
    st
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImproveReport {
    pub timetable: Timetable,
    pub quality_before: soft::Quality,
    pub quality_after: soft::Quality,
    pub seconds: f64,
}

/// Improves an existing, complete-or-partial timetable (for example after hand edits).
/// Locked sessions stay where they are. Nothing is changed if the quality cannot be improved.
pub fn improve_timetable(
    setup: &SchoolSetup,
    seconds: Duration,
    seed: u64,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f64),
) -> ImproveReport {
    let t0 = Instant::now();
    let m = compile(setup);
    let tt = setup.timetable.clone().unwrap_or_default();
    let mut st = state_from(&m, &tt, &[]);
    let w = &setup.settings.weights;
    let before = soft::quality_of(&soft::measure(&st), w);
    let res = soft::improve(&mut st, w, seed, Instant::now() + seconds, cancel, progress);
    let locked = st.locked.clone();
    let st2 = state_from_places(&m, &res.best, &locked);
    let after = soft::quality_of(&soft::measure(&st2), w);
    let (timetable, after) = if after.penalty < before.penalty {
        let mut cards = Vec::new();
        for v in 0..m.cards.len() {
            if let Some(p) = res.best[v] {
                cards.extend(emit(&m, v, &p, locked[v]));
            }
        }
        (Timetable { cards }, after)
    } else {
        (tt, before.clone())
    };
    ImproveReport { timetable, quality_before: before, quality_after: after, seconds: t0.elapsed().as_secs_f64() }
}

/// Lists the bendable rules that the given placements break, in plain English.
fn describe_relaxed(m: &Model, places: &[Option<Pl>], locked: &[bool]) -> Vec<Relaxation> {
    let mut st = state_from_places(m, places, locked);
    let mut out: Vec<Relaxation> = Vec::new();
    let mut add = |kind: &str, message: String| {
        if !out.iter().any(|r| r.message == message) {
            out.push(Relaxation { kind: kind.into(), message });
        }
    };
    for v in 0..m.cards.len() {
        let Some(pl) = st.place[v] else { continue };
        let Some(start) = m.cards[v].domain.iter().copied().find(|s| s.day == pl.day && s.period == pl.period) else { continue };
        st.unplace(v);
        let violations = st.violations(v, start);
        st.place(v, pl);
        st.commit();
        let day = &m.day_names[pl.day as usize];
        for x in violations {
            match x {
                Violation::SameDay => add("sameDay", format!("{} has two sessions on {day}.", m.card_title(v))),
                Violation::Subject(c, s, n, lim) => add("subjectPerDay", format!(
                    "Class {} has {n} sessions of {} on {day} (the limit is {lim}).", m.class_names[c], m.subject_names[s])),
                Violation::Consecutive(t, run, max) => add("consecutive", format!(
                    "{} has {run} lessons in a row on {day} (the limit is {max}).", m.teacher_names[t])),
                Violation::PerDay(t, load, max) => add("perDay", format!(
                    "{} has {load} lessons on {day} (the limit is {max}).", m.teacher_names[t])),
            }
        }
    }
    out
}

/// Benchmark helper (stress tests only): how long compiling the setup takes, and how big it is.
#[cfg(test)]
pub fn bench_compile(setup: &SchoolSetup) -> (f64, usize, usize) {
    let t = Instant::now();
    let m = compile(setup);
    let secs = t.elapsed().as_secs_f64();
    (secs, m.cards.len(), m.cards.iter().map(|c| c.domain.len()).sum())
}
