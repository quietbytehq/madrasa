//! Admissions pipeline (Standard stage 3): configurable stages, applications with a dated
//! stage history, bulk moves, conversion of an admitted application into a student (with
//! guardian and enrolment, and an admission fee voucher when a fee head is configured),
//! the funnel, and the offer letter (PDF, en/ur). Every route needs the `admissions`
//! feature and `admissions.read`; changes need `admissions.manage`.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use chrono::NaiveDate;
use madrasa_core::admissions::StageKind;
use madrasa_core::edition::Feature;
use madrasa_core::permissions::Permission;
use madrasa_db::admissions as repo;
use madrasa_db::admissions::{ApplicationView, Event, Stage};
use madrasa_db::models::AcademicYear;
use madrasa_db::{academics, fees, guardians, settings, students};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::reports::{self, Lang};
use crate::{ApiError, ApiResult, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admissions/stages", get(list_stages).post(create_stage))
        .route(
            "/admissions/stages/{id}",
            put(update_stage).delete(delete_stage),
        )
        .route("/admissions/settings", get(get_settings).put(put_settings))
        .route("/admissions/board", get(board))
        .route("/admissions/applications", get(list).post(create))
        .route("/admissions/applications/move", post(bulk_move))
        .route(
            "/admissions/applications/{id}",
            get(get_one).put(update).delete(delete),
        )
        .route("/admissions/applications/{id}/move", post(move_one))
        .route("/admissions/applications/{id}/convert", post(convert))
        .route(
            "/admissions/applications/{id}/offer-letter",
            get(offer_letter),
        )
}

fn gate(state: &AppState, user: &CurrentUser, manage: bool) -> ApiResult<()> {
    state.require_feature(Feature::Admissions)?;
    user.require(if manage {
        Permission::AdmissionsManage
    } else {
        Permission::AdmissionsRead
    })
}

async fn year_of(
    c: &mut PgConnection,
    user: &CurrentUser,
    id: Option<Uuid>,
) -> ApiResult<AcademicYear> {
    match id {
        Some(id) => {
            let y = academics::get_year(c, id).await?;
            if y.school_id != user.user.school_id {
                return Err(ApiError::not_found("academic year"));
            }
            Ok(y)
        }
        None => academics::active_year(c, user.user.school_id)
            .await?
            .ok_or_else(|| ApiError::conflict("There is no active academic year.")),
    }
}

// ---- stages ---------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct StageRequest {
    pub name: String,
    #[serde(default)]
    pub code: String,
    pub kind: StageKind,
    #[serde(default)]
    pub position: i32,
}

impl StageRequest {
    fn input(&self) -> ApiResult<repo::StageInput> {
        if self.name.trim().is_empty() {
            return Err(ApiError::validation("The stage needs a name."));
        }
        Ok(repo::StageInput {
            code: self.code.clone(),
            name: self.name.clone(),
            kind: self.kind,
            position: self.position,
        })
    }
}

async fn list_stages(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<Stage>>> {
    gate(&state, &user, false)?;
    let mut tx = state.pool.begin().await?;
    let stages =
        repo::ensure_stages(&mut tx, &state.hlc, user.actor(), user.user.school_id).await?;
    tx.commit().await?;
    Ok(Json(stages))
}

async fn create_stage(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<StageRequest>,
) -> ApiResult<(StatusCode, Json<Stage>)> {
    gate(&state, &user, true)?;
    let input = req.input()?;
    let mut tx = state.pool.begin().await?;
    repo::ensure_stages(&mut tx, &state.hlc, user.actor(), user.user.school_id).await?;
    let row = repo::create_stage(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        &input,
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(row)))
}

async fn load_stage(c: &mut PgConnection, user: &CurrentUser, id: Uuid) -> ApiResult<Stage> {
    let s = repo::get_stage(c, id).await?;
    if s.school_id != user.user.school_id {
        return Err(ApiError::not_found("admission stage"));
    }
    Ok(s)
}

async fn update_stage(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<StageRequest>,
) -> ApiResult<Json<Stage>> {
    gate(&state, &user, true)?;
    let input = req.input()?;
    let mut tx = state.pool.begin().await?;
    load_stage(&mut tx, &user, id).await?;
    let row = repo::update_stage(&mut tx, &state.hlc, user.actor(), id, &input).await?;
    tx.commit().await?;
    Ok(Json(row))
}

