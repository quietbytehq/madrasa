//! Session authentication and permission checks.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use chrono::Utc;
use madrasa_core::permissions::{Permission, PermissionSet};
use madrasa_core::session_token;
use madrasa_db::models::{Session, User};
use madrasa_db::{roles, sessions, Actor};

use crate::{ApiError, AppState};

/// The authenticated caller. Extracting it fails with `401` when there is no live session.
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub user: User,
    pub session: Session,
    pub roles: Vec<String>,
    pub permissions: PermissionSet,
}

impl CurrentUser {
    pub fn actor(&self) -> Actor {
        Actor::user(self.user.id, self.user.school_id)
    }

    pub fn can(&self, permission: Permission) -> bool {
        self.permissions.contains(permission)
    }

    /// Fail with `403 forbidden` unless the caller holds the permission.
    pub fn require(&self, permission: Permission) -> Result<(), ApiError> {
        if self.can(permission) {
            Ok(())
        } else {
            tracing::warn!(user = %self.user.username, permission = %permission, "permission denied");
            Err(ApiError::forbidden())
        }
    }
}

fn bearer_token(parts: &Parts) -> Option<&str> {
    parts
        .headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|t| !t.is_empty())
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = bearer_token(parts).ok_or_else(ApiError::unauthenticated)?;
        let hash = session_token::hash(token);
        let now = Utc::now();
        let mut conn = state.pool.acquire().await?;
        let (session, user) = sessions::find_live(&mut conn, &hash, now)
            .await?
            .ok_or_else(ApiError::unauthenticated)?;
        sessions::touch(
            &mut conn,
            &state.hlc,
            &session,
            now,
            session_token::default_lifetime(),
        )
        .await?;
        let role_codes = roles::codes_for_user(&mut conn, user.id).await?;
        let codes = roles::permission_codes_for_user(&mut conn, user.id).await?;
        let permissions = PermissionSet::from_codes(codes.iter().map(String::as_str));
        Ok(CurrentUser {
            user,
            session,
            roles: role_codes,
            permissions,
        })
    }
}
