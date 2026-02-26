use nostr_sdk::{
    Client, Event, EventBuilder, Filter, Keys, Kind, PublicKey, SecretKey, Tag, TagKind,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// d-tag for wallet mnemonic backup events (NIP-78).
pub const WALLET_BACKUP_D_TAG: &str = "deadcat-wallet-backup";

// Re-export SDK-owned types and functions for use by the app layer.
pub use deadcat_sdk::discovery::{
    // Builders
    build_announcement_event,
    build_attestation_event,
    build_attestation_filter,
    build_contract_filter,
    // Relay helpers
    connect_client,
    // Conversions
    discovered_market_to_contract_params,
    fetch_announcements,
    parse_announcement_event,
    publish_event,
    sign_attestation,
    // Types
    AttestationContent,
    AttestationResult,
    DiscoveredMarket,
    // Constants
    APP_EVENT_KIND,
    ATTESTATION_TAG,
    CONTRACT_TAG,
    DEFAULT_RELAYS,
    NETWORK_TAG,
};
pub use deadcat_sdk::{ContractAnnouncement, ContractMetadata};

// ---------------------------------------------------------------------------
// App-layer-only types (Tauri command request/response types)
// ---------------------------------------------------------------------------

/// Response from identity initialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityResponse {
    pub pubkey_hex: String,
    pub npub: String,
}

/// Request to create a new contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateContractRequest {
    pub question: String,
    pub description: String,
    pub category: String,
    pub resolution_source: String,
    pub starting_yes_price: u8,
    pub settlement_deadline_unix: u64,
    pub collateral_per_token: u64,
}

/// A relay entry with connection status and backup indicator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayEntry {
    pub url: String,
    pub has_backup: bool,
}

/// Status of wallet backup across relays.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NostrBackupStatus {
    pub has_backup: bool,
    pub relay_results: Vec<RelayBackupResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayBackupResult {
    pub url: String,
    pub has_backup: bool,
}

/// User profile metadata from kind 0.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NostrProfile {
    pub picture: Option<String>,
    pub name: Option<String>,
    pub display_name: Option<String>,
}

// ---------------------------------------------------------------------------
// Multi-relay client (app-layer, wraps SDK single-relay connect_client)
// ---------------------------------------------------------------------------

/// Connect a Nostr client to multiple relays (or defaults if empty).
pub async fn connect_multi_relay_client(relays: &[String]) -> Result<Client, String> {
    let client = Client::default();
    let urls: Vec<&str> = if relays.is_empty() {
        DEFAULT_RELAYS.to_vec()
    } else {
        relays.iter().map(|s| s.as_str()).collect()
    };
    for url in &urls {
        client
            .add_relay(*url)
            .await
            .map_err(|e| format!("failed to add relay {url}: {e}"))?;
    }
    client.connect_with_timeout(Duration::from_secs(5)).await;
    Ok(client)
}

// ---------------------------------------------------------------------------
// NIP-44 wallet backup (kind 30078)
// ---------------------------------------------------------------------------

/// Build a kind 30078 event containing a NIP-44 encrypted wallet mnemonic.
pub fn build_wallet_backup_event(keys: &Keys, encrypted_content: &str) -> Result<Event, String> {
    let tags = vec![
        Tag::identifier(WALLET_BACKUP_D_TAG),
        Tag::custom(TagKind::custom("encrypted"), vec!["true".to_string()]),
        Tag::custom(TagKind::custom("encryption"), vec!["nip44".to_string()]),
    ];

    EventBuilder::new(APP_EVENT_KIND, encrypted_content)
        .tags(tags)
        .sign_with_keys(keys)
        .map_err(|e| format!("failed to build backup event: {e}"))
}

/// Build a filter to query the wallet backup event for a given pubkey.
pub fn build_backup_query_filter(pubkey: &PublicKey) -> Filter {
    Filter::new()
        .kind(APP_EVENT_KIND)
        .author(*pubkey)
        .identifier(WALLET_BACKUP_D_TAG)
}

/// Encrypt a plaintext string to self using NIP-44.
pub fn nip44_encrypt_to_self(keys: &Keys, plaintext: &str) -> Result<String, String> {
    use nostr_sdk::nostr::nips::nip44;
    nip44::encrypt(
        keys.secret_key(),
        &keys.public_key(),
        plaintext,
        nip44::Version::V2,
    )
    .map_err(|e| format!("nip44 encryption failed: {e}"))
}

