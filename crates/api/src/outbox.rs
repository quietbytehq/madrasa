//! The LAN client's outbox (ADR 0009): keep working when the school server drops.
//!
//! The desktop app in client mode reaches the school server through a loopback proxy
//! ([`crate::lan::serve_proxy`]). With an outbox the proxy also:
//!
//! - **keeps what the screens read.** Every JSON answer to a `GET` is kept, encrypted, per
//!   user; while the server is out of reach the last copy is served (`X-Madrasa-Offline:
//!   cached`), so screens keep showing what they showed.
//! - **queues changes.** A change the server cannot take is written to disk, encrypted, and
//!   answered `202` with `{ "queued": … }`. Changes are sent in the order they were made as
//!   soon as the server answers again; while anything is waiting, new changes join the end of
//!   the line so nothing overtakes.
//! - **never applies a change twice.** Every change carries an `Idempotency-Key` from the
//!   first attempt on (a change whose connection dropped half way is queued with the same key),
//!   and the server keeps its first answer ([`crate::offline::idempotency`]). The same change
//!   queued twice (Save pressed again) is kept once.
//! - **merges like sync.** A queued edit remembers the record as the user saw it. On sending,
//!   fields changed only on this PC are sent, fields changed only on the server are kept, and a
//!   field changed on both keeps the server's value (as head office's value wins in sync):
//!   the lost value is listed for the user, as are values the server reports it kept
//!   (`X-Madrasa-Overwritten`, attendance marks).
//! - **issues provisional receipts.** A fee payment taken offline gets a number
//!   `P-<PC>-<n>` at once; the real receipt keeps it and the outbox remembers which real number
//!   replaced it.
//!
//! Everything on disk is sealed with AES-256-GCM under a key made on first use and kept in the
//! same folder (owner-only on Linux): copies of the queue or cache files are unreadable without
//! it. It does not protect against someone who can sign in as the same Windows or Linux user.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use axum::body::{Body, Bytes};
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, Method, Response, StatusCode};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, Notify};
use uuid::Uuid;

use crate::offline::{self, idempotency};

/// Local endpoints of the proxy, never forwarded.
pub const LOCAL_PREFIX: &str = "/__madrasa/outbox";
pub const OFFLINE_HEADER: HeaderName = HeaderName::from_static("x-madrasa-offline");

/// Larger answers are not kept for offline reading; larger changes are not queued.
const MAX_CACHED: usize = 4 * 1024 * 1024;
const MAX_QUEUED: usize = 1024 * 1024;
/// After this long without an answer, "Reconnecting" becomes "Offline".
const RECONNECT_GRACE: i64 = 15;
/// How often the server is asked whether it is there while connected, and how long one such
/// question may take. Together four seconds, and the screen asks this PC every second, so a
/// pulled cable shows as "Reconnecting" within about five seconds even on an idle PC.
const HEARTBEAT: Duration = Duration::from_secs(2);
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(2);
/// While the server is known to be out of reach, a read waits at most this long for it
/// before the kept copy is served.
const AWAY_READ_TIMEOUT: Duration = Duration::from_secs(1);
/// How many past outcomes are kept for the user to read.
const KEEP_OUTCOMES: usize = 200;

/// The link to the school server, as the top bar shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Link {
    Connected,
    Reconnecting,
    Offline,
}

/// A change waiting to be sent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    /// Also its `Idempotency-Key`.
    pub id: Uuid,
    pub seq: u64,
    pub method: String,
    pub path: String,
    #[serde(with = "b64")]
    pub body: Vec<u8>,
    pub content_type: Option<String>,
    pub authorization: Option<String>,
    pub user_id: Option<Uuid>,
    /// When the user made the change (sent as `X-Madrasa-Queued-At`).
    pub queued_at: DateTime<Utc>,
    /// The record as the user saw it (the cached answer to `GET` of the same path), for the
    /// field merge of an edit.
    pub base: Option<Value>,
    pub provisional: Option<Provisional>,
    /// The change may already have reached the server (its connection dropped half way): it
    /// is sent again exactly as it was, and the server answers from its kept copy.
    pub sent_before: bool,
    pub attempts: u32,
    pub last_error: Option<String>,
}

/// A fee payment taken offline and the provisional receipt printed for it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Provisional {
    pub provisional_no: String,
    pub voucher_id: Uuid,
    /// The voucher as last read (student, class, amounts), for printing.
    pub voucher: Option<Value>,
    pub amount: Value,
    pub method: Value,
    pub reference: Value,
    pub note: Value,
    pub paid_on: String,
    pub taken_at: DateTime<Utc>,
    /// The real receipt number, once the server has it.
    pub receipt_no: Option<String>,
    pub receipt_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Failed {
    pub id: Uuid,
    pub method: String,
    pub path: String,
    pub queued_at: DateTime<Utc>,
    pub failed_at: DateTime<Utc>,
    pub status: u16,
    pub code: String,
    pub message: String,
    pub provisional_no: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverwrittenRecord {
    pub id: Uuid,
    pub item_id: Uuid,
    pub path: String,
    pub at: DateTime<Utc>,
    pub label: String,
    pub field: String,
    pub yours: Value,
    pub kept: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoticeRecord {
    pub id: Uuid,
    pub item_id: Uuid,
    pub at: DateTime<Utc>,
    pub code: String,
    pub detail: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sent {
    pub item_id: Uuid,
    pub method: String,
    pub path: String,
    pub queued_at: DateTime<Utc>,
    pub sent_at: DateTime<Utc>,
    pub provisional_no: Option<String>,
    pub receipt_no: Option<String>,
}

/// What the outbox remembers besides the waiting changes (`book.bin`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Book {
    next_seq: u64,
    next_provisional: u64,
    /// Four letters naming this PC on provisional receipts.
    device: String,
    failed: Vec<Failed>,
    overwritten: Vec<OverwrittenRecord>,
    notices: Vec<NoticeRecord>,
    sent: Vec<Sent>,
    receipts: Vec<Provisional>,
    /// Which user a session token belongs to (by the token's hash), learned from the answers
    /// to signing in and `/auth/me`; and each user's newest token, so a change queued under a
    /// session that has since expired is sent under the same person's new one, never another's.
    users: HashMap<String, Uuid>,
    latest: HashMap<Uuid, String>,
}

struct Inner {
    items: Vec<Item>,
    book: Book,
    link: Link,
    since: DateTime<Utc>,
    last_contact: Option<DateTime<Utc>>,
    needs_sign_in: Option<Uuid>,
}

/// What the top bar and the "waiting" panel read (`GET /__madrasa/outbox/status`).
#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub link: Link,
    pub since: DateTime<Utc>,
    pub last_contact: Option<DateTime<Utc>>,
    pub waiting: usize,
    pub needs_sign_in: bool,
    pub items: Vec<ItemView>,
    pub failed: Vec<Failed>,
    pub overwritten: Vec<OverwrittenRecord>,
    pub notices: Vec<NoticeRecord>,
    pub sent: Vec<Sent>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemView {
    pub id: Uuid,
    pub method: String,
    pub path: String,
    pub queued_at: DateTime<Utc>,
    pub provisional_no: Option<String>,
    /// A fee payment taken offline: the voucher it pays and the amount, so every screen that
    /// shows the voucher can say "paid (provisional, waiting to send)".
    pub voucher_id: Option<Uuid>,
    pub amount: Option<Value>,
    pub attempts: u32,
    pub last_error: Option<String>,
}

pub struct Outbox {
    dir: PathBuf,
    cipher: Aes256Gcm,
    inner: Mutex<Inner>,
    wake: Arc<Notify>,
}

mod b64 {
    use base64::Engine;
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&base64::engine::general_purpose::STANDARD.encode(v))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        base64::engine::general_purpose::STANDARD
            .decode(s)
            .map_err(serde::de::Error::custom)
    }
}

