pub use simplicityhl::elements;
pub use simplicityhl::simplicity;

pub mod announcement;
pub mod contract;
pub mod error;
pub mod maker_order;
pub mod network;
pub mod oracle;
pub mod params;
pub mod pset;
pub mod sdk;
pub mod state;
pub mod taproot;
pub mod witness;

// Core types
pub use contract::CompiledContract;
pub use error::{Error, Result};
pub use network::Network;
pub use params::{ContractParams, IssuanceAssets, MarketId, compute_issuance_assets};
pub use sdk::DeadcatSdk;
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
    maker_order_script_pubkey, maker_order_taptweak,
};
pub use maker_order::witness::{
    build_maker_order_witness, satisfy_maker_order,
    serialize_satisfied as serialize_maker_order_satisfied,
};