async fn delete_stage(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    load_stage(&mut tx, &user, id).await?;
    repo::delete_stage(&mut tx, &state.hlc, user.actor(), id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- settings -------------------------------------------------------------------------------

/// Mirrored by `AdmissionSettings` in the UI.
#[derive(Debug, Serialize, Deserialize)]
pub struct AdmissionSettings {
    /// Fee head of the admission fee voucher raised on conversion (empty = no voucher).
    pub fee_head_id: Option<Uuid>,
    pub fee_head_name: Option<String>,
}

async fn fee_head(c: &mut PgConnection, school_id: Uuid) -> ApiResult<Option<Uuid>> {
    Ok(settings::get(c, Some(school_id), repo::KEY_FEE_HEAD)
        .await?
        .and_then(|v| v.as_str().and_then(|s| s.parse().ok())))
}

async fn get_settings(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<AdmissionSettings>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    let id = fee_head(&mut c, user.user.school_id).await?;
    let name = match id {
        Some(id) => fees::get_head(&mut c, id).await.ok().map(|h| h.name),
        None => None,
    };
    Ok(Json(AdmissionSettings {
        fee_head_id: id,
        fee_head_name: name,
    }))
}

#[derive(Debug, Deserialize)]
pub struct SettingsRequest {
    pub fee_head_id: Option<Uuid>,
}

async fn put_settings(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<SettingsRequest>,
) -> ApiResult<Json<AdmissionSettings>> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    let name = match req.fee_head_id {
        Some(id) => {
            let h = fees::get_head(&mut tx, id).await?;
            if h.school_id != user.user.school_id {
                return Err(ApiError::not_found("fee head"));
            }
            Some(h.name)
        }
        None => None,
    };
    settings::set(
        &mut tx,
        &state.hlc,
        user.actor(),
        Some(user.user.school_id),
        repo::KEY_FEE_HEAD,
        match req.fee_head_id {
            Some(id) => Value::String(id.to_string()),
            None => Value::Null,
        },
    )
    .await?;
    tx.commit().await?;
    Ok(Json(AdmissionSettings {
        fee_head_id: req.fee_head_id,
        fee_head_name: name,
    }))
}

// ---- applications ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub academic_year_id: Option<Uuid>,
    pub class_id: Option<Uuid>,
    pub stage_id: Option<Uuid>,
    pub q: Option<String>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Vec<ApplicationView>>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    let year = year_of(&mut c, &user, q.academic_year_id).await?;
    Ok(Json(
        repo::list(
            &mut c,
            user.user.school_id,
            &repo::ApplicationFilter {
                academic_year_id: Some(year.id),
                class_id: q.class_id,
                stage_id: q.stage_id,
                q: q.q,
                from: q.from,
                to: q.to,
            },
        )
        .await?,
    ))
}

/// Mirrored by `AdmissionsBoard` in the UI.
#[derive(Debug, Serialize)]
pub struct Board {
    pub academic_year: AcademicYear,
    pub stages: Vec<Stage>,
    pub applications: Vec<ApplicationView>,
    /// Per stage: applications that reached it and that sit in it now.
    pub funnel: Vec<FunnelRow>,
}

#[derive(Debug, Serialize)]
pub struct FunnelRow {
    pub stage_id: Uuid,
    pub reached: usize,
    pub current: usize,
}

