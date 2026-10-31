//! Performance check for the pilot profile: 1,200 students, 120 staff, a full year of fees
//! and attendance. Ignored by default (it loads for a few minutes); run with
//! `cargo test -p madrasa-demo-data --test perf -- --ignored --nocapture`.
//! Each key query must answer in under one second. Results are recorded in docs/PROGRESS.md.

use chrono::Datelike;
use madrasa_core::hlc::Hlc;
use madrasa_db::testing::TestDb;
use madrasa_db::{academics, attendance, exams, fees, schools, students, test_db};
use madrasa_demo_data::generate::{generate, DemoOptions};
use madrasa_demo_data::load::{load, LoadOptions};
use std::time::{Duration, Instant};

async fn timed<F, T>(label: &str, budget: Duration, f: F) -> T
where
    F: std::future::Future<Output = T>,
{
    let start = Instant::now();
    let out = f.await;
    let took = start.elapsed();
    println!("{label:<48} {:>7.1} ms", took.as_secs_f64() * 1000.0);
    assert!(took < budget, "{label} took {took:?}, budget {budget:?}");
    out
}

#[tokio::test]
#[ignore]
async fn key_queries_under_one_second_with_a_full_year_of_data() {
    let db: TestDb = test_db!();
    let demo = generate(&DemoOptions::default());
    let start = Instant::now();
    let report = load(
        &db.pool,
        &Hlc::new(3),
        &demo,
        &LoadOptions {
            confirmed: true,
            out_dir: None,
            full_year: true,
        },
    )
    .await
    .unwrap();
    println!(
        "loaded {} students, {} staff, {} vouchers, {} receipts, {} attendance sheets, {} mark sheets in {:.1} s",
        report.students,
        report.staff_accounts,
        report.vouchers,
        report.receipts,
        report.attendance_sheets,
        report.mark_sheets,
        start.elapsed().as_secs_f64()
    );
    let mut c = db.pool.acquire().await.unwrap();
    let school = schools::get(&mut c, report.school_id).await.unwrap();
    let year = academics::active_year(&mut c, school.id)
        .await
        .unwrap()
        .unwrap();
    let sections = academics::list_sections(&mut c, school.id, Some(year.id), None)
        .await
        .unwrap();
    let section = sections[5].clone();
    let budget = Duration::from_secs(1);
    let today = chrono::Utc::now().date_naive();
    let month =
        chrono::NaiveDate::from_ymd_opt(today.year_ce().1 as i32, today.month0() + 1, 1).unwrap();

    let page = timed(
        "students directory, page 1 of 50",
        budget,
        students::list(
            &mut c,
            school.id,
            &students::StudentFilter::default(),
            students::StudentSort::Name,
            false,
            1,
            50,
        ),
    )
    .await
    .unwrap();
    assert_eq!(page.total, 1200);
    timed(
        "students directory, search by name",
        budget,
        students::list(
            &mut c,
            school.id,
            &students::StudentFilter {
                q: Some("Ahmed".into()),
                ..Default::default()
            },
            students::StudentSort::Name,
            false,
            1,
            50,
        ),
    )
    .await
    .unwrap();
    let vouchers = timed(
        "fee vouchers for a month (all classes)",
        budget,
        fees::list_vouchers(
            &mut c,
            school.id,
            &fees::VoucherFilter {
                billing_month: Some(month),
                class_id: None,
                section_id: None,
                status: None,
                student_id: None,
                q: None,
            },
            5000,
        ),
    )
    .await
    .unwrap();
    assert!(vouchers.len() >= 1000, "{}", vouchers.len());
    timed(
        "defaulters as of today",
        budget,
        fees::defaulters(&mut c, school.id, today, None, 0),
    )
    .await
    .unwrap();
    let student_id = page.items[0].id;
    timed(
        "student fee ledger totals",
        budget,
        fees::ledger_totals(&mut c, student_id),
    )
    .await
    .unwrap();
    timed(
        "student vouchers",
        budget,
        fees::vouchers_for_student(&mut c, student_id),
    )
    .await
    .unwrap();
    timed(
        "daily collection summary",
        budget,
        fees::collection_summary(&mut c, school.id, today),
    )
    .await
    .unwrap();
    timed(
        "attendance status of every section today",
        budget,
        attendance::daily_status(&mut c, school.id, year.id, today, None),
    )
    .await
    .unwrap();
    timed(
        "attendance register entries for a section/month",
        budget,
        attendance::section_entries(&mut c, section.id, month, today),
    )
    .await
    .unwrap();
    timed(
        "absentees today",
        budget,
        attendance::absentees(&mut c, school.id, today, None, None),
    )
    .await
    .unwrap();
    timed(
        "student attendance summary for the year",
        budget,
        attendance::student_summary(&mut c, student_id, year.start_date, year.end_date),
    )
    .await
    .unwrap();
    let assessments = exams::list_assessments(&mut c, year.id).await.unwrap();
    timed(
        "section marks for an assessment",
        budget,
        exams::section_marks(&mut c, assessments[0].id, section.id),
    )
    .await
    .unwrap();
    timed(
        "student marks for the year",
        budget,
        exams::student_marks(&mut c, student_id, year.id),
    )
    .await
    .unwrap();
    timed(
        "backup dump (whole database)",
        Duration::from_secs(30),
        madrasa_db::backup::dump(&mut c),
    )
    .await
    .unwrap();
}
