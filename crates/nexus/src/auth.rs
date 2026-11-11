//! Hub session authentication and permission checks.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use chrono::Utc;
use madrasa_core::session_token;

use crate::permissions::HubPermission;
use crate::store::{self, HubSession, HubUser};
use crate::{HubError, HubState};

/// The authenticated hub user. Extracting it fails with `401` when there is no live session.
#[derive(Debug, Clone)]
pub struct CurrentHubUser {
    pub user: HubUser,
    pub session: HubSession,
}

impl CurrentHubUser {
    pub fn can(&self, permission: HubPermission) -> bool {
        self.user.permissions.iter().any(|p| p == permission.code())
    }

    /// Fail with `403 forbidden` unless the caller holds the permission.
    pub fn require(&self, permission: HubPermission) -> Result<(), HubError> {
        if self.can(permission) {
            Ok(())
        } else {
            tracing::warn!(user = %self.user.username, permission = %permission, "hub permission denied");
            Err(HubError::forbidden())
        }
    }
}

pub(crate) fn bearer_token(parts: &Parts) -> Option<&str> {
    parts
        .headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|t| !t.is_empty())
}

impl FromRequestParts<HubState> for CurrentHubUser {
    type Rejection = HubError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &HubState,
    ) -> Result<Self, Self::Rejection> {
        let token = bearer_token(parts).ok_or_else(HubError::unauthenticated)?;
        let hash = session_token::hash(token);
        let now = Utc::now();
        let mut conn = state.pool.acquire().await?;
        let (session, user) = store::find_live_session(&mut conn, &hash, now)
            .await?
            .ok_or_else(HubError::unauthenticated)?;
        store::touch_session(
            &mut conn,
            &state.hlc,
            session.id,
            now,
            session_token::default_lifetime(),
        )
        .await?;
        Ok(CurrentHubUser { user, session })
    }
}
