use simplicityhl::elements::hashes::{Hash, HashEngine, sha256};
use simplicityhl::elements::secp256k1_zkp::{Secp256k1, XOnlyPublicKey};
use simplicityhl::elements::{Address, AddressParams, Script};
use simplicityhl::simplicity::Cmr;

/// NUMS (Nothing Up My Sleeve) key — provably unspendable internal key.
pub const NUMS_KEY_BYTES: [u8; 32] = [
    0x50, 0x92, 0x9b, 0x74, 0xc1, 0xa0, 0x49, 0x54, 0xb7, 0x8b, 0x4b, 0x60, 0x35, 0xe9, 0x7a, 0x5e,
    0x07, 0x8a, 0x5a, 0x0f, 0x28, 0xec, 0x96, 0xd5, 0x47, 0xbf, 0xee, 0x9a, 0xce, 0x80, 0x3a, 0xc0,
];

/// The Simplicity tapleaf version used by Elements/Liquid.
pub const SIMPLICITY_LEAF_VERSION: u8 = 0xbe;

/// Compute a SHA256 tagged hash: SHA256(SHA256(tag) || SHA256(tag) || data).
pub(crate) fn tagged_hash(tag: &[u8], data: &[u8]) -> [u8; 32] {
    let tag_hash = sha256::Hash::hash(tag);
    let mut engine = sha256::Hash::engine();
    engine.input(tag_hash.as_ref());
    engine.input(tag_hash.as_ref());
    engine.input(data);
    sha256::Hash::from_engine(engine).to_byte_array()
}

/// Compute the tapdata leaf hash for a given state value.
///
/// Uses the Simplicity "TapData" tagged hash — NOT the standard "TapLeaf/elements" tag.
/// Format: `TaggedHash("TapData", state_be_bytes)`
///
/// This matches the `jet::tapdata_init()` introspection jet in `simplicity-lang`,
/// which initializes a SHA-256 context with `SHA256("TapData") || SHA256("TapData")`.
pub fn tapdata_hash(state: u64) -> [u8; 32] {
    let state_bytes = state.to_be_bytes();
    tagged_hash(b"TapData", &state_bytes)
}

/// Compute the Simplicity tapleaf hash from a CMR.
///
/// Format: `TaggedHash("TapLeaf/elements", leaf_version || compact_size(len) || CMR)`
/// This matches how `elements::taproot::TapLeafHash::from_script` computes the hash
/// (script consensus encoding includes a compact_size length prefix).
pub fn simplicity_leaf_hash(cmr: &Cmr) -> [u8; 32] {
    let cmr_bytes = cmr.to_byte_array(); // 32 bytes
    let mut leaf_data = Vec::with_capacity(1 + 1 + cmr_bytes.len());
    leaf_data.push(SIMPLICITY_LEAF_VERSION);
    leaf_data.push(cmr_bytes.len() as u8); // compact_size(32) = 0x20
    leaf_data.extend_from_slice(&cmr_bytes);
    tagged_hash(b"TapLeaf/elements", &leaf_data)
}

/// Compute the tapbranch hash from two children (sorted lexicographically).
pub fn tapbranch_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let (a, b) = if left <= right {
        (left, right)
    } else {
        (right, left)
    };
    let mut data = Vec::with_capacity(64);
    data.extend_from_slice(a);
    data.extend_from_slice(b);
    tagged_hash(b"TapBranch/elements", &data)
}

/// Compute the taptweak hash for key tweaking.
pub fn taptweak_hash(pubkey: &[u8; 32], merkle_root: &[u8; 32]) -> [u8; 32] {
    let mut data = Vec::with_capacity(64);
    data.extend_from_slice(pubkey);
    data.extend_from_slice(merkle_root);
    tagged_hash(b"TapTweak/elements", &data)
}

/// Compute the full P2TR script pubkey from a CMR and state.
pub fn covenant_script_pubkey(cmr: &Cmr, state: u64) -> Script {
    let sim_leaf = simplicity_leaf_hash(cmr);
    let data_leaf = tapdata_hash(state);
    let branch = tapbranch_hash(&sim_leaf, &data_leaf);
    let tweak = taptweak_hash(&NUMS_KEY_BYTES, &branch);

    let secp = Secp256k1::new();
    let nums_key =
        XOnlyPublicKey::from_slice(&NUMS_KEY_BYTES).expect("NUMS key is a valid x-only public key");

    let (tweaked_key, _parity) = nums_key
        .add_tweak(
            &secp,
            &simplicityhl::elements::secp256k1_zkp::Scalar::from_be_bytes(tweak)
                .expect("tweak is a valid scalar"),
        )
        .expect("tweak should not overflow");

    // P2TR witness v1 script: OP_1 <32-byte-x-only-key>
    let mut script_bytes = Vec::with_capacity(34);
    script_bytes.push(0x51); // OP_1 (witness version 1)
    script_bytes.push(0x20); // push 32 bytes
    script_bytes.extend_from_slice(&tweaked_key.serialize());
    Script::from(script_bytes)
}

