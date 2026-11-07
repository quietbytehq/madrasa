//! The search: difficulty ordering, direct placement, depth-limited ejection chains and
//! conflict-weighted kicks (blueprint section 7, phases 1 and 2).
use super::model::{distinct_assignment, Model, Start, MAXR};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pl {
    pub day: u8,
    pub period: u8,
    /// One room per room pool of the card (in order); -1 = none.
    pub rooms: [i16; MAXR],
}

impl Pl {
    pub fn new(day: u8, period: u8) -> Self {
        Pl { day, period, rooms: [-1; MAXR] }
    }
}

enum Op {
    Placed(usize),
    Unplaced(usize, Pl),
}

pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1)
    }
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545F4914F6CDD1D)
    }
    pub fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Why a start time is unusable right now.
#[derive(Debug, Clone, PartialEq)]
pub enum Blocker {
    Teacher(usize),
    Class(usize),
    Room(usize),
    SameDay,
    /// Too many sessions of this subject for this class that day.
    SameSubject(usize, usize),
}

/// A bendable rule that a placement would break (used by relaxation mode).
#[derive(Debug, Clone, PartialEq)]
pub enum Violation {
    SameDay,
    /// class, subject, sessions that day, limit
    Subject(usize, usize, u32, u32),
    /// teacher, lessons in a row, limit
    Consecutive(usize, u32, u32),
    /// teacher, lessons that day, limit
    PerDay(usize, u32, u32),
}

/// Rules that relaxation mode may switch off (everything else is always enforced).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rules {
    pub same_day: bool,
    pub subject_day: bool,
    pub consecutive: bool,
    pub per_day: bool,
}

impl Rules {
    pub const STRICT: Rules = Rules { same_day: true, subject_day: true, consecutive: true, per_day: true };
    pub const RELAXED: Rules = Rules { same_day: false, subject_day: false, consecutive: false, per_day: false };
}

pub struct State<'a> {
    pub m: &'a Model,
    pub place: Vec<Option<Pl>>,
    pub locked: Vec<bool>,
    pub weight: Vec<u32>,
    pub rules: Rules,
    pub(super) t_occ: Vec<u32>,
    pub(super) r_occ: Vec<u32>,
    pub(super) c_mask: Vec<u64>,
    pub(super) c_cards: Vec<Vec<u32>>,
    pub(super) t_day_load: Vec<u32>,
    pub(super) t_week_load: Vec<u32>,
    pub(super) t_days_used: Vec<u32>,
    pub(super) c_day_load: Vec<u32>,
    /// Cards with a member of (class, subject) on a day: [(class * nsub + subject) * nd + day]
    cs_day: Vec<Vec<u32>>,
    journal: Vec<Op>,
    pub placed: usize,
    /// Lesson sessions placed (a card shared by linked lessons counts once per lesson).
    pub placed_sessions: usize,
}

impl<'a> State<'a> {
    pub fn new(m: &'a Model) -> Self {
        let nt = m.teacher_ids.len();
        let nc = m.class_names.len();
        let nr = m.room_ids.len();
        State {
            m,
            place: vec![None; m.cards.len()],
            locked: vec![false; m.cards.len()],
            weight: vec![0; m.cards.len()],
            rules: Rules::STRICT,
            t_occ: vec![0; nt * m.slots],
            r_occ: vec![0; nr * m.slots],
            c_mask: vec![0; nc * m.slots],
            c_cards: vec![Vec::new(); nc * m.slots],
            t_day_load: vec![0; nt * m.nd],
            t_week_load: vec![0; nt],
            t_days_used: vec![0; nt],
            c_day_load: vec![0; nc * m.nd],
            cs_day: vec![Vec::new(); nc * m.nsub.max(1) * m.nd],
            journal: Vec::new(),
            placed: 0,
            placed_sessions: 0,
        }
    }

    fn card_mask(&self, card: usize, class: usize) -> u64 {
        self.m.cards[card].classes.iter().find(|(c, _)| *c == class).map_or(0, |x| x.1)
    }

