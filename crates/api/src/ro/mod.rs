//! The read-only API (Standard stage 9), served under `/api/ro/v1` for the school's other
//! systems: API keys with scopes, an IP allow-list and an expiry; a rate limit per key;
//! `ETag` / `If-None-Match` on every answer; every call logged; no writes. On editions
//! without the feature the whole prefix answers 404.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;

use axum::body::{to_bytes, Body};
use axum::extract::{ConnectInfo, Query, Request, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Extension, Json, Router};
use chrono::{DateTime, NaiveDate, Utc};
use madrasa_core::api_keys::{self as rules, RateWindow, Scope, RATE_LIMIT_PER_MINUTE};
use madrasa_core::edition::Feature;
use madrasa_core::session_token;
use madrasa_db::api_keys::ApiKey;
use madrasa_db::{academics, api_keys, library, ro, schools};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{ApiError, ApiResult, AppState};

pub const RO_PREFIX: &str = "/api/ro/v1";

/// One rate window per key, in memory (a restart forgives the current minute).
#[derive(Debug, Default)]
pub struct RateLimits {
    windows: Mutex<HashMap<Uuid, RateWindow>>,
}

impl RateLimits {
    /// `Err(seconds)` when the key is over its limit.
    pub fn tick(&self, key: Uuid, now: DateTime<Utc>) -> Result<(), u32> {
        let mut map = self.windows.lock().unwrap_or_else(|e| e.into_inner());
        let next = rules::rate_tick(map.get(&key).copied(), now, RATE_LIMIT_PER_MINUTE)?;
        map.insert(key, next);
        Ok(())
    }

    pub fn forget(&self, key: Uuid) {
        self.windows
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&key);
    }
}

/// The key a request was made with, handed to the handlers.
#[derive(Debug, Clone)]
pub struct KeyContext {
    pub key: ApiKey,
}

impl KeyContext {
    fn require(&self, scope: Scope) -> ApiResult<()> {
        if self.key.scopes.iter().any(|s| s == scope.as_str()) {
            Ok(())
        } else {
            Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "scope_missing",
                format!("This key does not have the \"{}\" scope.", scope.as_str()),
            ))
        }
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/openapi.json", get(openapi))
        .route("/students", get(students))
        .route("/sections", get(sections))
        .route("/attendance/daily", get(attendance_daily))
        .route("/fees/balances", get(fee_balances))
        .route("/exams/assessments", get(assessments))
        .route("/exams/results", get(results))
        .route("/timetable", get(timetable))
        .route("/library/loans", get(loans))
        .layer(middleware::from_fn_with_state(state.clone(), guard))
        .with_state(state)
}

fn presented_key(req: &Request) -> Option<String> {
    let h = req.headers();
    h.get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .or_else(|| h.get("x-api-key").and_then(|v| v.to_str().ok()))
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

fn peer_ip(req: &Request) -> Option<IpAddr> {
    req.extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip())
}

/// Feature, key, expiry, IP allow-list, rate limit; then the handler; then ETag and the log.
async fn guard(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    if !state.has_feature(Feature::ReadonlyApi) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let method = req.method().to_string();
    // Inside a nested router the URI has lost its prefix; log the full path.
    let path = req
        .extensions()
        .get::<axum::extract::OriginalUri>()
        .map(|u| u.0.path().to_string())
        .unwrap_or_else(|| format!("{RO_PREFIX}{}", req.uri().path()));
    let ip = peer_ip(&req);
    let ip_text = ip.map(|i| i.to_string()).unwrap_or_default();
    let if_none_match = req
        .headers()
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    // The request body is not `Sync`, so nothing borrowing `req` may live across an await.
    let presented = presented_key(&req);
    let key = match authenticate(&state, presented).await {
        Ok(k) => k,
        Err(e) => return e.into_response(),
    };
    if !rules::ip_allowed(&key.ip_allow, ip) {
        let r = ApiError::new(
            StatusCode::FORBIDDEN,
            "ip_not_allowed",
            "This key cannot be used from this address.",
        )
        .into_response();
        return log(&state, &key, &method, &path, r, &ip_text).await;
    }
    let now = Utc::now();
    if let Err(wait) = state.ro_limits.tick(key.id, now) {
        let mut r = ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            format!("Rate limit of {RATE_LIMIT_PER_MINUTE} requests per minute reached."),
        )
        .into_response();
        r.headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from(wait));
        return log(&state, &key, &method, &path, r, &ip_text).await;
    }
    req.extensions_mut().insert(KeyContext { key: key.clone() });
    let res = next.run(req).await;
    let res = with_etag(res, if_none_match.as_deref()).await;
    log(&state, &key, &method, &path, res, &ip_text).await
}

