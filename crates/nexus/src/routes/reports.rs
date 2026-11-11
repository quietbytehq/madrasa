//! Network-wide reports (`nexus.reports`): the catalogue, and each report as JSON, CSV or
//! Excel.

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::NaiveDate;
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::CurrentHubUser;
use crate::permissions::HubPermission;
use crate::reports::{self, CatalogueEntry, Lang, Params};
use crate::{store, HubError, HubResult, HubState};

pub fn router() -> Router<HubState> {
    Router::new()
        .route("/reports", get(catalogue))
        .route("/reports/{code}", get(run))
}

#[derive(Debug, Deserialize)]
pub struct CatalogueQuery {
    #[serde(default)]
    pub lang: Option<String>,
}

async fn catalogue(
    current: CurrentHubUser,
    q: Result<Query<CatalogueQuery>, axum::extract::rejection::QueryRejection>,
) -> HubResult<Json<Vec<CatalogueEntry>>> {
    current.require(HubPermission::Reports)?;
    let Query(q) = q?;
    Ok(Json(reports::catalogue(Lang::parse(q.lang.as_deref()))))
}

#[derive(Debug, Deserialize)]
pub struct RunQuery {
    #[serde(default)]
    pub from: Option<NaiveDate>,
    #[serde(default)]
    pub to: Option<NaiveDate>,
    /// One school (its id in the hub's register) instead of all.
    #[serde(default)]
    pub school_id: Option<Uuid>,
    #[serde(default)]
    pub lang: Option<String>,
    /// `json` (default), `csv` or `xlsx`.
    #[serde(default)]
    pub format: Option<String>,
}

fn download(file_name: &str, mime: &'static str, bytes: Vec<u8>) -> Response {
    let disposition = format!("attachment; filename=\"{file_name}\"");
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, HeaderValue::from_static(mime)),
            (
                header::CONTENT_DISPOSITION,
                HeaderValue::from_str(&disposition)
                    .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
            ),
        ],
        bytes,
    )
        .into_response()
}

async fn run(
    State(state): State<HubState>,
    current: CurrentHubUser,
    Path(code): Path<String>,
    q: Result<Query<RunQuery>, axum::extract::rejection::QueryRejection>,
) -> HubResult<Response> {
    current.require(HubPermission::Reports)?;
    let Query(q) = q?;
    let format = q.format.as_deref().unwrap_or("json");
    if !matches!(format, "json" | "csv" | "xlsx") {
        return Err(HubError::validation(
            "The format must be json, csv or xlsx.",
        ));
    }
    let params = Params {
        from: q.from,
        to: q.to,
        school: q.school_id,
        lang: Lang::parse(q.lang.as_deref()),
    };
    let report = reports::run(&state, &code, &params).await?;
    // Who looked at what, as the school reports do.
    let mut conn = state.pool.acquire().await?;
    store::audit(
        &mut conn,
        &state.hlc,
        Some(current.user.id),
        "run",
        "hub_report",
        None,
        None,
        Some(serde_json::json!({
            "code": report.code, "from": report.from, "to": report.to,
            "school_id": q.school_id, "format": format,
        })),
    )
    .await?;
    drop(conn);
    let stamp = report.generated_at.format("%Y-%m-%d");
    Ok(match format {
        "csv" => download(
            &format!("nexus-{}-{stamp}.csv", report.code),
            "text/csv; charset=utf-8",
            reports::to_csv(&report),
        ),
        "xlsx" => download(
            &format!("nexus-{}-{stamp}.xlsx", report.code),
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            reports::to_xlsx(&report).map_err(|_| HubError::internal())?,
        ),
        _ => Json(report).into_response(),
    })
}
