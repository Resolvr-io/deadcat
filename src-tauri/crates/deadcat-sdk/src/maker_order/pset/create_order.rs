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

    // Detect combined mode: funding and fee share the same outpoint (same-asset case)
    let combined = params.funding_utxo.outpoint == params.fee_utxo.outpoint;

    if combined {
        // Single UTXO must cover order + fee
        if params.funding_utxo.value < params.order_amount + params.fee_amount {
            return Err(Error::InsufficientCollateral);
        }

        let total_change = params.funding_utxo.value - params.order_amount - params.fee_amount;
        if total_change > 0 && params.change_destination.is_none() {
            return Err(Error::MissingChangeDestination);
        }
    } else {
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
    }

    let mut pset = new_pset();

    // Determine the offered asset based on direction
    let offered_asset = match contract.params().direction {
        OrderDirection::SellBase => &contract.params().base_asset_id,
        OrderDirection::SellQuote => &contract.params().quote_asset_id,
    };

    let covenant_spk = contract.script_pubkey(&params.maker_base_pubkey);

    // Inputs
    add_pset_input(&mut pset, &params.funding_utxo);
    if !combined {
        add_pset_input(&mut pset, &params.fee_utxo);
    }

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

    if combined {
        // Single combined change output (if any)
        let total_change = params.funding_utxo.value - params.order_amount - params.fee_amount;
        if total_change > 0
            && let Some(ref change_spk) = params.change_destination
        {
            add_pset_output(
                &mut pset,
                explicit_txout(&params.fee_asset_id, total_change, change_spk),
            );
        }
    } else {
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
    }

    Ok(pset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maker_order::contract::CompiledMakerOrder;
    use crate::maker_order::params::{MakerOrderParams, OrderDirection};
    use crate::pset::UnblindedUtxo;
    use crate::taproot::NUMS_KEY_BYTES;
    use crate::testing::test_explicit_utxo;

    const BASE_ASSET: [u8; 32] = [0x01; 32];
    const QUOTE_ASSET: [u8; 32] = [0xbb; 32];
    const MAKER_PUBKEY: [u8; 32] = [0xaa; 32];

    fn sell_quote_contract() -> CompiledMakerOrder {
        let (params, _) = MakerOrderParams::new(
            BASE_ASSET,
            QUOTE_ASSET,
            50_000,
            1,
            1,
            OrderDirection::SellQuote,
            NUMS_KEY_BYTES,
            &MAKER_PUBKEY,
            &[0x22; 32],
        );
        CompiledMakerOrder::new(params).unwrap()
    }

    fn sell_base_contract() -> CompiledMakerOrder {
        let (params, _) = MakerOrderParams::new(
            BASE_ASSET,
            QUOTE_ASSET,
            50_000,
            1,
            1,
            OrderDirection::SellBase,
            NUMS_KEY_BYTES,
            &MAKER_PUBKEY,
            &[0x11; 32],
        );
        CompiledMakerOrder::new(params).unwrap()
    }

    /// UTXO at the default outpoint (tag=0).
    fn utxo(asset_id: [u8; 32], value: u64) -> UnblindedUtxo {
        test_explicit_utxo(&asset_id, value, &Script::new(), 0)
    }

    /// UTXO at a distinct outpoint (tag=1).
    fn utxo_separate(asset_id: [u8; 32], value: u64) -> UnblindedUtxo {
        test_explicit_utxo(&asset_id, value, &Script::new(), 1)
    }

    // -- Combined-UTXO detection --

    #[test]
    fn combined_detected_when_same_outpoint() {
        let u = utxo(QUOTE_ASSET, 1000);
        assert_eq!(u.outpoint, u.outpoint);
        // Same outpoint → combined path
        let contract = sell_quote_contract();
        let params = CreateOrderParams {
            funding_utxo: u.clone(),
            fee_utxo: u,
            order_amount: 500,
            fee_amount: 400,
            fee_asset_id: QUOTE_ASSET,
            change_destination: Some(Script::new()),
            fee_change_destination: None,
            maker_base_pubkey: MAKER_PUBKEY,
        };
        let pset = build_create_order_pset(&contract, &params).unwrap();
        assert_eq!(pset.inputs().len(), 1);
    }

    #[test]
    fn separate_when_different_outpoints() {
        let contract = sell_base_contract();
        let params = CreateOrderParams {
            funding_utxo: utxo(BASE_ASSET, 100),
            fee_utxo: utxo_separate(QUOTE_ASSET, 500),
            order_amount: 100,
            fee_amount: 500,
            fee_asset_id: QUOTE_ASSET,
            change_destination: None,
            fee_change_destination: None,
            maker_base_pubkey: MAKER_PUBKEY,
        };
        let pset = build_create_order_pset(&contract, &params).unwrap();
        assert_eq!(pset.inputs().len(), 2);
    }

    // -- Combined-UTXO validation --

    #[test]
    fn combined_exact_amount_no_change() {
        let contract = sell_quote_contract();
        let u = utxo(QUOTE_ASSET, 600); // 500 order + 100 fee
        let params = CreateOrderParams {
            funding_utxo: u.clone(),
            fee_utxo: u,
            order_amount: 500,
            fee_amount: 100,
            fee_asset_id: QUOTE_ASSET,
            change_destination: None,
            fee_change_destination: None,
            maker_base_pubkey: MAKER_PUBKEY,
        };
        let pset = build_create_order_pset(&contract, &params).unwrap();
        assert_eq!(pset.inputs().len(), 1);
        // covenant + fee, no change
        assert_eq!(pset.outputs().len(), 2);
    }

    #[test]
    fn combined_with_change() {
        let contract = sell_quote_contract();
        let u = utxo(QUOTE_ASSET, 1000); // 500 order + 100 fee + 400 change
        let params = CreateOrderParams {
            funding_utxo: u.clone(),
            fee_utxo: u,
            order_amount: 500,
            fee_amount: 100,
            fee_asset_id: QUOTE_ASSET,
            change_destination: Some(Script::new()),
            fee_change_destination: None,
            maker_base_pubkey: MAKER_PUBKEY,
        };
        let pset = build_create_order_pset(&contract, &params).unwrap();
        assert_eq!(pset.inputs().len(), 1);
        // covenant + fee + change
        assert_eq!(pset.outputs().len(), 3);
    }

    #[test]
    fn combined_insufficient_for_order_plus_fee() {
        let contract = sell_quote_contract();
        let u = utxo(QUOTE_ASSET, 500); // enough for order, not order + fee
        let params = CreateOrderParams {
            funding_utxo: u.clone(),
            fee_utxo: u,
            order_amount: 500,
            fee_amount: 100,
            fee_asset_id: QUOTE_ASSET,
            change_destination: None,
            fee_change_destination: None,
            maker_base_pubkey: MAKER_PUBKEY,
        };
        let result = build_create_order_pset(&contract, &params);
        assert!(matches!(result, Err(Error::InsufficientCollateral)));
    }

    #[test]
    fn combined_excess_without_change_dest_rejected() {
        let contract = sell_quote_contract();
        let u = utxo(QUOTE_ASSET, 1000);
        let params = CreateOrderParams {
            funding_utxo: u.clone(),
            fee_utxo: u,
            order_amount: 500,
            fee_amount: 100,
            fee_asset_id: QUOTE_ASSET,
            change_destination: None, // excess but no change destination
            fee_change_destination: None,
            maker_base_pubkey: MAKER_PUBKEY,
        };
        let result = build_create_order_pset(&contract, &params);
        assert!(matches!(result, Err(Error::MissingChangeDestination)));
    }

    // -- Separate path still works --

    #[test]
    fn separate_insufficient_fee_rejected() {
        let contract = sell_base_contract();
        let params = CreateOrderParams {
            funding_utxo: utxo(BASE_ASSET, 100),
            fee_utxo: utxo_separate(QUOTE_ASSET, 50),
            order_amount: 100,
            fee_amount: 100,
            fee_asset_id: QUOTE_ASSET,
            change_destination: None,
            fee_change_destination: None,
            maker_base_pubkey: MAKER_PUBKEY,
        };
        let result = build_create_order_pset(&contract, &params);
        assert!(matches!(result, Err(Error::InsufficientFee)));
    }

    #[test]
    fn separate_with_both_changes() {
        let contract = sell_base_contract();
        let params = CreateOrderParams {
            funding_utxo: utxo(BASE_ASSET, 200),
            fee_utxo: utxo_separate(QUOTE_ASSET, 1000),
            order_amount: 100,
            fee_amount: 500,
            fee_asset_id: QUOTE_ASSET,
            change_destination: Some(Script::new()),
            fee_change_destination: Some(Script::new()),
            maker_base_pubkey: MAKER_PUBKEY,
        };
        let pset = build_create_order_pset(&contract, &params).unwrap();
        assert_eq!(pset.inputs().len(), 2);
        // covenant + fee + funding_change + fee_change
        assert_eq!(pset.outputs().len(), 4);
    }
}
