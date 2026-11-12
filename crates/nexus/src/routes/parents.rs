//! Parent app accounts (docs/adr/0006-parent-app.md).
//!
//! Two callers, kept apart:
//! - **A school**, with its sync token: asks for a one-time access code for one of its
//!   guardians, sees which guardians have access, ends a guardian's access.
//! - **A parent**, in the app: activates an access code (which opens an account, adds a
//!   school to an existing one, or sets a new password), signs in, signs out.
//!
//! A parent session is its own kind of token: it opens nothing in the head office console
//! and nothing in the school sync API, and neither of those opens anything here.

use axum::extract::{FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use madrasa_core::parent_app::{
    generate_access_code, normalize_access_code, normalize_phone, AccessCode, AccessCodeRequest,
    GuardianAccess,
};
use madrasa_core::password::{hash_password, validate_password, verify_password};
use madrasa_core::session_token;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::sync::SyncSchool;
use super::JsonBody;
use crate::auth::bearer_token;
use crate::parents::{self, Parent};
use crate::tenants::{self, HubSchool};
use crate::{store, HubError, HubResult, HubState};

pub fn router() -> Router<HubState> {
    // For the parent, in the app (which calls from another origin; see `app_cors`).
    let app = Router::new()
        .route("/parent/activate/check", post(check_code))
        .route("/parent/activate", post(activate))
        .route("/parent/login", post(login))
        .route("/parent/logout", post(logout))
        .route("/parent/me", get(me))
        .route("/parent/password", post(change_password))
        .layer(super::parent_data::app_cors());
    Router::new()
        // For the school.
        .route("/sync/parents", get(access_list))
        .route("/sync/parents/codes", post(issue_code))
        .route("/sync/parents/{guardian_id}", delete(end_access))
        .merge(app)
}

/// The signed-in parent. Extracting it fails with `401` without a live parent session.
#[derive(Debug, Clone)]
pub struct CurrentParent {
    pub parent: Parent,
    pub session_id: Uuid,
}

impl FromRequestParts<HubState> for CurrentParent {
    type Rejection = HubError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &HubState,
    ) -> Result<Self, Self::Rejection> {
        let token = bearer_token(parts).ok_or_else(HubError::unauthenticated)?;
        let mut conn = state.pool.acquire().await?;
        let (session_id, parent) = parents::find_live_session(
            &mut conn,
            &state.hlc,
            &session_token::hash(token),
            Utc::now(),
        )
        .await?
        .ok_or_else(HubError::unauthenticated)?;
        Ok(CurrentParent { parent, session_id })
    }
}

// ---- for the school -------------------------------------------------------------------------

/// The guardian's name, when the guardian exists in the school's records and has at least
/// one student.
async fn guardian_with_children(
    state: &HubState,
    school: &HubSchool,
    guardian_id: Uuid,
) -> HubResult<Option<(String, Vec<String>)>> {
    let mut tx = tenants::begin(&state.pool, school).await?;
    let name: Option<String> =
        sqlx::query_scalar("SELECT full_name FROM guardians WHERE id = $1 AND deleted_at IS NULL")
            .bind(guardian_id)
            .fetch_optional(&mut *tx)
            .await?;
    let children: Vec<String> = sqlx::query_scalar(
        "SELECT s.full_name FROM student_guardians sg
         JOIN students s ON s.id = sg.student_id AND s.deleted_at IS NULL
         WHERE sg.guardian_id = $1 AND sg.deleted_at IS NULL ORDER BY s.full_name, s.id",
    )
    .bind(guardian_id)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(name.filter(|_| !children.is_empty()).map(|n| (n, children)))
}

