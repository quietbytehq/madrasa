//! Consent manager (M2 stage 8): which consents a student needs and what the current state
//! of each one is, from the school-defined consent types and the recorded history.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Who gives this kind of consent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GivenBy {
    Guardian,
    Student,
    Either,
}

/// For whom the consent is required.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Required {
    /// Optional: recorded when offered, never reported as missing.
    None,
    /// Every active student.
    All,
    /// Students whose class level is within `min_level..=max_level`.
    ClassRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Renewal {
    /// Given once, valid until withdrawn.
    None,
    /// Must be given again each academic year.
    Yearly,
}

/// A recorded status (a new row per change; the history is kept).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Given,
    Refused,
    Withdrawn,
}

impl Status {
    pub fn parse(s: &str) -> Option<Status> {
        match s {
            "given" => Some(Status::Given),
            "refused" => Some(Status::Refused),
            "withdrawn" => Some(Status::Withdrawn),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Given => "given",
            Status::Refused => "refused",
            Status::Withdrawn => "withdrawn",
        }
    }
}

/// The rule part of a consent type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    pub required: Required,
    pub min_level: Option<i32>,
    pub max_level: Option<i32>,
    pub renewal: Renewal,
}

impl Rule {
    /// Whether a student in a class of this level needs this consent. A student with no
    /// enrolment (`None`) needs only the consents required for everyone.
    pub fn is_required_for(&self, class_level: Option<i32>) -> bool {
        match self.required {
            Required::None => false,
            Required::All => true,
            Required::ClassRange => match class_level {
                None => false,
                Some(level) => {
                    self.min_level.is_none_or(|m| level >= m)
                        && self.max_level.is_none_or(|m| level <= m)
                }
            },
        }
    }
}

/// The latest recorded row for one student and type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Latest {
    pub status: Status,
    pub academic_year_id: Option<Uuid>,
    pub recorded_on: NaiveDate,
}

/// What the latest record means today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Current {
    Given,
    Refused,
    Withdrawn,
    /// Given (or decided) in an earlier year under yearly renewal: must be asked again.
    Expired,
    /// Never recorded.
    Missing,
}

impl Current {
    /// Counts as missing on the reports: nothing usable is on file.
    pub fn needs_collecting(self) -> bool {
        matches!(
            self,
            Current::Missing | Current::Expired | Current::Withdrawn
        )
    }
}

/// The current state of a consent from its latest record and the type's renewal rule.
pub fn current(rule: &Rule, latest: Option<Latest>, academic_year_id: Uuid) -> Current {
    let Some(l) = latest else {
        return Current::Missing;
    };
    if rule.renewal == Renewal::Yearly && l.academic_year_id != Some(academic_year_id) {
        return Current::Expired;
    }
    match l.status {
        Status::Given => Current::Given,
        Status::Refused => Current::Refused,
        Status::Withdrawn => Current::Withdrawn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(required: Required, min: Option<i32>, max: Option<i32>, renewal: Renewal) -> Rule {
        Rule {
            required,
            min_level: min,
            max_level: max,
            renewal,
        }
    }

    #[test]
    fn required_for_everyone_or_a_class_range() {
        let all = rule(Required::All, None, None, Renewal::None);
        assert!(all.is_required_for(None));
        assert!(all.is_required_for(Some(3)));
        let seniors = rule(Required::ClassRange, Some(9), Some(12), Renewal::None);
        assert!(!seniors.is_required_for(Some(8)));
        assert!(seniors.is_required_for(Some(9)));
        assert!(seniors.is_required_for(Some(12)));
        assert!(!seniors.is_required_for(Some(13)));
        assert!(!seniors.is_required_for(None));
        let open_ended = rule(Required::ClassRange, Some(6), None, Renewal::None);
        assert!(open_ended.is_required_for(Some(12)));
        assert!(!rule(Required::None, None, None, Renewal::None).is_required_for(Some(1)));
    }

    #[test]
    fn yearly_consents_expire_with_the_year() {
        let y1 = Uuid::from_u128(1);
        let y2 = Uuid::from_u128(2);
        let date = NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
        let once = rule(Required::All, None, None, Renewal::None);
        let yearly = rule(Required::All, None, None, Renewal::Yearly);
        let given_y1 = Some(Latest {
            status: Status::Given,
            academic_year_id: Some(y1),
            recorded_on: date,
        });
        assert_eq!(current(&once, None, y2), Current::Missing);
        assert_eq!(current(&once, given_y1, y2), Current::Given);
        assert_eq!(current(&yearly, given_y1, y1), Current::Given);
        assert_eq!(current(&yearly, given_y1, y2), Current::Expired);
        let refused = Some(Latest {
            status: Status::Refused,
            academic_year_id: Some(y2),
            recorded_on: date,
        });
        assert_eq!(current(&yearly, refused, y2), Current::Refused);
        let withdrawn = Some(Latest {
            status: Status::Withdrawn,
            academic_year_id: Some(y1),
            recorded_on: date,
        });
        assert_eq!(current(&once, withdrawn, y2), Current::Withdrawn);
        assert!(Current::Withdrawn.needs_collecting());
        assert!(Current::Expired.needs_collecting());
        assert!(!Current::Refused.needs_collecting());
        assert_eq!(Status::parse("given"), Some(Status::Given));
        assert_eq!(Status::parse("maybe"), None);
    }
}
