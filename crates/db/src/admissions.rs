//! Admissions pipeline (Standard stage 3): stages, applications, the dated stage history,
//! conversion into a student, and the funnel. Rules live in `madrasa_core::admissions`.

use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Utc};
use madrasa_core::admissions::{self as rules, StageKind};
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::audit::{self, snapshot, Action};
use crate::{map_unique, students, Actor, DbError, DbResult};

/// Setting: the fee head used for the admission fee voucher (empty = none).
pub const KEY_FEE_HEAD: &str = "admissions.fee_head_id";

// ---- stages ---------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Stage {
    pub id: Uuid,
    pub school_id: Uuid,
    pub code: String,
    pub name: String,
    pub kind: String,
    pub position: i32,
    pub hlc: String,
}

impl Stage {
    pub fn kind(&self) -> StageKind {
        self.kind.parse().unwrap_or(StageKind::Open)
    }
}

#[derive(Debug, Clone)]
pub struct StageInput {
    pub code: String,
    pub name: String,
    pub kind: StageKind,
    pub position: i32,
}

const STAGE_COLS: &str = "id, school_id, code, name, kind, position, hlc";

pub async fn list_stages(conn: &mut PgConnection, school_id: Uuid) -> DbResult<Vec<Stage>> {
    Ok(sqlx::query_as::<_, Stage>(sqlx::AssertSqlSafe(format!(
        "SELECT {STAGE_COLS} FROM admission_stages WHERE school_id = $1 AND deleted_at IS NULL ORDER BY position, name"
    )))
    .bind(school_id)
    .fetch_all(conn)
    .await?)
}

/// The stages, creating the default pipeline when the school has none yet.
pub async fn ensure_stages(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
) -> DbResult<Vec<Stage>> {
    let existing = list_stages(&mut *conn, school_id).await?;
    if !existing.is_empty() {
        return Ok(existing);
    }
    for (i, d) in rules::DEFAULT_STAGES.iter().enumerate() {
        create_stage(
            &mut *conn,
            hlc,
            actor,
            school_id,
            &StageInput {
                code: d.code.into(),
                name: d.name.into(),
                kind: d.kind,
                position: i as i32,
            },
        )
        .await?;
    }
    list_stages(conn, school_id).await
}

pub async fn get_stage(conn: &mut PgConnection, id: Uuid) -> DbResult<Stage> {
    sqlx::query_as::<_, Stage>(sqlx::AssertSqlSafe(format!(
        "SELECT {STAGE_COLS} FROM admission_stages WHERE id = $1 AND deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("admission stage".into()))
}

fn stage_code(name: &str, code: &str) -> String {
    let base = if code.trim().is_empty() { name } else { code };
    let mut out: String = base
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    out.trim_matches('_').to_string()
}

pub async fn create_stage(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    input: &StageInput,
) -> DbResult<Stage> {
    if input.name.trim().is_empty() {
        return Err(DbError::Conflict("the stage needs a name".into()));
    }
    let code = stage_code(&input.name, &input.code);
    if code.is_empty() {
        return Err(DbError::Conflict("the stage needs a code".into()));
    }
    let row = sqlx::query_as::<_, Stage>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO admission_stages (id, school_id, code, name, kind, position, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING {STAGE_COLS}"
    )))
    .bind(new_id())
    .bind(school_id)
    .bind(&code)
    .bind(input.name.trim())
    .bind(input.kind.as_str())
    .bind(input.position)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a stage with this code"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "admission_stage",
        Some(row.id),
        None,
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

pub async fn update_stage(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    input: &StageInput,
) -> DbResult<Stage> {
    if input.name.trim().is_empty() {
        return Err(DbError::Conflict("the stage needs a name".into()));
    }
    let old = get_stage(&mut *conn, id).await?;
    let code = stage_code(&input.name, &input.code);
    let row = sqlx::query_as::<_, Stage>(sqlx::AssertSqlSafe(format!(
        "UPDATE admission_stages SET code = $2, name = $3, kind = $4, position = $5, updated_at = now(), updated_by = $6, hlc = $7
         WHERE id = $1 AND deleted_at IS NULL RETURNING {STAGE_COLS}"
    )))
    .bind(id)
    .bind(&code)
    .bind(input.name.trim())
    .bind(input.kind.as_str())
    .bind(input.position)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| map_unique(e, "a stage with this code"))?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "admission_stage",
        Some(id),
        Some(snapshot(&old)),
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

/// Remove an empty stage (no application sits in it).
pub async fn delete_stage(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
) -> DbResult<()> {
    let old = get_stage(&mut *conn, id).await?;
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM admission_applications WHERE stage_id = $1 AND deleted_at IS NULL",
    )
    .bind(id)
    .fetch_one(&mut *conn)
    .await?;
    if n > 0 {
        return Err(DbError::Conflict(
            "move the applications out of this stage first".into(),
        ));
    }
    sqlx::query("UPDATE admission_stages SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1")
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
        "admission_stage",
        Some(id),
        Some(snapshot(&old)),
        None,
    )
    .await?;
    Ok(())
}

