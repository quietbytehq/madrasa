//! Shared hub state (cheap to clone).

use madrasa_core::hlc::Hlc;
use madrasa_core::password::LockoutPolicy;
use madrasa_core::session_token;
use sqlx::PgPool;
use std::sync::{Arc, RwLock};

use crate::config::Config;
use crate::store;

/// The version the hub reports: the workspace version, or the release tag when built by the
/// Release workflow (same rule as the school API).
pub const VERSION: &str = match option_env!("MADRASA_RELEASE") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};

#[derive(Clone)]
pub struct HubState {
    pub pool: PgPool,
    pub hlc: Arc<Hlc>,
    pub lockout: LockoutPolicy,
    pub version: &'static str,
    pub public_url: Option<String>,
    /// Folder of the built web console, when there is one.
    pub console_dir: Option<std::path::PathBuf>,
    /// SHA-256 of the one-time setup token while first-run setup is still open.
    setup_token_hash: Arc<RwLock<Option<String>>>,
}

impl HubState {
    /// Build state from a migrated hub pool.
    ///
    /// While the hub has no users, first-run setup is open and guarded by a one-time token:
    /// `NEXUS_SETUP_TOKEN` when set, otherwise a generated one returned here so that `main`
    /// can print it once. The token is kept only as a hash.
    pub async fn new(pool: PgPool, config: &Config) -> anyhow::Result<(Self, Option<String>)> {
        let mut conn = pool.acquire().await?;
        let node = store::node_id(&mut conn).await?;
        let needs_setup = store::count_users(&mut conn).await? == 0;
        drop(conn);
        let (hash, generated) = match (needs_setup, &config.setup_token) {
            (false, _) => (None, None),
            (true, Some(t)) => (Some(session_token::hash(t)), None),
            (true, None) => {
                let t = session_token::generate().token;
                (Some(session_token::hash(&t)), Some(t))
            }
        };
        Ok((
            Self {
                pool,
                hlc: Arc::new(Hlc::new(node)),
                lockout: LockoutPolicy::default(),
                version: VERSION,
                public_url: config.public_url.clone(),
                console_dir: config.console_dir.clone(),
                setup_token_hash: Arc::new(RwLock::new(hash)),
            },
            generated,
        ))
    }

    /// Does this token open first-run setup? Compared by hash.
    pub fn setup_token_matches(&self, token: &str) -> bool {
        let guard = self
            .setup_token_hash
            .read()
            .unwrap_or_else(|e| e.into_inner());
        match guard.as_deref() {
            Some(expected) => session_token::hash(token.trim()) == expected,
            None => false,
        }
    }

    /// Setup is done: the token stops working for good.
    pub fn close_setup(&self) {
        *self
            .setup_token_hash
            .write()
            .unwrap_or_else(|e| e.into_inner()) = None;
    }
}
