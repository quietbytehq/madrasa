//! Directory (Active Directory / LDAP) login rules (Standard stage 8): the settings and
//! their validation, LDAP filter escaping, and the group-to-role mapping. Pure; the API
//! talks to the directory.

use serde::{Deserialize, Serialize};

/// One directory group that grants a built-in role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupRole {
    /// The group's distinguished name, e.g. `CN=Teachers,OU=Groups,DC=school,DC=local`.
    pub group: String,
    /// A `RoleCode` code: principal, admin, accountant, teacher, librarian, storekeeper.
    pub role: String,
}

/// Directory settings as stored (the bind password lives in its own sealed setting).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DirectoryConfig {
    pub enabled: bool,
    /// `ldaps://host:636` (TLS is required).
    pub url: String,
    /// The account that searches for users, e.g. `CN=madrasa,OU=Service,DC=school,DC=local`.
    pub bind_dn: String,
    pub user_base_dn: String,
    /// LDAP filter with `{username}` where the typed username goes.
    pub user_filter: String,
    pub username_attr: String,
    pub name_attr: String,
    pub email_attr: String,
    pub group_attr: String,
    pub group_roles: Vec<GroupRole>,
    /// Role for users in no mapped group; empty = such users cannot log in.
    pub default_role: String,
    /// A hint shown on the login screen.
    pub login_hint: String,
}

impl Default for DirectoryConfig {
    fn default() -> Self {
        DirectoryConfig {
            enabled: false,
            url: String::new(),
            bind_dn: String::new(),
            user_base_dn: String::new(),
            user_filter: "(&(objectClass=user)(sAMAccountName={username}))".into(),
            username_attr: "sAMAccountName".into(),
            name_attr: "displayName".into(),
            email_attr: "mail".into(),
            group_attr: "memberOf".into(),
            group_roles: Vec::new(),
            default_role: String::new(),
            login_hint: String::new(),
        }
    }
}

impl DirectoryConfig {
    /// Everything a login needs; checked whenever the settings are saved with `enabled`.
    pub fn validate(&self) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }
        let url = self.url.trim();
        if !url.starts_with("ldaps://") || url.len() <= "ldaps://".len() {
            return Err("The server URL must start with ldaps:// (TLS is required).".into());
        }
        if self.bind_dn.trim().is_empty() {
            return Err("The bind account is required.".into());
        }
        if self.user_base_dn.trim().is_empty() {
            return Err("The user base DN is required.".into());
        }
        if !self.user_filter.contains("{username}") {
            return Err("The user filter must contain {username}.".into());
        }
        for attr in [&self.username_attr, &self.name_attr, &self.group_attr] {
            if attr.trim().is_empty() {
                return Err("The attribute names are required.".into());
            }
        }
        for m in &self.group_roles {
            if m.group.trim().is_empty() || m.role.trim().is_empty() {
                return Err("Every group mapping needs a group and a role.".into());
            }
        }
        Ok(())
    }

    /// The role of a user with these groups: the first mapping whose group matches (DNs
    /// compare case-insensitively), else the default role, else none.
    pub fn role_for(&self, groups: &[String]) -> Option<String> {
        let lower: Vec<String> = groups.iter().map(|g| g.trim().to_lowercase()).collect();
        self.group_roles
            .iter()
            .find(|m| lower.contains(&m.group.trim().to_lowercase()))
            .map(|m| m.role.trim().to_lowercase())
            .or_else(|| {
                let d = self.default_role.trim().to_lowercase();
                (!d.is_empty()).then_some(d)
            })
    }

    /// The search filter for a username, escaped per RFC 4515.
    pub fn filter_for(&self, username: &str) -> String {
        self.user_filter
            .replace("{username}", &escape_filter(username))
    }
}

/// Escape a value for use inside an LDAP search filter (RFC 4515).
pub fn escape_filter(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'*' => out.push_str("\\2a"),
            b'(' => out.push_str("\\28"),
            b')' => out.push_str("\\29"),
            b'\\' => out.push_str("\\5c"),
            0 => out.push_str("\\00"),
            b if !(0x20..0x7f).contains(&b) => out.push_str(&format!("\\{b:02x}")),
            b => out.push(b as char),
        }
    }
    out
}

/// What the directory says about a user who authenticated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryUser {
    pub dn: String,
    pub username: String,
    pub full_name: String,
    pub email: Option<String>,
    pub groups: Vec<String>,
    /// Active Directory `userAccountControl` bit 2 (ACCOUNTDISABLE), when the attribute
    /// is present.
    pub disabled: bool,
}

/// Active Directory's `userAccountControl` flag for a disabled account.
pub fn account_disabled(user_account_control: Option<&str>) -> bool {
    user_account_control
        .and_then(|v| v.trim().parse::<u64>().ok())
        .is_some_and(|v| v & 2 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> DirectoryConfig {
        DirectoryConfig {
            enabled: true,
            url: "ldaps://dc1.school.local:636".into(),
            bind_dn: "CN=madrasa,OU=Service,DC=school,DC=local".into(),
            user_base_dn: "OU=Staff,DC=school,DC=local".into(),
            group_roles: vec![
                GroupRole {
                    group: "CN=Principals,OU=Groups,DC=school,DC=local".into(),
                    role: "principal".into(),
                },
                GroupRole {
                    group: "CN=Teachers,OU=Groups,DC=school,DC=local".into(),
                    role: "teacher".into(),
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn validation_requires_tls_and_the_essentials() {
        assert!(cfg().validate().is_ok());
        let mut c = cfg();
        c.url = "ldap://dc1.school.local".into();
        assert!(c.validate().is_err());
        c.url = "ldaps://".into();
        assert!(c.validate().is_err());
        let mut c = cfg();
        c.user_filter = "(sAMAccountName=x)".into();
        assert!(c.validate().is_err());
        let mut c = cfg();
        c.enabled = false;
        c.url.clear();
        assert!(c.validate().is_ok(), "disabled settings need nothing");
    }

    #[test]
    fn groups_map_to_roles_in_order_then_the_default() {
        let c = cfg();
        let teacher = vec!["cn=teachers,ou=groups,dc=school,dc=local".to_string()];
        assert_eq!(c.role_for(&teacher), Some("teacher".into()));
        let both = vec![
            "CN=Teachers,OU=Groups,DC=school,DC=local".to_string(),
            "CN=Principals,OU=Groups,DC=school,DC=local".to_string(),
        ];
        assert_eq!(c.role_for(&both), Some("principal".into()));
        assert_eq!(c.role_for(&["CN=Other".to_string()]), None);
        let mut d = cfg();
        d.default_role = "Teacher".into();
        assert_eq!(d.role_for(&[]), Some("teacher".into()));
    }

    #[test]
    fn filters_are_escaped() {
        assert_eq!(escape_filter("a*b(c)d\\e"), "a\\2ab\\28c\\29d\\5ce");
        assert_eq!(
            cfg().filter_for("ali*"),
            "(&(objectClass=user)(sAMAccountName=ali\\2a))"
        );
        assert!(account_disabled(Some("514")));
        assert!(!account_disabled(Some("512")));
        assert!(!account_disabled(None));
    }
}
