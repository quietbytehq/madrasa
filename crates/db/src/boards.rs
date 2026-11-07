//! External examination boards (M2 stage 4): boards, their exams, the school's candidates and
//! the imported results. Every change is audit-logged; results carry the import batch so a
//! batch can be rolled back.

use chrono::{DateTime, Utc};
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::audit::{self, snapshot, Action};
use crate::{map_unique, Actor, DbError, DbResult};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ExamBoard {
    pub id: Uuid,
    pub school_id: Uuid,
    pub name: String,
    pub short_code: String,
    pub country: String,
    pub grading_mode: String,
    pub pass_percent: Decimal,
    pub grade_bands: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<Uuid>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub hlc: String,
}

#[derive(Debug, Clone)]
pub struct BoardInput {
    pub name: String,
    pub short_code: String,
    pub country: String,
    pub grading_mode: String,
    pub pass_percent: Decimal,
    pub grade_bands: Value,
}

pub async fn create_board(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    b: BoardInput,
) -> DbResult<ExamBoard> {
    let id = new_id();
    let row = sqlx::query_as::<_, ExamBoard>(
        "INSERT INTO exam_boards (id, school_id, name, short_code, country, grading_mode, pass_percent, grade_bands, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING *",
    )
    .bind(id)
    .bind(school_id)
    .bind(b.name.trim())
    .bind(b.short_code.trim())
    .bind(b.country.trim().to_uppercase())
    .bind(&b.grading_mode)
    .bind(b.pass_percent)
    .bind(&b.grade_bands)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a board with this name"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "exam_board",
        Some(id),
        None,
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

pub async fn update_board(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    b: BoardInput,
) -> DbResult<ExamBoard> {
    let before = get_board(&mut *conn, id).await?;
    let row = sqlx::query_as::<_, ExamBoard>(
        "UPDATE exam_boards SET name = $2, short_code = $3, country = $4, grading_mode = $5, pass_percent = $6, grade_bands = $7,
                updated_at = now(), updated_by = $8, hlc = $9
         WHERE id = $1 AND deleted_at IS NULL RETURNING *",
    )
    .bind(id)
    .bind(b.name.trim())
    .bind(b.short_code.trim())
    .bind(b.country.trim().to_uppercase())
    .bind(&b.grading_mode)
    .bind(b.pass_percent)
    .bind(&b.grade_bands)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a board with this name"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "exam_board",
        Some(id),
        Some(snapshot(&before)),
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

/// Soft-delete a board that has no exams.
pub async fn delete_board(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
) -> DbResult<()> {
    let before = get_board(&mut *conn, id).await?;
    let exams: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM board_exams WHERE board_id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    if exams > 0 {
        return Err(DbError::Conflict("Remove the board's exams first.".into()));
    }
    sqlx::query("UPDATE exam_boards SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1")
        .bind(id)
        .bind(actor.user_id)
        .bind(hlc.now().to_string())
        .execute(&mut *conn)
        .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Delete,
        "exam_board",
        Some(id),
        Some(snapshot(&before)),
        None,
    )
    .await?;
    Ok(())
}

pub async fn get_board(conn: &mut PgConnection, id: Uuid) -> DbResult<ExamBoard> {
    sqlx::query_as::<_, ExamBoard>("SELECT * FROM exam_boards WHERE id = $1 AND deleted_at IS NULL")
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or_else(|| DbError::NotFound("board".into()))
}

pub async fn list_boards(conn: &mut PgConnection, school_id: Uuid) -> DbResult<Vec<ExamBoard>> {
    Ok(sqlx::query_as::<_, ExamBoard>("SELECT * FROM exam_boards WHERE school_id = $1 AND deleted_at IS NULL ORDER BY lower(name)")
        .bind(school_id)
        .fetch_all(conn)
        .await?)
}

// ---- exams --------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BoardExam {
    pub id: Uuid,
    pub school_id: Uuid,
    pub board_id: Uuid,
    pub academic_year_id: Uuid,
    pub name: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<Uuid>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub hlc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BoardSubject {
    pub id: Uuid,
    pub board_exam_id: Uuid,
    pub name: String,
    pub subject_id: Option<Uuid>,
    pub max_marks: Decimal,
    pub position: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubjectInput {
    pub name: String,
    pub subject_id: Option<Uuid>,
    pub max_marks: Decimal,
}

pub async fn create_exam(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    board_id: Uuid,
    academic_year_id: Uuid,
    name: &str,
) -> DbResult<BoardExam> {
    let id = new_id();
    let row = sqlx::query_as::<_, BoardExam>(
        "INSERT INTO board_exams (id, school_id, board_id, academic_year_id, name, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING *",
    )
    .bind(id)
    .bind(school_id)
    .bind(board_id)
    .bind(academic_year_id)
    .bind(name.trim())
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "an exam with this name for the board and year"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "board_exam",
        Some(id),
        None,
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

pub async fn update_exam(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    name: &str,
    status: &str,
) -> DbResult<BoardExam> {
    let before = get_exam(&mut *conn, id).await?;
    let row = sqlx::query_as::<_, BoardExam>(
        "UPDATE board_exams SET name = $2, status = $3, updated_at = now(), updated_by = $4, hlc = $5
         WHERE id = $1 AND deleted_at IS NULL RETURNING *",
    )
    .bind(id)
    .bind(name.trim())
    .bind(status)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "an exam with this name for the board and year"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "board_exam",
        Some(id),
        Some(snapshot(&before)),
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

/// Soft-delete an exam and everything under it (classes, subjects, candidates, results).
pub async fn delete_exam(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
) -> DbResult<()> {
    let before = get_exam(&mut *conn, id).await?;
    let stamp = hlc.now().to_string();
    sqlx::query("UPDATE board_results SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE deleted_at IS NULL AND candidate_id IN (SELECT id FROM board_candidates WHERE board_exam_id = $1)")
        .bind(id).bind(actor.user_id).bind(&stamp).execute(&mut *conn).await?;
    for table in [
        "board_candidates",
        "board_exam_subjects",
        "board_exam_classes",
    ] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE {table} SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE board_exam_id = $1 AND deleted_at IS NULL"
        )))
        .bind(id)
        .bind(actor.user_id)
        .bind(&stamp)
        .execute(&mut *conn)
        .await?;
    }
    sqlx::query("UPDATE board_exams SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1")
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
        "board_exam",
        Some(id),
        Some(snapshot(&before)),
        None,
    )
    .await?;
    Ok(())
}

