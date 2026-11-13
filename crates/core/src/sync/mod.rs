//! Sync between a school and its copy in a Nexus hub (ADR 0005).
//!
//! Pure rules only: how each table syncs ([`policy`]), what travels between the two sides
//! ([`wire`]) and how an incoming change is merged with the local record ([`merge`]). The
//! SQL that captures and applies changes is `madrasa_db::sync`; both the school's API and
//! the hub run it, so the two ends cannot disagree about the rules.

pub mod merge;
pub mod policy;
pub mod wire;

pub use merge::{decide_field, plan_update, FieldDecision, LocalRow, LocalStamp, RowPlan};
pub use policy::{policy, Mode, TablePolicy, META_FIELDS, TABLES};
pub use wire::{
    AckRequest, AckResponse, ApplyReport, Change, JoinRequest, JoinResponse, LeaveRequest, Origin,
    Overwrite, Parked, PullRequest, PullResponse, PushRequest, PushResponse, RestoredRequest,
    RestoredResponse,
};

/// Rows per request. Small enough for a slow school uplink, large enough that a first
/// upload of a full year finishes in a few minutes.
pub const BATCH_SIZE: usize = 500;

/// The order in which tables must be sent so that a record arrives after the records it
/// refers to. `edges` are (child, parent) foreign keys between the given tables. Ties are
/// broken alphabetically so the order is stable; tables caught in a cycle come last.
pub fn insertion_order(tables: &[&str], edges: &[(String, String)]) -> Vec<String> {
    let mut remaining: Vec<&str> = tables.to_vec();
    remaining.sort_unstable();
    remaining.dedup();
    let mut done: Vec<String> = Vec::with_capacity(remaining.len());
    while !remaining.is_empty() {
        let ready: Vec<&str> = remaining
            .iter()
            .copied()
            .filter(|t| {
                edges.iter().all(|(child, parent)| {
                    child != t
                        || parent == t
                        || done.iter().any(|d| d == parent)
                        || !tables.contains(&parent.as_str())
                })
            })
            .collect();
        if ready.is_empty() {
            // A cycle: send the rest as they are; the receiver retries records that arrive
            // before their parents.
            done.extend(remaining.iter().map(|t| t.to_string()));
            break;
        }
        remaining.retain(|t| !ready.contains(t));
        done.extend(ready.into_iter().map(str::to_string));
    }
    done
}

/// The hub address a school may use. Schools send their records to it, so it must be HTTPS;
/// plain HTTP is accepted only for this computer itself (trying a hub out). Returns the
/// address without a trailing slash.
pub fn hub_url(input: &str) -> Result<String, &'static str> {
    let url = input.trim().trim_end_matches('/');
    let rest = if let Some(rest) = url.strip_prefix("https://") {
        rest
    } else if let Some(rest) = url.strip_prefix("http://") {
        let host = rest.split(['/', ':']).next().unwrap_or_default();
        let bracketed = rest.starts_with("[::1]");
        if !(bracketed || host == "localhost" || host == "127.0.0.1") {
            return Err("The hub address must start with https://. Plain http:// is only accepted for this computer (localhost).");
        }
        rest
    } else {
        return Err("Enter the hub address in full, for example https://nexus.example.org.");
    };
    if rest.is_empty() || rest.starts_with('/') || rest.contains(char::is_whitespace) {
        return Err("Enter the hub address in full, for example https://nexus.example.org.");
    }
    Ok(url.to_string())
}

/// Why a hub refuses to sync with a school.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VersionError {
    #[error("This school runs a newer version of Madrasa than the hub. Update the hub first, then sync again.")]
    SchoolIsNewer,
}

/// The hub must know every table and column the school can send, so it must be on the same
/// schema version or a newer one. An older school is fine: fields it does not send keep the
/// hub's defaults.
pub fn check_versions(hub_schema: i64, school_schema: i64) -> Result<(), VersionError> {
    if school_schema > hub_schema {
        Err(VersionError::SchoolIsNewer)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parents_are_sent_before_children() {
        let e = |c: &str, p: &str| (c.to_string(), p.to_string());
        let edges = [
            e("students", "schools"),
            e("enrollments", "students"),
            e("enrollments", "sections"),
            e("sections", "schools"),
            e("fee_receipts", "fee_receipts"),
            e("fee_receipts", "students"),
            e("students", "settings"),
        ];
        let order = insertion_order(
            &[
                "enrollments",
                "fee_receipts",
                "students",
                "schools",
                "sections",
            ],
            &edges,
        );
        assert_eq!(
            order,
            [
                "schools",
                "sections",
                "students",
                "enrollments",
                "fee_receipts"
            ]
        );
        // A cycle does not hang; every table is still listed once.
        let cyc = [e("a", "b"), e("b", "a")];
        assert_eq!(insertion_order(&["b", "a", "c"], &cyc), ["c", "a", "b"]);
    }

    #[test]
    fn hub_addresses_must_be_https_except_on_this_computer() {
        assert_eq!(
            hub_url(" https://nexus.example.org/ "),
            Ok("https://nexus.example.org".into())
        );
        assert_eq!(
            hub_url("https://nexus.example.org:8443/hub"),
            Ok("https://nexus.example.org:8443/hub".into())
        );
        assert_eq!(
            hub_url("http://localhost:4780"),
            Ok("http://localhost:4780".into())
        );
        assert_eq!(
            hub_url("http://127.0.0.1:4780/"),
            Ok("http://127.0.0.1:4780".into())
        );
        assert_eq!(hub_url("http://[::1]:4780"), Ok("http://[::1]:4780".into()));
        for bad in [
            "http://nexus.example.org",
            "http://localhost.evil.org",
            "http://127.0.0.10",
            "nexus.example.org",
            "ftp://x",
            "https://",
            "https:///x",
            "https://a b",
            "",
        ] {
            assert!(hub_url(bad).is_err(), "{bad} must be refused");
        }
    }

    #[test]
    fn the_hub_must_not_be_older_than_the_school() {
        assert_eq!(check_versions(26, 26), Ok(()));
        assert_eq!(check_versions(27, 26), Ok(()));
        assert_eq!(check_versions(26, 27), Err(VersionError::SchoolIsNewer));
    }
}
