//! Subject allocation per student (M2 stage 6): electives and subject groups per class, the
//! subjects each student takes, bulk allocation per section, and an Excel import of
//! allocations that is an import batch with rollback.

use std::collections::{BTreeMap, HashMap};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use madrasa_core::imports::ImportKind;
use madrasa_core::permissions::Permission;
use madrasa_core::subjects::split_subject_list;
use madrasa_db::imports as batches;
use madrasa_db::models::{Section, Subject};
use madrasa_db::subjects as repo;
use madrasa_db::subjects::{AllocationValue, GroupWithSubjects};
use madrasa_db::{academics, students};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::imports as engine;
use crate::{export, ApiError, ApiResult, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/classes/{id}/subject-groups",
            get(list_groups).post(create_group),
        )
        .route(
            "/subject-groups/{id}",
            put(update_group).delete(delete_group),
        )
        .route(
            "/sections/{id}/subject-allocations",
            get(section_allocations).put(put_section_allocations),
        )
        .route(
            "/students/{id}/subjects",
            get(student_subjects).put(put_student_subjects),
        )
        .route("/subject-allocations/import/template", get(import_template))
        .route("/subject-allocations/import/parse", post(parse_allocations))
        .route(
            "/subject-allocations/import/preview",
            post(preview_allocations),
        )
        .route("/subject-allocations/import/apply", post(apply_allocations))
}

// ---- groups ---------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct GroupRequest {
    pub name: String,
    #[serde(default)]
    pub position: i32,
    #[serde(default)]
    pub subject_ids: Vec<Uuid>,
}

async fn class_of(c: &mut sqlx::PgConnection, user: &CurrentUser, id: Uuid) -> ApiResult<()> {
    if academics::get_class(c, id).await?.school_id != user.user.school_id {
        return Err(ApiError::not_found("class"));
    }
    Ok(())
}

async fn list_groups(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Vec<GroupWithSubjects>>> {
    user.require(Permission::AcademicsRead)?;
    let mut c = state.pool.acquire().await?;
    class_of(&mut c, &user, id).await?;
    Ok(Json(repo::list_groups(&mut c, id).await?))
}

async fn create_group(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<GroupRequest>,
) -> ApiResult<(StatusCode, Json<GroupWithSubjects>)> {
    user.require(Permission::AcademicsManage)?;
    if req.name.trim().is_empty() {
        return Err(ApiError::validation("The group needs a name."));
    }
    let mut tx = state.pool.begin().await?;
    class_of(&mut tx, &user, id).await?;
    let group = repo::create_group(
        &mut tx,
        &state.hlc,
        user.actor(),
        id,
        &req.name,
        req.position,
        &req.subject_ids,
    )
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(group)))
}

async fn load_group(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    id: Uuid,
) -> ApiResult<GroupWithSubjects> {
    let group = repo::get_group(c, id).await?;
    if group.group.school_id != user.user.school_id {
        return Err(ApiError::not_found("subject group"));
    }
    Ok(group)
}

async fn update_group(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<GroupRequest>,
) -> ApiResult<Json<GroupWithSubjects>> {
    user.require(Permission::AcademicsManage)?;
    if req.name.trim().is_empty() {
        return Err(ApiError::validation("The group needs a name."));
    }
    let mut tx = state.pool.begin().await?;
    load_group(&mut tx, &user, id).await?;
    let group = repo::update_group(
        &mut tx,
        &state.hlc,
        user.actor(),
        id,
        &req.name,
        req.position,
        &req.subject_ids,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(group))
}