pub async fn get_exam(conn: &mut PgConnection, id: Uuid) -> DbResult<BoardExam> {
    sqlx::query_as::<_, BoardExam>("SELECT * FROM board_exams WHERE id = $1 AND deleted_at IS NULL")
        .bind(id)
        .fetch_optional(conn)
        .await?
        .ok_or_else(|| DbError::NotFound("board exam".into()))
}

pub async fn list_exams(
    conn: &mut PgConnection,
    school_id: Uuid,
    board_id: Option<Uuid>,
    academic_year_id: Option<Uuid>,
) -> DbResult<Vec<BoardExam>> {
    Ok(sqlx::query_as::<_, BoardExam>(
        "SELECT * FROM board_exams WHERE school_id = $1 AND deleted_at IS NULL
           AND ($2::uuid IS NULL OR board_id = $2) AND ($3::uuid IS NULL OR academic_year_id = $3)
         ORDER BY created_at DESC",
    )
    .bind(school_id)
    .bind(board_id)
    .bind(academic_year_id)
    .fetch_all(conn)
    .await?)
}

pub async fn exam_class_ids(conn: &mut PgConnection, exam_id: Uuid) -> DbResult<Vec<Uuid>> {
    Ok(sqlx::query_scalar(
        "SELECT class_id FROM board_exam_classes WHERE board_exam_id = $1 AND deleted_at IS NULL",
    )
    .bind(exam_id)
    .fetch_all(conn)
    .await?)
}

