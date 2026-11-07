//! Repair (ruin and recreate). When the main search stalls with a few sessions left over,
//! take one of them, force it into its least-blocked time, lift out the sessions it displaces
//! together with a few related ones, and place those all again. The change is kept when no more
//! sessions end up unplaced than before (so the timetable can drift sideways), and undone otherwise.
//! This finishes what ejection chains alone cannot: sessions that need several busy teachers free
//! at the same time, such as option blocks.
use super::engine::{Config, Engine, Pl, Rng, Rules};
use super::model::Model;
use super::state_from_places;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// How hard to try: give up after this many attempts in a row that placed nothing new (more for
/// bigger schools), or after `spent` x 2 of effort (at least 1.5 s, at most 45 s) - whichever is first.
/// Small or hopeless schools therefore stop quickly; a large one that nearly worked gets longer.
pub(super) fn limits(cards: usize, spent: Duration) -> (u32, Duration) {
    let stale = (cards as f64 * 0.4).clamp(60.0, 1500.0) as u32;
    (stale, (spent * 2).clamp(Duration::from_millis(1500), Duration::from_secs(45)))
}

/// Returns the improved placements and whether the user cancelled.
#[allow(clippy::too_many_arguments)]
pub(super) fn repair(
    m: &Model,
    start: Vec<Option<Pl>>,
    locked: &[bool],
    rules: Rules,
    depth: usize,
    seed: u64,
    deadline: Instant,
    cancel: &AtomicBool,
    spent: Duration,
    progress: &mut dyn FnMut(&[Option<Pl>]),
) -> (Vec<Option<Pl>>, bool) {
    let (max_stale, max_time) = limits(m.cards.len(), spent);
    let deadline = deadline.min(Instant::now() + max_time);
    let placeable: Vec<bool> = m.cards.iter().map(|c| c.broken.is_none() && !c.domain.is_empty()).collect();
    let mut cur = start;
    let mut rng = Rng::new(seed ^ 0x00C0_FFEE);
    let (mut stale, mut round) = (0u32, 0u64);
    loop {
        let missing: Vec<usize> = (0..cur.len()).filter(|&v| cur[v].is_none() && placeable[v]).collect();
        if missing.is_empty() || stale >= max_stale || Instant::now() >= deadline {
            return (cur, false);
        }
        if cancel.load(Ordering::Relaxed) {
            return (cur, true);
        }
        round += 1;
        let u = missing[(rng.next() % missing.len() as u64) as usize];
        let cfg = Config { max_depth: depth, stagnation: Duration::from_millis(200), deadline, seed: seed.wrapping_add(round.wrapping_mul(7919)), limit: Some(120), node_budget: Some(40) };
        let mut eng = Engine::new(m, &cfg);
        eng.s = state_from_places(m, &cur, locked);
        eng.s.rules = rules;
        let have = cur.iter().flatten().count();

        // Force the session in; whatever stood in the way is lifted out.
        let Some(mut lifted) = eng.kick(u) else {
            stale += 1;
            continue;
        };
        eng.s.commit();

        // Lift out a few related sessions as well (same teachers, or the same class), to give the
        // ones just displaced more room to land somewhere new.
        let mut teachers: Vec<usize> = m.cards[u].teachers.clone();
        let mut classes: Vec<usize> = m.cards[u].classes.iter().map(|x| x.0).collect();
        for &o in &lifted {
            teachers.extend(m.cards[o].teachers.iter().copied());
            classes.extend(m.cards[o].classes.iter().map(|x| x.0));
        }
        let related: Vec<usize> = (0..m.cards.len())
            .filter(|&o| o != u && eng.s.place[o].is_some() && !eng.s.locked[o] && !lifted.contains(&o))
            .filter(|&o| m.cards[o].teachers.iter().any(|t| teachers.contains(t)) || m.cards[o].classes.iter().any(|c| classes.contains(&c.0)))
            .collect();
        let extra = (rng.next() % 12) as usize;
        for _ in 0..extra.min(related.len()) {
            let o = related[(rng.next() % related.len() as u64) as usize];
            if eng.s.place[o].is_some() {
                eng.s.unplace(o);
                lifted.push(o);
            }
        }
        eng.s.commit();

        let out = eng.run(lifted, &cfg, cancel, &mut |_| {});
        let after = out.best.iter().flatten().count();
        if after > have {
            cur = out.best;
            stale = 0;
            progress(&cur);
        } else if after == have {
            cur = out.best; // a sideways move: keeps the search from getting stuck
            stale += 1;
        } else {
            stale += 1;
        }
        if out.cancelled {
            return (cur, true);
        }
    }
}
