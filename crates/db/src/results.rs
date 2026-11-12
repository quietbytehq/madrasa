//! Result sheets and report cards: the calculation behind a section's results and a
//! student's report card. It lives here, below the school API, so that the Nexus hub can
//! run the very same code on its copy of a school (the parent app shows a child's report
//! card from it): a figure a parent sees is the figure the school sees.
//!
//! Callers check permissions; nothing here does.

use madrasa_core::exams::{self, GradingScheme, Score, SheetStatus};
use rust_decimal::Decimal;
use serde::Serialize;
use std::collections::HashMap;
use uuid::Uuid;

use crate::exams as repo;
use crate::exams::Term;
use crate::models::{AssessmentType, MarkRow, Section};
use crate::{academics, attendance, schools, students, DbError, DbResult};

// ---- terms and their components -------------------------------------------------------------

/// Mirrored by `ExamStructure` in the UI: the year's terms with their components in order,
/// plus components without a term (years from before terms, until they are migrated).
#[derive(Debug, Serialize)]
pub struct Structure {
    pub terms: Vec<TermWithComponents>,
    pub unassigned: Vec<AssessmentType>,
}

#[derive(Debug, Serialize)]
pub struct TermWithComponents {
    #[serde(flatten)]
    pub term: Term,
    pub components: Vec<AssessmentType>,
}

pub async fn structure_of(
    c: &mut sqlx::PgConnection,
    academic_year_id: Uuid,
) -> DbResult<Structure> {
    let terms = repo::list_terms(c, academic_year_id).await?;
    let all = repo::list_assessments(c, academic_year_id).await?;
    let mut out = Vec::new();
    for t in terms {
        let components = all
            .iter()
            .filter(|a| a.term_id == Some(t.id))
            .cloned()
            .collect();
        out.push(TermWithComponents {
            term: t,
            components,
        });
    }
    let unassigned = all
        .iter()
        .filter(|a| a.term_id.is_none())
        .cloned()
        .collect();
    Ok(Structure {
        terms: out,
        unassigned,
    })
}

// ---- section header ----------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct SectionHeader {
    pub id: Uuid,
    pub class_id: Uuid,
    pub class_name: String,
    pub section_name: String,
    pub class_teacher_name: Option<String>,
}

pub async fn section_header(c: &mut sqlx::PgConnection, s: &Section) -> DbResult<SectionHeader> {
    let class = academics::get_class(c, s.class_id).await?;
    let teacher = match s.class_teacher_id {
        Some(id) => Some(crate::staff::get(c, id).await?.full_name),
        None => None,
    };
    Ok(SectionHeader {
        id: s.id,
        class_id: class.id,
        class_name: class.name,
        section_name: s.name.clone(),
        class_teacher_name: teacher,
    })
}

// ---- results ------------------------------------------------------------------------------

#[derive(Debug, Serialize, Clone)]
pub struct SubjectHeader {
    pub subject_id: Uuid,
    pub subject_name: String,
    pub max_marks: Decimal,
    pub sheet_status: SheetStatus,
}

#[derive(Debug, Serialize, Clone)]
pub struct SubjectMark {
    pub subject_id: Uuid,
    /// False when the student does not take this subject (M2 stage 6); the cell is empty.
    pub takes: bool,
    pub obtained: Option<Decimal>,
    pub is_absent: bool,
    pub percent: Decimal,
    pub grade: String,
}

#[derive(Debug, Serialize)]
pub struct ResultRow {
    pub student_id: Uuid,
    pub student_number: String,
    pub full_name: String,
    pub roll_no: Option<i32>,
    pub marks: Vec<SubjectMark>,
    pub total: Decimal,
    pub max: Decimal,
    pub percent: Decimal,
    pub grade: String,
    pub pass: bool,
    pub position: u32,
}

#[derive(Debug, Serialize)]
pub struct SubjectAnalysis {
    pub subject_id: Uuid,
    pub subject_name: String,
    pub max_marks: Decimal,
    pub appeared: u32,
    pub absent: u32,
    pub average_percent: Option<Decimal>,
    pub highest: Option<Decimal>,
    pub lowest: Option<Decimal>,
    pub passed: u32,
    pub failed: u32,
}

