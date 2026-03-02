//! Combined trade PSET builder.
//!
//! Constructs a single Partially Signed Elements Transaction (PSET) that can
//! spend from an LMSR pool and/or one or more limit orders simultaneously.
//!
//! ## PSET Layout
//!
//! ```text
//! Inputs:
//!   [IN_BASE..+2]    LMSR pool covenant (YES, NO, collateral) — if LMSR pool leg present
//!   [P..P+M-1]       M maker order covenant inputs
//!   [P+M..]          Taker wallet funding UTXOs + fee UTXO
//!
//! Outputs:
//!   [OUT_BASE..+2]   LMSR reserve outputs (explicit)          — if LMSR pool leg present
//!   [i]              Maker receive output at each maker input index `i`
//!   [j]              Remainder at `last_order_input_index + 1` (partial last only)
//!   [next]           Taker receive (blindable)
//!   [next]           Taker change (blindable, if applicable)
//!   [next]           Fee output (explicit, empty script)
//!   [next]           Fee change (blindable, if applicable)
//! ```
//!
//! The LMSR builder path supports base-indexed reserve windows where
//! `IN_BASE`/`OUT_BASE` place the three reserve slots within the combined
//! order+pool prefix.
//! Each maker order covenant checks `output(current_index())` for its
//! maker receive, so order inputs and their corresponding outputs must
//! be at matching indices. Only the last order may be a partial fill.

use std::collections::BTreeMap;

use simplicityhl::elements::Script;
use simplicityhl::elements::pset::PartiallySignedTransaction;

use crate::error::{Error, Result};
use crate::lmsr_pool::contract::CompiledLmsrPool;
use crate::lmsr_pool::table::verify_lmsr_table_proof;
use crate::maker_order::contract::CompiledMakerOrder;
use crate::maker_order::params::{
    OrderDirection, derive_maker_receive, maker_receive_script_pubkey,
};
use crate::pset::{
    UnblindedUtxo, add_pset_input, add_pset_output, explicit_txout, fee_txout, new_pset,
};

use super::types::ExecutionPlan;

// ── Parameter and result types ──────────────────────────────────────────

