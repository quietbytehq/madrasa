//! Edition feature flags. The single place that decides which features an edition enables.
//! Both the API (authoritative) and the UI (cosmetic) consult this mapping; the UI keeps a
//! mirror of the feature codes in `apps/desktop/src/api/types.ts`.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Product edition. Stored as the instance-wide `edition` setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Edition {
    Lite,
    Standard,
    Nexus,
}

impl Edition {
    pub const DEFAULT: Edition = Edition::Lite;

    pub fn as_str(self) -> &'static str {
        match self {
            Edition::Lite => "lite",
            Edition::Standard => "standard",
            Edition::Nexus => "nexus",
        }
    }

    /// Features enabled for this edition, in stable order.
    pub fn features(self) -> &'static [Feature] {
        match self {
            Edition::Lite => LITE_FEATURES,
            Edition::Standard => STANDARD_FEATURES,
            Edition::Nexus => NEXUS_FEATURES,
        }
    }

    pub fn enables(self, feature: Feature) -> bool {
        self.features().contains(&feature)
    }

    /// Advisory soft limits (Lite only). `None` means unlimited.
    pub fn soft_limits(self) -> Option<SoftLimits> {
        match self {
            Edition::Lite => Some(SoftLimits {
                students: 1_200,
                staff_accounts: 250,
            }),
            _ => None,
        }
    }
}

impl fmt::Display for Edition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown edition: {0}")]
pub struct UnknownEdition(pub String);

impl FromStr for Edition {
    type Err = UnknownEdition;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "lite" => Ok(Edition::Lite),
            "standard" => Ok(Edition::Standard),
            "nexus" => Ok(Edition::Nexus),
            other => Err(UnknownEdition(other.to_string())),
        }
    }
}

/// Advisory limits for an edition. The UI warns when they are approached; nothing is blocked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SoftLimits {
    pub students: u32,
    pub staff_accounts: u32,
}

/// Feature codes. Keep in sync with `Feature` in `apps/desktop/src/api/types.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Feature {
    Dashboard,
    Students,
    Attendance,
    Examinations,
    Fees,
    Teachers,
    Reports,
    Backup,
    Settings,
    Campuses,
    Transport,
    Library,
    Hostel,
    /// HR basics: staff leave with balances and approvals, staff attendance polish.
    Hr,
    ParentsPortal,
    NexusHub,
    // Standard alpha (docs/TASKS-STANDARD.md)
    Timetable,
    /// Student leave requests linked to attendance.
    Leave,
    Admissions,
    Inventory,
    /// Active Directory / LDAP login.
    DirectoryLogin,
    /// Read-only API keys and endpoints.
    ReadonlyApi,
    /// Joining a Madrasa Nexus hub and syncing with it (docs/TASKS-NEXUS.md). Every school
    /// edition may join; a school that has not joined is unchanged.
    NexusLink,
}

impl Feature {
    pub const ALL: &'static [Feature] = &[
        Feature::Dashboard,
        Feature::Students,
        Feature::Attendance,
        Feature::Examinations,
        Feature::Fees,
        Feature::Teachers,
        Feature::Reports,
        Feature::Backup,
        Feature::Settings,
        Feature::Campuses,
        Feature::Transport,
        Feature::Library,
        Feature::Hostel,
        Feature::Hr,
        Feature::ParentsPortal,
        Feature::NexusHub,
        Feature::Timetable,
        Feature::Leave,
        Feature::Admissions,
        Feature::Inventory,
        Feature::DirectoryLogin,
        Feature::ReadonlyApi,
        Feature::NexusLink,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Feature::Dashboard => "dashboard",
            Feature::Students => "students",
            Feature::Attendance => "attendance",
            Feature::Examinations => "examinations",
            Feature::Fees => "fees",
            Feature::Teachers => "teachers",
            Feature::Reports => "reports",
            Feature::Backup => "backup",
            Feature::Settings => "settings",
            Feature::Campuses => "campuses",
            Feature::Transport => "transport",
            Feature::Library => "library",
            Feature::Hostel => "hostel",
            Feature::Hr => "hr",
            Feature::ParentsPortal => "parents_portal",
            Feature::NexusHub => "nexus_hub",
            Feature::Timetable => "timetable",
            Feature::Leave => "leave",
            Feature::Admissions => "admissions",
            Feature::Inventory => "inventory",
            Feature::DirectoryLogin => "directory_login",
            Feature::ReadonlyApi => "readonly_api",
            Feature::NexusLink => "nexus_link",
        }
    }
}

impl fmt::Display for Feature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

const LITE_FEATURES: &[Feature] = &[
    Feature::Dashboard,
    Feature::Students,
    Feature::Attendance,
    Feature::Examinations,
    Feature::Fees,
    Feature::Teachers,
    Feature::Reports,
    Feature::Backup,
    Feature::Settings,
    Feature::NexusLink,
];

