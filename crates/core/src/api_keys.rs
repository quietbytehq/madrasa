//! Read-only API keys (Standard stage 9): scopes, expiry, the IP allow-list and the rate
//! limit window. Pure; the API applies them.

use std::net::IpAddr;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

/// What a key may read. Each `/api/ro/v1` endpoint needs one scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Students,
    Attendance,
    Fees,
    Exams,
    Timetable,
    Library,
}

impl Scope {
    pub const ALL: &'static [Scope] = &[
        Scope::Students,
        Scope::Attendance,
        Scope::Fees,
        Scope::Exams,
        Scope::Timetable,
        Scope::Library,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Students => "students",
            Scope::Attendance => "attendance",
            Scope::Fees => "fees",
            Scope::Exams => "exams",
            Scope::Timetable => "timetable",
            Scope::Library => "library",
        }
    }

    pub fn parse(s: &str) -> Option<Scope> {
        Scope::ALL.iter().copied().find(|k| k.as_str() == s.trim())
    }
}

/// Requests a key may make per minute.
pub const RATE_LIMIT_PER_MINUTE: u32 = 120;

/// A key is usable while it is not revoked and not past its expiry.
pub fn is_usable(
    revoked_at: Option<DateTime<Utc>>,
    expires_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> bool {
    revoked_at.is_none() && expires_at.is_none_or(|e| e > now)
}

/// One entry of the allow-list: an address or a CIDR prefix (`10.0.0.0/8`, `fd00::/64`).
fn matches_entry(entry: &str, ip: IpAddr) -> bool {
    let entry = entry.trim();
    if entry.is_empty() {
        return false;
    }
    let (addr, bits) = match entry.split_once('/') {
        Some((a, b)) => (a, b.parse::<u32>().ok()),
        None => (entry, None),
    };
    let Ok(base) = addr.trim().parse::<IpAddr>() else {
        return false;
    };
    match (base, ip) {
        (IpAddr::V4(b), IpAddr::V4(i)) => {
            let bits = bits.unwrap_or(32).min(32);
            let mask = if bits == 0 {
                0
            } else {
                u32::MAX << (32 - bits)
            };
            (u32::from(b) & mask) == (u32::from(i) & mask)
        }
        (IpAddr::V6(b), IpAddr::V6(i)) => {
            let bits = bits.unwrap_or(128).min(128);
            let mask = if bits == 0 {
                0
            } else {
                u128::MAX << (128 - bits)
            };
            (u128::from(b) & mask) == (u128::from(i) & mask)
        }
        (IpAddr::V4(b), IpAddr::V6(i)) => i.to_ipv4_mapped().is_some_and(|v4| {
            let bits = bits.unwrap_or(32).min(32);
            let mask = if bits == 0 {
                0
            } else {
                u32::MAX << (32 - bits)
            };
            (u32::from(b) & mask) == (u32::from(v4) & mask)
        }),
        _ => false,
    }
}

/// An empty allow-list admits every address; otherwise the caller's address must match an
/// entry. A missing address (unknown peer) passes only an empty list.
pub fn ip_allowed(allow: &[String], ip: Option<IpAddr>) -> bool {
    if allow.iter().all(|e| e.trim().is_empty()) {
        return true;
    }
    ip.is_some_and(|ip| allow.iter().any(|e| matches_entry(e, ip)))
}

/// Every entry must be an address or a CIDR prefix.
pub fn validate_allow_list(allow: &[String]) -> Result<(), String> {
    for e in allow.iter().map(|e| e.trim()).filter(|e| !e.is_empty()) {
        let (addr, bits) = match e.split_once('/') {
            Some((a, b)) => (a, Some(b)),
            None => (e, None),
        };
        let ip: IpAddr = addr
            .trim()
            .parse()
            .map_err(|_| format!("\"{e}\" is not an IP address or CIDR range"))?;
        if let Some(b) = bits {
            let max = if ip.is_ipv4() { 32 } else { 128 };
            let n: u32 = b
                .trim()
                .parse()
                .map_err(|_| format!("\"{e}\" has an invalid prefix length"))?;
            if n > max {
                return Err(format!("\"{e}\" has an invalid prefix length"));
            }
        }
    }
    Ok(())
}

/// A fixed one-minute window per key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateWindow {
    pub started_at: DateTime<Utc>,
    pub count: u32,
}

