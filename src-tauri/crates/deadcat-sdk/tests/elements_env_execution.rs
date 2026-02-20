/// Integration tests that execute the Simplicity program against an ElementsEnv
/// to validate PSET builder output against the contract's runtime assertions.
///
/// These tests catch "Assertion failed inside jet" errors locally — the same class
/// of bug that causes broadcast failures on the Liquid network.
use std::sync::Arc;

use deadcat_sdk::elements::confidential::{Asset, Nonce, Value as ConfValue};
use deadcat_sdk::elements::hashes::Hash;
use deadcat_sdk::elements::taproot::ControlBlock;
use deadcat_sdk::elements::{
    AssetId, BlockHash, LockTime, OutPoint, Script, Sequence, Transaction, TxIn, TxOut,
    TxOutWitness, Txid,
};
use deadcat_sdk::simplicity::bit_machine::BitMachine;
use deadcat_sdk::simplicity::jet::elements::{ElementsEnv, ElementsUtxo};
use deadcat_sdk::witness::{satisfy_contract, SpendingPath};
use deadcat_sdk::{CompiledContract, ContractParams, MarketState};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

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
    let satisfied =
        satisfy_contract(contract, path, state).map_err(|e| format!("satisfy: {e}"))?;
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

    let mut machine =
        BitMachine::for_program(redeem).map_err(|e| format!("bit machine: {e}"))?;

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
    let params = test_params();
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
    let params = test_params();
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
    let params = test_params();
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
    let params = test_params();
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