/// A one-time access code for one guardian of the calling school. Shown once.
async fn issue_code(
    State(state): State<HubState>,
    SyncSchool(school): SyncSchool,
    body: JsonBody<AccessCodeRequest>,
) -> HubResult<Json<AccessCode>> {
    let Json(req) = body?;
    let Some((guardian_name, _)) = guardian_with_children(&state, &school, req.guardian_id).await?
    else {
        return Err(HubError::new(
            StatusCode::NOT_FOUND,
            "guardian_unknown",
            "Head office does not have this guardian yet, or the guardian has no student. Sync, then try again.",
        ));
    };
    let code = generate_access_code();
    let hash = session_token::hash(&normalize_access_code(&code).ok_or_else(HubError::internal)?);
    let mut tx = state.pool.begin().await?;
    let expires_at = parents::issue_code(
        &mut tx,
        &state.hlc,
        school.id,
        req.guardian_id,
        &guardian_name,
        &hash,
        Utc::now(),
    )
    .await?;
    // The log says a code was issued and for whom, never the code.
    store::audit(
        &mut tx,
        &state.hlc,
        None,
        "parent_code",
        "hub_school",
        Some(school.id),
        None,
        Some(serde_json::json!({ "guardian_id": req.guardian_id })),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(AccessCode {
        code,
        expires_at,
        guardian_name,
    }))
}

async fn access_list(
    State(state): State<HubState>,
    SyncSchool(school): SyncSchool,
) -> HubResult<Json<Vec<GuardianAccess>>> {
    let mut conn = state.pool.acquire().await?;
    Ok(Json(
        parents::access_list(&mut conn, school.id, Utc::now()).await?,
    ))
}

#[derive(Debug, Serialize)]
pub struct Ended {
    /// An account was linked, or a code was waiting, and no longer is.
    pub ended: bool,
}

