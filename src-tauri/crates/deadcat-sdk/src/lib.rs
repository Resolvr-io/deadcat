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
pub(crate) mod tx;

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
    CancelOrderResult, CancellationResult, ContractCreationResult, CreateOrderResult,
    FillOrderResult, IssuanceResult, PreparedCancelOrder, PreparedCancellation,
    PreparedContractCreation, PreparedCreateOrder, PreparedFillOrder, PreparedIssuance,
    PreparedRedemption, PreparedResolution, PreparedSendLbtc, RedemptionResult, ResolutionResult,
};
pub use taproot::NUMS_KEY_BYTES;

// Re-export LWK for app-layer use
pub use lwk_wollet;

// ── Node ──────────────────────────────────────────────────────────
pub use node::{
    MarketCreationResult, PreparedLimitOrderCancellation, PreparedLimitOrderCreation,
    PreparedMarketCreation, WalletSnapshot,
};

// ── Maker orders ───────────────────────────────────────────────────
pub use maker_order::contract::CompiledMakerOrder;
pub use maker_order::params::{
    MakerOrderParams, OrderDirection, derive_maker_receive, maker_receive_script_pubkey,
};

// ── LMSR pools ─────────────────────────────────────────────────────
pub use lmsr_pool::api::{
    AdjustLmsrPoolRequest, AdjustLmsrPoolResult, CloseLmsrPoolRequest, CloseLmsrPoolResult,
    CreateLmsrPoolRequest, CreateLmsrPoolResult, LmsrPoolLocator, LmsrPoolSnapshot,
    PreparedAdjustLmsrPool, PreparedCloseLmsrPool, PreparedCreateLmsrPool,
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
    LiquiditySource, PreparedTrade, RouteLeg, TradeAmount, TradeDirection, TradeQuote, TradeResult,
    TradeSide,
};
pub use tx::{MinerFeePolicy, PreparedTransaction, ResolvedMinerFee, TxOptions};

// ── Discovery ─────────────────────────────────────────────────────
pub use discovery::{
    // Constants
    APP_EVENT_KIND,
    ATTESTATION_TAG,
    // Types
    AttestationContent,
    AttestationResult,
    CLIENT_NAME,
    COMMENT_KIND,
    CONTRACT_TAG,
    CommentParent,
    CommentRoot,
    ContractMetadataInput,
    DEFAULT_RELAYS,
    DEFAULT_SOURCE_NPUB,
    DiscoveredMarket,
    DiscoveredOrder,
    DiscoveredPool,
    DiscoveryConfig,
    DiscoveryEvent,
    DiscoveryService,
    DiscoveryStore,
    EventReactionSummary,
    FOLLOW_LIST_KIND,
    FollowList,
    LmsrPoolIngestInput,
    LmsrPoolStateSource,
    LmsrPoolStateUpdateInput,
    MAX_COMMENT_LEN,
    MUTE_LIST_KIND,
    MarketComment,
    MuteEntry,
    MuteList,
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
    REACTION_KIND,
    ReactionCount,
    StoredOrderStatus,
    ZapRequest,
    ZapSummary,
    // Functions
    build_announcement_event,
    build_attestation_event,
    build_attestation_filter,
    build_comment_deletion_event,
    build_comment_event,
    build_comment_filter_for_market,
    build_contract_filter,
    build_follow_list_event,
    build_mute_list_event,
    build_pool_event,
    build_reaction_deletion_event,
    build_reaction_event,
    build_zap_request_event,
    client_tag,
    connect_client,
    deserialize_private_mutes,
    discovered_market_to_contract_params,
    fetch_announcements,
    fetch_follow_list_event,
    fetch_market_comments,
    fetch_mute_list_event,
    fetch_reaction_summaries_for_events,
    fetch_zap_summaries_for_events,
    parse_announcement_event,
    parse_comment_event,
    parse_follow_list,
    parse_mute_list,
    publish_event,
    serialize_private_mutes,
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
