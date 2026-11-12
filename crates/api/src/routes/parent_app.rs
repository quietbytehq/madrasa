//! Parent app, the school's side (docs/adr/0006-parent-app.md): school notices and the
//! conversations between guardians and class teachers. Only for a school that has joined a
//! hub (the parent app reads and writes through the hub); otherwise every route here
//! answers `409` with a plain message.
//!
//! - Notices: `notices.manage`.
//! - Conversations: `messages.view_all` reads every conversation; `messages.reply` reads
//!   and answers the conversations of the sections the caller is class teacher of. Nobody
//!   answers for another teacher's class, whatever their permissions.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::NaiveDate;
use madrasa_core::edition::Feature;
use madrasa_core::parent_app::{clean_message, clean_notice, Side};
use madrasa_core::permissions::Permission;
use madrasa_db::parent_app::{self, ConversationSummary, Message, NewMessage, Notice, NoticeInput};
use madrasa_db::{nexus_link, staff};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::{ApiError, ApiResult, AppState};

type JsonBody<T> = Result<Json<T>, axum::extract::rejection::JsonRejection>;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/parent-app/status", get(status))
        .route("/parent-app/notices", get(list_notices).post(create_notice))
        .route(
            "/parent-app/notices/{id}",
            axum::routing::patch(update_notice).delete(delete_notice),
        )
        .route("/parent-app/notices/{id}/publish", post(publish_notice))
        .route("/parent-app/notices/{id}/withdraw", post(withdraw_notice))
        .route("/parent-app/access", get(access_list))
        .route("/parent-app/access/{guardian_id}/code", post(access_code))
        .route(
            "/parent-app/access/{guardian_id}",
            axum::routing::delete(access_end),
        )
        .route("/parent-app/conversations", get(list_conversations))
        .route(
            "/parent-app/conversations/{student_id}/{guardian_id}",
            get(read_conversation),
        )
        .route(
            "/parent-app/conversations/{student_id}/{guardian_id}/messages",
            post(send),
        )
        .route(
            "/parent-app/conversations/{student_id}/{guardian_id}/read",
            post(mark_read),
        )
}

/// Mirrored by `ParentAppStatus` in the UI.
#[derive(Debug, Serialize)]
pub struct Status {
    /// The school is joined to a hub, so the parent app can work.
    pub joined: bool,
    pub hub_name: Option<String>,
    /// Messages from guardians the caller has not read, in the sections the caller is
    /// class teacher of. `0` for anyone else.
    pub unread: i64,
}

/// What the app's sidebar needs to know: is there a Parent app area to show (the school is
/// joined to a hub), and how many messages wait for the caller. For anyone who holds one
/// of the parent app permissions; never an error for a school that has not joined.
async fn status(State(state): State<AppState>, user: CurrentUser) -> ApiResult<Json<Status>> {
    let may = [
        Permission::ParentsManage,
        Permission::NoticesManage,
        Permission::MessagesReply,
        Permission::MessagesViewAll,
    ]
    .into_iter()
    .any(|p| user.can(p));
    if !may {
        return Err(ApiError::forbidden());
    }
    let nothing = Status {
        joined: false,
        hub_name: None,
        unread: 0,
    };
    if !state.edition().enables(Feature::NexusLink) {
        return Ok(Json(nothing));
    }
    let mut c = state.pool.acquire().await?;
    let Some(link) = nexus_link::get(&mut c, user.user.school_id)
        .await?
        .filter(|link| link.is_active())
    else {
        return Ok(Json(nothing));
    };
    let mut unread = 0;
    if user.can(Permission::MessagesReply) {
        if let Some(me) = staff::find_by_user(&mut c, user.user.id).await? {
            let sections = parent_app::sections_as_class_teacher(&mut c, me.id).await?;
            if !sections.is_empty() {
                unread = parent_app::conversations(&mut c, user.user.school_id, Some(&sections))
                    .await?
                    .iter()
                    .map(|r| r.unread)
                    .sum();
            }
        }
    }
    Ok(Json(Status {
        joined: true,
        hub_name: Some(link.hub_name),
        unread,
    }))
}

/// The parent app works through the hub: without a hub there is nobody to read a notice
/// or to write a message.
pub(crate) async fn require_joined(state: &AppState, user: &CurrentUser) -> ApiResult<()> {
    state.require_feature(Feature::NexusLink)?;
    let mut conn = state.pool.acquire().await?;
    let joined = nexus_link::get(&mut conn, user.user.school_id)
        .await?
        .is_some_and(|link| link.is_active());
    if !joined {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "not_joined",
            "The parent app works through a school network. Join one first (Settings → School network).",
        ));
    }
    Ok(())
}

