//! Exam arithmetic: assessment weights, grading schemes, weighted results and positions.
//! Pure, exact decimals (`rust_decimal`); every stored or shown number is rounded half away
//! from zero to two places.
//!
//! Rules:
//! - An academic year has assessment types (default: Bi-monthly Test 1, Bi-monthly Test 2,
//!   Mid-year, Final) whose weights must total 100.
//! - A subject's percentage in one assessment = obtained / max × 100. Absent counts as 0.
//! - The year's weighted percentage of a subject = Σ weight × percentage / Σ weight over the
//!   assessments that have marks for that subject (an assessment not yet held is left out).
//! - Grades come from a scheme of bands: the band with the highest `min_percent` that is
//!   ≤ the percentage wins. `pass_percent` decides pass/fail.
//! - Positions use competition ranking (1, 1, 3) on the percentage, highest first.

use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};

pub fn round2(d: Decimal) -> Decimal {
    d.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

/// Percentage of `obtained` out of `max`, rounded; zero when `max` is zero.
pub fn percent(obtained: Decimal, max: Decimal) -> Decimal {
    if max <= Decimal::ZERO {
        return Decimal::ZERO;
    }
    round2(obtained * Decimal::ONE_HUNDRED / max)
}

/// Default assessment types for a new academic year, with weights that total 100.
pub const DEFAULT_ASSESSMENTS: &[(&str, u32)] = &[
    ("Bi-monthly Test 1", 15),
    ("Bi-monthly Test 2", 15),
    ("Mid-year", 30),
    ("Final", 40),
];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WeightError {
    #[error("weights must be between 0 and 100")]
    Range,
    #[error("weights must total 100 (currently {0})")]
    Total(Decimal),
}

/// Weights must each be 0–100 and add up to exactly 100.
pub fn validate_weights(weights: &[Decimal]) -> Result<(), WeightError> {
    if weights
        .iter()
        .any(|w| *w < Decimal::ZERO || *w > Decimal::ONE_HUNDRED)
    {
        return Err(WeightError::Range);
    }
    let total: Decimal = weights.iter().sum();
    if total != Decimal::ONE_HUNDRED {
        return Err(WeightError::Total(total));
    }
    Ok(())
}

// ---- grading ------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GradeBand {
    pub grade: String,
    /// Inclusive lower bound of the percentage for this grade.
    pub min_percent: Decimal,
    #[serde(default)]
    pub remarks: String,
}

/// Mirrored by `GradingScheme` in the UI; stored in settings as `exams.grading`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GradingScheme {
    pub bands: Vec<GradeBand>,
    pub pass_percent: Decimal,
}

