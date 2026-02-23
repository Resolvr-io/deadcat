use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use simplicityhl::elements::hashes::{Hash, sha256};
use simplicityhl::elements::secp256k1_zkp::{Secp256k1, XOnlyPublicKey};
use simplicityhl::num::U256;
use simplicityhl::str::WitnessName;
use simplicityhl::value::ValueConstructible;
use simplicityhl::{Arguments, Value};

use crate::taproot::NUMS_KEY_BYTES;

/// Order direction: whether the maker is selling BASE or QUOTE.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderDirection {
    /// Maker offers BASE (outcome tokens), wants QUOTE (e.g. L-BTC).
    SellBase,
    /// Maker offers QUOTE (e.g. L-BTC), wants BASE (outcome tokens).
    SellQuote,
}

/// Compile-time parameters for a maker order covenant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MakerOrderParams {
    /// Outcome token asset ID.
    pub base_asset_id: [u8; 32],
    /// Quote asset ID (e.g. L-BTC).
    pub quote_asset_id: [u8; 32],
    /// Quote units per BASE lot.
    pub price: u64,
    /// Minimum lots per fill.
    pub min_fill_lots: u64,
    /// Minimum lots remaining after partial fill.
    pub min_remainder_lots: u64,
    /// Order direction.
    pub direction: OrderDirection,
    /// SHA256 of the maker's unique receive scriptPubKey (P_order).
    pub maker_receive_spk_hash: [u8; 32],
    /// Optional cosigner x-only pubkey. NUMS key bytes = no cosigner.
    pub cosigner_pubkey: [u8; 32],
    /// Maker's x-only public key (for cancel path signature verification).
    pub maker_pubkey: [u8; 32],
}

impl MakerOrderParams {
    /// Create params with automatically derived `maker_receive_spk_hash`.
    #[allow(clippy::too_many_arguments)]
    ///
    /// Internally computes `order_uid → tweak → P_order → spk_hash` and
    /// returns the fully-initialized params together with `p_order`.
    ///
    /// Struct fields remain `pub` so callers can still construct or
    /// deserialize params directly when needed.
    pub fn new(
        base_asset_id: [u8; 32],
        quote_asset_id: [u8; 32],
        price: u64,
        min_fill_lots: u64,
        min_remainder_lots: u64,
        direction: OrderDirection,
        cosigner_pubkey: [u8; 32],
        maker_base_pubkey: &[u8; 32],
        order_nonce: &[u8; 32],
    ) -> (Self, [u8; 32]) {
        let mut params = Self {
            base_asset_id,
            quote_asset_id,
            price,
            min_fill_lots,
            min_remainder_lots,
            direction,
            maker_receive_spk_hash: [0; 32],
            cosigner_pubkey,
            maker_pubkey: *maker_base_pubkey,
        };
        let (p_order, spk_hash) = derive_maker_receive(maker_base_pubkey, order_nonce, &params);
        params.maker_receive_spk_hash = spk_hash;
        (params, p_order)
    }

    /// Whether the cosigner is enabled (pubkey is not NUMS).
    pub fn has_cosigner(&self) -> bool {
        self.cosigner_pubkey != NUMS_KEY_BYTES
    }