async fn board(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Board>> {
    gate(&state, &user, false)?;
    let mut tx = state.pool.begin().await?;
    let year = year_of(&mut tx, &user, q.academic_year_id).await?;
    let stages =
        repo::ensure_stages(&mut tx, &state.hlc, user.actor(), user.user.school_id).await?;
    let applications = repo::list(
        &mut tx,
        user.user.school_id,
        &repo::ApplicationFilter {
            academic_year_id: Some(year.id),
            class_id: q.class_id,
            stage_id: None,
            q: q.q,
            from: q.from,
            to: q.to,
        },
    )
    .await?;
    let funnel = repo::funnel(&mut tx, user.user.school_id, year.id)
        .await?
        .into_iter()
        .map(|(s, reached, current)| FunnelRow {
            stage_id: s.id,
            reached,
            current,
        })
        .collect();
    tx.commit().await?;
    Ok(Json(Board {
        academic_year: year,
        stages,
        applications,
        funnel,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ApplicationRequest {
    pub academic_year_id: Option<Uuid>,
    #[serde(default)]
    pub class_id: Option<Uuid>,
    #[serde(default)]
    pub stage_id: Option<Uuid>,
    #[serde(default)]
    pub enquiry_date: Option<NaiveDate>,
    #[serde(default)]
    pub source: String,
    pub full_name: String,
    #[serde(default)]
    pub father_name: String,
    #[serde(default)]
    pub date_of_birth: Option<NaiveDate>,
    #[serde(default)]
    pub gender: String,
    #[serde(default)]
    pub cnic: Option<String>,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub previous_school: String,
    #[serde(default)]
    pub guardian_name: String,
    #[serde(default = "father")]
    pub guardian_relationship: String,
    #[serde(default)]
    pub guardian_phone: Option<String>,
    #[serde(default)]
    pub guardian_cnic: Option<String>,
    #[serde(default)]
    pub guardian_email: Option<String>,
    #[serde(default)]
    pub guardian_occupation: String,
    #[serde(default)]
    pub test_date: Option<NaiveDate>,
    #[serde(default)]
    pub test_marks: Option<Decimal>,
    #[serde(default)]
    pub test_total: Option<Decimal>,
    #[serde(default)]
    pub interview_date: Option<NaiveDate>,
    #[serde(default)]
    pub interview_notes: String,
    #[serde(default)]
    pub offer_date: Option<NaiveDate>,
    #[serde(default)]
    pub offer_fee: Option<Decimal>,
    #[serde(default)]
    pub offer_expires: Option<NaiveDate>,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub custom: Value,
}

fn father() -> String {
    "father".into()
}

fn clean(s: &Option<String>) -> Option<String> {
    s.as_deref()
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(str::to_owned)
}

impl ApplicationRequest {
    async fn input(
        &self,
        c: &mut PgConnection,
        user: &CurrentUser,
        country: &str,
    ) -> ApiResult<repo::ApplicationInput> {
        if self.full_name.trim().is_empty() {
            return Err(ApiError::validation("The applicant needs a name."));
        }
        let year = year_of(c, user, self.academic_year_id).await?;
        if let Some(cid) = self.class_id {
            if academics::get_class(c, cid).await?.school_id != user.user.school_id {
                return Err(ApiError::not_found("class"));
            }
        }
        let cnic = crate::routes::students::clean_cnic(country, clean(&self.cnic))?;
        let guardian_cnic =
            crate::routes::students::clean_cnic(country, clean(&self.guardian_cnic))?;
        Ok(repo::ApplicationInput {
            academic_year_id: year.id,
            class_id: self.class_id,
            enquiry_date: self.enquiry_date.unwrap_or_else(crate::clock::today),
            source: self.source.clone(),
            full_name: self.full_name.clone(),
            father_name: self.father_name.clone(),
            date_of_birth: self.date_of_birth,
            gender: self.gender.trim().to_lowercase(),
            cnic,
            address: self.address.clone(),
            phone: clean(&self.phone),
            email: clean(&self.email),
            previous_school: self.previous_school.clone(),
            guardian_name: self.guardian_name.clone(),
            guardian_relationship: self.guardian_relationship.trim().to_lowercase(),
            guardian_phone: clean(&self.guardian_phone),
            guardian_cnic,
            guardian_email: clean(&self.guardian_email),
            guardian_occupation: self.guardian_occupation.clone(),
            test_date: self.test_date,
            test_marks: self.test_marks,
            test_total: self.test_total,
            interview_date: self.interview_date,
            interview_notes: self.interview_notes.clone(),
            offer_date: self.offer_date,
            offer_fee: self.offer_fee,
            offer_expires: self.offer_expires,
            notes: self.notes.clone(),
            custom: if self.custom.is_object() {
                self.custom.clone()
            } else {
                serde_json::json!({})
            },
        })
    }
}

/// Mirrored by `ApplicationDetail` in the UI.
#[derive(Debug, Serialize)]
pub struct ApplicationDetail {
    #[serde(flatten)]
    pub application: ApplicationView,
    pub events: Vec<Event>,
}

async fn load(c: &mut PgConnection, user: &CurrentUser, id: Uuid) -> ApiResult<ApplicationView> {
    let a = repo::get(c, id).await?;
    if a.application.school_id != user.user.school_id {
        return Err(ApiError::not_found("application"));
    }
    Ok(a)
}

async fn detail(
    c: &mut PgConnection,
    user: &CurrentUser,
    id: Uuid,
) -> ApiResult<ApplicationDetail> {
    let application = load(c, user, id).await?;
    let events = repo::events(c, id).await?;
    Ok(ApplicationDetail {
        application,
        events,
    })
}

async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<ApplicationRequest>,
) -> ApiResult<(StatusCode, Json<ApplicationDetail>)> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    let input = req.input(&mut tx, &user, &state.profile().country).await?;
    if let Some(s) = req.stage_id {
        load_stage(&mut tx, &user, s).await?;
    }
    let row = repo::create(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        &input,
        req.stage_id,
        Some(user.user.id),
    )
    .await?;
    let d = detail(&mut tx, &user, row.id).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(d)))
}

async fn get_one(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<ApplicationDetail>> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(detail(&mut c, &user, id).await?))
}

