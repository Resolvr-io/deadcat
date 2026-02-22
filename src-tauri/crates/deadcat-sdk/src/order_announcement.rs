use serde::{Deserialize, Serialize};

use crate::maker_order::params::MakerOrderParams;

/// Published to Nostr, contains maker order params + discovery metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderAnnouncement {
    pub version: u8,
    pub params: MakerOrderParams,
    pub market_id: String,
    pub maker_base_pubkey: String,
    pub order_nonce: String,
    pub covenant_address: String,
    pub offered_amount: u64,
    pub direction_label: String,
}

/// Parsed from a Nostr event — what the taker sees.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredOrder {
    pub id: String,
    pub market_id: String,
    pub base_asset_id: String,
    pub quote_asset_id: String,
    pub price: u64,
    pub min_fill_lots: u64,
    pub min_remainder_lots: u64,
    pub direction: String,
    pub direction_label: String,
    pub maker_base_pubkey: String,
    pub order_nonce: String,
    pub covenant_address: String,
    pub offered_amount: u64,
    pub cosigner_pubkey: String,
    pub maker_receive_spk_hash: String,
    pub creator_pubkey: String,
    pub created_at: u64,
}
