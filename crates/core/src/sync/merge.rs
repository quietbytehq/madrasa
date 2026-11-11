//! Merging an incoming change with the local record. Pure decisions; `madrasa_db::sync`
//! carries them out.
//!
//! There are exactly two copies of a school's data: the school's and the hub's. "Changed on
//! both sides since the last sync" therefore has an exact meaning: the incoming change
//! touches a field that this side has changed and not yet sent. It does not depend on
//! either machine's clock. The stamps order replays and decide attendance, where the later
//! mark stands.

use serde_json::{Map, Value};
use std::collections::BTreeMap;
use uuid::Uuid;

use super::policy::{Mode, TablePolicy, META_FIELDS};
use super::wire::{Change, Origin, Overwrite};

/// What to do with one incoming field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldDecision {
    /// Take the incoming value.
    Apply,
    /// Take the incoming value and log the local one it replaces.
    ApplyLogged,
    /// Keep the local value and log the incoming one that was refused.
    KeepLogged,
    /// Nothing to do: the values are equal, or the change is a replay or out of date.
    Ignore,
}

/// The facts one decision needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldFacts {
    /// The incoming value equals the local one.
    pub same_value: bool,
    /// This side changed the field and has not sent it yet.
    pub local_dirty: bool,
    /// This side already received this (or a later) version of the field from the other.
    pub replay: bool,
    /// The incoming record's stamp is later than the local record's.
    pub incoming_newer: bool,
}

/// The merge rule for one field of one record.
pub fn decide_field(mode: Mode, local_side: Origin, f: FieldFacts) -> FieldDecision {
    if f.same_value || f.replay {
        return FieldDecision::Ignore;
    }
    match mode {
        // Existing append-only records never change; skipped tables never arrive.
        Mode::Append | Mode::Skip => FieldDecision::Ignore,
        Mode::Fields => match (f.local_dirty, local_side) {
            (false, _) => FieldDecision::Apply,
            // Both sides changed it: head office wins, whichever side we are.
            (true, Origin::HeadOffice) => FieldDecision::KeepLogged,
            (true, Origin::School) => FieldDecision::ApplyLogged,
        },
        // Attendance: the later mark stands; head office has no priority.
        Mode::Attendance => match (f.incoming_newer, f.local_dirty) {
            (true, false) => FieldDecision::Apply,
            (true, true) => FieldDecision::ApplyLogged,
            (false, true) => FieldDecision::KeepLogged,
            (false, false) => FieldDecision::Ignore,
        },
    }
}

/// What this side remembers about one field of a record (`sync_field_stamps`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalStamp {
    /// Stamp of the field's current value.
    pub hlc: String,
    /// Which side wrote the current value.
    pub origin: Origin,
    /// Stamp of the latest version received from the other side, if any.
    pub remote_hlc: Option<String>,
}

/// The local record an incoming change meets.
#[derive(Debug, Clone, Copy)]
pub struct LocalRow<'a> {
    /// The record as JSON (column → value).
    pub row: &'a Map<String, Value>,
    /// The whole record is waiting to be sent (it is new on this side).
    pub whole_dirty: bool,
    /// Fields changed here and not yet sent.
    pub dirty_fields: &'a [String],
    pub stamps: &'a BTreeMap<String, LocalStamp>,
}

impl LocalRow<'_> {
    fn is_dirty(&self, field: &str) -> bool {
        self.whole_dirty || self.dirty_fields.iter().any(|f| f == field)
    }

    fn hlc(&self) -> &str {
        self.row.get("hlc").and_then(Value::as_str).unwrap_or("")
    }
}

