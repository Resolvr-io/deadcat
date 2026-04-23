//! NIP-46 remote signing (Nostr Connect) connection management.
//!
//! Ships our own signer rather than using `nostr-connect` 0.38 because that
//! crate hardcodes NIP-04 encryption in `EventBuilder::nostr_connect` —
//! Primal iOS (and other modern signers) speak NIP-44 only and respond
//! "Failed to decrypt content." to NIP-04 messages. We implement the RPC
//! transport manually here: NIP-44 outgoing, both NIP-04/NIP-44 incoming
//! (some signers still send NIP-04 responses), with a background listener
//! task routing responses to pending requests by `id`.
//!
//! Persisted connection state lives at `<app_data_dir>/nip46_connection.json`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use nostr_sdk::prelude::*;
use once_cell::sync::OnceCell;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{oneshot, Mutex};

/// Simple error type so we can convert `String` messages into something
/// `SignerError::backend` (which wants `std::error::Error`) will accept.
#[derive(Debug)]
struct StrError(String);

impl std::fmt::Display for StrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for StrError {}

fn to_signer_err(msg: String) -> SignerError {
    SignerError::backend(StrError(msg))
}

const NIP46_CONNECTION_FILE: &str = "nip46_connection.json";

/// Per-RPC timeout (sign_event, nip44_encrypt, etc.).
const NIP46_RPC_TIMEOUT_SECS: u64 = 60;

/// Longer timeout for the app-initiated (`nostrconnect://`) flow, where the
/// user must scan the QR code and approve the connection in the remote
/// signer. 60s is often too tight when the signer needs to be launched or
/// switched to.
const NOSTRCONNECT_HANDSHAKE_TIMEOUT_SECS: u64 = 180;

/// Form-encode a string the way JS `URLSearchParams.toString()` does:
/// space → `+`, everything outside the unreserved set → `%XX`. Matches
/// the byte-for-byte output of reference NIP-46 clients (Zap Cooking,
/// nostr-tools-based apps, etc.) so strict signer-side parsers (Primal
/// iOS) accept the URI.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'*' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Persisted NIP-46 connection state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Nip46Connection {
    /// The bunker URI used to establish the connection.
    pub bunker_uri: String,
    /// The ephemeral app keypair (hex-encoded secret key).
    pub app_secret_key_hex: String,
    /// The remote signer's public key (hex).
    pub remote_signer_pubkey_hex: String,
    /// The user's actual public key (hex) — obtained via `get_public_key`.
    pub user_pubkey_hex: Option<String>,
    /// Relay URLs used for the NIP-46 communication channel.
    pub relay_urls: Vec<String>,
}

/// Status of the NIP-46 connection, returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Nip46Status {
    pub connected: bool,
    pub remote_signer_pubkey: String,
    pub user_pubkey: Option<String>,
    pub relay_urls: Vec<String>,
    pub bunker_uri: String,
}

// ── Persistence ────────────────────────────────────────────────────────────

fn connection_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(NIP46_CONNECTION_FILE)
}

/// Load a persisted NIP-46 connection from disk.
pub fn load_connection(app_data_dir: &Path) -> Option<Nip46Connection> {
    let path = connection_path(app_data_dir);
    let contents = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

/// Persist a NIP-46 connection to disk.
pub fn save_connection(app_data_dir: &Path, conn: &Nip46Connection) -> Result<(), String> {
    let path = connection_path(app_data_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("failed to create data dir: {e}"))?;
    }
    let json =
        serde_json::to_string_pretty(conn).map_err(|e| format!("failed to serialize: {e}"))?;
    std::fs::write(path, json).map_err(|e| format!("failed to write nip46 connection: {e}"))
}

/// Delete the persisted NIP-46 connection.
pub fn delete_connection(app_data_dir: &Path) -> Result<(), String> {
    let path = connection_path(app_data_dir);
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| format!("failed to delete nip46 connection: {e}"))?;
    }
    Ok(())
}

/// Returns `true` if a persisted NIP-46 connection exists.
pub fn has_connection(app_data_dir: &Path) -> bool {
    connection_path(app_data_dir).exists()
}

// ── Manual NIP-46 signer ───────────────────────────────────────────────────

type PendingMap = Arc<Mutex<HashMap<String, oneshot::Sender<Result<String, String>>>>>;

/// A `NostrSigner` impl that talks NIP-46 over NIP-44, bypassing the
/// rust `nostr-connect` crate's hardcoded NIP-04 transport. Owns a
/// persistent `nostr_sdk::Client` subscription for incoming responses
/// and routes them to pending callers by request id.
pub struct Nip46ManualSigner {
    app_keys: Keys,
    signer_pubkey: PublicKey,
    user_pubkey: OnceCell<PublicKey>,
    client: Client,
    pending: PendingMap,
    timeout: Duration,
}

