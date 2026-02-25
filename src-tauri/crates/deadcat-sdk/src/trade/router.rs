//! Trade routing: given discovered liquidity (AMM pool + limit orders) and a
//! trade request, compute the optimal execution plan.
//!
//! The routing algorithm is greedy:
//! 1. Sort eligible limit orders by effective price (cheapest first for Buy,
//!    best payout first for Sell).
//! 2. Fill orders that beat the AMM's current marginal price.
//! 3. Route the remaining amount through the AMM pool.

use crate::amm_pool::math::{
    PoolReserves, SwapPair, compute_swap_exact_input, spot_price_no_lbtc, spot_price_yes_lbtc,
};
use crate::amm_pool::params::AmmPoolParams;
use crate::discovery::DiscoveredOrder;
use crate::error::{Error, Result};
use crate::maker_order::params::OrderDirection;
use crate::pset::UnblindedUtxo;

use super::types::*;

// ── Scanned order: DiscoveredOrder + on-chain UTXO state ───────────────

/// A limit order with its on-chain UTXO scanned, ready for routing.
#[derive(Debug, Clone)]
pub(crate) struct ScannedOrder {
    /// The Nostr-discovered order metadata.
    pub discovered: DiscoveredOrder,
    /// The live on-chain UTXO (may differ from `offered_amount`).
    pub utxo: UnblindedUtxo,
    /// Decoded `maker_base_pubkey` bytes.
    pub maker_base_pubkey: [u8; 32],
    /// Decoded `order_nonce` bytes.
    pub order_nonce: [u8; 32],
    /// Decoded `MakerOrderParams`.
    pub params: crate::maker_order::params::MakerOrderParams,
}

/// Scanned pool state ready for routing.
#[derive(Debug, Clone)]
pub(crate) struct ScannedPool {
    pub params: AmmPoolParams,
    pub issued_lp: u64,
    pub reserves: PoolReserves,
    pub utxos: PoolUtxos,
    pub pool_id: String,
}

// ── Price helpers ───────────────────────────────────────────────────────

/// AMM spot price in collateral-per-token terms for the given side.
///
/// Buy YES: how many sats per YES token (r_lbtc / r_yes).
/// Buy NO:  how many sats per NO token (r_lbtc / r_no).
pub(crate) fn amm_spot_price(reserves: &PoolReserves, side: TradeSide) -> f64 {
    match side {
        TradeSide::Yes => spot_price_yes_lbtc(reserves),
        TradeSide::No => spot_price_no_lbtc(reserves),
    }
}

/// Effective collateral-per-token price of a limit order for a Buy trade.
///
/// For a taker buying tokens:
/// - Eligible orders have `direction = SellBase` (maker sells tokens).
/// - Effective price = `order.price` (sats per lot, where 1 lot = 1 token unit).
///
/// Returns `None` if the order is not eligible for this trade.
fn order_buy_price(order: &ScannedOrder, target_token_asset: &[u8; 32]) -> Option<f64> {
    if order.params.base_asset_id != *target_token_asset {
        return None;
    }
    if order.params.direction != OrderDirection::SellBase {
        return None;
    }
    Some(order.params.price as f64)
}

/// Effective collateral-per-token price of a limit order for a Sell trade.
///
/// For a taker selling tokens:
/// - Eligible orders have `direction = SellQuote` (maker sells collateral,
///   i.e. maker is buying tokens at `price` sats per token).
/// - Effective price = `order.price` (sats per token the maker pays).
///
/// Returns `None` if the order is not eligible for this trade.
fn order_sell_price(order: &ScannedOrder, target_token_asset: &[u8; 32]) -> Option<f64> {
    if order.params.base_asset_id != *target_token_asset {
        return None;
    }
    if order.params.direction != OrderDirection::SellQuote {
        return None;
    }
    Some(order.params.price as f64)
}

// ── Core routing ────────────────────────────────────────────────────────

