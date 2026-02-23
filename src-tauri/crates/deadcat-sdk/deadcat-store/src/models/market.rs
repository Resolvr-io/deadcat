use diesel::prelude::*;

use crate::schema::markets;

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = markets)]
pub struct MarketRow {
    pub market_id: Vec<u8>,
    pub oracle_public_key: Vec<u8>,
    pub collateral_asset_id: Vec<u8>,
    pub yes_token_asset: Vec<u8>,
    pub no_token_asset: Vec<u8>,
    pub yes_reissuance_token: Vec<u8>,
    pub no_reissuance_token: Vec<u8>,
    pub collateral_per_token: i64,
    pub expiry_time: i32,
    pub cmr: Vec<u8>,
    pub dormant_spk: Vec<u8>,
    pub unresolved_spk: Vec<u8>,
    pub resolved_yes_spk: Vec<u8>,
    pub resolved_no_spk: Vec<u8>,
    pub current_state: i32,
    pub created_at: String,
    pub updated_at: String,
    pub yes_issuance_entropy: Option<Vec<u8>>,
    pub no_issuance_entropy: Option<Vec<u8>>,
    pub yes_issuance_blinding_nonce: Option<Vec<u8>>,
    pub no_issuance_blinding_nonce: Option<Vec<u8>>,
    pub question: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub resolution_source: Option<String>,
    pub starting_yes_price: Option<i32>,
    pub creator_pubkey: Option<Vec<u8>>,
    pub creation_txid: Option<String>,
    pub nevent: Option<String>,
    pub nostr_event_id: Option<String>,
    pub nostr_event_json: Option<String>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = markets)]
pub struct NewMarketRow {
    pub market_id: Vec<u8>,
    pub oracle_public_key: Vec<u8>,
    pub collateral_asset_id: Vec<u8>,
    pub yes_token_asset: Vec<u8>,
    pub no_token_asset: Vec<u8>,
    pub yes_reissuance_token: Vec<u8>,
    pub no_reissuance_token: Vec<u8>,
    pub collateral_per_token: i64,
    pub expiry_time: i32,
    pub cmr: Vec<u8>,
    pub dormant_spk: Vec<u8>,
    pub unresolved_spk: Vec<u8>,
    pub resolved_yes_spk: Vec<u8>,
    pub resolved_no_spk: Vec<u8>,
    pub question: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub resolution_source: Option<String>,
    pub starting_yes_price: Option<i32>,
    pub creator_pubkey: Option<Vec<u8>>,
    pub creation_txid: Option<String>,
    pub nevent: Option<String>,
    pub nostr_event_id: Option<String>,
    pub nostr_event_json: Option<String>,
}
