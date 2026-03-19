use std::sync::Arc;

use deadcat_sdk::elements::confidential::Value as ConfValue;
use deadcat_sdk::elements::hashes::Hash;
use deadcat_sdk::elements::secp256k1_zkp::ZERO_TWEAK;
use deadcat_sdk::elements::{
    AssetIssuance, LockTime, OutPoint, Script, Sequence, Transaction, TxIn, Txid,
};
use deadcat_sdk::simplicity::jet::elements::ElementsUtxo;
use deadcat_sdk::testing::{
    AssembledEnvTx, TestCancellationParams, TestExpireTransitionParams, TestExpiryRedemptionParams,
    TestOracleResolveParams, TestPostResolutionRedemptionParams, assemble_cancellation_for_env,
    assemble_expire_transition_for_env, assemble_expiry_redemption_for_env,
    assemble_issuance_for_env, assemble_oracle_resolve_for_env,
    assemble_post_resolution_redemption_for_env, confidential_rt_txout, execute_against_env,
    explicit_txout, issuance_txin, simple_txin, test_blinding, test_change_script,
    test_confidential_rt_utxo, test_contract_params, test_contract_params_with_defining_outpoints,
    test_contract_params_with_oracle_pubkey, test_explicit_utxo, test_issuance_entropy,
    test_oracle_keypair, test_oracle_signature, test_outpoint, test_script,
};
use deadcat_sdk::{
    AllBlindingFactors, CollateralSource, CompiledPredictionMarket, IssuanceAssemblyInputs,
    MarketSlot, MarketState, PredictionMarketSpendingPath,
};

fn fee_output(asset: &[u8; 32], amount: u64) -> deadcat_sdk::elements::TxOut {
    explicit_txout(asset, amount, &Script::new())
}

fn inflation_keys_txin(outpoint: OutPoint, inflation_keys: u64) -> TxIn {
    TxIn {
        previous_output: outpoint,
        is_pegin: false,
        script_sig: Script::new(),
        sequence: Sequence::ENABLE_LOCKTIME_NO_RBF,
        asset_issuance: AssetIssuance {
            asset_blinding_nonce: ZERO_TWEAK,
            asset_entropy: [0x30; 32],
            amount: ConfValue::Null,
            inflation_keys: ConfValue::Explicit(inflation_keys),
        },
        witness: Default::default(),
    }
}

fn assert_case_input_executes(
    contract: &CompiledPredictionMarket,
    case: AssembledEnvTx,
    case_input_index: usize,
) {
    let spend = case.covenant_inputs[case_input_index].clone();
    let result = execute_against_env(
        contract,
        spend.slot,
        &spend.path,
        case.tx,
        case.utxos,
        spend.input_index,
    );

    assert!(result.is_ok(), "{result:?}");
}

fn build_subsequent_issuance_case() -> (CompiledPredictionMarket, AssembledEnvTx) {
    let yes_defining_outpoint = test_outpoint(0xa1);
    let no_defining_outpoint = test_outpoint(0xa2);
    let params = test_contract_params_with_defining_outpoints(
        [0xaa; 32],
        yes_defining_outpoint,
        no_defining_outpoint,
    );
    let contract = CompiledPredictionMarket::new(params).expect("compile");
    let assembly_contract = CompiledPredictionMarket::new(params).expect("compile");
    let blinding = test_blinding();
    let pairs = 5;
    let input_yes_spk = contract.script_pubkey(MarketSlot::UnresolvedYesRt);
    let input_no_spk = contract.script_pubkey(MarketSlot::UnresolvedNoRt);
    let collateral_spk = contract.script_pubkey(MarketSlot::UnresolvedCollateral);

    let inputs = IssuanceAssemblyInputs {
        contract: assembly_contract,
        current_state: MarketState::Unresolved,
        yes_reissuance_utxo: test_confidential_rt_utxo(
            &params.yes_reissuance_token,
            &input_yes_spk,
            &blinding.yes.input_abf,
            &blinding.yes.input_vbf,
            0x11,
        ),
        no_reissuance_utxo: test_confidential_rt_utxo(
            &params.no_reissuance_token,
            &input_no_spk,
            &blinding.no.input_abf,
            &blinding.no.input_vbf,
            0x12,
        ),
        collateral_source: CollateralSource::Subsequent {
            covenant_collateral: test_explicit_utxo(
                &params.collateral_asset_id,
                2_000_000,
                &collateral_spk,
                0x13,
            ),
            new_wallet_utxo: test_explicit_utxo(
                &params.collateral_asset_id,
                pairs * 2 * params.collateral_per_token,
                &test_script(1),
                0x14,
            ),
        },
        fee_utxo: test_explicit_utxo(&params.collateral_asset_id, 1_000, &test_script(2), 0x15),
        pairs,
        fee_amount: 1_000,
        token_destination: test_script(3),
        change_destination: None,
        issuance_entropy: test_issuance_entropy(
            yes_defining_outpoint,
            no_defining_outpoint,
            blinding.yes.input_abf,
            blinding.no.input_abf,
        ),
        lock_time: 0,
    };

    let case = assemble_issuance_for_env(inputs).expect("assemble issuance");
    (contract, case)
}

