use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WalletStatus {
    NotCreated,
    Locked,
    Unlocked,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletBalance {
    /// Map of asset_id hex -> satoshi amount
    pub assets: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletAddress {
    pub index: u32,
    pub address: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletUtxo {
    pub txid: String,
    pub vout: u32,
    pub asset_id: String,
    pub value: u64,
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletTransaction {
    pub txid: String,
    /// Net L-BTC balance change in satoshis (positive = received, negative = sent)
    pub balance_change: i64,
    pub fee: u64,
    pub height: Option<u32>,
    pub timestamp: Option<u32>,
    /// Transaction type from LWK: "issuance", "reissuance", "burn", "incoming", "outgoing", etc.
    pub tx_type: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiquidSendResult {
    pub txid: String,
    pub fee_sat: u64,
}
