//! Hub permissions. Checked in the backend on every route; the console only hides things.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HubPermission {
    /// Schools, invitations, hub users, corrections to school records.
    Admin,
    /// Network-wide reports.
    Reports,
    /// Release an escrowed backup key.
    Escrow,
}

impl HubPermission {
    pub const ALL: [HubPermission; 3] = [
        HubPermission::Admin,
        HubPermission::Reports,
        HubPermission::Escrow,
    ];

    pub fn code(self) -> &'static str {
        match self {
            HubPermission::Admin => "nexus.admin",
            HubPermission::Reports => "nexus.reports",
            HubPermission::Escrow => "nexus.escrow",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.code() == code)
    }
}

impl fmt::Display for HubPermission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// Parse permission codes from a request: unknown codes are refused, duplicates removed,
/// order normalised.
pub fn parse_codes(codes: &[String]) -> Result<Vec<String>, String> {
    let mut out: Vec<HubPermission> = Vec::new();
    for c in codes {
        let p = HubPermission::from_code(c.trim()).ok_or_else(|| {
            let all: Vec<&str> = HubPermission::ALL.iter().map(|p| p.code()).collect();
            format!("Permissions must be chosen from: {}.", all.join(", "))
        })?;
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out.sort_by_key(|p| HubPermission::ALL.iter().position(|x| x == p));
    Ok(out.into_iter().map(|p| p.code().to_string()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip() {
        for p in HubPermission::ALL {
            assert_eq!(HubPermission::from_code(p.code()), Some(p));
        }
        assert_eq!(HubPermission::from_code("fees.manage"), None);
    }

    #[test]
    fn parse_normalises_and_refuses_unknown_codes() {
        let got = parse_codes(&[
            "nexus.escrow".into(),
            " nexus.admin ".into(),
            "nexus.escrow".into(),
        ])
        .unwrap();
        assert_eq!(got, ["nexus.admin", "nexus.escrow"]);
        let err = parse_codes(&["students.read".into()]).unwrap_err();
        assert_eq!(
            err,
            "Permissions must be chosen from: nexus.admin, nexus.reports, nexus.escrow."
        );
    }
}