/// Replace the classes that sit the exam.
pub async fn set_exam_classes(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    exam: &BoardExam,
    class_ids: &[Uuid],
) -> DbResult<Vec<Uuid>> {
    let stamp = hlc.now().to_string();
    sqlx::query("UPDATE board_exam_classes SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE board_exam_id = $1 AND deleted_at IS NULL AND NOT (class_id = ANY($4))")
        .bind(exam.id).bind(actor.user_id).bind(&stamp).bind(class_ids).execute(&mut *conn).await?;
    for cid in class_ids {
        sqlx::query(
            "INSERT INTO board_exam_classes (id, school_id, board_exam_id, class_id, updated_by, hlc)
             SELECT $1, $2, $3, $4, $5, $6
             WHERE NOT EXISTS (SELECT 1 FROM board_exam_classes WHERE board_exam_id = $3 AND class_id = $4 AND deleted_at IS NULL)",
        )
        .bind(new_id())
        .bind(exam.school_id)
        .bind(exam.id)
        .bind(cid)
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
        "board_exam_classes",
        Some(exam.id),
        None,
        Some(serde_json::json!(class_ids)),
    )
    .await?;
    exam_class_ids(conn, exam.id).await
}

pub async fn exam_subjects(conn: &mut PgConnection, exam_id: Uuid) -> DbResult<Vec<BoardSubject>> {
    Ok(sqlx::query_as::<_, BoardSubject>(
        "SELECT id, board_exam_id, name, subject_id, max_marks, position FROM board_exam_subjects
         WHERE board_exam_id = $1 AND deleted_at IS NULL ORDER BY position, lower(name)",
    )
    .bind(exam_id)
    .fetch_all(conn)
    .await?)
}

/// Replace the exam's subjects: matched by name (case-insensitively), updated in place so
/// existing results keep pointing at them; subjects left out are removed with their results.
pub async fn set_exam_subjects(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    exam: &BoardExam,
    subjects: &[SubjectInput],
) -> DbResult<Vec<BoardSubject>> {
    let stamp = hlc.now().to_string();
    let existing = exam_subjects(&mut *conn, exam.id).await?;
    let keep: Vec<Uuid> = existing
        .iter()
        .filter(|e| {
            subjects
                .iter()
                .any(|s| s.name.trim().eq_ignore_ascii_case(&e.name))
        })
        .map(|e| e.id)
        .collect();
    sqlx::query("UPDATE board_results SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE deleted_at IS NULL AND board_subject_id IN (SELECT id FROM board_exam_subjects WHERE board_exam_id = $1 AND deleted_at IS NULL AND NOT (id = ANY($4)))")
        .bind(exam.id).bind(actor.user_id).bind(&stamp).bind(&keep).execute(&mut *conn).await?;
    sqlx::query("UPDATE board_exam_subjects SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE board_exam_id = $1 AND deleted_at IS NULL AND NOT (id = ANY($4))")
        .bind(exam.id).bind(actor.user_id).bind(&stamp).bind(&keep).execute(&mut *conn).await?;
    for (i, s) in subjects.iter().enumerate() {
        match existing
            .iter()
            .find(|e| s.name.trim().eq_ignore_ascii_case(&e.name))
        {
            Some(e) => {
                sqlx::query("UPDATE board_exam_subjects SET name = $2, subject_id = $3, max_marks = $4, position = $5, updated_at = now(), updated_by = $6, hlc = $7 WHERE id = $1")
                    .bind(e.id).bind(s.name.trim()).bind(s.subject_id).bind(s.max_marks).bind(i as i32).bind(actor.user_id).bind(&stamp)
                    .execute(&mut *conn).await?;
            }
            None => {
                sqlx::query("INSERT INTO board_exam_subjects (id, school_id, board_exam_id, name, subject_id, max_marks, position, updated_by, hlc) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)")
                    .bind(new_id()).bind(exam.school_id).bind(exam.id).bind(s.name.trim()).bind(s.subject_id).bind(s.max_marks).bind(i as i32).bind(actor.user_id).bind(&stamp)
                    .execute(&mut *conn).await.map_err(|e| map_unique(e, "a subject with this name in the exam"))?;
            }
        }
    }
    let after = exam_subjects(&mut *conn, exam.id).await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "board_exam_subjects",
        Some(exam.id),
        Some(snapshot(&existing)),
        Some(snapshot(&after)),
    )
    .await?;
    Ok(after)
}

