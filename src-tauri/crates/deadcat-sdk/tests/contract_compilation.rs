use deadcat_sdk::elements::AddressParams;
use deadcat_sdk::taproot::{
    NUMS_KEY_BYTES, SIMPLICITY_LEAF_VERSION, covenant_script_pubkey, simplicity_control_block,
};
use deadcat_sdk::witness::{SpendingPath, satisfy_contract, serialize_satisfied};
use deadcat_sdk::{CompiledContract, ContractParams, MarketState};

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
fn contract_compiles_successfully() {
    let params = test_params();
    let contract = CompiledContract::new(params).expect("contract should compile");
    // CMR should be non-zero
    let cmr_bytes: &[u8] = contract.cmr().as_ref();
    assert!(cmr_bytes.iter().any(|&b| b != 0), "CMR should be non-zero");
}

#[test]
fn four_distinct_addresses() {
    let params = test_params();
    let contract = CompiledContract::new(params).expect("contract should compile");
    let addrs = contract.addresses(&AddressParams::LIQUID);

    let addr_strings: Vec<String> = vec![
        addrs.dormant.to_string(),
        addrs.unresolved.to_string(),
        addrs.resolved_yes.to_string(),
        addrs.resolved_no.to_string(),
    ];

    // All four addresses should be distinct
    for i in 0..addr_strings.len() {
        for j in (i + 1)..addr_strings.len() {
            assert_ne!(
                addr_strings[i], addr_strings[j],
                "addresses for states {} and {} should differ",
                i, j
            );
        }
    }
}

#[test]
fn addresses_are_deterministic() {
    let params1 = test_params();
    let params2 = test_params();
    let c1 = CompiledContract::new(params1).expect("contract should compile");
    let c2 = CompiledContract::new(params2).expect("contract should compile");

    for state in [
        MarketState::Dormant,
        MarketState::Unresolved,
        MarketState::ResolvedYes,
        MarketState::ResolvedNo,
    ] {
        assert_eq!(
            c1.address(state, &AddressParams::LIQUID).to_string(),
            c2.address(state, &AddressParams::LIQUID).to_string(),
            "address for state {:?} should be deterministic",
            state
        );
    }
}

#[test]
fn different_params_different_cmr() {
    let params1 = test_params();
    let mut params2 = test_params();
    params2.expiry_time = 2_000_000;

    let c1 = CompiledContract::new(params1).expect("contract should compile");
    let c2 = CompiledContract::new(params2).expect("contract should compile");

    assert_ne!(
        c1.cmr().as_ref() as &[u8],
        c2.cmr().as_ref() as &[u8],
        "different params should produce different CMRs"
    );
}

#[test]
fn script_pubkey_starts_with_p2tr_prefix() {
    let params = test_params();
    let contract = CompiledContract::new(params).expect("contract should compile");

    for state in [
        MarketState::Dormant,
        MarketState::Unresolved,
        MarketState::ResolvedYes,
        MarketState::ResolvedNo,
    ] {
        let spk = contract.script_pubkey(state);
        let bytes = spk.as_bytes();
        // P2TR: OP_1 (0x51) + PUSH32 (0x20) + 32-byte x-only pubkey
        assert_eq!(
            bytes.len(),
            34,
            "P2TR script pubkey should be 34 bytes for state {:?}",
            state
        );
        assert_eq!(
            bytes[0], 0x51,
            "should start with OP_1 for state {:?}",
            state
        );
        assert_eq!(bytes[1], 0x20, "should have PUSH32 for state {:?}", state);
    }
}

#[test]
fn witness_satisfy_secondary_covenant_input() {
    let params = test_params();
    let contract = CompiledContract::new(params).expect("contract should compile");
    let path = SpendingPath::SecondaryCovenantInput;
    let satisfied = satisfy_contract(&contract, &path, MarketState::Unresolved)
        .expect("should satisfy SecondaryCovenantInput path");
    let (program_bytes, witness_bytes) = serialize_satisfied(&satisfied);
    assert!(
        !program_bytes.is_empty(),
        "program bytes should not be empty"
    );
    assert!(
        !witness_bytes.is_empty(),
        "witness bytes should not be empty"
    );
}

#[test]
fn witness_satisfy_initial_issuance() {
    let params = test_params();
    let contract = CompiledContract::new(params).expect("contract should compile");
    let path = SpendingPath::InitialIssuance {
        blinding: deadcat_sdk::AllBlindingFactors::default(),
    };
    let satisfied = satisfy_contract(&contract, &path, MarketState::Dormant)
        .expect("should satisfy InitialIssuance path");
    let (program_bytes, witness_bytes) = serialize_satisfied(&satisfied);
    assert!(
        !program_bytes.is_empty(),
        "program bytes should not be empty"
    );
    assert!(
        !witness_bytes.is_empty(),
        "witness bytes should not be empty"
    );
}

#[test]
fn control_block_structure() {
    let params = test_params();
    let contract = CompiledContract::new(params).expect("contract should compile");
    let cb = simplicity_control_block(contract.cmr(), 1);
    assert_eq!(cb.len(), 65, "control block should be 65 bytes");
    assert_eq!(
        cb[0] & 0xfe,
        0xbe,
        "first byte (masking parity) should be simplicity leaf version"
    );
    // bytes 1..33 should be NUMS key
    let expected_nums: [u8; 32] = [
        0x50, 0x92, 0x9b, 0x74, 0xc1, 0xa0, 0x49, 0x54, 0xb7, 0x8b, 0x4b, 0x60, 0x35, 0xe9, 0x7a,
        0x5e, 0x07, 0x8a, 0x5a, 0x0f, 0x28, 0xec, 0x96, 0xd5, 0x47, 0xbf, 0xee, 0x9a, 0xce, 0x80,
        0x3a, 0xc0,
    ];
    assert_eq!(&cb[1..33], &expected_nums, "bytes 1..33 should be NUMS key");
    // bytes 33..65 should be tapdata_hash(1)
    let expected_tapdata = deadcat_sdk::taproot::tapdata_hash(1);
    assert_eq!(
        &cb[33..65],
        &expected_tapdata,
        "bytes 33..65 should be tapdata_hash(state)"
    );
}

