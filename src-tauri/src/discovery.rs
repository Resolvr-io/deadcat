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

/// Load or generate a Nostr keypair.
///
/// Persists the secret key as hex in `<app_data_dir>/nostr_identity.key`.
pub fn load_or_generate_keys(
    app_data_dir: &std::path::Path,
) -> Result<nostr_sdk::Keys, String> {
    use nostr_sdk::prelude::*;

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

