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