fn token_hash(authorization: &str) -> String {
    let mut h = Sha256::new();
    h.update(authorization.trim().as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

fn device_code() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let bytes = Uuid::new_v4().into_bytes();
    bytes[..4]
        .iter()
        .map(|b| ALPHABET[(*b as usize) % ALPHABET.len()] as char)
        .collect()
}

impl Outbox {
    /// Open (or create) the outbox in `dir`.
    pub fn open(dir: &Path) -> anyhow::Result<Arc<Outbox>> {
        crate::private::create_private_dir(dir)?;
        crate::private::create_private_dir(&dir.join("queue"))?;
        crate::private::create_private_dir(&dir.join("cache"))?;
        let key_path = dir.join("outbox.key");
        let key = match std::fs::read(&key_path) {
            Ok(k) if k.len() == 32 => k,
            _ => {
                let k = Aes256Gcm::generate_key(OsRng).to_vec();
                crate::lan::write_private(&key_path, &k)?;
                k
            }
        };
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key));
        let mut outbox = Outbox {
            dir: dir.to_path_buf(),
            cipher,
            inner: Mutex::new(Inner {
                items: Vec::new(),
                book: Book::default(),
                link: Link::Connected,
                since: Utc::now(),
                last_contact: None,
                needs_sign_in: None,
            }),
            wake: Arc::new(Notify::new()),
        };
        let book = outbox
            .read_sealed::<Book>(&dir.join("book.bin"))
            .unwrap_or_default();
        let mut items = Vec::new();
        for entry in std::fs::read_dir(dir.join("queue"))?.flatten() {
            match outbox.read_sealed::<Item>(&entry.path()) {
                Some(item) => items.push(item),
                None => {
                    tracing::warn!(file = %entry.path().display(), "an outbox file could not be read")
                }
            }
        }
        items.sort_by_key(|i| i.seq);
        {
            let inner = outbox.inner.get_mut();
            inner.book = book;
            if inner.book.device.is_empty() {
                inner.book.device = device_code();
            }
            if let Some(last) = items.last() {
                inner.book.next_seq = inner.book.next_seq.max(last.seq + 1);
            }
            inner.items = items;
        }
        let outbox = Arc::new(outbox);
        Ok(outbox)
    }

    fn seal(&self, plain: &[u8]) -> Vec<u8> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let mut out = nonce.to_vec();
        // Encryption with a valid key and nonce cannot fail.
        out.extend(self.cipher.encrypt(&nonce, plain).unwrap_or_default());
        out
    }

    fn unseal(&self, sealed: &[u8]) -> Option<Vec<u8>> {
        if sealed.len() < 12 {
            return None;
        }
        let (nonce, ct) = sealed.split_at(12);
        self.cipher.decrypt(Nonce::from_slice(nonce), ct).ok()
    }

    fn read_sealed<T: for<'de> Deserialize<'de>>(&self, path: &Path) -> Option<T> {
        let bytes = std::fs::read(path).ok()?;
        serde_json::from_slice(&self.unseal(&bytes)?).ok()
    }

    fn write_sealed<T: Serialize>(&self, path: &Path, value: &T) {
        let Ok(json) = serde_json::to_vec(value) else {
            return;
        };
        let tmp = path.with_extension("tmp");
        let ok = crate::lan::write_private(&tmp, &self.seal(&json))
            .and_then(|_| std::fs::rename(&tmp, path));
        if let Err(e) = ok {
            tracing::error!(error = %e, file = %path.display(), "could not write to the outbox");
        }
    }

    fn item_path(&self, item: &Item) -> PathBuf {
        self.dir
            .join("queue")
            .join(format!("{:012}-{}.bin", item.seq, item.id))
    }

    fn save_book(&self, book: &Book) {
        self.write_sealed(&self.dir.join("book.bin"), book);
    }

    fn cache_path(&self, user: &str, path: &str) -> PathBuf {
        let mut h = Sha256::new();
        h.update(user.as_bytes());
        h.update(b"\n");
        h.update(path.as_bytes());
        let name: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
        self.dir.join("cache").join(format!("{name}.bin"))
    }

    /// Who the cache is kept for: the user behind the token, or the token itself.
    async fn cache_owner(&self, authorization: Option<&str>) -> String {
        let Some(a) = authorization else {
            return "anonymous".into();
        };
        let hash = token_hash(a);
        let inner = self.inner.lock().await;
        inner
            .book
            .users
            .get(&hash)
            .map(|u| u.to_string())
            .unwrap_or(hash)
    }

    async fn cache_put(&self, authorization: Option<&str>, path: &str, body: &[u8]) {
        let owner = self.cache_owner(authorization).await;
        let file = self.cache_path(&owner, path);
        let sealed = self.seal(body);
        let _ = crate::lan::write_private(&file, &sealed);
    }

    async fn cache_get(&self, authorization: Option<&str>, path: &str) -> Option<Vec<u8>> {
        let owner = self.cache_owner(authorization).await;
        let bytes = std::fs::read(self.cache_path(&owner, path)).ok()?;
        self.unseal(&bytes)
    }

    /// Note the user of a token, from an answer to signing in or `/auth/me`.
    async fn learn_user(&self, authorization: Option<&str>, answer: &[u8]) {
        let Ok(v) = serde_json::from_slice::<Value>(answer) else {
            return;
        };
        let Some(user) = v
            .pointer("/user/id")
            .and_then(Value::as_str)
            .and_then(|s| Uuid::parse_str(s).ok())
        else {
            return;
        };
        // Signing in answers with the new token in the body.
        let token = v
            .get("token")
            .and_then(Value::as_str)
            .map(|t| format!("Bearer {t}"))
            .or_else(|| authorization.map(str::to_string));
        let Some(token) = token else { return };
        let mut inner = self.inner.lock().await;
        let hash = token_hash(&token);
        let fresh = inner.book.users.get(&hash) != Some(&user)
            || inner.book.latest.get(&user) != Some(&token);
        if fresh {
            inner.book.users.insert(hash, user);
            inner.book.latest.insert(user, token);
            if inner.needs_sign_in == Some(user) {
                inner.needs_sign_in = None;
                self.wake.notify_one();
            }
            self.save_book(&inner.book);
        }
    }

    /// The server answered.
    pub async fn reached(&self) {
        let mut inner = self.inner.lock().await;
        let now = Utc::now();
        inner.last_contact = Some(now);
        if inner.link != Link::Connected {
            inner.link = Link::Connected;
            inner.since = now;
            self.wake.notify_one();
        }
    }

    /// The server did not answer.
    pub async fn unreachable(&self) {
        let mut inner = self.inner.lock().await;
        let now = Utc::now();
        if inner.link == Link::Connected {
            inner.link = Link::Reconnecting;
            inner.since = now;
        }
        settle(&mut inner, now);
    }

    pub async fn link(&self) -> Link {
        let mut inner = self.inner.lock().await;
        settle(&mut inner, Utc::now());
        inner.link
    }

    pub async fn waiting(&self) -> usize {
        self.inner.lock().await.items.len()
    }

    pub async fn status(&self) -> Status {
        let mut inner = self.inner.lock().await;
        settle(&mut inner, Utc::now());
        Status {
            link: inner.link,
            since: inner.since,
            last_contact: inner.last_contact,
            waiting: inner.items.len(),
            needs_sign_in: inner.needs_sign_in.is_some(),
            items: inner
                .items
                .iter()
                .map(|i| ItemView {
                    id: i.id,
                    method: i.method.clone(),
                    path: i.path.clone(),
                    queued_at: i.queued_at,
                    provisional_no: i.provisional.as_ref().map(|p| p.provisional_no.clone()),
                    voucher_id: i.provisional.as_ref().map(|p| p.voucher_id),
                    amount: i.provisional.as_ref().map(|p| p.amount.clone()),
                    attempts: i.attempts,
                    last_error: i.last_error.clone(),
                })
                .collect(),
            failed: inner.book.failed.clone(),
            overwritten: inner.book.overwritten.clone(),
            notices: inner.book.notices.clone(),
            sent: inner.book.sent.iter().rev().take(50).cloned().collect(),
        }
    }

    /// A provisional receipt, by its number.
    pub async fn receipt(&self, provisional_no: &str) -> Option<Provisional> {
        let inner = self.inner.lock().await;
        inner
            .items
            .iter()
            .filter_map(|i| i.provisional.clone())
            .chain(inner.book.receipts.iter().cloned())
            .find(|p| p.provisional_no == provisional_no)
    }

    /// Forget failed changes, overwritten values or notices the user has read.
    pub async fn dismiss(&self, kind: &str, id: Option<Uuid>) {
        let mut inner = self.inner.lock().await;
        let keep = |x: Uuid| id.is_some_and(|id| id != x);
        match kind {
            "failed" => inner.book.failed.retain(|f| keep(f.id)),
            "overwritten" => inner.book.overwritten.retain(|o| keep(o.id)),
            "notices" => inner.book.notices.retain(|n| keep(n.id)),
            _ => return,
        }
        self.save_book(&inner.book);
    }

    /// Try to send now.
    pub fn nudge(&self) {
        self.wake.notify_one();
    }

    /// Queue a change. The same change already waiting (Save pressed twice) is not queued
    /// again; its entry is returned.
    #[allow(clippy::too_many_arguments)]
    async fn enqueue(
        &self,
        id: Uuid,
        method: &Method,
        path: &str,
        mut body: Vec<u8>,
        content_type: Option<String>,
        authorization: Option<String>,
        sent_before: bool,
    ) -> Item {
        let base = if matches!(*method, Method::PUT | Method::PATCH) && !sent_before {
            self.cache_get(authorization.as_deref(), path)
                .await
                .and_then(|b| serde_json::from_slice(&b).ok())
        } else {
            None
        };
        let voucher = match payment_voucher(method, path) {
            Some(v) if !sent_before => Some(v),
            _ => None,
        };
        let voucher_json = match voucher {
            Some(v) => self
                .cache_get(
                    authorization.as_deref(),
                    &format!("/api/v1/fees/vouchers/{v}"),
                )
                .await
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok()),
            None => None,
        };
        let now = Utc::now();
        let mut inner = self.inner.lock().await;
        let user_id = authorization
            .as_deref()
            .and_then(|a| inner.book.users.get(&token_hash(a)).copied());
        if let Some(same) = inner.items.iter().find(|i| {
            i.method == method.as_str()
                && i.path == path
                && i.authorization == authorization
                && (i.body == body || i.provisional.is_some() && same_payment(&i.body, &body))
        }) {
            return same.clone();
        }
        let provisional = match voucher {
            Some(voucher_id) => {
                inner.book.next_provisional += 1;
                let provisional_no =
                    format!("P-{}-{:04}", inner.book.device, inner.book.next_provisional);
                let mut v: Value = serde_json::from_slice(&body).unwrap_or_else(|_| json!({}));
                let paid_on = v
                    .get("paid_on")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| chrono::Local::now().date_naive().to_string());
                if let Some(o) = v.as_object_mut() {
                    o.insert("provisional_no".into(), json!(provisional_no));
                    o.insert("paid_on".into(), json!(paid_on));
                }
                body = serde_json::to_vec(&v).unwrap_or(body);
                Some(Provisional {
                    provisional_no,
                    voucher_id,
                    voucher: voucher_json,
                    amount: v.get("amount").cloned().unwrap_or(Value::Null),
                    method: v.get("method").cloned().unwrap_or(Value::Null),
                    reference: v.get("reference").cloned().unwrap_or(Value::Null),
                    note: v.get("note").cloned().unwrap_or(Value::Null),
                    paid_on,
                    taken_at: now,
                    receipt_no: None,
                    receipt_id: None,
                })
            }
            None => None,
        };
        let item = Item {
            id,
            seq: inner.book.next_seq,
            method: method.to_string(),
            path: path.to_string(),
            body,
            content_type,
            authorization,
            user_id,
            queued_at: now,
            base,
            provisional,
            sent_before,
            attempts: 0,
            last_error: None,
        };
        inner.book.next_seq += 1;
        self.write_sealed(&self.item_path(&item), &item);
        self.save_book(&inner.book);
        inner.items.push(item.clone());
        item
    }

    /// Send what is waiting, in order, until the line is empty or the server stops answering.
    /// Returns how many changes left the queue.
    pub async fn flush(&self, client: &reqwest::Client, upstream: &str) -> usize {
        let mut done = 0;
        loop {
            let (mut item, token) = {
                let inner = self.inner.lock().await;
                if inner.needs_sign_in.is_some() {
                    return done;
                }
                let Some(item) = inner.items.first().cloned() else {
                    return done;
                };
                // Prefer the same person's newest session; never another person's.
                let token = item
                    .user_id
                    .and_then(|u| inner.book.latest.get(&u).cloned())
                    .or_else(|| item.authorization.clone());
                (item, token)
            };
            // An edit not yet sent: merge with the record as it is now, once, and keep the
            // merged change so every later attempt sends exactly the same thing.
            let mut lost = Vec::new();
            if !item.sent_before && matches!(item.method.as_str(), "PUT" | "PATCH") {
                if let Some(base) = item.base.clone() {
                    match get_json(client, upstream, &item.path, token.as_deref()).await {
                        Ok(Some(current)) => {
                            let (merged, l) = merge_fields(&item.body, &base, &current);
                            item.body = merged;
                            lost = l;
                        }
                        Ok(None) => {}
                        Err(()) => {
                            self.unreachable().await;
                            return done;
                        }
                    }
                }
            }
            if !item.sent_before {
                item.sent_before = true;
                item.base = None;
                let mut inner = self.inner.lock().await;
                if let Some(slot) = inner.items.iter_mut().find(|i| i.id == item.id) {
                    *slot = item.clone();
                }
                self.write_sealed(&self.item_path(&item), &item);
                drop(inner);
            }
            let Ok(method) = Method::from_bytes(item.method.as_bytes()) else {
                self.drop_item(&item, 400, "bad_method", "This change could not be read.")
                    .await;
                continue;
            };
            let mut rb = client
                .request(method, format!("{upstream}{}", item.path))
                .header(idempotency::HEADER, item.id.to_string())
                .header(offline::QUEUED_AT, item.queued_at.to_rfc3339())
                .body(item.body.clone());
            if let Some(t) = &item.content_type {
                rb = rb.header(header::CONTENT_TYPE, t);
            }
            if let Some(t) = &token {
                rb = rb.header(header::AUTHORIZATION, t);
            }
            let resp = match rb.send().await {
                Ok(r) => r,
                Err(e) => {
                    self.note_attempt(&item, &e.to_string()).await;
                    self.unreachable().await;
                    return done;
                }
            };
            self.reached().await;
            let status = resp.status();
            let headers = resp.headers().clone();
            let body = resp.bytes().await.unwrap_or_default();
            let (code, message) = error_of(&body, status);
            if status.is_success() {
                self.complete(&item, &headers, &body, lost).await;
                done += 1;
            } else if status == StatusCode::UNAUTHORIZED {
                let mut inner = self.inner.lock().await;
                inner.needs_sign_in = Some(item.user_id.unwrap_or_else(Uuid::nil));
                return done;
            } else if status.is_server_error()
                || status == StatusCode::TOO_MANY_REQUESTS
                || code == "idempotency_in_progress"
            {
                self.note_attempt(&item, &message).await;
                return done;
            } else {
                self.drop_item(&item, status.as_u16(), &code, &message)
                    .await;
            }
        }
    }

    async fn note_attempt(&self, item: &Item, error: &str) {
        let mut inner = self.inner.lock().await;
        if let Some(slot) = inner.items.iter_mut().find(|i| i.id == item.id) {
            slot.attempts += 1;
            slot.last_error = Some(error.to_string());
            let copy = slot.clone();
            self.write_sealed(&self.item_path(&copy), &copy);
        }
    }

    fn remove(&self, inner: &mut Inner, item: &Item) {
        inner.items.retain(|i| i.id != item.id);
        let _ = std::fs::remove_file(self.item_path(item));
    }

    /// The server refused the change: it leaves the line and is listed for the user.
    async fn drop_item(&self, item: &Item, status: u16, code: &str, message: &str) {
        let mut inner = self.inner.lock().await;
        self.remove(&mut inner, item);
        inner.book.failed.push(Failed {
            id: item.id,
            method: item.method.clone(),
            path: item.path.clone(),
            queued_at: item.queued_at,
            failed_at: Utc::now(),
            status,
            code: code.to_string(),
            message: message.to_string(),
            provisional_no: item.provisional.as_ref().map(|p| p.provisional_no.clone()),
        });
        trim(&mut inner.book.failed);
        self.save_book(&inner.book);
    }

    async fn complete(
        &self,
        item: &Item,
        headers: &reqwest::header::HeaderMap,
        body: &[u8],
        mut lost: Vec<offline::Overwritten>,
    ) {
        if let Some(list) = headers
            .get(offline::OVERWRITTEN.as_str())
            .and_then(|v| HeaderValue::from_bytes(v.as_bytes()).ok())
            .and_then(|v| offline::decode::<Vec<offline::Overwritten>>(&v))
        {
            lost.extend(list);
        }
        let notices = headers
            .get(offline::NOTICE.as_str())
            .and_then(|v| HeaderValue::from_bytes(v.as_bytes()).ok())
            .and_then(|v| offline::decode::<Vec<offline::Notice>>(&v))
            .unwrap_or_default();
        let answer: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
        let now = Utc::now();
        let mut inner = self.inner.lock().await;
        self.remove(&mut inner, item);
        let mut receipt_no = None;
        if let Some(mut p) = item.provisional.clone() {
            receipt_no = answer
                .pointer("/receipt/receipt_no")
                .and_then(Value::as_str)
                .map(str::to_string);
            p.receipt_no = receipt_no.clone();
            p.receipt_id = answer
                .pointer("/receipt/id")
                .and_then(Value::as_str)
                .and_then(|s| Uuid::parse_str(s).ok());
            inner.book.receipts.push(p);
            trim(&mut inner.book.receipts);
        }
        for o in lost {
            inner.book.overwritten.push(OverwrittenRecord {
                id: Uuid::now_v7(),
                item_id: item.id,
                path: item.path.clone(),
                at: now,
                label: o.label,
                field: o.field,
                yours: o.yours,
                kept: o.kept,
            });
        }
        trim(&mut inner.book.overwritten);
        for n in notices {
            inner.book.notices.push(NoticeRecord {
                id: Uuid::now_v7(),
                item_id: item.id,
                at: now,
                code: n.code,
                detail: n.detail,
            });
        }
        trim(&mut inner.book.notices);
        inner.book.sent.push(Sent {
            item_id: item.id,
            method: item.method.clone(),
            path: item.path.clone(),
            queued_at: item.queued_at,
            sent_at: now,
            provisional_no: item.provisional.as_ref().map(|p| p.provisional_no.clone()),
            receipt_no,
        });
        trim(&mut inner.book.sent);
        self.save_book(&inner.book);
    }

    /// Keep checking the server and send what waits whenever it answers. Stops when the proxy
    /// that owns the outbox is gone (only this task would still hold it).
    pub fn spawn_sender(self: &Arc<Self>, client: reqwest::Client, upstream: String) {
        let weak = Arc::downgrade(self);
        let wake = Arc::clone(&self.wake);
        tokio::spawn(async move {
            loop {
                let Some(me) = weak.upgrade() else { return };
                let pause = match me.link().await {
                    Link::Connected => HEARTBEAT,
                    _ => Duration::from_secs(1),
                };
                // Wait without keeping the outbox alive.
                drop(me);
                let _ = tokio::time::timeout(pause, wake.notified()).await;
                let Some(me) = weak.upgrade() else { return };
                let health = client
                    .get(format!("{upstream}/api/v1/health"))
                    .timeout(HEARTBEAT_TIMEOUT)
                    .send()
                    .await;
                match health {
                    Ok(r) if r.status().is_success() => {
                        me.reached().await;
                        if me.waiting().await > 0 {
                            me.flush(&client, &upstream).await;
                        }
                    }
                    _ => me.unreachable().await,
                }
            }
        });
    }
}