// ---- applications ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Application {
    pub id: Uuid,
    pub school_id: Uuid,
    pub application_no: String,
    pub academic_year_id: Uuid,
    pub class_id: Option<Uuid>,
    pub stage_id: Uuid,
    pub enquiry_date: NaiveDate,
    pub source: String,
    pub full_name: String,
    pub father_name: String,
    pub date_of_birth: Option<NaiveDate>,
    pub gender: String,
    pub cnic: Option<String>,
    pub address: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub previous_school: String,
    pub guardian_name: String,
    pub guardian_relationship: String,
    pub guardian_phone: Option<String>,
    pub guardian_cnic: Option<String>,
    pub guardian_email: Option<String>,
    pub guardian_occupation: String,
    pub test_date: Option<NaiveDate>,
    pub test_marks: Option<Decimal>,
    pub test_total: Option<Decimal>,
    pub interview_date: Option<NaiveDate>,
    pub interview_notes: String,
    pub offer_date: Option<NaiveDate>,
    pub offer_fee: Option<Decimal>,
    pub offer_expires: Option<NaiveDate>,
    pub notes: String,
    pub custom: Value,
    pub student_id: Option<Uuid>,
    pub admitted_on: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub hlc: String,
}

/// An application with the names shown in lists. Mirrored by `ApplicationView` in the UI.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ApplicationView {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub application: Application,
    pub stage_name: String,
    pub stage_kind: String,
    pub class_name: Option<String>,
    pub student_number: Option<String>,
}

/// Every editable field of an application.
#[derive(Debug, Clone, Default)]
pub struct ApplicationInput {
    pub academic_year_id: Uuid,
    pub class_id: Option<Uuid>,
    pub enquiry_date: NaiveDate,
    pub source: String,
    pub full_name: String,
    pub father_name: String,
    pub date_of_birth: Option<NaiveDate>,
    pub gender: String,
    pub cnic: Option<String>,
    pub address: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub previous_school: String,
    pub guardian_name: String,
    pub guardian_relationship: String,
    pub guardian_phone: Option<String>,
    pub guardian_cnic: Option<String>,
    pub guardian_email: Option<String>,
    pub guardian_occupation: String,
    pub test_date: Option<NaiveDate>,
    pub test_marks: Option<Decimal>,
    pub test_total: Option<Decimal>,
    pub interview_date: Option<NaiveDate>,
    pub interview_notes: String,
    pub offer_date: Option<NaiveDate>,
    pub offer_fee: Option<Decimal>,
    pub offer_expires: Option<NaiveDate>,
    pub notes: String,
    pub custom: Value,
}

fn check_application(input: &ApplicationInput) -> DbResult<()> {
    if input.full_name.trim().is_empty() {
        return Err(DbError::Conflict("the applicant needs a name".into()));
    }
    if !["", "male", "female", "other"].contains(&input.gender.as_str()) {
        return Err(DbError::Conflict("gender is male, female or other".into()));
    }
    if !["father", "mother", "guardian", "other"].contains(&input.guardian_relationship.as_str()) {
        return Err(DbError::Conflict(
            "the guardian is the father, mother, a guardian or other".into(),
        ));
    }
    if let (Some(m), Some(t)) = (input.test_marks, input.test_total) {
        if m > t {
            return Err(DbError::Conflict(
                "test marks cannot exceed the total".into(),
            ));
        }
    }
    if input.offer_fee.is_some_and(|f| f < Decimal::ZERO) {
        return Err(DbError::Conflict("the fee cannot be negative".into()));
    }
    Ok(())
}