    fn cs_keys(&self, v: usize, day: usize) -> Vec<usize> {
        let m = self.m;
        let mut keys = Vec::new();
        for mb in &m.cards[v].members {
            let Some(sub) = mb.subject else { continue };
            for &(ci, _) in &mb.classes {
                let k = (ci * m.nsub + sub) * m.nd + day;
                if !keys.contains(&k) {
                    keys.push(k);
                }
            }
        }
        keys
    }

    fn place_raw(&mut self, v: usize, pl: Pl) {
        let m = self.m;
        let c = &m.cards[v];
        let base = m.slot(c.shift, pl.day as usize, pl.period as usize);
        for k in 0..c.len {
            for &t in &c.teachers {
                self.t_occ[t * m.slots + base + k] = v as u32 + 1;
            }
            for &r in pl.rooms.iter().filter(|r| **r >= 0) {
                self.r_occ[r as usize * m.slots + base + k] = v as u32 + 1;
            }
            for &(ci, mask) in &c.classes {
                self.c_mask[ci * m.slots + base + k] |= mask;
                self.c_cards[ci * m.slots + base + k].push(v as u32);
            }
        }
        for &t in &c.teachers {
            let i = t * m.nd + pl.day as usize;
            if self.t_day_load[i] == 0 {
                self.t_days_used[t] += 1;
            }
            self.t_day_load[i] += c.len as u32;
            self.t_week_load[t] += c.len as u32;
        }
        for &(ci, _) in &c.classes {
            self.c_day_load[ci * m.nd + pl.day as usize] += c.len as u32;
        }
        for k in self.cs_keys(v, pl.day as usize) {
            self.cs_day[k].push(v as u32);
        }
        self.place[v] = Some(pl);
        self.placed += 1;
        self.placed_sessions += c.members.len();
    }

    fn unplace_raw(&mut self, v: usize) -> Pl {
        let m = self.m;
        let pl = self.place[v].take().expect("card is placed");
        let c = &m.cards[v];
        let base = m.slot(c.shift, pl.day as usize, pl.period as usize);
        for k in 0..c.len {
            for &t in &c.teachers {
                self.t_occ[t * m.slots + base + k] = 0;
            }
            for &r in pl.rooms.iter().filter(|r| **r >= 0) {
                self.r_occ[r as usize * m.slots + base + k] = 0;
            }
            for &(ci, mask) in &c.classes {
                let i = ci * m.slots + base + k;
                self.c_mask[i] &= !mask;
                self.c_cards[i].retain(|&o| o != v as u32);
            }
        }
        for &t in &c.teachers {
            let i = t * m.nd + pl.day as usize;
            self.t_day_load[i] -= c.len as u32;
            self.t_week_load[t] -= c.len as u32;
            if self.t_day_load[i] == 0 {
                self.t_days_used[t] -= 1;
            }
        }
        for &(ci, _) in &c.classes {
            self.c_day_load[ci * m.nd + pl.day as usize] -= c.len as u32;
        }
        for k in self.cs_keys(v, pl.day as usize) {
            self.cs_day[k].retain(|&o| o != v as u32);
        }
        self.placed -= 1;
        self.placed_sessions -= c.members.len();
        pl
    }

    pub fn place(&mut self, v: usize, pl: Pl) {
        self.place_raw(v, pl);
        self.journal.push(Op::Placed(v));
    }

    pub fn unplace(&mut self, v: usize) {
        let pl = self.unplace_raw(v);
        self.journal.push(Op::Unplaced(v, pl));
    }

    pub(super) fn undo(&mut self, to: usize) {
        while self.journal.len() > to {
            match self.journal.pop().unwrap() {
                Op::Placed(v) => {
                    self.unplace_raw(v);
                }
                Op::Unplaced(v, pl) => self.place_raw(v, pl),
            }
        }
    }

    pub(super) fn journal_len(&self) -> usize {
        self.journal.len()
    }

