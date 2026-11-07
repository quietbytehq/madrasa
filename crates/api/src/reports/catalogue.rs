//! The built-in reports: code, module, parameters. The UI builds its parameter form from
//! this; the route dispatches on the code.

use serde::Serialize;

use super::text::tr;
use super::Lang;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Module {
    Students,
    Fees,
    Attendance,
    Exams,
    /// Standard edition only (hidden on Lite).
    Timetable,
    /// Standard edition only (hidden on Lite).
    Leave,
}

impl Module {
    fn key(self) -> &'static str {
        match self {
            Module::Students => "module.students",
            Module::Fees => "module.fees",
            Module::Attendance => "module.attendance",
            Module::Exams => "module.exams",
            Module::Timetable => "module.timetable",
            Module::Leave => "module.leave",
        }
    }
}

/// What kind of value a parameter takes; the UI picks the control from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamKind {
    /// An academic year id; defaults to the active year.
    AcademicYear,
    /// A class id of the chosen year; empty = all classes.
    Class,
    /// A section id of the chosen class; empty = all sections.
    Section,
    /// `active`, `left` or `graduated`.
    Status,
    /// `YYYY-MM`.
    Month,
    /// `YYYY-MM-DD`.
    DateFrom,
    DateTo,
    /// A single `YYYY-MM-DD`; defaults to today.
    Date,
    /// An assessment id of the chosen year.
    Assessment,
    /// A small whole number (threshold, top N, minimum days).
    Number,
    /// A student number as printed on vouchers.
    StudentNumber,
    /// A consent type id; empty = every type.
    ConsentType,
    /// A board exam id of the chosen year.
    BoardExam,
    /// `pending`, `approved`, `rejected` or `withdrawn`; empty = every status.
    LeaveStatus,
}

/// Who may run a report. Office roles hold `reports.read`; teachers may run attendance and
/// exam reports for their own sections with the module's read permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    ReportsRead,
    AttendanceRead,
    ExamsRead,
    /// Timetable reports: everyone with `timetable.read`.
    TimetableRead,
    /// Leave reports: everyone with `leave.read` (teachers: own sections / own leave).
    LeaveRead,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ParamDef {
    pub name: &'static str,
    pub kind: ParamKind,
    pub required: bool,
}

