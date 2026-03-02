use lwk_wollet::elements::confidential::{
    AssetBlindingFactor, Value as ConfValue, ValueBlindingFactor,
};
use lwk_wollet::elements::pset::PartiallySignedTransaction;
use lwk_wollet::elements::secp256k1_zkp;
use lwk_wollet::elements::{
    AssetId, AssetIssuance, LockTime, OutPoint, Script, Sequence, Transaction, TxIn,
};

use crate::error::{Error, Result};
use crate::pset::UnblindedUtxo;

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
