//! Test utilities for validating assembled transactions against ElementsEnv.
//!
//! This module bridges the assembly pipeline with the Simplicity execution
//! environment, enabling integration tests that cover the full path from
//! PSET construction through witness satisfaction without a live network.

use std::sync::Arc;

use simplicityhl::elements::confidential::{Asset, Nonce, Value as ConfValue};
use simplicityhl::elements::hashes::Hash;
use simplicityhl::elements::secp256k1_zkp::{Generator, PedersenCommitment, Secp256k1, Tag, Tweak};
use simplicityhl::elements::taproot::ControlBlock;
use simplicityhl::elements::{
    AssetId, AssetIssuance, BlockHash, LockTime, OutPoint, Script, Sequence, Transaction, TxIn,
    TxOut, TxOutWitness,
};
use simplicityhl::simplicity::bit_machine::BitMachine;
use simplicityhl::simplicity::jet::elements::{ElementsEnv, ElementsUtxo};

use crate::assembly::{IssuanceAssemblyInputs, attach_witnesses, build_issuance_pset};
use crate::contract::CompiledContract;
use crate::error::{Error, Result};
use crate::state::MarketState;
use crate::witness::{
    AllBlindingFactors, ReissuanceBlindingFactors, SpendingPath, satisfy_contract,
};

// ---------------------------------------------------------------------------
// Shared test helpers (promoted from tests/elements_env_execution.rs)
// ---------------------------------------------------------------------------

/// Build an explicit (non-confidential) TxOut for tests.
pub fn explicit_txout(asset_bytes: &[u8; 32], amount: u64, spk: &Script) -> TxOut {
    TxOut {
        asset: Asset::Explicit(AssetId::from_slice(asset_bytes).expect("valid asset")),
        value: ConfValue::Explicit(amount),
        nonce: Nonce::Null,
        script_pubkey: spk.clone(),
        witness: TxOutWitness::default(),
    }
}

