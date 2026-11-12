//! Nexus sync, stage 3: receipts and attendance are always merged. The fee and attendance
//! repository functions run on both copies of one school (the school's and the hub's), then
//! the copies sync. Runs when `MADRASA_TEST_DATABASE_URL` is set; skipped otherwise.

mod sync_support;

use chrono::NaiveDate;
use madrasa_core::attendance::{AttendanceStatus, StaffStatus};
use madrasa_core::fees::PaymentMethod;
use madrasa_core::hlc::Hlc;
use madrasa_db::models::{FeeReceipt, FeeVoucher};
use madrasa_db::{attendance, fees, PgPool};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use sync_support::{d, log, rows, Net};
use uuid::Uuid;

fn money(v: i64) -> Decimal {
    Decimal::new(v * 100, 2)
}

async fn voucher(net: &Net, student: Uuid, amount: i64) -> FeeVoucher {
    let mut c = net.school.pool.acquire().await.unwrap();
    fees::create_one_off_voucher(
        &mut c,
        &net.school_hlc,
        net.actor(),
        net.sch.id,
        student,
        net.year.id,
        Some(net.section.id),
        d(2026, 4, 10),
        net.fee_head,
        "Tuition April",
        money(amount),
    )
    .await
    .unwrap()
}

async fn pay(pool: &PgPool, hlc: &Hlc, net: &Net, voucher_id: Uuid, amount: i64) -> FeeReceipt {
    let mut c = pool.acquire().await.unwrap();
    let policy = fees::policy(&mut c, net.sch.id).await.unwrap();
    fees::record_payment(
        &mut c,
        hlc,
        net.actor(),
        &policy,
        fees::NewPayment {
            voucher_id,
            amount: money(amount),
            method: PaymentMethod::Cash,
            reference: None,
            note: None,
            paid_on: d(2026, 4, 10),
            received_by: net.admin,
        },
    )
    .await
    .unwrap()
    .0
}

async fn voucher_state(pool: &PgPool, id: Uuid) -> (Decimal, String) {
    let mut c = pool.acquire().await.unwrap();
    let v = fees::get_voucher(&mut c, id).await.unwrap();
    (v.net_paid, v.status)
}

