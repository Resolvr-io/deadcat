use std::collections::HashMap;
use std::sync::Arc;

use lwk_wollet::elements::confidential::{
    AssetBlindingFactor, Value as ConfValue, ValueBlindingFactor,
};
use lwk_wollet::elements::pset::PartiallySignedTransaction;
use lwk_wollet::elements::secp256k1_zkp::{self, PublicKey};
use lwk_wollet::elements::{
    AssetId, AssetIssuance, BlockHash, ContractHash, LockTime, OutPoint, Script, Sequence,
    Transaction, TxIn,
};
use rand::thread_rng;
use simplicityhl::elements::hashes::Hash;
use simplicityhl::elements::taproot::ControlBlock;
use simplicityhl::simplicity::jet::elements::{ElementsEnv, ElementsUtxo};

use crate::contract::CompiledContract;
use crate::error::{Error, Result};
use crate::pset::UnblindedUtxo;
use crate::pset::initial_issuance::{InitialIssuanceParams, build_initial_issuance_pset};
use crate::pset::issuance::{SubsequentIssuanceParams, build_subsequent_issuance_pset};
use crate::state::MarketState;
use crate::witness::{
    AllBlindingFactors, ReissuanceBlindingFactors, SpendingPath, satisfy_contract_with_env,
    serialize_satisfied,
};

/// Precomputed issuance entropy from the creation transaction.
pub struct IssuanceEntropy {
    pub yes_blinding_nonce: [u8; 32],
    pub yes_entropy: [u8; 32],
    pub no_blinding_nonce: [u8; 32],
    pub no_entropy: [u8; 32],
}

/// Where collateral comes from depends on the market state.
#[allow(clippy::large_enum_variant)]
pub enum CollateralSource {
    /// Dormant → Unresolved: collateral from wallet only.
    Initial { wallet_utxo: UnblindedUtxo },
    /// Unresolved → Unresolved: old collateral from covenant + new from wallet.
    Subsequent {
        covenant_collateral: UnblindedUtxo,
        new_wallet_utxo: UnblindedUtxo,
    },
}

/// All inputs needed to assemble an issuance transaction.
pub struct IssuanceAssemblyInputs {
    pub contract: CompiledContract,
    pub current_state: MarketState,
    pub yes_reissuance_utxo: UnblindedUtxo,
    pub no_reissuance_utxo: UnblindedUtxo,
    pub collateral_source: CollateralSource,
    pub fee_utxo: UnblindedUtxo,
    pub pairs: u64,
    pub fee_amount: u64,
    pub token_destination: Script,
    pub change_destination: Option<Script>,
    pub issuance_entropy: IssuanceEntropy,
    pub lock_time: u32,
}

/// Result of assembling an issuance transaction (before signing).
pub struct AssembledIssuance {
    pub pset: PartiallySignedTransaction,
    pub spending_path: SpendingPath,
    pub current_state: MarketState,
}

/// Compute issuance entropy from the creation transaction.
///
/// Extracts the defining outpoints from the creation tx's first two inputs and
/// computes the asset entropy for YES and NO reissuance tokens.
pub fn compute_issuance_entropy(
    creation_tx: &Transaction,
    yes_rt_abf: &[u8; 32],
    no_rt_abf: &[u8; 32],
) -> Result<IssuanceEntropy> {
    use lwk_wollet::elements::hashes::Hash;

    let yes_defining_outpoint = creation_tx
        .input
        .first()
        .ok_or_else(|| Error::CovenantScan("creation tx has no inputs".into()))?
        .previous_output;
    let no_defining_outpoint = creation_tx
        .input
        .get(1)
        .ok_or_else(|| Error::CovenantScan("creation tx has < 2 inputs".into()))?
        .previous_output;

    let zero_contract_hash = ContractHash::from_byte_array([0u8; 32]);
    let yes_entropy = AssetId::generate_asset_entropy(yes_defining_outpoint, zero_contract_hash);
    let no_entropy = AssetId::generate_asset_entropy(no_defining_outpoint, zero_contract_hash);

    Ok(IssuanceEntropy {
        yes_blinding_nonce: *yes_rt_abf,
        yes_entropy: yes_entropy.to_byte_array(),
        no_blinding_nonce: *no_rt_abf,
        no_entropy: no_entropy.to_byte_array(),
    })
}

