//! M2 stage 5: terms and assessment components. An existing four-assessment year keeps its
//! results; a two-term year with a calculated component rolls up to term and year results.
//! Needs PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use madrasa_api::{router, AppState};
use madrasa_db::testing::TestDb;
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
    async fn call(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
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
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
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
    async fn patch(&self, p: &str, t: &str, b: Value) -> (StatusCode, Value) {
        self.call(Method::PATCH, p, Some(t), Some(b)).await
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

#[tokio::test]
async fn terms_and_components_roll_up_to_term_and_year_results() {
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
    let year_id = id(&year["id"]);
    let (_, c1) = app
        .post("/classes", &admin, json!({ "name": "1", "level": 1 }))
        .await;
    let (_, s1) = app
        .post(
            "/sections",
            &admin,
            json!({ "class_id": c1["id"], "academic_year_id": year["id"], "name": "A" }),
        )
        .await;
    let (c1, s1) = (id(&c1["id"]), id(&s1["id"]));
    let (_, eng) = app
        .post(
            "/subjects",
            &admin,
            json!({ "name": "English", "code": "ENG" }),
        )
        .await;
    let eng = id(&eng["id"]);
    app.put(
        &format!("/classes/{c1}/subjects"),
        &admin,
        json!({ "subject_ids": [eng] }),
    )
    .await;
    let student = |name: &str, roll: i32| json!({ "full_name": name, "father_name": "Father", "date_of_birth": "2015-01-01", "gender": "male", "admission_date": "2026-04-01", "section_id": s1, "roll_no": roll });
    let (_, ali) = app.post("/students", &admin, student("Ali Khan", 1)).await;
    let (_, sara) = app.post("/students", &admin, student("Sara Khan", 2)).await;
    let (ali, sara) = (id(&ali["id"]), id(&sara["id"]));

    // Defaults as before: one term "Year" holding the four assessments; weights per term.
    let (s, list) = app
        .post(
            "/exams/assessments/defaults",
            &admin,
            json!({ "academic_year_id": year_id }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{list}");
    let list = list.as_array().unwrap().clone();
    assert_eq!(list.len(), 4);
    assert!(
        list.iter()
            .all(|a| a["kind"] == "entered" && !a["term_id"].is_null()),
        "{list:?}"
    );
    let (s, st) = app
        .get(
            &format!("/exams/structure?academic_year_id={year_id}"),
            &admin,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{st}");
    assert_eq!(st["terms"].as_array().unwrap().len(), 1);
    assert_eq!(st["terms"][0]["name"], "Year");
    assert_eq!(st["terms"][0]["weight"], "100.00");
    assert_eq!(st["terms"][0]["components"].as_array().unwrap().len(), 4);
    let test1 = id(&list[0]["id"]);
    let test2 = id(&list[1]["id"]);
    let weights = |w: [u32; 4]| json!({ "academic_year_id": year_id, "weights": list.iter().zip(w).map(|(a, w)| json!({ "id": a["id"], "weight": w.to_string() })).collect::<Vec<_>>() });
    let (s, _) = app
        .put(
            "/exams/assessments/weights",
            &admin,
            weights([25, 25, 25, 25]),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = app
        .put(
            "/exams/assessments/weights",
            &admin,
            weights([25, 25, 25, 24]),
        )
        .await;
    assert_eq!(
        s,
        StatusCode::BAD_REQUEST,
        "a term's weights must total 100"
    );

    // Marks in Test 1 and Test 2; a calculated "Best test" = best of the two.
    app.put("/exams/max-marks", &admin, json!({ "assessment_id": test1, "class_id": c1, "lines": [{ "subject_id": eng, "max_marks": "50" }] })).await;
    app.put("/exams/max-marks", &admin, json!({ "assessment_id": test2, "class_id": c1, "lines": [{ "subject_id": eng, "max_marks": "50" }] })).await;
    let (s, sh) = app.put("/exams/sheet", &admin, json!({ "assessment_id": test1, "section_id": s1, "subject_id": eng, "marks": [{ "student_id": ali, "obtained": "40" }, { "student_id": sara, "obtained": "20" }] })).await;
    assert_eq!(s, StatusCode::OK, "{sh}");
    let (s, sh) = app.put("/exams/sheet", &admin, json!({ "assessment_id": test2, "section_id": s1, "subject_id": eng, "marks": [{ "student_id": ali, "obtained": "30" }, { "student_id": sara, "is_absent": true }] })).await;
    assert_eq!(s, StatusCode::OK, "{sh}");
    let term_id = id(&st["terms"][0]["id"]);
    let (s, e) = app.post("/exams/assessments", &admin, json!({ "academic_year_id": year_id, "name": "Best test", "kind": "calculated", "term_id": term_id, "weight": "0", "formula": { "op": "best_of", "inputs": [{ "assessment_type_id": test1 }, { "assessment_type_id": test2 }], "n": 3 } })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "N above the inputs: {e}");
    let (s, e) = app.post("/exams/assessments", &admin, json!({ "academic_year_id": year_id, "name": "Best test", "kind": "calculated", "term_id": term_id, "weight": "0", "formula": { "op": "sum", "inputs": [] } })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    let (s, best) = app.post("/exams/assessments", &admin, json!({ "academic_year_id": year_id, "name": "Best test", "kind": "calculated", "term_id": term_id, "weight": "0", "formula": { "op": "best_of", "inputs": [{ "assessment_type_id": test1 }, { "assessment_type_id": test2 }], "n": 1 } })).await;
    assert_eq!(s, StatusCode::CREATED, "{best}");
    let best_id = id(&best["id"]);
    assert_eq!(best["kind"], "calculated");
    // No sheets for a calculated component.
    let (s, e) = app
        .get(
            &format!("/exams/sheet?assessment_id={best_id}&section_id={s1}&subject_id={eng}"),
            &admin,
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    let (s, _) = app.put("/exams/max-marks", &admin, json!({ "assessment_id": best_id, "class_id": c1, "lines": [{ "subject_id": eng, "max_marks": "50" }] })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // Its result sheet is computed: Ali best 80%, Sara best 40% (absent counts 0).
    let (s, rs) = app
        .get(
            &format!("/exams/results/section?assessment_id={best_id}&section_id={s1}"),
            &admin,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{rs}");
    assert_eq!(rs["subjects"][0]["max_marks"], "100.00");
    let row_of = |rs: &Value, sid: &str| {
        rs["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["student_id"] == sid)
            .unwrap()
            .clone()
    };
    assert_eq!(row_of(&rs, &ali)["marks"][0]["obtained"], "80.00");
    assert_eq!(row_of(&rs, &sara)["marks"][0]["obtained"], "40.00");
    assert_eq!(row_of(&rs, &ali)["position"], 1);
    // A weighted-average component with weights, and a cycle through it is refused.
    let (s, avg) = app.post("/exams/assessments", &admin, json!({ "academic_year_id": year_id, "name": "Tests average", "kind": "calculated", "term_id": term_id, "weight": "0", "formula": { "op": "weighted", "inputs": [{ "assessment_type_id": test1, "weight": "3" }, { "assessment_type_id": test2, "weight": "1" }] } })).await;
    assert_eq!(s, StatusCode::CREATED, "{avg}");
    let (s, rs) = app
        .get(
            &format!(
                "/exams/results/section?assessment_id={}&section_id={s1}",
                id(&avg["id"])
            ),
            &admin,
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        row_of(&rs, &ali)["marks"][0]["obtained"],
        "75.00",
        "(3×80 + 1×60) / 4"
    );
    // Absences (v0.2.0-alpha O1): a weighted average counts Sara's absent Test 2 as zero
    // by default ((3×40 + 0) / 4); an average skips it; told to count it, it is zero.
    assert_eq!(row_of(&rs, &sara)["marks"][0]["obtained"], "30.00");
    let (s, avg2) = app.post("/exams/assessments", &admin, json!({ "academic_year_id": year_id, "name": "Tests mean", "kind": "calculated", "term_id": term_id, "weight": "0", "formula": { "op": "average", "inputs": [{ "assessment_type_id": test1 }, { "assessment_type_id": test2 }] } })).await;
    assert_eq!(s, StatusCode::CREATED, "{avg2}");
    assert!(avg2["formula"].get("absent").is_none_or(|a| a.is_null()));
    let (_, rs) = app
        .get(
            &format!(
                "/exams/results/section?assessment_id={}&section_id={s1}",
                id(&avg2["id"])
            ),
            &admin,
        )
        .await;
    assert_eq!(
        row_of(&rs, &sara)["marks"][0]["obtained"],
        "40.00",
        "absent paper skipped"
    );
    assert_eq!(row_of(&rs, &ali)["marks"][0]["obtained"], "70.00");
    let (s, avg3) = app.post("/exams/assessments", &admin, json!({ "academic_year_id": year_id, "name": "Tests mean (zero)", "kind": "calculated", "term_id": term_id, "weight": "0", "formula": { "op": "average", "absent": "zero", "inputs": [{ "assessment_type_id": test1 }, { "assessment_type_id": test2 }] } })).await;
    assert_eq!(s, StatusCode::CREATED, "{avg3}");
    assert_eq!(avg3["formula"]["absent"], "zero");
    let (_, rs) = app
        .get(
            &format!(
                "/exams/results/section?assessment_id={}&section_id={s1}",
                id(&avg3["id"])
            ),
            &admin,
        )
        .await;
    assert_eq!(
        row_of(&rs, &sara)["marks"][0]["obtained"],
        "20.00",
        "absent paper is zero"
    );
    let (s, e) = app.patch(&format!("/exams/assessments/{best_id}"), &admin, json!({ "name": "Best test", "formula": { "op": "sum", "inputs": [{ "assessment_type_id": avg["id"] }, { "assessment_type_id": best_id }] } })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "self reference: {e}");

    // Report card: the single term's result equals the old weighted result (identical to
    // before terms existed), with the calculated columns present but weightless.
    let (s, card) = app
        .get(&format!("/exams/results/student/{ali}"), &admin)
        .await;
    assert_eq!(s, StatusCode::OK, "{card}");
    assert_eq!(card["terms"].as_array().unwrap().len(), 1);
    assert_eq!(card["assessments"].as_array().unwrap().len(), 8);
    let engs = &card["subjects"][0];
    assert_eq!(
        engs["term_percents"][0], "70.00",
        "25:25 over 80% and 60%: {card}"
    );
    assert_eq!(engs["weighted_percent"], "70.00");
    assert_eq!(card["terms"][0]["percent"], "70.00");
    assert_eq!(card["overall_percent"], "70.00");

    // A second year with two default terms: Term 1 (Test 1 30 + Mid-year 70), Term 2.
    let (_, y2) = app
        .post(
            "/academic-years",
            &admin,
            json!({ "start_year": 2027, "start_month": 4 }),
        )
        .await;
    let y2 = id(&y2["id"]);
    let (s, list2) = app
        .post(
            "/exams/assessments/defaults",
            &admin,
            json!({ "academic_year_id": y2, "terms": 2 }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{list2}");
    let (_, st2) = app
        .get(&format!("/exams/structure?academic_year_id={y2}"), &admin)
        .await;
    let terms = st2["terms"].as_array().unwrap();
    assert_eq!(terms.len(), 2);
    assert_eq!(
        (terms[0]["name"].as_str(), terms[0]["weight"].as_str()),
        (Some("Term 1"), Some("50.00"))
    );
    assert_eq!(terms[0]["components"].as_array().unwrap().len(), 2);
    assert_eq!(terms[0]["components"][1]["weight"], "70.00");
    // Term weights must total 100; terms with components cannot be removed; dates checked.
    let (s, _) = app.put("/exams/terms/weights", &admin, json!({ "academic_year_id": y2, "weights": [{ "id": terms[0]["id"], "weight": "40" }, { "id": terms[1]["id"], "weight": "60" }] })).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = app.put("/exams/terms/weights", &admin, json!({ "academic_year_id": y2, "weights": [{ "id": terms[0]["id"], "weight": "40" }, { "id": terms[1]["id"], "weight": "50" }] })).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = app
        .call(
            Method::DELETE,
            &format!("/exams/terms/{}", id(&terms[0]["id"])),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, t3) = app.post("/exams/terms", &admin, json!({ "academic_year_id": y2, "name": "Term 3", "position": 2, "starts_on": "2028-01-01", "ends_on": "2027-12-01" })).await;
    assert_eq!(s, StatusCode::CONFLICT, "ends before it starts: {t3}");
    let (s, t3) = app
        .post(
            "/exams/terms",
            &admin,
            json!({ "academic_year_id": y2, "name": "Term 3", "position": 2 }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{t3}");
    let (s, _) = app
        .call(
            Method::DELETE,
            &format!("/exams/terms/{}", id(&t3["id"])),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    // Moving a component to another term re-validates the term weights on the next save.
    let (s, moved) = app
        .patch(
            &format!(
                "/exams/assessments/{}",
                id(&terms[0]["components"][0]["id"])
            ),
            &admin,
            json!({ "name": "Bi-monthly Test 1", "term_id": terms[1]["id"] }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{moved}");
    assert_eq!(moved["term_id"], terms[1]["id"]);
    // The structure is also what the exams reports see: the report card report still runs.
    let (s, r) = app
        .get(
            &format!("/reports/exams.report_cards?class_id={c1}&section_id={s1}&format=pdf"),
            &admin,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    app.finish().await;
}
