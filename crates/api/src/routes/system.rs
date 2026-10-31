use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use madrasa_core::edition::{Edition, Feature};
use madrasa_core::locale::{self, CountryInfo, CurrencyInfo};
use serde::Serialize;

use crate::AppState;

/// Mirrored by `FeaturesResponse` in the UI.
#[derive(Debug, Serialize)]
pub struct Features {
    pub edition: Edition,
    pub features: Vec<Feature>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/system/features", get(features))
        .route("/system/locale", get(locale_lists))
}

async fn features(State(state): State<AppState>) -> Json<Features> {
    let edition = state.edition();
    Json(Features {
        edition,
        features: edition.features().to_vec(),
    })
}

/// Mirrored by `LocaleLists` in the UI: what setup and settings can choose from. Public, since
/// the setup wizard runs before any login.
#[derive(Debug, Serialize)]
pub struct LocaleLists {
    pub countries: &'static [CountryInfo],
    pub currencies: &'static [CurrencyInfo],
    pub time_zones: Vec<&'static str>,
}

async fn locale_lists() -> Json<LocaleLists> {
    Json(LocaleLists {
        countries: locale::COUNTRIES,
        currencies: locale::CURRENCIES,
        time_zones: locale::time_zone_names(),
    })
}