/// "Reconnecting" becomes "Offline" by the clock: fifteen seconds after the server's last
/// answer (or, before any answer, after trouble started), whenever anyone looks, not only
/// when the next attempt happens to fail.
fn settle(inner: &mut Inner, now: DateTime<Utc>) {
    if inner.link != Link::Reconnecting {
        return;
    }
    let silent_since = inner.last_contact.unwrap_or(inner.since);
    if (now - silent_since).num_seconds() >= RECONNECT_GRACE {
        inner.link = Link::Offline;
    }
}

fn trim<T>(v: &mut Vec<T>) {
    if v.len() > KEEP_OUTCOMES {
        let extra = v.len() - KEEP_OUTCOMES;
        v.drain(..extra);
    }
}

/// Two payments are the same if everything but the provisional number and date (added when
/// queued) matches.
fn same_payment(queued: &[u8], new: &[u8]) -> bool {
    let strip = |b: &[u8]| -> Option<Value> {
        let mut v: Value = serde_json::from_slice(b).ok()?;
        let o = v.as_object_mut()?;
        o.remove("provisional_no");
        o.remove("paid_on");
        Some(v)
    };
    match (strip(queued), strip(new)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// `POST /api/v1/fees/vouchers/{id}/payments`: the voucher being paid.
fn payment_voucher(method: &Method, path: &str) -> Option<Uuid> {
    if *method != Method::POST {
        return None;
    }
    let rest = path.strip_prefix("/api/v1/fees/vouchers/")?;
    let id = rest.strip_suffix("/payments")?;
    Uuid::parse_str(id).ok()
}

fn error_of(body: &[u8], status: StatusCode) -> (String, String) {
    let v: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    let code = v
        .pointer("/error/code")
        .and_then(Value::as_str)
        .unwrap_or("error")
        .to_string();
    let message = v
        .pointer("/error/message")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| status.canonical_reason().unwrap_or("Refused").to_string());
    (code, message)
}

async fn get_json(
    client: &reqwest::Client,
    upstream: &str,
    path: &str,
    token: Option<&str>,
) -> Result<Option<Value>, ()> {
    let mut rb = client.get(format!("{upstream}{path}"));
    if let Some(t) = token {
        rb = rb.header(header::AUTHORIZATION, t);
    }
    let resp = rb.send().await.map_err(|_| ())?;
    if !resp.status().is_success() {
        return Ok(None);
    }
    Ok(resp.json::<Value>().await.ok())
}

/// Look a field up at the top of a record, or one level down (`{ "student": { … } }`).
fn field<'a>(record: &'a Value, key: &str) -> Option<&'a Value> {
    let o = record.as_object()?;
    o.get(key).or_else(|| {
        o.values()
            .filter_map(Value::as_object)
            .find_map(|inner| inner.get(key))
    })
}