/// Build the PSET for an issuance transaction (step E).
pub fn build_issuance_pset(inputs: &IssuanceAssemblyInputs) -> Result<PartiallySignedTransaction> {
    match inputs.current_state {
        MarketState::Dormant => {
            let wallet_utxo = match &inputs.collateral_source {
                CollateralSource::Initial { wallet_utxo } => wallet_utxo,
                _ => return Err(Error::InvalidState),
            };
            build_initial_issuance_pset(
                &inputs.contract,
                &InitialIssuanceParams {
                    yes_reissuance_utxo: inputs.yes_reissuance_utxo.clone(),
                    no_reissuance_utxo: inputs.no_reissuance_utxo.clone(),
                    collateral_utxo: wallet_utxo.clone(),
                    fee_utxo: inputs.fee_utxo.clone(),
                    pairs: inputs.pairs,
                    fee_amount: inputs.fee_amount,
                    yes_token_destination: inputs.token_destination.clone(),
                    no_token_destination: inputs.token_destination.clone(),
                    collateral_change_destination: inputs.change_destination.clone(),
                    fee_change_destination: inputs.change_destination.clone(),
                    yes_issuance_blinding_nonce: inputs.issuance_entropy.yes_blinding_nonce,
                    yes_issuance_asset_entropy: inputs.issuance_entropy.yes_entropy,
                    no_issuance_blinding_nonce: inputs.issuance_entropy.no_blinding_nonce,
                    no_issuance_asset_entropy: inputs.issuance_entropy.no_entropy,
                    lock_time: inputs.lock_time,
                },
            )
        }
        MarketState::Unresolved => {
            let (cov_collateral, new_wallet_utxo) = match &inputs.collateral_source {
                CollateralSource::Subsequent {
                    covenant_collateral,
                    new_wallet_utxo,
                } => (covenant_collateral, new_wallet_utxo),
                _ => return Err(Error::InvalidState),
            };
            build_subsequent_issuance_pset(
                &inputs.contract,
                &SubsequentIssuanceParams {
                    yes_reissuance_utxo: inputs.yes_reissuance_utxo.clone(),
                    no_reissuance_utxo: inputs.no_reissuance_utxo.clone(),
                    collateral_utxo: cov_collateral.clone(),
                    new_collateral_utxo: new_wallet_utxo.clone(),
                    fee_utxo: inputs.fee_utxo.clone(),
                    pairs: inputs.pairs,
                    fee_amount: inputs.fee_amount,
                    yes_token_destination: inputs.token_destination.clone(),
                    no_token_destination: inputs.token_destination.clone(),
                    collateral_change_destination: inputs.change_destination.clone(),
                    fee_change_destination: inputs.change_destination.clone(),
                    yes_issuance_blinding_nonce: inputs.issuance_entropy.yes_blinding_nonce,
                    yes_issuance_asset_entropy: inputs.issuance_entropy.yes_entropy,
                    no_issuance_blinding_nonce: inputs.issuance_entropy.no_blinding_nonce,
                    no_issuance_asset_entropy: inputs.issuance_entropy.no_entropy,
                    lock_time: inputs.lock_time,
                },
            )
        }
        other => Err(Error::NotIssuable(other)),
    }
}

