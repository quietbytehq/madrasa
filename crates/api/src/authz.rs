//! Who may call each route: one table, enforced before any handler runs.
//!
//! Every handler still checks its own permission (and the finer rules: a teacher's own
//! sections, the owner of a saved report). This table is the first gate and the record:
//! - a request is refused here, with `401` or `403`, before its body or query is read, so
//!   the answer to "may I?" never depends on what was sent;
//! - a route that is not in the table is refused outright, so a new route cannot ship
//!   without saying who may call it;
//! - `docs/permissions.md` lists the same table, and `tests/authz_matrix.rs` calls every
//!   route as every built-in role and holds the three (code, document, behaviour) together.
//!
//! The table can only narrow what a handler allows, never widen it.

use std::collections::HashMap;
use std::sync::LazyLock;

use axum::extract::{FromRequestParts, MatchedPath, Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use madrasa_core::permissions::Permission as P;

use crate::auth::CurrentUser;
use crate::{ApiError, AppState, API_PREFIX};

/// Who may call a route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// No login: the login itself, first-run setup, the health check, what the login screen
    /// needs to draw itself.
    Public,
    /// Any signed-in user.
    SignedIn,
    /// A signed-in user holding at least one of these permissions.
    Any(&'static [P]),
}

impl Access {
    pub fn allows(&self, user: &CurrentUser) -> bool {
        match self {
            Access::Public | Access::SignedIn => true,
            Access::Any(permissions) => permissions.iter().any(|p| user.can(*p)),
        }
    }
}

/// `(method, route as declared in `routes/*.rs`, who may call it)`.
pub const ROUTES: &[(&str, &str, Access)] = &[
    // academics
    ("GET", "/academic-years", Access::Any(&[P::AcademicsRead])),
    (
        "POST",
        "/academic-years",
        Access::Any(&[P::AcademicsManage]),
    ),
    (
        "POST",
        "/academic-years/{id}/activate",
        Access::Any(&[P::AcademicsManage]),
    ),
    ("GET", "/classes", Access::Any(&[P::AcademicsRead])),
    ("POST", "/classes", Access::Any(&[P::AcademicsManage])),
    ("PATCH", "/classes/{id}", Access::Any(&[P::AcademicsManage])),
    (
        "GET",
        "/classes/{id}/subjects",
        Access::Any(&[P::AcademicsRead]),
    ),
    (
        "PUT",
        "/classes/{id}/subjects",
        Access::Any(&[P::AcademicsManage]),
    ),
    ("GET", "/sections", Access::Any(&[P::AcademicsRead])),
    ("POST", "/sections", Access::Any(&[P::AcademicsManage])),
    (
        "PATCH",
        "/sections/{id}",
        Access::Any(&[P::AcademicsManage]),
    ),
    ("GET", "/subjects", Access::Any(&[P::AcademicsRead])),
    ("POST", "/subjects", Access::Any(&[P::AcademicsManage])),
    (
        "PATCH",
        "/subjects/{id}",
        Access::Any(&[P::AcademicsManage]),
    ),
    // admissions
    (
        "GET",
        "/admissions/applications",
        Access::Any(&[P::AdmissionsRead]),
    ),
    (
        "POST",
        "/admissions/applications",
        Access::Any(&[P::AdmissionsManage]),
    ),
    (
        "POST",
        "/admissions/applications/move",
        Access::Any(&[P::AdmissionsManage]),
    ),
    (
        "GET",
        "/admissions/applications/{id}",
        Access::Any(&[P::AdmissionsRead]),
    ),
    (
        "PUT",
        "/admissions/applications/{id}",
        Access::Any(&[P::AdmissionsManage]),
    ),
    (
        "DELETE",
        "/admissions/applications/{id}",
        Access::Any(&[P::AdmissionsManage]),
    ),
    (
        "POST",
        "/admissions/applications/{id}/convert",
        Access::Any(&[P::StudentsManage]),
    ),
    (
        "POST",
        "/admissions/applications/{id}/move",
        Access::Any(&[P::AdmissionsManage]),
    ),
    (
        "GET",
        "/admissions/applications/{id}/offer-letter",
        Access::Any(&[P::AdmissionsRead]),
    ),
    (
        "GET",
        "/admissions/board",
        Access::Any(&[P::AdmissionsRead]),
    ),
    (
        "GET",
        "/admissions/settings",
        Access::Any(&[P::AdmissionsRead]),
    ),
    (
        "PUT",
        "/admissions/settings",
        Access::Any(&[P::AdmissionsManage]),
    ),
    (
        "GET",
        "/admissions/stages",
        Access::Any(&[P::AdmissionsRead]),
    ),
    (
        "POST",
        "/admissions/stages",
        Access::Any(&[P::AdmissionsManage]),
    ),
    (
        "PUT",
        "/admissions/stages/{id}",
        Access::Any(&[P::AdmissionsManage]),
    ),
    (
        "DELETE",
        "/admissions/stages/{id}",
        Access::Any(&[P::AdmissionsManage]),
    ),
    // allocation
    (
        "POST",
        "/allocation/balance",
        Access::Any(&[P::StudentsManage]),
    ),
    (
        "POST",
        "/allocation/moves",
        Access::Any(&[P::StudentsManage]),
    ),
    (
        "GET",
        "/allocation/sections",
        Access::Any(&[P::StudentsManage]),
    ),
    (
        "GET",
        "/allocation/teachers",
        Access::Any(&[P::TeachersRead]),
    ),
    (
        "PUT",
        "/allocation/teachers/cell",
        Access::Any(&[P::TeachersManage]),
    ),
    // api_keys
    (
        "GET",
        "/settings/api-keys",
        Access::Any(&[P::SettingsManage]),
    ),
    (
        "POST",
        "/settings/api-keys",
        Access::Any(&[P::SettingsManage]),
    ),
    (
        "DELETE",
        "/settings/api-keys/{id}",
        Access::Any(&[P::SettingsManage]),
    ),
    (
        "GET",
        "/settings/api-keys/{id}/calls",
        Access::Any(&[P::SettingsManage]),
    ),
    // attendance
    (
        "GET",
        "/attendance/absentees",
        Access::Any(&[P::AttendanceRead]),
    ),
    (
        "GET",
        "/attendance/calendar",
        Access::Any(&[P::AttendanceRead]),
    ),
    (
        "POST",
        "/attendance/calendar/holidays",
        Access::Any(&[P::AcademicsManage]),
    ),
    (
        "DELETE",
        "/attendance/calendar/holidays/{id}",
        Access::Any(&[P::AcademicsManage]),
    ),
    (
        "PUT",
        "/attendance/calendar/weekend",
        Access::Any(&[P::AcademicsManage]),
    ),
    (
        "GET",
        "/attendance/register",
        Access::Any(&[P::AttendanceRead]),
    ),
    (
        "GET",
        "/attendance/sheet",
        Access::Any(&[P::AttendanceRead]),
    ),
    (
        "PUT",
        "/attendance/sheet",
        Access::Any(&[P::AttendanceManage]),
    ),
    (
        "GET",
        "/attendance/sheet/history",
        Access::Any(&[P::AttendanceRead]),
    ),
    ("GET", "/attendance/staff", Access::Any(&[P::TeachersRead])),
    (
        "PUT",
        "/attendance/staff",
        Access::Any(&[P::TeachersManage]),
    ),
    (
        "GET",
        "/attendance/staff/{id}",
        Access::Any(&[P::TeachersRead]),
    ),
    (
        "GET",
        "/attendance/status",
        Access::Any(&[P::AttendanceRead]),
    ),
    (
        "GET",
        "/attendance/students/{id}",
        Access::Any(&[P::AttendanceRead]),
    ),
    ("GET", "/dashboard", Access::Any(&[P::StudentsRead])),
    // audit
    ("GET", "/audit", Access::Any(&[P::AuditRead])),
    // auth
    ("POST", "/auth/login", Access::Public),
    ("POST", "/auth/logout", Access::SignedIn),
    ("GET", "/auth/me", Access::SignedIn),
    ("GET", "/auth/options", Access::Public),
    ("POST", "/auth/password", Access::SignedIn),
    // backup
    ("GET", "/backup/files", Access::Any(&[P::BackupManage])),
    ("POST", "/backup/password", Access::Any(&[P::BackupManage])),
    ("POST", "/backup/preview", Access::Any(&[P::BackupManage])),
    ("POST", "/backup/restore", Access::Any(&[P::BackupManage])),
    ("POST", "/backup/run", Access::Any(&[P::BackupManage])),
    ("GET", "/backup/runs", Access::Any(&[P::BackupManage])),
    ("GET", "/backup/settings", Access::Any(&[P::BackupManage])),
    ("PUT", "/backup/settings", Access::Any(&[P::BackupManage])),
    ("GET", "/backup/status", Access::Any(&[P::BackupManage])),
    ("POST", "/backup/unlock", Access::Any(&[P::BackupManage])),
    // boards
    ("GET", "/boards", Access::Any(&[P::ExamsRead])),
    ("POST", "/boards", Access::Any(&[P::AcademicsManage])),
    (
        "PUT",
        "/boards/candidates/{id}",
        Access::Any(&[P::AcademicsManage]),
    ),
    (
        "DELETE",
        "/boards/candidates/{id}",
        Access::Any(&[P::AcademicsManage]),
    ),
    ("GET", "/boards/exams", Access::Any(&[P::ExamsRead])),
    ("POST", "/boards/exams", Access::Any(&[P::AcademicsManage])),
    ("GET", "/boards/exams/{id}", Access::Any(&[P::ExamsRead])),
    (
        "PUT",
        "/boards/exams/{id}",
        Access::Any(&[P::AcademicsManage]),
    ),
    (
        "DELETE",
        "/boards/exams/{id}",
        Access::Any(&[P::AcademicsManage]),
    ),
    (
        "GET",
        "/boards/exams/{id}/candidates",
        Access::Any(&[P::ExamsRead]),
    ),
    (
        "POST",
        "/boards/exams/{id}/candidates",
        Access::Any(&[P::AcademicsManage]),
    ),
    (
        "GET",
        "/boards/exams/{id}/results",
        Access::Any(&[P::ExamsRead]),
    ),
    (
        "POST",
        "/boards/exams/{id}/results/apply",
        Access::Any(&[P::AcademicsManage]),
    ),
    (
        "POST",
        "/boards/exams/{id}/results/parse",
        Access::Any(&[P::AcademicsManage]),
    ),
    (
        "POST",
        "/boards/exams/{id}/results/preview",
        Access::Any(&[P::AcademicsManage]),
    ),
    ("PUT", "/boards/{id}", Access::Any(&[P::AcademicsManage])),
    ("DELETE", "/boards/{id}", Access::Any(&[P::AcademicsManage])),
    (
        "GET",
        "/students/{id}/board-results",
        Access::Any(&[P::ExamsRead]),
    ),
    // consents
    ("GET", "/consent-types", Access::Any(&[P::StudentsRead])),
    ("POST", "/consent-types", Access::Any(&[P::AcademicsManage])),
    (
        "PUT",
        "/consent-types/{id}",
        Access::Any(&[P::AcademicsManage]),
    ),
    (
        "DELETE",
        "/consent-types/{id}",
        Access::Any(&[P::AcademicsManage]),
    ),
    (
        "GET",
        "/sections/{id}/consents",
        Access::Any(&[P::StudentsRead]),
    ),
    (
        "POST",
        "/sections/{id}/consents",
        Access::Any(&[P::StudentsManage]),
    ),
    (
        "GET",
        "/students/{id}/consents",
        Access::Any(&[P::StudentsRead]),
    ),
    (
        "POST",
        "/students/{id}/consents",
        Access::Any(&[P::StudentsManage]),
    ),
    // directory
    (
        "GET",
        "/settings/directory",
        Access::Any(&[P::SettingsManage]),
    ),
    (
        "PUT",
        "/settings/directory",
        Access::Any(&[P::SettingsManage]),
    ),
    (
        "POST",
        "/settings/directory/test",
        Access::Any(&[P::SettingsManage]),
    ),
    // edition
    (
        "GET",
        "/settings/edition",
        Access::Any(&[P::SettingsManage]),
    ),
    (
        "PUT",
        "/settings/edition",
        Access::Any(&[P::SettingsManage]),
    ),
    // exam_terms
    ("GET", "/exams/structure", Access::Any(&[P::ExamsRead])),
    ("GET", "/exams/terms", Access::Any(&[P::ExamsRead])),
    ("POST", "/exams/terms", Access::Any(&[P::AcademicsManage])),
    (
        "PUT",
        "/exams/terms/weights",
        Access::Any(&[P::AcademicsManage]),
    ),
    (
        "PUT",
        "/exams/terms/{id}",
        Access::Any(&[P::AcademicsManage]),
    ),
    (
        "DELETE",
        "/exams/terms/{id}",
        Access::Any(&[P::AcademicsManage]),
    ),
    // exams
    ("GET", "/exams/assessments", Access::Any(&[P::ExamsRead])),
    (
        "POST",
        "/exams/assessments",
        Access::Any(&[P::AcademicsManage]),
    ),
    (
        "POST",
        "/exams/assessments/defaults",
        Access::Any(&[P::AcademicsManage]),
    ),
    (
        "PUT",
        "/exams/assessments/weights",
        Access::Any(&[P::AcademicsManage]),
    ),
    (
        "PATCH",
        "/exams/assessments/{id}",
        Access::Any(&[P::AcademicsManage]),
    ),
    (
        "DELETE",
        "/exams/assessments/{id}",
        Access::Any(&[P::AcademicsManage]),
    ),
    ("GET", "/exams/grading", Access::Any(&[P::ExamsRead])),
    ("PUT", "/exams/grading", Access::Any(&[P::AcademicsManage])),
    ("GET", "/exams/max-marks", Access::Any(&[P::ExamsRead])),
    (
        "PUT",
        "/exams/max-marks",
        Access::Any(&[P::AcademicsManage]),
    ),
    (
        "GET",
        "/exams/report-cards/pdf",
        Access::Any(&[P::ExamsRead]),
    ),
    (
        "GET",
        "/exams/results/section",
        Access::Any(&[P::ExamsRead]),
    ),
    (
        "GET",
        "/exams/results/section/report-cards",
        Access::Any(&[P::ExamsRead]),
    ),
    (
        "GET",
        "/exams/results/student/{id}",
        Access::Any(&[P::ExamsRead]),
    ),
    ("GET", "/exams/sheet", Access::Any(&[P::ExamsRead])),
    ("PUT", "/exams/sheet", Access::Any(&[P::ExamsManage])),
    ("GET", "/exams/sheet/template", Access::Any(&[P::ExamsRead])),
    (
        "POST",
        "/exams/sheet/upload",
        Access::Any(&[P::ExamsManage]),
    ),
    ("GET", "/exams/sheets", Access::Any(&[P::ExamsRead])),
    (
        "POST",
        "/exams/sheets/{id}/approve",
        Access::Any(&[P::ExamsApprove]),
    ),
    (
        "POST",
        "/exams/sheets/{id}/reopen",
        Access::Any(&[P::ExamsApprove]),
    ),
    (
        "POST",
        "/exams/sheets/{id}/submit",
        Access::Any(&[P::ExamsManage]),
    ),
    // export
    (
        "GET",
        "/export/attendance/absentees",
        Access::Any(&[P::AttendanceRead]),
    ),
    (
        "GET",
        "/export/attendance/register",
        Access::Any(&[P::AttendanceRead]),
    ),
    (
        "GET",
        "/data-health",
        Access::Any(&[P::StudentsManage, P::SettingsManage]),
    ),
    ("GET", "/export/everything", Access::Any(&[P::BackupManage])),
    ("GET", "/export/exams/results", Access::Any(&[P::ExamsRead])),
    (
        "GET",
        "/export/fees/defaulters",
        Access::Any(&[P::FeesRead]),
    ),
    ("GET", "/export/fees/vouchers", Access::Any(&[P::FeesRead])),
    ("GET", "/export/staff", Access::Any(&[P::TeachersRead])),
    ("GET", "/export/students", Access::Any(&[P::StudentsRead])),
    // fees
    ("GET", "/fees/collections", Access::Any(&[P::FeesRead])),
    (
        "DELETE",
        "/fees/concessions/{id}",
        Access::Any(&[P::FeesApprove]),
    ),
    ("GET", "/fees/defaulters", Access::Any(&[P::FeesRead])),
    ("GET", "/fees/heads", Access::Any(&[P::FeesRead])),
    ("POST", "/fees/heads", Access::Any(&[P::FeesApprove])),
    ("PATCH", "/fees/heads/{id}", Access::Any(&[P::FeesApprove])),
    ("POST", "/fees/late-fees/run", Access::Any(&[P::FeesManage])),
    ("GET", "/fees/policy", Access::Any(&[P::FeesRead])),
    ("PUT", "/fees/policy", Access::Any(&[P::FeesApprove])),
    ("GET", "/fees/receipts/{id}", Access::Any(&[P::FeesRead])),
    (
        "POST",
        "/fees/receipts/{id}/reverse",
        Access::Any(&[P::FeesApprove]),
    ),
    ("GET", "/fees/structure", Access::Any(&[P::FeesRead])),
    ("PUT", "/fees/structure", Access::Any(&[P::FeesApprove])),
    (
        "GET",
        "/fees/students/{id}/concessions",
        Access::Any(&[P::FeesRead]),
    ),
    (
        "POST",
        "/fees/students/{id}/concessions",
        Access::Any(&[P::FeesApprove]),
    ),
    (
        "GET",
        "/fees/students/{id}/ledger",
        Access::Any(&[P::FeesRead]),
    ),
    ("GET", "/fees/vouchers", Access::Any(&[P::FeesRead])),
    (
        "POST",
        "/fees/vouchers/generate",
        Access::Any(&[P::FeesManage]),
    ),
    ("GET", "/fees/vouchers/pdf", Access::Any(&[P::FeesRead])),
    ("GET", "/fees/vouchers/{id}", Access::Any(&[P::FeesRead])),
    (
        "POST",
        "/fees/vouchers/{id}/cancel",
        Access::Any(&[P::FeesApprove]),
    ),
    (
        "POST",
        "/fees/vouchers/{id}/payments",
        Access::Any(&[P::FeesManage]),
    ),
    (
        "GET",
        "/fees/vouchers/{id}/pdf",
        Access::Any(&[P::FeesRead]),
    ),
    (
        "POST",
        "/fees/vouchers/{id}/waive-late-fee",
        Access::Any(&[P::FeesApprove]),
    ),
    // health
    ("GET", "/health", Access::Public),
    // hostel
    ("GET", "/hostel/allocations", Access::Any(&[P::HostelRead])),
    (
        "POST",
        "/hostel/allocations",
        Access::Any(&[P::HostelManage]),
    ),
    (
        "PUT",
        "/hostel/allocations/{id}",
        Access::Any(&[P::HostelManage]),
    ),
    (
        "DELETE",
        "/hostel/allocations/{id}",
        Access::Any(&[P::HostelManage]),
    ),
    ("GET", "/hostel/board", Access::Any(&[P::HostelRead])),
    ("GET", "/hostel/hostels", Access::Any(&[P::HostelRead])),
    ("POST", "/hostel/hostels", Access::Any(&[P::HostelManage])),
    ("GET", "/hostel/hostels/{id}", Access::Any(&[P::HostelRead])),
    (
        "PUT",
        "/hostel/hostels/{id}",
        Access::Any(&[P::HostelManage]),
    ),
    (
        "DELETE",
        "/hostel/hostels/{id}",
        Access::Any(&[P::HostelManage]),
    ),
    (
        "POST",
        "/hostel/hostels/{id}/rooms",
        Access::Any(&[P::HostelManage]),
    ),
    ("GET", "/hostel/register", Access::Any(&[P::HostelRead])),
    ("POST", "/hostel/register", Access::Any(&[P::HostelManage])),
    (
        "POST",
        "/hostel/register/{id}/close",
        Access::Any(&[P::HostelManage]),
    ),
    ("GET", "/hostel/residents", Access::Any(&[P::HostelRead])),
    ("PUT", "/hostel/rooms/{id}", Access::Any(&[P::HostelManage])),
    (
        "DELETE",
        "/hostel/rooms/{id}",
        Access::Any(&[P::HostelManage]),
    ),
    // imports
    (
        "POST",
        "/imports/apply",
        Access::Any(&[
            P::StudentsManage,
            P::TeachersManage,
            P::AdmissionsManage,
            P::FeesManage,
        ]),
    ),
    (
        "GET",
        "/imports/batches",
        Access::Any(&[
            P::StudentsManage,
            P::TeachersManage,
            P::AdmissionsManage,
            P::FeesManage,
        ]),
    ),
    (
        "POST",
        "/imports/batches/{id}/rollback",
        Access::Any(&[
            P::StudentsManage,
            P::TeachersManage,
            P::AdmissionsManage,
            P::AcademicsManage,
            P::FeesManage,
        ]),
    ),
    (
        "GET",
        "/imports/fields/{kind}",
        Access::Any(&[
            P::StudentsManage,
            P::TeachersManage,
            P::AdmissionsManage,
            P::FeesManage,
        ]),
    ),
    (
        "GET",
        "/imports/mappings/{kind}",
        Access::Any(&[
            P::StudentsManage,
            P::TeachersManage,
            P::AdmissionsManage,
            P::FeesManage,
        ]),
    ),
    (
        "PUT",
        "/imports/mappings/{kind}",
        Access::Any(&[
            P::StudentsManage,
            P::TeachersManage,
            P::AdmissionsManage,
            P::FeesManage,
        ]),
    ),
    (
        "POST",
        "/imports/parse",
        Access::Any(&[
            P::StudentsManage,
            P::TeachersManage,
            P::AdmissionsManage,
            P::FeesManage,
        ]),
    ),
    (
        "POST",
        "/imports/preview",
        Access::Any(&[
            P::StudentsManage,
            P::TeachersManage,
            P::AdmissionsManage,
            P::FeesManage,
        ]),
    ),
    (
        "POST",
        "/imports/report",
        Access::Any(&[
            P::StudentsManage,
            P::TeachersManage,
            P::AdmissionsManage,
            P::FeesManage,
        ]),
    ),
    (
        "GET",
        "/imports/templates/{kind}",
        Access::Any(&[
            P::StudentsManage,
            P::TeachersManage,
            P::AdmissionsManage,
            P::FeesManage,
        ]),
    ),
    // inventory
    ("GET", "/inventory/assets", Access::Any(&[P::InventoryRead])),
    (
        "POST",
        "/inventory/assets",
        Access::Any(&[P::InventoryManage]),
    ),
    (
        "PUT",
        "/inventory/assets/{id}",
        Access::Any(&[P::InventoryManage]),
    ),
    (
        "DELETE",
        "/inventory/assets/{id}",
        Access::Any(&[P::InventoryManage]),
    ),
    (
        "GET",
        "/inventory/categories",
        Access::Any(&[P::InventoryRead]),
    ),
    (
        "POST",
        "/inventory/categories",
        Access::Any(&[P::InventoryManage]),
    ),
    (
        "PUT",
        "/inventory/categories/{id}",
        Access::Any(&[P::InventoryManage]),
    ),
    (
        "DELETE",
        "/inventory/categories/{id}",
        Access::Any(&[P::InventoryManage]),
    ),
    ("GET", "/inventory/items", Access::Any(&[P::InventoryRead])),
    (
        "POST",
        "/inventory/items",
        Access::Any(&[P::InventoryManage]),
    ),
    (
        "GET",
        "/inventory/items/{id}",
        Access::Any(&[P::InventoryRead]),
    ),
    (
        "PUT",
        "/inventory/items/{id}",
        Access::Any(&[P::InventoryManage]),
    ),
    (
        "DELETE",
        "/inventory/items/{id}",
        Access::Any(&[P::InventoryManage]),
    ),
    (
        "GET",
        "/inventory/locations",
        Access::Any(&[P::InventoryRead]),
    ),
    (
        "POST",
        "/inventory/locations",
        Access::Any(&[P::InventoryManage]),
    ),
    (
        "PUT",
        "/inventory/locations/{id}",
        Access::Any(&[P::InventoryManage]),
    ),
    (
        "DELETE",
        "/inventory/locations/{id}",
        Access::Any(&[P::InventoryManage]),
    ),
    (
        "GET",
        "/inventory/movements",
        Access::Any(&[P::InventoryRead]),
    ),
    (
        "POST",
        "/inventory/movements",
        Access::Any(&[P::InventoryManage]),
    ),
    (
        "DELETE",
        "/inventory/movements/{id}",
        Access::Any(&[P::InventoryManage]),
    ),
    (
        "GET",
        "/inventory/recipients",
        Access::Any(&[P::InventoryRead]),
    ),
    ("GET", "/inventory/stock", Access::Any(&[P::InventoryRead])),
    (
        "POST",
        "/inventory/transfers",
        Access::Any(&[P::InventoryManage]),
    ),
    // leave
    ("GET", "/leave/balances", Access::Any(&[P::LeaveRead])),
    ("PUT", "/leave/balances", Access::Any(&[P::LeaveManage])),
    (
        "POST",
        "/leave/balances/carry-over",
        Access::Any(&[P::LeaveManage]),
    ),
    ("GET", "/leave/requests", Access::Any(&[P::LeaveRead])),
    (
        "POST",
        "/leave/requests",
        Access::Any(&[P::LeaveRead, P::LeaveManage]),
    ),
    // `leave.approve`, or the staff member the school named as approver (who needs only
    // `leave.read`); the handler tells them apart.
    (
        "POST",
        "/leave/requests/{id}/decide",
        Access::Any(&[P::LeaveApprove, P::LeaveRead]),
    ),
    (
        "POST",
        "/leave/requests/{id}/withdraw",
        Access::Any(&[P::LeaveRead]),
    ),
    ("GET", "/leave/settings", Access::Any(&[P::LeaveRead])),
    ("PUT", "/leave/settings", Access::Any(&[P::LeaveManage])),
    ("GET", "/leave/staff-register", Access::Any(&[P::LeaveRead])),
    ("GET", "/leave/students", Access::Any(&[P::LeaveRead])),
    ("POST", "/leave/students", Access::Any(&[P::LeaveManage])),
    (
        "POST",
        "/leave/students/{id}/decide",
        Access::Any(&[P::LeaveRead]),
    ),
    (
        "POST",
        "/leave/students/{id}/withdraw",
        Access::Any(&[P::LeaveManage]),
    ),
    ("GET", "/leave/types", Access::Any(&[P::LeaveRead])),
    ("POST", "/leave/types", Access::Any(&[P::LeaveManage])),
    ("PUT", "/leave/types/{id}", Access::Any(&[P::LeaveManage])),
    (
        "DELETE",
        "/leave/types/{id}",
        Access::Any(&[P::LeaveManage]),
    ),
    // library
    ("GET", "/library/books", Access::Any(&[P::LibraryRead])),
    ("POST", "/library/books", Access::Any(&[P::LibraryManage])),
    ("GET", "/library/books/{id}", Access::Any(&[P::LibraryRead])),
    (
        "PUT",
        "/library/books/{id}",
        Access::Any(&[P::LibraryManage]),
    ),
    (
        "DELETE",
        "/library/books/{id}",
        Access::Any(&[P::LibraryManage]),
    ),
    (
        "POST",
        "/library/books/{id}/copies",
        Access::Any(&[P::LibraryManage]),
    ),
    ("GET", "/library/copies", Access::Any(&[P::LibraryRead])),
    (
        "GET",
        "/library/copies/lookup",
        Access::Any(&[P::LibraryRead]),
    ),
    (
        "PUT",
        "/library/copies/{id}",
        Access::Any(&[P::LibraryManage]),
    ),
    ("GET", "/library/loans", Access::Any(&[P::LibraryRead])),
    ("POST", "/library/loans", Access::Any(&[P::LibraryManage])),
    (
        "POST",
        "/library/loans/{id}/renew",
        Access::Any(&[P::LibraryManage]),
    ),
    (
        "POST",
        "/library/loans/{id}/return",
        Access::Any(&[P::LibraryManage]),
    ),
    (
        "POST",
        "/library/loans/{id}/settle",
        Access::Any(&[P::LibraryManage]),
    ),
    (
        "POST",
        "/library/loans/{id}/waive",
        Access::Any(&[P::LibraryManage]),
    ),
    ("GET", "/library/members", Access::Any(&[P::LibraryRead])),
    (
        "GET",
        "/library/members/{kind}/{id}",
        Access::Any(&[P::LibraryRead]),
    ),
    ("GET", "/library/settings", Access::Any(&[P::LibraryRead])),
    ("PUT", "/library/settings", Access::Any(&[P::LibraryManage])),
    ("GET", "/library/stock", Access::Any(&[P::LibraryRead])),
    // nexus
    // A stub that exists to exercise edition gating: on Lite and Standard it answers
    // "feature unavailable" to anyone signed in; its handler asks for `nexus.admin` (which
    // no school role holds) once the feature is on.
    ("GET", "/nexus/status", Access::SignedIn),
    // nexus_link
    ("GET", "/nexus/link", Access::Any(&[P::SettingsManage])),
    (
        "GET",
        "/nexus/link/escrow",
        Access::Any(&[P::SettingsManage]),
    ),
    (
        "POST",
        "/nexus/link/escrow",
        Access::Any(&[P::BackupManage]),
    ),
    (
        "DELETE",
        "/nexus/link/escrow",
        Access::Any(&[P::BackupManage]),
    ),
    (
        "POST",
        "/nexus/link/join",
        Access::Any(&[P::SettingsManage]),
    ),
    (
        "POST",
        "/nexus/link/leave",
        Access::Any(&[P::SettingsManage]),
    ),
    (
        "GET",
        "/nexus/link/overwrites",
        Access::Any(&[P::SettingsManage]),
    ),
    (
        "POST",
        "/nexus/link/sync",
        Access::Any(&[P::SettingsManage]),
    ),
    // parent_app
    (
        "GET",
        "/parent-app/access",
        Access::Any(&[P::ParentsManage]),
    ),
    (
        "DELETE",
        "/parent-app/access/{guardian_id}",
        Access::Any(&[P::ParentsManage]),
    ),
    (
        "POST",
        "/parent-app/access/{guardian_id}/code",
        Access::Any(&[P::ParentsManage]),
    ),
    (
        "GET",
        "/parent-app/conversations",
        Access::Any(&[P::MessagesReply, P::MessagesViewAll]),
    ),
    (
        "GET",
        "/parent-app/conversations/{student_id}/{guardian_id}",
        Access::Any(&[P::MessagesReply, P::MessagesViewAll]),
    ),
    (
        "POST",
        "/parent-app/conversations/{student_id}/{guardian_id}/messages",
        Access::Any(&[P::MessagesReply]),
    ),
    (
        "POST",
        "/parent-app/conversations/{student_id}/{guardian_id}/read",
        Access::Any(&[P::MessagesReply]),
    ),
    (
        "GET",
        "/parent-app/notices",
        Access::Any(&[P::NoticesManage]),
    ),
    (
        "POST",
        "/parent-app/notices",
        Access::Any(&[P::NoticesManage]),
    ),
    (
        "PATCH",
        "/parent-app/notices/{id}",
        Access::Any(&[P::NoticesManage]),
    ),
    (
        "DELETE",
        "/parent-app/notices/{id}",
        Access::Any(&[P::NoticesManage]),
    ),
    (
        "POST",
        "/parent-app/notices/{id}/publish",
        Access::Any(&[P::NoticesManage]),
    ),
    (
        "POST",
        "/parent-app/notices/{id}/withdraw",
        Access::Any(&[P::NoticesManage]),
    ),
    (
        "GET",
        "/parent-app/status",
        Access::Any(&[
            P::ParentsManage,
            P::NoticesManage,
            P::MessagesReply,
            P::MessagesViewAll,
        ]),
    ),
    // reports
    (
        "GET",
        "/reports",
        Access::Any(&[
            P::ReportsRead,
            P::AttendanceRead,
            P::ExamsRead,
            P::TimetableRead,
            P::LeaveRead,
            P::AdmissionsRead,
            P::LibraryRead,
            P::TransportRead,
            P::HostelRead,
            P::InventoryRead,
        ]),
    ),
    ("GET", "/reports/custom", Access::Any(&[P::ReportsRead])),
    ("POST", "/reports/custom", Access::Any(&[P::ReportsRead])),
    (
        "POST",
        "/reports/custom/run",
        Access::Any(&[P::ReportsRead]),
    ),
    (
        "GET",
        "/reports/custom/sources",
        Access::Any(&[P::ReportsRead]),
    ),
    (
        "GET",
        "/reports/custom/{id}",
        Access::Any(&[P::ReportsRead]),
    ),
    (
        "PUT",
        "/reports/custom/{id}",
        Access::Any(&[P::ReportsRead]),
    ),
    (
        "DELETE",
        "/reports/custom/{id}",
        Access::Any(&[P::ReportsRead]),
    ),
    (
        "GET",
        "/reports/{code}",
        Access::Any(&[
            P::ReportsRead,
            P::AttendanceRead,
            P::ExamsRead,
            P::TimetableRead,
            P::LeaveRead,
            P::AdmissionsRead,
            P::LibraryRead,
            P::TransportRead,
            P::HostelRead,
            P::InventoryRead,
        ]),
    ),
    // rollover
    (
        "POST",
        "/academic-years/rollover",
        Access::Any(&[P::AcademicsManage]),
    ),
    (
        "GET",
        "/academic-years/rollover/preview",
        Access::Any(&[P::AcademicsManage]),
    ),
    // school_settings
    ("GET", "/settings/school", Access::Any(&[P::SettingsManage])),
    ("PUT", "/settings/school", Access::Any(&[P::SettingsManage])),
    // schools
    ("GET", "/schools/current", Access::Any(&[P::SchoolRead])),
    (
        "PATCH",
        "/schools/current",
        Access::Any(&[P::SchoolManage, P::SettingsManage]),
    ),
    // setup
    ("POST", "/setup", Access::Public),
    ("POST", "/setup/demo", Access::Public),
    ("GET", "/setup/status", Access::Public),
    // staff
    ("GET", "/staff", Access::Any(&[P::TeachersRead])),
    ("POST", "/staff", Access::Any(&[P::TeachersManage])),
    ("GET", "/staff/{id}", Access::Any(&[P::TeachersRead])),
    ("PATCH", "/staff/{id}", Access::Any(&[P::TeachersManage])),
    (
        "GET",
        "/staff/{id}/assignments",
        Access::Any(&[P::TeachersRead]),
    ),
    (
        "PUT",
        "/staff/{id}/assignments",
        Access::Any(&[P::TeachersManage]),
    ),
    ("POST", "/staff/{id}/login", Access::Any(&[P::UsersManage])),
    ("PATCH", "/staff/{id}/login", Access::Any(&[P::UsersManage])),
    (
        "POST",
        "/staff/{id}/login/password",
        Access::Any(&[P::UsersManage]),
    ),
    // students
    ("GET", "/guardians", Access::Any(&[P::StudentsRead])),
    ("POST", "/guardians", Access::Any(&[P::StudentsManage])),
    ("GET", "/guardians/{id}", Access::Any(&[P::StudentsRead])),
    (
        "PATCH",
        "/guardians/{id}",
        Access::Any(&[P::StudentsManage]),
    ),
    ("GET", "/students", Access::Any(&[P::StudentsRead])),
    ("POST", "/students", Access::Any(&[P::StudentsManage])),
    (
        "POST",
        "/students/check-duplicates",
        Access::Any(&[P::StudentsManage]),
    ),
    ("GET", "/students/{id}", Access::Any(&[P::StudentsRead])),
    ("PATCH", "/students/{id}", Access::Any(&[P::StudentsManage])),
    (
        "POST",
        "/students/{id}/enrol",
        Access::Any(&[P::StudentsManage]),
    ),
    (
        "POST",
        "/students/{id}/guardians",
        Access::Any(&[P::StudentsManage]),
    ),
    (
        "DELETE",
        "/students/{id}/guardians/{guardian_id}",
        Access::Any(&[P::StudentsManage]),
    ),
    (
        "GET",
        "/students/{id}/photo",
        Access::Any(&[P::StudentsRead]),
    ),
    (
        "PUT",
        "/students/{id}/photo",
        Access::Any(&[P::StudentsManage]),
    ),
    (
        "DELETE",
        "/students/{id}/photo",
        Access::Any(&[P::StudentsManage]),
    ),
    (
        "POST",
        "/students/{id}/withdraw",
        Access::Any(&[P::StudentsManage]),
    ),
    // subjects
    (
        "GET",
        "/classes/{id}/subject-groups",
        Access::Any(&[P::AcademicsRead]),
    ),
    (
        "POST",
        "/classes/{id}/subject-groups",
        Access::Any(&[P::AcademicsManage]),
    ),
    (
        "GET",
        "/sections/{id}/subject-allocations",
        Access::Any(&[P::StudentsRead]),
    ),
    (
        "PUT",
        "/sections/{id}/subject-allocations",
        Access::Any(&[P::StudentsManage]),
    ),
    (
        "GET",
        "/students/{id}/subjects",
        Access::Any(&[P::StudentsRead]),
    ),
    (
        "PUT",
        "/students/{id}/subjects",
        Access::Any(&[P::StudentsManage]),
    ),
    (
        "POST",
        "/subject-allocations/import/apply",
        Access::Any(&[P::StudentsManage]),
    ),
    (
        "POST",
        "/subject-allocations/import/parse",
        Access::Any(&[P::StudentsManage]),
    ),
    (
        "POST",
        "/subject-allocations/import/preview",
        Access::Any(&[P::StudentsManage]),
    ),
    (
        "GET",
        "/subject-allocations/import/template",
        Access::Any(&[P::StudentsRead]),
    ),
    (
        "PUT",
        "/subject-groups/{id}",
        Access::Any(&[P::AcademicsManage]),
    ),
    (
        "DELETE",
        "/subject-groups/{id}",
        Access::Any(&[P::AcademicsManage]),
    ),
    // system
    ("GET", "/system/features", Access::Public),
    ("GET", "/system/locale", Access::Public),
    // timetable
    (
        "POST",
        "/timetable/absences",
        Access::Any(&[P::TimetableManage]),
    ),
    (
        "DELETE",
        "/timetable/absences/{id}",
        Access::Any(&[P::TimetableManage]),
    ),
    (
        "GET",
        "/timetable/cover-sheet",
        Access::Any(&[P::TimetableRead]),
    ),
    (
        "GET",
        "/timetable/cover-sheet/pdf",
        Access::Any(&[P::TimetableRead]),
    ),
    (
        "PUT",
        "/timetable/covers",
        Access::Any(&[P::TimetableManage]),
    ),
    (
        "DELETE",
        "/timetable/covers",
        Access::Any(&[P::TimetableManage]),
    ),
    (
        "GET",
        "/timetable/current",
        Access::Any(&[P::TimetableRead]),
    ),
    (
        "POST",
        "/timetable/generate",
        Access::Any(&[P::TimetableManage]),
    ),
    (
        "GET",
        "/timetable/generate/{id}",
        Access::Any(&[P::TimetableRead]),
    ),
    (
        "POST",
        "/timetable/generate/{id}/cancel",
        Access::Any(&[P::TimetableManage]),
    ),
    (
        "POST",
        "/timetable/lessons",
        Access::Any(&[P::TimetableManage]),
    ),
    (
        "POST",
        "/timetable/lessons/seed",
        Access::Any(&[P::TimetableManage]),
    ),
    (
        "PUT",
        "/timetable/lessons/{id}",
        Access::Any(&[P::TimetableManage]),
    ),
    (
        "DELETE",
        "/timetable/lessons/{id}",
        Access::Any(&[P::TimetableManage]),
    ),
    ("GET", "/timetable/mine", Access::Any(&[P::TimetableRead])),
    ("GET", "/timetable/print", Access::Any(&[P::TimetableRead])),
    ("GET", "/timetable/rooms", Access::Any(&[P::TimetableRead])),
    (
        "POST",
        "/timetable/rooms",
        Access::Any(&[P::TimetableManage]),
    ),
    (
        "PUT",
        "/timetable/rooms/{id}",
        Access::Any(&[P::TimetableManage]),
    ),
    (
        "DELETE",
        "/timetable/rooms/{id}",
        Access::Any(&[P::TimetableManage]),
    ),
    (
        "PUT",
        "/timetable/rules",
        Access::Any(&[P::TimetableManage]),
    ),
    ("GET", "/timetable/setup", Access::Any(&[P::TimetableRead])),
    (
        "POST",
        "/timetable/shifts",
        Access::Any(&[P::TimetableManage]),
    ),
    (
        "POST",
        "/timetable/shifts/default",
        Access::Any(&[P::TimetableManage]),
    ),
    (
        "PUT",
        "/timetable/shifts/{id}",
        Access::Any(&[P::TimetableManage]),
    ),
    (
        "DELETE",
        "/timetable/shifts/{id}",
        Access::Any(&[P::TimetableManage]),
    ),
    (
        "GET",
        "/timetable/substitutions",
        Access::Any(&[P::TimetableRead]),
    ),
    (
        "POST",
        "/timetable/timetables",
        Access::Any(&[P::TimetableManage]),
    ),
    (
        "GET",
        "/timetable/timetables/{id}",
        Access::Any(&[P::TimetableRead]),
    ),
    (
        "PATCH",
        "/timetable/timetables/{id}",
        Access::Any(&[P::TimetableManage]),
    ),
    (
        "DELETE",
        "/timetable/timetables/{id}",
        Access::Any(&[P::TimetableManage]),
    ),
    (
        "POST",
        "/timetable/timetables/{id}/lock",
        Access::Any(&[P::TimetableManage]),
    ),
    (
        "POST",
        "/timetable/timetables/{id}/move",
        Access::Any(&[P::TimetableManage]),
    ),
    (
        "GET",
        "/timetable/timetables/{id}/options",
        Access::Any(&[P::TimetableRead]),
    ),
    (
        "POST",
        "/timetable/timetables/{id}/publish",
        Access::Any(&[P::TimetableManage]),
    ),
    (
        "POST",
        "/timetable/timetables/{id}/swap",
        Access::Any(&[P::TimetableManage]),
    ),
    (
        "GET",
        "/timetable/timetables/{id}/swap-options",
        Access::Any(&[P::TimetableRead]),
    ),
    // transport
    (
        "GET",
        "/transport/assignments",
        Access::Any(&[P::TransportRead]),
    ),
    (
        "POST",
        "/transport/assignments",
        Access::Any(&[P::TransportManage]),
    ),
    (
        "POST",
        "/transport/assignments/bulk",
        Access::Any(&[P::TransportManage]),
    ),
    (
        "PUT",
        "/transport/assignments/{id}",
        Access::Any(&[P::TransportManage]),
    ),
    (
        "DELETE",
        "/transport/assignments/{id}",
        Access::Any(&[P::TransportManage]),
    ),
    ("GET", "/transport/routes", Access::Any(&[P::TransportRead])),
    (
        "POST",
        "/transport/routes",
        Access::Any(&[P::TransportManage]),
    ),
    (
        "GET",
        "/transport/routes/{id}",
        Access::Any(&[P::TransportRead]),
    ),
    (
        "PUT",
        "/transport/routes/{id}",
        Access::Any(&[P::TransportManage]),
    ),
    (
        "DELETE",
        "/transport/routes/{id}",
        Access::Any(&[P::TransportManage]),
    ),
    (
        "GET",
        "/transport/routes/{id}/roster",
        Access::Any(&[P::TransportRead]),
    ),
    (
        "GET",
        "/transport/settings",
        Access::Any(&[P::TransportRead]),
    ),
    (
        "PUT",
        "/transport/settings",
        Access::Any(&[P::TransportManage]),
    ),
    (
        "GET",
        "/transport/students",
        Access::Any(&[P::TransportRead]),
    ),
    (
        "GET",
        "/transport/vehicles",
        Access::Any(&[P::TransportRead]),
    ),
    (
        "POST",
        "/transport/vehicles",
        Access::Any(&[P::TransportManage]),
    ),
    (
        "PUT",
        "/transport/vehicles/{id}",
        Access::Any(&[P::TransportManage]),
    ),
    (
        "DELETE",
        "/transport/vehicles/{id}",
        Access::Any(&[P::TransportManage]),
    ),
    // users
    (
        "GET",
        "/users",
        Access::Any(&[P::UsersRead, P::UsersManage]),
    ),
    ("POST", "/users", Access::Any(&[P::UsersManage])),
    ("PATCH", "/users/{id}", Access::Any(&[P::UsersManage])),
    (
        "POST",
        "/users/{id}/password",
        Access::Any(&[P::UsersManage]),
    ),
];

static INDEX: LazyLock<HashMap<(&'static str, &'static str), Access>> =
    LazyLock::new(|| ROUTES.iter().map(|(m, p, a)| ((*m, *p), *a)).collect());

/// The rule for a method and a route pattern (`/students/{id}`), if the table has one.
pub fn rule(method: &str, route: &str) -> Option<Access> {
    INDEX.get(&(method, route)).copied()
}

/// The gate. Runs after routing (so the matched pattern is known) and before extraction.
pub async fn layer(State(state): State<AppState>, req: Request, next: Next) -> Response {
    // No route matched: the router answers 404 itself.
    let Some(matched) = req.extensions().get::<MatchedPath>().cloned() else {
        return next.run(req).await;
    };
    let pattern = matched
        .as_str()
        .strip_prefix(API_PREFIX)
        .unwrap_or(matched.as_str());
    let Some(access) = rule(req.method().as_str(), pattern) else {
        tracing::error!(method = %req.method(), route = pattern, "route missing from the authorisation table; refused");
        return ApiError::forbidden().into_response();
    };
    if access == Access::Public {
        return next.run(req).await;
    }
    let (mut parts, body) = req.into_parts();
    let user = match CurrentUser::from_request_parts(&mut parts, &state).await {
        Ok(user) => user,
        Err(e) => return e.into_response(),
    };
    if !access.allows(&user) {
        tracing::warn!(user = %user.user.username, method = %parts.method, route = pattern, "permission denied");
        return ApiError::forbidden().into_response();
    }
    // The handler's own `CurrentUser` is this one: the session is read once per request.
    parts.extensions.insert(user);
    next.run(Request::from_parts(parts, body)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_has_no_route_twice_and_no_empty_rule() {
        assert_eq!(INDEX.len(), ROUTES.len(), "a route is listed twice");
        for (method, route, access) in ROUTES {
            assert!(route.starts_with('/'), "{route}");
            assert!(matches!(
                *method,
                "GET" | "POST" | "PUT" | "PATCH" | "DELETE"
            ));
            if let Access::Any(permissions) = access {
                assert!(
                    !permissions.is_empty(),
                    "{method} {route} names no permission"
                );
            }
        }
    }

    #[test]
    fn only_what_must_be_open_is_public() {
        let mut public: Vec<String> = ROUTES
            .iter()
            .filter(|(_, _, a)| *a == Access::Public)
            .map(|(m, p, _)| format!("{m} {p}"))
            .collect();
        public.sort();
        assert_eq!(
            public,
            [
                "GET /auth/options",
                "GET /health",
                "GET /setup/status",
                "GET /system/features",
                "GET /system/locale",
                "POST /auth/login",
                "POST /setup",
                "POST /setup/demo",
            ]
        );
    }
}
