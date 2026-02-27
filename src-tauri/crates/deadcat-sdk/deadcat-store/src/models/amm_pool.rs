use diesel::prelude::*;

use crate::schema::amm_pools;

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = amm_pools)]
pub struct AmmPoolRow {
    pub pool_id: Vec<u8>,
    pub yes_asset_id: Vec<u8>,
    pub no_asset_id: Vec<u8>,
    pub lbtc_asset_id: Vec<u8>,
    pub lp_asset_id: Vec<u8>,
    pub lp_reissuance_token_id: Vec<u8>,
    pub fee_bps: i32,
    pub cosigner_pubkey: Vec<u8>,
    pub cmr: Vec<u8>,
    pub issued_lp: i64,
    /// Cached: re-derivable from AmmPoolParams + issued_lp via
    /// `CompiledAmmPool::new(params)?.script_pubkey(issued_lp)`.
    /// Stored to avoid ~800ms Simplicity recompilation per lookup.
    pub covenant_spk: Vec<u8>,
    pub pool_status: i32,
    pub created_at: String,
    pub updated_at: String,
    pub nostr_event_id: Option<String>,
    pub nostr_event_json: Option<String>,
    pub market_id: Option<Vec<u8>>,
    pub creation_txid: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = amm_pools)]
pub struct NewAmmPoolRow {
    pub pool_id: Vec<u8>,
    pub yes_asset_id: Vec<u8>,
    pub no_asset_id: Vec<u8>,
    pub lbtc_asset_id: Vec<u8>,
    pub lp_asset_id: Vec<u8>,
    pub lp_reissuance_token_id: Vec<u8>,
    pub fee_bps: i32,
    pub cosigner_pubkey: Vec<u8>,
    pub cmr: Vec<u8>,
    pub issued_lp: i64,
    pub covenant_spk: Vec<u8>,
    pub pool_status: i32,
    pub nostr_event_id: Option<String>,
    pub nostr_event_json: Option<String>,
    pub market_id: Option<Vec<u8>>,
    pub creation_txid: Option<Vec<u8>>,
}
