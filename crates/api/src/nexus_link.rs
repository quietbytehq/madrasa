//! The school's side of Madrasa Nexus (docs/adr/0005-nexus-hub-and-sync.md): joining a hub,
//! syncing with it and leaving.
//!
//! The school always calls the hub; the hub never calls the school. A school that has not
//! joined runs none of this: no triggers, no network calls. Its data is its own either way:
//! leaving, or being removed, never touches a record.

use std::time::Duration;

use chrono::{DateTime, Utc};
use madrasa_core::escrow::{self, DepositRequest, EscrowStatus, ReleaseInfo, SeenRequest};
use madrasa_core::sync::{
    hub_url, AckRequest, AckResponse, JoinRequest, JoinResponse, LeaveRequest, Origin, Parked,
    PullRequest, PullResponse, PushRequest, PushResponse,
};
use madrasa_db::nexus_link::{self, Ended, Link};
use madrasa_db::sync::round::{self, RoundError, Transport, TransportError};
use madrasa_db::{audit, schools, sync, Actor};
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

/// POST JSON to the hub and read its answer.
async fn post<B: Serialize + Sync, R: DeserializeOwned>(
    http: &reqwest::Client,
    base: &str,
    path: &str,
    token: Option<&str>,
    body: &B,
) -> Result<R, TransportError> {
    send(
        http.post(format!("{base}{HUB_API}{path}")).json(body),
        token,
    )
    .await
}

