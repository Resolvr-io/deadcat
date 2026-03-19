//! Persistent storage for discovery candidates, canonical markets, maker
//! orders, and their synced covenant state.
//!
//! Prediction markets are modeled in two stages:
//! - `market_candidates`: level-2-valid off-chain announcements keyed by
//!   `market_id + anchor`
//! - `markets`: the single canonical on-chain market per `market_id`
//!
//! This crate does not handle reorgs. Callers must only promote candidates and
//! apply canonical market sync updates once the relevant transactions are
//! irreversible on Liquid.

mod conversions;
mod error;
mod models;
mod schema;
mod store;
mod sync;

/// Hard Liquid finality threshold used by higher-level promotion/sync code.
///
/// `deadcat-store` assumes no reorg handling; once callers promote a
/// prediction-market candidate after this many confirmations, the canonical row
/// is treated as final.
pub const LIQUID_IRREVERSIBLE_CONFIRMATIONS: u32 = 2;

pub use deadcat_sdk::{
    ContractMetadataInput, LmsrPoolIngestInput, LmsrPoolStateSource, LmsrPoolStateUpdateInput,
    MarketSlot, MarketState, PredictionMarketCandidateIngestInput,
};
pub use error::StoreError;
pub use store::{
    DeadcatStore, IssuanceData, MakerOrderInfo, MarketCandidateFilter, MarketCandidateInfo,
    MarketFilter, MarketInfo, OrderFilter, OrderStatus,
};
pub use sync::{ChainSource, ChainUtxo, MarketStateChange, OrderStatusChange, SyncReport};

pub type Result<T> = std::result::Result<T, StoreError>;