impl std::fmt::Debug for Nip46ManualSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Nip46ManualSigner")
            .field("app_pubkey", &self.app_keys.public_key().to_hex())
            .field("signer_pubkey", &self.signer_pubkey.to_hex())
            .finish()
    }
}

impl Nip46ManualSigner {
    /// Build a signer bound to an already-known signer pubkey (e.g. after
    /// a completed nostrconnect handshake, or restored from disk).
    /// Connects to the given relays and starts listening for responses.
    pub async fn new(
        app_keys: Keys,
        signer_pubkey: PublicKey,
        relay_urls: Vec<String>,
        user_pubkey: Option<PublicKey>,
    ) -> Result<Arc<Self>, String> {
        let client = Client::default();
        for url in &relay_urls {
            let _ = client.add_relay(url.as_str()).await;
        }
        client.connect_with_timeout(Duration::from_secs(5)).await;

        // Subscribe to events FROM the signer, p-tagged to us.
        let filter = Filter::new()
            .kind(Kind::NostrConnect)
            .author(signer_pubkey)
            .pubkey(app_keys.public_key())
            .limit(0);
        client
            .subscribe(vec![filter], None)
            .await
            .map_err(|e| format!("subscribe: {e}"))?;

        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let user_cell = OnceCell::new();
        if let Some(pk) = user_pubkey {
            let _ = user_cell.set(pk);
        }
        let signer = Arc::new(Self {
            app_keys: app_keys.clone(),
            signer_pubkey,
            user_pubkey: user_cell,
            client: client.clone(),
            pending: pending.clone(),
            timeout: Duration::from_secs(NIP46_RPC_TIMEOUT_SECS),
        });

        // Background task: decrypt incoming events, dispatch by id.
        let secret_key = app_keys.secret_key().clone();
        let mut notifications = client.notifications();
        tokio::spawn(async move {
            while let Ok(notif) = notifications.recv().await {
                let RelayPoolNotification::Event { event, .. } = notif else {
                    continue;
                };
                if event.pubkey != signer_pubkey || event.kind != Kind::NostrConnect {
                    continue;
                }
                let decrypted = decrypt_nip46(&secret_key, &event.pubkey, &event.content);
                let content = match decrypted {
                    Ok(s) => s,
                    Err(e) => {
                        log::debug!("[nip46] could not decrypt event: {e}");
                        continue;
                    }
                };
                let parsed: serde_json::Value = match serde_json::from_str(&content) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                let Some(id) = parsed.get("id").and_then(|v| v.as_str()).map(String::from) else {
                    continue;
                };
                let error = parsed
                    .get("error")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                let result = parsed
                    .get("result")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .unwrap_or_default();
                let mut p = pending.lock().await;
                if let Some(tx) = p.remove(&id) {
                    let _ = tx.send(if let Some(err) = error {
                        Err(err)
                    } else {
                        Ok(result)
                    });
                }
            }
        });

        Ok(signer)
    }

    /// Send a single RPC and wait for the response. Encrypts with NIP-44.
    async fn send_request(
        &self,
        method: &str,
        params: Vec<serde_json::Value>,
    ) -> Result<String, String> {
        let mut raw = [0u8; 8];
        rand::thread_rng().fill_bytes(&mut raw);
        let req_id = hex::encode(raw);
        let req = json!({
            "id": req_id,
            "method": method,
            "params": params,
        });
        let plaintext = req.to_string();

        let ciphertext = nostr_sdk::nips::nip44::encrypt(
            self.app_keys.secret_key(),
            &self.signer_pubkey,
            &plaintext,
            nostr_sdk::nips::nip44::Version::V2,
        )
        .map_err(|e| format!("nip44 encrypt: {e}"))?;

        let event = EventBuilder::new(Kind::NostrConnect, ciphertext)
            .tag(Tag::public_key(self.signer_pubkey))
            .sign_with_keys(&self.app_keys)
            .map_err(|e| format!("sign event: {e}"))?;

        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(req_id.clone(), tx);

        self.client
            .send_event(event)
            .await
            .map_err(|e| format!("send event: {e}"))?;

        match tokio::time::timeout(self.timeout, rx).await {
            Ok(Ok(Ok(result))) => Ok(result),
            Ok(Ok(Err(err))) => Err(err),
            Ok(Err(_)) => Err("response channel closed".into()),
            Err(_) => {
                self.pending.lock().await.remove(&req_id);
                Err(format!("{method} timeout"))
            }
        }
    }
}