/// The field merge of a queued edit (ADR 0009, after ADR 0005): `body` is the change as made,
/// `base` the record as the user saw it, `current` the record now. Returns the body to send
/// and what of the user's change did not stand.
pub fn merge_fields(
    body: &[u8],
    base: &Value,
    current: &Value,
) -> (Vec<u8>, Vec<offline::Overwritten>) {
    let Ok(Value::Object(mut ours)) = serde_json::from_slice::<Value>(body) else {
        return (body.to_vec(), Vec::new());
    };
    let mut lost = Vec::new();
    let label = record_label(current);
    for (key, mine) in ours.iter_mut() {
        let (Some(was), Some(now)) = (field(base, key), field(current, key)) else {
            continue;
        };
        if now == was || now == mine {
            continue; // the server did not change it, or agrees
        }
        if mine != was {
            // Changed on both sides: the server's value stands; the user's is listed.
            lost.push(offline::Overwritten {
                label: label.clone(),
                field: key.clone(),
                yours: mine.clone(),
                kept: now.clone(),
            });
        }
        // Changed only on the server (or on both): keep the server's value.
        *mine = now.clone();
    }
    (
        serde_json::to_vec(&Value::Object(ours)).unwrap_or_else(|_| body.to_vec()),
        lost,
    )
}

