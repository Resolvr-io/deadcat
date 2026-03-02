use simplicityhl::elements::{Address, AddressParams, Script};
use simplicityhl::simplicity::Cmr;
use simplicityhl::{CompiledProgram, TemplateProgram};

use crate::error::{Error, Result};
use crate::taproot;

use super::params::LmsrPoolParams;

const LMSR_PRIMARY_CONTRACT_SOURCE: &str = concat!(
    include_str!("../../contract/lmsr_pool.simf"),
    "\n\nfn main() {\n",
    "    let path_primary: PathPrimary = witness::PATH_PRIMARY;\n",
    "    let in_base: u32 = witness::IN_BASE;\n",
    "    let out_base: u32 = witness::OUT_BASE;\n",
    "    let trade_kind: u8 = witness::TRADE_KIND;\n",
    "    let old_s_index: u64 = witness::OLD_S_INDEX;\n",
    "    let new_s_index: u64 = witness::NEW_S_INDEX;\n",
    "    let f_old: u64 = witness::F_OLD;\n",
    "    let f_new: u64 = witness::F_NEW;\n",
    "    let old_proof: List<(u256, bool), 64> = witness::OLD_PROOF;\n",
    "    let new_proof: List<(u256, bool), 64> = witness::NEW_PROOF;\n",
    "    let delta_in: u64 = witness::DELTA_IN;\n",
    "    let delta_out: u64 = witness::DELTA_OUT;\n",
    "    let admin_signature: Signature = witness::ADMIN_SIGNATURE;\n",
    "    let scan_payload: ScanPayload = witness::SCAN_PAYLOAD;\n",
    "    lmsr_primary_main(\n",
    "        path_primary,\n",
    "        in_base,\n",
    "        out_base,\n",
    "        trade_kind,\n",
    "        old_s_index,\n",
    "        new_s_index,\n",
    "        f_old,\n",
    "        f_new,\n",
    "        old_proof,\n",
    "        new_proof,\n",
    "        delta_in,\n",
    "        delta_out,\n",
    "        admin_signature,\n",
    "        scan_payload\n",
    "    );\n",
    "}\n",
);
const LMSR_SECONDARY_CONTRACT_SOURCE: &str = concat!(
    include_str!("../../contract/lmsr_pool.simf"),
    "\n\nfn main() {\n",
    "    let in_base: u32 = witness::IN_BASE;\n",
    "    lmsr_secondary_main(in_base);\n",
    "}\n",
);

#[derive(Debug, Clone, Copy)]
enum LmsrLeafEntry {
    Primary,
    Secondary,
}

impl LmsrLeafEntry {
    fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Secondary => "secondary",
        }
    }
}

fn render_leaf_source(entry: LmsrLeafEntry) -> &'static str {
    match entry {
        LmsrLeafEntry::Primary => LMSR_PRIMARY_CONTRACT_SOURCE,
        LmsrLeafEntry::Secondary => LMSR_SECONDARY_CONTRACT_SOURCE,
    }
}

/// A typed LMSR covenant handle built from already-known primary/secondary leaf CMRs.
#[derive(Debug)]
pub struct CompiledLmsrPool {
    params: LmsrPoolParams,
    primary_cmr: Cmr,
    secondary_cmr: Cmr,
    primary_program: Option<CompiledProgram>,
    secondary_program: Option<CompiledProgram>,
}