async fn update(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<ApplicationRequest>,
) -> ApiResult<Json<ApplicationDetail>> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    load(&mut tx, &user, id).await?;
    let input = req.input(&mut tx, &user, &state.profile().country).await?;
    repo::update(&mut tx, &state.hlc, user.actor(), id, &input).await?;
    let d = detail(&mut tx, &user, id).await?;
    tx.commit().await?;
    Ok(Json(d))
}

async fn delete(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    load(&mut tx, &user, id).await?;
    repo::delete(&mut tx, &state.hlc, user.actor(), id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct MoveRequest {
    pub stage_id: Uuid,
    #[serde(default)]
    pub note: String,
}

async fn move_one(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<MoveRequest>,
) -> ApiResult<Json<ApplicationDetail>> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    load(&mut tx, &user, id).await?;
    load_stage(&mut tx, &user, req.stage_id).await?;
    repo::move_stage(
        &mut tx,
        &state.hlc,
        user.actor(),
        id,
        req.stage_id,
        &req.note,
        Some(user.user.id),
    )
    .await?;
    let d = detail(&mut tx, &user, id).await?;
    tx.commit().await?;
    Ok(Json(d))
}

#[derive(Debug, Deserialize)]
pub struct BulkMoveRequest {
    pub ids: Vec<Uuid>,
    pub stage_id: Uuid,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Serialize)]
pub struct BulkMoveResult {
    pub moved: usize,
    /// Applications that could not move, with the reason.
    pub refused: Vec<(Uuid, String)>,
}

/// Move several applications; the ones a rule refuses are reported and the rest still move.
async fn bulk_move(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<BulkMoveRequest>,
) -> ApiResult<Json<BulkMoveResult>> {
    gate(&state, &user, true)?;
    let mut tx = state.pool.begin().await?;
    load_stage(&mut tx, &user, req.stage_id).await?;
    let mut moved = 0;
    let mut refused = Vec::new();
    for id in &req.ids {
        load(&mut tx, &user, *id).await?;
        match repo::move_stage(
            &mut tx,
            &state.hlc,
            user.actor(),
            *id,
            req.stage_id,
            &req.note,
            Some(user.user.id),
        )
        .await
        {
            Ok(_) => moved += 1,
            Err(madrasa_db::DbError::Conflict(m)) => refused.push((*id, m)),
            Err(e) => return Err(e.into()),
        }
    }
    tx.commit().await?;
    Ok(Json(BulkMoveResult { moved, refused }))
}

// ---- conversion -----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ConvertRequest {
    pub section_id: Uuid,
    #[serde(default)]
    pub roll_no: Option<i32>,
    #[serde(default)]
    pub admission_date: Option<NaiveDate>,
    #[serde(default)]
    pub student_number: Option<String>,
    /// Create even if a similar student exists.
    #[serde(default)]
    pub force: bool,
}

/// Mirrored by `ConversionResult` in the UI.
#[derive(Debug, Serialize)]
pub struct ConversionResult {
    pub application: ApplicationDetail,
    pub student_id: Uuid,
    pub student_number: String,
    pub guardian_id: Option<Uuid>,
    pub voucher_id: Option<Uuid>,
}

