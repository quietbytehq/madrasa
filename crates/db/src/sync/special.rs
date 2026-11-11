//! Receipts and attendance are always merged (ADR 0005, stage 3).
//!
//! The generic rule for a new record is "insert it". These tables add a rule for a new
//! record that meets a *twin*: a different record on this side that stands for the same
//! thing (the same receipt number, a second reversal of one receipt, attendance for the
//! same section, student or staff member on the same day). Nothing is dropped:
//!
//! - **Receipts**: both are kept; the later one is renumbered with a suffix. A receipt
//!   reversed on both sides is reversed once (the earlier reversal stands). Afterwards each
//!   voucher's paid amount is the sum of its receipts.
//! - **Attendance**: the two records fold into one. The earlier record (by id) survives,
//!   the other is soft-deleted, and the survivor carries the later mark. A replaced mark is
//!   written to the overwrite log. Sheet totals are counted again from the entries.
//!
//! Every rule depends only on the two records, never on which side runs it, so the school
//! and the hub reach the same result whichever syncs first. Records a merge rewrites are
//! queued to be sent back, stamped later than both originals, so the other side takes them.

use super::*;

fn text<'a>(row: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    row.get(key).and_then(Value::as_str)
}

fn uuid_of(row: &Map<String, Value>, key: &str) -> Option<Uuid> {
    text(row, key)?.parse().ok()
}

fn is_deleted(row: &Map<String, Value>) -> bool {
    row.get("deleted_at").is_some_and(|v| !v.is_null())
}

/// A different record on this side that stands for the same thing as an incoming one.
struct Twin {
    id: Uuid,
    hlc: String,
    row: Map<String, Value>,
}

fn twin_from(found: Option<(Uuid, String, Value)>) -> Option<Twin> {
    found.and_then(|(id, hlc, row)| match row {
        Value::Object(row) => Some(Twin { id, hlc, row }),
        _ => None,
    })
}