#[derive(Debug, Clone, Default)]
pub struct ApplicationFilter {
    pub academic_year_id: Option<Uuid>,
    pub class_id: Option<Uuid>,
    pub stage_id: Option<Uuid>,
    /// Name, application number or phone.
    pub q: Option<String>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
}

const APP_COLS: &str = "a.id, a.school_id, a.application_no, a.academic_year_id, a.class_id, a.stage_id, a.enquiry_date, a.source,
            a.full_name, a.father_name, a.date_of_birth, a.gender, a.cnic, a.address, a.phone, a.email, a.previous_school,
            a.guardian_name, a.guardian_relationship, a.guardian_phone, a.guardian_cnic, a.guardian_email, a.guardian_occupation,
            a.test_date, a.test_marks, a.test_total, a.interview_date, a.interview_notes, a.offer_date, a.offer_fee, a.offer_expires,
            a.notes, a.custom, a.student_id, a.admitted_on, a.created_at, a.updated_at, a.hlc";

const VIEW_JOIN: &str = "FROM admission_applications a
         JOIN admission_stages st ON st.id = a.stage_id
         LEFT JOIN classes c ON c.id = a.class_id
         LEFT JOIN students s ON s.id = a.student_id";

pub async fn list(
    conn: &mut PgConnection,
    school_id: Uuid,
    f: &ApplicationFilter,
) -> DbResult<Vec<ApplicationView>> {
    Ok(sqlx::query_as::<_, ApplicationView>(sqlx::AssertSqlSafe(format!(
        "SELECT {APP_COLS}, st.name AS stage_name, st.kind AS stage_kind, c.name AS class_name, s.student_number
         {VIEW_JOIN}
         WHERE a.school_id = $1 AND a.deleted_at IS NULL
           AND ($2::uuid IS NULL OR a.academic_year_id = $2)
           AND ($3::uuid IS NULL OR a.class_id = $3)
           AND ($4::uuid IS NULL OR a.stage_id = $4)
           AND ($5::text IS NULL OR a.full_name ILIKE '%' || $5 || '%' OR a.application_no ILIKE '%' || $5 || '%'
                OR a.phone ILIKE '%' || $5 || '%' OR a.guardian_phone ILIKE '%' || $5 || '%' OR a.guardian_name ILIKE '%' || $5 || '%')
           AND ($6::date IS NULL OR a.enquiry_date >= $6)
           AND ($7::date IS NULL OR a.enquiry_date <= $7)
         ORDER BY st.position, a.enquiry_date DESC, a.application_no DESC"
    )))
    .bind(school_id)
    .bind(f.academic_year_id)
    .bind(f.class_id)
    .bind(f.stage_id)
    .bind(f.q.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(f.from)
    .bind(f.to)
    .fetch_all(conn)
    .await?)
}

pub async fn get(conn: &mut PgConnection, id: Uuid) -> DbResult<ApplicationView> {
    sqlx::query_as::<_, ApplicationView>(sqlx::AssertSqlSafe(format!(
        "SELECT {APP_COLS}, st.name AS stage_name, st.kind AS stage_kind, c.name AS class_name, s.student_number
         {VIEW_JOIN} WHERE a.id = $1 AND a.deleted_at IS NULL"
    )))
    .bind(id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("application".into()))
}

fn bind_input<'q>(
    q: sqlx::query::QueryAs<'q, sqlx::Postgres, Application, sqlx::postgres::PgArguments>,
    input: &'q ApplicationInput,
) -> sqlx::query::QueryAs<'q, sqlx::Postgres, Application, sqlx::postgres::PgArguments> {
    q.bind(input.academic_year_id)
        .bind(input.class_id)
        .bind(input.enquiry_date)
        .bind(input.source.trim())
        .bind(input.full_name.trim())
        .bind(input.father_name.trim())
        .bind(input.date_of_birth)
        .bind(input.gender.as_str())
        .bind(input.cnic.as_deref())
        .bind(input.address.trim())
        .bind(input.phone.as_deref())
        .bind(input.email.as_deref())
        .bind(input.previous_school.trim())
        .bind(input.guardian_name.trim())
        .bind(input.guardian_relationship.as_str())
        .bind(input.guardian_phone.as_deref())
        .bind(input.guardian_cnic.as_deref())
        .bind(input.guardian_email.as_deref())
        .bind(input.guardian_occupation.trim())
        .bind(input.test_date)
        .bind(input.test_marks)
        .bind(input.test_total)
        .bind(input.interview_date)
        .bind(input.interview_notes.trim())
        .bind(input.offer_date)
        .bind(input.offer_fee)
        .bind(input.offer_expires)
        .bind(input.notes.trim())
        .bind(&input.custom)
}

