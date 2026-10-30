//! M1 stage 2: fees. Needs PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

use chrono::NaiveDate;
use madrasa_core::fees::{FeePolicy, PaymentMethod, SiblingRule};
use madrasa_core::hlc::Hlc;
use madrasa_core::password::hash_password;
use madrasa_db::models::{AcademicYear, School, Section};
use madrasa_db::testing::TestDb;
use madrasa_db::{
    academics, audit, fees, guardians, schools, students, test_db, users, Actor, DbError, PgPool,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

fn hlc() -> Hlc {
    Hlc::new(9)
}
fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

struct Fixture {
    school: School,
    year: AcademicYear,
    sec1: Section,
    sec2: Section,
    admin: Uuid,
    tuition: Uuid,
    transport: Uuid,
    #[allow(dead_code)]
    admission: Uuid,
}

async fn fixture(pool: &PgPool) -> Fixture {
    let mut c = pool.acquire().await.unwrap();
    let school = schools::create(
        &mut c,
        &hlc(),
        Actor::SYSTEM,
        schools::NewSchool {
            name: "T".into(),
            code: "KHI-MC".into(),
            city: "".into(),
            is_demo: false,
            academic_year_start_month: 4,
            currency: "PKR".into(),
        },
    )
    .await
    .unwrap();
    let actor = Actor::system_for(school.id);
    let (name, start, end) = academics::default_year_bounds(2026, 4).unwrap();
    let year = academics::create_year(
        &mut c,
        &hlc(),
        actor,
        academics::NewAcademicYear {
            school_id: school.id,
            name,
            start_date: start,
            end_date: end,
            start_month: 4,
        },
    )
    .await
    .unwrap();
    academics::activate_year(&mut c, &hlc(), actor, year.id)
        .await
        .unwrap();
    let c1 = academics::create_class(&mut c, &hlc(), actor, school.id, "1", 1)
        .await
        .unwrap();
    let c2 = academics::create_class(&mut c, &hlc(), actor, school.id, "2", 2)
        .await
        .unwrap();
    let sec1 = academics::create_section(
        &mut c,
        &hlc(),
        actor,
        academics::NewSection {
            school_id: school.id,
            class_id: c1.id,
            academic_year_id: year.id,
            name: "A".into(),
            class_teacher_id: None,
            capacity: None,
        },
    )
    .await
    .unwrap();
    let sec2 = academics::create_section(
        &mut c,
        &hlc(),
        actor,
        academics::NewSection {
            school_id: school.id,
            class_id: c2.id,
            academic_year_id: year.id,
            name: "A".into(),
            class_teacher_id: None,
            capacity: None,
        },
    )
    .await
    .unwrap();
    let admin = users::create(
        &mut c,
        &hlc(),
        actor,
        users::NewUser {
            school_id: school.id,
            username: "admin".into(),
            full_name: "Admin".into(),
            email: None,
            password_hash: hash_password("admin-password-1").unwrap(),
        },
    )
    .await
    .unwrap();
    let tuition = fees::create_head(&mut c, &hlc(), actor, school.id, "Tuition", "TUI", true, 0)
        .await
        .unwrap();
    let transport = fees::create_head(
        &mut c,
        &hlc(),
        actor,
        school.id,
        "Transport",
        "TRN",
        true,
        1,
    )
    .await
    .unwrap();
    let admission = fees::create_head(
        &mut c,
        &hlc(),
        actor,
        school.id,
        "Admission",
        "ADM",
        false,
        2,
    )
    .await
    .unwrap();
    // Class 1: tuition 2,500 + transport 800 + admission 5,000 (one-time). Class 2: tuition 3,333.33.
    fees::set_structure(
        &mut c,
        &hlc(),
        actor,
        school.id,
        year.id,
        c1.id,
        tuition.id,
        dec!(2500),
    )
    .await
    .unwrap();
    fees::set_structure(
        &mut c,
        &hlc(),
        actor,
        school.id,
        year.id,
        c1.id,
        transport.id,
        dec!(800),
    )
    .await
    .unwrap();
    fees::set_structure(
        &mut c,
        &hlc(),
        actor,
        school.id,
        year.id,
        c1.id,
        admission.id,
        dec!(5000),
    )
    .await
    .unwrap();
    fees::set_structure(
        &mut c,
        &hlc(),
        actor,
        school.id,
        year.id,
        c2.id,
        tuition.id,
        dec!(3333.33),
    )
    .await
    .unwrap();
    Fixture {
        school,
        year,
        sec1,
        sec2,
        admin: admin.id,
        tuition: tuition.id,
        transport: transport.id,
        admission: admission.id,
    }
}

async fn student(
    pool: &PgPool,
    f: &Fixture,
    name: &str,
    father: &str,
    section: &Section,
    admitted: NaiveDate,
) -> Uuid {
    let mut c = pool.acquire().await.unwrap();
    let actor = Actor::system_for(f.school.id);
    let s = students::create(
        &mut c,
        &hlc(),
        actor,
        students::NewStudent {
            school_id: f.school.id,
            student_number: None,
            legacy_id: None,
            full_name: name.into(),
            father_name: father.into(),
            date_of_birth: d(2015, 1, 1),
            gender: "male".into(),
            cnic: None,
            address: "".into(),
            admission_date: admitted,
            board_roll_no: None,
            board_registration_no: None,
            custom: json!({}),
        },
    )
    .await
    .unwrap();
    students::enrol(&mut c, &hlc(), actor, s.id, section.id, None)
        .await
        .unwrap();
    s.id
}

async fn link_father(pool: &PgPool, f: &Fixture, guardian: Uuid, student_id: Uuid) {
    let mut c = pool.acquire().await.unwrap();
    guardians::link(
        &mut c,
        &hlc(),
        Actor::system_for(f.school.id),
        student_id,
        guardian,
        "father",
        true,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn structure_upsert_and_policy_defaults() {
    let db: TestDb = test_db!();
    let f = fixture(&db.pool).await;
    let mut c = db.pool.acquire().await.unwrap();
    let actor = Actor::system_for(f.school.id);
    assert_eq!(
        fees::policy(&mut c, f.school.id).await.unwrap(),
        FeePolicy::default()
    );
    let custom = FeePolicy {
        sibling_discount_percent: dec!(15),
        sibling_rule: SiblingRule::SecondChildOnwards,
        late_fee_percent: dec!(5),
        due_day: 10,
    };
    fees::set_policy(&mut c, &hlc(), actor, f.school.id, &custom)
        .await
        .unwrap();
    assert_eq!(fees::policy(&mut c, f.school.id).await.unwrap(), custom);

    let rows = fees::list_structure(&mut c, f.year.id, Some(f.sec1.class_id))
        .await
        .unwrap();
    assert_eq!(rows.len(), 3);
    // Update, then zero removes.
    let upd = fees::set_structure(
        &mut c,
        &hlc(),
        actor,
        f.school.id,
        f.year.id,
        f.sec1.class_id,
        f.transport,
        dec!(900.005),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(upd.amount, dec!(900.01));
    assert!(fees::set_structure(
        &mut c,
        &hlc(),
        actor,
        f.school.id,
        f.year.id,
        f.sec1.class_id,
        f.transport,
        dec!(0)
    )
    .await
    .unwrap()
    .is_none());
    assert_eq!(
        fees::list_structure(&mut c, f.year.id, Some(f.sec1.class_id))
            .await
            .unwrap()
            .len(),
        2
    );
    assert!(matches!(
        fees::create_head(&mut c, &hlc(), actor, f.school.id, "Dup", "tui", true, 0).await,
        Err(DbError::Conflict(_))
    ));
    drop(c);
    db.cleanup().await;
}

#[tokio::test]
async fn generation_applies_structure_sibling_discount_concessions_and_is_idempotent() {
    let db: TestDb = test_db!();
    let f = fixture(&db.pool).await;
    let actor = Actor::system_for(f.school.id);
    let ali = student(&db.pool, &f, "Ali", "Ahmed", &f.sec1, d(2024, 4, 1)).await;
    let sara = student(&db.pool, &f, "Sara", "Ahmed", &f.sec2, d(2025, 4, 1)).await;
    let omar = student(&db.pool, &f, "Omar", "Tariq", &f.sec1, d(2026, 4, 1)).await;
    let mut c = db.pool.acquire().await.unwrap();
    let father = guardians::create(
        &mut c,
        &hlc(),
        actor,
        guardians::NewGuardian {
            school_id: f.school.id,
            full_name: "Ahmed".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    drop(c);
    link_father(&db.pool, &f, father.id, ali).await;
    link_father(&db.pool, &f, father.id, sara).await;
    let mut c = db.pool.acquire().await.unwrap();
    // Omar: 50% scholarship.
    fees::create_concession(
        &mut c,
        &hlc(),
        actor,
        fees::NewConcession {
            school_id: f.school.id,
            student_id: omar,
            kind: "scholarship".into(),
            percent: Some(dec!(50)),
            amount: None,
            reason: "Merit".into(),
            approved_by: f.admin,
            valid_from: d(2026, 4, 1),
            valid_to: None,
        },
    )
    .await
    .unwrap();

    let report = fees::generate_vouchers(
        &mut c,
        &hlc(),
        actor,
        f.school.id,
        f.year.id,
        d(2026, 4, 1),
        None,
        d(2026, 4, 1),
    )
    .await
    .unwrap();
    assert_eq!((report.created, report.skipped_existing), (3, 0));
    let again = fees::generate_vouchers(
        &mut c,
        &hlc(),
        actor,
        f.school.id,
        f.year.id,
        d(2026, 4, 1),
        None,
        d(2026, 4, 1),
    )
    .await
    .unwrap();
    assert_eq!(
        (again.created, again.skipped_existing),
        (0, 3),
        "idempotent"
    );

    let v_ali = fees::vouchers_for_student(&mut c, ali)
        .await
        .unwrap()
        .remove(0);
    // Class 1 first voucher: 2,500 + 800 + 5,000 admission = 8,300; sibling 10% = 830.
    assert_eq!(v_ali.gross, dec!(8300));
    assert_eq!(v_ali.sibling_discount, dec!(830));
    assert_eq!(v_ali.net_monthly, dec!(7470));
    assert_eq!(v_ali.total_due, dec!(7470));
    assert_eq!(v_ali.due_date, d(2026, 4, 15));
    assert_eq!(v_ali.voucher_no, "V202604-00001");
    assert_eq!(v_ali.status, "unpaid");
    let lines = fees::voucher_lines(&mut c, v_ali.id).await.unwrap();
    assert_eq!(
        lines
            .iter()
            .map(|l| (l.description.as_str(), l.amount))
            .collect::<Vec<_>>(),
        [
            ("Tuition", dec!(2500)),
            ("Transport", dec!(800)),
            ("Admission", dec!(5000))
        ]
    );
    // Sara (class 2): 3,333.33, sibling 333.33.
    let v_sara = fees::vouchers_for_student(&mut c, sara)
        .await
        .unwrap()
        .remove(0);
    assert_eq!(
        (v_sara.gross, v_sara.sibling_discount, v_sara.net_monthly),
        (dec!(3333.33), dec!(333.33), dec!(3000.00))
    );
    // Omar: no sibling, 50% concession on 8,300 = 4,150.
    let v_omar = fees::vouchers_for_student(&mut c, omar)
        .await
        .unwrap()
        .remove(0);
    assert_eq!(
        (
            v_omar.sibling_discount,
            v_omar.concession_discount,
            v_omar.net_monthly
        ),
        (dec!(0), dec!(4150), dec!(4150))
    );

    // May: admission (one-time) is not billed again; Ali's April is unpaid → arrears + late fee.
    let report = fees::generate_vouchers(
        &mut c,
        &hlc(),
        actor,
        f.school.id,
        f.year.id,
        d(2026, 5, 1),
        Some(&[f.sec1.id]),
        d(2026, 5, 1),
    )
    .await
    .unwrap();
    assert_eq!(report.created, 2, "only section 1A");
    let v_ali_may = fees::vouchers_for_student(&mut c, ali)
        .await
        .unwrap()
        .remove(0);
    assert_eq!(v_ali_may.billing_month, d(2026, 5, 1));
    assert_eq!(v_ali_may.gross, dec!(3300));
    assert_eq!(v_ali_may.sibling_discount, dec!(330));
    // April late fee assessed at generation: 10% of 7,470 = 747 → arrears 8,217.
    assert_eq!(v_ali_may.arrears, dec!(8217));
    assert_eq!(v_ali_may.total_due, dec!(2970) + dec!(8217));
    let v_ali_apr = fees::get_voucher(&mut c, v_ali.id).await.unwrap();
    assert_eq!(v_ali_apr.late_fee, dec!(747));
    assert_eq!(v_ali_apr.balance(), dec!(8217));
    assert_eq!(v_ali_apr.status, "carried", "balance moved into May");
    assert!(
        matches!(
            fees::record_payment(
                &mut c,
                &hlc(),
                actor,
                &FeePolicy::default(),
                fees::NewPayment {
                    voucher_id: v_ali_apr.id,
                    amount: dec!(100),
                    method: PaymentMethod::Cash,
                    reference: None,
                    note: None,
                    paid_on: d(2026, 5, 2),
                    received_by: f.admin
                }
            )
            .await,
            Err(DbError::Conflict(_))
        ),
        "carried vouchers are not payable"
    );
    assert_eq!(
        fees::student_balance(&mut c, ali).await.unwrap(),
        v_ali_may.total_due
    );
    assert_eq!(
        fees::ledger_totals(&mut c, ali).await.unwrap().balance,
        v_ali_may.total_due
    );

    // Second-child-onwards rule: Ali (admitted first) pays full, Sara still discounted.
    fees::set_policy(
        &mut c,
        &hlc(),
        actor,
        f.school.id,
        &FeePolicy {
            sibling_rule: SiblingRule::SecondChildOnwards,
            ..FeePolicy::default()
        },
    )
    .await
    .unwrap();
    fees::generate_vouchers(
        &mut c,
        &hlc(),
        actor,
        f.school.id,
        f.year.id,
        d(2026, 6, 1),
        None,
        d(2026, 6, 1),
    )
    .await
    .unwrap();
    let june_ali = fees::vouchers_for_student(&mut c, ali)
        .await
        .unwrap()
        .remove(0);
    let june_sara = fees::vouchers_for_student(&mut c, sara)
        .await
        .unwrap()
        .remove(0);
    assert_eq!(june_ali.sibling_discount, dec!(0));
    assert_eq!(june_sara.sibling_discount, dec!(333.33));
    assert_eq!(fees::sibling_rank(&mut c, ali).await.unwrap(), (1, 0));
    assert_eq!(fees::sibling_rank(&mut c, sara).await.unwrap(), (1, 1));
    assert_eq!(fees::sibling_rank(&mut c, omar).await.unwrap(), (0, 0));

    // Audit: every voucher creation and late-fee assessment is logged.
    let trail = audit::for_entity(&mut c, "fee_voucher", v_ali.id)
        .await
        .unwrap();
    assert_eq!(
        trail.iter().map(|e| e.action.as_str()).collect::<Vec<_>>(),
        ["create", "update", "update"]
    );
    assert_eq!(trail[1].new_value.as_ref().unwrap()["late_fee"], "747.00");
    assert_eq!(trail[2].new_value.as_ref().unwrap()["status"], "carried");
    drop(c);
    db.cleanup().await;
}

#[tokio::test]
async fn payments_partial_late_fee_waiver_reversal_and_immutability() {
    let db: TestDb = test_db!();
    let f = fixture(&db.pool).await;
    let actor = Actor::user(f.admin, f.school.id);
    let omar = student(&db.pool, &f, "Omar", "Tariq", &f.sec2, d(2026, 4, 1)).await;
    let mut c = db.pool.acquire().await.unwrap();
    let policy = FeePolicy::default();
    fees::generate_vouchers(
        &mut c,
        &hlc(),
        actor,
        f.school.id,
        f.year.id,
        d(2026, 4, 1),
        None,
        d(2026, 4, 1),
    )
    .await
    .unwrap();
    let v = fees::vouchers_for_student(&mut c, omar)
        .await
        .unwrap()
        .remove(0);
    assert_eq!(v.total_due, dec!(3333.33));

    // Partial payment on time.
    let (r1, v) = fees::record_payment(
        &mut c,
        &hlc(),
        actor,
        &policy,
        fees::NewPayment {
            voucher_id: v.id,
            amount: dec!(1000),
            method: PaymentMethod::Cash,
            reference: None,
            note: None,
            paid_on: d(2026, 4, 10),
            received_by: f.admin,
        },
    )
    .await
    .unwrap();
    assert_eq!(r1.receipt_no, "R2026-000001");
    assert_eq!(
        (v.net_paid, v.late_fee, v.status.as_str()),
        (dec!(1000), dec!(0), "partial")
    );
    assert_eq!(v.balance(), dec!(2333.33));

    // Over-payment refused; zero refused.
    assert!(matches!(
        fees::record_payment(
            &mut c,
            &hlc(),
            actor,
            &policy,
            fees::NewPayment {
                voucher_id: v.id,
                amount: dec!(5000),
                method: PaymentMethod::Cash,
                reference: None,
                note: None,
                paid_on: d(2026, 4, 10),
                received_by: f.admin
            }
        )
        .await,
        Err(DbError::Conflict(_))
    ));
    assert!(matches!(
        fees::record_payment(
            &mut c,
            &hlc(),
            actor,
            &policy,
            fees::NewPayment {
                voucher_id: v.id,
                amount: dec!(0),
                method: PaymentMethod::Cash,
                reference: None,
                note: None,
                paid_on: d(2026, 4, 10),
                received_by: f.admin
            }
        )
        .await,
        Err(DbError::Conflict(_))
    ));

    // Late payment assesses 10% of net monthly (3,333.33 → 333.33) once.
    let (r2, v) = fees::record_payment(
        &mut c,
        &hlc(),
        actor,
        &policy,
        fees::NewPayment {
            voucher_id: v.id,
            amount: dec!(500),
            method: PaymentMethod::Kuickpay,
            reference: Some("KP-77".into()),
            note: None,
            paid_on: d(2026, 4, 20),
            received_by: f.admin,
        },
    )
    .await
    .unwrap();
    assert_eq!(v.late_fee, dec!(333.33));
    assert_eq!(v.balance(), dec!(3333.33) + dec!(333.33) - dec!(1500));
    assert_eq!(r2.reference.as_deref(), Some("KP-77"));

    // Waiver zeroes the late fee and records who/why.
    let v = fees::waive_late_fee(&mut c, &hlc(), actor, v.id, "Family hardship", f.admin)
        .await
        .unwrap();
    assert_eq!(
        (
            v.late_fee,
            v.late_fee_waived,
            v.waiver_reason.as_deref(),
            v.waived_by
        ),
        (dec!(0), true, Some("Family hardship"), Some(f.admin))
    );
    assert_eq!(v.balance(), dec!(1833.33));
    // A later late payment does not re-assess once waived.
    let (_, v) = fees::record_payment(
        &mut c,
        &hlc(),
        actor,
        &policy,
        fees::NewPayment {
            voucher_id: v.id,
            amount: dec!(1833.33),
            method: PaymentMethod::Bank,
            reference: Some("TX1".into()),
            note: None,
            paid_on: d(2026, 5, 2),
            received_by: f.admin,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        (v.late_fee, v.status.as_str(), v.balance()),
        (dec!(0), "paid", dec!(0))
    );

    // Receipts are immutable.
    let upd = sqlx::query("UPDATE fee_receipts SET amount = 1 WHERE id = $1")
        .bind(r1.id)
        .execute(&mut *c)
        .await;
    assert!(upd.unwrap_err().to_string().contains("immutable"));
    let del = sqlx::query("DELETE FROM fee_receipts WHERE id = $1")
        .bind(r1.id)
        .execute(&mut *c)
        .await;
    assert!(del.unwrap_err().to_string().contains("immutable"));

    // Reversal: new negative receipt, voucher back to partial; cannot reverse twice or reverse a reversal.
    let (rev, v) = fees::reverse_receipt(
        &mut c,
        &hlc(),
        actor,
        r2.id,
        "Entered twice",
        f.admin,
        d(2026, 5, 3),
    )
    .await
    .unwrap();
    assert_eq!(
        (rev.amount, rev.reverses_id, rev.method.as_str()),
        (dec!(-500), Some(r2.id), "kuickpay")
    );
    assert_eq!(
        (v.net_paid, v.status.as_str(), v.balance()),
        (dec!(2833.33), "partial", dec!(500))
    );
    assert!(matches!(
        fees::reverse_receipt(
            &mut c,
            &hlc(),
            actor,
            r2.id,
            "again",
            f.admin,
            d(2026, 5, 3)
        )
        .await,
        Err(DbError::Conflict(_))
    ));
    assert!(matches!(
        fees::reverse_receipt(
            &mut c,
            &hlc(),
            actor,
            rev.id,
            "undo",
            f.admin,
            d(2026, 5, 3)
        )
        .await,
        Err(DbError::Conflict(_))
    ));
    assert!(matches!(
        fees::reverse_receipt(&mut c, &hlc(), actor, r1.id, "  ", f.admin, d(2026, 5, 3)).await,
        Err(DbError::Conflict(_))
    ));
    let original = fees::get_receipt(&mut c, r2.id).await.unwrap();
    assert_eq!(
        original.amount,
        dec!(500),
        "original untouched, reprintable"
    );
    assert_eq!(
        fees::receipts_for_voucher(&mut c, v.id)
            .await
            .unwrap()
            .len(),
        4
    );

    // Cancel refused while paid; ledger totals exact.
    assert!(matches!(
        fees::cancel_voucher(&mut c, &hlc(), actor, v.id, "oops").await,
        Err(DbError::Conflict(_))
    ));
    let t = fees::ledger_totals(&mut c, omar).await.unwrap();
    assert_eq!(
        (t.billed, t.discounts, t.fines, t.paid, t.balance),
        (dec!(3333.33), dec!(0), dec!(0), dec!(2833.33), dec!(500.00))
    );

    // Every money change is audited: receipts (3) + voucher updates.
    let receipt_events = audit::list_recent(&mut c, Some(f.school.id), 100)
        .await
        .unwrap()
        .into_iter()
        .filter(|e| e.entity == "fee_receipt")
        .count();
    assert_eq!(receipt_events, 4);
    let voucher_events: Vec<String> = audit::for_entity(&mut c, "fee_voucher", v.id)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.action)
        .collect();
    assert!(voucher_events.len() >= 6);
    drop(c);
    db.cleanup().await;
}

#[tokio::test]
async fn carry_forward_across_months_and_into_next_year_with_defaulters_and_collections() {
    let db: TestDb = test_db!();
    let f = fixture(&db.pool).await;
    let actor = Actor::user(f.admin, f.school.id);
    let omar = student(&db.pool, &f, "Omar", "Tariq", &f.sec2, d(2026, 4, 1)).await;
    let paid_kid = student(&db.pool, &f, "Zain", "Bilal", &f.sec2, d(2026, 4, 1)).await;
    let mut c = db.pool.acquire().await.unwrap();
    let policy = FeePolicy::default();
    // April and May unpaid for Omar; Zain pays April fully.
    fees::generate_vouchers(
        &mut c,
        &hlc(),
        actor,
        f.school.id,
        f.year.id,
        d(2026, 4, 1),
        None,
        d(2026, 4, 1),
    )
    .await
    .unwrap();
    let zain_apr = fees::vouchers_for_student(&mut c, paid_kid)
        .await
        .unwrap()
        .remove(0);
    fees::record_payment(
        &mut c,
        &hlc(),
        actor,
        &policy,
        fees::NewPayment {
            voucher_id: zain_apr.id,
            amount: dec!(3333.33),
            method: PaymentMethod::Cash,
            reference: None,
            note: None,
            paid_on: d(2026, 4, 5),
            received_by: f.admin,
        },
    )
    .await
    .unwrap();
    fees::generate_vouchers(
        &mut c,
        &hlc(),
        actor,
        f.school.id,
        f.year.id,
        d(2026, 5, 1),
        None,
        d(2026, 5, 1),
    )
    .await
    .unwrap();
    let may = fees::vouchers_for_student(&mut c, omar)
        .await
        .unwrap()
        .remove(0);
    // April late fee 333.33 → arrears 3,666.66; May total 3,333.33 + 3,666.66.
    assert_eq!(may.arrears, dec!(3666.66));
    assert_eq!(may.total_due, dec!(6999.99));
    let zain_may = fees::vouchers_for_student(&mut c, paid_kid)
        .await
        .unwrap()
        .remove(0);
    assert_eq!(zain_may.arrears, dec!(0));

    // Defaulters as of 1 June: Omar's April (carried, due 15 Apr) makes him 47 days overdue with
    // a balance equal to his ledger (May already carries April); Zain owes only May (17 days).
    let defaulters = fees::defaulters(&mut c, f.school.id, d(2026, 6, 1), None, 0)
        .await
        .unwrap();
    assert_eq!(defaulters.len(), 2);
    let omar_row = defaulters.iter().find(|x| x.student_id == omar).unwrap();
    assert_eq!(
        (
            omar_row.days_overdue,
            omar_row.unpaid_vouchers,
            omar_row.balance
        ),
        (47, 2, dec!(6999.99))
    );
    assert_eq!(
        fees::ledger_totals(&mut c, omar).await.unwrap().balance,
        dec!(6999.99)
    );
    assert_eq!(
        fees::student_balance(&mut c, omar).await.unwrap(),
        dec!(6999.99)
    );
    let zain_row = defaulters
        .iter()
        .find(|x| x.student_id == paid_kid)
        .unwrap();
    assert_eq!(
        (
            zain_row.days_overdue,
            zain_row.unpaid_vouchers,
            zain_row.balance
        ),
        (17, 1, dec!(3333.33))
    );
    let strict = fees::defaulters(&mut c, f.school.id, d(2026, 6, 1), None, 30)
        .await
        .unwrap();
    assert_eq!(strict.len(), 1);
    assert_eq!(strict[0].student_id, omar);
    let by_class = fees::defaulters(&mut c, f.school.id, d(2026, 6, 1), Some(f.sec1.class_id), 0)
        .await
        .unwrap();
    assert!(by_class.is_empty());
    // Paying May in full (incl. carried arrears) clears Zain-style... for Omar: pay 500 partial.
    let (_, _) = fees::record_payment(
        &mut c,
        &hlc(),
        actor,
        &policy,
        fees::NewPayment {
            voucher_id: may.id,
            amount: dec!(500),
            method: PaymentMethod::Cash,
            reference: None,
            note: None,
            paid_on: d(2026, 5, 10),
            received_by: f.admin,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        fees::student_balance(&mut c, omar).await.unwrap(),
        dec!(6499.99)
    );

    // Daily collection summary.
    let summary = fees::collection_summary(&mut c, f.school.id, d(2026, 4, 5))
        .await
        .unwrap();
    assert_eq!(summary.len(), 1);
    assert_eq!(
        (
            summary[0].method.as_str(),
            summary[0].receipts,
            summary[0].amount
        ),
        ("cash", 1, dec!(3333.33))
    );
    assert!(fees::collection_summary(&mut c, f.school.id, d(2026, 4, 6))
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        fees::receipts_on(&mut c, f.school.id, d(2026, 4, 5))
            .await
            .unwrap()
            .len(),
        1
    );

    // Next academic year: the first voucher carries the whole outstanding balance.
    let (name, start, end) = academics::default_year_bounds(2027, 4).unwrap();
    let y2 = academics::create_year(
        &mut c,
        &hlc(),
        actor,
        academics::NewAcademicYear {
            school_id: f.school.id,
            name,
            start_date: start,
            end_date: end,
            start_month: 4,
        },
    )
    .await
    .unwrap();
    academics::activate_year(&mut c, &hlc(), actor, y2.id)
        .await
        .unwrap();
    let sec2_y2 = academics::create_section(
        &mut c,
        &hlc(),
        actor,
        academics::NewSection {
            school_id: f.school.id,
            class_id: f.sec2.class_id,
            academic_year_id: y2.id,
            name: "A".into(),
            class_teacher_id: None,
            capacity: None,
        },
    )
    .await
    .unwrap();
    students::enrol(&mut c, &hlc(), actor, omar, sec2_y2.id, None)
        .await
        .unwrap();
    fees::set_structure(
        &mut c,
        &hlc(),
        actor,
        f.school.id,
        y2.id,
        f.sec2.class_id,
        f.tuition,
        dec!(3600),
    )
    .await
    .unwrap();
    let rep = fees::generate_vouchers(
        &mut c,
        &hlc(),
        actor,
        f.school.id,
        y2.id,
        d(2027, 4, 1),
        None,
        d(2027, 4, 1),
    )
    .await
    .unwrap();
    assert_eq!(rep.created, 1, "only Omar is enrolled in the new year");
    let apr27 = fees::vouchers_for_student(&mut c, omar)
        .await
        .unwrap()
        .remove(0);
    // May's late fee (333.33 on net monthly 3,333.33) is assessed now that it is overdue and
    // unpaid: outstanding = May 6,999.99 + 333.33 − 500 paid = 6,833.32, carried as arrears.
    assert_eq!(apr27.arrears, dec!(6833.32));
    assert_eq!(apr27.total_due, dec!(3600) + dec!(6833.32));
    // Ledger: billed 3,333.33 × 2 + 3,600; fines 333.33 × 2; paid 500.
    let t = fees::ledger_totals(&mut c, omar).await.unwrap();
    assert_eq!(
        (t.billed, t.fines, t.paid),
        (dec!(10266.66), dec!(666.66), dec!(500))
    );
    assert_eq!(t.balance, dec!(10433.32));
    assert_eq!(
        fees::student_balance(&mut c, omar).await.unwrap(),
        t.balance,
        "open balance equals the ledger"
    );
    let chain: Vec<(Decimal, String)> = sqlx::query(
        "SELECT arrears, status FROM fee_vouchers WHERE student_id = $1 ORDER BY billing_month",
    )
    .bind(omar)
    .fetch_all(&mut *c)
    .await
    .unwrap()
    .into_iter()
    .map(|r| (r.get::<Decimal, _>(0), r.get::<String, _>(1)))
    .collect();
    assert_eq!(
        chain,
        [
            (dec!(0), "carried".to_string()),
            (dec!(3666.66), "carried".to_string()),
            (dec!(6833.32), "unpaid".to_string())
        ]
    );
    // Paying the latest voucher in full settles everything.
    fees::record_payment(
        &mut c,
        &hlc(),
        actor,
        &policy,
        fees::NewPayment {
            voucher_id: apr27.id,
            amount: dec!(10433.32),
            method: PaymentMethod::Bank,
            reference: Some("TX9".into()),
            note: None,
            paid_on: d(2027, 4, 5),
            received_by: f.admin,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        fees::ledger_totals(&mut c, omar).await.unwrap().balance,
        dec!(0)
    );
    let later = fees::defaulters(&mut c, f.school.id, d(2027, 5, 1), None, 0)
        .await
        .unwrap();
    assert!(
        later.iter().all(|x| x.student_id != omar),
        "Omar is settled"
    );
    assert_eq!(later.len(), 1, "Zain still owes May 2026");
    drop(c);
    db.cleanup().await;
}