/// How to apply one incoming change to an existing record.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RowPlan {
    /// Fields to copy from the incoming record.
    pub set: Vec<String>,
    /// Also take `updated_at`, `updated_by` and `hlc` from the incoming record.
    pub take_meta: bool,
    pub overwrites: Vec<Overwrite>,
    /// (field, stamp): the field now holds the other side's value with this stamp.
    pub applied_stamps: Vec<(String, String)>,
    /// (field, stamp): the other side's version was seen but not taken (equal or refused);
    /// remembered so a replay of it is recognised.
    pub seen_stamps: Vec<(String, String)>,
    /// Fields whose pending local change lost and must not be sent any more.
    pub clear_dirty: Vec<String>,
}

impl RowPlan {
    /// Does the plan change the record at all?
    pub fn changes_row(&self) -> bool {
        !self.set.is_empty() || self.take_meta
    }
}

fn uuid_field(row: &Map<String, Value>, key: &str) -> Option<Uuid> {
    row.get(key)?.as_str()?.parse().ok()
}

/// Plan the merge of `change` into an existing local record.
pub fn plan_update(
    policy: &TablePolicy,
    local_side: Origin,
    local: &LocalRow<'_>,
    change: &Change,
) -> RowPlan {
    let mut plan = RowPlan::default();
    if matches!(policy.mode, Mode::Append | Mode::Skip) {
        return plan;
    }
    let partial = change.fields.is_some();
    let candidates: Vec<&String> = match &change.fields {
        Some(list) => list
            .iter()
            .filter(|f| change.row.contains_key(*f))
            .collect(),
        None => change.row.keys().collect(),
    };
    let incoming_newer = change.hlc.as_str() > local.hlc();
    // Did this side keep one of its own values against the incoming record?
    let mut kept_ours = false;
    for field in candidates {
        let name = field.as_str();
        if name == "id"
            || META_FIELDS.contains(&name)
            || policy.is_excluded(name)
            || policy.is_derived(name)
        {
            continue;
        }
        // A column this version does not have (the sender is newer): nothing to store it in.
        let Some(local_value) = local.row.get(name) else {
            continue;
        };
        let incoming_value = &change.row[name];
        let stamp = change.stamp_of(name);
        let known = local.stamps.get(name);
        let replay = partial
            && known
                .and_then(|s| s.remote_hlc.as_deref())
                .is_some_and(|seen| stamp <= seen);
        let decision = decide_field(
            policy.mode,
            local_side,
            FieldFacts {
                same_value: local_value == incoming_value,
                local_dirty: local.is_dirty(name),
                replay,
                incoming_newer,
            },
        );
        let reason = if policy.mode == Mode::Attendance {
            "later_mark"
        } else {
            "head_office_wins"
        };
        let local_stamp = known.map(|s| s.hlc.as_str()).unwrap_or(local.hlc());
        let logs = policy.logs(name, local_value, incoming_value);
        match decision {
            FieldDecision::Apply => {
                plan.set.push(name.to_string());
                if partial {
                    plan.applied_stamps
                        .push((name.to_string(), stamp.to_string()));
                }
            }
            FieldDecision::ApplyLogged => {
                plan.set.push(name.to_string());
                plan.clear_dirty.push(name.to_string());
                if partial {
                    plan.applied_stamps
                        .push((name.to_string(), stamp.to_string()));
                }
                if !logs {
                    continue;
                }
                plan.overwrites.push(Overwrite {
                    uid: None,
                    at: None,
                    table: change.table.clone(),
                    id: change.id,
                    field: name.to_string(),
                    kept: incoming_value.clone(),
                    discarded: local_value.clone(),
                    kept_origin: local_side.other(),
                    kept_hlc: stamp.to_string(),
                    discarded_hlc: local_stamp.to_string(),
                    discarded_by: uuid_field(local.row, "updated_by"),
                    reason: reason.to_string(),
                });
            }
            FieldDecision::KeepLogged => {
                if partial {
                    plan.seen_stamps.push((name.to_string(), stamp.to_string()));
                }
                kept_ours = true;
                if !logs {
                    continue;
                }
                plan.overwrites.push(Overwrite {
                    uid: None,
                    at: None,
                    table: change.table.clone(),
                    id: change.id,
                    field: name.to_string(),
                    kept: local_value.clone(),
                    discarded: incoming_value.clone(),
                    kept_origin: local_side,
                    kept_hlc: local_stamp.to_string(),
                    discarded_hlc: stamp.to_string(),
                    discarded_by: uuid_field(&change.row, "updated_by"),
                    reason: reason.to_string(),
                });
            }
            FieldDecision::Ignore => {
                if partial && !replay {
                    plan.seen_stamps.push((name.to_string(), stamp.to_string()));
                }
            }
        }
    }
    // The bookkeeping columns follow the later record, but only when something of it was
    // taken or nothing of ours was kept against it.
    plan.take_meta = incoming_newer && (!plan.set.is_empty() || !kept_ours);
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::policy::policy;
    use serde_json::json;

    const T1: &str = "1700000000000-00000-000000000aaa";
    const T2: &str = "1700000000500-00000-000000000bbb";
    const T3: &str = "1700000000900-00000-000000000aaa";

    fn facts(
        same_value: bool,
        local_dirty: bool,
        replay: bool,
        incoming_newer: bool,
    ) -> FieldFacts {
        FieldFacts {
            same_value,
            local_dirty,
            replay,
            incoming_newer,
        }
    }

    #[test]
    fn field_rule_head_office_wins_only_when_both_changed() {
        use FieldDecision::*;
        use Origin::*;
        // One side changed it: apply, whoever we are.
        assert_eq!(
            decide_field(Mode::Fields, School, facts(false, false, false, true)),
            Apply
        );
        assert_eq!(
            decide_field(Mode::Fields, HeadOffice, facts(false, false, false, false)),
            Apply
        );
        // Both changed it: head office wins on both sides, whatever the clocks say.
        for newer in [true, false] {
            assert_eq!(
                decide_field(Mode::Fields, HeadOffice, facts(false, true, false, newer)),
                KeepLogged
            );
            assert_eq!(
                decide_field(Mode::Fields, School, facts(false, true, false, newer)),
                ApplyLogged
            );
        }
        // Equal values and replays never change or log anything.
        assert_eq!(
            decide_field(Mode::Fields, School, facts(true, true, false, true)),
            Ignore
        );
        assert_eq!(
            decide_field(Mode::Fields, HeadOffice, facts(false, true, true, true)),
            Ignore
        );
    }

    #[test]
    fn attendance_rule_the_later_mark_stands_on_either_side() {
        use FieldDecision::*;
        for side in [Origin::School, Origin::HeadOffice] {
            assert_eq!(
                decide_field(Mode::Attendance, side, facts(false, false, false, true)),
                Apply
            );
            assert_eq!(
                decide_field(Mode::Attendance, side, facts(false, true, false, true)),
                ApplyLogged
            );
            assert_eq!(
                decide_field(Mode::Attendance, side, facts(false, true, false, false)),
                KeepLogged
            );
            assert_eq!(
                decide_field(Mode::Attendance, side, facts(false, false, false, false)),
                Ignore
            );
        }
    }

    #[test]
    fn append_only_records_never_change() {
        for side in [Origin::School, Origin::HeadOffice] {
            assert_eq!(
                decide_field(Mode::Append, side, facts(false, true, false, true)),
                FieldDecision::Ignore
            );
        }
    }

    fn obj(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    fn student(name: &str, father: &str, hlc: &str) -> Map<String, Value> {
        obj(json!({
            "id": "00000000-0000-7000-8000-000000000001", "full_name": name, "father_name": father,
            "updated_at": "2026-11-01T10:00:00Z", "updated_by": "00000000-0000-7000-8000-0000000000aa", "hlc": hlc
        }))
    }

    fn change(fields: Option<&[&str]>, row: Value, hlc: &str) -> Change {
        Change {
            table: "students".into(),
            id: "00000000-0000-7000-8000-000000000001".parse().unwrap(),
            fields: fields.map(|f| f.iter().map(|s| s.to_string()).collect()),
            stamps: fields
                .map(|f| f.iter().map(|s| (s.to_string(), hlc.to_string())).collect())
                .unwrap_or_default(),
            row: obj(row),
            hlc: hlc.into(),
            seq: 1,
        }
    }

    fn plan(
        side: Origin,
        local: &Map<String, Value>,
        dirty: &[&str],
        stamps: &BTreeMap<String, LocalStamp>,
        c: &Change,
    ) -> RowPlan {
        let dirty: Vec<String> = dirty.iter().map(|s| s.to_string()).collect();
        plan_update(
            policy(&c.table).unwrap(),
            side,
            &LocalRow {
                row: local,
                whole_dirty: false,
                dirty_fields: &dirty,
                stamps,
            },
            c,
        )
    }

    #[test]
    fn different_fields_of_one_record_both_survive() {
        // Here: father_name changed and not sent. Incoming: full_name changed.
        let local = student("Ali Khan", "Imran K.", T1);
        let c = change(
            Some(&["full_name"]),
            json!({ "full_name": "Ali Ahmed Khan", "hlc": T2, "updated_at": "x", "updated_by": null }),
            T2,
        );
        for side in [Origin::School, Origin::HeadOffice] {
            let p = plan(side, &local, &["father_name"], &BTreeMap::new(), &c);
            assert_eq!(p.set, ["full_name"]);
            assert!(p.overwrites.is_empty());
            assert!(
                p.clear_dirty.is_empty(),
                "our father_name is still to be sent"
            );
            assert_eq!(
                p.applied_stamps,
                [("full_name".to_string(), T2.to_string())]
            );
            assert!(p.take_meta);
        }
    }

    #[test]
    fn same_field_on_both_sides_head_office_wins_and_the_loser_is_logged() {
        let c = change(
            Some(&["full_name"]),
            json!({ "full_name": "Incoming Name", "hlc": T2, "updated_by": "00000000-0000-7000-8000-0000000000bb" }),
            T2,
        );
        let local = student("Local Name", "Imran", T3);

        // At the hub the school's value is refused, even though the hub's edit is older.
        let p = plan(
            Origin::HeadOffice,
            &local,
            &["full_name"],
            &BTreeMap::new(),
            &c,
        );
        assert!(p.set.is_empty());
        assert!(!p.take_meta, "nothing of the incoming record was taken");
        assert_eq!(p.overwrites.len(), 1);
        let o = &p.overwrites[0];
        assert_eq!(
            (o.kept.clone(), o.discarded.clone()),
            (json!("Local Name"), json!("Incoming Name"))
        );
        assert_eq!(o.kept_origin, Origin::HeadOffice);
        assert_eq!(o.reason, "head_office_wins");
        assert_eq!(
            o.discarded_by,
            "00000000-0000-7000-8000-0000000000bb".parse().ok()
        );
        assert_eq!(p.seen_stamps, [("full_name".to_string(), T2.to_string())]);

        // At the school the head office value replaces the local one, even though ours is newer.
        let p = plan(Origin::School, &local, &["full_name"], &BTreeMap::new(), &c);
        assert_eq!(p.set, ["full_name"]);
        assert_eq!(p.clear_dirty, ["full_name"], "the losing edit is not sent");
        let o = &p.overwrites[0];
        assert_eq!(
            (o.kept.clone(), o.discarded.clone()),
            (json!("Incoming Name"), json!("Local Name"))
        );
        assert_eq!(o.kept_origin, Origin::HeadOffice);
        assert_eq!(
            o.discarded_by,
            "00000000-0000-7000-8000-0000000000aa".parse().ok()
        );
        assert_eq!(o.discarded_hlc, T3);
    }

    #[test]
    fn a_replayed_change_is_ignored_even_after_a_local_edit() {
        // We applied the other side's T2 version, then edited the field here.
        let local = student("Edited Here Later", "Imran", T3);
        let stamps = BTreeMap::from([(
            "full_name".to_string(),
            LocalStamp {
                hlc: T3.into(),
                origin: Origin::School,
                remote_hlc: Some(T2.into()),
            },
        )]);
        let c = change(
            Some(&["full_name"]),
            json!({ "full_name": "Old Remote Value", "hlc": T2 }),
            T2,
        );
        let p = plan(Origin::School, &local, &["full_name"], &stamps, &c);
        assert_eq!(p, RowPlan::default(), "nothing changes, nothing is logged");
        // A genuinely newer version from the other side is not a replay.
        let c = change(
            Some(&["full_name"]),
            json!({ "full_name": "New Remote Value", "hlc": T3 }),
            "1700000001000-00000-000000000bbb",
        );
        let p = plan(Origin::School, &local, &["full_name"], &stamps, &c);
        assert_eq!(p.set, ["full_name"]);
        assert_eq!(p.overwrites.len(), 1);
    }

    #[test]
    fn equal_values_change_nothing_but_are_remembered() {
        let local = student("Ali Khan", "Imran", T1);
        let c = change(
            Some(&["full_name"]),
            json!({ "full_name": "Ali Khan", "hlc": T2 }),
            T2,
        );
        let p = plan(
            Origin::HeadOffice,
            &local,
            &["full_name"],
            &BTreeMap::new(),
            &c,
        );
        assert!(p.set.is_empty() && p.overwrites.is_empty());
        assert_eq!(p.seen_stamps, [("full_name".to_string(), T2.to_string())]);
        assert!(p.take_meta, "the newer bookkeeping columns are still taken");
    }

    #[test]
    fn bookkeeping_excluded_and_unknown_columns_are_never_copied() {
        let mut local = obj(
            json!({ "id": "00000000-0000-7000-8000-000000000001", "username": "ali", "full_name": "Ali",
            "password_hash": "$argon2id$real", "failed_login_count": 2, "hlc": T1, "updated_at": "a", "updated_by": null }),
        );
        let c = Change {
            table: "users".into(),
            id: "00000000-0000-7000-8000-000000000001".parse().unwrap(),
            fields: None,
            row: obj(
                json!({ "id": "00000000-0000-7000-8000-000000000001", "username": "ali", "full_name": "Ali Raza",
                "password_hash": "!", "failed_login_count": 0, "brand_new_column": 1, "hlc": T2, "updated_at": "b", "updated_by": null }),
            ),
            stamps: BTreeMap::new(),
            hlc: T2.into(),
            seq: 1,
        };
        let p = plan_update(
            policy("users").unwrap(),
            Origin::School,
            &LocalRow {
                row: &local,
                whole_dirty: false,
                dirty_fields: &[],
                stamps: &BTreeMap::new(),
            },
            &c,
        );
        assert_eq!(p.set, ["full_name"]);
        assert!(
            p.applied_stamps.is_empty() && p.seen_stamps.is_empty(),
            "whole records leave no field stamps"
        );
        // An older incoming record does not take the bookkeeping columns.
        local.insert("hlc".into(), json!(T3));
        let p = plan_update(
            policy("users").unwrap(),
            Origin::School,
            &LocalRow {
                row: &local,
                whole_dirty: false,
                dirty_fields: &[],
                stamps: &BTreeMap::new(),
            },
            &c,
        );
        assert_eq!(p.set, ["full_name"]);
        assert!(!p.take_meta);
    }

    #[test]
    fn attendance_the_later_mark_stands_and_the_earlier_is_logged() {
        let local = obj(
            json!({ "id": "00000000-0000-7000-8000-000000000001", "status": "absent", "note": null, "hlc": T2, "updated_by": null }),
        );
        let mk = |status: &str, hlc: &str| Change {
            table: "attendance_entries".into(),
            id: "00000000-0000-7000-8000-000000000001".parse().unwrap(),
            fields: Some(vec!["status".into()]),
            row: obj(json!({ "status": status, "hlc": hlc, "updated_by": null })),
            stamps: BTreeMap::new(),
            hlc: hlc.into(),
            seq: 1,
        };
        let dirty = vec!["status".to_string()];
        for side in [Origin::School, Origin::HeadOffice] {
            let row = LocalRow {
                row: &local,
                whole_dirty: false,
                dirty_fields: &dirty,
                stamps: &BTreeMap::new(),
            };
            // An earlier mark from the other side loses, on either side.
            let p = plan_update(
                policy("attendance_entries").unwrap(),
                side,
                &row,
                &mk("present", T1),
            );
            assert!(p.set.is_empty());
            assert_eq!(p.overwrites[0].reason, "later_mark");
            assert_eq!(p.overwrites[0].kept, json!("absent"));
            // A later mark from the other side wins, on either side.
            let p = plan_update(
                policy("attendance_entries").unwrap(),
                side,
                &row,
                &mk("late", T3),
            );
            assert_eq!(p.set, ["status"]);
            assert_eq!(p.overwrites[0].discarded, json!("absent"));
            assert_eq!(p.overwrites[0].kept, json!("late"));
        }
    }

    #[test]
    fn a_vouchers_payment_status_merges_without_a_log_entry() {
        let local = obj(
            json!({ "id": "00000000-0000-7000-8000-000000000001", "status": "partial", "due_date": "2026-05-10",
            "net_paid": "500.00", "hlc": T2, "updated_by": null }),
        );
        let c = |status: &str| Change {
            table: "fee_vouchers".into(),
            id: "00000000-0000-7000-8000-000000000001".parse().unwrap(),
            fields: Some(vec!["status".into()]),
            row: obj(
                json!({ "status": status, "net_paid": "9999.00", "hlc": T3, "updated_by": null }),
            ),
            stamps: BTreeMap::new(),
            hlc: T3.into(),
            seq: 1,
        };
        let dirty = vec!["status".to_string()];
        let row = LocalRow {
            row: &local,
            whole_dirty: false,
            dirty_fields: &dirty,
            stamps: &BTreeMap::new(),
        };
        let v = policy("fee_vouchers").unwrap();
        // Both sides took a payment: no log; the status is recomputed from the receipts.
        let hub = plan_update(v, Origin::HeadOffice, &row, &c("paid"));
        assert!(hub.set.is_empty() && hub.overwrites.is_empty());
        let school = plan_update(v, Origin::School, &row, &c("paid"));
        assert_eq!(school.set, ["status"]);
        assert!(school.overwrites.is_empty());
        // The paid amount is never copied: each side sums its receipts.
        assert!(!school.set.contains(&"net_paid".to_string()));
        // A cancellation against a payment is a real conflict and is logged.
        let hub = plan_update(v, Origin::HeadOffice, &row, &c("cancelled"));
        assert_eq!(hub.overwrites.len(), 1);
        assert_eq!(hub.overwrites[0].discarded, json!("cancelled"));
    }

    #[test]
    fn existing_receipts_are_never_touched() {
        let local = obj(
            json!({ "id": "00000000-0000-7000-8000-000000000001", "amount": "500.00", "hlc": T1 }),
        );
        let c = Change {
            table: "fee_receipts".into(),
            id: "00000000-0000-7000-8000-000000000001".parse().unwrap(),
            fields: None,
            row: obj(
                json!({ "id": "00000000-0000-7000-8000-000000000001", "amount": "999.00", "hlc": T2 }),
            ),
            stamps: BTreeMap::new(),
            hlc: T2.into(),
            seq: 1,
        };
        let p = plan_update(
            policy("fee_receipts").unwrap(),
            Origin::School,
            &LocalRow {
                row: &local,
                whole_dirty: false,
                dirty_fields: &[],
                stamps: &BTreeMap::new(),
            },
            &c,
        );
        assert_eq!(p, RowPlan::default());
        assert!(!p.changes_row());
    }
}
