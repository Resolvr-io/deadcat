use simplicityhl::elements::LockTime;
use simplicityhl::elements::pset::PartiallySignedTransaction;

use crate::error::Result;
use crate::prediction_market::contract::CompiledPredictionMarket;
use crate::prediction_market::state::MarketState;

use super::{
    UnblindedUtxo, add_pset_input, add_pset_output, covenant_spk, explicit_txout, fee_txout,
    new_pset, reissuance_token_output,
};

/// Parameters for constructing an expire-transition PSET (state 1 -> 4).
pub struct ExpireTransitionParams {
    pub yes_reissuance_utxo: UnblindedUtxo,
    pub no_reissuance_utxo: UnblindedUtxo,
    pub collateral_utxo: UnblindedUtxo,
    pub fee_utxo: UnblindedUtxo,
    pub fee_amount: u64,
    pub lock_time: u32,
}

/// Build the expire-transition PSET (state 1 -> 4).
pub fn build_expire_transition_pset(
    contract: &CompiledPredictionMarket,
    params: &ExpireTransitionParams,
) -> Result<PartiallySignedTransaction> {
    let expired_spk = covenant_spk(contract, MarketState::Expired);

    let mut pset = new_pset();

    add_pset_input(&mut pset, &params.yes_reissuance_utxo);
    add_pset_input(&mut pset, &params.no_reissuance_utxo);
    add_pset_input(&mut pset, &params.collateral_utxo);
    add_pset_input(&mut pset, &params.fee_utxo);

    add_pset_output(&mut pset, reissuance_token_output(&expired_spk));
    add_pset_output(&mut pset, reissuance_token_output(&expired_spk));
    add_pset_output(
        &mut pset,
        explicit_txout(
            &contract.params().collateral_asset_id,
            params.collateral_utxo.value,
            &expired_spk,
        ),
    );
    add_pset_output(
        &mut pset,
        fee_txout(&contract.params().collateral_asset_id, params.fee_amount),
    );

    pset.global.tx_data.fallback_locktime = Some(LockTime::from_consensus(params.lock_time));

    Ok(pset)
}
