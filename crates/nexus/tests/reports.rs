//! Stage 5: network-wide reports over two demo schools. Every figure the hub reports is
//! compared with what the school's own database says.

mod common;

use axum::http::{Method, StatusCode};
use common::{Hub, SchoolSim};
use madrasa_core::hlc::Hlc;
use madrasa_db::testing::TestDb;
use madrasa_db::{reports as school_reports, schools};
use madrasa_demo_data::{generate, load, DemoOptions, LoadOptions};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// A demo school in its own database, renamed so two of them can be told apart.
async fn demo_school(name: &str, code: &str, currency: &str) -> Option<SchoolSim> {
    let db = TestDb::new().await?;
    let hlc = Hlc::new(0x5c);
    let demo = generate(&DemoOptions {
        students: 40,
        staff: 40,
        ..DemoOptions::default()
    });
    let report = load(
        &db.pool,
        &hlc,
        &demo,
        &LoadOptions {
            confirmed: true,
            out_dir: None,
            full_year: false,
        },
    )
    .await
    .unwrap();
    sqlx::query("UPDATE schools SET name = $2, code = $3, currency = $4, hlc = $5 WHERE id = $1")
        .bind(report.school_id)
        .bind(name)
        .bind(code)
        .bind(currency)
        .bind(hlc.now().to_string())
        .execute(&db.pool)
        .await
        .unwrap();
    let mut c = db.pool.acquire().await.unwrap();
    let school = schools::get(&mut c, report.school_id).await.unwrap();
    drop(c);
    Some(SchoolSim {
        db,
        hlc,
        school,
        token: None,
        school_ref: None,
    })
}

async fn join(hub: &Hub, admin: &str, sim: &mut SchoolSim) {
    let code = hub.invite(admin, &sim.school.name).await;
    let (s, b) = sim.join(hub, &code).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let report = sim.sync(hub).await.unwrap();
    assert!(
        report.parked_at_hub.is_empty(),
        "{:?}",
        report.parked_at_hub
    );
}

async fn report(hub: &Hub, token: &str, path: &str) -> Value {
    let (s, b) = hub.get(path, token).await;
    assert_eq!(s, StatusCode::OK, "{path}: {b}");
    b
}

/// The rows of a report that belong to one school (the school is always the first column).
fn rows_of<'a>(r: &'a Value, school: &str) -> Vec<&'a Value> {
    r["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row[0] == school)
        .collect()
}

fn dec(v: &Value) -> Decimal {
    v.as_str()
        .unwrap_or_else(|| panic!("money is sent as a string: {v}"))
        .parse()
        .unwrap()
}

