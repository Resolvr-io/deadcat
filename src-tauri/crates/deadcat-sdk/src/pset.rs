use simplicityhl::elements::confidential::{Asset, Nonce, Value as ConfValue};
use simplicityhl::elements::pset::PartiallySignedTransaction;
use simplicityhl::elements::{AssetId, OutPoint, Script, Sequence, TxOut, TxOutWitness};

/// An unblinded UTXO with its secrets revealed — needed for PSET construction.
#[derive(Debug, Clone)]
pub struct UnblindedUtxo {
    pub outpoint: OutPoint,
    pub txout: TxOut,
    pub asset_id: [u8; 32],
    pub value: u64,
    pub asset_blinding_factor: [u8; 32],
    pub value_blinding_factor: [u8; 32],
}

/// Create a new empty PSET v2.
pub(crate) fn new_pset() -> PartiallySignedTransaction {
    PartiallySignedTransaction::new_v2()
}

/// Build an explicit (non-confidential) TxOut.
pub(crate) fn explicit_txout(asset_id: &[u8; 32], amount: u64, script_pubkey: &Script) -> TxOut {
    TxOut {
        asset: Asset::Explicit(AssetId::from_slice(asset_id).expect("valid asset id")),
        value: ConfValue::Explicit(amount),
        nonce: Nonce::Null,
        script_pubkey: script_pubkey.clone(),
        witness: TxOutWitness::default(),
    }
}

/// Build a fee TxOut.
pub(crate) fn fee_txout(asset_id: &[u8; 32], amount: u64) -> TxOut {
    TxOut {
        asset: Asset::Explicit(AssetId::from_slice(asset_id).expect("valid asset id")),
        value: ConfValue::Explicit(amount),
        nonce: Nonce::Null,
        script_pubkey: Script::new(),
        witness: TxOutWitness::default(),
    }
}

/// Add a standard input to a PSET.
pub(crate) fn add_pset_input(pset: &mut PartiallySignedTransaction, utxo: &UnblindedUtxo) {
    let input = simplicityhl::elements::pset::Input {
        previous_txid: utxo.outpoint.txid,
        previous_output_index: utxo.outpoint.vout,
        witness_utxo: Some(utxo.txout.clone()),
        sequence: Some(Sequence::ENABLE_LOCKTIME_NO_RBF),
        ..Default::default()
    };
    pset.add_input(input);
}

/// Add an output to a PSET.
pub(crate) fn add_pset_output(pset: &mut PartiallySignedTransaction, txout: TxOut) {
    let output = simplicityhl::elements::pset::Output {
        amount: match txout.value {
            ConfValue::Explicit(v) => Some(v),
            _ => None,
        },
        asset: match txout.asset {
            Asset::Explicit(id) => Some(id),
            _ => None,
        },
        script_pubkey: txout.script_pubkey,
        ..Default::default()
    };
    pset.add_output(output);
}

/// Build a burn TxOut (empty script) for token burning.
pub(crate) fn burn_txout(asset_id: &[u8; 32], amount: u64) -> TxOut {
    TxOut {
        asset: Asset::Explicit(AssetId::from_slice(asset_id).expect("valid asset id")),
        value: ConfValue::Explicit(amount),
        nonce: Nonce::Null,
        script_pubkey: Script::new(),
        witness: TxOutWitness::default(),
    }
}

/// Placeholder for reissuance token output (confidential, set by blinder).
pub(crate) fn reissuance_token_output(script_pubkey: &Script) -> TxOut {
    TxOut {
        asset: Asset::Null,
        value: ConfValue::Null,
        nonce: Nonce::Null,
        script_pubkey: script_pubkey.clone(),
        witness: TxOutWitness::default(),
    }
}
