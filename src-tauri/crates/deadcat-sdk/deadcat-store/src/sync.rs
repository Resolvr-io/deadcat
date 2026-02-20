use deadcat_sdk::{MarketId, MarketState};

use crate::store::OrderStatus;

/// A UTXO discovered by a chain source.
///
/// Covenant outputs on Elements are typically **explicit** (not confidential),
/// since Simplicity introspection requires explicit amounts. This means blinding
/// factors are zeros and values are directly readable from the `TxOut`. As a result,
/// `ChainUtxo` does not carry blinding factors — they are stored as zeros in the DB.
#[derive(Debug, Clone)]
pub struct ChainUtxo {
    pub txid: [u8; 32],
    pub vout: u32,
    pub value: u64,
    pub asset_id: [u8; 32],
    /// Serialized `TxOut` bytes for `witness_utxo` in PSETs.
    pub raw_txout: Vec<u8>,
    pub block_height: Option<u32>,
}

/// Trait for querying an Elements/Liquid chain backend.
pub trait ChainSource {
    type Error: std::error::Error + Send + Sync + 'static;

    /// Returns the current best block height.
    fn best_block_height(&self) -> std::result::Result<u32, Self::Error>;

    /// Lists unspent outputs paying to the given `script_pubkey`.
    fn list_unspent(
        &self,
        script_pubkey: &[u8],
    ) -> std::result::Result<Vec<ChainUtxo>, Self::Error>;

    /// Checks if a given outpoint has been spent. Returns `Some(spending_txid)` if spent.
    fn is_spent(
        &self,
        txid: &[u8; 32],
        vout: u32,
    ) -> std::result::Result<Option<[u8; 32]>, Self::Error>;

    /// Returns the raw serialized transaction bytes for a given txid.
    /// `Ok(Some(raw_bytes))` if found, `Ok(None)` if not available.
    fn get_transaction(
        &self,
        txid: &[u8; 32],
    ) -> std::result::Result<Option<Vec<u8>>, Self::Error>;
}

/// Report returned by `DeadcatStore::sync()`.
#[derive(Debug, Clone, Default)]
pub struct SyncReport {
    pub new_utxos: u32,
    pub spent_utxos: u32,
    pub market_state_changes: Vec<MarketStateChange>,
    pub order_status_changes: Vec<OrderStatusChange>,
    pub block_height: u32,
}

#[derive(Debug, Clone)]
pub struct MarketStateChange {
    pub market_id: MarketId,
    pub old_state: MarketState,
    pub new_state: MarketState,
}

#[derive(Debug, Clone)]
pub struct OrderStatusChange {
    pub order_id: i32,
    pub old_status: OrderStatus,
    pub new_status: OrderStatus,
}
