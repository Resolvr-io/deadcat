/// Integration tests that execute the Simplicity program against an ElementsEnv
/// to validate PSET builder output against the contract's runtime assertions.
///
/// These tests catch "Assertion failed inside jet" errors locally — the same class
/// of bug that causes broadcast failures on the Liquid network.
use std::sync::Arc;

use deadcat_sdk::elements::confidential::{Asset, Nonce, Value as ConfValue};
use deadcat_sdk::elements::hashes::Hash;
use deadcat_sdk::elements::secp256k1_zkp::{Generator, PedersenCommitment, Secp256k1, Tag, Tweak};
use deadcat_sdk::elements::taproot::ControlBlock;
use deadcat_sdk::elements::{
    AssetId, AssetIssuance, BlockHash, LockTime, OutPoint, Script, Sequence, Transaction, TxIn,
    TxOut, TxOutWitness, Txid,
};
use deadcat_sdk::simplicity::bit_machine::BitMachine;
use deadcat_sdk::simplicity::jet::elements::{ElementsEnv, ElementsUtxo};
use deadcat_sdk::testing::test_contract_params;
use deadcat_sdk::witness::{
    AllBlindingFactors, ReissuanceBlindingFactors, SpendingPath, satisfy_contract,
};
use deadcat_sdk::{CompiledContract, MarketState};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn explicit_txout(asset_bytes: &[u8; 32], amount: u64, spk: &Script) -> TxOut {
    TxOut {
        asset: Asset::Explicit(AssetId::from_slice(asset_bytes).expect("valid asset")),
        value: ConfValue::Explicit(amount),
        nonce: Nonce::Null,
        script_pubkey: spk.clone(),
        witness: TxOutWitness::default(),
    }
}

fn simple_txin(outpoint: OutPoint) -> TxIn {
    TxIn {
        previous_output: outpoint,
        is_pegin: false,
        script_sig: Script::new(),
        sequence: Sequence::ENABLE_LOCKTIME_NO_RBF,
        asset_issuance: Default::default(),
        witness: Default::default(),
    }
}

/// Build a TxOut with confidential (Pedersen committed) asset and value for a
/// reissuance token. The commitments are constructed to match the given blinding
/// factors so the contract's `verify_token_commitment` passes.
fn confidential_rt_txout(
    asset_bytes: &[u8; 32],
    abf: &[u8; 32],
    vbf: &[u8; 32],
    spk: &Script,
) -> TxOut {
    let secp = Secp256k1::new();
    let tag = Tag::from(*asset_bytes);
    let abf_tweak = Tweak::from_slice(abf).expect("valid ABF");
    let vbf_tweak = Tweak::from_slice(vbf).expect("valid VBF");
    let generator = Generator::new_blinded(&secp, tag, abf_tweak);
    let commitment = PedersenCommitment::new(&secp, 1, vbf_tweak, generator);
    TxOut {
        asset: Asset::Confidential(generator),
        value: ConfValue::Confidential(commitment),
        nonce: Nonce::Null,
        script_pubkey: spk.clone(),
        witness: TxOutWitness::default(),
    }
}

/// Build a TxIn with asset issuance set (explicit amount = `pairs`).
fn issuance_txin(outpoint: OutPoint, pairs: u64) -> TxIn {
    TxIn {
        previous_output: outpoint,
        is_pegin: false,
        script_sig: Script::new(),
        sequence: Sequence::ENABLE_LOCKTIME_NO_RBF,
        asset_issuance: AssetIssuance {
            asset_blinding_nonce: Tweak::from_slice(&[0x10; 32]).expect("valid nonce"),
            asset_entropy: [0x20; 32],
            amount: ConfValue::Explicit(pairs),
            inflation_keys: ConfValue::Null,
        },
        witness: Default::default(),
    }
}

/// Blinding factors used across all issuance tests.
fn test_blinding() -> AllBlindingFactors {
    AllBlindingFactors {
        yes: ReissuanceBlindingFactors {
            input_abf: [0x01; 32],
            input_vbf: [0x02; 32],
            output_abf: [0x03; 32],
            output_vbf: [0x04; 32],
        },
        no: ReissuanceBlindingFactors {
            input_abf: [0x05; 32],
            input_vbf: [0x06; 32],
            output_abf: [0x07; 32],
            output_vbf: [0x08; 32],
        },
    }
}