// ---- candidates ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Candidate {
    pub id: Uuid,
    pub board_exam_id: Uuid,
    pub student_id: Uuid,
    pub roll_no: Option<String>,
    pub registration_no: Option<String>,
    pub group_name: Option<String>,
    pub student_number: String,
    pub full_name: String,
    pub father_name: String,
    pub class_name: Option<String>,
    pub section_name: Option<String>,
    pub section_id: Option<Uuid>,
}

/// Register students (roll and registration numbers pre-filled from the student record);
/// students already registered are left as they are. Returns how many were added.
pub async fn register(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    exam: &BoardExam,
    student_ids: &[Uuid],
    group_name: Option<&str>,
) -> DbResult<usize> {
    let stamp = hlc.now().to_string();
    let mut added = 0usize;
    for sid in student_ids {
        let n = sqlx::query(
            "INSERT INTO board_candidates (id, school_id, board_exam_id, student_id, roll_no, registration_no, group_name, updated_by, hlc)
             SELECT $1, $2, $3, s.id, s.board_roll_no, s.board_registration_no, $4, $5, $6
             FROM students s WHERE s.id = $7 AND s.school_id = $2 AND s.deleted_at IS NULL
               AND NOT EXISTS (SELECT 1 FROM board_candidates c WHERE c.board_exam_id = $3 AND c.student_id = s.id AND c.deleted_at IS NULL)",
        )
        .bind(new_id())
        .bind(exam.school_id)
        .bind(exam.id)
        .bind(group_name)
        .bind(actor.user_id)
        .bind(&stamp)
        .bind(sid)
        .execute(&mut *conn)
        .await?
        .rows_affected();
        added += n as usize;
    }
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "board_candidates",
        Some(exam.id),
        None,
        Some(serde_json::json!({ "registered": student_ids, "added": added })),
    )
    .await?;
    Ok(added)
}

pub async fn update_candidate(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    roll_no: Option<&str>,
    registration_no: Option<&str>,
    group_name: Option<&str>,
) -> DbResult<Candidate> {
    let before = get_candidate(&mut *conn, id).await?;
    sqlx::query("UPDATE board_candidates SET roll_no = $2, registration_no = $3, group_name = $4, updated_at = now(), updated_by = $5, hlc = $6 WHERE id = $1 AND deleted_at IS NULL")
        .bind(id)
        .bind(roll_no.map(str::trim).filter(|s| !s.is_empty()))
        .bind(registration_no.map(str::trim).filter(|s| !s.is_empty()))
        .bind(group_name.map(str::trim).filter(|s| !s.is_empty()))
        .bind(actor.user_id)
        .bind(hlc.now().to_string())
        .execute(&mut *conn)
        .await
        .map_err(|e| map_unique(e, "a candidate with this roll number"))?;
    // Keep the student's board numbers in step so the next exam pre-fills them.
    sqlx::query("UPDATE students SET board_roll_no = coalesce($2, board_roll_no), board_registration_no = coalesce($3, board_registration_no), updated_at = now(), updated_by = $4, hlc = $5 WHERE id = $1")
        .bind(before.student_id)
        .bind(roll_no.map(str::trim).filter(|s| !s.is_empty()))
        .bind(registration_no.map(str::trim).filter(|s| !s.is_empty()))
        .bind(actor.user_id)
        .bind(hlc.now().to_string())
        .execute(&mut *conn)
        .await?;
    let after = get_candidate(&mut *conn, id).await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "board_candidate",
        Some(id),
        Some(snapshot(&before)),
        Some(snapshot(&after)),
    )
    .await?;
    Ok(after)
}

