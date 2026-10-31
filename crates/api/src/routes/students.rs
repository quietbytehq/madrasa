//! Student directory, profiles, guardians and photos.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use chrono::NaiveDate;
use madrasa_core::permissions::Permission;
use madrasa_core::students::{
    normalise_cnic, normalise_phone, DuplicateProbe, Relationship, StudentStatus, MAX_PHOTO_BYTES,
};
use madrasa_db::models::{Enrollment, Guardian, GuardianLink, Student, StudentListRow};
use madrasa_db::students::{DuplicateMatch, Page, StudentFilter, StudentSort};
use madrasa_db::{academics, guardians, students};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::{CurrentUser, StudentScope};
use crate::{ApiError, ApiResult, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/students", get(list).post(create))
        .route("/students/check-duplicates", post(check_duplicates))
        .route("/students/{id}", get(get_one).patch(update))
        .route("/students/{id}/withdraw", post(withdraw))
        .route("/students/{id}/enrol", post(enrol))
        .route(
            "/students/{id}/photo",
            get(get_photo).put(put_photo).delete(delete_photo),
        )
        .route("/students/{id}/guardians", post(link_guardian))
        .route(
            "/students/{id}/guardians/{guardian_id}",
            axum::routing::delete(unlink_guardian),
        )
        .route("/guardians", get(search_guardians).post(create_guardian))
        .route("/guardians/{id}", get(get_guardian).patch(update_guardian))
}

type JsonBody<T> = Result<Json<T>, axum::extract::rejection::JsonRejection>;

// ---- helpers ------------------------------------------------------------------------------

fn clean_cnic(v: Option<String>) -> ApiResult<Option<String>> {
    match v.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => normalise_cnic(s).map(Some).ok_or_else(|| {
            ApiError::validation("CNIC / B-Form must have 13 digits, e.g. 42101-1234567-1.")
        }),
        None => Ok(None),
    }
}

fn clean_phone(v: Option<String>) -> ApiResult<Option<String>> {
    match v.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => normalise_phone(s).map(Some).ok_or_else(|| {
            ApiError::validation("Phone number must have 11 digits, e.g. 0300-1234567.")
        }),
        None => Ok(None),
    }
}

fn check_gender(g: &str) -> ApiResult<()> {
    if matches!(g, "male" | "female" | "other") {
        Ok(())
    } else {
        Err(ApiError::validation(
            "Gender must be male, female or other.",
        ))
    }
}

fn clean_opt(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Load a student of the caller's school, applying the teacher section scope.
async fn load_scoped(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    id: Uuid,
) -> ApiResult<(Student, Option<Uuid>)> {
    let student = students::get(c, id).await?;
    if student.school_id != user.user.school_id {
        return Err(ApiError::not_found("student"));
    }
    let scope = user.student_scope(c).await?;
    let current_section = match academics::active_year(c, user.user.school_id).await? {
        Some(y) => students::section_in_year(c, id, y.id).await?,
        None => None,
    };
    if !scope.allows_section(current_section) {
        return Err(ApiError::forbidden());
    }
    Ok((student, current_section))
}

// ---- directory ----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub q: Option<String>,
    pub class_id: Option<Uuid>,
    pub section_id: Option<Uuid>,
    pub status: Option<String>,
    /// name | number | class | admission_date
    pub sort: Option<String>,
    pub order: Option<String>,
    pub page: Option<i64>,
    pub page_size: Option<i64>,
}

async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<ListQuery>,
) -> ApiResult<Json<Page<StudentListRow>>> {
    let mut c = state.pool.acquire().await?;
    let scope = user.student_scope(&mut c).await?;
    if let Some(s) = &q.status {
        s.parse::<StudentStatus>()
            .map_err(|_| ApiError::validation("Status must be active, left or graduated."))?;
    }
    let sort = match q.sort.as_deref().unwrap_or("name") {
        "name" => StudentSort::Name,
        "number" => StudentSort::Number,
        "class" => StudentSort::Class,
        "admission_date" => StudentSort::AdmissionDate,
        _ => {
            return Err(ApiError::validation(
                "Sort must be name, number, class or admission_date.",
            ))
        }
    };
    let filter = StudentFilter {
        q: q.q,
        class_id: q.class_id,
        section_id: q.section_id,
        status: q.status,
        section_scope: match scope {
            StudentScope::All => None,
            StudentScope::Sections(ids) => Some(ids),
        },
    };
    let page = students::list(
        &mut c,
        user.user.school_id,
        &filter,
        sort,
        q.order.as_deref() == Some("desc"),
        q.page.unwrap_or(1),
        q.page_size.unwrap_or(50),
    )
    .await?;
    Ok(Json(page))
}

