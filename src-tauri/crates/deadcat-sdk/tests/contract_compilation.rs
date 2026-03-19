use deadcat_sdk::elements::AddressParams;
use deadcat_sdk::taproot::{
    NUMS_KEY_BYTES, PREDICTION_MARKET_SLOT_TAPDATA_VERSION, SIMPLICITY_LEAF_VERSION,
    prediction_market_control_block, prediction_market_script_pubkey,
    prediction_market_slot_tapdata_hash,
};
use deadcat_sdk::testing::test_contract_params;
use deadcat_sdk::{
    AllBlindingFactors, CompiledPredictionMarket, MarketSlot, PredictionMarketSpendingPath,
    satisfy_contract, serialize_satisfied,
};

#[test]
fn contract_compiles_successfully() {
    let params = test_contract_params();
    let contract = CompiledPredictionMarket::new(params).expect("contract should compile");
    let cmr_bytes: &[u8] = contract.cmr().as_ref();
    assert!(cmr_bytes.iter().any(|&b| b != 0), "CMR should be non-zero");
}

#[test]
fn eight_distinct_slot_addresses() {
    let params = test_contract_params();
    let contract = CompiledPredictionMarket::new(params).expect("contract should compile");
    let addrs = contract.addresses(&AddressParams::LIQUID);

    let addr_strings: Vec<String> = vec![
        addrs.dormant.yes_rt.to_string(),
        addrs.dormant.no_rt.to_string(),
        addrs.unresolved.yes_rt.to_string(),
        addrs.unresolved.no_rt.to_string(),
        addrs.unresolved.collateral.to_string(),
        addrs.resolved_yes_collateral.to_string(),
        addrs.resolved_no_collateral.to_string(),
        addrs.expired_collateral.to_string(),
    ];

    for i in 0..addr_strings.len() {
        for j in (i + 1)..addr_strings.len() {
            assert_ne!(addr_strings[i], addr_strings[j]);
        }
    }
}

#[test]
fn slot_addresses_are_deterministic() {
    let params1 = test_contract_params();
    let params2 = test_contract_params();
    let c1 = CompiledPredictionMarket::new(params1).expect("contract should compile");
    let c2 = CompiledPredictionMarket::new(params2).expect("contract should compile");

    for slot in MarketSlot::ALL {
        assert_eq!(
            c1.address(slot, &AddressParams::LIQUID).to_string(),
            c2.address(slot, &AddressParams::LIQUID).to_string(),
            "address for slot {:?} should be deterministic",
            slot
        );
    }
}

#[test]
fn script_pubkeys_are_p2tr_for_all_slots() {
    let params = test_contract_params();
    let contract = CompiledPredictionMarket::new(params).expect("contract should compile");

    for slot in MarketSlot::ALL {
        let spk = contract.script_pubkey(slot);
        let bytes = spk.as_bytes();
        assert_eq!(bytes.len(), 34, "slot {:?} should be P2TR", slot);
        assert_eq!(bytes[0], 0x51);
        assert_eq!(bytes[1], 0x20);
    }
}

#[test]
fn witness_satisfy_initial_issuance_primary() {
    let params = test_contract_params();
    let contract = CompiledPredictionMarket::new(params).expect("contract should compile");
    let path = PredictionMarketSpendingPath::InitialIssuancePrimary {
        blinding: AllBlindingFactors::default(),
    };
    let satisfied = satisfy_contract(&contract, &path, MarketSlot::DormantYesRt)
        .expect("should satisfy InitialIssuancePrimary path");
    let (program_bytes, witness_bytes) = serialize_satisfied(&satisfied);
    assert!(!program_bytes.is_empty());
    assert!(!witness_bytes.is_empty());
}

#[test]
fn witness_satisfy_full_cancel_secondary_no_rt() {
    let params = test_contract_params();
    let contract = CompiledPredictionMarket::new(params).expect("contract should compile");
    let path = PredictionMarketSpendingPath::CancellationFullSecondaryNoRt {
        blinding: AllBlindingFactors::default(),
    };
    let satisfied = satisfy_contract(&contract, &path, MarketSlot::UnresolvedNoRt)
        .expect("should satisfy CancellationFullSecondaryNoRt path");
    let (program_bytes, witness_bytes) = serialize_satisfied(&satisfied);
    assert!(!program_bytes.is_empty());
    assert!(!witness_bytes.is_empty());
}

#[test]
fn control_block_structure_is_versioned_for_slots() {
    let params = test_contract_params();
    let contract = CompiledPredictionMarket::new(params).expect("contract should compile");
    let cb =
        prediction_market_control_block(contract.cmr(), MarketSlot::UnresolvedCollateral.as_u8());
    assert_eq!(cb.len(), 65);
    assert_eq!(cb[0] & 0xfe, 0xbe);
    assert_eq!(&cb[1..33], &NUMS_KEY_BYTES);
    let expected_tapdata =
        prediction_market_slot_tapdata_hash(MarketSlot::UnresolvedCollateral.as_u8());
    assert_eq!(&cb[33..65], &expected_tapdata);
    assert_eq!(expected_tapdata[0] != 0, true);
}

#[test]
fn prediction_market_slot_tapdata_hashes_are_versioned() {
    let dormant = prediction_market_slot_tapdata_hash(MarketSlot::DormantYesRt.as_u8());
    let unresolved = prediction_market_slot_tapdata_hash(MarketSlot::UnresolvedYesRt.as_u8());
    assert_ne!(dormant, unresolved);
    assert_ne!(
        dormant,
        deadcat_sdk::taproot::tapdata_hash(MarketSlot::DormantYesRt.as_u64())
    );
    assert_eq!(PREDICTION_MARKET_SLOT_TAPDATA_VERSION, 0x01);
}

#[test]
fn taproot_output_key_consistency_for_slots() {
    use deadcat_sdk::elements::secp256k1_zkp::{Scalar, Secp256k1, XOnlyPublicKey};

    let params = test_contract_params();
    let contract = CompiledPredictionMarket::new(params).expect("contract should compile");
    let cmr = contract.cmr();
    let secp = Secp256k1::new();
    let nums_key = XOnlyPublicKey::from_slice(&NUMS_KEY_BYTES).expect("valid NUMS key");

    for slot in MarketSlot::ALL {
        let spk = prediction_market_script_pubkey(cmr, slot.as_u8());
        let spk_bytes = spk.as_bytes();
        let output_key_from_spk = &spk_bytes[2..34];

        let sim_leaf = deadcat_sdk::taproot::simplicity_leaf_hash(cmr);
        let data_leaf = prediction_market_slot_tapdata_hash(slot.as_u8());
        let branch = deadcat_sdk::taproot::tapbranch_hash(&sim_leaf, &data_leaf);
        let tweak_bytes = deadcat_sdk::taproot::taptweak_hash(&NUMS_KEY_BYTES, &branch);
        let scalar = Scalar::from_be_bytes(tweak_bytes).expect("valid scalar");
        let (tweaked_key, _parity) = nums_key.add_tweak(&secp, &scalar).expect("valid tweak");

        assert_eq!(tweaked_key.serialize().as_slice(), output_key_from_spk);
    }
}

#[test]
fn control_block_leaf_version_stays_simplicity() {
    let params = test_contract_params();
    let contract = CompiledPredictionMarket::new(params).expect("contract should compile");
    let cb =
        prediction_market_control_block(contract.cmr(), MarketSlot::ResolvedYesCollateral.as_u8());
    assert_eq!(cb[0] & 0xfe, SIMPLICITY_LEAF_VERSION);
}
