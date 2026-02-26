use simplicityhl::elements::Script;
use simplicityhl::elements::pset::PartiallySignedTransaction;

use crate::error::{Error, Result};
use crate::prediction_market::contract::CompiledPredictionMarket;
use crate::prediction_market::state::MarketState;

use super::{
    UnblindedUtxo, add_pset_input, add_pset_output, burn_txout, covenant_spk, explicit_txout,
    fee_txout, new_pset,
};

/// Parameters for constructing a post-resolution redemption PSET.
pub struct PostResolutionRedemptionParams {
    pub collateral_utxo: UnblindedUtxo,
    pub token_utxos: Vec<UnblindedUtxo>,
    pub fee_utxo: UnblindedUtxo,
    pub tokens_burned: u64,
    pub resolved_state: MarketState,
    pub fee_amount: u64,
    pub payout_destination: Script,
    pub fee_change_destination: Option<Script>,
    /// Where to send excess tokens if token UTXOs hold more than `tokens_burned`.
    pub token_change_destination: Option<Script>,
}

/// Build the post-resolution redemption PSET (state 2/3).
/// Burns winning tokens from a ResolvedYes or ResolvedNo state.
pub fn build_post_resolution_redemption_pset(
    contract: &CompiledPredictionMarket,
    params: &PostResolutionRedemptionParams,
) -> Result<PartiallySignedTransaction> {
    if !params.resolved_state.is_resolved() {
        return Err(Error::InvalidState);
    }

    let cpt = contract.params().collateral_per_token;
    let payout = params
        .tokens_burned
        .checked_mul(2)
        .and_then(|v| v.checked_mul(cpt))
        .ok_or(Error::CollateralOverflow)?;

    if params.collateral_utxo.value < payout {
        return Err(Error::InsufficientCollateral);
    }

    let remaining = params.collateral_utxo.value - payout;
    let winning_asset = params
        .resolved_state
        .winning_token_asset(contract.params())
        .ok_or(Error::InvalidState)?;

    let state_spk = covenant_spk(contract, params.resolved_state);

    let mut pset = new_pset();

    add_pset_input(&mut pset, &params.collateral_utxo);
    for utxo in &params.token_utxos {
        add_pset_input(&mut pset, utxo);
    }
    add_pset_input(&mut pset, &params.fee_utxo);

    if remaining > 0 {
        add_pset_output(
            &mut pset,
            explicit_txout(
                &contract.params().collateral_asset_id,
                remaining,
                &state_spk,
            ),
        );
    }
    add_pset_output(&mut pset, burn_txout(&winning_asset, params.tokens_burned));
    add_pset_output(
        &mut pset,
        explicit_txout(
            &contract.params().collateral_asset_id,
            payout,
            &params.payout_destination,
        ),
    );
    // Token change output (if UTXOs hold more than tokens_burned)
    if let Some(ref change_spk) = params.token_change_destination {
        let total: u64 = params.token_utxos.iter().map(|u| u.value).sum();
        let change = total.saturating_sub(params.tokens_burned);
        if change > 0 {
            add_pset_output(
                &mut pset,
                explicit_txout(&winning_asset, change, change_spk),
            );
        }
    }
    add_pset_output(
        &mut pset,
        fee_txout(&contract.params().collateral_asset_id, params.fee_amount),
    );

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

    Ok(pset)
}
