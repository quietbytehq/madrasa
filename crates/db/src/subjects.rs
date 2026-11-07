//! Subject allocation per student (M2 stage 6): electives among a class's subjects, named
//! subject groups per class, and the subjects each enrolment takes. Every change is
//! audit-logged. The rule for a student's effective subjects is `madrasa_core::subjects`.

use std::collections::HashMap;

use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use madrasa_core::subjects::{effective_subjects, ClassSubject};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::audit::{self, Action};
use crate::{academics, map_unique, Actor, DbError, DbResult};

// ---- electives ------------------------------------------------------------------------------

/// The subjects of a class with their elective flag, in subject-name order.
pub async fn class_subjects(
    conn: &mut PgConnection,
    class_id: Uuid,
) -> DbResult<Vec<ClassSubject>> {
    let rows: Vec<(Uuid, bool)> = sqlx::query_as(
        "SELECT cs.subject_id, cs.is_elective FROM class_subjects cs
         JOIN subjects s ON s.id = cs.subject_id
         WHERE cs.class_id = $1 AND cs.deleted_at IS NULL AND s.deleted_at IS NULL ORDER BY s.name",
    )
    .bind(class_id)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(subject_id, is_elective)| ClassSubject {
            subject_id,
            is_elective,
        })
        .collect())
}

/// Elective subject ids of a class.
pub async fn class_elective_ids(conn: &mut PgConnection, class_id: Uuid) -> DbResult<Vec<Uuid>> {
    Ok(class_subjects(conn, class_id)
        .await?
        .into_iter()
        .filter(|c| c.is_elective)
        .map(|c| c.subject_id)
        .collect())
}

/// Mark exactly these class subjects as electives (the rest become compulsory); audited.
pub async fn set_class_electives(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    class_id: Uuid,
    elective_ids: &[Uuid],
) -> DbResult<Vec<Uuid>> {
    let taught = academics::class_subject_ids(&mut *conn, class_id).await?;
    if let Some(bad) = elective_ids.iter().find(|id| !taught.contains(id)) {
        return Err(DbError::Conflict(format!(
            "subject {bad} is not taught in this class"
        )));
    }
    let before = class_elective_ids(&mut *conn, class_id).await?;
    sqlx::query(
        "UPDATE class_subjects SET is_elective = (subject_id = ANY($2)), updated_at = now(), updated_by = $3, hlc = $4
         WHERE class_id = $1 AND deleted_at IS NULL AND is_elective <> (subject_id = ANY($2))",
    )
    .bind(class_id)
    .bind(elective_ids)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await?;
    let after = class_elective_ids(&mut *conn, class_id).await?;
    if before != after {
        audit::record(
            conn,
            hlc,
            actor,
            Action::Update,
            "class_electives",
            Some(class_id),
            Some(serde_json::json!({ "elective_ids": before })),
            Some(serde_json::json!({ "elective_ids": after })),
        )
        .await?;
    }
    Ok(after)
}

// ---- subject groups -------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SubjectGroup {
    pub id: Uuid,
    pub school_id: Uuid,
    pub class_id: Uuid,
    pub name: String,
    pub position: i32,
    pub hlc: String,
}

/// A group with the subjects it allocates. Mirrored by `SubjectGroup` in the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupWithSubjects {
    #[serde(flatten)]
    pub group: SubjectGroup,
    pub subject_ids: Vec<Uuid>,
}

async fn group_subject_ids(conn: &mut PgConnection, group_id: Uuid) -> DbResult<Vec<Uuid>> {
    Ok(sqlx::query_scalar(
        "SELECT gs.subject_id FROM subject_group_subjects gs JOIN subjects s ON s.id = gs.subject_id
         WHERE gs.group_id = $1 AND gs.deleted_at IS NULL AND s.deleted_at IS NULL ORDER BY s.name",
    )
    .bind(group_id)
    .fetch_all(conn)
    .await?)
}