fn decrypt_nip46(
    secret_key: &SecretKey,
    other: &PublicKey,
    ciphertext: &str,
) -> Result<String, String> {
    // NIP-04 ciphertexts contain "?iv=" as a separator; NIP-44 does not.
    if ciphertext.contains("?iv=") {
        nostr_sdk::nips::nip04::decrypt(secret_key, other, ciphertext)
            .map_err(|e| format!("nip04 decrypt: {e}"))
    } else {
        nostr_sdk::nips::nip44::decrypt(secret_key, other, ciphertext.as_bytes())
            .map_err(|e| format!("nip44 decrypt: {e}"))
    }
}

#[async_trait]
impl NostrSigner for Nip46ManualSigner {
    fn backend(&self) -> SignerBackend<'_> {
        SignerBackend::NostrConnect
    }

    async fn get_public_key(&self) -> Result<PublicKey, SignerError> {
        if let Some(pk) = self.user_pubkey.get() {
            return Ok(*pk);
        }
        let result = self
            .send_request("get_public_key", vec![])
            .await
            .map_err(to_signer_err)?;
        let pk = PublicKey::from_hex(&result)
            .map_err(|e| to_signer_err(format!("invalid pubkey hex: {e}")))?;
        let _ = self.user_pubkey.set(pk);
        Ok(pk)
    }

    async fn sign_event(&self, unsigned: UnsignedEvent) -> Result<Event, SignerError> {
        let unsigned_json = unsigned.as_json();
        let result = self
            .send_request("sign_event", vec![json!(unsigned_json)])
            .await
            .map_err(to_signer_err)?;
        let signed: Event = Event::from_json(&result)
            .map_err(|e| to_signer_err(format!("parse signed event: {e}")))?;
        Ok(signed)
    }

    async fn nip04_encrypt(
        &self,
        public_key: &PublicKey,
        content: &str,
    ) -> Result<String, SignerError> {
        self.send_request(
            "nip04_encrypt",
            vec![json!(public_key.to_hex()), json!(content)],
        )
        .await
        .map_err(to_signer_err)
    }

    async fn nip04_decrypt(
        &self,
        public_key: &PublicKey,
        encrypted_content: &str,
    ) -> Result<String, SignerError> {
        self.send_request(
            "nip04_decrypt",
            vec![json!(public_key.to_hex()), json!(encrypted_content)],
        )
        .await
        .map_err(to_signer_err)
    }

    async fn nip44_encrypt(
        &self,
        public_key: &PublicKey,
        content: &str,
    ) -> Result<String, SignerError> {
        self.send_request(
            "nip44_encrypt",
            vec![json!(public_key.to_hex()), json!(content)],
        )
        .await
        .map_err(to_signer_err)
    }

    async fn nip44_decrypt(
        &self,
        public_key: &PublicKey,
        payload: &str,
    ) -> Result<String, SignerError> {
        self.send_request(
            "nip44_decrypt",
            vec![json!(public_key.to_hex()), json!(payload)],
        )
        .await
        .map_err(to_signer_err)
    }
}

// ── Connection flows ───────────────────────────────────────────────────────

/// Parsed bunker URI parts.
struct BunkerUriParts {
    signer_pubkey: PublicKey,
    relays: Vec<String>,
    /// Optional secret that the signer expects the client to echo back in
    /// the `connect` request. Present on most bunker URIs; required by
    /// compliant signers that embed it. Omitted triggers "invalid secret".
    secret: Option<String>,
}

fn parse_bunker_uri(bunker_uri: &str) -> Result<BunkerUriParts, String> {
    // bunker://<pubkey>?relay=<r1>&relay=<r2>&...&secret=<s>
    let rest = bunker_uri
        .strip_prefix("bunker://")
        .ok_or_else(|| "Expected bunker:// URI".to_string())?;
    let (pubkey_hex, query) = match rest.find('?') {
        Some(i) => (&rest[..i], &rest[i + 1..]),
        None => (rest, ""),
    };
    let signer_pubkey =
        PublicKey::from_hex(pubkey_hex).map_err(|e| format!("invalid signer pubkey: {e}"))?;
    let mut relays = Vec::new();
    let mut secret: Option<String> = None;
    for pair in query.split('&').filter(|s| !s.is_empty()) {
        if let Some(value) = pair.strip_prefix("relay=") {
            let decoded = percent_decode(value);
            if !decoded.is_empty() {
                relays.push(decoded);
            }
        } else if let Some(value) = pair.strip_prefix("secret=") {
            let decoded = percent_decode(value);
            if !decoded.is_empty() {
                secret = Some(decoded);
            }
        }
    }
    Ok(BunkerUriParts {
        signer_pubkey,
        relays,
        secret,
    })
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                if let Ok(b) = u8::from_str_radix(hex, 16) {
                    out.push(b);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).unwrap_or_default()
}

