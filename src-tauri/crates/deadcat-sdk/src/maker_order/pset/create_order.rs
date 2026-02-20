use simplicityhl::elements::Script;
use simplicityhl::elements::pset::PartiallySignedTransaction;

use crate::error::{Error, Result};
use crate::maker_order::contract::CompiledMakerOrder;
use crate::maker_order::params::OrderDirection;
use crate::pset::{
    UnblindedUtxo, add_pset_input, add_pset_output, explicit_txout, fee_txout, new_pset,
};

/// Parameters for constructing a create-order PSET.
///
/// The maker funds a covenant-locked UTXO with the asset they are offering.
/// For SellBase: the funding input holds BASE tokens.
/// For SellQuote: the funding input holds QUOTE (e.g. L-BTC).
pub struct CreateOrderParams {
    /// The UTXO providing the offered asset (BASE or QUOTE depending on direction).
    pub funding_utxo: UnblindedUtxo,
    /// The UTXO providing the fee (must be the fee asset, typically L-BTC).
    pub fee_utxo: UnblindedUtxo,
    /// Amount of offered asset to lock in the order.
    pub order_amount: u64,
    /// Fee amount in sats.
    pub fee_amount: u64,
    /// The fee asset ID (typically L-BTC).
    pub fee_asset_id: [u8; 32],
    /// Where to send change from the funding UTXO (if any).
    pub change_destination: Option<Script>,
    /// Where to send fee change (if any).
    pub fee_change_destination: Option<Script>,
    /// The maker's base pubkey (used to derive the covenant address).
    pub maker_base_pubkey: [u8; 32],
}

/// Build the create-order PSET.
///
/// ```text
/// Inputs:  [0] funding input (offered asset)
///          [1] fee input
/// Outputs: [0] order UTXO -> covenant address
///          [1] fee output
///          [2] change (optional)
///          [3] fee change (optional)
/// ```
pub fn build_create_order_pset(
    contract: &CompiledMakerOrder,
    params: &CreateOrderParams,
) -> Result<PartiallySignedTransaction> {
    if contract.params().price == 0 {
        return Err(Error::ZeroPrice);
    }

    if params.order_amount == 0 {
        return Err(Error::ZeroOrderAmount);
    }

    if params.funding_utxo.value < params.order_amount {
        return Err(Error::InsufficientCollateral);
    }

    if params.fee_utxo.value < params.fee_amount {
        return Err(Error::InsufficientFee);
    }

    if params.funding_utxo.value > params.order_amount && params.change_destination.is_none() {
        return Err(Error::MissingChangeDestination);
    }

    if params.fee_utxo.value > params.fee_amount && params.fee_change_destination.is_none() {
        return Err(Error::MissingChangeDestination);
    }

    let mut pset = new_pset();

    // Determine the offered asset based on direction
    let offered_asset = match contract.params().direction {
        OrderDirection::SellBase => &contract.params().base_asset_id,
        OrderDirection::SellQuote => &contract.params().quote_asset_id,
    };

    let covenant_spk = contract.script_pubkey(&params.maker_base_pubkey);

    // Input 0: funding
    add_pset_input(&mut pset, &params.funding_utxo);
    // Input 1: fee
    add_pset_input(&mut pset, &params.fee_utxo);

    // Output 0: order UTXO -> covenant address
    add_pset_output(
        &mut pset,
        explicit_txout(offered_asset, params.order_amount, &covenant_spk),
    );

    // Output 1: fee
    add_pset_output(
        &mut pset,
        fee_txout(&params.fee_asset_id, params.fee_amount),
    );

    // Output 2: change from funding (optional)
    let funding_change = params.funding_utxo.value - params.order_amount;
    if funding_change > 0
        && let Some(ref change_spk) = params.change_destination
    {
        add_pset_output(
            &mut pset,
            explicit_txout(offered_asset, funding_change, change_spk),
        );
    }

    // Output 3: fee change (optional)
    let fee_change = params.fee_utxo.value - params.fee_amount;
    if fee_change > 0
        && let Some(ref change_spk) = params.fee_change_destination
    {
        add_pset_output(
            &mut pset,
            explicit_txout(&params.fee_asset_id, fee_change, change_spk),
        );
    }

    Ok(pset)
}