    pub fn commit(&mut self) {
        self.journal.clear();
    }

    /// The students of `class` that card `o` teaches this subject to (0 = none of them).
    fn subject_mask(&self, o: u32, class: usize, subject: usize) -> u64 {
        self.m.cards[o as usize].members.iter().filter(|mb| mb.subject == Some(subject)).flat_map(|mb| mb.classes.iter()).filter(|&&(c, _)| c == class).fold(0, |acc, &(_, mk)| acc | mk)
    }

    /// Sessions of (class, subject) on a day that share students with `mask`, with the students each one has.
    fn same_subject(&self, v: usize, class: usize, subject: usize, day: usize, mask: u64) -> Vec<(u32, u64)> {
        let key = (class * self.m.nsub + subject) * self.m.nd + day;
        self.cs_day[key]
            .iter()
            .copied()
            .filter(|&o| o as usize != v)
            .map(|o| (o, self.subject_mask(o, class, subject)))
            .filter(|x| x.1 & mask != 0)
            .collect()
    }

    /// A room for each pool, all different. Prefers rooms that are free; otherwise the ones with
    /// the fewest users (those cards become blockers).
    fn assign_rooms(&self, v: usize, base: usize, out: &mut Vec<(u32, Blocker)>) -> [i16; MAXR] {
        let m = self.m;
        let c = &m.cards[v];
        let mut rooms = [-1i16; MAXR];
        if c.pools.is_empty() {
            return rooms;
        }
        let closed = |r: usize| (0..c.len).any(|k| m.r_forb[r * m.slots + base + k]);
        let users = |r: usize| -> Vec<u32> {
            let mut u = Vec::new();
            for k in 0..c.len {
                let o = self.r_occ[r * m.slots + base + k];
                if o != 0 && o - 1 != v as u32 && !u.contains(&(o - 1)) {
                    u.push(o - 1);
                }
            }
            u
        };
        if let Some(a) = distinct_assignment(&c.pools, &|r| !closed(r) && users(r).is_empty()) {
            for (i, r) in a.into_iter().enumerate() {
                rooms[i] = r as i16;
            }
            return rooms;
        }
        // Nothing fully free: take the least-used rooms, trying the least-used first.
        fn go(i: usize, pools: &[Vec<usize>], order: &dyn Fn(&[usize]) -> Vec<usize>, used: &mut Vec<usize>) -> bool {
            if i == pools.len() {
                return true;
            }
            for r in order(&pools[i]) {
                if !used.contains(&r) {
                    used.push(r);
                    if go(i + 1, pools, order, used) {
                        return true;
                    }
                    used.pop();
                }
            }
            false
        }
        let order = |pool: &[usize]| {
            let mut v: Vec<usize> = pool.iter().copied().filter(|&r| !closed(r)).collect();
            v.sort_by_key(|&r| users(r).len());
            v
        };
        let mut used = Vec::new();
        if go(0, &c.pools, &order, &mut used) {
            for (i, &r) in used.iter().enumerate() {
                rooms[i] = r as i16;
                for o in users(r) {
                    if !out.iter().any(|(x, _)| *x == o) {
                        out.push((o, Blocker::Room(r)));
                    }
                }
            }
        }
        rooms
    }