async fn with_subjects(
    conn: &mut PgConnection,
    groups: Vec<SubjectGroup>,
) -> DbResult<Vec<GroupWithSubjects>> {
    let mut out = Vec::with_capacity(groups.len());
    for group in groups {
        let subject_ids = group_subject_ids(&mut *conn, group.id).await?;
        out.push(GroupWithSubjects { group, subject_ids });
    }
    Ok(out)
}

pub async fn list_groups(
    conn: &mut PgConnection,
    class_id: Uuid,
) -> DbResult<Vec<GroupWithSubjects>> {
    let groups = sqlx::query_as::<_, SubjectGroup>(
        "SELECT id, school_id, class_id, name, position, hlc FROM subject_groups
         WHERE class_id = $1 AND deleted_at IS NULL ORDER BY position, name",
    )
    .bind(class_id)
    .fetch_all(&mut *conn)
    .await?;
    with_subjects(conn, groups).await
}

/// Every group of the school (for allocation screens that span classes).
pub async fn list_school_groups(
    conn: &mut PgConnection,
    school_id: Uuid,
) -> DbResult<Vec<GroupWithSubjects>> {
    let groups = sqlx::query_as::<_, SubjectGroup>(
        "SELECT id, school_id, class_id, name, position, hlc FROM subject_groups
         WHERE school_id = $1 AND deleted_at IS NULL ORDER BY class_id, position, name",
    )
    .bind(school_id)
    .fetch_all(&mut *conn)
    .await?;
    with_subjects(conn, groups).await
}

pub async fn get_group(conn: &mut PgConnection, id: Uuid) -> DbResult<GroupWithSubjects> {
    let group = sqlx::query_as::<_, SubjectGroup>(
        "SELECT id, school_id, class_id, name, position, hlc FROM subject_groups
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| DbError::NotFound("subject group".into()))?;
    let subject_ids = group_subject_ids(conn, group.id).await?;
    Ok(GroupWithSubjects { group, subject_ids })
}

fn check_group_subjects(electives: &[Uuid], subject_ids: &[Uuid]) -> DbResult<()> {
    if let Some(bad) = subject_ids.iter().find(|id| !electives.contains(id)) {
        return Err(DbError::Conflict(format!(
            "subject {bad} is not an elective of this class"
        )));
    }
    Ok(())
}

async fn write_group_subjects(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    group: &SubjectGroup,
    subject_ids: &[Uuid],
) -> DbResult<()> {
    let stamp = hlc.now().to_string();
    sqlx::query(
        "UPDATE subject_group_subjects SET deleted_at = now(), updated_at = now(), updated_by = $3, hlc = $4
         WHERE group_id = $1 AND deleted_at IS NULL AND NOT (subject_id = ANY($2))",
    )
    .bind(group.id)
    .bind(subject_ids)
    .bind(actor.user_id)
    .bind(&stamp)
    .execute(&mut *conn)
    .await?;
    for subject_id in subject_ids {
        sqlx::query(
            "INSERT INTO subject_group_subjects (id, school_id, group_id, subject_id, updated_by, hlc)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (group_id, subject_id) DO UPDATE SET deleted_at = NULL, updated_at = now(),
                updated_by = EXCLUDED.updated_by, hlc = EXCLUDED.hlc",
        )
        .bind(new_id())
        .bind(group.school_id)
        .bind(group.id)
        .bind(subject_id)
        .bind(actor.user_id)
        .bind(&stamp)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

pub async fn create_group(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    class_id: Uuid,
    name: &str,
    position: i32,
    subject_ids: &[Uuid],
) -> DbResult<GroupWithSubjects> {
    let name = name.trim();
    if name.is_empty() {
        return Err(DbError::Conflict("group name is required".into()));
    }
    let class = academics::get_class(&mut *conn, class_id).await?;
    check_group_subjects(
        &class_elective_ids(&mut *conn, class_id).await?,
        subject_ids,
    )?;
    let group = sqlx::query_as::<_, SubjectGroup>(
        "INSERT INTO subject_groups (id, school_id, class_id, name, position, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id, school_id, class_id, name, position, hlc",
    )
    .bind(new_id())
    .bind(class.school_id)
    .bind(class_id)
    .bind(name)
    .bind(position)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a subject group with this name in the class"))?;
    write_group_subjects(&mut *conn, hlc, actor, &group, subject_ids).await?;
    let out = get_group(&mut *conn, group.id).await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "subject_group",
        Some(group.id),
        None,
        Some(serde_json::to_value(&out).unwrap_or_default()),
    )
    .await?;
    Ok(out)
}

pub async fn update_group(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    name: &str,
    position: i32,
    subject_ids: &[Uuid],
) -> DbResult<GroupWithSubjects> {
    let name = name.trim();
    if name.is_empty() {
        return Err(DbError::Conflict("group name is required".into()));
    }
    let old = get_group(&mut *conn, id).await?;
    check_group_subjects(
        &class_elective_ids(&mut *conn, old.group.class_id).await?,
        subject_ids,
    )?;
    let group = sqlx::query_as::<_, SubjectGroup>(
        "UPDATE subject_groups SET name = $2, position = $3, updated_at = now(), updated_by = $4, hlc = $5
         WHERE id = $1 AND deleted_at IS NULL RETURNING id, school_id, class_id, name, position, hlc",
    )
    .bind(id)
    .bind(name)
    .bind(position)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a subject group with this name in the class"))?;
    write_group_subjects(&mut *conn, hlc, actor, &group, subject_ids).await?;
    let out = get_group(&mut *conn, id).await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "subject_group",
        Some(id),
        Some(serde_json::to_value(&old).unwrap_or_default()),
        Some(serde_json::to_value(&out).unwrap_or_default()),
    )
    .await?;
    Ok(out)
}

