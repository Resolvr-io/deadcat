use simplicityhl::elements::pset::PartiallySignedTransaction;
use simplicityhl::elements::{LockTime, Script};

use crate::error::Result;
use crate::prediction_market::contract::CompiledPredictionMarket;
use crate::prediction_market::state::MarketSlot;

use super::{
    UnblindedUtxo, add_pset_input, add_pset_output, burn_txout, covenant_spk, explicit_txout,
    fee_txout, new_pset,
};

/// Parameters for constructing an expire-transition PSET (state 1 -> 4).
pub struct ExpireTransitionParams {
    pub yes_reissuance_utxo: UnblindedUtxo,
    pub no_reissuance_utxo: UnblindedUtxo,
    pub collateral_utxo: UnblindedUtxo,
    pub fee_utxo: UnblindedUtxo,
    pub fee_amount: u64,
    pub fee_change_destination: Option<Script>,
    pub lock_time: u32,
}

/// Build the expire-transition PSET (state 1 -> 4).
///
/// Outputs 0-2 are fixed by the covenant: YES RT burn, NO RT burn, and expired
/// collateral. An optional fee-change output may appear before the final fee output.
pub fn build_expire_transition_pset(
    contract: &CompiledPredictionMarket,
    params: &ExpireTransitionParams,
) -> Result<PartiallySignedTransaction> {
    let expired_spk = covenant_spk(contract, MarketSlot::ExpiredCollateral);

    let mut pset = new_pset();

    add_pset_input(&mut pset, &params.yes_reissuance_utxo);
    add_pset_input(&mut pset, &params.no_reissuance_utxo);
    add_pset_input(&mut pset, &params.collateral_utxo);
    add_pset_input(&mut pset, &params.fee_utxo);

    add_pset_output(
        &mut pset,
        burn_txout(&contract.params().yes_reissuance_token, 1),
    );
    add_pset_output(
        &mut pset,
        burn_txout(&contract.params().no_reissuance_token, 1),
    );
    add_pset_output(
        &mut pset,
        explicit_txout(
            &contract.params().collateral_asset_id,
            params.collateral_utxo.value,
            &expired_spk,
        ),
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

    add_pset_output(
        &mut pset,
        fee_txout(&contract.params().collateral_asset_id, params.fee_amount),
    );

    pset.global.tx_data.fallback_locktime = Some(LockTime::from_consensus(params.lock_time));

    Ok(pset)
}
