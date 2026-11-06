//! Rules for external examination boards (M2 stage 4): grade from a board's bands, a
//! candidate's totals and overall result, and positions. Exact decimals throughout.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::exams::{percent, positions, round2};

/// A grade band of a board: the grade earned from `min_percent` upwards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardBand {
    pub grade: String,
    pub min_percent: Decimal,
}

/// Bands sorted highest first; empty bands mean the board reports no grades.
pub fn validate_bands(bands: &[BoardBand]) -> Result<Vec<BoardBand>, String> {
    let mut out = bands.to_vec();
    for b in &out {
        if b.grade.trim().is_empty() {
            return Err("Every grade band needs a grade.".into());
        }
        if b.min_percent < Decimal::ZERO || b.min_percent > Decimal::from(100) {
            return Err("Band minimums must be between 0 and 100.".into());
        }
    }
    out.sort_by_key(|b| std::cmp::Reverse(b.min_percent));
    for w in out.windows(2) {
        if w[0].min_percent == w[1].min_percent {
            return Err("Two grade bands start at the same percentage.".into());
        }
    }
    let mut names: Vec<&str> = out.iter().map(|b| b.grade.trim()).collect();
    names.sort_unstable();
    names.dedup();
    if names.len() != out.len() {
        return Err("Grade names must be unique.".into());
    }
    Ok(out)
}

/// The grade for a percentage, or `None` when the board has no bands or the percentage is
/// below the lowest band.
pub fn grade_for(bands: &[BoardBand], pct: Decimal) -> Option<String> {
    bands
        .iter()
        .filter(|b| pct >= b.min_percent)
        .max_by(|a, b| a.min_percent.cmp(&b.min_percent))
        .map(|b| b.grade.clone())
}

/// Per-subject result status as the board reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubjectStatus {
    Pass,
    Fail,
    Absent,
    Withheld,
}

impl SubjectStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SubjectStatus::Pass => "pass",
            SubjectStatus::Fail => "fail",
            SubjectStatus::Absent => "absent",
            SubjectStatus::Withheld => "withheld",
        }
    }
    pub fn parse(s: &str) -> Option<SubjectStatus> {
        match s.trim().to_ascii_lowercase().as_str() {
            "pass" | "p" | "passed" => Some(SubjectStatus::Pass),
            "fail" | "f" | "failed" => Some(SubjectStatus::Fail),
            "absent" | "a" | "ab" | "abs" => Some(SubjectStatus::Absent),
            "withheld" | "w" | "rl" | "result later" => Some(SubjectStatus::Withheld),
            _ => None,
        }
    }
}

/// One subject's imported result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubjectResult {
    pub obtained: Option<Decimal>,
    pub max_marks: Decimal,
    pub status: SubjectStatus,
    pub grade: Option<String>,
}

/// Work out a subject's status and grade from the marks when the board gave only marks:
/// absent stays absent, otherwise pass/fail against the pass percentage and the grade from
/// the bands (an imported grade wins over a computed one).
pub fn subject_outcome(
    obtained: Option<Decimal>,
    max_marks: Decimal,
    status: Option<SubjectStatus>,
    grade: Option<String>,
    pass_percent: Decimal,
    bands: &[BoardBand],
) -> SubjectResult {
    let status = status.unwrap_or_else(|| match obtained {
        None => SubjectStatus::Absent,
        Some(o) if percent(o, max_marks) >= pass_percent => SubjectStatus::Pass,
        Some(_) => SubjectStatus::Fail,
    });
    let grade = grade
        .map(|g| g.trim().to_string())
        .filter(|g| !g.is_empty())
        .or_else(|| match (status, obtained) {
            (SubjectStatus::Absent | SubjectStatus::Withheld, _) => None,
            (_, Some(o)) => grade_for(bands, percent(o, max_marks)),
            _ => None,
        });
    SubjectResult {
        obtained,
        max_marks,
        status,
        grade,
    }
}

/// Overall status of a candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CandidateStatus {
    Pass,
    Fail,
    Absent,
    Withheld,
    /// No results imported yet.
    Pending,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateTotals {
    pub total: Decimal,
    pub max: Decimal,
    pub percent: Option<Decimal>,
    pub grade: Option<String>,
    pub status: CandidateStatus,
}

