//! Talking to the directory (Standard stage 8): find a user with the bind account, then bind
//! as that user to check the password. `LdapDirectory` speaks LDAP over TLS (`ldap3`);
//! `StubDirectory` holds a fixed set of users for tests and demos.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use madrasa_core::directory::{account_disabled, DirectoryConfig, DirectoryUser};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DirectoryError {
    #[error("the directory did not accept the bind account: {0}")]
    BindAccount(String),
    #[error("cannot reach the directory: {0}")]
    Connect(String),
    /// The directory was reached but its TLS certificate was refused. The message says what
    /// to do; `detail` is the TLS library's own text, for whoever looks after the directory.
    #[error("{}", .problem.message())]
    Certificate {
        problem: CertificateProblem,
        detail: String,
    },
    #[error("no such user in the directory")]
    NotFound,
    #[error("the password is wrong")]
    BadPassword,
}

/// Why the directory's certificate was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertificateProblem {
    /// Issued by an authority this PC does not trust (often the organisation's own CA).
    Untrusted,
    Expired,
    /// Valid only from a later date: usually this PC's clock is wrong.
    NotYetValid,
    /// Issued for another name than the server address entered.
    WrongName,
    /// Any other reason the TLS library gave.
    Other,
}

impl CertificateProblem {
    pub fn message(self) -> &'static str {
        match self {
            CertificateProblem::Untrusted => "The directory's certificate is not trusted by the Madrasa server PC: it was issued by a certificate authority (CA) this PC does not know, often the organisation's own. Install that CA's certificate in the server PC's trusted root certificates (Windows: \"Trusted Root Certification Authorities\" of the computer; Linux: /usr/local/share/ca-certificates, then update-ca-certificates), restart Madrasa, and test again.",
            CertificateProblem::Expired => "The directory's certificate has expired. Ask whoever runs the directory to renew it, then test again.",
            CertificateProblem::NotYetValid => "The directory's certificate is not valid yet. Check the date and time of the Madrasa server PC.",
            CertificateProblem::WrongName => "The directory's certificate was issued for another name than the server address entered. Enter the name the certificate was issued for (usually the server's full DNS name, not its IP address).",
            CertificateProblem::Other => "The directory's certificate was refused. Ask whoever runs the directory to check it; the details are below.",
        }
    }
}

/// Classify a connection failure. TLS certificate problems get their own plain message;
/// anything else is "cannot reach the directory". The texts matched are those of the TLS
/// library (rustls) the LDAP client uses, in both its short and its detailed forms.
pub fn connect_error(e: impl std::fmt::Display) -> DirectoryError {
    let detail = e.to_string();
    let text = detail.to_ascii_lowercase();
    let problem = if text.contains("unknownissuer") || text.contains("unknown issuer") {
        Some(CertificateProblem::Untrusted)
    } else if text.contains("certificate expired")
        || text.contains("certificate: expired")
        || text.contains("certificateexpired")
    {
        Some(CertificateProblem::Expired)
    } else if text.contains("not valid yet") || text.contains("notvalidyet") {
        Some(CertificateProblem::NotYetValid)
    } else if text.contains("not valid for name") || text.contains("notvalidforname") {
        Some(CertificateProblem::WrongName)
    } else if text.contains("invalid peer certificate") || text.contains("badcertificate") {
        Some(CertificateProblem::Other)
    } else {
        None
    };
    match problem {
        Some(problem) => DirectoryError::Certificate { problem, detail },
        None => DirectoryError::Connect(detail),
    }
}

#[async_trait]
pub trait DirectoryClient: Send + Sync {
    /// Bind with the service account only (Settings → Test connection).
    async fn test_bind(
        &self,
        cfg: &DirectoryConfig,
        bind_password: &str,
    ) -> Result<(), DirectoryError>;
    /// Find the user and read their attributes (no password check).
    async fn lookup(
        &self,
        cfg: &DirectoryConfig,
        bind_password: &str,
        username: &str,
    ) -> Result<DirectoryUser, DirectoryError>;
    /// Find the user, then bind as them with the given password.
    async fn authenticate(
        &self,
        cfg: &DirectoryConfig,
        bind_password: &str,
        username: &str,
        password: &str,
    ) -> Result<DirectoryUser, DirectoryError>;
}