// ---- access to the parent app ---------------------------------------------------------------

/// A guardian's access, with the names the school knows them by.
#[derive(Debug, Serialize)]
pub struct AccessRow {
    #[serde(flatten)]
    pub access: madrasa_core::parent_app::GuardianAccess,
    pub guardian_name: String,
    pub children: Vec<String>,
}

async fn access_gate(state: &AppState, user: &CurrentUser) -> ApiResult<()> {
    user.require(Permission::ParentsManage)?;
    require_joined(state, user).await
}

/// Guardians who have access to the parent app, or a code waiting to be used.
async fn access_list(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<AccessRow>>> {
    access_gate(&state, &user).await?;
    let hub = crate::nexus_link::parent_access(&state).await?;
    let ids: Vec<Uuid> = hub.iter().map(|a| a.guardian_id).collect();
    let mut c = state.pool.acquire().await?;
    let names: Vec<(Uuid, String, Vec<String>)> = sqlx::query_as(
        "SELECT g.id, g.full_name,
                COALESCE(array_agg(s.full_name ORDER BY s.full_name) FILTER (WHERE s.id IS NOT NULL), '{}')
         FROM guardians g
         LEFT JOIN student_guardians sg ON sg.guardian_id = g.id AND sg.deleted_at IS NULL
         LEFT JOIN students s ON s.id = sg.student_id AND s.deleted_at IS NULL
         WHERE g.school_id = $1 AND g.id = ANY($2)
         GROUP BY g.id, g.full_name",
    )
    .bind(user.user.school_id)
    .bind(&ids)
    .fetch_all(&mut *c)
    .await?;
    let mut rows: Vec<AccessRow> = hub
        .into_iter()
        .map(|access| {
            let known = names.iter().find(|(id, _, _)| *id == access.guardian_id);
            AccessRow {
                guardian_name: known.map(|(_, n, _)| n.clone()).unwrap_or_default(),
                children: known.map(|(_, _, c)| c.clone()).unwrap_or_default(),
                access,
            }
        })
        .collect();
    rows.sort_by(|a, b| a.guardian_name.cmp(&b.guardian_name));
    Ok(Json(rows))
}

/// A one-time access code for a guardian, to hand over on a slip. Shown once.
async fn access_code(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(guardian_id): Path<Uuid>,
) -> ApiResult<Json<crate::nexus_link::ParentCode>> {
    access_gate(&state, &user).await?;
    let mut c = state.pool.acquire().await?;
    let has_child: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM student_guardians sg
                        JOIN guardians g ON g.id = sg.guardian_id AND g.deleted_at IS NULL
                        JOIN students s ON s.id = sg.student_id AND s.deleted_at IS NULL
                        WHERE sg.guardian_id = $1 AND sg.school_id = $2 AND sg.deleted_at IS NULL)",
    )
    .bind(guardian_id)
    .bind(user.user.school_id)
    .fetch_one(&mut *c)
    .await?;
    drop(c);
    if !has_child {
        return Err(ApiError::not_found("A guardian with a student"));
    }
    Ok(Json(
        crate::nexus_link::parent_access_code(&state, user.actor(), guardian_id).await?,
    ))
}

#[derive(Debug, Serialize)]
pub struct AccessEnded {
    pub ended: bool,
}

async fn access_end(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(guardian_id): Path<Uuid>,
) -> ApiResult<Json<AccessEnded>> {
    access_gate(&state, &user).await?;
    let ended = crate::nexus_link::parent_access_end(&state, user.actor(), guardian_id).await?;
    Ok(Json(AccessEnded { ended }))
}

// ---- notices --------------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct NoticeBody {
    pub title: String,
    #[serde(default)]
    pub body: String,
    /// Classes the notice is for; empty or absent means the whole school.
    #[serde(default)]
    pub class_ids: Vec<Uuid>,
    #[serde(default)]
    pub expires_on: Option<NaiveDate>,
    /// On create: publish at once instead of saving a draft.
    #[serde(default)]
    pub publish: bool,
}

fn notice_input(b: &NoticeBody) -> ApiResult<NoticeInput> {
    let (title, body) = clean_notice(&b.title, &b.body).map_err(ApiError::validation)?;
    Ok(NoticeInput {
        title,
        body,
        class_ids: b.class_ids.clone(),
        expires_on: b.expires_on,
    })
}