/// Send a request to the hub and read its answer. Error texts are fixed: nothing from the
/// request (the token least of all) is ever put into a message or a log line.
async fn send<R: DeserializeOwned>(
    mut request: reqwest::RequestBuilder,
    token: Option<&str>,
) -> Result<R, TransportError> {
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
        Ok(report) => {
            sync::note_round(&mut conn, Ok(&report)).await?;
            // The round worked, so the hub is there: ask whether head office released this
            // school's backup key. Not being able to ask is not a failed sync.
            if let Err(e) = note_releases(state, school, &link, &token).await {
                tracing::info!(problem = %e, "nexus: could not check for key releases");
            }
        }
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

// ---- backup key escrow ------------------------------------------------------------------------

/// What Settings → Nexus shows about the school's backup key and head office.
#[derive(Debug, Clone, Serialize)]
pub struct EscrowView {
    /// Head office has set key escrow up, so a key can be left with it.
    pub available: bool,
    /// This school has left a key with head office.
    pub deposited: bool,
    /// The key left there is the school's current backup key.
    pub current: bool,
    pub deposited_at: Option<DateTime<Utc>>,
    /// Every time head office released this school's key: who, when and why.
    pub releases: Vec<ReleaseInfo>,
}

async fn escrow_link(state: &AppState, school: Uuid) -> ApiResult<(Link, String)> {
    let mut conn = state.pool.acquire().await?;
    let link = nexus_link::get(&mut conn, school)
        .await?
        .filter(Link::is_active)
        .ok_or_else(|| ApiError::conflict("This school has not joined a network."))?;
    let token = nexus_link::token(&mut conn, &state.hlc, school)
        .await?
        .ok_or_else(|| {
            ApiError::conflict("The key for the hub is missing. Leave the network and join again.")
        })?;
    Ok((link, token))
}

async fn escrow_view(state: &AppState, school: Uuid, hub: EscrowStatus) -> ApiResult<EscrowView> {
    let mut conn = state.pool.acquire().await?;
    let local = crate::backup::current_key_id(&mut conn, school).await?;
    Ok(EscrowView {
        available: hub.public_key.is_some(),
        deposited: hub.deposit.is_some(),
        current: matches!((&hub.deposit, &local), (Some(d), Some(l)) if d.key_id == *l),
        deposited_at: hub.deposit.as_ref().map(|d| d.deposited_at),
        releases: hub.releases,
    })
}

async fn hub_escrow(link: &Link, token: &str) -> Result<EscrowStatus, TransportError> {
    let http = http()?;
    send(
        http.get(format!("{}{HUB_API}/sync/escrow", link.hub_url)),
        Some(token),
    )
    .await
}

/// Ask the hub what it holds for this school. Needs the hub to be reachable.
pub async fn escrow_status(state: &AppState) -> ApiResult<EscrowView> {
    let school = school_id(state).await?;
    let (link, token) = escrow_link(state, school).await?;
    let hub = hub_escrow(&link, &token).await.map_err(refused)?;
    escrow_view(state, school, hub).await
}

/// Leave the school's backup key with head office. The key is unlocked with the backup
/// password (or recovery key), sealed to the hub's escrow public key here, and only the
/// sealed form is sent: the hub cannot open it without its escrow passphrase.
pub async fn escrow_deposit(
    state: &AppState,
    actor: Actor,
    secret: &crate::backup::Secret,
) -> ApiResult<EscrowView> {
    let school = school_id(state).await?;
    let (link, token) = escrow_link(state, school).await?;
    let hub = hub_escrow(&link, &token).await.map_err(refused)?;
    let public_key = hub.public_key.ok_or_else(|| {
        ApiError::conflict(
            "Head office has not set up key escrow yet. Ask head office, then try again.",
        )
    })?;
    let mut dek = crate::backup::data_key(state, actor, school, secret, "escrow_deposit").await?;
    let sealed = escrow::seal(&public_key, &dek, &escrow::context_for(school));
    dek.fill(0);
    let deposit = sealed.map_err(|_| {
        ApiError::new(
            axum::http::StatusCode::BAD_GATEWAY,
            "hub_refused",
            "The hub's escrow key could not be used. Ask head office to check the hub.",
        )
    })?;
    let key_id = deposit.key_id.clone();
    let http = http().map_err(refused)?;
    let after: EscrowStatus = send(
        http.put(format!("{}{HUB_API}/sync/escrow", link.hub_url))
            .json(&DepositRequest { deposit }),
        Some(&token),
    )
    .await
    .map_err(refused)?;
    let mut conn = state.pool.acquire().await?;
    audit::record(
        &mut conn,
        &state.hlc,
        actor,
        audit::Action::Create,
        "nexus_escrow",
        None,
        None,
        Some(serde_json::json!({ "hub": link.hub_name, "key_id": key_id })),
    )
    .await?;
    drop(conn);
    escrow_view(state, school, after).await
}

/// Take the school's key back from head office: the hub erases the sealed key.
pub async fn escrow_withdraw(state: &AppState, actor: Actor) -> ApiResult<EscrowView> {
    let school = school_id(state).await?;
    let (link, token) = escrow_link(state, school).await?;
    let http = http().map_err(refused)?;
    let after: EscrowStatus = send(
        http.delete(format!("{}{HUB_API}/sync/escrow", link.hub_url)),
        Some(&token),
    )
    .await
    .map_err(refused)?;
    let mut conn = state.pool.acquire().await?;
    audit::record(
        &mut conn,
        &state.hlc,
        actor,
        audit::Action::Delete,
        "nexus_escrow",
        None,
        Some(serde_json::json!({ "hub": link.hub_name })),
        None,
    )
    .await?;
    drop(conn);
    escrow_view(state, school, after).await
}

/// Write every release the school has not recorded yet into its own audit log, then tell the
/// hub. If telling fails the release is recorded again next time: twice is better than never.
async fn note_releases(
    state: &AppState,
    school: Uuid,
    link: &Link,
    token: &str,
) -> Result<usize, String> {
    let hub = hub_escrow(link, token).await.map_err(|e| e.to_string())?;
    let new: Vec<&ReleaseInfo> = hub.releases.iter().filter(|r| !r.seen).collect();
    if new.is_empty() {
        return Ok(0);
    }
    let mut conn = state.pool.acquire().await.map_err(|e| e.to_string())?;
    for release in &new {
        audit::record(
            &mut conn,
            &state.hlc,
            Actor::system_for(school),
            audit::Action::Update,
            "nexus_escrow",
            Some(release.id),
            None,
            Some(serde_json::json!({
                "event": "released_by_head_office", "hub": link.hub_name,
                "released_by": release.released_by, "released_at": release.released_at,
                "reason": release.reason, "key_id": release.key_id,
            })),
        )
        .await
        .map_err(|e| e.to_string())?;
    }
    drop(conn);
    tracing::warn!(
        count = new.len(),
        "head office released this school's backup key"
    );
    let http = http().map_err(|e| e.to_string())?;
    let _: EscrowStatus = send(
        http.post(format!("{}{HUB_API}/sync/escrow/seen", link.hub_url))
            .json(&SeenRequest {
                ids: new.iter().map(|r| r.id).collect(),
            }),
        Some(token),
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(new.len())
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
