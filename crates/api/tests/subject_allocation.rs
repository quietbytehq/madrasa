//! M2 stage 6: subject allocation per student. Electives and groups per class, per-student
//! subjects on rosters, sheets, result sheets and report cards, and an import with rollback.
//! Needs PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use base64::Engine;
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
async fn students_take_only_their_own_subjects() {
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
    let (_, c9) = app
        .post("/classes", &admin, json!({ "name": "9", "level": 9 }))
        .await;
    let (_, s9a) = app
        .post(
            "/sections",
            &admin,
            json!({ "class_id": c9["id"], "academic_year_id": year["id"], "name": "A" }),
        )
        .await;
    let (c9, s9a) = (id(&c9["id"]), id(&s9a["id"]));
    let mut subj = std::collections::HashMap::new();
    for (name, code) in [
        ("English", "ENG"),
        ("Physics", "PHY"),
        ("Chemistry", "CHE"),
        ("Biology", "BIO"),
        ("Computer", "CS"),
    ] {
        let (_, r) = app
            .post("/subjects", &admin, json!({ "name": name, "code": code }))
            .await;
        subj.insert(code, id(&r["id"]));
    }
    let (eng, phy, che, bio, cs) = (
        subj["ENG"].clone(),
        subj["PHY"].clone(),
        subj["CHE"].clone(),
        subj["BIO"].clone(),
        subj["CS"].clone(),
    );
    // Class subjects with electives; the electives come back on GET.
    let (s, r) = app
        .put(
            &format!("/classes/{c9}/subjects"),
            &admin,
            json!({ "subject_ids": [eng, phy, che, bio, cs], "elective_ids": [phy, che, bio, cs] }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["elective_ids"].as_array().unwrap().len(), 4);
    let (_, r) = app.get(&format!("/classes/{c9}/subjects"), &admin).await;
    assert_eq!(r["elective_ids"].as_array().unwrap().len(), 4);
    // Groups hold electives only.
    let (s, e) = app
        .post(
            &format!("/classes/{c9}/subject-groups"),
            &admin,
            json!({ "name": "Bad", "subject_ids": [eng] }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{e}");
    let (s, pre_med) = app
        .post(
            &format!("/classes/{c9}/subject-groups"),
            &admin,
            json!({ "name": "Pre-Medical", "position": 0, "subject_ids": [phy, che, bio] }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{pre_med}");
    let (s, comp) = app
        .post(
            &format!("/classes/{c9}/subject-groups"),
            &admin,
            json!({ "name": "Computer Science", "position": 1, "subject_ids": [phy, che, cs] }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{comp}");
    let (pre_med, comp) = (id(&pre_med["id"]), id(&comp["id"]));
    let (s, e) = app
        .post(
            &format!("/classes/{c9}/subject-groups"),
            &admin,
            json!({ "name": "pre-medical", "subject_ids": [] }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "duplicate name: {e}");

    let student = |name: &str, roll: i32| json!({ "full_name": name, "father_name": "Father", "date_of_birth": "2011-01-01", "gender": "male", "admission_date": "2026-04-01", "section_id": s9a, "roll_no": roll });
    let (_, ali) = app.post("/students", &admin, student("Ali Khan", 1)).await;
    let (_, sara) = app.post("/students", &admin, student("Sara Khan", 2)).await;
    let (_, omar) = app.post("/students", &admin, student("Omar Khan", 3)).await;
    let numbers: Vec<String> = [&ali, &sara, &omar]
        .iter()
        .map(|v| id(&v["student_number"]))
        .collect();
    let (ali, sara, omar) = (id(&ali["id"]), id(&sara["id"]), id(&omar["id"]));

    // Nothing allocated: everyone takes every class subject.
    let (s, view) = app
        .get(&format!("/sections/{s9a}/subject-allocations"), &admin)
        .await;
    assert_eq!(s, StatusCode::OK, "{view}");
    assert_eq!(view["subjects"].as_array().unwrap().len(), 5);
    assert_eq!(view["groups"].as_array().unwrap().len(), 2);
    let students = view["students"].as_array().unwrap();
    assert_eq!(students.len(), 3);
    assert!(students
        .iter()
        .all(|st| st["allocated"] == false && st["subject_ids"].as_array().unwrap().len() == 5));
    let enrol_of = |v: &Value, sid: &str| {
        v["students"]
            .as_array()
            .unwrap()
            .iter()
            .find(|st| st["student_id"] == sid)
            .unwrap()
            .clone()
    };
    let ali_e = id(&enrol_of(&view, &ali)["enrollment_id"]);
    let sara_e = id(&enrol_of(&view, &sara)["enrollment_id"]);
    // Ali: Pre-Medical (the group's subjects); Sara: Computer only, no group; Omar: untouched.
    let (s, view) = app
        .put(
            &format!("/sections/{s9a}/subject-allocations"),
            &admin,
            json!({ "rows": [
                { "enrollment_id": ali_e, "group_id": pre_med },
                { "enrollment_id": sara_e, "subject_ids": [cs] }
            ] }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{view}");
    let own = |v: &Value, sid: &str| -> Vec<String> {
        enrol_of(v, sid)["subject_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(id)
            .collect()
    };
    let mut ali_own = own(&view, &ali);
    ali_own.sort();
    let mut want = vec![eng.clone(), phy.clone(), che.clone(), bio.clone()];
    want.sort();
    assert_eq!(ali_own, want);
    assert_eq!(enrol_of(&view, &ali)["group_id"], pre_med);
    let mut sara_own = own(&view, &sara);
    sara_own.sort();
    let mut want = vec![eng.clone(), cs.clone()];
    want.sort();
    assert_eq!(sara_own, want);
    assert_eq!(own(&view, &omar).len(), 5);
    assert_eq!(enrol_of(&view, &omar)["allocated"], false);
    assert!(view.get("warnings").is_none());
    // Asking to allocate (or drop) a compulsory subject is ignored and said so (B6).
    let (s, view) = app
        .put(
            &format!("/sections/{s9a}/subject-allocations"),
            &admin,
            json!({ "rows": [{ "enrollment_id": sara_e, "subject_ids": [cs, eng] }] }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{view}");
    assert_eq!(view["warnings"].as_array().unwrap().len(), 1);
    assert!(view["warnings"][0]
        .as_str()
        .unwrap()
        .starts_with("English is compulsory in the class and stays allocated to Sara Khan"));
    assert_eq!(own(&view, &sara).len(), 2);
    let (s, mine) = app
        .put(
            &format!("/students/{sara}/subjects"),
            &admin,
            json!({ "subject_ids": [cs, eng] }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{mine}");
    assert_eq!(mine["warnings"].as_array().unwrap().len(), 1);
    // A subject outside the class is refused.
    let (_, other) = app
        .post("/subjects", &admin, json!({ "name": "Art", "code": "ART" }))
        .await;
    let (s, _) = app
        .put(
            &format!("/sections/{s9a}/subject-allocations"),
            &admin,
            json!({ "rows": [{ "enrollment_id": sara_e, "subject_ids": [other["id"]] }] }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT);

    // Exams: rosters, sheets and results follow the allocation.
    let (_, list) = app
        .post(
            "/exams/assessments/defaults",
            &admin,
            json!({ "academic_year_id": year_id }),
        )
        .await;
    let test1 = id(&list[0]["id"]);
    let lines: Vec<Value> = [&eng, &phy, &che, &bio, &cs]
        .iter()
        .map(|sid| json!({ "subject_id": sid, "max_marks": "100" }))
        .collect();
    let (s, _) = app
        .put(
            "/exams/max-marks",
            &admin,
            json!({ "assessment_id": test1, "class_id": c9, "lines": lines }),
        )
        .await;
    assert_eq!(s, StatusCode::OK);
    let (s, sheets) = app
        .get(
            &format!("/exams/sheets?assessment_id={test1}&section_id={s9a}"),
            &admin,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{sheets}");
    let count = |sid: &str| {
        sheets["sheets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["subject_id"] == sid)
            .map(|x| x["students"].as_u64().unwrap())
            .unwrap()
    };
    assert_eq!((count(&eng), count(&bio), count(&cs)), (3, 2, 2));
    let (s, sheet) = app
        .get(
            &format!("/exams/sheet?assessment_id={test1}&section_id={s9a}&subject_id={bio}"),
            &admin,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{sheet}");
    let roster: Vec<String> = sheet["students"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| id(&x["student_id"]))
        .collect();
    assert_eq!(roster, vec![ali.clone(), omar.clone()]);
    // Marks for a student who does not take the subject are refused.
    let (s, e) = app
        .put(
            "/exams/sheet",
            &admin,
            json!({ "assessment_id": test1, "section_id": s9a, "subject_id": bio, "marks": [{ "student_id": sara, "obtained": "50" }] }),
        )
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{e}");
    let save = |subject: &str, marks: Value| json!({ "assessment_id": test1, "section_id": s9a, "subject_id": subject, "marks": marks });
    let (s, saved) = app
        .put(
            "/exams/sheet",
            &admin,
            save(&bio, json!([{ "student_id": ali, "obtained": "80" }, { "student_id": omar, "obtained": "60" }])),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{saved}");
    // Submitting needs marks for the takers only.
    let (s, sub) = app
        .post(
            &format!("/exams/sheets/{}/submit", id(&saved["sheet"]["id"])),
            &admin,
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{sub}");
    app.put("/exams/sheet", &admin, save(&eng, json!([{ "student_id": ali, "obtained": "70" }, { "student_id": sara, "obtained": "90" }, { "student_id": omar, "obtained": "50" }]))).await;
    app.put(
        "/exams/sheet",
        &admin,
        save(&cs, json!([{ "student_id": sara, "obtained": "70" }, { "student_id": omar, "obtained": "40" }])),
    )
    .await;
    let (s, rs) = app
        .get(
            &format!("/exams/results/section?assessment_id={test1}&section_id={s9a}"),
            &admin,
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{rs}");
    let row = |sid: &str| {
        rs["rows"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["student_id"] == sid)
            .unwrap()
            .clone()
    };
    let cell = |r: &Value, subject: &str| {
        r["marks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["subject_id"] == subject)
            .unwrap()
            .clone()
    };
    let sara_row = row(&sara);
    assert_eq!(cell(&sara_row, &bio)["takes"], false);
    assert_eq!(cell(&sara_row, &cs)["takes"], true);
    // Sara's total is over her own subjects only: 90 + 70 out of 200.
    assert_eq!(sara_row["total"], "160.00");
    assert_eq!(sara_row["max"], "200.00");
    assert_eq!(sara_row["percent"], "80.00");
    // Ali takes four subjects (ENG, PHY, CHE, BIO), so his maximum is 400.
    let ali_row = row(&ali);
    assert_eq!(ali_row["max"], "400.00");
    // Biology analysis counts its two takers only.
    let bio_analysis = rs["analysis"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["subject_id"] == bio)
        .unwrap();
    assert_eq!(bio_analysis["appeared"], 2);
    // Report card: Sara sees her own subjects only.
    let (s, card) = app
        .get(&format!("/exams/results/student/{sara}"), &admin)
        .await;
    assert_eq!(s, StatusCode::OK, "{card}");
    let card_subjects: Vec<String> = card["subjects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| id(&x["subject_name"]))
        .collect();
    assert_eq!(card_subjects, vec!["Computer", "English"]);
    let (_, card) = app
        .get(&format!("/exams/results/student/{omar}"), &admin)
        .await;
    assert_eq!(card["subjects"].as_array().unwrap().len(), 5);

    // One student: the profile endpoint shows taken flags; a group picks its subjects.
    let (s, mine) = app.get(&format!("/students/{sara}/subjects"), &admin).await;
    assert_eq!(s, StatusCode::OK, "{mine}");
    assert_eq!(mine["allocated"], true);
    let taken: Vec<&str> = mine["subjects"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|x| x["taken"] == true)
        .map(|x| x["code"].as_str().unwrap())
        .collect();
    assert_eq!(taken, vec!["CS", "ENG"]);
    let (s, mine) = app
        .put(
            &format!("/students/{sara}/subjects"),
            &admin,
            json!({ "group_id": comp }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{mine}");
    assert_eq!(
        mine["subjects"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|x| x["taken"] == true)
            .count(),
        4
    );
    assert_eq!(mine["group_id"], comp);

    // Import: a group name, a subject list, and an unknown student.
    let csv = format!(
        "Admission No,Group,Electives\n{},Pre-Medical,\n{},,CS; Biology\nNOPE-1,,BIO\n",
        numbers[2], numbers[0]
    );
    let content = base64::engine::general_purpose::STANDARD.encode(csv.as_bytes());
    let (s, parsed) = app
        .post(
            "/subject-allocations/import/parse",
            &admin,
            json!({ "file_name": "alloc.csv", "content_base64": content }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{parsed}");
    assert_eq!(parsed["mapping"]["student_number"], 0);
    assert_eq!(parsed["mapping"]["group"], 1);
    assert_eq!(parsed["mapping"]["subjects"], 2);
    let body = |rows: Value| json!({ "file_name": "alloc.csv", "headers": parsed["headers"], "rows": rows, "mapping": parsed["mapping"] });
    let (s, preview) = app
        .post(
            "/subject-allocations/import/preview",
            &admin,
            body(parsed["rows"].clone()),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{preview}");
    assert_eq!(preview["summary"]["errors"], 1);
    assert_eq!(preview["summary"]["changed"], 2);
    assert!(preview["rows"][2]["errors"][0]
        .as_str()
        .unwrap()
        .contains("NOPE-1"));
    let (s, e) = app
        .post(
            "/subject-allocations/import/apply",
            &admin,
            body(parsed["rows"].clone()),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    let good: Vec<Value> = parsed["rows"].as_array().unwrap()[..2].to_vec();
    let (s, applied) = app
        .post(
            "/subject-allocations/import/apply",
            &admin,
            body(json!(good)),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{applied}");
    let (_, view) = app
        .get(&format!("/sections/{s9a}/subject-allocations"), &admin)
        .await;
    assert_eq!(enrol_of(&view, &omar)["group_id"], pre_med);
    assert_eq!(own(&view, &omar).len(), 4);
    let mut ali_own = own(&view, &ali);
    ali_own.sort();
    let mut want = vec![eng.clone(), cs.clone(), bio.clone()];
    want.sort();
    assert_eq!(ali_own, want);
    assert!(enrol_of(&view, &ali)["group_id"].is_null());
    // Rollback restores Ali's group and clears Omar.
    let (s, rb) = app
        .post(
            &format!("/imports/batches/{}/rollback", id(&applied["batch_id"])),
            &admin,
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{rb}");
    let (_, view) = app
        .get(&format!("/sections/{s9a}/subject-allocations"), &admin)
        .await;
    assert_eq!(enrol_of(&view, &ali)["group_id"], pre_med);
    assert_eq!(enrol_of(&view, &omar)["allocated"], false);
    assert_eq!(own(&view, &omar).len(), 5);

    // Removing a group keeps the students' subjects but drops the label.
    let (s, _) = app
        .call(
            Method::DELETE,
            &format!("/subject-groups/{pre_med}"),
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, view) = app
        .get(&format!("/sections/{s9a}/subject-allocations"), &admin)
        .await;
    assert!(enrol_of(&view, &ali)["group_id"].is_null());
    assert_eq!(own(&view, &ali).len(), 4);
    assert_eq!(view["groups"].as_array().unwrap().len(), 1);
    app.finish().await;
}
