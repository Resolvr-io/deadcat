use simplicityhl::elements::Script;
use simplicityhl::elements::pset::PartiallySignedTransaction;

use crate::error::{Error, Result};
use crate::maker_order::contract::CompiledMakerOrder;
use crate::maker_order::params::OrderDirection;
use crate::pset::{
    UnblindedUtxo, add_pset_input, add_pset_output, explicit_txout, fee_txout, new_pset,
};

/// A single taker's contribution to a fill transaction.
pub struct TakerFill {
    /// The taker's funding UTXO (holds the asset they are offering).
    pub funding_utxo: UnblindedUtxo,
    /// Where the taker receives the asset they are buying.
    pub receive_destination: Script,
    /// Amount the taker receives.
    pub receive_amount: u64,
    /// Asset ID of what the taker receives.
    pub receive_asset_id: [u8; 32],
    /// Change destination for the taker's funding UTXO (if overfunded).
    pub change_destination: Option<Script>,
    /// Change amount returned to the taker.
    pub change_amount: u64,
    /// Asset ID of the taker's change (same as funding asset).
    pub change_asset_id: [u8; 32],
}

/// A single maker order to be filled.
pub struct MakerOrderFill {
    /// The compiled maker order contract.
    pub contract: CompiledMakerOrder,
    /// The maker's order UTXO (covenant-locked).
    pub order_utxo: UnblindedUtxo,
    /// The maker's base pubkey (for address derivation).
    pub maker_base_pubkey: [u8; 32],
    /// Amount the maker receives (in the wanted asset).
    pub maker_receive_amount: u64,
    /// The maker receive scriptPubKey (P_order P2TR output).
    pub maker_receive_script: Script,
    /// Whether this is a partial fill. If true, a remainder output is created.
    pub is_partial: bool,
    /// Remainder amount (only used if `is_partial` is true).
    pub remainder_amount: u64,
}

/// Parameters for constructing a fill-order PSET.
///
/// Supports multi-taker, multi-maker batch fills with the takers-first layout.
pub struct FillOrderParams {
    /// Taker fills (one per taker). Takers sign ANYONECANPAY|SINGLE.
    pub takers: Vec<TakerFill>,
    /// Maker order fills (one per order). Only the last may be partial.
    pub orders: Vec<MakerOrderFill>,
    /// Fee UTXO (provides the transaction fee).
    pub fee_utxo: UnblindedUtxo,
    /// Fee amount in sats.
    pub fee_amount: u64,
    /// Fee asset ID.
    pub fee_asset_id: [u8; 32],
    /// Fee change destination (optional).
    pub fee_change_destination: Option<Script>,
}

