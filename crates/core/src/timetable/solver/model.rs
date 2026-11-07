//! Turns the school setup into the compact, index-based model the solver works on.
//! Everything is a small integer; availability is plain arrays; class groups are bitmasks.
//! A "card" is one session that must be placed. Lessons linked as "same time" share a card.
use crate::timetable::master::{Class, TimeOffCell};
use crate::timetable::model::SchoolSetup;
use std::collections::HashMap;

/// Most rooms one card can need at once (one per linked lesson that has a room pool).
pub const MAXR: usize = 8;

/// A possible start time for a card.
#[derive(Debug, Clone, Copy)]
pub struct Start {
    pub day: u8,
    /// 0-based period index.
    pub period: u8,
    /// How many "avoid if possible" cells this start touches (for choosing nicer slots).
    pub avoid: u8,
}

/// One lesson's part of a card.
#[derive(Debug, Clone)]
pub struct Member {
    pub lesson: usize,
    /// 1-based session number within the lesson.
    pub number: u32,
    pub subject: Option<usize>,
    /// (class index, atomic-group bitmask)
    pub classes: Vec<(usize, u64)>,
    pub pool: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct CardInfo {
    pub members: Vec<Member>,
    pub len: usize,
    pub shift: usize,
    pub teachers: Vec<usize>,
    /// All classes of all members; masks of the same class are combined.
    pub classes: Vec<(usize, u64)>,
    /// The room pools of the members that need a room, in member order.
    pub pools: Vec<Vec<usize>>,
    pub domain: Vec<Start>,
    /// True when the lesson has more sessions than usable days, so two may share a day.
    pub same_day_ok: bool,
    pub difficulty: f64,
    /// Set when the card can never be placed (bad data); explains why.
    pub broken: Option<String>,
}

pub struct Model {
    pub nd: usize,
    pub pm: usize,
    /// Slots in one shift = nd * pm; total = shifts * that.
    pub slots: usize,
    pub shift_ids: Vec<String>,
    pub teacher_ids: Vec<String>,
    pub teacher_names: Vec<String>,
    pub class_names: Vec<String>,
    pub room_ids: Vec<String>,
    pub room_names: Vec<String>,
    pub subject_names: Vec<String>,
    pub day_names: Vec<String>,
    pub lesson_ids: Vec<String>,
    /// Lesson id -> position in `lesson_ids` (so a session can be found without scanning every lesson).
    pub lesson_index: HashMap<String, usize>,
    pub lesson_titles: Vec<String>,
    pub cards: Vec<CardInfo>,
    /// For each lesson, the card holding session n (index n-1).
    pub cards_of_lesson: Vec<Vec<usize>>,
    /// Effective hard limits per teacher (their own, or the school default).
    pub max_per_day: Vec<Option<u32>>,
    pub max_per_week: Vec<Option<u32>>,
    pub max_days: Vec<Option<u32>>,
    pub max_consecutive: Vec<Option<u32>>,
    /// Most sessions of one subject a day for each group of students (atomic bucket) of a class:
    /// [(class * nsub + subject) * 64 + bucket]. Raised above the school's setting only as far as
    /// the number of sessions that bucket has forces it to be.
    pub subj_limit: Vec<u32>,
    /// Forbidden (state 2) and "avoid" (state 1) grids: entity * slots + slot.
    pub t_forb: Vec<bool>,
    pub c_forb: Vec<bool>,
    pub r_forb: Vec<bool>,
    pub t_avoid: Vec<bool>,
    pub c_avoid: Vec<bool>,
    /// periods per (shift, day)
    pub periods: Vec<Vec<usize>>,
    /// cross_ok[shift][k]: a double/triple may include the boundary between period k-1 and k (k >= 1)
    pub cross_ok: Vec<Vec<bool>>,
    /// Shortest time-off-free start list is used for explanations only.
    pub nsub: usize,
}

impl Model {
    #[inline]
    pub fn slot(&self, shift: usize, day: usize, period: usize) -> usize {
        (shift * self.nd + day) * self.pm + period
    }

