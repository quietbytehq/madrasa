//! Test harness: a hub on a throwaway database, called through its router.
#![allow(dead_code)]

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use http_body_util::BodyExt;
use madrasa_db::testing::TestDb;
use madrasa_nexus::{store, Config, HubState};
use serde_json::{json, Value};
use tower::ServiceExt;

pub const SETUP_TOKEN: &str = "test-setup-token";
pub const ADMIN_PASSWORD: &str = "admin-password-1";

pub struct Hub {
    pub db: TestDb,
    pub state: HubState,
    pub router: axum::Router,
}

pub fn config(url: &str, setup_token: Option<&str>) -> Config {
    Config {
        database_url: url.to_string(),
        bind: "127.0.0.1:0".parse().unwrap(),
        public_url: Some("https://hub.example.org".into()),
        setup_token: setup_token.map(str::to_string),
        console_dir: None,
    }
}

impl Hub {
    /// A migrated hub with first-run setup still open, or `None` without a test database.
    pub async fn new() -> Option<Hub> {
        let db = TestDb::empty().await?;
        let pool = store::connect(&db.url).await.expect("connect hub pool");
        store::migrate(&pool).await.expect("migrate hub");
        let (state, _) = HubState::new(pool, &config(&db.url, Some(SETUP_TOKEN)))
            .await
            .expect("hub state");
        let router = madrasa_nexus::router(state.clone());
        Some(Hub { db, state, router })
    }