/// The usable key behind the presented secret (unknown keys are not logged: there is no
/// key to log them against).
async fn authenticate(state: &AppState, presented: Option<String>) -> ApiResult<ApiKey> {
    let presented = presented.ok_or_else(ApiError::unauthenticated)?;
    let mut c = state.pool.acquire().await?;
    let key = api_keys::find_by_hash(&mut c, &session_token::hash(&presented))
        .await?
        .ok_or_else(ApiError::unauthenticated)?;
    if !rules::is_usable(key.revoked_at, key.expires_at, Utc::now()) {
        return Err(ApiError::unauthenticated());
    }
    Ok(key)
}

/// Hash the body of a successful JSON answer into a weak ETag; answer 304 on a match.
async fn with_etag(res: Response, if_none_match: Option<&str>) -> Response {
    if res.status() != StatusCode::OK {
        return res;
    }
    let (mut parts, body) = res.into_parts();
    let bytes = match to_bytes(body, 32 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => return ApiError::internal().into_response(),
    };
    let digest = Sha256::digest(&bytes);
    let etag = format!("W/\"{:x}\"", digest);
    if if_none_match.is_some_and(|v| v.split(',').any(|t| t.trim() == etag)) {
        let mut r = StatusCode::NOT_MODIFIED.into_response();
        r.headers_mut()
            .insert(header::ETAG, HeaderValue::from_str(&etag).unwrap());
        return r;
    }
    parts
        .headers
        .insert(header::ETAG, HeaderValue::from_str(&etag).unwrap());
    parts.headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-cache"),
    );
    Response::from_parts(parts, Body::from(bytes))
}

async fn log(
    state: &AppState,
    key: &ApiKey,
    method: &str,
    path: &str,
    res: Response,
    ip: &str,
) -> Response {
    if let Ok(mut c) = state.pool.acquire().await {
        if let Err(e) = api_keys::log_call(
            &mut c,
            &state.hlc,
            key,
            method,
            path,
            res.status().as_u16(),
            ip,
        )
        .await
        {
            tracing::warn!(error = %e, "read-only API call not logged");
        }
    }
    res
}

// ---- handlers -------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct YearQuery {
    pub academic_year_id: Option<Uuid>,
}

async fn year(
    state: &AppState,
    c: &mut sqlx::PgConnection,
    school: Uuid,
    q: Option<Uuid>,
) -> ApiResult<Uuid> {
    let _ = state;
    match q {
        Some(y) => Ok(y),
        None => academics::active_year(c, school)
            .await?
            .map(|y| y.id)
            .ok_or_else(|| ApiError::validation("No active academic year; pass academic_year_id.")),
    }
}

async fn school(c: &mut sqlx::PgConnection) -> ApiResult<Uuid> {
    schools::current(c)
        .await?
        .map(|s| s.id)
        .ok_or_else(|| ApiError::not_found("school"))
}

/// Mirrored by the OpenAPI document: a page of rows.
#[derive(Debug, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub page: i64,
    pub page_size: i64,
    pub total: i64,
}

#[derive(Debug, Serialize)]
pub struct Index {
    pub name: &'static str,
    pub version: &'static str,
    pub key: String,
    pub scopes: Vec<String>,
    pub endpoints: Vec<&'static str>,
    pub rate_limit_per_minute: u32,
}

async fn index(
    State(state): State<AppState>,
    Extension(ctx): Extension<KeyContext>,
) -> Json<Index> {
    Json(Index {
        name: "Madrasa read-only API",
        version: state.version,
        key: ctx.key.name.clone(),
        scopes: ctx.key.scopes.clone(),
        endpoints: ENDPOINTS.iter().map(|e| e.0).collect(),
        rate_limit_per_minute: RATE_LIMIT_PER_MINUTE,
    })
}

