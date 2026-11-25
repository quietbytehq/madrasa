//! Every API route, called as every built-in role, checked against `docs/permissions.md`.
//!
//! The document's "Routes" table says which permission each route needs. This test
//! - reads every `.route(...)` from `crates/api/src/routes/*.rs` and fails if one is missing
//!   from the table (a new route cannot ship without saying who may call it), or the table
//!   lists one that no longer exists;
//! - calls each route without a login (must be `401`, unless the table says `public`);
//! - calls each route as principal, admin, accountant, teacher, librarian and store keeper,
//!   and checks that the answer is `403 forbidden` exactly when the role's default grants
//!   (the matrix at the top of the same document) do not include the permission.
//!
//! Calls are made with made-up ids and an empty body, so a permitted caller usually gets
//! `404` or `422`; what matters is that authorisation is decided first, and decided right.
//! Data scoping inside a permission (a teacher's own sections) is covered by the module
//! tests, not here.
//!
//! `MADRASA_AUTHZ_DUMP=<file>` writes what was observed as JSON instead of asserting (used to
//! draft the table). Needs PostgreSQL via `MADRASA_TEST_DATABASE_URL`.

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use madrasa_api::authz::{self, Access};
use madrasa_api::{router, AppState};
use madrasa_core::password::hash_password;
use madrasa_core::permissions::{Permission, RoleCode};
use madrasa_db::testing::TestDb;
use madrasa_db::{roles, users, Actor};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use tower::ServiceExt;

const ROLES: [RoleCode; 6] = [
    RoleCode::Principal,
    RoleCode::Admin,
    RoleCode::Accountant,
    RoleCode::Teacher,
    RoleCode::Librarian,
    RoleCode::StoreKeeper,
];

/// Who may call a route, as the document states it.
#[derive(Debug, Clone, PartialEq)]
enum Rule {
    /// No login needed.
    Public,
    /// Any signed-in user.
    SignedIn,
    /// Holding any one of these permissions.
    AnyOf(Vec<Permission>),
}

impl Rule {
    fn parse(cell: &str) -> Rule {
        let cell = cell.trim();
        match cell {
            "public" => Rule::Public,
            "signed in" => Rule::SignedIn,
            _ => Rule::AnyOf(
                cell.split(" or ")
                    .map(|p| {
                        p.trim()
                            .trim_matches('`')
                            .parse()
                            .unwrap_or_else(|_| panic!("unknown permission in the routes table: {p}"))
                    })
                    .collect(),
            ),
        }
    }

    fn of(access: Access) -> Rule {
        match access {
            Access::Public => Rule::Public,
            Access::SignedIn => Rule::SignedIn,
            Access::Any(perms) => Rule::AnyOf(perms.to_vec()),
        }
    }

