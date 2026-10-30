use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use madrasa_core::edition::{Edition, Feature};
use serde::Serialize;

use crate::AppState;

/// Mirrored by `FeaturesResponse` in the UI.
#[derive(Debug, Serialize)]
pub struct Features {
    pub edition: Edition,
    pub features: Vec<Feature>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/system/features", get(features))
}

async fn features(State(state): State<AppState>) -> Json<Features> {
    let edition = state.edition();
    Json(Features {
        edition,
        features: edition.features().to_vec(),
    })
}
