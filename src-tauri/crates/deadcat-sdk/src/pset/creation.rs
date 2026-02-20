use simplicityhl::elements::pset::PartiallySignedTransaction;
use simplicityhl::elements::{LockTime, Script};

use crate::contract::CompiledContract;
use crate::error::{Error, Result};
use crate::state::MarketState;

use super::{
    UnblindedUtxo, add_pset_input, add_pset_output, explicit_txout, fee_txout, new_pset,
    reissuance_token_output,
};

/// Parameters for constructing a market creation PSET.
///
/// This builds a plain Elements transaction (no covenant input) that performs
/// the initial issuance of reissuance tokens and deposits them to the Dormant
/// covenant address. No tokens are minted and no collateral is deposited.
///
/// Both defining UTXOs are assumed to be L-BTC. Their combined value covers
/// the fee; any remainder goes to change.
pub struct CreationParams {
    pub yes_defining_utxo: UnblindedUtxo,
    pub no_defining_utxo: UnblindedUtxo,
    pub fee_amount: u64,
    pub change_destination: Option<Script>,
    pub lock_time: u32,
}

/// Build the market creation PSET.
///
/// This is a plain Elements transaction — no covenant input, no Simplicity
/// validation. It issues reissuance tokens only (no YES/NO tokens, no collateral)
/// and deposits them to the Dormant covenant address.
///
/// Input 0: YES defining UTXO (with issuance — token only, no value)
/// Input 1: NO defining UTXO (with issuance — token only, no value)
///
/// Outputs: reissuance tokens → Dormant covenant + fee + optional change
pub fn build_creation_pset(
    contract: &CompiledContract,
    params: &CreationParams,
) -> Result<PartiallySignedTransaction> {
    let combined_value = params
        .yes_defining_utxo
        .value
        .checked_add(params.no_defining_utxo.value)
        .ok_or(Error::CollateralOverflow)?;

    if combined_value < params.fee_amount {
        return Err(Error::InsufficientFee);
    }

    let mut pset = new_pset();
    let dormant_spk = super::covenant_spk(contract, MarketState::Dormant);

    // Input 0: YES defining UTXO (with issuance)
    add_pset_input(&mut pset, &params.yes_defining_utxo);
    // Input 1: NO defining UTXO (with issuance)
    add_pset_input(&mut pset, &params.no_defining_utxo);

    // Mark inputs 0 and 1 as issuance (reissuance token only, no asset value)
    if let Some(input) = pset.inputs_mut().get_mut(0) {
        input.issuance_inflation_keys = Some(1);
    }
    if let Some(input) = pset.inputs_mut().get_mut(1) {
        input.issuance_inflation_keys = Some(1);
    }

    // Output 0: YES reissuance token → Dormant covenant
    add_pset_output(&mut pset, reissuance_token_output(&dormant_spk));
    // Output 1: NO reissuance token → Dormant covenant
    add_pset_output(&mut pset, reissuance_token_output(&dormant_spk));
    // Output 2: fee
    add_pset_output(
        &mut pset,
        fee_txout(&contract.params().collateral_asset_id, params.fee_amount),
    );

    // Change from defining UTXOs
    let change = combined_value - params.fee_amount;
    if change > 0
        && let Some(ref change_spk) = params.change_destination
    {
        add_pset_output(
            &mut pset,
            explicit_txout(&contract.params().collateral_asset_id, change, change_spk),
        );
    }

    pset.global.tx_data.fallback_locktime = Some(LockTime::from_consensus(params.lock_time));

    Ok(pset)
}