    fn allows(&self, role: RoleCode) -> bool {
        match self {
            Rule::Public | Rule::SignedIn => true,
            Rule::AnyOf(perms) => {
                let grants = role.default_permissions();
                perms.iter().any(|p| grants.contains(p))
            }
        }
    }
}

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// `(METHOD, path)` for every route the API declares.
fn source_routes() -> BTreeSet<(String, String)> {
    let dir = repo_root().join("crates/api/src/routes");
    let mut out = BTreeSet::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        let mut rest = src.as_str();
        while let Some(at) = rest.find(".route(") {
            rest = &rest[at + ".route(".len()..];
            let Some(open) = rest.find('"') else { break };
            // Only whitespace may come between `.route(` and the path.
            if !rest[..open].trim().is_empty() {
                continue;
            }
            let after = &rest[open + 1..];
            let Some(close) = after.find('"') else { break };
            let route = &after[..close];
            // The method chain runs to the parenthesis that closes `.route(`.
            let chain_start = &after[close + 1..];
            let mut depth = 1usize;
            let mut end = 0usize;
            for (i, c) in chain_start.char_indices() {
                match c {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            end = i;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let chain = &chain_start[..end];
            for method in ["get", "post", "put", "patch", "delete"] {
                let needle = format!("{method}(");
                let mut from = 0;
                while let Some(i) = chain[from..].find(&needle) {
                    let i = from + i;
                    let before = chain[..i].chars().next_back();
                    if before.is_none_or(|c| !(c.is_alphanumeric() || c == '_')) {
                        out.insert((method.to_uppercase(), route.to_string()));
                    }
                    from = i + needle.len();
                }
            }
            rest = chain_start;
        }
    }
    out
}

/// The "Routes" table of `docs/permissions.md`: `| GET | `/students` | `students.read` |`.
fn documented_routes() -> BTreeMap<(String, String), Rule> {
    let doc = std::fs::read_to_string(repo_root().join("docs/permissions.md")).unwrap();
    let mut out = BTreeMap::new();
    for line in doc.lines() {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        if cells.len() < 5 || !matches!(cells[1], "GET" | "POST" | "PUT" | "PATCH" | "DELETE") {
            continue;
        }
        let path = cells[2].trim_matches('`').to_string();
        let previous = out.insert((cells[1].to_string(), path.clone()), Rule::parse(cells[3]));
        assert!(previous.is_none(), "{} {path} is listed twice", cells[1]);
    }
    out
}

struct App {
    _db: TestDb,
    state: AppState,
    app: Router,
}

#[derive(Debug, Clone, PartialEq)]
enum Outcome {
    Unauthenticated,
    Forbidden,
    /// Authorisation passed (or was never asked): the status and error code that came back.
    Other(u16, String),
}

impl App {
    async fn call(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(Method::from_bytes(method.as_bytes()).unwrap())
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
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    async fn login(&self, username: &str, password: &str) -> String {
        let (s, b) = self
            .call(
                "POST",
                "/auth/login",
                None,
                Some(json!({ "username": username, "password": password })),
            )
            .await;
        assert_eq!(s, StatusCode::OK, "{b}");
        b["token"].as_str().unwrap().to_string()
    }

    /// Call a route the way the matrix does: made-up ids, an empty JSON body.
    async fn probe(&self, method: &str, route: &str, token: Option<&str>) -> Outcome {
        let path = concrete(route);
        let body = (method != "GET" && method != "DELETE").then(|| json!({}));
        let (status, value) = self.call(method, &path, token, body).await;
        let code = value["error"]["code"].as_str().unwrap_or("").to_string();
        match (status, code.as_str()) {
            (StatusCode::UNAUTHORIZED, "unauthenticated") => Outcome::Unauthenticated,
            (StatusCode::FORBIDDEN, "forbidden") => Outcome::Forbidden,
            _ => Outcome::Other(status.as_u16(), code),
        }
    }
}

/// Fill a route's `{parameters}` with harmless made-up values.
fn concrete(route: &str) -> String {
    let mut out = String::new();
    for part in route.split('/') {
        if part.is_empty() {
            continue;
        }
        out.push('/');
        if part.starts_with('{') {
            out.push_str(match part.trim_matches(|c| c == '{' || c == '}') {
                "kind" => "students",
                "code" => "no-such-code",
                "month" => "2026-04-01",
                "date" => "2026-04-06",
                "number" => "X-0000",
                "format" => "xlsx",
                _ => "0199aaaa-0000-7000-8000-00000000abcd",
            });
        } else {
            out.push_str(part);
        }
    }
    out
}

#[tokio::test]
async fn every_route_as_every_role_matches_the_permissions_document() {
    let Some(db) = TestDb::new().await else {
        eprintln!("skipped: MADRASA_TEST_DATABASE_URL is not set");
        return;
    };
    // Anything a permitted call writes to disk (a backup) goes to a throwaway folder.
    let data_dir = std::env::temp_dir().join(format!("madrasa-authz-{}", uuid::Uuid::now_v7()));
    std::env::set_var("MADRASA_DATA_DIR", &data_dir);
    let state = AppState::new(db.pool.clone()).await.unwrap();
    let app = App {
        app: router(state.clone()),
        state,
        _db: db,
    };

    // The routes that need no school at all are asked before one exists.
    let source = source_routes();
    assert!(source.len() > 300, "the route scan found {}", source.len());
    let dump = std::env::var("MADRASA_AUTHZ_DUMP").ok();
    let documented = documented_routes();
    if dump.is_none() {
        let missing: Vec<_> = source
            .iter()
            .filter(|r| !documented.contains_key(*r))
            .map(|(m, p)| format!("{m} {p}"))
            .collect();
        assert!(
            missing.is_empty(),
            "routes missing from the Routes table in docs/permissions.md:\n  {}",
            missing.join("\n  ")
        );
        let gone: Vec<_> = documented
            .keys()
            .filter(|r| !source.contains(*r))
            .map(|(m, p)| format!("{m} {p}"))
            .collect();
        assert!(
            gone.is_empty(),
            "docs/permissions.md lists routes the API no longer has:\n  {}",
            gone.join("\n  ")
        );
    }

    // A Standard school (every module switched on), one login per built-in role.
    let (s, b) = app
        .call(
            "POST",
            "/setup",
            None,
            Some(json!({
                "school_name": "Gulshan", "school_code": "KHI-MC", "academic_year_start_month": 4,
                "admin_full_name": "Ayesha", "admin_username": "ayesha",
                "admin_password": "admin-password-1", "backup_password": "backup-password-1"
            })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let school_id: uuid::Uuid = b["school_id"].as_str().unwrap().parse().unwrap();
    let admin = app.login("ayesha", "admin-password-1").await;
    let (s, b) = app
        .call(
            "PUT",
            "/settings/edition",
            Some(&admin),
            Some(json!({ "edition": "standard" })),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let mut tokens = Vec::new();
    for role in ROLES {
        let username = format!("role-{}", role.code());
        let mut c = app.state.pool.acquire().await.unwrap();
        let u = users::create(
            &mut c,
            &app.state.hlc,
            Actor::system_for(school_id),
            users::NewUser {
                school_id,
                username: username.clone(),
                full_name: username.clone(),
                email: None,
                password_hash: hash_password("user-password-1").unwrap(),
            },
        )
        .await
        .unwrap();
        let r = roles::find_by_code(&mut c, school_id, role.code())
            .await
            .unwrap();
        roles::assign(
            &mut c,
            &app.state.hlc,
            Actor::system_for(school_id),
            u.id,
            &r,
        )
        .await
        .unwrap();
        drop(c);
        tokens.push((role, app.login(&username, "user-password-1").await));
    }

    // Logging out ends the token, so it is asked last.
    let mut order: Vec<&(String, String)> = source.iter().collect();
    order.sort_by_key(|(_, p)| p == "/auth/logout");

    let mut observed: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut wrong: Vec<String> = Vec::new();
    for (method, route) in order {
        let key = format!("{method} {route}");
        let rule = documented.get(&(method.clone(), route.clone()));
        let anonymous = app.probe(method, route, None).await;
        observed
            .entry(key.clone())
            .or_default()
            .insert("anonymous".into(), format!("{anonymous:?}"));
        if let Some(rule) = rule {
            let expect_401 = *rule != Rule::Public;
            if expect_401 != (anonymous == Outcome::Unauthenticated) {
                wrong.push(format!(
                    "{key}: without a login got {anonymous:?}; the table says {rule:?}"
                ));
            }
        }
        for (role, token) in &tokens {
            let got = app.probe(method, route, Some(token)).await;
            observed
                .entry(key.clone())
                .or_default()
                .insert(role.code().to_string(), format!("{got:?}"));
            let Some(rule) = rule else { continue };
            let allowed = rule.allows(*role);
            match (&got, allowed) {
                (Outcome::Forbidden, false) => {}
                (Outcome::Other(..), true) => {}
                (Outcome::Unauthenticated, _) => wrong.push(format!(
                    "{key}: {role} was signed in and got 401"
                )),
                (Outcome::Forbidden, true) => wrong.push(format!(
                    "{key}: {role} is refused, but the table says {rule:?} and the role has it"
                )),
                (Outcome::Other(status, code), false) => wrong.push(format!(
                    "{key}: {role} is NOT refused (got {status} {code}), but the table says {rule:?} and the role lacks it"
                )),
            }
        }
    }
    let _ = std::fs::remove_dir_all(&data_dir);

    if let Some(file) = dump {
        std::fs::write(&file, serde_json::to_string_pretty(&observed).unwrap()).unwrap();
        eprintln!("observed outcomes written to {file}");
        return;
    }
    assert!(
        wrong.is_empty(),
        "{} route/role pairs do not match docs/permissions.md:\n  {}",
        wrong.len(),
        wrong.join("\n  ")
    );
}

/// The table in the code, the table in the document and the routes in the source are one
/// list: nothing undocumented, nothing documented differently, nothing stale.
#[test]
fn the_code_table_the_document_and_the_routes_agree() {
    let source = source_routes();
    let documented = documented_routes();
    let code: BTreeMap<(String, String), Rule> = authz::ROUTES
        .iter()
        .map(|(m, p, a)| ((m.to_string(), p.to_string()), Rule::of(*a)))
        .collect();
    let names = |set: Vec<&(String, String)>| -> Vec<String> {
        set.into_iter().map(|(m, p)| format!("{m} {p}")).collect()
    };
    let unlisted = names(source.iter().filter(|r| !code.contains_key(*r)).collect());
    assert!(
        unlisted.is_empty(),
        "routes missing from crates/api/src/authz.rs (they would be refused):\n  {}",
        unlisted.join("\n  ")
    );
    let stale = names(code.keys().filter(|r| !source.contains(*r)).collect());
    assert!(stale.is_empty(), "authz.rs lists routes that no longer exist: {stale:?}");
    for (route, rule) in &code {
        assert_eq!(
            documented.get(route),
            Some(rule),
            "docs/permissions.md and authz.rs differ for {} {}",
            route.0,
            route.1
        );
    }
    assert_eq!(documented.len(), code.len(), "the document lists extra routes");
}

#[test]
fn the_route_scan_reads_method_chains() {
    let routes = source_routes();
    for (m, p) in [
        ("POST", "/auth/login"),
        ("GET", "/students"),
        ("POST", "/students"),
        ("GET", "/students/{id}"),
        ("POST", "/users/{id}/password"),
        ("DELETE", "/timetable/covers"),
    ] {
        assert!(
            routes.contains(&(m.to_string(), p.to_string())),
            "{m} {p} not found by the scan"
        );
    }
}
