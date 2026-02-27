use simplicityhl::elements::pset::PartiallySignedTransaction;
use simplicityhl::elements::secp256k1_zkp::Tweak;
use simplicityhl::elements::{LockTime, Script};

use crate::error::{Error, Result};
use crate::prediction_market::contract::CompiledPredictionMarket;
use crate::prediction_market::state::MarketState;

use super::{
    UnblindedUtxo, add_pset_input, add_pset_output, covenant_spk, explicit_txout, fee_txout,
    new_pset, reissuance_token_output,
};

/// Parameters for constructing a subsequent issuance PSET (state 1 → 1).
pub struct SubsequentIssuanceParams {
    pub yes_reissuance_utxo: UnblindedUtxo,
    pub no_reissuance_utxo: UnblindedUtxo,
    pub collateral_utxo: UnblindedUtxo,
    pub new_collateral_utxo: UnblindedUtxo,
    pub fee_utxo: UnblindedUtxo,
    pub pairs: u64,
    pub fee_amount: u64,
    pub yes_token_destination: Script,
    pub no_token_destination: Script,
    pub collateral_change_destination: Option<Script>,
    pub fee_change_destination: Option<Script>,
    pub yes_issuance_blinding_nonce: [u8; 32],
    pub yes_issuance_asset_entropy: [u8; 32],
    pub no_issuance_blinding_nonce: [u8; 32],
    pub no_issuance_asset_entropy: [u8; 32],
    pub lock_time: u32,
}

/// Build the subsequent issuance PSET (state 1 → 1).
pub fn build_subsequent_issuance_pset(
    contract: &CompiledPredictionMarket,
    params: &SubsequentIssuanceParams,
) -> Result<PartiallySignedTransaction> {
    let cpt = contract.params().collateral_per_token;
    let new_collateral = params
        .pairs
        .checked_mul(2)
        .and_then(|v| v.checked_mul(cpt))
        .ok_or(Error::CollateralOverflow)?;

    let total_collateral = params
        .collateral_utxo
        .value
        .checked_add(new_collateral)
        .ok_or(Error::CollateralOverflow)?;

    if params.new_collateral_utxo.value < new_collateral {
        return Err(Error::InsufficientCollateral);
    }

    let mut pset = new_pset();
    let unresolved_spk = covenant_spk(contract, MarketState::Unresolved);

    add_pset_input(&mut pset, &params.yes_reissuance_utxo);
    add_pset_input(&mut pset, &params.no_reissuance_utxo);
    add_pset_input(&mut pset, &params.collateral_utxo);
    add_pset_input(&mut pset, &params.new_collateral_utxo);
    add_pset_input(&mut pset, &params.fee_utxo);

    // Mark inputs 0 and 1 as reissuance
    if let Some(input) = pset.inputs_mut().get_mut(0) {
        input.issuance_value_amount = Some(params.pairs);
        input.issuance_blinding_nonce = Some(
            Tweak::from_slice(&params.yes_issuance_blinding_nonce)
                .expect("valid yes issuance blinding nonce"),
        );
        input.issuance_asset_entropy = Some(params.yes_issuance_asset_entropy);
    }
    if let Some(input) = pset.inputs_mut().get_mut(1) {
        input.issuance_value_amount = Some(params.pairs);
        input.issuance_blinding_nonce = Some(
            Tweak::from_slice(&params.no_issuance_blinding_nonce)
                .expect("valid no issuance blinding nonce"),
        );
        input.issuance_asset_entropy = Some(params.no_issuance_asset_entropy);
    }

    add_pset_output(&mut pset, reissuance_token_output(&unresolved_spk));
    add_pset_output(&mut pset, reissuance_token_output(&unresolved_spk));
    add_pset_output(
        &mut pset,
        explicit_txout(
            &contract.params().collateral_asset_id,
            total_collateral,
            &unresolved_spk,
        ),
    );
    add_pset_output(
        &mut pset,
        explicit_txout(
            &contract.params().yes_token_asset,
            params.pairs,
            &params.yes_token_destination,
        ),
    );
    add_pset_output(
        &mut pset,
        explicit_txout(
            &contract.params().no_token_asset,
            params.pairs,
            &params.no_token_destination,
        ),
    );
    add_pset_output(
        &mut pset,
        fee_txout(&contract.params().collateral_asset_id, params.fee_amount),
    );

    let change = params.new_collateral_utxo.value - new_collateral;
    if change > 0
        && let Some(ref change_spk) = params.collateral_change_destination
    {
        add_pset_output(
            &mut pset,
            explicit_txout(&contract.params().collateral_asset_id, change, change_spk),
        );
    }

    let fee_change = params.fee_utxo.value.saturating_sub(params.fee_amount);
    if fee_change > 0
        && let Some(ref change_spk) = params.fee_change_destination
    {
        add_pset_output(
            &mut pset,
            explicit_txout(
                &contract.params().collateral_asset_id,
                fee_change,
                change_spk,
            ),
        );
    }

    pset.global.tx_data.fallback_locktime = Some(LockTime::from_consensus(params.lock_time));

    Ok(pset)
}