/// The school ends a guardian's access: the link ends and unused codes stop working.
async fn end_access(
    State(state): State<HubState>,
    SyncSchool(school): SyncSchool,
    Path(guardian_id): Path<Uuid>,
) -> HubResult<Json<Ended>> {
    let mut tx = state.pool.begin().await?;
    let links =
        parents::end_links(&mut tx, &state.hlc, school.id, Some(guardian_id), "school").await?;
    let codes = parents::revoke_codes(&mut tx, &state.hlc, school.id, Some(guardian_id)).await?;
    if links + codes > 0 {
        store::audit(
            &mut tx,
            &state.hlc,
            None,
            "parent_access_ended",
            "hub_school",
            Some(school.id),
            None,
            Some(serde_json::json!({ "guardian_id": guardian_id, "by": "school" })),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Json(Ended {
        ended: links + codes > 0,
    }))
}

// ---- for the parent -------------------------------------------------------------------------

fn bad_code() -> HubError {
    HubError::new(
        StatusCode::FORBIDDEN,
        "bad_access_code",
        "This access code is not valid. It may have been used already or have expired. Ask the school for a new one.",
    )
}

#[derive(Deserialize)]
pub struct CodeBody {
    pub code: String,
}

#[derive(Debug, Serialize)]
pub struct CodeCheck {
    pub guardian_name: String,
    pub school_name: String,
    /// The children this code gives access to.
    pub children: Vec<String>,
    /// This guardian already has an account (a hint of its phone number): activating again
    /// with that number sets a new password.
    pub existing_phone_hint: Option<String>,
}

/// What a code is for, before anything is created: lets the app greet the parent by name
/// and show whose records the code opens.
async fn check_code(
    State(state): State<HubState>,
    body: JsonBody<CodeBody>,
) -> HubResult<Json<CodeCheck>> {
    let Json(b) = body?;
    let hash = normalize_access_code(&b.code)
        .map(|c| session_token::hash(&c))
        .ok_or_else(bad_code)?;
    let mut tx = state.pool.begin().await?;
    let code = parents::live_code(&mut tx, &hash, Utc::now())
        .await?
        .ok_or_else(bad_code)?;
    let school = tenants::get(&mut tx, code.hub_school_id)
        .await?
        .filter(|s| s.status == "active")
        .ok_or_else(bad_code)?;
    let existing =
        match parents::linked_parent(&mut tx, code.hub_school_id, code.guardian_id).await? {
            Some(id) => parents::get(&mut tx, id)
                .await?
                .map(|p| madrasa_core::parent_app::mask_phone(&p.phone)),
            None => None,
        };
    tx.commit().await?;
    let (guardian_name, children) = guardian_with_children(&state, &school, code.guardian_id)
        .await?
        .ok_or_else(bad_code)?;
    Ok(Json(CodeCheck {
        guardian_name,
        school_name: school.name,
        children,
        existing_phone_hint: existing,
    }))
}

#[derive(Deserialize)]
pub struct ActivateBody {
    pub code: String,
    pub phone: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub parent: Parent,
}

async fn open_session(
    state: &HubState,
    conn: &mut sqlx::PgConnection,
    parent_id: Uuid,
) -> HubResult<SessionResponse> {
    let now = Utc::now();
    let token = session_token::generate();
    parents::record_login(conn, &state.hlc, parent_id, now).await?;
    let expires_at = parents::create_session(conn, &state.hlc, parent_id, &token.hash, now).await?;
    let parent = parents::get(conn, parent_id)
        .await?
        .ok_or_else(HubError::internal)?;
    Ok(SessionResponse {
        token: token.token,
        expires_at,
        parent,
    })
}

/// A wrong password counts towards the lock, like a failed sign-in.
async fn check_password(
    state: &HubState,
    conn: &mut sqlx::PgConnection,
    parent: &Parent,
    password: &str,
) -> HubResult<()> {
    let now = Utc::now();
    if state.lockout.is_locked(parent.locked_until, now) {
        return Err(HubError::account_locked());
    }
    if verify_password(password, &parent.password_hash) {
        return Ok(());
    }
    let next = state
        .lockout
        .after_failure(parent.failed_login_count.max(0) as u32, now);
    parents::record_failed_login(
        conn,
        &state.hlc,
        parent.id,
        next.failed_count as i32,
        next.locked_until,
    )
    .await?;
    if next.locked_until.is_some() {
        tracing::warn!("a parent account was locked after failed sign-ins");
        return Err(HubError::account_locked());
    }
    Err(HubError::new(
        StatusCode::UNAUTHORIZED,
        "invalid_credentials",
        "The phone number or password is incorrect.",
    ))
}

/// Use an access code. Three outcomes, by the phone number given:
/// - no account has it: a new account is opened and linked;
/// - the account already holds this guardian's link: the password is replaced (the school
///   issued a new code because the parent forgot it);
/// - another account has it: its password must be given, and the school is added to it.
async fn activate(
    State(state): State<HubState>,
    body: JsonBody<ActivateBody>,
) -> HubResult<Json<SessionResponse>> {
    let Json(b) = body?;
    let hash = normalize_access_code(&b.code)
        .map(|c| session_token::hash(&c))
        .ok_or_else(bad_code)?;
    let phone = normalize_phone(&b.phone).map_err(HubError::validation)?;
    validate_password(&b.password).map_err(|e| HubError::validation(format!("Password: {e}.")))?;

    let mut tx = state.pool.begin().await?;
    let code = parents::live_code(&mut tx, &hash, Utc::now())
        .await?
        .ok_or_else(bad_code)?;
    let school = tenants::get(&mut tx, code.hub_school_id)
        .await?
        .filter(|s| s.status == "active")
        .ok_or_else(bad_code)?;
    if guardian_with_children(&state, &school, code.guardian_id)
        .await?
        .is_none()
    {
        return Err(bad_code());
    }
    let holder = parents::linked_parent(&mut tx, code.hub_school_id, code.guardian_id).await?;
    let (parent_id, outcome) = match parents::find_by_phone(&mut tx, &phone).await? {
        None => {
            let password_hash = hash_password(&b.password).map_err(|_| HubError::internal())?;
            let parent = parents::create(
                &mut tx,
                &state.hlc,
                &phone,
                &code.guardian_name,
                &password_hash,
            )
            .await?;
            (parent.id, "new_account")
        }
        Some(existing) if holder == Some(existing.id) => {
            let password_hash = hash_password(&b.password).map_err(|_| HubError::internal())?;
            parents::set_password(&mut tx, &state.hlc, existing.id, &password_hash).await?;
            (existing.id, "new_password")
        }
        Some(existing) => {
            if let Err(refused) = check_password(&state, &mut tx, &existing, &b.password).await {
                // Keep the failed attempt; the code stays unused.
                tx.commit().await?;
                return Err(match refused.code {
                    "invalid_credentials" => HubError::new(
                        StatusCode::UNAUTHORIZED,
                        "account_exists",
                        "This phone number already has an account. Enter that account's password to add this school to it.",
                    ),
                    _ => refused,
                });
            }
            (existing.id, "added_to_account")
        }
    };
    parents::link(
        &mut tx,
        &state.hlc,
        parent_id,
        code.hub_school_id,
        code.guardian_id,
    )
    .await?;
    parents::use_code(&mut tx, &state.hlc, code.id, parent_id).await?;
    store::audit(
        &mut tx,
        &state.hlc,
        None,
        "parent_activated",
        "hub_school",
        Some(code.hub_school_id),
        None,
        Some(serde_json::json!({ "guardian_id": code.guardian_id, "parent_id": parent_id, "outcome": outcome })),
    )
    .await?;
    let session = open_session(&state, &mut tx, parent_id).await?;
    tx.commit().await?;
    Ok(Json(session))
}

#[derive(Deserialize)]
pub struct LoginBody {
    pub phone: String,
    pub password: String,
}

/// Dummy hash used to keep timing similar when no account has the phone number.
const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$Q0M2Qz6Uq2kZ0t4uY3O0+3o2wFzqJ3Yw1x0o0Zy5x2E";

async fn login(
    State(state): State<HubState>,
    body: JsonBody<LoginBody>,
) -> HubResult<Json<SessionResponse>> {
    let Json(b) = body?;
    let wrong = || {
        HubError::new(
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
            "The phone number or password is incorrect.",
        )
    };
    let Ok(phone) = normalize_phone(&b.phone) else {
        return Err(wrong());
    };
    let mut conn = state.pool.acquire().await?;
    let Some(parent) = parents::find_by_phone(&mut conn, &phone).await? else {
        let _ = verify_password(&b.password, DUMMY_HASH);
        return Err(wrong());
    };
    check_password(&state, &mut conn, &parent, &b.password).await?;
    Ok(Json(open_session(&state, &mut conn, parent.id).await?))
}

async fn logout(State(state): State<HubState>, current: CurrentParent) -> HubResult<Json<()>> {
    let mut conn = state.pool.acquire().await?;
    parents::revoke_session(&mut conn, &state.hlc, current.session_id).await?;
    Ok(Json(()))
}

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub parent: Parent,
    pub hub_name: Option<String>,
    /// Guardian records of joined schools this account may act as.
    pub links: usize,
}

