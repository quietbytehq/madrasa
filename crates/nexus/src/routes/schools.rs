//! The school register (`nexus.admin`): invitations, schools and their sync state, removing
//! a school, purging an archive.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Duration, Utc};
use madrasa_core::backup::{generate_recovery_key, normalize_recovery_key};
use madrasa_core::ids::new_id;
use madrasa_core::session_token;
use madrasa_core::sync::{Overwrite, Parked};
use madrasa_db::sync::{self, SyncStatus};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{required, JsonBody};
use crate::auth::CurrentHubUser;
use crate::permissions::HubPermission;
use crate::tenants::{self, Ending, HubSchool};
use crate::{store, HubError, HubResult, HubState};

/// Days a join code works when the request does not say.
const DEFAULT_INVITATION_DAYS: i64 = 7;

pub fn router() -> Router<HubState> {
    Router::new()
        .route(
            "/invitations",
            get(list_invitations).post(create_invitation),
        )
        .route("/invitations/{id}/revoke", post(revoke_invitation))
        .route("/schools", get(list_schools))
        .route("/schools/{id}/overwrites", get(overwrites))
        .route("/schools/{id}/parked", get(parked))
        .route("/schools/{id}/remove", post(remove))
        .route("/schools/{id}/purge", post(purge))
        .route("/schools/{id}/resend", post(resend))
}

/// The storable form of a join code: what the school typed, normalised, then hashed.
pub(crate) fn join_code_hash(code: &str) -> Option<String> {
    normalize_recovery_key(code).map(|c| session_token::hash(&c))
}

// ---- invitations ------------------------------------------------------------------------------

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Invitation {
    pub id: Uuid,
    pub school_name: String,
    pub note: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
    pub used_by: Option<Uuid>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

const INVITATION_COLS: &str =
    "id, school_name, note, expires_at, used_at, used_by, revoked_at, created_at";

#[derive(Debug, Deserialize)]
pub struct NewInvitation {
    pub school_name: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub expires_in_days: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct CreatedInvitation {
    #[serde(flatten)]
    pub invitation: Invitation,
    /// Shown once. The hub keeps only its hash.
    pub join_code: String,
    /// The address the school types, when the hub knows its public address.
    pub hub_url: Option<String>,
}

async fn list_invitations(
    State(state): State<HubState>,
    current: CurrentHubUser,
) -> HubResult<Json<Vec<Invitation>>> {
    current.require(HubPermission::Admin)?;
    let rows = sqlx::query_as::<_, Invitation>(sqlx::AssertSqlSafe(format!(
        "SELECT {INVITATION_COLS} FROM hub_invitations WHERE deleted_at IS NULL ORDER BY created_at DESC"
    )))
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

async fn create_invitation(
    State(state): State<HubState>,
    current: CurrentHubUser,
    body: JsonBody<NewInvitation>,
) -> HubResult<(StatusCode, Json<CreatedInvitation>)> {
    current.require(HubPermission::Admin)?;
    let Json(b) = body?;
    let school_name = required(&b.school_name, "The school's name")?;
    let days = b.expires_in_days.unwrap_or(DEFAULT_INVITATION_DAYS);
    if !(1..=90).contains(&days) {
        return Err(HubError::validation(
            "An invitation can be valid for 1 to 90 days.",
        ));
    }
    let join_code = generate_recovery_key();
    let hash = join_code_hash(&join_code).ok_or_else(HubError::internal)?;
    let id = new_id();
    let mut tx = state.pool.begin().await?;
    let invitation = sqlx::query_as::<_, Invitation>(sqlx::AssertSqlSafe(format!(
        "INSERT INTO hub_invitations (id, school_name, note, code_hash, expires_at, created_by, updated_by, hlc)
         VALUES ($1, $2, $3, $4, $5, $6, $6, $7) RETURNING {INVITATION_COLS}"
    )))
    .bind(id)
    .bind(&school_name)
    .bind(b.note.as_deref().map(str::trim).filter(|n| !n.is_empty()))
    .bind(hash)
    .bind(Utc::now() + Duration::days(days))
    .bind(current.user.id)
    .bind(state.hlc.now().to_string())
    .fetch_one(&mut *tx)
    .await?;
    // The audit row says an invitation was made, never the code.
    store::audit(
        &mut tx,
        &state.hlc,
        Some(current.user.id),
        "create",
        "hub_invitation",
        Some(id),
        None,
        Some(serde_json::json!(invitation)),
    )
    .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(CreatedInvitation {
            invitation,
            join_code,
            hub_url: state.public_url.clone(),
        }),
    ))
}

async fn revoke_invitation(
    State(state): State<HubState>,
    current: CurrentHubUser,
    Path(id): Path<Uuid>,
) -> HubResult<Json<Invitation>> {
    current.require(HubPermission::Admin)?;
    let mut tx = state.pool.begin().await?;
    let row = sqlx::query_as::<_, Invitation>(sqlx::AssertSqlSafe(format!(
        "UPDATE hub_invitations SET revoked_at = COALESCE(revoked_at, now()), updated_at = now(),
             updated_by = $2, hlc = $3
         WHERE id = $1 AND deleted_at IS NULL RETURNING {INVITATION_COLS}"
    )))
    .bind(id)
    .bind(current.user.id)
    .bind(state.hlc.now().to_string())
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| HubError::not_found("Invitation"))?;
    if row.used_at.is_some() {
        return Err(HubError::conflict(
            "This invitation has already been used. Remove the school instead.",
        ));
    }
    store::audit(
        &mut tx,
        &state.hlc,
        Some(current.user.id),
        "revoke",
        "hub_invitation",
        Some(id),
        None,
        None,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(row))
}

