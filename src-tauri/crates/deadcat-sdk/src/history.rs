use serde::{Deserialize, Serialize};

use crate::lmsr_pool::params::LmsrPoolParams;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LmsrPoolSyncInfo {
    pub pool_id: String,
    pub market_id: String,
    pub creation_txid: String,
    pub stored_initial_reserve_outpoints: Option<[String; 3]>,
    pub witness_schema_version: String,
    pub current_s_index: u64,
    pub params_json: String,
    pub lmsr_table_values: Option<Vec<u64>>,
    pub nostr_event_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LmsrPoolSyncRepairInput {
    pub pool_id: String,
    pub market_id: String,
    pub creation_txid: String,
    pub witness_schema_version: String,
    pub params: LmsrPoolParams,
    pub initial_reserve_outpoints: [String; 3],
    pub lmsr_table_values: Option<Vec<u64>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LmsrPriceTransitionInput {
    pub pool_id: String,
    pub market_id: String,
    pub transition_txid: String,
    pub old_s_index: u64,
    pub new_s_index: u64,
    pub reserve_yes: u64,
    pub reserve_no: u64,
    pub reserve_collateral: u64,
    pub implied_yes_price_bps: u16,
    pub block_height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LmsrPriceHistoryEntry {
    pub pool_id: String,
    pub market_id: String,
    pub transition_txid: String,
    pub old_s_index: u64,
    pub new_s_index: u64,
    pub reserve_yes: u64,
    pub reserve_no: u64,
    pub reserve_collateral: u64,
    pub implied_yes_price_bps: u16,
    pub block_height: u32,
}
