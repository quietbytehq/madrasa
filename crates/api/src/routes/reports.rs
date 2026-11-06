//! Report library: catalogue and runs (M2 stage 1).

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use madrasa_core::permissions::Permission;
use madrasa_db::audit::{self, Action};
use serde_json::json;

use crate::auth::CurrentUser;
use crate::export::file_response;
use crate::reports::catalogue::{Access, ReportDef};
use crate::reports::{
    attendance, catalogue, exams, fees, pdf, students, Context, Lang, Params, Report,
};
use crate::{ApiError, ApiResult, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/reports", get(list))
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
        }
}

async fn list(
    user: CurrentUser,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult<Json<Vec<catalogue::CatalogueModule>>> {
    if !user.can(Permission::ReportsRead)
        && !user.can(Permission::AttendanceRead)
        && !user.can(Permission::ExamsRead)
    {
        return Err(ApiError::forbidden());
    }
    let lang = Lang::parse(q.get("lang").map(String::as_str));
    Ok(Json(catalogue::catalogue(lang, |d| may_run(&user, d))))
}

/// Build the report document for a code. Shared by the formats and by later stages.
pub async fn build(
    state: &AppState,
    user: &CurrentUser,
    code: &str,
    params: &Params,
) -> Result<Report, ApiError> {
    let def = catalogue::find(code).ok_or_else(|| ApiError::not_found("report"))?;
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
