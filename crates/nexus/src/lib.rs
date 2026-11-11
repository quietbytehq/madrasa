//! Madrasa Nexus hub server. Mounted under `/api/nexus/v1`; design in
//! `docs/adr/0005-nexus-hub-and-sync.md`.
//!
//! The hub joins schools into one network: each school syncs with its own schema here, head
//! office reads network-wide reports and can hold backup keys in escrow. Permission checks
//! happen in this crate, never only in the console.

pub mod auth;
pub mod config;
pub mod demo;
pub mod error;
pub mod escrow;
pub mod permissions;
pub mod reports;
pub mod routes;
pub mod server;
pub mod state;
pub mod store;
pub mod tenants;

pub use config::Config;
pub use error::{HubError, HubResult};
pub use state::{HubState, VERSION};

use axum::http::header::{self, HeaderName, HeaderValue};
use axum::http::StatusCode;
use axum::Router;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

/// API version prefix of the hub.
pub const API_PREFIX: &str = "/api/nexus/v1";

/// What the console may load: itself. No other site's scripts, frames or connections.
const CONSOLE_CSP: &str = "default-src 'self'; style-src 'self' 'unsafe-inline'; \
     img-src 'self' data:; font-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; \
     base-uri 'self'; form-action 'self'";

async fn no_such_api() -> HubError {
    HubError::new(
        StatusCode::NOT_FOUND,
        "not_found",
        "This address is not part of the hub.",
    )
}

/// Build the hub router: the API, and the web console when its files are present.
pub fn router(state: HubState) -> Router {
    let console = state
        .console_dir
        .clone()
        .filter(|dir| dir.join("index.html").is_file());
    let api = Router::new()
        .merge(routes::health::router())
        .merge(routes::setup::router())
        .merge(routes::auth::router())
        .merge(routes::users::router())
        .merge(routes::audit::router())
        .merge(routes::schools::router())
        .merge(routes::reports::router())
        .merge(routes::escrow::router())
        .merge(routes::corrections::router())
        .merge(routes::sync::router())
        .fallback(no_such_api)
        .with_state(state);
    let app = Router::new().nest(API_PREFIX, api);
    let app = match console {
        Some(dir) => {
            // Built files have a content hash in their names and never change; the page
            // itself is always asked for again, so a new version shows at once. Any other
            // path is a page of the console: the browser gets the page and routes it.
            let page = || {
                tower::ServiceBuilder::new()
                    .layer(SetResponseHeaderLayer::overriding(
                        header::CACHE_CONTROL,
                        HeaderValue::from_static("no-cache"),
                    ))
                    .service(ServeFile::new(dir.join("index.html")))
            };
            let assets = tower::ServiceBuilder::new()
                .layer(SetResponseHeaderLayer::overriding(
                    header::CACHE_CONTROL,
                    HeaderValue::from_static("public, max-age=31536000, immutable"),
                ))
                .service(ServeDir::new(dir.join("assets")));
            app.nest_service("/assets", assets)
                .route_service("/", page())
                .fallback_service(page())
        }
        None => app.fallback(|| async {
            (
                StatusCode::NOT_FOUND,
                "Madrasa Nexus hub. The web console is not installed here; the API is under /api/nexus/v1.",
            )
        }),
    };
    app.layer(SetResponseHeaderLayer::overriding(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CONSOLE_CSP),
    ))
    .layer(SetResponseHeaderLayer::overriding(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    ))
    .layer(SetResponseHeaderLayer::overriding(
        header::X_FRAME_OPTIONS,
        HeaderValue::from_static("DENY"),
    ))
    .layer(SetResponseHeaderLayer::overriding(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("no-referrer"),
    ))
    .layer(TraceLayer::new_for_http())
}