pub async fn remove_candidate(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
) -> DbResult<()> {
    let before = get_candidate(&mut *conn, id).await?;
    let stamp = hlc.now().to_string();
    sqlx::query("UPDATE board_results SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE candidate_id = $1 AND deleted_at IS NULL")
        .bind(id).bind(actor.user_id).bind(&stamp).execute(&mut *conn).await?;
    sqlx::query("UPDATE board_candidates SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1")
        .bind(id).bind(actor.user_id).bind(&stamp).execute(&mut *conn).await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Delete,
        "board_candidate",
        Some(id),
        Some(snapshot(&before)),
        None,
    )
    .await?;
    Ok(())
}

const CANDIDATE_SELECT: &str = "SELECT c.id, c.board_exam_id, c.student_id, c.roll_no, c.registration_no, c.group_name,
            s.student_number, s.full_name, s.father_name, cl.name AS class_name, sec.name AS section_name, sec.id AS section_id
     FROM board_candidates c
     JOIN students s ON s.id = c.student_id
     JOIN board_exams x ON x.id = c.board_exam_id
     LEFT JOIN enrollments e ON e.student_id = s.id AND e.academic_year_id = x.academic_year_id AND e.deleted_at IS NULL
     LEFT JOIN sections sec ON sec.id = e.section_id
     LEFT JOIN classes cl ON cl.id = sec.class_id";

