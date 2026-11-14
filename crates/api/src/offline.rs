//! Working offline (ADR 0009): what the server does for changes that a LAN client queued
//! while the school server was out of reach, and sends when it is back.
//!
//! - Every queued change carries an `Idempotency-Key` (see [`idempotency`]): sent twice, it is
//!   applied once and the first answer is returned again.
//! - It also carries `X-Madrasa-Queued-At`, the time the user made the change. Routes that merge
//!   by time (attendance: the later mark stands) read it with [`Queued`].
//! - What a merge kept from the server instead of the queued value is returned in
//!   `X-Madrasa-Overwritten` (base64url JSON, [`Overwritten`]), so the client can show it.
//! - Other things the user should know (a receipt that made a voucher overpaid) come back in
//!   `X-Madrasa-Notice` ([`Notice`]).

pub mod idempotency;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;

pub const QUEUED_AT: HeaderName = HeaderName::from_static("x-madrasa-queued-at");
pub const OVERWRITTEN: HeaderName = HeaderName::from_static("x-madrasa-overwritten");
pub const NOTICE: HeaderName = HeaderName::from_static("x-madrasa-notice");

/// When the user made a change that a LAN client queued offline; `None` for a change made
/// online. A malformed value is refused rather than treated as online.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Queued(pub Option<DateTime<Utc>>);

impl<S: Send + Sync> FromRequestParts<S> for Queued {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let Some(v) = parts.headers.get(&QUEUED_AT) else {
            return Ok(Queued(None));
        };
        let at = v
            .to_str()
            .ok()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .ok_or_else(|| ApiError::validation("X-Madrasa-Queued-At must be an RFC 3339 time."))?;
        let at = at.with_timezone(&Utc);
        // A change cannot have been made in the future; a client clock running fast is pulled
        // back to now so it never wins a merge it should not.
        Ok(Queued(Some(at.min(Utc::now()))))
    }
}

/// A value the server kept instead of the one in a queued change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Overwritten {
    /// What it is about, for people: "Attendance of Ali Khan on 12-11-2026".
    pub label: String,
    /// The field, for programs (`status`, `phone`, ...).
    pub field: String,
    /// The value in the queued change, which did not stand.
    pub yours: serde_json::Value,
    /// The value that stands.
    pub kept: serde_json::Value,
}

/// Something about a queued change the user should know, by code (the UI words it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notice {
    /// `overpaid`: the receipt was recorded but the voucher was already paid meanwhile.
    pub code: String,
    #[serde(default)]
    pub detail: serde_json::Value,
}

fn encode<T: Serialize>(v: &T) -> Option<HeaderValue> {
    let json = serde_json::to_vec(v).ok()?;
    HeaderValue::from_str(&base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json)).ok()
}

/// Decode a header written by [`headers`] (the LAN proxy reads them).
pub fn decode<T: for<'de> Deserialize<'de>>(v: &HeaderValue) -> Option<T> {
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(v.as_bytes())
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Response headers carrying what was overwritten and any notices; empty when there is neither.
pub fn headers(overwritten: &[Overwritten], notices: &[Notice]) -> HeaderMap {
    let mut h = HeaderMap::new();
    if !overwritten.is_empty() {
        if let Some(v) = encode(&overwritten) {
            h.insert(OVERWRITTEN, v);
        }
    }
    if !notices.is_empty() {
        if let Some(v) = encode(&notices) {
            h.insert(NOTICE, v);
        }
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    async fn queued(value: Option<&str>) -> Result<Queued, ApiError> {
        let mut b = Request::builder();
        if let Some(v) = value {
            b = b.header("x-madrasa-queued-at", v);
        }
        let (mut parts, _) = b.body(()).unwrap().into_parts();
        Queued::from_request_parts(&mut parts, &()).await
    }

    #[tokio::test]
    async fn queued_time_is_read_checked_and_never_in_the_future() {
        assert_eq!(queued(None).await.unwrap(), Queued(None));
        let at = queued(Some("2026-11-12T08:30:00+05:00"))
            .await
            .unwrap()
            .0
            .unwrap();
        assert_eq!(at.to_rfc3339(), "2026-11-12T03:30:00+00:00");
        assert!(queued(Some("yesterday")).await.is_err());
        let future = queued(Some("2999-01-01T00:00:00Z"))
            .await
            .unwrap()
            .0
            .unwrap();
        assert!(future <= Utc::now());
    }

    #[test]
    fn overwritten_values_round_trip_through_a_header() {
        let list = vec![Overwritten {
            label: "Attendance of عائشہ on 12-11-2026".into(),
            field: "status".into(),
            yours: serde_json::json!("absent"),
            kept: serde_json::json!("present"),
        }];
        let h = headers(&list, &[]);
        assert!(h.get(&NOTICE).is_none());
        let back: Vec<Overwritten> = decode(h.get(&OVERWRITTEN).unwrap()).unwrap();
        assert_eq!(back, list);
        assert!(headers(&[], &[]).is_empty());
    }
}