fn build_oracle_resolve_case(outcome_yes: bool) -> (CompiledPredictionMarket, AssembledEnvTx) {
    let (oracle_pubkey, oracle_keypair) = test_oracle_keypair();
    let params = test_contract_params_with_oracle_pubkey(oracle_pubkey);
    let contract = CompiledPredictionMarket::new(params).expect("compile");
    let blinding = test_blinding();
    let signature = test_oracle_signature(contract.params(), outcome_yes, &oracle_keypair);
    let unresolved_yes_spk = contract.script_pubkey(MarketSlot::UnresolvedYesRt);
    let unresolved_no_spk = contract.script_pubkey(MarketSlot::UnresolvedNoRt);
    let unresolved_collateral_spk = contract.script_pubkey(MarketSlot::UnresolvedCollateral);

    let case = assemble_oracle_resolve_for_env(
        &contract,
        TestOracleResolveParams {
            yes_reissuance_utxo: test_confidential_rt_utxo(
                &params.yes_reissuance_token,
                &unresolved_yes_spk,
                &blinding.yes.input_abf,
                &blinding.yes.input_vbf,
                0x21,
            ),
            no_reissuance_utxo: test_confidential_rt_utxo(
                &params.no_reissuance_token,
                &unresolved_no_spk,
                &blinding.no.input_abf,
                &blinding.no.input_vbf,
                0x22,
            ),
            collateral_utxo: test_explicit_utxo(
                &params.collateral_asset_id,
                2_000_000,
                &unresolved_collateral_spk,
                0x23,
            ),
            fee_utxo: test_explicit_utxo(&params.collateral_asset_id, 2_000, &test_script(4), 0x24),
            outcome_yes,
            fee_amount: 1_000,
            fee_change_destination: Some(test_change_script()),
            lock_time: 0,
        },
        signature,
    )
    .expect("assemble resolve");

    (contract, case)
}

fn build_oracle_resolve_no_change_case(
    outcome_yes: bool,
) -> (CompiledPredictionMarket, AssembledEnvTx) {
    let (oracle_pubkey, oracle_keypair) = test_oracle_keypair();
    let params = test_contract_params_with_oracle_pubkey(oracle_pubkey);
    let contract = CompiledPredictionMarket::new(params).expect("compile");
    let blinding = test_blinding();
    let signature = test_oracle_signature(contract.params(), outcome_yes, &oracle_keypair);
    let unresolved_yes_spk = contract.script_pubkey(MarketSlot::UnresolvedYesRt);
    let unresolved_no_spk = contract.script_pubkey(MarketSlot::UnresolvedNoRt);
    let unresolved_collateral_spk = contract.script_pubkey(MarketSlot::UnresolvedCollateral);

    let case = assemble_oracle_resolve_for_env(
        &contract,
        TestOracleResolveParams {
            yes_reissuance_utxo: test_confidential_rt_utxo(
                &params.yes_reissuance_token,
                &unresolved_yes_spk,
                &blinding.yes.input_abf,
                &blinding.yes.input_vbf,
                0x25,
            ),
            no_reissuance_utxo: test_confidential_rt_utxo(
                &params.no_reissuance_token,
                &unresolved_no_spk,
                &blinding.no.input_abf,
                &blinding.no.input_vbf,
                0x26,
            ),
            collateral_utxo: test_explicit_utxo(
                &params.collateral_asset_id,
                2_000_000,
                &unresolved_collateral_spk,
                0x27,
            ),
            fee_utxo: test_explicit_utxo(&params.collateral_asset_id, 1_000, &test_script(6), 0x28),
            outcome_yes,
            fee_amount: 1_000,
            fee_change_destination: None,
            lock_time: 0,
        },
        signature,
    )
    .expect("assemble resolve without change");

    (contract, case)
}

fn build_expire_transition_case() -> (CompiledPredictionMarket, AssembledEnvTx) {
    let params = test_contract_params();
    let contract = CompiledPredictionMarket::new(params).expect("compile");
    let blinding = test_blinding();
    let case = assemble_expire_transition_for_env(
        &contract,
        TestExpireTransitionParams {
            yes_reissuance_utxo: test_confidential_rt_utxo(
                &params.yes_reissuance_token,
                &contract.script_pubkey(MarketSlot::UnresolvedYesRt),
                &blinding.yes.input_abf,
                &blinding.yes.input_vbf,
                0x31,
            ),
            no_reissuance_utxo: test_confidential_rt_utxo(
                &params.no_reissuance_token,
                &contract.script_pubkey(MarketSlot::UnresolvedNoRt),
                &blinding.no.input_abf,
                &blinding.no.input_vbf,
                0x32,
            ),
            collateral_utxo: test_explicit_utxo(
                &params.collateral_asset_id,
                2_000_000,
                &contract.script_pubkey(MarketSlot::UnresolvedCollateral),
                0x33,
            ),
            fee_utxo: test_explicit_utxo(&params.collateral_asset_id, 2_000, &test_script(5), 0x34),
            fee_amount: 1_000,
            fee_change_destination: Some(test_change_script()),
            lock_time: params.expiry_time,
        },
    )
    .expect("assemble expire");

    (contract, case)
}

