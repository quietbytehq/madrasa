//! Permission codes and built-in roles.
//!
//! A permission is a stable string code such as `users.manage`. Roles are per-school rows in
//! the database; the built-in roles below are created at setup with the default grants in
//! [`RoleCode::default_permissions`]. The backend checks permissions on every endpoint.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::str::FromStr;

use crate::edition::Edition;

/// All permission codes. Keep the list sorted by module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    // School / settings
    #[serde(rename = "school.read")]
    SchoolRead,
    #[serde(rename = "school.manage")]
    SchoolManage,
    #[serde(rename = "settings.manage")]
    SettingsManage,
    // Users and roles
    #[serde(rename = "users.read")]
    UsersRead,
    #[serde(rename = "users.manage")]
    UsersManage,
    #[serde(rename = "roles.manage")]
    RolesManage,
    // Audit
    #[serde(rename = "audit.read")]
    AuditRead,
    // Academic structure (years, classes, sections, subjects)
    #[serde(rename = "academics.read")]
    AcademicsRead,
    #[serde(rename = "academics.manage")]
    AcademicsManage,
    // Modules (M1+ will add finer grains)
    #[serde(rename = "students.read")]
    StudentsRead,
    #[serde(rename = "students.manage")]
    StudentsManage,
    #[serde(rename = "attendance.read")]
    AttendanceRead,
    #[serde(rename = "attendance.manage")]
    AttendanceManage,
    #[serde(rename = "exams.read")]
    ExamsRead,
    #[serde(rename = "exams.manage")]
    ExamsManage,
    #[serde(rename = "fees.read")]
    FeesRead,
    #[serde(rename = "fees.manage")]
    FeesManage,
    #[serde(rename = "teachers.read")]
    TeachersRead,
    #[serde(rename = "teachers.manage")]
    TeachersManage,
    #[serde(rename = "reports.read")]
    ReportsRead,
    #[serde(rename = "backup.manage")]
    BackupManage,
    // Nexus
    #[serde(rename = "nexus.admin")]
    NexusAdmin,
}

impl Permission {
    pub const ALL: &'static [Permission] = &[
        Permission::SchoolRead,
        Permission::SchoolManage,
        Permission::SettingsManage,
        Permission::UsersRead,
        Permission::UsersManage,
        Permission::RolesManage,
        Permission::AuditRead,
        Permission::AcademicsRead,
        Permission::AcademicsManage,
        Permission::StudentsRead,
        Permission::StudentsManage,
        Permission::AttendanceRead,
        Permission::AttendanceManage,
        Permission::ExamsRead,
        Permission::ExamsManage,
        Permission::FeesRead,
        Permission::FeesManage,
        Permission::TeachersRead,
        Permission::TeachersManage,
        Permission::ReportsRead,
        Permission::BackupManage,
        Permission::NexusAdmin,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Permission::SchoolRead => "school.read",
            Permission::SchoolManage => "school.manage",
            Permission::SettingsManage => "settings.manage",
            Permission::UsersRead => "users.read",
            Permission::UsersManage => "users.manage",
            Permission::RolesManage => "roles.manage",
            Permission::AuditRead => "audit.read",
            Permission::AcademicsRead => "academics.read",
            Permission::AcademicsManage => "academics.manage",
            Permission::StudentsRead => "students.read",
            Permission::StudentsManage => "students.manage",
            Permission::AttendanceRead => "attendance.read",
            Permission::AttendanceManage => "attendance.manage",
            Permission::ExamsRead => "exams.read",
            Permission::ExamsManage => "exams.manage",
            Permission::FeesRead => "fees.read",
            Permission::FeesManage => "fees.manage",
            Permission::TeachersRead => "teachers.read",
            Permission::TeachersManage => "teachers.manage",
            Permission::ReportsRead => "reports.read",
            Permission::BackupManage => "backup.manage",
            Permission::NexusAdmin => "nexus.admin",
        }
    }
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown permission code: {0}")]
pub struct UnknownPermission(pub String);

impl FromStr for Permission {
    type Err = UnknownPermission;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Permission::ALL
            .iter()
            .copied()
            .find(|p| p.code() == s)
            .ok_or_else(|| UnknownPermission(s.to_string()))
    }
}

/// Built-in role codes. `Parent` is reserved for Nexus (parent accounts span schools).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoleCode {
    Principal,
    Admin,
    Accountant,
    Teacher,
    Parent,
}