// ---- create / update ----------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct GuardianInput {
    /// Link an existing guardian instead of creating one.
    pub guardian_id: Option<Uuid>,
    pub full_name: Option<String>,
    pub relationship: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub cnic: Option<String>,
    pub occupation: Option<String>,
    pub address: Option<String>,
    #[serde(default)]
    pub is_primary: bool,
}

#[derive(Debug, Deserialize)]
pub struct NewStudentBody {
    pub full_name: String,
    pub father_name: String,
    pub date_of_birth: NaiveDate,
    pub gender: String,
    pub cnic: Option<String>,
    pub address: Option<String>,
    pub admission_date: Option<NaiveDate>,
    pub student_number: Option<String>,
    pub legacy_id: Option<String>,
    pub board_roll_no: Option<String>,
    pub board_registration_no: Option<String>,
    pub section_id: Option<Uuid>,
    pub roll_no: Option<i32>,
    #[serde(default)]
    pub custom: serde_json::Value,
    #[serde(default)]
    pub guardians: Vec<GuardianInput>,
    /// Create even if duplicates were found.
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Serialize)]
pub struct StudentDetail {
    #[serde(flatten)]
    pub student: Student,
    pub guardians: Vec<GuardianLink>,
    pub enrollments: Vec<EnrollmentView>,
    pub current: Option<EnrollmentView>,
    pub siblings: Vec<StudentListRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnrollmentView {
    #[serde(flatten)]
    pub enrollment: Enrollment,
    pub academic_year_name: String,
    pub class_id: Uuid,
    pub class_name: String,
    pub section_name: String,
}

async fn enrollment_view(c: &mut sqlx::PgConnection, e: Enrollment) -> ApiResult<EnrollmentView> {
    let section = academics::get_section(c, e.section_id).await?;
    let class = academics::get_class(c, section.class_id).await?;
    let year = academics::get_year(c, e.academic_year_id).await?;
    Ok(EnrollmentView {
        enrollment: e,
        academic_year_name: year.name,
        class_id: class.id,
        class_name: class.name,
        section_name: section.name,
    })
}

async fn detail(
    c: &mut sqlx::PgConnection,
    student: Student,
    current_section: Option<Uuid>,
) -> ApiResult<StudentDetail> {
    let guardians = guardians::for_student(c, student.id).await?;
    let siblings = guardians::siblings(c, student.id).await?;
    let mut enrollments = Vec::new();
    for e in students::enrollments(c, student.id).await? {
        enrollments.push(enrollment_view(c, e).await?);
    }
    let current = enrollments
        .iter()
        .find(|e| Some(e.enrollment.section_id) == current_section)
        .cloned();
    Ok(StudentDetail {
        student,
        guardians,
        enrollments,
        current,
        siblings,
    })
}

fn probe(full_name: &str, father_name: &str, dob: NaiveDate, cnic: Option<&str>) -> DuplicateProbe {
    DuplicateProbe {
        full_name: full_name.to_string(),
        father_name: father_name.to_string(),
        date_of_birth: dob,
        cnic: cnic.map(String::from),
    }
}

/// Create (or link) a guardian for a student. Existing records matched by CNIC/phone are reused
/// so that siblings share one guardian.
async fn attach_guardian(
    c: &mut sqlx::PgConnection,
    state: &AppState,
    user: &CurrentUser,
    student_id: Uuid,
    g: GuardianInput,
) -> ApiResult<()> {
    g.relationship.parse::<Relationship>().map_err(|_| {
        ApiError::validation("Relationship must be father, mother, guardian or other.")
    })?;
    let sid = user.user.school_id;
    let guardian_id = match g.guardian_id {
        Some(id) => {
            if guardians::get(c, id).await?.school_id != sid {
                return Err(ApiError::not_found("guardian"));
            }
            id
        }
        None => {
            let full_name = g
                .full_name
                .as_deref()
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .ok_or_else(|| ApiError::validation("Guardian name is required."))?;
            let cnic = clean_cnic(g.cnic)?;
            let phone = clean_phone(g.phone)?;
            match guardians::find_matching(c, sid, cnic.as_deref(), phone.as_deref()).await? {
                Some(existing) => existing.id,
                None => {
                    guardians::create(
                        c,
                        &state.hlc,
                        user.actor(),
                        guardians::NewGuardian {
                            school_id: sid,
                            full_name: full_name.into(),
                            phone,
                            email: clean_opt(g.email),
                            cnic,
                            occupation: g.occupation.unwrap_or_default(),
                            address: g.address.unwrap_or_default(),
                        },
                    )
                    .await?
                    .id
                }
            }
        }
    };
    guardians::link(
        c,
        &state.hlc,
        user.actor(),
        student_id,
        guardian_id,
        &g.relationship,
        g.is_primary,
    )
    .await?;
    Ok(())
}

async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<NewStudentBody>,
) -> ApiResult<(StatusCode, Json<StudentDetail>)> {
    user.require(Permission::StudentsManage)?;
    let Json(b) = body?;
    if b.full_name.trim().is_empty() || b.father_name.trim().is_empty() {
        return Err(ApiError::validation(
            "Student name and father's name are required.",
        ));
    }
    check_gender(&b.gender)?;
    let cnic = clean_cnic(b.cnic)?;
    let sid = user.user.school_id;
    let mut tx = state.pool.begin().await?;
    if !b.force {
        let matches = students::find_duplicates(
            &mut tx,
            sid,
            &probe(
                &b.full_name,
                &b.father_name,
                b.date_of_birth,
                cnic.as_deref(),
            ),
            None,
        )
        .await?;
        if !matches.is_empty() {
            return Err(duplicate_error(matches));
        }
    }
    if let Some(section_id) = b.section_id {
        if academics::get_section(&mut tx, section_id).await?.school_id != sid {
            return Err(ApiError::not_found("section"));
        }
    }
    let student = students::create(
        &mut tx,
        &state.hlc,
        user.actor(),
        students::NewStudent {
            school_id: sid,
            student_number: clean_opt(b.student_number),
            legacy_id: clean_opt(b.legacy_id),
            full_name: b.full_name,
            father_name: b.father_name,
            date_of_birth: b.date_of_birth,
            gender: b.gender,
            cnic,
            address: b.address.unwrap_or_default(),
            admission_date: b.admission_date.unwrap_or_else(crate::clock::today),
            board_roll_no: clean_opt(b.board_roll_no),
            board_registration_no: clean_opt(b.board_registration_no),
            custom: if b.custom.is_object() {
                b.custom
            } else {
                serde_json::json!({})
            },
        },
    )
    .await?;
    let mut current_section = None;
    if let Some(section_id) = b.section_id {
        students::enrol(
            &mut tx,
            &state.hlc,
            user.actor(),
            student.id,
            section_id,
            b.roll_no,
        )
        .await?;
        current_section = Some(section_id);
    }
    for g in b.guardians {
        attach_guardian(&mut tx, &state, &user, student.id, g).await?;
    }
    let d = detail(&mut tx, student, current_section).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(d)))
}

