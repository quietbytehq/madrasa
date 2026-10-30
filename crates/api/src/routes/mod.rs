pub mod audit;
pub mod auth;
pub mod health;
pub mod nexus;
pub mod schools;
pub mod setup;
pub mod system;
pub mod users;

use madrasa_db::models::User;
use serde::Serialize;
use uuid::Uuid;

/// Public view of a user. Mirrored by `UserSummary` in the UI.
#[derive(Debug, Clone, Serialize)]
pub struct UserSummary {
    pub id: Uuid,
    pub school_id: Uuid,
    pub username: String,
    pub full_name: String,
    pub email: Option<String>,
    pub is_active: bool,
    pub roles: Vec<String>,
}

impl UserSummary {
    pub fn from_user(user: &User, roles: Vec<String>) -> Self {
        Self {
            id: user.id,
            school_id: user.school_id,
            username: user.username.clone(),
            full_name: user.full_name.clone(),
            email: user.email.clone(),
            is_active: user.is_active,
            roles,
        }
    }
}