impl Applier<'_> {
    /// Remember what has to be recounted after the batch.
    pub(super) fn touch(&mut self, table: &str, row: &Map<String, Value>) {
        let id = match table {
            "fee_receipts" => uuid_of(row, "voucher_id").map(|v| (true, v)),
            "fee_vouchers" => uuid_of(row, "id").map(|v| (true, v)),
            "attendance_entries" => uuid_of(row, "sheet_id").map(|v| (false, v)),
            _ => None,
        };
        match id {
            Some((true, voucher)) => {
                self.vouchers.insert(voucher);
            }
            Some((false, sheet)) => {
                self.sheets.insert(sheet);
            }
            None => {}
        }
    }

    /// A stamp later than anything this copy has written and later than `others`.
    fn fresh_stamp(&self, others: &[&str]) -> String {
        let wall = Utc::now().timestamp_millis().max(0) as u64;
        for other in others {
            if let Ok(parsed) = other.parse::<HlcStamp>() {
                self.hlc.observe(parsed, wall);
            }
        }
        self.hlc.now().to_string()
    }

    /// Queue a whole record to be sent to the other side.
    async fn queue(
        &self,
        conn: &mut PgConnection,
        table: &str,
        ids: &[Uuid],
        stamp: &str,
    ) -> DbResult<()> {
        sqlx::query(
            "INSERT INTO sync_changes (school_id, table_name, row_id, fields, seq, hlc)
             SELECT $1, $2, i, NULL, nextval('sync_change_seq'), $4 FROM unnest($3::uuid[]) i
             ON CONFLICT (table_name, row_id) DO UPDATE
                 SET fields = NULL, seq = EXCLUDED.seq, hlc = EXCLUDED.hlc, changed_at = now()",
        )
        .bind(self.state.school_id)
        .bind(table)
        .bind(ids)
        .bind(stamp)
        .execute(conn)
        .await?;
        Ok(())
    }

    /// Log what a merge replaced. The same merge seen again (the other side reports it too)
    /// is logged once.
    async fn log_merge(&mut self, conn: &mut PgConnection, entry: Overwrite) -> DbResult<()> {
        let known: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM sync_overwrites
             WHERE table_name = $1 AND row_id = $2 AND field = $3 AND reason = $4
               AND kept_value IS NOT DISTINCT FROM $5 AND discarded_value IS NOT DISTINCT FROM $6)",
        )
        .bind(&entry.table)
        .bind(entry.id)
        .bind(&entry.field)
        .bind(&entry.reason)
        .bind(&entry.kept)
        .bind(&entry.discarded)
        .fetch_one(&mut *conn)
        .await?;
        if !known {
            let logged = log_overwrite(conn, self.state.school_id, &entry, false).await?;
            self.report.overwrites.push(logged);
        }
        Ok(())
    }

    /// May a run of new records go in as one statement? Attendance entries may not when one
    /// of them would meet a twin, or sits on a sheet that was folded into another: no unique
    /// index would stop the insert, but the merge rule has to run.
    pub(super) async fn may_insert_together(
        &mut self,
        conn: &mut PgConnection,
        p: &TablePolicy,
        run: &[Change],
    ) -> DbResult<bool> {
        if p.table != "attendance_entries" {
            return Ok(true);
        }
        let rows: Vec<Value> = run
            .iter()
            .filter(|c| !is_deleted(&c.row))
            .map(|c| {
                serde_json::json!({
                    "student_id": c.row.get("student_id"), "date": c.row.get("date"),
                    "sheet_id": c.row.get("sheet_id"),
                })
            })
            .collect();
        let clear: bool = sqlx::query_scalar(
            "SELECT NOT EXISTS (
                 SELECT 1 FROM jsonb_to_recordset($1) AS e(student_id uuid, date date, sheet_id uuid)
                 JOIN attendance_entries f ON f.student_id = e.student_id AND f.date = e.date
                 WHERE f.deleted_at IS NULL)
             AND NOT EXISTS (
                 SELECT 1 FROM jsonb_to_recordset($1) AS e(student_id uuid, date date, sheet_id uuid)
                 JOIN attendance_sheets s ON s.id = e.sheet_id WHERE s.deleted_at IS NOT NULL)",
        )
        .bind(Value::Array(rows))
        .fetch_one(conn)
        .await?;
        Ok(clear)
    }

    /// Insert a new record of a table with its own merge rule. `None` means the generic
    /// insert applies (no twin).
    pub(super) async fn insert_special(
        &mut self,
        conn: &mut PgConnection,
        p: &'static TablePolicy,
        change: &Change,
    ) -> DbResult<Option<Outcome>> {
        match p.table {
            "fee_receipts" => self.insert_receipt(conn, p, change).await.map(Some),
            _ if is_deleted(&change.row) => Ok(None),
            "staff_attendance" => self.insert_staff_mark(conn, p, change).await,
            "attendance_sheets" => self.insert_sheet(conn, p, change).await,
            "attendance_entries" => self.insert_entry(conn, p, change).await,
            _ => Ok(None),
        }
    }

    // ---- receipts ---------------------------------------------------------------------------

    async fn insert_receipt(
        &mut self,
        conn: &mut PgConnection,
        p: &'static TablePolicy,
        change: &Change,
    ) -> DbResult<Outcome> {
        self.touch(p.table, &change.row);
        let remote = self.state.role.other();
        let number = text(&change.row, "receipt_no")
            .unwrap_or_default()
            .to_string();

        // The same receipt reversed on both sides is reversed once: the earlier reversal
        // stands, the later one is removed and logged in full.
        if let Some(reversed) = uuid_of(&change.row, "reverses_id") {
            let local = twin_from(
                sqlx::query_as(
                    "SELECT id, hlc, sync_row(to_jsonb(t)) FROM fee_receipts t
                     WHERE reverses_id = $1 AND id <> $2",
                )
                .bind(reversed)
                .bind(change.id)
                .fetch_optional(&mut *conn)
                .await?,
            );
            if let Some(local) = local {
                let mut entry = Overwrite {
                    uid: None,
                    at: None,
                    table: p.table.to_string(),
                    id: reversed,
                    field: "reversal".into(),
                    kept: Value::Null,
                    discarded: Value::Null,
                    kept_origin: remote,
                    kept_hlc: change.hlc.clone(),
                    discarded_hlc: local.hlc.clone(),
                    discarded_by: uuid_of(&local.row, "received_by"),
                    reason: "duplicate_reversal".into(),
                };
                if change.id < local.id {
                    entry.kept = Value::String(number.clone());
                    entry.discarded = Value::Object(local.row.clone());
                    sqlx::query("DELETE FROM fee_receipts WHERE id = $1")
                        .bind(local.id)
                        .execute(&mut *conn)
                        .await?;
                    sqlx::query(
                        "DELETE FROM sync_changes WHERE table_name = 'fee_receipts' AND row_id = $1",
                    )
                    .bind(local.id)
                    .execute(&mut *conn)
                    .await?;
                    self.log_merge(conn, entry).await?;
                } else {
                    entry.kept = local.row.get("receipt_no").cloned().unwrap_or(Value::Null);
                    entry.discarded = Value::Object(change.row.clone());
                    entry.kept_origin = self.state.role;
                    entry.kept_hlc = local.hlc.clone();
                    entry.discarded_hlc = change.hlc.clone();
                    entry.discarded_by = uuid_of(&change.row, "received_by");
                    self.log_merge(conn, entry).await?;
                    return Ok(Outcome::Unchanged);
                }
            }
        }

        // Two different receipts with one number: both are kept. The one created later
        // (ids are time-ordered) takes a suffix, whichever side it is on.
        let mut incoming = change.clone();
        let clash: Option<(Uuid, String)> = sqlx::query_as(
            "SELECT id, hlc FROM fee_receipts WHERE school_id = $1 AND receipt_no = $2 AND id <> $3",
        )
        .bind(self.state.school_id)
        .bind(&number)
        .bind(change.id)
        .fetch_optional(&mut *conn)
        .await?;
        if let Some((local_id, local_hlc)) = clash {
            let mut suffix = 2;
            let free = loop {
                let candidate = format!("{number}-{suffix}");
                let taken: bool = sqlx::query_scalar(
                    "SELECT EXISTS (SELECT 1 FROM fee_receipts WHERE school_id = $1 AND receipt_no = $2)",
                )
                .bind(self.state.school_id)
                .bind(&candidate)
                .fetch_one(&mut *conn)
                .await?;
                if !taken {
                    break candidate;
                }
                suffix += 1;
            };
            let incoming_is_later = change.id > local_id;
            let renumbered = if incoming_is_later {
                incoming
                    .row
                    .insert("receipt_no".into(), Value::String(free.clone()));
                change.id
            } else {
                sqlx::query("UPDATE fee_receipts SET receipt_no = $2 WHERE id = $1")
                    .bind(local_id)
                    .bind(&free)
                    .execute(&mut *conn)
                    .await?;
                local_id
            };
            self.log_merge(
                conn,
                Overwrite {
                    uid: None,
                    at: None,
                    table: p.table.to_string(),
                    id: renumbered,
                    field: "receipt_no".into(),
                    kept: Value::String(free),
                    discarded: Value::String(number),
                    kept_origin: if incoming_is_later {
                        self.state.role
                    } else {
                        remote
                    },
                    kept_hlc: if incoming_is_later {
                        local_hlc.clone()
                    } else {
                        change.hlc.clone()
                    },
                    discarded_hlc: if incoming_is_later {
                        change.hlc.clone()
                    } else {
                        local_hlc
                    },
                    discarded_by: None,
                    reason: "renumbered".into(),
                },
            )
            .await?;
        }
        self.insert_many(conn, p, &[&incoming]).await?;
        Ok(Outcome::Applied)
    }

    // ---- attendance -------------------------------------------------------------------------

    /// Fold an incoming record and its twin into one. The record with the earlier id
    /// survives; the other is soft-deleted; the survivor takes the `marks` of whichever was
    /// written later. Both are queued to be sent back. Returns whether the incoming record
    /// is the survivor.
    async fn merge_pair(
        &mut self,
        conn: &mut PgConnection,
        p: &'static TablePolicy,
        change: &Change,
        twin: &Twin,
        marks: &[&str],
        overrides: &[(&str, Value)],
    ) -> DbResult<bool> {
        let table = p.table;
        let stamp = self.fresh_stamp(&[&change.hlc, &twin.hlc]);
        let incoming_survives = change.id < twin.id;
        let incoming_is_later = (change.hlc.as_str(), change.id) > (twin.hlc.as_str(), twin.id);
        let later = if incoming_is_later {
            &change.row
        } else {
            &twin.row
        };

        let mut incoming = change.clone();
        for (key, value) in overrides {
            incoming.row.insert((*key).to_string(), value.clone());
        }
        incoming
            .row
            .insert("hlc".into(), Value::String(stamp.clone()));
        if incoming_survives {
            sqlx::query(sql(format!(
                "UPDATE {table} SET deleted_at = now(), updated_at = now(), hlc = $2 WHERE id = $1"
            )))
            .bind(twin.id)
            .bind(&stamp)
            .execute(&mut *conn)
            .await?;
            for mark in marks {
                if let Some(value) = later.get(*mark) {
                    incoming.row.insert((*mark).to_string(), value.clone());
                }
            }
            self.insert_many(conn, p, &[&incoming]).await?;
        } else {
            incoming
                .row
                .insert("deleted_at".into(), Value::String(Utc::now().to_rfc3339()));
            self.insert_many(conn, p, &[&incoming]).await?;
            let known = self.columns_of(conn, table).await?.clone();
            let cols: Vec<String> = marks
                .iter()
                .filter(|m| known.iter().any(|k| k == *m) && later.contains_key(**m))
                .map(|m| ident(m))
                .collect();
            if incoming_is_later && !cols.is_empty() {
                let list = cols.join(", ");
                sqlx::query(sql(format!(
                    "UPDATE {table} SET ({list}) = (SELECT {list} FROM jsonb_populate_record(NULL::{table}, $2)),
                         updated_at = now(), hlc = $3
                     WHERE id = $1"
                )))
                .bind(twin.id)
                .bind(Value::Object(later.clone()))
                .bind(&stamp)
                .execute(&mut *conn)
                .await?;
            } else {
                sqlx::query(sql(format!(
                    "UPDATE {table} SET updated_at = now(), hlc = $2 WHERE id = $1"
                )))
                .bind(twin.id)
                .bind(&stamp)
                .execute(&mut *conn)
                .await?;
            }
        }
        self.queue(conn, table, &[change.id, twin.id], &stamp)
            .await?;

        // Where the two sides marked differently, the later mark stands; log the other.
        if let (Some(ours), Some(theirs)) = (twin.row.get("status"), change.row.get("status")) {
            if ours != theirs {
                let (kept, discarded, kept_hlc, discarded_hlc, by) = if incoming_is_later {
                    (
                        theirs,
                        ours,
                        &change.hlc,
                        &twin.hlc,
                        uuid_of(&twin.row, "updated_by"),
                    )
                } else {
                    (
                        ours,
                        theirs,
                        &twin.hlc,
                        &change.hlc,
                        uuid_of(&change.row, "updated_by"),
                    )
                };
                let entry = Overwrite {
                    uid: None,
                    at: None,
                    table: table.to_string(),
                    id: change.id.min(twin.id),
                    field: "status".into(),
                    kept: kept.clone(),
                    discarded: discarded.clone(),
                    kept_origin: if incoming_is_later {
                        self.state.role.other()
                    } else {
                        self.state.role
                    },
                    kept_hlc: kept_hlc.clone(),
                    discarded_hlc: discarded_hlc.clone(),
                    discarded_by: by,
                    reason: "later_mark".into(),
                };
                self.log_merge(conn, entry).await?;
            }
        }
        Ok(incoming_survives)
    }

    /// Staff attendance: one live record per staff member and day.
    async fn insert_staff_mark(
        &mut self,
        conn: &mut PgConnection,
        p: &'static TablePolicy,
        change: &Change,
    ) -> DbResult<Option<Outcome>> {
        let twin = twin_from(
            sqlx::query_as(
                "SELECT id, hlc, sync_row(to_jsonb(t)) FROM staff_attendance t
                 WHERE staff_id = $1 AND date = $2::date AND deleted_at IS NULL AND id <> $3",
            )
            .bind(uuid_of(&change.row, "staff_id"))
            .bind(text(&change.row, "date"))
            .bind(change.id)
            .fetch_optional(&mut *conn)
            .await?,
        );
        let Some(twin) = twin else {
            return Ok(None);
        };
        let marks = [
            "status",
            "note",
            "marked_by",
            "leave_request_id",
            "arrived_at",
        ];
        self.merge_pair(conn, p, change, &twin, &marks, &[]).await?;
        Ok(Some(Outcome::Applied))
    }

    /// Student attendance sheets: one live sheet per section and day. When the incoming
    /// sheet survives, the entries of the local sheet move onto it.
    async fn insert_sheet(
        &mut self,
        conn: &mut PgConnection,
        p: &'static TablePolicy,
        change: &Change,
    ) -> DbResult<Option<Outcome>> {
        let twin = twin_from(
            sqlx::query_as(
                "SELECT id, hlc, sync_row(to_jsonb(t)) FROM attendance_sheets t
                 WHERE section_id = $1 AND date = $2::date AND deleted_at IS NULL AND id <> $3",
            )
            .bind(uuid_of(&change.row, "section_id"))
            .bind(text(&change.row, "date"))
            .bind(change.id)
            .fetch_optional(&mut *conn)
            .await?,
        );
        let Some(twin) = twin else {
            return Ok(None);
        };
        let survives = self
            .merge_pair(conn, p, change, &twin, &["marked_by", "submitted_at"], &[])
            .await?;
        if survives {
            let stamp = self.fresh_stamp(&[]);
            let moved: Vec<Uuid> = sqlx::query_scalar(
                "UPDATE attendance_entries SET sheet_id = $2, updated_at = now(), hlc = $3
                 WHERE sheet_id = $1 AND deleted_at IS NULL RETURNING id",
            )
            .bind(twin.id)
            .bind(change.id)
            .bind(&stamp)
            .fetch_all(&mut *conn)
            .await?;
            self.queue(conn, "attendance_entries", &moved, &stamp)
                .await?;
        }
        self.sheets.insert(change.id);
        self.sheets.insert(twin.id);
        Ok(Some(Outcome::Applied))
    }

    /// Student attendance entries: one live entry per student and day, on the live sheet of
    /// its section and day.
    async fn insert_entry(
        &mut self,
        conn: &mut PgConnection,
        p: &'static TablePolicy,
        change: &Change,
    ) -> DbResult<Option<Outcome>> {
        let Some(sheet_id) = uuid_of(&change.row, "sheet_id") else {
            return Ok(None);
        };
        // The entry's sheet may have been folded into another one: it belongs on that one.
        let target: Uuid = sqlx::query_scalar(
            "SELECT COALESCE(
                 (SELECT live.id FROM attendance_sheets gone
                  JOIN attendance_sheets live
                    ON live.section_id = gone.section_id AND live.date = gone.date AND live.deleted_at IS NULL
                  WHERE gone.id = $1 AND gone.deleted_at IS NOT NULL
                  ORDER BY live.id LIMIT 1),
                 $1)",
        )
        .bind(sheet_id)
        .fetch_one(&mut *conn)
        .await?;
        let twin = twin_from(
            sqlx::query_as(
                "SELECT id, hlc, sync_row(to_jsonb(t)) FROM attendance_entries t
                 WHERE student_id = $1 AND date = $2::date AND deleted_at IS NULL AND id <> $3",
            )
            .bind(uuid_of(&change.row, "student_id"))
            .bind(text(&change.row, "date"))
            .bind(change.id)
            .fetch_optional(&mut *conn)
            .await?,
        );
        self.sheets.insert(sheet_id);
        self.sheets.insert(target);
        let Some(twin) = twin else {
            if target == sheet_id {
                return Ok(None);
            }
            // No twin, but the sheet moved: store the entry on the surviving sheet and tell
            // the other side where it went.
            let stamp = self.fresh_stamp(&[&change.hlc]);
            let mut moved = change.clone();
            moved
                .row
                .insert("sheet_id".into(), Value::String(target.to_string()));
            moved.row.insert("hlc".into(), Value::String(stamp.clone()));
            self.insert_many(conn, p, &[&moved]).await?;
            self.queue(conn, p.table, &[change.id], &stamp).await?;
            return Ok(Some(Outcome::Applied));
        };
        if let Some(twin_sheet) = uuid_of(&twin.row, "sheet_id") {
            self.sheets.insert(twin_sheet);
        }
        let survives = self
            .merge_pair(
                conn,
                p,
                change,
                &twin,
                &["status", "note", "leave_request_id"],
                &[("sheet_id", Value::String(target.to_string()))],
            )
            .await?;
        if !survives {
            sqlx::query(
                "UPDATE attendance_entries SET sheet_id = $2 WHERE id = $1 AND sheet_id <> $2",
            )
            .bind(twin.id)
            .bind(target)
            .execute(&mut *conn)
            .await?;
        }
        Ok(Some(Outcome::Applied))
    }

    // ---- derived values -----------------------------------------------------------------------

    /// Count the derived values again for everything this batch touched: a voucher's paid
    /// amount is the sum of its receipts (and its payment status follows), a sheet's totals
    /// are counted from its live entries. Silent: both sides compute the same.
    pub(super) async fn recount(&mut self, conn: &mut PgConnection) -> DbResult<()> {
        if !self.vouchers.is_empty() {
            let ids: Vec<Uuid> = self.vouchers.drain().collect();
            sqlx::query(
                "UPDATE fee_vouchers v
                 SET net_paid = s.paid,
                     status = CASE
                         WHEN v.status IN ('cancelled', 'carried') THEN v.status
                         WHEN v.total_due + v.late_fee - s.paid <= 0 THEN 'paid'
                         WHEN s.paid > 0 THEN 'partial'
                         ELSE 'unpaid' END
                 FROM (SELECT x.id, COALESCE((SELECT sum(r.amount) FROM fee_receipts r WHERE r.voucher_id = x.id), 0) AS paid
                       FROM fee_vouchers x WHERE x.id = ANY($1)) s
                 WHERE v.id = s.id AND v.net_paid <> s.paid",
            )
            .bind(&ids)
            .execute(&mut *conn)
            .await?;
        }
        if !self.sheets.is_empty() {
            let ids: Vec<Uuid> = self.sheets.drain().collect();
            sqlx::query(
                "UPDATE attendance_sheets s
                 SET present = c.present, absent = c.absent, late = c.late, leave = c.leave
                 FROM (SELECT x.id,
                              (count(e.id) FILTER (WHERE e.status = 'present'))::int AS present,
                              (count(e.id) FILTER (WHERE e.status = 'absent'))::int AS absent,
                              (count(e.id) FILTER (WHERE e.status = 'late'))::int AS late,
                              (count(e.id) FILTER (WHERE e.status = 'leave'))::int AS leave
                       FROM attendance_sheets x
                       LEFT JOIN attendance_entries e ON e.sheet_id = x.id AND e.deleted_at IS NULL
                       WHERE x.id = ANY($1) GROUP BY x.id) c
                 WHERE s.id = c.id
                   AND (s.present, s.absent, s.late, s.leave) IS DISTINCT FROM (c.present, c.absent, c.late, c.leave)",
            )
            .bind(&ids)
            .execute(&mut *conn)
            .await?;
        }
        Ok(())
    }
}