    /// "Maths for 5A" or "Maths for 5A + ICT for 5B" for linked lessons.
    pub fn card_title(&self, v: usize) -> String {
        let c = &self.cards[v];
        c.members.iter().map(|x| self.lesson_titles[x.lesson].clone()).collect::<Vec<_>>().join(" + ")
    }
}

/// Splits a class into atomic student buckets (the product of all its divisions) and
/// returns (number of buckets, mask of every group by group id).
pub fn class_masks(c: &Class) -> Option<(u32, Vec<(String, u64)>)> {
    let mut atoms: u64 = 1;
    for d in &c.divisions {
        atoms = atoms.checked_mul(d.groups.len().max(1) as u64)?;
        if atoms > 64 {
            return None;
        }
    }
    let atoms = atoms as u32;
    let mut out = Vec::new();
    let mut stride = 1u32;
    for d in &c.divisions {
        let n = d.groups.len().max(1) as u32;
        for (gi, g) in d.groups.iter().enumerate() {
            let mut m = 0u64;
            for a in 0..atoms {
                if (a / stride) % n == gi as u32 {
                    m |= 1 << a;
                }
            }
            out.push((g.id.clone(), m));
        }
        stride *= n;
    }
    Some((atoms, out))
}

pub fn whole_mask(atoms: u32) -> u64 {
    if atoms >= 64 { u64::MAX } else { (1u64 << atoms) - 1 }
}

#[allow(clippy::too_many_arguments)]
fn fill(grid: &mut [bool], idx: usize, model_slots: usize, shift_idx: &[(String, usize)], cells: &[TimeOffCell], want: u8, nd: usize, pm: usize) {
    for c in cells.iter().filter(|c| c.state == want) {
        let Some((_, si)) = shift_idx.iter().find(|(id, _)| *id == c.shift) else { continue };
        let (d, p) = (c.day as usize, (c.period as usize).wrapping_sub(1));
        if d < nd && p < pm {
            grid[idx * model_slots + (si * nd + d) * pm + p] = true;
        }
    }
}

/// Can every pool get its own distinct room, using only rooms `ok` accepts?
pub fn distinct_assignment(pools: &[Vec<usize>], ok: &dyn Fn(usize) -> bool) -> Option<Vec<usize>> {
    fn go(pools: &[Vec<usize>], ok: &dyn Fn(usize) -> bool, used: &mut Vec<usize>) -> bool {
        let i = used.len();
        if i == pools.len() {
            return true;
        }
        for &r in &pools[i] {
            if ok(r) && !used.contains(&r) {
                used.push(r);
                if go(pools, ok, used) {
                    return true;
                }
                used.pop();
            }
        }
        false
    }
    let mut used = Vec::new();
    go(pools, ok, &mut used).then_some(used)
}

/// What one lesson needs, before it is turned into cards.
struct Base {
    title: String,
    broken: Option<String>,
    teachers: Vec<usize>,
    classes: Vec<(usize, u64)>,
    pool: Vec<usize>,
    shift: Option<usize>,
    len: usize,
    cpw: u32,
    subject: Option<usize>,
}

/// For every lesson: on how many days can it really happen? The checker asks the solver's own
/// model, so both always agree (breaks, rooms, session length, time off and part-time
/// teachers all count).
pub fn usable_days_by_lesson(s: &SchoolSetup) -> HashMap<String, u32> {
    let m = compile(s);
    s.lessons
        .iter()
        .enumerate()
        .filter_map(|(i, l)| m.cards_of_lesson[i].first().map(|&c| (l.id.clone(), usable_days(&m, c) as u32)))
        .collect()
}

fn usable_days(model: &Model, v: usize) -> usize {
    let mut d: Vec<u8> = model.cards[v].domain.iter().map(|x| x.day).collect();
    d.dedup();
    let mut u = d.len();
    for &t in &model.cards[v].teachers {
        if let Some(mx) = model.max_days[t] {
            u = u.min(mx as usize);
        }
    }
    u
}

pub fn compile(s: &SchoolSetup) -> Model {
    let m = &s.master;
    let nd = s.days.len();
    let pm = s.shifts.iter().flat_map(|sh| sh.days.iter().map(|d| d.len())).max().unwrap_or(0).max(1);
    let slots = s.shifts.len() * nd * pm;
    let shift_idx: Vec<(String, usize)> = s.shifts.iter().enumerate().map(|(i, sh)| (sh.id.clone(), i)).collect();

    let periods: Vec<Vec<usize>> = s
        .shifts
        .iter()
        .map(|sh| (0..nd).map(|d| if s.days[d].working && sh.enabled { sh.days.get(d).map_or(0, |x| x.len()) } else { 0 }).collect())
        .collect();
    let cross_ok: Vec<Vec<bool>> = s
        .shifts
        .iter()
        .map(|sh| (0..=pm).map(|k| !sh.breaks.iter().any(|b| b.cannot_cross && b.after_period as usize == k)).collect())
        .collect();

    let (nt, nc, nr, nsub) = (m.teachers.len(), m.classes.len(), m.rooms.len(), m.subjects.len());
    let mut t_forb = vec![false; nt * slots];
    let mut t_avoid = vec![false; nt * slots];
    let mut c_forb = vec![false; nc * slots];
    let mut c_avoid = vec![false; nc * slots];
    let mut r_forb = vec![false; nr * slots];
    for (i, t) in m.teachers.iter().enumerate() {
        fill(&mut t_forb, i, slots, &shift_idx, &t.time_off, 2, nd, pm);
        fill(&mut t_avoid, i, slots, &shift_idx, &t.time_off, 1, nd, pm);
    }
    for (i, c) in m.classes.iter().enumerate() {
        fill(&mut c_forb, i, slots, &shift_idx, &c.time_off, 2, nd, pm);
        fill(&mut c_avoid, i, slots, &shift_idx, &c.time_off, 1, nd, pm);
    }
    for (i, r) in m.rooms.iter().enumerate() {
        fill(&mut r_forb, i, slots, &shift_idx, &r.time_off, 2, nd, pm);
    }

    let masks: Vec<Option<(u32, Vec<(String, u64)>)>> = m.classes.iter().map(class_masks).collect();
    let class_names: Vec<String> = m.classes.iter().map(|x| x.name.trim().to_string()).collect();
    let teacher_names: Vec<String> = m.teachers.iter().map(|x| x.name.trim().to_string()).collect();

    // ---- one Base per lesson ----
    let mut bases: Vec<Base> = Vec::new();
    for l in &s.lessons {
        let title = crate::timetable::links::lesson_title(s, &l.id);
        let mut broken: Option<String> = None;
        if !(1..=3).contains(&l.periods_per_card) {
            broken = Some(format!("{title}: the session length must be 1, 2 or 3 periods."));
        }
        let mut teachers = Vec::new();
        for t in &l.teachers {
            match m.teachers.iter().position(|x| &x.id == t) {
                Some(i) => {
                    if !teachers.contains(&i) {
                        teachers.push(i)
                    }
                }
                None => broken = broken.take().or(Some(format!("{title}: a teacher of this lesson no longer exists."))),
            }
        }
        let mut classes: Vec<(usize, u64)> = Vec::new();
        for p in &l.parts {
            let Some(ci) = m.classes.iter().position(|c| c.id == p.class_id) else {
                broken = broken.take().or(Some(format!("{title}: a class of this lesson no longer exists.")));
                continue;
            };
            let Some((atoms, groups)) = &masks[ci] else {
                broken = broken.take().or(Some(format!(
                    "Class {} is split into too many groups (more than 64 combinations), which Timewizer cannot schedule.",
                    class_names[ci]
                )));
                continue;
            };
            let mask = match &p.group_id {
                None => whole_mask(*atoms),
                Some(g) => match groups.iter().find(|(id, _)| id == g) {
                    Some((_, mk)) => *mk,
                    None => {
                        broken = broken.take().or(Some(format!("{title}: a group of this lesson no longer exists.")));
                        continue;
                    }
                },
            };
            match classes.iter_mut().find(|(c, _)| *c == ci) {
                Some((_, old)) => *old |= mask,
                None => classes.push((ci, mask)),
            }
        }
        if classes.is_empty() && broken.is_none() {
            broken = Some(format!("{title}: the lesson has no class."));
        }
        let shift = classes.first().and_then(|(c, _)| s.shifts.iter().position(|x| x.id == m.classes[*c].shift));
        if broken.is_none() {
            match shift {
                None => broken = Some(format!("{title}: the class has no shift.")),
                Some(sh) => {
                    if classes.iter().any(|(c, _)| m.classes[*c].shift != s.shifts[sh].id) {
                        broken = Some(format!("{title}: the classes of one lesson must all be in the same shift."));
                    } else if !s.shifts[sh].enabled {
                        broken = Some(format!("{title}: its shift is switched off."));
                    } else if let Some(&t) = teachers.iter().find(|&&t| !m.teachers[t].shifts.contains(&s.shifts[sh].id)) {
                        broken = Some(format!(
                            "{} does not work in the {} shift, but {title} is in that shift.",
                            teacher_names[t], s.shifts[sh].name.to_lowercase()
                        ));
                    }
                }
            }
        }
        bases.push(Base {
            title,
            broken,
            teachers,
            classes,
            pool: l.rooms.iter().filter_map(|r| m.rooms.iter().position(|x| &x.id == r)).collect(),
            shift,
            len: l.periods_per_card.clamp(1, 3) as usize,
            cpw: l.cards_per_week,
            subject: m.subjects.iter().position(|x| x.id == l.subject),
        });
    }

    // ---- groups of lessons that share cards ----
    let mut group_of: Vec<Option<usize>> = vec![None; s.lessons.len()];
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut group_problem: Vec<Option<String>> = Vec::new();
    for link in &s.links {
        let idx: Vec<usize> = link.lessons.iter().filter_map(|id| s.lessons.iter().position(|l| &l.id == id)).collect();
        let gi = groups.len();
        let mut problem = None;
        if idx.len() != link.lessons.len() || idx.len() < 2 {
            problem = Some("the link is incomplete".to_string());
        } else if idx.iter().any(|&i| group_of[i].is_some()) {
            problem = Some("a lesson is in more than one link".to_string());
        } else {
            problem = problem.or_else(|| crate::timetable::links::problem(s, &link.lessons));
        }
        for &i in &idx {
            if group_of[i].is_none() {
                group_of[i] = Some(gi);
            }
        }
        groups.push(idx);
        group_problem.push(problem);
    }

    let mut model = Model {
        nd, pm, slots, nsub,
        shift_ids: s.shifts.iter().map(|x| x.id.clone()).collect(),
        teacher_ids: m.teachers.iter().map(|x| x.id.clone()).collect(),
        teacher_names,
        class_names,
        room_ids: m.rooms.iter().map(|x| x.id.clone()).collect(),
        room_names: m.rooms.iter().map(|x| x.name.trim().to_string()).collect(),
        subject_names: m.subjects.iter().map(|x| x.name.trim().to_string()).collect(),
        day_names: s.days.iter().map(|d| d.name.clone()).collect(),
        lesson_ids: s.lessons.iter().map(|x| x.id.clone()).collect(),
        lesson_index: s.lessons.iter().enumerate().map(|(i, x)| (x.id.clone(), i)).collect(),
        lesson_titles: bases.iter().map(|b| b.title.clone()).collect(),
        cards: Vec::new(),
        cards_of_lesson: vec![Vec::new(); s.lessons.len()],
        max_per_day: m.teachers.iter().map(|t| s.settings.max_per_day(t)).collect(),
        max_per_week: m.teachers.iter().map(|t| s.settings.max_per_week(t)).collect(),
        max_days: m.teachers.iter().map(|t| t.max_days_per_week).collect(),
        max_consecutive: m.teachers.iter().map(|t| t.max_consecutive).collect(),
        subj_limit: vec![s.settings.max_same_subject_per_day.max(1); nc * nsub.max(1) * 64],
        t_forb, c_forb, r_forb, t_avoid, c_avoid, periods, cross_ok,
    };

    // ---- cards ----
    let mut done_group = vec![false; groups.len()];
    for li in 0..s.lessons.len() {
        let lessons: Vec<usize> = match group_of[li] {
            Some(g) => {
                if done_group[g] {
                    continue;
                }
                done_group[g] = true;
                groups[g].clone()
            }
            None => vec![li],
        };
        let problem = group_of[li].and_then(|g| group_problem[g].clone()).map(|p| format!("These lessons are linked to happen at the same time, but cannot: {p}."));
        for n in 1..=bases[lessons[0]].cpw {
            let idx = model.cards.len();
            let card = make_card(&bases, &lessons, n, problem.clone());
            model.cards.push(card);
            for &l in &lessons {
                model.cards_of_lesson[l].push(idx);
            }
        }
    }
    // The domain needs the finished model (grids), so fill it in now.
    for v in 0..model.cards.len() {
        if model.cards[v].broken.is_none() {
            let domain = build_domain(&model, &model.cards[v]);
            model.cards[v].domain = domain;
        }
        let usable = usable_days(&model, v);
        let cpw = model.cards[v].members.first().map_or(0, |mb| bases[mb.lesson].cpw);
        model.cards[v].same_day_ok = cpw as usize > usable;
    }

    // ---- most sessions of one subject per class per day ----
    let mut count: HashMap<(usize, usize), [u32; 64]> = HashMap::new();
    let mut usable_min: HashMap<(usize, usize), usize> = HashMap::new();
    for c in &model.cards {
        let mut used: Vec<u8> = c.domain.iter().map(|x| x.day).collect();
        used.dedup();
        let mut usable = used.len();
        for &t in &c.teachers {
            if let Some(mx) = model.max_days[t] {
                usable = usable.min(mx as usize);
            }
        }
        for mb in &c.members {
            if let Some(sub) = mb.subject {
                for &(ci, _) in &mb.classes {
                    let e = usable_min.entry((ci, sub)).or_insert(usable);
                    *e = (*e).min(usable);
                }
            }
            let Some(sub) = mb.subject else { continue };
            for &(ci, mask) in &mb.classes {
                let e = count.entry((ci, sub)).or_insert([0; 64]);
                for b in 0..64 {
                    if mask >> b & 1 == 1 {
                        e[b] += 1;
                    }
                }
            }
        }
    }
    for ((ci, sub), e) in count {
        let days = (0..nd)
            .filter(|&d| s.shifts.iter().position(|x| x.id == m.classes[ci].shift).map_or(false, |sh| model.periods[sh][d] > 0))
            .count()
            .min(usable_min.get(&(ci, sub)).copied().unwrap_or(usize::MAX))
            .max(1) as u32;
        for (b, &n) in e.iter().enumerate() {
            let need = (n + days - 1) / days;
            let cur = &mut model.subj_limit[(ci * nsub + sub) * 64 + b];
            *cur = (*cur).max(need);
        }
    }

    // ---- difficulty (blueprint section 7, phase 1) ----
    let mut teacher_load = vec![0u32; nt];
    for l in &s.lessons {
        for t in &l.teachers {
            if let Some(i) = m.teachers.iter().position(|x| &x.id == t) {
                teacher_load[i] += l.periods_per_card * l.cards_per_week;
            }
        }
    }
    for c in &mut model.cards {
        let len = c.len as f64;
        let busiest = c.teachers.iter().map(|&t| teacher_load[t]).max().unwrap_or(0) as f64;
        let part_time = c.teachers.iter().any(|&t| m.teachers[t].max_days_per_week.is_some()) as u8 as f64;
        let rooms: f64 = c.pools.iter().map(|p| 30.0 / p.len().max(1) as f64).sum();
        c.difficulty = 6.0 * len * len
            + 4.0 * c.teachers.len() as f64
            + 5.0 * c.classes.len() as f64
            + 60.0 / (c.domain.len().max(1) as f64)
            + rooms
            + busiest * 0.4
            + 8.0 * part_time
            + 15.0 * (c.members.len() as f64 - 1.0);
    }
    model
}

fn make_card(bases: &[Base], lessons: &[usize], n: u32, group_problem: Option<String>) -> CardInfo {
    let first = &bases[lessons[0]];
    let mut broken = group_problem;
    let mut teachers: Vec<usize> = Vec::new();
    let mut classes: Vec<(usize, u64)> = Vec::new();
    let mut members = Vec::new();
    let mut pools = Vec::new();
    for &li in lessons {
        let b = &bases[li];
        if broken.is_none() {
            broken = b.broken.clone();
        }
        for &t in &b.teachers {
            if !teachers.contains(&t) {
                teachers.push(t);
            }
        }
        for &(ci, mask) in &b.classes {
            match classes.iter_mut().find(|(c, _)| *c == ci) {
                Some((_, old)) => *old |= mask,
                None => classes.push((ci, mask)),
            }
        }
        if !b.pool.is_empty() {
            pools.push(b.pool.clone());
        }
        members.push(Member { lesson: li, number: n, subject: b.subject, classes: b.classes.clone(), pool: b.pool.clone() });
    }
    if broken.is_none() && pools.len() > MAXR {
        broken = Some(format!("{}: too many linked lessons need a room at once (at most {MAXR}).", first.title));
    }
    CardInfo {
        members,
        len: first.len,
        shift: first.shift.unwrap_or(0),
        teachers,
        classes,
        pools,
        domain: Vec::new(),
        same_day_ok: false,
        difficulty: 0.0,
        broken,
    }
}

fn build_domain(model: &Model, c: &CardInfo) -> Vec<Start> {
    let mut domain = Vec::new();
    for d in 0..model.nd {
        let n = model.periods[c.shift][d];
        for p in 0..n {
            if p + c.len > n {
                break;
            }
            if (p + 1..p + c.len).any(|k| !model.cross_ok[c.shift][k]) {
                continue;
            }
            let mut ok = true;
            let mut avoid = 0u8;
            for k in 0..c.len {
                let sl = model.slot(c.shift, d, p + k);
                for &t in &c.teachers {
                    ok &= !model.t_forb[t * model.slots + sl];
                    avoid += model.t_avoid[t * model.slots + sl] as u8;
                }
                for &(ci, _) in &c.classes {
                    ok &= !model.c_forb[ci * model.slots + sl];
                    avoid += model.c_avoid[ci * model.slots + sl] as u8;
                }
            }
            if ok && !c.pools.is_empty() {
                let base = model.slot(c.shift, d, p);
                ok = distinct_assignment(&c.pools, &|r| (0..c.len).all(|k| !model.r_forb[r * model.slots + base + k])).is_some();
            }
            if ok {
                domain.push(Start { day: d as u8, period: p as u8, avoid });
            }
        }
    }
    domain
}