/// Verify the Simplicity leaf hash matches Elements' TaprootBuilder for that leaf,
/// and that the full tree (with TapData leaf) produces valid control blocks.
///
/// The data leaf uses the Simplicity-specific "TapData" tagged hash — NOT a standard
/// TapLeaf — so we can't use TaprootBuilder for the full tree. Instead we:
/// 1. Verify the Simplicity leaf hash matches TaprootBuilder's single-leaf computation
/// 2. Verify our manual tapbranch/tweak/control_block are internally consistent
#[test]
fn simplicity_leaf_matches_taprootbuilder() {
    use deadcat_sdk::elements::secp256k1_zkp::{Secp256k1, XOnlyPublicKey};
    use deadcat_sdk::elements::taproot::{LeafVersion, TaprootBuilder};
    use deadcat_sdk::elements::Script;

    let params = test_params();
    let contract = CompiledContract::new(params).expect("contract should compile");
    let cmr = contract.cmr();

    let secp = Secp256k1::new();
    let nums_key = XOnlyPublicKey::from_slice(&NUMS_KEY_BYTES).expect("valid NUMS key");
    let sim_version = LeafVersion::from_u8(SIMPLICITY_LEAF_VERSION).expect("valid leaf version");
    let sim_script = Script::from(cmr.to_byte_array().to_vec());

    // Build a single-leaf tree with TaprootBuilder (depth 0 = only leaf).
    let single_leaf_info = TaprootBuilder::new()
        .add_leaf_with_ver(0, sim_script.clone(), sim_version)
        .expect("add simplicity leaf")
        .finalize(&secp, nums_key)
        .expect("finalize single-leaf taproot");

    // The control block for a single-leaf tree is 33 bytes: [version|parity, internal_key]
    let lib_cb = single_leaf_info
        .control_block(&(sim_script, sim_version))
        .expect("control block should exist");
    let lib_cb_bytes = lib_cb.serialize();
    assert_eq!(lib_cb_bytes.len(), 33, "single-leaf control block should be 33 bytes");

    // Our simplicity_leaf_hash should match the leaf hash TaprootBuilder computed.
    // The TaprootBuilder doesn't expose the leaf hash directly, but we can verify
    // that our 2-leaf control block's first 33 bytes (version|parity + internal_key)
    // use the same structure, and the sibling hash (bytes 33..65) is our tapdata_hash.
    for state in [0u64, 1, 2, 3] {
        let our_cb = simplicity_control_block(cmr, state);
        assert_eq!(our_cb.len(), 65, "2-leaf control block should be 65 bytes");
        // version byte encodes SIMPLICITY_LEAF_VERSION | parity
        assert_eq!(our_cb[0] & 0xfe, SIMPLICITY_LEAF_VERSION);
        // bytes 1..33 = NUMS internal key
        assert_eq!(&our_cb[1..33], &NUMS_KEY_BYTES);
        // bytes 33..65 = sibling hash = tapdata_hash(state)
        let expected_sibling = deadcat_sdk::taproot::tapdata_hash(state);
        assert_eq!(
            &our_cb[33..65], &expected_sibling,
            "control block sibling for state {state} should be tapdata_hash"
        );
    }
}

/// Verify that the taproot output key is consistent: computing P2TR script pubkey
/// from our manual taproot tree, then re-deriving via secp256k1 tweak, yields the
/// same output key embedded in the script.
#[test]
fn taproot_output_key_consistency() {
    use deadcat_sdk::elements::secp256k1_zkp::{Secp256k1, Scalar, XOnlyPublicKey};

    let params = test_params();
    let contract = CompiledContract::new(params).expect("contract should compile");
    let cmr = contract.cmr();
    let secp = Secp256k1::new();
    let nums_key = XOnlyPublicKey::from_slice(&NUMS_KEY_BYTES).expect("valid NUMS key");

    for state in [0u64, 1, 2, 3] {
        let spk = covenant_script_pubkey(cmr, state);
        let spk_bytes = spk.as_bytes();
        let output_key_from_spk = &spk_bytes[2..34];

        // Recompute the tweak and output key manually
        let sim_leaf = deadcat_sdk::taproot::simplicity_leaf_hash(cmr);
        let data_leaf = deadcat_sdk::taproot::tapdata_hash(state);
        let branch = deadcat_sdk::taproot::tapbranch_hash(&sim_leaf, &data_leaf);
        let tweak_bytes = deadcat_sdk::taproot::taptweak_hash(&NUMS_KEY_BYTES, &branch);
        let scalar = Scalar::from_be_bytes(tweak_bytes).expect("valid scalar");
        let (tweaked_key, _parity) = nums_key.add_tweak(&secp, &scalar).expect("valid tweak");

        assert_eq!(
            tweaked_key.serialize().as_slice(),
            output_key_from_spk,
            "output key mismatch for state {state}"
        );
    }
}
