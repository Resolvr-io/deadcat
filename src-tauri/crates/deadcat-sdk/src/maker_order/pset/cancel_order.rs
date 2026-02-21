use simplicityhl::elements::Script;
use simplicityhl::elements::pset::PartiallySignedTransaction;

use crate::error::{Error, Result};
use crate::pset::{
    UnblindedUtxo, add_pset_input, add_pset_output, explicit_txout, fee_txout, new_pset,
};

/// Parameters for constructing a cancel-order PSET.
///
/// Cancellation is a key-path spend — the maker signs with their tweaked key.
/// No covenant code executes. No cosigner is required.
pub struct CancelOrderParams {
    /// The order UTXO to cancel (key-path spend).
    pub order_utxo: UnblindedUtxo,
    /// The fee UTXO.
    pub fee_utxo: UnblindedUtxo,
    /// Fee amount in sats.
    pub fee_amount: u64,
    /// Fee asset ID.
    pub fee_asset_id: [u8; 32],
    /// The asset ID of the order UTXO (BASE or QUOTE).
    pub order_asset_id: [u8; 32],
    /// Where to send the refunded order amount.
    pub refund_destination: Script,
    /// Where to send fee change (optional).
    pub fee_change_destination: Option<Script>,
}

/// Build the cancel-order PSET.
///
/// ```text
/// Inputs:  [0] order UTXO (key-path spend — maker signature)
///          [1] fee input
/// Outputs: [0] refund to maker
///          [1] fee output
///          [2] fee change (optional)
/// ```
pub fn build_cancel_order_pset(params: &CancelOrderParams) -> Result<PartiallySignedTransaction> {
    if params.fee_utxo.value < params.fee_amount {
        return Err(Error::InsufficientFee);
    }

    if params.fee_utxo.value > params.fee_amount && params.fee_change_destination.is_none() {
        return Err(Error::MissingChangeDestination);
    }

    let mut pset = new_pset();

    // Input 0: order UTXO (key-path spend)
    add_pset_input(&mut pset, &params.order_utxo);
    // Input 1: fee
    add_pset_input(&mut pset, &params.fee_utxo);

    // Output 0: refund to maker
    add_pset_output(
        &mut pset,
        explicit_txout(
            &params.order_asset_id,
            params.order_utxo.value,
            &params.refund_destination,
        ),
    );

    // Output 1: fee
    add_pset_output(
        &mut pset,
        fee_txout(&params.fee_asset_id, params.fee_amount),
    );

    // Output 2: fee change (optional)
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
