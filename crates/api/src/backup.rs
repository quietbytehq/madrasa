//! Backup service: settings, manual and scheduled backups, preview, restore and the backup
//! password. Files are `.mbk` archives (see `madrasa_core::backup::archive`).

use chrono::{DateTime, Local, NaiveTime, Utc};
use madrasa_core::backup::{self as keys, archive, BackupKeySet};
use madrasa_db::models::BackupRun;
use madrasa_db::{audit, backup as repo, schools, settings, Actor};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use uuid::Uuid;

use crate::{ApiError, AppState};

pub const KEY_SCHEDULE: &str = "backup.schedule";
/// A last backup older than this is shown as a warning.
pub const STALE_AFTER_HOURS: i64 = 48;

/// Mirrored by `BackupSettings` in the UI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupSettings {
    pub enabled: bool,
    /// Local time of day, `HH:MM`.
    pub time: String,
    /// Folder for backups; `None` means the default documents folder.
    pub directory: Option<String>,
    /// How many files to keep in the folder (older ones are removed after a new backup).
    pub keep: u32,
}

impl Default for BackupSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            time: "02:00".into(),
            directory: None,
            keep: 30,
        }
    }
}

impl BackupSettings {
    pub fn validate(&self) -> Result<NaiveTime, ApiError> {
        let time = NaiveTime::parse_from_str(&self.time, "%H:%M")
            .map_err(|_| ApiError::validation("Backup time must be HH:MM."))?;
        if self.keep == 0 || self.keep > 3650 {
            return Err(ApiError::validation(
                "Keep between 1 and 3650 backup files.",
            ));
        }
        Ok(time)
    }

    pub fn directory(&self) -> PathBuf {
        match &self.directory {
            Some(d) if !d.trim().is_empty() => PathBuf::from(d.trim()),
            _ => default_directory(),
        }
    }
}

/// `~/Documents/Madrasa Backups`, or the app data folder when there is no documents folder.
pub fn default_directory() -> PathBuf {
    dirs::document_dir()
        .map(|d| d.join("Madrasa Backups"))
        .unwrap_or_else(|| crate::bootstrap::default_data_dir().join("backups"))
}

pub async fn load_settings(
    conn: &mut sqlx::PgConnection,
    school_id: Uuid,
) -> Result<BackupSettings, ApiError> {
    Ok(settings::get(conn, Some(school_id), KEY_SCHEDULE)
        .await?
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default())
}

pub async fn save_settings(
    state: &AppState,
    actor: Actor,
    school_id: Uuid,
    s: &BackupSettings,
) -> Result<BackupSettings, ApiError> {
    s.validate()?;
    let dir = s.directory();
    std::fs::create_dir_all(&dir).map_err(|e| {
        ApiError::validation(format!(
            "The backup folder cannot be used ({}): {e}",
            dir.display()
        ))
    })?;
    let mut c = state.pool.acquire().await?;
    settings::set(
        &mut c,
        &state.hlc,
        actor,
        Some(school_id),
        KEY_SCHEDULE,
        serde_json::to_value(s).map_err(|_| ApiError::internal())?,
    )
    .await?;
    Ok(s.clone())
}

async fn key_set(conn: &mut sqlx::PgConnection, school_id: Uuid) -> Result<BackupKeySet, ApiError> {
    let value = settings::get(conn, Some(school_id), settings::KEY_BACKUP_KEYS)
        .await?
        .ok_or_else(|| ApiError::conflict("No backup keys exist; run the setup wizard first."))?;
    serde_json::from_value(value).map_err(|_| ApiError::internal())
}

async fn save_key_set(
    state: &AppState,
    actor: Actor,
    school_id: Uuid,
    set: &BackupKeySet,
) -> Result<(), ApiError> {
    let mut c = state.pool.acquire().await?;
    settings::set(
        &mut c,
        &state.hlc,
        actor,
        Some(school_id),
        settings::KEY_BACKUP_KEYS,
        serde_json::to_value(set).map_err(|_| ApiError::internal())?,
    )
    .await?;
    Ok(())
}