/// Create an application in a stage (the first open stage when `None`) and its first event.
pub async fn create(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    input: &ApplicationInput,
    stage_id: Option<Uuid>,
    changed_by: Option<Uuid>,
) -> DbResult<Application> {
    check_application(input)?;
    let stages = ensure_stages(&mut *conn, hlc, actor, school_id).await?;
    let stage = match stage_id {
        Some(id) => stages
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .ok_or_else(|| DbError::NotFound("admission stage".into()))?,
        None => stages
            .iter()
            .find(|s| s.kind() == StageKind::Open)
            .cloned()
            .ok_or_else(|| DbError::Conflict("the pipeline has no open stage".into()))?,
    };
    let seq = students::next_sequence(
        &mut *conn,
        hlc,
        school_id,
        &format!("application_no:{}", input.enquiry_date.format("%Y")),
    )
    .await?;
    let application_no = format!("APP-{}-{:04}", input.enquiry_date.format("%Y"), seq);
    let row = bind_input(
        sqlx::query_as::<_, Application>(sqlx::AssertSqlSafe(format!(
            "INSERT INTO admission_applications AS a (id, school_id, application_no, stage_id, updated_by, hlc,
                academic_year_id, class_id, enquiry_date, source, full_name, father_name, date_of_birth, gender, cnic, address, phone, email,
                previous_school, guardian_name, guardian_relationship, guardian_phone, guardian_cnic, guardian_email, guardian_occupation,
                test_date, test_marks, test_total, interview_date, interview_notes, offer_date, offer_fee, offer_expires, notes, custom)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25,
                     $26, $27, $28, $29, $30, $31, $32, $33, $34, $35)
             RETURNING {APP_COLS}"
        )))
        .bind(new_id())
        .bind(school_id)
        .bind(&application_no)
        .bind(stage.id)
        .bind(actor.user_id)
        .bind(hlc.now().to_string()),
        input,
    )
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        &mut *conn,
        hlc,
        actor,
        Action::Create,
        "admission_application",
        Some(row.id),
        None,
        Some(snapshot(&row)),
    )
    .await?;
    add_event(&mut *conn, hlc, actor, &row, None, stage.id, "", changed_by).await?;
    Ok(row)
}

pub async fn update(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    input: &ApplicationInput,
) -> DbResult<Application> {
    check_application(input)?;
    let old = get(&mut *conn, id).await?.application;
    if old.student_id.is_some()
        && (old.full_name != input.full_name.trim() || old.class_id != input.class_id)
    {
        return Err(DbError::Conflict(
            "a converted application keeps its applicant and class; edit the student instead"
                .into(),
        ));
    }
    let row = bind_input(
        sqlx::query_as::<_, Application>(sqlx::AssertSqlSafe(format!(
            "UPDATE admission_applications AS a SET updated_at = now(), updated_by = $2, hlc = $3,
                academic_year_id = $4, class_id = $5, enquiry_date = $6, source = $7, full_name = $8, father_name = $9, date_of_birth = $10,
                gender = $11, cnic = $12, address = $13, phone = $14, email = $15, previous_school = $16, guardian_name = $17,
                guardian_relationship = $18, guardian_phone = $19, guardian_cnic = $20, guardian_email = $21, guardian_occupation = $22,
                test_date = $23, test_marks = $24, test_total = $25, interview_date = $26, interview_notes = $27, offer_date = $28,
                offer_fee = $29, offer_expires = $30, notes = $31, custom = $32
             WHERE a.id = $1 AND a.deleted_at IS NULL RETURNING {APP_COLS}"
        )))
        .bind(id)
        .bind(actor.user_id)
        .bind(hlc.now().to_string()),
        input,
    )
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "admission_application",
        Some(id),
        Some(snapshot(&old)),
        Some(snapshot(&row)),
    )
    .await?;
    Ok(row)
}

