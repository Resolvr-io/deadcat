use crate::prediction_market::anchor::{PredictionMarketAnchor, parse_prediction_market_anchor};
use crate::prediction_market::contract::CompiledPredictionMarket;
use crate::prediction_market::params::PredictionMarketParams;
use crate::prediction_market::state::{MarketSlot, MarketState};
use crate::pset::burn_script_pubkey;
use crate::{
    elements,
    elements::confidential::{Asset, Value},
};

use elements::hashes::Hash as _;
use elements::secp256k1_zkp::{Generator, PedersenCommitment, Secp256k1, Tag, Tweak, ZERO_TWEAK};
use elements::{AssetId, ContractHash, OutPoint, Script, Transaction, TxOut, Txid};

#[doc(hidden)]
pub trait PredictionMarketScanBackend {
    fn fetch_transaction(&self, txid: &Txid) -> std::result::Result<Transaction, String>;
    fn spending_txid(
        &self,
        outpoint: &OutPoint,
        script_pubkey: &Script,
    ) -> std::result::Result<Option<Txid>, String>;
}

#[derive(Debug, Clone)]
#[doc(hidden)]
pub struct CanonicalMarketUtxo {
    pub slot: MarketSlot,
    pub outpoint: OutPoint,
    pub txout: TxOut,
}

#[derive(Debug, Clone)]
#[doc(hidden)]
pub struct CanonicalMarketScan {
    pub state: MarketState,
    pub utxos: Vec<CanonicalMarketUtxo>,
    pub last_transition_txid: Txid,
}

fn is_canonical_creation_input(txin: &elements::TxIn, expected_token: AssetId) -> bool {
    let zero_contract_hash = [0u8; 32];

    if txin.asset_issuance.is_null()
        || txin.asset_issuance.asset_blinding_nonce != ZERO_TWEAK
        || txin.asset_issuance.asset_entropy != zero_contract_hash
        || !matches!(txin.asset_issuance.amount, Value::Null)
        || !matches!(txin.asset_issuance.inflation_keys, Value::Explicit(1))
    {
        return false;
    }

    let entropy = AssetId::generate_asset_entropy(
        txin.previous_output,
        ContractHash::from_byte_array(zero_contract_hash),
    );

    AssetId::reissuance_token_from_entropy(entropy, false) == expected_token
}

fn matches_dormant_creation_output(
    txout: &TxOut,
    expected_script: &Script,
    expected_asset: AssetId,
) -> bool {
    if txout.script_pubkey != *expected_script {
        return false;
    }

    matches!(
        (&txout.asset, &txout.value, &txout.nonce),
        (Asset::Explicit(asset), Value::Explicit(1), elements::confidential::Nonce::Null)
            if *asset == expected_asset
    )
}

fn matches_confidential_dormant_creation_output(
    txout: &TxOut,
    expected_script: &Script,
    expected_asset: AssetId,
    opening_asset_blinding_factor: [u8; 32],
    opening_value_blinding_factor: [u8; 32],
) -> std::result::Result<bool, String> {
    if txout.script_pubkey != *expected_script {
        return Ok(false);
    }

    let (
        Asset::Confidential(actual_generator),
        Value::Confidential(actual_commitment),
        elements::confidential::Nonce::Confidential(_),
    ) = (&txout.asset, &txout.value, &txout.nonce)
    else {
        return Ok(false);
    };

    let secp = Secp256k1::new();
    let expected_generator = Generator::new_blinded(
        &secp,
        Tag::from(expected_asset.into_inner().to_byte_array()),
        Tweak::from_slice(&opening_asset_blinding_factor)
            .map_err(|e| format!("invalid dormant asset blinding factor: {e}"))?,
    );
    let expected_commitment = PedersenCommitment::new(
        &secp,
        1,
        Tweak::from_slice(&opening_value_blinding_factor)
            .map_err(|e| format!("invalid dormant value blinding factor: {e}"))?,
        expected_generator,
    );

    Ok(*actual_generator == expected_generator && *actual_commitment == expected_commitment)
}