/// Blind a PSET for issuance (step F).
///
/// Sets up blinding keys on RT outputs and change outputs, provides input
/// txout secrets, and calls `blind_last()`.
pub fn blind_issuance_pset(
    pset: &mut PartiallySignedTransaction,
    inputs: &IssuanceAssemblyInputs,
    blinding_pubkey: PublicKey,
) -> Result<()> {
    let yes_rt_id = AssetId::from_slice(&inputs.contract.params().yes_reissuance_token)
        .map_err(|e| Error::Blinding(format!("bad YES reissuance asset: {e}")))?;
    let no_rt_id = AssetId::from_slice(&inputs.contract.params().no_reissuance_token)
        .map_err(|e| Error::Blinding(format!("bad NO reissuance asset: {e}")))?;

    let outputs = pset.outputs_mut();
    outputs[0].amount = Some(1);
    outputs[0].asset = Some(yes_rt_id);
    outputs[1].amount = Some(1);
    outputs[1].asset = Some(no_rt_id);

    let pset_blinding_key = lwk_wollet::elements::bitcoin::PublicKey {
        inner: blinding_pubkey,
        compressed: true,
    };

    for idx in [0usize, 1] {
        outputs[idx].blinding_key = Some(pset_blinding_key);
        outputs[idx].blinder_index = Some(0);
    }
    // Blind the token destination outputs (3, 4) so the wallet can detect them.
    // Outputs 2 (collateral→covenant) and 5 (fee) stay explicit.
    for idx in [3usize, 4] {
        outputs[idx].blinding_key = Some(pset_blinding_key);
        outputs[idx].blinder_index = Some(0);
    }
    for output in &mut outputs[6..] {
        output.blinding_key = Some(pset_blinding_key);
        output.blinder_index = Some(0);
    }

    let pset_inputs = pset.inputs_mut();
    pset_inputs[0].blinded_issuance = Some(0x00);
    pset_inputs[1].blinded_issuance = Some(0x00);

    let collateral_id = AssetId::from_slice(&inputs.contract.params().collateral_asset_id)
        .map_err(|e| Error::Blinding(format!("bad collateral asset: {e}")))?;

    let mut inp_txout_sec = HashMap::new();
    inp_txout_sec.insert(
        0usize,
        lwk_wollet::elements::TxOutSecrets {
            asset: yes_rt_id,
            asset_bf: AssetBlindingFactor::from_slice(
                &inputs.yes_reissuance_utxo.asset_blinding_factor,
            )
            .map_err(|e| Error::Blinding(format!("YES ABF: {e}")))?,
            value: inputs.yes_reissuance_utxo.value,
            value_bf: ValueBlindingFactor::from_slice(
                &inputs.yes_reissuance_utxo.value_blinding_factor,
            )
            .map_err(|e| Error::Blinding(format!("YES VBF: {e}")))?,
        },
    );
    inp_txout_sec.insert(
        1usize,
        lwk_wollet::elements::TxOutSecrets {
            asset: no_rt_id,
            asset_bf: AssetBlindingFactor::from_slice(
                &inputs.no_reissuance_utxo.asset_blinding_factor,
            )
            .map_err(|e| Error::Blinding(format!("NO ABF: {e}")))?,
            value: inputs.no_reissuance_utxo.value,
            value_bf: ValueBlindingFactor::from_slice(
                &inputs.no_reissuance_utxo.value_blinding_factor,
            )
            .map_err(|e| Error::Blinding(format!("NO VBF: {e}")))?,
        },
    );

    match (&inputs.current_state, &inputs.collateral_source) {
        (MarketState::Dormant, CollateralSource::Initial { wallet_utxo }) => {
            inp_txout_sec.insert(2, txout_secrets_from_unblinded(wallet_utxo, collateral_id)?);
            inp_txout_sec.insert(
                3,
                txout_secrets_from_unblinded(&inputs.fee_utxo, collateral_id)?,
            );
        }
        (
            MarketState::Unresolved,
            CollateralSource::Subsequent {
                covenant_collateral,
                new_wallet_utxo,
            },
        ) => {
            inp_txout_sec.insert(
                2,
                lwk_wollet::elements::TxOutSecrets {
                    asset: collateral_id,
                    asset_bf: AssetBlindingFactor::from_slice(&[0u8; 32])
                        .map_err(|e| Error::Blinding(format!("cov ABF: {e}")))?,
                    value: covenant_collateral.value,
                    value_bf: ValueBlindingFactor::from_slice(&[0u8; 32])
                        .map_err(|e| Error::Blinding(format!("cov VBF: {e}")))?,
                },
            );
            inp_txout_sec.insert(
                3,
                txout_secrets_from_unblinded(new_wallet_utxo, collateral_id)?,
            );
            inp_txout_sec.insert(
                4,
                txout_secrets_from_unblinded(&inputs.fee_utxo, collateral_id)?,
            );
        }
        _ => return Err(Error::InvalidState),
    }

    let secp = secp256k1_zkp::Secp256k1::new();
    let mut rng = thread_rng();

    pset.blind_last(&mut rng, &secp, &inp_txout_sec)
        .map_err(|e| Error::Blinding(format!("{e:?}")))?;

    Ok(())
}

/// Recover blinding factors for RT outputs (0, 1) using SLIP77 (step G).
pub fn recover_blinding_factors(
    pset: &PartiallySignedTransaction,
    slip77_key: &lwk_wollet::elements_miniscript::confidential::slip77::MasterBlindingKey,
    change_spk: &Script,
    yes_rt_input: &UnblindedUtxo,
    no_rt_input: &UnblindedUtxo,
) -> Result<AllBlindingFactors> {
    let blinding_sk = slip77_key.blinding_private_key(change_spk);

    let secp = secp256k1_zkp::Secp256k1::new();
    let yes_rt_txout = pset.outputs()[0].to_txout();
    let no_rt_txout = pset.outputs()[1].to_txout();

    let yes_secrets = yes_rt_txout
        .unblind(&secp, blinding_sk)
        .map_err(|e| Error::Blinding(format!("unblind YES RT output: {e}")))?;
    let no_secrets = no_rt_txout
        .unblind(&secp, blinding_sk)
        .map_err(|e| Error::Blinding(format!("unblind NO RT output: {e}")))?;

    let mut yes_output_abf = [0u8; 32];
    yes_output_abf.copy_from_slice(yes_secrets.asset_bf.into_inner().as_ref());
    let mut yes_output_vbf = [0u8; 32];
    yes_output_vbf.copy_from_slice(yes_secrets.value_bf.into_inner().as_ref());
    let mut no_output_abf = [0u8; 32];
    no_output_abf.copy_from_slice(no_secrets.asset_bf.into_inner().as_ref());
    let mut no_output_vbf = [0u8; 32];
    no_output_vbf.copy_from_slice(no_secrets.value_bf.into_inner().as_ref());

    Ok(AllBlindingFactors {
        yes: ReissuanceBlindingFactors {
            input_abf: yes_rt_input.asset_blinding_factor,
            input_vbf: yes_rt_input.value_blinding_factor,
            output_abf: yes_output_abf,
            output_vbf: yes_output_vbf,
        },
        no: ReissuanceBlindingFactors {
            input_abf: no_rt_input.asset_blinding_factor,
            input_vbf: no_rt_input.value_blinding_factor,
            output_abf: no_output_abf,
            output_vbf: no_output_vbf,
        },
    })
}

