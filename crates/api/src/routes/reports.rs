//! Report library: catalogue and runs (M2 stage 1).

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use madrasa_core::edition::Feature;
use madrasa_core::permissions::Permission;
use madrasa_db::audit::{self, Action};
use serde_json::json;

use crate::auth::CurrentUser;
use crate::export::file_response;
use crate::reports::catalogue::{Access, ReportDef};
use crate::reports::{admissions, consents, leave, library, timetable};
use crate::reports::{
    attendance, boards, builder, catalogue, exams, fees, pdf, students, Context, Lang, Params,
    Report,
};
use crate::{ApiError, ApiResult, AppState};
use madrasa_db::report_definitions::{self as defs, ReportDefinition};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/reports", get(list))
        .route("/reports/custom", get(list_custom).post(create_custom))
        .route("/reports/custom/sources", get(custom_sources))
        .route("/reports/custom/run", post(run_custom_adhoc))
        .route(
            "/reports/custom/{id}",
            get(run_custom).put(update_custom).delete(delete_custom),
        )
        .route("/reports/{code}", get(run))
}

/// Office roles run everything with `reports.read`; teachers run attendance and exam reports
/// (for their own sections) with the module's read permission.
fn may_run(user: &CurrentUser, def: &ReportDef) -> bool {
    user.can(Permission::ReportsRead)
        || match def.access {
            Access::ReportsRead => false,
            Access::AttendanceRead => user.can(Permission::AttendanceRead),
            Access::ExamsRead => user.can(Permission::ExamsRead),
            Access::TimetableRead => user.can(Permission::TimetableRead),
            Access::LeaveRead => user.can(Permission::LeaveRead),
            Access::AdmissionsRead => user.can(Permission::AdmissionsRead),
            Access::LibraryRead => user.can(Permission::LibraryRead),
        }
}

/// Reports of a module gated by the edition are hidden and refused on editions without it.
fn module_enabled(state: &AppState, def: &ReportDef) -> bool {
    match def.module {
        catalogue::Module::Timetable => state.has_feature(Feature::Timetable),
        catalogue::Module::Leave => state.has_feature(Feature::Leave),
        catalogue::Module::Admissions => state.has_feature(Feature::Admissions),
        catalogue::Module::Library => state.has_feature(Feature::Library),
        _ => true,
    }
}

async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<Vec<catalogue::CatalogueModule>>> {
    if !user.can(Permission::ReportsRead)
        && !user.can(Permission::AttendanceRead)
        && !user.can(Permission::ExamsRead)
        && !user.can(Permission::TimetableRead)
        && !user.can(Permission::LeaveRead)
        && !user.can(Permission::AdmissionsRead)
        && !user.can(Permission::LibraryRead)
    {
        return Err(ApiError::forbidden());
    }
    let lang = Lang::parse(q.get("lang").map(String::as_str));
    Ok(Json(catalogue::catalogue(lang, |d| {
        module_enabled(&state, d) && may_run(&user, d)
    })))
}