/// Mirrored by `ResultSheet` in the UI.
#[derive(Debug, Serialize)]
pub struct ResultSheet {
    pub assessment: AssessmentType,
    pub section: SectionHeader,
    pub subjects: Vec<SubjectHeader>,
    pub rows: Vec<ResultRow>,
    pub analysis: Vec<SubjectAnalysis>,
    pub grading: GradingScheme,
    /// Every subject's sheet is approved.
    pub complete: bool,
}

pub fn grade_of(scheme: &GradingScheme, pct: Decimal) -> String {
    scheme
        .grade_for(pct)
        .map(|b| b.grade.clone())
        .unwrap_or_default()
}

/// Marks of a component for a section. An entered component reads its sheets; a calculated
/// one evaluates its formula over the inputs' marks (recursively), student by student and
/// subject by subject, carrying the weakest sheet status of its inputs.
pub async fn marks_for(
    c: &mut sqlx::PgConnection,
    assessment: &AssessmentType,
    section_id: Uuid,
    depth: u8,
) -> DbResult<Vec<MarkRow>> {
    if assessment.kind != "calculated" {
        return repo::section_marks(c, assessment.id, section_id).await;
    }
    if depth > 8 {
        return Err(DbError::Invalid("The formula refers to itself.".into()));
    }
    let formula: madrasa_core::exams::Formula = assessment
        .formula
        .clone()
        .and_then(|v| serde_json::from_value(v).ok())
        .ok_or_else(|| DbError::Invalid("The calculated component has no formula.".into()))?;
    let mut inputs: Vec<(Uuid, Vec<MarkRow>)> = Vec::new();
    for i in &formula.inputs {
        let a = repo::get_assessment(c, i.assessment_type_id).await?;
        let rows = Box::pin(marks_for(c, &a, section_id, depth + 1)).await?;
        inputs.push((a.id, rows));
    }
    Ok(evaluate_rows(assessment.id, &formula, &inputs))
}

/// Combine input rows into one row per (student, subject).
pub fn evaluate_rows(
    component_id: Uuid,
    formula: &madrasa_core::exams::Formula,
    inputs: &[(Uuid, Vec<MarkRow>)],
) -> Vec<MarkRow> {
    let mut keys: Vec<(Uuid, Uuid)> = inputs
        .iter()
        .flat_map(|(_, rows)| rows.iter().map(|r| (r.student_id, r.subject_id)))
        .collect();
    keys.sort();
    keys.dedup();
    let mut out = Vec::new();
    for (student_id, subject_id) in keys {
        let scores: Vec<(Uuid, Option<Score>)> = inputs
            .iter()
            .map(|(id, rows)| {
                let found = rows
                    .iter()
                    .find(|r| r.student_id == student_id && r.subject_id == subject_id);
                (
                    *id,
                    found.map(|r| Score {
                        max: r.max_marks,
                        obtained: if r.is_absent { None } else { r.obtained },
                    }),
                )
            })
            .collect();
        let absent: Vec<Uuid> = inputs
            .iter()
            .filter(|(_, rows)| {
                rows.iter().any(|r| {
                    r.student_id == student_id && r.subject_id == subject_id && r.is_absent
                })
            })
            .map(|(id, _)| *id)
            .collect();
        let Some(score) = formula.evaluate(&scores, &absent) else {
            continue;
        };
        let statuses: Vec<&str> = inputs
            .iter()
            .filter_map(|(_, rows)| {
                rows.iter()
                    .find(|r| r.student_id == student_id && r.subject_id == subject_id)
                    .map(|r| r.sheet_status.as_str())
            })
            .collect();
        let status = if statuses
            .iter()
            .all(|s| *s == SheetStatus::Approved.as_str())
        {
            SheetStatus::Approved
        } else if statuses.iter().all(|s| *s != SheetStatus::Draft.as_str()) {
            SheetStatus::Submitted
        } else {
            SheetStatus::Draft
        };
        out.push(MarkRow {
            assessment_type_id: component_id,
            subject_id,
            student_id,
            max_marks: score.max,
            obtained: Some(score.obtained_or_zero()),
            is_absent: false,
            sheet_status: status.as_str().into(),
        });
    }
    out
}

