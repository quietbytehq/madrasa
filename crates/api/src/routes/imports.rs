//! Import wizard endpoints. Everything needs `students.manage` (students, opening balances)
//! or `teachers.manage` (staff); rollback needs the same permission as the batch's kind.

use axum::extract::{Path, Query, State};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use madrasa_core::imports::{self as rules, FieldDef, ImportKind};
use madrasa_core::permissions::Permission;
use madrasa_db::models::ImportBatch;
use serde::Deserialize;

use crate::auth::CurrentUser;
use crate::imports::{self, ImportPreview, ImportRequest, ImportResult, ParsedFile, SavedMapping};
use crate::{ApiError, ApiResult, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/imports/fields/{kind}", get(fields))
        .route("/imports/templates/{kind}", get(template))
        .route("/imports/parse", post(parse))
        .route(
            "/imports/mappings/{kind}",
            get(get_mapping).put(put_mapping),
        )
        .route("/imports/preview", post(preview))
        .route("/imports/apply", post(apply))
        .route("/imports/batches", get(batches))
        .route("/imports/batches/{id}/rollback", post(rollback))
}

type JsonBody<T> = Result<Json<T>, axum::extract::rejection::JsonRejection>;

fn kind_of(s: &str) -> ApiResult<ImportKind> {
    s.parse().map_err(|_| {
        ApiError::validation("Import type must be students, staff or opening_balances.")
    })
}

fn permission_for(kind: ImportKind) -> Permission {
    match kind {
        ImportKind::Students | ImportKind::OpeningBalances => Permission::StudentsManage,
        ImportKind::Staff => Permission::TeachersManage,
    }
}

async fn fields(
    user: CurrentUser,
    Path(kind): Path<String>,
) -> ApiResult<Json<&'static [FieldDef]>> {
    let kind = kind_of(&kind)?;
    user.require(permission_for(kind))?;
    Ok(Json(rules::fields(kind)))
}

#[derive(Debug, Deserialize)]
pub struct FormatQuery {
    pub format: Option<String>,
}

async fn template(
    user: CurrentUser,
    Path(kind): Path<String>,
    Query(q): Query<FormatQuery>,
) -> ApiResult<Response> {
    let kind = kind_of(&kind)?;
    user.require(permission_for(kind))?;
    let xlsx = q.format.as_deref().unwrap_or("xlsx") != "csv";
    let bytes = imports::template(kind, xlsx)?;
    let name = format!(
        "madrasa-{}-template.{}",
        kind.as_str().replace('_', "-"),
        if xlsx { "xlsx" } else { "csv" }
    );
    Ok(crate::export::file_response(&name, bytes))
}

#[derive(Debug, Deserialize)]
pub struct ParseRequest {
    pub kind: ImportKind,
    pub file_name: String,
    pub content_base64: String,
    pub sheet: Option<String>,
}

async fn parse(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<ParseRequest>,
) -> ApiResult<Json<ParsedFile>> {
    let Json(req) = body?;
    user.require(permission_for(req.kind))?;
    if req.content_base64.len() > 40 * 1024 * 1024 {
        return Err(ApiError::validation(
            "The file is too large (30 MB at most).",
        ));
    }
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        imports::parsed(
            &mut c,
            user.user.school_id,
            req.kind,
            &req.file_name,
            &req.content_base64,
            req.sheet.as_deref(),
        )
        .await?,
    ))
}

async fn get_mapping(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(kind): Path<String>,
) -> ApiResult<Json<Option<SavedMapping>>> {
    let kind = kind_of(&kind)?;
    user.require(permission_for(kind))?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        imports::saved_mapping(&mut c, user.user.school_id, kind).await?,
    ))
}

async fn put_mapping(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(kind): Path<String>,
    body: JsonBody<SavedMapping>,
) -> ApiResult<Json<SavedMapping>> {
    let kind = kind_of(&kind)?;
    user.require(permission_for(kind))?;
    let Json(m) = body?;
    imports::save_mapping(&state, user.actor(), user.user.school_id, kind, &m).await?;
    Ok(Json(m))
}

fn check_mapping(req: &ImportRequest) -> ApiResult<()> {
    if let Some(f) = rules::missing_required(req.kind, &req.mapping).first() {
        return Err(ApiError::validation(format!(
            "Choose a column for \"{}\" before continuing.",
            f.label
        )));
    }
    if req.mapping.values().any(|i| *i >= req.headers.len()) {
        return Err(ApiError::validation(
            "The mapping points at a column the file does not have.",
        ));
    }
    Ok(())
}

async fn preview(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<ImportRequest>,
) -> ApiResult<Json<ImportPreview>> {
    let Json(req) = body?;
    user.require(permission_for(req.kind))?;
    check_mapping(&req)?;
    Ok(Json(
        imports::preview(&state, user.user.school_id, &req).await?,
    ))
}

async fn apply(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<ImportRequest>,
) -> ApiResult<Json<ImportResult>> {
    let Json(req) = body?;
    user.require(permission_for(req.kind))?;
    check_mapping(&req)?;
    Ok(Json(
        imports::apply(
            &state,
            user.actor(),
            user.user.id,
            user.user.school_id,
            &req,
        )
        .await?,
    ))
}

async fn batches(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<ImportBatch>>> {
    if !user.can(Permission::StudentsManage) && !user.can(Permission::TeachersManage) {
        return Err(ApiError::forbidden());
    }
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        madrasa_db::imports::list_batches(&mut c, user.user.school_id, 50).await?,
    ))
}

async fn rollback(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<uuid::Uuid>,
) -> ApiResult<Json<ImportBatch>> {
    let mut c = state.pool.acquire().await?;
    let batch = madrasa_db::imports::get_batch(&mut c, id).await?;
    if batch.school_id != user.user.school_id {
        return Err(ApiError::not_found("import"));
    }
    user.require(permission_for(kind_of(&batch.kind)?))?;
    drop(c);
    imports::rollback(&state, user.actor(), user.user.school_id, id).await?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(madrasa_db::imports::get_batch(&mut c, id).await?))
}
