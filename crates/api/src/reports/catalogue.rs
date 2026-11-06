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
}

impl Module {
    fn key(self) -> &'static str {
        match self {
            Module::Students => "module.students",
            Module::Fees => "module.fees",
            Module::Attendance => "module.attendance",
            Module::Exams => "module.exams",
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
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ParamDef {
    pub name: &'static str,
    pub kind: ParamKind,
    pub required: bool,
}

impl ParamDef {
    fn label_key(&self) -> &'static str {
        match self.kind {
            ParamKind::AcademicYear => "param.academic_year",
            ParamKind::Class => "param.class",
            ParamKind::Section => "param.section",
            ParamKind::Status => "param.status",
            ParamKind::Month => "param.month",
            ParamKind::DateFrom => "param.from",
            ParamKind::DateTo => "param.to",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ReportDef {
    pub code: &'static str,
    pub module: Module,
    pub params: &'static [ParamDef],
    pub landscape: bool,
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

pub const REPORTS: &[ReportDef] = &[
    ReportDef {
        code: "students.list",
        module: Module::Students,
        params: &[YEAR, CLASS, SECTION, STATUS],
        landscape: true,
    },
    ReportDef {
        code: "students.strength",
        module: Module::Students,
        params: &[YEAR, CLASS],
        landscape: false,
    },
    ReportDef {
        code: "students.movements",
        module: Module::Students,
        params: &[YEAR, FROM, TO, CLASS],
        landscape: false,
    },
    ReportDef {
        code: "students.contacts",
        module: Module::Students,
        params: &[YEAR, CLASS, SECTION],
        landscape: true,
    },
    ReportDef {
        code: "students.birthdays",
        module: Module::Students,
        params: &[YEAR, MONTH, CLASS],
        landscape: false,
    },
    ReportDef {
        code: "students.missing",
        module: Module::Students,
        params: &[YEAR, CLASS, SECTION],
        landscape: true,
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
    pub params: Vec<CatalogueParam>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogueParam {
    pub name: String,
    pub kind: ParamKind,
    pub label: String,
    pub required: bool,
}

pub fn catalogue(lang: Lang) -> Vec<CatalogueModule> {
    let mut modules: Vec<CatalogueModule> = Vec::new();
    for def in REPORTS {
        let entry = CatalogueReport {
            code: def.code.into(),
            title: tr(lang, &format!("report.{}", def.code)).into(),
            description: tr(lang, &format!("report.{}.desc", def.code)).into(),
            landscape: def.landscape,
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
        let ur = catalogue(Lang::Ur);
        assert_eq!(ur[0].label, "طلبہ");
        assert!(ur[0]
            .reports
            .iter()
            .any(|r| r.code == "students.list" && r.title == "طلبہ کی فہرست"));
        assert!(find("students.list").is_some());
        assert!(find("nope").is_none());
    }
}
