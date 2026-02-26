pub use simplicityhl::elements;
pub use simplicityhl::simplicity;

pub mod amm_pool;
pub mod announcement;
pub(crate) mod assembly;
pub(crate) mod chain;
pub(crate) mod contract;
pub mod discovery;
pub(crate) mod error;
pub mod maker_order;
pub(crate) mod network;
pub mod node;
pub(crate) mod oracle;
pub mod params;
pub(crate) mod pset;
pub(crate) mod sdk;
pub(crate) mod state;
#[cfg(any(test, feature = "testing"))]
pub mod taproot;
#[cfg(not(any(test, feature = "testing")))]
pub(crate) mod taproot;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
pub(crate) mod trade;
pub(crate) mod witness;

// ── Core types ─────────────────────────────────────────────────────
pub use contract::CompiledContract;
pub use error::{Error, NodeError, Result};
pub use network::Network;
pub use node::DeadcatNode;
pub use params::{ContractParams, MarketId};
pub use pset::UnblindedUtxo;
pub use sdk::{
    CancelOrderResult, CancellationResult, CreateOrderResult, FillOrderResult, IssuanceResult,
    PoolCreationResult, PoolLpResult, PoolSwapResult, RedemptionResult, ResolutionResult,
};
pub use state::MarketState;

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
pub use assembly::{
    CollateralSource, IssuanceAssemblyInputs, IssuanceEntropy, compute_issuance_entropy,
};
#[cfg(feature = "testing")]
pub use oracle::oracle_message;
#[cfg(feature = "testing")]
pub use sdk::DeadcatSdk;
#[cfg(feature = "testing")]
pub use witness::{
    AllBlindingFactors, ReissuanceBlindingFactors, SpendingPath, satisfy_contract,
    satisfy_contract_with_env, serialize_satisfied,
};
