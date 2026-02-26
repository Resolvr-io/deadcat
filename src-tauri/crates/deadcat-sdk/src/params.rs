use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use simplicityhl::elements::{AssetId, ContractHash, OutPoint};
use simplicityhl::num::U256;
use simplicityhl::str::WitnessName;
use simplicityhl::value::ValueConstructible;
use simplicityhl::{Arguments, Value};

/// SHA256(YES_TOKEN_ASSET || NO_TOKEN_ASSET) — unique per-market domain separator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MarketId(pub [u8; 32]);

impl MarketId {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Display for MarketId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl AsRef<[u8]> for MarketId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Deterministic asset IDs derived from the defining UTXOs used in issuance.
///
/// Use [`compute_issuance_assets`] to compute these from the outpoints that will
/// be spent in the initial issuance transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IssuanceAssets {
    pub(crate) yes_token_asset: [u8; 32],
    pub(crate) no_token_asset: [u8; 32],
    pub(crate) yes_reissuance_token: [u8; 32],
    pub(crate) no_reissuance_token: [u8; 32],
}

/// Compute the deterministic asset IDs for a binary prediction market from the
/// defining UTXO outpoints.
///
/// The `yes_defining_outpoint` and `no_defining_outpoint` are the outpoints that
/// will be spent in PSET inputs 0 and 1 respectively during the creation transaction.
/// The `contract_hash` is typically `ContractHash::from_byte_array([0u8; 32])` when
/// no asset contract metadata is used. Set `confidential` to `true` if the issuance
/// amounts will be blinded.
pub(crate) fn compute_issuance_assets(
    yes_defining_outpoint: OutPoint,
    no_defining_outpoint: OutPoint,
    contract_hash: ContractHash,
    confidential: bool,
) -> IssuanceAssets {
    let yes_token_asset = AssetId::new_issuance(yes_defining_outpoint, contract_hash);
    let yes_reissuance_token =
        AssetId::new_reissuance_token(yes_defining_outpoint, contract_hash, confidential);
    let no_token_asset = AssetId::new_issuance(no_defining_outpoint, contract_hash);
    let no_reissuance_token =
        AssetId::new_reissuance_token(no_defining_outpoint, contract_hash, confidential);

    IssuanceAssets {
        yes_token_asset: yes_token_asset.into_inner().to_byte_array(),
        no_token_asset: no_token_asset.into_inner().to_byte_array(),
        yes_reissuance_token: yes_reissuance_token.into_inner().to_byte_array(),
        no_reissuance_token: no_reissuance_token.into_inner().to_byte_array(),
    }
}

/// Compile-time parameters for a binary prediction market contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractParams {
    /// X-only Schnorr pubkey (FROST aggregate key).
    pub oracle_public_key: [u8; 32],
    /// Asset ID of the collateral (typically L-BTC).
    pub collateral_asset_id: [u8; 32],
    /// Deterministic asset ID for YES tokens.
    pub yes_token_asset: [u8; 32],
    /// Deterministic asset ID for NO tokens.
    pub no_token_asset: [u8; 32],
    /// Reissuance token for YES asset.
    pub yes_reissuance_token: [u8; 32],
    /// Reissuance token for NO asset.
    pub no_reissuance_token: [u8; 32],
    /// Satoshis backing each individual token.
    pub collateral_per_token: u64,
    /// Block height deadline for oracle resolution.
    pub expiry_time: u32,
}

impl ContractParams {
    /// Derive the market ID: SHA256(yes_token_asset || no_token_asset).
    pub fn market_id(&self) -> MarketId {
        let mut hasher = Sha256::new();
        hasher.update(self.yes_token_asset);
        hasher.update(self.no_token_asset);
        let result: [u8; 32] = hasher.finalize().into();
        MarketId(result)
    }

    /// Build SimplicityHL `Arguments` for contract compilation.
    pub(crate) fn build_arguments(&self) -> Arguments {
        let map = HashMap::from([
            (
                WitnessName::from_str_unchecked("ORACLE_PUBLIC_KEY"),
                Value::u256(U256::from_byte_array(self.oracle_public_key)),
            ),
            (
                WitnessName::from_str_unchecked("COLLATERAL_ASSET_ID"),
                Value::u256(U256::from_byte_array(self.collateral_asset_id)),
            ),
            (
                WitnessName::from_str_unchecked("YES_TOKEN_ASSET"),
                Value::u256(U256::from_byte_array(self.yes_token_asset)),
            ),
            (
                WitnessName::from_str_unchecked("NO_TOKEN_ASSET"),
                Value::u256(U256::from_byte_array(self.no_token_asset)),
            ),
            (
                WitnessName::from_str_unchecked("YES_REISSUANCE_TOKEN"),
                Value::u256(U256::from_byte_array(self.yes_reissuance_token)),
            ),
            (
                WitnessName::from_str_unchecked("NO_REISSUANCE_TOKEN"),
                Value::u256(U256::from_byte_array(self.no_reissuance_token)),
            ),
            (
                WitnessName::from_str_unchecked("COLLATERAL_PER_TOKEN"),
                Value::u64(self.collateral_per_token),
            ),
            (
                WitnessName::from_str_unchecked("EXPIRY_TIME"),
                Value::u32(self.expiry_time),
            ),
        ]);
        Arguments::from(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_params() -> ContractParams {
        ContractParams {
            oracle_public_key: [0xaa; 32],
            collateral_asset_id: [0xbb; 32],
            yes_token_asset: [0x01; 32],
            no_token_asset: [0x02; 32],
            yes_reissuance_token: [0x03; 32],
            no_reissuance_token: [0x04; 32],
            collateral_per_token: 100_000,
            expiry_time: 1_000_000,
        }
    }

    #[test]
    fn market_id_deterministic() {
        let params = test_params();
        let id1 = params.market_id();
        let id2 = params.market_id();
        assert_eq!(id1, id2);
    }

    #[test]
    fn market_id_depends_on_token_assets() {
        let mut params = test_params();
        let id1 = params.market_id();
        params.yes_token_asset = [0xff; 32];
        let id2 = params.market_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn contract_params_is_copy() {
        let params = test_params();
        let params2 = params; // Copy
        assert_eq!(params, params2);
    }

    #[test]
    fn market_id_display_is_hex() {
        let id = MarketId([0xab; 32]);
        let s = format!("{id}");
        assert_eq!(s.len(), 64);
        assert!(s.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(&s[..4], "abab");
    }
}