impl Default for GradingScheme {
    fn default() -> Self {
        let band = |g: &str, min: u32, r: &str| GradeBand {
            grade: g.into(),
            min_percent: Decimal::from(min),
            remarks: r.into(),
        };
        Self {
            bands: vec![
                band("A+", 90, "Outstanding"),
                band("A", 80, "Excellent"),
                band("B", 70, "Very good"),
                band("C", 60, "Good"),
                band("D", 50, "Satisfactory"),
                band("E", 40, "Needs improvement"),
                band("F", 0, "Fail"),
            ],
            pass_percent: Decimal::from(40),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GradingError {
    #[error("at least one grade band is required")]
    Empty,
    #[error("grade names must be unique and not empty")]
    Names,
    #[error("band minimums must be between 0 and 100 and distinct")]
    Bounds,
    #[error("one band must start at 0 so every percentage gets a grade")]
    NoFloor,
    #[error("pass percentage must be between 0 and 100")]
    Pass,
}

impl GradingScheme {
    /// Validate and return the scheme with bands sorted from the highest minimum down.
    pub fn normalized(mut self) -> Result<Self, GradingError> {
        if self.bands.is_empty() {
            return Err(GradingError::Empty);
        }
        for b in &mut self.bands {
            b.grade = b.grade.trim().to_string();
            b.remarks = b.remarks.trim().to_string();
        }
        let mut names: Vec<&str> = self.bands.iter().map(|b| b.grade.as_str()).collect();
        names.sort_unstable();
        if names.iter().any(|n| n.is_empty()) || names.windows(2).any(|w| w[0] == w[1]) {
            return Err(GradingError::Names);
        }
        let mut mins: Vec<Decimal> = self.bands.iter().map(|b| b.min_percent).collect();
        mins.sort_unstable();
        if mins
            .iter()
            .any(|m| *m < Decimal::ZERO || *m > Decimal::ONE_HUNDRED)
            || mins.windows(2).any(|w| w[0] == w[1])
        {
            return Err(GradingError::Bounds);
        }
        if mins[0] != Decimal::ZERO {
            return Err(GradingError::NoFloor);
        }
        if self.pass_percent < Decimal::ZERO || self.pass_percent > Decimal::ONE_HUNDRED {
            return Err(GradingError::Pass);
        }
        self.bands.sort_by(|a, b| b.min_percent.cmp(&a.min_percent));
        Ok(self)
    }

    /// Grade for a percentage (bands must be normalized: highest minimum first).
    pub fn grade_for(&self, percent: Decimal) -> Option<&GradeBand> {
        self.bands.iter().find(|b| percent >= b.min_percent)
    }

    pub fn is_pass(&self, percent: Decimal) -> bool {
        percent >= self.pass_percent
    }
}

// ---- results ------------------------------------------------------------------------------

/// Marks of one subject in one assessment. `obtained` is `None` for absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Score {
    pub max: Decimal,
    pub obtained: Option<Decimal>,
}

impl Score {
    pub fn obtained_or_zero(&self) -> Decimal {
        self.obtained.unwrap_or(Decimal::ZERO)
    }
    pub fn percent(&self) -> Decimal {
        percent(self.obtained_or_zero(), self.max)
    }
}

/// Totals over several subjects of one assessment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Totals {
    pub max: Decimal,
    pub obtained: Decimal,
    pub percent: Decimal,
}

pub fn totals(scores: impl IntoIterator<Item = Score>) -> Totals {
    let mut t = Totals::default();
    for s in scores {
        t.max += s.max;
        t.obtained += s.obtained_or_zero();
    }
    t.obtained = round2(t.obtained);
    t.max = round2(t.max);
    t.percent = percent(t.obtained, t.max);
    t
}

/// Weighted percentage of a subject across assessments: `(weight, score)` pairs. Only the
/// assessments listed count; the result is `None` when no assessment has marks yet or the
/// listed weights total zero.
pub fn weighted_percent(parts: &[(Decimal, Score)]) -> Option<Decimal> {
    let total_weight: Decimal = parts.iter().map(|(w, _)| *w).sum();
    if parts.is_empty() || total_weight <= Decimal::ZERO {
        return None;
    }
    let sum: Decimal = parts.iter().map(|(w, s)| *w * s.percent()).sum();
    Some(round2(sum / total_weight))
}

/// Competition ranking (1, 1, 3) of percentages, highest first. Returns `(index, position)`
/// in the original order.
pub fn positions(percents: &[Decimal]) -> Vec<u32> {
    let mut order: Vec<usize> = (0..percents.len()).collect();
    order.sort_by(|a, b| percents[*b].cmp(&percents[*a]));
    let mut out = vec![0u32; percents.len()];
    let mut last: Option<(Decimal, u32)> = None;
    for (rank0, idx) in order.iter().enumerate() {
        let p = percents[*idx];
        let pos = match last {
            Some((lp, lpos)) if lp == p => lpos,
            _ => rank0 as u32 + 1,
        };
        last = Some((p, pos));
        out[*idx] = pos;
    }
    out
}

/// Workflow state of a marks sheet (one subject of one section in one assessment).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SheetStatus {
    /// Being entered by the teacher.
    #[default]
    Draft,
    /// Sent for approval; the teacher can no longer edit.
    Submitted,
    /// Locked by the principal.
    Approved,
}

impl SheetStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SheetStatus::Draft => "draft",
            SheetStatus::Submitted => "submitted",
            SheetStatus::Approved => "approved",
        }
    }
}

impl std::str::FromStr for SheetStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "draft" => Ok(SheetStatus::Draft),
            "submitted" => Ok(SheetStatus::Submitted),
            "approved" => Ok(SheetStatus::Approved),
            other => Err(format!("unknown sheet status: {other}")),
        }
    }
}

