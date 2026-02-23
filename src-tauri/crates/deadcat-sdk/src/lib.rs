pub use simplicityhl::elements;
pub use simplicityhl::simplicity;

pub mod announcement;
pub mod assembly;
pub mod chain;
pub mod contract;
pub mod discovery;
pub mod error;
pub mod maker_order;
pub mod network;
pub mod node;
pub mod oracle;
pub mod params;
pub mod pset;
pub mod sdk;
pub mod state;
pub mod taproot;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
pub mod witness;

// Core types
pub use assembly::{
    AssembledIssuance, AssembledTransaction, CollateralSource, IssuanceAssemblyInputs,
    IssuanceEntropy, assemble_cancellation, assemble_expiry_redemption, assemble_issuance,
    assemble_oracle_resolve, assemble_post_resolution_redemption, build_issuance_pset,
    compute_issuance_entropy,
};
pub use chain::{ChainBackend, ElectrumBackend};
pub use contract::CompiledContract;
pub use error::{Error, NodeError, Result};
pub use network::Network;
pub use params::{ContractParams, IssuanceAssets, MarketId, compute_issuance_assets};
pub use sdk::{
    CancelOrderResult, CancellationResult, CreateOrderResult, DeadcatSdk, FillOrderResult,
    IssuanceResult, RedemptionResult, ResolutionResult,
};
pub use node::DeadcatNode;
pub use state::MarketState;

// Re-export LWK for app-layer use
pub use lwk_wollet;

// Witness types and API
pub use witness::{
    AllBlindingFactors, ReissuanceBlindingFactors, SpendingPath, satisfy_contract,
    serialize_satisfied,
};

// PSET types and builders
pub use pset::UnblindedUtxo;
pub use pset::cancellation::{CancellationParams, build_cancellation_pset};
pub use pset::creation::{CreationParams, build_creation_pset};
pub use pset::expiry_redemption::{ExpiryRedemptionParams, build_expiry_redemption_pset};
pub use pset::initial_issuance::{InitialIssuanceParams, build_initial_issuance_pset};
pub use pset::issuance::{SubsequentIssuanceParams, build_subsequent_issuance_pset};
pub use pset::oracle_resolve::{OracleResolveParams, build_oracle_resolve_pset};
pub use pset::post_resolution_redemption::{
    PostResolutionRedemptionParams, build_post_resolution_redemption_pset,
};

// Taproot utilities
pub use taproot::{MarketAddresses, nums_internal_key, simplicity_control_block};

// Maker order types and builders
pub use maker_order::contract::CompiledMakerOrder;
pub use maker_order::params::{MakerOrderParams, OrderDirection};
pub use maker_order::params::{
    derive_maker_receive, derive_p_order, maker_receive_script_pubkey, maker_receive_spk_hash,
    order_tweak, order_uid,
};
pub use maker_order::pset::cancel_order::{CancelOrderParams, build_cancel_order_pset};
pub use maker_order::pset::create_order::{CreateOrderParams, build_create_order_pset};
pub use maker_order::pset::fill_order::{
    FillOrderParams, MakerOrderFill, TakerFill, build_fill_order_pset,
};
pub use maker_order::taproot::{
    maker_order_address, maker_order_control_block, maker_order_script_hash,
    maker_order_script_pubkey,
};
pub use maker_order::witness::{
    build_maker_order_cancel_witness, build_maker_order_fill_witness, build_maker_order_witness,
    satisfy_maker_order, serialize_satisfied as serialize_maker_order_satisfied,
};

// Discovery (replaces order_announcement + order_discovery)
pub use discovery::{
    // Types
    DiscoveredMarket, DiscoveredOrder, OrderAnnouncement,
    DiscoveryConfig, DiscoveryEvent, DiscoveryService,
    AttestationContent, AttestationResult,
    ContractMetadataInput, DiscoveryStore,
    // Order builders
    build_order_event, build_order_filter, parse_order_event,
    fetch_orders,
    // Market builders
    build_announcement_event, build_contract_filter, parse_announcement_event,
    fetch_announcements, discovered_market_to_contract_params,
    // Attestation builders
    build_attestation_event, build_attestation_filter, parse_attestation_event,
    sign_attestation,
    // Relay helpers
    connect_client, publish_event,
    // Constants
    APP_EVENT_KIND,
    CONTRACT_TAG, ORDER_TAG, ATTESTATION_TAG,
    NETWORK_TAG, DEFAULT_RELAYS,
};