impl ParamDef {
    fn label_key(&self) -> &'static str {
        match self.name {
            "threshold" => "param.threshold",
            "top" => "param.top_n",
            "min_days" => "param.min_days",
            "as_of" => "param.as_of",
            "day" => "param.day",
            "status" if self.kind == ParamKind::LeaveStatus => "param.status",
            _ => match self.kind {
                ParamKind::AcademicYear => "param.academic_year",
                ParamKind::Class => "param.class",
                ParamKind::Section => "param.section",
                ParamKind::Status => "param.status",
                ParamKind::Month => "param.month",
                ParamKind::DateFrom => "param.from",
                ParamKind::DateTo => "param.to",
                ParamKind::Date => "param.date",
                ParamKind::Assessment => "param.assessment",
                ParamKind::Number => "param.top_n",
                ParamKind::StudentNumber => "param.student_number",
                ParamKind::BoardExam => "param.board_exam",
                ParamKind::ConsentType => "param.consent_type",
                ParamKind::LeaveStatus => "param.status",
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ReportDef {
    pub code: &'static str,
    pub module: Module,
    pub params: &'static [ParamDef],
    pub landscape: bool,
    pub access: Access,
}

const YEAR: ParamDef = ParamDef {
    name: "academic_year_id",
    kind: ParamKind::AcademicYear,
    required: false,
};
const CLASS: ParamDef = ParamDef {
    name: "class_id",
    kind: ParamKind::Class,
    required: false,
};
const SECTION: ParamDef = ParamDef {
    name: "section_id",
    kind: ParamKind::Section,
    required: false,
};
const STATUS: ParamDef = ParamDef {
    name: "status",
    kind: ParamKind::Status,
    required: false,
};
const MONTH: ParamDef = ParamDef {
    name: "month",
    kind: ParamKind::Month,
    required: true,
};
const FROM: ParamDef = ParamDef {
    name: "from",
    kind: ParamKind::DateFrom,
    required: true,
};
const TO: ParamDef = ParamDef {
    name: "to",
    kind: ParamKind::DateTo,
    required: true,
};
const SECTION_REQ: ParamDef = ParamDef {
    name: "section_id",
    kind: ParamKind::Section,
    required: true,
};
const CLASS_REQ: ParamDef = ParamDef {
    name: "class_id",
    kind: ParamKind::Class,
    required: true,
};
const ASSESSMENT: ParamDef = ParamDef {
    name: "assessment_id",
    kind: ParamKind::Assessment,
    required: true,
};
const AS_OF: ParamDef = ParamDef {
    name: "as_of",
    kind: ParamKind::Date,
    required: false,
};
const MIN_DAYS: ParamDef = ParamDef {
    name: "min_days",
    kind: ParamKind::Number,
    required: false,
};
const THRESHOLD: ParamDef = ParamDef {
    name: "threshold",
    kind: ParamKind::Number,
    required: true,
};
const TOP: ParamDef = ParamDef {
    name: "top",
    kind: ParamKind::Number,
    required: false,
};
const STUDENT: ParamDef = ParamDef {
    name: "student_number",
    kind: ParamKind::StudentNumber,
    required: true,
};
const CONSENT_TYPE: ParamDef = ParamDef {
    name: "consent_type_id",
    kind: ParamKind::ConsentType,
    required: false,
};
const LEAVE_STATUS: ParamDef = ParamDef {
    name: "status",
    kind: ParamKind::LeaveStatus,
    required: false,
};
const FROM_OPT: ParamDef = ParamDef {
    name: "from",
    kind: ParamKind::DateFrom,
    required: false,
};
const TO_OPT: ParamDef = ParamDef {
    name: "to",
    kind: ParamKind::DateTo,
    required: false,
};
const DAY: ParamDef = ParamDef {
    name: "day",
    kind: ParamKind::Number,
    required: true,
};
const BOARD_EXAM: ParamDef = ParamDef {
    name: "board_exam_id",
    kind: ParamKind::BoardExam,
    required: true,
};

pub const REPORTS: &[ReportDef] = &[
    ReportDef {
        code: "students.list",
        module: Module::Students,
        params: &[YEAR, CLASS, SECTION, STATUS],
        landscape: true,
        access: Access::ReportsRead,
    },
    ReportDef {
        code: "students.strength",
        module: Module::Students,
        params: &[YEAR, CLASS],
        landscape: false,
        access: Access::ReportsRead,
    },
    ReportDef {
        code: "students.movements",
        module: Module::Students,
        params: &[YEAR, FROM, TO, CLASS],
        landscape: false,
        access: Access::ReportsRead,
    },
    ReportDef {
        code: "students.contacts",
        module: Module::Students,
        params: &[YEAR, CLASS, SECTION],
        landscape: true,
        access: Access::ReportsRead,
    },
    ReportDef {
        code: "students.birthdays",
        module: Module::Students,
        params: &[YEAR, MONTH, CLASS],
        landscape: false,
        access: Access::ReportsRead,
    },
    ReportDef {
        code: "students.missing",
        module: Module::Students,
        params: &[YEAR, CLASS, SECTION],
        landscape: true,
        access: Access::ReportsRead,
    },
    ReportDef {
        code: "students.consents_missing",
        module: Module::Students,
        params: &[YEAR, CLASS, SECTION, CONSENT_TYPE],
        landscape: false,
        access: Access::ReportsRead,
    },
    ReportDef {
        code: "students.consents_expiring",
        module: Module::Students,
        params: &[YEAR, CLASS, SECTION],
        landscape: false,
        access: Access::ReportsRead,
    },
    ReportDef {
        code: "students.consent_forms",
        module: Module::Students,
        params: &[YEAR, CLASS, SECTION],
        landscape: true,
        access: Access::ReportsRead,
    },
    // ---- Fees
    ReportDef {
        code: "fees.collection",
        module: Module::Fees,
        params: &[FROM, TO, CLASS],
        landscape: true,
        access: Access::ReportsRead,
    },
    ReportDef {
        code: "fees.monthly",
        module: Module::Fees,
        params: &[FROM, TO],
        landscape: false,
        access: Access::ReportsRead,
    },
    ReportDef {
        code: "fees.defaulters",
        module: Module::Fees,
        params: &[AS_OF, CLASS, MIN_DAYS],
        landscape: true,
        access: Access::ReportsRead,
    },
    ReportDef {
        code: "fees.ledger",
        module: Module::Fees,
        params: &[STUDENT],
        landscape: true,
        access: Access::ReportsRead,
    },
    ReportDef {
        code: "fees.concessions",
        module: Module::Fees,
        params: &[YEAR, FROM, TO, CLASS],
        landscape: true,
        access: Access::ReportsRead,
    },
    ReportDef {
        code: "fees.vouchers",
        module: Module::Fees,
        params: &[MONTH, CLASS, SECTION],
        landscape: true,
        access: Access::ReportsRead,
    },
    ReportDef {
        code: "fees.late_fees",
        module: Module::Fees,
        params: &[FROM, TO, CLASS],
        landscape: true,
        access: Access::ReportsRead,
    },
    ReportDef {
        code: "fees.heads",
        module: Module::Fees,
        params: &[YEAR],
        landscape: false,
        access: Access::ReportsRead,
    },
    // ---- Attendance
    ReportDef {
        code: "attendance.register",
        module: Module::Attendance,
        params: &[YEAR, CLASS_REQ, SECTION_REQ, MONTH],
        landscape: true,
        access: Access::AttendanceRead,
    },
    ReportDef {
        code: "attendance.percent",
        module: Module::Attendance,
        params: &[YEAR, FROM, TO, CLASS],
        landscape: false,
        access: Access::AttendanceRead,
    },
    ReportDef {
        code: "attendance.absentees",
        module: Module::Attendance,
        params: &[FROM, TO, CLASS, SECTION],
        landscape: true,
        access: Access::AttendanceRead,
    },
    ReportDef {
        code: "attendance.below",
        module: Module::Attendance,
        params: &[YEAR, FROM, TO, THRESHOLD, CLASS, SECTION],
        landscape: true,
        access: Access::AttendanceRead,
    },
    ReportDef {
        code: "attendance.staff",
        module: Module::Attendance,
        params: &[MONTH],
        landscape: false,
        access: Access::ReportsRead,
    },
    // ---- Exams
    ReportDef {
        code: "exams.result_sheet",
        module: Module::Exams,
        params: &[YEAR, ASSESSMENT, CLASS_REQ, SECTION_REQ],
        landscape: true,
        access: Access::ExamsRead,
    },
    ReportDef {
        code: "exams.merit",
        module: Module::Exams,
        params: &[YEAR, ASSESSMENT, CLASS_REQ, TOP],
        landscape: false,
        access: Access::ExamsRead,
    },
    ReportDef {
        code: "exams.analysis",
        module: Module::Exams,
        params: &[YEAR, ASSESSMENT, CLASS_REQ, SECTION],
        landscape: true,
        access: Access::ExamsRead,
    },
    ReportDef {
        code: "exams.grades",
        module: Module::Exams,
        params: &[YEAR, ASSESSMENT, CLASS_REQ],
        landscape: false,
        access: Access::ExamsRead,
    },
    ReportDef {
        code: "exams.failing",
        module: Module::Exams,
        params: &[YEAR, ASSESSMENT, CLASS_REQ, SECTION],
        landscape: false,
        access: Access::ExamsRead,
    },
    ReportDef {
        code: "exams.report_cards",
        module: Module::Exams,
        params: &[YEAR, CLASS_REQ, SECTION_REQ],
        landscape: false,
        access: Access::ExamsRead,
    },
    // ---- Board results
    ReportDef {
        code: "exams.board_sheet",
        module: Module::Exams,
        params: &[YEAR, BOARD_EXAM, CLASS],
        landscape: true,
        access: Access::ExamsRead,
    },
    ReportDef {
        code: "exams.board_merit",
        module: Module::Exams,
        params: &[YEAR, BOARD_EXAM, CLASS, TOP],
        landscape: false,
        access: Access::ExamsRead,
    },
    ReportDef {
        code: "exams.board_pass",
        module: Module::Exams,
        params: &[YEAR, BOARD_EXAM, CLASS],
        landscape: false,
        access: Access::ExamsRead,
    },
    ReportDef {
        code: "exams.board_card",
        module: Module::Exams,
        params: &[YEAR, BOARD_EXAM, STUDENT],
        landscape: false,
        access: Access::ExamsRead,
    },
    // Timetable (Standard stage 1)
    ReportDef {
        code: "timetable.section",
        module: Module::Timetable,
        params: &[YEAR, CLASS_REQ, SECTION_REQ],
        landscape: true,
        access: Access::TimetableRead,
    },
    ReportDef {
        code: "timetable.teacher_load",
        module: Module::Timetable,
        params: &[YEAR],
        landscape: false,
        access: Access::TimetableRead,
    },
    ReportDef {
        code: "timetable.free_rooms",
        module: Module::Timetable,
        params: &[YEAR, DAY],
        landscape: true,
        access: Access::TimetableRead,
    },
    ReportDef {
        code: "timetable.cover_sheet",
        module: Module::Timetable,
        params: &[YEAR, FROM, TO],
        landscape: true,
        access: Access::TimetableRead,
    },
    // Leave (Standard stage 2)
    ReportDef {
        code: "leave.balances",
        module: Module::Leave,
        params: &[YEAR],
        landscape: false,
        access: Access::LeaveRead,
    },
    ReportDef {
        code: "leave.register",
        module: Module::Leave,
        params: &[YEAR, FROM_OPT, TO_OPT, LEAVE_STATUS],
        landscape: true,
        access: Access::LeaveRead,
    },
    ReportDef {
        code: "leave.students",
        module: Module::Leave,
        params: &[YEAR, CLASS, SECTION, FROM_OPT, TO_OPT],
        landscape: true,
        access: Access::LeaveRead,
    },
];

pub fn find(code: &str) -> Option<&'static ReportDef> {
    REPORTS.iter().find(|r| r.code == code)
}

/// The catalogue as the UI sees it, labels in the requested language.
#[derive(Debug, Clone, Serialize)]
pub struct CatalogueModule {
    pub module: Module,
    pub label: String,
    pub reports: Vec<CatalogueReport>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogueReport {
    pub code: String,
    pub title: String,
    pub description: String,
    pub landscape: bool,
    pub access: Access,
    pub params: Vec<CatalogueParam>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogueParam {
    pub name: String,
    pub kind: ParamKind,
    pub label: String,
    pub required: bool,
}

/// The catalogue for a caller: only the reports `allowed` says they may run.
pub fn catalogue(lang: Lang, allowed: impl Fn(&ReportDef) -> bool) -> Vec<CatalogueModule> {
    let mut modules: Vec<CatalogueModule> = Vec::new();
    for def in REPORTS.iter().filter(|d| allowed(d)) {
        let entry = CatalogueReport {
            code: def.code.into(),
            title: tr(lang, &format!("report.{}", def.code)).into(),
            description: tr(lang, &format!("report.{}.desc", def.code)).into(),
            landscape: def.landscape,
            access: def.access,
            params: def
                .params
                .iter()
                .map(|p| CatalogueParam {
                    name: p.name.into(),
                    kind: p.kind,
                    label: tr(lang, p.label_key()).into(),
                    required: p.required,
                })
                .collect(),
        };
        match modules.iter_mut().find(|m| m.module == def.module) {
            Some(m) => m.reports.push(entry),
            None => modules.push(CatalogueModule {
                module: def.module,
                label: tr(lang, def.module.key()).into(),
                reports: vec![entry],
            }),
        }
    }
    modules
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_report_has_texts_in_both_languages_and_unique_codes() {
        let mut codes = std::collections::HashSet::new();
        for r in REPORTS {
            assert!(codes.insert(r.code), "duplicate code {}", r.code);
            assert!(
                super::super::text::has(&format!("report.{}", r.code)),
                "title for {}",
                r.code
            );
            assert!(
                super::super::text::has(&format!("report.{}.desc", r.code)),
                "description for {}",
                r.code
            );
        }
        let ur = catalogue(Lang::Ur, |_| true);
        assert_eq!(ur[0].label, "طلبہ");
        assert_eq!(
            ur.len(),
            6,
            "students, fees, attendance, exams, timetable, leave"
        );
        assert!(ur[0]
            .reports
            .iter()
            .any(|r| r.code == "students.list" && r.title == "طلبہ کی فہرست"));
        let teacher = catalogue(Lang::En, |d| d.access != Access::ReportsRead);
        assert_eq!(
            teacher.len(),
            4,
            "attendance, exams, timetable and leave only"
        );
        assert!(teacher.iter().all(|m| m.module != Module::Fees));
        for r in REPORTS {
            for p in r.params {
                assert!(
                    super::super::text::has(p.label_key()),
                    "label for {} in {}",
                    p.name,
                    r.code
                );
            }
        }
        assert!(find("students.list").is_some());
        assert!(find("nope").is_none());
    }
}
