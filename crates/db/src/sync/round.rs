//! One sync round as the school runs it: push, then pull, then acknowledge (ADR 0005). The
//! school always calls and the hub answers, so this drives a [`Transport`] the caller
//! supplies: HTTP in the school's API, the hub's router directly in tests.

use std::future::Future;

use madrasa_core::hlc::Hlc;
use madrasa_core::sync::{
    AckRequest, AckResponse, Parked, PullRequest, PullResponse, PushRequest, PushResponse,
    BATCH_SIZE,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    acknowledge, apply, collect, mark_overwrites_reported, schema_version, store_remote_overwrites,
    unreported_overwrites,
};
use crate::DbError;

/// Why a call to the hub did not succeed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransportError {
    /// The hub could not be reached (no network, wrong address, the hub is down).
    #[error("{0}")]
    Offline(String),
    /// Head office removed this school; the message says so in plain words.
    #[error("{0}")]
    Removed(String),
    /// The hub answered and refused (`code` is its error code).
    #[error("{message}")]
    Refused { code: String, message: String },
}

/// How a school reaches its hub.
pub trait Transport: Sync {
    fn push(
        &self,
        req: PushRequest,
    ) -> impl Future<Output = Result<PushResponse, TransportError>> + Send;
    fn pull(
        &self,
        req: PullRequest,
    ) -> impl Future<Output = Result<PullResponse, TransportError>> + Send;
    fn ack(
        &self,
        req: AckRequest,
    ) -> impl Future<Output = Result<AckResponse, TransportError>> + Send;
}

#[derive(Debug, thiserror::Error)]
pub enum RoundError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error(transparent)]
    Db(#[from] DbError),
}

impl From<sqlx::Error> for RoundError {
    fn from(e: sqlx::Error) -> Self {
        RoundError::Db(DbError::Sqlx(e))
    }
}

/// What one round did.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RoundReport {
    /// Records sent to the hub.
    pub sent: u32,
    /// Records received from the hub and applied here.
    pub received: u32,
    /// New overwrite-log entries from this round (both directions).
    pub overwrites: u32,
    /// Records waiting at the hub.
    pub parked_at_hub: Vec<Parked>,
    /// Records waiting here.
    pub parked_here: Vec<Parked>,
}

/// A safety stop for one round: a first upload of a very large school carries on in the
/// next round rather than holding one call open for an hour.
const MAX_BATCHES: usize = 2_000;

/// Run one round. Every step is its own short transaction, so an interrupted round leaves
/// nothing half-done: what was not acknowledged is simply sent again.
pub async fn run<T: Transport>(
    pool: &PgPool,
    hlc: &Hlc,
    app_version: &str,
    transport: &T,
) -> Result<RoundReport, RoundError> {
    let mut report = RoundReport::default();
    let schema_version = {
        let mut conn = pool.acquire().await?;
        schema_version(&mut conn).await?
    };
    // Hub log entries stored here that the hub has not been told about yet.
    let mut stored_here: Vec<Uuid> = Vec::new();

    // Push: what changed at the school.
    for _ in 0..MAX_BATCHES {
        let mut tx = pool.begin().await?;
        let changes = collect(&mut tx, BATCH_SIZE as i64).await?;
        let overwrites = unreported_overwrites(&mut tx, 200).await?;
        tx.commit().await?;
        let full = changes.len() == BATCH_SIZE;
        let sent = changes.len() as u32;
        let answer = transport
            .push(PushRequest {
                app_version: app_version.to_string(),
                schema_version,
                changes,
                overwrites,
                stored_overwrites: std::mem::take(&mut stored_here),
            })
            .await?;
        let mut tx = pool.begin().await?;
        acknowledge(&mut tx, &answer.handled).await?;
        let known_before = super::status(&mut tx).await?.overwrites;
        stored_here = store_remote_overwrites(&mut tx, &answer.overwrites).await?;
        report.overwrites += (super::status(&mut tx).await?.overwrites - known_before) as u32;
        mark_overwrites_reported(&mut tx, &answer.stored_overwrites).await?;
        tx.commit().await?;
        report.sent += sent;
        report.parked_at_hub = answer.parked;
        if !full {
            break;
        }
    }

    // Pull: what head office changed. An empty answer is still applied once, because that
    // is when records parked here are retried.
    for _ in 0..MAX_BATCHES {
        let answer = transport.pull(PullRequest { limit: BATCH_SIZE }).await?;
        let mut tx = pool.begin().await?;
        let applied = apply(&mut tx, hlc, &answer.changes).await?;
        let overwrites = unreported_overwrites(&mut tx, 200).await?;
        tx.commit().await?;
        report.received += applied.applied;
        report.overwrites += applied.overwrites.len() as u32;
        let confirmed = transport
            .ack(AckRequest {
                handled: applied.handled,
                overwrites,
                stored_overwrites: std::mem::take(&mut stored_here),
                parked: applied.parked.len() as u32,
            })
            .await?;
        let mut tx = pool.begin().await?;
        mark_overwrites_reported(&mut tx, &confirmed.stored_overwrites).await?;
        tx.commit().await?;
        report.parked_here = applied.parked;
        if !answer.more {
            break;
        }
    }
    Ok(report)
}
