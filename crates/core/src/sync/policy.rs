//! How each table syncs. Every table of the school schema is listed here exactly once; a
//! database test fails when a migration adds a table without classifying it.

/// How the records of one table are merged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Field by field. When both sides changed the same field since the last sync, head
    /// office wins and the losing value is logged.
    Fields,
    /// Records are never changed after they are written: the result is the union of both
    /// sides (receipts, the audit log).
    Append,
    /// Attendance: marks from both sides are always merged; where both marked the same
    /// record the later mark stands. Head office does not win here.
    Attendance,
    /// Stays on the side that made it; never sent.
    Skip,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Fields => "fields",
            Mode::Append => "append",
            Mode::Attendance => "attendance",
            Mode::Skip => "skip",
        }
    }
}

/// The sync rule of one table.
#[derive(Debug, Clone, Copy)]
pub struct TablePolicy {
    pub table: &'static str,
    pub mode: Mode,
    /// Columns that never leave the side that owns them (password hashes, lockout
    /// counters). The second value is the JSON literal the receiver stores in a new
    /// record; `None` leaves the column to its database default.
    pub exclude: &'static [(&'static str, Option<&'static str>)],
    /// Columns computed from other records (a voucher's paid amount, a sheet's totals).
    /// They are never sent as changes; each side recomputes them after a merge.
    pub derived: &'static [&'static str],
    /// When set, only conflicts on these fields are written to the overwrite log; the
    /// others still merge, silently. Attendance logs marks, not bookkeeping.
    pub log_only: &'static [&'static str],
    /// A conflict on (field, local value, incoming value) that needs no log entry because
    /// both values are recomputed anyway (a voucher's payment status).
    pub quiet: Option<fn(&str, &serde_json::Value, &serde_json::Value) -> bool>,
}

impl TablePolicy {
    pub fn is_synced(&self) -> bool {
        self.mode != Mode::Skip
    }

    pub fn is_excluded(&self, column: &str) -> bool {
        self.exclude.iter().any(|(c, _)| *c == column)
    }

    pub fn is_derived(&self, column: &str) -> bool {
        self.derived.contains(&column)
    }

    /// Should a conflict on this field be written to the overwrite log?
    pub fn logs(
        &self,
        field: &str,
        local: &serde_json::Value,
        incoming: &serde_json::Value,
    ) -> bool {
        if !self.log_only.is_empty() && !self.log_only.contains(&field) {
            return false;
        }
        !self
            .quiet
            .is_some_and(|quiet| quiet(field, local, incoming))
    }

    /// Columns whose local changes never make a record "changed": excluded ones and the
    /// bookkeeping columns that change with every edit. (A change to a derived column alone
    /// sends the bookkeeping columns and nothing else.)
    pub fn untracked_columns(&self) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = self.exclude.iter().map(|(c, _)| *c).collect();
        out.extend(META_FIELDS);
        out
    }
}

/// Bookkeeping columns. They follow the later stamp and are never reported as conflicts.
pub const META_FIELDS: [&str; 3] = ["updated_at", "updated_by", "hlc"];

const fn fields(table: &'static str) -> TablePolicy {
    TablePolicy {
        table,
        mode: Mode::Fields,
        exclude: &[],
        derived: &[],
        log_only: &[],
        quiet: None,
    }
}

const fn skip(table: &'static str) -> TablePolicy {
    TablePolicy {
        table,
        mode: Mode::Skip,
        exclude: &[],
        derived: &[],
        log_only: &[],
        quiet: None,
    }
}

/// Voucher statuses that follow from the receipts. `carried` and `cancelled` are decisions
/// a person made and are not recomputed.
pub const PAYMENT_STATUSES: [&str; 3] = ["unpaid", "partial", "paid"];

/// A voucher's payment status is recomputed from its receipts after every merge, so two
/// sides that each took a payment do not have a conflict worth logging.
fn voucher_status_is_recomputed(
    field: &str,
    local: &serde_json::Value,
    incoming: &serde_json::Value,
) -> bool {
    let payment = |v: &serde_json::Value| v.as_str().is_some_and(|s| PAYMENT_STATUSES.contains(&s));
    field == "status" && payment(local) && payment(incoming)
}

