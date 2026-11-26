//! Pilot onboarding: bilingual templates, guardians and fee structure imports, the dry-run
//! report, opening balances on the first vouchers. Need PostgreSQL via
//! `MADRASA_TEST_DATABASE_URL`.

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use calamine::Reader;
use http_body_util::BodyExt;
use madrasa_api::{router, AppState};
use madrasa_db::testing::TestDb;
use serde_json::{json, Value};
use tower::ServiceExt;

struct App {
    db: Option<TestDb>,
    state: AppState,
    app: Router,
}

impl App {
    async fn new() -> Option<App> {
        let db = TestDb::new().await?;
        let state = AppState::new(db.pool.clone()).await.unwrap();
        let app = router(state.clone());
        Some(App {
            db: Some(db),
            state,
            app,
        })
    }
    async fn raw(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Vec<u8>) {
        let mut req = Request::builder()
            .method(method)
            .uri(format!("/api/v1{path}"));
        if let Some(t) = token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
        }
        let req = match body {
            Some(b) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string())),
            None => req.body(Body::empty()),
        }
        .unwrap();
        let res = self.app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        (
            status,
            res.into_body().collect().await.unwrap().to_bytes().to_vec(),
        )
    }
    async fn call(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let (status, bytes) = self.raw(method, path, token, body).await;
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
    async fn get(&self, p: &str, t: &str) -> (StatusCode, Value) {
        self.call(Method::GET, p, Some(t), None).await
    }
    async fn post(&self, p: &str, t: &str, b: Value) -> (StatusCode, Value) {
        self.call(Method::POST, p, Some(t), Some(b)).await
    }
    /// Setup, admin login, year 2026-27 active, class 1 sections A and B, class 2 section A.
    async fn seed(&self) -> (String, Value) {
        let (s, b) = self.call(Method::POST, "/setup", None, Some(json!({ "school_name": "Gulshan", "school_code": "KHI-MC", "academic_year_start_month": 4, "admin_full_name": "Ayesha", "admin_username": "ayesha", "admin_password": "admin-password-1", "backup_password": "backup-password-1" }))).await;
        assert_eq!(s, StatusCode::OK, "{b}");
        let (s, b) = self
            .call(
                Method::POST,
                "/auth/login",
                None,
                Some(json!({ "username": "ayesha", "password": "admin-password-1" })),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        let admin = b["token"].as_str().unwrap().to_string();
        let (_, year) = self
            .post(
                "/academic-years",
                &admin,
                json!({ "start_year": 2026, "start_month": 4, "activate": true }),
            )
            .await;
        let (_, c1) = self
            .post("/classes", &admin, json!({ "name": "1", "level": 1 }))
            .await;
        let (_, c2) = self
            .post("/classes", &admin, json!({ "name": "2", "level": 2 }))
            .await;
        let mut sections = Vec::new();
        for (class, name) in [(&c1, "A"), (&c1, "B"), (&c2, "A")] {
            let (s, sec) = self
                .post(
                    "/sections",
                    &admin,
                    json!({ "class_id": class["id"], "academic_year_id": year["id"], "name": name }),
                )
                .await;
            assert!(s.is_success(), "{sec}");
            sections.push(sec["id"].clone());
        }
        (
            admin,
            json!({ "year": year["id"], "c1": c1["id"], "c2": c2["id"], "s1a": sections[0], "s1b": sections[1], "s2a": sections[2] }),
        )
    }
    async fn student(
        &self,
        admin: &str,
        name: &str,
        father: &str,
        dob: &str,
        section: &Value,
    ) -> Value {
        let body = json!({ "full_name": name, "father_name": father, "date_of_birth": dob, "gender": "male", "admission_date": "2026-04-01", "section_id": section });
        let (s, b) = self.post("/students", admin, body).await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
        b
    }
    /// Parse a CSV as an import file of this kind and return the request to preview or apply.
    async fn request(&self, admin: &str, kind: &str, csv: &str) -> Value {
        let (s, parsed) = self
            .post("/imports/parse", admin, json!({ "kind": kind, "file_name": format!("{kind}.csv"), "content_base64": STANDARD.encode(csv) }))
            .await;
        assert_eq!(s, StatusCode::OK, "{parsed}");
        json!({ "kind": kind, "file_name": format!("{kind}.csv"), "headers": parsed["headers"], "rows": parsed["rows"], "mapping": parsed["mapping"] })
    }
    async fn count(&self, table: &str) -> i64 {
        let mut c = self.state.pool.acquire().await.unwrap();
        sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT count(*) FROM \"{table}\" WHERE deleted_at IS NULL"
        )))
        .fetch_one(&mut *c)
        .await
        .unwrap()
    }
    async fn finish(mut self) {
        if let Some(db) = self.db.take() {
            db.cleanup().await;
        }
    }
}

macro_rules! app {
    () => {
        match App::new().await {
            Some(a) => a,
            None => return,
        }
    };
}

fn sheet(bytes: &[u8], name: &str) -> Vec<Vec<String>> {
    let mut wb: calamine::Xlsx<_> =
        calamine::open_workbook_from_rs(std::io::Cursor::new(bytes.to_vec())).unwrap();
    wb.worksheet_range(name)
        .unwrap()
        .rows()
        .map(|r| r.iter().map(|c| c.to_string()).collect())
        .collect()
}

fn sheet_names(bytes: &[u8]) -> Vec<String> {
    let wb: calamine::Xlsx<_> =
        calamine::open_workbook_from_rs(std::io::Cursor::new(bytes.to_vec())).unwrap();
    wb.sheet_names().to_vec()
}

