use crate::amm_pool::math::PoolReserves;
use crate::amm_pool::params::{AmmPoolParams, PoolId};
use crate::maker_order::params::MakerOrderParams;
use crate::prediction_market::params::{MarketId, PredictionMarketParams};

/// Metadata passed alongside a market when persisting to the store.
#[derive(Debug, Clone, Default)]
pub struct ContractMetadataInput {
    pub question: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub resolution_source: Option<String>,
    pub creator_pubkey: Option<Vec<u8>>,
    pub creation_txid: Option<String>,
    pub nevent: Option<String>,
    pub nostr_event_id: Option<String>,
    pub nostr_event_json: Option<String>,
}

/// Trait abstracting store operations needed by `DiscoveryService`.
///
/// This avoids a circular dependency between `deadcat-sdk` and `deadcat-store`.
/// The `deadcat-store` crate implements this trait for `DeadcatStore`.
pub trait DiscoveryStore: Send + 'static {
    /// Persist a discovered market. If it already exists, this should be a no-op.
    fn ingest_market(
        &mut self,
        params: &PredictionMarketParams,
        meta: Option<&ContractMetadataInput>,
    ) -> Result<(), String>;

    /// Persist a discovered maker order. If it already exists, this should be a no-op.
    fn ingest_maker_order(
        &mut self,
        params: &MakerOrderParams,
        maker_pubkey: Option<&[u8; 32]>,
        nonce: Option<&[u8; 32]>,
        nostr_event_id: Option<&str>,
        nostr_event_json: Option<&str>,
    ) -> Result<(), String>;

    /// Persist a discovered AMM pool. If it already exists, update state.
    #[allow(clippy::too_many_arguments)]
    fn ingest_amm_pool(
        &mut self,
        params: &AmmPoolParams,
        issued_lp: u64,
        nostr_event_id: Option<&str>,
        nostr_event_json: Option<&str>,
        market_id: Option<&[u8; 32]>,
        creation_txid: Option<&[u8; 32]>,
    ) -> Result<(), String>;

    /// Update pool state (issued_lp, covenant_spk).
    /// Reserves are tracked exclusively in pool_state_snapshots.
    fn update_pool_state(
        &mut self,
        pool_id: &crate::amm_pool::params::PoolId,
        params: &AmmPoolParams,
        issued_lp: u64,
    ) -> Result<(), String>;

    /// Get a pool's params, creation_txid, and pool_id by PoolId.
    /// Returns (params, pool_id_bytes, creation_txid_bytes) or None.
    fn get_pool_info(
        &mut self,
        pool_id: &crate::amm_pool::params::PoolId,
    ) -> Result<Option<PoolInfo>, String>;

    /// Get the latest pool snapshot (txid, issued_lp) for incremental sync.
    fn get_latest_pool_snapshot_resume(
        &mut self,
        pool_id: &[u8; 32],
    ) -> Result<Option<([u8; 32], u64)>, String>;

    /// Insert a pool state snapshot (idempotent).
    #[allow(clippy::too_many_arguments)]
    fn insert_pool_snapshot(
        &mut self,
        pool_id: &[u8; 32],
        txid: &[u8; 32],
        r_yes: u64,
        r_no: u64,
        r_lbtc: u64,
        issued_lp: u64,
        block_height: Option<i32>,
    ) -> Result<(), String>;

    /// Find the active pool for a market.
    fn get_pool_id_for_market(&mut self, market_id: &MarketId) -> Result<Option<PoolId>, String>;

    /// Most recent pool state snapshot.
    fn get_latest_pool_snapshot(
        &mut self,
        pool_id: &PoolId,
    ) -> Result<Option<PoolSnapshot>, String>;

    /// All pool state snapshots in chronological order.
    fn get_pool_snapshot_history(&mut self, pool_id: &PoolId) -> Result<Vec<PoolSnapshot>, String>;
}

/// Lightweight pool info returned by `DiscoveryStore::get_pool_info`.
#[derive(Debug, Clone)]
pub struct PoolInfo {
    pub params: AmmPoolParams,
    pub pool_id: [u8; 32],
    pub creation_txid: Option<[u8; 32]>,
}

/// A single pool state observation (reserves + LP supply at a point in time).
#[derive(Debug, Clone)]
pub struct PoolSnapshot {
    pub reserves: PoolReserves,
    pub issued_lp: u64,
    pub block_height: Option<i32>,
}