/// Decrypt a NIP-44 ciphertext from self.
pub fn nip44_decrypt_from_self(keys: &Keys, ciphertext: &str) -> Result<String, String> {
    use nostr_sdk::nostr::nips::nip44;
    nip44::decrypt(keys.secret_key(), &keys.public_key(), ciphertext)
        .map_err(|e| format!("nip44 decryption failed: {e}"))
}

/// Build a kind 5 deletion event (NIP-09) targeting the wallet backup addressable event.
pub fn build_backup_deletion_event(keys: &Keys) -> Result<Event, String> {
    let coordinate = format!(
        "{}:{}:{}",
        APP_EVENT_KIND.as_u16(),
        keys.public_key(),
        WALLET_BACKUP_D_TAG,
    );
    let tags = vec![Tag::custom(TagKind::custom("a"), vec![coordinate])];

    EventBuilder::new(Kind::Custom(5), "delete wallet backup")
        .tags(tags)
        .sign_with_keys(keys)
        .map_err(|e| format!("failed to build deletion event: {e}"))
}

// ---------------------------------------------------------------------------
// NIP-65 relay list (kind 10002)
// ---------------------------------------------------------------------------

/// Kind for relay list metadata (NIP-65).
pub const RELAY_LIST_KIND: Kind = Kind::Custom(10002);

/// Build a kind 10002 event with relay `r` tags.
pub fn build_relay_list_event(keys: &Keys, relays: &[String]) -> Result<Event, String> {
    let tags: Vec<Tag> = relays
        .iter()
        .map(|url| Tag::custom(TagKind::custom("r"), vec![url.clone()]))
        .collect();

    EventBuilder::new(RELAY_LIST_KIND, "")
        .tags(tags)
        .sign_with_keys(keys)
        .map_err(|e| format!("failed to build relay list event: {e}"))
}

/// Parse relay URLs from a kind 10002 event.
pub fn parse_relay_list_event(event: &Event) -> Vec<String> {
    event
        .tags
        .iter()
        .filter_map(|tag| {
            let v = tag.as_slice();
            if v.len() >= 2 && v[0] == "r" {
                Some(normalize_relay_url(&v[1]))
            } else {
                None
            }
        })
        .collect()
}

/// Fetch a user's NIP-65 relay list from connected relays.
pub async fn fetch_relay_list(
    client: &Client,
    pubkey: &PublicKey,
) -> Result<Option<Vec<String>>, String> {
    let filter = Filter::new().kind(RELAY_LIST_KIND).author(*pubkey).limit(1);

    let events = client
        .fetch_events(vec![filter], Duration::from_secs(10))
        .await
        .map_err(|e| format!("failed to fetch relay list: {e}"))?;

    let result = {
        let mut iter = events.iter();
        if let Some(event) = iter.next() {
            let relays = parse_relay_list_event(event);
            if relays.is_empty() {
                None
            } else {
                Some(relays)
            }
        } else {
            None
        }
    };
    Ok(result)
}

/// Normalize a relay URL: lowercase, ensure wss://, strip trailing slash.
pub fn normalize_relay_url(url: &str) -> String {
    let mut s = url.trim().to_lowercase();
    if !s.starts_with("wss://") && !s.starts_with("ws://") {
        s = format!("wss://{s}");
    }
    s.trim_end_matches('/').to_string()
}

// ---------------------------------------------------------------------------
// Kind 0 profile metadata
// ---------------------------------------------------------------------------

/// Fetch the user's kind 0 profile from relays.
pub async fn fetch_profile(
    client: &Client,
    pubkey: &PublicKey,
) -> Result<Option<NostrProfile>, String> {
    let filter = Filter::new().kind(Kind::Metadata).author(*pubkey).limit(1);

    let events = client
        .fetch_events(vec![filter], Duration::from_secs(10))
        .await
        .map_err(|e| format!("failed to fetch profile: {e}"))?;

    let result = {
        let mut iter = events.iter();
        if let Some(event) = iter.next() {
            let parsed: serde_json::Value = serde_json::from_str(&event.content)
                .map_err(|e| format!("failed to parse profile JSON: {e}"))?;
            Some(NostrProfile {
                picture: parsed
                    .get("picture")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                name: parsed
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                display_name: parsed
                    .get("display_name")
                    .and_then(|v| v.as_str())
                    .map(String::from),
            })
        } else {
            None
        }
    };
    Ok(result)
}

// ---------------------------------------------------------------------------
// Identity persistence (app-layer concern)
// ---------------------------------------------------------------------------