/// Build a simple TxIn with no issuance.
pub fn simple_txin(outpoint: OutPoint) -> TxIn {
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
/// reissuance token. The commitments match the given blinding factors.
pub fn confidential_rt_txout(
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
pub fn issuance_txin(outpoint: OutPoint, pairs: u64) -> TxIn {
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

/// Standard test blinding factors.
pub fn test_blinding() -> AllBlindingFactors {
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

// ---------------------------------------------------------------------------
// ElementsEnv execution
// ---------------------------------------------------------------------------

/// Execute a satisfied Simplicity program against a mock ElementsEnv.
///
/// Returns Ok(()) on success, or an error description on failure.
pub fn execute_against_env(
    contract: &CompiledContract,
    state: MarketState,
    path: &SpendingPath,
    tx: Arc<Transaction>,
    utxos: Vec<ElementsUtxo>,
    input_index: u32,
) -> std::result::Result<(), String> {
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
// Assembly → ElementsEnv bridge
// ---------------------------------------------------------------------------

/// Build a Transaction + Vec<ElementsUtxo> from IssuanceAssemblyInputs using
/// predetermined blinding factors (no blind_last, no SLIP77).
///
/// This closes the test gap: PSET builder → witnessed Transaction → ElementsEnv.
pub fn assemble_issuance_for_env(
    inputs: IssuanceAssemblyInputs,
    blinding: AllBlindingFactors,
) -> Result<(Arc<Transaction>, Vec<ElementsUtxo>, SpendingPath)> {
    let state = inputs.current_state;
    let contract = &inputs.contract;
    let params = contract.params();

    // 1. Build the PSET (same as production path)
    let mut pset = build_issuance_pset(&inputs)?;

    // 2. Attach witnesses with provided blinding factors (skips blind_last + SLIP77)
    let spending_path = attach_witnesses(&mut pset, contract, state, blinding.clone())?;

    // 3. Convert PSET to Transaction by hand-constructing inputs/outputs
    //    that match what blind_last would have produced, using our test blinding factors.

    let unresolved_spk = contract.script_pubkey(MarketState::Unresolved);

    // Build outputs
    let mut outputs = Vec::new();

    // [0] YES RT → Unresolved (confidential)
    outputs.push(confidential_rt_txout(
        &params.yes_reissuance_token,
        &blinding.yes.output_abf,
        &blinding.yes.output_vbf,
        &unresolved_spk,
    ));

    // [1] NO RT → Unresolved (confidential)
    outputs.push(confidential_rt_txout(
        &params.no_reissuance_token,
        &blinding.no.output_abf,
        &blinding.no.output_vbf,
        &unresolved_spk,
    ));

    // Remaining outputs from the PSET are explicit — read them directly.
    // Output 2+: collateral, YES tokens, NO tokens, fee, optional change
    for pset_out in pset.outputs().iter().skip(2) {
        let asset = pset_out
            .asset
            .ok_or_else(|| Error::Pset("output missing asset".into()))?;
        let amount = pset_out
            .amount
            .ok_or_else(|| Error::Pset("output missing amount".into()))?;

        outputs.push(TxOut {
            asset: Asset::Explicit(asset),
            value: ConfValue::Explicit(amount),
            nonce: Nonce::Null,
            script_pubkey: pset_out.script_pubkey.clone(),
            witness: TxOutWitness::default(),
        });
    }

    // Build inputs
    let mut tx_inputs = Vec::new();
    for (i, pset_in) in pset.inputs().iter().enumerate() {
        let outpoint = OutPoint::new(pset_in.previous_txid, pset_in.previous_output_index);

        // Inputs 0 and 1 are reissuance token inputs with asset issuance
        if i < 2 {
            tx_inputs.push(issuance_txin(outpoint, inputs.pairs));
        } else {
            tx_inputs.push(simple_txin(outpoint));
        }
    }

    let tx = Arc::new(Transaction {
        version: 2,
        lock_time: LockTime::ZERO,
        input: tx_inputs,
        output: outputs,
    });

    // 4. Build UTXOs from the PSET's witness_utxo fields
    let mut utxos = Vec::new();
    for (i, pset_in) in pset.inputs().iter().enumerate() {
        if let Some(ref witness_utxo) = pset_in.witness_utxo {
            utxos.push(ElementsUtxo::from(witness_utxo.clone()));
        } else {
            return Err(Error::Pset(format!("input {} missing witness_utxo", i)));
        }
    }

    // 5. Replace RT input UTXOs with confidential versions matching the input blinding factors.
    //    The PSET stores the original (confidential) witness_utxo, but in tests we need
    //    UTXOs whose commitments match the blinding factors we provide to the contract.
    let input_state_spk = contract.script_pubkey(state);
    utxos[0] = ElementsUtxo::from(confidential_rt_txout(
        &params.yes_reissuance_token,
        &blinding.yes.input_abf,
        &blinding.yes.input_vbf,
        &input_state_spk,
    ));
    utxos[1] = ElementsUtxo::from(confidential_rt_txout(
        &params.no_reissuance_token,
        &blinding.no.input_abf,
        &blinding.no.input_vbf,
        &input_state_spk,
    ));

    Ok((tx, utxos, spending_path))
}

// ---------------------------------------------------------------------------
// In-memory discovery store for tests
// ---------------------------------------------------------------------------

use crate::discovery::store_trait::{ContractMetadataInput, DiscoveryStore};
use crate::maker_order::params::MakerOrderParams;
use crate::params::ContractParams;

/// Minimal in-memory store implementing `DiscoveryStore` for integration tests.
///
/// Deduplicates markets by `market_id` and stores orders as-is.
#[derive(Debug, Default)]
pub struct TestStore {
    pub markets: Vec<ContractParams>,
    pub orders: Vec<(MakerOrderParams, Option<String>)>,
}

impl DiscoveryStore for TestStore {
    fn ingest_market(
        &mut self,
        params: &ContractParams,
        _meta: Option<&ContractMetadataInput>,
    ) -> std::result::Result<(), String> {
        let mid = params.market_id();
        if !self.markets.iter().any(|p| p.market_id() == mid) {
            self.markets.push(*params);
        }
        Ok(())
    }

    fn ingest_maker_order(
        &mut self,
        params: &MakerOrderParams,
        _maker_pubkey: Option<&[u8; 32]>,
        _nonce: Option<&[u8; 32]>,
        nostr_event_id: Option<&str>,
        _nostr_event_json: Option<&str>,
    ) -> std::result::Result<(), String> {
        self.orders
            .push((*params, nostr_event_id.map(|s| s.to_string())));
        Ok(())
    }
}