    /// Cards (and the rooms to use) that stand in the way of starting `v` at `st`.
    /// Empty result = the slot is free of collisions (limits are checked separately).
    pub fn conflicts(&self, v: usize, st: Start) -> (Vec<(u32, Blocker)>, [i16; MAXR]) {
        let m = self.m;
        let c = &m.cards[v];
        let base = m.slot(c.shift, st.day as usize, st.period as usize);
        let mut out: Vec<(u32, Blocker)> = Vec::new();
        fn add(out: &mut Vec<(u32, Blocker)>, o: u32, b: Blocker) {
            if !out.iter().any(|(x, _)| *x == o) {
                out.push((o, b));
            }
        }
        for k in 0..c.len {
            for &t in &c.teachers {
                let o = self.t_occ[t * m.slots + base + k];
                if o != 0 {
                    add(&mut out, o - 1, Blocker::Teacher(t));
                }
            }
            for &(ci, mask) in &c.classes {
                let i = ci * m.slots + base + k;
                if self.c_mask[i] & mask != 0 {
                    for &o in &self.c_cards[i] {
                        if self.card_mask(o as usize, ci) & mask != 0 {
                            add(&mut out, o, Blocker::Class(ci));
                        }
                    }
                }
            }
        }
        if !c.same_day_ok && self.rules.same_day {
            for mb in &c.members {
                for &o in &m.cards_of_lesson[mb.lesson] {
                    if o != v {
                        if let Some(p) = self.place[o] {
                            if p.day == st.day {
                                add(&mut out, o as u32, Blocker::SameDay);
                            }
                        }
                    }
                }
            }
        }
        if self.rules.subject_day {
            for mb in &c.members {
                let Some(sub) = mb.subject else { continue };
                for &(ci, mask) in &mb.classes {
                    let others = self.same_subject(v, ci, sub, st.day as usize, mask);
                    if others.is_empty() {
                        continue;
                    }
                    // The limit belongs to each group of students: every one of them may have only so many that day.
                    let mut bits = mask;
                    while bits != 0 {
                        let b = bits.trailing_zeros() as usize;
                        bits &= bits - 1;
                        let on: Vec<u32> = others.iter().filter(|(_, mk)| mk >> b & 1 == 1).map(|(o, _)| *o).collect();
                        if on.len() as u32 + 1 > m.subj_limit[(ci * m.nsub + sub) * 64 + b] {
                            for o in on {
                                add(&mut out, o, Blocker::SameSubject(ci, sub));
                            }
                        }
                    }
                }
            }
        }
        let rooms = self.assign_rooms(v, base, &mut out);
        out.retain(|(o, _)| *o as usize != v);
        (out, rooms)
    }

    /// Bendable rules that starting `v` at `st` would break, whatever `self.rules` says.
    pub fn violations(&self, v: usize, st: Start) -> Vec<Violation> {
        let m = self.m;
        let c = &m.cards[v];
        let base = m.slot(c.shift, st.day as usize, st.period as usize);
        let mut out = Vec::new();
        if !c.same_day_ok
            && c.members.iter().any(|mb| {
                m.cards_of_lesson[mb.lesson].iter().any(|&o| o != v && self.place[o].map_or(false, |p| p.day == st.day))
            })
        {
            out.push(Violation::SameDay);
        }
        for mb in &c.members {
            let Some(sub) = mb.subject else { continue };
            for &(ci, mask) in &mb.classes {
                let others = self.same_subject(v, ci, sub, st.day as usize, mask);
                let mut bits = mask;
                while bits != 0 {
                    let b = bits.trailing_zeros() as usize;
                    bits &= bits - 1;
                    let n = others.iter().filter(|(_, mk)| mk >> b & 1 == 1).count() as u32 + 1;
                    let limit = m.subj_limit[(ci * m.nsub + sub) * 64 + b];
                    if n > limit {
                        out.push(Violation::Subject(ci, sub, n, limit));
                    }
                }
            }
        }
        for &t in &c.teachers {
            let load = self.t_day_load[t * m.nd + st.day as usize];
            if let Some(max) = m.max_per_day[t] {
                if load - if self.place[v].map_or(false, |p| p.day == st.day) { c.len as u32 } else { 0 } + c.len as u32 > max {
                    out.push(Violation::PerDay(t, load + c.len as u32, max));
                }
            }
            if let Some(max) = m.max_consecutive[t] {
                let p = st.period as usize;
                let row = t * m.slots + base - p;
                let mut left = 0;
                while left < p && self.t_occ[row + p - 1 - left] != 0 && self.t_occ[row + p - 1 - left] != v as u32 + 1 {
                    left += 1;
                }
                let mut right = 0;
                while p + c.len + right < m.pm && self.t_occ[row + p + c.len + right] != 0 && self.t_occ[row + p + c.len + right] != v as u32 + 1 {
                    right += 1;
                }
                let run = (left + c.len + right) as u32;
                if run > max {
                    out.push(Violation::Consecutive(t, run, max));
                }
            }
        }
        out
    }

