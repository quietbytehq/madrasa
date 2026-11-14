use madrasa_core::edition::{Edition, Feature};
use madrasa_core::hlc::Hlc;
use madrasa_core::locale;
use madrasa_core::password::LockoutPolicy;
use madrasa_db::{schools, settings, PgPool};
use std::sync::{Arc, RwLock};

use crate::directory::{LdapDirectory, SharedDirectory};
use crate::ro::RateLimits;
use crate::timetable_jobs::TimetableJobs;
use crate::{clock, ApiError};

/// The parts of the school profile the API needs on every request (validation rules, money
/// formatting, the calendar). Loaded at start and after the profile changes.
#[derive(Debug, Clone)]
pub struct SchoolProfile {
    /// ISO 3166-1 alpha-2.
    pub country: String,
    /// ISO 4217.
    pub currency: String,
    /// IANA time zone name.
    pub time_zone: String,
}

impl Default for SchoolProfile {
    fn default() -> Self {
        let c = locale::country_or_default(locale::DEFAULT_COUNTRY);
        Self {
            country: c.code.into(),
            currency: c.currency.into(),
            time_zone: c.time_zone.into(),
        }
    }
}

/// The version the API and the app report. A tagged release build sets `MADRASA_RELEASE`
/// (e.g. `0.1.3-alpha`, from the git tag) at compile time; otherwise the Cargo version, which
/// the Release workflow checks against the tag.
pub const VERSION: &str = match option_env!("MADRASA_RELEASE") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};

/// Shared application state (cheap to clone).
#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub hlc: Arc<Hlc>,
    edition: Arc<RwLock<Edition>>,
    profile: Arc<RwLock<SchoolProfile>>,
    pub lockout: LockoutPolicy,
    pub version: &'static str,
    /// Timetable generation runs in progress (Standard stage 1).
    pub timetable_jobs: Arc<TimetableJobs>,
    /// The directory login client (Standard stage 8); tests swap in a stub.
    pub directory: SharedDirectory,
    /// Per-key rate windows of the read-only API (Standard stage 9).
    pub ro_limits: Arc<RateLimits>,
    /// Held while a Nexus sync round runs, so the timer and "Sync now" never interleave.
    pub nexus_sync: Arc<tokio::sync::Mutex<()>>,
    /// Wakes the sync timer early (a teacher answered a parent, a notice was published).
    pub nexus_wake: Arc<tokio::sync::Notify>,
}

impl AppState {
    /// Build state from a migrated pool: loads the node id and edition from settings.
    pub async fn new(pool: PgPool) -> anyhow::Result<Self> {
        let mut conn = pool.acquire().await?;
        let node = settings::node_id(&mut conn).await?;
        let edition = settings::edition(&mut conn).await?;
        let profile = profile_from(schools::current(&mut conn).await?);
        drop(conn);
        apply_zone(&profile);
        if let Err(e) = crate::offline::idempotency::prune(&pool).await {
            tracing::warn!(error = %e, "could not drop old idempotency keys");
        }
        Ok(Self {
            pool,
            hlc: Arc::new(Hlc::new(node)),
            edition: Arc::new(RwLock::new(edition)),
            profile: Arc::new(RwLock::new(profile)),
            lockout: LockoutPolicy::default(),
            version: VERSION,
            directory: Arc::new(LdapDirectory),
            ro_limits: Arc::new(RateLimits::default()),
            timetable_jobs: Arc::new(TimetableJobs::default()),
            nexus_sync: Arc::new(tokio::sync::Mutex::new(())),
            nexus_wake: Arc::new(tokio::sync::Notify::new()),
        })
    }

    /// Use another directory client (tests use `StubDirectory`).
    pub fn with_directory(mut self, directory: SharedDirectory) -> Self {
        self.directory = directory;
        self
    }

    /// The school's country, currency and time zone.
    pub fn profile(&self) -> SchoolProfile {
        self.profile
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Re-read the school profile (after setup or a profile change) and reset the clock's zone.
    pub async fn reload_profile(&self) -> anyhow::Result<SchoolProfile> {
        let mut conn = self.pool.acquire().await?;
        let profile = profile_from(schools::current(&mut conn).await?);
        drop(conn);
        apply_zone(&profile);
        *self.profile.write().unwrap_or_else(|e| e.into_inner()) = profile.clone();
        Ok(profile)
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

    /// Ask the background sync to run now instead of at its next turn. Does nothing for a
    /// school that has not joined a hub (the timer checks), and nothing in a process
    /// without the timer.
    pub fn wake_nexus_sync(&self) {
        self.nexus_wake.notify_one();
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

fn profile_from(school: Option<madrasa_db::models::School>) -> SchoolProfile {
    match school {
        Some(s) => SchoolProfile {
            country: s.country,
            currency: s.currency,
            time_zone: s.time_zone,
        },
        None => SchoolProfile::default(),
    }
}

fn apply_zone(profile: &SchoolProfile) {
    let tz = locale::time_zone(&profile.time_zone);
    if tz.is_none() {
        tracing::warn!(zone = %profile.time_zone, "school time zone not recognised; using the machine's");
    }
    clock::set_time_zone(tz);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The version files must agree; the Release workflow checks the tag against them.
    #[test]
    fn package_json_and_tauri_conf_match_the_cargo_version() {
        let cargo = env!("CARGO_PKG_VERSION");
        let package: serde_json::Value =
            serde_json::from_str(include_str!("../../../apps/desktop/package.json")).unwrap();
        let tauri: serde_json::Value = serde_json::from_str(include_str!(
            "../../../apps/desktop/src-tauri/tauri.conf.json"
        ))
        .unwrap();
        let root: serde_json::Value =
            serde_json::from_str(include_str!("../../../package.json")).unwrap();
        assert_eq!(package["version"], cargo, "apps/desktop/package.json");
        assert_eq!(tauri["version"], cargo, "tauri.conf.json");
        assert_eq!(root["version"], cargo, "package.json");
        // The parent app (apps/parent) is versioned with everything else.
        for (name, text) in [
            (
                "apps/parent/package.json",
                include_str!("../../../apps/parent/package.json"),
            ),
            (
                "apps/parent/src-tauri/tauri.conf.json",
                include_str!("../../../apps/parent/src-tauri/tauri.conf.json"),
            ),
        ] {
            let file: serde_json::Value = serde_json::from_str(text).unwrap();
            assert_eq!(file["version"], cargo, "{name}");
        }
        assert!(VERSION.starts_with(cargo) || option_env!("MADRASA_RELEASE").is_some());
    }
}