/// Count a request against the window; `Err(seconds)` says how long to wait.
pub fn rate_tick(
    window: Option<RateWindow>,
    now: DateTime<Utc>,
    limit: u32,
) -> Result<RateWindow, u32> {
    let w = match window {
        Some(w) if now - w.started_at < Duration::minutes(1) => w,
        _ => RateWindow {
            started_at: now,
            count: 0,
        },
    };
    if w.count >= limit {
        let wait = 60 - (now - w.started_at).num_seconds().clamp(0, 59);
        return Err(wait as u32);
    }
    Ok(RateWindow {
        started_at: w.started_at,
        count: w.count + 1,
    })
}

/// A new key: `mk_` + 32 url-safe random characters; stored as its SHA-256 hex.
pub fn generate_key() -> (String, String) {
    let token = crate::session_token::generate();
    let key = format!("mk_{}", token.token);
    (key.clone(), crate::session_token::hash(&key))
}

/// The part of a key shown in lists after it was created.
pub fn prefix(key: &str) -> String {
    key.chars().take(11).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn scopes_round_trip() {
        for s in Scope::ALL {
            assert_eq!(Scope::parse(s.as_str()), Some(*s));
        }
        assert_eq!(Scope::parse("payroll"), None);
    }

    #[test]
    fn usable_until_expiry_or_revocation() {
        let now = t("2026-04-01T10:00:00Z");
        assert!(is_usable(None, None, now));
        assert!(is_usable(None, Some(t("2026-05-01T00:00:00Z")), now));
        assert!(!is_usable(None, Some(t("2026-03-01T00:00:00Z")), now));
        assert!(!is_usable(Some(now), None, now));
    }

    #[test]
    fn ip_allow_lists() {
        let ip = |s: &str| s.parse::<IpAddr>().unwrap();
        assert!(ip_allowed(&[], Some(ip("8.8.8.8"))));
        assert!(ip_allowed(&[], None));
        let list = vec![
            "10.0.0.0/8".to_string(),
            "192.168.1.20".to_string(),
            "fd00::/64".to_string(),
        ];
        assert!(ip_allowed(&list, Some(ip("10.20.30.40"))));
        assert!(ip_allowed(&list, Some(ip("192.168.1.20"))));
        assert!(!ip_allowed(&list, Some(ip("192.168.1.21"))));
        assert!(ip_allowed(&list, Some(ip("fd00::1"))));
        assert!(!ip_allowed(&list, Some(ip("fd01::1"))));
        assert!(ip_allowed(&list, Some(ip("::ffff:10.1.1.1"))), "v4-mapped");
        assert!(!ip_allowed(&list, None), "unknown peer with a list");
        assert!(validate_allow_list(&list).is_ok());
        assert!(validate_allow_list(&["10.0.0.0/33".to_string()]).is_err());
        assert!(validate_allow_list(&["school.local".to_string()]).is_err());
    }

    #[test]
    fn rate_windows() {
        let now = t("2026-04-01T10:00:00Z");
        let mut w = None;
        for _ in 0..3 {
            w = Some(rate_tick(w, now, 3).unwrap());
        }
        assert_eq!(w.unwrap().count, 3);
        assert_eq!(rate_tick(w, now + Duration::seconds(10), 3), Err(50));
        let later = rate_tick(w, now + Duration::seconds(61), 3).unwrap();
        assert_eq!(later.count, 1);
    }

    #[test]
    fn keys_have_a_prefix_and_a_hash() {
        let (key, hash) = generate_key();
        assert!(key.starts_with("mk_"));
        assert_eq!(hash.len(), 64);
        assert_eq!(prefix(&key).len(), 11);
        assert_ne!(generate_key().0, key);
    }
}