/// Build the report document for a code. Shared by the formats and by later stages.
pub async fn build(
    state: &AppState,
    user: &CurrentUser,
    code: &str,
    params: &Params,
) -> Result<Report, ApiError> {
    let def = catalogue::find(code).ok_or_else(|| ApiError::not_found("report"))?;
    if !module_enabled(state, def) {
        let feature = if def.module == catalogue::Module::Leave {
            Feature::Leave
        } else {
            Feature::Timetable
        };
        return Err(ApiError::feature_unavailable(feature.as_str()));
    }
    if !may_run(user, def) {
        return Err(ApiError::forbidden());
    }
    let lang = Lang::parse(params.get("lang"));
    let mut c = state.pool.acquire().await?;
    let school = madrasa_db::schools::get(&mut c, user.user.school_id).await?;
    let year = match params.uuid("academic_year_id")? {
        Some(id) => {
            let y = madrasa_db::academics::get_year(&mut c, id).await?;
            if y.school_id != school.id {
                return Err(ApiError::not_found("academic year"));
            }
            y
        }
        None => madrasa_db::academics::active_year(&mut c, school.id)
            .await?
            .ok_or_else(|| {
                ApiError::validation("No active academic year. Set one under Settings.")
            })?,
    };
    let ctx = Context {
        school: &school,
        year: &year,
        lang,
        section_scope: user.section_scope(&mut c).await?,
    };
    let report = match def.code {
        "students.list" => students::list(&mut c, &ctx, params).await?,
        "students.strength" => students::strength(&mut c, &ctx, params).await?,
        "students.movements" => students::movements(&mut c, &ctx, params).await?,
        "students.contacts" => students::contacts(&mut c, &ctx, params).await?,
        "students.birthdays" => students::birthdays(&mut c, &ctx, params).await?,
        "students.missing" => students::missing(&mut c, &ctx, params).await?,
        "students.consents_missing" => consents::missing(&mut c, &ctx, params).await?,
        "students.consents_expiring" => consents::expiring(&mut c, &ctx, params).await?,
        "students.consent_forms" => consents::forms(&mut c, &ctx, params).await?,
        "fees.collection" => fees::collection(&mut c, &ctx, params).await?,
        "fees.monthly" => fees::monthly(&mut c, &ctx, params).await?,
        "fees.defaulters" => fees::defaulters(&mut c, &ctx, params).await?,
        "fees.ledger" => fees::ledger(&mut c, &ctx, params).await?,
        "fees.concessions" => fees::concessions(&mut c, &ctx, params).await?,
        "fees.vouchers" => fees::vouchers(&mut c, &ctx, params).await?,
        "fees.late_fees" => fees::late_fees(&mut c, &ctx, params).await?,
        "fees.heads" => fees::heads(&mut c, &ctx, params).await?,
        "attendance.register" => attendance::register(&mut c, &ctx, params).await?,
        "attendance.percent" => attendance::percent_by_section(&mut c, &ctx, params).await?,
        "attendance.absentees" => attendance::absentees(&mut c, &ctx, params).await?,
        "attendance.below" => attendance::below(&mut c, &ctx, params).await?,
        "attendance.staff" => attendance::staff(&mut c, &ctx, params).await?,
        "exams.result_sheet" => exams::result_sheet(&mut c, user, &ctx, params).await?,
        "exams.merit" => exams::merit(&mut c, user, &ctx, params).await?,
        "exams.analysis" => exams::analysis(&mut c, user, &ctx, params).await?,
        "exams.grades" => exams::grades(&mut c, user, &ctx, params).await?,
        "exams.failing" => exams::failing(&mut c, user, &ctx, params).await?,
        "exams.report_cards" => exams::report_cards(&mut c, user, &ctx, params).await?,
        "exams.board_sheet" => boards::sheet(&mut c, user, &ctx, params).await?,
        "exams.board_merit" => boards::merit(&mut c, user, &ctx, params).await?,
        "exams.board_pass" => boards::pass_rates(&mut c, user, &ctx, params).await?,
        "exams.board_card" => boards::card(&mut c, user, &ctx, params).await?,
        "timetable.section" => timetable::section(&mut c, &ctx, params).await?,
        "timetable.teacher_load" => timetable::teacher_load(&mut c, &ctx, params).await?,
        "timetable.free_rooms" => timetable::free_rooms(&mut c, &ctx, params).await?,
        "timetable.cover_sheet" => timetable::cover_sheet(&mut c, &ctx, params).await?,
        "admissions.funnel" => admissions::funnel(&mut c, &ctx, params).await?,
        "admissions.by_class" => admissions::by_class(&mut c, &ctx, params).await?,
        "admissions.pending_tests" => admissions::pending_tests(&mut c, &ctx, params).await?,
        "library.overdue" => library::overdue(&mut c, &ctx, params).await?,
        "library.fines" => library::fines(&mut c, &ctx, params).await?,
        "library.most_borrowed" => library::most_borrowed(&mut c, &ctx, params).await?,
        "library.stock" => library::stock(&mut c, &ctx, params).await?,
        "leave.balances" => leave::balances(&mut c, &ctx, params).await?,
        "leave.register" => leave::register(&mut c, &ctx, params).await?,
        "leave.students" => leave::students(&mut c, &ctx, params).await?,
        _ => return Err(ApiError::not_found("report")),
    };
    audit::record(
        &mut c,
        &state.hlc,
        user.actor(),
        Action::Report,
        "report",
        None,
        None,
        Some(json!({ "code": code, "params": params.as_json(), "rows": report.row_count })),
    )
    .await?;
    Ok(report)
}

