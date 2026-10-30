//! Row structs. Secrets are `#[serde(skip)]` so they never reach API responses or audit JSON.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct School {
    pub id: Uuid,
    pub name: String,
    pub code: String,
    pub city: String,
    pub is_demo: bool,
    pub academic_year_start_month: i16,
    pub currency: String,
    pub custom: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<Uuid>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub hlc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub school_id: Uuid,
    pub username: String,
    pub full_name: String,
    pub email: Option<String>,
    #[serde(skip)]
    pub password_hash: String,
    pub is_active: bool,
    pub failed_login_count: i32,
    pub locked_until: Option<DateTime<Utc>>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub custom: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<Uuid>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub hlc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Role {
    pub id: Uuid,
    pub school_id: Uuid,
    pub code: String,
    pub name: String,
    pub is_system: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<Uuid>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub hlc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PermissionGrant {
    pub id: Uuid,
    pub school_id: Uuid,
    pub role_id: Uuid,
    pub code: String,
    pub hlc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UserRole {
    pub id: Uuid,
    pub school_id: Uuid,
    pub user_id: Uuid,
    pub role_id: Uuid,
    pub hlc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Session {
    pub id: Uuid,
    pub school_id: Uuid,
    pub user_id: Uuid,
    #[serde(skip)]
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub hlc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Setting {
    pub id: Uuid,
    pub school_id: Option<Uuid>,
    pub key: String,
    pub value: Value,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<Uuid>,
    pub hlc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AuditEntry {
    pub id: Uuid,
    pub school_id: Option<Uuid>,
    pub actor_user_id: Option<Uuid>,
    pub action: String,
    pub entity: String,
    pub entity_id: Option<Uuid>,
    pub old_value: Option<Value>,
    pub new_value: Option<Value>,
    pub at: DateTime<Utc>,
    pub hlc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CustomFieldDefinition {
    pub id: Uuid,
    pub school_id: Uuid,
    pub entity: String,
    pub key: String,
    pub label: String,
    pub field_type: String,
    pub required: bool,
    pub options: Value,
    pub position: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<Uuid>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub hlc: String,
}

// ---- M1 stage 1 -------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AcademicYear {
    pub id: Uuid,
    pub school_id: Uuid,
    pub name: String,
    pub start_date: chrono::NaiveDate,
    pub end_date: chrono::NaiveDate,
    pub start_month: i16,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<Uuid>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub hlc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Class {
    pub id: Uuid,
    pub school_id: Uuid,
    pub name: String,
    pub level: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<Uuid>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub hlc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Subject {
    pub id: Uuid,
    pub school_id: Uuid,
    pub name: String,
    pub code: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<Uuid>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub hlc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Section {
    pub id: Uuid,
    pub school_id: Uuid,
    pub class_id: Uuid,
    pub academic_year_id: Uuid,
    pub name: String,
    pub class_teacher_id: Option<Uuid>,
    pub capacity: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<Uuid>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub hlc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Staff {
    pub id: Uuid,
    pub school_id: Uuid,
    pub user_id: Option<Uuid>,
    pub staff_number: String,
    pub full_name: String,
    pub father_name: String,
    pub gender: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub cnic: Option<String>,
    pub designation: String,
    pub joined_on: Option<chrono::NaiveDate>,
    pub status: String,
    pub custom: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<Uuid>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub hlc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SectionTeacher {
    pub id: Uuid,
    pub school_id: Uuid,
    pub staff_id: Uuid,
    pub section_id: Uuid,
    pub subject_id: Option<Uuid>,
    pub hlc: String,
}

/// Student row. The photo bytes are never selected with the row (see `students::photo`).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Student {
    pub id: Uuid,
    pub school_id: Uuid,
    pub student_number: String,
    pub legacy_id: Option<String>,
    pub full_name: String,
    pub father_name: String,
    pub date_of_birth: chrono::NaiveDate,
    pub gender: String,
    pub cnic: Option<String>,
    pub address: String,
    pub admission_date: chrono::NaiveDate,
    pub status: String,
    pub left_on: Option<chrono::NaiveDate>,
    pub left_reason: Option<String>,
    pub board_roll_no: Option<String>,
    pub board_registration_no: Option<String>,
    pub has_photo: bool,
    pub custom: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<Uuid>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub hlc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Enrollment {
    pub id: Uuid,
    pub school_id: Uuid,
    pub student_id: Uuid,
    pub academic_year_id: Uuid,
    pub section_id: Uuid,
    pub roll_no: Option<i32>,
    pub hlc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Guardian {
    pub id: Uuid,
    pub school_id: Uuid,
    pub full_name: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub cnic: Option<String>,
    pub occupation: String,
    pub address: String,
    pub custom: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<Uuid>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub hlc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StudentGuardian {
    pub id: Uuid,
    pub school_id: Uuid,
    pub student_id: Uuid,
    pub guardian_id: Uuid,
    pub relationship: String,
    pub is_primary: bool,
    pub hlc: String,
}

/// A guardian together with the link details for one student.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct GuardianLink {
    pub id: Uuid,
    pub school_id: Uuid,
    pub full_name: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub cnic: Option<String>,
    pub occupation: String,
    pub address: String,
    pub relationship: String,
    pub is_primary: bool,
}

/// Directory row: student plus current enrollment (active year) and class/section names.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StudentListRow {
    pub id: Uuid,
    pub student_number: String,
    pub full_name: String,
    pub father_name: String,
    pub gender: String,
    pub date_of_birth: chrono::NaiveDate,
    pub status: String,
    pub admission_date: chrono::NaiveDate,
    pub class_id: Option<Uuid>,
    pub class_name: Option<String>,
    pub section_id: Option<Uuid>,
    pub section_name: Option<String>,
    pub roll_no: Option<i32>,
    pub has_photo: bool,
}
