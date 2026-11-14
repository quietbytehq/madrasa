//! `Idempotency-Key`: a change sent twice is applied once (ADR 0009).
//!
//! A LAN client puts a fresh UUID v7 on every change it queues while the school server is out
//! of reach, and sends the same key every time it tries again. The first time a key arrives
//! the request runs and its answer is kept; every later arrival gets that answer back
//! (`Idempotent-Replay: true`) without running anything. The request runs in its own task, so
//! a connection that drops half way through cannot cut it short and leave the client unsure:
//! the answer is kept either way and the next attempt receives it.
//!
//! - A key reused with a different request (method, path or body) is refused (422).
//! - A key whose request is still running is answered `409 idempotency_in_progress`; the
//!   client tries again later. One left running for more than ten minutes (the server stopped
//!   in the middle) is answered `409 idempotency_unknown`, for a person to check.
//! - A request that failed on the server's side (5xx) is forgotten, so it can be tried again.
//! - Keys are kept for 30 days.

use axum::body::{Body, Bytes};
use axum::extract::{FromRequestParts, Request, State};
use axum::http::{header, HeaderName, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::error::ApiError;
use crate::AppState;

pub const HEADER: HeaderName = HeaderName::from_static("idempotency-key");
pub const REPLAY: HeaderName = HeaderName::from_static("idempotent-replay");

/// Requests and answers larger than this are not kept (imports, restores); they run normally.
const MAX_KEPT: usize = 8 * 1024 * 1024;
const STALE_MINUTES: i64 = 10;
const KEEP_DAYS: i64 = 30;

fn hash(method: &Method, path: &str, body: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(method.as_str().as_bytes());
    h.update(b"\n");
    h.update(path.as_bytes());
    h.update(b"\n");
    h.update(body);
    hex(&h.finalize())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(sqlx::FromRow)]
struct Kept {
    user_id: Uuid,
    request_hash: String,
    state: String,
    status: Option<i32>,
    response: Option<Vec<u8>>,
    response_type: Option<String>,
    stale: bool,
}

fn replay(k: Kept) -> Response {
    let status = k
        .status
        .and_then(|s| u16::try_from(s).ok())
        .and_then(|s| StatusCode::from_u16(s).ok())
        .unwrap_or(StatusCode::OK);
    let mut r = Response::builder().status(status).header(REPLAY, "true");
    if let Some(t) = k.response_type.and_then(|t| HeaderValue::from_str(&t).ok()) {
        r = r.header(header::CONTENT_TYPE, t);
    }
    r.body(Body::from(k.response.unwrap_or_default()))
        .unwrap_or_else(|_| ApiError::internal().into_response())
}

/// The middleware, on every route under `/api/v1`.
pub async fn layer(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let mutating = matches!(
        *req.method(),
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );
    let Some(raw) = req.headers().get(&HEADER).cloned().filter(|_| mutating) else {
        return next.run(req).await;
    };
    let Some(key) = raw
        .to_str()
        .ok()
        .and_then(|s| Uuid::parse_str(s.trim()).ok())
    else {
        return ApiError::validation("Idempotency-Key must be a UUID.").into_response();
    };
    let (mut parts, body) = req.into_parts();
    // Unauthenticated requests (logging in) are not kept; the route answers as usual.
    let Ok(user) = CurrentUser::from_request_parts(&mut parts, &state).await else {
        return next.run(Request::from_parts(parts, body)).await;
    };
    let bytes = match axum::body::to_bytes(body, MAX_KEPT).await {
        Ok(b) => b,
        Err(_) => {
            return ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "too_large",
                "This change is too large to be sent again safely. Make it while connected.",
            )
            .into_response()
        }
    };
    let path = parts
        .uri
        .path_and_query()
        .map(|p| p.as_str().to_string())
        .unwrap_or_default();
    let digest = hash(&parts.method, &path, &bytes);

    match claim(&state, key, &user, &parts.method, &path, &digest).await {
        Ok(None) => {}
        Ok(Some(kept)) => {
            if kept.user_id != user.user.id || kept.request_hash != digest {
                return ApiError::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "idempotency_key_reused",
                    "This change was already sent with different contents. It was not applied again.",
                )
                .into_response();
            }
            if kept.state == "done" {
                return replay(kept);
            }
            return if kept.stale {
                ApiError::new(
                    StatusCode::CONFLICT,
                    "idempotency_unknown",
                    "The server stopped while saving this change. Check whether it was saved before entering it again.",
                )
                .into_response()
            } else {
                ApiError::new(
                    StatusCode::CONFLICT,
                    "idempotency_in_progress",
                    "This change is still being saved. It will be tried again shortly.",
                )
                .into_response()
            };
        }
        Err(e) => return e.into_response(),
    }

    // Run the request to the end even if the caller goes away.
    let req = Request::from_parts(parts, Body::from(bytes));
    let response = match tokio::spawn(next.run(req)).await {
        Ok(r) => r,
        Err(_) => {
            forget(&state, key).await;
            return ApiError::internal().into_response();
        }
    };
    let (rparts, rbody) = response.into_parts();
    let rbytes: Bytes = match axum::body::to_bytes(rbody, MAX_KEPT).await {
        Ok(b) => b,
        Err(_) => {
            // Too large to keep: forget the key (the change itself ran) and say so.
            forget(&state, key).await;
            return ApiError::internal().into_response();
        }
    };
    if rparts.status.is_server_error() {
        forget(&state, key).await;
    } else {
        let content_type = rparts
            .headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        if let Err(e) = sqlx::query(
            "UPDATE idempotency_keys SET state = 'done', status = $2, response = $3,
                    response_type = $4, updated_at = now(), hlc = $5
              WHERE id = $1",
        )
        .bind(key)
        .bind(i32::from(rparts.status.as_u16()))
        .bind(rbytes.as_ref())
        .bind(content_type)
        .bind(state.hlc.now().to_string())
        .execute(&state.pool)
        .await
        {
            tracing::error!(error = %e, "could not keep the answer to an idempotent request");
        }
    }
    Response::from_parts(rparts, Body::from(rbytes))
}