/// Map `TradeSide` + `TradeDirection` to AMM swap parameters.
pub(crate) fn swap_params(side: TradeSide, direction: TradeDirection) -> (SwapPair, bool) {
    match (side, direction) {
        // Buy YES: deposit L-BTC, receive YES → YesLbtc, sell_a=false
        (TradeSide::Yes, TradeDirection::Buy) => (SwapPair::YesLbtc, false),
        // Sell YES: deposit YES, receive L-BTC → YesLbtc, sell_a=true
        (TradeSide::Yes, TradeDirection::Sell) => (SwapPair::YesLbtc, true),
        // Buy NO: deposit L-BTC, receive NO → NoLbtc, sell_a=false
        (TradeSide::No, TradeDirection::Buy) => (SwapPair::NoLbtc, false),
        // Sell NO: deposit NO, receive L-BTC → NoLbtc, sell_a=true
        (TradeSide::No, TradeDirection::Sell) => (SwapPair::NoLbtc, true),
    }
}

/// Determine the token asset ID targeted by a trade.
pub(crate) fn target_token_asset(
    side: TradeSide,
    yes_asset: &[u8; 32],
    no_asset: &[u8; 32],
) -> [u8; 32] {
    match side {
        TradeSide::Yes => *yes_asset,
        TradeSide::No => *no_asset,
    }
}

/// Build the complete execution plan for an `ExactInput` trade.
///
/// `total_input` is the exact amount the taker wants to spend (collateral
/// for Buy, tokens for Sell).
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_execution_plan(
    pool: Option<&ScannedPool>,
    orders: &[ScannedOrder],
    side: TradeSide,
    direction: TradeDirection,
    total_input: u64,
    collateral_asset: &[u8; 32],
    yes_asset: &[u8; 32],
    no_asset: &[u8; 32],
) -> Result<ExecutionPlan> {
    let token_asset = target_token_asset(side, yes_asset, no_asset);
    let (taker_send_asset, taker_receive_asset) = match direction {
        TradeDirection::Buy => (*collateral_asset, token_asset),
        TradeDirection::Sell => (token_asset, *collateral_asset),
    };

    // Determine AMM spot price (if pool exists).
    let amm_spot = pool.as_ref().map(|p| amm_spot_price(&p.reserves, side));

    // Filter and sort eligible orders.
    let mut eligible: Vec<(usize, f64)> = orders
        .iter()
        .enumerate()
        .filter_map(|(i, o)| {
            let price = match direction {
                TradeDirection::Buy => order_buy_price(o, &token_asset),
                TradeDirection::Sell => order_sell_price(o, &token_asset),
            }?;
            // For Buy: order is eligible if its price < AMM spot price.
            // For Sell: order is eligible if its payout > AMM payout (price > AMM price).
            // If no pool exists, all orders are eligible.
            let eligible = match (direction, amm_spot) {
                (TradeDirection::Buy, Some(spot)) => price <= spot,
                (TradeDirection::Sell, Some(spot)) => {
                    // AMM "spot" for selling = 1/spot_price_token_lbtc = token/lbtc.
                    // Order pays `price` sats per token. AMM pays ~`1/spot` sats per token.
                    // Actually, for selling: AMM price is r_lbtc/r_token and order price
                    // is also in sats-per-token. The order is better if it pays MORE.
                    price >= spot
                }
                (_, None) => true,
            };
            if eligible { Some((i, price)) } else { None }
        })
        .collect();

    // Sort: cheapest first for Buy, best payout first for Sell.
    match direction {
        TradeDirection::Buy => eligible.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap()),
        TradeDirection::Sell => eligible.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap()),
    }

    // Greedily fill orders.
    let mut remaining_input = total_input;
    let mut order_legs = Vec::new();
    let mut total_output = 0u64;

    for (idx, _price) in &eligible {
        if remaining_input == 0 {
            break;
        }
        let order = &orders[*idx];
        let on_chain_value = order.utxo.value;
        if on_chain_value == 0 {
            continue;
        }

        let leg = match direction {
            TradeDirection::Buy => fill_buy_order(order, on_chain_value, remaining_input)?,
            TradeDirection::Sell => fill_sell_order(order, on_chain_value, remaining_input)?,
        };

        if let Some(leg) = leg {
            remaining_input = remaining_input.saturating_sub(leg.taker_pays);
            total_output += leg.taker_receives;
            order_legs.push(leg);
        }
    }

    // Route remainder through AMM.
    let pool_leg = if remaining_input > 0 {
        match pool {
            Some(scanned_pool) => {
                let (swap_pair, sell_a) = swap_params(side, direction);
                let swap = compute_swap_exact_input(
                    &scanned_pool.reserves,
                    swap_pair,
                    remaining_input,
                    scanned_pool.params.fee_bps,
                    sell_a,
                )?;
                total_output += swap.delta_out;
                Some(PoolSwapLeg {
                    pool_params: scanned_pool.params,
                    issued_lp: scanned_pool.issued_lp,
                    pool_utxos: scanned_pool.utxos.clone(),
                    swap_pair,
                    sell_a,
                    delta_in: swap.delta_in,
                    delta_out: swap.delta_out,
                    new_reserves: swap.new_reserves,
                })
            }
            None if order_legs.is_empty() => return Err(Error::NoLiquidity),
            // No pool, but we consumed everything via orders — shouldn't happen
            // since remaining_input > 0 means we still have unrouted amount.
            None => return Err(Error::NoLiquidity),
        }
    } else {
        None
    };

    if total_output == 0 {
        return Err(Error::NoLiquidity);
    }

    // Ensure at most the last order is partial.
    for (i, leg) in order_legs.iter().enumerate() {
        if leg.is_partial && i < order_legs.len() - 1 {
            return Err(Error::PartialFillNotLast);
        }
    }

    Ok(ExecutionPlan {
        order_legs,
        pool_leg,
        taker_send_asset,
        taker_receive_asset,
        total_taker_input: total_input,
        total_taker_output: total_output,
        quoted_reserves: pool.as_ref().map(|p| p.reserves),
    })
}