/// Build a Transaction from the PSET for use as an ElementsEnv during pruning.
///
/// The resulting transaction has the correct structure (inputs, outputs, locktime)
/// matching what will be broadcast, but with empty witnesses.
pub(crate) fn pset_to_pruning_transaction(
    pset: &PartiallySignedTransaction,
) -> Result<Transaction> {
    let outputs: Vec<_> = pset.outputs().iter().map(|o| o.to_txout()).collect();

    let mut inputs = Vec::new();
    for inp in pset.inputs() {
        let outpoint = OutPoint::new(inp.previous_txid, inp.previous_output_index);

        let has_issuance = inp.issuance_value_amount.is_some()
            || inp.issuance_value_comm.is_some()
            || inp.issuance_blinding_nonce.is_some()
            || inp.issuance_asset_entropy.is_some()
            || inp.issuance_inflation_keys.is_some()
            || inp.issuance_inflation_keys_comm.is_some();

        let asset_issuance = if has_issuance {
            let amount = if let Some(comm) = inp.issuance_value_comm {
                ConfValue::Confidential(comm)
            } else if let Some(amt) = inp.issuance_value_amount {
                ConfValue::Explicit(amt)
            } else {
                ConfValue::Null
            };
            let inflation_keys = if let Some(comm) = inp.issuance_inflation_keys_comm {
                ConfValue::Confidential(comm)
            } else if let Some(keys) = inp.issuance_inflation_keys {
                ConfValue::Explicit(keys)
            } else {
                ConfValue::Null
            };

            let zero_nonce =
                || secp256k1_zkp::Tweak::from_slice(&[0u8; 32]).expect("valid zero tweak");

            AssetIssuance {
                asset_blinding_nonce: inp.issuance_blinding_nonce.unwrap_or_else(zero_nonce),
                asset_entropy: inp.issuance_asset_entropy.unwrap_or_default(),
                amount,
                inflation_keys,
            }
        } else {
            Default::default()
        };

        inputs.push(TxIn {
            previous_output: outpoint,
            is_pegin: false,
            script_sig: Script::new(),
            sequence: inp.sequence.unwrap_or(Sequence::ENABLE_LOCKTIME_NO_RBF),
            asset_issuance,
            witness: Default::default(),
        });
    }

    let lock_time = pset
        .global
        .tx_data
        .fallback_locktime
        .unwrap_or(LockTime::ZERO);

    Ok(Transaction {
        version: 2,
        lock_time,
        input: inputs,
        output: outputs,
    })
}

/// Build the ElementsEnv for a specific covenant input, enabling Simplicity pruning.
fn build_pruning_env(
    tx: &Arc<Transaction>,
    utxos: &[ElementsUtxo],
    input_index: u32,
    contract: &CompiledContract,
    state: MarketState,
) -> Result<ElementsEnv<Arc<Transaction>>> {
    let cb_bytes = contract.control_block(state);
    let control_block = ControlBlock::from_slice(&cb_bytes)
        .map_err(|e| Error::Witness(format!("control block: {e}")))?;

    Ok(ElementsEnv::new(
        Arc::clone(tx),
        utxos.to_vec(),
        input_index,
        *contract.cmr(),
        control_block,
        None,
        BlockHash::all_zeros(),
    ))
}

