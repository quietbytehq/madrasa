//! The edition switch (Standard alpha, stage 0): Settings → Edition lets the principal move
//! an instance between Lite and Standard. The flags are reloaded at once and the built-in
//! roles receive the default grants of any permission they are missing, so modules that
//! arrive with Standard are usable without touching roles by hand. Nexus is not set here
//! (it is an installation choice).

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use madrasa_core::edition::{Edition, Feature};
use madrasa_core::permissions::Permission;
use madrasa_db::audit::{self, Action};
use madrasa_db::{roles, settings};
use serde::{Deserialize, Serialize};

use crate::auth::CurrentUser;
use crate::{ApiError, ApiResult, AppState};

/// Mirrored by `EditionSettings` in the UI.
#[derive(Debug, Serialize)]
pub struct EditionSettings {
    pub edition: Edition,
    /// Editions this screen may switch to.
    pub available: Vec<Edition>,
    /// What the current edition enables, for the explanation on screen.
    pub features: Vec<Feature>,
    /// What Standard would add (or what switching back to Lite would hide).
    pub standard_only: Vec<Feature>,
}

#[derive(Debug, Deserialize)]
pub struct EditionRequest {
    pub edition: Edition,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/settings/edition", get(get_edition).put(put_edition))
}

fn view(edition: Edition) -> EditionSettings {
    EditionSettings {
        edition,
        available: if edition == Edition::Nexus {
            vec![Edition::Nexus]
        } else {
            vec![Edition::Lite, Edition::Standard]
        },
        features: edition.features().to_vec(),
        standard_only: madrasa_core::edition::STANDARD_ONLY.to_vec(),
    }
}

async fn get_edition(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<EditionSettings>> {
    user.require(Permission::SettingsManage)?;
    Ok(Json(view(state.edition())))
}

async fn put_edition(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<EditionRequest>,
) -> ApiResult<Json<EditionSettings>> {
    user.require(Permission::SettingsManage)?;
    let current = state.edition();
    if current == Edition::Nexus || req.edition == Edition::Nexus {
        return Err(ApiError::validation(
            "The Nexus edition is chosen at installation and cannot be switched here.",
        ));
    }
    if req.edition != current {
        let mut tx = state.pool.begin().await?;
        settings::set_edition(&mut tx, &state.hlc, user.actor(), req.edition).await?;
        let granted = roles::sync_defaults(
            &mut tx,
            &state.hlc,
            user.actor(),
            user.user.school_id,
            req.edition,
        )
        .await?;
        audit::record(
            &mut tx,
            &state.hlc,
            user.actor(),
            Action::Update,
            "edition",
            None,
            Some(serde_json::json!({ "edition": current })),
            Some(serde_json::json!({ "edition": req.edition, "grants_added": granted })),
        )
        .await?;
        tx.commit().await?;
        state.reload_edition().await.map_err(|e| {
            tracing::error!(error = %e, "edition reload failed");
            ApiError::internal()
        })?;
    }
    Ok(Json(view(state.edition())))
}