/// Validate one mark against the maximum: 0 ≤ obtained ≤ max, at most two decimals.
pub fn validate_mark(obtained: Decimal, max: Decimal) -> Result<Decimal, String> {
    if obtained < Decimal::ZERO {
        return Err("marks cannot be negative".into());
    }
    if obtained > max {
        return Err(format!("marks cannot exceed the maximum of {max}"));
    }
    if round2(obtained) != obtained {
        return Err("marks may have at most two decimal places".into());
    }
    Ok(obtained)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn s(max: u32, obtained: Option<&str>) -> Score {
        Score {
            max: Decimal::from(max),
            obtained: obtained.map(|o| o.parse().unwrap()),
        }
    }

    #[test]
    fn default_weights_total_100_and_validation_catches_others() {
        let w: Vec<Decimal> = DEFAULT_ASSESSMENTS
            .iter()
            .map(|(_, w)| Decimal::from(*w))
            .collect();
        assert_eq!(validate_weights(&w), Ok(()));
        assert_eq!(
            validate_weights(&[dec!(50), dec!(49.99)]),
            Err(WeightError::Total(dec!(99.99)))
        );
        assert_eq!(
            validate_weights(&[dec!(101), dec!(-1)]),
            Err(WeightError::Range)
        );
        assert_eq!(
            validate_weights(&[dec!(33.33), dec!(33.33), dec!(33.34)]),
            Ok(())
        );
    }

    #[test]
    fn percent_rounds_half_away_from_zero_and_handles_zero_max() {
        assert_eq!(percent(dec!(2), dec!(3)), dec!(66.67));
        assert_eq!(percent(dec!(1), dec!(8)), dec!(12.5));
        // 0.125 exactly → 12.5 (no further rounding); 7/16 = 43.75 → stays; 1/16 × 100 = 6.25
        assert_eq!(percent(dec!(0.005), dec!(1)), dec!(0.5));
        assert_eq!(percent(dec!(5), dec!(0)), dec!(0));
        assert_eq!(
            percent(dec!(33.335), dec!(100)),
            dec!(33.34),
            "half away from zero"
        );
        assert_eq!(round2(dec!(2.345)), dec!(2.35));
        assert_eq!(round2(dec!(2.344)), dec!(2.34));
    }

    #[test]
    fn grading_boundaries_are_inclusive_and_scheme_is_validated() {
        let g = GradingScheme::default().normalized().unwrap();
        assert_eq!(g.grade_for(dec!(90)).unwrap().grade, "A+");
        assert_eq!(g.grade_for(dec!(89.99)).unwrap().grade, "A");
        assert_eq!(g.grade_for(dec!(100)).unwrap().grade, "A+");
        assert_eq!(g.grade_for(dec!(40)).unwrap().grade, "E");
        assert_eq!(g.grade_for(dec!(39.99)).unwrap().grade, "F");
        assert_eq!(g.grade_for(dec!(0)).unwrap().grade, "F");
        assert!(g.is_pass(dec!(40)));
        assert!(!g.is_pass(dec!(39.99)));

        // Bands may be given in any order; they come back sorted.
        let custom = GradingScheme {
            bands: vec![
                GradeBand {
                    grade: "Fail".into(),
                    min_percent: dec!(0),
                    remarks: "".into(),
                },
                GradeBand {
                    grade: "Pass".into(),
                    min_percent: dec!(33),
                    remarks: "".into(),
                },
            ],
            pass_percent: dec!(33),
        }
        .normalized()
        .unwrap();
        assert_eq!(custom.bands[0].grade, "Pass");
        assert_eq!(custom.grade_for(dec!(33)).unwrap().grade, "Pass");
        assert_eq!(custom.grade_for(dec!(32.99)).unwrap().grade, "Fail");

        let bad = |bands: Vec<(&str, Decimal)>, pass: Decimal| {
            GradingScheme {
                bands: bands
                    .into_iter()
                    .map(|(g, m)| GradeBand {
                        grade: g.into(),
                        min_percent: m,
                        remarks: String::new(),
                    })
                    .collect(),
                pass_percent: pass,
            }
            .normalized()
            .unwrap_err()
        };
        assert_eq!(bad(vec![], dec!(40)), GradingError::Empty);
        assert_eq!(
            bad(vec![("A", dec!(50)), ("A", dec!(0))], dec!(40)),
            GradingError::Names
        );
        assert_eq!(
            bad(vec![("A", dec!(50)), ("B", dec!(50))], dec!(40)),
            GradingError::Bounds
        );
        assert_eq!(
            bad(vec![("A", dec!(101)), ("B", dec!(0))], dec!(40)),
            GradingError::Bounds
        );
        assert_eq!(
            bad(vec![("A", dec!(50)), ("B", dec!(10))], dec!(40)),
            GradingError::NoFloor
        );
        assert_eq!(bad(vec![("A", dec!(0))], dec!(140)), GradingError::Pass);
    }

    #[test]
    fn totals_treat_absent_as_zero() {
        let t = totals([s(100, Some("72.5")), s(50, None), s(75, Some("60"))]);
        assert_eq!(t.max, dec!(225));
        assert_eq!(t.obtained, dec!(132.5));
        assert_eq!(t.percent, dec!(58.89)); // 132.5 / 225 = 58.888…
        assert_eq!(totals([]).percent, dec!(0));
    }

    #[test]
    fn weighted_percent_uses_only_assessments_that_have_marks() {
        // Test 1 (15): 18/20 = 90%; Mid-year (30): 45/50 = 90%; Final (40): 60/100 = 60%.
        let parts = [
            (dec!(15), s(20, Some("18"))),
            (dec!(30), s(50, Some("45"))),
            (dec!(40), s(100, Some("60"))),
        ];
        // (15×90 + 30×90 + 40×60) / 85 = (1350 + 2700 + 2400) / 85 = 6450 / 85 = 75.882…
        assert_eq!(weighted_percent(&parts), Some(dec!(75.88)));
        // With all four assessments and the student absent in Test 2 (counts as 0):
        let all = [
            (dec!(15), s(20, Some("18"))),
            (dec!(15), s(20, None)),
            (dec!(30), s(50, Some("45"))),
            (dec!(40), s(100, Some("60"))),
        ];
        // (1350 + 0 + 2700 + 2400) / 100 = 64.5
        assert_eq!(weighted_percent(&all), Some(dec!(64.5)));
        assert_eq!(weighted_percent(&[]), None);
        assert_eq!(weighted_percent(&[(dec!(0), s(20, Some("18")))]), None);
        // Rounding of the weighted value itself: 1/3 weights → repeating decimals.
        let thirds = [
            (dec!(33.33), s(100, Some("50"))),
            (dec!(33.33), s(100, Some("50"))),
            (dec!(33.34), s(100, Some("51"))),
        ];
        // (1666.5 + 1666.5 + 1700.34) / 100 = 50.3334 → 50.33
        assert_eq!(weighted_percent(&thirds), Some(dec!(50.33)));
    }

    #[test]
    fn positions_use_competition_ranking() {
        let p = positions(&[dec!(80), dec!(95.5), dec!(80), dec!(60), dec!(95.5)]);
        assert_eq!(p, [3, 1, 3, 5, 1]);
        assert_eq!(positions(&[]), Vec::<u32>::new());
        assert_eq!(positions(&[dec!(0)]), [1]);
    }

    #[test]
    fn marks_are_validated_against_the_maximum() {
        assert_eq!(validate_mark(dec!(0), dec!(50)), Ok(dec!(0)));
        assert_eq!(validate_mark(dec!(50), dec!(50)), Ok(dec!(50)));
        assert_eq!(validate_mark(dec!(37.5), dec!(50)), Ok(dec!(37.5)));
        assert!(validate_mark(dec!(50.01), dec!(50)).is_err());
        assert!(validate_mark(dec!(-1), dec!(50)).is_err());
        assert!(validate_mark(dec!(12.345), dec!(50)).is_err());
    }

    #[test]
    fn sheet_status_round_trips() {
        for st in [
            SheetStatus::Draft,
            SheetStatus::Submitted,
            SheetStatus::Approved,
        ] {
            assert_eq!(st.as_str().parse::<SheetStatus>().unwrap(), st);
        }
        assert!("locked".parse::<SheetStatus>().is_err());
        assert_eq!(
            serde_json::to_value(SheetStatus::Approved).unwrap(),
            "approved"
        );
    }
}