pub async fn build_result_sheet(
    c: &mut sqlx::PgConnection,
    school_id: Uuid,
    assessment: AssessmentType,
    section: Section,
) -> DbResult<ResultSheet> {
    let grading = repo::grading(c, school_id).await?;
    let subjects_all = academics::list_subjects(c, school_id).await?;
    let rows_raw = marks_for(c, &assessment, section.id, 0).await?;
    let subjects: Vec<SubjectHeader> = if assessment.kind == "calculated" {
        // Subjects and maxima come from the computed rows; the status is approved only when
        // every contributing sheet is.
        let mut out: Vec<SubjectHeader> = Vec::new();
        for r in &rows_raw {
            if out.iter().any(|s| s.subject_id == r.subject_id) {
                continue;
            }
            let all_approved = rows_raw
                .iter()
                .filter(|x| x.subject_id == r.subject_id)
                .all(|x| x.sheet_status == SheetStatus::Approved.as_str());
            out.push(SubjectHeader {
                subject_id: r.subject_id,
                subject_name: subjects_all
                    .iter()
                    .find(|s| s.id == r.subject_id)
                    .map(|s| s.name.clone())
                    .unwrap_or_default(),
                max_marks: r.max_marks,
                sheet_status: if all_approved {
                    SheetStatus::Approved
                } else {
                    SheetStatus::Draft
                },
            });
        }
        out.sort_by(|a, b| a.subject_name.cmp(&b.subject_name));
        out
    } else {
        let summaries = repo::sheet_summaries(c, assessment.id, section.id).await?;
        summaries
            .iter()
            .filter_map(|s| {
                s.max_marks.map(|max| SubjectHeader {
                    subject_id: s.subject_id,
                    subject_name: s.subject_name.clone(),
                    max_marks: max,
                    sheet_status: s
                        .status
                        .as_deref()
                        .and_then(|st| st.parse().ok())
                        .unwrap_or_default(),
                })
            })
            .collect()
    };
    let by_key: HashMap<(Uuid, Uuid), &MarkRow> = rows_raw
        .iter()
        .map(|r| ((r.student_id, r.subject_id), r))
        .collect();
    let takes = crate::subjects::effective_for_section(c, section.id).await?;
    // A subject nobody in the section takes is not a column.
    let subjects: Vec<SubjectHeader> = subjects
        .into_iter()
        .filter(|s| takes.values().any(|ids| ids.contains(&s.subject_id)))
        .collect();
    let roster = repo::roster(c, section.id).await?;
    let mut rows: Vec<ResultRow> = roster
        .into_iter()
        .map(|r| {
            let own = takes.get(&r.student_id);
            let marks: Vec<SubjectMark> = subjects
                .iter()
                .map(|s| {
                    let takes_it = own.is_some_and(|ids| ids.contains(&s.subject_id));
                    let m = by_key
                        .get(&(r.student_id, s.subject_id))
                        .filter(|_| takes_it);
                    let obtained = m.and_then(|m| m.obtained);
                    let is_absent = m.map(|m| m.is_absent).unwrap_or(false);
                    let pct = exams::percent(obtained.unwrap_or(Decimal::ZERO), s.max_marks);
                    SubjectMark {
                        subject_id: s.subject_id,
                        takes: takes_it,
                        obtained,
                        is_absent,
                        percent: pct,
                        grade: grade_of(&grading, pct),
                    }
                })
                .collect();
            let totals = exams::totals(subjects.iter().zip(&marks).filter(|(_, m)| m.takes).map(
                |(s, m)| Score {
                    max: s.max_marks,
                    obtained: m.obtained,
                },
            ));
            ResultRow {
                student_id: r.student_id,
                student_number: r.student_number,
                full_name: r.full_name,
                roll_no: r.roll_no,
                marks,
                total: totals.obtained,
                max: totals.max,
                grade: grade_of(&grading, totals.percent),
                pass: grading.is_pass(totals.percent),
                percent: totals.percent,
                position: 0,
            }
        })
        .collect();
    let positions = exams::positions(&rows.iter().map(|r| r.percent).collect::<Vec<_>>());
    for (row, pos) in rows.iter_mut().zip(positions) {
        row.position = pos;
    }
    let analysis = subjects
        .iter()
        .map(|s| {
            let mut appeared = 0u32;
            let mut absent = 0u32;
            let mut passed = 0u32;
            let mut sum = Decimal::ZERO;
            let mut highest: Option<Decimal> = None;
            let mut lowest: Option<Decimal> = None;
            for row in &rows {
                let m = row
                    .marks
                    .iter()
                    .find(|m| m.subject_id == s.subject_id)
                    .unwrap();
                if !m.takes {
                    continue;
                }
                if m.is_absent {
                    absent += 1;
                } else if let Some(o) = m.obtained {
                    appeared += 1;
                    sum += m.percent;
                    highest = Some(highest.map_or(o, |h| h.max(o)));
                    lowest = Some(lowest.map_or(o, |l| l.min(o)));
                    if grading.is_pass(m.percent) {
                        passed += 1;
                    }
                }
            }
            SubjectAnalysis {
                subject_id: s.subject_id,
                subject_name: s.subject_name.clone(),
                max_marks: s.max_marks,
                appeared,
                absent,
                average_percent: (appeared > 0)
                    .then(|| exams::round2(sum / Decimal::from(appeared))),
                highest,
                lowest,
                passed,
                failed: appeared - passed,
            }
        })
        .collect();
    let complete = !subjects.is_empty()
        && subjects
            .iter()
            .all(|s| s.sheet_status == SheetStatus::Approved);
    Ok(ResultSheet {
        assessment,
        section: section_header(c, &section).await?,
        subjects,
        rows,
        analysis,
        grading,
        complete,
    })
}