impl CompiledLmsrPool {
    /// Compile both LMSR leaf contracts with canonical LMSR parameters.
    pub fn new(params: LmsrPoolParams) -> Result<Self> {
        params
            .validate()
            .map_err(|e| Error::LmsrPool(format!("invalid params: {e}")))?;

        let primary_template = TemplateProgram::new(render_leaf_source(LmsrLeafEntry::Primary))
            .map_err(|e| {
                Error::Compilation(format!(
                    "lmsr {} template parse error: {e}",
                    LmsrLeafEntry::Primary.as_str()
                ))
            })?;
        let secondary_template = TemplateProgram::new(render_leaf_source(LmsrLeafEntry::Secondary))
            .map_err(|e| {
                Error::Compilation(format!(
                    "lmsr {} template parse error: {e}",
                    LmsrLeafEntry::Secondary.as_str()
                ))
            })?;

        // The unified source references `SECONDARY_LEAF_HASH` in primary-only code paths.
        // Secondary compilation still requires all params to be bound, so provide a
        // deterministic placeholder that is never consumed by the secondary leaf DAG.
        let secondary_program = secondary_template
            .instantiate(params.build_primary_arguments([0u8; 32]), false)
            .map_err(|e| Error::Compilation(format!("lmsr secondary instantiation error: {e}")))?;
        let secondary_cmr = secondary_program.commit().cmr();
        let secondary_leaf_hash = crate::taproot::simplicity_leaf_hash(&secondary_cmr);

        let primary_program = primary_template
            .instantiate(params.build_primary_arguments(secondary_leaf_hash), false)
            .map_err(|e| Error::Compilation(format!("lmsr primary instantiation error: {e}")))?;
        let primary_cmr = primary_program.commit().cmr();

        Ok(Self {
            params,
            primary_cmr,
            secondary_cmr,
            primary_program: Some(primary_program),
            secondary_program: Some(secondary_program),
        })
    }

    /// Construct from canonical LMSR params + leaf CMRs.
    ///
    /// Use this when only Taproot addressing is needed (for example
    /// discovery/tracking), and witness satisfaction is not required.
    pub fn from_cmrs(params: LmsrPoolParams, primary_cmr: Cmr, secondary_cmr: Cmr) -> Result<Self> {
        params
            .validate()
            .map_err(|e| Error::LmsrPool(format!("invalid params: {e}")))?;
        Ok(Self {
            params,
            primary_cmr,
            secondary_cmr,
            primary_program: None,
            secondary_program: None,
        })
    }

    /// Construct from raw 32-byte CMR encodings.
    pub fn from_cmr_bytes(
        params: LmsrPoolParams,
        primary_cmr: [u8; 32],
        secondary_cmr: [u8; 32],
    ) -> Result<Self> {
        Self::from_cmrs(
            params,
            Cmr::from_byte_array(primary_cmr),
            Cmr::from_byte_array(secondary_cmr),
        )
    }

    /// Pool parameters.
    pub fn params(&self) -> &LmsrPoolParams {
        &self.params
    }

    /// Primary leaf CMR.
    pub fn primary_cmr(&self) -> &Cmr {
        &self.primary_cmr
    }

    /// Secondary leaf CMR.
    pub fn secondary_cmr(&self) -> &Cmr {
        &self.secondary_cmr
    }

    fn primary_program_inner(&self) -> Result<&CompiledProgram> {
        self.primary_program.as_ref().ok_or_else(|| {
            Error::LmsrPool("primary program unavailable for this pool handle".into())
        })
    }

    fn secondary_program_inner(&self) -> Result<&CompiledProgram> {
        self.secondary_program.as_ref().ok_or_else(|| {
            Error::LmsrPool("secondary program unavailable for this pool handle".into())
        })
    }

    /// The compiled primary program (for witness satisfaction).
    #[cfg(any(test, feature = "testing"))]
    pub fn primary_program(&self) -> Result<&CompiledProgram> {
        self.primary_program_inner()
    }

    /// The compiled primary program (for witness satisfaction).
    #[cfg(not(any(test, feature = "testing")))]
    pub(crate) fn primary_program(&self) -> Result<&CompiledProgram> {
        self.primary_program_inner()
    }

    /// The compiled secondary program (for witness satisfaction).
    #[cfg(any(test, feature = "testing"))]
    pub fn secondary_program(&self) -> Result<&CompiledProgram> {
        self.secondary_program_inner()
    }

    /// The compiled secondary program (for witness satisfaction).
    #[cfg(not(any(test, feature = "testing")))]
    pub(crate) fn secondary_program(&self) -> Result<&CompiledProgram> {
        self.secondary_program_inner()
    }

    /// Canonical LMSR Taproot merkle root for the given `s_index`.
    pub fn merkle_root(&self, s_index: u64) -> [u8; 32] {
        taproot::lmsr_merkle_root(&self.primary_cmr, &self.secondary_cmr, s_index)
    }

