pub use simplicityhl::elements;
pub use simplicityhl::simplicity;

pub(crate) mod announcement;
pub(crate) mod assembly;
pub(crate) mod chain;
pub(crate) mod discovery;
pub(crate) mod error;
pub(crate) mod history;
pub(crate) mod lmsr_pool;
#[cfg(any(test, feature = "testing"))]
pub mod maker_order;
#[cfg(not(any(test, feature = "testing")))]
pub(crate) mod maker_order;
pub(crate) mod network;
pub(crate) mod node;
pub(crate) mod pool;
pub(crate) mod prediction_market;
#[doc(hidden)]
pub mod prediction_market_scan;
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
pub use announcement::{CONTRACT_ANNOUNCEMENT_VERSION, ContractAnnouncement, ContractMetadata};
pub use error::{Error, NodeError, Result};
pub use history::{
    LmsrPoolSyncInfo, LmsrPoolSyncRepairInput, LmsrPriceHistoryEntry, LmsrPriceTransitionInput,
};
pub use network::Network;
pub use node::DeadcatNode;
pub use prediction_market::anchor::{
    DormantOutputOpening, PredictionMarketAnchor, parse_market_creation_txid,
    parse_prediction_market_anchor,
};
pub use prediction_market::contract::CompiledPredictionMarket;
pub use prediction_market::params::{MarketId, PredictionMarketParams};
pub use prediction_market::state::{MarketSlot, MarketState};
pub use pset::UnblindedUtxo;
pub use sdk::{
    CancelOrderResult, CancellationResult, CreateOrderResult, FillOrderResult, IssuanceResult,
    RedemptionResult, ResolutionResult,
};
pub use taproot::NUMS_KEY_BYTES;

// Re-export LWK for app-layer use
pub use lwk_wollet;

// ── Node ──────────────────────────────────────────────────────────
pub use node::WalletSnapshot;

// ── Maker orders ───────────────────────────────────────────────────
pub use maker_order::contract::CompiledMakerOrder;
pub use maker_order::params::{
    MakerOrderParams, OrderDirection, derive_maker_receive, maker_receive_script_pubkey,
};

// ── LMSR pools ─────────────────────────────────────────────────────
pub use lmsr_pool::api::{
    AdjustLmsrPoolRequest, AdjustLmsrPoolResult, CloseLmsrPoolRequest, CloseLmsrPoolResult,
    CreateLmsrPoolRequest, CreateLmsrPoolResult, LmsrPoolLocator, LmsrPoolSnapshot,
    build_pool_announcement_from_snapshot,
};
pub use lmsr_pool::contract::CompiledLmsrPool;
pub use lmsr_pool::math::{
    LmsrQuote, LmsrTradeKind, fee_free_yes_spot_price_bps, max_collateral_out, min_collateral_in,
    quote_exact_input_from_manifest, quote_from_table,
};
pub use lmsr_pool::params::{LmsrInitialOutpoint, LmsrPoolId, LmsrPoolIdInput, LmsrPoolParams};
pub use lmsr_pool::table::{
    LmsrTableManifest, generate_lmsr_table, lmsr_table_leaf_hash, lmsr_table_node_hash,
    lmsr_table_root,
};

// ── Pool helpers ───────────────────────────────────────────────────
pub use pool::{PoolReserves, implied_probability_bps};

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
    LmsrPoolIngestInput,
    LmsrPoolStateSource,
    LmsrPoolStateUpdateInput,
    NETWORK_TAG,
    NodeStore,
    NoopStore,
    OrderAnnouncement,
    OwnMakerOrderRecordInput,
    OwnOrderStatusChange,
    PendingOrderDeletion,
    PoolAnnouncement,
    PoolParams,
    PredictionMarketCandidateIngestInput,
    StoredOrderStatus,
    // Functions
    build_announcement_event,
    build_attestation_event,
    build_attestation_filter,
    build_contract_filter,
    build_pool_event,
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