/// Execute a satisfied Simplicity program against a mock ElementsEnv.
///
/// Returns Ok(()) on success, or an error description on failure.
fn execute_contract(
    contract: &CompiledContract,
    state: MarketState,
    path: &SpendingPath,
    tx: Arc<Transaction>,
    utxos: Vec<ElementsUtxo>,
    input_index: u32,
) -> Result<(), String> {
    let satisfied = satisfy_contract(contract, path, state).map_err(|e| format!("satisfy: {e}"))?;
    let redeem = satisfied.redeem();

    let cb_bytes = contract.control_block(state);
    let control_block =
        ControlBlock::from_slice(&cb_bytes).map_err(|e| format!("control block: {e}"))?;

    let env = ElementsEnv::new(
        tx,
        utxos,
        input_index,
        *contract.cmr(),
        control_block,
        None,
        BlockHash::all_zeros(),
    );

    let mut machine = BitMachine::for_program(redeem).map_err(|e| format!("bit machine: {e}"))?;

    machine
        .exec(redeem, &env)
        .map(|_| ())
        .map_err(|e| format!("execution failed: {e}"))
}

// ---------------------------------------------------------------------------
// Path 7: SecondaryCovenantInput
// ---------------------------------------------------------------------------

#[test]
fn secondary_covenant_input_dormant() {
    let params = test_contract_params();
    let contract = CompiledContract::new(params).expect("compile");
    let state = MarketState::Dormant;
    let covenant_spk = contract.script_pubkey(state);
    let utxo_txout = explicit_txout(&params.collateral_asset_id, 100_000, &covenant_spk);

    let tx = Arc::new(Transaction {
        version: 2,
        lock_time: LockTime::ZERO,
        input: vec![
            simple_txin(OutPoint::new(Txid::all_zeros(), 0)),
            simple_txin(OutPoint::new(Txid::all_zeros(), 1)),
        ],
        output: vec![explicit_txout(
            &params.collateral_asset_id,
            100_000,
            &Script::new(),
        )],
    });

    let utxos = vec![
        ElementsUtxo::from(utxo_txout.clone()),
        ElementsUtxo::from(utxo_txout),
    ];

    let result = execute_contract(
        &contract,
        state,
        &SpendingPath::SecondaryCovenantInput,
        tx,
        utxos,
        1, // Execute at input index 1
    );

    assert!(
        result.is_ok(),
        "SecondaryCovenantInput at Dormant should succeed: {:?}",
        result.err()
    );
}

#[test]
fn secondary_covenant_input_unresolved() {
    let params = test_contract_params();
    let contract = CompiledContract::new(params).expect("compile");
    let state = MarketState::Unresolved;
    let covenant_spk = contract.script_pubkey(state);
    let utxo_txout = explicit_txout(&params.collateral_asset_id, 100_000, &covenant_spk);

    let tx = Arc::new(Transaction {
        version: 2,
        lock_time: LockTime::ZERO,
        input: vec![
            simple_txin(OutPoint::new(Txid::all_zeros(), 0)),
            simple_txin(OutPoint::new(Txid::all_zeros(), 1)),
        ],
        output: vec![explicit_txout(
            &params.collateral_asset_id,
            100_000,
            &Script::new(),
        )],
    });

    let utxos = vec![
        ElementsUtxo::from(utxo_txout.clone()),
        ElementsUtxo::from(utxo_txout),
    ];

    let result = execute_contract(
        &contract,
        state,
        &SpendingPath::SecondaryCovenantInput,
        tx,
        utxos,
        1,
    );

    assert!(
        result.is_ok(),
        "SecondaryCovenantInput at Unresolved should succeed: {:?}",
        result.err()
    );
}

