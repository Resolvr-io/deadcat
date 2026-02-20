use std::time::Duration;

use deadcat_sdk::oracle::oracle_message;
use deadcat_sdk::params::MarketId;
use nostr_sdk::prelude::*;
use nostr_sdk::secp256k1;
use serde::{Deserialize, Serialize};

/// Nostr event kind for app-specific data (NIP-78).
pub const CONTRACT_EVENT_KIND: Kind = Kind::Custom(30078);

/// Tag value identifying a deadcat contract announcement.
pub const CONTRACT_TAG: &str = "deadcat-contract";

/// Tag value identifying a deadcat oracle attestation.
pub const ATTESTATION_TAG: &str = "deadcat-attestation";

/// Network tag value for Liquid Testnet.
pub const NETWORK_TAG: &str = "liquid-testnet";

/// Default relay URLs.
pub const DEFAULT_RELAYS: &[&str] = &["wss://relay.damus.io", "wss://relay.primal.net"];

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

pub use deadcat_sdk::announcement::{ContractAnnouncement, ContractMetadata};

/// What the frontend receives — maps to existing Market type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredMarket {
    pub id: String,
    pub nevent: String,
    pub market_id: String,
    pub question: String,
    pub category: String,
    pub description: String,
    pub resolution_source: String,
    pub oracle_pubkey: String,
    pub expiry_height: u32,
    pub cpt_sats: u64,
    pub collateral_asset_id: String,
    pub yes_asset_id: String,
    pub no_asset_id: String,
    pub yes_reissuance_token: String,
    pub no_reissuance_token: String,
    pub starting_yes_price: u8,
    pub creator_pubkey: String,
    pub created_at: u64,
    pub creation_txid: Option<String>,
    pub state: u8,
}

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

/// Result of an oracle attestation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationResult {
    pub market_id: String,
    pub outcome_yes: bool,
    pub signature_hex: String,
    pub nostr_event_id: String,
}

/// Content of an attestation event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationContent {
    pub market_id: String,
    pub outcome_yes: bool,
    pub oracle_signature: String,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn bytes_to_hex(bytes: &[u8; 32]) -> String {
    hex::encode(bytes)
}

// ---------------------------------------------------------------------------
// Nostr event building
// ---------------------------------------------------------------------------

/// Build a Nostr event for a contract announcement.
pub fn build_announcement_event(
    keys: &Keys,
    announcement: &ContractAnnouncement,
) -> Result<Event, String> {
    let market_id = announcement.contract_params.market_id();
    let market_id_hex = bytes_to_hex(market_id.as_bytes());
    let category_lower = announcement.metadata.category.to_lowercase();

    let content =
        serde_json::to_string(announcement).map_err(|e| format!("failed to serialize: {e}"))?;

    let tags = vec![
        Tag::identifier(&market_id_hex),
        Tag::hashtag(CONTRACT_TAG),
        Tag::hashtag(&category_lower),
        Tag::custom(TagKind::custom("network"), vec![NETWORK_TAG.to_string()]),
    ];

    let event = EventBuilder::new(CONTRACT_EVENT_KIND, &content)
        .tags(tags)
        .sign_with_keys(keys)
        .map_err(|e| format!("failed to build event: {e}"))?;

    Ok(event)
}

/// Build a Nostr event for an oracle attestation.
pub fn build_attestation_event(
    keys: &Keys,
    market_id_hex: &str,
    announcement_event_id: &str,
    outcome_yes: bool,
    signature_hex: &str,
    message_hex: &str,
) -> Result<Event, String> {
    let d_tag = format!("{market_id_hex}:attestation");

    let content = serde_json::to_string(&AttestationContent {
        market_id: market_id_hex.to_string(),
        outcome_yes,
        oracle_signature: signature_hex.to_string(),
        message: message_hex.to_string(),
    })
    .map_err(|e| format!("failed to serialize attestation: {e}"))?;

    let outcome_str = if outcome_yes { "yes" } else { "no" };

    let tags = vec![
        Tag::identifier(&d_tag),
        Tag::hashtag(ATTESTATION_TAG),
        Tag::event(
            EventId::from_hex(announcement_event_id)
                .map_err(|e| format!("invalid event id: {e}"))?,
        ),
        Tag::custom(
            TagKind::custom("outcome"),
            vec![outcome_str.to_string()],
        ),
        Tag::custom(TagKind::custom("network"), vec![NETWORK_TAG.to_string()]),
    ];

    let event = EventBuilder::new(CONTRACT_EVENT_KIND, &content)
        .tags(tags)
        .sign_with_keys(keys)
        .map_err(|e| format!("failed to build attestation event: {e}"))?;

    Ok(event)
}

/// Build a Nostr filter for fetching contract announcements.
pub fn build_contract_filter() -> Filter {
    Filter::new()
        .kind(CONTRACT_EVENT_KIND)
        .hashtag(CONTRACT_TAG)
}

/// Build a Nostr filter for fetching attestations for a specific market.
pub fn build_attestation_filter(market_id_hex: &str) -> Filter {
    let d_tag = format!("{market_id_hex}:attestation");
    Filter::new()
        .kind(CONTRACT_EVENT_KIND)
        .identifier(&d_tag)
        .hashtag(ATTESTATION_TAG)
}

// ---------------------------------------------------------------------------
// Event parsing
// ---------------------------------------------------------------------------