pub async fn delete(conn: &mut PgConnection, hlc: &Hlc, actor: Actor, id: Uuid) -> DbResult<()> {
    let old = get(&mut *conn, id).await?.application;
    if old.student_id.is_some() {
        return Err(DbError::Conflict(
            "a converted application cannot be removed".into(),
        ));
    }
    sqlx::query("UPDATE admission_applications SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1")
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
        "admission_application",
        Some(id),
        Some(snapshot(&old)),
        None,
    )
    .await?;
    Ok(())
}

// ---- stage changes ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Event {
    pub id: Uuid,
    pub application_id: Uuid,
    pub from_stage_id: Option<Uuid>,
    pub to_stage_id: Uuid,
    pub to_stage_name: String,
    pub note: String,
    pub changed_by: Option<Uuid>,
    pub changed_by_name: Option<String>,
    pub changed_at: DateTime<Utc>,
}

#[allow(clippy::too_many_arguments)] // one event, every field named
async fn add_event(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    app: &Application,
    from: Option<Uuid>,
    to: Uuid,
    note: &str,
    changed_by: Option<Uuid>,
) -> DbResult<()> {
    sqlx::query(
        "INSERT INTO admission_events (id, school_id, application_id, from_stage_id, to_stage_id, note, changed_by, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(new_id())
    .bind(app.school_id)
    .bind(app.id)
    .bind(from)
    .bind(to)
    .bind(note.trim())
    .bind(changed_by)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn events(conn: &mut PgConnection, application_id: Uuid) -> DbResult<Vec<Event>> {
    Ok(sqlx::query_as::<_, Event>(
        "SELECT e.id, e.application_id, e.from_stage_id, e.to_stage_id, st.name AS to_stage_name, e.note, e.changed_by,
                u.full_name AS changed_by_name, e.changed_at
         FROM admission_events e
         JOIN admission_stages st ON st.id = e.to_stage_id
         LEFT JOIN users u ON u.id = e.changed_by
         WHERE e.application_id = $1 AND e.deleted_at IS NULL ORDER BY e.changed_at, e.created_at",
    )
    .bind(application_id)
    .fetch_all(conn)
    .await?)
}

/// Move an application to a stage, recording the event; audited.
pub async fn move_stage(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    to_stage: Uuid,
    note: &str,
    changed_by: Option<Uuid>,
) -> DbResult<Application> {
    let old = get(&mut *conn, id).await?.application;
    let from = get_stage(&mut *conn, old.stage_id).await?;
    let to = get_stage(&mut *conn, to_stage).await?;
    if to.school_id != old.school_id {
        return Err(DbError::NotFound("admission stage".into()));
    }
    rules::can_move(from.kind(), to.kind(), old.student_id.is_some()).map_err(DbError::Conflict)?;
    if from.id == to.id {
        return Ok(old);
    }
    let row = sqlx::query_as::<_, Application>(sqlx::AssertSqlSafe(format!(
        "UPDATE admission_applications AS a SET stage_id = $2,
                admitted_on = CASE WHEN $4 THEN COALESCE(admitted_on, CURRENT_DATE) ELSE admitted_on END,
                test_date = CASE WHEN $6 = 'test_scheduled' THEN COALESCE(test_date, CURRENT_DATE) ELSE test_date END,
                interview_date = CASE WHEN $6 = 'interviewed' THEN COALESCE(interview_date, CURRENT_DATE) ELSE interview_date END,
                offer_date = CASE WHEN $6 = 'offered' THEN COALESCE(offer_date, CURRENT_DATE) ELSE offer_date END,
                updated_at = now(), updated_by = $3, hlc = $5
         WHERE a.id = $1 RETURNING {APP_COLS}"
    )))
    .bind(id)
    .bind(to.id)
    .bind(actor.user_id)
    .bind(to.kind() == StageKind::Admitted)
    .bind(hlc.now().to_string())
    .bind(&to.code)
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        &mut *conn,
        hlc,
        actor,
        Action::Update,
        "admission_application",
        Some(id),
        Some(json!({ "stage_id": from.id, "stage": from.name })),
        Some(json!({ "stage_id": to.id, "stage": to.name, "note": note })),
    )
    .await?;
    add_event(
        &mut *conn,
        hlc,
        actor,
        &row,
        Some(from.id),
        to.id,
        note,
        changed_by,
    )
    .await?;
    Ok(row)
}

/// Link the created student to the application (the conversion itself is done by the API,
/// which creates the student, guardian and enrolment through the students module).
pub async fn link_student(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    id: Uuid,
    student_id: Uuid,
    admitted_on: NaiveDate,
) -> DbResult<Application> {
    let old = get(&mut *conn, id).await?.application;
    if old.student_id.is_some() {
        return Err(DbError::Conflict(
            "this application was already converted".into(),
        ));
    }
    let row = sqlx::query_as::<_, Application>(sqlx::AssertSqlSafe(format!(
        "UPDATE admission_applications AS a SET student_id = $2, admitted_on = $3, updated_at = now(), updated_by = $4, hlc = $5
         WHERE a.id = $1 RETURNING {APP_COLS}"
    )))
    .bind(id)
    .bind(student_id)
    .bind(admitted_on)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "admission_application",
        Some(id),
        Some(json!({ "student_id": Value::Null })),
        Some(json!({ "student_id": student_id, "admitted_on": admitted_on })),
    )
    .await?;
    Ok(row)
}