fn duplicate_error(matches: Vec<DuplicateMatch>) -> ApiError {
    let summary = matches
        .iter()
        .map(|m| format!("{} ({})", m.student.full_name, m.student.student_number))
        .collect::<Vec<_>>()
        .join(", ");
    let mut err = ApiError::new(
        StatusCode::CONFLICT,
        "possible_duplicate",
        format!(
            "A student with the same details already exists: {summary}. Check before adding again."
        ),
    );
    err.details = serde_json::to_value(matches).ok();
    err
}

#[derive(Debug, Deserialize)]
pub struct DuplicateQuery {
    pub full_name: String,
    pub father_name: String,
    pub date_of_birth: NaiveDate,
    pub cnic: Option<String>,
    pub exclude_id: Option<Uuid>,
}

async fn check_duplicates(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<DuplicateQuery>,
) -> ApiResult<Json<Vec<DuplicateMatch>>> {
    user.require(Permission::StudentsManage)?;
    let Json(b) = body?;
    let cnic = b.cnic.as_deref().and_then(normalise_cnic);
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        students::find_duplicates(
            &mut c,
            user.user.school_id,
            &probe(
                &b.full_name,
                &b.father_name,
                b.date_of_birth,
                cnic.as_deref(),
            ),
            b.exclude_id,
        )
        .await?,
    ))
}