/// Load an existing Nostr keypair from disk. Returns `None` if no key file exists.
pub fn load_keys(app_data_dir: &std::path::Path) -> Result<Option<Keys>, String> {
    let key_path = app_data_dir.join("nostr_identity.key");

    if key_path.exists() {
        let hex_str = std::fs::read_to_string(&key_path)
            .map_err(|e| format!("failed to read key file: {e}"))?;
        let secret_key = SecretKey::from_hex(hex_str.trim())
            .map_err(|e| format!("failed to parse secret key: {e}"))?;
        Ok(Some(Keys::new(secret_key)))
    } else {
        Ok(None)
    }
}

/// Generate a new Nostr keypair, persist to disk, and return it.
pub fn generate_keys(app_data_dir: &std::path::Path) -> Result<Keys, String> {
    let key_path = app_data_dir.join("nostr_identity.key");
    let keys = Keys::generate();
    let secret_hex = keys.secret_key().to_secret_hex();
    if let Some(parent) = key_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("failed to create data dir: {e}"))?;
    }
    std::fs::write(&key_path, secret_hex).map_err(|e| format!("failed to write key file: {e}"))?;
    Ok(keys)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use deadcat_sdk::{MarketId, PredictionMarketParams};
    use nostr_sdk::secp256k1;

    fn test_metadata() -> ContractMetadata {
        ContractMetadata {
            question: "Will BTC hit 100k?".to_string(),
            description: "Resolves via exchange data.".to_string(),
            category: "Bitcoin".to_string(),
            resolution_source: "Exchange basket".to_string(),
            starting_yes_price: 55,
        }
    }

    fn test_params() -> PredictionMarketParams {
        PredictionMarketParams {
            oracle_public_key: [0xaa; 32],
            collateral_asset_id: [0xbb; 32],
            yes_token_asset: [0x01; 32],
            no_token_asset: [0x02; 32],
            yes_reissuance_token: [0x03; 32],
            no_reissuance_token: [0x04; 32],
            collateral_per_token: 5000,
            expiry_time: 3_650_000,
        }
    }

    #[test]
    fn contract_announcement_serde_roundtrip() {
        let announcement = ContractAnnouncement {
            version: 1,
            contract_params: test_params(),
            metadata: test_metadata(),
            creation_txid: Some("abc123".to_string()),
        };

        let json = serde_json::to_string(&announcement).unwrap();
        let parsed: ContractAnnouncement = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.contract_params, announcement.contract_params);
        assert_eq!(parsed.metadata.question, "Will BTC hit 100k?");
        assert_eq!(parsed.creation_txid, Some("abc123".to_string()));
    }

    #[test]
    fn contract_filter_construction() {
        let filter = build_contract_filter();
        // Filter should target kind 30078 with deadcat-contract hashtag
        assert!(format!("{filter:?}").contains("30078"));
    }

    #[test]
    fn attestation_filter_construction() {
        let market_id_hex = "abcd1234";
        let filter = build_attestation_filter(market_id_hex);
        assert!(format!("{filter:?}").contains("abcd1234:attestation"));
    }

    #[test]
    fn sign_attestation_works() {
        let keys = Keys::generate();
        let market_id = MarketId([0xab; 32]);

        let (sig, msg) = sign_attestation(&keys, &market_id, true).unwrap();
        assert_eq!(sig.len(), 64);
        assert_eq!(msg.len(), 32);

        // Verify the signature
        let secp = secp256k1::Secp256k1::new();
        let message = secp256k1::Message::from_digest(msg);
        let pk_hex = keys.public_key().to_hex();
        let pk_bytes = hex::decode(&pk_hex).unwrap();
        let xonly = secp256k1::XOnlyPublicKey::from_slice(&pk_bytes).unwrap();
        let schnorr_sig = secp256k1::schnorr::Signature::from_slice(&sig).unwrap();
        assert!(secp.verify_schnorr(&schnorr_sig, &message, &xonly).is_ok());
    }

    #[test]
    fn sign_attestation_yes_no_differ() {
        let keys = Keys::generate();
        let market_id = MarketId([0xab; 32]);

        let (sig_yes, msg_yes) = sign_attestation(&keys, &market_id, true).unwrap();
        let (sig_no, msg_no) = sign_attestation(&keys, &market_id, false).unwrap();

        assert_ne!(msg_yes, msg_no);
        assert_ne!(sig_yes, sig_no);
    }
}