    /// Is room `r` open and unoccupied for a card of `len` periods starting at `st`?
    pub fn room_free(&self, r: usize, shift: usize, st: Start, len: usize) -> bool {
        let base = self.m.slot(shift, st.day as usize, st.period as usize);
        (0..len).all(|k| !self.m.r_forb[r * self.m.slots + base + k] && self.r_occ[r * self.m.slots + base + k] == 0)
    }

    /// Teacher limits (hard): per-day maximum, weekly maximum, working days, and lessons in a row.
    pub fn limit_problem(&self, v: usize, st: Start) -> Option<usize> {
        let m = self.m;
        let c = &m.cards[v];
        let base = m.slot(c.shift, st.day as usize, st.period as usize);
        for &t in &c.teachers {
            let load = self.t_day_load[t * m.nd + st.day as usize];
            if self.rules.per_day {
                if let Some(max) = m.max_per_day[t] {
                    if load + c.len as u32 > max {
                        return Some(t);
                    }
                }
            }
            if let Some(max) = m.max_per_week[t] {
                if self.t_week_load[t] + c.len as u32 > max {
                    return Some(t);
                }
            }
            if let Some(max) = m.max_days[t] {
                if load == 0 && self.t_days_used[t] >= max {
                    return Some(t);
                }
            }
            if self.rules.consecutive {
                if let Some(max) = m.max_consecutive[t] {
                    let p = st.period as usize;
                    let row = t * m.slots + base - p;
                    let mut left = 0;
                    while left < p && self.t_occ[row + p - 1 - left] != 0 {
                        left += 1;
                    }
                    let mut right = 0;
                    while p + c.len + right < m.pm && self.t_occ[row + p + c.len + right] != 0 {
                        right += 1;
                    }
                    if (left + c.len + right) as u32 > max {
                        return Some(t);
                    }
                }
            }
        }
        None
    }

    /// Lower is nicer. Cheap stand-ins for the soft rules while placing (the real
    /// optimisation happens afterwards).
    fn score(&self, v: usize, st: Start, rng: &mut Rng) -> f64 {
        let m = self.m;
        let c = &m.cards[v];
        let base = m.slot(c.shift, st.day as usize, st.period as usize);
        let p = st.period as usize;
        let mut s = st.avoid as f64 * 10.0 + rng.unit();
        if let Some(&(ci, _)) = c.classes.first() {
            let day_load = self.c_day_load[ci * m.nd + st.day as usize];
            s += day_load as f64 * 1.5;
            let row = ci * m.slots + base - p;
            let before = p > 0 && self.c_mask[row + p - 1] != 0;
            let after = p + c.len < m.pm && self.c_mask[row + p + c.len] != 0;
            if day_load == 0 {
                s += p as f64 * 2.0;
            } else if !before && !after {
                s += 6.0;
            }
        }
        for &t in &c.teachers {
            let load = self.t_day_load[t * m.nd + st.day as usize];
            let row = t * m.slots + base - p;
            let before = p > 0 && self.t_occ[row + p - 1] != 0;
            let after = p + c.len < m.pm && self.t_occ[row + p + c.len] != 0;
            if load > 0 && !before && !after {
                s += 3.0;
            }
        }
        s
    }
}

pub struct Config {
    pub max_depth: usize,
    /// Give up an attempt after this long without placing one more card.
    pub stagnation: Duration,
    pub deadline: Instant,
    pub seed: u64,
    /// Cap on search steps (and half of it as patience). `None` = scaled to the number of cards.
    pub limit: Option<u64>,
    /// Search nodes one ejection chain may use. `None` = the usual 200.
    pub node_budget: Option<i64>,
}

