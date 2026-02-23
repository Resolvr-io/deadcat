use nostr_sdk::{Keys, SecretKey};
use serde::{Deserialize, Serialize};

// Re-export SDK-owned types and functions for use by the app layer.
pub use deadcat_sdk::announcement::{ContractAnnouncement, ContractMetadata};
pub use deadcat_sdk::discovery::{
    // Types
    AttestationContent, AttestationResult, DiscoveredMarket,
    // Builders
    build_announcement_event, build_attestation_event, build_attestation_filter,
    build_contract_filter, parse_announcement_event, sign_attestation,
    // Conversions
    discovered_market_to_contract_params,
    // Relay helpers
    connect_client, publish_event, fetch_announcements,
    // Constants
    APP_EVENT_KIND, CONTRACT_TAG, ATTESTATION_TAG,
    NETWORK_TAG, DEFAULT_RELAYS,
};

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