/// Totals over a candidate's subject results. Withheld anywhere withholds the result; absent
/// everywhere is absent; any failed or absent paper fails; otherwise the overall percentage
/// decides against the pass percentage. Marks of absent papers count as zero.
pub fn candidate_totals(
    results: &[SubjectResult],
    pass_percent: Decimal,
    bands: &[BoardBand],
) -> CandidateTotals {
    if results.is_empty() {
        return CandidateTotals {
            total: Decimal::ZERO,
            max: Decimal::ZERO,
            percent: None,
            grade: None,
            status: CandidateStatus::Pending,
        };
    }
    let total = round2(results.iter().filter_map(|r| r.obtained).sum());
    let max = round2(results.iter().map(|r| r.max_marks).sum());
    let pct = (max > Decimal::ZERO).then(|| percent(total, max));
    let status = if results.iter().any(|r| r.status == SubjectStatus::Withheld) {
        CandidateStatus::Withheld
    } else if results.iter().all(|r| r.status == SubjectStatus::Absent) {
        CandidateStatus::Absent
    } else if results
        .iter()
        .any(|r| matches!(r.status, SubjectStatus::Fail | SubjectStatus::Absent))
        || pct.is_some_and(|p| p < pass_percent)
    {
        CandidateStatus::Fail
    } else {
        CandidateStatus::Pass
    };
    let grade = match status {
        CandidateStatus::Pass | CandidateStatus::Fail => pct.and_then(|p| grade_for(bands, p)),
        _ => None,
    };
    CandidateTotals {
        total,
        max,
        percent: pct,
        grade,
        status,
    }
}

