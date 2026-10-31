//! Madrasa REST API. Mounted under `/api/v1`; see `docs/spec.md` §6.
//!
//! The same router runs in-process inside the desktop shell (Lite) and as the standalone
//! `madrasa-server` binary (Standard). Permission checks happen here, never only in the UI.

pub mod auth;
pub mod backup;
pub mod bootstrap;
pub mod error;
pub mod export;
pub mod imports;
pub mod late_fees;
pub mod routes;
pub mod server;
pub mod state;

pub use error::{ApiError, ApiResult};
pub use state::AppState;

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
        .merge(routes::academics::router())
        .merge(routes::rollover::router())
        .merge(routes::staff::router())
        .merge(routes::students::router())
        .merge(routes::school_settings::router())
        .merge(routes::fees::router())
        .merge(routes::attendance::router())
        .merge(routes::exams::router())
        .merge(routes::backup::router())
        .merge(routes::imports::router())
        .merge(routes::export::router())
        .with_state(state);

    // The UI is served from a different origin in `vite dev` (http://localhost:1420) and from
    // `tauri://localhost` in the packaged app; the API only ever binds to loopback.
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .nest(API_PREFIX, api)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}