fn record_label(record: &Value) -> String {
    for key in ["full_name", "name", "title", "voucher_no", "application_no"] {
        if let Some(s) = field(record, key).and_then(Value::as_str) {
            return s.to_string();
        }
    }
    String::new()
}

// ---- the proxy's side ---------------------------------------------------------------------

fn json_response(status: StatusCode, value: &Value) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(value.to_string()))
        .unwrap_or_default()
}

/// The path without its query string.
fn bare(path: &str) -> &str {
    path.split('?').next().unwrap_or(path)
}

/// Paths whose changes need the server there and then; never queued. Signing in, setup,
/// backups and imports; settings that hold credentials or reach other systems; anything that
/// creates a login or sets a password (a password never waits in a file); and whole-school
/// jobs whose result the user has to see (rollover, voucher runs, timetable generation).
fn needs_server(path: &str) -> bool {
    const LIVE: &[&str] = &[
        "/api/v1/auth/",
        "/api/v1/setup",
        "/api/v1/backup",
        "/api/v1/imports",
        "/api/v1/system",
        "/api/v1/edition",
        "/api/v1/nexus",
        "/api/v1/settings/directory",
        "/api/v1/settings/api-keys",
        "/api/v1/users",
        "/api/v1/parent-app/access",
        "/api/v1/academic-years/rollover",
        "/api/v1/fees/vouchers/generate",
        "/api/v1/fees/late-fees/run",
        "/api/v1/timetable/generate",
    ];
    let path = bare(path);
    if LIVE.iter().any(|p| path.starts_with(p)) {
        return true;
    }
    // `/api/v1/staff/{id}/login` and `/login/password`.
    path.strip_prefix("/api/v1/staff/")
        .and_then(|rest| rest.split_once('/'))
        .is_some_and(|(_, tail)| tail == "login" || tail.starts_with("login/"))
}

/// Requests that only read or check although they are sent as `POST` (a duplicate check, a
/// preview, a file being read before an import): they change nothing, so they are never
/// queued and never counted as waiting. Without the server they answer `offline_check`.
fn is_check(method: &Method, path: &str) -> bool {
    if *method != Method::POST {
        return false;
    }
    const CHECKS: &[&str] = &[
        "/api/v1/students/check-duplicates",
        "/api/v1/allocation/balance",
        "/api/v1/reports/custom/run",
        "/api/v1/subject-allocations/import/parse",
    ];
    let path = bare(path);
    CHECKS.contains(&path)
        || (path.starts_with("/api/v1/boards/exams/")
            && (path.ends_with("/results/parse") || path.ends_with("/results/preview")))
}

/// Answer the browser's CORS preflight ourselves, so changes can be made while the server is
/// away (the server allows any origin too).
pub fn preflight(req_headers: &HeaderMap) -> Response<Body> {
    let mut r = Response::builder()
        .status(StatusCode::NO_CONTENT)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            "GET, POST, PUT, PATCH, DELETE, OPTIONS",
        )
        .header(header::ACCESS_CONTROL_MAX_AGE, "600");
    let allow = req_headers
        .get(header::ACCESS_CONTROL_REQUEST_HEADERS)
        .cloned()
        .unwrap_or_else(|| HeaderValue::from_static("*"));
    r = r.header(header::ACCESS_CONTROL_ALLOW_HEADERS, allow);
    r.body(Body::empty()).unwrap_or_default()
}

/// The proxy's local endpoints under [`LOCAL_PREFIX`].
pub async fn local(outbox: &Outbox, method: &Method, path: &str, body: &[u8]) -> Response<Body> {
    let rest = path.strip_prefix(LOCAL_PREFIX).unwrap_or("");
    let rest = rest.split('?').next().unwrap_or("");
    match (method.as_str(), rest) {
        ("GET", "/status") => json_response(StatusCode::OK, &json!(outbox.status().await)),
        ("POST", "/retry") => {
            outbox.nudge();
            json_response(StatusCode::ACCEPTED, &json!({ "ok": true }))
        }
        ("POST", "/dismiss") => {
            let v: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
            let kind = v.get("kind").and_then(Value::as_str).unwrap_or("");
            let id = v
                .get("id")
                .and_then(Value::as_str)
                .and_then(|s| Uuid::parse_str(s).ok());
            outbox.dismiss(kind, id).await;
            json_response(StatusCode::OK, &json!(outbox.status().await))
        }
        ("GET", r) if r.starts_with("/receipts/") => {
            match outbox.receipt(&r["/receipts/".len()..]).await {
                Some(p) => json_response(StatusCode::OK, &json!(p)),
                None => json_response(
                    StatusCode::NOT_FOUND,
                    &json!({ "error": { "code": "not_found", "message": "No provisional receipt with this number on this PC." } }),
                ),
            }
        }
        _ => json_response(
            StatusCode::NOT_FOUND,
            &json!({ "error": { "code": "not_found", "message": "Not found." } }),
        ),
    }
}