/// Attach Simplicity witness stacks to covenant inputs in the PSET (step H).
///
/// Constructs an ElementsEnv from the PSET to enable Simplicity program pruning.
/// Pruning replaces un-taken case branches with HIDDEN nodes (their CMR hashes),
/// which is required by Simplicity's anti-DOS consensus rules.
pub fn attach_witnesses(
    pset: &mut PartiallySignedTransaction,
    contract: &CompiledContract,
    state: MarketState,
    blinding: AllBlindingFactors,
) -> Result<SpendingPath> {
    let spending_path = match state {
        MarketState::Dormant => SpendingPath::InitialIssuance { blinding },
        MarketState::Unresolved => SpendingPath::SubsequentIssuance { blinding },
        other => return Err(Error::NotIssuable(other)),
    };

    // Build a Transaction + UTXOs from the PSET for Simplicity pruning.
    let tx = Arc::new(pset_to_pruning_transaction(pset)?);
    let utxos: Vec<ElementsUtxo> = pset
        .inputs()
        .iter()
        .enumerate()
        .map(|(i, inp)| {
            inp.witness_utxo
                .as_ref()
                .map(|u| ElementsUtxo::from(u.clone()))
                .ok_or_else(|| Error::Pset(format!("input {i} missing witness_utxo")))
        })
        .collect::<Result<Vec<_>>>()?;

    let control_block_bytes = contract.control_block(state);
    let cmr_bytes = contract.cmr().to_byte_array().to_vec();

    // Prune, serialize, and build the witness stack for a covenant input.
    let build_witness_stack = |path: &SpendingPath, input_index: u32| -> Result<Vec<Vec<u8>>> {
        let env = build_pruning_env(&tx, &utxos, input_index, contract, state)?;
        let satisfied = satisfy_contract_with_env(contract, path, state, Some(&env))
            .map_err(|e| Error::Witness(e.to_string()))?;
        let (program_bytes, witness_bytes) = serialize_satisfied(&satisfied);

        let stack = vec![
            witness_bytes,
            program_bytes,
            cmr_bytes.clone(),
            control_block_bytes.clone(),
        ];

        debug_assert!(
            satisfied.redeem().bounds().cost.is_budget_valid(&stack),
            "input {input_index}: Simplicity program cost exceeds witness budget"
        );

        Ok(stack)
    };

    // Primary covenant input (index 0)
    pset.inputs_mut()[0].final_script_witness = Some(build_witness_stack(&spending_path, 0)?);

    // Secondary covenant input (index 1)
    let secondary_path = SpendingPath::SecondaryCovenantInput;
    pset.inputs_mut()[1].final_script_witness = Some(build_witness_stack(&secondary_path, 1)?);

    // For SubsequentIssuance, input 2 is also a covenant input (collateral)
    if state == MarketState::Unresolved {
        pset.inputs_mut()[2].final_script_witness = Some(build_witness_stack(&secondary_path, 2)?);
    }

    Ok(spending_path)
}

/// Full production assembly pipeline: build → blind → recover → attach witnesses.
pub fn assemble_issuance(
    inputs: IssuanceAssemblyInputs,
    slip77_key: &lwk_wollet::elements_miniscript::confidential::slip77::MasterBlindingKey,
    blinding_pubkey: PublicKey,
    change_spk: &Script,
) -> Result<AssembledIssuance> {
    let state = inputs.current_state;

    let mut pset = build_issuance_pset(&inputs)?;
    blind_issuance_pset(&mut pset, &inputs, blinding_pubkey)?;

    let blinding = recover_blinding_factors(
        &pset,
        slip77_key,
        change_spk,
        &inputs.yes_reissuance_utxo,
        &inputs.no_reissuance_utxo,
    )?;

    let spending_path = attach_witnesses(&mut pset, &inputs.contract, state, blinding)?;

    Ok(AssembledIssuance {
        pset,
        spending_path,
        current_state: state,
    })
}

/// Result of assembling a non-issuance transaction (before signing).
pub struct AssembledTransaction {
    pub pset: PartiallySignedTransaction,
    pub spending_path: SpendingPath,
}

/// Ensure the fee output (OP_RETURN, empty script_pubkey) is the last output.
///
/// The contract checks `ensure_fee_output(num_outputs - 1)`, meaning the fee
/// must be the last output. PSET builders may place a fee change output after
/// the fee. This function swaps the last two outputs if needed.
fn ensure_fee_output_last(pset: &mut PartiallySignedTransaction) {
    let outputs = pset.outputs_mut();
    let n = outputs.len();
    if n < 2 {
        return;
    }
    // If the last output is NOT empty script (fee), but the second-to-last is,
    // swap them so the fee is last.
    let last_is_fee = outputs[n - 1].script_pubkey.is_empty();
    let prev_is_fee = outputs[n - 2].script_pubkey.is_empty();
    if !last_is_fee && prev_is_fee {
        outputs.swap(n - 2, n - 1);
    }
}

