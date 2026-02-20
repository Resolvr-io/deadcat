use deadcat_sdk::elements::AddressParams;
use deadcat_sdk::taproot::simplicity_control_block;
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
    assert_eq!(cb[0], 0xbe, "first byte should be simplicity leaf version");
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
