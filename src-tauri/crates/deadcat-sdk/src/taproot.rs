use simplicityhl::elements::hashes::{Hash, HashEngine, sha256};
use simplicityhl::elements::secp256k1_zkp::{Parity, Secp256k1, XOnlyPublicKey};
use simplicityhl::elements::{Address, AddressParams, Script};
use simplicityhl::simplicity::Cmr;

/// NUMS (Nothing Up My Sleeve) key — provably unspendable internal key.
pub const NUMS_KEY_BYTES: [u8; 32] = [
    0x50, 0x92, 0x9b, 0x74, 0xc1, 0xa0, 0x49, 0x54, 0xb7, 0x8b, 0x4b, 0x60, 0x35, 0xe9, 0x7a, 0x5e,
    0x07, 0x8a, 0x5a, 0x0f, 0x28, 0xec, 0x96, 0xd5, 0x47, 0xbf, 0xee, 0x9a, 0xce, 0x80, 0x3a, 0xc0,
];

/// The Simplicity tapleaf version used by Elements/Liquid.
pub const SIMPLICITY_LEAF_VERSION: u8 = 0xbe;
/// Version byte for prediction-market slot TapData leaves.
pub const PREDICTION_MARKET_SLOT_TAPDATA_VERSION: u8 = 0x01;

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
    tapdata_hash_bytes(&state.to_be_bytes())
}

/// Compute the TapData hash for a prediction-market slot commitment.
///
/// Format: `TaggedHash("TapData", [0x01, slot])`
pub fn prediction_market_slot_tapdata_hash(slot: u8) -> [u8; 32] {
    tapdata_hash_bytes(&[PREDICTION_MARKET_SLOT_TAPDATA_VERSION, slot])
}