fn build_expire_transition_no_change_case() -> (CompiledPredictionMarket, AssembledEnvTx) {
    let params = test_contract_params();
    let contract = CompiledPredictionMarket::new(params).expect("compile");
    let blinding = test_blinding();
    let case = assemble_expire_transition_for_env(
        &contract,
        TestExpireTransitionParams {
            yes_reissuance_utxo: test_confidential_rt_utxo(
                &params.yes_reissuance_token,
                &contract.script_pubkey(MarketSlot::UnresolvedYesRt),
                &blinding.yes.input_abf,
                &blinding.yes.input_vbf,
                0x35,
            ),
            no_reissuance_utxo: test_confidential_rt_utxo(
                &params.no_reissuance_token,
                &contract.script_pubkey(MarketSlot::UnresolvedNoRt),
                &blinding.no.input_abf,
                &blinding.no.input_vbf,
                0x36,
            ),
            collateral_utxo: test_explicit_utxo(
                &params.collateral_asset_id,
                2_000_000,
                &contract.script_pubkey(MarketSlot::UnresolvedCollateral),
                0x37,
            ),
            fee_utxo: test_explicit_utxo(&params.collateral_asset_id, 1_000, &test_script(7), 0x38),
            fee_amount: 1_000,
            fee_change_destination: None,
            lock_time: params.expiry_time,
        },
    )
    .expect("assemble expire without change");

    (contract, case)
}

fn build_post_resolution_redemption_case(
    outcome_yes: bool,
) -> (CompiledPredictionMarket, AssembledEnvTx) {
    let params = test_contract_params();
    let contract = CompiledPredictionMarket::new(params).expect("compile");
    let resolved_state = if outcome_yes {
        MarketState::ResolvedYes
    } else {
        MarketState::ResolvedNo
    };
    let burn_asset = if outcome_yes {
        params.yes_token_asset
    } else {
        params.no_token_asset
    };
    let collateral_slot = resolved_state
        .collateral_slot()
        .expect("resolved collateral slot");

    let case = assemble_post_resolution_redemption_for_env(
        &contract,
        TestPostResolutionRedemptionParams {
            collateral_utxo: test_explicit_utxo(
                &params.collateral_asset_id,
                2_000_000,
                &contract.script_pubkey(collateral_slot),
                0x41,
            ),
            token_utxos: vec![test_explicit_utxo(&burn_asset, 5, &test_script(6), 0x42)],
            fee_utxo: test_explicit_utxo(&params.collateral_asset_id, 1_000, &test_script(7), 0x43),
            tokens_burned: 5,
            resolved_state,
            fee_amount: 1_000,
            payout_destination: test_script(8),
            fee_change_destination: None,
            token_change_destination: None,
        },
    )
    .expect("assemble resolved redemption");

    (contract, case)
}

fn build_expiry_redemption_case(burn_yes: bool) -> (CompiledPredictionMarket, AssembledEnvTx) {
    let params = test_contract_params();
    let contract = CompiledPredictionMarket::new(params).expect("compile");
    let burn_token_asset = if burn_yes {
        params.yes_token_asset
    } else {
        params.no_token_asset
    };

    let case = assemble_expiry_redemption_for_env(
        &contract,
        TestExpiryRedemptionParams {
            collateral_utxo: test_explicit_utxo(
                &params.collateral_asset_id,
                2_000_000,
                &contract.script_pubkey(MarketSlot::ExpiredCollateral),
                0x51,
            ),
            token_utxos: vec![test_explicit_utxo(
                &burn_token_asset,
                5,
                &test_script(9),
                0x52,
            )],
            fee_utxo: test_explicit_utxo(
                &params.collateral_asset_id,
                1_000,
                &test_script(10),
                0x53,
            ),
            tokens_burned: 5,
            burn_token_asset,
            fee_amount: 1_000,
            payout_destination: test_script(11),
            fee_change_destination: None,
            token_change_destination: None,
            lock_time: params.expiry_time,
        },
    )
    .expect("assemble expiry redemption");

    (contract, case)
}

fn build_partial_cancellation_case() -> (CompiledPredictionMarket, AssembledEnvTx) {
    let params = test_contract_params();
    let contract = CompiledPredictionMarket::new(params).expect("compile");
    let case = assemble_cancellation_for_env(
        &contract,
        TestCancellationParams {
            collateral_utxo: test_explicit_utxo(
                &params.collateral_asset_id,
                2_000_000,
                &contract.script_pubkey(MarketSlot::UnresolvedCollateral),
                0x61,
            ),
            yes_reissuance_utxo: None,
            no_reissuance_utxo: None,
            yes_token_utxos: vec![test_explicit_utxo(
                &params.yes_token_asset,
                5,
                &test_script(12),
                0x62,
            )],
            no_token_utxos: vec![test_explicit_utxo(
                &params.no_token_asset,
                5,
                &test_script(13),
                0x63,
            )],
            fee_utxo: test_explicit_utxo(
                &params.collateral_asset_id,
                1_000,
                &test_script(14),
                0x64,
            ),
            pairs_burned: 5,
            fee_amount: 1_000,
            refund_destination: test_script(15),
            fee_change_destination: None,
            token_change_destination: None,
        },
    )
    .expect("assemble partial cancel");

    (contract, case)
}

