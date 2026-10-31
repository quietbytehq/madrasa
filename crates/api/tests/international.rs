//! A school outside Pakistan: US setup, country defaults, and the import wizard fed a US-style
//! register (Maple Ridge demo: 1,123 students, 40 staff). Needs PostgreSQL via
//! `MADRASA_TEST_DATABASE_URL`.

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
    async fn patch(&self, p: &str, t: &str, b: Value) -> (StatusCode, Value) {
        self.call(Method::PATCH, p, Some(t), Some(b)).await
    }

    /// US setup: Maple Ridge, Portland (Oregon), year from August, classes K–12 with sections
    /// A–C in the active year.
    async fn seed_us(&self) -> (String, String) {
        let (s, b) = self
            .call(
                Method::POST,
                "/setup",
                None,
                Some(json!({
                    "school_name": "Maple Ridge Elementary & Middle School",
                    "school_code": "MRPDX",
                    "city": "Portland",
                    "academic_year_start_month": 8,
                    "country": "us",
                    "time_zone": "America/Los_Angeles",
                    "admin_full_name": "Dana Whitfield",
                    "admin_username": "dana",
                    "admin_password": "admin-password-1",
                    "backup_password": "backup-password-1"
                })),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        let school_id = b["school_id"].as_str().unwrap().to_string();
        let admin = self.login("dana", "admin-password-1").await;
        let (s, year) = self
            .post(
                "/academic-years",
                &admin,
                json!({ "start_year": 2026, "start_month": 8, "activate": true }),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{year}");
        for (level, name) in GRADES.iter().enumerate() {
            let (s, class) = self
                .post("/classes", &admin, json!({ "name": name, "level": level }))
                .await;
            assert_eq!(s, StatusCode::OK, "{class}");
            for section in ["A", "B", "C"] {
                let (s, sec) = self
                    .post(
                        "/sections",
                        &admin,
                        json!({ "class_id": class["id"], "academic_year_id": year["id"], "name": section }),
                    )
                    .await;
                assert_eq!(s, StatusCode::OK, "{sec}");
            }
        }
        (admin, school_id)
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

const GRADES: [&str; 13] = [
    "K", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12",
];
const STUDENTS: usize = 1123;
const STAFF: usize = 40;
const FIRST: [&str; 40] = [
    "Olivia",
    "Liam",
    "Emma",
    "Noah",
    "Ava",
    "Oliver",
    "Sophia",
    "Elijah",
    "Isabella",
    "Lucas",
    "Mia",
    "Mason",
    "Amelia",
    "Logan",
    "Harper",
    "Ethan",
    "Evelyn",
    "James",
    "Abigail",
    "Aiden",
    "Emily",
    "Benjamin",
    "Ella",
    "Jackson",
    "Scarlett",
    "Sebastian",
    "Grace",
    "Jack",
    "Chloe",
    "Henry",
    "Victoria",
    "Owen",
    "Riley",
    "Samuel",
    "Aria",
    "Wyatt",
    "Lily",
    "Carter",
    "Zoe",
    "Julian",
];
const LAST: [&str; 40] = [
    "Nguyen",
    "Garcia",
    "Johnson",
    "Patel",
    "Kim",
    "Martinez",
    "Brown",
    "Okafor",
    "Smith",
    "Hernandez",
    "Lee",
    "Williams",
    "Cohen",
    "Rodriguez",
    "Jones",
    "Singh",
    "Miller",
    "Lopez",
    "Davis",
    "Chen",
    "Wilson",
    "Anderson",
    "Haddad",
    "Thomas",
    "Taylor",
    "Moore",
    "Jackson",
    "Martin",
    "Thompson",
    "White",
    "Harris",
    "Clark",
    "Lewis",
    "Robinson",
    "Walker",
    "Young",
    "Allen",
    "King",
    "Wright",
    "Scott",
];

/// The expected date of birth of student `i` (ISO), from the month-first text in the file.
fn dob(i: usize) -> (String, String) {
    let grade = i % 13;
    let year = 2021 - grade as i32 - (i % 2) as i32;
    // Days 1–12 make every numeric date ambiguous: the file must be read month-first.
    let month = (i % 12) + 1;
    let day = (i % 11) + 1;
    (
        format!("{month:02}/{day:02}/{year}"),
        format!("{year}-{month:02}-{day:02}"),
    )
}

/// Every 10th student (i % 10 == 9) is the younger sibling of the one before: same family id,
/// same guardians, same last name.
fn family_of(i: usize) -> usize {
    if i % 10 == 9 {
        i - 1
    } else {
        i
    }
}

/// The Maple Ridge demo workbook as the test report described it: a "Read Me" sheet first,
/// then "Students" and "Staff". The student register has first and last names, month-first
/// dates, "Grade" and "Homeroom", two numbered guardian column groups, a Family ID that
/// links siblings, US phone formatting, emails and a street address.
fn maple_ridge_xlsx() -> Vec<u8> {
    let mut wb = rust_xlsxwriter::Workbook::new();
    let readme = wb.add_worksheet();
    readme.set_name("Read Me").unwrap();
    readme
        .write_string(
            0,
            0,
            "Maple Ridge demo export. Data on the Students and Staff sheets.",
        )
        .unwrap();
    let ws = wb.add_worksheet();
    ws.set_name("Students").unwrap();
    let headers = [
        "Student ID",
        "First Name",
        "Last Name",
        "Date of Birth",
        "Gender",
        "Grade",
        "Homeroom",
        "Guardian 1 Name",
        "Guardian 1 Relationship",
        "Guardian 1 Phone",
        "Guardian 1 Email",
        "Guardian 2 Name",
        "Guardian 2 Relationship",
        "Guardian 2 Phone",
        "Guardian 2 Email",
        "Family ID",
        "Street Address",
        "Enrollment Date",
    ];
    for (c, h) in headers.iter().enumerate() {
        ws.write_string(0, c as u16, *h).unwrap();
    }
    for i in 0..STUDENTS {
        let fam = family_of(i);
        let first = FIRST[i % 40];
        let last = LAST[(fam / 40) % 40];
        let grade = GRADES[i % 13];
        let grade_text = if i % 10 == 0 && grade != "K" {
            format!("Grade {grade}")
        } else {
            grade.to_string()
        };
        let section = ["A", "B", "C"][(i / 13) % 3];
        let parent_first = FIRST[(fam + 17) % 40];
        let parent2_first = FIRST[(fam + 23) % 40];
        let relationship = ["Mother", "Father", "Guardian"][fam % 3];
        let relationship2 = ["Father", "Mother", "Grandmother"][fam % 3];
        let (dob_text, _) = dob(i);
        // Every third family has no second guardian.
        let has_second = fam % 3 != 2;
        let second = |v: String| if has_second { v } else { String::new() };
        let row = [
            format!("MR-{:05}", i + 1),
            first.to_string(),
            last.to_string(),
            dob_text,
            if i % 2 == 0 { "F" } else { "M" }.to_string(),
            grade_text,
            section.to_string(),
            format!("{parent_first} {last}"),
            relationship.to_string(),
            format!("(503) 555-{:04}", (fam * 37) % 10000),
            format!(
                "{}.{}{}@example.com",
                parent_first.to_lowercase(),
                last.to_lowercase(),
                fam
            ),
            second(format!("{parent2_first} {last}")),
            second(relationship2.to_string()),
            second(format!("503-555-{:04}", (fam * 53 + 1) % 10000)),
            second(format!(
                "{}.{}{}@example.com",
                parent2_first.to_lowercase(),
                last.to_lowercase(),
                fam
            )),
            format!("FAM-{fam:04}"),
            format!("{} NE Maple St, Portland, OR 97211", 100 + fam),
            "08/24/2026".to_string(),
        ];
        for (c, v) in row.iter().enumerate() {
            ws.write_string((i + 1) as u32, c as u16, v).unwrap();
        }
    }
    let ws = wb.add_worksheet();
    ws.set_name("Staff").unwrap();
    let headers = [
        "Employee ID",
        "First Name",
        "Last Name",
        "Gender",
        "Email",
        "Phone",
        "Job Title",
        "Hire Date",
    ];
    for (c, h) in headers.iter().enumerate() {
        ws.write_string(0, c as u16, *h).unwrap();
    }
    for i in 0..STAFF {
        let first = FIRST[(i * 3) % 40];
        let last = LAST[(i * 7 + 5) % 40];
        let title = [
            "Teacher",
            "Teacher",
            "Teacher",
            "Counselor",
            "Office Manager",
        ][i % 5];
        let row = [
            format!("E-{:03}", i + 1),
            first.to_string(),
            last.to_string(),
            if i % 3 == 0 { "Male" } else { "Female" }.to_string(),
            format!(
                "{}.{}@mapleridge.example",
                first.to_lowercase(),
                last.to_lowercase()
            ),
            format!("+1 (503) 555-{:04}", 9000 + i),
            title.to_string(),
            format!("08/15/{}", 2010 + (i % 15)),
        ];
        for (c, v) in row.iter().enumerate() {
            ws.write_string((i + 1) as u32, c as u16, v).unwrap();
        }
    }
    wb.save_to_buffer().unwrap()
}

use base64::engine::general_purpose::STANDARD;
use base64::Engine;

#[tokio::test]
async fn us_school_setup_defaults_and_country_rules() {
    let app = app!();
    // Country must be in the list; the start month may be any month.
    let (s, e) = app
        .call(
            Method::POST,
            "/setup",
            None,
            Some(json!({
                "school_name": "X", "school_code": "X1", "academic_year_start_month": 9, "country": "ZZ",
                "admin_full_name": "A", "admin_username": "admin", "admin_password": "admin-password-1",
                "backup_password": "backup-password-1"
            })),
        )
        .await;
    assert_eq!(
        (s, e["error"]["code"].as_str()),
        (StatusCode::BAD_REQUEST, Some("validation")),
        "{e}"
    );
    let (admin, _school_id) = app.seed_us().await;
    let (s, school) = app.get("/schools/current", &admin).await;
    assert_eq!(s, StatusCode::OK, "{school}");
    assert_eq!(
        (
            school["country"].as_str(),
            school["currency"].as_str(),
            school["currency_symbol"].as_str(),
            school["time_zone"].as_str(),
            school["id_label"].as_str(),
            school["date_order"].as_str(),
            school["academic_year_start_month"].as_i64(),
        ),
        (
            Some("US"),
            Some("USD"),
            Some("$"),
            Some("America/Los_Angeles"),
            Some("National ID"),
            Some("month_first"),
            Some(8)
        ),
        "{school}"
    );
    // The weekend defaults by country: Saturday and Sunday.
    let (s, cal) = app.get("/attendance/calendar", &admin).await;
    assert_eq!(
        (s, cal["weekend_days"].clone()),
        (StatusCode::OK, json!([6, 7]))
    );
    // Pakistan's 13-digit CNIC rule does not apply: any ID and a US phone are accepted.
    let (s, student) = app
        .post(
            "/students",
            &admin,
            json!({
                "full_name": "Noah Patel", "father_name": "Raj Patel", "date_of_birth": "2015-03-04",
                "gender": "male", "admission_date": "2026-08-24", "cnic": "or-4471-22",
                "guardians": [{ "full_name": "Priya Patel", "relationship": "mother", "phone": "(503) 555-0142", "cnic": "OR 99 1" }]
            }),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{student}");
    assert_eq!(student["cnic"], "OR-4471-22");
    let (_, detail) = app
        .get(
            &format!("/students/{}", student["id"].as_str().unwrap()),
            &admin,
        )
        .await;
    assert_eq!(detail["guardians"][0]["phone"], "5035550142");
    assert_eq!(detail["guardians"][0]["cnic"], "OR 99 1");
    let (s, e) = app
        .post(
            "/students",
            &admin,
            json!({
                "full_name": "Ava Kim", "father_name": "Min Kim", "date_of_birth": "2015-03-04",
                "gender": "female", "guardians": [{ "full_name": "Min Kim", "relationship": "father", "phone": "12" }]
            }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    assert!(
        e["error"]["message"]
            .as_str()
            .unwrap()
            .contains("7–15 digits"),
        "{e}"
    );
    // The profile can be changed later; the summary follows.
    let (s, updated) = app
        .patch(
            "/schools/current",
            &admin,
            json!({ "time_zone": "America/Chicago", "currency": "CAD" }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{updated}");
    assert_eq!(
        (
            updated["time_zone"].as_str(),
            updated["currency_symbol"].as_str()
        ),
        (Some("America/Chicago"), Some("C$"))
    );
    let (s, e) = app
        .patch(
            "/schools/current",
            &admin,
            json!({ "time_zone": "Mars/Olympus" }),
        )
        .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{e}");
    let (s, locale) = app.call(Method::GET, "/system/locale", None, None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(locale["countries"].as_array().unwrap().len() >= 20);
    assert!(locale["time_zones"].as_array().unwrap().len() > 300);
    app.finish().await;
}

#[tokio::test]
async fn maple_ridge_register_imports_in_full() {
    let app = app!();
    let (admin, _school_id) = app.seed_us().await;

    // Students: headers map on their own, including first/last name and parent columns.
    let content = STANDARD.encode(maple_ridge_xlsx());
    let (s, parsed) = app
        .post(
            "/imports/parse",
            &admin,
            json!({ "kind": "students", "file_name": "Madrasa_Demo_MapleRidge_USA.xlsx", "content_base64": content }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{parsed}");
    assert_eq!(parsed["sheets"], json!(["Read Me", "Students", "Staff"]));
    assert_eq!(parsed["sheet"], "Students");
    assert_eq!(parsed["rows"].as_array().unwrap().len(), STUDENTS);
    let mapping = parsed["mapping"].as_object().unwrap();
    for key in [
        "student_number",
        "first_name",
        "last_name",
        "date_of_birth",
        "gender",
        "class",
        "section",
        "guardian_name",
        "guardian_relationship",
        "guardian_phone",
        "guardian_email",
        "guardian2_name",
        "guardian2_relationship",
        "guardian2_phone",
        "guardian2_email",
        "family_id",
        "address",
        "admission_date",
    ] {
        assert!(mapping.contains_key(key), "{key} not mapped: {mapping:?}");
    }
    assert!(!mapping.contains_key("full_name") && !mapping.contains_key("father_name"));
    let body = |save: bool| json!({ "kind": "students", "file_name": "Madrasa_Demo_MapleRidge_USA.xlsx", "headers": parsed["headers"], "rows": parsed["rows"], "mapping": parsed["mapping"], "options": { "duplicates": "skip" }, "save_mapping": save });
    let (s, preview) = app.post("/imports/preview", &admin, body(false)).await;
    assert_eq!(s, StatusCode::OK, "{preview}");
    let errors: Vec<&Value> = preview["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["action"] == "error")
        .take(5)
        .collect();
    assert_eq!(
        (
            preview["summary"]["create"].as_u64(),
            preview["summary"]["error"].as_u64()
        ),
        (Some(STUDENTS as u64), Some(0)),
        "first errors: {errors:?}"
    );
    let (s, result) = app.post("/imports/apply", &admin, body(true)).await;
    assert_eq!(s, StatusCode::OK, "{result}");
    assert_eq!(
        result["summary"]["create"].as_u64(),
        Some(STUDENTS as u64),
        "{result}"
    );

    // Spot checks: month-first dates, names joined, the parent stands in for the father,
    // the phone and email kept, the class ("Grade 3" / "K") and homeroom matched.
    let (s, page) = app.get("/students?q=MR-00004&page_size=5", &admin).await;
    assert_eq!(s, StatusCode::OK, "{page}");
    let row = &page["items"][0];
    assert_eq!(page["total"].as_i64(), Some(1), "{page}");
    assert_eq!(row["full_name"], format!("{} {}", FIRST[3], LAST[0]));
    assert_eq!(row["date_of_birth"], dob(3).1);
    let (_, detail) = app
        .get(
            &format!("/students/{}", row["id"].as_str().unwrap()),
            &admin,
        )
        .await;
    assert_eq!(detail["father_name"], format!("{} {}", FIRST[20], LAST[0]));
    assert_eq!(detail["admission_date"], "2026-08-24");
    assert_eq!(detail["guardians"][0]["relationship"], "mother");
    // Row 3's phone was "(503) 555-0111".
    assert_eq!(detail["guardians"][0]["phone"], "5035550111");
    assert_eq!(
        detail["guardians"][0]["email"],
        format!(
            "{}.{}3@example.com",
            FIRST[20].to_lowercase(),
            LAST[0].to_lowercase()
        )
    );
    // Family 3 has a second guardian (3 % 3 == 0): linked, not primary, own phone.
    let gs = detail["guardians"].as_array().unwrap();
    assert_eq!(gs.len(), 2, "{gs:?}");
    assert_eq!(
        (
            gs[1]["relationship"].as_str(),
            gs[1]["is_primary"].as_bool(),
            gs[1]["phone"].as_str()
        ),
        (Some("father"), Some(false), Some("5035550160")),
        "{gs:?}"
    );
    // Rows 8 and 9 are siblings (FAM-0008): one guardian record for the family, so each
    // lists the other as a sibling and the sibling discount can apply.
    let (_, older) = app.get("/students?q=MR-00009&page_size=5", &admin).await;
    let (_, older) = app
        .get(
            &format!("/students/{}", older["items"][0]["id"].as_str().unwrap()),
            &admin,
        )
        .await;
    let (_, younger) = app.get("/students?q=MR-00010&page_size=5", &admin).await;
    let (_, younger) = app
        .get(
            &format!("/students/{}", younger["items"][0]["id"].as_str().unwrap()),
            &admin,
        )
        .await;
    assert_eq!(older["custom"]["family_id"], "FAM-0008");
    assert_eq!(younger["custom"]["family_id"], "FAM-0008");
    assert_eq!(
        older["guardians"][0]["id"], younger["guardians"][0]["id"],
        "one guardian record for the family"
    );
    assert_eq!(
        younger["siblings"].as_array().unwrap().len(),
        1,
        "{younger}"
    );
    assert_eq!(younger["siblings"][0]["id"], older["id"]);
    // Family 2 (2 % 3 == 2) has one guardian only.
    let (_, solo) = app.get("/students?q=MR-00003&page_size=5", &admin).await;
    let (_, solo) = app
        .get(
            &format!("/students/{}", solo["items"][0]["id"].as_str().unwrap()),
            &admin,
        )
        .await;
    assert_eq!(solo["guardians"].as_array().unwrap().len(), 1);
    assert_eq!(detail["current"]["class_name"], GRADES[3]);
    assert_eq!(detail["current"]["section_name"], "A");
    // Row 0 is in kindergarten ("K"), row 10 was written as "Grade 10".
    let (_, k) = app.get("/students?q=MR-00001&page_size=5", &admin).await;
    let (_, k) = app
        .get(
            &format!("/students/{}", k["items"][0]["id"].as_str().unwrap()),
            &admin,
        )
        .await;
    assert_eq!(k["current"]["class_name"], "K");
    let (_, g) = app.get("/students?q=MR-00011&page_size=5", &admin).await;
    let (_, g) = app
        .get(
            &format!("/students/{}", g["items"][0]["id"].as_str().unwrap()),
            &admin,
        )
        .await;
    assert_eq!(g["current"]["class_name"], "10");
    let (_, all) = app.get("/students?page_size=1", &admin).await;
    assert_eq!(all["total"].as_i64(), Some(STUDENTS as i64));

    // Staff: the same workbook; "Staff" is picked for a staff import, and a sheet can also be
    // named explicitly.
    let (s, parsed) = app
        .post(
            "/imports/parse",
            &admin,
            json!({ "kind": "staff", "file_name": "Madrasa_Demo_MapleRidge_USA.xlsx", "content_base64": content }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{parsed}");
    assert_eq!(parsed["sheet"], "Staff");
    let (s, explicit) = app
        .post(
            "/imports/parse",
            &admin,
            json!({ "kind": "staff", "file_name": "Madrasa_Demo_MapleRidge_USA.xlsx", "content_base64": content, "sheet": "Read Me" }),
        )
        .await;
    assert_eq!(
        (s, explicit["sheet"].as_str()),
        (StatusCode::OK, Some("Read Me"))
    );
    assert_eq!(explicit["rows"].as_array().unwrap().len(), 0, "{explicit}");
    let mapping = parsed["mapping"].as_object().unwrap();
    for key in [
        "staff_number",
        "first_name",
        "last_name",
        "gender",
        "email",
        "phone",
        "designation",
        "joined_on",
    ] {
        assert!(mapping.contains_key(key), "{key} not mapped: {mapping:?}");
    }
    let body = json!({ "kind": "staff", "file_name": "Madrasa_Demo_MapleRidge_USA.xlsx", "headers": parsed["headers"], "rows": parsed["rows"], "mapping": parsed["mapping"], "options": { "duplicates": "skip" }, "save_mapping": true });
    let (s, preview) = app.post("/imports/preview", &admin, body.clone()).await;
    assert_eq!(s, StatusCode::OK, "{preview}");
    let errors: Vec<&Value> = preview["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["action"] == "error")
        .take(5)
        .collect();
    assert_eq!(
        (
            preview["summary"]["create"].as_u64(),
            preview["summary"]["error"].as_u64()
        ),
        (Some(STAFF as u64), Some(0)),
        "first errors: {errors:?}"
    );
    let (s, result) = app.post("/imports/apply", &admin, body).await;
    assert_eq!(s, StatusCode::OK, "{result}");
    assert_eq!(result["summary"]["create"].as_u64(), Some(STAFF as u64));
    let (s, staff) = app.get("/staff?q=E-004", &admin).await;
    assert_eq!(s, StatusCode::OK, "{staff}");
    let member = &staff[0];
    assert_eq!(member["designation"], "Counselor");
    assert_eq!(member["phone"], "+15035559003");
    assert_eq!(member["joined_on"], "2013-08-15");
    app.finish().await;
}