// ---- schools ----------------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct SchoolRow {
    #[serde(flatten)]
    pub school: HubSchool,
    /// Sync state of the hub's copy; absent once the data has been purged.
    pub sync: Option<SyncStatus>,
}

async fn load(state: &HubState, id: Uuid) -> HubResult<HubSchool> {
    let mut conn = state.pool.acquire().await?;
    tenants::get(&mut conn, id)
        .await?
        .ok_or_else(|| HubError::not_found("School"))
}

async fn list_schools(
    State(state): State<HubState>,
    current: CurrentHubUser,
) -> HubResult<Json<Vec<SchoolRow>>> {
    // Report readers see which schools exist and when they last synced.
    if !current.can(HubPermission::Admin) {
        current.require(HubPermission::Reports)?;
    }
    let schools = {
        let mut conn = state.pool.acquire().await?;
        tenants::list(&mut conn).await?
    };
    let mut out = Vec::with_capacity(schools.len());
    for school in schools {
        let sync = if tenants::is_schema_name(&school.schema_name) {
            let mut tx = tenants::begin(&state.pool, &school).await?;
            let status = sync::status(&mut tx).await?;
            tx.commit().await?;
            Some(status)
        } else {
            None
        };
        out.push(SchoolRow { school, sync });
    }
    Ok(Json(out))
}

#[derive(Debug, Deserialize)]
pub struct PageQuery {
    #[serde(default)]
    pub page: Option<i64>,
    #[serde(default)]
    pub page_size: Option<i64>,
}

async fn overwrites(
    State(state): State<HubState>,
    current: CurrentHubUser,
    Path(id): Path<Uuid>,
    q: Result<Query<PageQuery>, axum::extract::rejection::QueryRejection>,
) -> HubResult<Json<Vec<Overwrite>>> {
    current.require(HubPermission::Admin)?;
    let Query(q) = q?;
    let size = q.page_size.unwrap_or(50).clamp(1, 200);
    let page = q.page.unwrap_or(1).max(1);
    let school = load(&state, id).await?;
    let mut tx = tenants::begin(&state.pool, &school)
        .await
        .map_err(|_| HubError::conflict("This school's data has been purged."))?;
    let rows = sync::list_overwrites(&mut tx, size, (page - 1) * size).await?;
    tx.commit().await?;
    Ok(Json(rows))
}

