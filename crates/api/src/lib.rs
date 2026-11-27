//! Madrasa REST API. Mounted under `/api/v1`; see `docs/spec.md` §6.
//!
//! The same router runs in-process inside the desktop shell (Lite) and as the standalone
//! `madrasa-server` binary (Standard). Permission checks happen here, never only in the UI.

pub mod auth;
pub mod authz;
pub mod backup;
pub mod bootstrap;
pub mod clock;
pub mod directory;
pub mod error;
pub mod everything;
pub mod export;
pub mod imports;
pub mod lan;
pub mod late_fees;
pub mod nexus_link;
pub mod offline;
pub mod outbox;
pub mod private;
pub mod reports;
pub mod ro;
pub mod routes;
pub mod server;
pub mod state;
pub mod timetable_jobs;

pub use error::{ApiError, ApiResult};
pub use state::{AppState, SchoolProfile, VERSION};

use axum::Router;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

/// API version prefix.
pub const API_PREFIX: &str = "/api/v1";

/// Build the full application router.
pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .merge(routes::health::router())
        .merge(routes::system::router())
        .merge(routes::setup::router())
        .merge(routes::auth::router())
        .merge(routes::users::router())
        .merge(routes::schools::router())
        .merge(routes::audit::router())
        .merge(routes::nexus::router())
        .merge(routes::nexus_link::router())
        .merge(routes::parent_app::router())
        .merge(routes::academics::router())
        .merge(routes::rollover::router())
        .merge(routes::staff::router())
        .merge(routes::students::router())
        .merge(routes::school_settings::router())
        .merge(routes::edition::router())
        .merge(routes::fees::router())
        .merge(routes::attendance::router())
        .merge(routes::exams::router())
        .merge(routes::exam_terms::router())
        .merge(routes::subjects::router())
        .merge(routes::allocation::router())
        .merge(routes::consents::router())
        .merge(routes::timetable::router())
        .merge(routes::leave::router())
        .merge(routes::admissions::router())
        .merge(routes::library::router())
        .merge(routes::transport::router())
        .merge(routes::hostel::router())
        .merge(routes::inventory::router())
        .merge(routes::directory::router())
        .merge(routes::api_keys::router())
        .merge(routes::backup::router())
        .merge(routes::boards::router())
        .merge(routes::imports::router())
        .merge(routes::export::router())
        .merge(routes::onboarding::router())
        .merge(routes::reports::router())
        // A change sent twice (queued offline on a LAN client, ADR 0009) is applied once.
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            offline::idempotency::layer,
        ))
        // Who may call what, decided before anything else looks at the request (authz.rs).
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            authz::layer,
        ))
        .with_state(state.clone());

    // The UI is served from a different origin than the API: `tauri://localhost` (or
    // `http://tauri.localhost` on Windows) in the packaged app, `http://localhost:1420` in
    // `vite dev`. Only those may read answers from a browser context. A school server also
    // listens on the school network, so "any origin" would let a web page opened on any PC
    // of the school talk to it from that PC's browser; logins are bearer tokens, not
    // cookies, so such a page has no session to ride on, but there is no reason to let it
    // read even the public answers.
    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::AllowOrigin::predicate(|origin, _| {
            is_app_origin(origin.as_bytes())
        }))
        .allow_methods(Any)
        .allow_headers(Any)
        // A download says its file name in this header; without leave to read it the app
        // saved every file as "download".
        .expose_headers([axum::http::header::CONTENT_DISPOSITION]);

    Router::new()
        .nest(API_PREFIX, api)
        .nest(ro::RO_PREFIX, ro::router(state))
        .layer(axum::middleware::from_fn(to_completion))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}

/// Every request is carried out to its end, whether or not whoever sent it is still there.
///
/// Without this a request is dropped at whatever line it has reached the moment its client
/// goes away (a closed window, a client PC switched off, a proxy that stopped waiting). Half
/// of a change is then undone by its transaction, which is right, but a request dropped
/// between two steps that are not in one transaction leaves the first one done. (With sqlx
/// 0.9.0, one dropped while it opened a transaction also left its database connection
/// inside that transaction; sqlx 0.9.1 fixed that, see `madrasa_db::connect`.) Finishing
/// what was started is the simpler promise; a change sent again by a client PC is
/// recognised by its `Idempotency-Key` and not applied twice.
pub async fn to_completion(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    match tokio::spawn(next.run(req)).await {
        Ok(response) => response,
        Err(_) => ApiError::internal().into_response(),
    }
}

/// Is this `Origin` the Madrasa app itself (packaged, or its development server)? Anything
/// served from this computer counts; nothing served from elsewhere does.
pub fn is_app_origin(origin: &[u8]) -> bool {
    let Ok(origin) = std::str::from_utf8(origin) else {
        return false;
    };
    let Some((scheme, rest)) = origin.split_once("://") else {
        return false;
    };
    let host = match rest.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) => host,
        _ => rest,
    };
    match scheme {
        "tauri" => host == "localhost",
        "http" | "https" => matches!(
            host,
            "localhost" | "127.0.0.1" | "tauri.localhost" | "[::1]"
        ),
        _ => false,
    }
}

#[cfg(test)]
mod origin_tests {
    use super::is_app_origin;

    #[test]
    fn only_the_app_itself_is_an_allowed_origin() {
        for ok in [
            "tauri://localhost",
            "http://tauri.localhost",
            "https://tauri.localhost",
            "http://localhost:1420",
            "http://127.0.0.1:5173",
            "http://localhost",
            "http://[::1]:1420",
        ] {
            assert!(is_app_origin(ok.as_bytes()), "{ok}");
        }
        for no in [
            "https://example.com",
            "http://192.168.1.20:8080",
            "http://localhost.example.com",
            "http://example.com:localhost",
            "http://tauri.localhost.example.com",
            "tauri://example.com",
            "file://",
            "null",
            "",
        ] {
            assert!(!is_app_origin(no.as_bytes()), "{no}");
        }
    }
}