/// Competition ranking (1, 1, 3) of candidates with a percentage; others get `None`.
pub fn candidate_positions(percents: &[Option<Decimal>]) -> Vec<Option<u32>> {
    let ranked: Vec<Decimal> = percents.iter().filter_map(|p| *p).collect();
    let pos = positions(&ranked);
    let mut it = pos.into_iter();
    percents
        .iter()
        .map(|p| p.map(|_| it.next().unwrap_or(0)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn bands() -> Vec<BoardBand> {
        validate_bands(&[
            BoardBand {
                grade: "C".into(),
                min_percent: dec!(50),
            },
            BoardBand {
                grade: "A".into(),
                min_percent: dec!(70),
            },
            BoardBand {
                grade: "A+".into(),
                min_percent: dec!(80),
            },
            BoardBand {
                grade: "B".into(),
                min_percent: dec!(60),
            },
            BoardBand {
                grade: "D".into(),
                min_percent: dec!(40),
            },
            BoardBand {
                grade: "E".into(),
                min_percent: dec!(33),
            },
        ])
        .unwrap()
    }

    #[test]
    fn bands_are_validated_sorted_and_looked_up() {
        let b = bands();
        assert_eq!(b[0].grade, "A+");
        assert_eq!(grade_for(&b, dec!(95)), Some("A+".into()));
        assert_eq!(grade_for(&b, dec!(70)), Some("A".into()));
        assert_eq!(grade_for(&b, dec!(32.9)), None);
        assert_eq!(grade_for(&[], dec!(90)), None);
        assert!(validate_bands(&[BoardBand {
            grade: "".into(),
            min_percent: dec!(1)
        }])
        .is_err());
        assert!(validate_bands(&[BoardBand {
            grade: "A".into(),
            min_percent: dec!(101)
        }])
        .is_err());
        assert!(validate_bands(&[
            BoardBand {
                grade: "A".into(),
                min_percent: dec!(50)
            },
            BoardBand {
                grade: "B".into(),
                min_percent: dec!(50)
            }
        ])
        .is_err());
        assert!(validate_bands(&[
            BoardBand {
                grade: "A".into(),
                min_percent: dec!(50)
            },
            BoardBand {
                grade: "A".into(),
                min_percent: dec!(60)
            }
        ])
        .is_err());
    }

    #[test]
    fn subject_outcomes_from_marks_or_from_the_board() {
        let b = bands();
        let r = subject_outcome(Some(dec!(72)), dec!(100), None, None, dec!(33), &b);
        assert_eq!(
            (r.status, r.grade.as_deref()),
            (SubjectStatus::Pass, Some("A"))
        );
        let r = subject_outcome(Some(dec!(20)), dec!(100), None, None, dec!(33), &b);
        assert_eq!((r.status, r.grade), (SubjectStatus::Fail, None));
        let r = subject_outcome(None, dec!(100), None, None, dec!(33), &b);
        assert_eq!((r.status, r.grade), (SubjectStatus::Absent, None));
        // The board's own grade and status win.
        let r = subject_outcome(
            Some(dec!(72)),
            dec!(100),
            Some(SubjectStatus::Fail),
            Some("B".into()),
            dec!(33),
            &b,
        );
        assert_eq!(
            (r.status, r.grade.as_deref()),
            (SubjectStatus::Fail, Some("B"))
        );
        assert_eq!(SubjectStatus::parse("ABS"), Some(SubjectStatus::Absent));
        assert_eq!(SubjectStatus::parse("RL"), Some(SubjectStatus::Withheld));
        assert_eq!(SubjectStatus::parse("maybe"), None);
    }

    #[test]
    fn candidate_totals_follow_the_board_rules() {
        let b = bands();
        let sr = |o: Option<Decimal>, st: SubjectStatus| SubjectResult {
            obtained: o,
            max_marks: dec!(100),
            status: st,
            grade: None,
        };
        let pass = candidate_totals(
            &[
                sr(Some(dec!(80)), SubjectStatus::Pass),
                sr(Some(dec!(60)), SubjectStatus::Pass),
            ],
            dec!(33),
            &b,
        );
        assert_eq!(
            (
                pass.total,
                pass.max,
                pass.percent,
                pass.grade.as_deref(),
                pass.status
            ),
            (
                dec!(140),
                dec!(200),
                Some(dec!(70)),
                Some("A"),
                CandidateStatus::Pass
            )
        );
        let fail = candidate_totals(
            &[
                sr(Some(dec!(80)), SubjectStatus::Pass),
                sr(Some(dec!(10)), SubjectStatus::Fail),
            ],
            dec!(33),
            &b,
        );
        assert_eq!(
            (fail.status, fail.grade.as_deref()),
            (CandidateStatus::Fail, Some("D"))
        );
        let absent_paper = candidate_totals(
            &[
                sr(Some(dec!(90)), SubjectStatus::Pass),
                sr(None, SubjectStatus::Absent),
            ],
            dec!(33),
            &b,
        );
        assert_eq!(
            (
                absent_paper.status,
                absent_paper.total,
                absent_paper.percent
            ),
            (CandidateStatus::Fail, dec!(90), Some(dec!(45)))
        );
        let all_absent = candidate_totals(&[sr(None, SubjectStatus::Absent)], dec!(33), &b);
        assert_eq!(all_absent.status, CandidateStatus::Absent);
        let withheld = candidate_totals(
            &[
                sr(Some(dec!(90)), SubjectStatus::Pass),
                sr(None, SubjectStatus::Withheld),
            ],
            dec!(33),
            &b,
        );
        assert_eq!(
            (withheld.status, withheld.grade),
            (CandidateStatus::Withheld, None)
        );
        let low = candidate_totals(
            &[
                sr(Some(dec!(34)), SubjectStatus::Pass),
                sr(Some(dec!(34)), SubjectStatus::Pass),
            ],
            dec!(40),
            &b,
        );
        assert_eq!(
            low.status,
            CandidateStatus::Fail,
            "overall below the pass percentage"
        );
        assert_eq!(
            candidate_totals(&[], dec!(33), &b).status,
            CandidateStatus::Pending
        );
    }

    #[test]
    fn positions_skip_candidates_without_a_percentage() {
        let p = candidate_positions(&[Some(dec!(90)), None, Some(dec!(90)), Some(dec!(70))]);
        assert_eq!(p, vec![Some(1), None, Some(1), Some(3)]);
    }
}