/// Everything Standard adds to Lite (docs/TASKS-STANDARD.md), in sidebar order.
pub const STANDARD_ONLY: &[Feature] = &[
    Feature::Timetable,
    Feature::Leave,
    Feature::Admissions,
    Feature::Library,
    Feature::Transport,
    Feature::Hostel,
    Feature::Inventory,
    Feature::Hr,
    Feature::Campuses,
    Feature::DirectoryLogin,
    Feature::ReadonlyApi,
];

const STANDARD_FEATURES: &[Feature] = &[
    Feature::Dashboard,
    Feature::Students,
    Feature::Attendance,
    Feature::Examinations,
    Feature::Fees,
    Feature::Teachers,
    Feature::Reports,
    Feature::Backup,
    Feature::Settings,
    Feature::Timetable,
    Feature::Leave,
    Feature::Admissions,
    Feature::Library,
    Feature::Transport,
    Feature::Hostel,
    Feature::Inventory,
    Feature::Hr,
    Feature::Campuses,
    Feature::DirectoryLogin,
    Feature::ReadonlyApi,
    Feature::NexusLink,
];

const NEXUS_FEATURES: &[Feature] = &[
    Feature::Dashboard,
    Feature::Students,
    Feature::Attendance,
    Feature::Examinations,
    Feature::Fees,
    Feature::Teachers,
    Feature::Reports,
    Feature::Backup,
    Feature::Settings,
    Feature::Timetable,
    Feature::Leave,
    Feature::Admissions,
    Feature::Library,
    Feature::Transport,
    Feature::Hostel,
    Feature::Inventory,
    Feature::Hr,
    Feature::Campuses,
    Feature::DirectoryLogin,
    Feature::ReadonlyApi,
    Feature::ParentsPortal,
    Feature::NexusHub,
    Feature::NexusLink,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lite_has_core_modules_only() {
        assert!(Edition::Lite.enables(Feature::Students));
        assert!(Edition::Lite.enables(Feature::Fees));
        assert!(!Edition::Lite.enables(Feature::Campuses));
        assert!(!Edition::Lite.enables(Feature::ParentsPortal));
        assert!(!Edition::Lite.enables(Feature::NexusHub));
    }

    #[test]
    fn editions_are_supersets_of_each_other() {
        for f in Edition::Lite.features() {
            assert!(Edition::Standard.enables(*f));
            assert!(Edition::Nexus.enables(*f));
        }
        for f in Edition::Standard.features() {
            assert!(Edition::Nexus.enables(*f));
        }
        assert!(!Edition::Standard.enables(Feature::NexusHub));
        assert!(Edition::Nexus.enables(Feature::NexusHub));
    }

    #[test]
    fn lite_enables_no_standard_module() {
        for f in STANDARD_ONLY {
            assert!(!Edition::Lite.enables(*f), "{f} must not be on in Lite");
            assert!(Edition::Standard.enables(*f), "{f} missing from standard");
        }
        // Lite's list is exactly the nine core modules, plus the choice to join a Nexus
        // hub: nothing Standard leaks in.
        assert_eq!(Edition::Lite.features().len(), 10);
        assert!(Edition::Lite.enables(Feature::NexusLink));
        assert!(!STANDARD_ONLY.contains(&Feature::NexusLink));
    }

    #[test]
    fn nexus_enables_everything() {
        for f in Feature::ALL {
            assert!(Edition::Nexus.enables(*f), "{f} missing from nexus");
        }
    }

    #[test]
    fn parses_and_serializes_lowercase() {
        assert_eq!("Lite".parse::<Edition>().unwrap(), Edition::Lite);
        assert_eq!(" nexus ".parse::<Edition>().unwrap(), Edition::Nexus);
        assert!("pro".parse::<Edition>().is_err());
        assert_eq!(
            serde_json::to_string(&Edition::Standard).unwrap(),
            "\"standard\""
        );
        assert_eq!(serde_json::to_string(&Feature::Hr).unwrap(), "\"hr\"");
        assert_eq!(
            serde_json::to_string(&Feature::DirectoryLogin).unwrap(),
            "\"directory_login\""
        );
        assert_eq!(Edition::DEFAULT, Edition::Lite);
    }

    #[test]
    fn feature_strings_match_serde() {
        for f in Feature::ALL {
            let json = serde_json::to_string(f).unwrap();
            assert_eq!(json, format!("\"{}\"", f.as_str()));
        }
    }

    #[test]
    fn soft_limits_only_for_lite() {
        assert_eq!(Edition::Lite.soft_limits().unwrap().students, 1_200);
        assert!(Edition::Standard.soft_limits().is_none());
    }
}