fn build_full_cancellation_case() -> (CompiledPredictionMarket, AssembledEnvTx) {
    let params = test_contract_params();
    let contract = CompiledPredictionMarket::new(params).expect("compile");
    let blinding = test_blinding();
    let case = assemble_cancellation_for_env(
        &contract,
        TestCancellationParams {
            collateral_utxo: test_explicit_utxo(
                &params.collateral_asset_id,
                2_000_000,
                &contract.script_pubkey(MarketSlot::UnresolvedCollateral),
                0x71,
            ),
            yes_reissuance_utxo: Some(test_confidential_rt_utxo(
                &params.yes_reissuance_token,
                &contract.script_pubkey(MarketSlot::UnresolvedYesRt),
                &blinding.yes.input_abf,
                &blinding.yes.input_vbf,
                0x72,
            )),
            no_reissuance_utxo: Some(test_confidential_rt_utxo(
                &params.no_reissuance_token,
                &contract.script_pubkey(MarketSlot::UnresolvedNoRt),
                &blinding.no.input_abf,
                &blinding.no.input_vbf,
                0x73,
            )),
            yes_token_utxos: vec![test_explicit_utxo(
                &params.yes_token_asset,
                10,
                &test_script(1),
                0x74,
            )],
            no_token_utxos: vec![test_explicit_utxo(
                &params.no_token_asset,
                10,
                &test_script(2),
                0x75,
            )],
            fee_utxo: test_explicit_utxo(&params.collateral_asset_id, 1_000, &test_script(3), 0x76),
            pairs_burned: 10,
            fee_amount: 1_000,
            refund_destination: test_script(4),
            fee_change_destination: None,
            token_change_destination: None,
        },
    )
    .expect("assemble full cancel");

    (contract, case)
}

