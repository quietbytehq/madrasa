//! API error type. Serializes as `{ "error": { "code", "message" } }`.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{code}: {message}")]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
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

    pub fn feature_unavailable(feature: &str) -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            "feature_unavailable",
            format!("The '{feature}' feature is not available in this edition."),
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

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(serde_json::json!({
            "error": ErrorBody { code: self.code.to_string(), message: self.message }
        }));
        (self.status, body).into_response()
    }
}

impl From<madrasa_db::DbError> for ApiError {
    fn from(err: madrasa_db::DbError) -> Self {
        use madrasa_db::DbError;
        match err {
            DbError::Conflict(msg) => ApiError::conflict(format!("{msg}.")),
            DbError::NotFound(what) => ApiError::not_found(&what),
            DbError::Sqlx(e) => {
                tracing::error!(error = %e, "database error");
                ApiError::internal()
            }
            DbError::Migrate(e) => {
                tracing::error!(error = %e, "migration error");
                ApiError::internal()
            }
        }
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        madrasa_db::DbError::from(err).into()
    }
}

impl From<madrasa_core::password::PasswordError> for ApiError {
    fn from(err: madrasa_core::password::PasswordError) -> Self {
        use madrasa_core::password::PasswordError;
        match err {
            PasswordError::TooShort => ApiError::validation(err.to_string()),
            PasswordError::Hash => ApiError::internal(),
        }
    }
}

/// Rejection for malformed JSON bodies, mapped to the standard error shape.
impl From<axum::extract::rejection::JsonRejection> for ApiError {
    fn from(err: axum::extract::rejection::JsonRejection) -> Self {
        ApiError::validation(format!("Invalid request body: {}", err.body_text()))
    }
}
