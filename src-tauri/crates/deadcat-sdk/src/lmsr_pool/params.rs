use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use simplicityhl::num::U256;
use simplicityhl::str::WitnessName;
use simplicityhl::value::ValueConstructible;
use simplicityhl::{Arguments, Value};

use crate::error::{Error, Result};

const LMSR_POOL_ID_V1_DOMAIN: &[u8] = b"DEADCAT/LMSR_POOL_ID_V1";

/// Compile-time parameters for a binary LMSR pool covenant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LmsrPoolParams {
    /// Asset ID of the YES outcome token.
    pub yes_asset_id: [u8; 32],
    /// Asset ID of the NO outcome token.
    pub no_asset_id: [u8; 32],
    /// Asset ID of collateral (quote asset).
    pub collateral_asset_id: [u8; 32],
    /// Merkle root for `(index, F(index))` LMSR table commitment.
    pub lmsr_table_root: [u8; 32],
    /// Table depth in bits.
    pub table_depth: u32,
    /// Lots per LMSR index step.
    pub q_step_lots: u64,
    /// Signed index bias encoded as a scalar.
    pub s_bias: u64,
    /// Maximum allowed table index.
    pub s_max_index: u64,
    /// Half payout in sats (`U / 2` in the design doc).
    pub half_payout_sats: u64,
    /// Swap fee in basis points.
    pub fee_bps: u64,
    /// Minimum YES reserve enforced after each transition.
    pub min_r_yes: u64,
    /// Minimum NO reserve enforced after each transition.
    pub min_r_no: u64,
    /// Minimum collateral reserve enforced after each transition.
    pub min_r_collateral: u64,
    /// Admin cosigner x-only pubkey (NUMS means fail-closed).
    pub cosigner_pubkey: [u8; 32],
}

impl LmsrPoolParams {
    /// Validate parameters before compilation or announcement.
    pub fn validate(&self) -> std::result::Result<(), String> {
        if self.fee_bps >= 10_000 {
            return Err(format!("fee_bps must be < 10000, got {}", self.fee_bps));
        }
        if self.q_step_lots == 0 {
            return Err("q_step_lots must be > 0".into());
        }
        if self.half_payout_sats == 0 {
            return Err("half_payout_sats must be > 0".into());
        }
        if self.min_r_yes == 0 {
            return Err("min_r_yes must be > 0".into());
        }
        if self.min_r_no == 0 {
            return Err("min_r_no must be > 0".into());
        }
        if self.min_r_collateral == 0 {
            return Err("min_r_collateral must be > 0".into());
        }
        if self.table_depth == 0 || self.table_depth > 63 {
            return Err(format!(
                "table_depth must be in [1, 63], got {}",
                self.table_depth
            ));
        }
        let max_index_by_depth = (1u128 << self.table_depth) - 1;
        if (self.s_max_index as u128) > max_index_by_depth {
            return Err(format!(
                "s_max_index {} exceeds table_depth {} capacity {}",
                self.s_max_index, self.table_depth, max_index_by_depth
            ));
        }
        if self.yes_asset_id == self.no_asset_id {
            return Err("yes_asset_id and no_asset_id must differ".into());
        }
        if self.yes_asset_id == self.collateral_asset_id {
            return Err("yes_asset_id and collateral_asset_id must differ".into());
        }
        if self.no_asset_id == self.collateral_asset_id {
            return Err("no_asset_id and collateral_asset_id must differ".into());
        }
        Ok(())
    }

    fn build_arguments_internal(&self, secondary_leaf_hash: Option<[u8; 32]>) -> Arguments {
        let mut map = HashMap::from([
            (
                WitnessName::from_str_unchecked("YES_ASSET_ID"),
                Value::u256(U256::from_byte_array(self.yes_asset_id)),
            ),
            (
                WitnessName::from_str_unchecked("NO_ASSET_ID"),
                Value::u256(U256::from_byte_array(self.no_asset_id)),
            ),
            (
                WitnessName::from_str_unchecked("COLLATERAL_ASSET_ID"),
                Value::u256(U256::from_byte_array(self.collateral_asset_id)),
            ),
            (
                WitnessName::from_str_unchecked("LMSR_TABLE_ROOT"),
                Value::u256(U256::from_byte_array(self.lmsr_table_root)),
            ),
            (
                WitnessName::from_str_unchecked("TABLE_DEPTH"),
                Value::u32(self.table_depth),
            ),
            (
                WitnessName::from_str_unchecked("Q_STEP_LOTS"),
                Value::u64(self.q_step_lots),
            ),
            (
                WitnessName::from_str_unchecked("S_BIAS"),
                Value::u64(self.s_bias),
            ),
            (
                WitnessName::from_str_unchecked("S_MAX_INDEX"),
                Value::u64(self.s_max_index),
            ),
            (
                WitnessName::from_str_unchecked("HALF_PAYOUT_SATS"),
                Value::u64(self.half_payout_sats),
            ),
            (
                WitnessName::from_str_unchecked("FEE_BPS"),
                Value::u64(self.fee_bps),
            ),
            (
                WitnessName::from_str_unchecked("MIN_R_YES"),
                Value::u64(self.min_r_yes),
            ),
            (
                WitnessName::from_str_unchecked("MIN_R_NO"),
                Value::u64(self.min_r_no),
            ),
            (
                WitnessName::from_str_unchecked("MIN_R_COLLATERAL"),
                Value::u64(self.min_r_collateral),
            ),
            (
                WitnessName::from_str_unchecked("COSIGNER_PUBKEY"),
                Value::u256(U256::from_byte_array(self.cosigner_pubkey)),
            ),
        ]);
        if let Some(secondary_leaf_hash) = secondary_leaf_hash {
            map.insert(
                WitnessName::from_str_unchecked("SECONDARY_LEAF_HASH"),
                Value::u256(U256::from_byte_array(secondary_leaf_hash)),
            );
        }
        Arguments::from(map)
    }