#[test]
fn initial_issuance_primary_executes() {
    let params = test_contract_params();
    let contract = CompiledPredictionMarket::new(params).expect("compile");
    let blinding = test_blinding();
    let pairs = 10;
    let total_collateral = pairs * 2 * params.collateral_per_token;

    let utxos = vec![
        ElementsUtxo::from(confidential_rt_txout(
            &params.yes_reissuance_token,
            &blinding.yes.input_abf,
            &blinding.yes.input_vbf,
            &contract.script_pubkey(MarketSlot::DormantYesRt),
        )),
        ElementsUtxo::from(confidential_rt_txout(
            &params.no_reissuance_token,
            &blinding.no.input_abf,
            &blinding.no.input_vbf,
            &contract.script_pubkey(MarketSlot::DormantNoRt),
        )),
        ElementsUtxo::from(explicit_txout(
            &params.collateral_asset_id,
            total_collateral,
            &Script::new(),
        )),
        ElementsUtxo::from(explicit_txout(
            &params.collateral_asset_id,
            1_000,
            &Script::new(),
        )),
    ];

    let tx = Arc::new(Transaction {
        version: 2,
        lock_time: LockTime::ZERO,
        input: vec![
            issuance_txin(OutPoint::new(Txid::all_zeros(), 0), pairs),
            issuance_txin(OutPoint::new(Txid::all_zeros(), 1), pairs),
            simple_txin(OutPoint::new(Txid::all_zeros(), 2)),
            simple_txin(OutPoint::new(Txid::all_zeros(), 3)),
        ],
        output: vec![
            confidential_rt_txout(
                &params.yes_reissuance_token,
                &blinding.yes.output_abf,
                &blinding.yes.output_vbf,
                &contract.script_pubkey(MarketSlot::UnresolvedYesRt),
            ),
            confidential_rt_txout(
                &params.no_reissuance_token,
                &blinding.no.output_abf,
                &blinding.no.output_vbf,
                &contract.script_pubkey(MarketSlot::UnresolvedNoRt),
            ),
            explicit_txout(
                &params.collateral_asset_id,
                total_collateral,
                &contract.script_pubkey(MarketSlot::UnresolvedCollateral),
            ),
            explicit_txout(&params.yes_token_asset, pairs, &Script::new()),
            explicit_txout(&params.no_token_asset, pairs, &Script::new()),
            fee_output(&params.collateral_asset_id, 1_000),
        ],
    });

    let result = execute_against_env(
        &contract,
        MarketSlot::DormantYesRt,
        &PredictionMarketSpendingPath::InitialIssuancePrimary { blinding },
        tx,
        utxos,
        0,
    );

    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn initial_issuance_secondary_no_rt_executes() {
    let params = test_contract_params();
    let contract = CompiledPredictionMarket::new(params).expect("compile");
    let blinding = test_blinding();
    let pairs = 10;
    let total_collateral = pairs * 2 * params.collateral_per_token;

    let utxos = vec![
        ElementsUtxo::from(confidential_rt_txout(
            &params.yes_reissuance_token,
            &blinding.yes.input_abf,
            &blinding.yes.input_vbf,
            &contract.script_pubkey(MarketSlot::DormantYesRt),
        )),
        ElementsUtxo::from(confidential_rt_txout(
            &params.no_reissuance_token,
            &blinding.no.input_abf,
            &blinding.no.input_vbf,
            &contract.script_pubkey(MarketSlot::DormantNoRt),
        )),
        ElementsUtxo::from(explicit_txout(
            &params.collateral_asset_id,
            total_collateral,
            &Script::new(),
        )),
        ElementsUtxo::from(explicit_txout(
            &params.collateral_asset_id,
            1_000,
            &Script::new(),
        )),
    ];

    let tx = Arc::new(Transaction {
        version: 2,
        lock_time: LockTime::ZERO,
        input: vec![
            issuance_txin(OutPoint::new(Txid::all_zeros(), 0), pairs),
            issuance_txin(OutPoint::new(Txid::all_zeros(), 1), pairs),
            simple_txin(OutPoint::new(Txid::all_zeros(), 2)),
            simple_txin(OutPoint::new(Txid::all_zeros(), 3)),
        ],
        output: vec![
            confidential_rt_txout(
                &params.yes_reissuance_token,
                &blinding.yes.output_abf,
                &blinding.yes.output_vbf,
                &contract.script_pubkey(MarketSlot::UnresolvedYesRt),
            ),
            confidential_rt_txout(
                &params.no_reissuance_token,
                &blinding.no.output_abf,
                &blinding.no.output_vbf,
                &contract.script_pubkey(MarketSlot::UnresolvedNoRt),
            ),
            explicit_txout(
                &params.collateral_asset_id,
                total_collateral,
                &contract.script_pubkey(MarketSlot::UnresolvedCollateral),
            ),
            explicit_txout(&params.yes_token_asset, pairs, &Script::new()),
            explicit_txout(&params.no_token_asset, pairs, &Script::new()),
            fee_output(&params.collateral_asset_id, 1_000),
        ],
    });

    let result = execute_against_env(
        &contract,
        MarketSlot::DormantNoRt,
        &PredictionMarketSpendingPath::InitialIssuanceSecondaryNoRt { blinding },
        tx,
        utxos,
        1,
    );

    assert!(result.is_ok(), "{result:?}");
}

#[test]
fn expire_transition_primary_executes_with_rt_burns() {
    let (contract, case) = build_expire_transition_case();
    assert_case_input_executes(&contract, case, 0);
}

#[test]
fn subsequent_issuance_primary_executes() {
    let (contract, case) = build_subsequent_issuance_case();
    assert_case_input_executes(&contract, case, 0);
}

#[test]
fn subsequent_issuance_secondary_no_rt_executes() {
    let (contract, case) = build_subsequent_issuance_case();
    assert_case_input_executes(&contract, case, 1);
}

#[test]
fn subsequent_issuance_secondary_collateral_executes() {
    let (contract, case) = build_subsequent_issuance_case();
    assert_case_input_executes(&contract, case, 2);
}

#[test]
fn oracle_resolve_primary_yes_executes() {
    let (contract, case) = build_oracle_resolve_case(true);
    assert_case_input_executes(&contract, case, 0);
}

#[test]
fn oracle_resolve_secondary_no_rt_yes_executes() {
    let (contract, case) = build_oracle_resolve_case(true);
    assert_case_input_executes(&contract, case, 1);
}

#[test]
fn oracle_resolve_secondary_collateral_yes_executes() {
    let (contract, case) = build_oracle_resolve_case(true);
    assert_case_input_executes(&contract, case, 2);
}

#[test]
fn oracle_resolve_primary_yes_executes_without_change() {
    let (contract, case) = build_oracle_resolve_no_change_case(true);
    assert_case_input_executes(&contract, case, 0);
}

#[test]
fn oracle_resolve_secondary_no_rt_yes_executes_without_change() {
    let (contract, case) = build_oracle_resolve_no_change_case(true);
    assert_case_input_executes(&contract, case, 1);
}

#[test]
fn oracle_resolve_secondary_collateral_yes_executes_without_change() {
    let (contract, case) = build_oracle_resolve_no_change_case(true);
    assert_case_input_executes(&contract, case, 2);
}

#[test]
fn oracle_resolve_primary_no_executes() {
    let (contract, case) = build_oracle_resolve_case(false);
    assert_case_input_executes(&contract, case, 0);
}

#[test]
fn oracle_resolve_secondary_no_rt_no_executes() {
    let (contract, case) = build_oracle_resolve_case(false);
    assert_case_input_executes(&contract, case, 1);
}

#[test]
fn oracle_resolve_secondary_collateral_no_executes() {
    let (contract, case) = build_oracle_resolve_case(false);
    assert_case_input_executes(&contract, case, 2);
}

#[test]
fn expire_transition_secondary_no_rt_executes() {
    let (contract, case) = build_expire_transition_case();
    assert_case_input_executes(&contract, case, 1);
}

#[test]
fn expire_transition_secondary_collateral_executes() {
    let (contract, case) = build_expire_transition_case();
    assert_case_input_executes(&contract, case, 2);
}

#[test]
fn expire_transition_primary_executes_without_change() {
    let (contract, case) = build_expire_transition_no_change_case();
    assert_case_input_executes(&contract, case, 0);
}

#[test]
fn expire_transition_secondary_no_rt_executes_without_change() {
    let (contract, case) = build_expire_transition_no_change_case();
    assert_case_input_executes(&contract, case, 1);
}

#[test]
fn expire_transition_secondary_collateral_executes_without_change() {
    let (contract, case) = build_expire_transition_no_change_case();
    assert_case_input_executes(&contract, case, 2);
}

#[test]
fn post_resolution_redemption_resolved_yes_executes() {
    let (contract, case) = build_post_resolution_redemption_case(true);
    assert_case_input_executes(&contract, case, 0);
}

#[test]
fn post_resolution_redemption_resolved_no_executes() {
    let (contract, case) = build_post_resolution_redemption_case(false);
    assert_case_input_executes(&contract, case, 0);
}

#[test]
fn expiry_redemption_yes_executes() {
    let (contract, case) = build_expiry_redemption_case(true);
    assert_case_input_executes(&contract, case, 0);
}

#[test]
fn expiry_redemption_no_executes() {
    let (contract, case) = build_expiry_redemption_case(false);
    assert_case_input_executes(&contract, case, 0);
}

#[test]
fn cancellation_partial_executes() {
    let (contract, case) = build_partial_cancellation_case();
    assert_case_input_executes(&contract, case, 0);
}

#[test]
fn cancellation_full_primary_executes() {
    let (contract, case) = build_full_cancellation_case();
    assert_case_input_executes(&contract, case, 0);
}

#[test]
fn cancellation_full_secondary_yes_rt_executes() {
    let (contract, case) = build_full_cancellation_case();
    assert_case_input_executes(&contract, case, 1);
}

#[test]
fn cancellation_full_secondary_no_rt_executes() {
    let (contract, case) = build_full_cancellation_case();
    assert_case_input_executes(&contract, case, 2);
}

#[test]
fn post_resolution_redemption_rejects_unresolved_rt_slot() {
    let params = test_contract_params();
    let contract = CompiledPredictionMarket::new(params).expect("compile");

    let utxos = vec![ElementsUtxo::from(explicit_txout(
        &params.collateral_asset_id,
        2_000_000,
        &contract.script_pubkey(MarketSlot::UnresolvedYesRt),
    ))];

    let tx = Arc::new(Transaction {
        version: 2,
        lock_time: LockTime::ZERO,
        input: vec![simple_txin(OutPoint::new(Txid::all_zeros(), 0))],
        output: vec![fee_output(&params.collateral_asset_id, 1_000)],
    });

    let result = execute_against_env(
        &contract,
        MarketSlot::UnresolvedYesRt,
        &PredictionMarketSpendingPath::PostResolutionRedemption { tokens_burned: 1 },
        tx,
        utxos,
        0,
    );

    assert!(result.is_err());
}

#[test]
fn post_resolution_redemption_rejects_asset_issuance_on_collateral() {
    let params = test_contract_params();
    let contract = CompiledPredictionMarket::new(params).expect("compile");
    let tokens_burned = 1;
    let payout = tokens_burned * 2 * params.collateral_per_token;
    let collateral = 2_000_000;

    let utxos = vec![ElementsUtxo::from(explicit_txout(
        &params.collateral_asset_id,
        collateral,
        &contract.script_pubkey(MarketSlot::ResolvedYesCollateral),
    ))];

    let tx = Arc::new(Transaction {
        version: 2,
        lock_time: LockTime::ZERO,
        input: vec![issuance_txin(
            OutPoint::new(Txid::all_zeros(), 0),
            tokens_burned,
        )],
        output: vec![
            explicit_txout(
                &params.collateral_asset_id,
                collateral - payout,
                &contract.script_pubkey(MarketSlot::ResolvedYesCollateral),
            ),
            explicit_txout(&params.yes_token_asset, tokens_burned, &Script::new()),
            fee_output(&params.collateral_asset_id, 1_000),
        ],
    });

    let result = execute_against_env(
        &contract,
        MarketSlot::ResolvedYesCollateral,
        &PredictionMarketSpendingPath::PostResolutionRedemption { tokens_burned },
        tx,
        utxos,
        0,
    );

    assert!(result.is_err());
}

#[test]
fn post_resolution_redemption_rejects_inflation_keys_on_collateral() {
    let params = test_contract_params();
    let contract = CompiledPredictionMarket::new(params).expect("compile");
    let tokens_burned = 1;
    let payout = tokens_burned * 2 * params.collateral_per_token;
    let collateral = 2_000_000;

    let utxos = vec![ElementsUtxo::from(explicit_txout(
        &params.collateral_asset_id,
        collateral,
        &contract.script_pubkey(MarketSlot::ResolvedYesCollateral),
    ))];

    let tx = Arc::new(Transaction {
        version: 2,
        lock_time: LockTime::ZERO,
        input: vec![inflation_keys_txin(
            OutPoint::new(Txid::all_zeros(), 0),
            tokens_burned,
        )],
        output: vec![
            explicit_txout(
                &params.collateral_asset_id,
                collateral - payout,
                &contract.script_pubkey(MarketSlot::ResolvedYesCollateral),
            ),
            explicit_txout(&params.yes_token_asset, tokens_burned, &Script::new()),
            fee_output(&params.collateral_asset_id, 1_000),
        ],
    });

    let result = execute_against_env(
        &contract,
        MarketSlot::ResolvedYesCollateral,
        &PredictionMarketSpendingPath::PostResolutionRedemption { tokens_burned },
        tx,
        utxos,
        0,
    );

    assert!(result.is_err());
}

#[test]
fn cancellation_partial_rejects_inflation_keys_on_collateral() {
    let params = test_contract_params();
    let contract = CompiledPredictionMarket::new(params).expect("compile");
    let pairs_burned = 1;
    let refund = pairs_burned * 2 * params.collateral_per_token;
    let collateral = 2_000_000;

    let utxos = vec![ElementsUtxo::from(explicit_txout(
        &params.collateral_asset_id,
        collateral,
        &contract.script_pubkey(MarketSlot::UnresolvedCollateral),
    ))];

    let tx = Arc::new(Transaction {
        version: 2,
        lock_time: LockTime::ZERO,
        input: vec![inflation_keys_txin(
            OutPoint::new(Txid::all_zeros(), 0),
            pairs_burned,
        )],
        output: vec![
            explicit_txout(
                &params.collateral_asset_id,
                collateral - refund,
                &contract.script_pubkey(MarketSlot::UnresolvedCollateral),
            ),
            explicit_txout(&params.yes_token_asset, pairs_burned, &Script::new()),
            explicit_txout(&params.no_token_asset, pairs_burned, &Script::new()),
            fee_output(&params.collateral_asset_id, 1_000),
        ],
    });

    let result = execute_against_env(
        &contract,
        MarketSlot::UnresolvedCollateral,
        &PredictionMarketSpendingPath::CancellationPartial { pairs_burned },
        tx,
        utxos,
        0,
    );

    assert!(result.is_err());
}

#[test]
fn expire_transition_secondary_collateral_rejects_inflation_keys() {
    let params = test_contract_params();
    let contract = CompiledPredictionMarket::new(params).expect("compile");
    let blinding = test_blinding();

    let utxos = vec![
        ElementsUtxo::from(confidential_rt_txout(
            &params.yes_reissuance_token,
            &blinding.yes.input_abf,
            &blinding.yes.input_vbf,
            &contract.script_pubkey(MarketSlot::UnresolvedYesRt),
        )),
        ElementsUtxo::from(confidential_rt_txout(
            &params.no_reissuance_token,
            &blinding.no.input_abf,
            &blinding.no.input_vbf,
            &contract.script_pubkey(MarketSlot::UnresolvedNoRt),
        )),
        ElementsUtxo::from(explicit_txout(
            &params.collateral_asset_id,
            2_000_000,
            &contract.script_pubkey(MarketSlot::UnresolvedCollateral),
        )),
        ElementsUtxo::from(explicit_txout(
            &params.collateral_asset_id,
            1_000,
            &Script::new(),
        )),
    ];

    let tx = Arc::new(Transaction {
        version: 2,
        lock_time: LockTime::from_consensus(params.expiry_time),
        input: vec![
            simple_txin(OutPoint::new(Txid::all_zeros(), 0)),
            simple_txin(OutPoint::new(Txid::all_zeros(), 1)),
            inflation_keys_txin(OutPoint::new(Txid::all_zeros(), 2), 1),
            simple_txin(OutPoint::new(Txid::all_zeros(), 3)),
        ],
        output: vec![
            explicit_txout(&params.yes_reissuance_token, 1, &Script::new()),
            explicit_txout(&params.no_reissuance_token, 1, &Script::new()),
            explicit_txout(
                &params.collateral_asset_id,
                2_000_000,
                &contract.script_pubkey(MarketSlot::ExpiredCollateral),
            ),
            fee_output(&params.collateral_asset_id, 1_000),
        ],
    });

    let result = execute_against_env(
        &contract,
        MarketSlot::UnresolvedCollateral,
        &PredictionMarketSpendingPath::ExpireTransitionSecondaryCollateral,
        tx,
        utxos,
        2,
    );

    assert!(result.is_err());
}

#[test]
fn subsequent_issuance_secondary_collateral_rejects_issuance_metadata() {
    let params = test_contract_params();
    let contract = CompiledPredictionMarket::new(params).expect("compile");
    let blinding = test_blinding();
    let pairs = 10;
    let old_collateral = 2_000_000;
    let new_collateral = pairs * 2 * params.collateral_per_token;

    let utxos = vec![
        ElementsUtxo::from(confidential_rt_txout(
            &params.yes_reissuance_token,
            &blinding.yes.input_abf,
            &blinding.yes.input_vbf,
            &contract.script_pubkey(MarketSlot::UnresolvedYesRt),
        )),
        ElementsUtxo::from(confidential_rt_txout(
            &params.no_reissuance_token,
            &blinding.no.input_abf,
            &blinding.no.input_vbf,
            &contract.script_pubkey(MarketSlot::UnresolvedNoRt),
        )),
        ElementsUtxo::from(explicit_txout(
            &params.collateral_asset_id,
            old_collateral,
            &contract.script_pubkey(MarketSlot::UnresolvedCollateral),
        )),
    ];

    let tx = Arc::new(Transaction {
        version: 2,
        lock_time: LockTime::ZERO,
        input: vec![
            issuance_txin(OutPoint::new(Txid::all_zeros(), 0), pairs),
            issuance_txin(OutPoint::new(Txid::all_zeros(), 1), pairs),
            issuance_txin(OutPoint::new(Txid::all_zeros(), 2), pairs),
        ],
        output: vec![
            confidential_rt_txout(
                &params.yes_reissuance_token,
                &blinding.yes.output_abf,
                &blinding.yes.output_vbf,
                &contract.script_pubkey(MarketSlot::UnresolvedYesRt),
            ),
            confidential_rt_txout(
                &params.no_reissuance_token,
                &blinding.no.output_abf,
                &blinding.no.output_vbf,
                &contract.script_pubkey(MarketSlot::UnresolvedNoRt),
            ),
            explicit_txout(
                &params.collateral_asset_id,
                old_collateral + new_collateral,
                &contract.script_pubkey(MarketSlot::UnresolvedCollateral),
            ),
            explicit_txout(&params.yes_token_asset, pairs, &Script::new()),
            explicit_txout(&params.no_token_asset, pairs, &Script::new()),
            fee_output(&params.collateral_asset_id, 1_000),
        ],
    });

    let result = execute_against_env(
        &contract,
        MarketSlot::UnresolvedCollateral,
        &PredictionMarketSpendingPath::SubsequentIssuanceSecondaryCollateral,
        tx,
        utxos,
        2,
    );

    assert!(result.is_err());
}

#[test]
fn full_cancel_secondary_no_rt_rejects_issuance() {
    let params = test_contract_params();
    let contract = CompiledPredictionMarket::new(params).expect("compile");
    let blinding: AllBlindingFactors = test_blinding();
    let pairs = 10;
    let refund = pairs * 2 * params.collateral_per_token;

    let utxos = vec![
        ElementsUtxo::from(explicit_txout(
            &params.collateral_asset_id,
            refund,
            &contract.script_pubkey(MarketSlot::UnresolvedCollateral),
        )),
        ElementsUtxo::from(confidential_rt_txout(
            &params.yes_reissuance_token,
            &blinding.yes.input_abf,
            &blinding.yes.input_vbf,
            &contract.script_pubkey(MarketSlot::UnresolvedYesRt),
        )),
        ElementsUtxo::from(confidential_rt_txout(
            &params.no_reissuance_token,
            &blinding.no.input_abf,
            &blinding.no.input_vbf,
            &contract.script_pubkey(MarketSlot::UnresolvedNoRt),
        )),
    ];

    let tx = Arc::new(Transaction {
        version: 2,
        lock_time: LockTime::ZERO,
        input: vec![
            simple_txin(OutPoint::new(Txid::all_zeros(), 0)),
            simple_txin(OutPoint::new(Txid::all_zeros(), 1)),
            issuance_txin(OutPoint::new(Txid::all_zeros(), 2), pairs),
        ],
        output: vec![
            confidential_rt_txout(
                &params.yes_reissuance_token,
                &blinding.yes.output_abf,
                &blinding.yes.output_vbf,
                &contract.script_pubkey(MarketSlot::DormantYesRt),
            ),
            confidential_rt_txout(
                &params.no_reissuance_token,
                &blinding.no.output_abf,
                &blinding.no.output_vbf,
                &contract.script_pubkey(MarketSlot::DormantNoRt),
            ),
            explicit_txout(&params.yes_token_asset, pairs, &Script::new()),
            explicit_txout(&params.no_token_asset, pairs, &Script::new()),
            explicit_txout(&params.collateral_asset_id, refund, &Script::new()),
            fee_output(&params.collateral_asset_id, 1_000),
        ],
    });

    let result = execute_against_env(
        &contract,
        MarketSlot::UnresolvedNoRt,
        &PredictionMarketSpendingPath::CancellationFullSecondaryNoRt { blinding },
        tx,
        utxos,
        2,
    );

    assert!(result.is_err());
}
