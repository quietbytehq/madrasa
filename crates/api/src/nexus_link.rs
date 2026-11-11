//! The school's side of Madrasa Nexus (docs/adr/0005-nexus-hub-and-sync.md): joining a hub,
//! syncing with it and leaving.
//!
//! The school always calls the hub; the hub never calls the school. A school that has not
//! joined runs none of this: no triggers, no network calls. Its data is its own either way:
//! leaving, or being removed, never touches a record.

use std::time::Duration;

use chrono::{DateTime, Utc};
use madrasa_core::sync::{
    hub_url, AckRequest, AckResponse, JoinRequest, JoinResponse, LeaveRequest, Origin, Parked,
    PullRequest, PullResponse, PushRequest, PushResponse,
};
use madrasa_db::nexus_link::{self, Ended, Link};
use madrasa_db::sync::round::{self, RoundError, Transport, TransportError};
use madrasa_db::{schools, sync, Actor};
use serde::de::DeserializeOwned;
use serde::Serialize;
use uuid::Uuid;

use crate::{ApiError, ApiResult, AppState};

/// Path of the hub API under the hub's address.
const HUB_API: &str = "/api/nexus/v1";
/// How often a joined school syncs by itself.
pub const SYNC_EVERY: Duration = Duration::from_secs(5 * 60);
/// The first automatic sync after the app starts.
const FIRST_SYNC_AFTER: Duration = Duration::from_secs(20);

const OFFLINE: &str = "The hub could not be reached. Check the internet connection and the hub address; the school carries on working and will sync when the hub is back.";

/// What Settings → Nexus shows.
#[derive(Debug, Clone, Serialize)]
pub struct LinkStatus {
    /// Joined and syncing.
    pub linked: bool,
    pub hub_url: Option<String>,
    pub hub_name: Option<String>,
    pub joined_at: Option<DateTime<Utc>>,
    pub last_sync_at: Option<DateTime<Utc>>,
    /// Why the last sync failed, in plain words.
    pub last_error: Option<String>,
    pub last_report: Option<serde_json::Value>,
    /// How the last membership ended, when the school is no longer joined.
    pub ended: Option<Ended>,
    /// Records changed here and not yet sent.
    pub pending: i64,
    /// Records from head office waiting to be stored here, with the reason.
    pub parked: Vec<Parked>,
    /// Entries in the overwrite log.
    pub overwrites: i64,
}

// ---- talking to the hub -----------------------------------------------------------------------

fn http() -> Result<reqwest::Client, TransportError> {
    crate::lan::init();
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|_| TransportError::Offline(OFFLINE.into()))
}

/// POST JSON to the hub and read its answer. Error texts are fixed: nothing from the request
/// (the token least of all) is ever put into a message or a log line.
async fn post<B: Serialize + Sync, R: DeserializeOwned>(
    http: &reqwest::Client,
    base: &str,
    path: &str,
    token: Option<&str>,
    body: &B,
) -> Result<R, TransportError> {
    let mut request = http.post(format!("{base}{HUB_API}{path}")).json(body);
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .map_err(|_| TransportError::Offline(OFFLINE.into()))?;
    let status = response.status();
    if status.is_success() {
        return response
            .json::<R>()
            .await
            .map_err(|_| TransportError::Refused {
                code: "bad_answer".into(),
                message:
                    "The answer could not be read. Check that the address is a Madrasa Nexus hub."
                        .into(),
            });
    }
    let answer: serde_json::Value = response.json().await.unwrap_or(serde_json::Value::Null);
    let code = answer["error"]["code"]
        .as_str()
        .unwrap_or("refused")
        .to_string();
    let message = answer["error"]["message"]
        .as_str()
        .filter(|m| !m.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            "The hub refused the request. Check that the address is a Madrasa Nexus hub."
                .to_string()
        });
    if status == reqwest::StatusCode::GONE {
        Err(TransportError::Removed(message))
    } else {
        Err(TransportError::Refused { code, message })
    }
}

/// The hub over HTTPS, with this school's token.
pub struct HubClient {
    http: reqwest::Client,
    base: String,
    token: String,
}

impl HubClient {
    pub fn new(base: &str, token: &str) -> Result<Self, TransportError> {
        Ok(Self {
            http: http()?,
            base: base.to_string(),
            token: token.to_string(),
        })
    }
}

impl Transport for HubClient {
    async fn push(&self, req: PushRequest) -> Result<PushResponse, TransportError> {
        post(
            &self.http,
            &self.base,
            "/sync/push",
            Some(&self.token),
            &req,
        )
        .await
    }
    async fn pull(&self, req: PullRequest) -> Result<PullResponse, TransportError> {
        post(
            &self.http,
            &self.base,
            "/sync/pull",
            Some(&self.token),
            &req,
        )
        .await
    }
    async fn ack(&self, req: AckRequest) -> Result<AckResponse, TransportError> {
        post(&self.http, &self.base, "/sync/ack", Some(&self.token), &req).await
    }
}