async fn get_one(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<StudentDetail>> {
    let mut c = state.pool.acquire().await?;
    let (student, current) = load_scoped(&mut c, &user, id).await?;
    Ok(Json(detail(&mut c, student, current).await?))
}

#[derive(Debug, Deserialize)]
pub struct StudentPatch {
    pub full_name: Option<String>,
    pub father_name: Option<String>,
    pub date_of_birth: Option<NaiveDate>,
    pub gender: Option<String>,
    #[serde(default, deserialize_with = "crate::routes::double_option")]
    pub cnic: Option<Option<String>>,
    pub address: Option<String>,
    pub admission_date: Option<NaiveDate>,
    #[serde(default, deserialize_with = "crate::routes::double_option")]
    pub legacy_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "crate::routes::double_option")]
    pub board_roll_no: Option<Option<String>>,
    #[serde(default, deserialize_with = "crate::routes::double_option")]
    pub board_registration_no: Option<Option<String>>,
    pub custom: Option<serde_json::Value>,
}

async fn update(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: JsonBody<StudentPatch>,
) -> ApiResult<Json<StudentDetail>> {
    user.require(Permission::StudentsManage)?;
    let Json(b) = body?;
    if b.full_name.as_deref().is_some_and(|n| n.trim().is_empty())
        || b.father_name
            .as_deref()
            .is_some_and(|n| n.trim().is_empty())
    {
        return Err(ApiError::validation(
            "Student name and father's name are required.",
        ));
    }
    if let Some(g) = &b.gender {
        check_gender(g)?;
    }
    let cnic = match b.cnic {
        Some(v) => Some(clean_cnic(v)?),
        None => None,
    };
    let mut tx = state.pool.begin().await?;
    let (_, current) = load_scoped(&mut tx, &user, id).await?;
    let student = students::update(
        &mut tx,
        &state.hlc,
        user.actor(),
        id,
        students::StudentUpdate {
            legacy_id: b.legacy_id.map(clean_opt),
            full_name: b.full_name,
            father_name: b.father_name,
            date_of_birth: b.date_of_birth,
            gender: b.gender,
            cnic,
            address: b.address,
            admission_date: b.admission_date,
            board_roll_no: b.board_roll_no.map(clean_opt),
            board_registration_no: b.board_registration_no.map(clean_opt),
            custom: b.custom.filter(|c| c.is_object()),
        },
    )
    .await?;
    let d = detail(&mut tx, student, current).await?;
    tx.commit().await?;
    Ok(Json(d))
}

#[derive(Debug, Deserialize)]
pub struct WithdrawBody {
    /// left | graduated | active (re-admit)
    pub status: String,
    pub left_on: Option<NaiveDate>,
    pub reason: Option<String>,
}

async fn withdraw(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: JsonBody<WithdrawBody>,
) -> ApiResult<Json<StudentDetail>> {
    user.require(Permission::StudentsManage)?;
    let Json(b) = body?;
    let status: StudentStatus = b
        .status
        .parse()
        .map_err(|_| ApiError::validation("Status must be active, left or graduated."))?;
    if status != StudentStatus::Active
        && b.reason.as_deref().map(str::trim).unwrap_or("").is_empty()
    {
        return Err(ApiError::validation(
            "A reason is required when a student leaves.",
        ));
    }
    let mut tx = state.pool.begin().await?;
    let (_, current) = load_scoped(&mut tx, &user, id).await?;
    let left_on = b.left_on.unwrap_or_else(crate::clock::today);
    let student = students::set_status(
        &mut tx,
        &state.hlc,
        user.actor(),
        id,
        status.as_str(),
        Some(left_on),
        b.reason.as_deref().map(str::trim),
    )
    .await?;
    let d = detail(&mut tx, student, current).await?;
    tx.commit().await?;
    Ok(Json(d))
}

