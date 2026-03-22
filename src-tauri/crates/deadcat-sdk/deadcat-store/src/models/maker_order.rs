use diesel::prelude::*;

use crate::schema::maker_orders;

#[derive(Debug, Clone, Queryable, Selectable)]
#[diesel(table_name = maker_orders)]
pub struct MakerOrderRow {
    pub id: i32,
    pub base_asset_id: Vec<u8>,
    pub quote_asset_id: Vec<u8>,
    pub price: i64,
    pub min_fill_lots: i64,
    pub min_remainder_lots: i64,
    pub direction: i32,
    pub maker_receive_spk_hash: Vec<u8>,
    pub cosigner_pubkey: Vec<u8>,
    pub cmr: Vec<u8>,
    pub maker_base_pubkey: Option<Vec<u8>>,
    /// Cached: re-derivable via `CompiledMakerOrder::new(params)?.script_pubkey(pubkey)`.
    /// Stored to avoid ~100ms Simplicity recompilation per lookup.
    pub covenant_spk: Option<Vec<u8>>,
    pub order_status: i32,
    pub created_at: String,
    pub updated_at: String,
    pub order_nonce: Option<Vec<u8>>,
    pub maker_receive_spk: Option<Vec<u8>>,
    pub nostr_event_id: Option<String>,
    pub nostr_event_json: Option<String>,
    pub creation_txid: Option<String>,
    pub market_id: Option<String>,
    pub direction_label: Option<String>,
    pub offered_amount: Option<i64>,
    pub nostr_delete_event_id: Option<String>,
    pub nostr_delete_requested_at: Option<String>,
    pub nostr_delete_last_error: Option<String>,
}

#[derive(Debug, Clone, Insertable)]
#[diesel(table_name = maker_orders)]
pub struct NewMakerOrderRow {
    pub base_asset_id: Vec<u8>,
    pub quote_asset_id: Vec<u8>,
    pub price: i64,
    pub min_fill_lots: i64,
    pub min_remainder_lots: i64,
    pub direction: i32,
    pub maker_receive_spk_hash: Vec<u8>,
    pub cosigner_pubkey: Vec<u8>,
    pub cmr: Vec<u8>,
    pub maker_base_pubkey: Option<Vec<u8>>,
    pub covenant_spk: Option<Vec<u8>>,
    pub order_nonce: Option<Vec<u8>>,
    pub maker_receive_spk: Option<Vec<u8>>,
    pub nostr_event_id: Option<String>,
    pub nostr_event_json: Option<String>,
    pub creation_txid: Option<String>,
    pub market_id: Option<String>,
    pub direction_label: Option<String>,
    pub offered_amount: Option<i64>,
    pub nostr_delete_event_id: Option<String>,
    pub nostr_delete_requested_at: Option<String>,
    pub nostr_delete_last_error: Option<String>,
}