#[derive(Debug, Serialize, Clone)]
pub struct AssessmentHeader {
    pub id: Uuid,
    pub name: String,
    /// Weight within its term.
    pub weight: Decimal,
    /// The student has marks in at least one subject of this assessment.
    pub held: bool,
    pub term_id: Option<Uuid>,
    /// `entered` or `calculated`.
    pub kind: String,
}

/// A term of the year on the card: its components (by id, in order) and the term's totals.
#[derive(Debug, Clone, Serialize)]
pub struct CardTerm {
    pub id: Uuid,
    pub name: String,
    pub weight: Decimal,
    pub component_ids: Vec<Uuid>,
    /// Weighted percentage of the term's assessment totals (held components only).
    pub percent: Option<Decimal>,
    pub grade: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CardCell {
    pub max_marks: Option<Decimal>,
    pub obtained: Option<Decimal>,
    pub is_absent: bool,
    pub percent: Option<Decimal>,
    pub approved: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CardSubject {
    pub subject_id: Uuid,
    pub subject_name: String,
    /// One per assessment, in order.
    pub cells: Vec<CardCell>,
    /// One per term, in order: the weighted percentage of the term's held components.
    pub term_percents: Vec<Option<Decimal>>,
    /// Year result: term percentages weighted by term weight.
    pub weighted_percent: Option<Decimal>,
    pub grade: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CardAssessmentTotal {
    pub assessment_id: Uuid,
    pub total: Decimal,
    pub max: Decimal,
    pub percent: Option<Decimal>,
    pub grade: Option<String>,
    pub position: Option<u32>,
    pub students: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SchoolHeader {
    pub name: String,
    pub code: String,
    pub city: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AttendanceLine {
    pub present: u32,
    pub total: u32,
    pub percent: Option<f64>,
}

/// Mirrored by `ReportCard` in the UI.
#[derive(Debug, Clone, Serialize)]
pub struct ReportCard {
    pub school: SchoolHeader,
    pub academic_year: String,
    pub student_id: Uuid,
    pub student_number: String,
    pub full_name: String,
    pub father_name: String,
    pub class_name: Option<String>,
    pub section_name: Option<String>,
    pub roll_no: Option<i32>,
    pub assessments: Vec<AssessmentHeader>,
    pub terms: Vec<CardTerm>,
    pub subjects: Vec<CardSubject>,
    pub totals: Vec<CardAssessmentTotal>,
    pub overall_percent: Option<Decimal>,
    pub overall_grade: Option<String>,
    pub overall_remarks: Option<String>,
    pub pass: Option<bool>,
    pub attendance: AttendanceLine,
    pub grading: GradingScheme,
}

/// A student's report card as the school sees it: every mark entered so far, each cell
/// saying whether its sheet is approved.
pub async fn build_report_card(
    c: &mut sqlx::PgConnection,
    school_id: Uuid,
    student_id: Uuid,
    year: &crate::models::AcademicYear,
    section: Option<&Section>,
    positions_cache: &mut HashMap<Uuid, HashMap<Uuid, (u32, usize)>>,
) -> DbResult<ReportCard> {
    report_card(
        c,
        school_id,
        student_id,
        year,
        section,
        positions_cache,
        false,
    )
    .await
}

/// The report card a parent sees: built from **approved** marks sheets only. Marks still
/// in draft or waiting for approval are left out as if they had not been entered, and a
/// position in class is given only for an assessment whose sheets are all approved (a
/// rank that leans on unapproved marks of classmates is not shown).
pub async fn build_approved_report_card(
    c: &mut sqlx::PgConnection,
    school_id: Uuid,
    student_id: Uuid,
    year: &crate::models::AcademicYear,
    section: Option<&Section>,
) -> DbResult<ReportCard> {
    let mut cache = HashMap::new();
    report_card(c, school_id, student_id, year, section, &mut cache, true).await
}

async fn report_card(
    c: &mut sqlx::PgConnection,
    school_id: Uuid,
    student_id: Uuid,
    year: &crate::models::AcademicYear,
    section: Option<&Section>,
    positions_cache: &mut HashMap<Uuid, HashMap<Uuid, (u32, usize)>>,
    approved_only: bool,
) -> DbResult<ReportCard> {
    let student = students::get(c, student_id).await?;
    let school = schools::get(c, school_id).await?;
    let grading = repo::grading(c, school_id).await?;
    let structure = structure_of(c, year.id).await?;
    // Components in term order; components without a term come last (pre-term years).
    let mut components: Vec<AssessmentType> = structure
        .terms
        .iter()
        .flat_map(|t| t.components.iter().cloned())
        .chain(structure.unassigned.iter().cloned())
        .collect();
    components.sort_by_key(|a| {
        let term_pos = structure
            .terms
            .iter()
            .position(|t| Some(t.term.id) == a.term_id)
            .unwrap_or(usize::MAX);
        (term_pos, a.position, a.name.clone())
    });
    let mut entered_rows = repo::student_marks(c, student_id, year.id).await?;
    if approved_only {
        entered_rows.retain(|r| r.sheet_status == SheetStatus::Approved.as_str());
    }
    // Rows for every component: entered ones from the sheets, calculated ones evaluated.
    let mut rows: Vec<MarkRow> = entered_rows.clone();
    for a in components.iter().filter(|a| a.kind == "calculated") {
        if let Some(formula) = a
            .formula
            .clone()
            .and_then(|v| serde_json::from_value::<madrasa_core::exams::Formula>(v).ok())
        {
            let inputs: Vec<(Uuid, Vec<MarkRow>)> = formula
                .inputs
                .iter()
                .map(|i| {
                    (
                        i.assessment_type_id,
                        rows.iter()
                            .filter(|r| r.assessment_type_id == i.assessment_type_id)
                            .cloned()
                            .collect(),
                    )
                })
                .collect();
            rows.extend(evaluate_rows(a.id, &formula, &inputs));
        }
    }
    let subjects_all = academics::list_subjects(c, school_id).await?;
    let (class_name, section_name, roll_no) = match section {
        Some(s) => {
            let class = academics::get_class(c, s.class_id).await?;
            let roll = repo::roster(c, s.id)
                .await?
                .into_iter()
                .find(|r| r.student_id == student_id)
                .and_then(|r| r.roll_no);
            (Some(class.name), Some(s.name.clone()), roll)
        }
        None => (None, None, None),
    };
    let assessments: Vec<AssessmentHeader> = components
        .iter()
        .map(|a| AssessmentHeader {
            id: a.id,
            name: a.name.clone(),
            weight: a.weight,
            held: rows.iter().any(|r| r.assessment_type_id == a.id),
            term_id: a.term_id,
            kind: a.kind.clone(),
        })
        .collect();
    let term_defs: Vec<(Uuid, String, Decimal, Vec<Uuid>)> = structure
        .terms
        .iter()
        .map(|t| {
            (
                t.term.id,
                t.term.name.clone(),
                t.term.weight,
                t.components.iter().map(|a| a.id).collect(),
            )
        })
        .collect();
    let mut subject_ids: Vec<Uuid> = rows.iter().map(|r| r.subject_id).collect();
    if let Some(s) = section {
        // The student's own subjects (every class subject unless allocated), M2 stage 6.
        let own = crate::subjects::effective_for_section(c, s.id)
            .await?
            .remove(&student_id)
            .unwrap_or_else(Vec::new);
        if !own.is_empty() {
            subject_ids.retain(|id| own.contains(id));
            rows.retain(|r| own.contains(&r.subject_id));
        }
        subject_ids.extend(own);
    }
    subject_ids.sort();
    subject_ids.dedup();
    // Year result of a list of (weight, score) by term: weight within the term first, then
    // the term weights. A year without terms is one group.
    let year_percent = |cells: &[(Option<Uuid>, Decimal, Option<Score>)]| -> (Vec<Option<Decimal>>, Option<Decimal>) {
        if term_defs.is_empty() {
            let parts: Vec<(Decimal, Score)> = cells.iter().filter_map(|(_, w, s)| s.map(|s| (*w, s))).collect();
            return (Vec::new(), exams::weighted_percent(&parts));
        }
        let term_percents: Vec<Option<Decimal>> = term_defs
            .iter()
            .map(|(tid, _, _, _)| {
                let parts: Vec<(Decimal, Score)> = cells
                    .iter()
                    .filter(|(t, _, _)| *t == Some(*tid))
                    .filter_map(|(_, w, s)| s.map(|s| (*w, s)))
                    .collect();
                exams::weighted_percent(&parts)
            })
            .collect();
        let year_parts: Vec<(Decimal, Score)> = term_defs
            .iter()
            .zip(&term_percents)
            .filter_map(|((_, _, w, _), p)| {
                p.map(|p| {
                    (
                        *w,
                        Score {
                            max: Decimal::ONE_HUNDRED,
                            obtained: Some(p),
                        },
                    )
                })
            })
            .collect();
        (term_percents, exams::weighted_percent(&year_parts))
    };
    let mut subjects: Vec<CardSubject> = subject_ids
        .iter()
        .filter_map(|sid| {
            let name = subjects_all.iter().find(|s| s.id == *sid)?.name.clone();
            let cells: Vec<CardCell> = assessments
                .iter()
                .map(|a| {
                    match rows
                        .iter()
                        .find(|r| r.assessment_type_id == a.id && r.subject_id == *sid)
                    {
                        Some(r) => CardCell {
                            max_marks: Some(r.max_marks),
                            obtained: r.obtained,
                            is_absent: r.is_absent,
                            percent: Some(exams::percent(
                                r.obtained.unwrap_or(Decimal::ZERO),
                                r.max_marks,
                            )),
                            approved: r.sheet_status == SheetStatus::Approved.as_str(),
                        },
                        None => CardCell {
                            max_marks: None,
                            obtained: None,
                            is_absent: false,
                            percent: None,
                            approved: false,
                        },
                    }
                })
                .collect();
            let scored: Vec<(Option<Uuid>, Decimal, Option<Score>)> = assessments
                .iter()
                .zip(&cells)
                .map(|(a, cell)| {
                    (
                        a.term_id,
                        a.weight,
                        cell.max_marks.map(|max| Score {
                            max,
                            obtained: cell.obtained,
                        }),
                    )
                })
                .collect();
            let (term_percents, weighted) = year_percent(&scored);
            Some(CardSubject {
                subject_id: *sid,
                subject_name: name,
                cells,
                term_percents,
                weighted_percent: weighted,
                grade: weighted.map(|w| grade_of(&grading, w)),
            })
        })
        .collect();
    subjects.sort_by(|a, b| a.subject_name.cmp(&b.subject_name));

    let mut totals = Vec::new();
    for a in &assessments {
        let scores: Vec<Score> = subjects
            .iter()
            .filter_map(|s| {
                let i = assessments.iter().position(|x| x.id == a.id)?;
                let cell = &s.cells[i];
                cell.max_marks.map(|max| Score {
                    max,
                    obtained: cell.obtained,
                })
            })
            .collect();
        let held = !scores.is_empty();
        let t = exams::totals(scores);
        let (position, count) = match (section, held) {
            (Some(sec), true) => {
                let ranks = match positions_cache.get(&a.id) {
                    Some(r) => r,
                    None => {
                        let assessment = repo::get_assessment(c, a.id).await?;
                        let sheet =
                            build_result_sheet(c, school_id, assessment, sec.clone()).await?;
                        let map: HashMap<Uuid, (u32, usize)> = if approved_only && !sheet.complete {
                            HashMap::new()
                        } else {
                            sheet
                                .rows
                                .iter()
                                .map(|r| (r.student_id, (r.position, sheet.rows.len())))
                                .collect()
                        };
                        positions_cache.entry(a.id).or_insert(map)
                    }
                };
                ranks
                    .get(&student_id)
                    .map(|(p, n)| (Some(*p), *n))
                    .unwrap_or((None, 0))
            }
            _ => (None, 0),
        };
        totals.push(CardAssessmentTotal {
            assessment_id: a.id,
            total: t.obtained,
            max: t.max,
            percent: held.then_some(t.percent),
            grade: held.then(|| grade_of(&grading, t.percent)),
            position,
            students: count,
        });
    }
    let total_scores: Vec<(Option<Uuid>, Decimal, Option<Score>)> = assessments
        .iter()
        .zip(&totals)
        .map(|(a, t)| {
            (
                a.term_id,
                a.weight,
                (t.max > Decimal::ZERO).then_some(Score {
                    max: t.max,
                    obtained: Some(t.total),
                }),
            )
        })
        .collect();
    let (term_percents, overall) = year_percent(&total_scores);
    let terms: Vec<CardTerm> = term_defs
        .iter()
        .zip(term_percents.iter().chain(std::iter::repeat(&None)))
        .map(|((id, name, weight, ids), p)| CardTerm {
            id: *id,
            name: name.clone(),
            weight: *weight,
            component_ids: ids.clone(),
            percent: *p,
            grade: p.map(|p| grade_of(&grading, p)),
        })
        .collect();
    let band = overall.and_then(|o| grading.grade_for(o).cloned());
    let att = attendance::student_summary(c, student_id, year.start_date, year.end_date).await?;
    Ok(ReportCard {
        school: SchoolHeader {
            name: school.name,
            code: school.code,
            city: school.city,
        },
        academic_year: year.name.clone(),
        student_id,
        student_number: student.student_number,
        full_name: student.full_name,
        father_name: student.father_name,
        class_name,
        section_name,
        roll_no,
        assessments,
        terms,
        subjects,
        totals,
        overall_percent: overall,
        overall_grade: band.as_ref().map(|b| b.grade.clone()),
        overall_remarks: band.as_ref().map(|b| b.remarks.clone()),
        pass: overall.map(|o| grading.is_pass(o)),
        attendance: AttendanceLine {
            present: att.present + att.late,
            total: att.total(),
            percent: att.percent(),
        },
        grading,
    })
}
