//! Admissions pipeline rules (Standard stage 3): the stages an application moves through,
//! which moves are allowed, and the funnel counts. Pure; the database layer applies them.

use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// What a stage means for the pipeline. Schools name and order their stages; the kind
/// decides the rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageKind {
    /// Still in the pipeline (enquiry, applied, test, interview, offered…).
    Open,
    /// The applicant was admitted; the application can be converted into a student.
    Admitted,
    Rejected,
    Withdrawn,
}

impl StageKind {
    pub const ALL: &'static [StageKind] = &[
        StageKind::Open,
        StageKind::Admitted,
        StageKind::Rejected,
        StageKind::Withdrawn,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            StageKind::Open => "open",
            StageKind::Admitted => "admitted",
            StageKind::Rejected => "rejected",
            StageKind::Withdrawn => "withdrawn",
        }
    }

    pub fn is_terminal(self) -> bool {
        self != StageKind::Open
    }
}

impl FromStr for StageKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        StageKind::ALL
            .iter()
            .copied()
            .find(|k| k.as_str() == s)
            .ok_or_else(|| format!("unknown stage kind: {s}"))
    }
}

/// A stage as created for a new school: code, name, kind, in pipeline order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefaultStage {
    pub code: &'static str,
    pub name: &'static str,
    pub kind: StageKind,
}

/// The pipeline every school starts with; stages can be renamed, reordered, added or
/// removed afterwards.
pub const DEFAULT_STAGES: &[DefaultStage] = &[
    DefaultStage {
        code: "enquiry",
        name: "Enquiry",
        kind: StageKind::Open,
    },
    DefaultStage {
        code: "applied",
        name: "Applied",
        kind: StageKind::Open,
    },
    DefaultStage {
        code: "test_scheduled",
        name: "Test scheduled",
        kind: StageKind::Open,
    },
    DefaultStage {
        code: "interviewed",
        name: "Interviewed",
        kind: StageKind::Open,
    },
    DefaultStage {
        code: "offered",
        name: "Offered",
        kind: StageKind::Open,
    },
    DefaultStage {
        code: "admitted",
        name: "Admitted",
        kind: StageKind::Admitted,
    },
    DefaultStage {
        code: "rejected",
        name: "Rejected",
        kind: StageKind::Rejected,
    },
    DefaultStage {
        code: "withdrawn",
        name: "Withdrawn",
        kind: StageKind::Withdrawn,
    },
];

/// Whether an application may move between two stage kinds. `converted` = it already
/// became a student (then it is frozen).
pub fn can_move(from: StageKind, to: StageKind, converted: bool) -> Result<(), String> {
    if converted {
        return Err(
            "This application was converted into a student and cannot change stage.".into(),
        );
    }
    match (from, to) {
        (StageKind::Admitted, StageKind::Admitted) => Ok(()),
        (StageKind::Admitted, _) => {
            Err("An admitted application stays admitted; withdraw the student instead.".into())
        }
        _ => Ok(()),
    }
}

/// One stage's count in the funnel: applications that reached it (moved to it at least once
/// or sit in it now) and applications in it now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FunnelRow {
    pub stage_id: String,
    pub reached: usize,
    pub current: usize,
}

/// Funnel from the pipeline order and, per application, the stages it visited in order
/// (its current stage last).
pub fn funnel(stage_ids: &[String], visits: &[Vec<String>]) -> Vec<FunnelRow> {
    stage_ids
        .iter()
        .map(|id| FunnelRow {
            stage_id: id.clone(),
            reached: visits.iter().filter(|v| v.contains(id)).count(),
            current: visits.iter().filter(|v| v.last() == Some(id)).count(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_pipeline_is_ordered_and_ends_in_terminal_stages() {
        assert_eq!(DEFAULT_STAGES[0].code, "enquiry");
        assert_eq!(
            DEFAULT_STAGES
                .iter()
                .filter(|s| s.kind == StageKind::Open)
                .count(),
            5
        );
        assert!(DEFAULT_STAGES.iter().any(|s| s.kind == StageKind::Admitted));
        let codes: std::collections::HashSet<_> = DEFAULT_STAGES.iter().map(|s| s.code).collect();
        assert_eq!(codes.len(), DEFAULT_STAGES.len());
        assert_eq!(
            "admitted".parse::<StageKind>().unwrap(),
            StageKind::Admitted
        );
        assert!(StageKind::Rejected.is_terminal() && !StageKind::Open.is_terminal());
    }

    #[test]
    fn moves_follow_the_rules() {
        use StageKind::*;
        assert!(can_move(Open, Open, false).is_ok());
        assert!(can_move(Open, Admitted, false).is_ok());
        assert!(
            can_move(Rejected, Open, false).is_ok(),
            "a rejection can be reopened"
        );
        assert!(can_move(Withdrawn, Admitted, false).is_ok());
        assert!(
            can_move(Admitted, Open, false).is_err(),
            "admitted is final"
        );
        assert!(can_move(Admitted, Admitted, false).is_ok());
        assert!(
            can_move(Open, Open, true).is_err(),
            "converted applications are frozen"
        );
    }

    #[test]
    fn funnel_counts_reached_and_current() {
        let stages = vec!["e".to_string(), "a".to_string(), "x".to_string()];
        let visits = vec![
            vec!["e".to_string()],
            vec!["e".to_string(), "a".to_string()],
            vec!["e".to_string(), "a".to_string(), "x".to_string()],
            vec!["a".to_string()],
        ];
        let f = funnel(&stages, &visits);
        assert_eq!((f[0].reached, f[0].current), (3, 1));
        assert_eq!((f[1].reached, f[1].current), (3, 2));
        assert_eq!((f[2].reached, f[2].current), (1, 1));
    }
}