    /// P2TR scriptPubKey for the given `s_index`.
    pub fn script_pubkey(&self, s_index: u64) -> Script {
        taproot::lmsr_script_pubkey(&self.primary_cmr, &self.secondary_cmr, s_index)
    }

    /// Script hash (SHA256(scriptPubKey)) for the given `s_index`.
    pub fn script_hash(&self, s_index: u64) -> [u8; 32] {
        taproot::lmsr_script_hash(&self.primary_cmr, &self.secondary_cmr, s_index)
    }

    /// Address for the given `s_index`.
    pub fn address(&self, s_index: u64, network: &'static AddressParams) -> Address {
        taproot::lmsr_address(&self.primary_cmr, &self.secondary_cmr, s_index, network)
    }

    /// Control block for the primary leaf spend path.
    pub fn primary_control_block(&self, s_index: u64) -> Vec<u8> {
        taproot::lmsr_primary_control_block(&self.primary_cmr, &self.secondary_cmr, s_index)
    }

    /// Control block for the secondary leaf spend path.
    pub fn secondary_control_block(&self, s_index: u64) -> Vec<u8> {
        taproot::lmsr_secondary_control_block(&self.primary_cmr, &self.secondary_cmr, s_index)
    }

    /// Control block vector for the tapdata branch.
    pub fn tapdata_control_block(&self, s_index: u64) -> Vec<u8> {
        taproot::lmsr_tapdata_control_block(&self.primary_cmr, &self.secondary_cmr, s_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_params() -> LmsrPoolParams {
        LmsrPoolParams {
            yes_asset_id: [0x01; 32],
            no_asset_id: [0x02; 32],
            collateral_asset_id: [0x03; 32],
            lmsr_table_root: [0x04; 32],
            table_depth: 16,
            q_step_lots: 1,
            s_bias: 1024,
            s_max_index: 65_535,
            half_payout_sats: 10_000,
            fee_bps: 30,
            min_r_yes: 1,
            min_r_no: 1,
            min_r_collateral: 1,
            cosigner_pubkey: crate::taproot::NUMS_KEY_BYTES,
        }
    }

    #[test]
    fn script_pubkey_changes_with_s_index() {
        let pool = CompiledLmsrPool::from_cmr_bytes(test_params(), [0xaa; 32], [0xbb; 32]).unwrap();
        let spk_1 = pool.script_pubkey(10);
        let spk_2 = pool.script_pubkey(11);
        assert_ne!(spk_1, spk_2);
    }

    #[test]
    fn compiles_programs_with_valid_params() {
        let pool = CompiledLmsrPool::new(test_params()).unwrap();
        assert!(!pool.primary_cmr().to_byte_array().iter().all(|&b| b == 0));
        assert!(!pool.secondary_cmr().to_byte_array().iter().all(|&b| b == 0));
        pool.primary_program().unwrap();
        pool.secondary_program().unwrap();
    }

    #[test]
    fn from_cmrs_has_no_programs() {
        let pool = CompiledLmsrPool::from_cmr_bytes(test_params(), [0xaa; 32], [0xbb; 32]).unwrap();
        assert!(pool.primary_program().is_err());
        assert!(pool.secondary_program().is_err());
    }

    #[test]
    fn script_pubkey_changes_with_leaf_cmrs() {
        let p = test_params();
        let a = CompiledLmsrPool::from_cmr_bytes(p, [0x10; 32], [0x11; 32]).unwrap();
        let b = CompiledLmsrPool::from_cmr_bytes(p, [0x12; 32], [0x11; 32]).unwrap();
        assert_ne!(a.script_pubkey(42), b.script_pubkey(42));
    }

    #[test]
    fn control_blocks_have_expected_lengths() {
        let pool = CompiledLmsrPool::from_cmr_bytes(test_params(), [0xaa; 32], [0xbb; 32]).unwrap();
        assert_eq!(pool.primary_control_block(50).len(), 97);
        assert_eq!(pool.secondary_control_block(50).len(), 97);
        assert_eq!(pool.tapdata_control_block(50).len(), 65);
    }
}
