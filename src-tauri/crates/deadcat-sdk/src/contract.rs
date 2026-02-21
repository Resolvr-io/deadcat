use simplicityhl::elements::hashes::Hash as _;
use simplicityhl::elements::{Address, AddressParams, ContractHash, OutPoint, Script};
use simplicityhl::simplicity::Cmr;
use simplicityhl::{CompiledProgram, TemplateProgram};

use crate::error::{Error, Result};
use crate::params::{ContractParams, compute_issuance_assets};
use crate::state::MarketState;
use crate::taproot::{self, MarketAddresses};

const CONTRACT_SOURCE: &str = include_str!("../contract/prediction_market.simf");

/// A compiled prediction market contract, ready for address derivation and spending.
pub struct CompiledContract {
    program: CompiledProgram,
    cmr: Cmr,
    params: ContractParams,
}

impl CompiledContract {
    /// Create a new prediction market from the non-derivable parameters and the
    /// two outpoints that will define the YES and NO asset IDs.
    ///
    /// This is the primary entry point for market creation. It computes the
    /// deterministic asset IDs, builds the full [`ContractParams`], and compiles
    /// the contract in one step.
    ///
    /// Use [`CompiledContract::new`] instead when reconstructing from a persisted
    /// [`ContractParams`].
    pub fn create(
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

        let params = ContractParams {
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
    pub fn new(params: ContractParams) -> Result<Self> {
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
    pub fn program(&self) -> &CompiledProgram {
        &self.program
    }

    /// The contract parameters.
    pub fn params(&self) -> &ContractParams {
        &self.params
    }

    /// Compute the covenant script pubkey for a given state.
    pub fn script_pubkey(&self, state: MarketState) -> Script {
        taproot::covenant_script_pubkey(&self.cmr, state.as_u64())
    }

    /// Compute the covenant script hash for a given state.
    pub fn script_hash(&self, state: MarketState) -> [u8; 32] {
        taproot::covenant_script_hash(&self.cmr, state.as_u64())
    }

    /// Compute the covenant address for a given state and network.
    pub fn address(&self, state: MarketState, network: &'static AddressParams) -> Address {
        taproot::covenant_address(&self.cmr, state.as_u64(), network)
    }

    /// Build the Simplicity control block for a given state.
    pub fn control_block(&self, state: MarketState) -> Vec<u8> {
        taproot::simplicity_control_block(&self.cmr, state.as_u64())
    }

    /// Compute all four covenant addresses for this market.
    pub fn addresses(&self, network: &'static AddressParams) -> MarketAddresses {
        MarketAddresses {
            dormant: self.address(MarketState::Dormant, network),
            unresolved: self.address(MarketState::Unresolved, network),
            resolved_yes: self.address(MarketState::ResolvedYes, network),
            resolved_no: self.address(MarketState::ResolvedNo, network),
        }
    }
}