    /// Build SimplicityHL `Arguments` for LMSR contract compilation.
    #[allow(dead_code)]
    pub(crate) fn build_arguments(&self) -> Arguments {
        self.build_arguments_internal(None)
    }

    /// Build SimplicityHL `Arguments` for LMSR primary contract compilation.
    ///
    /// The primary leaf validates old/new state script hashes against the full
    /// LMSR Taproot tree and therefore needs the sibling secondary leaf hash.
    #[allow(dead_code)]
    pub(crate) fn build_primary_arguments(&self, secondary_leaf_hash: [u8; 32]) -> Arguments {
        self.build_arguments_internal(Some(secondary_leaf_hash))
    }

    /// Whether the admin path is configured with a non-NUMS key.
    pub fn has_admin_cosigner(&self) -> bool {
        self.cosigner_pubkey != crate::taproot::NUMS_KEY_BYTES
    }
}

/// Canonical `(txid, vout)` outpoint encoding for LMSR identity anchoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LmsrInitialOutpoint {
    pub txid: [u8; 32],
    pub vout: u32,
}

/// Input bundle required to derive canonical `LMSR_POOL_ID` (v0.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LmsrPoolIdInput {
    pub chain_genesis_hash: [u8; 32],
    pub params: LmsrPoolParams,
    pub covenant_cmr: [u8; 32],
    pub creation_txid: [u8; 32],
    pub initial_yes_outpoint: LmsrInitialOutpoint,
    pub initial_no_outpoint: LmsrInitialOutpoint,
    pub initial_collateral_outpoint: LmsrInitialOutpoint,
}

/// Canonical LMSR pool identity key (`LMSR_POOL_ID` in design doc §15).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LmsrPoolId(pub [u8; 32]);

impl LmsrPoolId {
    /// Parse canonical LMSR pool identity from a 32-byte hex string.
    pub fn from_hex(hex_str: &str) -> Result<Self> {
        let bytes = hex::decode(hex_str)
            .map_err(|e| Error::LmsrPool(format!("invalid lmsr_pool_id hex: {e}")))?;
        if bytes.len() != 32 {
            return Err(Error::LmsrPool(format!(
                "invalid lmsr_pool_id length: expected 32 bytes, got {}",
                bytes.len()
            )));
        }
        let mut id = [0u8; 32];
        id.copy_from_slice(&bytes);
        Ok(Self(id))
    }

    /// Derive `LMSR_POOL_ID` using the v0.1 canonical domain and field ordering.
    pub fn derive_v1(input: &LmsrPoolIdInput) -> Result<Self> {
        input
            .params
            .validate()
            .map_err(|e| Error::LmsrPool(format!("invalid LMSR params: {e}")))?;
        let fee_bps = u32::try_from(input.params.fee_bps).map_err(|_| {
            Error::LmsrPool(format!(
                "fee_bps {} exceeds u32::MAX for LMSR_POOL_ID derivation",
                input.params.fee_bps
            ))
        })?;

        let mut hasher = Sha256::new();
        hasher.update(LMSR_POOL_ID_V1_DOMAIN);
        hasher.update(input.chain_genesis_hash);
        hasher.update(input.params.yes_asset_id);
        hasher.update(input.params.no_asset_id);
        hasher.update(input.params.collateral_asset_id);
        hasher.update(input.params.lmsr_table_root);
        hasher.update(input.params.table_depth.to_be_bytes());
        hasher.update(input.params.q_step_lots.to_be_bytes());
        hasher.update(input.params.s_bias.to_be_bytes());
        hasher.update(input.params.s_max_index.to_be_bytes());
        hasher.update(input.params.half_payout_sats.to_be_bytes());
        hasher.update(fee_bps.to_be_bytes());
        hasher.update(input.params.min_r_yes.to_be_bytes());
        hasher.update(input.params.min_r_no.to_be_bytes());
        hasher.update(input.params.min_r_collateral.to_be_bytes());
        hasher.update(input.params.cosigner_pubkey);
        hasher.update(input.covenant_cmr);
        hasher.update(input.creation_txid);
        Self::hash_outpoint(&mut hasher, input.initial_yes_outpoint);
        Self::hash_outpoint(&mut hasher, input.initial_no_outpoint);
        Self::hash_outpoint(&mut hasher, input.initial_collateral_outpoint);
        Ok(Self(hasher.finalize().into()))
    }

