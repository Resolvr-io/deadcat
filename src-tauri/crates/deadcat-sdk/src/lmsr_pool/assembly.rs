use std::ops::Range;
use std::sync::Arc;

use lwk_wollet::elements::pset::PartiallySignedTransaction;
use simplicityhl::elements::hashes::Hash;
use simplicityhl::elements::taproot::ControlBlock;
use simplicityhl::simplicity::jet::elements::{ElementsEnv, ElementsUtxo};

use crate::assembly::pset_to_pruning_transaction;
use crate::error::{Error, Result};
use crate::trade::types::LmsrPoolSwapLeg;

use super::contract::CompiledLmsrPool;
use super::witness::{
    satisfy_lmsr_primary_with_env, satisfy_lmsr_secondary_with_env, serialize_satisfied,
};

/// Attach LMSR pool witness stacks to covenant inputs in the PSET.
///
/// - Primary covenant input: `IN_BASE + 0`
/// - Secondary covenant inputs: `IN_BASE + 1`, `IN_BASE + 2`
pub fn attach_lmsr_pool_witnesses(
    pset: &mut PartiallySignedTransaction,
    leg: &LmsrPoolSwapLeg,
    lmsr_input_range: Range<usize>,
) -> Result<()> {
    let input_start = lmsr_input_range.start;
    let input_end = lmsr_input_range.end;
    let input_count = input_end.saturating_sub(input_start);
    if input_count != 3 {
        return Err(Error::Pset(format!(
            "LMSR witness attachment requires exactly 3 covenant inputs, got range {lmsr_input_range:?}"
        )));
    }
    if pset.inputs().len() < input_end {
        return Err(Error::Pset(format!(
            "LMSR witness attachment range {lmsr_input_range:?} exceeds input count {}",
            pset.inputs().len()
        )));
    }
    for idx in input_start..input_end {
        if pset.inputs()[idx].witness_utxo.is_none() {
            return Err(Error::Pset(format!(
                "input {idx} missing witness_utxo for LMSR witness attachment"
            )));
        }
    }

    let in_base = usize::try_from(leg.in_base)
        .map_err(|_| Error::TradeRouting("LMSR IN_BASE does not fit usize".into()))?;
    if in_base != input_start {
        return Err(Error::TradeRouting(format!(
            "LMSR IN_BASE {} does not match input range start {}",
            leg.in_base, input_start
        )));
    }

    let out_base = usize::try_from(leg.out_base)
        .map_err(|_| Error::TradeRouting("LMSR OUT_BASE does not fit usize".into()))?;
    if pset.outputs().len() < out_base + 3 {
        return Err(Error::Pset(format!(
            "LMSR OUT_BASE window [{out_base}..{}) exceeds output count {}",
            out_base + 3,
            pset.outputs().len()
        )));
    }

    let contract = CompiledLmsrPool::new(leg.pool_params)?;
    let old_spk = contract.script_pubkey(leg.old_s_index);
    for utxo in [
        &leg.pool_utxos.yes,
        &leg.pool_utxos.no,
        &leg.pool_utxos.collateral,
    ] {
        if utxo.txout.script_pubkey != old_spk {
            return Err(Error::TradeRouting(
                "LMSR reserve input script does not match expected old state script".into(),
            ));
        }
    }
    let new_spk = contract.script_pubkey(leg.new_s_index);
    for idx in out_base..out_base + 3 {
        if pset.outputs()[idx].script_pubkey != new_spk {
            return Err(Error::TradeRouting(format!(
                "LMSR reserve output {idx} script does not match expected new state script"
            )));
        }
    }

    let tx = Arc::new(pset_to_pruning_transaction(pset)?);
    let utxos: Vec<ElementsUtxo> = pset
        .inputs()
        .iter()
        .enumerate()
        .map(|(i, inp)| {
            inp.witness_utxo
                .as_ref()
                .map(|u| ElementsUtxo::from(u.clone()))
                .ok_or_else(|| Error::Pset(format!("input {i} missing witness_utxo")))
        })
        .collect::<Result<Vec<_>>>()?;

    let build_primary_stack = |input_index: usize| -> Result<Vec<Vec<u8>>> {
        let cmr = *contract.primary_cmr();
        let cb_bytes = contract.primary_control_block(leg.old_s_index);
        let control_block = ControlBlock::from_slice(&cb_bytes)
            .map_err(|e| Error::Witness(format!("LMSR primary control block: {e}")))?;
        let env = ElementsEnv::new(
            Arc::clone(&tx),
            utxos.clone(),
            input_index as u32,
            cmr,
            control_block,
            None,
            lwk_wollet::elements::BlockHash::all_zeros(),
        );
        let satisfied = satisfy_lmsr_primary_with_env(&contract, leg, Some(&env))?;
        let (program_bytes, witness_bytes) = serialize_satisfied(&satisfied);
        let cmr_bytes = cmr.to_byte_array().to_vec();
        let stack = vec![witness_bytes, program_bytes, cmr_bytes, cb_bytes];
        debug_assert!(
            satisfied.redeem().bounds().cost.is_budget_valid(&stack),
            "lmsr primary input {input_index}: Simplicity program cost exceeds witness budget"
        );
        Ok(stack)
    };

    let build_secondary_stack = |input_index: usize| -> Result<Vec<Vec<u8>>> {
        let cmr = *contract.secondary_cmr();
        let cb_bytes = contract.secondary_control_block(leg.old_s_index);
        let control_block = ControlBlock::from_slice(&cb_bytes)
            .map_err(|e| Error::Witness(format!("LMSR secondary control block: {e}")))?;
        let env = ElementsEnv::new(
            Arc::clone(&tx),
            utxos.clone(),
            input_index as u32,
            cmr,
            control_block,
            None,
            lwk_wollet::elements::BlockHash::all_zeros(),
        );
        let satisfied = satisfy_lmsr_secondary_with_env(&contract, leg.in_base, Some(&env))?;
        let (program_bytes, witness_bytes) = serialize_satisfied(&satisfied);
        let cmr_bytes = cmr.to_byte_array().to_vec();
        let stack = vec![witness_bytes, program_bytes, cmr_bytes, cb_bytes];
        debug_assert!(
            satisfied.redeem().bounds().cost.is_budget_valid(&stack),
            "lmsr secondary input {input_index}: Simplicity program cost exceeds witness budget"
        );
        Ok(stack)
    };

    let primary_idx = input_start;
    pset.inputs_mut()[primary_idx].final_script_witness = Some(build_primary_stack(primary_idx)?);

    for idx in primary_idx + 1..primary_idx + 3 {
        pset.inputs_mut()[idx].final_script_witness = Some(build_secondary_stack(idx)?);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lmsr_pool::math::LmsrTradeKind;
    use crate::lmsr_pool::params::LmsrPoolParams;
    use crate::lmsr_pool::table::{LmsrTableManifest, lmsr_table_root};
    use crate::pset::{UnblindedUtxo, add_pset_input, add_pset_output, explicit_txout, new_pset};
    use crate::trade::types::{LmsrPoolUtxos, LmsrPrimaryPath};
    use lwk_wollet::elements::{OutPoint, Script, Txid};

    fn dummy_utxo(asset_id: [u8; 32], value: u64, vout: u32, spk: &Script) -> UnblindedUtxo {
        let txout = explicit_txout(&asset_id, value, spk);
        UnblindedUtxo {
            outpoint: OutPoint::new(Txid::all_zeros(), vout),
            txout,
            asset_id,
            value,
            asset_blinding_factor: [0u8; 32],
            value_blinding_factor: [0u8; 32],
        }
    }

    fn test_leg() -> LmsrPoolSwapLeg {
        let yes = [0x11; 32];
        let no = [0x22; 32];
        let collateral = [0x33; 32];
        let table_values: Vec<u64> = (0..16u64).map(|i| 2_000 + i * 10).collect();
        let table_root = lmsr_table_root(&table_values).unwrap();
        let manifest = LmsrTableManifest::new(4, table_values.clone()).unwrap();
        let old_proof = manifest.proof_at(8).unwrap();
        let new_proof = manifest.proof_at(9).unwrap();
        let params = LmsrPoolParams {
            yes_asset_id: yes,
            no_asset_id: no,
            collateral_asset_id: collateral,
            lmsr_table_root: table_root,
            table_depth: 4,
            q_step_lots: 10,
            s_bias: 8,
            s_max_index: 15,
            half_payout_sats: 1_000,
            fee_bps: 30,
            min_r_yes: 1,
            min_r_no: 1,
            min_r_collateral: 1,
            cosigner_pubkey: crate::taproot::NUMS_KEY_BYTES,
        };
        let contract = CompiledLmsrPool::new(params).unwrap();
        let old_spk = contract.script_pubkey(8);

        LmsrPoolSwapLeg {
            primary_path: LmsrPrimaryPath::Swap,
            pool_params: params,
            pool_id: "lmsr-test-pool".into(),
            old_s_index: 8,
            new_s_index: 9,
            old_path_bits: old_proof.path_bits,
            new_path_bits: new_proof.path_bits,
            old_siblings: old_proof.siblings,
            new_siblings: new_proof.siblings,
            in_base: 0,
            out_base: 0,
            pool_utxos: LmsrPoolUtxos {
                yes: dummy_utxo(yes, 50_000, 0, &old_spk),
                no: dummy_utxo(no, 50_000, 1, &old_spk),
                collateral: dummy_utxo(collateral, 100_000, 2, &old_spk),
            },
            trade_kind: LmsrTradeKind::BuyYes,
            old_f: table_values[8],
            new_f: table_values[9],
            delta_in: 10_131,
            delta_out: 10,
            admin_signature: [0u8; 64],
        }
    }

    fn build_swap_pset(leg: &LmsrPoolSwapLeg) -> PartiallySignedTransaction {
        let contract = CompiledLmsrPool::new(leg.pool_params).unwrap();
        let new_spk = contract.script_pubkey(leg.new_s_index);
        let mut pset = new_pset();
        add_pset_input(&mut pset, &leg.pool_utxos.yes);
        add_pset_input(&mut pset, &leg.pool_utxos.no);
        add_pset_input(&mut pset, &leg.pool_utxos.collateral);
        add_pset_output(
            &mut pset,
            explicit_txout(
                &leg.pool_params.yes_asset_id,
                leg.pool_utxos.yes.value - leg.delta_out,
                &new_spk,
            ),
        );
        add_pset_output(
            &mut pset,
            explicit_txout(
                &leg.pool_params.no_asset_id,
                leg.pool_utxos.no.value,
                &new_spk,
            ),
        );
        add_pset_output(
            &mut pset,
            explicit_txout(
                &leg.pool_params.collateral_asset_id,
                leg.pool_utxos.collateral.value + leg.delta_in,
                &new_spk,
            ),
        );
        pset
    }

    #[test]
    fn rejects_non_three_input_window() {
        let leg = test_leg();
        let mut pset = new_pset();
        add_pset_input(&mut pset, &leg.pool_utxos.yes);
        add_pset_input(&mut pset, &leg.pool_utxos.no);
        add_pset_input(&mut pset, &leg.pool_utxos.collateral);

        let err = attach_lmsr_pool_witnesses(&mut pset, &leg, 0..2).unwrap_err();
        assert!(
            err.to_string()
                .contains("requires exactly 3 covenant inputs")
        );
    }

    #[test]
    fn attaches_primary_and_secondary_witnesses() {
        let leg = test_leg();
        let mut pset = build_swap_pset(&leg);

        attach_lmsr_pool_witnesses(&mut pset, &leg, 0..3).unwrap();
        assert!(pset.inputs()[0].final_script_witness.is_some());
        assert!(pset.inputs()[1].final_script_witness.is_some());
        assert!(pset.inputs()[2].final_script_witness.is_some());
    }

    #[test]
    fn rejects_buy_leg_that_violates_fee_inequality() {
        let mut leg = test_leg();
        leg.delta_in = 1_000; // below required min collateral-in for (old_f,new_f,fee_bps)
        let mut pset = build_swap_pset(&leg);

        assert!(attach_lmsr_pool_witnesses(&mut pset, &leg, 0..3).is_err());
    }

    #[test]
    fn rejects_leg_with_invalid_merkle_proof() {
        let mut leg = test_leg();
        leg.old_siblings[0][0] ^= 0x01;
        let mut pset = build_swap_pset(&leg);
        assert!(attach_lmsr_pool_witnesses(&mut pset, &leg, 0..3).is_err());
    }

    #[test]
    fn rejects_leg_with_missing_merkle_proof_levels() {
        let mut leg = test_leg();
        let _ = leg.old_siblings.pop();
        let _ = leg.new_siblings.pop();
        let mut pset = build_swap_pset(&leg);
        assert!(attach_lmsr_pool_witnesses(&mut pset, &leg, 0..3).is_err());
    }

    #[test]
    fn rejects_leg_when_state_index_exceeds_s_max() {
        let mut leg = test_leg();
        leg.pool_params.s_max_index = 8;
        let mut pset = build_swap_pset(&leg);
        assert!(attach_lmsr_pool_witnesses(&mut pset, &leg, 0..3).is_err());
    }
}