/// Attach Simplicity witness stacks to parameterized covenant inputs.
///
/// Unlike `attach_witnesses` (issuance-only, hardcoded indices 0/1/2), this
/// function lets the caller specify which PSET inputs are covenant inputs.
pub fn attach_covenant_witnesses(
    pset: &mut PartiallySignedTransaction,
    contract: &CompiledContract,
    state: MarketState,
    primary_path: SpendingPath,
    primary_index: usize,
    secondary_indices: &[usize],
) -> Result<SpendingPath> {
    let tx = Arc::new(pset_to_pruning_transaction(pset)?);
    let utxos: Vec<ElementsUtxo> = pset
        .inputs()
        .iter()
        .enumerate()
        .map(|(i, inp)| {
            inp.witness_utxo
                .as_ref()
                .map(|u| ElementsUtxo::from(u.clone()))
                .ok_or_else(|| Error::Pset(format!("input {i} missing witness_utxo")))
        })
        .collect::<Result<Vec<_>>>()?;

    let control_block_bytes = contract.control_block(state);
    let cmr_bytes = contract.cmr().to_byte_array().to_vec();

    let build_witness_stack = |path: &SpendingPath, input_index: u32| -> Result<Vec<Vec<u8>>> {
        let env = build_pruning_env(&tx, &utxos, input_index, contract, state)?;
        let satisfied = satisfy_contract_with_env(contract, path, state, Some(&env))
            .map_err(|e| Error::Witness(e.to_string()))?;
        let (program_bytes, witness_bytes) = serialize_satisfied(&satisfied);

        let stack = vec![
            witness_bytes,
            program_bytes,
            cmr_bytes.clone(),
            control_block_bytes.clone(),
        ];

        debug_assert!(
            satisfied.redeem().bounds().cost.is_budget_valid(&stack),
            "input {input_index}: Simplicity program cost exceeds witness budget"
        );

        Ok(stack)
    };

    // Primary covenant input
    pset.inputs_mut()[primary_index].final_script_witness =
        Some(build_witness_stack(&primary_path, primary_index as u32)?);

    // Secondary covenant inputs
    let secondary_path = SpendingPath::SecondaryCovenantInput;
    for &idx in secondary_indices {
        pset.inputs_mut()[idx].final_script_witness =
            Some(build_witness_stack(&secondary_path, idx as u32)?);
    }

    Ok(primary_path)
}

/// Blind a PSET by setting up RT outputs (if any), marking wallet outputs for
/// blinding, providing ALL input txout secrets, and calling `blind_last`.
///
/// - `rt_setup`: If Some, sets up outputs 0,1 as RT outputs with the given asset IDs
/// - `wallet_output_indices`: output indices to mark for blinding (wallet destinations)
/// - `input_utxos`: (index, utxo) pairs for ALL inputs that need secrets
fn blind_pset(
    pset: &mut PartiallySignedTransaction,
    rt_setup: Option<(&UnblindedUtxo, &UnblindedUtxo)>,
    wallet_output_indices: &[usize],
    input_utxos: &[(usize, &UnblindedUtxo)],
    blinding_pubkey: PublicKey,
) -> Result<()> {
    let pset_blinding_key = lwk_wollet::elements::bitcoin::PublicKey {
        inner: blinding_pubkey,
        compressed: true,
    };

    if let Some((yes_rt, no_rt)) = rt_setup {
        let yes_rt_id = AssetId::from_slice(&yes_rt.asset_id)
            .map_err(|e| Error::Blinding(format!("bad YES reissuance asset: {e}")))?;
        let no_rt_id = AssetId::from_slice(&no_rt.asset_id)
            .map_err(|e| Error::Blinding(format!("bad NO reissuance asset: {e}")))?;

        let outputs = pset.outputs_mut();
        outputs[0].amount = Some(1);
        outputs[0].asset = Some(yes_rt_id);
        outputs[1].amount = Some(1);
        outputs[1].asset = Some(no_rt_id);

        for idx in [0usize, 1] {
            outputs[idx].blinding_key = Some(pset_blinding_key);
            outputs[idx].blinder_index = Some(0);
        }
    }

    let outputs = pset.outputs_mut();
    for &idx in wallet_output_indices {
        outputs[idx].blinding_key = Some(pset_blinding_key);
        outputs[idx].blinder_index = Some(0);
    }

    let mut inp_txout_sec = HashMap::new();
    for &(idx, utxo) in input_utxos {
        let asset_id = AssetId::from_slice(&utxo.asset_id)
            .map_err(|e| Error::Blinding(format!("input {idx} asset: {e}")))?;
        inp_txout_sec.insert(idx, txout_secrets_from_unblinded(utxo, asset_id)?);
    }

    let secp = secp256k1_zkp::Secp256k1::new();
    let mut rng = thread_rng();
    pset.blind_last(&mut rng, &secp, &inp_txout_sec)
        .map_err(|e| Error::Blinding(format!("{e:?}")))?;

    Ok(())
}

