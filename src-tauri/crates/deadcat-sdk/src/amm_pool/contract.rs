use simplicityhl::elements::{Address, AddressParams, Script};
use simplicityhl::simplicity::Cmr;
use simplicityhl::{CompiledProgram, TemplateProgram};

use crate::error::{Error, Result};
use crate::taproot;

use super::params::AmmPoolParams;

const CONTRACT_SOURCE: &str = include_str!("../../contract/amm_pool.simf");

/// A compiled AMM pool covenant, ready for address derivation and spending.
pub struct CompiledAmmPool {
    program: CompiledProgram,
    cmr: Cmr,
    params: AmmPoolParams,
}

impl CompiledAmmPool {
    /// Compile the AMM pool contract with the given parameters.
    pub fn new(params: AmmPoolParams) -> Result<Self> {
        params
            .validate()
            .map_err(|e| Error::AmmPool(format!("invalid params: {e}")))?;

        let template = TemplateProgram::new(CONTRACT_SOURCE)
            .map_err(|e| Error::Compilation(format!("amm pool template parse error: {e}")))?;

        let program = template
            .instantiate(params.build_arguments(), false)
            .map_err(|e| Error::Compilation(format!("amm pool instantiation error: {e}")))?;

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
    pub fn params(&self) -> &AmmPoolParams {
        &self.params
    }

    /// Compute the covenant script pubkey for a given `issued_lp` state.
    pub fn script_pubkey(&self, issued_lp: u64) -> Script {
        taproot::covenant_script_pubkey(&self.cmr, issued_lp)
    }

    /// Compute the covenant script hash for a given `issued_lp` state.
    pub fn script_hash(&self, issued_lp: u64) -> [u8; 32] {
        taproot::covenant_script_hash(&self.cmr, issued_lp)
    }

    /// Compute the covenant address for a given `issued_lp` state and network.
    pub fn address(&self, issued_lp: u64, network: &'static AddressParams) -> Address {
        taproot::covenant_address(&self.cmr, issued_lp, network)
    }

    /// Build the Simplicity control block (65 bytes).
    pub fn control_block(&self, issued_lp: u64) -> Vec<u8> {
        taproot::simplicity_control_block(&self.cmr, issued_lp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taproot::NUMS_KEY_BYTES;

    fn test_params() -> AmmPoolParams {
        AmmPoolParams {
            yes_asset_id: [0x01; 32],
            no_asset_id: [0x02; 32],
            lbtc_asset_id: [0x03; 32],
            lp_asset_id: [0x04; 32],
            lp_reissuance_token_id: [0x05; 32],
            fee_bps: 30,
            cosigner_pubkey: NUMS_KEY_BYTES,
        }
    }

    #[test]
    fn compiles_with_valid_params() {
        let contract = CompiledAmmPool::new(test_params()).unwrap();
        assert!(!contract.cmr().to_byte_array().iter().all(|&b| b == 0));
    }

    #[test]
    fn cmr_deterministic() {
        let c1 = CompiledAmmPool::new(test_params()).unwrap();
        let c2 = CompiledAmmPool::new(test_params()).unwrap();
        assert_eq!(c1.cmr(), c2.cmr());
    }

    #[test]
    fn cmr_changes_with_params() {
        let c1 = CompiledAmmPool::new(test_params()).unwrap();
        let mut params2 = test_params();
        params2.fee_bps = 100;
        let c2 = CompiledAmmPool::new(params2).unwrap();
        assert_ne!(c1.cmr(), c2.cmr());
    }

    #[test]
    fn script_pubkey_changes_with_issued_lp() {
        let contract = CompiledAmmPool::new(test_params()).unwrap();
        let spk1 = contract.script_pubkey(1000);
        let spk2 = contract.script_pubkey(2000);
        assert_ne!(
            spk1, spk2,
            "different issued_lp must produce different addresses"
        );
    }

    #[test]
    fn control_block_is_65_bytes() {
        let contract = CompiledAmmPool::new(test_params()).unwrap();
        let cb = contract.control_block(1000);
        assert_eq!(cb.len(), 65);
    }

    #[test]
    fn rejects_invalid_fee_bps() {
        let mut params = test_params();
        params.fee_bps = 10_000;
        assert!(CompiledAmmPool::new(params).is_err());
    }
}