/// Parse a Nostr event into a DiscoveredMarket.
pub fn parse_announcement_event(event: &Event) -> Result<DiscoveredMarket, String> {
    let announcement: ContractAnnouncement = serde_json::from_str(&event.content)
        .map_err(|e| format!("failed to parse announcement: {e}"))?;

    let params = &announcement.contract_params;
    let market_id = params.market_id();

    let nevent = Nip19Event::new(event.id, DEFAULT_RELAYS.iter().map(|r| r.to_string()))
        .to_bech32()
        .unwrap_or_default();

    Ok(DiscoveredMarket {
        id: event.id.to_hex(),
        nevent,
        market_id: bytes_to_hex(market_id.as_bytes()),
        question: announcement.metadata.question,
        category: announcement.metadata.category,
        description: announcement.metadata.description,
        resolution_source: announcement.metadata.resolution_source,
        oracle_pubkey: bytes_to_hex(&params.oracle_public_key),
        expiry_height: params.expiry_time,
        cpt_sats: params.collateral_per_token,
        collateral_asset_id: bytes_to_hex(&params.collateral_asset_id),
        yes_asset_id: bytes_to_hex(&params.yes_token_asset),
        no_asset_id: bytes_to_hex(&params.no_token_asset),
        yes_reissuance_token: bytes_to_hex(&params.yes_reissuance_token),
        no_reissuance_token: bytes_to_hex(&params.no_reissuance_token),
        starting_yes_price: announcement.metadata.starting_yes_price,
        creator_pubkey: event.pubkey.to_hex(),
        created_at: event.created_at.as_u64(),
        creation_txid: announcement.creation_txid,
        state: 0, // Default Dormant for discovered markets
    })
}

// ---------------------------------------------------------------------------
// Oracle signing
// ---------------------------------------------------------------------------

/// Sign an oracle attestation using the Nostr keypair.
///
/// The Nostr x-only public key doubles as the oracle signing key.
/// Uses BIP-340 Schnorr signature over SHA256(market_id || outcome_byte).
pub fn sign_attestation(
    keys: &Keys,
    market_id: &MarketId,
    outcome_yes: bool,
) -> Result<([u8; 64], [u8; 32]), String> {
    let msg = oracle_message(market_id, outcome_yes);
    let secp = secp256k1::Secp256k1::new();
    let message = secp256k1::Message::from_digest(msg);
    let secret_bytes = keys.secret_key().as_secret_bytes().to_owned();
    let sk = secp256k1::SecretKey::from_slice(&secret_bytes)
        .map_err(|e| format!("invalid secret key: {e}"))?;
    let keypair = secp256k1::Keypair::from_secret_key(&secp, &sk);
    let sig = secp.sign_schnorr_no_aux_rand(&message, &keypair);
    Ok((sig.serialize(), msg))
}

// ---------------------------------------------------------------------------
// Relay interaction
// ---------------------------------------------------------------------------

/// Connect a Nostr client to the default relays.
pub async fn connect_client(relay_url: Option<&str>) -> Result<Client, String> {
    let client = Client::default();
    if let Some(url) = relay_url {
        client
            .add_relay(url)
            .await
            .map_err(|e| format!("failed to add relay {url}: {e}"))?;
    } else {
        for url in DEFAULT_RELAYS {
            client
                .add_relay(*url)
                .await
                .map_err(|e| format!("failed to add relay {url}: {e}"))?;
        }
    }
    client.connect().await;
    Ok(client)
}

/// Publish an event to the connected relays.
pub async fn publish_event(client: &Client, event: Event) -> Result<EventId, String> {
    let output = client
        .send_event(event)
        .await
        .map_err(|e| format!("failed to send event: {e}"))?;
    Ok(*output.id())
}

/// Fetch contract announcements from relays.
pub async fn fetch_announcements(client: &Client) -> Result<Vec<DiscoveredMarket>, String> {
    let filter = build_contract_filter();
    let events = client
        .fetch_events(vec![filter], Duration::from_secs(15))
        .await
        .map_err(|e| format!("failed to fetch events: {e}"))?;

    let mut markets = Vec::new();
    for event in events.iter() {
        match parse_announcement_event(event) {
            Ok(market) => markets.push(market),
            Err(e) => {
                log::warn!("skipping unparseable announcement {}: {e}", event.id);
            }
        }
    }

    Ok(markets)
}

// ---------------------------------------------------------------------------
// Identity persistence
// ---------------------------------------------------------------------------

/// Load or generate a Nostr keypair.
///
/// Persists the secret key as hex in `<app_data_dir>/nostr_identity.key`.
pub fn load_or_generate_keys(app_data_dir: &std::path::Path) -> Result<Keys, String> {
    let key_path = app_data_dir.join("nostr_identity.key");

    if key_path.exists() {
        let hex_str = std::fs::read_to_string(&key_path)
            .map_err(|e| format!("failed to read key file: {e}"))?;
        let secret_key = SecretKey::from_hex(hex_str.trim())
            .map_err(|e| format!("failed to parse secret key: {e}"))?;
        Ok(Keys::new(secret_key))
    } else {
        let keys = Keys::generate();
        let secret_hex = keys.secret_key().to_secret_hex();
        if let Some(parent) = key_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create data dir: {e}"))?;
        }
        std::fs::write(&key_path, secret_hex)
            .map_err(|e| format!("failed to write key file: {e}"))?;
        Ok(keys)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use deadcat_sdk::ContractParams;

    fn test_metadata() -> ContractMetadata {
        ContractMetadata {
            question: "Will BTC hit 100k?".to_string(),
            description: "Resolves via exchange data.".to_string(),
            category: "Bitcoin".to_string(),
            resolution_source: "Exchange basket".to_string(),
            starting_yes_price: 55,
        }
    }

    fn test_params() -> ContractParams {
        ContractParams {
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
    fn bytes_to_hex_works() {
        let bytes = [0xab; 32];
        let hex_str = bytes_to_hex(&bytes);
        assert_eq!(hex_str.len(), 64);
        assert!(hex_str.starts_with("abab"));
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
        assert!(secp
            .verify_schnorr(&schnorr_sig, &message, &xonly)
            .is_ok());
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