fn tapdata_hash_bytes(data: &[u8]) -> [u8; 32] {
    tagged_hash(b"TapData", data)
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

fn tweak_output_key(
    internal_key_bytes: &[u8; 32],
    merkle_root: &[u8; 32],
) -> (XOnlyPublicKey, Parity) {
    let tweak = taptweak_hash(internal_key_bytes, merkle_root);
    let secp = Secp256k1::new();
    let internal_key = XOnlyPublicKey::from_slice(internal_key_bytes)
        .expect("internal key must be a valid x-only public key");
    internal_key
        .add_tweak(
            &secp,
            &simplicityhl::elements::secp256k1_zkp::Scalar::from_be_bytes(tweak)
                .expect("taptweak is a valid scalar"),
        )
        .expect("taptweak should not overflow")
}

fn parity_bit(parity: Parity) -> u8 {
    match parity {
        Parity::Even => 0,
        Parity::Odd => 1,
    }
}

fn build_p2tr_script(output_key: &XOnlyPublicKey) -> Script {
    let mut script_bytes = Vec::with_capacity(34);
    script_bytes.push(0x51); // OP_1 (witness version 1)
    script_bytes.push(0x20); // push 32 bytes
    script_bytes.extend_from_slice(&output_key.serialize());
    Script::from(script_bytes)
}

fn nums_control_block(merkle_root: &[u8; 32], merkle_path: &[[u8; 32]]) -> Vec<u8> {
    let (_output_key, parity) = tweak_output_key(&NUMS_KEY_BYTES, merkle_root);
    let mut cb = Vec::with_capacity(1 + 32 + 32 * merkle_path.len());
    cb.push(SIMPLICITY_LEAF_VERSION | parity_bit(parity));
    cb.extend_from_slice(&NUMS_KEY_BYTES);
    for sibling in merkle_path {
        cb.extend_from_slice(sibling);
    }
    cb
}

/// Compute the full P2TR script pubkey from a Taproot merkle root using NUMS as internal key.
pub fn covenant_script_pubkey_from_root(merkle_root: &[u8; 32]) -> Script {
    let (output_key, _parity) = tweak_output_key(&NUMS_KEY_BYTES, merkle_root);
    build_p2tr_script(&output_key)
}

/// Compute the script hash (SHA256 of scriptPubKey) from a Taproot merkle root.
pub fn covenant_script_hash_from_root(merkle_root: &[u8; 32]) -> [u8; 32] {
    let spk = covenant_script_pubkey_from_root(merkle_root);
    sha256::Hash::hash(spk.as_bytes()).to_byte_array()
}

/// Compute the covenant address from a Taproot merkle root.
pub fn covenant_address_from_root(
    merkle_root: &[u8; 32],
    params: &'static AddressParams,
) -> Address {
    let spk = covenant_script_pubkey_from_root(merkle_root);
    Address::from_script(&spk, None, params).expect("valid P2TR script should produce an address")
}

/// Compute the full P2TR script pubkey from a CMR and state.
#[allow(dead_code)]
pub fn covenant_script_pubkey(cmr: &Cmr, state: u64) -> Script {
    covenant_script_pubkey_with_tapdata_leaf(cmr, &tapdata_hash(state))
}

/// Compute the full P2TR script pubkey from a CMR and prediction-market slot.
pub fn prediction_market_script_pubkey(cmr: &Cmr, slot: u8) -> Script {
    covenant_script_pubkey_with_tapdata_leaf(cmr, &prediction_market_slot_tapdata_hash(slot))
}

fn covenant_script_pubkey_with_tapdata_leaf(cmr: &Cmr, tapdata_leaf: &[u8; 32]) -> Script {
    let sim_leaf = simplicity_leaf_hash(cmr);
    let merkle_root = tapbranch_hash(&sim_leaf, tapdata_leaf);
    covenant_script_pubkey_from_root(&merkle_root)
}

/// Compute the script hash (SHA256 of scriptPubKey) used for introspection jets.
#[allow(dead_code)]
pub fn covenant_script_hash(cmr: &Cmr, state: u64) -> [u8; 32] {
    covenant_script_hash_with_tapdata_leaf(cmr, &tapdata_hash(state))
}

/// Compute the script hash for a prediction-market slot.
pub fn prediction_market_script_hash(cmr: &Cmr, slot: u8) -> [u8; 32] {
    covenant_script_hash_with_tapdata_leaf(cmr, &prediction_market_slot_tapdata_hash(slot))
}

fn covenant_script_hash_with_tapdata_leaf(cmr: &Cmr, tapdata_leaf: &[u8; 32]) -> [u8; 32] {
    let sim_leaf = simplicity_leaf_hash(cmr);
    let merkle_root = tapbranch_hash(&sim_leaf, tapdata_leaf);
    covenant_script_hash_from_root(&merkle_root)
}

/// Compute the covenant address for a given CMR and state.
#[allow(dead_code)]
pub fn covenant_address(cmr: &Cmr, state: u64, params: &'static AddressParams) -> Address {
    covenant_address_with_tapdata_leaf(cmr, &tapdata_hash(state), params)
}

/// Compute the covenant address for a prediction-market slot.
pub fn prediction_market_address(cmr: &Cmr, slot: u8, params: &'static AddressParams) -> Address {
    covenant_address_with_tapdata_leaf(cmr, &prediction_market_slot_tapdata_hash(slot), params)
}

fn covenant_address_with_tapdata_leaf(
    cmr: &Cmr,
    tapdata_leaf: &[u8; 32],
    params: &'static AddressParams,
) -> Address {
    let sim_leaf = simplicity_leaf_hash(cmr);
    let merkle_root = tapbranch_hash(&sim_leaf, tapdata_leaf);
    covenant_address_from_root(&merkle_root, params)
}

/// Build the Simplicity control block for a given CMR and state.
///
/// Returns 65 bytes: `[(leaf_version | parity) | NUMS_KEY | tapdata_hash(state)]`
///
/// The first byte encodes both the leaf version (upper 7 bits) and the parity of
/// the tweaked output key (lowest bit), per BIP-341.
#[allow(dead_code)]
pub fn simplicity_control_block(cmr: &Cmr, state: u64) -> Vec<u8> {
    simplicity_control_block_with_tapdata_leaf(cmr, &tapdata_hash(state))
}

/// Build the Simplicity control block for a prediction-market slot.
pub fn prediction_market_control_block(cmr: &Cmr, slot: u8) -> Vec<u8> {
    simplicity_control_block_with_tapdata_leaf(cmr, &prediction_market_slot_tapdata_hash(slot))
}

fn simplicity_control_block_with_tapdata_leaf(cmr: &Cmr, tapdata_leaf: &[u8; 32]) -> Vec<u8> {
    let sim_leaf = simplicity_leaf_hash(cmr);
    let merkle_root = tapbranch_hash(&sim_leaf, tapdata_leaf);
    nums_control_block(&merkle_root, &[*tapdata_leaf])
}

/// Hashes for the canonical LMSR tree shape: `((primary, secondary), tapdata)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub struct LmsrTreeHashes {
    pub primary_leaf: [u8; 32],
    pub secondary_leaf: [u8; 32],
    pub tapdata_leaf: [u8; 32],
    pub primary_secondary_branch: [u8; 32],
    pub merkle_root: [u8; 32],
}