async fn delete_group(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    user.require(Permission::AcademicsManage)?;
    let mut tx = state.pool.begin().await?;
    load_group(&mut tx, &user, id).await?;
    repo::delete_group(&mut tx, &state.hlc, user.actor(), id).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- section allocations --------------------------------------------------------------------

/// Mirrored by `AllocationSubject` in the UI.
#[derive(Debug, Clone, Serialize)]
pub struct AllocationSubject {
    pub subject_id: Uuid,
    pub name: String,
    pub code: String,
    pub is_elective: bool,
}

/// Mirrored by `AllocationStudent` in the UI.
#[derive(Debug, Clone, Serialize)]
pub struct AllocationStudent {
    pub enrollment_id: Uuid,
    pub student_id: Uuid,
    pub student_number: String,
    pub full_name: String,
    pub roll_no: Option<i32>,
    pub group_id: Option<Uuid>,
    /// False when the student takes every class subject (nothing allocated yet).
    pub allocated: bool,
    /// Electives stored for the student.
    pub allocated_ids: Vec<Uuid>,
    /// The subjects the student takes.
    pub subject_ids: Vec<Uuid>,
}

/// Mirrored by `SectionAllocations` in the UI.
#[derive(Debug, Serialize)]
pub struct SectionAllocations {
    pub section_id: Uuid,
    pub class_id: Uuid,
    pub class_name: String,
    pub section_name: String,
    pub subjects: Vec<AllocationSubject>,
    pub groups: Vec<GroupWithSubjects>,
    pub students: Vec<AllocationStudent>,
}

async fn load_section(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    id: Uuid,
) -> ApiResult<Section> {
    let section = academics::get_section(c, id).await?;
    if section.school_id != user.user.school_id {
        return Err(ApiError::not_found("section"));
    }
    if let Some(allowed) = user.section_scope(c).await? {
        if !allowed.contains(&section.id) {
            return Err(ApiError::not_found("section"));
        }
    }
    Ok(section)
}

fn subject_list(
    all: &[Subject],
    class: &[madrasa_core::subjects::ClassSubject],
) -> Vec<AllocationSubject> {
    class
        .iter()
        .filter_map(|cs| {
            all.iter()
                .find(|s| s.id == cs.subject_id)
                .map(|s| AllocationSubject {
                    subject_id: s.id,
                    name: s.name.clone(),
                    code: s.code.clone(),
                    is_elective: cs.is_elective,
                })
        })
        .collect()
}

async fn section_view(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    section: &Section,
) -> ApiResult<SectionAllocations> {
    let class = academics::get_class(c, section.class_id).await?;
    let all = academics::list_subjects(c, user.user.school_id).await?;
    let class_subjects = repo::class_subjects(c, section.class_id).await?;
    let groups = repo::list_groups(c, section.class_id).await?;
    let roster = madrasa_db::exams::roster(c, section.id).await?;
    let allocations = repo::allocations_for_section(c, section.id).await?;
    let students = roster
        .into_iter()
        .filter_map(|r| {
            let a = allocations.iter().find(|a| a.student_id == r.student_id)?;
            Some(AllocationStudent {
                enrollment_id: a.enrollment_id,
                student_id: r.student_id,
                student_number: r.student_number,
                full_name: r.full_name,
                roll_no: r.roll_no,
                group_id: a.group_id,
                allocated: a.group_id.is_some() || !a.allocated_ids.is_empty(),
                allocated_ids: a.allocated_ids.clone(),
                subject_ids: a.subject_ids.clone(),
            })
        })
        .collect();
    Ok(SectionAllocations {
        section_id: section.id,
        class_id: section.class_id,
        class_name: class.name,
        section_name: section.name.clone(),
        subjects: subject_list(&all, &class_subjects),
        groups,
        students,
    })
}

async fn section_allocations(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SectionAllocations>> {
    user.require(Permission::StudentsRead)?;
    let mut c = state.pool.acquire().await?;
    let section = load_section(&mut c, &user, id).await?;
    Ok(Json(section_view(&mut c, &user, &section).await?))
}

#[derive(Debug, Deserialize)]
pub struct AllocationLine {
    pub enrollment_id: Uuid,
    #[serde(default)]
    pub group_id: Option<Uuid>,
    /// Electives chosen; with a group and no subjects, the group's subjects are used.
    #[serde(default)]
    pub subject_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct AllocationsRequest {
    pub rows: Vec<AllocationLine>,
}

fn value_of(line: &AllocationLine, groups: &[GroupWithSubjects]) -> AllocationValue {
    let subject_ids = if line.subject_ids.is_empty() {
        line.group_id
            .and_then(|g| groups.iter().find(|x| x.group.id == g))
            .map(|g| g.subject_ids.clone())
            .unwrap_or_default()
    } else {
        line.subject_ids.clone()
    };
    AllocationValue {
        group_id: line.group_id,
        subject_ids,
    }
}

async fn put_section_allocations(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<AllocationsRequest>,
) -> ApiResult<Json<SectionAllocations>> {
    user.require(Permission::StudentsManage)?;
    let mut tx = state.pool.begin().await?;
    let section = load_section(&mut tx, &user, id).await?;
    let current = repo::allocations_for_section(&mut tx, section.id).await?;
    let groups = repo::list_groups(&mut tx, section.class_id).await?;
    for line in &req.rows {
        if !current
            .iter()
            .any(|a| a.enrollment_id == line.enrollment_id)
        {
            return Err(ApiError::not_found("student in this section"));
        }
        repo::set_allocation(
            &mut tx,
            &state.hlc,
            user.actor(),
            line.enrollment_id,
            &value_of(line, &groups),
        )
        .await?;
    }
    let view = section_view(&mut tx, &user, &section).await?;
    tx.commit().await?;
    Ok(Json(view))
}

// ---- one student ----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct YearQuery {
    pub academic_year_id: Option<Uuid>,
}

/// Mirrored by `StudentSubjects` in the UI.
#[derive(Debug, Serialize)]
pub struct StudentSubjects {
    pub enrollment_id: Uuid,
    pub academic_year_id: Uuid,
    pub class_id: Uuid,
    pub class_name: String,
    pub group_id: Option<Uuid>,
    pub allocated: bool,
    pub subjects: Vec<StudentSubjectLine>,
    pub groups: Vec<GroupWithSubjects>,
}

#[derive(Debug, Serialize)]
pub struct StudentSubjectLine {
    #[serde(flatten)]
    pub subject: AllocationSubject,
    pub taken: bool,
}

async fn student_enrollment(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    student_id: Uuid,
    year_id: Option<Uuid>,
) -> ApiResult<(Uuid, Uuid)> {
    let student = students::get(c, student_id).await?;
    if student.school_id != user.user.school_id {
        return Err(ApiError::not_found("student"));
    }
    let year_id = match year_id {
        Some(y) => y,
        None => {
            academics::active_year(c, user.user.school_id)
                .await?
                .ok_or_else(|| ApiError::conflict("There is no active academic year."))?
                .id
        }
    };
    let enrol = students::enrollments(c, student_id)
        .await?
        .into_iter()
        .find(|e| e.academic_year_id == year_id)
        .ok_or_else(|| ApiError::not_found("enrolment in this year"))?;
    if let Some(allowed) = user.section_scope(c).await? {
        if !allowed.contains(&enrol.section_id) {
            return Err(ApiError::not_found("student"));
        }
    }
    Ok((enrol.id, year_id))
}

async fn student_view(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    enrollment_id: Uuid,
    year_id: Uuid,
) -> ApiResult<StudentSubjects> {
    let a = repo::allocation_for_enrollment(c, enrollment_id).await?;
    let class = academics::get_class(c, a.class_id).await?;
    let all = academics::list_subjects(c, user.user.school_id).await?;
    let class_subjects = repo::class_subjects(c, a.class_id).await?;
    let subjects = subject_list(&all, &class_subjects)
        .into_iter()
        .map(|s| StudentSubjectLine {
            taken: a.subject_ids.contains(&s.subject_id),
            subject: s,
        })
        .collect();
    Ok(StudentSubjects {
        enrollment_id,
        academic_year_id: year_id,
        class_id: a.class_id,
        class_name: class.name,
        group_id: a.group_id,
        allocated: a.group_id.is_some() || !a.allocated_ids.is_empty(),
        subjects,
        groups: repo::list_groups(c, a.class_id).await?,
    })
}

async fn student_subjects(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Query(q): Query<YearQuery>,
) -> ApiResult<Json<StudentSubjects>> {
    user.require(Permission::StudentsRead)?;
    let mut c = state.pool.acquire().await?;
    let (enrollment_id, year_id) =
        student_enrollment(&mut c, &user, id, q.academic_year_id).await?;
    Ok(Json(
        student_view(&mut c, &user, enrollment_id, year_id).await?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct StudentSubjectsRequest {
    pub academic_year_id: Option<Uuid>,
    #[serde(default)]
    pub group_id: Option<Uuid>,
    #[serde(default)]
    pub subject_ids: Vec<Uuid>,
}

async fn put_student_subjects(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<StudentSubjectsRequest>,
) -> ApiResult<Json<StudentSubjects>> {
    user.require(Permission::StudentsManage)?;
    let mut tx = state.pool.begin().await?;
    let (enrollment_id, year_id) =
        student_enrollment(&mut tx, &user, id, req.academic_year_id).await?;
    let a = repo::allocation_for_enrollment(&mut tx, enrollment_id).await?;
    let groups = repo::list_groups(&mut tx, a.class_id).await?;
    let line = AllocationLine {
        enrollment_id,
        group_id: req.group_id,
        subject_ids: req.subject_ids,
    };
    repo::set_allocation(
        &mut tx,
        &state.hlc,
        user.actor(),
        enrollment_id,
        &value_of(&line, &groups),
    )
    .await?;
    let view = student_view(&mut tx, &user, enrollment_id, year_id).await?;
    tx.commit().await?;
    Ok(Json(view))
}

// ---- import ---------------------------------------------------------------------------------

const TEMPLATE_HEADERS: [&str; 6] = [
    "Student number",
    "Name",
    "Class",
    "Section",
    "Group",
    "Subjects",
];

#[derive(Debug, Deserialize)]
pub struct TemplateQuery {
    pub section_id: Uuid,
    #[serde(default)]
    pub format: Option<String>,
}

async fn import_template(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<TemplateQuery>,
) -> ApiResult<axum::response::Response> {
    user.require(Permission::StudentsRead)?;
    let mut c = state.pool.acquire().await?;
    let section = load_section(&mut c, &user, q.section_id).await?;
    let view = section_view(&mut c, &user, &section).await?;
    let name_of = |id: &Uuid| {
        view.subjects
            .iter()
            .find(|s| s.subject_id == *id)
            .map(|s| s.code.clone())
            .unwrap_or_default()
    };
    let rows: Vec<Vec<String>> = view
        .students
        .iter()
        .map(|s| {
            vec![
                s.student_number.clone(),
                s.full_name.clone(),
                view.class_name.clone(),
                view.section_name.clone(),
                s.group_id
                    .and_then(|g| view.groups.iter().find(|x| x.group.id == g))
                    .map(|g| g.group.name.clone())
                    .unwrap_or_default(),
                s.allocated_ids
                    .iter()
                    .map(name_of)
                    .collect::<Vec<_>>()
                    .join(", "),
            ]
        })
        .collect();
    let headers: Vec<String> = TEMPLATE_HEADERS.iter().map(|h| h.to_string()).collect();
    let base = format!(
        "subject-allocation-{}-{}",
        view.class_name, view.section_name
    )
    .replace(' ', "-");
    if q.format.as_deref() == Some("csv") {
        return Ok(export::file_response(
            &format!("{base}.csv"),
            export::csv(&headers, &rows),
        ));
    }
    Ok(export::file_response(
        &format!("{base}.xlsx"),
        export::xlsx("Subjects", &headers, &rows)?,
    ))
}

#[derive(Debug, Deserialize)]
pub struct ParseRequest {
    pub file_name: String,
    pub content_base64: String,
    #[serde(default)]
    pub sheet: Option<String>,
}

/// Mirrored by `AllocationParsed` in the UI.
#[derive(Debug, Serialize)]
pub struct ParsedAllocations {
    pub file_name: String,
    pub sheets: Vec<String>,
    pub sheet: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub mapping: BTreeMap<String, usize>,
}

fn norm(h: &str) -> String {
    madrasa_core::imports::normalize_header(h)
}

fn guess_mapping(headers: &[String]) -> BTreeMap<String, usize> {
    let mut mapping = BTreeMap::new();
    for (i, h) in headers.iter().enumerate() {
        let n = norm(h);
        let key = if n.contains("student_number")
            || n.contains("admission")
            || n == "number"
            || n == "student_no"
            || n == "student_id"
        {
            "student_number"
        } else if n.contains("group") || n.contains("stream") {
            "group"
        } else if n.contains("subject") || n.contains("elective") {
            "subjects"
        } else {
            continue;
        };
        mapping.entry(key.to_string()).or_insert(i);
    }
    mapping
}

async fn parse_allocations(
    user: CurrentUser,
    Json(req): Json<ParseRequest>,
) -> ApiResult<Json<ParsedAllocations>> {
    user.require(Permission::StudentsManage)?;
    let (sheets, chosen, headers, rows) = engine::parse_file(
        ImportKind::Students,
        &req.file_name,
        &req.content_base64,
        req.sheet.as_deref(),
    )?;
    let mapping = guess_mapping(&headers);
    Ok(Json(ParsedAllocations {
        file_name: req.file_name,
        sheets,
        sheet: chosen.into_iter().next().unwrap_or_default(),
        headers,
        rows,
        mapping,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ImportRequest {
    #[serde(default)]
    pub file_name: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub mapping: BTreeMap<String, usize>,
}

/// Mirrored by `AllocationImportRow` in the UI.
#[derive(Debug, Clone, Serialize)]
pub struct PlannedRow {
    pub row_no: usize,
    pub student_number: String,
    pub student_id: Option<Uuid>,
    pub student_name: Option<String>,
    pub class_name: Option<String>,
    pub enrollment_id: Option<Uuid>,
    pub group_id: Option<Uuid>,
    pub group_name: Option<String>,
    pub subject_ids: Vec<Uuid>,
    pub subject_names: Vec<String>,
    /// False when the file holds what is already stored.
    pub changed: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct ImportSummary {
    pub rows: usize,
    pub changed: usize,
    pub unchanged: usize,
    pub errors: usize,
}

#[derive(Debug, Serialize)]
pub struct ImportPreview {
    pub rows: Vec<PlannedRow>,
    pub summary: ImportSummary,
}

#[derive(Debug, Serialize)]
pub struct ImportApplied {
    pub batch_id: Uuid,
    pub summary: ImportSummary,
}

fn cell<'a>(row: &'a [String], mapping: &BTreeMap<String, usize>, key: &str) -> Option<&'a str> {
    mapping
        .get(key)
        .and_then(|i| row.get(*i))
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
}

struct Plan {
    preview: ImportPreview,
    /// Planned value per enrolment (for rows without errors), with the old value.
    values: Vec<(Uuid, AllocationValue, AllocationValue)>,
}

async fn plan_for(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    req: &ImportRequest,
) -> ApiResult<Plan> {
    if req.rows.len() > engine::MAX_ROWS {
        return Err(ApiError::validation(format!(
            "At most {} rows per file.",
            engine::MAX_ROWS
        )));
    }
    if !req.mapping.contains_key("student_number") {
        return Err(ApiError::validation("Map the student number column."));
    }
    if !req.mapping.contains_key("subjects") && !req.mapping.contains_key("group") {
        return Err(ApiError::validation(
            "Map the subjects column or the group column.",
        ));
    }
    let year = academics::active_year(c, user.user.school_id)
        .await?
        .ok_or_else(|| ApiError::conflict("There is no active academic year."))?;
    let all = academics::list_subjects(c, user.user.school_id).await?;
    let groups = repo::list_school_groups(c, user.user.school_id).await?;
    let mut class_cache: HashMap<Uuid, (String, Vec<madrasa_core::subjects::ClassSubject>)> =
        HashMap::new();
    let mut rows = Vec::new();
    let mut values = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    for (i, raw) in req.rows.iter().enumerate() {
        if raw.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        let row_no = i + 2;
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let number = cell(raw, &req.mapping, "student_number")
            .unwrap_or("")
            .to_string();
        let mut planned = PlannedRow {
            row_no,
            student_number: number.clone(),
            student_id: None,
            student_name: None,
            class_name: None,
            enrollment_id: None,
            group_id: None,
            group_name: None,
            subject_ids: Vec::new(),
            subject_names: Vec::new(),
            changed: false,
            errors: Vec::new(),
            warnings: Vec::new(),
        };
        if number.is_empty() {
            errors.push("No student number.".into());
        } else if let Some(first) = seen.insert(number.to_lowercase(), row_no) {
            errors.push(format!("Student number repeated (also on row {first})."));
        }
        let student = if number.is_empty() {
            None
        } else {
            students::find_by_number(c, user.user.school_id, &number).await?
        };
        let enrol = match &student {
            Some(s) => {
                planned.student_id = Some(s.id);
                planned.student_name = Some(s.full_name.clone());
                let e = students::enrollments(c, s.id)
                    .await?
                    .into_iter()
                    .find(|e| e.academic_year_id == year.id);
                if e.is_none() {
                    errors.push("The student is not enrolled in the active year.".into());
                }
                e
            }
            None => {
                if !number.is_empty() {
                    errors.push(format!("No student with number '{number}'."));
                }
                None
            }
        };
        if let Some(e) = &enrol {
            let section = academics::get_section(c, e.section_id).await?;
            if let std::collections::hash_map::Entry::Vacant(slot) =
                class_cache.entry(section.class_id)
            {
                let class = academics::get_class(c, section.class_id).await?;
                let subjects = repo::class_subjects(c, section.class_id).await?;
                slot.insert((class.name, subjects));
            }
            let (class_name, class_subjects) = &class_cache[&section.class_id];
            planned.class_name = Some(class_name.clone());
            planned.enrollment_id = Some(e.id);
            let electives: Vec<&Subject> = class_subjects
                .iter()
                .filter(|cs| cs.is_elective)
                .filter_map(|cs| all.iter().find(|s| s.id == cs.subject_id))
                .collect();
            // Group: by name within the class; its subjects are the default electives.
            let mut subject_ids: Vec<Uuid> = Vec::new();
            if let Some(gname) = cell(raw, &req.mapping, "group") {
                match groups.iter().find(|g| {
                    g.group.class_id == section.class_id && g.group.name.eq_ignore_ascii_case(gname)
                }) {
                    Some(g) => {
                        planned.group_id = Some(g.group.id);
                        planned.group_name = Some(g.group.name.clone());
                        subject_ids = g.subject_ids.clone();
                    }
                    None => {
                        errors.push(format!("No subject group '{gname}' in class {class_name}."))
                    }
                }
            }
            if let Some(list) = cell(raw, &req.mapping, "subjects") {
                let mut chosen = Vec::new();
                for name in split_subject_list(list) {
                    let key = norm(&name);
                    match electives
                        .iter()
                        .find(|s| norm(&s.code) == key || norm(&s.name) == key)
                    {
                        Some(s) => chosen.push(s.id),
                        None => {
                            if class_subjects.iter().any(|cs| {
                                all.iter()
                                    .find(|s| s.id == cs.subject_id)
                                    .is_some_and(|s| norm(&s.code) == key || norm(&s.name) == key)
                            }) {
                                warnings.push(format!("{name} is compulsory in class {class_name}; it is always taken."));
                            } else {
                                errors.push(format!("No elective '{name}' in class {class_name}."));
                            }
                        }
                    }
                }
                if !chosen.is_empty() || planned.group_id.is_none() {
                    subject_ids = chosen;
                }
            }
            subject_ids.sort();
            subject_ids.dedup();
            planned.subject_names = subject_ids
                .iter()
                .filter_map(|id| all.iter().find(|s| s.id == *id).map(|s| s.name.clone()))
                .collect();
            planned.subject_ids = subject_ids.clone();
            if errors.is_empty() {
                let current = repo::allocation_for_enrollment(c, e.id).await?;
                let mut old_ids = current.allocated_ids.clone();
                old_ids.sort();
                let old = AllocationValue {
                    group_id: current.group_id,
                    subject_ids: old_ids,
                };
                let new = AllocationValue {
                    group_id: planned.group_id,
                    subject_ids,
                };
                planned.changed = old != new;
                if planned.changed {
                    values.push((e.id, old, new));
                }
            }
        }
        planned.errors = errors;
        planned.warnings = warnings;
        rows.push(planned);
    }
    let summary = ImportSummary {
        rows: rows.len(),
        changed: rows
            .iter()
            .filter(|r| r.errors.is_empty() && r.changed)
            .count(),
        unchanged: rows
            .iter()
            .filter(|r| r.errors.is_empty() && !r.changed)
            .count(),
        errors: rows.iter().filter(|r| !r.errors.is_empty()).count(),
    };
    Ok(Plan {
        preview: ImportPreview { rows, summary },
        values,
    })
}

async fn preview_allocations(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<ImportRequest>,
) -> ApiResult<Json<ImportPreview>> {
    user.require(Permission::StudentsManage)?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(plan_for(&mut c, &user, &req).await?.preview))
}

async fn apply_allocations(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<ImportRequest>,
) -> ApiResult<Json<ImportApplied>> {
    user.require(Permission::StudentsManage)?;
    let mut tx = state.pool.begin().await?;
    let plan = plan_for(&mut tx, &user, &req).await?;
    if plan.preview.summary.errors > 0 {
        return Err(ApiError::validation(format!(
            "{} rows have errors; fix them or remove them from the file.",
            plan.preview.summary.errors
        )));
    }
    if plan.values.is_empty() {
        return Err(ApiError::validation(
            "The file holds no changes to the stored allocations.",
        ));
    }
    let batch = batches::create_batch(
        &mut tx,
        &state.hlc,
        user.actor(),
        batches::NewBatch {
            school_id: user.user.school_id,
            kind: "student_subjects".into(),
            file_name: if req.file_name.is_empty() {
                "subject-allocations".into()
            } else {
                req.file_name.clone()
            },
            options: json!({}),
            total_rows: plan.preview.summary.rows as i32,
            applied_by: user.user.id,
        },
    )
    .await?;
    for (enrollment_id, old, new) in &plan.values {
        repo::set_allocation(&mut tx, &state.hlc, user.actor(), *enrollment_id, new).await?;
        let row_no = plan
            .preview
            .rows
            .iter()
            .find(|r| r.enrollment_id == Some(*enrollment_id))
            .map(|r| r.row_no)
            .unwrap_or(0);
        batches::add_row(
            &mut tx,
            &state.hlc,
            user.actor(),
            &batch,
            row_no as i32,
            "updated",
            "student_subjects",
            Some(*enrollment_id),
            Some(serde_json::to_value(old).unwrap_or(Value::Null)),
        )
        .await?;
    }
    let changed = plan.values.len() as i32;
    batches::finish_batch(&mut tx, &state.hlc, user.actor(), batch.id, 0, changed, 0).await?;
    tx.commit().await?;
    Ok(Json(ImportApplied {
        batch_id: batch.id,
        summary: plan.preview.summary,
    }))
}