/// Remove a group. Enrolments that chose it keep their subjects but lose the group label.
pub async fn delete_group(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
) -> DbResult<()> {
    let old = get_group(&mut *conn, id).await?;
    let stamp = hlc.now().to_string();
    sqlx::query(
        "UPDATE enrollments SET subject_group_id = NULL, updated_at = now(), updated_by = $2, hlc = $3
         WHERE subject_group_id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(actor.user_id)
    .bind(&stamp)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE subject_group_subjects SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3
         WHERE group_id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(actor.user_id)
    .bind(&stamp)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE subject_groups SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(actor.user_id)
    .bind(&stamp)
    .execute(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Delete,
        "subject_group",
        Some(id),
        Some(serde_json::to_value(&old).unwrap_or_default()),
        None,
    )
    .await?;
    Ok(())
}

// ---- allocations ----------------------------------------------------------------------------

/// What is stored for one enrolment: the chosen group (a label) and the allocated subjects.
/// Empty `subject_ids` with no group means "not allocated: takes every class subject".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllocationValue {
    pub group_id: Option<Uuid>,
    pub subject_ids: Vec<Uuid>,
}

impl AllocationValue {
    pub fn is_allocated(&self) -> bool {
        self.group_id.is_some() || !self.subject_ids.is_empty()
    }
}

/// One enrolment's allocation with its student and the effective subjects.
#[derive(Debug, Clone, Serialize)]
pub struct Allocation {
    pub enrollment_id: Uuid,
    pub student_id: Uuid,
    pub section_id: Uuid,
    pub class_id: Uuid,
    pub group_id: Option<Uuid>,
    /// Subjects stored on the enrolment (electives chosen; empty when not allocated).
    pub allocated_ids: Vec<Uuid>,
    /// The subjects the student takes after the rule is applied.
    pub subject_ids: Vec<Uuid>,
}

#[derive(Debug, sqlx::FromRow)]
struct EnrolRow {
    id: Uuid,
    student_id: Uuid,
    section_id: Uuid,
    class_id: Uuid,
    subject_group_id: Option<Uuid>,
}