async fn me(State(state): State<HubState>, current: CurrentParent) -> HubResult<Json<MeResponse>> {
    let mut conn = state.pool.acquire().await?;
    let links = parents::links(&mut conn, current.parent.id).await?.len();
    let hub_name = store::setting(&mut conn, store::KEY_HUB_NAME)
        .await?
        .and_then(|v| v.as_str().map(str::to_string));
    Ok(Json(MeResponse {
        parent: current.parent,
        hub_name,
        links,
    }))
}

#[derive(Deserialize)]
pub struct PasswordBody {
    pub current: String,
    pub new: String,
}

/// Change the password. Every phone is signed out; the answer carries a new session for
/// this one.
async fn change_password(
    State(state): State<HubState>,
    current: CurrentParent,
    body: JsonBody<PasswordBody>,
) -> HubResult<Json<SessionResponse>> {
    let Json(b) = body?;
    validate_password(&b.new).map_err(|e| HubError::validation(format!("New password: {e}.")))?;
    let mut tx = state.pool.begin().await?;
    if let Err(refused) = check_password(&state, &mut tx, &current.parent, &b.current).await {
        tx.commit().await?;
        return Err(refused);
    }
    let password_hash = hash_password(&b.new).map_err(|_| HubError::internal())?;
    parents::set_password(&mut tx, &state.hlc, current.parent.id, &password_hash).await?;
    let session = open_session(&state, &mut tx, current.parent.id).await?;
    tx.commit().await?;
    Ok(Json(session))
}