/// Attendance tables: marks merge by the later mark; only a replaced mark is logged.
const fn attendance(table: &'static str, derived: &'static [&'static str]) -> TablePolicy {
    TablePolicy {
        table,
        mode: Mode::Attendance,
        exclude: &[],
        derived,
        log_only: &["status"],
        quiet: None,
    }
}

const fn with_mode(table: &'static str, mode: Mode) -> TablePolicy {
    TablePolicy {
        table,
        mode,
        exclude: &[],
        derived: &[],
        log_only: &[],
        quiet: None,
    }
}

/// Every table of the school schema, in alphabetical order.
pub const TABLES: &[TablePolicy] = &[
    fields("academic_years"),
    fields("admission_applications"),
    fields("admission_events"),
    fields("admission_stages"),
    // API keys are credentials of one install; their call log is local too.
    skip("api_key_calls"),
    skip("api_keys"),
    fields("assessment_max_marks"),
    fields("assessment_types"),
    attendance("attendance_entries", &[]),
    // The totals are counted from the entries on each side after every merge.
    attendance("attendance_sheets", &["present", "absent", "late", "leave"]),
    // Append-only by database trigger.
    with_mode("audit_log", Mode::Append),
    // Where and when this install made its backups.
    skip("backup_runs"),
    fields("board_candidates"),
    fields("board_exam_classes"),
    fields("board_exam_subjects"),
    fields("board_exams"),
    fields("board_results"),
    fields("class_subjects"),
    fields("classes"),
    fields("consent_types"),
    fields("consents"),
    fields("custom_field_definitions"),
    fields("enrollments"),
    fields("exam_boards"),
    fields("fee_concessions"),
    fields("fee_heads"),
    // Immutable by database trigger; corrections are reversal receipts.
    with_mode("fee_receipts", Mode::Append),
    fields("fee_structures"),
    fields("fee_voucher_lines"),
    TablePolicy {
        table: "fee_vouchers",
        mode: Mode::Fields,
        exclude: &[],
        // The paid amount is the sum of the voucher's receipts on each side, so receipts
        // taken on both sides add up instead of one total replacing the other.
        derived: &["net_paid"],
        log_only: &[],
        quiet: Some(voucher_status_is_recomputed),
    },
    fields("guardians"),
    fields("holidays"),
    fields("hostel_allocations"),
    fields("hostel_register"),
    fields("hostel_rooms"),
    fields("hostels"),
    // The staged rows of an import are working data of the wizard; the batch itself is
    // synced because imported results refer to it.
    skip("import_batch_rows"),
    fields("import_batches"),
    fields("inventory_assets"),
    fields("inventory_categories"),
    fields("inventory_items"),
    fields("inventory_locations"),
    fields("inventory_movements"),
    fields("leave_balances"),
    fields("leave_requests"),
    fields("leave_types"),
    fields("library_books"),
    fields("library_copies"),
    fields("library_loans"),
    fields("mark_sheets"),
    fields("marks"),
    fields("permissions"),
    fields("report_definitions"),
    fields("roles"),
    fields("rooms"),
    // Number counters belong to the side that issues the numbers.
    skip("school_sequences"),
    fields("schools"),
    fields("section_teachers"),
    fields("sections"),
    // Login sessions of one install.
    skip("sessions"),
    // Holds sealed secrets, key material and install-only switches (node id, edition).
    skip("settings"),
    fields("staff"),
    attendance("staff_attendance", &[]),
    fields("student_guardians"),
    fields("student_leave_requests"),
    fields("student_subjects"),
    fields("students"),
    fields("subject_group_subjects"),
    fields("subject_groups"),
    fields("subjects"),
    fields("terms"),
    fields("timetable_absences"),
    fields("timetable_cards"),
    fields("timetable_covers"),
    fields("timetable_lessons"),
    fields("timetable_rules"),
    fields("timetable_shifts"),
    fields("timetables"),
    fields("transport_assignments"),
    fields("transport_routes"),
    fields("transport_stops"),
    fields("transport_vehicles"),
    fields("user_roles"),
    TablePolicy {
        table: "users",
        mode: Mode::Fields,
        // A school's password hashes and lockout state never leave the school. The hub
        // stores a placeholder that matches no password, so nobody can log in with it.
        exclude: &[
            ("password_hash", Some("\"!\"")),
            ("failed_login_count", None),
            ("locked_until", None),
            ("last_login_at", None),
            ("directory_synced_at", None),
        ],
        derived: &[],
        log_only: &[],
        quiet: None,
    },
];

/// The rule for a table, or `None` for a table this version does not know.
pub fn policy(table: &str) -> Option<&'static TablePolicy> {
    TABLES.iter().find(|p| p.table == table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_are_listed_once_and_in_order() {
        let names: Vec<&str> = TABLES.iter().map(|p| p.table).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(names, sorted, "alphabetical, no duplicates");
    }

    #[test]
    fn receipts_and_attendance_never_use_the_head_office_rule() {
        assert_eq!(policy("fee_receipts").unwrap().mode, Mode::Append);
        assert_eq!(policy("audit_log").unwrap().mode, Mode::Append);
        for t in [
            "attendance_sheets",
            "attendance_entries",
            "staff_attendance",
        ] {
            assert_eq!(policy(t).unwrap().mode, Mode::Attendance, "{t}");
        }
        assert_eq!(policy("students").unwrap().mode, Mode::Fields);
        assert!(policy("no_such_table").is_none());
    }

    #[test]
    fn derived_values_are_recomputed_not_argued_over() {
        use serde_json::json;
        let v = policy("fee_vouchers").unwrap();
        assert!(v.is_derived("net_paid"));
        assert!(
            !v.untracked_columns().contains(&"net_paid"),
            "derived, not excluded"
        );
        // Two payment statuses: recomputed, nothing to log. A cancellation is a decision.
        assert!(!v.logs("status", &json!("partial"), &json!("paid")));
        assert!(v.logs("status", &json!("cancelled"), &json!("paid")));
        assert!(v.logs("status", &json!("partial"), &json!("carried")));
        assert!(v.logs("due_date", &json!("2026-05-10"), &json!("2026-05-15")));
        let sheet = policy("attendance_sheets").unwrap();
        assert!(sheet.is_derived("present") && sheet.is_derived("leave"));
        // Attendance logs a replaced mark, not bookkeeping fields.
        let e = policy("attendance_entries").unwrap();
        assert!(e.logs("status", &json!("absent"), &json!("present")));
        assert!(!e.logs(
            "deleted_at",
            &json!(null),
            &json!("2026-05-01T00:00:00+00:00")
        ));
        assert!(!e.logs("sheet_id", &json!("a"), &json!("b")));
        assert!(policy("students")
            .unwrap()
            .logs("full_name", &json!("a"), &json!("b")));
    }

    #[test]
    fn secrets_and_local_state_stay_at_home() {
        for t in [
            "sessions",
            "settings",
            "api_keys",
            "api_key_calls",
            "backup_runs",
            "school_sequences",
        ] {
            assert!(!policy(t).unwrap().is_synced(), "{t} must not sync");
        }
        let users = policy("users").unwrap();
        assert!(users.is_excluded("password_hash"));
        assert!(users.is_excluded("locked_until"));
        assert!(!users.is_excluded("full_name"));
        assert!(users.untracked_columns().contains(&"password_hash"));
        assert!(users.untracked_columns().contains(&"hlc"));
        // The placeholder is valid JSON and is not a usable Argon2 hash.
        let placeholder = users
            .exclude
            .iter()
            .find(|(c, _)| *c == "password_hash")
            .unwrap()
            .1
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(placeholder).unwrap();
        assert_eq!(value, "!");
    }
}
