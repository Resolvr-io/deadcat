//! Combined trade PSET builder.
//!
//! Constructs a single Partially Signed Elements Transaction (PSET) that can
//! spend from an AMM pool and/or one or more limit orders simultaneously.
//!
//! ## PSET Layout
//!
//! ```text
//! Inputs:
//!   [0-3]            Pool covenant (YES, NO, LBTC, RT)  — if pool leg present
//!   [P..P+M-1]       M maker order covenant inputs       — P = 4 if pool, 0 otherwise
//!   [P+M..]          Taker wallet funding UTXOs + fee UTXO
//!
//! Outputs:
//!   [0-3]            Pool reserve outputs (explicit)      — if pool leg present
//!   [Q..Q+M-1]       M maker receive outputs (explicit)   — Q = 4 if pool, 0 otherwise
//!   [Q+M]            Remainder (explicit, only if last order is partial)
//!   [next]           Taker receive (blindable)
//!   [next]           Taker change (blindable, if applicable)
//!   [next]           Fee output (explicit, empty script)
//!   [next]           Fee change (blindable, if applicable)
//! ```
//!
//! The AMM pool covenant hard-codes outputs 0-3 for its reserves.
//! Each maker order covenant checks `output(current_index())` for its
//! maker receive, so order inputs and their corresponding outputs must
//! be at matching indices. Only the last order may be a partial fill.

use simplicityhl::elements::Script;
use simplicityhl::elements::pset::PartiallySignedTransaction;

use crate::amm_pool::contract::CompiledAmmPool;
use crate::error::{Error, Result};
use crate::maker_order::contract::CompiledMakerOrder;
use crate::maker_order::params::{
    OrderDirection, derive_maker_receive, maker_receive_script_pubkey,
};
use crate::pset::{
    UnblindedUtxo, add_pset_input, add_pset_output, explicit_txout, fee_txout, new_pset,
    reissuance_token_output,
};

use super::types::ExecutionPlan;

// ── Parameter and result types ──────────────────────────────────────────

/// Parameters for building a combined trade PSET.
pub(crate) struct TradePsetParams<'a> {
    /// The execution plan produced by the router.
    pub plan: &'a ExecutionPlan,
    /// Compiled AMM pool contract (required when `plan.pool_leg` is `Some`).
    pub pool_contract: Option<&'a CompiledAmmPool>,
    /// Compiled maker order contracts, one per `plan.order_legs` entry (same order).
    pub order_contracts: &'a [CompiledMakerOrder],
    /// Taker's wallet funding UTXOs (must total at least `plan.total_taker_input`).
    pub taker_funding_utxos: Vec<UnblindedUtxo>,
    /// Fee UTXO (must have value >= `fee_amount`).
    pub fee_utxo: UnblindedUtxo,
    /// Fee amount in sats.
    pub fee_amount: u64,
    /// Fee asset ID (typically L-BTC policy asset).
    pub fee_asset_id: [u8; 32],
    /// Destination for the taker's received tokens/collateral.
    pub taker_receive_destination: Script,
    /// Destination for taker change (required when overfunded).
    pub taker_change_destination: Option<Script>,
}

/// Result of building a trade PSET, with metadata needed for blinding
/// and Simplicity witness attachment.
pub(crate) struct TradePsetResult {
    /// The constructed PSET (pre-blinding, pre-witness).
    pub pset: PartiallySignedTransaction,
    /// All input UTXOs in PSET input order (for blinding surjection proofs).
    pub all_input_utxos: Vec<UnblindedUtxo>,
    /// Output indices that should be blinded (taker receive, taker change, fee change).
    pub blind_output_indices: Vec<usize>,
    /// Input index range of pool covenant inputs (0..4 when present).
    #[allow(dead_code)] // useful for test assertions; attach_amm_pool_witnesses assumes 0..4
    pub pool_input_range: Option<std::ops::Range<usize>>,
    /// Input indices of maker order covenant inputs.
    pub order_input_indices: Vec<usize>,
}

// ── Builder ─────────────────────────────────────────────────────────────

