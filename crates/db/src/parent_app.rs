//! Parent app records (docs/adr/0006-parent-app.md): school notices and the messages between
//! a guardian and the class teacher. Used by the school API and, inside a school's schema,
//! by the hub. A conversation is the messages of one student and one guardian; there is no
//! thread record. Messages are never changed or removed.

use chrono::{DateTime, NaiveDate, Utc};
use madrasa_core::hlc::Hlc;
use madrasa_core::ids::new_id;
use madrasa_core::parent_app::{notice_is_for, notice_is_shown, preview, Side};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::audit::{self, snapshot, Action};
use crate::{Actor, DbError, DbResult};

// ---- notices --------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Notice {
    pub id: Uuid,
    pub school_id: Uuid,
    pub title: String,
    pub body: String,
    /// Classes the notice is for; empty means the whole school.
    pub class_ids: Vec<Uuid>,
    /// `None` while it is a draft or after it was withdrawn.
    pub published_at: Option<DateTime<Utc>>,
    pub expires_on: Option<NaiveDate>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

const NOTICE_COLS: &str =
    "id, school_id, title, body, class_ids, published_at, expires_on, created_by, created_at, updated_at";

/// What a notice says and who it is for. The text is expected to be cleaned already
/// (`madrasa_core::parent_app::clean_notice`).
#[derive(Debug, Clone)]
pub struct NoticeInput {
    pub title: String,
    pub body: String,
    pub class_ids: Vec<Uuid>,
    pub expires_on: Option<NaiveDate>,
}

/// Every notice of the school, drafts included, newest first.
pub async fn list_notices(conn: &mut PgConnection, school_id: Uuid) -> DbResult<Vec<Notice>> {
    Ok(sqlx::query_as::<_, Notice>(sqlx::AssertSqlSafe(format!(
        "SELECT {NOTICE_COLS} FROM notices WHERE school_id = $1 AND deleted_at IS NULL
         ORDER BY COALESCE(published_at, created_at) DESC, id DESC"
    )))
    .bind(school_id)
    .fetch_all(conn)
    .await?)
}

pub async fn get_notice(conn: &mut PgConnection, school_id: Uuid, id: Uuid) -> DbResult<Notice> {
    sqlx::query_as::<_, Notice>(sqlx::AssertSqlSafe(format!(
        "SELECT {NOTICE_COLS} FROM notices WHERE id = $1 AND school_id = $2 AND deleted_at IS NULL"
    )))
    .bind(id)
    .bind(school_id)
    .fetch_optional(conn)
    .await?
    .ok_or_else(|| DbError::NotFound("Notice".into()))
}

/// The classes named must be classes of this school.
async fn check_classes(
    conn: &mut PgConnection,
    school_id: Uuid,
    class_ids: &[Uuid],
) -> DbResult<()> {
    if class_ids.is_empty() {
        return Ok(());
    }
    let known: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM classes WHERE school_id = $1 AND id = ANY($2) AND deleted_at IS NULL",
    )
    .bind(school_id)
    .bind(class_ids)
    .fetch_one(conn)
    .await?;
    if known as usize != class_ids.len() {
        return Err(DbError::NotFound("One of the classes".into()));
    }
    Ok(())
}

fn distinct(mut ids: Vec<Uuid>) -> Vec<Uuid> {
    ids.sort_unstable();
    ids.dedup();
    ids
}

pub async fn create_notice(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    input: NoticeInput,
    publish: bool,
) -> DbResult<Notice> {
    let class_ids = distinct(input.class_ids);
    check_classes(conn, school_id, &class_ids).await?;
    let notice = sqlx::query_as::<_, Notice>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO notices (id, school_id, title, body, class_ids, published_at, expires_on, created_by, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, CASE WHEN $6 THEN now() END, $7, $8, $8, $9) RETURNING {NOTICE_COLS}"
    )))
    .bind(new_id())
    .bind(school_id)
    .bind(&input.title)
    .bind(&input.body)
    .bind(&class_ids)
    .bind(publish)
    .bind(input.expires_on)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "notice",
        Some(notice.id),
        None,
        Some(snapshot(&notice)),
    )
    .await?;
    Ok(notice)
}

pub async fn update_notice(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    id: Uuid,
    input: NoticeInput,
) -> DbResult<Notice> {
    let before = get_notice(conn, school_id, id).await?;
    let class_ids = distinct(input.class_ids);
    check_classes(conn, school_id, &class_ids).await?;
    let after = sqlx::query_as::<_, Notice>(sqlx::AssertSqlSafe(format!(
        "UPDATE notices SET title = $2, body = $3, class_ids = $4, expires_on = $5, updated_at = now(),
             updated_by = $6, hlc = $7
         WHERE id = $1 RETURNING {NOTICE_COLS}"
    )))
    .bind(id)
    .bind(&input.title)
    .bind(&input.body)
    .bind(&class_ids)
    .bind(input.expires_on)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "notice",
        Some(id),
        Some(snapshot(&before)),
        Some(snapshot(&after)),
    )
    .await?;
    Ok(after)
}

