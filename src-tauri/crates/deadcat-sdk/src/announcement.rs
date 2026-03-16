use serde::{Deserialize, Serialize};

use crate::prediction_market::anchor::PredictionMarketAnchor;
use crate::prediction_market::params::PredictionMarketParams;

pub const CONTRACT_ANNOUNCEMENT_VERSION: u8 = 4;

/// Off-chain, human-readable fields from the UI create form.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractMetadata {
    pub question: String,
    pub description: String,
    pub category: String,
    pub resolution_source: String,
}

/// Published to Nostr, contains both SDK params + metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractAnnouncement {
    pub version: u8,
    pub contract_params: PredictionMarketParams,
    pub metadata: ContractMetadata,
    pub anchor: PredictionMarketAnchor,
    /// Full serialized creation transaction hex for mandatory level-2 validation.
    pub creation_tx_hex: String,
}
