//! What travels between a school and the hub.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Which copy of a school's data made a change: the school itself or head office at the hub.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    School,
    HeadOffice,
}

impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Origin::School => "school",
            Origin::HeadOffice => "head_office",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "school" => Some(Origin::School),
            "head_office" => Some(Origin::HeadOffice),
            _ => None,
        }
    }

    /// The other side.
    pub fn other(self) -> Self {
        match self {
            Origin::School => Origin::HeadOffice,
            Origin::HeadOffice => Origin::School,
        }
    }
}

/// One changed record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Change {
    pub table: String,
    pub id: Uuid,
    /// `None`: the whole record (it is new to the other side). Otherwise the fields that
    /// changed since the last sync.
    pub fields: Option<Vec<String>>,
    /// Column → value. The whole record, or the changed fields plus the bookkeeping columns.
    /// Excluded columns (password hashes) are never present.
    pub row: Map<String, Value>,
    /// Field → HLC stamp of its last change, for the changed fields of a field-level table.
    #[serde(default)]
    pub stamps: BTreeMap<String, String>,
    /// The record's own HLC stamp.
    pub hlc: String,
    /// The sender's sequence number of this change; echoed back to acknowledge it.
    pub seq: i64,
}

impl Change {
    /// The stamp of one field: its own when it was changed after the record was created,
    /// otherwise the record's.
    pub fn stamp_of(&self, field: &str) -> &str {
        self.stamps
            .get(field)
            .map(String::as_str)
            .unwrap_or(&self.hlc)
    }
}

/// A value that sync replaced or refused. Written to `sync_overwrites` on both sides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Overwrite {
    /// Identifies the log entry on both sides; set by the side that detected it.
    #[serde(default)]
    pub uid: Option<Uuid>,
    /// When it was detected.
    #[serde(default)]
    pub at: Option<chrono::DateTime<chrono::Utc>>,
    pub table: String,
    pub id: Uuid,
    pub field: String,
    pub kept: Value,
    pub discarded: Value,
    pub kept_origin: Origin,
    pub kept_hlc: String,
    pub discarded_hlc: String,
    /// The user who made the edit that lost, when the record says so.
    pub discarded_by: Option<Uuid>,
    /// `head_office_wins` (same field edited on both sides), `later_mark` (attendance),
    /// `renumbered` or `duplicate_reversal` (receipts).
    pub reason: String,
}

/// An incoming record that could not be applied yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parked {
    pub table: String,
    pub id: Uuid,
    /// Plain-language reason.
    pub reason: String,
}

/// What applying a batch did.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ApplyReport {
    /// Records inserted or changed.
    pub applied: u32,
    /// Records that needed no change (already up to date, or a replay).
    pub unchanged: u32,
    /// Sequence numbers the receiver has dealt with (applied, unchanged or parked); the
    /// sender may forget these changes.
    pub handled: Vec<i64>,
    pub overwrites: Vec<Overwrite>,
    /// Records waiting on the receiver after this batch.
    pub parked: Vec<Parked>,
    /// The latest stamp seen, for the receiver's clock to fold in.
    pub max_hlc: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn origin_names_round_trip() {
        for o in [Origin::School, Origin::HeadOffice] {
            assert_eq!(Origin::parse(o.as_str()), Some(o));
            assert_eq!(o.other().other(), o);
            assert_eq!(serde_json::to_value(o).unwrap(), json!(o.as_str()));
        }
        assert_eq!(Origin::parse("hub"), None);
    }

    #[test]
    fn a_change_serializes_and_reads_back() {
        let c = Change {
            table: "students".into(),
            id: Uuid::nil(),
            fields: Some(vec!["full_name".into()]),
            row: json!({ "full_name": "Ali Khan", "hlc": "1700000000000-00001-000000000abc" })
                .as_object()
                .unwrap()
                .clone(),
            stamps: BTreeMap::from([(
                "full_name".to_string(),
                "1700000000000-00001-000000000abc".to_string(),
            )]),
            hlc: "1700000000000-00001-000000000abc".into(),
            seq: 7,
        };
        let text = serde_json::to_string(&c).unwrap();
        assert_eq!(serde_json::from_str::<Change>(&text).unwrap(), c);
        assert_eq!(c.stamp_of("full_name"), "1700000000000-00001-000000000abc");
        assert_eq!(
            c.stamp_of("father_name"),
            c.hlc,
            "unstamped fields use the record's stamp"
        );
    }
}