/// Publish a notice (parents see it from now) or withdraw it (they no longer do).
pub async fn set_published(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    id: Uuid,
    published: bool,
) -> DbResult<Notice> {
    let before = get_notice(conn, school_id, id).await?;
    if before.published_at.is_some() == published {
        return Ok(before);
    }
    let after = sqlx::query_as::<_, Notice>(sqlx::AssertSqlSafe(format!(
        "UPDATE notices SET published_at = CASE WHEN $2 THEN now() END, updated_at = now(), updated_by = $3, hlc = $4
         WHERE id = $1 RETURNING {NOTICE_COLS}"
    )))
    .bind(id)
    .bind(published)
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Update,
        "notice",
        Some(id),
        Some(snapshot(&before)),
        Some(snapshot(&after)),
    )
    .await?;
    Ok(after)
}

pub async fn delete_notice(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    id: Uuid,
) -> DbResult<()> {
    let before = get_notice(conn, school_id, id).await?;
    sqlx::query("UPDATE notices SET deleted_at = now(), updated_at = now(), updated_by = $2, hlc = $3 WHERE id = $1")
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
        "notice",
        Some(id),
        Some(snapshot(&before)),
        None,
    )
    .await?;
    Ok(())
}

/// The notices a parent of a child in `class_id` sees now: published, not past their last
/// day, for the whole school or for that class. Newest first.
pub async fn notices_for_class(
    conn: &mut PgConnection,
    school_id: Uuid,
    class_id: Option<Uuid>,
    now: DateTime<Utc>,
    today: NaiveDate,
) -> DbResult<Vec<Notice>> {
    let all = sqlx::query_as::<_, Notice>(sqlx::AssertSqlSafe(format!(
        "SELECT {NOTICE_COLS} FROM notices
         WHERE school_id = $1 AND deleted_at IS NULL AND published_at IS NOT NULL
         ORDER BY published_at DESC, id DESC LIMIT 500"
    )))
    .bind(school_id)
    .fetch_all(conn)
    .await?;
    Ok(all
        .into_iter()
        .filter(|n| {
            notice_is_shown(n.published_at, n.expires_on, now, today)
                && notice_is_for(&n.class_ids, class_id)
        })
        .collect())
}

// ---- who the class teacher is -------------------------------------------------------------

/// A student's section in the active academic year and its class teacher.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ClassTeacher {
    pub section_id: Uuid,
    pub class_id: Uuid,
    pub class_name: String,
    pub section_name: String,
    pub staff_id: Option<Uuid>,
    /// The teacher's login, when they have one.
    pub user_id: Option<Uuid>,
    pub teacher_name: Option<String>,
}

/// `None` when the student is in no section of the active year.
pub async fn class_teacher(
    conn: &mut PgConnection,
    student_id: Uuid,
) -> DbResult<Option<ClassTeacher>> {
    Ok(sqlx::query_as::<_, ClassTeacher>(
        "SELECT sec.id AS section_id, c.id AS class_id, c.name AS class_name, sec.name AS section_name,
                st.id AS staff_id, st.user_id, st.full_name AS teacher_name
         FROM enrollments e
         JOIN academic_years y ON y.id = e.academic_year_id AND y.is_active AND y.deleted_at IS NULL
         JOIN sections sec ON sec.id = e.section_id AND sec.deleted_at IS NULL
         JOIN classes c ON c.id = sec.class_id
         LEFT JOIN staff st ON st.id = sec.class_teacher_id AND st.deleted_at IS NULL
         WHERE e.student_id = $1 AND e.deleted_at IS NULL",
    )
    .bind(student_id)
    .fetch_optional(conn)
    .await?)
}

/// Sections of the active year whose class teacher is this member of staff.
pub async fn sections_as_class_teacher(
    conn: &mut PgConnection,
    staff_id: Uuid,
) -> DbResult<Vec<Uuid>> {
    Ok(sqlx::query_scalar(
        "SELECT sec.id FROM sections sec
         JOIN academic_years y ON y.id = sec.academic_year_id AND y.is_active AND y.deleted_at IS NULL
         WHERE sec.class_teacher_id = $1 AND sec.deleted_at IS NULL",
    )
    .bind(staff_id)
    .fetch_all(conn)
    .await?)
}

