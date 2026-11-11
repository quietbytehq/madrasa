//! Hub API error type. Serializes as `{ "error": { "code", "message" } }`, the same shape
//! the school API uses (docs/spec.md §6). Messages are plain language.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

#[derive(Debug, Clone, thiserror::Error)]
#[error("{code}: {message}")]
pub struct HubError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

pub type HubResult<T> = Result<T, HubError>;

impl HubError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }
    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "validation", message)
    }
    pub fn unauthenticated() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "unauthenticated",
            "Please log in to continue.",
        )
    }
    pub fn invalid_credentials() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
            "The username or password is incorrect.",
        )
    }
    pub fn forbidden() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "forbidden",
            "You do not have permission to do this.",
        )
    }
    pub fn not_found(what: &str) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("{what} was not found."),
        )
    }
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "conflict", message)
    }
    pub fn account_locked() -> Self {
        Self::new(
            StatusCode::LOCKED,
            "account_locked",
            "This account is temporarily locked after too many failed logins. Try again later.",
        )
    }
    pub fn account_inactive() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "account_inactive",
            "This account is disabled.",
        )
    }
    pub fn internal() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "Something went wrong. Please try again.",
        )
    }
}

impl IntoResponse for HubError {
    fn into_response(self) -> Response {
        let body = Json(serde_json::json!({
            "error": { "code": self.code, "message": self.message },
        }));
        (self.status, body).into_response()
    }
}

impl From<sqlx::Error> for HubError {
    fn from(err: sqlx::Error) -> Self {
        // The error text may name tables or values; log it, never return it.
        tracing::error!(error = %err, "hub database error");
        HubError::internal()
    }
}

impl From<madrasa_db::DbError> for HubError {
    fn from(err: madrasa_db::DbError) -> Self {
        use madrasa_db::DbError;
        match err {
            DbError::Conflict(msg) => HubError::conflict(sentence(&msg)),
            DbError::NotFound(what) => HubError::not_found(&what),
            DbError::Sqlx(e) => e.into(),
            DbError::Migrate(e) => {
                tracing::error!(error = %e, "hub migration error");
                HubError::internal()
            }
        }
    }
}

impl From<axum::extract::rejection::JsonRejection> for HubError {
    fn from(err: axum::extract::rejection::JsonRejection) -> Self {
        HubError::validation(format!("Invalid request body: {}", err.body_text()))
    }
}

impl From<axum::extract::rejection::QueryRejection> for HubError {
    fn from(err: axum::extract::rejection::QueryRejection) -> Self {
        HubError::validation(format!("Invalid query: {}", err.body_text()))
    }
}

/// One sentence with exactly one full stop.
fn sentence(msg: &str) -> String {
    let msg = msg.trim_end();
    if msg.ends_with(['.', '!', '?']) {
        msg.to_string()
    } else {
        format!("{msg}.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflicts_end_with_one_full_stop() {
        let e: HubError = madrasa_db::DbError::Conflict("already taken".into()).into();
        assert_eq!(e.message, "already taken.");
        let e: HubError = madrasa_db::DbError::Conflict("Already taken.".into()).into();
        assert_eq!(e.message, "Already taken.");
    }
}
