use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use simplicityhl::num::U256;
use simplicityhl::str::WitnessName;
use simplicityhl::value::ValueConstructible;
use simplicityhl::{Arguments, Value};

/// Compile-time parameters for an AMM pool covenant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AmmPoolParams {
    /// Asset ID of the YES outcome token.
    pub yes_asset_id: [u8; 32],
    /// Asset ID of the NO outcome token.
    pub no_asset_id: [u8; 32],
    /// Asset ID of L-BTC (collateral/quote asset).
    pub lbtc_asset_id: [u8; 32],
    /// Asset ID of the LP token.
    pub lp_asset_id: [u8; 32],
    /// Asset ID of the LP token's reissuance token.
    pub lp_reissuance_token_id: [u8; 32],
    /// Swap fee in basis points (e.g., 30 = 0.30%).
    pub fee_bps: u64,
    /// Optional cosigner x-only pubkey. NUMS key bytes = no cosigner.
    pub cosigner_pubkey: [u8; 32],
}

impl AmmPoolParams {
    /// Validate parameters. Returns an error if fee_bps >= 10000.
    pub fn validate(&self) -> std::result::Result<(), String> {
        if self.fee_bps >= 10_000 {
            return Err(format!("fee_bps must be < 10000, got {}", self.fee_bps));
        }
        Ok(())
    }

    /// Build SimplicityHL `Arguments` for contract compilation.
    pub fn build_arguments(&self) -> Arguments {
        let map = HashMap::from([
            (
                WitnessName::from_str_unchecked("YES_ASSET_ID"),
                Value::u256(U256::from_byte_array(self.yes_asset_id)),
            ),
            (
                WitnessName::from_str_unchecked("NO_ASSET_ID"),
                Value::u256(U256::from_byte_array(self.no_asset_id)),
            ),
            (
                WitnessName::from_str_unchecked("LBTC_ASSET_ID"),
                Value::u256(U256::from_byte_array(self.lbtc_asset_id)),
            ),
            (
                WitnessName::from_str_unchecked("LP_ASSET_ID"),
                Value::u256(U256::from_byte_array(self.lp_asset_id)),
            ),
            (
                WitnessName::from_str_unchecked("LP_REISSUANCE_TOKEN_ID"),
                Value::u256(U256::from_byte_array(self.lp_reissuance_token_id)),
            ),
            (
                WitnessName::from_str_unchecked("FEE_BPS"),
                Value::u64(self.fee_bps),
            ),
            (
                WitnessName::from_str_unchecked("COSIGNER_PUBKEY"),
                Value::u256(U256::from_byte_array(self.cosigner_pubkey)),
            ),
        ]);
        Arguments::from(map)
    }

    /// Whether the cosigner is enabled (pubkey is not NUMS).
    pub fn has_cosigner(&self) -> bool {
        self.cosigner_pubkey != crate::taproot::NUMS_KEY_BYTES
    }
}

/// Unique identifier for a pool, derived from all parameters.
///
/// `PoolId = SHA256(yes_asset_id || no_asset_id || lbtc_asset_id || lp_asset_id
///                  || lp_reissuance_token_id || fee_bps_be || cosigner_pubkey)`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PoolId(pub [u8; 32]);

impl PoolId {
    /// Compute the pool ID from parameters.
    pub fn from_params(params: &AmmPoolParams) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(params.yes_asset_id);
        hasher.update(params.no_asset_id);
        hasher.update(params.lbtc_asset_id);
        hasher.update(params.lp_asset_id);
        hasher.update(params.lp_reissuance_token_id);
        hasher.update(params.fee_bps.to_be_bytes());
        hasher.update(params.cosigner_pubkey);
        Self(hasher.finalize().into())
    }

    /// Return the pool ID as a hex string.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl std::fmt::Display for PoolId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
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
    fn pool_id_deterministic() {
        let params = test_params();
        let id1 = PoolId::from_params(&params);
        let id2 = PoolId::from_params(&params);
        assert_eq!(id1, id2);
    }

    #[test]
    fn pool_id_changes_with_fee() {
        let mut params = test_params();
        let id1 = PoolId::from_params(&params);
        params.fee_bps = 100;
        let id2 = PoolId::from_params(&params);
        assert_ne!(id1, id2);
    }

    #[test]
    fn pool_id_changes_with_asset() {
        let mut params = test_params();
        let id1 = PoolId::from_params(&params);
        params.yes_asset_id = [0xff; 32];
        let id2 = PoolId::from_params(&params);
        assert_ne!(id1, id2);
    }

    #[test]
    fn has_cosigner_with_nums() {
        let params = test_params();
        assert!(!params.has_cosigner());
    }

    #[test]
    fn has_cosigner_with_real_key() {
        let mut params = test_params();
        params.cosigner_pubkey = [0xff; 32];
        assert!(params.has_cosigner());
    }

    #[test]
    fn params_is_copy() {
        let params = test_params();
        let params2 = params;
        assert_eq!(params, params2);
    }

    #[test]
    fn pool_id_hex_roundtrip() {
        let params = test_params();
        let id = PoolId::from_params(&params);
        let hex_str = id.to_hex();
        assert_eq!(hex_str.len(), 64);
    }
}