/// Password or recovery key offered for a restore or a password change.
#[derive(Debug, Clone, Deserialize)]
pub struct Secret {
    pub password: Option<String>,
    pub recovery_key: Option<String>,
}

impl Secret {
    fn unlock(&self, set: &BackupKeySet) -> Result<[u8; 32], ApiError> {
        let result = match (&self.password, &self.recovery_key) {
            (Some(p), _) if !p.is_empty() => keys::unlock_with_password(set, p),
            (_, Some(k)) if !k.trim().is_empty() => keys::unlock_with_recovery_key(set, k),
            _ => {
                return Err(ApiError::validation(
                    "Enter the backup password or the recovery key.",
                ))
            }
        };
        result.map_err(|_| {
            ApiError::new(
                axum::http::StatusCode::UNAUTHORIZED,
                "backup_secret_invalid",
                "The backup password or recovery key is incorrect.",
            )
        })
    }
}

async fn audit_failed_secret(state: &AppState, actor: Actor, operation: &str) {
    if let Ok(mut c) = state.pool.acquire().await {
        let _ = audit::record(
            &mut c,
            &state.hlc,
            actor,
            audit::Action::BackupPasswordFailed,
            "backup",
            None,
            None,
            Some(serde_json::json!({ "operation": operation })),
        )
        .await;
    }
}

/// Whether automatic backups can run (the key set carries the public key).
pub async fn can_run_unattended(
    conn: &mut sqlx::PgConnection,
    school_id: Uuid,
) -> Result<bool, ApiError> {
    Ok(key_set(conn, school_id).await?.public_key.is_some())
}

/// Fill in the public key on a key set written before stage 5 (needs the password once).
pub async fn unlock(
    state: &AppState,
    actor: Actor,
    school_id: Uuid,
    secret: &Secret,
) -> Result<(), ApiError> {
    let set = {
        let mut c = state.pool.acquire().await?;
        key_set(&mut c, school_id).await?
    };
    let dek = match secret.unlock(&set) {
        Ok(d) => d,
        Err(e) => {
            audit_failed_secret(state, actor, "unlock").await;
            return Err(e);
        }
    };
    if set.public_key.is_none() {
        save_key_set(state, actor, school_id, &keys::with_public_key(&set, &dek)).await?;
    }
    Ok(())
}

pub async fn change_password(
    state: &AppState,
    actor: Actor,
    school_id: Uuid,
    current: &Secret,
    new_password: &str,
) -> Result<(), ApiError> {
    madrasa_core::password::validate_password(new_password)
        .map_err(|e| ApiError::validation(format!("New backup password: {e}.")))?;
    let set = {
        let mut c = state.pool.acquire().await?;
        key_set(&mut c, school_id).await?
    };
    let dek = match current.unlock(&set) {
        Ok(d) => d,
        Err(e) => {
            audit_failed_secret(state, actor, "password_change").await;
            return Err(e);
        }
    };
    let new_set =
        keys::change_password(&set, &dek, new_password).map_err(|_| ApiError::internal())?;
    save_key_set(state, actor, school_id, &new_set).await
}

// ---- backup -------------------------------------------------------------------------------

/// Run a backup now. Records the run and audits it whether it succeeds or fails.
pub async fn run_backup(
    state: &AppState,
    actor: Actor,
    school_id: Uuid,
    trigger: &str,
) -> Result<BackupRun, ApiError> {
    let mut c = state.pool.acquire().await?;
    let run = repo::start_run(&mut c, &state.hlc, actor, school_id, trigger).await?;
    let outcome = produce(state, &mut c, school_id).await;
    let result = match &outcome {
        Ok((path, size)) => Ok((path.display().to_string(), *size)),
        Err(e) => Err(e.message.clone()),
    };
    let run = repo::finish_run(&mut c, &state.hlc, actor, run.id, result).await?;
    match outcome {
        Ok(_) => Ok(run),
        Err(e) => Err(e),
    }
}