/// Take the key for this request; `Some` with what is already kept when it was taken before.
async fn claim(
    state: &AppState,
    key: Uuid,
    user: &CurrentUser,
    method: &Method,
    path: &str,
    digest: &str,
) -> Result<Option<Kept>, ApiError> {
    let inserted = sqlx::query(
        "INSERT INTO idempotency_keys (id, school_id, user_id, method, path, request_hash, state, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, 'running', $3, $7)
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(key)
    .bind(user.user.school_id)
    .bind(user.user.id)
    .bind(method.as_str())
    .bind(path)
    .bind(digest)
    .bind(state.hlc.now().to_string())
    .execute(&state.pool)
    .await
    .map_err(|_| ApiError::internal())?;
    if inserted.rows_affected() == 1 {
        return Ok(None);
    }
    let kept = sqlx::query_as::<_, Kept>(
        "SELECT user_id, request_hash, state, status, response, response_type,
                (state = 'running' AND updated_at < now() - make_interval(mins => $2)) AS stale
           FROM idempotency_keys WHERE id = $1",
    )
    .bind(key)
    .bind(STALE_MINUTES as i32)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| ApiError::internal())?;
    Ok(Some(kept))
}

async fn forget(state: &AppState, key: Uuid) {
    let _ = sqlx::query("DELETE FROM idempotency_keys WHERE id = $1")
        .bind(key)
        .execute(&state.pool)
        .await;
}

/// Drop keys older than 30 days (when the server starts).
pub async fn prune(pool: &sqlx::PgPool) -> sqlx::Result<u64> {
    let r = sqlx::query(
        "DELETE FROM idempotency_keys WHERE created_at < now() - make_interval(days => $1)",
    )
    .bind(KEEP_DAYS as i32)
    .execute(pool)
    .await?;
    Ok(r.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_covers_method_path_and_body() {
        let a = hash(&Method::POST, "/api/v1/students", b"{\"a\":1}");
        assert_eq!(a.len(), 64);
        assert_eq!(a, hash(&Method::POST, "/api/v1/students", b"{\"a\":1}"));
        assert_ne!(a, hash(&Method::PUT, "/api/v1/students", b"{\"a\":1}"));
        assert_ne!(a, hash(&Method::POST, "/api/v1/students?x=1", b"{\"a\":1}"));
        assert_ne!(a, hash(&Method::POST, "/api/v1/students", b"{\"a\":2}"));
    }
}