/// Is this guardian a guardian of this student, in this school?
pub async fn is_guardian_of(
    conn: &mut PgConnection,
    school_id: Uuid,
    guardian_id: Uuid,
    student_id: Uuid,
) -> DbResult<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1 FROM student_guardians sg
             JOIN guardians g ON g.id = sg.guardian_id AND g.deleted_at IS NULL
             JOIN students s ON s.id = sg.student_id AND s.deleted_at IS NULL
             WHERE sg.guardian_id = $2 AND sg.student_id = $3 AND sg.school_id = $1 AND sg.deleted_at IS NULL)",
    )
    .bind(school_id)
    .bind(guardian_id)
    .bind(student_id)
    .fetch_one(conn)
    .await?)
}

// ---- messages -------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Message {
    pub id: Uuid,
    pub student_id: Uuid,
    pub guardian_id: Uuid,
    /// `guardian` or `staff`.
    pub sender: String,
    pub sender_user_id: Option<Uuid>,
    pub sender_name: String,
    pub body: String,
    pub sent_at: DateTime<Utc>,
}

const MESSAGE_COLS: &str =
    "id, student_id, guardian_id, sender, sender_user_id, sender_name, body, sent_at";

/// A conversation, oldest message first.
pub async fn conversation(
    conn: &mut PgConnection,
    student_id: Uuid,
    guardian_id: Uuid,
) -> DbResult<Vec<Message>> {
    Ok(sqlx::query_as::<_, Message>(sqlx::AssertSqlSafe(format!(
        "SELECT {MESSAGE_COLS} FROM messages
         WHERE student_id = $1 AND guardian_id = $2 AND deleted_at IS NULL ORDER BY sent_at, id"
    )))
    .bind(student_id)
    .bind(guardian_id)
    .fetch_all(conn)
    .await?)
}

/// A message to store. The text is expected to be cleaned already
/// (`madrasa_core::parent_app::clean_message`).
#[derive(Debug, Clone)]
pub struct NewMessage {
    pub school_id: Uuid,
    pub student_id: Uuid,
    pub guardian_id: Uuid,
    pub side: Side,
    /// The staff member's login; `None` for a guardian.
    pub sender_user_id: Option<Uuid>,
    pub sender_name: String,
    pub body: String,
}

/// Store a message. Refused (`NotFound`) unless the guardian is a guardian of the student.
/// The audit log records that a message was sent and by which side, never its text.
pub async fn send_message(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    new: NewMessage,
) -> DbResult<Message> {
    if !is_guardian_of(conn, new.school_id, new.guardian_id, new.student_id).await? {
        return Err(DbError::NotFound("The conversation".into()));
    }
    let message = sqlx::query_as::<_, Message>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO messages (id, school_id, student_id, guardian_id, sender, sender_user_id, sender_name, body, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $6, $9) RETURNING {MESSAGE_COLS}"
    )))
    .bind(new_id())
    .bind(new.school_id)
    .bind(new.student_id)
    .bind(new.guardian_id)
    .bind(new.side.as_str())
    .bind(new.sender_user_id)
    .bind(&new.sender_name)
    .bind(&new.body)
    .bind(hlc.now().to_string())
    .fetch_one(&mut *conn)
    .await?;
    audit::record(
        conn,
        hlc,
        actor,
        Action::Create,
        "message",
        Some(message.id),
        None,
        Some(serde_json::json!({
            "student_id": new.student_id, "guardian_id": new.guardian_id, "sender": new.side.as_str(),
        })),
    )
    .await?;
    Ok(message)
}

/// One side has read the conversation up to now.
pub async fn mark_read(
    conn: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
    student_id: Uuid,
    guardian_id: Uuid,
    side: Side,
) -> DbResult<()> {
    let updated = sqlx::query(
        "UPDATE message_reads SET read_up_to = now(), updated_at = now(), updated_by = $4, hlc = $5
         WHERE student_id = $1 AND guardian_id = $2 AND reader = $3 AND deleted_at IS NULL",
    )
    .bind(student_id)
    .bind(guardian_id)
    .bind(side.as_str())
    .bind(actor.user_id)
    .bind(hlc.now().to_string())
    .execute(&mut *conn)
    .await?;
    if updated.rows_affected() == 0 {
        sqlx::query(
            "INSERT INTO message_reads (id, school_id, student_id, guardian_id, reader, read_up_to, updated_by, hlc)
             VALUES ($1, $2, $3, $4, $5, now(), $6, $7)",
        )
        .bind(new_id())
        .bind(school_id)
        .bind(student_id)
        .bind(guardian_id)
        .bind(side.as_str())
        .bind(actor.user_id)
        .bind(hlc.now().to_string())
        .execute(conn)
        .await?;
    }
    Ok(())
}