async fn produce(
    state: &AppState,
    c: &mut sqlx::PgConnection,
    school_id: Uuid,
) -> Result<(PathBuf, i64), ApiError> {
    let settings = load_settings(c, school_id).await?;
    let set = key_set(c, school_id).await?;
    if set.public_key.is_none() {
        return Err(ApiError::conflict(
            archive::ArchiveError::NoPublicKey.to_string(),
        ));
    }
    let school = schools::get(c, school_id).await?;
    let now = Utc::now();
    let meta = archive::BackupMeta {
        created_at: now,
        app_version: state.version.to_string(),
        school_id,
        school_name: school.name.clone(),
        school_code: school.code.clone(),
        migrations: repo::migration_versions(c).await?,
        counts: repo::record_counts(c).await?,
    };
    let dump = repo::dump(c).await?;
    let bytes = tokio::task::spawn_blocking(move || archive::seal(&meta, &set, &dump))
        .await
        .map_err(|_| ApiError::internal())?
        .map_err(|e| ApiError::conflict(e.to_string()))?;
    let dir = settings.directory();
    std::fs::create_dir_all(&dir).map_err(|e| {
        ApiError::conflict(format!(
            "The backup folder cannot be used ({}): {e}",
            dir.display()
        ))
    })?;
    let path = dir.join(archive::file_name(&school.code, now));
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, &bytes)
        .and_then(|_| std::fs::rename(&tmp, &path))
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            ApiError::conflict(format!(
                "The backup could not be written to {}: {e}",
                dir.display()
            ))
        })?;
    prune(&dir, settings.keep as usize);
    Ok((path, bytes.len() as i64))
}

/// Remove the oldest Madrasa backup files beyond `keep`.
fn prune(dir: &Path, keep: usize) {
    let mut files = list_files(dir);
    files.sort_by_key(|f| std::cmp::Reverse(f.modified));
    for f in files.into_iter().skip(keep.max(1)) {
        let _ = std::fs::remove_file(&f.path);
    }
}

/// Mirrored by `BackupFile` in the UI.
#[derive(Debug, Clone, Serialize)]
pub struct BackupFile {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub modified: DateTime<Utc>,
}

fn list_files(dir: &Path) -> Vec<BackupFile> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in entries.flatten() {
        let path = e.path();
        let is_backup = path.extension().is_some_and(|x| x == archive::EXTENSION)
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("madrasa-"));
        if !is_backup {
            continue;
        }
        let Ok(md) = e.metadata() else { continue };
        out.push(BackupFile {
            name: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string(),
            path: path.display().to_string(),
            size: md.len(),
            modified: md
                .modified()
                .map(DateTime::<Utc>::from)
                .unwrap_or_else(|_| Utc::now()),
        });
    }
    out.sort_by_key(|f| std::cmp::Reverse(f.modified));
    out
}

pub fn files_in(settings: &BackupSettings) -> Vec<BackupFile> {
    list_files(&settings.directory())
}

// ---- preview and restore ------------------------------------------------------------------

/// Mirrored by `RestorePreview` in the UI.
#[derive(Debug, Serialize)]
pub struct RestorePreview {
    pub path: String,
    pub size: u64,
    pub meta: archive::BackupMeta,
    /// The backup was made with the schema this version runs.
    pub schema_matches: bool,
    pub current_counts: std::collections::BTreeMap<String, i64>,
}

fn read_file(path: &str) -> Result<Vec<u8>, ApiError> {
    let p = Path::new(path);
    if !p.is_absolute() {
        return Err(ApiError::validation(
            "Give the full path of the backup file.",
        ));
    }
    std::fs::read(p)
        .map_err(|e| ApiError::validation(format!("The backup file cannot be read: {e}")))
}