#[derive(Debug, Deserialize)]
pub struct StudentsQuery {
    pub academic_year_id: Option<Uuid>,
    pub section_id: Option<Uuid>,
    pub status: Option<String>,
    pub updated_since: Option<DateTime<Utc>>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

async fn students(
    State(state): State<AppState>,
    Extension(ctx): Extension<KeyContext>,
    Query(q): Query<StudentsQuery>,
) -> ApiResult<Json<Page<ro::RoStudent>>> {
    ctx.require(Scope::Students)?;
    let mut c = state.pool.acquire().await?;
    let school = school(&mut c).await?;
    let year = year(&state, &mut c, school, q.academic_year_id).await?;
    let page = q.page.unwrap_or(1).max(1);
    let page_size = q.page_size.unwrap_or(200).clamp(1, 1000);
    let (items, total) = ro::students(
        &mut c,
        school,
        year,
        q.section_id,
        q.status.as_deref().filter(|s| !s.is_empty()),
        q.updated_since,
        page,
        page_size,
    )
    .await?;
    Ok(Json(Page {
        items,
        page,
        page_size,
        total,
    }))
}

async fn sections(
    State(state): State<AppState>,
    Extension(ctx): Extension<KeyContext>,
    Query(q): Query<YearQuery>,
) -> ApiResult<Json<Vec<ro::RoSection>>> {
    ctx.require(Scope::Students)?;
    let mut c = state.pool.acquire().await?;
    let school = school(&mut c).await?;
    let year = year(&state, &mut c, school, q.academic_year_id).await?;
    Ok(Json(ro::sections(&mut c, school, year).await?))
}

#[derive(Debug, Deserialize)]
pub struct DailyQuery {
    pub academic_year_id: Option<Uuid>,
    pub date: Option<NaiveDate>,
    pub section_id: Option<Uuid>,
}

async fn attendance_daily(
    State(state): State<AppState>,
    Extension(ctx): Extension<KeyContext>,
    Query(q): Query<DailyQuery>,
) -> ApiResult<Json<Vec<ro::RoAttendanceDay>>> {
    ctx.require(Scope::Attendance)?;
    let mut c = state.pool.acquire().await?;
    let school = school(&mut c).await?;
    let year = year(&state, &mut c, school, q.academic_year_id).await?;
    let date = q.date.unwrap_or_else(crate::clock::today);
    Ok(Json(
        ro::attendance_daily(&mut c, school, year, date, q.section_id).await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct SectionQuery {
    pub academic_year_id: Option<Uuid>,
    pub section_id: Option<Uuid>,
}

async fn fee_balances(
    State(state): State<AppState>,
    Extension(ctx): Extension<KeyContext>,
    Query(q): Query<SectionQuery>,
) -> ApiResult<Json<Vec<ro::RoBalance>>> {
    ctx.require(Scope::Fees)?;
    let mut c = state.pool.acquire().await?;
    let school = school(&mut c).await?;
    let year = year(&state, &mut c, school, q.academic_year_id).await?;
    Ok(Json(
        ro::fee_balances(&mut c, school, year, q.section_id).await?,
    ))
}

async fn assessments(
    State(state): State<AppState>,
    Extension(ctx): Extension<KeyContext>,
    Query(q): Query<YearQuery>,
) -> ApiResult<Json<Vec<ro::RoAssessment>>> {
    ctx.require(Scope::Exams)?;
    let mut c = state.pool.acquire().await?;
    let school = school(&mut c).await?;
    let year = year(&state, &mut c, school, q.academic_year_id).await?;
    Ok(Json(ro::assessments(&mut c, year).await?))
}

#[derive(Debug, Deserialize)]
pub struct ResultsQuery {
    pub assessment_id: Uuid,
    pub section_id: Uuid,
}

async fn results(
    State(state): State<AppState>,
    Extension(ctx): Extension<KeyContext>,
    Query(q): Query<ResultsQuery>,
) -> ApiResult<Json<Vec<ro::RoResult>>> {
    ctx.require(Scope::Exams)?;
    let mut c = state.pool.acquire().await?;
    let school = school(&mut c).await?;
    Ok(Json(
        ro::results(&mut c, school, q.assessment_id, q.section_id).await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct TimetableQuery {
    pub academic_year_id: Option<Uuid>,
    pub section_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct TimetableAnswer {
    pub published: bool,
    pub cards: Vec<ro::RoTimetableCard>,
}

async fn timetable(
    State(state): State<AppState>,
    Extension(ctx): Extension<KeyContext>,
    Query(q): Query<TimetableQuery>,
) -> ApiResult<Json<TimetableAnswer>> {
    ctx.require(Scope::Timetable)?;
    let mut c = state.pool.acquire().await?;
    let school = school(&mut c).await?;
    let year = year(&state, &mut c, school, q.academic_year_id).await?;
    let cards = ro::timetable(&mut c, school, year, q.section_id).await?;
    Ok(Json(TimetableAnswer {
        published: cards.is_some(),
        cards: cards.unwrap_or_default(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct LoansQuery {
    pub state: Option<String>,
}

async fn loans(
    State(state): State<AppState>,
    Extension(ctx): Extension<KeyContext>,
    Query(q): Query<LoansQuery>,
) -> ApiResult<Json<Vec<library::LoanView>>> {
    ctx.require(Scope::Library)?;
    let mut c = state.pool.acquire().await?;
    let school = school(&mut c).await?;
    Ok(Json(
        library::list_loans(
            &mut c,
            school,
            &library::LoanFilter {
                state: Some(q.state.unwrap_or_else(|| "open".into())),
                ..Default::default()
            },
            crate::clock::today(),
        )
        .await?,
    ))
}

// ---- OpenAPI --------------------------------------------------------------------------------

/// (path, scope, summary, query parameters)
type EndpointDoc = (
    &'static str,
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);
const ENDPOINTS: &[EndpointDoc] = &[
    ("/", "any", "What this key can do", &[]),
    (
        "/students",
        "students",
        "Students with their class and section (paged)",
        &[
            ("academic_year_id", "uuid; defaults to the active year"),
            ("section_id", "uuid"),
            ("status", "active | left | graduated"),
            ("updated_since", "RFC 3339 timestamp"),
            ("page", "1-based, default 1"),
            ("page_size", "1–1000, default 200"),
        ],
    ),
    (
        "/sections",
        "students",
        "Sections of the year with active student counts",
        &[("academic_year_id", "uuid; defaults to the active year")],
    ),
    (
        "/attendance/daily",
        "attendance",
        "Per section: marked or not, present / absent / late / leave counts for a day",
        &[
            ("academic_year_id", "uuid; defaults to the active year"),
            ("date", "YYYY-MM-DD; defaults to today"),
            ("section_id", "uuid"),
        ],
    ),
    (
        "/fees/balances",
        "fees",
        "Students with unpaid or partly paid vouchers and the balance owed",
        &[
            ("academic_year_id", "uuid; defaults to the active year"),
            ("section_id", "uuid"),
        ],
    ),
    (
        "/exams/assessments",
        "exams",
        "Assessments of the year, to pick one for /exams/results",
        &[("academic_year_id", "uuid; defaults to the active year")],
    ),
    (
        "/exams/results",
        "exams",
        "Approved marks of an assessment in a section, one row per student and subject",
        &[
            ("assessment_id", "uuid, required"),
            ("section_id", "uuid, required"),
        ],
    ),
    (
        "/timetable",
        "timetable",
        "The published timetable of a section: day (0 = Monday), period, subject, teachers, room",
        &[
            ("academic_year_id", "uuid; defaults to the active year"),
            ("section_id", "uuid, required"),
        ],
    ),
    (
        "/library/loans",
        "library",
        "Library loans by state",
        &[("state", "open (default) | overdue | returned | fines")],
    ),
];

async fn openapi(State(state): State<AppState>) -> Json<serde_json::Value> {
    use serde_json::json;
    let mut paths = serde_json::Map::new();
    for (path, scope, summary, params) in ENDPOINTS {
        let parameters: Vec<_> = params
            .iter()
            .map(|(name, desc)| {
                json!({ "name": name, "in": "query", "description": desc, "required": desc.contains("required"), "schema": { "type": "string" } })
            })
            .collect();
        paths.insert(
            (*path).to_string(),
            json!({ "get": {
                "summary": summary,
                "description": format!("Needs the \"{scope}\" scope."),
                "parameters": parameters,
                "responses": {
                    "200": { "description": "JSON", "headers": { "ETag": { "schema": { "type": "string" } } } },
                    "304": { "description": "Not modified (If-None-Match matched)" },
                    "401": { "description": "Missing, unknown, expired or revoked key" },
                    "403": { "description": "Scope missing or address not allowed" },
                    "429": { "description": "Rate limit reached; see Retry-After" }
                }
            } }),
        );
    }
    Json(json!({
        "openapi": "3.0.3",
        "info": {
            "title": "Madrasa read-only API",
            "version": state.version,
            "description": format!("Read-only JSON for the school's other systems. Send the key as `Authorization: Bearer mk_…` or `X-Api-Key`. {RATE_LIMIT_PER_MINUTE} requests per minute per key. Every answer carries an ETag; send it back as If-None-Match to get 304 when nothing changed.")
        },
        "servers": [{ "url": RO_PREFIX }],
        "components": { "securitySchemes": { "apiKey": { "type": "http", "scheme": "bearer" } } },
        "security": [{ "apiKey": [] }],
        "paths": paths
    }))
}
