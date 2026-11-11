//! Hub configuration, read from environment variables only (no config file, no secrets on
//! disk). Values are never logged.

use std::net::SocketAddr;

/// Default port of the hub.
pub const DEFAULT_PORT: u16 = 4780;

#[derive(Clone)]
pub struct Config {
    /// PostgreSQL URL of the hub database. Contains the password: never log it.
    pub database_url: String,
    pub bind: SocketAddr,
    /// The address schools and browsers use, shown in invitations (`https://hub.example.org`).
    pub public_url: Option<String>,
    /// One-time token for first-run setup. When unset, one is generated and printed once.
    pub setup_token: Option<String>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("NEXUS_DATABASE_URL is not set. Point it at the hub's PostgreSQL database.")]
    MissingDatabaseUrl,
    #[error("NEXUS_BIND is not a valid address (expected something like 0.0.0.0:4780).")]
    BadBind,
    #[error("PORT is not a valid port number.")]
    BadPort,
    #[error("NEXUS_PUBLIC_URL must start with https:// (or http:// for a local test).")]
    BadPublicUrl,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    /// Build from any key lookup (tests pass a map).
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let var = |k: &str| {
            get(k)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let database_url = var("NEXUS_DATABASE_URL").ok_or(ConfigError::MissingDatabaseUrl)?;
        // `NEXUS_BIND` wins; otherwise `PORT` (set by Cloud Run and Container Apps) on every
        // interface; otherwise the default port on every interface (the hub runs in a container).
        let bind = match (var("NEXUS_BIND"), var("PORT")) {
            (Some(b), _) => b.parse().map_err(|_| ConfigError::BadBind)?,
            (None, Some(p)) => {
                let port: u16 = p.parse().map_err(|_| ConfigError::BadPort)?;
                SocketAddr::from(([0, 0, 0, 0], port))
            }
            (None, None) => SocketAddr::from(([0, 0, 0, 0], DEFAULT_PORT)),
        };
        let public_url = match var("NEXUS_PUBLIC_URL") {
            Some(u) if u.starts_with("https://") || u.starts_with("http://") => {
                Some(u.trim_end_matches('/').to_string())
            }
            Some(_) => return Err(ConfigError::BadPublicUrl),
            None => None,
        };
        Ok(Self {
            database_url,
            bind,
            public_url,
            setup_token: var("NEXUS_SETUP_TOKEN"),
        })
    }
}

// The database URL and the setup token must never reach a log line through `{:?}`.
impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("database_url", &"<redacted>")
            .field("bind", &self.bind)
            .field("public_url", &self.public_url)
            .field(
                "setup_token",
                &self.setup_token.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn cfg(pairs: &[(&str, &str)]) -> Result<Config, ConfigError> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Config::from_lookup(|k| map.get(k).cloned())
    }

    #[test]
    fn database_url_is_required() {
        assert_eq!(cfg(&[]).unwrap_err(), ConfigError::MissingDatabaseUrl);
        assert_eq!(
            cfg(&[("NEXUS_DATABASE_URL", "  ")]).unwrap_err(),
            ConfigError::MissingDatabaseUrl
        );
    }

    #[test]
    fn bind_prefers_nexus_bind_then_port_then_default() {
        let db = ("NEXUS_DATABASE_URL", "postgres://x");
        assert_eq!(cfg(&[db]).unwrap().bind.to_string(), "0.0.0.0:4780");
        assert_eq!(
            cfg(&[db, ("PORT", "8080")]).unwrap().bind.to_string(),
            "0.0.0.0:8080"
        );
        assert_eq!(
            cfg(&[db, ("PORT", "8080"), ("NEXUS_BIND", "127.0.0.1:9000")])
                .unwrap()
                .bind
                .to_string(),
            "127.0.0.1:9000"
        );
        assert_eq!(
            cfg(&[db, ("NEXUS_BIND", "nonsense")]).unwrap_err(),
            ConfigError::BadBind
        );
        assert_eq!(cfg(&[db, ("PORT", "x")]).unwrap_err(), ConfigError::BadPort);
    }

    #[test]
    fn public_url_is_checked_and_trimmed() {
        let db = ("NEXUS_DATABASE_URL", "postgres://x");
        assert_eq!(
            cfg(&[db, ("NEXUS_PUBLIC_URL", "https://hub.example.org/")])
                .unwrap()
                .public_url
                .as_deref(),
            Some("https://hub.example.org")
        );
        assert_eq!(
            cfg(&[db, ("NEXUS_PUBLIC_URL", "hub.example.org")]).unwrap_err(),
            ConfigError::BadPublicUrl
        );
    }

    #[test]
    fn debug_output_hides_secrets() {
        let c = cfg(&[
            ("NEXUS_DATABASE_URL", "postgres://user:hunter2@db/hub"),
            ("NEXUS_SETUP_TOKEN", "letmein-token"),
        ])
        .unwrap();
        let text = format!("{c:?}");
        assert!(!text.contains("hunter2"), "{text}");
        assert!(!text.contains("letmein"), "{text}");
    }
}