fn archive_error(e: archive::ArchiveError) -> ApiError {
    use archive::ArchiveError::*;
    match e {
        NotABackup | Tampered => ApiError::new(
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "backup_invalid",
            e.to_string(),
        ),
        WrongKey => ApiError::new(
            axum::http::StatusCode::UNAUTHORIZED,
            "backup_secret_invalid",
            e.to_string(),
        ),
        NoPublicKey | Encrypt => ApiError::conflict(e.to_string()),
    }
}

pub async fn preview(state: &AppState, path: &str) -> Result<RestorePreview, ApiError> {
    let bytes = read_file(path)?;
    let (meta, _) = archive::read_meta(&bytes).map_err(archive_error)?;
    let mut c = state.pool.acquire().await?;
    let current = repo::migration_versions(&mut c).await?;
    Ok(RestorePreview {
        path: path.to_string(),
        size: bytes.len() as u64,
        schema_matches: current == meta.migrations,
        meta,
        current_counts: repo::record_counts(&mut c).await?,
    })
}

/// Mirrored by `RestoreResult` in the UI.
#[derive(Debug, Serialize)]
pub struct RestoreResult {
    pub tables: usize,
    pub rows: u64,
    pub meta: archive::BackupMeta,
    /// Sessions were restored from the backup too; everyone must log in again.
    pub relogin: bool,
}

pub async fn restore(
    state: &AppState,
    actor: Actor,
    path: &str,
    secret: &Secret,
) -> Result<RestoreResult, ApiError> {
    let bytes = read_file(path)?;
    let (_, set) = archive::read_meta(&bytes).map_err(archive_error)?;
    let dek = match secret.unlock(&set) {
        Ok(d) => d,
        Err(e) => {
            audit_failed_secret(state, actor, "restore").await;
            return Err(e);
        }
    };
    let opened = tokio::task::spawn_blocking(move || archive::open(&bytes, &dek))
        .await
        .map_err(|_| ApiError::internal())?;
    let (meta, dump) = match opened {
        Ok(v) => v,
        Err(archive::ArchiveError::WrongKey) => {
            audit_failed_secret(state, actor, "restore").await;
            return Err(archive_error(archive::ArchiveError::WrongKey));
        }
        Err(e) => return Err(archive_error(e)),
    };
    let mut c = state.pool.acquire().await?;
    let current = repo::migration_versions(&mut c).await?;
    repo::check_schema(&current, &meta.migrations)?;
    let report = repo::restore(&mut c, &dump).await?;
    audit::record(
        &mut c,
        &state.hlc,
        actor,
        audit::Action::Restore,
        "restore",
        None,
        None,
        Some(serde_json::json!({
            "file": path, "backup_created_at": meta.created_at, "school": meta.school_name,
            "tables": report.tables, "rows": report.rows,
        })),
    )
    .await?;
    drop(c);
    let _ = state.reload_edition().await;
    Ok(RestoreResult {
        tables: report.tables,
        rows: report.rows,
        meta,
        relogin: true,
    })
}

// ---- status and scheduler -----------------------------------------------------------------

/// Mirrored by `BackupStatus` in the UI.
#[derive(Debug, Serialize)]
pub struct BackupStatus {
    pub last_backup_at: Option<DateTime<Utc>>,
    pub last_backup_file: Option<String>,
    /// No successful backup in the last two days.
    pub stale: bool,
    pub enabled: bool,
    pub time: String,
    pub directory: String,
    pub unattended_ready: bool,
    pub last_error: Option<String>,
}

pub async fn status(
    conn: &mut sqlx::PgConnection,
    school_id: Uuid,
) -> Result<BackupStatus, ApiError> {
    let s = load_settings(conn, school_id).await?;
    let last = repo::last_ok_run(conn, school_id).await?;
    let recent = repo::recent_runs(conn, school_id, 1).await?;
    let last_at = last.as_ref().and_then(|r| r.finished_at);
    let stale = match last_at {
        Some(at) => Utc::now() - at > chrono::Duration::hours(STALE_AFTER_HOURS),
        None => true,
    };
    Ok(BackupStatus {
        last_backup_at: last_at,
        last_backup_file: last.and_then(|r| r.file_path),
        stale,
        enabled: s.enabled,
        time: s.time.clone(),
        directory: s.directory().display().to_string(),
        unattended_ready: can_run_unattended(conn, school_id).await.unwrap_or(false),
        last_error: recent
            .first()
            .filter(|r| r.status == "failed")
            .and_then(|r| r.error.clone()),
    })
}