    pub async fn call(
        &self,
        method: Method,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method)
            .uri(format!("{}{path}", madrasa_nexus::API_PREFIX));
        if let Some(t) = token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
        }
        let req = match body {
            Some(b) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string()))
                .unwrap(),
            None => req.body(Body::empty()).unwrap(),
        };
        let res = self.router.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes)
                .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
        };
        (status, value)
    }

    pub async fn get(&self, path: &str, token: &str) -> (StatusCode, Value) {
        self.call(Method::GET, path, Some(token), None).await
    }
    pub async fn post(&self, path: &str, token: &str, body: Value) -> (StatusCode, Value) {
        self.call(Method::POST, path, Some(token), Some(body)).await
    }
    pub async fn patch(&self, path: &str, token: &str, body: Value) -> (StatusCode, Value) {
        self.call(Method::PATCH, path, Some(token), Some(body))
            .await
    }

    /// Run first-run setup and return the administrator's session token.
    pub async fn setup(&self) -> String {
        let (s, b) = self
            .call(
                Method::POST,
                "/setup",
                None,
                Some(json!({
                    "setup_token": SETUP_TOKEN, "hub_name": "Gulshan Education Network",
                    "admin_username": "head.office", "admin_full_name": "Head Office",
                    "admin_password": ADMIN_PASSWORD,
                })),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        self.login("head.office", ADMIN_PASSWORD).await
    }

    pub async fn login(&self, username: &str, password: &str) -> String {
        let (s, b) = self
            .call(
                Method::POST,
                "/auth/login",
                None,
                Some(json!({ "username": username, "password": password })),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        b["token"].as_str().unwrap().to_string()
    }

    pub async fn finish(self) {
        self.state.pool.close().await;
        self.db.cleanup().await;
    }
}

/// Skip the current test unless a test database is available.
#[macro_export]
macro_rules! hub {
    () => {
        match $crate::common::Hub::new().await {
            Some(h) => h,
            None => return,
        }
    };
}

// ---- a school, as the hub sees one ----------------------------------------------------------

use chrono::NaiveDate;
use madrasa_core::hlc::Hlc;
use madrasa_core::password::hash_password;
use madrasa_core::sync::{
    AckRequest, AckResponse, JoinRequest, Origin, PullRequest, PullResponse, PushRequest,
    PushResponse,
};
use madrasa_db::models::School;
use madrasa_db::sync::round::{self, RoundError, RoundReport, Transport, TransportError};
use madrasa_db::{academics, schools, students, sync, users, Actor};
use uuid::Uuid;

/// A school install reduced to what matters to the hub: its database and its sync token.
pub struct SchoolSim {
    pub db: TestDb,
    pub hlc: Hlc,
    pub school: School,
    pub token: Option<String>,
    /// The hub's id for this membership, once joined.
    pub school_ref: Option<Uuid>,
}

/// Fill a database with a small school: a year, a class, a user and two students.
pub async fn seed_school(db: &TestDb, hlc: &Hlc, name: &str, code: &str) -> School {
    let mut c = db.pool.acquire().await.unwrap();
    let school = schools::create(
        &mut c,
        hlc,
        Actor::SYSTEM,
        schools::NewSchool {
            name: name.into(),
            code: code.into(),
            city: "Karachi".into(),
            is_demo: false,
            academic_year_start_month: 4,
            currency: "PKR".into(),
            country: "PK".into(),
            time_zone: "Asia/Karachi".into(),
        },
    )
    .await
    .unwrap();
    let actor = Actor::system_for(school.id);
    let (year, start, end) = academics::default_year_bounds(2026, 4).unwrap();
    academics::create_year(
        &mut c,
        hlc,
        actor,
        academics::NewAcademicYear {
            school_id: school.id,
            name: year,
            start_date: start,
            end_date: end,
            start_month: 4,
        },
    )
    .await
    .unwrap();
    academics::create_class(&mut c, hlc, actor, school.id, "5", 5)
        .await
        .unwrap();
    users::create(
        &mut c,
        hlc,
        actor,
        users::NewUser {
            school_id: school.id,
            username: "ayesha".into(),
            full_name: "Ayesha Khan".into(),
            email: None,
            password_hash: hash_password("school-password-1").unwrap(),
        },
    )
    .await
    .unwrap();
    for pupil in ["Ali Khan", "Sara Noor"] {
        students::create(
            &mut c,
            hlc,
            actor,
            students::NewStudent {
                school_id: school.id,
                student_number: None,
                legacy_id: None,
                full_name: pupil.into(),
                father_name: "Father".into(),
                date_of_birth: NaiveDate::from_ymd_opt(2015, 5, 1).unwrap(),
                gender: "male".into(),
                cnic: None,
                address: "".into(),
                admission_date: NaiveDate::from_ymd_opt(2026, 4, 10).unwrap(),
                board_roll_no: None,
                board_registration_no: None,
                custom: json!({}),
            },
        )
        .await
        .unwrap();
    }
    school
}

impl SchoolSim {
    pub async fn new(name: &str, code: &str) -> Option<SchoolSim> {
        let db = TestDb::new().await?;
        let hlc = Hlc::new(0x5c);
        let school = seed_school(&db, &hlc, name, code).await;
        Some(SchoolSim {
            db,
            hlc,
            school,
            token: None,
            school_ref: None,
        })
    }

    /// The join request this school would send with a join code.
    pub async fn join_request(&self, code: &str) -> JoinRequest {
        let mut c = self.db.pool.acquire().await.unwrap();
        JoinRequest {
            join_code: code.to_string(),
            school: sync::school_record(&mut c, self.school.id).await.unwrap(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            schema_version: sync::schema_version(&mut c).await.unwrap(),
            edition: "lite".into(),
        }
    }

    /// Join the hub with a code. On success the school switches capture on, queues
    /// everything it has and keeps its token.
    pub async fn join(&mut self, hub: &Hub, code: &str) -> (StatusCode, Value) {
        let request = self.join_request(code).await;
        let (status, body) = hub
            .call(Method::POST, "/join", None, Some(json!(request)))
            .await;
        if status == StatusCode::OK {
            self.token = Some(body["token"].as_str().unwrap().to_string());
            self.school_ref = Some(body["school_ref"].as_str().unwrap().parse().unwrap());
            let mut c = self.db.pool.begin().await.unwrap();
            sync::enable(&mut c, self.school.id, Origin::School)
                .await
                .unwrap();
            sync::queue_snapshot(&mut c).await.unwrap();
            c.commit().await.unwrap();
        }
        (status, body)
    }

    /// One sync round against the hub, exactly as a school's API runs it.
    pub async fn sync(&self, hub: &Hub) -> Result<RoundReport, RoundError> {
        let transport = RouterTransport {
            hub,
            token: self.token.clone().unwrap_or_default(),
        };
        round::run(
            &self.db.pool,
            &self.hlc,
            env!("CARGO_PKG_VERSION"),
            &transport,
        )
        .await
    }

    pub async fn student_names(&self) -> Vec<String> {
        sqlx::query_scalar("SELECT full_name FROM students ORDER BY full_name")
            .fetch_all(&self.db.pool)
            .await
            .unwrap()
    }

    pub async fn rename_student(&self, from: &str, to: &str) {
        sqlx::query(
            "UPDATE students SET full_name = $2, updated_at = now(), hlc = $3 WHERE full_name = $1",
        )
        .bind(from)
        .bind(to)
        .bind(self.hlc.now().to_string())
        .execute(&self.db.pool)
        .await
        .unwrap();
    }
}

/// Calls the hub's router as a school would over HTTP.
pub struct RouterTransport<'a> {
    pub hub: &'a Hub,
    pub token: String,
}

impl RouterTransport<'_> {
    async fn post<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: Value,
    ) -> Result<T, TransportError> {
        let (status, value) = self
            .hub
            .call(Method::POST, path, Some(&self.token), Some(body))
            .await;
        if status == StatusCode::OK {
            return serde_json::from_value(value)
                .map_err(|e| TransportError::Offline(e.to_string()));
        }
        let code = value["error"]["code"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let message = value["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        if status == StatusCode::GONE {
            Err(TransportError::Removed(message))
        } else {
            Err(TransportError::Refused { code, message })
        }
    }
}

