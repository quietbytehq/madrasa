use madrasa_core::edition::{Edition, Feature};
use madrasa_core::hlc::Hlc;
use madrasa_core::password::LockoutPolicy;
use madrasa_db::{settings, PgPool};
use std::sync::{Arc, RwLock};

use crate::ApiError;

/// Shared application state (cheap to clone).
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub hlc: Arc<Hlc>,
    edition: Arc<RwLock<Edition>>,
    pub lockout: LockoutPolicy,
    pub version: &'static str,
}

impl AppState {
    /// Build state from a migrated pool: loads the node id and edition from settings.
    pub async fn new(pool: PgPool) -> anyhow::Result<Self> {
        let mut conn = pool.acquire().await?;
        let node = settings::node_id(&mut conn).await?;
        let edition = settings::edition(&mut conn).await?;
        drop(conn);
        Ok(Self {
            pool,
            hlc: Arc::new(Hlc::new(node)),
            edition: Arc::new(RwLock::new(edition)),
            lockout: LockoutPolicy::default(),
            version: env!("CARGO_PKG_VERSION"),
        })
    }

    pub fn edition(&self) -> Edition {
        *self.edition.read().unwrap_or_else(|e| e.into_inner())
    }

    /// Re-read the edition from settings (after it was changed).
    pub async fn reload_edition(&self) -> anyhow::Result<Edition> {
        let mut conn = self.pool.acquire().await?;
        let edition = settings::edition(&mut conn).await?;
        *self.edition.write().unwrap_or_else(|e| e.into_inner()) = edition;
        Ok(edition)
    }

    pub fn has_feature(&self, feature: Feature) -> bool {
        self.edition().enables(feature)
    }

    /// Fail with `403 feature_unavailable` unless the edition enables the feature.
    pub fn require_feature(&self, feature: Feature) -> Result<(), ApiError> {
        if self.has_feature(feature) {
            Ok(())
        } else {
            Err(ApiError::feature_unavailable(feature.as_str()))
        }
    }
}