async fn allocated_ids(
    conn: &mut PgConnection,
    enrollment_ids: &[Uuid],
) -> DbResult<HashMap<Uuid, Vec<Uuid>>> {
    let rows: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT ss.enrollment_id, ss.subject_id FROM student_subjects ss
         WHERE ss.enrollment_id = ANY($1) AND ss.deleted_at IS NULL",
    )
    .bind(enrollment_ids)
    .fetch_all(conn)
    .await?;
    let mut out: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for (e, s) in rows {
        out.entry(e).or_default().push(s);
    }
    Ok(out)
}

async fn allocations_of(
    conn: &mut PgConnection,
    enrols: Vec<EnrolRow>,
) -> DbResult<Vec<Allocation>> {
    let ids: Vec<Uuid> = enrols.iter().map(|e| e.id).collect();
    let stored = allocated_ids(&mut *conn, &ids).await?;
    let mut classes: HashMap<Uuid, Vec<ClassSubject>> = HashMap::new();
    let mut out = Vec::with_capacity(enrols.len());
    for e in enrols {
        if let std::collections::hash_map::Entry::Vacant(slot) = classes.entry(e.class_id) {
            slot.insert(class_subjects(&mut *conn, e.class_id).await?);
        }
        let allocated = stored.get(&e.id).cloned().unwrap_or_default();
        let subject_ids = effective_subjects(&classes[&e.class_id], &allocated);
        out.push(Allocation {
            enrollment_id: e.id,
            student_id: e.student_id,
            section_id: e.section_id,
            class_id: e.class_id,
            group_id: e.subject_group_id,
            allocated_ids: allocated,
            subject_ids,
        });
    }
    Ok(out)
}

/// Allocations of every active student of a section, in roll-number order.
pub async fn allocations_for_section(
    conn: &mut PgConnection,
    section_id: Uuid,
) -> DbResult<Vec<Allocation>> {
    let enrols = sqlx::query_as::<_, EnrolRow>(
        "SELECT e.id, e.student_id, e.section_id, sec.class_id, e.subject_group_id
         FROM enrollments e
         JOIN sections sec ON sec.id = e.section_id
         JOIN students s ON s.id = e.student_id AND s.deleted_at IS NULL AND s.status = 'active'
         WHERE e.section_id = $1 AND e.deleted_at IS NULL
         ORDER BY e.roll_no NULLS LAST, s.full_name",
    )
    .bind(section_id)
    .fetch_all(&mut *conn)
    .await?;
    allocations_of(conn, enrols).await
}

/// The allocation of one enrolment.
pub async fn allocation_for_enrollment(
    conn: &mut PgConnection,
    enrollment_id: Uuid,
) -> DbResult<Allocation> {
    let enrol = sqlx::query_as::<_, EnrolRow>(
        "SELECT e.id, e.student_id, e.section_id, sec.class_id, e.subject_group_id
         FROM enrollments e JOIN sections sec ON sec.id = e.section_id
         WHERE e.id = $1 AND e.deleted_at IS NULL",
    )
    .bind(enrollment_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| DbError::NotFound("enrolment".into()))?;
    Ok(allocations_of(conn, vec![enrol]).await?.remove(0))
}

/// Effective subjects of every active student of a section, keyed by student.
pub async fn effective_for_section(
    conn: &mut PgConnection,
    section_id: Uuid,
) -> DbResult<HashMap<Uuid, Vec<Uuid>>> {
    Ok(allocations_for_section(conn, section_id)
        .await?
        .into_iter()
        .map(|a| (a.student_id, a.subject_ids))
        .collect())
}

/// Students of a section who take a subject.
pub async fn takers(
    conn: &mut PgConnection,
    section_id: Uuid,
    subject_id: Uuid,
) -> DbResult<Vec<Uuid>> {
    Ok(allocations_for_section(conn, section_id)
        .await?
        .into_iter()
        .filter(|a| a.subject_ids.contains(&subject_id))
        .map(|a| a.student_id)
        .collect())
}

