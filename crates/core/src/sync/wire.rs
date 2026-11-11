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

// ---- the school ↔ hub protocol ------------------------------------------------------------
//
// The school always calls; the hub never does. Every call after joining carries the school's
// sync token as a bearer token.

/// `POST /join`: a school joins with a one-time code and its own `schools` record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinRequest {
    pub join_code: String,
    /// The school's `schools` record as a whole-record change.
    pub school: Change,
    /// The school's Madrasa version (`0.3.1`) and schema version (latest migration).
    pub app_version: String,
    pub schema_version: i64,
    pub edition: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinResponse {
    /// The hub's id for this membership.
    pub school_ref: Uuid,
    /// The sync token. Shown to the school once; the hub keeps only its hash.
    pub token: String,
    pub hub_name: String,
    pub hub_version: String,
    pub hub_schema_version: i64,
}

/// `POST /sync/push`: changes made at the school since the last sync.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PushRequest {
    pub app_version: String,
    pub schema_version: i64,
    pub changes: Vec<Change>,
    /// Overwrite-log entries the school detected and the hub has not confirmed yet.
    #[serde(default)]
    pub overwrites: Vec<Overwrite>,
    /// Hub entries the school has stored since the last call.
    #[serde(default)]
    pub stored_overwrites: Vec<Uuid>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PushResponse {
    /// Sequence numbers the hub has dealt with; the school forgets those changes.
    pub handled: Vec<i64>,
    pub applied: u32,
    /// Records waiting at the hub.
    pub parked: Vec<Parked>,
    /// Hub log entries the school has not confirmed yet.
    pub overwrites: Vec<Overwrite>,
    /// School entries the hub has now stored.
    pub stored_overwrites: Vec<Uuid>,
    pub max_hlc: Option<String>,
}

/// `POST /sync/pull`: ask for head office's changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    pub limit: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PullResponse {
    pub changes: Vec<Change>,
    /// More is waiting: ask again after acknowledging these.
    pub more: bool,
}

/// `POST /sync/ack`: the school has dealt with a pulled batch.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AckRequest {
    pub handled: Vec<i64>,
    #[serde(default)]
    pub overwrites: Vec<Overwrite>,
    #[serde(default)]
    pub stored_overwrites: Vec<Uuid>,
    /// Records waiting at the school after this batch, for the hub's status screen.
    #[serde(default)]
    pub parked: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AckResponse {
    pub stored_overwrites: Vec<Uuid>,
}

/// `POST /sync/leave`: the school leaves the network.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LeaveRequest {
    #[serde(default)]
    pub reason: String,
}
