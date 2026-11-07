//! Allocation screens (M2 stage 7): the teacher allocation grid (sections × subjects) and
//! bulk student moves between sections with a balancing proposal.

use std::collections::{BTreeMap, HashMap};

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use madrasa_core::allocation::{balance, SectionRoster};
use madrasa_core::permissions::Permission;
use madrasa_db::allocation as repo;
use madrasa_db::allocation::SectionStudent;
use madrasa_db::models::{AcademicYear, Section};
use madrasa_db::{academics, staff, students};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::{ApiError, ApiResult, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/allocation/teachers", get(teacher_grid))
        .route("/allocation/teachers/cell", put(put_cell))
        .route("/allocation/sections", get(class_sections))
        .route("/allocation/balance", post(balance_preview))
        .route("/allocation/moves", post(apply_moves))
}

async fn year_of(
    c: &mut sqlx::PgConnection,
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

// ---- teacher grid ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct GridQuery {
    pub academic_year_id: Option<Uuid>,
    pub class_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct GridSubject {
    pub id: Uuid,
    pub name: String,
    pub code: String,
}

#[derive(Debug, Serialize)]
pub struct GridCell {
    pub subject_id: Uuid,
    pub staff_id: Option<Uuid>,
    pub staff_name: Option<String>,
    /// Further teachers assigned to the same cell (shown as "+n").
    pub extra: usize,
}

#[derive(Debug, Serialize)]
pub struct GridSection {
    pub section_id: Uuid,
    pub class_id: Uuid,
    pub class_name: String,
    pub section_name: String,
    pub class_teacher_id: Option<Uuid>,
    pub class_teacher_name: Option<String>,
    /// Subjects taught in the section's class, in grid order.
    pub subject_ids: Vec<Uuid>,
    pub cells: Vec<GridCell>,
}

#[derive(Debug, Serialize)]
pub struct GridStaff {
    pub id: Uuid,
    pub full_name: String,
    pub designation: String,
    /// Section × subject cells across the whole year (not only the filtered class).
    pub cells: usize,
    pub class_teacher_of: usize,
}

/// Mirrored by `TeacherGrid` in the UI.
#[derive(Debug, Serialize)]
pub struct TeacherGrid {
    pub academic_year_id: Uuid,
    pub subjects: Vec<GridSubject>,
    pub sections: Vec<GridSection>,
    pub staff: Vec<GridStaff>,
}

async fn teacher_grid(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<GridQuery>,
) -> ApiResult<Json<TeacherGrid>> {
    user.require(Permission::TeachersRead)?;
    let mut c = state.pool.acquire().await?;
    let year = year_of(&mut c, &user, q.academic_year_id).await?;
    let all_sections =
        academics::list_sections(&mut c, user.user.school_id, Some(year.id), None).await?;
    let classes = academics::list_classes(&mut c, user.user.school_id).await?;
    let subjects_all = academics::list_subjects(&mut c, user.user.school_id).await?;
    let staff_all = staff::list(&mut c, user.user.school_id, None).await?;
    let cells = repo::teacher_cells(&mut c, year.id).await?;
    let mut class_subjects: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for s in &all_sections {
        if let std::collections::hash_map::Entry::Vacant(slot) = class_subjects.entry(s.class_id) {
            slot.insert(academics::class_subject_ids(&mut c, s.class_id).await?);
        }
    }
    let shown: Vec<&Section> = all_sections
        .iter()
        .filter(|s| q.class_id.is_none_or(|cid| s.class_id == cid))
        .collect();
    let mut subject_ids: Vec<Uuid> = shown
        .iter()
        .flat_map(|s| class_subjects[&s.class_id].iter().copied())
        .collect();
    subject_ids.sort();
    subject_ids.dedup();
    let mut subjects: Vec<GridSubject> = subjects_all
        .iter()
        .filter(|s| subject_ids.contains(&s.id))
        .map(|s| GridSubject {
            id: s.id,
            name: s.name.clone(),
            code: s.code.clone(),
        })
        .collect();
    subjects.sort_by(|a, b| a.name.cmp(&b.name));
    let mut sections = Vec::with_capacity(shown.len());
    for s in shown {
        let class = classes.iter().find(|k| k.id == s.class_id);
        let taught = &class_subjects[&s.class_id];
        let section_cells: Vec<GridCell> = subjects
            .iter()
            .filter(|sub| taught.contains(&sub.id))
            .map(|sub| {
                let mine: Vec<&repo::TeacherCell> = cells
                    .iter()
                    .filter(|c| c.section_id == s.id && c.subject_id == sub.id)
                    .collect();
                GridCell {
                    subject_id: sub.id,
                    staff_id: mine.first().map(|c| c.staff_id),
                    staff_name: mine.first().map(|c| c.staff_name.clone()),
                    extra: mine.len().saturating_sub(1),
                }
            })
            .collect();
        sections.push(GridSection {
            section_id: s.id,
            class_id: s.class_id,
            class_name: class.map(|k| k.name.clone()).unwrap_or_default(),
            section_name: s.name.clone(),
            class_teacher_id: s.class_teacher_id,
            class_teacher_name: s
                .class_teacher_id
                .and_then(|id| staff_all.iter().find(|t| t.id == id))
                .map(|t| t.full_name.clone()),
            subject_ids: subjects
                .iter()
                .filter(|sub| taught.contains(&sub.id))
                .map(|sub| sub.id)
                .collect(),
            cells: section_cells,
        });
    }
    sections.sort_by(|a, b| {
        let la = classes.iter().find(|k| k.id == a.class_id).map(|k| k.level);
        let lb = classes.iter().find(|k| k.id == b.class_id).map(|k| k.level);
        la.cmp(&lb)
            .then(a.class_name.cmp(&b.class_name))
            .then(a.section_name.cmp(&b.section_name))
    });
    let staff_rows = staff_all
        .iter()
        .filter(|t| t.status == "active")
        .map(|t| GridStaff {
            id: t.id,
            full_name: t.full_name.clone(),
            designation: t.designation.clone(),
            cells: cells.iter().filter(|c| c.staff_id == t.id).count(),
            class_teacher_of: all_sections
                .iter()
                .filter(|s| s.class_teacher_id == Some(t.id))
                .count(),
        })
        .collect();
    Ok(Json(TeacherGrid {
        academic_year_id: year.id,
        subjects,
        sections,
        staff: staff_rows,
    }))
}

#[derive(Debug, Deserialize)]
pub struct CellRequest {
    pub section_id: Uuid,
    pub subject_id: Uuid,
    #[serde(default)]
    pub staff_id: Option<Uuid>,
}

async fn put_cell(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<CellRequest>,
) -> ApiResult<StatusCode> {
    user.require(Permission::TeachersManage)?;
    let mut tx = state.pool.begin().await?;
    let section = academics::get_section(&mut tx, req.section_id).await?;
    if section.school_id != user.user.school_id {
        return Err(ApiError::not_found("section"));
    }
    if !academics::class_subject_ids(&mut tx, section.class_id)
        .await?
        .contains(&req.subject_id)
    {
        return Err(ApiError::validation(
            "This subject is not taught in the section's class.",
        ));
    }
    if let Some(sid) = req.staff_id {
        let member = staff::get(&mut tx, sid).await?;
        if member.school_id != user.user.school_id {
            return Err(ApiError::not_found("staff member"));
        }
    }
    repo::set_cell_teacher(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        req.section_id,
        req.subject_id,
        req.staff_id,
    )
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- student moves --------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ClassQuery {
    pub class_id: Uuid,
    pub academic_year_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct MoveSection {
    pub section_id: Uuid,
    pub name: String,
    pub capacity: Option<i32>,
    pub class_teacher_name: Option<String>,
    pub size: usize,
    pub male: usize,
    pub female: usize,
    pub other: usize,
    pub students: Vec<SectionStudent>,
}

/// Mirrored by `ClassSections` in the UI.
#[derive(Debug, Serialize)]
pub struct ClassSections {
    pub academic_year_id: Uuid,
    pub class_id: Uuid,
    pub class_name: String,
    pub sections: Vec<MoveSection>,
}

async fn class_view(
    c: &mut sqlx::PgConnection,
    user: &CurrentUser,
    year: &AcademicYear,
    class_id: Uuid,
) -> ApiResult<ClassSections> {
    let class = academics::get_class(c, class_id).await?;
    if class.school_id != user.user.school_id {
        return Err(ApiError::not_found("class"));
    }
    let sections =
        academics::list_sections(c, user.user.school_id, Some(year.id), Some(class_id)).await?;
    let students = repo::class_students(c, year.id, class_id).await?;
    let mut out = Vec::with_capacity(sections.len());
    for s in sections {
        let mine: Vec<SectionStudent> = students
            .iter()
            .filter(|st| st.section_id == s.id)
            .cloned()
            .collect();
        let count = |g: &str| mine.iter().filter(|st| st.gender == g).count();
        let teacher = match s.class_teacher_id {
            Some(id) => staff::get(c, id).await.ok().map(|t| t.full_name),
            None => None,
        };
        out.push(MoveSection {
            section_id: s.id,
            name: s.name.clone(),
            capacity: s.capacity,
            class_teacher_name: teacher,
            size: mine.len(),
            male: count("male"),
            female: count("female"),
            other: count("other"),
            students: mine,
        });
    }
    Ok(ClassSections {
        academic_year_id: year.id,
        class_id,
        class_name: class.name,
        sections: out,
    })
}

async fn class_sections(
    State(state): State<AppState>,
    user: CurrentUser,
    Query(q): Query<ClassQuery>,
) -> ApiResult<Json<ClassSections>> {
    user.require(Permission::StudentsManage)?;
    let mut c = state.pool.acquire().await?;
    let year = year_of(&mut c, &user, q.academic_year_id).await?;
    Ok(Json(class_view(&mut c, &user, &year, q.class_id).await?))
}

#[derive(Debug, Deserialize)]
pub struct BalanceRequest {
    pub class_id: Uuid,
    pub academic_year_id: Option<Uuid>,
    #[serde(default)]
    pub exclude_student_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct PlannedMove {
    pub student_id: Uuid,
    pub full_name: String,
    pub student_number: String,
    pub roll_no: Option<i32>,
    pub from_section_id: Uuid,
    pub from_name: String,
    pub to_section_id: Uuid,
    pub to_name: String,
}

#[derive(Debug, Serialize)]
pub struct SizeAfter {
    pub section_id: Uuid,
    pub name: String,
    pub before: usize,
    pub after: usize,
}

/// Mirrored by `BalancePreview` in the UI.
#[derive(Debug, Serialize)]
pub struct BalancePreview {
    pub moves: Vec<PlannedMove>,
    pub sizes: Vec<SizeAfter>,
}

async fn balance_preview(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<BalanceRequest>,
) -> ApiResult<Json<BalancePreview>> {
    user.require(Permission::StudentsManage)?;
    let mut c = state.pool.acquire().await?;
    let year = year_of(&mut c, &user, req.academic_year_id).await?;
    let view = class_view(&mut c, &user, &year, req.class_id).await?;
    let rosters: Vec<SectionRoster> = view
        .sections
        .iter()
        .map(|s| SectionRoster {
            section_id: s.section_id,
            students: s.students.iter().map(|st| st.student_id).collect(),
        })
        .collect();
    let moves = balance(&rosters, &req.exclude_student_ids);
    let name_of = |id: Uuid| {
        view.sections
            .iter()
            .find(|s| s.section_id == id)
            .map(|s| s.name.clone())
            .unwrap_or_default()
    };
    let planned = moves
        .iter()
        .filter_map(|m| {
            let st = view
                .sections
                .iter()
                .flat_map(|s| s.students.iter())
                .find(|st| st.student_id == m.student_id)?;
            Some(PlannedMove {
                student_id: m.student_id,
                full_name: st.full_name.clone(),
                student_number: st.student_number.clone(),
                roll_no: st.roll_no,
                from_section_id: m.from_section_id,
                from_name: name_of(m.from_section_id),
                to_section_id: m.to_section_id,
                to_name: name_of(m.to_section_id),
            })
        })
        .collect();
    let sizes = view
        .sections
        .iter()
        .map(|s| SizeAfter {
            section_id: s.section_id,
            name: s.name.clone(),
            before: s.size,
            after: s.size
                - moves
                    .iter()
                    .filter(|m| m.from_section_id == s.section_id)
                    .count()
                + moves
                    .iter()
                    .filter(|m| m.to_section_id == s.section_id)
                    .count(),
        })
        .collect();
    Ok(Json(BalancePreview {
        moves: planned,
        sizes,
    }))
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RollNumbers {
    /// The student keeps the roll number from the old section.
    Keep,
    /// The student gets the next free roll number in the new section.
    #[default]
    Reassign,
}

#[derive(Debug, Deserialize)]
pub struct MoveLine {
    pub student_id: Uuid,
    pub to_section_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct MovesRequest {
    pub class_id: Uuid,
    pub academic_year_id: Option<Uuid>,
    pub moves: Vec<MoveLine>,
    #[serde(default)]
    pub roll_numbers: RollNumbers,
}

/// Mirrored by `MovesApplied` in the UI.
#[derive(Debug, Serialize)]
pub struct MovesApplied {
    pub moved: usize,
    pub view: ClassSections,
}

async fn apply_moves(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(req): Json<MovesRequest>,
) -> ApiResult<Json<MovesApplied>> {
    user.require(Permission::StudentsManage)?;
    let mut tx = state.pool.begin().await?;
    let year = year_of(&mut tx, &user, req.academic_year_id).await?;
    let view = class_view(&mut tx, &user, &year, req.class_id).await?;
    let mut seen: BTreeMap<Uuid, Uuid> = BTreeMap::new();
    for m in &req.moves {
        if seen.insert(m.student_id, m.to_section_id).is_some() {
            return Err(ApiError::validation("A student is listed twice."));
        }
    }
    let mut moved = 0usize;
    for m in &req.moves {
        let Some(st) = view
            .sections
            .iter()
            .flat_map(|s| s.students.iter())
            .find(|st| st.student_id == m.student_id)
        else {
            return Err(ApiError::not_found("student in this class"));
        };
        if !view
            .sections
            .iter()
            .any(|s| s.section_id == m.to_section_id)
        {
            return Err(ApiError::validation(
                "Students can only be moved between sections of the same class.",
            ));
        }
        if st.section_id == m.to_section_id {
            continue;
        }
        let roll = match req.roll_numbers {
            RollNumbers::Keep => st.roll_no,
            RollNumbers::Reassign => Some(repo::next_roll_no(&mut tx, m.to_section_id).await?),
        };
        students::enrol(
            &mut tx,
            &state.hlc,
            user.actor(),
            m.student_id,
            m.to_section_id,
            roll,
        )
        .await?;
        moved += 1;
    }
    let after = class_view(&mut tx, &user, &year, req.class_id).await?;
    tx.commit().await?;
    Ok(Json(MovesApplied { moved, view: after }))
}
