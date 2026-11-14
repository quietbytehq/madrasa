//! Settings → Directory login (Standard stage 8): the LDAP settings, the sealed bind
//! password and "test connection". Needs `settings.manage` and the `directory_login`
//! feature.

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use madrasa_core::directory::{DirectoryConfig, DirectoryUser};
use madrasa_core::edition::Feature;
use madrasa_core::permissions::{Permission, RoleCode};
use madrasa_db::directory as repo;
use serde::{Deserialize, Serialize};

use crate::auth::CurrentUser;
use crate::directory::{CertificateProblem, DirectoryError};
use crate::{ApiError, ApiResult, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/settings/directory", get(get_settings).put(put_settings))
        .route("/settings/directory/test", post(test))
}

/// Mirrored by `DirectorySettings` in the UI.
#[derive(Debug, Serialize)]
pub struct DirectorySettings {
    #[serde(flatten)]
    pub config: DirectoryConfig,
    pub has_bind_password: bool,
    /// Role codes the group mapping may use on this edition.
    pub roles: Vec<&'static str>,
}

/// Mirrored by `DirectorySettingsBody`: the config plus the bind password when it changes.
#[derive(Debug, Deserialize)]
pub struct DirectorySettingsBody {
    #[serde(flatten)]
    pub config: DirectoryConfig,
    pub bind_password: Option<String>,
}

fn gate(state: &AppState, user: &CurrentUser) -> ApiResult<()> {
    state.require_feature(Feature::DirectoryLogin)?;
    user.require(Permission::SettingsManage)
}

fn role_codes(state: &AppState) -> Vec<&'static str> {
    RoleCode::ALL
        .iter()
        .copied()
        .filter(|r| r.available_in(state.edition()) && *r != RoleCode::Parent)
        .map(RoleCode::code)
        .collect()
}

async fn view(state: &AppState, school: uuid::Uuid) -> ApiResult<DirectorySettings> {
    let mut c = state.pool.acquire().await?;
    Ok(DirectorySettings {
        config: repo::config(&mut c, school).await?,
        has_bind_password: repo::has_bind_password(&mut c, school).await?,
        roles: role_codes(state),
    })
}

async fn get_settings(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<DirectorySettings>> {
    gate(&state, &user)?;
    Ok(Json(view(&state, user.user.school_id).await?))
}

fn check_roles(state: &AppState, cfg: &DirectoryConfig) -> ApiResult<()> {
    let known = role_codes(state);
    let bad = cfg
        .group_roles
        .iter()
        .map(|m| m.role.trim().to_lowercase())
        .chain(Some(cfg.default_role.trim().to_lowercase()).filter(|d| !d.is_empty()))
        .find(|r| !known.contains(&r.as_str()));
    match bad {
        Some(r) => Err(ApiError::validation(format!("Unknown role \"{r}\"."))),
        None => Ok(()),
    }
}

async fn put_settings(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<DirectorySettingsBody>,
) -> ApiResult<Json<DirectorySettings>> {
    gate(&state, &user)?;
    let school = user.user.school_id;
    let mut cfg = body.config;
    cfg.url = cfg.url.trim().to_string();
    cfg.validate().map_err(ApiError::validation)?;
    check_roles(&state, &cfg)?;
    let mut tx = state.pool.begin().await?;
    if let Some(pw) = body.bind_password.as_deref() {
        repo::set_bind_password(&mut tx, &state.hlc, user.actor(), school, pw).await?;
    }
    if cfg.enabled && !repo::has_bind_password(&mut tx, school).await? {
        return Err(ApiError::validation(
            "Enter the bind account's password before enabling directory login.",
        ));
    }
    repo::set_config(&mut tx, &state.hlc, user.actor(), school, &cfg).await?;
    tx.commit().await?;
    Ok(Json(view(&state, school).await?))
}

/// Mirrored by `DirectoryTestBody`: settings to try (unsaved), the bind password when it
/// is being changed, and optionally a user to look up (no password check).
#[derive(Debug, Deserialize)]
pub struct DirectoryTestBody {
    #[serde(flatten)]
    pub config: DirectoryConfig,
    pub bind_password: Option<String>,
    pub username: Option<String>,
}

/// Mirrored by `DirectoryTestResult`.
#[derive(Debug, Serialize)]
pub struct DirectoryTestResult {
    pub bind_ok: bool,
    pub message: String,
    pub user: Option<DirectoryUser>,
    /// The role the user would get from the mapping.
    pub role: Option<String>,
    /// The technical text behind a refusal (for example the TLS library's reason for refusing
    /// the directory's certificate), for whoever looks after the directory.
    pub details: Option<String>,
    /// `certificate_untrusted`, `certificate_expired`, `certificate_not_yet_valid`,
    /// `certificate_wrong_name` or `certificate_refused` when the certificate was the problem.
    pub problem: Option<String>,
}

/// A failed test, said plainly, with the technical text kept apart.
fn refusal(e: DirectoryError) -> DirectoryTestResult {
    let (details, problem) = match &e {
        DirectoryError::Certificate { problem, detail } => (
            Some(detail.clone()),
            Some(
                match problem {
                    CertificateProblem::Untrusted => "certificate_untrusted",
                    CertificateProblem::Expired => "certificate_expired",
                    CertificateProblem::NotYetValid => "certificate_not_yet_valid",
                    CertificateProblem::WrongName => "certificate_wrong_name",
                    CertificateProblem::Other => "certificate_refused",
                }
                .to_string(),
            ),
        ),
        _ => (None, None),
    };
    DirectoryTestResult {
        bind_ok: false,
        message: e.to_string(),
        user: None,
        role: None,
        details,
        problem,
    }
}

async fn test(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(body): Json<DirectoryTestBody>,
) -> ApiResult<Json<DirectoryTestResult>> {
    gate(&state, &user)?;
    let school = user.user.school_id;
    let mut cfg = body.config;
    cfg.enabled = true;
    cfg.url = cfg.url.trim().to_string();
    cfg.validate().map_err(ApiError::validation)?;
    let bind_password = match body.bind_password.filter(|p| !p.is_empty()) {
        Some(p) => p,
        None => {
            let mut c = state.pool.acquire().await?;
            repo::bind_password(&mut c, &state.hlc, school)
                .await?
                .ok_or_else(|| ApiError::validation("Enter the bind account's password."))?
        }
    };
    let bind = state.directory.test_bind(&cfg, &bind_password).await;
    let mut out = match bind {
        Ok(()) => DirectoryTestResult {
            bind_ok: true,
            message: "Connected and the bind account was accepted.".into(),
            user: None,
            role: None,
            details: None,
            problem: None,
        },
        Err(e) => return Ok(Json(refusal(e))),
    };
    if let Some(name) = body
        .username
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        match state.directory.lookup(&cfg, &bind_password, name).await {
            Ok(u) => {
                out.role = cfg.role_for(&u.groups);
                out.message = match &out.role {
                    Some(r) => format!("Found {} ({}); they would log in as {r}.", u.full_name, u.dn),
                    None => format!(
                        "Found {} ({}) but none of their groups maps to a role and there is no default role.",
                        u.full_name, u.dn
                    ),
                };
                out.user = Some(u);
            }
            Err(DirectoryError::NotFound) => {
                out.message =
                    format!("Connected, but no user matches \"{name}\" under the user base DN.");
            }
            Err(e) => {
                out = refusal(e);
            }
        }
    }
    Ok(Json(out))
}