/// Parameters for building a combined trade PSET.
pub(crate) struct TradePsetParams<'a> {
    /// The execution plan produced by the router.
    pub plan: &'a ExecutionPlan,
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
    /// Input index range of LMSR covenant inputs (0..3 when present).
    #[allow(dead_code)] // reserved for upcoming LMSR witness attachment
    pub lmsr_input_range: Option<std::ops::Range<usize>>,
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
    let has_lmsr_pool = plan.lmsr_pool_leg.is_some();
    let has_pool = has_lmsr_pool;

    // ── Validate ────────────────────────────────────────────────────────

    if !has_pool && num_orders == 0 {
        return Err(Error::NoLiquidity);
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

    // Pool + maker order covenant inputs.
    //
    // For LMSR, we support base-indexed windows by splitting maker inputs
    // around the 3-slot reserve window at `IN_BASE`.
    let mut lmsr_input_range = None;
    if let Some(ref lmsr_leg) = plan.lmsr_pool_leg {
        if lmsr_leg.new_s_index == lmsr_leg.old_s_index {
            return Err(Error::TradeRouting(
                "LMSR swap requires NEW_S_INDEX != OLD_S_INDEX".into(),
            ));
        }
        validate_lmsr_table_membership(lmsr_leg)?;
        let in_base = usize::try_from(lmsr_leg.in_base).map_err(|_| {
            Error::TradeRouting(format!(
                "LMSR IN_BASE {} does not fit usize",
                lmsr_leg.in_base
            ))
        })?;
        if in_base > num_orders {
            return Err(Error::TradeRouting(format!(
                "LMSR IN_BASE {} exceeds order leg count {}",
                lmsr_leg.in_base, num_orders
            )));
        }

        for leg in plan.order_legs.iter().take(in_base) {
            let idx = pset.inputs().len();
            add_pset_input(&mut pset, &leg.order_utxo);
            all_input_utxos.push(leg.order_utxo.clone());
            order_input_indices.push(idx);
        }

        let reserve_start = pset.inputs().len();
        let utxos = &lmsr_leg.pool_utxos;
        for utxo in [&utxos.yes, &utxos.no, &utxos.collateral] {
            add_pset_input(&mut pset, utxo);
            all_input_utxos.push(utxo.clone());
        }
        lmsr_input_range = Some(reserve_start..reserve_start + 3);

        for leg in plan.order_legs.iter().skip(in_base) {
            let idx = pset.inputs().len();
            add_pset_input(&mut pset, &leg.order_utxo);
            all_input_utxos.push(leg.order_utxo.clone());
            order_input_indices.push(idx);
        }
    } else {
        for leg in &plan.order_legs {
            let idx = pset.inputs().len();
            add_pset_input(&mut pset, &leg.order_utxo);
            all_input_utxos.push(leg.order_utxo.clone());
            order_input_indices.push(idx);
        }
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

    let mut indexed_prefix_outputs = BTreeMap::new();
    let mut reserve_prefix_output =
        |index: usize, txout: simplicityhl::elements::TxOut, label: &str| -> Result<()> {
            if indexed_prefix_outputs.insert(index, txout).is_some() {
                return Err(Error::TradeRouting(format!(
                    "output index conflict at {index} while placing {label}"
                )));
            }
            Ok(())
        };

    if let Some(ref lmsr_leg) = plan.lmsr_pool_leg {
        let contract = CompiledLmsrPool::new(lmsr_leg.pool_params)?;
        let old_covenant_spk = contract.script_pubkey(lmsr_leg.old_s_index);
        let new_covenant_spk = contract.script_pubkey(lmsr_leg.new_s_index);
        if lmsr_leg.pool_utxos.yes.txout.script_pubkey != old_covenant_spk
            || lmsr_leg.pool_utxos.no.txout.script_pubkey != old_covenant_spk
            || lmsr_leg.pool_utxos.collateral.txout.script_pubkey != old_covenant_spk
        {
            return Err(Error::TradeRouting(
                "LMSR reserve inputs must match the old-state covenant script".into(),
            ));
        }

        let apply_delta = |start: u64, delta: i128, field: &str| -> Result<u64> {
            if delta >= 0 {
                start.checked_add(delta as u64).ok_or_else(|| {
                    Error::TradeRouting(format!("overflow applying LMSR delta to {field}"))
                })
            } else {
                start.checked_sub((-delta) as u64).ok_or_else(|| {
                    Error::TradeRouting(format!("underflow applying LMSR delta to {field}"))
                })
            }
        };
        let traded_lots = if lmsr_leg.trade_kind.is_buy() {
            lmsr_leg.delta_out
        } else {
            lmsr_leg.delta_in
        };
        let (yes_delta, no_delta) = match lmsr_leg.trade_kind {
            crate::lmsr_pool::math::LmsrTradeKind::BuyYes => (-i128::from(traded_lots), 0),
            crate::lmsr_pool::math::LmsrTradeKind::SellYes => (i128::from(traded_lots), 0),
            crate::lmsr_pool::math::LmsrTradeKind::BuyNo => (0, -i128::from(traded_lots)),
            crate::lmsr_pool::math::LmsrTradeKind::SellNo => (0, i128::from(traded_lots)),
        };

        let new_r_yes = apply_delta(lmsr_leg.pool_utxos.yes.value, yes_delta, "r_yes")?;
        let new_r_no = apply_delta(lmsr_leg.pool_utxos.no.value, no_delta, "r_no")?;
        let collateral_delta = if lmsr_leg.trade_kind.is_buy() {
            i128::from(lmsr_leg.delta_in)
        } else {
            -i128::from(lmsr_leg.delta_out)
        };
        let new_r_collateral = apply_delta(
            lmsr_leg.pool_utxos.collateral.value,
            collateral_delta,
            "r_collateral",
        )?;
        let out_base = usize::try_from(lmsr_leg.out_base).map_err(|_| {
            Error::TradeRouting(format!(
                "LMSR OUT_BASE {} does not fit usize",
                lmsr_leg.out_base
            ))
        })?;

        let pp = &lmsr_leg.pool_params;
        reserve_prefix_output(
            out_base,
            explicit_txout(&pp.yes_asset_id, new_r_yes, &new_covenant_spk),
            "LMSR YES reserve",
        )?;
        reserve_prefix_output(
            out_base + 1,
            explicit_txout(&pp.no_asset_id, new_r_no, &new_covenant_spk),
            "LMSR NO reserve",
        )?;
        reserve_prefix_output(
            out_base + 2,
            explicit_txout(&pp.collateral_asset_id, new_r_collateral, &new_covenant_spk),
            "LMSR collateral reserve",
        )?;
    }

    // Maker receive outputs are aligned 1:1 with maker input indices.
    for (i, leg) in plan.order_legs.iter().enumerate() {
        let maker_output_index = order_input_indices[i];
        let (p_order, _) =
            derive_maker_receive(&leg.maker_base_pubkey, &leg.order_nonce, &leg.params);
        let maker_receive_spk = Script::from(maker_receive_script_pubkey(&p_order));

        let receive_asset = match leg.params.direction {
            OrderDirection::SellBase => &leg.params.quote_asset_id,
            OrderDirection::SellQuote => &leg.params.base_asset_id,
        };
        reserve_prefix_output(
            maker_output_index,
            explicit_txout(receive_asset, leg.maker_receive_amount, &maker_receive_spk),
            "maker receive",
        )?;
    }

    // Last-order remainder output is pinned at `last_order_input_index + 1`.
    if let Some(last) = plan.order_legs.last()
        && last.is_partial
        && last.remainder_value > 0
    {
        let last_input_index = order_input_indices.last().copied().ok_or_else(|| {
            Error::TradeRouting("partial maker fill requires at least one maker input index".into())
        })?;
        let remainder_index = last_input_index.checked_add(1).ok_or_else(|| {
            Error::TradeRouting("overflow computing maker remainder output index".into())
        })?;
        let remainder_asset = match last.params.direction {
            OrderDirection::SellBase => &last.params.base_asset_id,
            OrderDirection::SellQuote => &last.params.quote_asset_id,
        };
        let contract = &params.order_contracts[num_orders - 1];
        let covenant_spk = contract.script_pubkey(&last.maker_base_pubkey);
        reserve_prefix_output(
            remainder_index,
            explicit_txout(remainder_asset, last.remainder_value, &covenant_spk),
            "maker remainder",
        )?;
    }

    let mut output_idx = 0usize;
    if !indexed_prefix_outputs.is_empty() {
        let max_index = *indexed_prefix_outputs
            .keys()
            .next_back()
            .expect("checked non-empty");
        for idx in 0..=max_index {
            let txout = indexed_prefix_outputs.remove(&idx).ok_or_else(|| {
                Error::TradeRouting(format!(
                    "cannot construct dense output prefix: missing output at index {idx}"
                ))
            })?;
            add_pset_output(&mut pset, txout);
        }
        output_idx = max_index + 1;
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
        lmsr_input_range,
        order_input_indices,
    })
}

