//! NIP-46 remote signing (Nostr Connect) connection management.
//!
//! Handles persisting and restoring `NostrConnect` sessions across app restarts.
//! The connection state is stored as JSON in `<app_data_dir>/nip46_connection.json`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use nostr_connect::prelude::NostrConnect;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

const NIP46_CONNECTION_FILE: &str = "nip46_connection.json";

/// Default timeout for NIP-46 request/response round-trips.
const NIP46_TIMEOUT_SECS: u64 = 60;

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

// ── NostrConnect construction ──────────────────────────────────────────────

/// Create a new `NostrConnect` signer from a `bunker://` URI string.
///
/// Generates fresh ephemeral app keys and returns both the signer and the
/// connection state to persist.
pub fn connect_from_bunker_uri(
    bunker_uri_str: &str,
) -> Result<(NostrConnect, Nip46Connection), String> {
    let uri =
        NostrConnectURI::parse(bunker_uri_str).map_err(|e| format!("invalid bunker URI: {e}"))?;

    if !uri.is_bunker() {
        return Err("Expected a bunker:// URI, got nostrconnect://".to_string());
    }

    // Extract data before moving uri into NostrConnect::new()
    let remote_signer_pubkey = *uri
        .remote_signer_public_key()
        .ok_or("bunker URI missing remote signer pubkey")?;
    let relay_urls: Vec<String> = uri.relays().iter().map(|r| r.to_string()).collect();

    let app_keys = Keys::generate();

    let signer = NostrConnect::new(
        uri,
        app_keys.clone(),
        Duration::from_secs(NIP46_TIMEOUT_SECS),
        None,
    )
    .map_err(|e| format!("failed to create NostrConnect: {e}"))?;

    let conn = Nip46Connection {
        bunker_uri: bunker_uri_str.to_string(),
        app_secret_key_hex: app_keys.secret_key().to_secret_hex(),
        remote_signer_pubkey_hex: remote_signer_pubkey.to_hex(),
        user_pubkey_hex: None,
        relay_urls,
    };

    Ok((signer, conn))
}

/// Restore a `NostrConnect` signer from a persisted connection.
///
/// Re-uses the same app keys so the remote signer recognizes us.
pub fn restore_from_connection(conn: &Nip46Connection) -> Result<NostrConnect, String> {
    let uri = NostrConnectURI::parse(&conn.bunker_uri)
        .map_err(|e| format!("invalid persisted bunker URI: {e}"))?;

    let secret_key = SecretKey::from_hex(&conn.app_secret_key_hex)
        .map_err(|e| format!("invalid persisted app secret key: {e}"))?;
    let app_keys = Keys::new(secret_key);

    let signer = NostrConnect::new(uri, app_keys, Duration::from_secs(NIP46_TIMEOUT_SECS), None)
        .map_err(|e| format!("failed to restore NostrConnect: {e}"))?;

    // If we already know the user's public key, set it to skip the
    // initial `get_public_key` round-trip on first use.
    if let Some(ref pk_hex) = conn.user_pubkey_hex {
        if let Ok(pk) = PublicKey::from_hex(pk_hex) {
            let _ = signer.non_secure_set_user_public_key(pk);
        }
    }

    Ok(signer)
}

/// Wrap a `NostrConnect` instance as an `Arc<dyn NostrSigner>`.
pub fn into_arc_signer(signer: NostrConnect) -> Arc<dyn NostrSigner> {
    Arc::new(signer)
}