/// (receipt number, amount) of every receipt, in id order.
async fn receipts(pool: &PgPool) -> Vec<(Uuid, String, Decimal)> {
    sqlx::query_as("SELECT id, receipt_no, amount FROM fee_receipts ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn receipts_taken_on_both_sides_add_up_on_the_voucher() {
    for push_first in [true, false] {
        let net = net!(1);
        let v = voucher(&net, net.students[0], 1000).await;
        net.settle(true).await;

        // The school takes 400 at its desk; head office takes 300 for the same voucher
        // before either has synced. Each side sees only its own receipt and total.
        let r_school = pay(&net.school.pool, &net.school_hlc, &net, v.id, 400).await;
        let r_hub = pay(&net.hub.pool, &net.hub_hlc, &net, v.id, 300).await;
        assert_eq!(
            r_school.receipt_no, r_hub.receipt_no,
            "both sides issued the same number"
        );
        assert_eq!(voucher_state(&net.school.pool, v.id).await.0, money(400));
        assert_eq!(voucher_state(&net.hub.pool, v.id).await.0, money(300));

        net.settle(push_first).await;

        // Nothing was lost in either direction: both receipts on both sides, adding up.
        for pool in [&net.school.pool, &net.hub.pool] {
            let all = receipts(pool).await;
            assert_eq!(all.len(), 2);
            assert_eq!(all.iter().map(|r| r.2).sum::<Decimal>(), money(700));
            assert_eq!(
                voucher_state(pool, v.id).await,
                (money(700), "partial".to_string())
            );
            // Both kept: the receipt created later carries the suffix, on both sides.
            assert_eq!(all[0].0, r_school.id);
            assert_eq!(all[0].1, r_school.receipt_no);
            assert_eq!(all[1].1, format!("{}-2", r_school.receipt_no));
        }
        net.assert_same(&["fee_receipts", "fee_vouchers"]).await;

        // The renumbering is in the log of both sides, once; the paid amount and payment
        // status are recomputed, not argued over, so there is no other entry.
        for pool in [&net.school.pool, &net.hub.pool] {
            let entries = log(pool).await;
            assert_eq!(entries.len(), 1, "{entries:?}");
            let (table, field, reason, kept, discarded) = &entries[0];
            assert_eq!(
                (table.as_str(), field.as_str(), reason.as_str()),
                ("fee_receipts", "receipt_no", "renumbered")
            );
            assert_eq!(kept, &json!(format!("{}-2", r_school.receipt_no)));
            assert_eq!(discarded, &json!(r_school.receipt_no));
        }

        // Head office takes the rest: paid on both sides.
        pay(&net.hub.pool, &net.hub_hlc, &net, v.id, 300).await;
        net.settle(push_first).await;
        for pool in [&net.school.pool, &net.hub.pool] {
            assert_eq!(
                voucher_state(pool, v.id).await,
                (money(1000), "paid".to_string())
            );
            assert_eq!(receipts(pool).await.len(), 3);
        }
        net.assert_same(&["fee_receipts", "fee_vouchers"]).await;
        net.finish().await;
    }
}

#[tokio::test]
async fn receipts_that_together_overpay_are_both_kept() {
    let net = net!(1);
    let v = voucher(&net, net.students[0], 1000).await;
    net.settle(true).await;
    // Each side takes 800 of the 1,000 due. Neither could know about the other.
    pay(&net.school.pool, &net.school_hlc, &net, v.id, 800).await;
    pay(&net.hub.pool, &net.hub_hlc, &net, v.id, 800).await;
    net.settle(true).await;
    for pool in [&net.school.pool, &net.hub.pool] {
        assert_eq!(receipts(pool).await.len(), 2, "neither receipt is dropped");
        assert_eq!(
            voucher_state(pool, v.id).await,
            (money(1600), "paid".to_string())
        );
    }
    net.assert_same(&["fee_receipts", "fee_vouchers"]).await;
    net.finish().await;
}

#[tokio::test]
async fn a_receipt_reversed_on_both_sides_is_reversed_once() {
    for push_first in [true, false] {
        let net = net!(1);
        let v = voucher(&net, net.students[0], 1000).await;
        let receipt = pay(&net.school.pool, &net.school_hlc, &net, v.id, 500).await;
        net.settle(true).await;
        assert_eq!(
            voucher_state(&net.hub.pool, v.id).await,
            (money(500), "partial".to_string())
        );

        // Number sequences are not synced, so a head office that issues receipts has its
        // own series (here from 900000) and cannot collide with a number it already holds.
        sqlx::query(
            "INSERT INTO school_sequences (id, school_id, key, next_value, hlc)
             VALUES ($1, $2, 'receipt_no:2026', 900000, $3)",
        )
        .bind(Uuid::now_v7())
        .bind(net.sch.id)
        .bind(net.hub_hlc.now().to_string())
        .execute(&net.hub.pool)
        .await
        .unwrap();
        // The cheque bounced: the school reverses the receipt, and so does head office.
        let mut reversals = Vec::new();
        for (pool, hlc) in [
            (&net.school.pool, &net.school_hlc),
            (&net.hub.pool, &net.hub_hlc),
        ] {
            let mut c = pool.acquire().await.unwrap();
            let (reversal, _) = fees::reverse_receipt(
                &mut c,
                hlc,
                net.actor(),
                receipt.id,
                "Cheque bounced",
                net.admin,
                d(2026, 4, 12),
            )
            .await
            .unwrap();
            reversals.push(reversal.id);
        }
        net.settle(push_first).await;

        // One reversal on both sides (the earlier one), so the amount is taken off once.
        let earlier = reversals[0].min(reversals[1]);
        for pool in [&net.school.pool, &net.hub.pool] {
            let all = receipts(pool).await;
            assert_eq!(all.len(), 2, "the receipt and one reversal: {all:?}");
            assert_eq!(all[1].0, earlier);
            assert_eq!(all.iter().map(|r| r.2).sum::<Decimal>(), money(0));
            assert_eq!(
                voucher_state(pool, v.id).await,
                (money(0), "unpaid".to_string())
            );
            let entries = log(pool).await;
            let dup: Vec<_> = entries
                .iter()
                .filter(|e| e.2 == "duplicate_reversal")
                .collect();
            assert_eq!(dup.len(), 1, "{entries:?}");
            // The removed reversal is kept in the log in full.
            assert_eq!(dup[0].4["amount"], json!("-500.00"));
            assert_eq!(dup[0].4["reversal_reason"], json!("Cheque bounced"));
        }
        net.assert_same(&["fee_receipts", "fee_vouchers"]).await;
        net.finish().await;
    }
}

#[tokio::test]
async fn receipts_stay_immutable_outside_a_merge() {
    let net = net!(1);
    let v = voucher(&net, net.students[0], 1000).await;
    let receipt = pay(&net.school.pool, &net.school_hlc, &net, v.id, 500).await;
    for sql in [
        "UPDATE fee_receipts SET amount = 1",
        "UPDATE fee_receipts SET receipt_no = 'X'",
        "DELETE FROM fee_receipts",
    ] {
        let err = sqlx::query(sql)
            .execute(&net.school.pool)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("immutable"), "{sql}: {err}");
    }
    // Even while a merge is applying, only the number may change, and only a reversal may go.
    let mut tx = net.school.pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('madrasa.sync_apply', 'on', true)")
        .execute(&mut *tx)
        .await
        .unwrap();
    for sql in [
        "UPDATE fee_receipts SET amount = 1",
        "DELETE FROM fee_receipts",
    ] {
        let mut sp = sqlx::Connection::begin(&mut *tx).await.unwrap();
        let err = sqlx::query(sql).execute(&mut *sp).await.unwrap_err();
        assert!(err.to_string().contains("immutable"), "{sql}: {err}");
        sp.rollback().await.unwrap();
    }
    sqlx::query("UPDATE fee_receipts SET receipt_no = 'R-MERGED' WHERE id = $1")
        .bind(receipt.id)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(receipts(&net.school.pool).await[0].1, receipt.receipt_no);
    net.finish().await;
}