    fn hash_outpoint(hasher: &mut Sha256, outpoint: LmsrInitialOutpoint) {
        hasher.update(outpoint.txid);
        hasher.update(outpoint.vout.to_be_bytes());
    }

    /// Return the canonical pool ID as lowercase hex.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl std::fmt::Display for LmsrPoolId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
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
            q_step_lots: 10,
            s_bias: 1000,
            s_max_index: 65_535,
            half_payout_sats: 5_000,
            fee_bps: 30,
            min_r_yes: 1,
            min_r_no: 1,
            min_r_collateral: 1,
            cosigner_pubkey: crate::taproot::NUMS_KEY_BYTES,
        }
    }

    fn test_id_input() -> LmsrPoolIdInput {
        LmsrPoolIdInput {
            chain_genesis_hash: [0xaa; 32],
            params: test_params(),
            covenant_cmr: [0xbb; 32],
            creation_txid: [0xcc; 32],
            initial_yes_outpoint: LmsrInitialOutpoint {
                txid: [0xdd; 32],
                vout: 0,
            },
            initial_no_outpoint: LmsrInitialOutpoint {
                txid: [0xdd; 32],
                vout: 1,
            },
            initial_collateral_outpoint: LmsrInitialOutpoint {
                txid: [0xdd; 32],
                vout: 2,
            },
        }
    }

    #[test]
    fn params_validate_rejects_bad_fee() {
        let mut p = test_params();
        p.fee_bps = 10_000;
        assert!(p.validate().is_err());
    }

    #[test]
    fn params_validate_rejects_bad_table_depth() {
        let mut p = test_params();
        p.table_depth = 0;
        assert!(p.validate().is_err());

        p.table_depth = 64;
        assert!(p.validate().is_err());
    }

    #[test]
    fn params_validate_rejects_s_max_index_overflow() {
        let mut p = test_params();
        p.table_depth = 8;
        p.s_max_index = 300;
        assert!(p.validate().is_err());
    }

    #[test]
    fn params_validate_rejects_non_distinct_assets() {
        let mut p = test_params();
        p.no_asset_id = p.yes_asset_id;
        assert!(p.validate().is_err());
    }

    #[test]
    fn params_has_admin_cosigner() {
        let mut p = test_params();
        assert!(!p.has_admin_cosigner());
        p.cosigner_pubkey = [0x22; 32];
        assert!(p.has_admin_cosigner());
    }

    #[test]
    fn lmsr_pool_id_is_deterministic() {
        let input = test_id_input();
        let id1 = LmsrPoolId::derive_v1(&input).unwrap();
        let id2 = LmsrPoolId::derive_v1(&input).unwrap();
        assert_eq!(id1, id2);
    }

    #[test]
    fn lmsr_pool_id_changes_when_anchor_changes() {
        let mut input = test_id_input();
        let id1 = LmsrPoolId::derive_v1(&input).unwrap();
        input.initial_no_outpoint.vout = 9;
        let id2 = LmsrPoolId::derive_v1(&input).unwrap();
        assert_ne!(id1, id2);
    }

    #[test]
    fn lmsr_pool_id_hex_length() {
        let id = LmsrPoolId::derive_v1(&test_id_input()).unwrap();
        assert_eq!(id.to_hex().len(), 64);
    }

    #[test]
    fn lmsr_pool_id_from_hex_roundtrip() {
        let id = LmsrPoolId::derive_v1(&test_id_input()).unwrap();
        let parsed = LmsrPoolId::from_hex(&id.to_hex()).unwrap();
        assert_eq!(parsed, id);
    }

    #[test]
    fn lmsr_pool_id_from_hex_rejects_invalid_length() {
        let err = LmsrPoolId::from_hex("abcd").unwrap_err();
        assert!(err.to_string().contains("invalid lmsr_pool_id length"));
    }
}
