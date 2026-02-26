pub use simplicityhl::elements;
pub use simplicityhl::simplicity;

pub mod amm_pool;
pub mod announcement;
pub(crate) mod assembly;
pub(crate) mod chain;
pub mod discovery;
pub(crate) mod error;
pub mod maker_order;
pub(crate) mod network;
pub mod node;
pub mod prediction_market;
pub(crate) mod pset;
pub(crate) mod sdk;
#[cfg(any(test, feature = "testing"))]
pub mod taproot;
#[cfg(not(any(test, feature = "testing")))]
pub(crate) mod taproot;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
pub(crate) mod trade;

// ── Core types ─────────────────────────────────────────────────────
pub use error::{Error, NodeError, Result};
pub use network::Network;
pub use node::DeadcatNode;
pub use prediction_market::contract::CompiledPredictionMarket;
pub use prediction_market::params::{MarketId, PredictionMarketParams};
pub use prediction_market::state::MarketState;
pub use pset::UnblindedUtxo;
pub use sdk::{
    CancelOrderResult, CancellationResult, CreateOrderResult, FillOrderResult, IssuanceResult,
    PoolCreationResult, PoolLpResult, PoolSwapResult, RedemptionResult, ResolutionResult,
};

// Re-export LWK for app-layer use
pub use lwk_wollet;

// ── Maker orders ───────────────────────────────────────────────────
pub use maker_order::contract::CompiledMakerOrder;
pub use maker_order::params::{
    MakerOrderParams, OrderDirection, derive_maker_receive, maker_receive_script_pubkey,
};

// ── AMM pools ──────────────────────────────────────────────────────
pub use amm_pool::contract::CompiledAmmPool;
pub use amm_pool::params::{AmmPoolParams, PoolId};

// ── Trade routing ──────────────────────────────────────────────────
pub use trade::types::{
    LiquiditySource, RouteLeg, TradeAmount, TradeDirection, TradeQuote, TradeResult, TradeSide,
};

// ── Testing-only re-exports ────────────────────────────────────────
// Internals exposed for integration tests; not part of the stable API.
// Access via `pub mod` paths (taproot, maker_order, discovery, etc.)
// for anything not listed here.
#[cfg(feature = "testing")]
pub use prediction_market::assembly::{
    CollateralSource, IssuanceAssemblyInputs, IssuanceEntropy, compute_issuance_entropy,
};
#[cfg(feature = "testing")]
pub use prediction_market::oracle::oracle_message;
#[cfg(feature = "testing")]
pub use prediction_market::witness::{
    AllBlindingFactors, PredictionMarketSpendingPath, ReissuanceBlindingFactors, satisfy_contract,
    satisfy_contract_with_env, serialize_satisfied,
};
#[cfg(feature = "testing")]
pub use sdk::DeadcatSdk;