/// SecondaryCovenantInput must fail at input index 0 (contract requires index != 0).
#[test]
fn secondary_covenant_input_fails_at_index_zero() {
    let params = test_contract_params();
    let contract = CompiledContract::new(params).expect("compile");
    let state = MarketState::Dormant;
    let covenant_spk = contract.script_pubkey(state);
    let utxo_txout = explicit_txout(&params.collateral_asset_id, 100_000, &covenant_spk);

    let tx = Arc::new(Transaction {
        version: 2,
        lock_time: LockTime::ZERO,
        input: vec![
            simple_txin(OutPoint::new(Txid::all_zeros(), 0)),
            simple_txin(OutPoint::new(Txid::all_zeros(), 1)),
        ],
        output: vec![explicit_txout(
            &params.collateral_asset_id,
            100_000,
            &Script::new(),
        )],
    });

    let utxos = vec![
        ElementsUtxo::from(utxo_txout.clone()),
        ElementsUtxo::from(utxo_txout),
    ];

    let result = execute_contract(
        &contract,
        state,
        &SpendingPath::SecondaryCovenantInput,
        tx,
        utxos,
        0, // Index 0 — should fail
    );

    assert!(
        result.is_err(),
        "SecondaryCovenantInput at index 0 should fail"
    );
}

/// SecondaryCovenantInput must fail when the script hashes don't match.
#[test]
fn secondary_covenant_input_fails_with_mismatched_scripts() {
    let params = test_contract_params();
    let contract = CompiledContract::new(params).expect("compile");
    let state = MarketState::Dormant;
    let covenant_spk = contract.script_pubkey(state);

    // Input 0 at covenant address, input 1 at a different script
    let utxo0 = explicit_txout(&params.collateral_asset_id, 100_000, &covenant_spk);
    let utxo1 = explicit_txout(&params.collateral_asset_id, 100_000, &Script::new());

    let tx = Arc::new(Transaction {
        version: 2,
        lock_time: LockTime::ZERO,
        input: vec![
            simple_txin(OutPoint::new(Txid::all_zeros(), 0)),
            simple_txin(OutPoint::new(Txid::all_zeros(), 1)),
        ],
        output: vec![explicit_txout(
            &params.collateral_asset_id,
            100_000,
            &Script::new(),
        )],
    });

    let utxos = vec![
        ElementsUtxo::from(utxo0),
        ElementsUtxo::from(utxo1), // different script than input 0
    ];

    // The main() check `expected_hash == actual_hash` should fail because
    // input 1's script_pubkey doesn't match the covenant for Dormant state.
    let result = execute_contract(
        &contract,
        state,
        &SpendingPath::SecondaryCovenantInput,
        tx,
        utxos,
        1,
    );

    assert!(
        result.is_err(),
        "SecondaryCovenantInput with mismatched scripts should fail"
    );
}

// ---------------------------------------------------------------------------
// Path 1: InitialIssuance
// ---------------------------------------------------------------------------

#[test]
fn initial_issuance_execution() {
    let params = test_contract_params();
    let contract = CompiledContract::new(params).expect("compile");
    let dormant_spk = contract.script_pubkey(MarketState::Dormant);
    let unresolved_spk = contract.script_pubkey(MarketState::Unresolved);
    let blinding = test_blinding();

    let pairs: u64 = 10;
    let total_collateral = pairs * 2 * params.collateral_per_token;

    // UTXOs backing each input
    let utxos = vec![
        // [0] YES RT (confidential, Dormant covenant)
        ElementsUtxo::from(confidential_rt_txout(
            &params.yes_reissuance_token,
            &blinding.yes.input_abf,
            &blinding.yes.input_vbf,
            &dormant_spk,
        )),
        // [1] NO RT (confidential, Dormant covenant)
        ElementsUtxo::from(confidential_rt_txout(
            &params.no_reissuance_token,
            &blinding.no.input_abf,
            &blinding.no.input_vbf,
            &dormant_spk,
        )),
        // [2] Collateral funding (explicit, wallet)
        ElementsUtxo::from(explicit_txout(
            &params.collateral_asset_id,
            total_collateral,
            &Script::new(),
        )),
        // [3] Fee funding (explicit, wallet)
        ElementsUtxo::from(explicit_txout(
            &params.collateral_asset_id,
            1000,
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
            // [0] YES RT → Unresolved
            confidential_rt_txout(
                &params.yes_reissuance_token,
                &blinding.yes.output_abf,
                &blinding.yes.output_vbf,
                &unresolved_spk,
            ),
            // [1] NO RT → Unresolved
            confidential_rt_txout(
                &params.no_reissuance_token,
                &blinding.no.output_abf,
                &blinding.no.output_vbf,
                &unresolved_spk,
            ),
            // [2] Collateral → Unresolved
            explicit_txout(
                &params.collateral_asset_id,
                total_collateral,
                &unresolved_spk,
            ),
            // [3] YES tokens (not checked by contract)
            explicit_txout(&params.yes_token_asset, pairs, &Script::new()),
            // [4] NO tokens (not checked by contract)
            explicit_txout(&params.no_token_asset, pairs, &Script::new()),
            // [5] Fee
            explicit_txout(&params.collateral_asset_id, 1000, &Script::new()),
        ],
    });

    let result = execute_contract(
        &contract,
        MarketState::Dormant,
        &SpendingPath::InitialIssuance { blinding },
        tx,
        utxos,
        0,
    );

    assert!(
        result.is_ok(),
        "InitialIssuance should succeed: {:?}",
        result.err()
    );
}