/// How a request went upstream, for [`Outbox::after`].
pub enum Upstream {
    /// The server answered (any status).
    Answered {
        status: StatusCode,
        headers: HeaderMap,
        body: Bytes,
    },
    /// It could not be reached; the request never left this PC.
    NotSent(String),
    /// It was sent but the answer was lost: it may have been applied.
    Lost(String),
}

/// One request through the proxy with an outbox: `send` performs it upstream (adding
/// `Idempotency-Key: key` to changes).
pub async fn handle<F, Fut>(
    outbox: &Arc<Outbox>,
    method: Method,
    path: String,
    headers: &HeaderMap,
    body: Bytes,
    send: F,
) -> Response<Body>
where
    F: FnOnce(Option<Uuid>) -> Fut,
    Fut: std::future::Future<Output = Upstream>,
{
    let authorization = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let is_change = matches!(
        method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );
    let check = is_check(&method, &path);
    let queueable = is_change && !check && !needs_server(&path) && body.len() <= MAX_QUEUED;
    let away = outbox.link().await != Link::Connected;

    // A check changes nothing: with the server away it is not asked, not queued, not counted.
    if check && away {
        return offline_check_response();
    }

    // While changes wait, new ones join the line so nothing overtakes; and while the server
    // is known to be away, a change goes straight to the line instead of waiting for a
    // connection that will not come.
    if queueable && (away || outbox.waiting().await > 0) {
        let item = outbox
            .enqueue(
                Uuid::now_v7(),
                &method,
                &path,
                body.to_vec(),
                content_type,
                authorization,
                false,
            )
            .await;
        outbox.nudge();
        return queued_response(outbox, &item).await;
    }

    // A read while the server is away: the kept copy at once when "Offline" is known, and
    // after a short wait while still "Reconnecting" (the heartbeat notices its return).
    let reading_away = away && method == Method::GET;
    if reading_away && outbox.link().await == Link::Offline {
        if let Some(bytes) = outbox.cache_get(authorization.as_deref(), &path).await {
            return cached_response(bytes);
        }
    }

    let key = (is_change && !check).then(Uuid::now_v7);
    let upstream = if reading_away {
        match tokio::time::timeout(AWAY_READ_TIMEOUT, send(key)).await {
            Ok(answer) => answer,
            Err(_) => Upstream::NotSent("the school server did not answer in time".into()),
        }
    } else {
        send(key).await
    };
    match upstream {
        Upstream::Answered {
            status,
            headers: rheaders,
            body: rbody,
        } => {
            outbox.reached().await;
            if status.is_success() {
                if method == Method::GET
                    && rbody.len() <= MAX_CACHED
                    && rheaders
                        .get(header::CONTENT_TYPE)
                        .and_then(|v| v.to_str().ok())
                        .is_some_and(|t| t.starts_with("application/json"))
                {
                    outbox
                        .cache_put(authorization.as_deref(), &path, &rbody)
                        .await;
                }
                if (method == Method::GET && path.starts_with("/api/v1/auth/me"))
                    || (method == Method::POST && path.starts_with("/api/v1/auth/login"))
                {
                    outbox.learn_user(authorization.as_deref(), &rbody).await;
                }
            }
            let mut r = Response::builder().status(status);
            for (name, value) in rheaders.iter() {
                r = r.header(name, value);
            }
            r.body(Body::from(rbody)).unwrap_or_default()
        }
        Upstream::NotSent(why) | Upstream::Lost(why) if method == Method::GET => {
            outbox.unreachable().await;
            match outbox.cache_get(authorization.as_deref(), &path).await {
                Some(bytes) => cached_response(bytes),
                None => unreachable_response(&why),
            }
        }
        Upstream::NotSent(_) | Upstream::Lost(_) if check => {
            outbox.unreachable().await;
            offline_check_response()
        }
        Upstream::NotSent(why) => {
            outbox.unreachable().await;
            if !queueable {
                return unreachable_response(&why);
            }
            let item = outbox
                .enqueue(
                    key.unwrap_or_else(Uuid::now_v7),
                    &method,
                    &path,
                    body.to_vec(),
                    content_type,
                    authorization,
                    false,
                )
                .await;
            queued_response(outbox, &item).await
        }
        Upstream::Lost(why) => {
            outbox.unreachable().await;
            if !queueable {
                return unreachable_response(&why);
            }
            // It may have been applied: queue it unchanged under the same key; the server
            // answers the next attempt from its kept copy if it was.
            let item = outbox
                .enqueue(
                    key.unwrap_or_else(Uuid::now_v7),
                    &method,
                    &path,
                    body.to_vec(),
                    content_type,
                    authorization,
                    true,
                )
                .await;
            queued_response(outbox, &item).await
        }
    }
}

/// The kept copy of a read, marked as such.
fn cached_response(bytes: Vec<u8>) -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(OFFLINE_HEADER, "cached")
        .header(header::ACCESS_CONTROL_EXPOSE_HEADERS, "x-madrasa-offline")
        .body(Body::from(bytes))
        .unwrap_or_default()
}

/// A check that could not be made because the server is away. Nothing was queued.
fn offline_check_response() -> Response<Body> {
    json_response(
        StatusCode::SERVICE_UNAVAILABLE,
        &json!({ "error": {
            "code": "offline_check",
            "message": "This cannot be checked right now: the school server is out of reach. It is checked when your change arrives there.",
        } }),
    )
}

fn unreachable_response(why: &str) -> Response<Body> {
    json_response(
        StatusCode::SERVICE_UNAVAILABLE,
        &json!({ "error": {
            "code": "server_unreachable",
            "message": "The school server cannot be reached, and this needs it. Try again when it is back.",
            "details": why,
        } }),
    )
}

async fn queued_response(outbox: &Outbox, item: &Item) -> Response<Body> {
    let waiting = outbox.waiting().await;
    json_response(
        StatusCode::ACCEPTED,
        &json!({ "queued": {
            "id": item.id,
            "queued_at": item.queued_at,
            "waiting": waiting,
            "provisional": item.provisional,
        } }),
    )
}