#[tokio::test]
async fn every_template_has_its_records_sheet_and_a_read_me_in_english_and_urdu_and_comes_back_readable(
) {
    let app = app!();
    let (admin, _) = app.seed().await;
    for (kind, records) in [
        ("students", "Students"),
        ("guardians", "Guardians"),
        ("staff", "Staff"),
        ("fee_structure", "Fee structure"),
        ("opening_balances", "Opening balances"),
    ] {
        let (s, bytes) = app
            .raw(
                Method::GET,
                &format!("/imports/templates/{kind}"),
                Some(&admin),
                None,
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{kind}");
        assert_eq!(
            sheet_names(&bytes),
            [records, "Read me", "ہدایات"],
            "{kind}"
        );

        let (_, fields) = app.get(&format!("/imports/fields/{kind}"), &admin).await;
        let fields = fields.as_array().unwrap();
        let data = sheet(&bytes, records);
        assert_eq!(data.len(), 2, "{kind}: headings and one example");
        assert_eq!(
            data[0],
            fields
                .iter()
                .map(|f| f["label"].as_str().unwrap().to_string())
                .collect::<Vec<_>>()
        );

        // English: instructions that mention the dry run, then every column with whether it
        // is required. Urdu: the same, in Urdu, laid out right to left.
        let read_me = sheet(&bytes, "Read me");
        let text = read_me
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("How to fill this file"), "{kind}");
        assert!(
            text.contains("Nothing is saved until you confirm"),
            "{kind}"
        );
        for f in fields {
            let label = f["label"].as_str().unwrap();
            let line = read_me
                .iter()
                .find(|r| r[0] == label)
                .unwrap_or_else(|| panic!("{kind}: {label}"));
            assert_eq!(line[1] == "Yes", f["required"] == true, "{kind}: {label}");
        }
        let urdu = sheet(&bytes, "ہدایات");
        let text = urdu
            .iter()
            .flatten()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("یہ فائل کیسے بھریں"), "{kind}");
        assert!(
            text.contains("آپ کی تصدیق تک کچھ محفوظ نہیں ہوتا"),
            "{kind}"
        );
        assert_eq!(
            urdu.iter()
                .filter(|r| fields.iter().any(|f| f["label"] == r[3].as_str()))
                .count(),
            fields.len(),
            "{kind}: every column is explained in Urdu"
        );
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes.clone())).unwrap();
        let mut xml = String::new();
        std::io::Read::read_to_string(
            &mut zip.by_name("xl/worksheets/sheet3.xml").unwrap(),
            &mut xml,
        )
        .unwrap();
        assert!(
            xml.contains("rightToLeft=\"1\""),
            "{kind}: the Urdu sheet reads right to left"
        );

        // The template, filled or not, comes back: the records sheet is the one that is read
        // (not the read-me) and every column finds its field.
        let (s, parsed) = app
            .post("/imports/parse", &admin, json!({ "kind": kind, "file_name": "t.xlsx", "content_base64": STANDARD.encode(&bytes) }))
            .await;
        assert_eq!(s, StatusCode::OK, "{parsed}");
        assert_eq!(parsed["sheet"], records, "{kind}");
        assert_eq!(
            parsed["mapping"].as_object().unwrap().len(),
            fields.len(),
            "{kind}: {}",
            parsed["mapping"]
        );
    }
    // The plain CSV template is still the headings and the example.
    let (s, bytes) = app
        .raw(
            Method::GET,
            "/imports/templates/guardians?format=csv",
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert!(String::from_utf8_lossy(&bytes).contains("Guardian name"));
    app.finish().await;
}