pub async fn get_candidate(conn: &mut PgConnection, id: Uuid) -> DbResult<Candidate> {
    sqlx::query_as::<_, Candidate>(sqlx::AssertSqlSafe(format!(
        "{CANDIDATE_SELECT} WHERE c.id = $1 AND c.deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("candidate".into()))
}

pub async fn candidates(conn: &mut PgConnection, exam_id: Uuid) -> DbResult<Vec<Candidate>> {
    Ok(sqlx::query_as::<_, Candidate>(sqlx::AssertSqlSafe(format!(
        "{CANDIDATE_SELECT} WHERE c.board_exam_id = $1 AND c.deleted_at IS NULL
         ORDER BY cl.level NULLS LAST, sec.name, c.roll_no NULLS LAST, lower(s.full_name)"
    )))
    .bind(exam_id)
    .fetch_all(conn)
    .await?)
}

pub async fn candidates_of_student(
    conn: &mut PgConnection,
    student_id: Uuid,
) -> DbResult<Vec<Candidate>> {
    Ok(sqlx::query_as::<_, Candidate>(sqlx::AssertSqlSafe(format!(
        "{CANDIDATE_SELECT} WHERE c.student_id = $1 AND c.deleted_at IS NULL ORDER BY x.created_at"
    )))
    .bind(student_id)
    .fetch_all(conn)
    .await?)
}

// ---- results ------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BoardResult {
    pub id: Uuid,
    pub candidate_id: Uuid,
    pub board_subject_id: Uuid,
    pub obtained: Option<Decimal>,
    pub max_marks: Decimal,
    pub grade: Option<String>,
    pub status: String,
    pub remarks: Option<String>,
    pub batch_id: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct ResultInput {
    pub candidate_id: Uuid,
    pub board_subject_id: Uuid,
    pub obtained: Option<Decimal>,
    pub max_marks: Decimal,
    pub grade: Option<String>,
    pub status: String,
    pub remarks: Option<String>,
    pub batch_id: Option<Uuid>,
}

/// What an upsert did, for the import batch log.
#[derive(Debug, Clone)]
pub enum ResultChange {
    Created(BoardResult),
    Updated {
        before: BoardResult,
        after: BoardResult,
    },
}

/// Insert or replace one subject result. Not audit-logged per row: the batch is.
pub async fn upsert_result(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    r: ResultInput,
) -> DbResult<ResultChange> {
    let existing = sqlx::query_as::<_, BoardResult>(
        "SELECT id, candidate_id, board_subject_id, obtained, max_marks, grade, status, remarks, batch_id FROM board_results
         WHERE candidate_id = $1 AND board_subject_id = $2 AND deleted_at IS NULL",
    )
    .bind(r.candidate_id)
    .bind(r.board_subject_id)
    .fetch_optional(&mut *conn)
    .await?;
    let stamp = hlc.now().to_string();
    match existing {
        Some(before) => {
            let after = sqlx::query_as::<_, BoardResult>(
                "UPDATE board_results SET obtained = $2, max_marks = $3, grade = $4, status = $5, remarks = $6, batch_id = $7,
                        updated_at = now(), updated_by = $8, hlc = $9
                 WHERE id = $1 RETURNING id, candidate_id, board_subject_id, obtained, max_marks, grade, status, remarks, batch_id",
            )
            .bind(before.id)
            .bind(r.obtained)
            .bind(r.max_marks)
            .bind(&r.grade)
            .bind(&r.status)
            .bind(&r.remarks)
            .bind(r.batch_id)
            .bind(actor.user_id)
            .bind(&stamp)
            .fetch_one(&mut *conn)
            .await?;
            Ok(ResultChange::Updated { before, after })
        }
        None => {
            let school_id: Uuid =
                sqlx::query_scalar("SELECT school_id FROM board_candidates WHERE id = $1")
                    .bind(r.candidate_id)
                    .fetch_one(&mut *conn)
                    .await?;
            let row = sqlx::query_as::<_, BoardResult>(
                "INSERT INTO board_results (id, school_id, candidate_id, board_subject_id, obtained, max_marks, grade, status, remarks, batch_id, updated_by, hlc)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                 RETURNING id, candidate_id, board_subject_id, obtained, max_marks, grade, status, remarks, batch_id",
            )
            .bind(new_id())
            .bind(school_id)
            .bind(r.candidate_id)
            .bind(r.board_subject_id)
            .bind(r.obtained)
            .bind(r.max_marks)
            .bind(&r.grade)
            .bind(&r.status)
            .bind(&r.remarks)
            .bind(r.batch_id)
            .bind(actor.user_id)
            .bind(&stamp)
            .fetch_one(&mut *conn)
            .await?;
            Ok(ResultChange::Created(row))
        }
    }
}

/// Every result of an exam's candidates.
pub async fn results_for_exam(
    conn: &mut PgConnection,
    exam_id: Uuid,
) -> DbResult<Vec<BoardResult>> {
    Ok(sqlx::query_as::<_, BoardResult>(
        "SELECT r.id, r.candidate_id, r.board_subject_id, r.obtained, r.max_marks, r.grade, r.status, r.remarks, r.batch_id
         FROM board_results r JOIN board_candidates c ON c.id = r.candidate_id
         WHERE c.board_exam_id = $1 AND r.deleted_at IS NULL AND c.deleted_at IS NULL",
    )
    .bind(exam_id)
    .fetch_all(conn)
    .await?)
}

pub async fn results_for_candidates(
    conn: &mut PgConnection,
    candidate_ids: &[Uuid],
) -> DbResult<Vec<BoardResult>> {
    Ok(sqlx::query_as::<_, BoardResult>(
        "SELECT id, candidate_id, board_subject_id, obtained, max_marks, grade, status, remarks, batch_id
         FROM board_results WHERE candidate_id = ANY($1) AND deleted_at IS NULL",
    )
    .bind(candidate_ids)
    .fetch_all(conn)
    .await?)
}