/// Identify wallet output indices: all outputs with non-empty script_pubkey
/// that are NOT covenant addresses and NOT burn (OP_RETURN) outputs.
fn find_wallet_output_indices(
    pset: &PartiallySignedTransaction,
    contract: &CompiledContract,
) -> Vec<usize> {
    let covenant_spks: Vec<Script> = [
        MarketState::Dormant,
        MarketState::Unresolved,
        MarketState::ResolvedYes,
        MarketState::ResolvedNo,
    ]
    .iter()
    .map(|s| contract.script_pubkey(*s))
    .collect();

    pset.outputs()
        .iter()
        .enumerate()
        .filter(|(_, o)| !o.script_pubkey.is_empty() && !covenant_spks.contains(&o.script_pubkey))
        .map(|(i, _)| i)
        .collect()
}

/// Assemble a post-resolution redemption transaction.
pub fn assemble_post_resolution_redemption(
    contract: &CompiledContract,
    params: &crate::pset::post_resolution_redemption::PostResolutionRedemptionParams,
    blinding_pubkey: PublicKey,
) -> Result<AssembledTransaction> {
    let mut pset = crate::pset::post_resolution_redemption::build_post_resolution_redemption_pset(
        contract, params,
    )?;
    ensure_fee_output_last(&mut pset);

    let wallet_outputs = find_wallet_output_indices(&pset, contract);
    let input_refs = build_input_refs(
        vec![(0, &params.collateral_utxo)],
        &[&params.token_utxos],
        &params.fee_utxo,
    );
    blind_pset(
        &mut pset,
        None,
        &wallet_outputs,
        &input_refs,
        blinding_pubkey,
    )?;

    let spending_path = SpendingPath::PostResolutionRedemption {
        tokens_burned: params.tokens_burned,
    };

    let spending_path = attach_covenant_witnesses(
        &mut pset,
        contract,
        params.resolved_state,
        spending_path,
        0,
        &[],
    )?;

    Ok(AssembledTransaction {
        pset,
        spending_path,
    })
}

/// Assemble an expiry redemption transaction.
pub fn assemble_expiry_redemption(
    contract: &CompiledContract,
    params: &crate::pset::expiry_redemption::ExpiryRedemptionParams,
    blinding_pubkey: PublicKey,
) -> Result<AssembledTransaction> {
    let mut pset = crate::pset::expiry_redemption::build_expiry_redemption_pset(contract, params)?;
    ensure_fee_output_last(&mut pset);

    let wallet_outputs = find_wallet_output_indices(&pset, contract);
    let input_refs = build_input_refs(
        vec![(0, &params.collateral_utxo)],
        &[&params.token_utxos],
        &params.fee_utxo,
    );
    blind_pset(
        &mut pset,
        None,
        &wallet_outputs,
        &input_refs,
        blinding_pubkey,
    )?;

    let spending_path = SpendingPath::ExpiryRedemption {
        tokens_burned: params.tokens_burned,
        burn_token_asset: params.burn_token_asset,
    };

    let spending_path = attach_covenant_witnesses(
        &mut pset,
        contract,
        MarketState::Unresolved,
        spending_path,
        0,
        &[],
    )?;

    Ok(AssembledTransaction {
        pset,
        spending_path,
    })
}

/// Assemble a cancellation transaction.
///
/// For partial cancellation: blind wallet outputs, witness at index 0 only.
/// For full cancellation: blind RT + wallet outputs, recover blinding factors,
/// witness at index 0 with secondaries [1, 2].
pub fn assemble_cancellation(
    contract: &CompiledContract,
    params: &crate::pset::cancellation::CancellationParams,
    slip77_key: &lwk_wollet::elements_miniscript::confidential::slip77::MasterBlindingKey,
    blinding_pubkey: PublicKey,
    change_spk: &Script,
) -> Result<AssembledTransaction> {
    let cpt = contract.params().collateral_per_token;
    let refund = params
        .pairs_burned
        .checked_mul(2)
        .and_then(|v| v.checked_mul(cpt))
        .ok_or(Error::CollateralOverflow)?;
    let remaining = params.collateral_utxo.value.saturating_sub(refund);
    let is_full = remaining == 0;

    let mut pset = crate::pset::cancellation::build_cancellation_pset(contract, params)?;
    ensure_fee_output_last(&mut pset);

    if is_full {
        let yes_rt = params
            .yes_reissuance_utxo
            .as_ref()
            .ok_or(Error::MissingReissuanceUtxos)?;
        let no_rt = params
            .no_reissuance_utxo
            .as_ref()
            .ok_or(Error::MissingReissuanceUtxos)?;

        let wallet_outputs = find_wallet_output_indices(&pset, contract);
        let input_refs = build_input_refs(
            vec![(0, &params.collateral_utxo), (1, yes_rt), (2, no_rt)],
            &[&params.yes_token_utxos, &params.no_token_utxos],
            &params.fee_utxo,
        );

        blind_pset(
            &mut pset,
            Some((yes_rt, no_rt)),
            &wallet_outputs,
            &input_refs,
            blinding_pubkey,
        )?;

        let blinding = recover_blinding_factors(&pset, slip77_key, change_spk, yes_rt, no_rt)?;

        let spending_path = SpendingPath::Cancellation {
            pairs_burned: params.pairs_burned,
            blinding: Some(blinding),
        };

        let spending_path = attach_covenant_witnesses(
            &mut pset,
            contract,
            MarketState::Unresolved,
            spending_path,
            0,
            &[1, 2],
        )?;

        Ok(AssembledTransaction {
            pset,
            spending_path,
        })
    } else {
        let wallet_outputs = find_wallet_output_indices(&pset, contract);
        let input_refs = build_input_refs(
            vec![(0, &params.collateral_utxo)],
            &[&params.yes_token_utxos, &params.no_token_utxos],
            &params.fee_utxo,
        );

        blind_pset(
            &mut pset,
            None,
            &wallet_outputs,
            &input_refs,
            blinding_pubkey,
        )?;

        let spending_path = SpendingPath::Cancellation {
            pairs_burned: params.pairs_burned,
            blinding: None,
        };

        let spending_path = attach_covenant_witnesses(
            &mut pset,
            contract,
            MarketState::Unresolved,
            spending_path,
            0,
            &[],
        )?;

        Ok(AssembledTransaction {
            pset,
            spending_path,
        })
    }
}

