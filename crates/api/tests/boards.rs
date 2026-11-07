//! M2 stage 4: boards, exams, candidates, result import (both layouts, errors, rollback),
//! the computed sheet, the student's results and the library reports. Needs PostgreSQL via
//! `MADRASA_TEST_DATABASE_URL`.

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use http_body_util::BodyExt;
use madrasa_api::{router, AppState};
use madrasa_db::testing::TestDb;
use rust_xlsxwriter::Workbook;
use serde_json::{json, Value};
use tower::ServiceExt;

struct App {
    db: Option<TestDb>,
    app: Router,
}

impl App {
    async fn new() -> Option<App> {
        let db = TestDb::new().await?;
        let state = AppState::new(db.pool.clone()).await.unwrap();
        let app = router(state.clone());
        Some(App { db: Some(db), app })
    }
    async fn raw(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, String, Vec<u8>) {
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
        let ctype = res
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, ctype, bytes.to_vec())
    }
    async fn call(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let (status, _, bytes) = self.raw(method, path, token, body).await;
        let value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        (status, value)
    }
    async fn get(&self, p: &str, t: &str) -> (StatusCode, Value) {
        self.call(Method::GET, p, Some(t), None).await
    }
    async fn post(&self, p: &str, t: &str, b: Value) -> (StatusCode, Value) {
        self.call(Method::POST, p, Some(t), Some(b)).await
    }
    async fn put(&self, p: &str, t: &str, b: Value) -> (StatusCode, Value) {
        self.call(Method::PUT, p, Some(t), Some(b)).await
    }
    async fn login(&self, u: &str, p: &str) -> String {
        let (s, b) = self
            .call(
                Method::POST,
                "/auth/login",
                None,
                Some(json!({ "username": u, "password": p })),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        b["token"].as_str().unwrap().into()
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

fn id(v: &Value) -> String {
    v.as_str().unwrap().to_string()
}

/// A workbook with one sheet of string/number cells, base64-encoded.
fn xlsx(rows: &[Vec<&str>]) -> String {
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    for (r, row) in rows.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            if let Ok(n) = v.parse::<f64>() {
                ws.write_number(r as u32, c as u16, n).unwrap();
            } else {
                ws.write_string(r as u32, c as u16, *v).unwrap();
            }
        }
    }
    STANDARD.encode(wb.save_to_buffer().unwrap())
}