pub struct Engine<'a> {
    pub s: State<'a>,
    cfg_depth: usize,
    rng: Rng,
    budget: i64,
    pub iterations: u64,
}

pub struct Outcome {
    pub best: Vec<Option<Pl>>,
    pub cancelled: bool,
    pub timed_out: bool,
}

impl<'a> Engine<'a> {
    pub fn new(m: &'a Model, cfg: &Config) -> Self {
        Engine { s: State::new(m), cfg_depth: cfg.max_depth, rng: Rng::new(cfg.seed), budget: 0, iterations: 0 }
    }

    /// Puts `v` in the nicest collision-free start, if there is one.
    fn try_direct(&mut self, v: usize) -> bool {
        let m = self.s.m;
        let mut best: Option<(f64, Start, [i16; MAXR])> = None;
        for &st in &m.cards[v].domain {
            let (bl, room) = self.s.conflicts(v, st);
            if !bl.is_empty() || self.s.limit_problem(v, st).is_some() {
                continue;
            }
            let mut sc = self.s.score(v, st, &mut self.rng);
            if self.s.rules != Rules::STRICT {
                sc += 1000.0 * self.s.violations(v, st).len() as f64;
            }
            if best.map_or(true, |(b, _, _)| sc < b) {
                best = Some((sc, st, room));
            }
        }
        match best {
            Some((_, st, room)) => {
                self.s.place(v, Pl { day: st.day, period: st.period, rooms: room });
                true
            }
            None => false,
        }
    }

    /// Place `v`, moving other cards out of the way if needed (they must find new homes).
    fn eject(&mut self, v: usize, depth: usize, chain: &mut Vec<usize>) -> bool {
        if self.try_direct(v) {
            return true;
        }
        if depth == 0 || self.budget <= 0 {
            return false;
        }
        self.budget -= 1;
        let m = self.s.m;
        let mut cands: Vec<(f64, Start, Vec<u32>)> = Vec::new();
        for &st in &m.cards[v].domain {
            let (bl, _) = self.s.conflicts(v, st);
            if bl.is_empty() || bl.len() > 4 {
                continue;
            }
            if bl.iter().any(|(o, _)| self.s.locked[*o as usize] || chain.contains(&(*o as usize))) {
                continue;
            }
            let cost: f64 = bl.iter().map(|(o, _)| 1.0 + self.s.weight[*o as usize] as f64).sum::<f64>() + self.rng.unit();
            cands.push((cost, st, bl.into_iter().map(|(o, _)| o).collect()));
        }
        cands.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let width = if depth + 2 >= self.cfg_depth { 5 } else { 3 };
        cands.truncate(width);
        for (_, st, bl) in cands {
            let cp = self.s.journal_len();
            for &o in &bl {
                self.s.unplace(o as usize);
            }
            let (left, room) = self.s.conflicts(v, st);
            if left.is_empty() && self.s.limit_problem(v, st).is_none() {
                self.s.place(v, Pl { day: st.day, period: st.period, rooms: room });
                chain.push(v);
                let ok = bl.iter().all(|&o| self.eject(o as usize, depth - 1, chain));
                chain.pop();
                if ok {
                    return true;
                }
            }
            self.s.undo(cp);
            if self.budget <= 0 {
                break;
            }
        }
        false
    }