impl RoleCode {
    pub const ALL: &'static [RoleCode] = &[
        RoleCode::Principal,
        RoleCode::Admin,
        RoleCode::Accountant,
        RoleCode::Teacher,
        RoleCode::Parent,
    ];

    pub fn code(self) -> &'static str {
        match self {
            RoleCode::Principal => "principal",
            RoleCode::Admin => "admin",
            RoleCode::Accountant => "accountant",
            RoleCode::Teacher => "teacher",
            RoleCode::Parent => "parent",
        }
    }

    /// English display name; the UI translates by code.
    pub fn display_name(self) -> &'static str {
        match self {
            RoleCode::Principal => "Principal",
            RoleCode::Admin => "Administrator",
            RoleCode::Accountant => "Accountant",
            RoleCode::Teacher => "Teacher",
            RoleCode::Parent => "Parent",
        }
    }

    /// Roles created at setup for the given edition.
    pub fn defaults_for(edition: Edition) -> Vec<RoleCode> {
        RoleCode::ALL
            .iter()
            .copied()
            .filter(|r| r.available_in(edition))
            .collect()
    }

    pub fn available_in(self, edition: Edition) -> bool {
        match self {
            RoleCode::Parent => edition == Edition::Nexus,
            _ => true,
        }
    }

    /// Default permission grants for a built-in role.
    pub fn default_permissions(self) -> Vec<Permission> {
        use Permission::*;
        match self {
            RoleCode::Principal => Permission::ALL
                .iter()
                .copied()
                .filter(|p| *p != NexusAdmin)
                .collect(),
            RoleCode::Admin => vec![
                SchoolRead,
                SchoolManage,
                SettingsManage,
                UsersRead,
                UsersManage,
                RolesManage,
                AuditRead,
                AcademicsRead,
                AcademicsManage,
                StudentsRead,
                StudentsManage,
                AttendanceRead,
                AttendanceManage,
                ExamsRead,
                ExamsManage,
                FeesRead,
                TeachersRead,
                TeachersManage,
                ReportsRead,
                BackupManage,
            ],
            RoleCode::Accountant => {
                vec![
                    SchoolRead,
                    AcademicsRead,
                    StudentsRead,
                    FeesRead,
                    FeesManage,
                    ReportsRead,
                ]
            }
            RoleCode::Teacher => vec![
                SchoolRead,
                AcademicsRead,
                StudentsRead,
                AttendanceRead,
                AttendanceManage,
                ExamsRead,
                ExamsManage,
            ],
            RoleCode::Parent => vec![SchoolRead],
        }
    }
}

impl fmt::Display for RoleCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl FromStr for RoleCode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        RoleCode::ALL
            .iter()
            .copied()
            .find(|r| r.code() == s)
            .ok_or_else(|| format!("unknown role code: {s}"))
    }
}

/// The effective permission set of a user: the union of their roles' grants.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PermissionSet(HashSet<Permission>);

impl PermissionSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_codes<'a>(codes: impl IntoIterator<Item = &'a str>) -> Self {
        Self(codes.into_iter().filter_map(|c| c.parse().ok()).collect())
    }

    pub fn insert(&mut self, p: Permission) {
        self.0.insert(p);
    }

    pub fn contains(&self, p: Permission) -> bool {
        self.0.contains(&p)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Sorted codes, for API responses.
    pub fn codes(&self) -> Vec<&'static str> {
        let mut v: Vec<_> = self.0.iter().map(|p| p.code()).collect();
        v.sort_unstable();
        v
    }
}

impl FromIterator<Permission> for PermissionSet {
    fn from_iter<T: IntoIterator<Item = Permission>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_codes_round_trip() {
        for p in Permission::ALL {
            assert_eq!(p.code().parse::<Permission>().unwrap(), *p);
            assert_eq!(
                serde_json::to_string(p).unwrap(),
                format!("\"{}\"", p.code())
            );
        }
        assert!("students.fly".parse::<Permission>().is_err());
    }

    #[test]
    fn codes_are_unique() {
        let set: HashSet<_> = Permission::ALL.iter().map(|p| p.code()).collect();
        assert_eq!(set.len(), Permission::ALL.len());
    }

    #[test]
    fn teacher_cannot_manage_users_but_admin_can() {
        let teacher = RoleCode::Teacher.default_permissions();
        assert!(!teacher.contains(&Permission::UsersManage));
        assert!(!teacher.contains(&Permission::StudentsManage));
        assert!(!teacher.contains(&Permission::AcademicsManage));
        assert!(teacher.contains(&Permission::AcademicsRead));
        assert!(!teacher.contains(&Permission::FeesManage));
        assert!(teacher.contains(&Permission::AttendanceManage));
        let admin = RoleCode::Admin.default_permissions();
        assert!(admin.contains(&Permission::UsersManage));
    }

    #[test]
    fn principal_has_everything_except_nexus_admin() {
        let p = RoleCode::Principal.default_permissions();
        assert_eq!(p.len(), Permission::ALL.len() - 1);
        assert!(!p.contains(&Permission::NexusAdmin));
    }

    #[test]
    fn parent_role_is_reserved_for_nexus() {
        assert_eq!(
            RoleCode::defaults_for(Edition::Lite),
            vec![
                RoleCode::Principal,
                RoleCode::Admin,
                RoleCode::Accountant,
                RoleCode::Teacher
            ]
        );
        assert!(RoleCode::defaults_for(Edition::Nexus).contains(&RoleCode::Parent));
        assert!(!RoleCode::Parent.available_in(Edition::Standard));
    }

    #[test]
    fn permission_set_unions_roles() {
        let set: PermissionSet = RoleCode::Teacher
            .default_permissions()
            .into_iter()
            .chain(RoleCode::Accountant.default_permissions())
            .collect();
        assert!(set.contains(Permission::FeesManage));
        assert!(set.contains(Permission::ExamsManage));
        assert!(!set.contains(Permission::UsersManage));
        let from_codes = PermissionSet::from_codes(["fees.read", "bogus"]);
        assert_eq!(from_codes.codes(), vec!["fees.read"]);
    }
}
