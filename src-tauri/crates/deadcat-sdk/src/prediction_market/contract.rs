use simplicityhl::elements::hashes::Hash as _;
use simplicityhl::elements::{Address, AddressParams, ContractHash, OutPoint, Script};
use simplicityhl::simplicity::Cmr;
use simplicityhl::{CompiledProgram, TemplateProgram};

use crate::error::{Error, Result};
use crate::prediction_market::params::{PredictionMarketParams, compute_issuance_assets};
use crate::prediction_market::state::MarketSlot;
use crate::taproot;

const CONTRACT_SOURCE: &str = include_str!("../../contract/prediction_market.simf");

/// Dormant slot addresses for a market.
#[derive(Debug, Clone)]
pub struct DormantMarketAddresses {
    pub yes_rt: Address,
    pub no_rt: Address,
}

/// Unresolved slot addresses for a market.
#[derive(Debug, Clone)]
pub struct UnresolvedMarketAddresses {
    pub yes_rt: Address,
    pub no_rt: Address,
    pub collateral: Address,
}

/// Grouped prediction-market covenant addresses.
#[derive(Debug, Clone)]
pub struct MarketAddresses {
    pub dormant: DormantMarketAddresses,
    pub unresolved: UnresolvedMarketAddresses,
    pub resolved_yes_collateral: Address,
    pub resolved_no_collateral: Address,
    pub expired_collateral: Address,
}

/// A compiled prediction market contract, ready for address derivation and spending.
pub struct CompiledPredictionMarket {
    program: CompiledProgram,
    cmr: Cmr,
    params: PredictionMarketParams,
}

impl CompiledPredictionMarket {
    /// Create a new prediction market from the non-derivable parameters and the
    /// two outpoints that will define the YES and NO asset IDs.
    ///
    /// This is the primary entry point for market creation. It computes the
    /// deterministic asset IDs, builds the full [`PredictionMarketParams`], and compiles
    /// the contract in one step.
    ///
    /// Use [`CompiledPredictionMarket::new`] instead when reconstructing from a persisted
    /// [`PredictionMarketParams`].
    pub(crate) fn create(
        oracle_public_key: [u8; 32],
        collateral_asset_id: [u8; 32],
        collateral_per_token: u64,
        expiry_time: u32,
        yes_defining_outpoint: OutPoint,
        no_defining_outpoint: OutPoint,
    ) -> Result<Self> {
        let assets = compute_issuance_assets(
            yes_defining_outpoint,
            no_defining_outpoint,
            ContractHash::from_byte_array([0u8; 32]),
            false,
        );

        let params = PredictionMarketParams {
            oracle_public_key,
            collateral_asset_id,
            yes_token_asset: assets.yes_token_asset,
            no_token_asset: assets.no_token_asset,
            yes_reissuance_token: assets.yes_reissuance_token,
            no_reissuance_token: assets.no_reissuance_token,
            collateral_per_token,
            expiry_time,
        };

        Self::new(params)
    }

    /// Compile the prediction market contract with the given parameters.
    pub fn new(params: PredictionMarketParams) -> Result<Self> {
        let template = TemplateProgram::new(CONTRACT_SOURCE)
            .map_err(|e| Error::Compilation(format!("template parse error: {e}")))?;

        let program = template
            .instantiate(params.build_arguments(), false)
            .map_err(|e| Error::Compilation(format!("instantiation error: {e}")))?;

        let cmr = program.commit().cmr();

        Ok(Self {
            program,
            cmr,
            params,
        })
    }

    /// The Commitment Merkle Root of the compiled program.
    pub fn cmr(&self) -> &Cmr {
        &self.cmr
    }

    /// The compiled program (for witness satisfaction).
    #[cfg(any(test, feature = "testing"))]
    pub fn program(&self) -> &CompiledProgram {
        &self.program
    }

    /// The compiled program (for witness satisfaction).
    #[cfg(not(any(test, feature = "testing")))]
    pub(crate) fn program(&self) -> &CompiledProgram {
        &self.program
    }

    /// The contract parameters.
    pub fn params(&self) -> &PredictionMarketParams {
        &self.params
    }

    /// Compute the covenant script pubkey for a specific slot.
    pub fn script_pubkey(&self, slot: MarketSlot) -> Script {
        taproot::prediction_market_script_pubkey(&self.cmr, slot.as_u8())
    }

    /// Compute the covenant script hash for a specific slot.
    pub fn script_hash(&self, slot: MarketSlot) -> [u8; 32] {
        taproot::prediction_market_script_hash(&self.cmr, slot.as_u8())
    }

    /// Compute the covenant address for a specific slot and network.
    pub fn address(&self, slot: MarketSlot, network: &'static AddressParams) -> Address {
        taproot::prediction_market_address(&self.cmr, slot.as_u8(), network)
    }

    /// Build the Simplicity control block for a specific slot.
    pub fn control_block(&self, slot: MarketSlot) -> Vec<u8> {
        taproot::prediction_market_control_block(&self.cmr, slot.as_u8())
    }

    /// Compute all slot addresses for this market.
    pub fn addresses(&self, network: &'static AddressParams) -> MarketAddresses {
        MarketAddresses {
            dormant: DormantMarketAddresses {
                yes_rt: self.address(MarketSlot::DormantYesRt, network),
                no_rt: self.address(MarketSlot::DormantNoRt, network),
            },
            unresolved: UnresolvedMarketAddresses {
                yes_rt: self.address(MarketSlot::UnresolvedYesRt, network),
                no_rt: self.address(MarketSlot::UnresolvedNoRt, network),
                collateral: self.address(MarketSlot::UnresolvedCollateral, network),
            },
            resolved_yes_collateral: self.address(MarketSlot::ResolvedYesCollateral, network),
            resolved_no_collateral: self.address(MarketSlot::ResolvedNoCollateral, network),
            expired_collateral: self.address(MarketSlot::ExpiredCollateral, network),
        }
    }
}