/// How many messages from the other side came after this side's "read up to" mark.
pub async fn unread(
    conn: &mut PgConnection,
    student_id: Uuid,
    guardian_id: Uuid,
    reader: Side,
) -> DbResult<i64> {
    let other = match reader {
        Side::Guardian => Side::Staff,
        Side::Staff => Side::Guardian,
    };
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM messages m
         WHERE m.student_id = $1 AND m.guardian_id = $2 AND m.sender = $4 AND m.deleted_at IS NULL
           AND m.sent_at > COALESCE((SELECT r.read_up_to FROM message_reads r
                                     WHERE r.student_id = $1 AND r.guardian_id = $2 AND r.reader = $3
                                       AND r.deleted_at IS NULL), '-infinity')",
    )
    .bind(student_id)
    .bind(guardian_id)
    .bind(reader.as_str())
    .bind(other.as_str())
    .fetch_one(conn)
    .await?)
}

/// A conversation in the school's list.
#[derive(Debug, Clone, Serialize)]
pub struct ConversationSummary {
    pub student_id: Uuid,
    pub student_name: String,
    pub guardian_id: Uuid,
    pub guardian_name: String,
    pub section_id: Option<Uuid>,
    pub class_name: Option<String>,
    pub section_name: Option<String>,
    pub messages: i64,
    pub last_at: DateTime<Utc>,
    /// `guardian` or `staff`.
    pub last_sender: String,
    pub last_preview: String,
    /// Messages from the guardian that staff have not read.
    pub unread: i64,
}

#[derive(sqlx::FromRow)]
struct SummaryRow {
    student_id: Uuid,
    student_name: String,
    guardian_id: Uuid,
    guardian_name: String,
    section_id: Option<Uuid>,
    class_name: Option<String>,
    section_name: Option<String>,
    messages: i64,
    last_at: DateTime<Utc>,
    last_sender: String,
    last_body: String,
    unread: i64,
}

/// Conversations that have at least one message, most recent first. `sections` limits the
/// list to students currently in those sections (a class teacher's own); `None` is every
/// conversation of the school.
pub async fn conversations(
    conn: &mut PgConnection,
    school_id: Uuid,
    sections: Option<&[Uuid]>,
) -> DbResult<Vec<ConversationSummary>> {
    let rows = sqlx::query_as::<_, SummaryRow>(
        "WITH t AS (
             SELECT m.student_id, m.guardian_id, count(*) AS messages, max(m.sent_at) AS last_at
             FROM messages m WHERE m.school_id = $1 AND m.deleted_at IS NULL
             GROUP BY m.student_id, m.guardian_id)
         SELECT t.student_id, s.full_name AS student_name, t.guardian_id, g.full_name AS guardian_name,
                cur.section_id, cur.class_name, cur.section_name, t.messages, t.last_at,
                last.sender AS last_sender, last.body AS last_body,
                (SELECT count(*) FROM messages u
                 WHERE u.student_id = t.student_id AND u.guardian_id = t.guardian_id AND u.sender = 'guardian'
                   AND u.deleted_at IS NULL
                   AND u.sent_at > COALESCE((SELECT r.read_up_to FROM message_reads r
                                             WHERE r.student_id = t.student_id AND r.guardian_id = t.guardian_id
                                               AND r.reader = 'staff' AND r.deleted_at IS NULL), '-infinity')) AS unread
         FROM t
         JOIN students s ON s.id = t.student_id
         JOIN guardians g ON g.id = t.guardian_id
         JOIN LATERAL (SELECT m.sender, m.body FROM messages m
                       WHERE m.student_id = t.student_id AND m.guardian_id = t.guardian_id AND m.deleted_at IS NULL
                       ORDER BY m.sent_at DESC, m.id DESC LIMIT 1) last ON TRUE
         LEFT JOIN LATERAL (SELECT sec.id AS section_id, c.name AS class_name, sec.name AS section_name
                            FROM enrollments e
                            JOIN academic_years y ON y.id = e.academic_year_id AND y.is_active AND y.deleted_at IS NULL
                            JOIN sections sec ON sec.id = e.section_id
                            JOIN classes c ON c.id = sec.class_id
                            WHERE e.student_id = t.student_id AND e.deleted_at IS NULL LIMIT 1) cur ON TRUE
         WHERE ($2::uuid[] IS NULL OR cur.section_id = ANY($2))
         ORDER BY t.last_at DESC, t.student_id, t.guardian_id",
    )
    .bind(school_id)
    .bind(sections)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| ConversationSummary {
            student_id: r.student_id,
            student_name: r.student_name,
            guardian_id: r.guardian_id,
            guardian_name: r.guardian_name,
            section_id: r.section_id,
            class_name: r.class_name,
            section_name: r.section_name,
            messages: r.messages,
            last_at: r.last_at,
            last_sender: r.last_sender,
            last_preview: preview(&r.last_body, 80),
            unread: r.unread,
        })
        .collect())
}