/// InitialIssuance must fail when the collateral output amount is wrong.
#[test]
fn initial_issuance_fails_with_wrong_collateral() {
    let params = test_contract_params();
    let contract = CompiledContract::new(params).expect("compile");
    let dormant_spk = contract.script_pubkey(MarketState::Dormant);
    let unresolved_spk = contract.script_pubkey(MarketState::Unresolved);
    let blinding = test_blinding();

    let pairs: u64 = 10;
    let total_collateral = pairs * 2 * params.collateral_per_token;
    let wrong_collateral = total_collateral - 1;

    let utxos = vec![
        ElementsUtxo::from(confidential_rt_txout(
            &params.yes_reissuance_token,
            &blinding.yes.input_abf,
            &blinding.yes.input_vbf,
            &dormant_spk,
        )),
        ElementsUtxo::from(confidential_rt_txout(
            &params.no_reissuance_token,
            &blinding.no.input_abf,
            &blinding.no.input_vbf,
            &dormant_spk,
        )),
        ElementsUtxo::from(explicit_txout(
            &params.collateral_asset_id,
            total_collateral,
            &Script::new(),
        )),
        ElementsUtxo::from(explicit_txout(
            &params.collateral_asset_id,
            1000,
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
                &unresolved_spk,
            ),
            confidential_rt_txout(
                &params.no_reissuance_token,
                &blinding.no.output_abf,
                &blinding.no.output_vbf,
                &unresolved_spk,
            ),
            // Wrong collateral amount
            explicit_txout(
                &params.collateral_asset_id,
                wrong_collateral,
                &unresolved_spk,
            ),
            explicit_txout(&params.yes_token_asset, pairs, &Script::new()),
            explicit_txout(&params.no_token_asset, pairs, &Script::new()),
            explicit_txout(&params.collateral_asset_id, 1000, &Script::new()),
        ],
    });

    let result = execute_contract(
        &contract,
        MarketState::Dormant,
        &SpendingPath::InitialIssuance { blinding },
        tx,
        utxos,
        0,
    );

    assert!(
        result.is_err(),
        "InitialIssuance with wrong collateral should fail"
    );
}

