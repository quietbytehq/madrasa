//! Hub routes, mounted under `/api/nexus/v1`.

pub mod audit;
pub mod auth;
pub mod health;
pub mod schools;
pub mod setup;
pub mod sync;
pub mod users;

use crate::HubError;

pub(crate) type JsonBody<T> = Result<axum::Json<T>, axum::extract::rejection::JsonRejection>;

/// Usernames: at least 3 characters, letters, digits, dots or underscores; stored lower-case.
pub(crate) fn clean_username(input: &str) -> Result<String, HubError> {
    let username = input.trim().to_lowercase();
    if username.len() < 3
        || !username
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_')
    {
        return Err(HubError::validation(
            "Username must be at least 3 characters: letters, digits, dots or underscores.",
        ));
    }
    Ok(username)
}

pub(crate) fn required(value: &str, what: &str) -> Result<String, HubError> {
    let v = value.trim();
    if v.is_empty() {
        return Err(HubError::validation(format!("{what} is required.")));
    }
    Ok(v.to_string())
}