fn refused(err: TransportError) -> ApiError {
    match err {
        TransportError::Offline(message) => ApiError::new(
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "hub_unreachable",
            message,
        ),
        TransportError::Removed(message) | TransportError::Refused { message, .. } => {
            ApiError::new(axum::http::StatusCode::CONFLICT, "hub_refused", message)
        }
    }
}

// ---- status ---------------------------------------------------------------------------------

async fn school_id(state: &AppState) -> ApiResult<Uuid> {
    let mut conn = state.pool.acquire().await?;
    Ok(schools::current(&mut conn)
        .await?
        .ok_or_else(|| ApiError::conflict("Finish the school's setup first."))?
        .id)
}

pub async fn status(state: &AppState) -> ApiResult<LinkStatus> {
    let school = school_id(state).await?;
    let mut conn = state.pool.acquire().await?;
    let link = nexus_link::get(&mut conn, school).await?;
    let counts = sync::status(&mut conn).await?;
    let parked = sync::list_parked(&mut conn).await?;
    let active = link.as_ref().is_some_and(Link::is_active);
    Ok(LinkStatus {
        linked: active,
        hub_url: link.as_ref().map(|l| l.hub_url.clone()),
        hub_name: link.as_ref().map(|l| l.hub_name.clone()),
        joined_at: link.as_ref().filter(|_| active).map(|l| l.joined_at),
        last_sync_at: counts.last_sync_at.filter(|_| active),
        last_error: counts.last_error.filter(|_| active),
        last_report: counts.last_report.filter(|_| active),
        ended: link.and_then(|l| l.ended),
        pending: counts.pending,
        parked,
        overwrites: counts.overwrites,
    })
}

// ---- joining and leaving ----------------------------------------------------------------------

/// Join a hub with a one-time join code. On success change capture is switched on and every
/// existing record is queued for the first upload.
pub async fn join(
    state: &AppState,
    actor: Actor,
    address: &str,
    join_code: &str,
) -> ApiResult<LinkStatus> {
    let school = school_id(state).await?;
    let base = hub_url(address).map_err(ApiError::validation)?;
    if join_code.trim().is_empty() {
        return Err(ApiError::validation(
            "Enter the join code head office gave you.",
        ));
    }
    let request = {
        let mut conn = state.pool.acquire().await?;
        if let Some(link) = nexus_link::get(&mut conn, school).await? {
            if link.is_active() {
                return Err(ApiError::conflict(format!(
                    "This school has already joined {}. Leave it before joining another network.",
                    link.hub_name
                )));
            }
        }
        JoinRequest {
            join_code: join_code.trim().to_string(),
            school: sync::school_record(&mut conn, school).await?,
            app_version: state.version.to_string(),
            schema_version: sync::schema_version(&mut conn).await?,
            edition: state.edition().as_str().to_string(),
        }
    };
    let http = http().map_err(refused)?;
    let joined: JoinResponse = post(&http, &base, "/join", None, &request)
        .await
        .map_err(refused)?;

    let mut tx = state.pool.begin().await?;
    sync::enable(&mut tx, school, Origin::School).await?;
    let queued = sync::queue_snapshot(&mut tx).await?;
    nexus_link::set_token(&mut tx, &state.hlc, actor, school, Some(&joined.token)).await?;
    nexus_link::set(
        &mut tx,
        &state.hlc,
        actor,
        school,
        &Link {
            hub_url: base,
            hub_name: joined.hub_name,
            school_ref: joined.school_ref,
            joined_at: Utc::now(),
            ended: None,
        },
    )
    .await?;
    tx.commit().await?;
    tracing::info!(queued, "joined a Nexus hub; the first upload is queued");
    status(state).await
}

/// Stop syncing and forget the hub's key. No record of the school is touched.
async fn unlink(
    state: &AppState,
    actor: Actor,
    school: Uuid,
    mut link: Link,
    by: &str,
    reason: &str,
) -> ApiResult<()> {
    let mut tx = state.pool.begin().await?;
    sync::disable(&mut tx).await?;
    nexus_link::set_token(&mut tx, &state.hlc, actor, school, None).await?;
    link.ended = Some(Ended {
        at: Utc::now(),
        by: by.to_string(),
        reason: reason.to_string(),
    });
    nexus_link::set(&mut tx, &state.hlc, actor, school, &link).await?;
    tx.commit().await?;
    Ok(())
}

/// What leaving did.
#[derive(Debug, Clone, Serialize)]
pub struct LeaveOutcome {
    /// The hub was reached and told. When false the school has still left; head office
    /// should remove it at the hub.
    pub hub_told: bool,
    pub status: LinkStatus,
}