/// Compute all intermediate hashes for the canonical LMSR Taproot tree.
#[allow(dead_code)]
pub fn lmsr_tree_hashes(primary_cmr: &Cmr, secondary_cmr: &Cmr, state: u64) -> LmsrTreeHashes {
    let primary_leaf = simplicity_leaf_hash(primary_cmr);
    let secondary_leaf = simplicity_leaf_hash(secondary_cmr);
    let tapdata_leaf = tapdata_hash(state);
    let primary_secondary_branch = tapbranch_hash(&primary_leaf, &secondary_leaf);
    let merkle_root = tapbranch_hash(&primary_secondary_branch, &tapdata_leaf);
    LmsrTreeHashes {
        primary_leaf,
        secondary_leaf,
        tapdata_leaf,
        primary_secondary_branch,
        merkle_root,
    }
}

/// Compute the canonical LMSR tree root.
#[allow(dead_code)]
pub fn lmsr_merkle_root(primary_cmr: &Cmr, secondary_cmr: &Cmr, state: u64) -> [u8; 32] {
    lmsr_tree_hashes(primary_cmr, secondary_cmr, state).merkle_root
}

/// Compute LMSR covenant scriptPubKey from `(primary, secondary, tapdata)` tree.
#[allow(dead_code)]
pub fn lmsr_script_pubkey(primary_cmr: &Cmr, secondary_cmr: &Cmr, state: u64) -> Script {
    let hashes = lmsr_tree_hashes(primary_cmr, secondary_cmr, state);
    covenant_script_pubkey_from_root(&hashes.merkle_root)
}

/// Compute LMSR covenant script hash from `(primary, secondary, tapdata)` tree.
#[allow(dead_code)]
pub fn lmsr_script_hash(primary_cmr: &Cmr, secondary_cmr: &Cmr, state: u64) -> [u8; 32] {
    let hashes = lmsr_tree_hashes(primary_cmr, secondary_cmr, state);
    covenant_script_hash_from_root(&hashes.merkle_root)
}

/// Compute LMSR covenant address from `(primary, secondary, tapdata)` tree.
#[allow(dead_code)]
pub fn lmsr_address(
    primary_cmr: &Cmr,
    secondary_cmr: &Cmr,
    state: u64,
    params: &'static AddressParams,
) -> Address {
    let hashes = lmsr_tree_hashes(primary_cmr, secondary_cmr, state);
    covenant_address_from_root(&hashes.merkle_root, params)
}

/// Build control block for LMSR primary leaf spend.
///
/// Merkle path is `[secondary_leaf, tapdata_leaf]`.
#[allow(dead_code)]
pub fn lmsr_primary_control_block(primary_cmr: &Cmr, secondary_cmr: &Cmr, state: u64) -> Vec<u8> {
    let hashes = lmsr_tree_hashes(primary_cmr, secondary_cmr, state);
    nums_control_block(
        &hashes.merkle_root,
        &[hashes.secondary_leaf, hashes.tapdata_leaf],
    )
}

