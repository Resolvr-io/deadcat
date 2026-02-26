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

/// Serializable event payload pushed to the frontend whenever the wallet
/// snapshot changes (after every `with_sdk` call).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletSnapshotEvent {
    pub balance: WalletBalance,
    pub transactions: Vec<WalletTransaction>,
    pub utxos: Vec<WalletUtxo>,
}

impl WalletSnapshotEvent {
    /// Convert an SDK `WalletSnapshot` into the serializable frontend payload.
    pub fn from_snapshot(
        snapshot: &deadcat_sdk::node::WalletSnapshot,
        policy_asset: &lwk_wollet::elements::AssetId,
    ) -> Self {
        let mut assets = HashMap::new();
        for (asset_id, amount) in &snapshot.balance {
            if *amount > 0 {
                assets.insert(asset_id.to_string(), *amount);
            }
        }

        let transactions = snapshot
            .transactions
            .iter()
            .map(|tx| {
                let balance_change = tx.balance.get(policy_asset).copied().unwrap_or(0);
                WalletTransaction {
                    txid: tx.txid.to_string(),
                    balance_change,
                    fee: tx.fee,
                    height: tx.height,
                    timestamp: tx.timestamp,
                    tx_type: tx.type_.clone(),
                }
            })
            .collect();

        let utxos = snapshot
            .utxos
            .iter()
            .map(|u| WalletUtxo {
                txid: u.outpoint.txid.to_string(),
                vout: u.outpoint.vout,
                asset_id: u.unblinded.asset.to_string(),
                value: u.unblinded.value,
                height: u.height,
            })
            .collect();

        Self {
            balance: WalletBalance { assets },
            transactions,
            utxos,
        }
    }
}
