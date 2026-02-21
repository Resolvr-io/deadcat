use simplicityhl::elements::Script;
use simplicityhl::elements::pset::PartiallySignedTransaction;

use crate::contract::CompiledContract;
use crate::error::{Error, Result};
use crate::state::MarketState;

use super::{
    UnblindedUtxo, add_pset_input, add_pset_output, burn_txout, covenant_spk, explicit_txout,
    fee_txout, new_pset, reissuance_token_output,
};

/// Parameters for constructing a cancellation PSET.
///
/// For partial cancellation (remaining > 0): reissuance UTXOs are ignored.
/// For full cancellation (remaining == 0): reissuance UTXOs are required to
/// cycle reissuance tokens back to the Dormant state (1 → 0).
pub struct CancellationParams {
    pub collateral_utxo: UnblindedUtxo,
    pub yes_reissuance_utxo: Option<UnblindedUtxo>,
    pub no_reissuance_utxo: Option<UnblindedUtxo>,
    pub yes_token_utxos: Vec<UnblindedUtxo>,
    pub no_token_utxos: Vec<UnblindedUtxo>,
    pub fee_utxo: UnblindedUtxo,
    pub pairs_burned: u64,
    pub fee_amount: u64,
    pub refund_destination: Script,
    pub fee_change_destination: Option<Script>,
    /// Where to send excess tokens if token UTXOs hold more than `pairs_burned`.
    pub token_change_destination: Option<Script>,
}

/// Build the cancellation PSET (state 1 → 1 partial, 1 → 0 full).
pub fn build_cancellation_pset(
    contract: &CompiledContract,
    params: &CancellationParams,
) -> Result<PartiallySignedTransaction> {
    let cpt = contract.params().collateral_per_token;
    let refund = params
        .pairs_burned
        .checked_mul(2)
        .and_then(|v| v.checked_mul(cpt))
        .ok_or(Error::CollateralOverflow)?;

    if params.collateral_utxo.value < refund {
        return Err(Error::InsufficientCollateral);
    }

    let remaining = params.collateral_utxo.value - refund;

    let mut pset = new_pset();

    if remaining > 0 {
        // Partial cancellation (1 → 1)
        let unresolved_spk = covenant_spk(contract, MarketState::Unresolved);

        add_pset_input(&mut pset, &params.collateral_utxo);
        for utxo in &params.yes_token_utxos {
            add_pset_input(&mut pset, utxo);
        }
        for utxo in &params.no_token_utxos {
            add_pset_input(&mut pset, utxo);
        }
        add_pset_input(&mut pset, &params.fee_utxo);

        add_pset_output(
            &mut pset,
            explicit_txout(
                &contract.params().collateral_asset_id,
                remaining,
                &unresolved_spk,
            ),
        );
        add_pset_output(
            &mut pset,
            burn_txout(&contract.params().yes_token_asset, params.pairs_burned),
        );
        add_pset_output(
            &mut pset,
            burn_txout(&contract.params().no_token_asset, params.pairs_burned),
        );
        add_pset_output(
            &mut pset,
            explicit_txout(
                &contract.params().collateral_asset_id,
                refund,
                &params.refund_destination,
            ),
        );
        // Token change outputs (if UTXOs hold more than pairs_burned)
        if let Some(ref change_spk) = params.token_change_destination {
            let yes_total: u64 = params.yes_token_utxos.iter().map(|u| u.value).sum();
            let yes_change = yes_total.saturating_sub(params.pairs_burned);
            if yes_change > 0 {
                add_pset_output(
                    &mut pset,
                    explicit_txout(&contract.params().yes_token_asset, yes_change, change_spk),
                );
            }
            let no_total: u64 = params.no_token_utxos.iter().map(|u| u.value).sum();
            let no_change = no_total.saturating_sub(params.pairs_burned);
            if no_change > 0 {
                add_pset_output(
                    &mut pset,
                    explicit_txout(&contract.params().no_token_asset, no_change, change_spk),
                );
            }
        }
        add_pset_output(
            &mut pset,
            fee_txout(&contract.params().collateral_asset_id, params.fee_amount),
        );
    } else {
        // Full cancellation (1 → 0): cycle reissuance tokens to Dormant
        let yes_reissuance = params
            .yes_reissuance_utxo
            .as_ref()
            .ok_or(Error::MissingReissuanceUtxos)?;
        let no_reissuance = params
            .no_reissuance_utxo
            .as_ref()
            .ok_or(Error::MissingReissuanceUtxos)?;

        let dormant_spk = covenant_spk(contract, MarketState::Dormant);

        // Input 0: collateral
        add_pset_input(&mut pset, &params.collateral_utxo);
        // Input 1: YES reissuance token
        add_pset_input(&mut pset, yes_reissuance);
        // Input 2: NO reissuance token
        add_pset_input(&mut pset, no_reissuance);
        // Inputs 3+: YES tokens
        for utxo in &params.yes_token_utxos {
            add_pset_input(&mut pset, utxo);
        }
        // Inputs: NO tokens
        for utxo in &params.no_token_utxos {
            add_pset_input(&mut pset, utxo);
        }
        // Input: fee
        add_pset_input(&mut pset, &params.fee_utxo);

        // Output 0: YES reissuance token → Dormant
        add_pset_output(&mut pset, reissuance_token_output(&dormant_spk));
        // Output 1: NO reissuance token → Dormant
        add_pset_output(&mut pset, reissuance_token_output(&dormant_spk));
        // Output 2: YES token burn
        add_pset_output(
            &mut pset,
            burn_txout(&contract.params().yes_token_asset, params.pairs_burned),
        );
        // Output 3: NO token burn
        add_pset_output(
            &mut pset,
            burn_txout(&contract.params().no_token_asset, params.pairs_burned),
        );
        // Output 4: refund
        add_pset_output(
            &mut pset,
            explicit_txout(
                &contract.params().collateral_asset_id,
                refund,
                &params.refund_destination,
            ),
        );
        // Output 5: fee
        add_pset_output(
            &mut pset,
            fee_txout(&contract.params().collateral_asset_id, params.fee_amount),
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

    Ok(pset)
}
