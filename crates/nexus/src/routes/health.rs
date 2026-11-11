use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;

use crate::HubState;

#[derive(Debug, Serialize)]
pub struct Health {
    pub status: &'static str,
    pub service: &'static str,
    pub version: &'static str,
    pub database: &'static str,
}

pub fn router() -> Router<HubState> {
    Router::new().route("/health", get(health))
}

/// Liveness for container health checks: no session needed, nothing sensitive returned.
async fn health(State(state): State<HubState>) -> Json<Health> {
    let database = match sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.pool)
        .await
    {
        Ok(_) => "ok",
        Err(_) => "error",
    };
    Json(Health {
        status: if database == "ok" { "ok" } else { "degraded" },
        service: "madrasa-nexus",
        version: state.version,
        database,
    })
}
