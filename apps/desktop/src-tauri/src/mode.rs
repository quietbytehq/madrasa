//! How this PC takes part: on its own (as before), as the school server on the LAN, or as a
//! client of a school server. Chosen on first start and kept in `<data_dir>/mode.json`.

use madrasa_api::lan;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Database and API on this PC, used only here.
    Standalone,
    /// Database and API on this PC, also served to the LAN over TLS.
    Server,
    /// No database here; the UI talks to a school server through a pinned loopback proxy.
    Client,
}

/// Mirrored by `ModeConfig` in the UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModeConfig {
    pub mode: Mode,
    /// Client: the server, `https://host:port`.
    #[serde(default)]
    pub server_url: Option<String>,
    /// Client: the server certificate's SHA-256 fingerprint, pinned on first connect.
    #[serde(default)]
    pub fingerprint: Option<String>,
    /// Server: the TLS port on the LAN.
    #[serde(default = "default_port")]
    pub listen_port: u16,
}

fn default_port() -> u16 {
    lan::DEFAULT_PORT
}

impl ModeConfig {
    pub fn path(data_dir: &Path) -> PathBuf {
        data_dir.join("mode.json")
    }

    /// `None` until the user has chosen on this PC.
    pub fn load(data_dir: &Path) -> Option<ModeConfig> {
        let text = std::fs::read_to_string(Self::path(data_dir)).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// The mode this PC runs in: the saved choice, or standalone on an install that already
    /// has a database from before modes existed (an upgrade keeps working without a question;
    /// Settings → Network changes it). `None` only on a fresh install.
    pub fn current(data_dir: &Path) -> Option<ModeConfig> {
        Self::load(data_dir).or_else(|| {
            data_dir.join("pg").is_dir().then_some(ModeConfig {
                mode: Mode::Standalone,
                server_url: None,
                fingerprint: None,
                listen_port: lan::DEFAULT_PORT,
            })
        })
    }

    pub fn save(&self, data_dir: &Path) -> anyhow::Result<()> {
        self.validate()?;
        std::fs::create_dir_all(data_dir)?;
        std::fs::write(Self::path(data_dir), serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        match self.mode {
            Mode::Client => {
                let url = self
                    .server_url
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("The server address is missing."))?;
                lan::normalise_url(url)?;
                if self
                    .fingerprint
                    .as_deref()
                    .map(|f| f.chars().filter(|c| c.is_ascii_hexdigit()).count())
                    .unwrap_or(0)
                    != 64
                {
                    anyhow::bail!("The server's certificate fingerprint is missing.");
                }
            }
            Mode::Server => {
                if self.listen_port == 0 {
                    anyhow::bail!("Choose a port for the school server.");
                }
            }
            Mode::Standalone => {}
        }
        Ok(())
    }
}

/// What the server shows so clients can find it and check the fingerprint. Mirrored by
/// `NetworkInfo` in the UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkInfo {
    pub mode: Mode,
    pub addresses: Vec<String>,
    pub port: u16,
    pub fingerprint: Option<String>,
    /// Client: the server it is connected to.
    pub server_url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trips_and_validates() {
        let dir = std::env::temp_dir().join(format!("madrasa-mode-{}", std::process::id()));
        assert!(ModeConfig::load(&dir).is_none());
        let server = ModeConfig {
            mode: Mode::Server,
            server_url: None,
            fingerprint: None,
            listen_port: 8471,
        };
        server.save(&dir).unwrap();
        assert_eq!(ModeConfig::load(&dir).unwrap(), server);
        assert_eq!(ModeConfig::current(&dir).unwrap(), server);
        let bad_client = ModeConfig {
            mode: Mode::Client,
            server_url: Some("192.168.1.10".into()),
            fingerprint: Some("AB:CD".into()),
            listen_port: 8471,
        };
        assert!(bad_client.validate().is_err(), "fingerprint too short");
        let client = ModeConfig {
            fingerprint: Some("AB".repeat(32)),
            ..bad_client
        };
        assert!(client.validate().is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_upgraded_install_with_a_database_is_standalone_until_changed() {
        let dir = std::env::temp_dir().join(format!("madrasa-upgrade-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(ModeConfig::current(&dir), None, "fresh install asks");
        std::fs::create_dir_all(dir.join("pg")).unwrap();
        let current = ModeConfig::current(&dir).expect("existing database");
        assert_eq!(current.mode, Mode::Standalone);
        assert_eq!(
            ModeConfig::load(&dir),
            None,
            "nothing is written until chosen"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