/// The school leaves its hub. The hub is told when it can be reached; either way the school
/// stops syncing and keeps all its data.
pub async fn leave(state: &AppState, actor: Actor, reason: &str) -> ApiResult<LeaveOutcome> {
    let school = school_id(state).await?;
    let _guard = state.nexus_sync.lock().await;
    let (link, token) = {
        let mut conn = state.pool.acquire().await?;
        let link = nexus_link::get(&mut conn, school)
            .await?
            .filter(Link::is_active)
            .ok_or_else(|| ApiError::conflict("This school has not joined a network."))?;
        let token = nexus_link::token(&mut conn, &state.hlc, school).await?;
        (link, token)
    };
    let hub_told = match (&token, http()) {
        (Some(token), Ok(http)) => {
            let told: Result<(), TransportError> = post(
                &http,
                &link.hub_url,
                "/sync/leave",
                Some(token),
                &LeaveRequest {
                    reason: reason.trim().to_string(),
                },
            )
            .await;
            // A hub that no longer knows the school needs no telling.
            matches!(told, Ok(()) | Err(TransportError::Removed(_)))
                || matches!(&told, Err(TransportError::Refused { code, .. }) if code == "unknown_school")
        }
        _ => false,
    };
    let why = if reason.trim().is_empty() {
        "The school left the network."
    } else {
        reason.trim()
    };
    unlink(state, actor, school, link, "school", why).await?;
    Ok(LeaveOutcome {
        hub_told,
        status: status(state).await?,
    })
}

// ---- syncing ------------------------------------------------------------------------------

/// Run one sync round now. Problems reaching the hub are not an error of this call: they are
/// recorded and shown in the status, and the next round tries again.
pub async fn sync_now(state: &AppState, actor: Actor) -> ApiResult<LinkStatus> {
    let school = school_id(state).await?;
    // One round at a time: the timer and the "Sync now" button must not interleave.
    let _guard = state.nexus_sync.lock().await;
    let (link, token) = {
        let mut conn = state.pool.acquire().await?;
        let link = nexus_link::get(&mut conn, school)
            .await?
            .filter(Link::is_active)
            .ok_or_else(|| ApiError::conflict("This school has not joined a network."))?;
        let token = nexus_link::token(&mut conn, &state.hlc, school)
            .await?
            .ok_or_else(|| {
                ApiError::conflict(
                    "The key for the hub is missing. Leave the network and join again.",
                )
            })?;
        (link, token)
    };
    let outcome = match HubClient::new(&link.hub_url, &token) {
        Ok(client) => round::run(&state.pool, &state.hlc, state.version, &client).await,
        Err(e) => Err(RoundError::Transport(e)),
    };
    // How the round went is local bookkeeping: writing it as a setting would be audited, and
    // the audit row would itself be a record to send at the next sync.
    let mut conn = state.pool.acquire().await?;
    match outcome {
        Ok(report) => sync::note_round(&mut conn, Ok(&report)).await?,
        Err(RoundError::Transport(TransportError::Removed(message))) => {
            drop(conn);
            tracing::warn!("this school was removed from its Nexus hub");
            unlink(state, actor, school, link, "head_office", &message).await?;
            return status(state).await;
        }
        Err(RoundError::Transport(e)) => {
            sync::note_round(&mut conn, Err(&e.to_string())).await?;
        }
        Err(RoundError::Db(e)) => return Err(e.into()),
    }
    drop(conn);
    status(state).await
}

/// At start: make sure change capture matches the link. A joined school gets the trigger on
/// any table a new version added; a school that is not joined has none (for example after
/// restoring a backup made before it joined).
pub async fn reconcile(state: &AppState) -> ApiResult<()> {
    let mut conn = state.pool.acquire().await?;
    let Some(school) = schools::current(&mut conn).await? else {
        return Ok(());
    };
    let active = nexus_link::get(&mut conn, school.id)
        .await?
        .is_some_and(|l| l.is_active());
    drop(conn);
    let mut tx = state.pool.begin().await?;
    if active {
        sync::enable(&mut tx, school.id, Origin::School).await?;
    } else if sync::triggers_installed(&mut tx).await? > 0 {
        sync::disable(&mut tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Background task: sync a joined school every few minutes. Does nothing, and makes no
/// network call, for a school that has not joined.
pub fn spawn_scheduler(state: AppState) {
    tokio::spawn(async move {
        if let Err(e) = reconcile(&state).await {
            tracing::warn!(error = %e, "nexus: could not check change capture at start");
        }
        tokio::time::sleep(FIRST_SYNC_AFTER).await;
        loop {
            match status(&state).await {
                Ok(s) if s.linked => {
                    let actor = match school_id(&state).await {
                        Ok(id) => Actor::system_for(id),
                        Err(_) => Actor::SYSTEM,
                    };
                    match sync_now(&state, actor).await {
                        Ok(after) => {
                            if let Some(problem) = after.last_error {
                                tracing::info!(%problem, "nexus: sync did not complete");
                            }
                        }
                        Err(e) => tracing::warn!(error = %e, "nexus: sync failed"),
                    }
                }
                _ => {}
            }
            tokio::time::sleep(SYNC_EVERY).await;
        }
    });
}