/// Compute the script hash (SHA256 of scriptPubKey) used for introspection jets.
pub fn covenant_script_hash(cmr: &Cmr, state: u64) -> [u8; 32] {
    let spk = covenant_script_pubkey(cmr, state);
    sha256::Hash::hash(spk.as_bytes()).to_byte_array()
}

/// Compute the covenant address for a given CMR and state.
pub fn covenant_address(cmr: &Cmr, state: u64, params: &'static AddressParams) -> Address {
    let spk = covenant_script_pubkey(cmr, state);
    Address::from_script(&spk, None, params).expect("valid P2TR script should produce an address")
}

/// Build the Simplicity control block for a given CMR and state.
///
/// Returns 65 bytes: `[(leaf_version | parity) | NUMS_KEY | tapdata_hash(state)]`
///
/// The first byte encodes both the leaf version (upper 7 bits) and the parity of
/// the tweaked output key (lowest bit), per BIP-341.
pub fn simplicity_control_block(cmr: &Cmr, state: u64) -> Vec<u8> {
    // Recompute the tweaked key to determine the output key parity.
    let sim_leaf = simplicity_leaf_hash(cmr);
    let data_leaf = tapdata_hash(state);
    let branch = tapbranch_hash(&sim_leaf, &data_leaf);
    let tweak = taptweak_hash(&NUMS_KEY_BYTES, &branch);

    let secp = Secp256k1::new();
    let nums_key =
        XOnlyPublicKey::from_slice(&NUMS_KEY_BYTES).expect("NUMS key is a valid x-only public key");
    let scalar = simplicityhl::elements::secp256k1_zkp::Scalar::from_be_bytes(tweak)
        .expect("tweak is a valid scalar");
    let (_tweaked_key, parity) = nums_key
        .add_tweak(&secp, &scalar)
        .expect("tweak should not overflow");

    let parity_bit: u8 = match parity {
        simplicityhl::elements::secp256k1_zkp::Parity::Even => 0,
        simplicityhl::elements::secp256k1_zkp::Parity::Odd => 1,
    };

    let mut cb = Vec::with_capacity(65);
    cb.push(SIMPLICITY_LEAF_VERSION | parity_bit);
    cb.extend_from_slice(&NUMS_KEY_BYTES);
    cb.extend_from_slice(&tapdata_hash(state));
    cb
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tapdata_hash_deterministic() {
        let h1 = tapdata_hash(0);
        let h2 = tapdata_hash(0);
        assert_eq!(h1, h2);
    }

    #[test]
    fn different_states_different_hashes() {
        let h0 = tapdata_hash(0);
        let h1 = tapdata_hash(1);
        let h2 = tapdata_hash(2);
        let h3 = tapdata_hash(3);
        assert_ne!(h0, h1);
        assert_ne!(h1, h2);
        assert_ne!(h2, h3);
    }

    #[test]
    fn tapbranch_commutative() {
        let a = [0x01; 32];
        let b = [0x02; 32];
        assert_eq!(tapbranch_hash(&a, &b), tapbranch_hash(&b, &a));
    }

    #[test]
    fn tapdata_hash_uses_tapdata_tag() {
        use sha2::{Digest, Sha256};

        // Manually compute TaggedHash("TapData", BE(1))
        // This matches jet::tapdata_init() in simplicity-lang.
        let tag_hash: [u8; 32] = Sha256::digest(b"TapData").into();
        let state_be = 1u64.to_be_bytes();

        let mut hasher = Sha256::new();
        hasher.update(tag_hash);
        hasher.update(tag_hash);
        hasher.update(state_be);
        let expected: [u8; 32] = hasher.finalize().into();

        assert_eq!(tapdata_hash(1), expected);
    }

    #[test]
    fn tapdata_hash_rejects_little_endian() {
        use sha2::{Digest, Sha256};

        // Compute the LE variant — must NOT match
        let tag_hash: [u8; 32] = Sha256::digest(b"TapData").into();
        let state_le = 1u64.to_le_bytes();

        let mut hasher = Sha256::new();
        hasher.update(tag_hash);
        hasher.update(tag_hash);
        hasher.update(state_le);
        let le_hash: [u8; 32] = hasher.finalize().into();

        assert_ne!(tapdata_hash(1), le_hash);
    }
}
