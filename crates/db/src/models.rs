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
    /// ISO 3166-1 alpha-2, e.g. `PK`, `US`.
    pub country: String,
    /// IANA time zone, e.g. `Asia/Karachi`.
    pub time_zone: String,
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

// ---- M1 stage 2: fees ----------------------------------------------------------------------

use rust_decimal::Decimal;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FeeHead {
    pub id: Uuid,
    pub school_id: Uuid,
    pub name: String,
    pub code: String,
    pub is_recurring: bool,
    pub position: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<Uuid>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub hlc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FeeStructure {
    pub id: Uuid,
    pub school_id: Uuid,
    pub academic_year_id: Uuid,
    pub class_id: Uuid,
    pub fee_head_id: Uuid,
    pub amount: Decimal,
    pub hlc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FeeConcession {
    pub id: Uuid,
    pub school_id: Uuid,
    pub student_id: Uuid,
    pub kind: String,
    pub percent: Option<Decimal>,
    pub amount: Option<Decimal>,
    pub reason: String,
    pub approved_by: Uuid,
    pub approved_at: DateTime<Utc>,
    pub valid_from: chrono::NaiveDate,
    pub valid_to: Option<chrono::NaiveDate>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<Uuid>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub hlc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FeeVoucher {
    pub id: Uuid,
    pub school_id: Uuid,
    pub student_id: Uuid,
    pub academic_year_id: Uuid,
    pub section_id: Option<Uuid>,
    pub voucher_no: String,
    pub billing_month: chrono::NaiveDate,
    pub issue_date: chrono::NaiveDate,
    pub due_date: chrono::NaiveDate,
    pub gross: Decimal,
    pub sibling_discount: Decimal,
    pub concession_discount: Decimal,
    pub net_monthly: Decimal,
    pub arrears: Decimal,
    pub total_due: Decimal,
    pub late_fee: Decimal,
    pub late_fee_waived: bool,
    pub waiver_reason: Option<String>,
    pub waived_by: Option<Uuid>,
    pub waived_at: Option<DateTime<Utc>>,
    pub net_paid: Decimal,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<Uuid>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub hlc: String,
}

impl FeeVoucher {
    /// Outstanding balance: total due + assessed late fee − net payments.
    pub fn balance(&self) -> Decimal {
        madrasa_core::fees::voucher_balance(self.total_due, self.late_fee, self.net_paid)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FeeVoucherLine {
    pub id: Uuid,
    pub voucher_id: Uuid,
    pub fee_head_id: Option<Uuid>,
    pub description: String,
    pub amount: Decimal,
    pub position: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FeeReceipt {
    pub id: Uuid,
    pub school_id: Uuid,
    pub voucher_id: Uuid,
    pub student_id: Uuid,
    pub receipt_no: String,
    pub paid_on: chrono::NaiveDate,
    pub amount: Decimal,
    pub method: String,
    pub reference: Option<String>,
    pub note: Option<String>,
    pub reverses_id: Option<Uuid>,
    pub reversal_reason: Option<String>,
    pub received_by: Uuid,
    pub created_at: DateTime<Utc>,
    pub hlc: String,
}

/// Voucher list row with student and class names.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FeeVoucherRow {
    pub id: Uuid,
    pub voucher_no: String,
    pub billing_month: chrono::NaiveDate,
    pub due_date: chrono::NaiveDate,
    pub student_id: Uuid,
    pub student_number: String,
    pub student_name: String,
    pub class_name: Option<String>,
    pub section_name: Option<String>,
    pub gross: Decimal,
    pub sibling_discount: Decimal,
    pub concession_discount: Decimal,
    pub arrears: Decimal,
    pub total_due: Decimal,
    pub late_fee: Decimal,
    pub late_fee_waived: bool,
    pub net_paid: Decimal,
    pub status: String,
}

/// Defaulter list row.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Defaulter {
    pub student_id: Uuid,
    pub student_number: String,
    pub student_name: String,
    pub father_name: String,
    pub class_name: Option<String>,
    pub section_name: Option<String>,
    pub guardian_phone: Option<String>,
    pub oldest_due_date: chrono::NaiveDate,
    pub days_overdue: i32,
    pub unpaid_vouchers: i64,
    pub balance: Decimal,
}

/// Daily collection summary row (per method).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CollectionByMethod {
    pub method: String,
    pub receipts: i64,
    pub amount: Decimal,
}

// ---- M1 stage 3: attendance ----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Holiday {
    pub id: Uuid,
    pub school_id: Uuid,
    pub name: String,
    pub starts_on: chrono::NaiveDate,
    pub ends_on: chrono::NaiveDate,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<Uuid>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub hlc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AttendanceSheet {
    pub id: Uuid,
    pub school_id: Uuid,
    pub section_id: Uuid,
    pub academic_year_id: Uuid,
    pub date: chrono::NaiveDate,
    pub marked_by: Option<Uuid>,
    pub submitted_at: DateTime<Utc>,
    pub present: i32,
    pub absent: i32,
    pub late: i32,
    pub leave: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<Uuid>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub hlc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AttendanceEntry {
    pub id: Uuid,
    pub school_id: Uuid,
    pub sheet_id: Uuid,
    pub student_id: Uuid,
    pub date: chrono::NaiveDate,
    pub status: String,
    pub note: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<Uuid>,
    pub hlc: String,
}

/// A student of a section with the entry for one day (status is `None` until marked).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RosterRow {
    pub student_id: Uuid,
    pub student_number: String,
    pub full_name: String,
    pub father_name: String,
    pub roll_no: Option<i32>,
    pub status: Option<String>,
    pub note: Option<String>,
}

/// One day in a student's history.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StudentDayRow {
    pub date: chrono::NaiveDate,
    pub status: String,
    pub note: Option<String>,
    pub section_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AbsenteeRow {
    pub student_id: Uuid,
    pub student_number: String,
    pub full_name: String,
    pub father_name: String,
    pub class_name: String,
    pub section_id: Uuid,
    pub section_name: String,
    pub roll_no: Option<i32>,
    pub status: String,
    pub note: Option<String>,
    pub guardian_phone: Option<String>,
}

/// Marking status of one section on one day.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SectionDayStatus {
    pub section_id: Uuid,
    pub class_id: Uuid,
    pub class_name: String,
    pub section_name: String,
    pub class_teacher_name: Option<String>,
    pub enrolled: i64,
    pub sheet_id: Option<Uuid>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub present: Option<i32>,
    pub absent: Option<i32>,
    pub late: Option<i32>,
    pub leave: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StaffAttendance {
    pub id: Uuid,
    pub school_id: Uuid,
    pub staff_id: Uuid,
    pub date: chrono::NaiveDate,
    pub status: String,
    pub note: Option<String>,
    pub marked_by: Option<Uuid>,
    pub updated_at: DateTime<Utc>,
    pub hlc: String,
}

/// A staff member with the record for one day (status is `None` until marked).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StaffDayRow {
    pub staff_id: Uuid,
    pub staff_number: String,
    pub full_name: String,
    pub designation: String,
    pub status: Option<String>,
    pub note: Option<String>,
}

// ---- M1 stage 4: exams ---------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AssessmentType {
    pub id: Uuid,
    pub school_id: Uuid,
    pub academic_year_id: Uuid,
    pub name: String,
    pub position: i32,
    pub weight: Decimal,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<Uuid>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub hlc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AssessmentMaxMark {
    pub id: Uuid,
    pub assessment_type_id: Uuid,
    pub class_id: Uuid,
    pub subject_id: Uuid,
    pub max_marks: Decimal,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MarkSheet {
    pub id: Uuid,
    pub school_id: Uuid,
    pub assessment_type_id: Uuid,
    pub section_id: Uuid,
    pub subject_id: Uuid,
    pub status: String,
    pub submitted_by: Option<Uuid>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub approved_by: Option<Uuid>,
    pub approved_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<Uuid>,
    pub hlc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Mark {
    pub id: Uuid,
    pub sheet_id: Uuid,
    pub student_id: Uuid,
    pub obtained: Option<Decimal>,
    pub is_absent: bool,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<Uuid>,
}

/// One subject of a section in an assessment: its maximum and the sheet state.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SheetSummaryRow {
    pub subject_id: Uuid,
    pub subject_name: String,
    pub subject_code: String,
    pub max_marks: Option<Decimal>,
    pub sheet_id: Option<Uuid>,
    pub status: Option<String>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub approved_at: Option<DateTime<Utc>>,
    pub entered: i64,
}

/// A mark of one student in one subject of one assessment, with its sheet state.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MarkRow {
    pub assessment_type_id: Uuid,
    pub subject_id: Uuid,
    pub student_id: Uuid,
    pub max_marks: Decimal,
    pub obtained: Option<Decimal>,
    pub is_absent: bool,
    pub sheet_status: String,
}

// ---- M1 stage 5: backup --------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BackupRun {
    pub id: Uuid,
    pub school_id: Uuid,
    pub trigger: String,
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub file_path: Option<String>,
    pub file_size: Option<i64>,
    pub error: Option<String>,
}

// ---- M1 stage 6: imports -------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ImportBatch {
    pub id: Uuid,
    pub school_id: Uuid,
    pub kind: String,
    pub file_name: String,
    pub status: String,
    pub options: Value,
    pub total_rows: i32,
    pub created_rows: i32,
    pub updated_rows: i32,
    pub skipped_rows: i32,
    pub applied_by: Option<Uuid>,
    pub applied_at: DateTime<Utc>,
    pub rolled_back_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ImportBatchRow {
    pub id: Uuid,
    pub batch_id: Uuid,
    pub row_no: i32,
    pub action: String,
    pub entity: String,
    pub entity_id: Option<Uuid>,
    pub old_value: Option<Value>,
}