async fn notices_gate(state: &AppState, user: &CurrentUser) -> ApiResult<()> {
    user.require(Permission::NoticesManage)?;
    require_joined(state, user).await
}

async fn list_notices(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<Notice>>> {
    notices_gate(&state, &user).await?;
    let mut c = state.pool.acquire().await?;
    Ok(Json(
        parent_app::list_notices(&mut c, user.user.school_id).await?,
    ))
}

async fn create_notice(
    State(state): State<AppState>,
    user: CurrentUser,
    body: JsonBody<NoticeBody>,
) -> ApiResult<(StatusCode, Json<Notice>)> {
    notices_gate(&state, &user).await?;
    let Json(b) = body?;
    let input = notice_input(&b)?;
    let mut tx = state.pool.begin().await?;
    let notice = parent_app::create_notice(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        input,
        b.publish,
    )
    .await?;
    tx.commit().await?;
    state.wake_nexus_sync();
    Ok((StatusCode::CREATED, Json(notice)))
}

async fn update_notice(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    body: JsonBody<NoticeBody>,
) -> ApiResult<Json<Notice>> {
    notices_gate(&state, &user).await?;
    let Json(b) = body?;
    let input = notice_input(&b)?;
    let mut tx = state.pool.begin().await?;
    let notice = parent_app::update_notice(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        id,
        input,
    )
    .await?;
    tx.commit().await?;
    state.wake_nexus_sync();
    Ok(Json(notice))
}

async fn set_published(
    state: &AppState,
    user: &CurrentUser,
    id: Uuid,
    published: bool,
) -> ApiResult<Json<Notice>> {
    notices_gate(state, user).await?;
    let mut tx = state.pool.begin().await?;
    let notice = parent_app::set_published(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        id,
        published,
    )
    .await?;
    tx.commit().await?;
    state.wake_nexus_sync();
    Ok(Json(notice))
}

async fn publish_notice(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Notice>> {
    set_published(&state, &user, id, true).await
}

async fn withdraw_notice(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Notice>> {
    set_published(&state, &user, id, false).await
}

async fn delete_notice(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    notices_gate(&state, &user).await?;
    let mut tx = state.pool.begin().await?;
    parent_app::delete_notice(&mut tx, &state.hlc, user.actor(), user.user.school_id, id).await?;
    tx.commit().await?;
    state.wake_nexus_sync();
    Ok(StatusCode::NO_CONTENT)
}

// ---- conversations --------------------------------------------------------------------------

/// What the caller may do with conversations.
struct Reach {
    /// Reads every conversation of the school.
    all: bool,
    /// Sections the caller is class teacher of (and holds `messages.reply`).
    own_sections: Vec<Uuid>,
}

impl Reach {
    fn may_read(&self, section: Option<Uuid>) -> bool {
        self.all || self.may_reply(section)
    }
    fn may_reply(&self, section: Option<Uuid>) -> bool {
        section.is_some_and(|s| self.own_sections.contains(&s))
    }
}

async fn reach(state: &AppState, user: &CurrentUser) -> ApiResult<Reach> {
    let all = user.can(Permission::MessagesViewAll);
    let replies = user.can(Permission::MessagesReply);
    if !all && !replies {
        return Err(ApiError::forbidden());
    }
    require_joined(state, user).await?;
    let mut own_sections = Vec::new();
    if replies {
        let mut c = state.pool.acquire().await?;
        if let Some(me) = staff::find_by_user(&mut c, user.user.id).await? {
            own_sections = parent_app::sections_as_class_teacher(&mut c, me.id).await?;
        }
    }
    Ok(Reach { all, own_sections })
}

#[derive(Debug, Serialize)]
pub struct ConversationRow {
    #[serde(flatten)]
    pub summary: ConversationSummary,
    /// The caller is the class teacher and may answer.
    pub can_reply: bool,
}

async fn list_conversations(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Json<Vec<ConversationRow>>> {
    let reach = reach(&state, &user).await?;
    let mut c = state.pool.acquire().await?;
    let sections = (!reach.all).then_some(reach.own_sections.as_slice());
    let rows = parent_app::conversations(&mut c, user.user.school_id, sections).await?;
    Ok(Json(
        rows.into_iter()
            .map(|summary| ConversationRow {
                can_reply: reach.may_reply(summary.section_id),
                summary,
            })
            .collect(),
    ))
}

#[derive(Debug, Serialize)]
pub struct ConversationView {
    pub student_id: Uuid,
    pub student_name: String,
    pub guardian_id: Uuid,
    pub guardian_name: String,
    pub class_name: Option<String>,
    pub section_name: Option<String>,
    /// The class teacher of the student's section, the person the guardian is writing to.
    pub teacher_name: Option<String>,
    pub can_reply: bool,
    /// Messages from the guardian the class teacher has not read.
    pub unread: i64,
    pub messages: Vec<Message>,
}

/// The student, the guardian and the caller's rights for one conversation, or `404` when
/// the two do not belong together, or `403` when it is another teacher's class.
async fn open(
    state: &AppState,
    user: &CurrentUser,
    student_id: Uuid,
    guardian_id: Uuid,
) -> ApiResult<(ConversationView, Reach)> {
    let reach = reach(state, user).await?;
    let mut c = state.pool.acquire().await?;
    let school_id = user.user.school_id;
    if !parent_app::is_guardian_of(&mut c, school_id, guardian_id, student_id).await? {
        return Err(ApiError::not_found("The conversation"));
    }
    let teacher = parent_app::class_teacher(&mut c, student_id).await?;
    let section = teacher.as_ref().map(|t| t.section_id);
    if !reach.may_read(section) {
        return Err(ApiError::forbidden());
    }
    let (student_name, guardian_name): (String, String) = sqlx::query_as(
        "SELECT s.full_name, g.full_name FROM students s, guardians g WHERE s.id = $1 AND g.id = $2",
    )
    .bind(student_id)
    .bind(guardian_id)
    .fetch_one(&mut *c)
    .await?;
    let view = ConversationView {
        student_id,
        student_name,
        guardian_id,
        guardian_name,
        class_name: teacher.as_ref().map(|t| t.class_name.clone()),
        section_name: teacher.as_ref().map(|t| t.section_name.clone()),
        teacher_name: teacher.as_ref().and_then(|t| t.teacher_name.clone()),
        can_reply: reach.may_reply(section),
        unread: parent_app::unread(&mut c, student_id, guardian_id, Side::Staff).await?,
        messages: parent_app::conversation(&mut c, student_id, guardian_id).await?,
    };
    Ok((view, reach))
}

async fn read_conversation(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((student_id, guardian_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<ConversationView>> {
    Ok(Json(open(&state, &user, student_id, guardian_id).await?.0))
}

fn not_the_class_teacher() -> ApiError {
    ApiError::new(
        StatusCode::FORBIDDEN,
        "not_class_teacher",
        "Only the class teacher of this student's section answers this conversation.",
    )
}

#[derive(Debug, Deserialize)]
pub struct SendBody {
    pub body: String,
}

async fn send(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((student_id, guardian_id)): Path<(Uuid, Uuid)>,
    body: JsonBody<SendBody>,
) -> ApiResult<(StatusCode, Json<Message>)> {
    let Json(b) = body?;
    let (view, _) = open(&state, &user, student_id, guardian_id).await?;
    if !view.can_reply {
        return Err(not_the_class_teacher());
    }
    let text = clean_message(&b.body).map_err(ApiError::validation)?;
    let mut tx = state.pool.begin().await?;
    let message = parent_app::send_message(
        &mut tx,
        &state.hlc,
        user.actor(),
        NewMessage {
            school_id: user.user.school_id,
            student_id,
            guardian_id,
            side: Side::Staff,
            sender_user_id: Some(user.user.id),
            sender_name: user.user.full_name.clone(),
            body: text,
        },
    )
    .await?;
    // Answering is reading.
    parent_app::mark_read(
        &mut tx,
        &state.hlc,
        user.actor(),
        user.user.school_id,
        student_id,
        guardian_id,
        Side::Staff,
    )
    .await?;
    tx.commit().await?;
    // Send it on its way now instead of at the next timed sync.
    state.wake_nexus_sync();
    Ok((StatusCode::CREATED, Json(message)))
}

async fn mark_read(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((student_id, guardian_id)): Path<(Uuid, Uuid)>,
) -> ApiResult<StatusCode> {
    let (view, _) = open(&state, &user, student_id, guardian_id).await?;
    // Only the class teacher's reading counts as "read by the school"; the principal looking
    // in does not hide a message from the teacher's unread list.
    if !view.can_reply {
        return Err(not_the_class_teacher());
    }
    if view.unread > 0 {
        let mut tx = state.pool.begin().await?;
        parent_app::mark_read(
            &mut tx,
            &state.hlc,
            user.actor(),
            user.user.school_id,
            student_id,
            guardian_id,
            Side::Staff,
        )
        .await?;
        tx.commit().await?;
        state.wake_nexus_sync();
    }
    Ok(StatusCode::NO_CONTENT)
}
