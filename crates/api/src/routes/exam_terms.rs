//! Terms and assessment components (M2 stage 5): the year's structure, term CRUD and
//! weights. Components themselves are the assessment types (routes in `exams.rs`).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use chrono::NaiveDate;
use madrasa_core::exams::Formula;
use madrasa_core::permissions::Permission;
use madrasa_db::exams as repo;
use madrasa_db::exams::Term;
use rust_decimal::Decimal;
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::{ApiError, ApiResult, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/exams/terms", get(list_terms).post(create_term))
        .route("/exams/terms/weights", put(put_term_weights))
        .route("/exams/terms/{id}", put(update_term).delete(delete_term))
        .route("/exams/structure", get(structure))
}

#[derive(Debug, Deserialize)]
pub struct YearQuery {
    pub academic_year_id: Uuid,
}

async fn year_of(c: &mut sqlx::PgConnection, user: &CurrentUser, id: Uuid) -> ApiResult<()> {
    let y = madrasa_db::academics::get_year(c, id).await?;
    if y.school_id != user.user.school_id {
        return Err(ApiError::not_found("academic year"));
    }
    Ok(())
}

async fn list_terms(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<YearQuery>,
) -> ApiResult<Json<Vec<Term>>> {
    user.require(Permission::ExamsRead)?;
    let mut c = state.pool.acquire().await?;
    year_of(&mut c, &user, q.academic_year_id).await?;
    Ok(Json(repo::list_terms(&mut c, q.academic_year_id).await?))
}

#[derive(Debug, Deserialize)]
pub struct TermRequest {
    pub academic_year_id: Option<Uuid>,
    pub name: String,
    #[serde(default)]
    pub position: i32,
    #[serde(default)]
    pub starts_on: Option<NaiveDate>,
    #[serde(default)]
    pub ends_on: Option<NaiveDate>,
    #[serde(default)]
    pub weight: Decimal,
}

async fn create_term(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<TermRequest>,
) -> ApiResult<(StatusCode, Json<Term>)> {
    user.require(Permission::AcademicsManage)?;
    let year = req
        .academic_year_id
        .ok_or_else(|| ApiError::validation("academic_year_id is required."))?;
    let mut c = state.pool.acquire().await?;
    year_of(&mut c, &user, year).await?;
    if req.name.trim().is_empty() {
        return Err(ApiError::validation("The term needs a name."));
    }
    let t = repo::create_term(
        &mut c,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        year,
        repo::NewTerm {
            name: req.name,
            position: req.position,
            starts_on: req.starts_on,
            ends_on: req.ends_on,
            weight: req.weight,
        },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(t)))
}

async fn load_term(c: &mut sqlx::PgConnection, user: &CurrentUser, id: Uuid) -> ApiResult<Term> {
    let t = repo::get_term(c, id).await?;
    if t.school_id != user.user.school_id {
        return Err(ApiError::not_found("term"));
    }
    Ok(t)
}

async fn update_term(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<TermRequest>,
) -> ApiResult<Json<Term>> {
    user.require(Permission::AcademicsManage)?;
    let mut c = state.pool.acquire().await?;
    let existing = load_term(&mut c, &user, id).await?;
    if req.name.trim().is_empty() {
        return Err(ApiError::validation("The term needs a name."));
    }
    Ok(Json(
        repo::update_term(
            &mut c,
            &state.hlc,
            user.actor(),
            id,
            repo::NewTerm {
                name: req.name,
                position: req.position,
                starts_on: req.starts_on,
                ends_on: req.ends_on,
                weight: existing.weight,
            },
        )
        .await?,
    ))
}

async fn delete_term(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    user.require(Permission::AcademicsManage)?;
    let mut c = state.pool.acquire().await?;
    load_term(&mut c, &user, id).await?;
    repo::delete_term(&mut c, &state.hlc, user.actor(), id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct TermWeightsRequest {
    pub academic_year_id: Uuid,
    pub weights: Vec<WeightLine>,
}

#[derive(Debug, Deserialize)]
pub struct WeightLine {
    pub id: Uuid,
    pub weight: Decimal,
}

async fn put_term_weights(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<TermWeightsRequest>,
) -> ApiResult<Json<Vec<Term>>> {
    user.require(Permission::AcademicsManage)?;
    let values: Vec<Decimal> = req.weights.iter().map(|w| w.weight).collect();
    madrasa_core::exams::validate_weights(&values)
        .map_err(|e| ApiError::validation(format!("{e}.")))?;
    let mut c = state.pool.acquire().await?;
    year_of(&mut c, &user, req.academic_year_id).await?;
    let weights: Vec<(Uuid, Decimal)> = req.weights.iter().map(|w| (w.id, w.weight)).collect();
    Ok(Json(
        repo::set_term_weights(
            &mut c,
            &state.hlc,
            user.actor(),
            req.academic_year_id,
            &weights,
        )
        .await?,
    ))
}

pub use madrasa_db::results::{structure_of, Structure, TermWithComponents};

async fn structure(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<YearQuery>,
) -> ApiResult<Json<Structure>> {
    user.require(Permission::ExamsRead)?;
    let mut c = state.pool.acquire().await?;
    year_of(&mut c, &user, q.academic_year_id).await?;
    Ok(Json(structure_of(&mut c, q.academic_year_id).await?))
}

/// Check a component's formula against its term's components.
pub(crate) async fn validate_formula(
    c: &mut sqlx::PgConnection,
    academic_year_id: Uuid,
    term_id: Option<Uuid>,
    self_id: Option<Uuid>,
    formula: &Formula,
) -> ApiResult<()> {
    let all = repo::list_assessments(c, academic_year_id).await?;
    let term: Vec<(Uuid, Option<Formula>)> = all
        .iter()
        .filter(|a| a.term_id == term_id && Some(a.id) != self_id)
        .map(|a| {
            (
                a.id,
                a.formula
                    .clone()
                    .and_then(|f| serde_json::from_value(f).ok()),
            )
        })
        .collect();
    formula
        .validate(self_id, &term)
        .map_err(ApiError::validation)
}
