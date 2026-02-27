pub use simplicityhl::elements;
pub use simplicityhl::simplicity;

pub(crate) mod amm_pool;
pub(crate) mod announcement;
pub(crate) mod assembly;
pub(crate) mod chain;
pub(crate) mod discovery;
pub(crate) mod error;
#[cfg(any(test, feature = "testing"))]
pub mod maker_order;
#[cfg(not(any(test, feature = "testing")))]
pub(crate) mod maker_order;
pub(crate) mod network;
pub(crate) mod node;
pub(crate) mod prediction_market;
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
pub use announcement::{ContractAnnouncement, ContractMetadata};
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

// ── Node ──────────────────────────────────────────────────────────
pub use node::WalletSnapshot;

// ── Maker orders ───────────────────────────────────────────────────
pub use maker_order::contract::CompiledMakerOrder;
pub use maker_order::params::{
    MakerOrderParams, OrderDirection, derive_maker_receive, maker_receive_script_pubkey,
};

// ── AMM pools ──────────────────────────────────────────────────────
pub use amm_pool::contract::CompiledAmmPool;
pub use amm_pool::math::{PoolReserves, SwapPair};
pub use amm_pool::params::{AmmPoolParams, PoolId};

// ── Trade routing ──────────────────────────────────────────────────
pub use trade::types::{
    LiquiditySource, RouteLeg, TradeAmount, TradeDirection, TradeQuote, TradeResult, TradeSide,
};

// ── Discovery ─────────────────────────────────────────────────────
pub use discovery::{
    // Constants
    APP_EVENT_KIND,
    ATTESTATION_TAG,
    // Types
    AttestationContent,
    AttestationResult,
    CONTRACT_TAG,
    ContractMetadataInput,
    DEFAULT_RELAYS,
    DiscoveredMarket,
    DiscoveredOrder,
    DiscoveredPool,
    DiscoveryConfig,
    DiscoveryEvent,
    DiscoveryService,
    DiscoveryStore,
    NETWORK_TAG,
    NoopStore,
    OrderAnnouncement,
    PoolAnnouncement,
    // Functions
    build_announcement_event,
    build_attestation_event,
    build_attestation_filter,
    build_contract_filter,
    connect_client,
    discovered_market_to_contract_params,
    fetch_announcements,
    parse_announcement_event,
    publish_event,
    sign_attestation,
};

// ── Testing-only re-exports ────────────────────────────────────────
// Internals exposed for integration tests; not part of the stable API.
// Access via `pub mod` paths (taproot, maker_order) for anything not
// listed here.
#[cfg(feature = "testing")]
pub use discovery::build_order_event;
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