// ── Per-order fill logic ────────────────────────────────────────────────

/// Compute a fill for a SellBase order (taker buys tokens).
///
/// The order's covenant holds BASE tokens. The taker pays L-BTC and
/// receives tokens. Each lot = 1 BASE unit, cost = `price` QUOTE per lot.
fn fill_buy_order(
    order: &ScannedOrder,
    on_chain_value: u64,
    remaining_lbtc: u64,
) -> Result<Option<OrderFillLeg>> {
    let price = order.params.price;
    if price == 0 {
        return Ok(None);
    }

    // How many lots can the taker afford?
    let affordable_lots = remaining_lbtc / price;
    // How many lots does the order have?
    let available_lots = on_chain_value; // SellBase: covenant holds BASE, 1 lot = 1 unit
    if available_lots == 0 || affordable_lots == 0 {
        return Ok(None);
    }

    let lots = affordable_lots.min(available_lots);

    // Check min_fill constraint.
    if lots < order.params.min_fill_lots {
        return Ok(None);
    }

    let is_partial = lots < available_lots;
    let remainder = available_lots - lots;

    // Check min_remainder constraint for partial fills.
    if is_partial && remainder < order.params.min_remainder_lots && remainder > 0 {
        // Can't partially fill — try filling up to leave min_remainder.
        let adjusted = available_lots.saturating_sub(order.params.min_remainder_lots);
        if adjusted < order.params.min_fill_lots || adjusted == 0 {
            return Ok(None);
        }
        return fill_buy_order_with_lots(order, on_chain_value, adjusted);
    }

    fill_buy_order_with_lots(order, on_chain_value, lots)
}

fn fill_buy_order_with_lots(
    order: &ScannedOrder,
    on_chain_value: u64,
    lots: u64,
) -> Result<Option<OrderFillLeg>> {
    let price = order.params.price;
    let taker_pays = lots
        .checked_mul(price)
        .ok_or_else(|| Error::TradeRouting("overflow computing taker payment".into()))?;
    let taker_receives = lots;
    let maker_receive_amount = taker_pays;
    let is_partial = lots < on_chain_value;
    let remainder_value = on_chain_value - lots;

    Ok(Some(OrderFillLeg {
        params: order.params,
        maker_base_pubkey: order.maker_base_pubkey,
        order_nonce: order.order_nonce,
        order_utxo: order.utxo.clone(),
        lots,
        taker_pays,
        taker_receives,
        maker_receive_amount,
        is_partial,
        remainder_value,
    }))
}