impl Transport for RouterTransport<'_> {
    async fn push(&self, req: PushRequest) -> Result<PushResponse, TransportError> {
        self.post("/sync/push", json!(req)).await
    }
    async fn pull(&self, req: PullRequest) -> Result<PullResponse, TransportError> {
        self.post("/sync/pull", json!(req)).await
    }
    async fn ack(&self, req: AckRequest) -> Result<AckResponse, TransportError> {
        self.post("/sync/ack", json!(req)).await
    }
}

impl Hub {
    /// Create an invitation and return its join code.
    pub async fn invite(&self, admin: &str, school_name: &str) -> String {
        let (s, b) = self
            .post("/invitations", admin, json!({ "school_name": school_name }))
            .await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
        b["join_code"].as_str().unwrap().to_string()
    }

    /// The register row of a school by its hub id.
    pub async fn school_row(&self, admin: &str, id: Uuid) -> Value {
        let (s, list) = self.get("/schools", admin).await;
        assert_eq!(s, StatusCode::OK, "{list}");
        list.as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == json!(id))
            .cloned()
            .unwrap_or(Value::Null)
    }

    /// Run a query inside one school's schema, as the hub does.
    pub async fn in_school<T>(&self, id: Uuid, sql: &'static str) -> Vec<T>
    where
        T: for<'r> sqlx::Decode<'r, sqlx::Postgres> + sqlx::Type<sqlx::Postgres> + Send + Unpin,
    {
        let mut conn = self.state.pool.acquire().await.unwrap();
        let school = madrasa_nexus::tenants::get(&mut conn, id)
            .await
            .unwrap()
            .unwrap();
        drop(conn);
        let mut tx = madrasa_nexus::tenants::begin(&self.state.pool, &school)
            .await
            .unwrap();
        let rows = sqlx::query_scalar::<_, T>(sql)
            .fetch_all(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        rows
    }
}

impl Hub {
    /// GET a download: status, content type and the raw bytes.
    pub async fn get_bytes(&self, path: &str, token: &str) -> (StatusCode, String, Vec<u8>) {
        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("{}{path}", madrasa_nexus::API_PREFIX))
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let res = self.router.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let ctype = res
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
        (status, ctype, bytes)
    }
}

// ---- parents ---------------------------------------------------------------------------------

/// A guardian in a school's own records, with one child (or none).
pub async fn guardian(sim: &SchoolSim, name: &str, child: Option<&str>) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO guardians (id, school_id, full_name, phone, hlc) VALUES ($1, $2, $3, '0300-0000000', $4)")
        .bind(id)
        .bind(sim.school.id)
        .bind(name)
        .bind(sim.hlc.now().to_string())
        .execute(&sim.db.pool)
        .await
        .unwrap();
    if let Some(child) = child {
        sqlx::query(
            "INSERT INTO student_guardians (id, school_id, student_id, guardian_id, relationship, is_primary, hlc)
             SELECT $1, $2, s.id, $3, 'father', TRUE, $4 FROM students s WHERE s.full_name = $5",
        )
        .bind(Uuid::now_v7())
        .bind(sim.school.id)
        .bind(id)
        .bind(sim.hlc.now().to_string())
        .bind(child)
        .execute(&sim.db.pool)
        .await
        .unwrap();
    }
    id
}

pub async fn joined(hub: &Hub, admin: &str, name: &str, code: &str) -> SchoolSim {
    let mut sim = SchoolSim::new(name, code).await.unwrap();
    let join_code = hub.invite(admin, name).await;
    let (s, b) = sim.join(hub, &join_code).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    sim
}

pub async fn as_school(
    hub: &Hub,
    sim: &SchoolSim,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    hub.call(method, path, sim.token.as_deref(), body).await
}

/// The school asks for a code for a guardian and gets it back in clear, once.
pub async fn code_for(hub: &Hub, sim: &SchoolSim, guardian: Uuid) -> String {
    let (s, b) = as_school(
        hub,
        sim,
        Method::POST,
        "/sync/parents/codes",
        Some(json!({ "guardian_id": guardian })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    b["code"].as_str().unwrap().to_string()
}

pub async fn activate(hub: &Hub, code: &str, phone: &str, password: &str) -> (StatusCode, Value) {
    hub.call(
        Method::POST,
        "/parent/activate",
        None,
        Some(json!({ "code": code, "phone": phone, "password": password })),
    )
    .await
}
