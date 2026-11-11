//! Madrasa Nexus hub server. Mounted under `/api/nexus/v1`; design in
//! `docs/adr/0005-nexus-hub-and-sync.md`.
//!
//! The hub joins schools into one network: each school syncs with its own schema here, head
//! office reads network-wide reports and can hold backup keys in escrow. Permission checks
//! happen in this crate, never only in the console.

pub mod auth;
pub mod config;
pub mod error;
pub mod permissions;
pub mod routes;
pub mod server;
pub mod state;
pub mod store;

pub use config::Config;
pub use error::{HubError, HubResult};
pub use state::{HubState, VERSION};

use axum::Router;
use tower_http::trace::TraceLayer;

/// API version prefix of the hub.
pub const API_PREFIX: &str = "/api/nexus/v1";

/// Build the hub router.
pub fn router(state: HubState) -> Router {
    let api = Router::new()
        .merge(routes::health::router())
        .merge(routes::setup::router())
        .merge(routes::auth::router())
        .merge(routes::users::router())
        .merge(routes::audit::router())
        .with_state(state);
    Router::new()
        .nest(API_PREFIX, api)
        .layer(TraceLayer::new_for_http())
}