/// Compute a fill for a SellQuote order (taker sells tokens).
///
/// The order's covenant holds QUOTE (L-BTC). The taker pays tokens and
/// receives L-BTC. Each lot = 1 BASE unit, payout = `price` QUOTE per lot.
fn fill_sell_order(
    order: &ScannedOrder,
    on_chain_value: u64,
    remaining_tokens: u64,
) -> Result<Option<OrderFillLeg>> {
    let price = order.params.price;
    if price == 0 {
        return Ok(None);
    }

    // How many lots can the taker supply?
    let affordable_lots = remaining_tokens; // 1 lot = 1 token unit
    // How many lots can the order pay out?
    // SellQuote: covenant holds QUOTE, lots payable = floor(on_chain_value / price)
    let available_lots = on_chain_value / price;
    if available_lots == 0 || affordable_lots == 0 {
        return Ok(None);
    }

    let lots = affordable_lots.min(available_lots);

    if lots < order.params.min_fill_lots {
        return Ok(None);
    }

    let is_partial = lots < available_lots;
    let quote_consumed = lots
        .checked_mul(price)
        .ok_or_else(|| Error::TradeRouting("overflow computing quote consumed".into()))?;
    let remainder = on_chain_value - quote_consumed;

    // Check min_remainder for partial fills (in quote terms).
    if is_partial
        && remainder < order.params.min_remainder_lots.saturating_mul(price)
        && remainder > 0
    {
        let min_remainder_quote = order.params.min_remainder_lots.saturating_mul(price);
        let max_consumable = on_chain_value.saturating_sub(min_remainder_quote);
        let adjusted_lots = max_consumable / price;
        if adjusted_lots < order.params.min_fill_lots || adjusted_lots == 0 {
            return Ok(None);
        }
        return fill_sell_order_with_lots(order, on_chain_value, adjusted_lots);
    }

    fill_sell_order_with_lots(order, on_chain_value, lots)
}

fn fill_sell_order_with_lots(
    order: &ScannedOrder,
    on_chain_value: u64,
    lots: u64,
) -> Result<Option<OrderFillLeg>> {
    let price = order.params.price;
    let taker_pays = lots; // taker sends this many tokens
    let quote_consumed = lots
        .checked_mul(price)
        .ok_or_else(|| Error::TradeRouting("overflow computing quote consumed".into()))?;
    let taker_receives = quote_consumed; // taker gets this much L-BTC
    let maker_receive_amount = lots; // maker gets tokens
    let is_partial = quote_consumed < on_chain_value;
    let remainder_value = on_chain_value - quote_consumed;

    Ok(Some(OrderFillLeg {
        params: order.params,
        maker_base_pubkey: order.maker_base_pubkey,
        order_nonce: order.order_nonce,
        order_utxo: order.utxo.clone(),
        lots,
        taker_pays,
        taker_receives,
        maker_receive_amount,
        is_partial,
        remainder_value,
    }))
}

// ── Helpers ─────────────────────────────────────────────────────────────