impl Outbox {
    /// Is `s` a provisional receipt number as this outbox makes them?
    pub fn is_provisional(s: &str) -> bool {
        madrasa_db::fees::is_provisional_no(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_merge_field_by_field_and_the_server_wins_a_clash() {
        let base = json!({ "student": { "full_name": "Ali", "phone": "0300", "address": "A" } });
        let current = json!({ "student": { "full_name": "Ali", "phone": "0311", "address": "B" } });
        let body = json!({ "full_name": "Ali Khan", "phone": "0322", "address": "A" });
        let (merged, lost) = merge_fields(body.to_string().as_bytes(), &base, &current);
        let merged: Value = serde_json::from_slice(&merged).unwrap();
        // Only here: kept. Only on the server: kept. On both: the server's.
        assert_eq!(merged["full_name"], "Ali Khan");
        assert_eq!(merged["address"], "B");
        assert_eq!(merged["phone"], "0311");
        assert_eq!(lost.len(), 1);
        assert_eq!(lost[0].field, "phone");
        assert_eq!(lost[0].yours, "0322");
        assert_eq!(lost[0].kept, "0311");
        assert_eq!(lost[0].label, "Ali");
    }

    #[test]
    fn payments_and_their_vouchers_are_recognised() {
        let v = Uuid::now_v7();
        let path = format!("/api/v1/fees/vouchers/{v}/payments");
        assert_eq!(payment_voucher(&Method::POST, &path), Some(v));
        assert_eq!(payment_voucher(&Method::GET, &path), None);
        assert_eq!(payment_voucher(&Method::POST, "/api/v1/students"), None);
        assert!(same_payment(
            br#"{"amount":"500","method":"cash","provisional_no":"P-ABCD-0001","paid_on":"2026-11-13"}"#,
            br#"{"amount":"500","method":"cash"}"#
        ));
        assert!(!same_payment(
            br#"{"amount":"500"}"#,
            br#"{"amount":"600"}"#
        ));
        assert!(needs_server("/api/v1/auth/login"));
        assert!(!needs_server("/api/v1/students"));
    }

    #[tokio::test]
    async fn what_is_kept_on_disk_is_sealed_and_survives_a_restart() {
        let dir = std::env::temp_dir().join(format!("madrasa-outbox-{}", Uuid::now_v7()));
        let outbox = Outbox::open(&dir).unwrap();
        let v = Uuid::now_v7();
        let path = format!("/api/v1/fees/vouchers/{v}/payments");
        let body = br#"{"amount":"1500.00","method":"cash","note":"Secret Sara Ahmed"}"#.to_vec();
        let a = outbox
            .enqueue(
                Uuid::now_v7(),
                &Method::POST,
                &path,
                body.clone(),
                None,
                Some("Bearer t".into()),
                false,
            )
            .await;
        let p = a.provisional.clone().unwrap();
        assert!(crate::outbox::Outbox::is_provisional(&p.provisional_no));
        // The same payment again (Save pressed twice): kept once.
        let b = outbox
            .enqueue(
                Uuid::now_v7(),
                &Method::POST,
                &path,
                body,
                None,
                Some("Bearer t".into()),
                false,
            )
            .await;
        assert_eq!(a.id, b.id);
        assert_eq!(outbox.waiting().await, 1);
        outbox
            .cache_put(
                Some("Bearer t"),
                "/api/v1/students",
                br#"[{"full_name":"Secret Sara Ahmed"}]"#,
            )
            .await;
        // Nothing on disk shows the contents.
        for sub in ["queue", "cache"] {
            for f in std::fs::read_dir(dir.join(sub)).unwrap().flatten() {
                let bytes = std::fs::read(f.path()).unwrap();
                assert!(!String::from_utf8_lossy(&bytes).contains("Secret Sara"));
            }
        }
        drop(outbox);
        let again = Outbox::open(&dir).unwrap();
        assert_eq!(again.waiting().await, 1);
        assert_eq!(
            again.receipt(&p.provisional_no).await.unwrap().amount,
            "1500.00"
        );
        assert!(again
            .cache_get(Some("Bearer t"), "/api/v1/students")
            .await
            .is_some());
        // Another key cannot read it.
        std::fs::write(dir.join("outbox.key"), [7u8; 32]).unwrap();
        let wrong = Outbox::open(&dir).unwrap();
        assert_eq!(wrong.waiting().await, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// One request through the proxy; `answer` is what the school server would do with it,
    /// and `asked` counts how often it was asked at all.
    async fn through(
        outbox: &Arc<Outbox>,
        method: Method,
        path: &str,
        body: &str,
        asked: &std::sync::atomic::AtomicUsize,
        answer: Option<&str>,
    ) -> (StatusCode, Value, Option<String>) {
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer t"));
        let answer = answer.map(str::to_string);
        let r = handle(
            outbox,
            method,
            path.to_string(),
            &headers,
            Bytes::from(body.to_string()),
            |_key| async move {
                asked.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                match answer {
                    Some(a) => {
                        let mut h = HeaderMap::new();
                        h.insert(
                            header::CONTENT_TYPE,
                            HeaderValue::from_static("application/json"),
                        );
                        Upstream::Answered {
                            status: StatusCode::OK,
                            headers: h,
                            body: Bytes::from(a),
                        }
                    }
                    None => {
                        // A server that is silently out of reach: no answer for a long time.
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        Upstream::NotSent("no route".into())
                    }
                }
            },
        )
        .await;
        let status = r.status();
        let offline = r
            .headers()
            .get(OFFLINE_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let bytes = axum::body::to_bytes(r.into_body(), usize::MAX)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            offline,
        )
    }

    fn temp_outbox() -> (Arc<Outbox>, PathBuf) {
        let dir = std::env::temp_dir().join(format!("madrasa-outbox-{}", Uuid::now_v7()));
        (Outbox::open(&dir).unwrap(), dir)
    }

    async fn force_link(outbox: &Outbox, link: Link, silent_for: i64) {
        let mut inner = outbox.inner.lock().await;
        inner.link = link;
        inner.since = Utc::now() - chrono::Duration::seconds(silent_for);
        inner.last_contact = Some(Utc::now() - chrono::Duration::seconds(silent_for));
    }

    #[test]
    fn checks_and_reads_sent_as_post_are_told_apart_from_changes() {
        for path in [
            "/api/v1/students/check-duplicates",
            "/api/v1/students/check-duplicates?x=1",
            "/api/v1/allocation/balance",
            "/api/v1/reports/custom/run",
            "/api/v1/subject-allocations/import/parse",
            "/api/v1/boards/exams/0199aaaa-0000-7000-8000-000000000001/results/preview",
            "/api/v1/boards/exams/0199aaaa-0000-7000-8000-000000000001/results/parse",
        ] {
            assert!(is_check(&Method::POST, path), "{path} only reads");
        }
        for path in [
            "/api/v1/students",
            "/api/v1/students/0199aaaa-0000-7000-8000-000000000001/withdraw",
            "/api/v1/fees/vouchers/0199aaaa-0000-7000-8000-000000000001/payments",
            "/api/v1/boards/exams/0199aaaa-0000-7000-8000-000000000001/results/apply",
            "/api/v1/attendance/sheets",
        ] {
            assert!(!is_check(&Method::POST, path), "{path} is a change");
        }
        assert!(!is_check(
            &Method::PATCH,
            "/api/v1/students/check-duplicates"
        ));
        // Never waiting in a file: passwords, credentials, whole-school jobs.
        for path in [
            "/api/v1/settings/directory",
            "/api/v1/settings/directory/test",
            "/api/v1/settings/api-keys",
            "/api/v1/users",
            "/api/v1/users/0199aaaa-0000-7000-8000-000000000001/password",
            "/api/v1/staff/0199aaaa-0000-7000-8000-000000000001/login",
            "/api/v1/staff/0199aaaa-0000-7000-8000-000000000001/login/password",
            "/api/v1/academic-years/rollover",
            "/api/v1/fees/vouchers/generate",
            "/api/v1/timetable/generate",
        ] {
            assert!(needs_server(path), "{path} needs the server there and then");
        }
        assert!(!needs_server("/api/v1/staff"));
        assert!(!needs_server(
            "/api/v1/staff/0199aaaa-0000-7000-8000-000000000001"
        ));
        assert!(!needs_server(
            "/api/v1/fees/vouchers/0199aaaa-0000-7000-8000-000000000001/payments"
        ));
    }

    /// B1 (v0.5.0 test): a duplicate check made offline was queued and counted as a change.
    #[tokio::test]
    async fn a_check_is_never_queued_and_the_waiting_count_is_the_real_changes() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let (outbox, dir) = temp_outbox();
        let asked = AtomicUsize::new(0);
        force_link(&outbox, Link::Offline, 60).await;
        // TESTING task 67: admit a student, edit another, mark attendance; with the checks
        // the forms make along the way.
        let id = Uuid::now_v7();
        let check = r#"{"full_name":"Sara","father_name":"Ahmed","date_of_birth":"2019-01-01"}"#;
        for (method, path, body) in [
            (
                Method::POST,
                "/api/v1/students/check-duplicates".to_string(),
                check,
            ),
            (
                Method::POST,
                "/api/v1/students".to_string(),
                r#"{"full_name":"Sara"}"#,
            ),
            (
                Method::POST,
                "/api/v1/students/check-duplicates".to_string(),
                check,
            ),
            (
                Method::PATCH,
                format!("/api/v1/students/{id}"),
                r#"{"address":"B"}"#,
            ),
            (
                Method::PUT,
                "/api/v1/attendance/sheets".to_string(),
                r#"{"entries":[]}"#,
            ),
        ] {
            let started = std::time::Instant::now();
            let (status, answer, _) =
                through(&outbox, method.clone(), &path, body, &asked, None).await;
            assert!(
                started.elapsed() < Duration::from_millis(500),
                "nothing waits for a server that is known to be away"
            );
            if path.ends_with("check-duplicates") {
                assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
                assert_eq!(answer["error"]["code"], "offline_check");
            } else {
                assert_eq!(status, StatusCode::ACCEPTED, "{method} {path} is queued");
            }
        }
        assert_eq!(
            outbox.waiting().await,
            3,
            "three real changes, three waiting"
        );
        assert_eq!(asked.load(Ordering::SeqCst), 0, "the server was not asked");
        let kinds: Vec<String> = outbox
            .status()
            .await
            .items
            .iter()
            .map(|i| i.path.clone())
            .collect();
        assert!(kinds.iter().all(|p| !p.contains("check-duplicates")));
        // Connected again, a check is asked and still never queued when the answer is lost.
        outbox.reached().await;
        let (status, _, _) = through(
            &outbox,
            Method::POST,
            "/api/v1/students/check-duplicates",
            check,
            &asked,
            Some("[]"),
        )
        .await;
        // (Changes are waiting, so a change would join the line; the check goes through.)
        assert_eq!(status, StatusCode::OK);
        assert_eq!(asked.load(Ordering::SeqCst), 1);
        assert_eq!(outbox.waiting().await, 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// B3 (v0.5.0 test): every read waited five seconds for a silently unreachable server.
    #[tokio::test]
    async fn offline_the_kept_copy_is_served_at_once() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let (outbox, dir) = temp_outbox();
        let asked = AtomicUsize::new(0);
        let list = r#"[{"full_name":"Sara"}]"#;
        let (status, _, offline) = through(
            &outbox,
            Method::GET,
            "/api/v1/students",
            "",
            &asked,
            Some(list),
        )
        .await;
        assert_eq!((status, offline), (StatusCode::OK, None));
        // Known to be offline: the kept copy, without asking.
        force_link(&outbox, Link::Offline, 60).await;
        let started = std::time::Instant::now();
        let (status, body, offline) =
            through(&outbox, Method::GET, "/api/v1/students", "", &asked, None).await;
        assert!(
            started.elapsed() < Duration::from_millis(300),
            "served at once"
        );
        assert_eq!(status, StatusCode::OK);
        assert_eq!(offline.as_deref(), Some("cached"));
        assert_eq!(body[0]["full_name"], "Sara");
        assert_eq!(
            asked.load(Ordering::SeqCst),
            1,
            "not asked again while offline"
        );
        // Still "Reconnecting": one short try, then the kept copy.
        force_link(&outbox, Link::Reconnecting, 4).await;
        let started = std::time::Instant::now();
        let (status, _, offline) =
            through(&outbox, Method::GET, "/api/v1/students", "", &asked, None).await;
        let took = started.elapsed();
        assert!(
            took < Duration::from_millis(1800),
            "about a second, not five: {took:?}"
        );
        assert_eq!(
            (status, offline.as_deref()),
            (StatusCode::OK, Some("cached"))
        );
        // Nothing kept for this screen: said plainly, after the short try.
        force_link(&outbox, Link::Offline, 60).await;
        let started = std::time::Instant::now();
        let (status, body, _) = through(
            &outbox,
            Method::GET,
            "/api/v1/students/never-opened",
            "",
            &asked,
            None,
        )
        .await;
        assert!(started.elapsed() < Duration::from_millis(1800));
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["error"]["code"], "server_unreachable");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// B4 (v0.5.0 test): "Reconnecting" took 12 to 16 seconds to appear, "Offline" over 30.
    #[tokio::test]
    async fn reconnecting_at_once_and_offline_fifteen_seconds_after_the_last_answer() {
        assert!(
            HEARTBEAT + HEARTBEAT_TIMEOUT <= Duration::from_secs(5),
            "an idle PC notices a pulled cable within five seconds"
        );
        let (outbox, dir) = temp_outbox();
        outbox.reached().await;
        outbox.unreachable().await;
        assert_eq!(outbox.link().await, Link::Reconnecting);
        // Ten seconds after the last answer: still trying.
        {
            let mut inner = outbox.inner.lock().await;
            inner.last_contact = Some(Utc::now() - chrono::Duration::seconds(10));
            inner.since = Utc::now() - chrono::Duration::seconds(6);
        }
        outbox.unreachable().await;
        assert_eq!(outbox.link().await, Link::Reconnecting);
        // Fifteen seconds after the last answer (only ten since it was noticed): offline.
        {
            let mut inner = outbox.inner.lock().await;
            inner.last_contact = Some(Utc::now() - chrono::Duration::seconds(15));
            inner.since = Utc::now() - chrono::Duration::seconds(10);
        }
        outbox.unreachable().await;
        assert_eq!(outbox.link().await, Link::Offline);
        outbox.reached().await;
        assert_eq!(outbox.link().await, Link::Connected);
        // "Offline" comes by the clock, without waiting for another attempt to fail: the
        // status the screen reads says so the moment fifteen seconds have passed.
        outbox.unreachable().await;
        assert_eq!(outbox.status().await.link, Link::Reconnecting);
        outbox.inner.lock().await.last_contact = Some(Utc::now() - chrono::Duration::seconds(15));
        assert_eq!(outbox.status().await.link, Link::Offline);
        assert_eq!(outbox.link().await, Link::Offline);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A payment that waits says which voucher it pays and how much, for the screens.
    #[tokio::test]
    async fn a_waiting_payment_names_its_voucher_and_amount() {
        use std::sync::atomic::AtomicUsize;
        let (outbox, dir) = temp_outbox();
        let asked = AtomicUsize::new(0);
        force_link(&outbox, Link::Offline, 60).await;
        let v = Uuid::now_v7();
        let (status, answer, _) = through(
            &outbox,
            Method::POST,
            &format!("/api/v1/fees/vouchers/{v}/payments"),
            r#"{"amount":"5000.00","method":"cash"}"#,
            &asked,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::ACCEPTED);
        assert!(answer["queued"]["provisional"]["provisional_no"].is_string());
        let status = serde_json::to_value(outbox.status().await).unwrap();
        assert_eq!(status["items"][0]["voucher_id"], v.to_string());
        assert_eq!(status["items"][0]["amount"], "5000.00");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
