//! Quality ("soft") rules and the improvement pass (milestone 8, blueprint 5.2 and 7 phase 3).
//! A timetable that already satisfies every hard rule is made better by moving and swapping
//! sessions (simulated annealing), never breaking a hard rule on the way.
use super::engine::{Pl, Rng, State};
use super::model::{Model, Start};
use crate::timetable::settings::Weights;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

/// Unweighted amount of each kind of imperfection.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Terms {
    pub teacher_gaps: f64,
    pub class_gaps: f64,
    pub spread: f64,
    pub avoid: f64,
    pub balance: f64,
    pub rooms: f64,
}

impl Terms {
    pub fn weighted(&self, w: &Weights) -> f64 {
        self.teacher_gaps * w.teacher_gaps as f64
            + self.class_gaps * w.class_gaps as f64
            + self.spread * w.spread as f64
            + self.avoid * w.avoid as f64
            + self.balance * w.balance as f64
            + self.rooms * w.room_consistency as f64
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QualityItem {
    pub key: String,
    /// Plain-English description of what is counted.
    pub label: String,
    pub value: f64,
    pub weight: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Quality {
    pub items: Vec<QualityItem>,
    /// Weighted total; lower is better.
    pub penalty: f64,
}

pub fn quality_of(t: &Terms, w: &Weights) -> Quality {
    let item = |key: &str, label: &str, value: f64, weight: u32| QualityItem { key: key.into(), label: label.into(), value, weight };
    Quality {
        items: vec![
            item("teacherGaps", "Free periods in the middle of teachers' days", t.teacher_gaps, w.teacher_gaps),
            item("classGaps", "Free periods and late starts in classes' days", t.class_gaps, w.class_gaps),
            item("spread", "Sessions of a lesson bunched on neighbouring days", (t.spread * 10.0).round() / 10.0, w.spread),
            item("avoid", "Times someone asked to avoid", t.avoid, w.avoid),
            item("balance", "Uneven days for classes (periods away from the average)", (t.balance * 10.0).round() / 10.0, w.balance),
            item("rooms", "Extra room changes between sessions of a lesson", t.rooms, w.room_consistency),
        ],
        penalty: (t.weighted(w) * 10.0).round() / 10.0,
    }
}

/// Which pool-ordinal of card `v` belongs to member `mi` (None if the member needs no room).
fn pool_ordinal(m: &Model, v: usize, mi: usize) -> Option<usize> {
    let c = &m.cards[v];
    if c.members[mi].pool.is_empty() {
        return None;
    }
    Some(c.members[..mi].iter().filter(|x| !x.pool.is_empty()).count())
}

fn teacher_line(st: &State, t: usize, shift: usize, day: usize) -> f64 {
    let m = st.m;
    let base = m.slot(shift, day, 0) + t * m.slots;
    let (mut first, mut last, mut n) = (usize::MAX, 0usize, 0usize);
    for p in 0..m.periods[shift][day] {
        if st.t_occ[base + p] != 0 {
            first = first.min(p);
            last = p;
            n += 1;
        }
    }
    if n == 0 { 0.0 } else { ((last - first + 1) - n) as f64 }
}

/// Free periods inside the class's day plus periods before its first lesson.
fn class_line(st: &State, c: usize, shift: usize, day: usize) -> f64 {
    let m = st.m;
    let base = m.slot(shift, day, 0) + c * m.slots;
    let (mut first, mut last, mut n) = (usize::MAX, 0usize, 0usize);
    for p in 0..m.periods[shift][day] {
        if st.c_mask[base + p] != 0 {
            first = first.min(p);
            last = p;
            n += 1;
        }
    }
    if n == 0 { 0.0 } else { ((last - first + 1) - n + first) as f64 }
}

fn class_days(m: &Model, shift: usize) -> f64 {
    (0..m.nd).filter(|&d| m.periods[shift][d] > 0).count().max(1) as f64
}

fn balance_line(st: &State, c: usize, shift: usize, day: usize) -> f64 {
    let m = st.m;
    let total: u32 = (0..m.nd).map(|d| st.c_day_load[c * m.nd + d]).sum();
    let avg = total as f64 / class_days(m, shift);
    (st.c_day_load[c * m.nd + day] as f64 - avg).abs()
}

/// Sessions bunched on neighbouring days, for one lesson.
fn spread_of(st: &State, lesson: usize) -> f64 {
    let m = st.m;
    let cards = &m.cards_of_lesson[lesson];
    if cards.len() < 2 {
        return 0.0;
    }
    let mut days: Vec<usize> = cards.iter().filter_map(|&v| st.place[v].map(|p| p.day as usize)).collect();
    if days.len() < 2 {
        return 0.0;
    }
    days.sort_unstable();
    let shift = m.cards[cards[0]].shift;
    let d = class_days(m, shift);
    let ideal = d / days.len() as f64;
    days.windows(2).map(|w| (ideal - (w[1] - w[0]) as f64).max(0.0)).sum()
}

fn avoid_of(st: &State, v: usize) -> f64 {
    let m = st.m;
    let Some(p) = st.place[v] else { return 0.0 };
    let c = &m.cards[v];
    let mut n = 0u32;
    for k in 0..c.len {
        let sl = m.slot(c.shift, p.day as usize, p.period as usize + k);
        n += c.teachers.iter().filter(|&&t| m.t_avoid[t * m.slots + sl]).count() as u32;
        n += c.classes.iter().filter(|&&(ci, _)| m.c_avoid[ci * m.slots + sl]).count() as u32;
    }
    n as f64
}

/// Extra rooms used by a lesson beyond one.
fn rooms_of_lesson(st: &State, lesson: usize) -> f64 {
    let m = st.m;
    let mut used: Vec<i16> = Vec::new();
    for &v in &m.cards_of_lesson[lesson] {
        let Some(p) = st.place[v] else { continue };
        let Some(mi) = m.cards[v].members.iter().position(|x| x.lesson == lesson) else { continue };
        if let Some(k) = pool_ordinal(m, v, mi) {
            let r = p.rooms[k];
            if r >= 0 && !used.contains(&r) {
                used.push(r);
            }
        }
    }
    used.len().saturating_sub(1) as f64
}

/// Every imperfection in the whole timetable.
pub fn measure(st: &State) -> Terms {
    let m = st.m;
    let mut t = Terms::default();
    for shift in 0..m.shift_ids.len() {
        for d in 0..m.nd {
            if m.periods[shift][d] == 0 {
                continue;
            }
            for te in 0..m.teacher_ids.len() {
                t.teacher_gaps += teacher_line(st, te, shift, d);
            }
        }
    }
    for (ci, _) in m.class_names.iter().enumerate() {
        // The class's shift is the shift of any card it appears in.
        let Some(shift) = m.cards.iter().find(|c| c.classes.iter().any(|x| x.0 == ci)).map(|c| c.shift) else { continue };
        for d in 0..m.nd {
            if m.periods[shift][d] == 0 {
                continue;
            }
            t.class_gaps += class_line(st, ci, shift, d);
            t.balance += balance_line(st, ci, shift, d);
        }
    }
    for l in 0..m.lesson_ids.len() {
        t.spread += spread_of(st, l);
        t.rooms += rooms_of_lesson(st, l);
    }
    for v in 0..m.cards.len() {
        t.avoid += avoid_of(st, v);
    }
    t
}

/// The imperfections that can change when `cards` move around `days` (weighted).
fn local(st: &State, cards: &[usize], days: &[usize], w: &Weights) -> f64 {
    let m = st.m;
    let mut total = 0.0;
    let mut tl: Vec<(usize, usize, usize)> = Vec::new();
    let mut cl: Vec<(usize, usize, usize)> = Vec::new();
    let mut lessons: Vec<usize> = Vec::new();
    for &v in cards {
        let c = &m.cards[v];
        for &d in days {
            for &t in &c.teachers {
                if !tl.contains(&(t, c.shift, d)) {
                    tl.push((t, c.shift, d));
                }
            }
            for &(ci, _) in &c.classes {
                if !cl.contains(&(ci, c.shift, d)) {
                    cl.push((ci, c.shift, d));
                }
            }
        }
        for mb in &c.members {
            if !lessons.contains(&mb.lesson) {
                lessons.push(mb.lesson);
            }
        }
        total += w.avoid as f64 * avoid_of(st, v);
    }
    for (t, s, d) in tl {
        total += w.teacher_gaps as f64 * teacher_line(st, t, s, d);
    }
    for (c, s, d) in cl {
        total += w.class_gaps as f64 * class_line(st, c, s, d) + w.balance as f64 * balance_line(st, c, s, d);
    }
    for l in lessons {
        total += w.spread as f64 * spread_of(st, l) + w.room_consistency as f64 * rooms_of_lesson(st, l);
    }
    total
}

#[cfg_attr(not(test), allow(dead_code))]
pub struct ImproveResult {
    pub best: Vec<Option<Pl>>,
    pub moves: u64,
    pub accepted: u64,
    /// Energy of the state the search ended in, tracked move by move.
    pub final_energy: f64,
    /// Energy of `best`: the lowest the search ever reached.
    pub best_energy: f64,
}

/// The best state seen so far. Every improvement is copied at once, so the search can stop at
/// any moment (time up, cancelled) and still hand back the best timetable it ever visited.
struct Best {
    place: Vec<Option<Pl>>,
    energy: f64,
}

impl Best {
    fn offer(&mut self, energy: f64, place: &[Option<Pl>]) {
        if energy < self.energy - 1e-9 {
            self.energy = energy;
            self.place = place.to_vec();
        }
    }
}

/// Simulated annealing over relocations and swaps. The caller's state is left in some visited
/// state; use `best` for the result. Locked sessions never move.
pub fn improve(
    st: &mut State,
    w: &Weights,
    seed: u64,
    deadline: Instant,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f64),
) -> ImproveResult {
    let m = st.m;
    let start = Instant::now();
    let total = deadline.saturating_duration_since(start).as_secs_f64().max(1e-6);
    let mut rng = Rng::new(seed ^ 0xABCDEF);
    let movable: Vec<usize> = (0..m.cards.len()).filter(|&v| st.place[v].is_some() && !st.locked[v] && m.cards[v].broken.is_none()).collect();
    let mut cur = measure(st).weighted(w);
    let mut best = Best { place: st.place.clone(), energy: cur };
    let (mut moves, mut accepted) = (0u64, 0u64);
    let (t_hot, t_cold) = (4.0f64, 0.05f64);
    let mut temp = t_hot;
    if movable.len() < 1 || cur <= 0.0 {
        return ImproveResult { best: best.place, moves, accepted, final_energy: cur, best_energy: best.energy };
    }
    loop {
        if moves % 256 == 0 {
            let frac = (start.elapsed().as_secs_f64() / total).min(1.0);
            temp = t_hot * (t_cold / t_hot).powf(frac);
            if cancel.load(Ordering::Relaxed) || Instant::now() >= deadline {
                break;
            }
            if moves % 8192 == 0 {
                progress(frac);
            }
        }
        moves += 1;
        let v = movable[(rng.next() % movable.len() as u64) as usize];
        let cv = &m.cards[v];
        let old = st.place[v].unwrap();
        let swap = rng.unit() < 0.35;
        let cp = st.journal_len();
        if !swap {
            // Move one session somewhere else.
            let target = cv.domain[(rng.next() % cv.domain.len() as u64) as usize];
            if target.day == old.day && target.period == old.period {
                continue;
            }
            let days = [old.day as usize, target.day as usize];
            let before = local(st, &[v], &days, w);
            st.unplace(v);
            let (bl, mut rooms) = st.conflicts(v, target);
            if !bl.is_empty() || st.limit_problem(v, target).is_some() {
                st.undo(cp);
                continue;
            }
            keep_rooms(st, v, target, &old, &mut rooms);
            let mut pl = Pl::new(target.day, target.period);
            pl.rooms = rooms;
            st.place(v, pl);
            let delta = local(st, &[v], &days, w) - before;
            if accept(delta, temp, &mut rng) {
                cur += delta;
                accepted += 1;
                st.commit();
            } else {
                st.undo(cp);
            }
        } else {
            // Swap two sessions of the same shift.
            let o = movable[(rng.next() % movable.len() as u64) as usize];
            let co = &m.cards[o];
            if o == v || co.shift != cv.shift {
                continue;
            }
            let oldo = st.place[o].unwrap();
            let find = |c: &super::model::CardInfo, p: &Pl| c.domain.iter().copied().find(|x| x.day == p.day && x.period == p.period);
            let (Some(sv), Some(so)) = (find(cv, &oldo), find(co, &old)) else { continue };
            let days = [old.day as usize, oldo.day as usize];
            let before = local(st, &[v, o], &days, w);
            st.unplace(v);
            st.unplace(o);
            let (bl, mut rv) = st.conflicts(v, sv);
            if !bl.is_empty() || st.limit_problem(v, sv).is_some() {
                st.undo(cp);
                continue;
            }
            keep_rooms(st, v, sv, &old, &mut rv);
            let mut pv = Pl::new(sv.day, sv.period);
            pv.rooms = rv;
            st.place(v, pv);
            let (bl, mut ro) = st.conflicts(o, so);
            if !bl.is_empty() || st.limit_problem(o, so).is_some() {
                st.undo(cp);
                continue;
            }
            keep_rooms(st, o, so, &oldo, &mut ro);
            let mut po = Pl::new(so.day, so.period);
            po.rooms = ro;
            st.place(o, po);
            let delta = local(st, &[v, o], &days, w) - before;
            if accept(delta, temp, &mut rng) {
                cur += delta;
                accepted += 1;
                st.commit();
            } else {
                st.undo(cp);
            }
        }
        best.offer(cur, &st.place);
    }
    progress(1.0);
    ImproveResult { best: best.place, moves, accepted, final_energy: cur, best_energy: best.energy }
}

/// A card that keeps using its old rooms when they are free in the new slot.
fn keep_rooms(st: &State, v: usize, to: Start, old: &Pl, rooms: &mut [i16; super::model::MAXR]) {
    let c = &st.m.cards[v];
    for k in 0..c.pools.len() {
        let r = old.rooms[k];
        if r >= 0 && !rooms[..c.pools.len()].contains(&r) && st.room_free(r as usize, c.shift, to, c.len) {
            rooms[k] = r;
        }
    }
}

fn accept(delta: f64, temp: f64, rng: &mut Rng) -> bool {
    delta <= 0.0 || rng.unit() < (-delta / temp).exp()
}

#[cfg(test)]
mod best_tests {
    use super::*;

    #[test]
    fn best_keeps_the_lowest_state_even_if_the_search_then_wanders_off() {
        let at = |period: u8| vec![Some(Pl::new(0, period))];
        let mut b = Best { place: at(1), energy: 10.0 };
        b.offer(5.0, &at(2)); // found just before the time ran out ...
        b.offer(8.0, &at(3)); // ... and the search then moved to something worse
        b.offer(5.0, &at(4)); // an equal state is not a new best
        assert_eq!(b.energy, 5.0);
        assert_eq!(b.place[0].unwrap().period, 2);
    }
}
