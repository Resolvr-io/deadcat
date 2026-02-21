use simplicityhl::elements::pset::PartiallySignedTransaction;
use simplicityhl::elements::{LockTime, Script};

use crate::contract::CompiledContract;
use crate::error::{Error, Result};
use crate::state::MarketState;

use super::{
    UnblindedUtxo, add_pset_input, add_pset_output, burn_txout, covenant_spk, explicit_txout,
    fee_txout, new_pset,
};

/// Parameters for constructing an expiry redemption PSET.
pub struct ExpiryRedemptionParams {
    pub collateral_utxo: UnblindedUtxo,
    pub token_utxos: Vec<UnblindedUtxo>,
    pub fee_utxo: UnblindedUtxo,
    pub tokens_burned: u64,
    pub burn_token_asset: [u8; 32],
    pub fee_amount: u64,
    pub payout_destination: Script,
    pub fee_change_destination: Option<Script>,
    /// Where to send excess tokens if token UTXOs hold more than `tokens_burned`.
    pub token_change_destination: Option<Script>,
    pub lock_time: u32,
}

/// Build the expiry redemption PSET (state 1, post-expiry).
/// Burns tokens from the Unresolved state after expiry block height.
pub fn build_expiry_redemption_pset(
    contract: &CompiledContract,
    params: &ExpiryRedemptionParams,
) -> Result<PartiallySignedTransaction> {
    let cpt = contract.params().collateral_per_token;
    let payout = params
        .tokens_burned
        .checked_mul(cpt)
        .ok_or(Error::CollateralOverflow)?;

    if params.collateral_utxo.value < payout {
        return Err(Error::InsufficientCollateral);
    }

    let remaining = params.collateral_utxo.value - payout;
    let state1_spk = covenant_spk(contract, MarketState::Unresolved);

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
                &state1_spk,
            ),
        );
    }
    add_pset_output(
        &mut pset,
        burn_txout(&params.burn_token_asset, params.tokens_burned),
    );
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
                explicit_txout(&params.burn_token_asset, change, change_spk),
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

    pset.global.tx_data.fallback_locktime = Some(LockTime::from_consensus(params.lock_time));

    Ok(pset)
}
