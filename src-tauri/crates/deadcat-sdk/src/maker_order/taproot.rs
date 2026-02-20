use simplicityhl::elements::hashes::{Hash, sha256};
use simplicityhl::elements::secp256k1_zkp::{Secp256k1, XOnlyPublicKey};
use simplicityhl::elements::{Address, AddressParams, Script};
use simplicityhl::simplicity::Cmr;

use crate::taproot::{SIMPLICITY_LEAF_VERSION, simplicity_leaf_hash, taptweak_hash};

/// Compute the P2TR script pubkey for a maker order covenant.
///
/// Unlike the prediction market (which uses NUMS + 2 leaves), the maker order
/// uses the maker's real pubkey as the internal key with a single Simplicity leaf.
///
/// ```text
/// leaf_hash  = TaggedHash("TapLeaf", [0xbe || CMR])
/// tweak      = TaggedHash("TapTweak", [maker_base_pubkey || leaf_hash])
/// output_key = maker_base_pubkey + tweak * G
/// spk        = OP_1 <output_key>
/// ```
pub fn maker_order_script_pubkey(cmr: &Cmr, maker_base_pubkey: &[u8; 32]) -> Script {
    let leaf = simplicity_leaf_hash(cmr);
    let tweak = taptweak_hash(maker_base_pubkey, &leaf);

    let secp = Secp256k1::new();
    let base_key = XOnlyPublicKey::from_slice(maker_base_pubkey).expect("valid x-only public key");
    let (tweaked_key, _parity) = base_key
        .add_tweak(
            &secp,
            &simplicityhl::elements::secp256k1_zkp::Scalar::from_be_bytes(tweak)
                .expect("tweak is a valid scalar"),
        )
        .expect("tweak should not overflow");

    let mut script_bytes = Vec::with_capacity(34);
    script_bytes.push(0x51); // OP_1
    script_bytes.push(0x20); // PUSH32
    script_bytes.extend_from_slice(&tweaked_key.serialize());
    Script::from(script_bytes)
}

/// Compute the script hash (SHA256 of scriptPubKey) for a maker order.
pub fn maker_order_script_hash(cmr: &Cmr, maker_base_pubkey: &[u8; 32]) -> [u8; 32] {
    let spk = maker_order_script_pubkey(cmr, maker_base_pubkey);
    sha256::Hash::hash(spk.as_bytes()).to_byte_array()
}

/// Compute the covenant address for a maker order.
pub fn maker_order_address(
    cmr: &Cmr,
    maker_base_pubkey: &[u8; 32],
    params: &'static AddressParams,
) -> Address {
    let spk = maker_order_script_pubkey(cmr, maker_base_pubkey);
    Address::from_script(&spk, None, params).expect("valid P2TR script should produce an address")
}

/// Build the Simplicity control block for a maker order.
///
/// Returns 33 bytes: `[leaf_version | maker_base_pubkey]`
///
/// This is smaller than the prediction market's 65-byte control block because
/// there is only one leaf (no sibling hash needed).
pub fn maker_order_control_block(maker_base_pubkey: &[u8; 32]) -> Vec<u8> {
    let mut cb = Vec::with_capacity(33);
    cb.push(SIMPLICITY_LEAF_VERSION);
    cb.extend_from_slice(maker_base_pubkey);
    cb
}

/// Compute the taptweak scalar for key-path spending (cancel).
///
/// The maker needs this to derive their tweaked private key:
/// ```text
/// d_tweaked = d_maker + tweak
/// ```
pub fn maker_order_taptweak(cmr: &Cmr, maker_base_pubkey: &[u8; 32]) -> [u8; 32] {
    let leaf = simplicity_leaf_hash(cmr);
    taptweak_hash(maker_base_pubkey, &leaf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_cmr() -> Cmr {
        // Use a dummy CMR for testing
        Cmr::from_byte_array([0x42; 32])
    }

    #[test]
    fn script_pubkey_is_p2tr() {
        let cmr = test_cmr();
        let pubkey = [0xaa; 32];
        let spk = maker_order_script_pubkey(&cmr, &pubkey);
        let bytes = spk.as_bytes();
        assert_eq!(bytes.len(), 34);
        assert_eq!(bytes[0], 0x51); // OP_1
        assert_eq!(bytes[1], 0x20); // PUSH32
    }

    #[test]
    fn script_pubkey_deterministic() {
        let cmr = test_cmr();
        let pubkey = [0xaa; 32];
        let spk1 = maker_order_script_pubkey(&cmr, &pubkey);
        let spk2 = maker_order_script_pubkey(&cmr, &pubkey);
        assert_eq!(spk1, spk2);
    }

    #[test]
    fn different_pubkeys_different_scripts() {
        use crate::taproot::NUMS_KEY_BYTES;
        let cmr = test_cmr();
        // Both must be valid x-only pubkeys on secp256k1
        let spk1 = maker_order_script_pubkey(&cmr, &[0xaa; 32]);
        let spk2 = maker_order_script_pubkey(&cmr, &NUMS_KEY_BYTES);
        assert_ne!(spk1, spk2);
    }

    #[test]
    fn control_block_is_33_bytes() {
        let pubkey = [0xaa; 32];
        let cb = maker_order_control_block(&pubkey);
        assert_eq!(cb.len(), 33);
        assert_eq!(cb[0], SIMPLICITY_LEAF_VERSION);
        assert_eq!(&cb[1..33], &pubkey);
    }

    #[test]
    fn taptweak_deterministic() {
        let cmr = test_cmr();
        let pubkey = [0xaa; 32];
        let t1 = maker_order_taptweak(&cmr, &pubkey);
        let t2 = maker_order_taptweak(&cmr, &pubkey);
        assert_eq!(t1, t2);
    }
}
