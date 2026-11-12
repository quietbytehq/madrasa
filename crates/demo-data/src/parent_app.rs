//! Parent app demo data: school notices and one conversation between a guardian and a
//! class teacher, so the school's Parent app screens and the parent app itself have
//! something to show. School records like any other; nothing here needs a hub.

use madrasa_core::hlc::Hlc;
use madrasa_core::parent_app::Side;
use madrasa_db::parent_app::{self, NewMessage, NoticeInput};
use madrasa_db::Actor;
use serde::Serialize;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::load::LoadError;

#[derive(Debug, Clone, Default, Serialize)]
pub struct ParentAppReport {
    /// Notices, the draft included.
    pub notices: usize,
    pub messages: usize,
    /// The guardian of the demo conversation: the one to give parent app access to first.
    pub guardian_id: Option<Uuid>,
}

#[derive(sqlx::FromRow)]
struct Family {
    student_id: Uuid,
    student_name: String,
    guardian_id: Uuid,
    guardian_name: String,
    class_id: Uuid,
    teacher_user_id: Uuid,
    teacher_name: String,
}

/// The guardian with the most children at the school, and one of those children whose
/// class teacher has a login to answer with. `None` in a school too small to have one.
async fn demo_family(
    tx: &mut PgConnection,
    school_id: Uuid,
) -> Result<Option<Family>, sqlx::Error> {
    sqlx::query_as::<_, Family>(
        "SELECT s.id AS student_id, s.full_name AS student_name, g.id AS guardian_id,
                g.full_name AS guardian_name, sec.class_id, st.user_id AS teacher_user_id,
                st.full_name AS teacher_name
         FROM guardians g
         JOIN student_guardians sg ON sg.guardian_id = g.id AND sg.deleted_at IS NULL
         JOIN students s ON s.id = sg.student_id AND s.status = 'active' AND s.deleted_at IS NULL
         JOIN enrollments e ON e.student_id = s.id AND e.deleted_at IS NULL
         JOIN academic_years y ON y.id = e.academic_year_id AND y.is_active
         JOIN sections sec ON sec.id = e.section_id
         JOIN staff st ON st.id = sec.class_teacher_id AND st.user_id IS NOT NULL
         WHERE g.school_id = $1 AND g.deleted_at IS NULL
         ORDER BY (SELECT count(*) FROM student_guardians c
                   WHERE c.guardian_id = g.id AND c.deleted_at IS NULL) DESC,
                  g.full_name, g.id, s.student_number
         LIMIT 1",
    )
    .bind(school_id)
    .fetch_optional(tx)
    .await
}

/// Three notices (whole school, one class, a draft) and a short conversation.
pub async fn load(
    tx: &mut PgConnection,
    hlc: &Hlc,
    actor: Actor,
    school_id: Uuid,
) -> Result<ParentAppReport, LoadError> {
    let family = demo_family(tx, school_id)
        .await
        .map_err(madrasa_db::DbError::from)?;
    let mut report = ParentAppReport::default();

    let class_notice = family.as_ref().map(|f| f.class_id);
    let notices: [(&str, &str, Vec<Uuid>, bool); 3] = [
        (
            "Parent teacher meeting on Saturday",
            "Parents are invited to meet the class teachers this Saturday from 9:00 to 12:00.\nPlease bring your child's diary.",
            Vec::new(),
            true,
        ),
        (
            "Class trip: permission slips",
            "The class visits the science museum next Thursday. Please sign the permission slip in your child's bag and send it back by Monday.",
            class_notice.into_iter().collect(),
            true,
        ),
        (
            "Winter uniform",
            "Winter uniform is worn from the first Monday of December.",
            Vec::new(),
            false,
        ),
    ];
    for (title, body, class_ids, publish) in notices {
        parent_app::create_notice(
            tx,
            hlc,
            actor,
            school_id,
            NoticeInput {
                title: title.into(),
                body: body.into(),
                class_ids,
                expires_on: None,
            },
            publish,
        )
        .await?;
        report.notices += 1;
    }

    let Some(f) = family else {
        return Ok(report);
    };
    let first = f
        .student_name
        .split_whitespace()
        .next()
        .unwrap_or("My child");
    let lines: [(Side, String); 3] = [
        (
            Side::Guardian,
            format!("Assalam o alaikum. {first} had a fever yesterday and stayed at home. Please excuse the absence."),
        ),
        (
            Side::Staff,
            format!("Wa alaikum assalam. Thank you for telling us. I hope {first} feels better soon."),
        ),
        (
            Side::Guardian,
            "Thank you. Could you tell me which chapters the science test covers?".to_string(),
        ),
    ];
    for (side, body) in lines {
        let (sender_user_id, sender_name) = match side {
            Side::Guardian => (None, f.guardian_name.clone()),
            Side::Staff => (Some(f.teacher_user_id), f.teacher_name.clone()),
        };
        parent_app::send_message(
            tx,
            hlc,
            actor,
            NewMessage {
                school_id,
                student_id: f.student_id,
                guardian_id: f.guardian_id,
                side,
                sender_user_id,
                sender_name,
                body,
            },
        )
        .await?;
        report.messages += 1;
    }
    report.guardian_id = Some(f.guardian_id);
    Ok(report)
}