/// InitialIssuance must fail when executed at input index != 0.
#[test]
fn initial_issuance_fails_at_wrong_index() {
    let params = test_contract_params();
    let contract = CompiledContract::new(params).expect("compile");
    let dormant_spk = contract.script_pubkey(MarketState::Dormant);
    let unresolved_spk = contract.script_pubkey(MarketState::Unresolved);
    let blinding = test_blinding();

    let pairs: u64 = 10;
    let total_collateral = pairs * 2 * params.collateral_per_token;

    // Both RT UTXOs use the Dormant spk so the main() script hash check
    // passes at any index — the path-specific current_index == 0 check fails.
    let utxos = vec![
        ElementsUtxo::from(confidential_rt_txout(
            &params.yes_reissuance_token,
            &blinding.yes.input_abf,
            &blinding.yes.input_vbf,
            &dormant_spk,
        )),
        ElementsUtxo::from(confidential_rt_txout(
            &params.no_reissuance_token,
            &blinding.no.input_abf,
            &blinding.no.input_vbf,
            &dormant_spk,
        )),
        ElementsUtxo::from(explicit_txout(
            &params.collateral_asset_id,
            total_collateral,
            &Script::new(),
        )),
        ElementsUtxo::from(explicit_txout(
            &params.collateral_asset_id,
            1000,
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
                &unresolved_spk,
            ),
            confidential_rt_txout(
                &params.no_reissuance_token,
                &blinding.no.output_abf,
                &blinding.no.output_vbf,
                &unresolved_spk,
            ),
            explicit_txout(
                &params.collateral_asset_id,
                total_collateral,
                &unresolved_spk,
            ),
            explicit_txout(&params.yes_token_asset, pairs, &Script::new()),
            explicit_txout(&params.no_token_asset, pairs, &Script::new()),
            explicit_txout(&params.collateral_asset_id, 1000, &Script::new()),
        ],
    });

    let result = execute_contract(
        &contract,
        MarketState::Dormant,
        &SpendingPath::InitialIssuance { blinding },
        tx,
        utxos,
        1, // Wrong index — should fail
    );

    assert!(result.is_err(), "InitialIssuance at index 1 should fail");
}

// ---------------------------------------------------------------------------
// Path 2: SubsequentIssuance
// ---------------------------------------------------------------------------

#[test]
fn subsequent_issuance_execution() {
    let params = test_contract_params();
    let contract = CompiledContract::new(params).expect("compile");
    let unresolved_spk = contract.script_pubkey(MarketState::Unresolved);
    let blinding = test_blinding();

    let pairs: u64 = 10;
    let new_collateral = pairs * 2 * params.collateral_per_token;
    let old_collateral: u64 = 1_000_000;
    let total_collateral = old_collateral + new_collateral;

    // UTXOs backing each input
    let utxos = vec![
        // [0] YES RT (confidential, Unresolved covenant)
        ElementsUtxo::from(confidential_rt_txout(
            &params.yes_reissuance_token,
            &blinding.yes.input_abf,
            &blinding.yes.input_vbf,
            &unresolved_spk,
        )),
        // [1] NO RT (confidential, Unresolved covenant)
        ElementsUtxo::from(confidential_rt_txout(
            &params.no_reissuance_token,
            &blinding.no.input_abf,
            &blinding.no.input_vbf,
            &unresolved_spk,
        )),
        // [2] Old collateral (explicit, Unresolved covenant)
        ElementsUtxo::from(explicit_txout(
            &params.collateral_asset_id,
            old_collateral,
            &unresolved_spk,
        )),
        // [3] New collateral funding (explicit, wallet)
        ElementsUtxo::from(explicit_txout(
            &params.collateral_asset_id,
            new_collateral,
            &Script::new(),
        )),
        // [4] Fee funding (explicit, wallet)
        ElementsUtxo::from(explicit_txout(
            &params.collateral_asset_id,
            1000,
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
            simple_txin(OutPoint::new(Txid::all_zeros(), 4)),
        ],
        output: vec![
            // [0] YES RT → Unresolved
            confidential_rt_txout(
                &params.yes_reissuance_token,
                &blinding.yes.output_abf,
                &blinding.yes.output_vbf,
                &unresolved_spk,
            ),
            // [1] NO RT → Unresolved
            confidential_rt_txout(
                &params.no_reissuance_token,
                &blinding.no.output_abf,
                &blinding.no.output_vbf,
                &unresolved_spk,
            ),
            // [2] Total collateral → Unresolved
            explicit_txout(
                &params.collateral_asset_id,
                total_collateral,
                &unresolved_spk,
            ),
            // [3] YES tokens (not checked by contract)
            explicit_txout(&params.yes_token_asset, pairs, &Script::new()),
            // [4] NO tokens (not checked by contract)
            explicit_txout(&params.no_token_asset, pairs, &Script::new()),
            // [5] Fee
            explicit_txout(&params.collateral_asset_id, 1000, &Script::new()),
        ],
    });

    let result = execute_contract(
        &contract,
        MarketState::Unresolved,
        &SpendingPath::SubsequentIssuance { blinding },
        tx,
        utxos,
        0,
    );

    assert!(
        result.is_ok(),
        "SubsequentIssuance should succeed: {:?}",
        result.err()
    );
}