/// Connect to a remote signer via a `bunker://` URI. Generates fresh
/// ephemeral app keys, sends a `connect` RPC to the signer, and returns
/// the signer + the persistable connection state.
pub async fn connect_from_bunker_uri(
    bunker_uri_str: &str,
) -> Result<(Arc<Nip46ManualSigner>, Nip46Connection), String> {
    let parts = parse_bunker_uri(bunker_uri_str)?;
    if parts.relays.is_empty() {
        return Err("bunker URI has no relays".to_string());
    }
    let app_keys = Keys::generate();

    let signer = Nip46ManualSigner::new(
        app_keys.clone(),
        parts.signer_pubkey,
        parts.relays.clone(),
        None,
    )
    .await?;

    // Send `connect` to establish the session with this signer. Per NIP-46
    // the params are `[<signer_pubkey>, <secret?>, <perms?>]` — if the
    // bunker URI carried a `secret`, we MUST echo it here or the signer
    // rejects the request with "invalid secret".
    let mut params = vec![json!(parts.signer_pubkey.to_hex())];
    if let Some(secret) = &parts.secret {
        params.push(json!(secret));
    }
    signer
        .send_request("connect", params)
        .await
        .map_err(|e| format!("connect request failed: {e}"))?;

    let conn = Nip46Connection {
        bunker_uri: bunker_uri_str.to_string(),
        app_secret_key_hex: app_keys.secret_key().to_secret_hex(),
        remote_signer_pubkey_hex: parts.signer_pubkey.to_hex(),
        user_pubkey_hex: None,
        relay_urls: parts.relays,
    };
    Ok((signer, conn))
}

/// Restore a signer from a persisted connection. Reuses the original
/// ephemeral app keys so the remote signer still recognizes us.
pub async fn restore_from_connection(
    conn: &Nip46Connection,
) -> Result<Arc<Nip46ManualSigner>, String> {
    let secret_key = SecretKey::from_hex(&conn.app_secret_key_hex)
        .map_err(|e| format!("invalid persisted app secret key: {e}"))?;
    let app_keys = Keys::new(secret_key);
    let signer_pubkey = PublicKey::from_hex(&conn.remote_signer_pubkey_hex)
        .map_err(|e| format!("invalid persisted signer pubkey: {e}"))?;
    let user_pubkey = conn
        .user_pubkey_hex
        .as_deref()
        .and_then(|h| PublicKey::from_hex(h).ok());
    Nip46ManualSigner::new(
        app_keys,
        signer_pubkey,
        conn.relay_urls.clone(),
        user_pubkey,
    )
    .await
}

/// Wrap a manual signer as an `Arc<dyn NostrSigner>`.
pub fn into_arc_signer(signer: Arc<Nip46ManualSigner>) -> Arc<dyn NostrSigner> {
    signer
}

// ── App-initiated (nostrconnect://) flow ───────────────────────────────────

/// State returned from `prepare_nostrconnect`. The caller displays
/// `uri` as a QR code, then calls `await_nostrconnect_handshake` to
/// wait for the remote signer to scan and approve.
pub struct NostrConnectPending {
    pub uri: String,
    pub app_keys: Keys,
    pub secret: String,
    pub relays: Vec<String>,
}

/// Prepare an app-initiated NIP-46 session. The URI includes a 16-hex
/// `secret` (REQUIRED by Primal iOS — without it no connect response is
/// published) and flat `name`/`url` params matching the nostr-tools
/// `createNostrConnectURI` format used by proven-working clients
/// (wordswithzaps, mutable, Zap Cooking).
pub fn prepare_nostrconnect(relay_urls: &[String]) -> Result<NostrConnectPending, String> {
    let app_keys = Keys::generate();

    // Relay list for the URI — primal.net FIRST (per wordswithzaps:
    // "primal first for Primal app compatibility").
    let mut relays: Vec<String> = Vec::with_capacity(relay_urls.len());
    for url in relay_urls {
        if url.contains("relay.primal.net") {
            relays.insert(0, url.clone());
        } else {
            relays.push(url.clone());
        }
    }
    if relays.is_empty() {
        return Err("At least one relay URL is required".to_string());
    }

    // 16 hex chars = 8 random bytes (nostr-tools convention).
    let mut raw = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut raw);
    let secret = hex::encode(raw);

    let relay_params: String = relays
        .iter()
        .map(|r| {
            let trimmed = r.strip_suffix('/').unwrap_or(r);
            format!("relay={}", percent_encode(trimmed))
        })
        .collect::<Vec<_>>()
        .join("&");
    let uri_str = format!(
        "nostrconnect://{}?{relay_params}&secret={secret}&name={}&url={}",
        app_keys.public_key().to_hex(),
        percent_encode("Deadcat Live"),
        percent_encode("https://deadcat.live"),
    );
    log::info!("[nostrconnect] URI = {uri_str}");

    Ok(NostrConnectPending {
        uri: uri_str,
        app_keys,
        secret,
        relays,
    })
}