pub type SharedDirectory = Arc<dyn DirectoryClient>;

// ---- LDAP -----------------------------------------------------------------------------------

/// The real thing: LDAP over TLS with the system's trusted roots.
#[derive(Debug, Default)]
pub struct LdapDirectory;

impl LdapDirectory {
    fn settings() -> ldap3::LdapConnSettings {
        ldap3::LdapConnSettings::new().set_conn_timeout(Duration::from_secs(10))
    }

    async fn connect(url: &str) -> Result<ldap3::Ldap, DirectoryError> {
        let (conn, ldap) = ldap3::LdapConnAsync::with_settings(Self::settings(), url.trim())
            .await
            .map_err(connect_error)?;
        ldap3::drive!(conn);
        Ok(ldap)
    }

    async fn search(
        cfg: &DirectoryConfig,
        bind_password: &str,
        username: &str,
    ) -> Result<DirectoryUser, DirectoryError> {
        let mut ldap = Self::connect(&cfg.url).await?;
        ldap.simple_bind(cfg.bind_dn.trim(), bind_password)
            .await
            .map_err(connect_error)?
            .success()
            .map_err(|e| DirectoryError::BindAccount(e.to_string()))?;
        let attrs = vec![
            cfg.username_attr.clone(),
            cfg.name_attr.clone(),
            cfg.email_attr.clone(),
            cfg.group_attr.clone(),
            "userAccountControl".to_string(),
        ];
        let (rows, _) = ldap
            .search(
                cfg.user_base_dn.trim(),
                ldap3::Scope::Subtree,
                &cfg.filter_for(username),
                attrs,
            )
            .await
            .map_err(connect_error)?
            .success()
            .map_err(connect_error)?;
        let _ = ldap.unbind().await;
        let entry = rows
            .into_iter()
            .next()
            .map(ldap3::SearchEntry::construct)
            .ok_or(DirectoryError::NotFound)?;
        let first = |a: &str| entry.attrs.get(a).and_then(|v| v.first()).cloned();
        Ok(DirectoryUser {
            dn: entry.dn.clone(),
            username: first(&cfg.username_attr).unwrap_or_else(|| username.to_string()),
            full_name: first(&cfg.name_attr).unwrap_or_else(|| username.to_string()),
            email: first(&cfg.email_attr).filter(|e| !e.is_empty()),
            groups: entry
                .attrs
                .get(&cfg.group_attr)
                .cloned()
                .unwrap_or_default(),
            disabled: account_disabled(first("userAccountControl").as_deref()),
        })
    }
}

#[async_trait]
impl DirectoryClient for LdapDirectory {
    async fn test_bind(
        &self,
        cfg: &DirectoryConfig,
        bind_password: &str,
    ) -> Result<(), DirectoryError> {
        let mut ldap = Self::connect(&cfg.url).await?;
        let r = ldap
            .simple_bind(cfg.bind_dn.trim(), bind_password)
            .await
            .map_err(connect_error)?
            .success()
            .map(|_| ())
            .map_err(|e| DirectoryError::BindAccount(e.to_string()));
        let _ = ldap.unbind().await;
        r
    }

    async fn lookup(
        &self,
        cfg: &DirectoryConfig,
        bind_password: &str,
        username: &str,
    ) -> Result<DirectoryUser, DirectoryError> {
        Self::search(cfg, bind_password, username).await
    }

    async fn authenticate(
        &self,
        cfg: &DirectoryConfig,
        bind_password: &str,
        username: &str,
        password: &str,
    ) -> Result<DirectoryUser, DirectoryError> {
        if password.is_empty() {
            // An empty password would be an anonymous bind, which some servers accept.
            return Err(DirectoryError::BadPassword);
        }
        let user = Self::search(cfg, bind_password, username).await?;
        let mut ldap = Self::connect(&cfg.url).await?;
        let r = ldap
            .simple_bind(&user.dn, password)
            .await
            .map_err(connect_error)?
            .success()
            .map_err(|_| DirectoryError::BadPassword);
        let _ = ldap.unbind().await;
        r?;
        Ok(user)
    }
}

// ---- stub -----------------------------------------------------------------------------------

