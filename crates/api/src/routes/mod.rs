pub mod academics;
pub mod admissions;
pub mod allocation;
pub mod api_keys;
pub mod attendance;
pub mod audit;
pub mod auth;
pub mod backup;
pub mod boards;
pub mod consents;
pub mod directory;
pub mod edition;
pub mod exam_terms;
pub mod exams;
pub mod export;
pub mod fees;
pub mod health;
pub mod hostel;
pub mod imports;
pub mod inventory;
pub mod leave;
pub mod library;
pub mod nexus;
pub mod nexus_link;
pub mod reports;
pub mod rollover;
pub mod school_settings;
pub mod schools;
pub mod setup;
pub mod staff;
pub mod students;
pub mod subjects;
pub mod system;
pub mod timetable;
pub mod transport;
pub mod users;

use madrasa_db::models::User;
use serde::{Deserialize, Serialize};
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
    /// `local` or `directory` (Standard stage 8).
    pub auth_source: String,
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
            auth_source: user.auth_source.clone(),
        }
    }
}

/// Deserialize a nullable patch field: absent → `None` (keep), `null` → `Some(None)` (clear),
/// value → `Some(Some(v))`.
pub fn double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: serde::Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}