/// Build control block for LMSR secondary leaf spend.
///
/// Merkle path is `[primary_leaf, tapdata_leaf]`.
#[allow(dead_code)]
pub fn lmsr_secondary_control_block(primary_cmr: &Cmr, secondary_cmr: &Cmr, state: u64) -> Vec<u8> {
    let hashes = lmsr_tree_hashes(primary_cmr, secondary_cmr, state);
    nums_control_block(
        &hashes.merkle_root,
        &[hashes.primary_leaf, hashes.tapdata_leaf],
    )
}

/// Build control block for LMSR tapdata leaf vectors.
///
/// Merkle path is `[tapbranch_hash(primary_leaf, secondary_leaf)]`.
#[allow(dead_code)]
pub fn lmsr_tapdata_control_block(primary_cmr: &Cmr, secondary_cmr: &Cmr, state: u64) -> Vec<u8> {
    let hashes = lmsr_tree_hashes(primary_cmr, secondary_cmr, state);
    nums_control_block(&hashes.merkle_root, &[hashes.primary_secondary_branch])
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

    #[test]
    fn covenant_script_pubkey_from_root_matches_legacy() {
        let cmr = Cmr::from_byte_array([0x55; 32]);
        let state = 42;
        let legacy_spk = covenant_script_pubkey(&cmr, state);

        let sim_leaf = simplicity_leaf_hash(&cmr);
        let data_leaf = tapdata_hash(state);
        let root = tapbranch_hash(&sim_leaf, &data_leaf);
        let from_root_spk = covenant_script_pubkey_from_root(&root);

        assert_eq!(legacy_spk, from_root_spk);
    }

    #[test]
    fn lmsr_tree_root_deterministic() {
        let p = Cmr::from_byte_array([0x11; 32]);
        let s = Cmr::from_byte_array([0x22; 32]);
        let r1 = lmsr_merkle_root(&p, &s, 5);
        let r2 = lmsr_merkle_root(&p, &s, 5);
        assert_eq!(r1, r2);
    }

    #[test]
    fn lmsr_control_blocks_have_expected_lengths() {
        let p = Cmr::from_byte_array([0x11; 32]);
        let s = Cmr::from_byte_array([0x22; 32]);
        let cb_primary = lmsr_primary_control_block(&p, &s, 7);
        let cb_secondary = lmsr_secondary_control_block(&p, &s, 7);
        let cb_tapdata = lmsr_tapdata_control_block(&p, &s, 7);

        assert_eq!(cb_primary.len(), 97); // 1 + 32 + 32 + 32
        assert_eq!(cb_secondary.len(), 97);
        assert_eq!(cb_tapdata.len(), 65); // 1 + 32 + 32
        assert_eq!(cb_primary[0] & 0xfe, SIMPLICITY_LEAF_VERSION);
        assert_eq!(cb_secondary[0] & 0xfe, SIMPLICITY_LEAF_VERSION);
        assert_eq!(cb_tapdata[0] & 0xfe, SIMPLICITY_LEAF_VERSION);
    }

    #[test]
    fn lmsr_control_blocks_share_same_parity_bit() {
        let p = Cmr::from_byte_array([0x31; 32]);
        let s = Cmr::from_byte_array([0x32; 32]);
        let cb_primary = lmsr_primary_control_block(&p, &s, 99);
        let cb_secondary = lmsr_secondary_control_block(&p, &s, 99);
        let cb_tapdata = lmsr_tapdata_control_block(&p, &s, 99);

        assert_eq!(cb_primary[0] & 0x01, cb_secondary[0] & 0x01);
        assert_eq!(cb_secondary[0] & 0x01, cb_tapdata[0] & 0x01);
    }
}