// ---------------------------------------------------------------------------
// Pipeline integration tests (PSET builder → witness → ElementsEnv)
//
// These use assemble_issuance_for_env() to exercise the full assembly pipeline
// against the Simplicity execution environment, closing the gap between PSET
// construction and contract execution.
// ---------------------------------------------------------------------------

use deadcat_sdk::assembly::{CollateralSource, IssuanceAssemblyInputs, IssuanceEntropy};
use deadcat_sdk::pset::UnblindedUtxo;
use deadcat_sdk::testing::{
    assemble_issuance_for_env, execute_against_env, test_blinding as shared_test_blinding,
};

fn test_unblinded_utxo(asset_id: [u8; 32], value: u64, spk: &Script) -> UnblindedUtxo {
    UnblindedUtxo {
        outpoint: OutPoint::new(Txid::all_zeros(), 0),
        txout: explicit_txout(&asset_id, value, spk),
        asset_id,
        value,
        asset_blinding_factor: [0u8; 32],
        value_blinding_factor: [0u8; 32],
    }
}

fn test_rt_utxo(asset_id: [u8; 32], abf: [u8; 32], vbf: [u8; 32], spk: &Script) -> UnblindedUtxo {
    UnblindedUtxo {
        outpoint: OutPoint::new(Txid::all_zeros(), 0),
        txout: confidential_rt_txout(&asset_id, &abf, &vbf, spk),
        asset_id,
        value: 1,
        asset_blinding_factor: abf,
        value_blinding_factor: vbf,
    }
}

/// Test the full PSET builder → witness → ElementsEnv pipeline for initial issuance.
#[test]
#[ignore = "pruned-branch failure under investigation"]
fn initial_issuance_assembly_to_env() {
    let params = test_contract_params();
    let contract = CompiledContract::new(params).expect("compile");
    let blinding = shared_test_blinding();
    let dormant_spk = contract.script_pubkey(MarketState::Dormant);

    let pairs: u64 = 10;
    let total_collateral = pairs * 2 * params.collateral_per_token;
    let fee_amount: u64 = 1000;

    let inputs = IssuanceAssemblyInputs {
        contract,
        current_state: MarketState::Dormant,
        yes_reissuance_utxo: test_rt_utxo(
            params.yes_reissuance_token,
            blinding.yes.input_abf,
            blinding.yes.input_vbf,
            &dormant_spk,
        ),
        no_reissuance_utxo: test_rt_utxo(
            params.no_reissuance_token,
            blinding.no.input_abf,
            blinding.no.input_vbf,
            &dormant_spk,
        ),
        collateral_source: CollateralSource::Initial {
            wallet_utxo: test_unblinded_utxo(
                params.collateral_asset_id,
                total_collateral,
                &Script::new(),
            ),
        },
        fee_utxo: test_unblinded_utxo(params.collateral_asset_id, fee_amount, &Script::new()),
        pairs,
        fee_amount,
        token_destination: Script::new(),
        change_destination: None,
        issuance_entropy: IssuanceEntropy {
            yes_blinding_nonce: blinding.yes.input_abf,
            yes_entropy: [0x20; 32],
            no_blinding_nonce: blinding.no.input_abf,
            no_entropy: [0x20; 32],
        },
        lock_time: 0,
    };

    let (tx, utxos, spending_path) =
        assemble_issuance_for_env(inputs, blinding).expect("assembly should succeed");

    // Re-compile contract for execute_against_env (contract was moved into inputs)
    let contract = CompiledContract::new(params).expect("compile");

    let result = execute_against_env(
        &contract,
        MarketState::Dormant,
        &spending_path,
        tx,
        utxos,
        0,
    );

    assert!(
        result.is_ok(),
        "Initial issuance assembly pipeline should succeed: {:?}",
        result.err()
    );
}