#[derive(Debug, Deserialize)]
pub struct EnrolBody {
    pub section_id: Uuid,
    pub roll_no: Option<i32>,
}

async fn enrol(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: JsonBody<EnrolBody>,
) -> ApiResult<Json<StudentDetail>> {
    user.require(Permission::StudentsManage)?;
    let Json(b) = body?;
    let mut tx = state.pool.begin().await?;
    let (student, _) = load_scoped(&mut tx, &user, id).await?;
    let section = academics::get_section(&mut tx, b.section_id).await?;
    if section.school_id != user.user.school_id {
        return Err(ApiError::not_found("section"));
    }
    students::enrol(
        &mut tx,
        &state.hlc,
        user.actor(),
        id,
        b.section_id,
        b.roll_no,
    )
    .await?;
    let active = academics::active_year(&mut tx, user.user.school_id).await?;
    let current = match active {
        Some(y) => students::section_in_year(&mut tx, id, y.id).await?,
        None => None,
    };
    let d = detail(&mut tx, student, current).await?;
    tx.commit().await?;
    Ok(Json(d))
}

// ---- photo --------------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct PhotoBody {
    pub mime: String,
    /// Base64 (standard alphabet) image bytes.
    pub data: String,
}

async fn get_photo(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<axum::response::Response> {
    use axum::response::IntoResponse;
    let mut c = state.pool.acquire().await?;
    load_scoped(&mut c, &user, id).await?;
    match students::photo(&mut c, id).await? {
        Some((bytes, mime)) => Ok((
            [
                (axum::http::header::CONTENT_TYPE, mime),
                (
                    axum::http::header::CACHE_CONTROL,
                    "private, max-age=60".into(),
                ),
            ],
            bytes,
        )
            .into_response()),
        None => Err(ApiError::not_found("photo")),
    }
}

async fn put_photo(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: JsonBody<PhotoBody>,
) -> ApiResult<StatusCode> {
    user.require(Permission::StudentsManage)?;
    let Json(b) = body?;
    if !matches!(b.mime.as_str(), "image/jpeg" | "image/png" | "image/webp") {
        return Err(ApiError::validation(
            "Photo must be a JPEG, PNG or WebP image.",
        ));
    }
    let bytes = B64
        .decode(b.data.trim())
        .map_err(|_| ApiError::validation("Photo data is not valid base64."))?;
    if bytes.is_empty() || bytes.len() > MAX_PHOTO_BYTES {
        return Err(ApiError::validation(
            "Photo must be between 1 byte and 512 KB.",
        ));
    }
    let mut c = state.pool.acquire().await?;
    load_scoped(&mut c, &user, id).await?;
    students::set_photo(&mut c, &state.hlc, user.actor(), id, Some((bytes, b.mime))).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_photo(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    user.require(Permission::StudentsManage)?;
    let mut c = state.pool.acquire().await?;
    load_scoped(&mut c, &user, id).await?;
    students::set_photo(&mut c, &state.hlc, user.actor(), id, None).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- guardians ----------------------------------------------------------------------------

async fn link_guardian(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: JsonBody<GuardianInput>,
) -> ApiResult<Json<Vec<GuardianLink>>> {
    user.require(Permission::StudentsManage)?;
    let Json(g) = body?;
    let mut tx = state.pool.begin().await?;
    load_scoped(&mut tx, &user, id).await?;
    attach_guardian(&mut tx, &state, &user, id, g).await?;
    let links = guardians::for_student(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(links))
}

async fn unlink_guardian(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, guardian_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<Vec<GuardianLink>>> {
    user.require(Permission::StudentsManage)?;
    let mut tx = state.pool.begin().await?;
    load_scoped(&mut tx, &user, id).await?;
    guardians::unlink(&mut tx, &state.hlc, user.actor(), id, guardian_id).await?;
    let links = guardians::for_student(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(links))
}

#[derive(Debug, Deserialize)]
pub struct GuardianQuery {
    pub q: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GuardianView {
    #[serde(flatten)]
    pub guardian: Guardian,
    pub students: Vec<StudentListRow>,
}

async fn search_guardians(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<GuardianQuery>,
) -> ApiResult<Json<Vec<GuardianView>>> {
    user.require(Permission::StudentsRead)?;
    let needle = q.q.as_deref().map(str::trim).unwrap_or("");
    if needle.len() < 2 {
        return Ok(Json(Vec::new()));
    }
    let mut c = state.pool.acquire().await?;
    let rows = guardians::search(&mut c, user.user.school_id, needle, 20).await?;
    let mut out = Vec::with_capacity(rows.len());
    for g in rows {
        let students = guardians::students_of(&mut c, g.id).await?;
        out.push(GuardianView {
            guardian: g,
            students,
        });
    }
    Ok(Json(out))
}

#[derive(Debug, Deserialize)]
pub struct GuardianBody {
    pub full_name: Option<String>,
    #[serde(default, deserialize_with = "crate::routes::double_option")]
    pub phone: Option<Option<String>>,
    #[serde(default, deserialize_with = "crate::routes::double_option")]
    pub email: Option<Option<String>>,
    #[serde(default, deserialize_with = "crate::routes::double_option")]
    pub cnic: Option<Option<String>>,
    pub occupation: Option<String>,
    pub address: Option<String>,
}

async fn create_guardian(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<GuardianBody>,
) -> ApiResult<(StatusCode, Json<GuardianView>)> {
    user.require(Permission::StudentsManage)?;
    let Json(b) = body?;
    let full_name = b
        .full_name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .ok_or_else(|| ApiError::validation("Guardian name is required."))?;
    let mut c = state.pool.acquire().await?;
    let g = guardians::create(
        &mut c,
        &state.hlc,
        user.actor(),
        guardians::NewGuardian {
            school_id: user.user.school_id,
            full_name: full_name.into(),
            phone: clean_phone(b.phone.flatten())?,
            email: clean_opt(b.email.flatten()),
            cnic: clean_cnic(b.cnic.flatten())?,
            occupation: b.occupation.unwrap_or_default(),
            address: b.address.unwrap_or_default(),
        },
    )
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(GuardianView {
            guardian: g,
            students: Vec::new(),
        }),
    ))
}

async fn get_guardian(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<GuardianView>> {
    user.require(Permission::StudentsRead)?;
    let mut c = state.pool.acquire().await?;
    let g = guardians::get(&mut c, id).await?;
    if g.school_id != user.user.school_id {
        return Err(ApiError::not_found("guardian"));
    }
    let students = guardians::students_of(&mut c, g.id).await?;
    Ok(Json(GuardianView {
        guardian: g,
        students,
    }))
}

async fn update_guardian(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: JsonBody<GuardianBody>,
) -> ApiResult<Json<GuardianView>> {
    user.require(Permission::StudentsManage)?;
    let Json(b) = body?;
    if b.full_name.as_deref().is_some_and(|n| n.trim().is_empty()) {
        return Err(ApiError::validation("Guardian name is required."));
    }
    let mut c = state.pool.acquire().await?;
    if guardians::get(&mut c, id).await?.school_id != user.user.school_id {
        return Err(ApiError::not_found("guardian"));
    }
    let phone = match b.phone {
        Some(v) => Some(clean_phone(v)?),
        None => None,
    };
    let cnic = match b.cnic {
        Some(v) => Some(clean_cnic(v)?),
        None => None,
    };
    let g = guardians::update(
        &mut c,
        &state.hlc,
        user.actor(),
        id,
        guardians::GuardianUpdate {
            full_name: b.full_name,
            phone,
            email: b.email.map(clean_opt),
            cnic,
            occupation: b.occupation,
            address: b.address,
        },
    )
    .await?;
    let students = guardians::students_of(&mut c, g.id).await?;
    Ok(Json(GuardianView {
        guardian: g,
        students,
    }))
}