    /// Build SimplicityHL `Arguments` for contract compilation.
    pub fn build_arguments(&self) -> Arguments {
        let map = HashMap::from([
            (
                WitnessName::from_str_unchecked("BASE_ASSET_ID"),
                Value::u256(U256::from_byte_array(self.base_asset_id)),
            ),
            (
                WitnessName::from_str_unchecked("QUOTE_ASSET_ID"),
                Value::u256(U256::from_byte_array(self.quote_asset_id)),
            ),
            (
                WitnessName::from_str_unchecked("PRICE"),
                Value::u64(self.price),
            ),
            (
                WitnessName::from_str_unchecked("MIN_FILL_LOTS"),
                Value::u64(self.min_fill_lots),
            ),
            (
                WitnessName::from_str_unchecked("MIN_REMAINDER_LOTS"),
                Value::u64(self.min_remainder_lots),
            ),
            (
                WitnessName::from_str_unchecked("IS_SELL_BASE"),
                Value::from(self.direction == OrderDirection::SellBase),
            ),
            (
                WitnessName::from_str_unchecked("MAKER_RECEIVE_SPK_HASH"),
                Value::u256(U256::from_byte_array(self.maker_receive_spk_hash)),
            ),
            (
                WitnessName::from_str_unchecked("COSIGNER_PUBKEY"),
                Value::u256(U256::from_byte_array(self.cosigner_pubkey)),
            ),
            (
                WitnessName::from_str_unchecked("MAKER_PUBKEY"),
                Value::u256(U256::from_byte_array(self.maker_pubkey)),
            ),
        ]);
        Arguments::from(map)
    }
}

/// Compute the order UID from maker pubkey, nonce, and order params.
///
/// ```text
/// order_uid = SHA256(
///     "deadcat/order_uid" ||
///     maker_base_pubkey   ||    // 32 bytes
///     order_nonce         ||    // 32 bytes
///     BASE_ASSET_ID       ||    // 32 bytes
///     QUOTE_ASSET_ID      ||    // 32 bytes
///     PRICE               ||    //  8 bytes (big-endian)
///     MIN_FILL_LOTS       ||    //  8 bytes (big-endian)
///     MIN_REMAINDER_LOTS  ||    //  8 bytes (big-endian)
///     IS_SELL_BASE              //  1 byte (0x00 or 0x01)
/// )
/// ```
pub fn order_uid(
    maker_base_pubkey: &[u8; 32],
    order_nonce: &[u8; 32],
    params: &MakerOrderParams,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"deadcat/order_uid");
    hasher.update(maker_base_pubkey);
    hasher.update(order_nonce);
    hasher.update(params.base_asset_id);
    hasher.update(params.quote_asset_id);
    hasher.update(params.price.to_be_bytes());
    hasher.update(params.min_fill_lots.to_be_bytes());
    hasher.update(params.min_remainder_lots.to_be_bytes());
    hasher.update([if params.direction == OrderDirection::SellBase {
        0x01
    } else {
        0x00
    }]);
    hasher.finalize().into()
}

/// Compute the order tweak from an order UID.
///
/// ```text
/// tweak = SHA256("deadcat/order_tweak" || order_uid)
/// ```
pub fn order_tweak(order_uid: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"deadcat/order_tweak");
    hasher.update(order_uid);
    hasher.finalize().into()
}

/// Derive P_order (the maker's unique receive x-only pubkey) from their base
/// pubkey and the order tweak.
///
/// ```text
/// P_order = P_maker_base + tweak * G
/// ```
///
/// Returns the x-only serialization of the tweaked key.
pub fn derive_p_order(maker_base_pubkey: &[u8; 32], tweak: &[u8; 32]) -> [u8; 32] {
    let secp = Secp256k1::new();
    let base_key = XOnlyPublicKey::from_slice(maker_base_pubkey).expect("valid x-only public key");
    let scalar =
        simplicityhl::elements::secp256k1_zkp::Scalar::from_be_bytes(*tweak).expect("valid scalar");
    let (tweaked, _parity) = base_key
        .add_tweak(&secp, &scalar)
        .expect("tweak should not overflow");
    tweaked.serialize()
}

/// Compute the maker receive scriptPubKey (P2TR) from a P_order x-only pubkey.
///
/// ```text
/// maker_receive_spk = OP_1 <P_order>    // 34 bytes
/// ```
pub fn maker_receive_script_pubkey(p_order: &[u8; 32]) -> Vec<u8> {
    let mut spk = Vec::with_capacity(34);
    spk.push(0x51); // OP_1
    spk.push(0x20); // PUSH32
    spk.extend_from_slice(p_order);
    spk
}