/// Is a scheduled backup due: enabled, past today's time, and none succeeded today.
pub fn is_due(s: &BackupSettings, last_ok: Option<DateTime<Utc>>, now: DateTime<Local>) -> bool {
    if !s.enabled {
        return false;
    }
    let Ok(time) = s.validate() else {
        return false;
    };
    if now.time() < time {
        return false;
    }
    match last_ok {
        Some(at) => at.with_timezone(&Local).date_naive() < now.date_naive(),
        None => true,
    }
}

/// Background task: once a minute, run the scheduled backup when it is due. A failed attempt
/// is retried after an hour, not every minute.
pub fn spawn_scheduler(state: AppState) {
    tokio::spawn(async move {
        let mut last_failure: Option<DateTime<Utc>> = None;
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            if let Err(e) = tick(&state, &mut last_failure).await {
                tracing::warn!(error = %e, "scheduled backup check failed");
            }
        }
    });
}

async fn tick(state: &AppState, last_failure: &mut Option<DateTime<Utc>>) -> anyhow::Result<()> {
    let mut c = state.pool.acquire().await?;
    let _ = repo::fail_stale_runs(&mut c, &state.hlc).await;
    let Some(school) = schools::current(&mut c).await? else {
        return Ok(());
    };
    let s = load_settings(&mut c, school.id).await?;
    let last_ok = repo::last_ok_run(&mut c, school.id)
        .await?
        .and_then(|r| r.finished_at);
    if !is_due(&s, last_ok, Local::now()) {
        return Ok(());
    }
    if let Some(f) = *last_failure {
        if Utc::now() - f < chrono::Duration::hours(1) {
            return Ok(());
        }
    }
    if !can_run_unattended(&mut c, school.id).await? {
        tracing::warn!(
            "scheduled backup skipped: backup password has not been entered on this installation"
        );
        *last_failure = Some(Utc::now());
        return Ok(());
    }
    drop(c);
    match run_backup(state, Actor::system_for(school.id), school.id, "scheduled").await {
        Ok(run) => {
            tracing::info!(file = ?run.file_path, "scheduled backup written");
            *last_failure = None;
        }
        Err(e) => {
            tracing::warn!(error = %e, "scheduled backup failed");
            *last_failure = Some(Utc::now());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn due_when_past_time_and_not_done_today() {
        let s = BackupSettings::default(); // 02:00, enabled
        let today_0300 = Local.with_ymd_and_hms(2026, 10, 30, 3, 0, 0).unwrap();
        let today_0100 = Local.with_ymd_and_hms(2026, 10, 30, 1, 0, 0).unwrap();
        assert!(is_due(&s, None, today_0300));
        assert!(!is_due(&s, None, today_0100), "before the scheduled time");
        let yesterday = Local
            .with_ymd_and_hms(2026, 10, 29, 2, 5, 0)
            .unwrap()
            .with_timezone(&Utc);
        assert!(is_due(&s, Some(yesterday), today_0300));
        let earlier_today = Local
            .with_ymd_and_hms(2026, 10, 30, 2, 5, 0)
            .unwrap()
            .with_timezone(&Utc);
        assert!(!is_due(&s, Some(earlier_today), today_0300));
        let off = BackupSettings {
            enabled: false,
            ..BackupSettings::default()
        };
        assert!(!is_due(&off, None, today_0300));
        let bad = BackupSettings {
            time: "25:99".into(),
            ..BackupSettings::default()
        };
        assert!(bad.validate().is_err());
        assert!(!is_due(&bad, None, today_0300));
    }
}