async fn run(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(code): Path<String>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Response> {
    let format = q.get("format").cloned().unwrap_or_else(|| "json".into());
    let params = Params(q);
    let report = build(&state, &user, &code, &params).await?;
    match format.as_str() {
        "json" => Ok(Json(report).into_response()),
        "xlsx" => {
            let (headers, rows) = report.flat();
            let bytes = crate::export::xlsx(&report.title, &headers, &rows)?;
            Ok(file_response(
                &format!("{}.xlsx", report.file_stem()),
                bytes,
            ))
        }
        "pdf" => {
            let name = format!("{}.pdf", report.file_stem());
            let bytes = tokio::task::spawn_blocking(move || pdf::render(&report))
                .await
                .map_err(|_| ApiError::internal())??;
            Ok(file_response(&name, bytes))
        }
        _ => Err(ApiError::validation("format must be json, pdf or xlsx.")),
    }
}

// ---- Custom reports (M2 stage 3) --------------------------------------------------------------

/// Resolve the school, year and scope for a custom run.
async fn context_parts(
    state: &AppState,
    user: &CurrentUser,
    params: &Params,
) -> Result<
    (
        madrasa_db::models::School,
        madrasa_db::models::AcademicYear,
        Option<Vec<Uuid>>,
    ),
    ApiError,
> {
    let mut c = state.pool.acquire().await?;
    let school = madrasa_db::schools::get(&mut c, user.user.school_id).await?;
    let year = match params.uuid("academic_year_id")? {
        Some(id) => {
            let y = madrasa_db::academics::get_year(&mut c, id).await?;
            if y.school_id != school.id {
                return Err(ApiError::not_found("academic year"));
            }
            y
        }
        None => madrasa_db::academics::active_year(&mut c, school.id)
            .await?
            .ok_or_else(|| {
                ApiError::validation("No active academic year. Set one under Settings.")
            })?,
    };
    let scope = user.section_scope(&mut c).await?;
    Ok((school, year, scope))
}

async fn respond(report: Report, format: &str) -> ApiResult<Response> {
    match format {
        "json" => Ok(Json(report).into_response()),
        "xlsx" => {
            let (headers, rows) = report.flat();
            let bytes = crate::export::xlsx(&report.title, &headers, &rows)?;
            Ok(file_response(
                &format!("{}.xlsx", report.file_stem()),
                bytes,
            ))
        }
        "pdf" => {
            let name = format!("{}.pdf", report.file_stem());
            let bytes = tokio::task::spawn_blocking(move || pdf::render(&report))
                .await
                .map_err(|_| ApiError::internal())??;
            Ok(file_response(&name, bytes))
        }
        _ => Err(ApiError::validation("format must be json, pdf or xlsx.")),
    }
}

async fn custom_sources(
    user: CurrentUser,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<builder::Catalogue>> {
    user.require(Permission::ReportsRead)?;
    Ok(Json(builder::catalogue(Lang::parse(
        q.get("lang").map(String::as_str),
    ))))
}

/// A saved report as the UI sees it.
#[derive(Debug, Serialize)]
pub struct CustomReportView {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub source: String,
    pub definition: builder::Definition,
    pub owner_user_id: Uuid,
    pub owner_name: Option<String>,
    pub is_shared: bool,
    pub can_edit: bool,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

async fn view(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    d: ReportDefinition,
) -> ApiResult<CustomReportView> {
    let definition: builder::Definition =
        serde_json::from_value(d.definition.clone()).map_err(|_| ApiError::internal())?;
    let owner_name = madrasa_db::users::get(c, d.owner_user_id)
        .await
        .ok()
        .map(|u| u.full_name);
    let can_edit = d.owner_user_id == user.user.id || user.can(Permission::ReportsManage);
    Ok(CustomReportView {
        id: d.id,
        name: d.name,
        description: d.description,
        source: d.source,
        definition,
        owner_user_id: d.owner_user_id,
        owner_name,
        is_shared: d.is_shared,
        can_edit,
        updated_at: d.updated_at,
    })
}

async fn list_custom(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<CustomReportView>>> {
    user.require(Permission::ReportsRead)?;
    let mut c = state.pool.acquire().await?;
    let mut out = Vec::new();
    for d in defs::list_visible(&mut c, user.user.school_id, user.user.id).await? {
        out.push(view(&mut c, &user, d).await?);
    }
    Ok(Json(out))
}

#[derive(Debug, Deserialize)]
pub struct CustomReportRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub definition: builder::Definition,
    #[serde(default)]
    pub is_shared: bool,
}

fn validate_request(user: &CurrentUser, req: &CustomReportRequest) -> ApiResult<()> {
    if req.name.trim().is_empty() || req.name.trim().chars().count() > 120 {
        return Err(ApiError::validation(
            "Give the report a name (up to 120 characters).",
        ));
    }
    builder::assemble(&req.definition)?;
    if req.is_shared && !user.can(Permission::ReportsManage) {
        return Err(ApiError::forbidden());
    }
    Ok(())
}

async fn create_custom(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<CustomReportRequest>,
) -> ApiResult<(axum::http::StatusCode, Json<CustomReportView>)> {
    user.require(Permission::ReportsRead)?;
    validate_request(&user, &req)?;
    let mut c = state.pool.acquire().await?;
    let row = defs::create(
        &mut c,
        &state.hlc,
        user.actor(),
        defs::NewDefinition {
            school_id: user.user.school_id,
            name: req.name,
            description: req.description,
            source: req.definition.source.clone(),
            definition: serde_json::to_value(&req.definition).map_err(|_| ApiError::internal())?,
            owner_user_id: user.user.id,
            is_shared: req.is_shared,
        },
    )
    .await?;
    Ok((
        axum::http::StatusCode::CREATED,
        Json(view(&mut c, &user, row).await?),
    ))
}

/// A saved report the caller may see: shared, or their own.
async fn load_visible(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    id: Uuid,
) -> ApiResult<ReportDefinition> {
    let d = defs::get(c, id).await?;
    if d.school_id != user.user.school_id || (!d.is_shared && d.owner_user_id != user.user.id) {
        return Err(ApiError::not_found("report"));
    }
    Ok(d)
}

async fn update_custom(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<CustomReportRequest>,
) -> ApiResult<Json<CustomReportView>> {
    user.require(Permission::ReportsRead)?;
    let mut c = state.pool.acquire().await?;
    let existing = load_visible(&mut c, &user, id).await?;
    if existing.owner_user_id != user.user.id && !user.can(Permission::ReportsManage) {
        return Err(ApiError::forbidden());
    }
    validate_request(&user, &req)?;
    let row = defs::update(
        &mut c,
        &state.hlc,
        user.actor(),
        id,
        defs::DefinitionUpdate {
            name: req.name,
            description: req.description,
            source: req.definition.source.clone(),
            definition: serde_json::to_value(&req.definition).map_err(|_| ApiError::internal())?,
            is_shared: req.is_shared,
        },
    )
    .await?;
    Ok(Json(view(&mut c, &user, row).await?))
}

async fn delete_custom(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<axum::http::StatusCode> {
    user.require(Permission::ReportsRead)?;
    let mut c = state.pool.acquire().await?;
    let existing = load_visible(&mut c, &user, id).await?;
    if existing.owner_user_id != user.user.id && !user.can(Permission::ReportsManage) {
        return Err(ApiError::forbidden());
    }
    defs::delete(&mut c, &state.hlc, user.actor(), id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct AdhocRequest {
    pub definition: builder::Definition,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub lang: Option<String>,
    #[serde(default)]
    pub academic_year_id: Option<String>,
    /// Preview: at most this many rows.
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Run a definition from the builder without saving it (preview).
async fn run_custom_adhoc(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<AdhocRequest>,
) -> ApiResult<Json<Report>> {
    user.require(Permission::ReportsRead)?;
    let mut params = Params::default();
    if let Some(l) = &req.lang {
        params.0.insert("lang".into(), l.clone());
    }
    if let Some(y) = &req.academic_year_id {
        params.0.insert("academic_year_id".into(), y.clone());
    }
    let (school, year, scope) = context_parts(&state, &user, &params).await?;
    let ctx = Context {
        school: &school,
        year: &year,
        lang: Lang::parse(req.lang.as_deref()),
        section_scope: scope,
    };
    let mut def = req.definition;
    if let Some(l) = req.limit {
        def.limit = Some(l.min(def.limit.unwrap_or(builder::DEFAULT_LIMIT)));
    }
    let mut c = state.pool.acquire().await?;
    let title = req
        .name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| "Custom report".into());
    let report = builder::run(&mut c, &ctx, &def, &title, "").await?;
    Ok(Json(report))
}

/// Run a saved report, like a built-in one.
async fn run_custom(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Response> {
    user.require(Permission::ReportsRead)?;
    let format = q.get("format").cloned().unwrap_or_else(|| "json".into());
    let params = Params(q);
    let (school, year, scope) = context_parts(&state, &user, &params).await?;
    let mut c = state.pool.acquire().await?;
    let saved = load_visible(&mut c, &user, id).await?;
    let def: builder::Definition =
        serde_json::from_value(saved.definition.clone()).map_err(|_| ApiError::internal())?;
    let ctx = Context {
        school: &school,
        year: &year,
        lang: Lang::parse(params.get("lang")),
        section_scope: scope,
    };
    let mut report = builder::run(&mut c, &ctx, &def, &saved.name, &saved.description).await?;
    report.code = format!("custom.{}", saved.id);
    audit::record(
        &mut c,
        &state.hlc,
        user.actor(),
        Action::Report,
        "report_definition",
        Some(saved.id),
        None,
        Some(json!({ "code": report.code, "params": params.as_json(), "rows": report.row_count })),
    )
    .await?;
    respond(report, &format).await
}