/// A user the stub knows.
#[derive(Debug, Clone)]
pub struct StubUser {
    pub password: String,
    pub user: DirectoryUser,
}

/// A fixed directory for tests: the bind account must match, users are looked up by
/// username (case-insensitive) and checked against their stub password.
#[derive(Debug, Default)]
pub struct StubDirectory {
    pub bind_dn: String,
    pub bind_password: String,
    pub users: HashMap<String, StubUser>,
}

impl StubDirectory {
    pub fn new(bind_dn: &str, bind_password: &str) -> Self {
        StubDirectory {
            bind_dn: bind_dn.into(),
            bind_password: bind_password.into(),
            users: HashMap::new(),
        }
    }

    pub fn with_user(mut self, password: &str, user: DirectoryUser) -> Self {
        self.users.insert(
            user.username.to_lowercase(),
            StubUser {
                password: password.into(),
                user,
            },
        );
        self
    }

    fn check_bind(&self, cfg: &DirectoryConfig, bind_password: &str) -> Result<(), DirectoryError> {
        if !cfg.url.starts_with("ldaps://") {
            return Err(DirectoryError::Connect("TLS is required".into()));
        }
        if cfg.bind_dn.trim().eq_ignore_ascii_case(&self.bind_dn)
            && bind_password == self.bind_password
        {
            Ok(())
        } else {
            Err(DirectoryError::BindAccount("invalid credentials".into()))
        }
    }
}

#[async_trait]
impl DirectoryClient for StubDirectory {
    async fn test_bind(
        &self,
        cfg: &DirectoryConfig,
        bind_password: &str,
    ) -> Result<(), DirectoryError> {
        self.check_bind(cfg, bind_password)
    }

    async fn lookup(
        &self,
        cfg: &DirectoryConfig,
        bind_password: &str,
        username: &str,
    ) -> Result<DirectoryUser, DirectoryError> {
        self.check_bind(cfg, bind_password)?;
        self.users
            .get(&username.trim().to_lowercase())
            .map(|u| u.user.clone())
            .ok_or(DirectoryError::NotFound)
    }

    async fn authenticate(
        &self,
        cfg: &DirectoryConfig,
        bind_password: &str,
        username: &str,
        password: &str,
    ) -> Result<DirectoryUser, DirectoryError> {
        self.check_bind(cfg, bind_password)?;
        let u = self
            .users
            .get(&username.trim().to_lowercase())
            .ok_or(DirectoryError::NotFound)?;
        if password.is_empty() || password != u.password {
            return Err(DirectoryError::BadPassword);
        }
        Ok(u.user.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn certificate_problems_get_their_own_plain_message() {
        let cases = [
            ("invalid peer certificate: UnknownIssuer", CertificateProblem::Untrusted),
            (
                "io error: invalid peer certificate: certificate expired: verification time 1763020800 (UNIX), but certificate is not valid after 1730000000 (33020800 seconds ago)",
                CertificateProblem::Expired,
            ),
            ("invalid peer certificate: Expired", CertificateProblem::Expired),
            ("invalid peer certificate: NotValidYet", CertificateProblem::NotYetValid),
            (
                "invalid peer certificate: certificate not valid for name \"10.0.0.5\"; certificate is only valid for dc1.school.local",
                CertificateProblem::WrongName,
            ),
            ("invalid peer certificate: BadSignature", CertificateProblem::Other),
        ];
        for (text, want) in cases {
            match connect_error(text) {
                DirectoryError::Certificate { problem, detail } => {
                    assert_eq!(problem, want, "{text}");
                    assert_eq!(detail, text);
                }
                other => panic!("{text}: {other:?}"),
            }
        }
        let untrusted = connect_error("invalid peer certificate: UnknownIssuer").to_string();
        assert!(
            untrusted.contains("not trusted by the Madrasa server PC"),
            "{untrusted}"
        );
        assert!(untrusted.contains("restart Madrasa"), "{untrusted}");
        assert!(connect_error("invalid peer certificate: Expired")
            .to_string()
            .contains("has expired"));
    }

    #[test]
    fn other_failures_stay_connection_failures() {
        for text in ["connection refused", "timed out", "dns error: no such host"] {
            assert_eq!(connect_error(text), DirectoryError::Connect(text.into()));
        }
    }
}