/// Compute the SHA256 hash of a maker receive scriptPubKey.
///
/// This is the value baked into the covenant as `MAKER_RECEIVE_SPK_HASH`.
pub fn maker_receive_spk_hash(p_order: &[u8; 32]) -> [u8; 32] {
    let spk = maker_receive_script_pubkey(p_order);
    sha256::Hash::hash(&spk).to_byte_array()
}

/// All-in-one: derive the P_order and its SPK hash from maker pubkey, nonce, and params.
///
/// Returns `(p_order, maker_receive_spk_hash)`.
pub fn derive_maker_receive(
    maker_base_pubkey: &[u8; 32],
    order_nonce: &[u8; 32],
    params: &MakerOrderParams,
) -> ([u8; 32], [u8; 32]) {
    let uid = order_uid(maker_base_pubkey, order_nonce, params);
    let tweak = order_tweak(&uid);
    let p_order = derive_p_order(maker_base_pubkey, &tweak);
    let spk_hash = maker_receive_spk_hash(&p_order);
    (p_order, spk_hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_PUBKEY: [u8; 32] = [0xaa; 32];
    const TEST_NONCE: [u8; 32] = [0x11; 32];

    fn test_params() -> MakerOrderParams {
        let (params, _p_order) = MakerOrderParams::new(
            [0x01; 32],
            [0xbb; 32],
            50_000,
            1,
            1,
            OrderDirection::SellBase,
            NUMS_KEY_BYTES,
            &TEST_PUBKEY,
            &TEST_NONCE,
        );
        params
    }

    #[test]
    fn order_uid_deterministic() {
        let pubkey = [0xaa; 32];
        let nonce = [0x11; 32];
        let params = test_params();
        let uid1 = order_uid(&pubkey, &nonce, &params);
        let uid2 = order_uid(&pubkey, &nonce, &params);
        assert_eq!(uid1, uid2);
    }

    #[test]
    fn order_uid_changes_with_nonce() {
        let pubkey = [0xaa; 32];
        let params = test_params();
        let uid1 = order_uid(&pubkey, &[0x11; 32], &params);
        let uid2 = order_uid(&pubkey, &[0x22; 32], &params);
        assert_ne!(uid1, uid2);
    }

    #[test]
    fn order_uid_changes_with_direction() {
        let pubkey = [0xaa; 32];
        let nonce = [0x11; 32];
        let mut params = test_params();
        let uid1 = order_uid(&pubkey, &nonce, &params);
        params.direction = OrderDirection::SellQuote;
        let uid2 = order_uid(&pubkey, &nonce, &params);
        assert_ne!(uid1, uid2);
    }

    #[test]
    fn order_uid_changes_with_price() {
        let pubkey = [0xaa; 32];
        let nonce = [0x11; 32];
        let mut params = test_params();
        let uid1 = order_uid(&pubkey, &nonce, &params);
        params.price = 100_000;
        let uid2 = order_uid(&pubkey, &nonce, &params);
        assert_ne!(uid1, uid2);
    }

    #[test]
    fn p_order_derivation_deterministic() {
        let pubkey = [0xaa; 32];
        let nonce = [0x11; 32];
        let params = test_params();
        let (p1, h1) = derive_maker_receive(&pubkey, &nonce, &params);
        let (p2, h2) = derive_maker_receive(&pubkey, &nonce, &params);
        assert_eq!(p1, p2);
        assert_eq!(h1, h2);
    }

    #[test]
    fn maker_receive_spk_is_p2tr() {
        let p_order = [0x42; 32];
        let spk = maker_receive_script_pubkey(&p_order);
        assert_eq!(spk.len(), 34);
        assert_eq!(spk[0], 0x51); // OP_1
        assert_eq!(spk[1], 0x20); // PUSH32
        assert_eq!(&spk[2..], &p_order);
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
}