#[tokio::test]
async fn the_dry_run_says_what_will_happen_and_why_and_writes_nothing() {
    let app = app!();
    let (admin, ids) = app.seed().await;
    let ali = app
        .student(&admin, "Ali Raza", "Imran Raza", "2015-03-14", &ids["s1a"])
        .await;
    let before = (
        app.count("students").await,
        app.count("guardians").await,
        app.count("import_batches").await,
    );

    let csv = "Student name,Father's name,Date of birth,Gender,Class,Section\n\
        Ali Raza,Imran Raza,2015-03-14,male,1,A\n\
        Ali Raza,Imran Raza,2016-01-01,male,1,A\n\
        Ali Raza,Kamran Raza,2015-03-14,male,1,B\n\
        Sara Khan,Bilal Khan,2015-05-05,female,7,A\n\
        Hina Khan,Bilal Khan,2014-05-05,female,7,A\n\
        Omar Shah,Asif Shah,2015-06-06,male,2,C\n\
        Zara Shah,Asif Shah,2015-06-06,female,2,A\n\
        Zara Shah,Asif Shah,2015-06-06,female,2,A\n\
        ,Nobody,2015-01-01,male,1,A\n";
    let body = app.request(&admin, "students", csv).await;
    let (s, pv) = app.post("/imports/preview", &admin, body.clone()).await;
    assert_eq!(s, StatusCode::OK, "{pv}");
    assert_eq!(
        pv["summary"],
        json!({ "total": 9, "create": 3, "update": 0, "skip": 1, "error": 5 }),
        "{}",
        pv["rows"]
    );
    let report = &pv["report"];

    // Missing classes and sections, each once, with the rows that need them.
    assert_eq!(
        report["missing_classes"],
        json!([{ "name": "7", "rows": [5, 6] }])
    );
    assert_eq!(
        report["missing_sections"],
        json!([{ "name": "2-C", "rows": [7] }])
    );

    // Likely duplicates: the same name with the same father (another birthday), and the
    // same name and birthday with another father. Both would still be imported.
    let alike = report["likely_duplicates"].as_array().unwrap();
    assert_eq!(alike.len(), 2, "{alike:?}");
    assert_eq!(alike[0]["row_no"], 3);
    assert!(
        alike[0]["looks_like"]
            .as_str()
            .unwrap()
            .contains("same father's name"),
        "{alike:?}"
    );
    assert!(alike[0]["looks_like"]
        .as_str()
        .unwrap()
        .contains(ali["student_number"].as_str().unwrap()));
    assert_eq!(alike[1]["row_no"], 4);
    assert!(alike[1]["looks_like"]
        .as_str()
        .unwrap()
        .contains("same date of birth"));
    assert_eq!(pv["rows"][1]["action"], "create");
    assert!(pv["rows"][1]["warnings"][0]["message"]
        .as_str()
        .unwrap()
        .starts_with("Looks like Ali Raza"));

    // Why rows are skipped or refused: one line per reason, most rows first.
    let reasons: Vec<(String, String, u64)> = report["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["action"].as_str().unwrap().into(),
                r["reason"].as_str().unwrap().into(),
                r["count"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        reasons[0],
        (
            "error".into(),
            "Class \"7\" is not set up in Settings".into(),
            2
        )
    );
    assert!(
        reasons.contains(&("skip".into(), "Already exists; skipped".into(), 1)),
        "{reasons:?}"
    );
    assert!(
        reasons.contains(&(
            "error".into(),
            "Section \"C\" of class 2 is not set up".into(),
            1
        )),
        "{reasons:?}"
    );
    assert!(
        reasons.contains(&(
            "error".into(),
            "Same student as an earlier row in this file".into(),
            1
        )),
        "{reasons:?}"
    );
    assert!(
        reasons.contains(&("error".into(), "Student name is required".into(), 1)),
        "{reasons:?}"
    );

    // The same as a workbook to keep.
    let (s, bytes) = app
        .raw(
            Method::POST,
            "/imports/report",
            Some(&admin),
            Some(body.clone()),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        sheet_names(&bytes),
        [
            "Summary",
            "Rows",
            "Missing classes, sections",
            "Likely duplicates"
        ]
    );
    let summary = sheet(&bytes, "Summary");
    let line = |label: &str| {
        summary
            .iter()
            .find(|r| r[0] == label)
            .unwrap_or_else(|| panic!("{label}"))[1]
            .clone()
    };
    assert_eq!(
        (
            line("Will be created"),
            line("Will be skipped"),
            line("Cannot be imported")
        ),
        ("3".into(), "1".into(), "5".into())
    );
    assert_eq!(line("Nothing has been saved"), "This is a dry run.");
    assert!(summary.iter().any(|r| r[0]
        == "Cannot be imported: Class \"7\" is not set up in Settings"
        && r[1] == "2"
        && r[2] == "5, 6"));
    let rows = sheet(&bytes, "Rows");
    assert_eq!(rows.len(), 10);
    assert_eq!(
        rows[1][..2],
        ["2".to_string(), "Will be skipped".to_string()]
    );
    assert_eq!(
        sheet(&bytes, "Missing classes, sections")[1][..2],
        ["Class".to_string(), "7".to_string()]
    );
    assert_eq!(sheet(&bytes, "Likely duplicates").len(), 3);

    // Nothing was written by any of it.
    assert_eq!(
        (
            app.count("students").await,
            app.count("guardians").await,
            app.count("import_batches").await
        ),
        before
    );

    // Confirmed: exactly what the dry run said.
    let (s, result) = app.post("/imports/apply", &admin, body).await;
    assert_eq!(s, StatusCode::OK, "{result}");
    assert_eq!(result["summary"], pv["summary"]);
    assert_eq!(app.count("students").await, before.0 + 3);
    app.finish().await;
}

#[tokio::test]
async fn guardians_are_imported_for_students_on_file_shared_by_siblings_and_undone() {
    let app = app!();
    let (admin, ids) = app.seed().await;
    let ali = app
        .student(&admin, "Ali Raza", "Imran Raza", "2015-03-14", &ids["s1a"])
        .await;
    let sana = app
        .student(&admin, "Sana Raza", "Imran Raza", "2016-02-02", &ids["s1b"])
        .await;
    let (s, _) = app
        .call(
            Method::PATCH,
            &format!("/students/{}", sana["id"].as_str().unwrap()),
            Some(&admin),
            Some(json!({ "legacy_id": "OLD-9" })),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let number = ali["student_number"].as_str().unwrap();

    let csv = format!(
        "Student number,Legacy ID,Guardian name,Relationship,Phone,CNIC,Primary contact\n\
         {number},,Imran Raza,Father,0300-1234567,42101-1234567-1,yes\n\
         ,OLD-9,Imran Raza,father,0300 1234567,,yes\n\
         {number},,Nadia Raza,mother,0301-7654321,,no\n\
         {number},,Nadia Raza,mother,0301-7654321,,no\n\
         NOPE-1,,Someone,father,0300-0000000,,\n\
         {number},,,father,0300-1111111,,\n"
    );
    let body = app.request(&admin, "guardians", &csv).await;
    let (s, pv) = app.post("/imports/preview", &admin, body.clone()).await;
    assert_eq!(s, StatusCode::OK, "{pv}");
    assert_eq!(
        pv["summary"],
        json!({ "total": 6, "create": 3, "update": 0, "skip": 0, "error": 3 }),
        "{}",
        pv["rows"]
    );
    assert!(pv["rows"][3]["errors"][0]["message"]
        .as_str()
        .unwrap()
        .contains("Same guardian and student as row 4"));
    assert_eq!(
        pv["rows"][4]["errors"][0]["message"],
        "No student with this number or legacy ID"
    );
    assert_eq!(pv["rows"][5]["errors"][0]["field"], "guardian_name");
    assert_eq!(app.count("guardians").await, 0, "a preview writes nothing");

    let (s, result) = app.post("/imports/apply", &admin, body.clone()).await;
    assert_eq!(s, StatusCode::OK, "{result}");
    // Two people, three links: the father is one record for both children.
    assert_eq!(app.count("guardians").await, 2);
    assert_eq!(app.count("student_guardians").await, 3);
    let (_, detail) = app
        .get(
            &format!("/students/{}", ali["id"].as_str().unwrap()),
            &admin,
        )
        .await;
    let guardians = detail["guardians"].as_array().unwrap();
    let father = guardians
        .iter()
        .find(|g| g["relationship"] == "father")
        .unwrap();
    assert_eq!(
        (father["is_primary"].clone(), father["phone"].clone()),
        (json!(true), json!("03001234567"))
    );
    let (_, detail) = app
        .get(
            &format!("/students/{}", sana["id"].as_str().unwrap()),
            &admin,
        )
        .await;
    assert_eq!(detail["guardians"][0]["id"], father["id"]);

    // The same file again: everyone is already linked, nothing is created twice.
    let (_, again) = app.post("/imports/preview", &admin, body).await;
    assert_eq!(again["summary"]["skip"], 3, "{}", again["rows"]);
    assert!(again["report"]["reasons"][0]["reason"]
        .as_str()
        .unwrap()
        .contains("is already a guardian of this student"));

    // Undone: the guardians and their links are gone, the students are untouched.
    let batch = result["batch"]["id"].as_str().unwrap();
    let (s, _) = app
        .post(
            &format!("/imports/batches/{batch}/rollback"),
            &admin,
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        (
            app.count("guardians").await,
            app.count("student_guardians").await,
            app.count("students").await
        ),
        (0, 0, 2)
    );
    app.finish().await;
}

#[tokio::test]
async fn the_fee_structure_is_imported_with_new_heads_updated_only_when_asked_and_undone() {
    let app = app!();
    let (admin, ids) = app.seed().await;
    let (s, tuition) = app
        .post(
            "/fees/heads",
            &admin,
            json!({ "name": "Tuition fee", "code": "TUITION", "is_recurring": true }),
        )
        .await;
    assert!(s.is_success(), "{tuition}");
    let (s, _) = app
        .call(Method::PUT, "/fees/structure", Some(&admin), Some(json!({ "academic_year_id": ids["year"], "class_id": ids["c1"], "lines": [{ "fee_head_id": tuition["id"], "amount": "2500" }] })))
        .await;
    assert_eq!(s, StatusCode::OK);
    let structure = |class: &'static str| {
        let app = &app;
        let admin = admin.clone();
        let ids = ids.clone();
        async move {
            let (_, lines) = app
                .get(
                    &format!(
                        "/fees/structure?academic_year_id={}&class_id={}",
                        ids["year"].as_str().unwrap(),
                        ids[class].as_str().unwrap()
                    ),
                    &admin,
                )
                .await;
            let (_, heads) = app.get("/fees/heads", &admin).await;
            let mut out: Vec<(String, String)> = lines
                .as_array()
                .unwrap()
                .iter()
                .map(|l| {
                    let head = heads
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|h| h["id"] == l["fee_head_id"])
                        .unwrap();
                    (
                        head["name"].as_str().unwrap().to_string(),
                        l["amount"].as_str().unwrap().to_string(),
                    )
                })
                .collect();
            out.sort();
            out
        }
    };

    let csv = "Class,Fee head,Amount\n\
        Class 1,Tuition fee,2500\n\
        1,Computer fee,400\n\
        Class 2,tuition fee,\"Rs 3,000\"\n\
        2,Computer fee,450.50\n\
        2,Computer fee,500\n\
        9,Tuition fee,2000\n\
        1,Exam fee,abc\n";
    let mut body = app.request(&admin, "fee_structure", csv).await;
    let (s, pv) = app.post("/imports/preview", &admin, body.clone()).await;
    assert_eq!(s, StatusCode::OK, "{pv}");
    assert_eq!(
        pv["summary"],
        json!({ "total": 7, "create": 3, "update": 0, "skip": 1, "error": 3 }),
        "{}",
        pv["rows"]
    );
    assert_eq!(
        pv["report"]["missing_classes"],
        json!([{ "name": "9", "rows": [7] }])
    );
    assert!(pv["rows"][0]["warnings"][0]["message"]
        .as_str()
        .unwrap()
        .starts_with("Already set to this amount"));
    assert!(pv["rows"][1]["warnings"][0]["message"]
        .as_str()
        .unwrap()
        .contains("will be created"));
    assert!(pv["rows"][4]["errors"][0]["message"]
        .as_str()
        .unwrap()
        .contains("Same class and fee head as row 5"));
    assert_eq!(app.count("fee_heads").await, 1, "a preview writes nothing");

    let (s, first) = app.post("/imports/apply", &admin, body.clone()).await;
    assert_eq!(s, StatusCode::OK, "{first}");
    assert_eq!(
        app.count("fee_heads").await,
        2,
        "one new head for both classes"
    );
    assert_eq!(
        structure("c1").await,
        [
            ("Computer fee".into(), "400.00".into()),
            ("Tuition fee".into(), "2500.00".into())
        ]
    );
    assert_eq!(
        structure("c2").await,
        [
            ("Computer fee".into(), "450.50".into()),
            ("Tuition fee".into(), "3000.00".into())
        ]
    );

    // A changed amount is left alone unless the person chooses to update.
    let csv = "Class,Fee head,Amount\n1,Tuition fee,2750\n1,Computer fee,0\n";
    body = app.request(&admin, "fee_structure", csv).await;
    let (_, pv) = app.post("/imports/preview", &admin, body.clone()).await;
    assert_eq!(pv["summary"]["skip"], 2, "{}", pv["rows"]);
    assert!(pv["rows"][0]["warnings"][0]["message"]
        .as_str()
        .unwrap()
        .contains("An amount is already set"));
    body["options"] = json!({ "duplicates": "update" });
    let (_, pv) = app.post("/imports/preview", &admin, body.clone()).await;
    assert_eq!(pv["summary"]["update"], 2, "{}", pv["rows"]);
    assert_eq!(
        pv["rows"][0]["matched"],
        "Tuition fee for class 1: now 2500.00"
    );
    let (s, second) = app.post("/imports/apply", &admin, body).await;
    assert_eq!(s, StatusCode::OK, "{second}");
    // 0 removes the head from the class.
    assert_eq!(
        structure("c1").await,
        [("Tuition fee".into(), "2750.00".into())]
    );

    // Undo the second import, then the first: back to where the school started.
    for result in [&second, &first] {
        let batch = result["batch"]["id"].as_str().unwrap();
        let (s, b) = app
            .post(
                &format!("/imports/batches/{batch}/rollback"),
                &admin,
                json!({}),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
    }
    assert_eq!(
        structure("c1").await,
        [("Tuition fee".into(), "2500.00".into())]
    );
    assert!(structure("c2").await.is_empty());
    assert_eq!(app.count("fee_heads").await, 1);
    app.finish().await;
}

#[tokio::test]
async fn an_accountant_imports_the_fee_structure_but_not_students() {
    use madrasa_core::password::hash_password;
    use madrasa_core::permissions::RoleCode;
    use madrasa_db::{roles, users, Actor};
    let app = app!();
    let (admin, _) = app.seed().await;
    let (_, me) = app.get("/auth/me", &admin).await;
    let sid: uuid::Uuid = me["user"]["school_id"].as_str().unwrap().parse().unwrap();
    {
        let mut c = app.state.pool.acquire().await.unwrap();
        let u = users::create(
            &mut c,
            &app.state.hlc,
            Actor::system_for(sid),
            users::NewUser {
                school_id: sid,
                username: "accounts".into(),
                full_name: "Accounts".into(),
                email: None,
                password_hash: hash_password("user-password-1").unwrap(),
            },
        )
        .await
        .unwrap();
        let r = roles::find_by_code(&mut c, sid, RoleCode::Accountant.code())
            .await
            .unwrap();
        roles::assign(&mut c, &app.state.hlc, Actor::system_for(sid), u.id, &r)
            .await
            .unwrap();
    }
    let (_, b) = app
        .call(
            Method::POST,
            "/auth/login",
            None,
            Some(json!({ "username": "accounts", "password": "user-password-1" })),
        )
        .await;
    let accountant = b["token"].as_str().unwrap().to_string();

    let (s, _) = app
        .raw(
            Method::GET,
            "/imports/templates/fee_structure",
            Some(&accountant),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let body = app
        .request(
            &accountant,
            "fee_structure",
            "Class,Fee head,Amount\n1,Tuition fee,2500\n",
        )
        .await;
    let (s, result) = app.post("/imports/apply", &accountant, body).await;
    assert_eq!(
        (s, result["summary"]["create"].clone()),
        (StatusCode::OK, json!(1)),
        "{result}"
    );

    for (method, path, body) in [
        (Method::GET, "/imports/templates/students", None),
        (Method::GET, "/imports/fields/guardians", None),
        (
            Method::POST,
            "/imports/parse",
            Some(
                json!({ "kind": "students", "file_name": "s.csv", "content_base64": STANDARD.encode("Name\nAli\n") }),
            ),
        ),
    ] {
        let (s, b) = app.call(method, path, Some(&accountant), body).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{path}: {b}");
    }
    app.finish().await;
}

#[tokio::test]
async fn an_opening_balance_shows_as_arrears_on_the_first_voucher() {
    let app = app!();
    let (admin, ids) = app.seed().await;
    let (_, tuition) = app
        .post(
            "/fees/heads",
            &admin,
            json!({ "name": "Tuition fee", "code": "TUITION", "is_recurring": true }),
        )
        .await;
    let (s, _) = app
        .call(Method::PUT, "/fees/structure", Some(&admin), Some(json!({ "academic_year_id": ids["year"], "class_id": ids["c1"], "lines": [{ "fee_head_id": tuition["id"], "amount": "2500" }] })))
        .await;
    assert_eq!(s, StatusCode::OK);
    let owes = app
        .student(&admin, "Ali Raza", "Imran Raza", "2015-03-14", &ids["s1a"])
        .await;
    let clear = app
        .student(
            &admin,
            "Bilal Khan",
            "Tariq Khan",
            "2015-04-04",
            &ids["s1a"],
        )
        .await;

    let csv = format!(
        "Student number,Opening balance,Note\n{},4500.50,From the 2025 register\n",
        owes["student_number"].as_str().unwrap()
    );
    let body = app.request(&admin, "opening_balances", &csv).await;
    let (s, result) = app.post("/imports/apply", &admin, body).await;
    assert_eq!(
        (s, result["summary"]["create"].clone()),
        (StatusCode::OK, json!(1)),
        "{result}"
    );

    // A late fee policy is in force: the opening balance must not earn one.
    let (_, mut policy) = app.get("/fees/policy", &admin).await;
    policy["late_fee_percent"] = json!("10");
    let (s, b) = app
        .call(Method::PUT, "/fees/policy", Some(&admin), Some(policy))
        .await;
    assert!(s.is_success(), "{b}");

    // The first monthly vouchers of the year, made well after the due day.
    let (s, generated) = app
        .post(
            "/fees/vouchers/generate",
            &admin,
            json!({ "month": "2026-04-01", "class_id": ids["c1"], "issue_date": "2026-04-25" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{generated}");
    // Both students get their April voucher: the opening balance takes no month's place.
    assert_eq!(
        (
            generated["created"].clone(),
            generated["skipped_existing"].clone()
        ),
        (json!(2), json!(0)),
        "{generated}"
    );
    let (_, vouchers) = app.get("/fees/vouchers?month=2026-04-01", &admin).await;
    let of = |student: &Value| {
        vouchers
            .as_array()
            .unwrap()
            .iter()
            .filter(|v| v["student_id"] == student["id"] && v["gross"] == "2500.00")
            .cloned()
            .collect::<Vec<_>>()
    };
    let first = &of(&owes)[0];
    assert_eq!(
        (first["arrears"].clone(), first["total_due"].clone()),
        (json!("4500.50"), json!("7000.50")),
        "{first}"
    );
    let other = &of(&clear)[0];
    assert_eq!(
        (other["arrears"].clone(), other["total_due"].clone()),
        (json!("0"), json!("2500.00")),
        "{other}"
    );

    // On the printed voucher, in words a parent can read.
    let (s, pdf) = app
        .raw(
            Method::GET,
            &format!(
                "/fees/vouchers/{}/pdf?lang=en",
                first["id"].as_str().unwrap()
            ),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert!(pdf.starts_with(b"%PDF"));

    // The ledger owes it once (the opening voucher is carried into April, not added again),
    // with no late fee on what came from the old register.
    let (_, ledger) = app
        .get(
            &format!("/fees/students/{}/ledger", owes["id"].as_str().unwrap()),
            &admin,
        )
        .await;
    assert_eq!(
        ledger["totals"]["balance"], "7000.50",
        "{}",
        ledger["totals"]
    );
    let opening = ledger["vouchers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["gross"] == "4500.50")
        .unwrap();
    assert_eq!(
        (
            opening["billing_month"].clone(),
            opening["status"].clone(),
            opening["late_fee"].clone()
        ),
        (json!("2026-03-01"), json!("carried"), json!("0")),
        "{opening}"
    );

    // Paid months later: what is left is April's own late fee (10% of 2,500), and nothing
    // on the 4,500.50 that came from the old register.
    let (s, receipt) = app
        .post(
            &format!("/fees/vouchers/{}/payments", first["id"].as_str().unwrap()),
            &admin,
            json!({ "amount": "7000.50", "method": "cash" }),
        )
        .await;
    assert!(s.is_success(), "{receipt}");
    let (_, ledger) = app
        .get(
            &format!("/fees/students/{}/ledger", owes["id"].as_str().unwrap()),
            &admin,
        )
        .await;
    assert_eq!(
        (
            ledger["totals"]["fines"].clone(),
            ledger["totals"]["balance"].clone()
        ),
        (json!("250.00"), json!("250.00")),
        "{}",
        ledger["totals"]
    );
    app.finish().await;
}

fn check<'a>(health: &'a Value, code: &str) -> &'a Value {
    health
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["code"] == code)
        .unwrap_or_else(|| panic!("no check {code} in {health}"))
}

#[tokio::test]
async fn data_health_finds_what_needs_a_person_and_is_quiet_about_what_is_in_order() {
    let app = app!();
    let (admin, ids) = app.seed().await;
    // In order: a class with its fees, a student with a guardian who has a phone.
    let (_, tuition) = app
        .post(
            "/fees/heads",
            &admin,
            json!({ "name": "Tuition fee", "code": "TUITION", "is_recurring": true }),
        )
        .await;
    let (s, _) = app
        .call(Method::PUT, "/fees/structure", Some(&admin), Some(json!({ "academic_year_id": ids["year"], "class_id": ids["c2"], "lines": [{ "fee_head_id": tuition["id"], "amount": "2500" }] })))
        .await;
    assert!(s.is_success());
    let fine = app
        .student(&admin, "Hina Shah", "Asif Shah", "2015-06-06", &ids["s2a"])
        .await;
    let (s, b) = app
        .post(&format!("/students/{}/guardians", fine["id"].as_str().unwrap()), &admin, json!({ "full_name": "Asif Shah", "phone": "03001234567", "relationship": "father", "is_primary": true }))
        .await;
    assert!(s.is_success(), "{b}");

    let (s, health) = app.get("/data-health", &admin).await;
    assert_eq!(s, StatusCode::OK, "{health}");
    for code in [
        "students_without_section",
        "students_without_guardian",
        "guardians_without_phone",
        "duplicate_student_names",
        "classes_without_fee_structure",
        "vouchers_without_fee_head",
    ] {
        assert_eq!(
            check(&health, code)["count"],
            0,
            "{code}: {}",
            check(&health, code)
        );
    }

    // Now what an import typically leaves behind.
    let twin_a = app
        .student(&admin, "Ali Raza", "Imran Raza", "2015-03-14", &ids["s1a"])
        .await;
    app.student(&admin, "ali  raza", "imran raza", "2016-01-01", &ids["s1b"])
        .await;
    let (s, b) = app
        .post(
            &format!("/students/{}/guardians", twin_a["id"].as_str().unwrap()),
            &admin,
            json!({ "full_name": "Imran Raza", "relationship": "father", "is_primary": true }),
        )
        .await;
    assert!(s.is_success(), "{b}");
    let body = json!({ "full_name": "Omar Khan", "father_name": "Bilal Khan", "date_of_birth": "1975-01-01", "gender": "male", "admission_date": "2026-04-01" });
    let (s, loose) = app.post("/students", &admin, body).await;
    assert_eq!(s, StatusCode::CREATED, "{loose}");

    let (_, health) = app.get("/data-health", &admin).await;
    let no_section = check(&health, "students_without_section");
    assert_eq!(
        (no_section["count"].clone(), no_section["severity"].clone()),
        (json!(1), json!("problem"))
    );
    assert_eq!(no_section["sample"][0]["label"], "Omar Khan");
    assert_eq!(
        no_section["sample"][0]["link"],
        format!("/students/{}", loose["id"].as_str().unwrap())
    );
    // Three children have no guardian at all (the twin's brother, and the one without a class).
    assert_eq!(
        check(&health, "students_without_guardian")["count"],
        2,
        "{}",
        check(&health, "students_without_guardian")
    );
    let no_phone = check(&health, "guardians_without_phone");
    assert_eq!(
        (
            no_phone["count"].clone(),
            no_phone["sample"][0]["label"].clone()
        ),
        (json!(1), json!("Imran Raza"))
    );
    let twins = check(&health, "duplicate_student_names");
    assert_eq!(
        (twins["count"].clone(), twins["severity"].clone()),
        (json!(1), json!("warning"))
    );
    assert!(twins["sample"][0]["detail"]
        .as_str()
        .unwrap()
        .contains(twin_a["student_number"].as_str().unwrap()));
    assert_eq!(
        check(&health, "doubtful_dates_of_birth")["sample"][0]["label"],
        "Omar Khan"
    );
    // Class 1 has children and no fees set; class 2 has both.
    let no_fees = check(&health, "classes_without_fee_structure");
    assert_eq!(
        (
            no_fees["count"].clone(),
            no_fees["sample"][0]["label"].clone(),
            no_fees["sample"][0]["detail"].clone()
        ),
        (json!(1), json!("Class 1"), json!("2 students"))
    );
    assert_eq!(check(&health, "sections_without_class_teacher")["count"], 3);
    app.finish().await;
}

#[tokio::test]
async fn export_everything_is_the_whole_school_readable_and_without_secrets() {
    use madrasa_core::password::hash_password;
    use madrasa_core::permissions::RoleCode;
    use madrasa_db::{roles, users, Actor};
    let app = app!();
    let (admin, ids) = app.seed().await;
    let (_, tuition) = app
        .post(
            "/fees/heads",
            &admin,
            json!({ "name": "Tuition fee", "code": "TUITION", "is_recurring": true }),
        )
        .await;
    app.call(Method::PUT, "/fees/structure", Some(&admin), Some(json!({ "academic_year_id": ids["year"], "class_id": ids["c1"], "lines": [{ "fee_head_id": tuition["id"], "amount": "2500" }] }))).await;
    let ali = app
        .student(&admin, "Ali Raza", "Imran Raza", "2015-03-14", &ids["s1a"])
        .await;
    app.post(&format!("/students/{}/guardians", ali["id"].as_str().unwrap()), &admin, json!({ "full_name": "Imran Raza", "phone": "03001234567", "relationship": "father", "is_primary": true })).await;
    let (s, generated) = app
        .post(
            "/fees/vouchers/generate",
            &admin,
            json!({ "month": "2026-04-01" }),
        )
        .await;
    assert_eq!(
        (s, generated["created"].clone()),
        (StatusCode::OK, json!(1))
    );
    let voucher = generated["voucher_ids"][0].as_str().unwrap();
    let (s, _) = app
        .post(
            &format!("/fees/vouchers/{voucher}/payments"),
            &admin,
            json!({ "amount": "1000", "method": "cash" }),
        )
        .await;
    assert!(s.is_success());
    app.post(
        "/attendance/calendar/holidays",
        &admin,
        json!({ "name": "Eid", "starts_on": "2026-05-27", "ends_on": "2026-05-29" }),
    )
    .await;

    let (s, bytes) = app
        .raw(Method::GET, "/export/everything", Some(&admin), None)
        .await;
    assert_eq!(s, StatusCode::OK);
    let names = sheet_names(&bytes);
    assert_eq!(names[0], "About this file");
    for wanted in [
        "Students",
        "Guardians",
        "Staff",
        "Logins",
        "Classes and sections",
        "Fee structure",
        "Fee vouchers",
        "Fee voucher lines",
        "Fee receipts",
        "Attendance",
        "Marks",
        "Settings",
        "data academic_years",
        "data classes",
        "data fee_heads",
        "data holidays",
        "data audit_log",
    ] {
        assert!(
            names.iter().any(|n| n == wanted),
            "{wanted} missing from {names:?}"
        );
    }
    for never in [
        "data sessions",
        "data users",
        "data settings",
        "data api_keys",
        "data sync_changes",
        "data idempotency_keys",
    ] {
        assert!(!names.iter().any(|n| n == never), "{never}");
    }

    let students = sheet(&bytes, "Students");
    assert_eq!(
        students[0][..3],
        [
            "Student number".to_string(),
            "Legacy ID".to_string(),
            "Student name".to_string()
        ]
    );
    let row = students.iter().find(|r| r[2] == "Ali Raza").unwrap();
    let col = |name: &str| students[0].iter().position(|h| h == name).unwrap();
    assert_eq!(
        (
            row[col("Class")].as_str(),
            row[col("Section")].as_str(),
            row[col("Date of birth")].as_str()
        ),
        ("1", "A", "2015-03-14")
    );
    let guardians = sheet(&bytes, "Guardians");
    // A phone number keeps its leading zero: it is text, not a number.
    assert!(guardians
        .iter()
        .any(|r| r[0] == "Imran Raza" && r[1] == "03001234567" && r[7] == "Ali Raza"));
    let receipts = sheet(&bytes, "Fee receipts");
    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts[1][5], "1000.00");
    assert!(sheet(&bytes, "Fee vouchers")
        .iter()
        .any(|r| r[2] == "Ali Raza" && r[3] == "2026-04"));
    assert!(sheet(&bytes, "data holidays")
        .iter()
        .any(|r| r.contains(&"Eid".to_string())));
    let about = sheet(&bytes, "About this file");
    assert!(about.iter().any(|r| r[0] == "School" && r[1] == "Gulshan"));
    assert!(about.iter().any(|r| r[0] == "Students" && r[1] == "1"));

    // No secret anywhere in the file: not a password hash, not the instance key, not the
    // backup keys.
    let secrets: Vec<String> = {
        let mut c = app.state.pool.acquire().await.unwrap();
        let mut out: Vec<String> = sqlx::query_scalar("SELECT password_hash FROM users")
            .fetch_all(&mut *c)
            .await
            .unwrap();
        let keys: Vec<serde_json::Value> = sqlx::query_scalar(
            "SELECT value FROM settings WHERE key IN ('instance.key', 'backup.keys')",
        )
        .fetch_all(&mut *c)
        .await
        .unwrap();
        assert!(!keys.is_empty());
        for k in keys {
            match k {
                serde_json::Value::String(s) => out.push(s),
                other => out.extend(
                    other
                        .as_object()
                        .unwrap()
                        .values()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .filter(|v| v.len() > 20),
                ),
            }
        }
        out
    };
    assert!(secrets.len() >= 2, "{}", secrets.len());
    for name in &names {
        for row in sheet(&bytes, name) {
            for cell in row {
                assert!(!cell.contains("$argon2"), "{name}: a password hash");
                assert!(
                    !secrets.iter().any(|s| cell.contains(s.as_str())),
                    "{name}: a secret"
                );
            }
        }
    }

    // It is written down who took everything, and a teacher cannot.
    let (_, log) = app.get("/audit?limit=20", &admin).await;
    assert!(log.to_string().contains("everything"), "{log}");
    let (_, me) = app.get("/auth/me", &admin).await;
    let sid: uuid::Uuid = me["user"]["school_id"].as_str().unwrap().parse().unwrap();
    {
        let mut c = app.state.pool.acquire().await.unwrap();
        let u = users::create(
            &mut c,
            &app.state.hlc,
            Actor::system_for(sid),
            users::NewUser {
                school_id: sid,
                username: "teacher".into(),
                full_name: "Teacher".into(),
                email: None,
                password_hash: hash_password("user-password-1").unwrap(),
            },
        )
        .await
        .unwrap();
        let r = roles::find_by_code(&mut c, sid, RoleCode::Teacher.code())
            .await
            .unwrap();
        roles::assign(&mut c, &app.state.hlc, Actor::system_for(sid), u.id, &r)
            .await
            .unwrap();
    }
    let (_, b) = app
        .call(
            Method::POST,
            "/auth/login",
            None,
            Some(json!({ "username": "teacher", "password": "user-password-1" })),
        )
        .await;
    let teacher = b["token"].as_str().unwrap().to_string();
    for path in ["/export/everything", "/data-health"] {
        let (s, _) = app.raw(Method::GET, path, Some(&teacher), None).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{path}");
    }
    app.finish().await;
}

/// The voucher check asks two plain questions (vouchers with no line; lines with no fee
/// head) instead of two per voucher, and the sections list counts its students in one
/// question: both must still find exactly what the slow forms found.
#[tokio::test]
async fn vouchers_without_a_fee_head_are_found_and_sections_count_their_students() {
    let app = app!();
    let (admin, ids) = app.seed().await;
    let (_, tuition) = app
        .post(
            "/fees/heads",
            &admin,
            json!({ "name": "Tuition fee", "code": "TUITION", "is_recurring": true }),
        )
        .await;
    for class in ["c1", "c2"] {
        app.call(Method::PUT, "/fees/structure", Some(&admin), Some(json!({ "academic_year_id": ids["year"], "class_id": ids[class], "lines": [{ "fee_head_id": tuition["id"], "amount": "2500" }] }))).await;
    }
    for (name, father, section) in [
        ("Ali Raza", "Imran Raza", "s1a"),
        ("Sara Khan", "Bilal Khan", "s1a"),
        ("Hina Shah", "Asif Shah", "s2a"),
        ("Omar Aziz", "Tariq Aziz", "s2a"),
    ] {
        app.student(&admin, name, father, "2015-03-14", &ids[section])
            .await;
    }
    // Someone who left is not counted in the section.
    let left = app
        .student(
            &admin,
            "Zain Malik",
            "Adil Malik",
            "2015-03-14",
            &ids["s1a"],
        )
        .await;
    let mut c = app.state.pool.acquire().await.unwrap();
    sqlx::query("UPDATE students SET status = 'left' WHERE id = $1::uuid")
        .bind(left["id"].as_str().unwrap())
        .execute(&mut *c)
        .await
        .unwrap();

    let (_, sections) = app.get("/sections", &admin).await;
    let count = |id: &Value| {
        sections
            .as_array()
            .unwrap()
            .iter()
            .find(|s| &s["id"] == id)
            .unwrap()["student_count"]
            .clone()
    };
    assert_eq!(
        (count(&ids["s1a"]), count(&ids["s1b"]), count(&ids["s2a"])),
        (json!(2), json!(0), json!(2))
    );

    let (s, generated) = app
        .post(
            "/fees/vouchers/generate",
            &admin,
            json!({ "month": "2026-04-01" }),
        )
        .await;
    assert_eq!(
        (s, generated["created"].clone()),
        (StatusCode::OK, json!(4))
    );
    let (_, health) = app.get("/data-health", &admin).await;
    assert_eq!(check(&health, "vouchers_without_fee_head")["count"], 0);

    let voucher = |i: usize| generated["voucher_ids"][i].as_str().unwrap().to_string();
    // One voucher has a line without a fee head, one has lost its lines altogether, and a
    // third is wrong in the same way but cancelled, so nobody needs to look at it.
    sqlx::query("UPDATE fee_voucher_lines SET fee_head_id = NULL WHERE voucher_id = $1::uuid")
        .bind(voucher(0))
        .execute(&mut *c)
        .await
        .unwrap();
    sqlx::query("UPDATE fee_voucher_lines SET deleted_at = now() WHERE voucher_id = $1::uuid")
        .bind(voucher(1))
        .execute(&mut *c)
        .await
        .unwrap();
    sqlx::query("UPDATE fee_voucher_lines SET fee_head_id = NULL WHERE voucher_id = $1::uuid")
        .bind(voucher(2))
        .execute(&mut *c)
        .await
        .unwrap();
    let (s, b) = app
        .post(
            &format!("/fees/vouchers/{}/cancel", voucher(2)),
            &admin,
            json!({ "reason": "made by mistake" }),
        )
        .await;
    assert!(s.is_success(), "{b}");
    drop(c);

    let (_, health) = app.get("/data-health", &admin).await;
    let found = check(&health, "vouchers_without_fee_head");
    assert_eq!(found["count"], 2, "{found}");
    let mut numbers: Vec<String> = Vec::new();
    for id in [voucher(0), voucher(1)] {
        let (_, v) = app.get(&format!("/fees/vouchers/{id}"), &admin).await;
        numbers.push(
            v["voucher"]["voucher_no"]
                .as_str()
                .or(v["voucher_no"].as_str())
                .unwrap()
                .to_string(),
        );
    }
    let mut labels: Vec<String> = found["sample"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["label"].as_str().unwrap().to_string())
        .collect();
    labels.sort();
    numbers.sort();
    assert_eq!(labels, numbers);
    assert!(found["sample"][0]["link"]
        .as_str()
        .unwrap()
        .ends_with("/print"));
    app.finish().await;
}

/// Found by the long-run test: someone closes the window while "Export everything" is being
/// made. The request is dropped at whatever line it had reached; the database connection it
/// held must go back to the pool clean. (It went back inside a read-only transaction, and
/// from then on every request that happened to get that connection failed.)
#[tokio::test]
async fn an_export_abandoned_half_way_leaves_every_connection_usable() {
    let app = app!();
    let (admin, ids) = app.seed().await;
    for i in 0..30 {
        app.student(
            &admin,
            &format!("Student {i}"),
            "Father",
            "2015-03-14",
            &ids["s1a"],
        )
        .await;
    }
    let school_id: uuid::Uuid = {
        let mut c = app.state.pool.acquire().await.unwrap();
        sqlx::query_scalar("SELECT id FROM schools LIMIT 1")
            .fetch_one(&mut *c)
            .await
            .unwrap()
    };
    // Abandoned at many different moments, some of them certainly in the middle.
    let mut abandoned = 0;
    for i in 0..40u64 {
        let cut = std::time::Duration::from_micros(300 * i);
        if tokio::time::timeout(
            cut,
            madrasa_api::everything::export(&app.state.pool, school_id),
        )
        .await
        .is_err()
        {
            abandoned += 1;
        }
    }
    assert!(abandoned >= 10, "only {abandoned} exports were cut short");
    // Every connection the pool holds, all at once: none is inside a transaction, and each
    // can still write.
    // (A transaction left open would have started a while ago by now.)
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let mut held = Vec::new();
    for _ in 0..app.state.pool.size().max(1) {
        held.push(app.state.pool.acquire().await.unwrap());
    }
    for c in &mut held {
        let (read_only, in_tx): (String, bool) = sqlx::query_as(
            "SELECT current_setting('transaction_read_only'), now() < clock_timestamp() - interval '150 milliseconds'",
        )
        .fetch_one(&mut **c)
        .await
        .unwrap();
        assert_eq!((read_only.as_str(), in_tx), ("off", false));
        sqlx::query("CREATE TEMP TABLE IF NOT EXISTS still_writable (n int)")
            .execute(&mut **c)
            .await
            .unwrap();
    }
    drop(held);
    // And the app works: a whole export, and a change.
    let (s, _) = app
        .raw(Method::GET, "/export/everything", Some(&admin), None)
        .await;
    assert_eq!(s, StatusCode::OK);
    app.student(&admin, "After", "Father", "2015-03-14", &ids["s1a"])
        .await;
    app.finish().await;
}

/// Found by the long-run test, with the one before. A request is carried out to its end
/// even when whoever sent it has gone (a closed window, a client PC switched off): the
/// alternative is a request dropped at whatever line it had reached.
#[tokio::test]
async fn a_request_whose_client_has_gone_is_still_carried_out() {
    use std::future::Future;
    let app = app!();
    let (admin, ids) = app.seed().await;
    for i in 0..8 {
        let body = json!({ "full_name": format!("Left Early {i}"), "father_name": "Father", "date_of_birth": "2015-03-14", "gender": "male", "admission_date": "2026-04-01", "section_id": ids["s1a"] });
        let req = Request::builder()
            .method(Method::POST)
            .uri("/api/v1/students")
            .header(header::AUTHORIZATION, format!("Bearer {admin}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let fut = app.app.clone().oneshot(req);
        tokio::pin!(fut);
        // The request has been taken up, no more: then the client is gone.
        let done =
            std::future::poll_fn(|cx| std::task::Poll::Ready(fut.as_mut().poll(cx).is_ready()))
                .await;
        assert!(
            !done,
            "the request was answered at once, so nothing was abandoned"
        );
    }
    let mut found = 0;
    for _ in 0..100 {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let (_, list) = app.get("/students?q=Left%20Early", &admin).await;
        found = list["total"].as_i64().unwrap_or(0);
        if found == 8 {
            break;
        }
    }
    assert_eq!(
        found, 8,
        "every student whose request was abandoned was admitted all the same"
    );
    // Each once, and whole: enrolled in the section and written in the audit log.
    assert_eq!(app.count("students").await, 8);
    assert_eq!(app.count("enrollments").await, 8);
    app.finish().await;
}
