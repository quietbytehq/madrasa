//! Hybrid logical clock stamps for the sync-ready `hlc` column.
//!
//! Format (32 chars, lexicographically sortable):
//! `<unix_ms 13 digits>-<counter 5 hex>-<node 12 hex>`.
//! No sync engine exists yet; the column and generator exist so that one can be added later
//! without touching every table.

use std::fmt;
use std::str::FromStr;
use std::sync::Mutex;

/// A single stamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HlcStamp {
    pub unix_ms: u64,
    pub counter: u32,
    pub node: u64,
}

impl HlcStamp {
    pub const LEN: usize = 13 + 1 + 5 + 1 + 12;
}

impl fmt::Display for HlcStamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:013}-{:05x}-{:012x}",
            self.unix_ms,
            self.counter & 0xf_ffff,
            self.node & 0xffff_ffff_ffff
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid hlc stamp: {0}")]
pub struct InvalidStamp(pub String);

impl FromStr for HlcStamp {
    type Err = InvalidStamp;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || InvalidStamp(s.to_string());
        if s.len() != Self::LEN {
            return Err(err());
        }
        let mut parts = s.split('-');
        let ms = parts.next().ok_or_else(err)?;
        let counter = parts.next().ok_or_else(err)?;
        let node = parts.next().ok_or_else(err)?;
        if parts.next().is_some() || ms.len() != 13 || counter.len() != 5 || node.len() != 12 {
            return Err(err());
        }
        Ok(HlcStamp {
            unix_ms: ms.parse().map_err(|_| err())?,
            counter: u32::from_str_radix(counter, 16).map_err(|_| err())?,
            node: u64::from_str_radix(node, 16).map_err(|_| err())?,
        })
    }
}

/// Thread-safe stamp generator for one node (one install).
#[derive(Debug)]
pub struct Hlc {
    node: u64,
    state: Mutex<(u64, u32)>,
}

impl Hlc {
    /// `node` identifies this install; only the low 48 bits are used.
    pub fn new(node: u64) -> Self {
        Self {
            node: node & 0xffff_ffff_ffff,
            state: Mutex::new((0, 0)),
        }
    }

    /// Random node id for a fresh install.
    pub fn with_random_node() -> Self {
        Self::new(rand::random::<u64>())
    }

    pub fn node(&self) -> u64 {
        self.node
    }

    /// Next stamp using the system clock.
    pub fn now(&self) -> HlcStamp {
        self.tick(chrono::Utc::now().timestamp_millis().max(0) as u64)
    }

    /// Next stamp for a given wall-clock reading. Monotonic even if the clock goes backwards.
    pub fn tick(&self, wall_ms: u64) -> HlcStamp {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let (last_ms, counter) = *state;
        let (ms, counter) = if wall_ms > last_ms {
            (wall_ms, 0)
        } else {
            (last_ms, counter + 1)
        };
        *state = (ms, counter);
        HlcStamp {
            unix_ms: ms,
            counter,
            node: self.node,
        }
    }

    /// Fold in a stamp received from another node so our next stamp is later than it.
    pub fn observe(&self, remote: HlcStamp, wall_ms: u64) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let (last_ms, counter) = *state;
        let max_ms = wall_ms.max(last_ms).max(remote.unix_ms);
        let counter = if max_ms == last_ms && max_ms == remote.unix_ms {
            counter.max(remote.counter) + 1
        } else if max_ms == last_ms {
            counter + 1
        } else if max_ms == remote.unix_ms {
            remote.counter + 1
        } else {
            0
        };
        *state = (max_ms, counter);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_is_fixed_width_and_sortable() {
        let s = HlcStamp {
            unix_ms: 1_700_000_000_000,
            counter: 3,
            node: 0xabc,
        };
        let text = s.to_string();
        assert_eq!(text.len(), HlcStamp::LEN);
        assert_eq!(text, "1700000000000-00003-000000000abc");
        assert_eq!(text.parse::<HlcStamp>().unwrap(), s);
        let later = HlcStamp {
            unix_ms: 1_700_000_000_001,
            counter: 0,
            node: 0,
        };
        assert!(later.to_string() > text);
        let same_ms_higher_counter = HlcStamp {
            unix_ms: s.unix_ms,
            counter: 4,
            node: 0,
        };
        assert!(same_ms_higher_counter.to_string() > text);
    }

    #[test]
    fn ticks_are_strictly_monotonic_even_when_clock_goes_back() {
        let hlc = Hlc::new(7);
        let a = hlc.tick(1000);
        let b = hlc.tick(1000);
        let c = hlc.tick(999);
        let d = hlc.tick(1001);
        assert!(a < b && b < c && c < d);
        assert_eq!(c.unix_ms, 1000);
        assert_eq!(c.counter, 2);
        assert_eq!(d.counter, 0);
    }

    #[test]
    fn observe_moves_past_remote() {
        let hlc = Hlc::new(1);
        hlc.tick(1000);
        hlc.observe(
            HlcStamp {
                unix_ms: 5000,
                counter: 9,
                node: 2,
            },
            1000,
        );
        let next = hlc.tick(1000);
        assert_eq!((next.unix_ms, next.counter), (5000, 11));
    }

    #[test]
    fn rejects_malformed() {
        assert!("abc".parse::<HlcStamp>().is_err());
        assert!("1700000000000-zzzzz-000000000abc"
            .parse::<HlcStamp>()
            .is_err());
    }

    #[test]
    fn now_uses_wall_clock() {
        let hlc = Hlc::with_random_node();
        let s = hlc.now();
        assert!(s.unix_ms > 1_600_000_000_000);
        assert_eq!(s.node, hlc.node());
    }
}