/// Build display-friendly route legs from an execution plan.
pub(crate) fn plan_to_route_legs(
    plan: &ExecutionPlan,
    pool_id: Option<&str>,
    orders: &[ScannedOrder],
) -> Vec<RouteLeg> {
    let mut legs = Vec::new();

    for order_leg in &plan.order_legs {
        // Find the matching DiscoveredOrder ID.
        let order_id = orders
            .iter()
            .find(|o| o.utxo.outpoint == order_leg.order_utxo.outpoint)
            .map(|o| o.discovered.id.clone())
            .unwrap_or_default();

        legs.push(RouteLeg {
            source: LiquiditySource::LimitOrder {
                order_id,
                price: order_leg.params.price,
                lots: order_leg.lots,
            },
            input_amount: order_leg.taker_pays,
            output_amount: order_leg.taker_receives,
        });
    }

    if let Some(ref pool_leg) = plan.pool_leg {
        legs.push(RouteLeg {
            source: LiquiditySource::AmmPool {
                pool_id: pool_id.unwrap_or_default().to_string(),
            },
            input_amount: pool_leg.delta_in,
            output_amount: pool_leg.delta_out,
        });
    }

    legs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::amm_pool::math::PoolReserves;
    use crate::maker_order::params::{MakerOrderParams, OrderDirection};
    use lwk_wollet::elements::hashes::Hash as _;
    use lwk_wollet::elements::{OutPoint, Txid};

    fn mock_utxo(asset: [u8; 32], value: u64) -> UnblindedUtxo {
        use lwk_wollet::elements::AssetId;
        use lwk_wollet::elements::confidential::{Asset, Nonce, Value as ConfValue};
        use lwk_wollet::elements::{Script, TxOut, TxOutWitness};

        UnblindedUtxo {
            outpoint: OutPoint::new(Txid::all_zeros(), 0),
            txout: TxOut {
                asset: Asset::Explicit(AssetId::from_slice(&asset).unwrap()),
                value: ConfValue::Explicit(value),
                nonce: Nonce::Null,
                script_pubkey: Script::new(),
                witness: TxOutWitness::default(),
            },
            asset_id: asset,
            value,
            asset_blinding_factor: [0u8; 32],
            value_blinding_factor: [0u8; 32],
        }
    }

    fn yes_asset() -> [u8; 32] {
        let mut a = [0u8; 32];
        a[0] = 0x01;
        a
    }

    fn no_asset() -> [u8; 32] {
        let mut a = [0u8; 32];
        a[0] = 0x02;
        a
    }

    fn lbtc_asset() -> [u8; 32] {
        let mut a = [0u8; 32];
        a[0] = 0x03;
        a
    }

    fn mock_pool(r_yes: u64, r_no: u64, r_lbtc: u64) -> ScannedPool {
        let params = AmmPoolParams {
            yes_asset_id: yes_asset(),
            no_asset_id: no_asset(),
            lbtc_asset_id: lbtc_asset(),
            lp_asset_id: [0x04; 32],
            lp_reissuance_token_id: [0x05; 32],
            fee_bps: 30,
            cosigner_pubkey: crate::taproot::NUMS_KEY_BYTES,
        };
        ScannedPool {
            params,
            issued_lp: 1000,
            reserves: PoolReserves {
                r_yes,
                r_no,
                r_lbtc,
            },
            utxos: PoolUtxos {
                yes: mock_utxo(yes_asset(), r_yes),
                no: mock_utxo(no_asset(), r_no),
                lbtc: mock_utxo(lbtc_asset(), r_lbtc),
                rt: mock_utxo([0x05; 32], 1),
            },
            pool_id: "test-pool".to_string(),
        }
    }

    fn mock_sell_base_order(price: u64, available_tokens: u64) -> ScannedOrder {
        ScannedOrder {
            discovered: DiscoveredOrder {
                id: "order-1".to_string(),
                market_id: "market-1".to_string(),
                base_asset_id: hex::encode(yes_asset()),
                quote_asset_id: hex::encode(lbtc_asset()),
                price,
                min_fill_lots: 1,
                min_remainder_lots: 1,
                direction: "sell-base".to_string(),
                direction_label: "sell-yes".to_string(),
                maker_base_pubkey: hex::encode([0xaa; 32]),
                order_nonce: hex::encode([0xbb; 32]),
                covenant_address: String::new(),
                offered_amount: available_tokens,
                cosigner_pubkey: hex::encode(crate::taproot::NUMS_KEY_BYTES),
                maker_receive_spk_hash: hex::encode([0xcc; 32]),
                creator_pubkey: String::new(),
                created_at: 0,
                nostr_event_json: None,
            },
            utxo: mock_utxo(yes_asset(), available_tokens),
            maker_base_pubkey: [0xaa; 32],
            order_nonce: [0xbb; 32],
            params: MakerOrderParams {
                base_asset_id: yes_asset(),
                quote_asset_id: lbtc_asset(),
                price,
                min_fill_lots: 1,
                min_remainder_lots: 1,
                direction: OrderDirection::SellBase,
                maker_receive_spk_hash: [0xcc; 32],
                cosigner_pubkey: crate::taproot::NUMS_KEY_BYTES,
                maker_pubkey: [0xaa; 32],
            },
        }
    }

    #[test]
    fn swap_params_mapping() {
        assert_eq!(
            swap_params(TradeSide::Yes, TradeDirection::Buy),
            (SwapPair::YesLbtc, false)
        );
        assert_eq!(
            swap_params(TradeSide::Yes, TradeDirection::Sell),
            (SwapPair::YesLbtc, true)
        );
        assert_eq!(
            swap_params(TradeSide::No, TradeDirection::Buy),
            (SwapPair::NoLbtc, false)
        );
        assert_eq!(
            swap_params(TradeSide::No, TradeDirection::Sell),
            (SwapPair::NoLbtc, true)
        );
    }

    #[test]
    fn amm_only_buy_yes() {
        let pool = mock_pool(1_000_000, 1_000_000, 500_000);
        let plan = build_execution_plan(
            Some(&pool),
            &[],
            TradeSide::Yes,
            TradeDirection::Buy,
            10_000,
            &lbtc_asset(),
            &yes_asset(),
            &no_asset(),
        )
        .unwrap();

        assert!(plan.order_legs.is_empty());
        assert!(plan.pool_leg.is_some());
        let pool_leg = plan.pool_leg.unwrap();
        assert_eq!(pool_leg.delta_in, 10_000);
        assert!(pool_leg.delta_out > 0);
        assert_eq!(plan.total_taker_input, 10_000);
        assert_eq!(plan.total_taker_output, pool_leg.delta_out);
    }

    #[test]
    fn order_cheaper_than_amm_gets_filled_first() {
        // Pool: YES/LBTC spot price = 500_000_000 / 1_000_000 = 500 sats per YES
        let pool = mock_pool(1_000_000, 1_000_000, 500_000_000);
        // Order: selling YES at 400 sats per token (cheaper than AMM spot of 500)
        let order = mock_sell_base_order(400, 10);

        let plan = build_execution_plan(
            Some(&pool),
            &[order],
            TradeSide::Yes,
            TradeDirection::Buy,
            10_000, // 10k sats to spend
            &lbtc_asset(),
            &yes_asset(),
            &no_asset(),
        )
        .unwrap();

        // Order should be filled first: 10 lots * 400 = 4000 sats
        assert_eq!(plan.order_legs.len(), 1);
        assert_eq!(plan.order_legs[0].lots, 10);
        assert_eq!(plan.order_legs[0].taker_pays, 4_000);
        assert_eq!(plan.order_legs[0].taker_receives, 10);
        // Remaining 6000 sats go through AMM
        assert!(plan.pool_leg.is_some());
        assert_eq!(plan.pool_leg.as_ref().unwrap().delta_in, 6_000);
    }

    #[test]
    fn order_more_expensive_than_amm_is_skipped() {
        // AMM spot = 500_000_000 / 1_000_000 = 500 sats per YES
        let pool = mock_pool(1_000_000, 1_000_000, 500_000_000);
        // Order at 600 sats per token (more expensive than AMM spot of 500)
        let order = mock_sell_base_order(600, 10);

        let plan = build_execution_plan(
            Some(&pool),
            &[order],
            TradeSide::Yes,
            TradeDirection::Buy,
            10_000,
            &lbtc_asset(),
            &yes_asset(),
            &no_asset(),
        )
        .unwrap();

        assert!(plan.order_legs.is_empty());
        assert!(plan.pool_leg.is_some());
        assert_eq!(plan.pool_leg.as_ref().unwrap().delta_in, 10_000);
    }

    #[test]
    fn orders_only_no_pool() {
        let order = mock_sell_base_order(400, 100);

        let plan = build_execution_plan(
            None,
            &[order],
            TradeSide::Yes,
            TradeDirection::Buy,
            10_000,
            &lbtc_asset(),
            &yes_asset(),
            &no_asset(),
        )
        .unwrap();

        assert!(!plan.order_legs.is_empty());
        assert!(plan.pool_leg.is_none());
        // 10000 / 400 = 25 lots, but order only has 100 lots → 25 lots
        assert_eq!(plan.order_legs[0].lots, 25);
    }

    #[test]
    fn no_liquidity_error() {
        let result = build_execution_plan(
            None,
            &[],
            TradeSide::Yes,
            TradeDirection::Buy,
            10_000,
            &lbtc_asset(),
            &yes_asset(),
            &no_asset(),
        );

        assert!(matches!(result, Err(Error::NoLiquidity)));
    }

    #[test]
    fn partial_fill_respects_min_fill() {
        // Order with min_fill_lots = 50, but taker can only afford 20
        let mut order = mock_sell_base_order(500, 100);
        order.params.min_fill_lots = 50;
        order.discovered.min_fill_lots = 50;

        let plan = build_execution_plan(
            None,
            &[order],
            TradeSide::Yes,
            TradeDirection::Buy,
            10_000, // 10000 / 500 = 20 lots, below min_fill of 50
            &lbtc_asset(),
            &yes_asset(),
            &no_asset(),
        );

        // Can't fill the order, no pool → NoLiquidity
        assert!(matches!(plan, Err(Error::NoLiquidity)));
    }

    // ── Sell-side helper ────────────────────────────────────────────────

    /// Create a SellQuote order (maker sells L-BTC, buys tokens).
    /// The covenant holds `available_lbtc` of L-BTC.
    /// `price` = sats per token lot the maker is willing to pay.
    fn mock_sell_quote_order(price: u64, available_lbtc: u64) -> ScannedOrder {
        ScannedOrder {
            discovered: DiscoveredOrder {
                id: "order-sell-1".to_string(),
                market_id: "market-1".to_string(),
                base_asset_id: hex::encode(yes_asset()),
                quote_asset_id: hex::encode(lbtc_asset()),
                price,
                min_fill_lots: 1,
                min_remainder_lots: 1,
                direction: "sell-quote".to_string(),
                direction_label: "buy-yes".to_string(),
                maker_base_pubkey: hex::encode([0xaa; 32]),
                order_nonce: hex::encode([0xbb; 32]),
                covenant_address: String::new(),
                offered_amount: available_lbtc,
                cosigner_pubkey: hex::encode(crate::taproot::NUMS_KEY_BYTES),
                maker_receive_spk_hash: hex::encode([0xcc; 32]),
                creator_pubkey: String::new(),
                created_at: 0,
                nostr_event_json: None,
            },
            utxo: mock_utxo(lbtc_asset(), available_lbtc),
            maker_base_pubkey: [0xaa; 32],
            order_nonce: [0xbb; 32],
            params: MakerOrderParams {
                base_asset_id: yes_asset(),
                quote_asset_id: lbtc_asset(),
                price,
                min_fill_lots: 1,
                min_remainder_lots: 1,
                direction: OrderDirection::SellQuote,
                maker_receive_spk_hash: [0xcc; 32],
                cosigner_pubkey: crate::taproot::NUMS_KEY_BYTES,
                maker_pubkey: [0xaa; 32],
            },
        }
    }

    // ── Sell-side tests ─────────────────────────────────────────────────

    #[test]
    fn sell_via_single_limit_order() {
        // Order: maker buys YES at 400 sats per token, covenant holds 40_000 L-BTC sats.
        // available_lots = 40_000 / 400 = 100 lots.
        let order = mock_sell_quote_order(400, 40_000);

        let plan = build_execution_plan(
            None,
            &[order],
            TradeSide::Yes,
            TradeDirection::Sell,
            10, // taker sells 10 YES tokens
            &lbtc_asset(),
            &yes_asset(),
            &no_asset(),
        )
        .unwrap();

        assert_eq!(plan.order_legs.len(), 1);
        assert!(plan.pool_leg.is_none());
        let leg = &plan.order_legs[0];
        assert_eq!(leg.lots, 10);
        assert_eq!(leg.taker_pays, 10); // taker sends 10 tokens
        assert_eq!(leg.taker_receives, 4_000); // 10 * 400 = 4000 sats
        assert_eq!(leg.maker_receive_amount, 10); // maker gets 10 tokens
        assert_eq!(plan.total_taker_input, 10);
        assert_eq!(plan.total_taker_output, 4_000);
    }

    #[test]
    fn sell_across_multiple_orders_best_price_first() {
        // Two SellQuote orders. The taker wants to sell tokens, so the best
        // orders are those paying the HIGHEST price (most sats per token).
        // Order A: 300 sats/token, covenant holds 30_000 sats → 100 lots
        let mut order_a = mock_sell_quote_order(300, 30_000);
        order_a.discovered.id = "order-a".to_string();
        // Order B: 500 sats/token, covenant holds 25_000 sats → 50 lots
        let mut order_b = mock_sell_quote_order(500, 25_000);
        order_b.discovered.id = "order-b".to_string();

        let plan = build_execution_plan(
            None,
            &[order_a, order_b],
            TradeSide::Yes,
            TradeDirection::Sell,
            70, // taker sells 70 tokens
            &lbtc_asset(),
            &yes_asset(),
            &no_asset(),
        )
        .unwrap();

        assert!(plan.pool_leg.is_none());
        // Order B (500 sats) should be filled first, then Order A (300 sats).
        assert_eq!(plan.order_legs.len(), 2);
        // First leg: order B — 50 lots at 500 sats
        assert_eq!(plan.order_legs[0].lots, 50);
        assert_eq!(plan.order_legs[0].taker_pays, 50);
        assert_eq!(plan.order_legs[0].taker_receives, 25_000);
        // Second leg: order A — remaining 20 lots at 300 sats
        assert_eq!(plan.order_legs[1].lots, 20);
        assert_eq!(plan.order_legs[1].taker_pays, 20);
        assert_eq!(plan.order_legs[1].taker_receives, 6_000);
        // Total: 50 + 20 = 70 tokens in, 25_000 + 6_000 = 31_000 sats out
        assert_eq!(plan.total_taker_input, 70);
        assert_eq!(plan.total_taker_output, 31_000);
    }

    #[test]
    fn sell_partial_fill_order_has_less_capacity() {
        // Order: 400 sats/token, covenant holds 2_000 sats → 5 lots.
        // Taker wants to sell 20 tokens, but only 5 can be filled by the order.
        // No pool → remaining 15 tokens cause NoLiquidity for the remainder,
        // but the 5-lot partial fill still appears.
        // We need a pool to absorb the remainder so the plan succeeds.
        let pool = mock_pool(1_000_000, 1_000_000, 500_000);
        let order = mock_sell_quote_order(400, 2_000);

        let plan = build_execution_plan(
            Some(&pool),
            &[order],
            TradeSide::Yes,
            TradeDirection::Sell,
            20, // taker sells 20 tokens
            &lbtc_asset(),
            &yes_asset(),
            &no_asset(),
        )
        .unwrap();

        // Order fills 5 lots (all it can afford), remainder goes to AMM.
        assert_eq!(plan.order_legs.len(), 1);
        let leg = &plan.order_legs[0];
        assert_eq!(leg.lots, 5);
        assert_eq!(leg.taker_pays, 5);
        assert_eq!(leg.taker_receives, 2_000); // 5 * 400
        assert!(!leg.is_partial); // fully consumed the order's capacity
        // Remaining 15 tokens routed through the AMM.
        assert!(plan.pool_leg.is_some());
        assert_eq!(plan.pool_leg.as_ref().unwrap().delta_in, 15);
        assert_eq!(plan.total_taker_input, 20);
    }

    #[test]
    fn sell_no_matching_orders_no_pool_returns_no_liquidity() {
        let result = build_execution_plan(
            None,
            &[],
            TradeSide::Yes,
            TradeDirection::Sell,
            10,
            &lbtc_asset(),
            &yes_asset(),
            &no_asset(),
        );

        assert!(matches!(result, Err(Error::NoLiquidity)));
    }

    #[test]
    fn sell_exact_amount_matching_order() {
        // Order: 500 sats/token, covenant holds 5_000 sats → 10 lots exactly.
        // Taker sells exactly 10 tokens.
        let order = mock_sell_quote_order(500, 5_000);

        let plan = build_execution_plan(
            None,
            &[order],
            TradeSide::Yes,
            TradeDirection::Sell,
            10,
            &lbtc_asset(),
            &yes_asset(),
            &no_asset(),
        )
        .unwrap();

        assert_eq!(plan.order_legs.len(), 1);
        assert!(plan.pool_leg.is_none());
        let leg = &plan.order_legs[0];
        assert_eq!(leg.lots, 10);
        assert_eq!(leg.taker_pays, 10);
        assert_eq!(leg.taker_receives, 5_000); // 10 * 500
        assert!(!leg.is_partial); // exact match: lots == available_lots
        assert_eq!(leg.remainder_value, 0);
        assert_eq!(plan.total_taker_input, 10);
        assert_eq!(plan.total_taker_output, 5_000);
    }
}
