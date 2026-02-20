use std::collections::HashMap;

use lwk_wollet::elements::confidential::{AssetBlindingFactor, ValueBlindingFactor};
use lwk_wollet::elements::pset::PartiallySignedTransaction;
use lwk_wollet::elements::secp256k1_zkp::{self, PublicKey};
use lwk_wollet::elements::{AssetId, ContractHash, Script, Transaction};
use rand::thread_rng;

use crate::contract::CompiledContract;
use crate::error::{Error, Result};
use crate::pset::UnblindedUtxo;
use crate::pset::initial_issuance::{InitialIssuanceParams, build_initial_issuance_pset};
use crate::pset::issuance::{SubsequentIssuanceParams, build_subsequent_issuance_pset};
use crate::state::MarketState;
use crate::witness::{
    AllBlindingFactors, ReissuanceBlindingFactors, SpendingPath, satisfy_contract,
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
    let yes_entropy =
        AssetId::generate_asset_entropy(yes_defining_outpoint, zero_contract_hash);
    let no_entropy =
        AssetId::generate_asset_entropy(no_defining_outpoint, zero_contract_hash);

    Ok(IssuanceEntropy {
        yes_blinding_nonce: *yes_rt_abf,
        yes_entropy: yes_entropy.to_byte_array(),
        no_blinding_nonce: *no_rt_abf,
        no_entropy: no_entropy.to_byte_array(),
    })
}

/// Build the PSET for an issuance transaction (step E).
pub fn build_issuance_pset(
    inputs: &IssuanceAssemblyInputs,
) -> Result<PartiallySignedTransaction> {
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
    for idx in 6..outputs.len() {
        outputs[idx].blinding_key = Some(pset_blinding_key);
        outputs[idx].blinder_index = Some(0);
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
            inp_txout_sec.insert(
                2,
                txout_secrets_from_unblinded(wallet_utxo, collateral_id)?,
            );
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
        .unblind(&secp, blinding_sk.into())
        .map_err(|e| Error::Blinding(format!("unblind YES RT output: {e}")))?;
    let no_secrets = no_rt_txout
        .unblind(&secp, blinding_sk.into())
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

/// Attach Simplicity witness stacks to covenant inputs in the PSET (step H).
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

    let satisfied = satisfy_contract(contract, &spending_path, state)
        .map_err(|e| Error::Witness(e.to_string()))?;
    let (program_bytes, witness_bytes) = serialize_satisfied(&satisfied);

    let control_block = contract.control_block(state);
    let cmr_bytes = contract.cmr().to_byte_array().to_vec();
    let witness_stack: Vec<Vec<u8>> =
        vec![witness_bytes, program_bytes, cmr_bytes.clone(), control_block.clone()];

    pset.inputs_mut()[0].final_script_witness = Some(witness_stack);

    let secondary_path = SpendingPath::SecondaryCovenantInput;
    let secondary_satisfied = satisfy_contract(contract, &secondary_path, state)
        .map_err(|e| Error::Witness(e.to_string()))?;
    let (sec_program, sec_witness) = serialize_satisfied(&secondary_satisfied);
    let sec_witness_stack: Vec<Vec<u8>> =
        vec![sec_witness, sec_program, cmr_bytes, control_block];

    pset.inputs_mut()[1].final_script_witness = Some(sec_witness_stack.clone());

    if state == MarketState::Unresolved {
        pset.inputs_mut()[2].final_script_witness = Some(sec_witness_stack);
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

fn txout_secrets_from_unblinded(
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