#[tokio::test]
async fn boards_exams_candidates_import_sheet_and_reports() {
    let app = app!();
    let (s, b) = app.call(Method::POST, "/setup", None, Some(json!({ "school_name": "Gulshan High", "school_code": "KHI-MC", "academic_year_start_month": 4, "admin_full_name": "Ayesha", "admin_username": "ayesha", "admin_password": "admin-password-1", "backup_password": "backup-password-1" }))).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let admin = app.login("ayesha", "admin-password-1").await;
    let (_, year) = app
        .post(
            "/academic-years",
            &admin,
            json!({ "start_year": 2026, "start_month": 4, "activate": true }),
        )
        .await;
    let (_, c9) = app
        .post("/classes", &admin, json!({ "name": "9", "level": 9 }))
        .await;
    let (_, c10) = app
        .post("/classes", &admin, json!({ "name": "10", "level": 10 }))
        .await;
    let (_, s9) = app
        .post(
            "/sections",
            &admin,
            json!({ "class_id": c9["id"], "academic_year_id": year["id"], "name": "A" }),
        )
        .await;
    let (_, s10) = app
        .post(
            "/sections",
            &admin,
            json!({ "class_id": c10["id"], "academic_year_id": year["id"], "name": "A" }),
        )
        .await;
    let (_, eng) = app
        .post(
            "/subjects",
            &admin,
            json!({ "name": "English", "code": "ENG" }),
        )
        .await;
    let student = |name: &str, roll: &str, section: &Value| {
        json!({ "full_name": name, "father_name": format!("Father of {name}"), "date_of_birth": "2011-05-05", "gender": "male",
                "admission_date": "2026-04-01", "section_id": section, "roll_no": 1, "board_roll_no": roll })
    };
    let (_, ali) = app
        .post(
            "/students",
            &admin,
            student("Ali Khan", "123001", &s9["id"]),
        )
        .await;
    let (_, sara) = app
        .post(
            "/students",
            &admin,
            student("Sara Khan", "123002", &s9["id"]),
        )
        .await;
    let (_, zain) = app
        .post("/students", &admin, student("Zain Ahmed", "", &s10["id"]))
        .await;
    let (ali, sara, zain) = (id(&ali["id"]), id(&sara["id"]), id(&zain["id"]));

    // Board with bands; validation of bands and codes.
    let (s, e) = app.post("/boards", &admin, json!({ "name": "BISE Karachi", "short_code": "BISEK", "grade_bands": [{ "grade": "A", "min_percent": 70 }, { "grade": "A", "min_percent": 80 }] })).await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("validation")),
        "{e}"
    );
    let (s, board) = app.post("/boards", &admin, json!({
        "name": "BISE Karachi", "short_code": "bisek", "pass_percent": 33,
        "grade_bands": [{ "grade": "A+", "min_percent": 80 }, { "grade": "A", "min_percent": 70 }, { "grade": "B", "min_percent": 60 }, { "grade": "C", "min_percent": 50 }, { "grade": "D", "min_percent": 40 }, { "grade": "E", "min_percent": 33 }]
    })).await;
    assert_eq!(s, StatusCode::CREATED, "{board}");
    assert_eq!(board["short_code"], "BISEK");
    assert_eq!(board["country"], "PK");
    assert_eq!(board["grade_bands"][0]["grade"], "A+");
    let board_id = id(&board["id"]);
    let (s, _) = app
        .post(
            "/boards",
            &admin,
            json!({ "name": "bise karachi", "short_code": "X" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);

    // Exam for classes 9 and 10 with three papers.
    let (s, exam) = app.post("/boards/exams", &admin, json!({
        "board_id": board_id, "name": "SSC Part I Annual 2026", "class_ids": [c9["id"], c10["id"]],
        "subjects": [{ "name": "English", "subject_id": eng["id"], "max_marks": 75 }, { "name": "Mathematics", "max_marks": 75 }, { "name": "Islamiat", "max_marks": 50 }]
    })).await;
    assert_eq!(s, StatusCode::CREATED, "{exam}");
    let exam_id = id(&exam["id"]);
    assert_eq!(exam["status"], "announced");
    assert_eq!(exam["class_names"], json!(["9", "10"]));
    assert_eq!(exam["subjects"].as_array().unwrap().len(), 3);
    let (s, e) = app.post("/boards/exams", &admin, json!({ "board_id": board_id, "name": "Dup", "subjects": [{ "name": "English", "max_marks": 0 }] })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    let (s, _) = app
        .call(
            Method::DELETE,
            &format!("/boards/{board_id}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(
        s,
        StatusCode::CONFLICT,
        "a board with exams cannot be removed"
    );

    // Candidates: whole class 9 plus Zain; roll numbers pre-filled from the student record.
    let (s, reg) = app
        .post(
            &format!("/boards/exams/{exam_id}/candidates"),
            &admin,
            json!({ "class_id": c9["id"], "student_ids": [zain], "group_name": "Science" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{reg}");
    assert_eq!(reg["added"], 3);
    let cands = reg["candidates"].as_array().unwrap();
    assert_eq!(cands.len(), 3);
    let zc = cands.iter().find(|c| c["student_id"] == zain).unwrap();
    assert!(zc["roll_no"].is_null());
    let ac = cands.iter().find(|c| c["student_id"] == ali).unwrap();
    assert_eq!(ac["roll_no"], "123001");
    let (s, reg2) = app
        .post(
            &format!("/boards/exams/{exam_id}/candidates"),
            &admin,
            json!({ "class_id": c9["id"] }),
        )
        .await;
    assert_eq!(
        (s, reg2["added"].as_i64()),
        (StatusCode::OK, Some(0)),
        "already registered"
    );
    let (s, zc) = app
        .put(
            &format!("/boards/candidates/{}", id(&zc["id"])),
            &admin,
            json!({ "roll_no": "123003", "registration_no": "R-3" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{zc}");
    let (_, zd) = app.get(&format!("/students/{zain}"), &admin).await;
    assert_eq!(zd["board_roll_no"], "123003", "student record kept in step");
    let (s, e) = app
        .put(
            &format!("/boards/candidates/{}", id(&ac["id"])),
            &admin,
            json!({ "roll_no": "123003" }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "roll numbers unique per exam: {e}");

    // Wide file: a bad roll, a mark over the maximum, an absent marker.
    let file = xlsx(&[
        vec!["Roll No", "Name", "English", "Mathematics", "Islamiat"],
        vec!["123001", "Ali", "60", "70", "40"],
        vec!["123002", "Sara", "20", "ABS", "30"],
        vec!["123003", "Zain", "90", "50", "45"],
        vec!["999999", "Nobody", "50", "50", "50"],
    ]);
    let (s, parsed) = app
        .post(
            &format!("/boards/exams/{exam_id}/results/parse"),
            &admin,
            json!({ "file_name": "ssc.xlsx", "content_base64": file }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{parsed}");
    assert_eq!(parsed["layout"], "wide");
    assert_eq!(parsed["mapping"]["roll_no"], 0, "{parsed}");
    let subjects = exam["subjects"].as_array().unwrap();
    let eng_key = format!("marks:{}", id(&subjects[0]["id"]));
    assert_eq!(parsed["mapping"][&eng_key], 2, "{parsed}");
    let body = |rows: Value| json!({ "file_name": "ssc.xlsx", "headers": parsed["headers"], "rows": rows, "mapping": parsed["mapping"], "layout": "wide", "save_mapping": true });
    let (s, pv) = app
        .post(
            &format!("/boards/exams/{exam_id}/results/preview"),
            &admin,
            body(parsed["rows"].clone()),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{pv}");
    assert_eq!(pv["summary"]["rows"], 4);
    assert_eq!(pv["summary"]["errors"], 2, "{pv}");
    let rows = pv["rows"].as_array().unwrap();
    assert_eq!(rows[0]["candidate_name"], "Ali Khan");
    assert_eq!(rows[0]["results"][0]["grade"], "A+", "60/75 = 80%");
    assert_eq!(rows[0]["results"][0]["status"], "pass");
    assert_eq!(rows[1]["results"][1]["status"], "absent");
    assert_eq!(rows[1]["results"][0]["status"], "fail", "20/75");
    assert!(
        rows[2]["errors"][0]
            .as_str()
            .unwrap()
            .contains("English: 90 is outside 0–75"),
        "{}",
        rows[2]["errors"]
    );
    assert!(rows[3]["errors"][0].as_str().unwrap().contains("999999"));
    assert_eq!(pv["unmapped"], json!(["Name"]));
    let (s, e) = app
        .post(
            &format!("/boards/exams/{exam_id}/results/apply"),
            &admin,
            body(parsed["rows"].clone()),
        )
        .await;
    assert_eq!(
        s,
        StatusCode::BAD_REQUEST,
        "rows with errors block the import: {e}"
    );

    // Fixed rows import; the exam moves to results_imported; the mapping is remembered.
    let good = json!([
        ["123001", "Ali", "60", "70", "40"],
        ["123002", "Sara", "20", "ABS", "30"],
        ["123003", "Zain", "70", "50", "45"]
    ]);
    let (s, applied) = app
        .post(
            &format!("/boards/exams/{exam_id}/results/apply"),
            &admin,
            body(good),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{applied}");
    assert_eq!(applied["summary"]["results"], 9);
    let batch1 = id(&applied["batch_id"]);
    let (_, ex) = app.get(&format!("/boards/exams/{exam_id}"), &admin).await;
    assert_eq!(
        (ex["status"].as_str(), ex["results"].as_i64()),
        (Some("results_imported"), Some(3))
    );
    let (_, parsed2) = app
        .post(
            &format!("/boards/exams/{exam_id}/results/parse"),
            &admin,
            json!({ "file_name": "ssc.xlsx", "content_base64": file }),
        )
        .await;
    assert_eq!(parsed2["mapping_saved"], true);

    // The sheet: totals, grades, statuses, positions; Sara fails (absent paper), Zain tops.
    let (s, sheet) = app
        .get(&format!("/boards/exams/{exam_id}/results"), &admin)
        .await;
    assert_eq!(s, StatusCode::OK, "{sheet}");
    let srows = sheet["rows"].as_array().unwrap();
    let row_of = |sid: &str| {
        srows
            .iter()
            .find(|r| r["candidate"]["student_id"] == sid)
            .unwrap()
            .clone()
    };
    let (ra, rs, rz) = (row_of(&ali), row_of(&sara), row_of(&zain));
    assert_eq!(
        (
            ra["total"].as_str(),
            ra["max"].as_str(),
            ra["percent"].as_str(),
            ra["grade"].as_str(),
            ra["status"].as_str(),
            ra["position"].as_i64()
        ),
        (
            Some("170.00"),
            Some("200.00"),
            Some("85.00"),
            Some("A+"),
            Some("pass"),
            Some(1)
        )
    );
    assert_eq!(
        (rz["percent"].as_str(), rz["position"].as_i64()),
        (Some("82.50"), Some(2))
    );
    assert_eq!(
        (rs["status"].as_str(), rs["position"].as_i64()),
        (Some("fail"), Some(3))
    );
    assert_eq!(
        (
            sheet["candidates"].as_i64(),
            sheet["with_results"].as_i64(),
            sheet["passed"].as_i64(),
            sheet["pass_percent"].as_str()
        ),
        (Some(3), Some(3), Some(2), Some("66.67"))
    );
    assert_eq!(
        sheet["subjects"][1]["appeared"], 2,
        "Sara absent in Mathematics"
    );

    // Long layout re-import updates Ali's English (replaces) and is rolled back as a batch.
    let long = xlsx(&[
        vec![
            "Roll Number",
            "Subject",
            "Marks Obtained",
            "Grade",
            "Result",
        ],
        vec!["123001", "English", "30", "D", "Pass"],
        vec!["123001", "Physics", "30", "", ""],
    ]);
    let (_, lp) = app
        .post(
            &format!("/boards/exams/{exam_id}/results/parse"),
            &admin,
            json!({ "file_name": "long.xlsx", "content_base64": long }),
        )
        .await;
    assert_eq!(lp["layout"], "long", "{lp}");
    let (s, pv) = app.post(&format!("/boards/exams/{exam_id}/results/preview"), &admin, json!({ "headers": lp["headers"], "rows": lp["rows"], "mapping": lp["mapping"], "layout": "long" })).await;
    assert_eq!(s, StatusCode::OK, "{pv}");
    assert_eq!(pv["rows"][0]["results"][0]["replaces"], true);
    assert_eq!(
        pv["rows"][0]["results"][0]["grade"], "D",
        "the board's grade wins"
    );
    assert!(pv["rows"][1]["errors"][0]
        .as_str()
        .unwrap()
        .contains("Physics"));
    let (s, applied2) = app.post(&format!("/boards/exams/{exam_id}/results/apply"), &admin, json!({ "headers": lp["headers"], "rows": [lp["rows"][0]], "mapping": lp["mapping"], "layout": "long" })).await;
    assert_eq!(s, StatusCode::OK, "{applied2}");
    assert_eq!(applied2["summary"]["replaced"], 1);
    let (_, sheet) = app
        .get(&format!("/boards/exams/{exam_id}/results"), &admin)
        .await;
    let ra = sheet["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["candidate"]["student_id"] == ali)
        .unwrap()
        .clone();
    assert_eq!(
        (ra["total"].as_str(), ra["cells"][0]["grade"].as_str()),
        (Some("140.00"), Some("D"))
    );
    let (s, rb) = app
        .post(
            &format!("/imports/batches/{}/rollback", id(&applied2["batch_id"])),
            &admin,
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{rb}");
    let (_, sheet) = app
        .get(&format!("/boards/exams/{exam_id}/results"), &admin)
        .await;
    let ra = sheet["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["candidate"]["student_id"] == ali)
        .unwrap()
        .clone();
    assert_eq!(
        (ra["total"].as_str(), ra["cells"][0]["grade"].as_str()),
        (Some("170.00"), Some("A+")),
        "rolled back to the first import"
    );
    let (s, _) = app
        .post(
            &format!("/imports/batches/{batch1}/rollback"),
            &admin,
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (_, sheet) = app
        .get(&format!("/boards/exams/{exam_id}/results"), &admin)
        .await;
    assert_eq!(
        sheet["with_results"], 0,
        "first import rolled back: {}",
        sheet["with_results"]
    );
    let (s, applied3) = app
        .post(
            &format!("/boards/exams/{exam_id}/results/apply"),
            &admin,
            body(json!([
                ["123001", "Ali", "60", "70", "40"],
                ["123002", "Sara", "20", "ABS", "30"],
                ["123003", "Zain", "70", "50", "45"]
            ])),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{applied3}");

    // The student's results and the library reports.
    let (s, mine) = app
        .get(&format!("/students/{ali}/board-results"), &admin)
        .await;
    assert_eq!(s, StatusCode::OK, "{mine}");
    assert_eq!(mine[0]["board_name"], "BISE Karachi");
    assert_eq!(mine[0]["row"]["grade"], "A+");
    let (s, cat) = app.get("/reports", &admin).await;
    assert_eq!(s, StatusCode::OK);
    let exams_mod = cat
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["module"] == "exams")
        .unwrap();
    let bs = exams_mod["reports"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["code"] == "exams.board_sheet")
        .unwrap();
    assert_eq!(bs["params"][1]["kind"], "board_exam");
    let (s, r) = app
        .get(
            &format!("/reports/exams.board_sheet?board_exam_id={exam_id}"),
            &admin,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["row_count"], 3);
    assert_eq!(r["columns"][4]["label"], "English /75");
    let first = &r["groups"][0]["rows"][0];
    assert_eq!(
        (
            first[0].as_str(),
            first[2].as_str(),
            first[4].as_str(),
            first[7].as_str(),
            first[9].as_str(),
            first[10].as_str(),
            first[11].as_str()
        ),
        (
            Some("123001"),
            Some("Ali Khan"),
            Some("60 (A+)"),
            Some("170/200"),
            Some("A+"),
            Some("Pass"),
            Some("1")
        )
    );
    let (_, r) = app
        .get(
            &format!(
                "/reports/exams.board_sheet?board_exam_id={exam_id}&class_id={}",
                id(&c10["id"])
            ),
            &admin,
        )
        .await;
    assert_eq!(r["row_count"], 1, "class filter: {r}");
    let (_, r) = app
        .get(
            &format!("/reports/exams.board_merit?board_exam_id={exam_id}&top=2&lang=ur"),
            &admin,
        )
        .await;
    assert_eq!(r["row_count"], 2);
    assert_eq!(r["title"], "بورڈ میرٹ لسٹ");
    assert_eq!(r["groups"][0]["rows"][0][2], "Ali Khan");
    let (_, r) = app
        .get(
            &format!("/reports/exams.board_pass?board_exam_id={exam_id}"),
            &admin,
        )
        .await;
    assert_eq!(
        r["groups"][0]["rows"][1],
        json!(["Mathematics", "2", "2", "0", "100.0", "80.0"]),
        "{r}"
    );
    assert_eq!(r["total"][4], "66.7");
    let (s, r) = app.get(&format!("/reports/exams.board_card?board_exam_id={exam_id}&student_number=KHI-MC-2026-00002"), &admin).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["groups"][0]["rows"][1][4], "Absent");
    assert_eq!(r["total"][4], "Fail");
    let (s, _, bytes) = app
        .raw(
            Method::GET,
            &format!("/reports/exams.board_sheet?board_exam_id={exam_id}&format=pdf&lang=ur"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert!(bytes.starts_with(b"%PDF"));

    // Removing the exam takes its candidates and results with it; then the board can go.
    let (s, _) = app
        .call(
            Method::DELETE,
            &format!("/boards/exams/{exam_id}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = app.get(&format!("/boards/exams/{exam_id}"), &admin).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = app
        .call(
            Method::DELETE,
            &format!("/boards/{board_id}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, boards) = app.get("/boards", &admin).await;
    assert_eq!(boards.as_array().unwrap().len(), 0);
    let _ = s10;
    app.finish().await;
}
