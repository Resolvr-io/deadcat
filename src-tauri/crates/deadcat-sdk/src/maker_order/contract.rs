use simplicityhl::elements::{Address, AddressParams, Script};
use simplicityhl::simplicity::Cmr;
use simplicityhl::{CompiledProgram, TemplateProgram};

use crate::error::{Error, Result};

use super::params::MakerOrderParams;
use super::taproot;

const CONTRACT_SOURCE: &str = include_str!("../../contract/maker_order.simf");

/// A compiled maker order covenant, ready for address derivation and spending.
pub struct CompiledMakerOrder {
    program: CompiledProgram,
    cmr: Cmr,
    params: MakerOrderParams,
}

impl CompiledMakerOrder {
    /// Compile the maker order contract with the given parameters.
    pub fn new(params: MakerOrderParams) -> Result<Self> {
        let template = TemplateProgram::new(CONTRACT_SOURCE)
            .map_err(|e| Error::Compilation(format!("maker order template parse error: {e}")))?;

        let program = template
            .instantiate(params.build_arguments(), false)
            .map_err(|e| Error::Compilation(format!("maker order instantiation error: {e}")))?;

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
    pub fn params(&self) -> &MakerOrderParams {
        &self.params
    }

    /// Compute the covenant script pubkey for a given maker base pubkey.
    pub fn script_pubkey(&self, maker_base_pubkey: &[u8; 32]) -> Script {
        taproot::maker_order_script_pubkey(&self.cmr, maker_base_pubkey)
    }

    /// Compute the covenant script hash for a given maker base pubkey.
    pub fn script_hash(&self, maker_base_pubkey: &[u8; 32]) -> [u8; 32] {
        taproot::maker_order_script_hash(&self.cmr, maker_base_pubkey)
    }

    /// Compute the covenant address for a given maker base pubkey and network.
    pub fn address(
        &self,
        maker_base_pubkey: &[u8; 32],
        network: &'static AddressParams,
    ) -> Address {
        taproot::maker_order_address(&self.cmr, maker_base_pubkey, network)
    }

    /// Build the Simplicity control block (33 bytes).
    pub fn control_block(&self, maker_base_pubkey: &[u8; 32]) -> Vec<u8> {
        taproot::maker_order_control_block(&self.cmr, maker_base_pubkey)
    }
}