// ---- funnel -----------------------------------------------------------------------------------

/// Per stage (pipeline order): applications that reached it and that sit in it now.
pub async fn funnel(
    conn: &mut PgConnection,
    school_id: Uuid,
    year_id: Uuid,
) -> DbResult<Vec<(Stage, usize, usize)>> {
    let stages = list_stages(&mut *conn, school_id).await?;
    let rows: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT e.application_id, e.to_stage_id FROM admission_events e
         JOIN admission_applications a ON a.id = e.application_id AND a.deleted_at IS NULL
         WHERE a.academic_year_id = $1 AND e.deleted_at IS NULL ORDER BY e.application_id, e.changed_at, e.created_at",
    )
    .bind(year_id)
    .fetch_all(conn)
    .await?;
    let mut visits: HashMap<Uuid, Vec<String>> = HashMap::new();
    for (app, stage) in rows {
        visits.entry(app).or_default().push(stage.to_string());
    }
    let visits: Vec<Vec<String>> = visits.into_values().collect();
    let ids: Vec<String> = stages.iter().map(|s| s.id.to_string()).collect();
    let f = rules::funnel(&ids, &visits);
    Ok(stages
        .into_iter()
        .zip(f)
        .map(|(s, row)| (s, row.reached, row.current))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_codes_are_slugs() {
        assert_eq!(stage_code("Test scheduled", ""), "test_scheduled");
        assert_eq!(stage_code("Offered", "OFFER-1"), "offer_1");
        assert_eq!(stage_code("  Interview / call ", ""), "interview_call");
    }

    #[test]
    fn applications_are_checked() {
        let mut a = ApplicationInput {
            full_name: "Ali".into(),
            guardian_relationship: "father".into(),
            ..Default::default()
        };
        assert!(check_application(&a).is_ok());
        a.test_marks = Some(Decimal::from(80));
        a.test_total = Some(Decimal::from(50));
        assert!(check_application(&a).is_err());
        a.test_total = Some(Decimal::from(100));
        a.gender = "boy".into();
        assert!(check_application(&a).is_err());
    }
}