// ---- attendance -------------------------------------------------------------------------------

const MONDAY: (i32, u32, u32) = (2026, 4, 13);

fn day(offset: u32) -> NaiveDate {
    d(MONDAY.0, MONDAY.1, MONDAY.2 + offset)
}

async fn mark(
    pool: &PgPool,
    hlc: &Hlc,
    net: &Net,
    date: NaiveDate,
    exceptions: &[(usize, AttendanceStatus)],
    correction: bool,
) {
    let mut c = pool.acquire().await.unwrap();
    attendance::save_sheet(
        &mut c,
        hlc,
        net.actor(),
        attendance::SaveSheet {
            school_id: net.sch.id,
            section_id: net.section.id,
            date,
            marked_by: net.admin,
            entries: exceptions
                .iter()
                .map(|(i, status)| attendance::EntryInput {
                    student_id: net.students[*i],
                    status: *status,
                    note: None,
                })
                .collect(),
        },
        correction,
    )
    .await
    .unwrap();
}

/// Live sheets of a day as (id, present, absent, late, leave).
async fn sheets(pool: &PgPool, date: NaiveDate) -> Vec<(Uuid, i32, i32, i32, i32)> {
    sqlx::query_as(
        "SELECT id, present, absent, late, leave FROM attendance_sheets
         WHERE date = $1 AND deleted_at IS NULL ORDER BY id",
    )
    .bind(date)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// Live marks of a day as (student, status, sheet), in the fixture's student order.
async fn marks(pool: &PgPool, net: &Net, date: NaiveDate) -> Vec<(String, Uuid)> {
    let found: Vec<(Uuid, String, Uuid)> = sqlx::query_as(
        "SELECT student_id, status, sheet_id FROM attendance_entries WHERE date = $1 AND deleted_at IS NULL",
    )
    .bind(date)
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(
        found.len(),
        net.students.len(),
        "exactly one live mark per student: {found:?}"
    );
    net.students
        .iter()
        .map(|s| {
            let m = found
                .iter()
                .find(|f| f.0 == *s)
                .expect("a mark for every student");
            (m.1.clone(), m.2)
        })
        .collect()
}

#[tokio::test]
async fn sheets_made_on_both_sides_for_one_day_fold_into_one() {
    for push_first in [true, false] {
        let net = net!(3);
        // The school marks Monday (A absent). Head office, not knowing, marks the same
        // section and day afterwards (B late).
        mark(
            &net.school.pool,
            &net.school_hlc,
            &net,
            day(0),
            &[(0, AttendanceStatus::Absent)],
            false,
        )
        .await;
        mark(
            &net.hub.pool,
            &net.hub_hlc,
            &net,
            day(0),
            &[(1, AttendanceStatus::Late)],
            false,
        )
        .await;
        let school_sheet = sheets(&net.school.pool, day(0)).await[0].0;
        let hub_sheet = sheets(&net.hub.pool, day(0)).await[0].0;
        assert_ne!(school_sheet, hub_sheet);

        net.settle(push_first).await;

        for pool in [&net.school.pool, &net.hub.pool] {
            // One live sheet (the earlier one), one live mark per student, all on it.
            let live = sheets(pool, day(0)).await;
            assert_eq!(live.len(), 1, "{live:?}");
            assert_eq!(live[0].0, school_sheet.min(hub_sheet));
            let m = marks(pool, &net, day(0)).await;
            assert!(m.iter().all(|(_, sheet)| *sheet == live[0].0));
            // The later marks stand: head office marked last, so A is present and B late.
            let statuses: Vec<&str> = m.iter().map(|(s, _)| s.as_str()).collect();
            assert_eq!(statuses, ["present", "late", "present"]);
            // The sheet's totals are counted from the marks that stand.
            assert_eq!((live[0].1, live[0].2, live[0].3, live[0].4), (2, 0, 1, 0));
            // Nothing vanished: both sheets and all six entries still exist, three set aside.
            let total: (i64, i64) = sqlx::query_as(
                "SELECT (SELECT count(*) FROM attendance_sheets WHERE date = $1),
                        (SELECT count(*) FROM attendance_entries WHERE date = $1)",
            )
            .bind(day(0))
            .fetch_one(pool)
            .await
            .unwrap();
            assert_eq!(total, (2, 6));
            // The two marks that were replaced are in the log, with what they were.
            let mut entries: Vec<_> = log(pool)
                .await
                .into_iter()
                .map(|e| (e.2, e.3, e.4))
                .collect();
            entries.sort_by_key(|e| e.1.to_string());
            assert_eq!(
                entries,
                [
                    ("later_mark".to_string(), json!("late"), json!("present")),
                    ("later_mark".to_string(), json!("present"), json!("absent")),
                ]
            );
        }
        net.assert_same(&["attendance_sheets", "attendance_entries"])
            .await;
        net.finish().await;
    }
}

#[tokio::test]
async fn attendance_for_different_days_simply_combines() {
    let net = net!(2);
    mark(
        &net.school.pool,
        &net.school_hlc,
        &net,
        day(0),
        &[(0, AttendanceStatus::Absent)],
        false,
    )
    .await;
    mark(
        &net.hub.pool,
        &net.hub_hlc,
        &net,
        day(1),
        &[(1, AttendanceStatus::Leave)],
        false,
    )
    .await;
    net.settle(true).await;
    for pool in [&net.school.pool, &net.hub.pool] {
        assert_eq!(sheets(pool, day(0)).await.len(), 1);
        assert_eq!(sheets(pool, day(1)).await.len(), 1);
        assert_eq!(marks(pool, &net, day(0)).await[0].0, "absent");
        assert_eq!(marks(pool, &net, day(1)).await[1].0, "leave");
        assert!(log(pool).await.is_empty(), "nothing was replaced");
    }
    net.assert_same(&["attendance_sheets", "attendance_entries"])
        .await;
    net.finish().await;
}

#[tokio::test]
async fn corrections_on_both_sides_the_later_mark_stands_even_against_head_office() {
    let net = net!(2);
    mark(&net.school.pool, &net.school_hlc, &net, day(0), &[], false).await;
    net.settle(true).await;
    // Head office corrects A to leave; afterwards the school, which saw the child arrive,
    // corrects A to late. The school's mark is the later one and stands on both sides.
    mark(
        &net.hub.pool,
        &net.hub_hlc,
        &net,
        day(0),
        &[(0, AttendanceStatus::Leave)],
        true,
    )
    .await;
    mark(
        &net.school.pool,
        &net.school_hlc,
        &net,
        day(0),
        &[(0, AttendanceStatus::Late)],
        true,
    )
    .await;
    net.settle(true).await;
    for pool in [&net.school.pool, &net.hub.pool] {
        assert_eq!(
            marks(pool, &net, day(0)).await[0].0,
            "late",
            "head office does not win attendance"
        );
        let s = sheets(pool, day(0)).await;
        assert_eq!(
            (s[0].1, s[0].3, s[0].4),
            (1, 1, 0),
            "totals follow the marks"
        );
        let entries = log(pool).await;
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(
            (entries[0].2.as_str(), &entries[0].3, &entries[0].4),
            ("later_mark", &json!("late"), &json!("leave"))
        );
    }
    net.assert_same(&["attendance_sheets", "attendance_entries"])
        .await;
    net.finish().await;
}

#[tokio::test]
async fn staff_attendance_from_both_sides_folds_into_one_record() {
    for push_first in [true, false] {
        let net = net!(1);
        let save = |pool: PgPool, hlc: Hlc, status: StaffStatus| {
            let (school, admin, staff, actor) = (net.sch.id, net.admin, net.staff[0], net.actor());
            async move {
                let mut c = pool.acquire().await.unwrap();
                attendance::save_staff_day(
                    &mut c,
                    &hlc,
                    actor,
                    school,
                    day(0),
                    admin,
                    &[attendance::StaffEntryInput {
                        staff_id: staff,
                        status,
                        note: None,
                        arrived_at: None,
                    }],
                )
                .await
                .unwrap();
            }
        };
        save(net.school.pool.clone(), Hlc::new(0x5c), StaffStatus::Absent).await;
        save(net.hub.pool.clone(), Hlc::new(0x4b), StaffStatus::Late).await;
        net.settle(push_first).await;
        for pool in [&net.school.pool, &net.hub.pool] {
            let live: Vec<(Uuid, String)> = sqlx::query_as(
                "SELECT id, status FROM staff_attendance WHERE staff_id = $1 AND date = $2 AND deleted_at IS NULL",
            )
            .bind(net.staff[0])
            .bind(day(0))
            .fetch_all(pool)
            .await
            .unwrap();
            assert_eq!(
                live.len(),
                1,
                "one record per staff member and day: {live:?}"
            );
            assert_eq!(live[0].1, "late", "the later mark stands");
            let entries = log(pool).await;
            assert_eq!(entries.len(), 1, "{entries:?}");
            assert_eq!(
                (&entries[0].3, &entries[0].4),
                (&json!("late"), &json!("absent"))
            );
        }
        net.assert_same(&["staff_attendance"]).await;
        net.finish().await;
    }
}

#[tokio::test]
async fn a_merged_day_survives_later_edits_and_another_sync() {
    let net = net!(3);
    mark(
        &net.school.pool,
        &net.school_hlc,
        &net,
        day(0),
        &[(0, AttendanceStatus::Absent)],
        false,
    )
    .await;
    mark(
        &net.hub.pool,
        &net.hub_hlc,
        &net,
        day(0),
        &[(1, AttendanceStatus::Late)],
        false,
    )
    .await;
    net.settle(true).await;
    // After the merge the school corrects C on the merged sheet; it syncs like any edit.
    mark(
        &net.school.pool,
        &net.school_hlc,
        &net,
        day(0),
        &[(1, AttendanceStatus::Late), (2, AttendanceStatus::Leave)],
        true,
    )
    .await;
    net.settle(true).await;
    for pool in [&net.school.pool, &net.hub.pool] {
        let statuses: Vec<String> = marks(pool, &net, day(0))
            .await
            .into_iter()
            .map(|m| m.0)
            .collect();
        assert_eq!(statuses, ["present", "late", "leave"]);
        let s = sheets(pool, day(0)).await;
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].1, s[0].2, s[0].3, s[0].4), (1, 0, 1, 1));
    }
    net.assert_same(&["attendance_sheets", "attendance_entries"])
        .await;
    let _: Vec<Value> = rows(&net.hub.pool, "attendance_sheets").await;
    net.finish().await;
}