    /// Last resort: put `v` where the fewest cards are in its way and push those back
    /// into the queue. Returns the evicted cards, or None if no start can ever work.
    pub(super) fn kick(&mut self, v: usize) -> Option<Vec<usize>> {
        let m = self.s.m;
        let mut cands: Vec<(f64, Start, Vec<u32>)> = Vec::new();
        for &st in &m.cards[v].domain {
            let (bl, _) = self.s.conflicts(v, st);
            if bl.iter().any(|(o, _)| self.s.locked[*o as usize]) {
                continue;
            }
            let mut cost: f64 = bl.iter().map(|(o, _)| 1.0 + self.s.weight[*o as usize] as f64).sum::<f64>() + self.rng.unit() * 1.5;
            if self.s.rules != Rules::STRICT {
                cost += 100.0 * self.s.violations(v, st).len() as f64;
            }
            cands.push((cost, st, bl.into_iter().map(|(o, _)| o).collect()));
        }
        cands.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        for (_, st, bl) in cands {
            let cp = self.s.journal_len();
            for &o in &bl {
                self.s.unplace(o as usize);
            }
            let (left, room) = self.s.conflicts(v, st);
            if left.is_empty() && self.s.limit_problem(v, st).is_none() {
                self.s.place(v, Pl { day: st.day, period: st.period, rooms: room });
                for &o in &bl {
                    self.s.weight[o as usize] += 1;
                }
                return Some(bl.into_iter().map(|o| o as usize).collect());
            }
            self.s.undo(cp);
        }
        None
    }

    /// Place every card in `queue`. Cards already in the state (locked ones) are never moved.
    pub fn run(
        &mut self,
        queue: Vec<usize>,
        cfg: &Config,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(usize),
    ) -> Outcome {  // progress gets lesson sessions placed
        let m = self.s.m;
        let total_cards = m.cards.len();
        let mut best_n = self.s.placed;
        let mut best_sessions = self.s.placed_sessions;
        let mut best = self.s.place.clone();
        // Fresh cards go in hardest-first. Cards thrown out again wait in `retry`,
        // where the ones that keep causing trouble (highest weight) come first.
        let mut fresh = queue;
        fresh.sort_by(|&a, &b| m.cards[a].difficulty.partial_cmp(&m.cards[b].difficulty).unwrap());
        let mut retry: Vec<usize> = Vec::new();
        let max_iter = cfg.limit.unwrap_or(400 * total_cards as u64 + 2000);
        let patience = cfg.limit.map_or(6 * total_cards as u64 + 300, |l| l / 2 + 10);
        let mut since_best = 0u64;
        let mut last_gain = Instant::now();
        let (mut cancelled, mut timed_out) = (false, false);
        let mut last_report = Instant::now();

        while !fresh.is_empty() || !retry.is_empty() {
            if cancel.load(Ordering::Relaxed) {
                cancelled = true;
                break;
            }
            if Instant::now() >= cfg.deadline || self.iterations >= max_iter {
                timed_out = true;
                break;
            }
            if since_best > patience || last_gain.elapsed() > cfg.stagnation {
                break; // going round in circles: stop and explain what is left
            }
            self.iterations += 1;
            since_best += 1;
            let v = match fresh.pop() {
                Some(v) => v,
                None => {
                    let (qi, _) = retry
                        .iter()
                        .enumerate()
                        .map(|(i, &c)| (i, self.s.weight[c] as f64 * 25.0 + m.cards[c].difficulty))
                        .fold((0, f64::MIN), |a, b| if b.1 > a.1 { b } else { a });
                    retry.swap_remove(qi)
                }
            };

            if m.cards[v].broken.is_some() || m.cards[v].domain.is_empty() {
                continue; // can never be placed; it will be explained at the end
            }
            self.budget = cfg.node_budget.unwrap_or(200);
            let mut chain = Vec::new();
            if self.eject(v, self.cfg_depth, &mut chain) {
                self.s.commit();
            } else {
                self.s.commit();
                match self.kick(v) {
                    Some(evicted) => {
                        self.s.commit();
                        retry.extend(evicted);
                    }
                    None => {} // nothing can be moved for it (everything in the way is locked)
                }
            }
            if self.s.placed > best_n {
                best_n = self.s.placed;
                best_sessions = self.s.placed_sessions;
                best = self.s.place.clone();
                since_best = 0;
                last_gain = Instant::now();
            }
            if last_report.elapsed() > Duration::from_millis(150) {
                progress(best_sessions);
                last_report = Instant::now();
            }
        }
        progress(best_sessions);
        Outcome { best, cancelled, timed_out }
    }
}