async fn parked(
    State(state): State<HubState>,
    current: CurrentHubUser,
    Path(id): Path<Uuid>,
) -> HubResult<Json<Vec<Parked>>> {
    current.require(HubPermission::Admin)?;
    let school = load(&state, id).await?;
    let mut tx = tenants::begin(&state.pool, &school)
        .await
        .map_err(|_| HubError::conflict("This school's data has been purged."))?;
    let rows = sync::list_parked(&mut tx).await?;
    tx.commit().await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
pub struct RemoveBody {
    pub reason: String,
}

/// Head office removes a school: its token stops working at once. Its data stays as an
/// archive, out of every report, until it is purged.
async fn remove(
    State(state): State<HubState>,
    current: CurrentHubUser,
    Path(id): Path<Uuid>,
    body: JsonBody<RemoveBody>,
) -> HubResult<Json<HubSchool>> {
    current.require(HubPermission::Admin)?;
    let Json(b) = body?;
    let reason = required(&b.reason, "A reason")?;
    let school = load(&state, id).await?;
    if school.status != "active" {
        return Err(HubError::conflict(
            "Only a school that is joined can be removed.",
        ));
    }
    let after = tenants::end(
        &state.pool,
        &state.hlc,
        Some(current.user.id),
        &school,
        Ending::Removed,
        &reason,
    )
    .await
    .map_err(internal)?;
    Ok(Json(after))
}

#[derive(Debug, Deserialize)]
pub struct PurgeBody {
    /// The school's code, typed again to confirm.
    pub confirm_code: String,
}

/// Delete the archive of a school that has left or was removed. This cannot be undone.
async fn purge(
    State(state): State<HubState>,
    current: CurrentHubUser,
    Path(id): Path<Uuid>,
    body: JsonBody<PurgeBody>,
) -> HubResult<Json<HubSchool>> {
    current.require(HubPermission::Admin)?;
    let Json(b) = body?;
    let school = load(&state, id).await?;
    if !matches!(school.status.as_str(), "left" | "removed") {
        return Err(HubError::conflict(
            "Only the archive of a school that has left or was removed can be purged. Remove the school first.",
        ));
    }
    if !b
        .confirm_code
        .trim()
        .eq_ignore_ascii_case(school.code.trim())
    {
        return Err(HubError::validation(
            "Type the school's code exactly to confirm.",
        ));
    }
    let after = tenants::purge(&state.pool, &state.hlc, Some(current.user.id), &school)
        .await
        .map_err(internal)?;
    Ok(Json(after))
}

#[derive(Debug, Serialize)]
pub struct Resent {
    pub queued: i64,
}

/// Queue the hub's whole copy to be sent down again (after a school restored an old
/// backup). The school takes what it lacks; its own unsent changes still go through the
/// usual rules.
async fn resend(
    State(state): State<HubState>,
    current: CurrentHubUser,
    Path(id): Path<Uuid>,
) -> HubResult<Json<Resent>> {
    current.require(HubPermission::Admin)?;
    let school = load(&state, id).await?;
    if school.status != "active" {
        return Err(HubError::conflict(
            "Only a school that is joined can be sent its data again.",
        ));
    }
    let mut tx = tenants::begin(&state.pool, &school).await?;
    let queued = sync::queue_snapshot(&mut tx).await?;
    tx.commit().await?;
    let mut conn = state.pool.acquire().await?;
    store::audit(
        &mut conn,
        &state.hlc,
        Some(current.user.id),
        "resend",
        "hub_school",
        Some(id),
        None,
        Some(serde_json::json!({ "queued": queued })),
    )
    .await?;
    Ok(Json(Resent { queued }))
}

pub(crate) fn internal(err: anyhow::Error) -> HubError {
    tracing::error!(error = %err, "hub operation failed");
    HubError::internal()
}