#[doc(hidden)]
pub fn validate_prediction_market_creation_tx(
    params: &PredictionMarketParams,
    tx: &Transaction,
    anchor: &PredictionMarketAnchor,
) -> std::result::Result<bool, String> {
    let parsed_anchor = parse_prediction_market_anchor(anchor)?;
    let compiled = CompiledPredictionMarket::new(*params).map_err(|e| e.to_string())?;
    let dormant_yes_spk = compiled.script_pubkey(MarketSlot::DormantYesRt);
    let dormant_no_spk = compiled.script_pubkey(MarketSlot::DormantNoRt);
    let expected_yes = AssetId::from_slice(&params.yes_reissuance_token)
        .map_err(|e| format!("bad YES reissuance token id: {e}"))?;
    let expected_no = AssetId::from_slice(&params.no_reissuance_token)
        .map_err(|e| format!("bad NO reissuance token id: {e}"))?;

    let Some(yes_input) = tx.input.first() else {
        return Ok(false);
    };
    let Some(no_input) = tx.input.get(1) else {
        return Ok(false);
    };

    if !is_canonical_creation_input(yes_input, expected_yes)
        || !is_canonical_creation_input(no_input, expected_no)
    {
        return Ok(false);
    }

    if tx
        .input
        .iter()
        .skip(2)
        .any(|txin| !txin.asset_issuance.is_null())
    {
        return Ok(false);
    }

    let Some(dormant_yes_output) = tx.output.first() else {
        return Ok(false);
    };
    let Some(dormant_no_output) = tx.output.get(1) else {
        return Ok(false);
    };

    if dormant_yes_output.script_pubkey != dormant_yes_spk
        || dormant_no_output.script_pubkey != dormant_no_spk
    {
        return Ok(false);
    }

    if !(matches_dormant_creation_output(dormant_yes_output, &dormant_yes_spk, expected_yes)
        || matches_confidential_dormant_creation_output(
            dormant_yes_output,
            &dormant_yes_spk,
            expected_yes,
            parsed_anchor.yes_dormant_opening.asset_blinding_factor,
            parsed_anchor.yes_dormant_opening.value_blinding_factor,
        )?)
        || !(matches_dormant_creation_output(dormant_no_output, &dormant_no_spk, expected_no)
            || matches_confidential_dormant_creation_output(
                dormant_no_output,
                &dormant_no_spk,
                expected_no,
                parsed_anchor.no_dormant_opening.asset_blinding_factor,
                parsed_anchor.no_dormant_opening.value_blinding_factor,
            )?)
    {
        return Ok(false);
    }

    let all_market_spks: Vec<(MarketSlot, Script)> = MarketSlot::ALL
        .into_iter()
        .map(|slot| (slot, compiled.script_pubkey(slot)))
        .collect();

    for (idx, txout) in tx.output.iter().enumerate() {
        if idx == 0 || idx == 1 {
            continue;
        }
        if all_market_spks.iter().any(|(slot, spk)| {
            txout.script_pubkey == *spk
                && !matches!(slot, MarketSlot::DormantYesRt | MarketSlot::DormantNoRt)
        }) {
            return Ok(false);
        }

        if txout.script_pubkey == dormant_yes_spk || txout.script_pubkey == dormant_no_spk {
            return Ok(false);
        }
    }

    Ok(true)
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
enum Bundle {
    Dormant {
        yes: CanonicalMarketUtxo,
        no: CanonicalMarketUtxo,
        last_transition_txid: Txid,
    },
    Unresolved {
        yes: CanonicalMarketUtxo,
        no: CanonicalMarketUtxo,
        collateral: CanonicalMarketUtxo,
        last_transition_txid: Txid,
    },
    ResolvedYes {
        collateral: CanonicalMarketUtxo,
        last_transition_txid: Txid,
    },
    ResolvedNo {
        collateral: CanonicalMarketUtxo,
        last_transition_txid: Txid,
    },
    Expired {
        collateral: CanonicalMarketUtxo,
        last_transition_txid: Txid,
    },
    EmptyTerminal {
        state: MarketState,
        last_transition_txid: Txid,
    },
}

impl Bundle {
    fn into_scan(self) -> CanonicalMarketScan {
        match self {
            Bundle::Dormant {
                yes,
                no,
                last_transition_txid,
            } => CanonicalMarketScan {
                state: MarketState::Dormant,
                utxos: vec![yes, no],
                last_transition_txid,
            },
            Bundle::Unresolved {
                yes,
                no,
                collateral,
                last_transition_txid,
            } => CanonicalMarketScan {
                state: MarketState::Unresolved,
                utxos: vec![yes, no, collateral],
                last_transition_txid,
            },
            Bundle::ResolvedYes {
                collateral,
                last_transition_txid,
            } => CanonicalMarketScan {
                state: MarketState::ResolvedYes,
                utxos: vec![collateral],
                last_transition_txid,
            },
            Bundle::ResolvedNo {
                collateral,
                last_transition_txid,
            } => CanonicalMarketScan {
                state: MarketState::ResolvedNo,
                utxos: vec![collateral],
                last_transition_txid,
            },
            Bundle::Expired {
                collateral,
                last_transition_txid,
            } => CanonicalMarketScan {
                state: MarketState::Expired,
                utxos: vec![collateral],
                last_transition_txid,
            },
            Bundle::EmptyTerminal {
                state,
                last_transition_txid,
            } => CanonicalMarketScan {
                state,
                utxos: Vec::new(),
                last_transition_txid,
            },
        }
    }
}

fn tx_spends_outpoint(tx: &Transaction, outpoint: OutPoint) -> bool {
    tx.input
        .iter()
        .any(|input| input.previous_output == outpoint)
}

fn tx_spends_all(tx: &Transaction, outpoints: &[OutPoint]) -> bool {
    outpoints
        .iter()
        .copied()
        .all(|outpoint| tx_spends_outpoint(tx, outpoint))
}

fn find_slot_outputs(
    tx: &Transaction,
    slot_scripts: &[(MarketSlot, Script)],
) -> Vec<CanonicalMarketUtxo> {
    tx.output
        .iter()
        .enumerate()
        .filter_map(|(vout, txout)| {
            slot_scripts.iter().find_map(|(slot, script)| {
                if txout.script_pubkey == *script {
                    Some(CanonicalMarketUtxo {
                        slot: *slot,
                        outpoint: OutPoint::new(tx.txid(), vout as u32),
                        txout: txout.clone(),
                    })
                } else {
                    None
                }
            })
        })
        .collect()
}

fn expect_exact_slot_set(
    tx: &Transaction,
    slot_scripts: &[(MarketSlot, Script)],
    expected: &[MarketSlot],
) -> std::result::Result<Vec<CanonicalMarketUtxo>, String> {
    let outputs = find_slot_outputs(tx, slot_scripts);
    if outputs.len() != expected.len() {
        return Err(format!(
            "expected {:?} slot outputs, found {:?}",
            expected,
            outputs.iter().map(|o| o.slot).collect::<Vec<_>>()
        ));
    }

    let mut matched = Vec::with_capacity(expected.len());
    for slot in expected {
        let slot_outputs: Vec<CanonicalMarketUtxo> = outputs
            .iter()
            .filter(|output| output.slot == *slot)
            .cloned()
            .collect();
        if slot_outputs.len() != 1 {
            return Err(format!(
                "expected exactly one output for slot {:?}, found {}",
                slot,
                slot_outputs.len()
            ));
        }
        matched.push(slot_outputs[0].clone());
    }
    Ok(matched)
}

fn burn_output_count(tx: &Transaction) -> usize {
    let burn_spk = burn_script_pubkey();
    tx.output
        .iter()
        .filter(|output| output.script_pubkey == burn_spk)
        .count()
}

fn canonical_slot_scripts(contract: &CompiledPredictionMarket) -> Vec<(MarketSlot, Script)> {
    MarketSlot::ALL
        .into_iter()
        .map(|slot| (slot, contract.script_pubkey(slot)))
        .collect()
}

fn resolve_dormant_transition<B: PredictionMarketScanBackend>(
    backend: &B,
    slot_scripts: &[(MarketSlot, Script)],
    yes: CanonicalMarketUtxo,
    no: CanonicalMarketUtxo,
    last_transition_txid: Txid,
) -> std::result::Result<Bundle, String> {
    let yes_spender = backend.spending_txid(&yes.outpoint, &yes.txout.script_pubkey)?;
    let no_spender = backend.spending_txid(&no.outpoint, &no.txout.script_pubkey)?;

    match (yes_spender, no_spender) {
        (None, None) => Ok(Bundle::Dormant {
            yes,
            no,
            last_transition_txid,
        }),
        (Some(a), Some(b)) if a == b => {
            let spend_tx = backend.fetch_transaction(&a)?;
            if !tx_spends_all(&spend_tx, &[yes.outpoint, no.outpoint]) {
                return Err(
                    "dormant transition tx does not spend both canonical RT outpoints".into(),
                );
            }
            if spend_tx.input.len() < 2
                || spend_tx.input[0].asset_issuance.is_null()
                || spend_tx.input[1].asset_issuance.is_null()
            {
                return Err("dormant transition is not a canonical initial issuance".into());
            }
            let outputs = expect_exact_slot_set(
                &spend_tx,
                slot_scripts,
                &[
                    MarketSlot::UnresolvedYesRt,
                    MarketSlot::UnresolvedNoRt,
                    MarketSlot::UnresolvedCollateral,
                ],
            )?;
            Ok(Bundle::Unresolved {
                yes: outputs[0].clone(),
                no: outputs[1].clone(),
                collateral: outputs[2].clone(),
                last_transition_txid: a,
            })
        }
        _ => Err("canonical dormant RT bundle was split across different spender txids".into()),
    }
}

fn resolve_unresolved_transition<B: PredictionMarketScanBackend>(
    backend: &B,
    slot_scripts: &[(MarketSlot, Script)],
    yes: CanonicalMarketUtxo,
    no: CanonicalMarketUtxo,
    collateral: CanonicalMarketUtxo,
    last_transition_txid: Txid,
) -> std::result::Result<Bundle, String> {
    let yes_spender = backend.spending_txid(&yes.outpoint, &yes.txout.script_pubkey)?;
    let no_spender = backend.spending_txid(&no.outpoint, &no.txout.script_pubkey)?;
    let collateral_spender =
        backend.spending_txid(&collateral.outpoint, &collateral.txout.script_pubkey)?;

    match (yes_spender, no_spender, collateral_spender) {
        (None, None, None) => Ok(Bundle::Unresolved {
            yes,
            no,
            collateral,
            last_transition_txid,
        }),
        (None, None, Some(txid)) => {
            let spend_tx = backend.fetch_transaction(&txid)?;
            if !tx_spends_outpoint(&spend_tx, collateral.outpoint) {
                return Err(
                    "partial cancellation tx does not spend canonical collateral outpoint".into(),
                );
            }
            if tx_spends_outpoint(&spend_tx, yes.outpoint)
                || tx_spends_outpoint(&spend_tx, no.outpoint)
            {
                return Err(
                    "partial cancellation tx unexpectedly spends canonical RT outpoints".into(),
                );
            }
            if burn_output_count(&spend_tx) < 2 {
                return Err("partial cancellation tx is missing burn outputs".into());
            }
            let outputs = expect_exact_slot_set(
                &spend_tx,
                slot_scripts,
                &[MarketSlot::UnresolvedCollateral],
            )?;
            Ok(Bundle::Unresolved {
                yes,
                no,
                collateral: outputs[0].clone(),
                last_transition_txid: txid,
            })
        }
        (Some(a), Some(b), Some(c)) if a == b && b == c => {
            let spend_tx = backend.fetch_transaction(&a)?;
            if !tx_spends_all(&spend_tx, &[yes.outpoint, no.outpoint, collateral.outpoint]) {
                return Err(
                    "bundle transition tx does not spend the full canonical unresolved bundle"
                        .into(),
                );
            }
            let slot_outputs = find_slot_outputs(&spend_tx, slot_scripts);
            let slots: Vec<MarketSlot> = slot_outputs.iter().map(|output| output.slot).collect();
            if slots
                == vec![
                    MarketSlot::UnresolvedYesRt,
                    MarketSlot::UnresolvedNoRt,
                    MarketSlot::UnresolvedCollateral,
                ]
                || {
                    let expected = [
                        MarketSlot::UnresolvedYesRt,
                        MarketSlot::UnresolvedNoRt,
                        MarketSlot::UnresolvedCollateral,
                    ];
                    expected.iter().all(|slot| slots.contains(slot))
                        && slots.len() == expected.len()
                }
            {
                if spend_tx.input.len() < 2
                    || spend_tx.input[0].asset_issuance.is_null()
                    || spend_tx.input[1].asset_issuance.is_null()
                {
                    return Err(
                        "unresolved self-transition is not a canonical subsequent issuance".into(),
                    );
                }
                let outputs = expect_exact_slot_set(
                    &spend_tx,
                    slot_scripts,
                    &[
                        MarketSlot::UnresolvedYesRt,
                        MarketSlot::UnresolvedNoRt,
                        MarketSlot::UnresolvedCollateral,
                    ],
                )?;
                return Ok(Bundle::Unresolved {
                    yes: outputs[0].clone(),
                    no: outputs[1].clone(),
                    collateral: outputs[2].clone(),
                    last_transition_txid: a,
                });
            }
            if burn_output_count(&spend_tx) >= 2 {
                if slots.len() == 2
                    && slots.contains(&MarketSlot::DormantYesRt)
                    && slots.contains(&MarketSlot::DormantNoRt)
                {
                    let outputs = expect_exact_slot_set(
                        &spend_tx,
                        slot_scripts,
                        &[MarketSlot::DormantYesRt, MarketSlot::DormantNoRt],
                    )?;
                    return Ok(Bundle::Dormant {
                        yes: outputs[0].clone(),
                        no: outputs[1].clone(),
                        last_transition_txid: a,
                    });
                }
                if slots == vec![MarketSlot::ResolvedYesCollateral]
                    || (slots.len() == 1 && slots[0] == MarketSlot::ResolvedYesCollateral)
                {
                    let outputs = expect_exact_slot_set(
                        &spend_tx,
                        slot_scripts,
                        &[MarketSlot::ResolvedYesCollateral],
                    )?;
                    return Ok(Bundle::ResolvedYes {
                        collateral: outputs[0].clone(),
                        last_transition_txid: a,
                    });
                }
                if slots == vec![MarketSlot::ResolvedNoCollateral]
                    || (slots.len() == 1 && slots[0] == MarketSlot::ResolvedNoCollateral)
                {
                    let outputs = expect_exact_slot_set(
                        &spend_tx,
                        slot_scripts,
                        &[MarketSlot::ResolvedNoCollateral],
                    )?;
                    return Ok(Bundle::ResolvedNo {
                        collateral: outputs[0].clone(),
                        last_transition_txid: a,
                    });
                }
                if slots == vec![MarketSlot::ExpiredCollateral]
                    || (slots.len() == 1 && slots[0] == MarketSlot::ExpiredCollateral)
                {
                    let outputs = expect_exact_slot_set(
                        &spend_tx,
                        slot_scripts,
                        &[MarketSlot::ExpiredCollateral],
                    )?;
                    return Ok(Bundle::Expired {
                        collateral: outputs[0].clone(),
                        last_transition_txid: a,
                    });
                }
            }
            Err(format!(
                "canonical unresolved bundle spent by unrecognized transition with slot outputs {:?}",
                slots
            ))
        }
        _ => Err("canonical unresolved bundle was split across different spender txids".into()),
    }
}

fn resolve_terminal_transition<B: PredictionMarketScanBackend>(
    backend: &B,
    slot_scripts: &[(MarketSlot, Script)],
    state: MarketState,
    collateral: CanonicalMarketUtxo,
    last_transition_txid: Txid,
) -> std::result::Result<Bundle, String> {
    let spender = backend.spending_txid(&collateral.outpoint, &collateral.txout.script_pubkey)?;
    let current_slot = collateral.slot;
    match spender {
        None => match state {
            MarketState::ResolvedYes => Ok(Bundle::ResolvedYes {
                collateral,
                last_transition_txid,
            }),
            MarketState::ResolvedNo => Ok(Bundle::ResolvedNo {
                collateral,
                last_transition_txid,
            }),
            MarketState::Expired => Ok(Bundle::Expired {
                collateral,
                last_transition_txid,
            }),
            _ => Err("invalid terminal state".into()),
        },
        Some(txid) => {
            let spend_tx = backend.fetch_transaction(&txid)?;
            if !tx_spends_outpoint(&spend_tx, collateral.outpoint) {
                return Err(
                    "terminal transition tx does not spend canonical collateral outpoint".into(),
                );
            }
            if burn_output_count(&spend_tx) < 1 {
                return Err("terminal redemption tx is missing burn output".into());
            }
            let slot_outputs = find_slot_outputs(&spend_tx, slot_scripts);
            if slot_outputs.is_empty() {
                return Ok(Bundle::EmptyTerminal {
                    state,
                    last_transition_txid: txid,
                });
            }
            if slot_outputs.len() != 1 || slot_outputs[0].slot != current_slot {
                return Err(format!(
                    "terminal transition produced invalid slot outputs {:?}",
                    slot_outputs
                        .iter()
                        .map(|output| output.slot)
                        .collect::<Vec<_>>()
                ));
            }
            match state {
                MarketState::ResolvedYes => Ok(Bundle::ResolvedYes {
                    collateral: slot_outputs[0].clone(),
                    last_transition_txid: txid,
                }),
                MarketState::ResolvedNo => Ok(Bundle::ResolvedNo {
                    collateral: slot_outputs[0].clone(),
                    last_transition_txid: txid,
                }),
                MarketState::Expired => Ok(Bundle::Expired {
                    collateral: slot_outputs[0].clone(),
                    last_transition_txid: txid,
                }),
                _ => Err("invalid terminal state".into()),
            }
        }
    }
}

#[doc(hidden)]
pub fn scan_prediction_market_canonical<B: PredictionMarketScanBackend>(
    backend: &B,
    params: &PredictionMarketParams,
    anchor: &PredictionMarketAnchor,
) -> std::result::Result<CanonicalMarketScan, String> {
    let parsed_anchor = parse_prediction_market_anchor(anchor)?;
    let creation_txid = parsed_anchor.creation_txid;
    let creation_tx = backend.fetch_transaction(&creation_txid)?;
    if !validate_prediction_market_creation_tx(params, &creation_tx, anchor)? {
        return Err(format!(
            "transaction {} is not a canonical prediction-market creation bootstrap",
            creation_txid
        ));
    }

    let contract = CompiledPredictionMarket::new(*params).map_err(|e| e.to_string())?;
    let slot_scripts = canonical_slot_scripts(&contract);

    let yes = creation_tx
        .output
        .first()
        .ok_or_else(|| "creation tx missing dormant YES output".to_string())?
        .clone();
    let no = creation_tx
        .output
        .get(1)
        .ok_or_else(|| "creation tx missing dormant NO output".to_string())?
        .clone();

    let mut bundle = Bundle::Dormant {
        yes: CanonicalMarketUtxo {
            slot: MarketSlot::DormantYesRt,
            outpoint: OutPoint::new(creation_txid, 0),
            txout: yes,
        },
        no: CanonicalMarketUtxo {
            slot: MarketSlot::DormantNoRt,
            outpoint: OutPoint::new(creation_txid, 1),
            txout: no,
        },
        last_transition_txid: creation_txid,
    };

    loop {
        bundle = match bundle {
            Bundle::Dormant {
                yes,
                no,
                last_transition_txid,
            } => resolve_dormant_transition(backend, &slot_scripts, yes, no, last_transition_txid)?,
            Bundle::Unresolved {
                yes,
                no,
                collateral,
                last_transition_txid,
            } => resolve_unresolved_transition(
                backend,
                &slot_scripts,
                yes,
                no,
                collateral,
                last_transition_txid,
            )?,
            Bundle::ResolvedYes {
                collateral,
                last_transition_txid,
            } => resolve_terminal_transition(
                backend,
                &slot_scripts,
                MarketState::ResolvedYes,
                collateral,
                last_transition_txid,
            )?,
            Bundle::ResolvedNo {
                collateral,
                last_transition_txid,
            } => resolve_terminal_transition(
                backend,
                &slot_scripts,
                MarketState::ResolvedNo,
                collateral,
                last_transition_txid,
            )?,
            Bundle::Expired {
                collateral,
                last_transition_txid,
            } => resolve_terminal_transition(
                backend,
                &slot_scripts,
                MarketState::Expired,
                collateral,
                last_transition_txid,
            )?,
            Bundle::EmptyTerminal { .. } => return Ok(bundle.into_scan()),
        };

        match &bundle {
            Bundle::Dormant { yes, no, .. } => {
                let yes_spent = backend.spending_txid(&yes.outpoint, &yes.txout.script_pubkey)?;
                let no_spent = backend.spending_txid(&no.outpoint, &no.txout.script_pubkey)?;
                if yes_spent.is_none() && no_spent.is_none() {
                    return Ok(bundle.into_scan());
                }
            }
            Bundle::Unresolved {
                yes,
                no,
                collateral,
                ..
            } => {
                let yes_spent = backend.spending_txid(&yes.outpoint, &yes.txout.script_pubkey)?;
                let no_spent = backend.spending_txid(&no.outpoint, &no.txout.script_pubkey)?;
                let collateral_spent =
                    backend.spending_txid(&collateral.outpoint, &collateral.txout.script_pubkey)?;
                if yes_spent.is_none() && no_spent.is_none() && collateral_spent.is_none() {
                    return Ok(bundle.into_scan());
                }
            }
            Bundle::ResolvedYes { collateral, .. }
            | Bundle::ResolvedNo { collateral, .. }
            | Bundle::Expired { collateral, .. } => {
                let spent =
                    backend.spending_txid(&collateral.outpoint, &collateral.txout.script_pubkey)?;
                if spent.is_none() {
                    return Ok(bundle.into_scan());
                }
            }
            Bundle::EmptyTerminal { .. } => return Ok(bundle.into_scan()),
        }
    }
}