/// What `set_allocation` stored and what it left out.
#[derive(Debug, Clone)]
pub struct AllocationChange {
    pub old: AllocationValue,
    pub new: AllocationValue,
    /// Requested subjects that are compulsory in the class: they are always taken, so the
    /// request to allocate or drop them was ignored (v0.2.0-alpha B6).
    pub compulsory_ignored: Vec<Uuid>,
}

/// Set an enrolment's allocation (group label and chosen electives); compulsory subjects are
/// implied. Clearing both means the student takes every class subject again. Returns the
/// old and new stored values; audited when they differ.
pub async fn set_allocation(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    enrollment_id: Uuid,
    value: &AllocationValue,
) -> DbResult<AllocationChange> {
    let current = allocation_for_enrollment(&mut *conn, enrollment_id).await?;
    let class = class_subjects(&mut *conn, current.class_id).await?;
    let mut subject_ids: Vec<Uuid> = value
        .subject_ids
        .iter()
        .copied()
        .filter(|id| class.iter().any(|c| c.subject_id == *id && c.is_elective))
        .collect();
    if let Some(bad) = value
        .subject_ids
        .iter()
        .find(|id| !class.iter().any(|c| c.subject_id == **id))
    {
        return Err(DbError::Conflict(format!(
            "subject {bad} is not taught in the student's class"
        )));
    }
    if let Some(gid) = value.group_id {
        let group = get_group(&mut *conn, gid).await?;
        if group.group.class_id != current.class_id {
            return Err(DbError::Conflict(
                "the subject group belongs to another class".into(),
            ));
        }
    }
    subject_ids.sort();
    subject_ids.dedup();
    let compulsory_ignored: Vec<Uuid> = value
        .subject_ids
        .iter()
        .copied()
        .filter(|id| class.iter().any(|c| c.subject_id == *id && !c.is_elective))
        .collect();
    let old = AllocationValue {
        group_id: current.group_id,
        subject_ids: {
            let mut v = current.allocated_ids.clone();
            v.sort();
            v
        },
    };
    let new = AllocationValue {
        group_id: value.group_id,
        subject_ids,
    };
    if old == new {
        return Ok(AllocationChange {
            old,
            new,
            compulsory_ignored,
        });
    }
    let school_id: Uuid = sqlx::query_scalar("SELECT school_id FROM enrollments WHERE id = $1")
        .bind(enrollment_id)
        .fetch_one(&mut *conn)
        .await?;
    let stamp = hlc.now().to_string();
    sqlx::query(
        "UPDATE enrollments SET subject_group_id = $2, updated_at = now(), updated_by = $3, hlc = $4 WHERE id = $1",
    )
    .bind(enrollment_id)
    .bind(new.group_id)
    .bind(actor.user_id)
    .bind(&stamp)
    .execute(&mut *conn)
    .await?;
    sqlx::query(
        "UPDATE student_subjects SET deleted_at = now(), updated_at = now(), updated_by = $3, hlc = $4
         WHERE enrollment_id = $1 AND deleted_at IS NULL AND NOT (subject_id = ANY($2))",
    )
    .bind(enrollment_id)
    .bind(&new.subject_ids)
    .bind(actor.user_id)
    .bind(&stamp)
    .execute(&mut *conn)
    .await?;
    for subject_id in &new.subject_ids {
        sqlx::query(
            "INSERT INTO student_subjects (id, school_id, enrollment_id, subject_id, updated_by, hlc)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (enrollment_id, subject_id) DO UPDATE SET deleted_at = NULL, updated_at = now(),
                updated_by = EXCLUDED.updated_by, hlc = EXCLUDED.hlc",
        )
        .bind(new_id())
        .bind(school_id)
        .bind(enrollment_id)
        .bind(subject_id)
        .bind(actor.user_id)
        .bind(&stamp)
        .execute(&mut *conn)
        .await?;
    }
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "student_subjects",
        Some(enrollment_id),
        Some(serde_json::to_value(&old).unwrap_or_default()),
        Some(serde_json::to_value(&new).unwrap_or_default()),
    )
    .await?;
    Ok(AllocationChange {
        old,
        new,
        compulsory_ignored,
    })
}