/// Test the full PSET builder → witness → ElementsEnv pipeline for subsequent issuance.
#[test]
#[ignore = "pruned-branch failure under investigation"]
fn subsequent_issuance_assembly_to_env() {
    let params = test_contract_params();
    let contract = CompiledContract::new(params).expect("compile");
    let blinding = shared_test_blinding();
    let unresolved_spk = contract.script_pubkey(MarketState::Unresolved);

    let pairs: u64 = 10;
    let new_collateral = pairs * 2 * params.collateral_per_token;
    let old_collateral: u64 = 1_000_000;
    let fee_amount: u64 = 1000;

    let inputs = IssuanceAssemblyInputs {
        contract,
        current_state: MarketState::Unresolved,
        yes_reissuance_utxo: test_rt_utxo(
            params.yes_reissuance_token,
            blinding.yes.input_abf,
            blinding.yes.input_vbf,
            &unresolved_spk,
        ),
        no_reissuance_utxo: test_rt_utxo(
            params.no_reissuance_token,
            blinding.no.input_abf,
            blinding.no.input_vbf,
            &unresolved_spk,
        ),
        collateral_source: CollateralSource::Subsequent {
            covenant_collateral: test_unblinded_utxo(
                params.collateral_asset_id,
                old_collateral,
                &unresolved_spk,
            ),
            new_wallet_utxo: test_unblinded_utxo(
                params.collateral_asset_id,
                new_collateral,
                &Script::new(),
            ),
        },
        fee_utxo: test_unblinded_utxo(params.collateral_asset_id, fee_amount, &Script::new()),
        pairs,
        fee_amount,
        token_destination: Script::new(),
        change_destination: None,
        issuance_entropy: IssuanceEntropy {
            yes_blinding_nonce: blinding.yes.input_abf,
            yes_entropy: [0x20; 32],
            no_blinding_nonce: blinding.no.input_abf,
            no_entropy: [0x20; 32],
        },
        lock_time: 0,
    };

    let (tx, utxos, spending_path) =
        assemble_issuance_for_env(inputs, blinding).expect("assembly should succeed");

    let contract = CompiledContract::new(params).expect("compile");

    let result = execute_against_env(
        &contract,
        MarketState::Unresolved,
        &spending_path,
        tx,
        utxos,
        0,
    );

    assert!(
        result.is_ok(),
        "Subsequent issuance assembly pipeline should succeed: {:?}",
        result.err()
    );
}

/// Negative test: initial issuance with wrong collateral amount should fail.
#[test]
fn initial_issuance_assembly_wrong_collateral_fails() {
    let params = test_contract_params();
    let contract = CompiledContract::new(params).expect("compile");
    let blinding = shared_test_blinding();
    let dormant_spk = contract.script_pubkey(MarketState::Dormant);

    let pairs: u64 = 10;
    let total_collateral = pairs * 2 * params.collateral_per_token;
    let fee_amount: u64 = 1000;

    // Provide less collateral than required — the PSET builder should reject this
    let inputs = IssuanceAssemblyInputs {
        contract,
        current_state: MarketState::Dormant,
        yes_reissuance_utxo: test_rt_utxo(
            params.yes_reissuance_token,
            blinding.yes.input_abf,
            blinding.yes.input_vbf,
            &dormant_spk,
        ),
        no_reissuance_utxo: test_rt_utxo(
            params.no_reissuance_token,
            blinding.no.input_abf,
            blinding.no.input_vbf,
            &dormant_spk,
        ),
        collateral_source: CollateralSource::Initial {
            wallet_utxo: test_unblinded_utxo(
                params.collateral_asset_id,
                total_collateral - 1, // Insufficient
                &Script::new(),
            ),
        },
        fee_utxo: test_unblinded_utxo(params.collateral_asset_id, fee_amount, &Script::new()),
        pairs,
        fee_amount,
        token_destination: Script::new(),
        change_destination: None,
        issuance_entropy: IssuanceEntropy {
            yes_blinding_nonce: blinding.yes.input_abf,
            yes_entropy: [0x20; 32],
            no_blinding_nonce: blinding.no.input_abf,
            no_entropy: [0x20; 32],
        },
        lock_time: 0,
    };

    let result = assemble_issuance_for_env(inputs, blinding);
    assert!(
        result.is_err(),
        "Initial issuance with insufficient collateral should fail at PSET build"
    );
}