/// Build the fill-order PSET with the takers-first layout.
///
/// ```text
/// Inputs:  [0..T-1]     T taker funding inputs   (signed ANYONECANPAY|SINGLE)
///          [T..T+M-1]   M maker order inputs      (covenant script-path spend)
///          [T+M]        fee input
///
/// Outputs: [0..T-1]     T taker receive outputs   (1:1 matched with taker inputs)
///          [T..T+M-1]   M maker receive outputs   (1:1 matched with order inputs)
///          [T+M]        remainder                  (only if last order is partial fill)
///          [T+M+1..]    fee, change
/// ```
pub fn build_fill_order_pset(params: &FillOrderParams) -> Result<PartiallySignedTransaction> {
    if params.takers.is_empty() {
        return Err(Error::Pset("at least one taker is required".into()));
    }

    if params.orders.is_empty() {
        return Err(Error::Pset("at least one order is required".into()));
    }

    if params.fee_utxo.value < params.fee_amount {
        return Err(Error::InsufficientFee);
    }

    // Validate: only the last order may be partially filled
    for (i, order) in params.orders.iter().enumerate() {
        if order.is_partial && i != params.orders.len() - 1 {
            return Err(Error::PartialFillNotLast);
        }
    }

    // Validate each order's fill against its contract params
    for order in &params.orders {
        validate_order_fill(order)?;
    }

    let mut pset = new_pset();

    // ── INPUTS ──

    // Taker funding inputs
    for taker in &params.takers {
        add_pset_input(&mut pset, &taker.funding_utxo);
    }

    // Maker order inputs (covenant)
    for order in &params.orders {
        add_pset_input(&mut pset, &order.order_utxo);
    }

    // Fee input
    add_pset_input(&mut pset, &params.fee_utxo);

    // ── OUTPUTS ──

    // Taker receive outputs (1:1 with taker inputs)
    for taker in &params.takers {
        add_pset_output(
            &mut pset,
            explicit_txout(
                &taker.receive_asset_id,
                taker.receive_amount,
                &taker.receive_destination,
            ),
        );
    }

    // Maker receive outputs (1:1 with order inputs)
    for order in &params.orders {
        let receive_asset = match order.contract.params().direction {
            OrderDirection::SellBase => &order.contract.params().quote_asset_id,
            OrderDirection::SellQuote => &order.contract.params().base_asset_id,
        };
        add_pset_output(
            &mut pset,
            explicit_txout(
                receive_asset,
                order.maker_receive_amount,
                &order.maker_receive_script,
            ),
        );
    }

    // Remainder output (only if last order is partial)
    if let Some(last) = params.orders.last()
        && last.is_partial
    {
        let remainder_asset = match last.contract.params().direction {
            OrderDirection::SellBase => &last.contract.params().base_asset_id,
            OrderDirection::SellQuote => &last.contract.params().quote_asset_id,
        };
        let covenant_spk = last.contract.script_pubkey(&last.maker_base_pubkey);
        add_pset_output(
            &mut pset,
            explicit_txout(remainder_asset, last.remainder_amount, &covenant_spk),
        );
    }

    // Taker change outputs (for overfunded taker UTXOs)
    for taker in &params.takers {
        if taker.change_amount > 0 {
            if let Some(ref change_spk) = taker.change_destination {
                add_pset_output(
                    &mut pset,
                    explicit_txout(&taker.change_asset_id, taker.change_amount, change_spk),
                );
            }
        }
    }

    // Fee output
    add_pset_output(
        &mut pset,
        fee_txout(&params.fee_asset_id, params.fee_amount),
    );

    // Fee change (optional)
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

/// Validate a single order fill against its contract parameters.
fn validate_order_fill(order: &MakerOrderFill) -> Result<()> {
    let p = order.contract.params();

    if p.price == 0 {
        return Err(Error::ZeroPrice);
    }

    let input_amount = order.order_utxo.value;
    let maker_amount = order.maker_receive_amount;

    match p.direction {
        OrderDirection::SellBase => {
            // Maker sells BASE lots, receives QUOTE
            if order.is_partial {
                let consumed = input_amount
                    .checked_sub(order.remainder_amount)
                    .ok_or(Error::MakerOrderOverflow)?;
                let expected_payment = consumed
                    .checked_mul(p.price)
                    .ok_or(Error::MakerOrderOverflow)?;
                if maker_amount != expected_payment {
                    return Err(Error::ConservationViolation);
                }
                if consumed < p.min_fill_lots {
                    return Err(Error::FillBelowMinimum);
                }
                if order.remainder_amount < p.min_remainder_lots {
                    return Err(Error::RemainderBelowMinimum);
                }
            } else {
                let expected_payment = input_amount
                    .checked_mul(p.price)
                    .ok_or(Error::MakerOrderOverflow)?;
                if maker_amount != expected_payment {
                    return Err(Error::ConservationViolation);
                }
                if input_amount < p.min_fill_lots {
                    return Err(Error::FillBelowMinimum);
                }
            }
        }
        OrderDirection::SellQuote => {
            // Maker sells QUOTE, receives BASE lots
            if order.is_partial {
                let maker_payment = maker_amount
                    .checked_mul(p.price)
                    .ok_or(Error::MakerOrderOverflow)?;
                let total = maker_payment
                    .checked_add(order.remainder_amount)
                    .ok_or(Error::MakerOrderOverflow)?;
                if total != input_amount {
                    return Err(Error::ConservationViolation);
                }
                if maker_amount < p.min_fill_lots {
                    return Err(Error::FillBelowMinimum);
                }
                let min_remainder_quote = p
                    .min_remainder_lots
                    .checked_mul(p.price)
                    .ok_or(Error::MakerOrderOverflow)?;
                if order.remainder_amount < min_remainder_quote {
                    return Err(Error::RemainderBelowMinimum);
                }
            } else {
                let expected_input = maker_amount
                    .checked_mul(p.price)
                    .ok_or(Error::MakerOrderOverflow)?;
                if expected_input != input_amount {
                    return Err(Error::ConservationViolation);
                }
                if maker_amount < p.min_fill_lots {
                    return Err(Error::FillBelowMinimum);
                }
            }
        }
    }

    Ok(())
}