fn validate_lmsr_table_membership(lmsr_leg: &super::types::LmsrPoolSwapLeg) -> Result<()> {
    let params = &lmsr_leg.pool_params;
    verify_lmsr_table_proof(
        params.lmsr_table_root,
        params.table_depth,
        lmsr_leg.old_s_index,
        lmsr_leg.old_f,
        lmsr_leg.old_path_bits,
        &lmsr_leg.old_siblings,
    )
    .map_err(|e| Error::TradeRouting(format!("invalid LMSR old-state table proof: {e}")))?;
    verify_lmsr_table_proof(
        params.lmsr_table_root,
        params.table_depth,
        lmsr_leg.new_s_index,
        lmsr_leg.new_f,
        lmsr_leg.new_path_bits,
        &lmsr_leg.new_siblings,
    )
    .map_err(|e| Error::TradeRouting(format!("invalid LMSR new-state table proof: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lmsr_pool::math::LmsrTradeKind;
    use crate::lmsr_pool::params::LmsrPoolParams;
    use crate::lmsr_pool::table::{LmsrTableManifest, lmsr_table_root};
    use crate::maker_order::params::MakerOrderParams;
    use crate::taproot::NUMS_KEY_BYTES;
    use crate::trade::types::{LmsrPoolSwapLeg, LmsrPoolUtxos, OrderFillLeg};
    use simplicityhl::elements::hashes::Hash as _;
    use simplicityhl::elements::{OutPoint, Txid};

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

    fn test_utxo_with_script(
        asset: [u8; 32],
        value: u64,
        vout: u32,
        spk: &Script,
    ) -> UnblindedUtxo {
        UnblindedUtxo {
            outpoint: OutPoint::new(Txid::all_zeros(), vout),
            txout: explicit_txout(&asset, value, spk),
            asset_id: asset,
            value,
            asset_blinding_factor: [0u8; 32],
            value_blinding_factor: [0u8; 32],
        }
    }

    fn test_utxo(asset: [u8; 32], value: u64, vout: u32) -> UnblindedUtxo {
        test_utxo_with_script(asset, value, vout, &Script::new())
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

    fn test_lmsr_values() -> Vec<u64> {
        (0u64..16).map(|i| 2_000 + i * 7).collect()
    }

    fn test_lmsr_params() -> LmsrPoolParams {
        let root = lmsr_table_root(&test_lmsr_values()).unwrap();
        LmsrPoolParams {
            yes_asset_id: yes_asset(),
            no_asset_id: no_asset(),
            collateral_asset_id: lbtc_asset(),
            lmsr_table_root: root,
            table_depth: 4,
            q_step_lots: 10,
            s_bias: 1_000,
            s_max_index: 15,
            half_payout_sats: 5_000,
            fee_bps: 30,
            min_r_yes: 1,
            min_r_no: 1,
            min_r_collateral: 1,
            cosigner_pubkey: NUMS_KEY_BYTES,
        }
    }

    fn test_lmsr_utxos(old_s: u64) -> LmsrPoolUtxos {
        let params = test_lmsr_params();
        let contract = CompiledLmsrPool::new(params).unwrap();
        let old_spk = contract.script_pubkey(old_s);
        LmsrPoolUtxos {
            yes: test_utxo_with_script(yes_asset(), 10_000, 0, &old_spk),
            no: test_utxo_with_script(no_asset(), 10_000, 1, &old_spk),
            collateral: test_utxo_with_script(lbtc_asset(), 50_000, 2, &old_spk),
        }
    }

    fn lmsr_swap_leg_with_base(
        delta_in: u64,
        delta_out: u64,
        in_base: u32,
        out_base: u32,
        old_s_index: u64,
        new_s_index: u64,
    ) -> LmsrPoolSwapLeg {
        let params = test_lmsr_params();
        let manifest = LmsrTableManifest::new(params.table_depth, test_lmsr_values()).unwrap();
        let old_proof = manifest.proof_at(old_s_index).unwrap();
        let new_proof = manifest.proof_at(new_s_index).unwrap();
        LmsrPoolSwapLeg {
            primary_path: crate::trade::types::LmsrPrimaryPath::Swap,
            pool_params: params,
            pool_id: hex::encode([0x11; 32]),
            old_s_index,
            new_s_index,
            old_path_bits: old_proof.path_bits,
            new_path_bits: new_proof.path_bits,
            old_siblings: old_proof.siblings,
            new_siblings: new_proof.siblings,
            in_base,
            out_base,
            pool_utxos: test_lmsr_utxos(old_s_index),
            trade_kind: LmsrTradeKind::BuyYes,
            old_f: old_proof.value,
            new_f: new_proof.value,
            delta_in,
            delta_out,
            admin_signature: [0u8; 64],
        }
    }

    #[test]
    fn orders_only_partial_fill_layout() {
        let nonce = [0xbb; 32];
        let params = test_order_params(400, &nonce);
        let contract = CompiledMakerOrder::new(params).unwrap();

        let plan = ExecutionPlan {
            order_legs: vec![order_fill_leg(400, 100, 10, nonce)],
            lmsr_pool_leg: None,
            taker_send_asset: lbtc_asset(),
            taker_receive_asset: yes_asset(),
            total_taker_input: 4_000,
            total_taker_output: 10,
            quoted_reserves: None,
        };

        let result = build_trade_pset(&TradePsetParams {
            plan: &plan,
            order_contracts: &[contract],
            taker_funding_utxos: vec![test_utxo(lbtc_asset(), 5_000, 30)],
            fee_utxo: test_utxo(lbtc_asset(), 500, 31),
            fee_amount: 500,
            fee_asset_id: lbtc_asset(),
            taker_receive_destination: Script::new(),
            taker_change_destination: Some(Script::new()),
        })
        .unwrap();

        assert_eq!(result.pset.n_inputs(), 3);
        assert_eq!(result.pset.n_outputs(), 5);
        assert_eq!(result.lmsr_input_range, None);
        assert_eq!(result.order_input_indices, vec![0]);
        assert_eq!(result.blind_output_indices, vec![2, 3]);
    }

    #[test]
    fn lmsr_only_layout() {
        let plan = ExecutionPlan {
            order_legs: vec![],
            lmsr_pool_leg: Some(lmsr_swap_leg_with_base(10_000, 100, 0, 0, 4, 5)),
            taker_send_asset: lbtc_asset(),
            taker_receive_asset: yes_asset(),
            total_taker_input: 10_000,
            total_taker_output: 100,
            quoted_reserves: None,
        };

        let result = build_trade_pset(&TradePsetParams {
            plan: &plan,
            order_contracts: &[],
            taker_funding_utxos: vec![test_utxo(lbtc_asset(), 10_000, 10)],
            fee_utxo: test_utxo(lbtc_asset(), 500, 11),
            fee_amount: 500,
            fee_asset_id: lbtc_asset(),
            taker_receive_destination: Script::new(),
            taker_change_destination: None,
        })
        .unwrap();

        assert_eq!(result.pset.n_inputs(), 5);
        assert_eq!(result.pset.n_outputs(), 5);
        assert_eq!(result.lmsr_input_range, Some(0..3));
        assert!(result.order_input_indices.is_empty());
        assert_eq!(result.blind_output_indices, vec![3]);
    }

    #[test]
    fn lmsr_nonzero_base_windows_with_orders() {
        let nonce1 = [0xbb; 32];
        let nonce2 = [0xcc; 32];
        let contract1 = CompiledMakerOrder::new(test_order_params(400, &nonce1)).unwrap();
        let contract2 = CompiledMakerOrder::new(test_order_params(450, &nonce2)).unwrap();

        let plan = ExecutionPlan {
            order_legs: vec![
                order_fill_leg(400, 10, 10, nonce1),
                order_fill_leg(450, 10, 10, nonce2),
            ],
            lmsr_pool_leg: Some(lmsr_swap_leg_with_base(10_000, 100, 1, 1, 4, 5)),
            taker_send_asset: lbtc_asset(),
            taker_receive_asset: yes_asset(),
            total_taker_input: 18_500,
            total_taker_output: 120,
            quoted_reserves: None,
        };

        let result = build_trade_pset(&TradePsetParams {
            plan: &plan,
            order_contracts: &[contract1, contract2],
            taker_funding_utxos: vec![test_utxo(lbtc_asset(), 18_500, 30)],
            fee_utxo: test_utxo(lbtc_asset(), 500, 31),
            fee_amount: 500,
            fee_asset_id: lbtc_asset(),
            taker_receive_destination: Script::new(),
            taker_change_destination: None,
        })
        .unwrap();

        assert_eq!(result.pset.n_inputs(), 7);
        assert_eq!(result.pset.n_outputs(), 7);
        assert_eq!(result.lmsr_input_range, Some(1..4));
        assert_eq!(result.order_input_indices, vec![0, 4]);
        assert_eq!(result.blind_output_indices, vec![5]);
    }

    #[test]
    fn lmsr_out_base_can_differ_when_dense_prefix_is_satisfied() {
        let nonce = [0xbb; 32];
        let contract = CompiledMakerOrder::new(test_order_params(400, &nonce)).unwrap();
        let plan = ExecutionPlan {
            order_legs: vec![order_fill_leg(400, 100, 10, nonce)],
            lmsr_pool_leg: Some(lmsr_swap_leg_with_base(10_000, 100, 1, 2, 4, 5)),
            taker_send_asset: lbtc_asset(),
            taker_receive_asset: yes_asset(),
            total_taker_input: 14_000,
            total_taker_output: 110,
            quoted_reserves: None,
        };

        let result = build_trade_pset(&TradePsetParams {
            plan: &plan,
            order_contracts: &[contract],
            taker_funding_utxos: vec![test_utxo(lbtc_asset(), 14_000, 10)],
            fee_utxo: test_utxo(lbtc_asset(), 500, 11),
            fee_amount: 500,
            fee_asset_id: lbtc_asset(),
            taker_receive_destination: Script::new(),
            taker_change_destination: None,
        })
        .unwrap();

        assert_eq!(result.pset.n_inputs(), 6);
        assert_eq!(result.pset.n_outputs(), 7);
        assert_eq!(result.lmsr_input_range, Some(1..4));
        assert_eq!(result.order_input_indices, vec![0]);
        assert_eq!(result.blind_output_indices, vec![5]);
    }

    #[test]
    fn error_conflicting_remainder_and_lmsr_output_index() {
        let nonce = [0xbb; 32];
        let contract = CompiledMakerOrder::new(test_order_params(400, &nonce)).unwrap();
        let leg = lmsr_swap_leg_with_base(10_000, 100, 1, 1, 4, 5);

        let plan = ExecutionPlan {
            order_legs: vec![order_fill_leg(400, 100, 10, nonce)],
            lmsr_pool_leg: Some(leg),
            taker_send_asset: lbtc_asset(),
            taker_receive_asset: yes_asset(),
            total_taker_input: 14_000,
            total_taker_output: 110,
            quoted_reserves: None,
        };

        let result = build_trade_pset(&TradePsetParams {
            plan: &plan,
            order_contracts: &[contract],
            taker_funding_utxos: vec![test_utxo(lbtc_asset(), 14_000, 10)],
            fee_utxo: test_utxo(lbtc_asset(), 500, 11),
            fee_amount: 500,
            fee_asset_id: lbtc_asset(),
            taker_receive_destination: Script::new(),
            taker_change_destination: None,
        });

        match result {
            Err(Error::TradeRouting(msg)) => assert!(msg.contains("output index conflict")),
            _ => panic!("expected output index conflict"),
        }
    }
}