/// Build a combined trade PSET from a routed execution plan.
///
/// The resulting PSET is unsigned and unblinded. The caller must:
/// 1. Blind the outputs listed in `blind_output_indices`.
/// 2. Attach Simplicity witnesses to covenant inputs.
/// 3. Sign wallet inputs and finalize.
pub(crate) fn build_trade_pset(params: &TradePsetParams) -> Result<TradePsetResult> {
    let plan = params.plan;
    let num_orders = plan.order_legs.len();
    let has_pool = plan.pool_leg.is_some();

    // ── Validate ────────────────────────────────────────────────────────

    if !has_pool && num_orders == 0 {
        return Err(Error::NoLiquidity);
    }
    if has_pool && params.pool_contract.is_none() {
        return Err(Error::TradeRouting(
            "pool_contract required when pool_leg is present".into(),
        ));
    }
    if params.order_contracts.len() != num_orders {
        return Err(Error::TradeRouting(format!(
            "expected {} order contracts, got {}",
            num_orders,
            params.order_contracts.len()
        )));
    }
    if params.taker_funding_utxos.is_empty() {
        return Err(Error::TradeRouting(
            "at least one taker funding UTXO required".into(),
        ));
    }
    if params.fee_utxo.value < params.fee_amount {
        return Err(Error::InsufficientFee);
    }
    for (i, leg) in plan.order_legs.iter().enumerate() {
        if leg.is_partial && i < num_orders - 1 {
            return Err(Error::PartialFillNotLast);
        }
    }
    let taker_total: u64 = params.taker_funding_utxos.iter().map(|u| u.value).sum();
    if taker_total < plan.total_taker_input {
        return Err(Error::TradeRouting(format!(
            "insufficient taker funding: have {taker_total}, need {}",
            plan.total_taker_input
        )));
    }

    let mut pset = new_pset();
    let mut all_input_utxos = Vec::new();
    let mut blind_output_indices = Vec::new();
    let mut order_input_indices = Vec::new();

    // ── INPUTS ──────────────────────────────────────────────────────────

    // Pool covenant inputs at indices 0-3.
    let pool_input_range = if let Some(ref pool_leg) = plan.pool_leg {
        let utxos = &pool_leg.pool_utxos;
        for utxo in [&utxos.yes, &utxos.no, &utxos.lbtc, &utxos.rt] {
            add_pset_input(&mut pset, utxo);
            all_input_utxos.push(utxo.clone());
        }
        Some(0..4)
    } else {
        None
    };

    // Maker order covenant inputs.
    let order_start = if has_pool { 4 } else { 0 };
    for (i, leg) in plan.order_legs.iter().enumerate() {
        add_pset_input(&mut pset, &leg.order_utxo);
        all_input_utxos.push(leg.order_utxo.clone());
        order_input_indices.push(order_start + i);
    }

    // Taker wallet funding inputs.
    for utxo in &params.taker_funding_utxos {
        add_pset_input(&mut pset, utxo);
        all_input_utxos.push(utxo.clone());
    }

    // Fee input.
    add_pset_input(&mut pset, &params.fee_utxo);
    all_input_utxos.push(params.fee_utxo.clone());

    // ── OUTPUTS ─────────────────────────────────────────────────────────

    let mut output_idx = 0usize;

    // Pool reserve outputs at indices 0-3.
    if let Some(ref pool_leg) = plan.pool_leg {
        let contract = params.pool_contract.unwrap();
        let covenant_spk = contract.script_pubkey(pool_leg.issued_lp);
        let pp = contract.params();

        add_pset_output(
            &mut pset,
            explicit_txout(&pp.yes_asset_id, pool_leg.new_reserves.r_yes, &covenant_spk),
        );
        add_pset_output(
            &mut pset,
            explicit_txout(&pp.no_asset_id, pool_leg.new_reserves.r_no, &covenant_spk),
        );
        add_pset_output(
            &mut pset,
            explicit_txout(
                &pp.lbtc_asset_id,
                pool_leg.new_reserves.r_lbtc,
                &covenant_spk,
            ),
        );
        // RT passthrough (Null placeholder, filled by blinder in sdk.rs).
        add_pset_output(&mut pset, reissuance_token_output(&covenant_spk));
        output_idx = 4;
    }

    // Maker receive outputs (aligned 1:1 with order inputs).
    for leg in &plan.order_legs {
        let (p_order, _) =
            derive_maker_receive(&leg.maker_base_pubkey, &leg.order_nonce, &leg.params);
        let maker_receive_spk = Script::from(maker_receive_script_pubkey(&p_order));

        let receive_asset = match leg.params.direction {
            OrderDirection::SellBase => &leg.params.quote_asset_id,
            OrderDirection::SellQuote => &leg.params.base_asset_id,
        };
        add_pset_output(
            &mut pset,
            explicit_txout(receive_asset, leg.maker_receive_amount, &maker_receive_spk),
        );
        output_idx += 1;
    }

    // Remainder output (only for the last order if partially filled).
    if let Some(last) = plan.order_legs.last()
        && last.is_partial
        && last.remainder_value > 0
    {
        let remainder_asset = match last.params.direction {
            OrderDirection::SellBase => &last.params.base_asset_id,
            OrderDirection::SellQuote => &last.params.quote_asset_id,
        };
        let contract = &params.order_contracts[num_orders - 1];
        let covenant_spk = contract.script_pubkey(&last.maker_base_pubkey);
        add_pset_output(
            &mut pset,
            explicit_txout(remainder_asset, last.remainder_value, &covenant_spk),
        );
        output_idx += 1;
    }

    // Taker receive output.
    add_pset_output(
        &mut pset,
        explicit_txout(
            &plan.taker_receive_asset,
            plan.total_taker_output,
            &params.taker_receive_destination,
        ),
    );
    blind_output_indices.push(output_idx);
    output_idx += 1;

    // Taker change output.
    let taker_change = taker_total - plan.total_taker_input;
    if taker_change > 0 {
        if let Some(ref change_spk) = params.taker_change_destination {
            add_pset_output(
                &mut pset,
                explicit_txout(&plan.taker_send_asset, taker_change, change_spk),
            );
            blind_output_indices.push(output_idx);
            output_idx += 1;
        } else {
            return Err(Error::MissingChangeDestination);
        }
    }

    // Fee output.
    add_pset_output(
        &mut pset,
        fee_txout(&params.fee_asset_id, params.fee_amount),
    );
    output_idx += 1;

    // Fee change.
    let fee_change = params.fee_utxo.value - params.fee_amount;
    if fee_change > 0 {
        if let Some(ref change_spk) = params.taker_change_destination {
            add_pset_output(
                &mut pset,
                explicit_txout(&params.fee_asset_id, fee_change, change_spk),
            );
            blind_output_indices.push(output_idx);
        } else {
            return Err(Error::MissingChangeDestination);
        }
    }

    Ok(TradePsetResult {
        pset,
        all_input_utxos,
        blind_output_indices,
        pool_input_range,
        order_input_indices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::amm_pool::math::{PoolReserves, SwapDirection, SwapPair};
    use crate::amm_pool::params::AmmPoolParams;
    use crate::maker_order::params::MakerOrderParams;
    use crate::taproot::NUMS_KEY_BYTES;
    use crate::trade::types::{OrderFillLeg, PoolSwapLeg, PoolUtxos};
    use simplicityhl::elements::hashes::Hash as _;
    use simplicityhl::elements::{OutPoint, Txid};

    // ── Helpers ─────────────────────────────────────────────────────────

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

    fn test_utxo(asset: [u8; 32], value: u64, vout: u32) -> UnblindedUtxo {
        UnblindedUtxo {
            outpoint: OutPoint::new(Txid::all_zeros(), vout),
            txout: explicit_txout(&asset, value, &Script::new()),
            asset_id: asset,
            value,
            asset_blinding_factor: [0u8; 32],
            value_blinding_factor: [0u8; 32],
        }
    }

    fn test_pool_params() -> AmmPoolParams {
        AmmPoolParams {
            yes_asset_id: yes_asset(),
            no_asset_id: no_asset(),
            lbtc_asset_id: lbtc_asset(),
            lp_asset_id: [0x04; 32],
            lp_reissuance_token_id: [0x05; 32],
            fee_bps: 30,
            cosigner_pubkey: NUMS_KEY_BYTES,
        }
    }

    fn test_pool_utxos(r_yes: u64, r_no: u64, r_lbtc: u64) -> PoolUtxos {
        PoolUtxos {
            yes: test_utxo(yes_asset(), r_yes, 0),
            no: test_utxo(no_asset(), r_no, 1),
            lbtc: test_utxo(lbtc_asset(), r_lbtc, 2),
            rt: test_utxo([0x05; 32], 1, 3),
        }
    }

    fn test_order_params(price: u64, nonce: &[u8; 32]) -> MakerOrderParams {
        let (params, _) = MakerOrderParams::new(
            yes_asset(),
            lbtc_asset(),
            price,
            1,
            1,
            OrderDirection::SellBase,
            NUMS_KEY_BYTES,
            &[0xaa; 32],
            nonce,
        );
        params
    }

    fn pool_swap_leg(delta_in: u64, delta_out: u64) -> PoolSwapLeg {
        let pp = test_pool_params();
        PoolSwapLeg {
            pool_params: pp,
            issued_lp: 1000,
            pool_utxos: test_pool_utxos(10_000, 10_000, 50_000),
            swap_pair: SwapPair::YesLbtc,
            swap_direction: SwapDirection::SellB,
            delta_in,
            delta_out,
            new_reserves: PoolReserves {
                r_yes: 10_000 - delta_out,
                r_no: 10_000,
                r_lbtc: 50_000 + delta_in,
            },
        }
    }

    fn order_fill_leg(price: u64, available: u64, lots: u64, nonce: [u8; 32]) -> OrderFillLeg {
        let params = test_order_params(price, &nonce);
        let is_partial = lots < available;
        OrderFillLeg {
            params,
            maker_base_pubkey: [0xaa; 32],
            order_nonce: nonce,
            order_utxo: test_utxo(yes_asset(), available, 20),
            lots,
            taker_pays: lots * price,
            taker_receives: lots,
            maker_receive_amount: lots * price,
            is_partial,
            remainder_value: available - lots,
        }
    }

    // ── Pool-only ───────────────────────────────────────────────────────

    #[test]
    fn pool_only_layout() {
        let pp = test_pool_params();
        let contract = CompiledAmmPool::new(pp).unwrap();

        let plan = ExecutionPlan {
            order_legs: vec![],
            pool_leg: Some(pool_swap_leg(10_000, 100)),
            taker_send_asset: lbtc_asset(),
            taker_receive_asset: yes_asset(),
            total_taker_input: 10_000,
            total_taker_output: 100,
            quoted_reserves: None,
        };

        let result = build_trade_pset(&TradePsetParams {
            plan: &plan,
            pool_contract: Some(&contract),
            order_contracts: &[],
            taker_funding_utxos: vec![test_utxo(lbtc_asset(), 10_000, 10)],
            fee_utxo: test_utxo(lbtc_asset(), 500, 11),
            fee_amount: 500,
            fee_asset_id: lbtc_asset(),
            taker_receive_destination: Script::new(),
            taker_change_destination: None,
        })
        .unwrap();

        // 4 pool + 1 taker + 1 fee = 6 inputs
        assert_eq!(result.pset.n_inputs(), 6);
        assert_eq!(result.all_input_utxos.len(), 6);
        // 4 pool + taker_receive + fee = 6 outputs
        assert_eq!(result.pset.n_outputs(), 6);
        assert_eq!(result.pool_input_range, Some(0..4));
        assert!(result.order_input_indices.is_empty());
        // Blind only taker receive (output 4)
        assert_eq!(result.blind_output_indices, vec![4]);
    }

    // ── Orders-only with partial fill ───────────────────────────────────

    #[test]
    fn orders_only_partial_fill() {
        let nonce = [0xbb; 32];
        let params = test_order_params(400, &nonce);
        let contract = CompiledMakerOrder::new(params).unwrap();

        let plan = ExecutionPlan {
            order_legs: vec![order_fill_leg(400, 100, 10, nonce)],
            pool_leg: None,
            taker_send_asset: lbtc_asset(),
            taker_receive_asset: yes_asset(),
            total_taker_input: 4_000,
            total_taker_output: 10,
            quoted_reserves: None,
        };

        let result = build_trade_pset(&TradePsetParams {
            plan: &plan,
            pool_contract: None,
            order_contracts: &[contract],
            taker_funding_utxos: vec![test_utxo(lbtc_asset(), 5_000, 30)],
            fee_utxo: test_utxo(lbtc_asset(), 500, 31),
            fee_amount: 500,
            fee_asset_id: lbtc_asset(),
            taker_receive_destination: Script::new(),
            taker_change_destination: Some(Script::new()),
        })
        .unwrap();

        // 1 order + 1 taker + 1 fee = 3 inputs
        assert_eq!(result.pset.n_inputs(), 3);
        // maker_receive + remainder + taker_receive + taker_change + fee = 5
        assert_eq!(result.pset.n_outputs(), 5);
        assert_eq!(result.pool_input_range, None);
        assert_eq!(result.order_input_indices, vec![0]);
        // Blind: taker_receive (idx 2), taker_change (idx 3)
        assert_eq!(result.blind_output_indices, vec![2, 3]);
    }

    // ── Combined pool + order (full fill) ───────────────────────────────

    #[test]
    fn combined_pool_and_order() {
        let pp = test_pool_params();
        let pool_contract = CompiledAmmPool::new(pp).unwrap();
        let nonce = [0xbb; 32];
        let order_params = test_order_params(400, &nonce);
        let order_contract = CompiledMakerOrder::new(order_params).unwrap();

        let plan = ExecutionPlan {
            order_legs: vec![order_fill_leg(400, 10, 10, nonce)], // full fill
            pool_leg: Some(pool_swap_leg(6_000, 60)),
            taker_send_asset: lbtc_asset(),
            taker_receive_asset: yes_asset(),
            total_taker_input: 10_000,
            total_taker_output: 70,
            quoted_reserves: None,
        };

        let result = build_trade_pset(&TradePsetParams {
            plan: &plan,
            pool_contract: Some(&pool_contract),
            order_contracts: &[order_contract],
            taker_funding_utxos: vec![test_utxo(lbtc_asset(), 12_000, 30)],
            fee_utxo: test_utxo(lbtc_asset(), 600, 31),
            fee_amount: 500,
            fee_asset_id: lbtc_asset(),
            taker_receive_destination: Script::new(),
            taker_change_destination: Some(Script::new()),
        })
        .unwrap();

        // 4 pool + 1 order + 1 taker + 1 fee = 7 inputs
        assert_eq!(result.pset.n_inputs(), 7);
        // 4 pool + maker_receive + taker_receive + taker_change + fee + fee_change = 9
        assert_eq!(result.pset.n_outputs(), 9);
        assert_eq!(result.pool_input_range, Some(0..4));
        assert_eq!(result.order_input_indices, vec![4]);
        // Blind: taker_receive (5), taker_change (6), fee_change (8)
        assert_eq!(result.blind_output_indices, vec![5, 6, 8]);
    }

    // ── Combined pool + order (partial fill) ────────────────────────────

    #[test]
    fn combined_with_partial_fill() {
        let pp = test_pool_params();
        let pool_contract = CompiledAmmPool::new(pp).unwrap();
        let nonce = [0xbb; 32];
        let order_params = test_order_params(400, &nonce);
        let order_contract = CompiledMakerOrder::new(order_params).unwrap();

        let plan = ExecutionPlan {
            order_legs: vec![order_fill_leg(400, 100, 10, nonce)], // partial: 10/100
            pool_leg: Some(pool_swap_leg(6_000, 60)),
            taker_send_asset: lbtc_asset(),
            taker_receive_asset: yes_asset(),
            total_taker_input: 10_000,
            total_taker_output: 70,
            quoted_reserves: None,
        };

        let result = build_trade_pset(&TradePsetParams {
            plan: &plan,
            pool_contract: Some(&pool_contract),
            order_contracts: &[order_contract],
            taker_funding_utxos: vec![test_utxo(lbtc_asset(), 10_000, 30)],
            fee_utxo: test_utxo(lbtc_asset(), 500, 31),
            fee_amount: 500,
            fee_asset_id: lbtc_asset(),
            taker_receive_destination: Script::new(),
            taker_change_destination: None,
        })
        .unwrap();

        // 4 pool + 1 order + 1 taker + 1 fee = 7
        assert_eq!(result.pset.n_inputs(), 7);
        // 4 pool + maker_receive + remainder + taker_receive + fee = 8
        assert_eq!(result.pset.n_outputs(), 8);
        // Blind only taker_receive (idx 6)
        assert_eq!(result.blind_output_indices, vec![6]);
    }

    // ── Multiple orders without pool ────────────────────────────────────

    #[test]
    fn multiple_orders_no_pool() {
        let nonce1 = [0xbb; 32];
        let nonce2 = [0xcc; 32];
        let params1 = test_order_params(400, &nonce1);
        let params2 = test_order_params(450, &nonce2);
        let contract1 = CompiledMakerOrder::new(params1).unwrap();
        let contract2 = CompiledMakerOrder::new(params2).unwrap();

        let plan = ExecutionPlan {
            order_legs: vec![
                order_fill_leg(400, 50, 50, nonce1), // full fill
                order_fill_leg(450, 30, 20, nonce2), // partial: 20/30
            ],
            pool_leg: None,
            taker_send_asset: lbtc_asset(),
            taker_receive_asset: yes_asset(),
            total_taker_input: 50 * 400 + 20 * 450, // 20_000 + 9_000 = 29_000
            total_taker_output: 70,
            quoted_reserves: None,
        };

        let result = build_trade_pset(&TradePsetParams {
            plan: &plan,
            pool_contract: None,
            order_contracts: &[contract1, contract2],
            taker_funding_utxos: vec![test_utxo(lbtc_asset(), 30_000, 30)],
            fee_utxo: test_utxo(lbtc_asset(), 500, 31),
            fee_amount: 500,
            fee_asset_id: lbtc_asset(),
            taker_receive_destination: Script::new(),
            taker_change_destination: Some(Script::new()),
        })
        .unwrap();

        // 2 orders + 1 taker + 1 fee = 4 inputs
        assert_eq!(result.pset.n_inputs(), 4);
        // 2 maker_receive + remainder + taker_receive + taker_change + fee = 6
        assert_eq!(result.pset.n_outputs(), 6);
        assert_eq!(result.order_input_indices, vec![0, 1]);
        // Blind: taker_receive (3), taker_change (4)
        assert_eq!(result.blind_output_indices, vec![3, 4]);
    }

    // ── Error cases ─────────────────────────────────────────────────────

    #[test]
    fn error_insufficient_taker_funding() {
        let pp = test_pool_params();
        let contract = CompiledAmmPool::new(pp).unwrap();

        let plan = ExecutionPlan {
            order_legs: vec![],
            pool_leg: Some(pool_swap_leg(10_000, 100)),
            taker_send_asset: lbtc_asset(),
            taker_receive_asset: yes_asset(),
            total_taker_input: 10_000,
            total_taker_output: 100,
            quoted_reserves: None,
        };

        let result = build_trade_pset(&TradePsetParams {
            plan: &plan,
            pool_contract: Some(&contract),
            order_contracts: &[],
            taker_funding_utxos: vec![test_utxo(lbtc_asset(), 5_000, 10)],
            fee_utxo: test_utxo(lbtc_asset(), 500, 11),
            fee_amount: 500,
            fee_asset_id: lbtc_asset(),
            taker_receive_destination: Script::new(),
            taker_change_destination: None,
        });

        assert!(matches!(result, Err(Error::TradeRouting(_))));
    }

    #[test]
    fn error_insufficient_fee() {
        let pp = test_pool_params();
        let contract = CompiledAmmPool::new(pp).unwrap();

        let plan = ExecutionPlan {
            order_legs: vec![],
            pool_leg: Some(pool_swap_leg(10_000, 100)),
            taker_send_asset: lbtc_asset(),
            taker_receive_asset: yes_asset(),
            total_taker_input: 10_000,
            total_taker_output: 100,
            quoted_reserves: None,
        };

        let result = build_trade_pset(&TradePsetParams {
            plan: &plan,
            pool_contract: Some(&contract),
            order_contracts: &[],
            taker_funding_utxos: vec![test_utxo(lbtc_asset(), 10_000, 10)],
            fee_utxo: test_utxo(lbtc_asset(), 100, 11),
            fee_amount: 500,
            fee_asset_id: lbtc_asset(),
            taker_receive_destination: Script::new(),
            taker_change_destination: None,
        });

        assert!(matches!(result, Err(Error::InsufficientFee)));
    }

    #[test]
    fn error_missing_change_destination() {
        let pp = test_pool_params();
        let contract = CompiledAmmPool::new(pp).unwrap();

        let plan = ExecutionPlan {
            order_legs: vec![],
            pool_leg: Some(pool_swap_leg(10_000, 100)),
            taker_send_asset: lbtc_asset(),
            taker_receive_asset: yes_asset(),
            total_taker_input: 10_000,
            total_taker_output: 100,
            quoted_reserves: None,
        };

        let result = build_trade_pset(&TradePsetParams {
            plan: &plan,
            pool_contract: Some(&contract),
            order_contracts: &[],
            taker_funding_utxos: vec![test_utxo(lbtc_asset(), 15_000, 10)],
            fee_utxo: test_utxo(lbtc_asset(), 500, 11),
            fee_amount: 500,
            fee_asset_id: lbtc_asset(),
            taker_receive_destination: Script::new(),
            taker_change_destination: None,
        });

        assert!(matches!(result, Err(Error::MissingChangeDestination)));
    }

    #[test]
    fn error_no_liquidity() {
        let plan = ExecutionPlan {
            order_legs: vec![],
            pool_leg: None,
            taker_send_asset: lbtc_asset(),
            taker_receive_asset: yes_asset(),
            total_taker_input: 10_000,
            total_taker_output: 0,
            quoted_reserves: None,
        };

        let result = build_trade_pset(&TradePsetParams {
            plan: &plan,
            pool_contract: None,
            order_contracts: &[],
            taker_funding_utxos: vec![test_utxo(lbtc_asset(), 10_000, 10)],
            fee_utxo: test_utxo(lbtc_asset(), 500, 11),
            fee_amount: 500,
            fee_asset_id: lbtc_asset(),
            taker_receive_destination: Script::new(),
            taker_change_destination: None,
        });

        assert!(matches!(result, Err(Error::NoLiquidity)));
    }

    #[test]
    fn error_contract_count_mismatch() {
        let nonce = [0xbb; 32];

        let plan = ExecutionPlan {
            order_legs: vec![order_fill_leg(400, 10, 10, nonce)],
            pool_leg: None,
            taker_send_asset: lbtc_asset(),
            taker_receive_asset: yes_asset(),
            total_taker_input: 4_000,
            total_taker_output: 10,
            quoted_reserves: None,
        };

        // Pass empty contracts for 1 order leg
        let result = build_trade_pset(&TradePsetParams {
            plan: &plan,
            pool_contract: None,
            order_contracts: &[],
            taker_funding_utxos: vec![test_utxo(lbtc_asset(), 4_000, 10)],
            fee_utxo: test_utxo(lbtc_asset(), 500, 11),
            fee_amount: 500,
            fee_asset_id: lbtc_asset(),
            taker_receive_destination: Script::new(),
            taker_change_destination: None,
        });

        assert!(matches!(result, Err(Error::TradeRouting(_))));
    }
}
