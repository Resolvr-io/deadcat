use serde::{Deserialize, Serialize};

use crate::prediction_market::params::PredictionMarketParams;

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
    pub creation_txid: Option<String>,
}