/// Turn an admitted application into a student, guardian and enrolment in one step, and
/// raise the admission fee voucher when a fee head is configured and an offer fee is set.
async fn convert(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<ConvertRequest>,
) -> ApiResult<Json<ConversionResult>> {
    gate(&state, &user, true)?;
    user.require(Permission::StudentsManage)?;
    let school = user.user.school_id;
    let mut tx = state.pool.begin().await?;
    let view = load(&mut tx, &user, id).await?;
    let app = view.application.clone();
    if app.student_id.is_some() {
        return Err(ApiError::conflict(
            "This application was already converted.",
        ));
    }
    if view.stage_kind != StageKind::Admitted.as_str() {
        return Err(ApiError::validation(
            "Move the application to an admitted stage first.",
        ));
    }
    let section = academics::get_section(&mut tx, req.section_id).await?;
    if section.school_id != school {
        return Err(ApiError::not_found("section"));
    }
    if section.academic_year_id != app.academic_year_id {
        return Err(ApiError::validation(
            "The section belongs to another academic year than the application.",
        ));
    }
    let dob = app.date_of_birth.ok_or_else(|| {
        ApiError::validation("Record the applicant's date of birth before converting.")
    })?;
    if app.father_name.trim().is_empty() {
        return Err(ApiError::validation(
            "Record the father's name before converting.",
        ));
    }
    let gender = if app.gender.is_empty() {
        "other".to_string()
    } else {
        app.gender.clone()
    };
    if !req.force {
        let matches = students::find_duplicates(
            &mut tx,
            school,
            &madrasa_core::students::DuplicateProbe {
                full_name: app.full_name.clone(),
                father_name: app.father_name.clone(),
                date_of_birth: dob,
                cnic: app.cnic.clone(),
            },
            None,
        )
        .await?;
        if !matches.is_empty() {
            let summary = matches
                .iter()
                .map(|m| format!("{} ({})", m.student.full_name, m.student.student_number))
                .collect::<Vec<_>>()
                .join(", ");
            let mut e = ApiError::new(
                StatusCode::CONFLICT,
                "duplicate",
                format!("A similar student already exists: {summary}. Convert anyway if this is someone else."),
            );
            e.details = serde_json::to_value(&matches).ok();
            return Err(e);
        }
    }
    let admission_date = req
        .admission_date
        .or(app.admitted_on)
        .unwrap_or_else(crate::clock::today);
    let student = students::create(
        &mut tx,
        &state.hlc,
        user.actor(),
        students::NewStudent {
            school_id: school,
            student_number: clean(&req.student_number),
            legacy_id: None,
            full_name: app.full_name.clone(),
            father_name: app.father_name.clone(),
            date_of_birth: dob,
            gender,
            cnic: app.cnic.clone(),
            address: app.address.clone(),
            admission_date,
            board_roll_no: None,
            board_registration_no: None,
            custom: serde_json::json!({ "application_no": app.application_no, "previous_school": app.previous_school }),
        },
    )
    .await?;
    students::enrol(
        &mut tx,
        &state.hlc,
        user.actor(),
        student.id,
        section.id,
        req.roll_no,
    )
    .await?;
    let mut guardian_id = None;
    if !app.guardian_name.trim().is_empty() {
        let existing = guardians::find_matching(
            &mut tx,
            school,
            app.guardian_cnic.as_deref(),
            app.guardian_phone.as_deref(),
            &app.guardian_name,
        )
        .await?;
        let g = match existing {
            Some(g) => g,
            None => {
                guardians::create(
                    &mut tx,
                    &state.hlc,
                    user.actor(),
                    guardians::NewGuardian {
                        school_id: school,
                        full_name: app.guardian_name.clone(),
                        phone: app.guardian_phone.clone(),
                        email: app.guardian_email.clone(),
                        cnic: app.guardian_cnic.clone(),
                        occupation: app.guardian_occupation.clone(),
                        address: app.address.clone(),
                        custom: serde_json::json!({}),
                    },
                )
                .await?
            }
        };
        guardians::link(
            &mut tx,
            &state.hlc,
            user.actor(),
            student.id,
            g.id,
            &app.guardian_relationship,
            true,
        )
        .await?;
        guardian_id = Some(g.id);
    }
    let mut voucher_id = None;
    if let (Some(head), Some(fee)) = (fee_head(&mut tx, school).await?, app.offer_fee) {
        if fee > Decimal::ZERO {
            let v = fees::create_one_off_voucher(
                &mut tx,
                &state.hlc,
                user.actor(),
                school,
                student.id,
                app.academic_year_id,
                Some(section.id),
                admission_date,
                head,
                "Admission fee",
                fee,
            )
            .await?;
            voucher_id = Some(v.id);
        }
    }
    repo::link_student(
        &mut tx,
        &state.hlc,
        user.actor(),
        id,
        student.id,
        admission_date,
    )
    .await?;
    let d = detail(&mut tx, &user, id).await?;
    tx.commit().await?;
    Ok(Json(ConversionResult {
        application: d,
        student_id: student.id,
        student_number: student.student_number,
        guardian_id,
        voucher_id,
    }))
}

// ---- offer letter ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct LangQuery {
    pub lang: Option<String>,
}

async fn offer_letter(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Query(q): Query<LangQuery>,
) -> ApiResult<Response> {
    gate(&state, &user, false)?;
    let mut c = state.pool.acquire().await?;
    let view = load(&mut c, &user, id).await?;
    let school = madrasa_db::schools::get(&mut c, user.user.school_id).await?;
    let year = academics::get_year(&mut c, view.application.academic_year_id).await?;
    let lang = Lang::parse(q.lang.as_deref());
    let bytes =
        reports::admissions::offer_letter(lang, &school, &year, &view, &state.profile().currency)?;
    Ok(crate::export::file_response(
        &format!("offer-{}.pdf", view.application.application_no),
        bytes,
    ))
}