/// Subscribe on the session's relays and wait for the remote signer's
/// `connect` response (secret echo). Returns the signer's pubkey.
pub async fn await_nostrconnect_handshake(
    pending: &NostrConnectPending,
) -> Result<PublicKey, String> {
    let client = Client::default();
    for url in &pending.relays {
        let _ = client.add_relay(url.as_str()).await;
    }
    client.connect_with_timeout(Duration::from_secs(5)).await;

    let filter = Filter::new()
        .kind(Kind::NostrConnect)
        .pubkey(pending.app_keys.public_key())
        .limit(0);
    client
        .subscribe(vec![filter], None)
        .await
        .map_err(|e| format!("subscribe: {e}"))?;

    let mut notifications = client.notifications();
    let deadline =
        tokio::time::Instant::now() + Duration::from_secs(NOSTRCONNECT_HANDSHAKE_TIMEOUT_SECS);

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err("timed out waiting for remote signer".to_string());
        }
        let recv = tokio::time::timeout(remaining, notifications.recv()).await;
        let notif = match recv {
            Ok(Ok(n)) => n,
            Ok(Err(_)) => return Err("notification channel closed".to_string()),
            Err(_) => return Err("timed out waiting for remote signer".to_string()),
        };
        let RelayPoolNotification::Event { event, .. } = notif else {
            continue;
        };
        if event.kind != Kind::NostrConnect {
            continue;
        }
        let content =
            match decrypt_nip46(pending.app_keys.secret_key(), &event.pubkey, &event.content) {
                Ok(s) => s,
                Err(_) => continue,
            };
        let parsed: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => continue,
        };
        // Ignore explicit error responses.
        if parsed
            .get("error")
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.is_empty())
        {
            continue;
        }
        // Accept two handshake completions:
        // - `result == <secret>` — Primal's echoed-secret pattern
        // - `result == "ack"` — standard NIP-46 connect response (Amber etc.)
        // Signer identity in both cases comes from the event's author,
        // which was vouched for by the NIP-44/NIP-04 decryption above
        // (the signer encrypted to our app pubkey).
        let result = parsed.get("result").and_then(|v| v.as_str());
        let matched = match result {
            Some(r) if r == pending.secret.as_str() => Some("echoed-secret"),
            Some("ack") => Some("ack"),
            _ => None,
        };
        if let Some(style) = matched {
            log::info!(
                "[nostrconnect] handshake: matched {style} response from signer {}",
                event.pubkey.to_hex()
            );
            return Ok(event.pubkey);
        }
    }
}

/// Build a bunker URI from a completed nostrconnect handshake.
pub fn bunker_uri_from_handshake(
    pending: &NostrConnectPending,
    signer_pubkey: PublicKey,
) -> String {
    let relay_params: String = pending
        .relays
        .iter()
        .map(|r| {
            let trimmed = r.strip_suffix('/').unwrap_or(r);
            format!("relay={}", percent_encode(trimmed))
        })
        .collect::<Vec<_>>()
        .join("&");
    format!("bunker://{}?{relay_params}", signer_pubkey.to_hex())
}

/// Build a manual signer from the keys + signer pubkey learned during
/// the app-initiated handshake. The session is already established on
/// Primal's side, so we skip the explicit `connect` RPC.
pub async fn signer_from_handshake(
    pending: &NostrConnectPending,
    signer_pubkey: PublicKey,
) -> Result<(Arc<Nip46ManualSigner>, Nip46Connection), String> {
    let signer = Nip46ManualSigner::new(
        pending.app_keys.clone(),
        signer_pubkey,
        pending.relays.clone(),
        None,
    )
    .await?;
    let conn = Nip46Connection {
        bunker_uri: bunker_uri_from_handshake(pending, signer_pubkey),
        app_secret_key_hex: pending.app_keys.secret_key().to_secret_hex(),
        remote_signer_pubkey_hex: signer_pubkey.to_hex(),
        user_pubkey_hex: None,
        relay_urls: pending.relays.clone(),
    };
    Ok((signer, conn))
}
