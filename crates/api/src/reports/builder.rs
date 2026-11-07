//! Custom report builder (M2 stage 3). A definition names a source and picks fields,
//! filters, sort, grouping and a limit from a fixed catalogue; SQL is assembled from the
//! catalogue's own expressions with every user value bound as a parameter. Nothing typed by
//! the user is ever concatenated into SQL.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, Row};

use super::text::{tr, value};
use super::{format_date, money, Column, Context, Group, Lang, Report};
use crate::ApiError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    Text,
    Number,
    Money,
    Date,
    Boolean,
    Choice,
}

#[derive(Debug, Clone, Copy)]
pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    pub ty: FieldType,
    /// SQL expression over the source's aliases. Cast to text when selected.
    pub sql: &'static str,
    /// Allowed values for a choice field (stored values; labels come from `value.*`).
    pub choices: &'static [&'static str],
}

const fn f(key: &'static str, label: &'static str, ty: FieldType, sql: &'static str) -> Field {
    Field {
        key,
        label,
        ty,
        sql,
        choices: &[],
    }
}

const fn choice(
    key: &'static str,
    label: &'static str,
    sql: &'static str,
    choices: &'static [&'static str],
) -> Field {
    Field {
        key,
        label,
        ty: FieldType::Choice,
        sql,
        choices,
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Source {
    pub key: &'static str,
    pub label: &'static str,
    /// `FROM … WHERE …` with `$1` = school id, `$2` = academic year id and `$3` = the
    /// caller's section scope (`uuid[]`, NULL for the whole school). Filters are appended.
    pub base: &'static str,
    pub fields: &'static [Field],
}

const GENDERS: &[&str] = &["male", "female", "other"];
const STUDENT_STATUS: &[&str] = &["active", "left", "graduated"];
const RELATIONSHIPS: &[&str] = &["father", "mother", "guardian", "other"];
const VOUCHER_STATUS: &[&str] = &["unpaid", "partial", "paid", "carried", "cancelled"];
const METHODS: &[&str] = &["cash", "bank", "online", "one_bill", "kuickpay"];
const ATT_STATUS: &[&str] = &["present", "absent", "late", "leave"];
const SHEET_STATUS: &[&str] = &["draft", "submitted", "approved"];
const STAFF_STATUS: &[&str] = &["active", "left"];

pub const SOURCES: &[Source] = &[
    Source {
        key: "students",
        label: "source.students",
        base: "FROM students s
         LEFT JOIN enrollments e ON e.student_id = s.id AND e.academic_year_id = $2 AND e.deleted_at IS NULL
         LEFT JOIN sections sec ON sec.id = e.section_id
         LEFT JOIN classes c ON c.id = sec.class_id
         LEFT JOIN LATERAL (
         SELECT g.full_name, g.phone, sg.relationship FROM student_guardians sg JOIN guardians g ON g.id = sg.guardian_id
         WHERE sg.student_id = s.id AND sg.deleted_at IS NULL AND g.deleted_at IS NULL
         ORDER BY sg.is_primary DESC, g.full_name LIMIT 1) pg ON TRUE
         WHERE s.school_id = $1 AND s.deleted_at IS NULL AND ($3::uuid[] IS NULL OR e.section_id = ANY($3))",
        fields: &[
            f("student_number", "col.student_number", FieldType::Text, "s.student_number"),
            f("full_name", "col.name", FieldType::Text, "s.full_name"),
            f("father_name", "col.father_name", FieldType::Text, "s.father_name"),
            choice("gender", "col.gender", "s.gender", GENDERS),
            f("date_of_birth", "col.date_of_birth", FieldType::Date, "s.date_of_birth"),
            f("cnic", "col.cnic", FieldType::Text, "s.cnic"),
            f("address", "col.address", FieldType::Text, "s.address"),
            f("admission_date", "col.admission_date", FieldType::Date, "s.admission_date"),
            choice("status", "col.status", "s.status", STUDENT_STATUS),
            f("left_on", "field.left_on", FieldType::Date, "s.left_on"),
            f("left_reason", "col.reason", FieldType::Text, "s.left_reason"),
            f("class", "col.class", FieldType::Text, "c.name"),
            f("class_level", "field.class_level", FieldType::Number, "c.level"),
            f("section", "col.section", FieldType::Text, "sec.name"),
            f("roll_no", "col.roll_no", FieldType::Number, "e.roll_no"),
            f("guardian_name", "col.guardian", FieldType::Text, "pg.full_name"),
            f("guardian_phone", "col.phone", FieldType::Text, "pg.phone"),
            choice("guardian_relationship", "col.relationship", "pg.relationship", RELATIONSHIPS),
            f("has_photo", "field.has_photo", FieldType::Boolean, "(s.photo IS NOT NULL)"),
            f("board_roll_no", "field.board_roll_no", FieldType::Text, "s.board_roll_no"),
            f("legacy_id", "field.legacy_id", FieldType::Text, "s.legacy_id"),
        ],
    },
    Source {
        key: "guardians",
        label: "source.guardians",
        base: "FROM student_guardians sg
         JOIN guardians g ON g.id = sg.guardian_id AND g.deleted_at IS NULL
         JOIN students s ON s.id = sg.student_id AND s.deleted_at IS NULL
         LEFT JOIN enrollments e ON e.student_id = s.id AND e.academic_year_id = $2 AND e.deleted_at IS NULL
         LEFT JOIN sections sec ON sec.id = e.section_id
         LEFT JOIN classes c ON c.id = sec.class_id
         WHERE sg.school_id = $1 AND sg.deleted_at IS NULL AND ($3::uuid[] IS NULL OR e.section_id = ANY($3))",
        fields: &[
            f("guardian_name", "col.guardian", FieldType::Text, "g.full_name"),
            choice("relationship", "col.relationship", "sg.relationship", RELATIONSHIPS),
            f("is_primary", "col.primary", FieldType::Boolean, "sg.is_primary"),
            f("phone", "col.phone", FieldType::Text, "g.phone"),
            f("email", "col.email", FieldType::Text, "g.email"),
            f("cnic", "col.cnic", FieldType::Text, "g.cnic"),
            f("occupation", "col.occupation", FieldType::Text, "g.occupation"),
            f("address", "col.address", FieldType::Text, "g.address"),
            f("student_number", "col.student_number", FieldType::Text, "s.student_number"),
            f("student_name", "col.name", FieldType::Text, "s.full_name"),
            choice("student_status", "col.status", "s.status", STUDENT_STATUS),
            f("class", "col.class", FieldType::Text, "c.name"),
            f("section", "col.section", FieldType::Text, "sec.name"),
        ],
    },
    Source {
        key: "fee_vouchers",
        label: "source.fee_vouchers",
        base: "FROM fee_vouchers v
         JOIN students s ON s.id = v.student_id
         LEFT JOIN sections sec ON sec.id = v.section_id
         LEFT JOIN classes c ON c.id = sec.class_id
         WHERE v.school_id = $1 AND v.deleted_at IS NULL AND v.academic_year_id = $2 AND ($3::uuid[] IS NULL OR v.section_id = ANY($3))",
        fields: &[
            f("voucher_no", "col.voucher_no", FieldType::Text, "v.voucher_no"),
            f("billing_month", "col.billing_month", FieldType::Date, "v.billing_month"),
            f("issue_date", "field.issue_date", FieldType::Date, "v.issue_date"),
            f("due_date", "col.due_date", FieldType::Date, "v.due_date"),
            f("student_number", "col.student_number", FieldType::Text, "s.student_number"),
            f("student_name", "col.name", FieldType::Text, "s.full_name"),
            f("father_name", "col.father_name", FieldType::Text, "s.father_name"),
            f("class", "col.class", FieldType::Text, "c.name"),
            f("section", "col.section", FieldType::Text, "sec.name"),
            f("gross", "col.gross", FieldType::Money, "v.gross"),
            f("sibling_discount", "field.sibling_discount", FieldType::Money, "v.sibling_discount"),
            f("concession_discount", "field.concession_discount", FieldType::Money, "v.concession_discount"),
            f("net_monthly", "field.net_monthly", FieldType::Money, "v.net_monthly"),
            f("arrears", "col.arrears", FieldType::Money, "v.arrears"),
            f("total_due", "col.total_due", FieldType::Money, "v.total_due"),
            f("late_fee", "col.late_fee", FieldType::Money, "v.late_fee"),
            f("late_fee_waived", "col.waived", FieldType::Boolean, "v.late_fee_waived"),
            f("net_paid", "col.paid", FieldType::Money, "v.net_paid"),
            f("balance", "col.balance", FieldType::Money, "(CASE WHEN v.status = 'cancelled' THEN 0 ELSE v.total_due + v.late_fee - v.net_paid END)"),
            choice("status", "col.status", "v.status", VOUCHER_STATUS),
        ],
    },
    Source {
        key: "fee_receipts",
        label: "source.fee_receipts",
        base: "FROM fee_receipts r
         JOIN fee_vouchers v ON v.id = r.voucher_id
         JOIN students s ON s.id = r.student_id
         LEFT JOIN sections sec ON sec.id = v.section_id
         LEFT JOIN classes c ON c.id = sec.class_id
         JOIN users u ON u.id = r.received_by
         WHERE r.school_id = $1 AND v.academic_year_id = $2 AND ($3::uuid[] IS NULL OR v.section_id = ANY($3))",
        fields: &[
            f("receipt_no", "col.receipt_no", FieldType::Text, "r.receipt_no"),
            f("paid_on", "field.paid_on", FieldType::Date, "r.paid_on"),
            f("amount", "col.amount", FieldType::Money, "r.amount"),
            choice("method", "col.method", "r.method", METHODS),
            f("reference", "col.reference", FieldType::Text, "r.reference"),
            f("is_reversal", "value.reversal", FieldType::Boolean, "(r.reverses_id IS NOT NULL)"),
            f("reversal_reason", "col.reason", FieldType::Text, "r.reversal_reason"),
            f("voucher_no", "col.voucher_no", FieldType::Text, "v.voucher_no"),
            f("billing_month", "col.billing_month", FieldType::Date, "v.billing_month"),
            f("student_number", "col.student_number", FieldType::Text, "s.student_number"),
            f("student_name", "col.name", FieldType::Text, "s.full_name"),
            f("class", "col.class", FieldType::Text, "c.name"),
            f("section", "col.section", FieldType::Text, "sec.name"),
            f("received_by", "col.received_by", FieldType::Text, "u.full_name"),
        ],
    },
    Source {
        key: "fee_balances",
        label: "source.fee_balances",
        base: "FROM students s
         LEFT JOIN enrollments e ON e.student_id = s.id AND e.academic_year_id = $2 AND e.deleted_at IS NULL
         LEFT JOIN sections sec ON sec.id = e.section_id
         LEFT JOIN classes c ON c.id = sec.class_id
         LEFT JOIN LATERAL (
         SELECT g.full_name, g.phone, sg.relationship FROM student_guardians sg JOIN guardians g ON g.id = sg.guardian_id
         WHERE sg.student_id = s.id AND sg.deleted_at IS NULL AND g.deleted_at IS NULL
         ORDER BY sg.is_primary DESC, g.full_name LIMIT 1) pg ON TRUE
         LEFT JOIN LATERAL (
         SELECT coalesce(sum(o.gross), 0) AS billed, coalesce(sum(o.sibling_discount + o.concession_discount), 0) AS discounts,
                coalesce(sum(o.late_fee), 0) AS late_fees, coalesce(sum(o.net_paid), 0) AS paid,
                coalesce(sum(o.total_due + o.late_fee - o.net_paid) FILTER (WHERE o.status IN ('unpaid', 'partial')), 0) AS balance,
                count(*) FILTER (WHERE o.status IN ('unpaid', 'partial')) AS unpaid_vouchers,
                min(o.due_date) FILTER (WHERE o.status IN ('unpaid', 'partial')) AS oldest_due
         FROM fee_vouchers o WHERE o.student_id = s.id AND o.deleted_at IS NULL AND o.status <> 'cancelled') fb ON TRUE
         WHERE s.school_id = $1 AND s.deleted_at IS NULL AND ($3::uuid[] IS NULL OR e.section_id = ANY($3))",
        fields: &[
            f("student_number", "col.student_number", FieldType::Text, "s.student_number"),
            f("full_name", "col.name", FieldType::Text, "s.full_name"),
            f("father_name", "col.father_name", FieldType::Text, "s.father_name"),
            choice("status", "col.status", "s.status", STUDENT_STATUS),
            f("class", "col.class", FieldType::Text, "c.name"),
            f("section", "col.section", FieldType::Text, "sec.name"),
            f("guardian_phone", "col.phone", FieldType::Text, "pg.phone"),
            f("billed", "col.billed", FieldType::Money, "fb.billed"),
            f("discounts", "col.discounts", FieldType::Money, "fb.discounts"),
            f("late_fees", "col.late_fee", FieldType::Money, "fb.late_fees"),
            f("paid", "col.paid", FieldType::Money, "fb.paid"),
            f("balance", "col.balance", FieldType::Money, "fb.balance"),
            f("unpaid_vouchers", "col.unpaid_vouchers", FieldType::Number, "fb.unpaid_vouchers"),
            f("oldest_due", "col.oldest_due", FieldType::Date, "fb.oldest_due"),
        ],
    },
    Source {
        key: "attendance_entries",
        label: "source.attendance_entries",
        base: "FROM attendance_entries a
         JOIN attendance_sheets sh ON sh.id = a.sheet_id AND sh.deleted_at IS NULL
         JOIN sections sec ON sec.id = sh.section_id
         JOIN classes c ON c.id = sec.class_id
         JOIN students s ON s.id = a.student_id
         LEFT JOIN enrollments e ON e.student_id = s.id AND e.section_id = sec.id AND e.deleted_at IS NULL
         WHERE a.school_id = $1 AND a.deleted_at IS NULL AND sh.academic_year_id = $2 AND ($3::uuid[] IS NULL OR sh.section_id = ANY($3))",
        fields: &[
            f("date", "col.date", FieldType::Date, "a.date"),
            choice("status", "col.status", "a.status", ATT_STATUS),
            f("note", "col.note", FieldType::Text, "a.note"),
            f("student_number", "col.student_number", FieldType::Text, "s.student_number"),
            f("student_name", "col.name", FieldType::Text, "s.full_name"),
            f("father_name", "col.father_name", FieldType::Text, "s.father_name"),
            f("class", "col.class", FieldType::Text, "c.name"),
            f("section", "col.section", FieldType::Text, "sec.name"),
            f("roll_no", "col.roll_no", FieldType::Number, "e.roll_no"),
        ],
    },
    Source {
        key: "attendance_summary",
        label: "source.attendance_summary",
        base: "FROM enrollments e
         JOIN students s ON s.id = e.student_id AND s.deleted_at IS NULL
         JOIN sections sec ON sec.id = e.section_id
         JOIN classes c ON c.id = sec.class_id
         LEFT JOIN LATERAL (
         SELECT count(*) FILTER (WHERE a.status = 'present') AS present, count(*) FILTER (WHERE a.status = 'absent') AS absent,
                count(*) FILTER (WHERE a.status = 'late') AS late, count(*) FILTER (WHERE a.status = 'leave') AS leave, count(*) AS marked
         FROM attendance_entries a JOIN attendance_sheets sh ON sh.id = a.sheet_id AND sh.deleted_at IS NULL
         WHERE a.student_id = s.id AND sh.section_id = sec.id AND a.deleted_at IS NULL) t ON TRUE
         WHERE e.school_id = $1 AND e.academic_year_id = $2 AND e.deleted_at IS NULL AND ($3::uuid[] IS NULL OR e.section_id = ANY($3))",
        fields: &[
            f("student_number", "col.student_number", FieldType::Text, "s.student_number"),
            f("full_name", "col.name", FieldType::Text, "s.full_name"),
            f("father_name", "col.father_name", FieldType::Text, "s.father_name"),
            choice("status", "col.status", "s.status", STUDENT_STATUS),
            f("class", "col.class", FieldType::Text, "c.name"),
            f("section", "col.section", FieldType::Text, "sec.name"),
            f("roll_no", "col.roll_no", FieldType::Number, "e.roll_no"),
            f("present", "col.present", FieldType::Number, "t.present"),
            f("absent", "col.absent", FieldType::Number, "t.absent"),
            f("late", "col.late", FieldType::Number, "t.late"),
            f("leave", "col.leave", FieldType::Number, "t.leave"),
            f("marked", "col.days_marked", FieldType::Number, "t.marked"),
            f("attendance_percent", "col.attendance_percent", FieldType::Number, "(CASE WHEN t.marked > 0 THEN round((t.present + t.late) * 100.0 / t.marked, 1) END)"),
        ],
    },
    Source {
        key: "marks",
        label: "source.marks",
        base: "FROM marks m
         JOIN mark_sheets ms ON ms.id = m.sheet_id AND ms.deleted_at IS NULL
         JOIN assessment_types at ON at.id = ms.assessment_type_id
         JOIN subjects sub ON sub.id = ms.subject_id
         JOIN sections sec ON sec.id = ms.section_id
         JOIN classes c ON c.id = sec.class_id
         JOIN students s ON s.id = m.student_id
         LEFT JOIN enrollments e ON e.student_id = s.id AND e.section_id = sec.id AND e.deleted_at IS NULL
         LEFT JOIN assessment_max_marks mm ON mm.assessment_type_id = at.id AND mm.class_id = c.id AND mm.subject_id = sub.id AND mm.deleted_at IS NULL
         WHERE m.school_id = $1 AND m.deleted_at IS NULL AND at.academic_year_id = $2 AND ($3::uuid[] IS NULL OR ms.section_id = ANY($3))",
        fields: &[
            f("assessment", "param.assessment", FieldType::Text, "at.name"),
            f("subject", "col.subject", FieldType::Text, "sub.name"),
            f("class", "col.class", FieldType::Text, "c.name"),
            f("section", "col.section", FieldType::Text, "sec.name"),
            f("student_number", "col.student_number", FieldType::Text, "s.student_number"),
            f("student_name", "col.name", FieldType::Text, "s.full_name"),
            f("roll_no", "col.roll_no", FieldType::Number, "e.roll_no"),
            f("obtained", "col.marks", FieldType::Number, "m.obtained"),
            f("max_marks", "col.max_marks", FieldType::Number, "mm.max_marks"),
            f("percent", "col.percent", FieldType::Number, "(CASE WHEN mm.max_marks > 0 THEN round(coalesce(m.obtained, 0) * 100.0 / mm.max_marks, 2) END)"),
            f("is_absent", "value.absent", FieldType::Boolean, "m.is_absent"),
            choice("sheet_status", "field.sheet_status", "ms.status", SHEET_STATUS),
        ],
    },
    Source {
        key: "staff",
        label: "source.staff",
        base: "FROM staff st WHERE st.school_id = $1 AND st.deleted_at IS NULL AND $2::uuid IS NOT NULL AND $3::uuid[] IS NULL",
        fields: &[
            f("staff_number", "col.staff_number", FieldType::Text, "st.staff_number"),
            f("full_name", "col.name", FieldType::Text, "st.full_name"),
            f("father_name", "col.father_name", FieldType::Text, "st.father_name"),
            choice("gender", "col.gender", "st.gender", GENDERS),
            f("phone", "col.phone", FieldType::Text, "st.phone"),
            f("email", "col.email", FieldType::Text, "st.email"),
            f("cnic", "col.cnic", FieldType::Text, "st.cnic"),
            f("designation", "col.designation", FieldType::Text, "st.designation"),
            f("joined_on", "field.joined_on", FieldType::Date, "st.joined_on"),
            choice("status", "col.status", "st.status", STAFF_STATUS),
        ],
    },
];

pub fn source(key: &str) -> Option<&'static Source> {
    SOURCES.iter().find(|s| s.key == key)
}

impl Source {
    pub fn field(&self, key: &str) -> Option<&'static Field> {
        self.fields.iter().find(|f| f.key == key)
    }
}