// ---- parent app: notices and messages (docs/adr/0006-parent-app.md) -----------------------------

/// A guardian of the first student, made on the school side and synced up.
async fn guardian(net: &Net) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO guardians (id, school_id, full_name, phone, hlc) VALUES ($1, $2, 'Imran Khan', '0300-1111111', $3)",
    )
    .bind(id)
    .bind(net.sch.id)
    .bind(net.school_hlc.now().to_string())
    .execute(&net.school.pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO student_guardians (id, school_id, student_id, guardian_id, relationship, is_primary, hlc)
         VALUES ($1, $2, $3, $4, 'father', TRUE, $5)",
    )
    .bind(Uuid::now_v7())
    .bind(net.sch.id)
    .bind(net.students[0])
    .bind(id)
    .bind(net.school_hlc.now().to_string())
    .execute(&net.school.pool)
    .await
    .unwrap();
    id
}

async fn say(
    pool: &PgPool,
    hlc: &Hlc,
    net: &Net,
    guardian: Uuid,
    side: madrasa_core::parent_app::Side,
    text: &str,
) {
    use madrasa_core::parent_app::Side;
    let mut c = pool.acquire().await.unwrap();
    madrasa_db::parent_app::send_message(
        &mut c,
        hlc,
        net.actor(),
        madrasa_db::parent_app::NewMessage {
            school_id: net.sch.id,
            student_id: net.students[0],
            guardian_id: guardian,
            side,
            sender_user_id: (side == Side::Staff).then_some(net.admin),
            sender_name: if side == Side::Staff {
                "Ayesha Khan".into()
            } else {
                "Imran Khan".into()
            },
            body: text.into(),
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn messages_from_both_sides_are_all_kept_and_read_marks_do_not_collide() {
    use madrasa_core::parent_app::Side;
    use madrasa_db::parent_app;
    let Some(net) = sync_support::net(2).await else {
        return;
    };
    let imran = guardian(&net).await;
    net.settle(true).await;

    // Before a sync in between: the parent writes at the hub, the teacher at the school,
    // and each marks the conversation read on their own side.
    say(
        &net.hub.pool,
        &net.hub_hlc,
        &net,
        imran,
        Side::Guardian,
        "Salaam, Ali was unwell today.",
    )
    .await;
    say(
        &net.school.pool,
        &net.school_hlc,
        &net,
        imran,
        Side::Staff,
        "Please send Ali's notebook tomorrow.",
    )
    .await;
    say(
        &net.hub.pool,
        &net.hub_hlc,
        &net,
        imran,
        Side::Guardian,
        "He will be back on Monday.",
    )
    .await;
    for (pool, hlc, side) in [
        (&net.hub.pool, &net.hub_hlc, Side::Guardian),
        (&net.school.pool, &net.school_hlc, Side::Staff),
    ] {
        let mut c = pool.acquire().await.unwrap();
        parent_app::mark_read(
            &mut c,
            hlc,
            net.actor(),
            net.sch.id,
            net.students[0],
            imran,
            side,
        )
        .await
        .unwrap();
    }
    net.settle(true).await;

    // Both copies hold all three messages, in the order they were written, and both marks.
    net.assert_same(&["messages", "message_reads"]).await;
    for pool in [&net.school.pool, &net.hub.pool] {
        let mut c = pool.acquire().await.unwrap();
        let thread = parent_app::conversation(&mut c, net.students[0], imran)
            .await
            .unwrap();
        let texts: Vec<(&str, &str)> = thread
            .iter()
            .map(|m| (m.sender.as_str(), m.body.as_str()))
            .collect();
        assert_eq!(texts.len(), 3, "{texts:?}");
        assert!(texts.contains(&("guardian", "Salaam, Ali was unwell today.")));
        assert!(texts.contains(&("staff", "Please send Ali's notebook tomorrow.")));
        assert!(thread.windows(2).all(|w| w[0].sent_at <= w[1].sent_at));
        assert_eq!(rows(pool, "message_reads").await.len(), 2);
    }
    // Nothing was overwritten and nothing is waiting.
    assert!(
        log(&net.school.pool).await.is_empty(),
        "{:?}",
        log(&net.school.pool).await
    );
    assert!(log(&net.hub.pool).await.is_empty());
    assert_eq!(sync_support::pending(&net.school.pool).await, 0);
    assert_eq!(sync_support::pending(&net.hub.pool).await, 0);

    // A message can never be changed or removed, on either side.
    for pool in [&net.school.pool, &net.hub.pool] {
        let changed = sqlx::query("UPDATE messages SET body = 'changed'")
            .execute(pool)
            .await;
        assert!(changed
            .unwrap_err()
            .to_string()
            .contains("never changed or removed"));
        let removed = sqlx::query("DELETE FROM messages").execute(pool).await;
        assert!(removed.is_err());
    }
    // A message for someone who is not the student's guardian is refused.
    let mut c = net.hub.pool.acquire().await.unwrap();
    let stranger = parent_app::send_message(
        &mut c,
        &net.hub_hlc,
        net.actor(),
        parent_app::NewMessage {
            school_id: net.sch.id,
            student_id: net.students[1],
            guardian_id: imran,
            side: Side::Guardian,
            sender_user_id: None,
            sender_name: "Imran Khan".into(),
            body: "About another child".into(),
        },
    )
    .await;
    assert!(
        matches!(stranger, Err(madrasa_db::DbError::NotFound(_))),
        "{stranger:?}"
    );
    drop(c);
    net.finish().await;
}

#[tokio::test]
async fn a_notice_with_its_classes_reaches_the_hub_and_a_withdrawal_follows() {
    use madrasa_db::parent_app::{self, NoticeInput};
    let Some(net) = sync_support::net(1).await else {
        return;
    };
    net.settle(true).await;
    let class: Uuid = sqlx::query_scalar("SELECT class_id FROM sections WHERE id = $1")
        .bind(net.section.id)
        .fetch_one(&net.school.pool)
        .await
        .unwrap();
    let mut c = net.school.pool.acquire().await.unwrap();
    let whole = parent_app::create_notice(
        &mut c,
        &net.school_hlc,
        net.actor(),
        net.sch.id,
        NoticeInput {
            title: "Sports day".into(),
            body: "Friday. Bring water.".into(),
            class_ids: vec![],
            expires_on: None,
        },
        true,
    )
    .await
    .unwrap();
    let one_class = parent_app::create_notice(
        &mut c,
        &net.school_hlc,
        net.actor(),
        net.sch.id,
        NoticeInput {
            title: "Class trip".into(),
            body: String::new(),
            class_ids: vec![class, class],
            expires_on: Some(d(2026, 12, 1)),
        },
        true,
    )
    .await
    .unwrap();
    assert_eq!(
        one_class.class_ids,
        [class],
        "a class named twice is stored once"
    );
    let draft = parent_app::create_notice(
        &mut c,
        &net.school_hlc,
        net.actor(),
        net.sch.id,
        NoticeInput {
            title: "Not ready".into(),
            body: String::new(),
            class_ids: vec![],
            expires_on: None,
        },
        false,
    )
    .await
    .unwrap();
    let unknown = parent_app::create_notice(
        &mut c,
        &net.school_hlc,
        net.actor(),
        net.sch.id,
        NoticeInput {
            title: "Bad".into(),
            body: String::new(),
            class_ids: vec![Uuid::now_v7()],
            expires_on: None,
        },
        true,
    )
    .await;
    assert!(matches!(unknown, Err(madrasa_db::DbError::NotFound(_))));
    drop(c);
    net.settle(true).await;
    net.assert_same(&["notices"]).await;

    // At the hub a parent of that class sees both published notices; another class sees one.
    let now = chrono::Utc::now();
    let today = d(2026, 11, 12);
    let mut h = net.hub.pool.acquire().await.unwrap();
    let titles =
        |list: Vec<parent_app::Notice>| list.into_iter().map(|n| n.title).collect::<Vec<_>>();
    let mut for_class = titles(
        parent_app::notices_for_class(&mut h, net.sch.id, Some(class), now, today)
            .await
            .unwrap(),
    );
    for_class.sort();
    assert_eq!(for_class, ["Class trip", "Sports day"]);
    assert_eq!(
        titles(
            parent_app::notices_for_class(&mut h, net.sch.id, Some(Uuid::now_v7()), now, today)
                .await
                .unwrap()
        ),
        ["Sports day"]
    );
    assert_eq!(
        titles(
            parent_app::notices_for_class(&mut h, net.sch.id, Some(class), now, d(2026, 12, 2))
                .await
                .unwrap()
        ),
        ["Sports day"],
        "past its last day"
    );
    drop(h);

    // The school withdraws one and publishes the draft: the hub follows.
    let mut c = net.school.pool.acquire().await.unwrap();
    parent_app::set_published(
        &mut c,
        &net.school_hlc,
        net.actor(),
        net.sch.id,
        whole.id,
        false,
    )
    .await
    .unwrap();
    parent_app::set_published(
        &mut c,
        &net.school_hlc,
        net.actor(),
        net.sch.id,
        draft.id,
        true,
    )
    .await
    .unwrap();
    parent_app::delete_notice(
        &mut c,
        &net.school_hlc,
        net.actor(),
        net.sch.id,
        one_class.id,
    )
    .await
    .unwrap();
    drop(c);
    net.settle(true).await;
    let mut h = net.hub.pool.acquire().await.unwrap();
    assert_eq!(
        titles(
            parent_app::notices_for_class(
                &mut h,
                net.sch.id,
                Some(class),
                chrono::Utc::now(),
                today
            )
            .await
            .unwrap()
        ),
        ["Not ready"]
    );
    drop(h);
    assert!(log(&net.hub.pool).await.is_empty());
    net.finish().await;
}

/// The parent app migration grants permissions to the built-in roles of existing schools.
/// It runs on the school and on the hub's copy of it: the two must end up with the very
/// same records and send nothing to each other, or the grants would collide at the next
/// sync and wait there for ever.
#[tokio::test]
async fn the_migrations_permission_grants_are_identical_on_both_copies() {
    let Some(net) = sync_support::net(1).await else {
        return;
    };
    {
        let mut c = net.school.pool.acquire().await.unwrap();
        madrasa_db::roles::create_defaults(
            &mut c,
            &net.school_hlc,
            net.actor(),
            net.sch.id,
            madrasa_core::edition::Edition::Lite,
        )
        .await
        .unwrap();
    }
    net.settle(true).await;
    // As both copies were before the upgrade: roles, but none of the new grants.
    let migration = include_str!("../migrations/0029_parent_app.sql");
    let grants = migration
        .split("SET LOCAL madrasa.sync_apply = 'on';")
        .nth(1)
        .expect("the migration silences change capture for its grants");
    let grants = format!("SET LOCAL madrasa.sync_apply = 'on';{grants}");
    let new_codes = [
        "parents.manage",
        "notices.manage",
        "messages.reply",
        "messages.view_all",
    ];
    for pool in [&net.school.pool, &net.hub.pool] {
        let removed = sqlx::query("DELETE FROM permissions WHERE code = ANY($1)")
            .bind(new_codes.as_slice())
            .execute(pool)
            .await
            .unwrap();
        assert_eq!(
            removed.rows_affected(),
            7,
            "principal 4, admin 2, teacher 1"
        );
        let mut tx = pool.begin().await.unwrap();
        sqlx::raw_sql(sqlx::AssertSqlSafe(grants.clone()))
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        // Running it twice changes nothing.
        let mut tx = pool.begin().await.unwrap();
        sqlx::raw_sql(sqlx::AssertSqlSafe(grants.clone()))
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            sync_support::pending(pool).await,
            0,
            "the grants are not queued for sending"
        );
    }
    let granted = |pool: PgPool| async move {
        sqlx::query_as::<_, (String, String, Uuid)>(
            "SELECT r.code, p.code, p.id FROM permissions p JOIN roles r ON r.id = p.role_id
             WHERE p.code = ANY($1) ORDER BY 1, 2",
        )
        .bind(new_codes.as_slice())
        .fetch_all(&pool)
        .await
        .unwrap()
    };
    let (at_school, at_hub) = (
        granted(net.school.pool.clone()).await,
        granted(net.hub.pool.clone()).await,
    );
    assert_eq!(at_school, at_hub, "same roles, same permissions, same ids");
    let pairs: Vec<(&str, &str)> = at_school
        .iter()
        .map(|(r, p, _)| (r.as_str(), p.as_str()))
        .collect();
    assert_eq!(
        pairs,
        [
            ("admin", "notices.manage"),
            ("admin", "parents.manage"),
            ("principal", "messages.reply"),
            ("principal", "messages.view_all"),
            ("principal", "notices.manage"),
            ("principal", "parents.manage"),
            ("teacher", "messages.reply"),
        ]
    );
    // And the code's own defaults agree with what the migration grants.
    for (role, code, _) in &at_school {
        let role: madrasa_core::permissions::RoleCode = role.parse().unwrap();
        let permission: madrasa_core::permissions::Permission = code.parse().unwrap();
        assert!(
            role.default_permissions().contains(&permission),
            "{role} {code}"
        );
    }
    // A later sync has nothing to argue about.
    net.settle(true).await;
    net.assert_same(&["permissions"]).await;
    for pool in [&net.school.pool, &net.hub.pool] {
        let parked: i64 = sqlx::query_scalar("SELECT count(*) FROM sync_parked")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(parked, 0);
    }
    net.finish().await;
}