#[tokio::test]
async fn network_reports_match_each_schools_own_figures() {
    let hub = hub!();
    let admin = hub.setup().await;
    let (Some(mut a), Some(mut b), Some(mut gone)) = (
        demo_school("DEMO North Campus", "DEMO-N", "PKR").await,
        demo_school("DEMO South Campus", "DEMO-S", "USD").await,
        SchoolSim::new("Left School", "KHI-LEFT").await,
    ) else {
        return;
    };
    join(&hub, &admin, &mut a).await;
    join(&hub, &admin, &mut b).await;
    join(&hub, &admin, &mut gone).await;
    // The third school leaves: it must drop out of every figure.
    let (s, _) = hub
        .call(
            Method::POST,
            "/sync/leave",
            gone.token.as_deref(),
            Some(json!({})),
        )
        .await;
    assert_eq!(s, StatusCode::OK);

    // Who may read reports.
    hub.post("/users", &admin, json!({ "username": "sana", "full_name": "Sana", "password": "viewer-password-1", "permissions": ["nexus.reports"] })).await;
    hub.post("/users", &admin, json!({ "username": "bilal", "full_name": "Bilal", "password": "escrow-password-1", "permissions": ["nexus.escrow"] })).await;
    let viewer = hub.login("sana", "viewer-password-1").await;
    let other = hub.login("bilal", "escrow-password-1").await;
    for path in ["/reports", "/reports/summary"] {
        let (s, _) = hub.get(path, &other).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{path}");
        let (s, _) = hub.get(path, a.token.as_deref().unwrap()).await;
        assert_eq!(
            s,
            StatusCode::UNAUTHORIZED,
            "a school's key opens no report"
        );
    }
    let catalogue = report(&hub, &viewer, "/reports").await;
    let codes: Vec<&str> = catalogue
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["code"].as_str().unwrap())
        .collect();
    assert_eq!(
        codes,
        ["summary", "enrolment", "fees", "attendance", "sync"]
    );

    // ---- summary ------------------------------------------------------------------------------
    let summary = report(&hub, &viewer, "/reports/summary").await;
    assert_eq!(summary["schools"], 2, "the school that left is not counted");
    assert!(rows_of(&summary, "Left School").is_empty());
    let mut total_students = 0;
    for sim in [&a, &b] {
        let own: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM students WHERE status = 'active' AND deleted_at IS NULL),
                    (SELECT count(*) FROM staff WHERE status = 'active' AND deleted_at IS NULL)",
        )
        .fetch_one(&sim.db.pool)
        .await
        .unwrap();
        let row = rows_of(&summary, &sim.school.name)[0];
        assert_eq!(
            (row[1].as_str(), row[4].as_i64(), row[5].as_i64()),
            (Some(sim.school.code.as_str()), Some(own.0), Some(own.1)),
            "{row}"
        );
        assert_eq!(own.0, 40);
        assert!(row[7].is_string(), "last sync is known");
        total_students += own.0;
    }
    assert_eq!(summary["totals"][0][4], total_students);

    // ---- enrolment ----------------------------------------------------------------------------
    let enrolment = report(&hub, &viewer, "/reports/enrolment").await;
    let mut network_total = 0;
    for sim in [&a, &b] {
        let mut c = sim.db.pool.acquire().await.unwrap();
        let year: uuid::Uuid = sqlx::query_scalar("SELECT id FROM academic_years WHERE is_active")
            .fetch_one(&mut *c)
            .await
            .unwrap();
        // The school's own report, per section; the network report is per class.
        let own = school_reports::class_strength(&mut c, sim.school.id, year, None)
            .await
            .unwrap();
        let mut by_class: BTreeMap<String, (i64, i64, i64)> = BTreeMap::new();
        for r in own {
            let e = by_class.entry(r.class_name).or_default();
            e.0 += r.boys;
            e.1 += r.girls;
            e.2 += r.total;
        }
        let rows = rows_of(&enrolment, &sim.school.name);
        assert_eq!(rows.len(), by_class.len());
        for row in rows {
            let own = by_class[row[1].as_str().unwrap()];
            assert_eq!(
                (row[2].as_i64(), row[3].as_i64(), row[5].as_i64()),
                (Some(own.0), Some(own.1), Some(own.2)),
                "{row}"
            );
            network_total += own.2;
        }
    }
    assert_eq!(enrolment["totals"][0][5], network_total);
    assert_eq!(network_total, 80);

    // ---- fees ---------------------------------------------------------------------------------
    let fees = report(&hub, &viewer, "/reports/fees").await;
    let (from, to): (chrono::NaiveDate, chrono::NaiveDate) = (
        fees["from"].as_str().unwrap().parse().unwrap(),
        fees["to"].as_str().unwrap().parse().unwrap(),
    );
    for (sim, currency) in [(&a, "PKR"), (&b, "USD")] {
        let mut c = sim.db.pool.acquire().await.unwrap();
        let own = school_reports::collection_by_month(&mut c, sim.school.id, from, to)
            .await
            .unwrap();
        let rows = rows_of(&fees, &sim.school.name);
        assert!(
            !rows.is_empty(),
            "the demo school has fees in the last twelve months"
        );
        let collected: Decimal = rows.iter().map(|r| dec(&r[4])).sum();
        assert_eq!(
            collected,
            own.iter().map(|m| m.amount).sum::<Decimal>(),
            "{}: collected",
            sim.school.name
        );
        assert!(collected > Decimal::ZERO);
        for m in &own {
            let row = rows
                .iter()
                .find(|r| r[2] == json!(m.month))
                .unwrap_or_else(|| panic!("month {}", m.month));
            assert_eq!(
                (dec(&row[4]), row[5].as_i64()),
                (m.amount, Some(m.receipts))
            );
        }
        let (billed, outstanding): (Decimal, Decimal) = sqlx::query_as(
            "SELECT COALESCE(sum(net_monthly + late_fee) FILTER (WHERE status <> 'cancelled'), 0)::numeric(14,2),
                    COALESCE(sum(total_due + late_fee - net_paid) FILTER (WHERE status IN ('unpaid', 'partial')), 0)::numeric(14,2)
             FROM fee_vouchers WHERE deleted_at IS NULL AND billing_month BETWEEN date_trunc('month', $1::date) AND $2",
        )
        .bind(from)
        .bind(to)
        .fetch_one(&mut *c)
        .await
        .unwrap();
        assert_eq!(
            rows.iter().map(|r| dec(&r[3])).sum::<Decimal>(),
            billed,
            "{}: billed",
            sim.school.name
        );
        assert_eq!(
            rows.iter().map(|r| dec(&r[6])).sum::<Decimal>(),
            outstanding,
            "{}: outstanding",
            sim.school.name
        );
        assert!(rows.iter().all(|r| r[1] == currency));
        // One total per currency: this school's money is never added to the other's.
        let total = fees["totals"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t[1] == currency)
            .unwrap();
        assert_eq!(
            (dec(&total[3]), dec(&total[4]), dec(&total[6])),
            (billed, collected, outstanding)
        );
    }
    assert_eq!(fees["totals"].as_array().unwrap().len(), 2);
    assert!(fees["notes"].as_array().unwrap().iter().any(|n| n
        .as_str()
        .unwrap()
        .contains("never added across currencies")));

    // ---- attendance ---------------------------------------------------------------------------
    let attendance = report(&hub, &viewer, "/reports/attendance").await;
    let mut marked_everywhere = 0;
    for sim in [&a, &b] {
        let own: Vec<(chrono::NaiveDate, i64, i64)> = sqlx::query_as(
            "SELECT date_trunc('month', date)::date, count(*), count(*) FILTER (WHERE status IN ('present', 'late'))
             FROM attendance_entries WHERE deleted_at IS NULL AND date BETWEEN $1 AND $2 GROUP BY 1 ORDER BY 1",
        )
        .bind(from)
        .bind(to)
        .fetch_all(&sim.db.pool)
        .await
        .unwrap();
        let rows = rows_of(&attendance, &sim.school.name);
        assert_eq!(rows.len(), own.len());
        assert!(!own.is_empty(), "the demo school has attendance");
        for (row, (month, marked, here)) in rows.iter().zip(&own) {
            assert_eq!(row[1], json!(month));
            assert_eq!(row[2].as_i64(), Some(*marked));
            assert_eq!(row[3].as_i64().unwrap() + row[4].as_i64().unwrap(), *here);
            let pct = (*here as f64 * 1000.0 / *marked as f64).round() / 10.0;
            assert_eq!(row[7].as_f64(), Some(pct));
            marked_everywhere += marked;
        }
    }
    assert_eq!(attendance["totals"][0][2], marked_everywhere);

    // ---- sync health --------------------------------------------------------------------------
    let sync = report(&hub, &viewer, "/reports/sync").await;
    assert_eq!(
        sync["rows"].as_array().unwrap().len(),
        3,
        "every school in the register is listed"
    );
    let left = rows_of(&sync, "Left School")[0];
    assert_eq!(left[2], "Left");
    for sim in [&a, &b] {
        let row = rows_of(&sync, &sim.school.name)[0];
        assert_eq!(
            (row[2].as_str(), row[5].as_i64(), row[6].as_i64()),
            (Some("Joined"), Some(0), Some(0)),
            "{row}"
        );
    }

    // ---- one school, another language, downloads ----------------------------------------------
    let one = report(
        &hub,
        &viewer,
        &format!("/reports/enrolment?school_id={}", a.school_ref.unwrap()),
    )
    .await;
    assert_eq!(one["schools"], 1);
    assert!(rows_of(&one, &b.school.name).is_empty());
    assert_eq!(one["totals"][0][5], 40);
    let ur = report(&hub, &viewer, "/reports/summary?lang=ur").await;
    assert_eq!(ur["title"], "نیٹ ورک کا خلاصہ");
    assert_eq!(ur["columns"][0]["label"], "اسکول");
    assert_eq!(
        ur["rows"], summary["rows"],
        "only the words change with the language"
    );
    let (s, ctype, bytes) = hub.get_bytes("/reports/fees?format=csv", &viewer).await;
    assert_eq!(
        (s, ctype.as_str()),
        (StatusCode::OK, "text/csv; charset=utf-8")
    );
    assert_eq!(&bytes[..3], [0xEF, 0xBB, 0xBF]);
    let text = String::from_utf8(bytes[3..].to_vec()).unwrap();
    let mut lines = text.lines();
    assert_eq!(
        lines.next(),
        Some("School,Currency,Month,Billed,Collected,Receipts,Outstanding")
    );
    assert_eq!(
        text.lines().count(),
        1 + fees["rows"].as_array().unwrap().len() + 2,
        "heading, rows, two totals"
    );
    let (s, ctype, bytes) = hub
        .get_bytes("/reports/attendance?format=xlsx&lang=ur", &viewer)
        .await;
    assert_eq!(s, StatusCode::OK);
    assert!(ctype.contains("spreadsheetml"));
    assert_eq!(&bytes[..2], b"PK");

    // ---- what is refused ----------------------------------------------------------------------
    for (path, status, code) in [
        ("/reports/nope", StatusCode::NOT_FOUND, "not_found"),
        (
            "/reports/fees?from=2026-05-01&to=2026-04-01",
            StatusCode::BAD_REQUEST,
            "validation",
        ),
        (
            "/reports/fees?from=2020-01-01&to=2026-04-01",
            StatusCode::BAD_REQUEST,
            "validation",
        ),
        (
            "/reports/fees?from=yesterday",
            StatusCode::BAD_REQUEST,
            "validation",
        ),
        (
            "/reports/fees?format=pdf",
            StatusCode::BAD_REQUEST,
            "validation",
        ),
        (
            "/reports/summary?school_id=00000000-0000-7000-8000-000000000000",
            StatusCode::NOT_FOUND,
            "not_found",
        ),
    ] {
        let (s, e) = hub.get(path, &viewer).await;
        assert_eq!(
            (s, e["error"]["code"].as_str()),
            (status, Some(code)),
            "{path}: {e}"
        );
    }
    let left_ref = gone.school_ref.unwrap();
    let (s, _) = hub
        .get(&format!("/reports/summary?school_id={left_ref}"), &viewer)
        .await;
    assert_eq!(
        s,
        StatusCode::NOT_FOUND,
        "a school that left cannot be reported on"
    );

    // Every run is in the hub's audit log with who and what.
    let (_, audit) = hub.get("/audit?page_size=100", &admin).await;
    let runs: Vec<&Value> = audit
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["entity"] == "hub_report")
        .collect();
    assert!(runs.len() >= 8, "{}", runs.len());
    assert!(runs
        .iter()
        .any(|r| r["new_value"]["code"] == "fees" && r["new_value"]["format"] == "csv"));

    a.db.cleanup().await;
    b.db.cleanup().await;
    gone.db.cleanup().await;
    hub.finish().await;
}