/// Filter operators. Which ones apply depends on the field type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    Eq,
    Ne,
    Contains,
    StartsWith,
    In,
    IsEmpty,
    NotEmpty,
    Gt,
    Gte,
    Lt,
    Lte,
    Between,
    Before,
    After,
    IsTrue,
    IsFalse,
}

impl Op {
    pub const ALL: &'static [Op] = &[
        Op::Eq,
        Op::Ne,
        Op::Contains,
        Op::StartsWith,
        Op::In,
        Op::IsEmpty,
        Op::NotEmpty,
        Op::Gt,
        Op::Gte,
        Op::Lt,
        Op::Lte,
        Op::Between,
        Op::Before,
        Op::After,
        Op::IsTrue,
        Op::IsFalse,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Op::Eq => "eq",
            Op::Ne => "ne",
            Op::Contains => "contains",
            Op::StartsWith => "starts_with",
            Op::In => "in",
            Op::IsEmpty => "is_empty",
            Op::NotEmpty => "not_empty",
            Op::Gt => "gt",
            Op::Gte => "gte",
            Op::Lt => "lt",
            Op::Lte => "lte",
            Op::Between => "between",
            Op::Before => "before",
            Op::After => "after",
            Op::IsTrue => "is_true",
            Op::IsFalse => "is_false",
        }
    }

    /// Operators a field type accepts.
    pub fn for_type(ty: FieldType) -> &'static [Op] {
        match ty {
            FieldType::Text => &[
                Op::Eq,
                Op::Ne,
                Op::Contains,
                Op::StartsWith,
                Op::In,
                Op::IsEmpty,
                Op::NotEmpty,
            ],
            FieldType::Number | FieldType::Money => &[
                Op::Eq,
                Op::Ne,
                Op::Gt,
                Op::Gte,
                Op::Lt,
                Op::Lte,
                Op::Between,
                Op::IsEmpty,
                Op::NotEmpty,
            ],
            FieldType::Date => &[
                Op::Eq,
                Op::Before,
                Op::After,
                Op::Between,
                Op::IsEmpty,
                Op::NotEmpty,
            ],
            FieldType::Boolean => &[Op::IsTrue, Op::IsFalse],
            FieldType::Choice => &[Op::Eq, Op::Ne, Op::In, Op::IsEmpty, Op::NotEmpty],
        }
    }

    /// How many values the operator takes.
    fn arity(self) -> usize {
        match self {
            Op::IsEmpty | Op::NotEmpty | Op::IsTrue | Op::IsFalse => 0,
            Op::Between => 2,
            _ => 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Filter {
    pub field: String,
    pub op: Op,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub value2: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Sort {
    pub field: String,
    #[serde(default)]
    pub desc: bool,
}

/// A custom report, as stored in `report_definitions.definition` and as sent by the builder.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Definition {
    pub source: String,
    pub columns: Vec<String>,
    #[serde(default)]
    pub filters: Vec<Filter>,
    #[serde(default)]
    pub sort: Vec<Sort>,
    #[serde(default)]
    pub group_by: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
}

pub const DEFAULT_LIMIT: u32 = 500;
pub const MAX_LIMIT: u32 = 5000;

/// A bound parameter; the only way a user value reaches the database.
#[derive(Debug, Clone)]
enum Bind {
    Text(String),
    Texts(Vec<String>),
    Number(Decimal),
    Date(NaiveDate),
}

/// The assembled query: SQL with `$n` placeholders and the binds in order after the three
/// fixed ones.
#[derive(Debug, Clone)]
pub struct Query {
    pub sql: String,
    binds: Vec<Bind>,
    pub source: &'static Source,
    pub columns: Vec<&'static Field>,
    pub group_by: Option<&'static Field>,
    pub limit: u32,
}

fn parse_value(field: &Field, raw: &str) -> Result<Bind, ApiError> {
    let raw = raw.trim();
    match field.ty {
        FieldType::Text => Ok(Bind::Text(raw.to_string())),
        FieldType::Choice => {
            if !field.choices.contains(&raw) {
                return Err(ApiError::validation(format!(
                    "{} must be one of: {}",
                    field.key,
                    field.choices.join(", ")
                )));
            }
            Ok(Bind::Text(raw.to_string()))
        }
        FieldType::Number | FieldType::Money => raw
            .parse::<Decimal>()
            .map(Bind::Number)
            .map_err(|_| ApiError::validation(format!("{} needs a number.", field.key))),
        FieldType::Date => NaiveDate::parse_from_str(raw, "%Y-%m-%d")
            .map(Bind::Date)
            .map_err(|_| ApiError::validation(format!("{} needs a date (YYYY-MM-DD).", field.key))),
        FieldType::Boolean => Err(ApiError::validation(format!(
            "{} takes no value.",
            field.key
        ))),
    }
}

/// Validate a definition against the catalogue and assemble the query. Every name must be in
/// the catalogue; every value is parsed by the field's type and bound.
pub fn assemble(def: &Definition) -> Result<Query, ApiError> {
    let source = source(&def.source)
        .ok_or_else(|| ApiError::validation(format!("Unknown data source: {}", def.source)))?;
    if def.columns.is_empty() {
        return Err(ApiError::validation("Pick at least one column."));
    }
    let mut columns = Vec::new();
    for key in &def.columns {
        let field = source
            .field(key)
            .ok_or_else(|| ApiError::validation(format!("Unknown field: {key}")))?;
        if columns.iter().any(|c: &&Field| c.key == field.key) {
            return Err(ApiError::validation(format!(
                "Column {key} is listed twice."
            )));
        }
        columns.push(field);
    }
    let group_by = match &def.group_by {
        Some(key) => Some(
            source
                .field(key)
                .ok_or_else(|| ApiError::validation(format!("Unknown field: {key}")))?,
        ),
        None => None,
    };
    let limit = def.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);

    let mut binds: Vec<Bind> = Vec::new();
    let mut wheres: Vec<String> = Vec::new();
    for flt in &def.filters {
        let field = source
            .field(&flt.field)
            .ok_or_else(|| ApiError::validation(format!("Unknown field: {}", flt.field)))?;
        if !Op::for_type(field.ty).contains(&flt.op) {
            return Err(ApiError::validation(format!(
                "{} cannot be filtered with {}.",
                field.key,
                flt.op.key()
            )));
        }
        let expr = field.sql;
        let next = |binds: &Vec<Bind>| 4 + binds.len();
        let clause = match flt.op {
            Op::IsEmpty => format!("({expr} IS NULL OR {expr}::text = '')"),
            Op::NotEmpty => format!("({expr} IS NOT NULL AND {expr}::text <> '')"),
            Op::IsTrue => format!("({expr}) IS TRUE"),
            Op::IsFalse => format!("({expr}) IS NOT TRUE"),
            Op::In => {
                let values: Vec<String> = flt
                    .value
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect();
                if values.is_empty() {
                    return Err(ApiError::validation(format!(
                        "{} needs at least one value.",
                        field.key
                    )));
                }
                for v in &values {
                    parse_value(field, v)?;
                }
                let n = next(&binds);
                binds.push(Bind::Texts(values));
                format!("{expr}::text = ANY(${n})")
            }
            Op::Between => {
                let lo = parse_value(field, &flt.value)?;
                let hi = parse_value(field, flt.value2.as_deref().unwrap_or(""))?;
                let n = next(&binds);
                binds.push(lo);
                binds.push(hi);
                format!("{expr} BETWEEN ${n} AND ${}", n + 1)
            }
            op => {
                if op.arity() != 1 {
                    unreachable!("arity handled above");
                }
                let v = parse_value(field, &flt.value)?;
                let n = next(&binds);
                let clause = match (op, field.ty) {
                    (Op::Contains, _) => format!("{expr} ILIKE '%' || ${n} || '%'"),
                    (Op::StartsWith, _) => format!("{expr} ILIKE ${n} || '%'"),
                    (Op::Eq, FieldType::Text) => format!("lower({expr}) = lower(${n})"),
                    (Op::Ne, FieldType::Text) => {
                        format!("lower({expr}) IS DISTINCT FROM lower(${n})")
                    }
                    (Op::Eq, _) => format!("{expr} = ${n}"),
                    (Op::Ne, _) => format!("{expr} IS DISTINCT FROM ${n}"),
                    (Op::Gt | Op::After, _) => format!("{expr} > ${n}"),
                    (Op::Gte, _) => format!("{expr} >= ${n}"),
                    (Op::Lt | Op::Before, _) => format!("{expr} < ${n}"),
                    (Op::Lte, _) => format!("{expr} <= ${n}"),
                    _ => unreachable!("operator with one value"),
                };
                binds.push(v);
                clause
            }
        };
        wheres.push(clause);
    }

    let mut order: Vec<String> = Vec::new();
    if let Some(g) = group_by {
        order.push(format!("{} NULLS LAST", g.sql));
    }
    for s in &def.sort {
        let field = source
            .field(&s.field)
            .ok_or_else(|| ApiError::validation(format!("Unknown field: {}", s.field)))?;
        order.push(format!(
            "{} {} NULLS LAST",
            field.sql,
            if s.desc { "DESC" } else { "ASC" }
        ));
    }
    if order.is_empty() {
        order.push(format!("{} ASC NULLS LAST", columns[0].sql));
    }
    let select: Vec<String> = columns
        .iter()
        .enumerate()
        .map(|(i, c)| format!("({})::text AS c{i}", c.sql))
        .collect();
    let group_select = group_by
        .map(|g| format!(", ({})::text AS grp", g.sql))
        .unwrap_or_default();
    let where_extra = if wheres.is_empty() {
        String::new()
    } else {
        format!(" AND {}", wheres.join(" AND "))
    };
    let sql = format!(
        "SELECT {}{} {}{} ORDER BY {} LIMIT {limit}",
        select.join(", "),
        group_select,
        source.base,
        where_extra,
        order.join(", ")
    );
    Ok(Query {
        sql,
        binds,
        source,
        columns,
        group_by,
        limit,
    })
}

/// Format a raw text value from the database for the report.
fn format_cell(lang: Lang, field: &Field, raw: Option<String>) -> String {
    let Some(raw) = raw else {
        return String::new();
    };
    match field.ty {
        FieldType::Text => raw,
        FieldType::Choice => value(lang, &raw),
        FieldType::Money => raw.parse::<Decimal>().map(money).unwrap_or(raw),
        FieldType::Number => raw
            .parse::<Decimal>()
            .map(|d| d.round_dp(2).normalize().to_string())
            .unwrap_or(raw),
        FieldType::Date => NaiveDate::parse_from_str(&raw, "%Y-%m-%d")
            .map(format_date)
            .unwrap_or(raw),
        FieldType::Boolean => tr(
            lang,
            if raw == "true" || raw == "t" {
                "common.yes"
            } else {
                "common.no"
            },
        )
        .into(),
    }
}

/// Run a definition and build the report document.
pub async fn run(
    conn: &mut PgConnection,
    ctx: &Context<'_>,
    def: &Definition,
    title: &str,
    description: &str,
) -> Result<Report, ApiError> {
    let lang = ctx.lang;
    let query = assemble(def)?;
    let mut q = sqlx::query(sqlx::AssertSqlSafe(query.sql.clone()))
        .bind(ctx.school.id)
        .bind(ctx.year.id)
        .bind(ctx.section_scope.as_deref());
    for b in &query.binds {
        q = match b {
            Bind::Text(s) => q.bind(s.clone()),
            Bind::Texts(v) => q.bind(v.clone()),
            Bind::Number(d) => q.bind(*d),
            Bind::Date(d) => q.bind(*d),
        };
    }
    let rows = q.fetch_all(conn).await?;
    let mut r = Report::new("custom", lang, &ctx.school.name, title);
    r.subtitle = (!description.is_empty()).then(|| description.to_string());
    r.landscape = query.columns.len() > 6;
    r.param("param.academic_year", ctx.year.name.clone());
    r.param("source.label", tr(lang, query.source.label));
    r.columns = query
        .columns
        .iter()
        .map(|c| {
            let label = match c.ty {
                FieldType::Money => format!("{} ({})", tr(lang, c.label), ctx.school.currency),
                _ => tr(lang, c.label).to_string(),
            };
            match c.ty {
                FieldType::Number | FieldType::Money => Column::number(label),
                FieldType::Text
                    if matches!(
                        c.key,
                        "full_name" | "student_name" | "guardian_name" | "address" | "father_name"
                    ) =>
                {
                    Column::wide(label)
                }
                _ => Column::text(label),
            }
        })
        .collect();
    let numeric: Vec<bool> = query
        .columns
        .iter()
        .map(|c| matches!(c.ty, FieldType::Number | FieldType::Money))
        .collect();
    let money_col: Vec<bool> = query
        .columns
        .iter()
        .map(|c| c.ty == FieldType::Money)
        .collect();
    let mut groups: Vec<(Option<String>, Group, Vec<Decimal>)> = Vec::new();
    let mut totals = vec![Decimal::ZERO; query.columns.len()];
    for row in rows {
        let key: Option<String> = if query.group_by.is_some() {
            row.try_get::<Option<String>, _>("grp")
                .unwrap_or(None)
                .or(Some(String::new()))
        } else {
            None
        };
        if groups.is_empty() || groups.last().map(|g| &g.0) != Some(&key) {
            let heading = key.as_ref().map(|k| {
                let g = query.group_by.expect("grouped");
                let shown = format_cell(lang, g, Some(k.clone()));
                format!(
                    "{}: {}",
                    tr(lang, g.label),
                    if shown.is_empty() {
                        tr(lang, "common.none").to_string()
                    } else {
                        shown
                    }
                )
            });
            groups.push((
                key.clone(),
                Group {
                    heading,
                    ..Default::default()
                },
                vec![Decimal::ZERO; query.columns.len()],
            ));
        }
        let g = groups.last_mut().expect("group");
        let mut cells = Vec::with_capacity(query.columns.len());
        for (i, field) in query.columns.iter().enumerate() {
            let raw: Option<String> = row.try_get(i).unwrap_or(None);
            if numeric[i] {
                if let Some(d) = raw.as_deref().and_then(|s| s.parse::<Decimal>().ok()) {
                    g.2[i] += d;
                    totals[i] += d;
                }
            }
            cells.push(format_cell(lang, field, raw));
        }
        g.1.rows.push(cells);
    }
    let sum_cells = |sums: &[Decimal], count: usize, label: &str| -> Vec<String> {
        let mut cells: Vec<String> = (0..query.columns.len())
            .map(|i| {
                if numeric[i] {
                    if money_col[i] {
                        money(sums[i])
                    } else {
                        sums[i].round_dp(2).normalize().to_string()
                    }
                } else {
                    String::new()
                }
            })
            .collect();
        cells[0] = format!("{label} ({count})");
        cells
    };
    let grouped = query.group_by.is_some();
    let mut row_count = 0;
    for (_, mut g, sums) in groups {
        if grouped {
            g.subtotal = Some(sum_cells(&sums, g.rows.len(), tr(lang, "subtotal")));
        }
        row_count += g.rows.len();
        r.push_group(g);
    }
    r.total = Some(sum_cells(&totals, row_count, tr(lang, "common.total")));
    if row_count as u32 >= query.limit {
        r.notes
            .push(format!("{} {}", tr(lang, "note.limit"), query.limit));
    }
    Ok(r)
}

/// Catalogue for the builder UI: sources, their fields and the operators per type.
#[derive(Debug, Serialize)]
pub struct CatalogueSource {
    pub key: String,
    pub label: String,
    pub fields: Vec<CatalogueField>,
}

#[derive(Debug, Serialize)]
pub struct CatalogueField {
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub ty: FieldType,
    pub choices: Vec<CatalogueChoice>,
    pub operators: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CatalogueChoice {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Serialize)]
pub struct Catalogue {
    pub sources: Vec<CatalogueSource>,
    pub operators: Vec<CatalogueChoice>,
    pub default_limit: u32,
    pub max_limit: u32,
}

pub fn catalogue(lang: Lang) -> Catalogue {
    Catalogue {
        sources: SOURCES
            .iter()
            .map(|s| CatalogueSource {
                key: s.key.into(),
                label: tr(lang, s.label).into(),
                fields: s
                    .fields
                    .iter()
                    .map(|f| CatalogueField {
                        key: f.key.into(),
                        label: tr(lang, f.label).into(),
                        ty: f.ty,
                        choices: f
                            .choices
                            .iter()
                            .map(|c| CatalogueChoice {
                                value: (*c).into(),
                                label: value(lang, c),
                            })
                            .collect(),
                        operators: Op::for_type(f.ty).iter().map(|o| o.key().into()).collect(),
                    })
                    .collect(),
            })
            .collect(),
        operators: Op::ALL
            .iter()
            .map(|o| CatalogueChoice {
                value: o.key().into(),
                label: tr(lang, &format!("op.{}", o.key())).into(),
            })
            .collect(),
        default_limit: DEFAULT_LIMIT,
        max_limit: MAX_LIMIT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(json: serde_json::Value) -> Definition {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn catalogue_texts_exist_and_fields_are_unique() {
        for s in SOURCES {
            assert!(super::super::text::has(s.label), "{}", s.label);
            let mut keys = std::collections::HashSet::new();
            for f in s.fields {
                assert!(keys.insert(f.key), "{} has {} twice", s.key, f.key);
                assert!(
                    super::super::text::has(f.label),
                    "{} label {}",
                    f.key,
                    f.label
                );
            }
        }
        for o in Op::ALL {
            assert!(
                super::super::text::has(&format!("op.{}", o.key())),
                "op {}",
                o.key()
            );
        }
        let cat = catalogue(Lang::Ur);
        assert_eq!(cat.sources.len(), SOURCES.len());
        assert!(cat.sources[0].fields[0]
            .operators
            .contains(&"contains".to_string()));
    }

    #[test]
    fn assembles_parameterised_sql_from_the_catalogue_only() {
        let q = assemble(&def(serde_json::json!({
            "source": "students",
            "columns": ["student_number", "full_name", "roll_no"],
            "filters": [
                { "field": "full_name", "op": "contains", "value": "Khan'; DROP TABLE students; --" },
                { "field": "roll_no", "op": "between", "value": "1", "value2": "10" },
                { "field": "gender", "op": "in", "value": "male, female" },
                { "field": "has_photo", "op": "is_false" },
                { "field": "date_of_birth", "op": "after", "value": "2015-01-01" }
            ],
            "sort": [{ "field": "full_name", "desc": true }],
            "group_by": "class",
            "limit": 9999
        })))
        .unwrap();
        assert!(
            !q.sql.contains("DROP"),
            "user text never reaches SQL: {}",
            q.sql
        );
        assert!(
            q.sql.contains("s.full_name ILIKE '%' || $4 || '%'"),
            "{}",
            q.sql
        );
        assert!(q.sql.contains("e.roll_no BETWEEN $5 AND $6"), "{}", q.sql);
        assert!(q.sql.contains("s.gender::text = ANY($7)"), "{}", q.sql);
        assert!(
            q.sql.contains("((s.photo IS NOT NULL)) IS NOT TRUE"),
            "{}",
            q.sql
        );
        assert!(q.sql.contains("s.date_of_birth > $8"), "{}", q.sql);
        assert!(
            q.sql
                .contains("ORDER BY c.name NULLS LAST, s.full_name DESC NULLS LAST LIMIT 5000"),
            "{}",
            q.sql
        );
        assert!(q.sql.contains("(c.name)::text AS grp"), "{}", q.sql);
        assert_eq!(q.binds.len(), 5);
        assert_eq!(q.columns.len(), 3);
    }

    #[test]
    fn refuses_anything_outside_the_catalogue() {
        let err = |v: serde_json::Value| assemble(&def(v)).unwrap_err().to_string();
        assert!(
            err(serde_json::json!({ "source": "pg_tables", "columns": ["x"] }))
                .contains("Unknown data source")
        );
        assert!(
            err(serde_json::json!({ "source": "students", "columns": ["password"] }))
                .contains("Unknown field")
        );
        assert!(
            err(serde_json::json!({ "source": "students", "columns": [] }))
                .contains("at least one column")
        );
        assert!(err(serde_json::json!({ "source": "students", "columns": ["full_name"], "filters": [{ "field": "full_name", "op": "gt", "value": "1" }] })).contains("cannot be filtered"));
        assert!(err(serde_json::json!({ "source": "students", "columns": ["full_name"], "filters": [{ "field": "roll_no", "op": "eq", "value": "abc" }] })).contains("needs a number"));
        assert!(err(serde_json::json!({ "source": "students", "columns": ["full_name"], "filters": [{ "field": "gender", "op": "eq", "value": "unknown" }] })).contains("must be one of"));
        assert!(err(serde_json::json!({ "source": "students", "columns": ["full_name"], "filters": [{ "field": "admission_date", "op": "before", "value": "yesterday" }] })).contains("needs a date"));
        assert!(err(serde_json::json!({ "source": "students", "columns": ["full_name"], "sort": [{ "field": "secret" }] })).contains("Unknown field"));
        assert!(err(
            serde_json::json!({ "source": "students", "columns": ["full_name", "full_name"] })
        )
        .contains("twice"));
        assert!(serde_json::from_value::<Definition>(serde_json::json!({ "source": "students", "columns": ["x"], "filters": [{ "field": "x", "op": "exec" }] })).is_err(), "unknown operator does not parse");
    }
}