/// Assemble an oracle resolve transaction.
#[allow(clippy::too_many_arguments)]
pub fn assemble_oracle_resolve(
    contract: &CompiledContract,
    params: &crate::pset::oracle_resolve::OracleResolveParams,
    oracle_signature: [u8; 64],
    slip77_key: &lwk_wollet::elements_miniscript::confidential::slip77::MasterBlindingKey,
    blinding_pubkey: PublicKey,
    change_spk: &Script,
    yes_rt_input: &UnblindedUtxo,
    no_rt_input: &UnblindedUtxo,
) -> Result<AssembledTransaction> {
    let mut pset = crate::pset::oracle_resolve::build_oracle_resolve_pset(contract, params)?;

    // Oracle resolve: 4 outputs (RT0, RT1, collateral, fee). Only RT outputs are blinded.
    // Inputs: yes_rt(0), no_rt(1), collateral(2), fee(3)
    let input_refs: Vec<(usize, &UnblindedUtxo)> = vec![
        (0, yes_rt_input),
        (1, no_rt_input),
        (2, &params.collateral_utxo),
        (3, &params.fee_utxo),
    ];
    blind_pset(
        &mut pset,
        Some((yes_rt_input, no_rt_input)),
        &[],
        &input_refs,
        blinding_pubkey,
    )?;

    let blinding =
        recover_blinding_factors(&pset, slip77_key, change_spk, yes_rt_input, no_rt_input)?;

    let spending_path = SpendingPath::OracleResolve {
        outcome_yes: params.outcome_yes,
        oracle_signature,
        blinding,
    };

    let spending_path = attach_covenant_witnesses(
        &mut pset,
        contract,
        MarketState::Unresolved,
        spending_path,
        0,
        &[1, 2],
    )?;

    Ok(AssembledTransaction {
        pset,
        spending_path,
    })
}

/// Build sequential input refs from fixed inputs, token groups, and a fee UTXO.
///
/// Assigns consecutive indices starting after the last fixed input.
fn build_input_refs<'a>(
    fixed: Vec<(usize, &'a UnblindedUtxo)>,
    token_groups: &[&'a [UnblindedUtxo]],
    fee_utxo: &'a UnblindedUtxo,
) -> Vec<(usize, &'a UnblindedUtxo)> {
    let mut refs = fixed;
    let mut idx = refs.last().map_or(0, |(i, _)| i + 1);
    for group in token_groups {
        for u in *group {
            refs.push((idx, u));
            idx += 1;
        }
    }
    refs.push((idx, fee_utxo));
    refs
}

pub(crate) fn txout_secrets_from_unblinded(
    utxo: &UnblindedUtxo,
    expected_asset: AssetId,
) -> Result<lwk_wollet::elements::TxOutSecrets> {
    Ok(lwk_wollet::elements::TxOutSecrets {
        asset: expected_asset,
        asset_bf: AssetBlindingFactor::from_slice(&utxo.asset_blinding_factor)
            .map_err(|e| Error::Blinding(format!("ABF: {e}")))?,
        value: utxo.value,
        value_bf: ValueBlindingFactor::from_slice(&utxo.value_blinding_factor)
            .map_err(|e| Error::Blinding(format!("VBF: {e}")))?,
    })
}
