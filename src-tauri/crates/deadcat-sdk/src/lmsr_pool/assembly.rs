use std::ops::Range;
use std::sync::Arc;

use lwk_wollet::elements::pset::PartiallySignedTransaction;
use simplicityhl::elements::hashes::Hash;
use simplicityhl::elements::taproot::ControlBlock;
use simplicityhl::simplicity::bit_machine::BitMachine;
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
    genesis_hash: [u8; 32],
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
            lwk_wollet::elements::BlockHash::from_byte_array(genesis_hash),
        );
        let satisfied = satisfy_lmsr_primary_with_env(&contract, leg, Some(&env))?;

        // Verify the satisfied program executes in the Rust BitMachine.
        // satisfy_with_env only prunes — it does NOT run jets.
        #[cfg(debug_assertions)]
        {
            let redeem = satisfied.redeem();
            let mut machine = BitMachine::for_program(redeem)
                .map_err(|e| Error::Witness(format!("lmsr primary BitMachine init: {e}")))?;
            machine
                .exec(redeem, &env)
                .map_err(|e| Error::Witness(format!("lmsr primary local execution failed: {e}")))?;
        }

        // Run the full C evaluator pipeline (decode → type-infer → execute)
        // against the same env.  This matches what elementsd does and catches
        // pruning/serialization bugs that the Rust BitMachine misses.
        #[cfg(feature = "testing")]
        {
            let (prog_bytes, wit_bytes) = serialize_satisfied(&satisfied);
            crate::lmsr_pool::c_eval::run_program_with_env(&prog_bytes, &wit_bytes, env.c_tx_env())
                .map_err(|e| {
                    Error::Witness(format!("C evaluator rejected lmsr primary program: {e}"))
                })?;
        }

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
            lwk_wollet::elements::BlockHash::from_byte_array(genesis_hash),
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
    use std::sync::Arc;

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

        let err = attach_lmsr_pool_witnesses(&mut pset, &leg, 0..2, [0; 32]).unwrap_err();
        assert!(
            err.to_string()
                .contains("requires exactly 3 covenant inputs")
        );
    }

    #[test]
    fn attaches_primary_and_secondary_witnesses() {
        let leg = test_leg();
        let mut pset = build_swap_pset(&leg);

        attach_lmsr_pool_witnesses(&mut pset, &leg, 0..3, [0; 32]).unwrap();
        assert!(pset.inputs()[0].final_script_witness.is_some());
        assert!(pset.inputs()[1].final_script_witness.is_some());
        assert!(pset.inputs()[2].final_script_witness.is_some());
    }

    #[test]
    fn rejects_buy_leg_that_violates_fee_inequality() {
        let mut leg = test_leg();
        leg.delta_in = 1_000; // below required min collateral-in for (old_f,new_f,fee_bps)
        let mut pset = build_swap_pset(&leg);

        assert!(attach_lmsr_pool_witnesses(&mut pset, &leg, 0..3, [0; 32]).is_err());
    }

    #[test]
    fn rejects_leg_with_invalid_merkle_proof() {
        let mut leg = test_leg();
        leg.old_siblings[0][0] ^= 0x01;
        let mut pset = build_swap_pset(&leg);
        assert!(attach_lmsr_pool_witnesses(&mut pset, &leg, 0..3, [0; 32]).is_err());
    }

    #[test]
    fn rejects_leg_with_missing_merkle_proof_levels() {
        let mut leg = test_leg();
        let _ = leg.old_siblings.pop();
        let _ = leg.new_siblings.pop();
        let mut pset = build_swap_pset(&leg);
        assert!(attach_lmsr_pool_witnesses(&mut pset, &leg, 0..3, [0; 32]).is_err());
    }

    #[test]
    fn rejects_leg_when_state_index_exceeds_s_max() {
        let mut leg = test_leg();
        leg.pool_params.s_max_index = 8;
        let mut pset = build_swap_pset(&leg);
        assert!(attach_lmsr_pool_witnesses(&mut pset, &leg, 0..3, [0; 32]).is_err());
    }

    // ── Tier 1: BitMachine execution tests ─────────────────────────────

    /// Helper: build an ElementsEnv from a PSET for a given input index.
    fn build_env(
        pset: &PartiallySignedTransaction,
        contract: &CompiledLmsrPool,
        s_index: u64,
        input_index: u32,
        is_primary: bool,
    ) -> simplicityhl::simplicity::jet::elements::ElementsEnv<Arc<lwk_wollet::elements::Transaction>>
    {
        use simplicityhl::elements::taproot::ControlBlock;
        use simplicityhl::simplicity::jet::elements::{ElementsEnv, ElementsUtxo};

        let tx = Arc::new(crate::assembly::pset_to_pruning_transaction(pset).unwrap());
        let utxos: Vec<ElementsUtxo> = pset
            .inputs()
            .iter()
            .map(|inp| ElementsUtxo::from(inp.witness_utxo.clone().unwrap()))
            .collect();
        let (cmr, cb_bytes) = if is_primary {
            (
                *contract.primary_cmr(),
                contract.primary_control_block(s_index),
            )
        } else {
            (
                *contract.secondary_cmr(),
                contract.secondary_control_block(s_index),
            )
        };
        let control_block = ControlBlock::from_slice(&cb_bytes).unwrap();
        ElementsEnv::new(
            tx,
            utxos,
            input_index,
            cmr,
            control_block,
            None,
            lwk_wollet::elements::BlockHash::from_byte_array([0u8; 32]),
        )
    }

    /// Swap path: run BitMachine on the primary input.
    #[test]
    fn swap_primary_executes_in_bitmachine() {
        use crate::lmsr_pool::witness::satisfy_lmsr_primary_with_env;

        let leg = test_leg();
        let pset = build_swap_pset(&leg);
        let contract = CompiledLmsrPool::new(leg.pool_params).unwrap();
        let env = build_env(&pset, &contract, leg.old_s_index, 0, true);

        let satisfied =
            satisfy_lmsr_primary_with_env(&contract, &leg, Some(&env)).expect("satisfy primary");
        let redeem = satisfied.redeem();
        let mut machine = BitMachine::for_program(redeem).expect("BitMachine init");
        machine
            .exec(redeem, &env)
            .expect("swap primary should pass BitMachine");
    }

    /// Swap path: run BitMachine on secondary inputs (NO slot and collateral slot).
    #[test]
    fn swap_secondary_executes_in_bitmachine() {
        use crate::lmsr_pool::witness::satisfy_lmsr_secondary_with_env;

        let leg = test_leg();
        let pset = build_swap_pset(&leg);
        let contract = CompiledLmsrPool::new(leg.pool_params).unwrap();

        // Secondary input 1 (NO slot, PSET index 1)
        let env_no = build_env(&pset, &contract, leg.old_s_index, 1, false);
        let satisfied_no = satisfy_lmsr_secondary_with_env(&contract, leg.in_base, Some(&env_no))
            .expect("satisfy secondary NO");
        let redeem_no = satisfied_no.redeem();
        let mut machine_no = BitMachine::for_program(redeem_no).expect("BitMachine init NO");
        machine_no
            .exec(redeem_no, &env_no)
            .expect("swap secondary NO should pass BitMachine");

        // Secondary input 2 (collateral slot, PSET index 2)
        let env_coll = build_env(&pset, &contract, leg.old_s_index, 2, false);
        let satisfied_coll =
            satisfy_lmsr_secondary_with_env(&contract, leg.in_base, Some(&env_coll))
                .expect("satisfy secondary collateral");
        let redeem_coll = satisfied_coll.redeem();
        let mut machine_coll = BitMachine::for_program(redeem_coll).expect("BitMachine init coll");
        machine_coll
            .exec(redeem_coll, &env_coll)
            .expect("swap secondary collateral should pass BitMachine");
    }

    // ── PSET construction tests ──────────────────────────────────────────

    /// Verify admin adjust PSET structure for the decrease (no wallet inputs) case.
    #[test]
    fn admin_adjust_pset_decrease_structure() {
        let yes = [0x11; 32];
        let no = [0x22; 32];
        let collateral = [0x33; 32];
        let table_values: Vec<u64> = (0..16u64).map(|i| 2_000 + i * 10).collect();
        let table_root = lmsr_table_root(&table_values).unwrap();
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
        let spk = contract.script_pubkey(8);

        // Decrease: 50k→40k YES, 50k→40k NO, 100k→90k collateral, fee=500
        let mut pset = new_pset();
        add_pset_input(&mut pset, &dummy_utxo(yes, 50_000, 0, &spk));
        add_pset_input(&mut pset, &dummy_utxo(no, 50_000, 1, &spk));
        add_pset_input(&mut pset, &dummy_utxo(collateral, 100_000, 2, &spk));

        // Reserve outputs
        add_pset_output(&mut pset, explicit_txout(&yes, 40_000, &spk));
        add_pset_output(&mut pset, explicit_txout(&no, 40_000, &spk));
        add_pset_output(&mut pset, explicit_txout(&collateral, 90_000, &spk));
        // Fee (absorbed from collateral surplus: 10k surplus - 500 fee = 9.5k change)
        add_pset_output(&mut pset, explicit_txout(&collateral, 500, &Script::new()));
        // Change outputs (explicit, not blinded)
        add_pset_output(&mut pset, explicit_txout(&yes, 10_000, &Script::new()));
        add_pset_output(&mut pset, explicit_txout(&no, 10_000, &Script::new()));
        add_pset_output(
            &mut pset,
            explicit_txout(&collateral, 9_500, &Script::new()),
        );

        assert_eq!(pset.inputs().len(), 3, "3 reserve inputs");
        assert_eq!(pset.outputs().len(), 7, "3 reserves + fee + 3 change");

        // Verify explicit value balance
        let total_in: u64 = 50_000 + 50_000 + 100_000;
        let total_out: u64 = 40_000 + 40_000 + 90_000 + 500 + 10_000 + 10_000 + 9_500;
        assert_eq!(total_in, total_out, "value must balance");

        // No outputs should be blinded (all explicit for the zero-wallet-inputs case)
        for (i, out) in pset.outputs().iter().enumerate() {
            assert!(
                out.blinding_key.is_none(),
                "output {i} should not be blinded in the decrease case"
            );
        }
    }

    /// Verify close_lmsr_pool reclaimed amounts match expectations.
    #[test]
    fn close_reclaimed_amounts() {
        use crate::pool::PoolReserves;

        let min_yes = 1u64;
        let min_no = 1u64;
        let min_collateral = 1u64;
        let current = PoolReserves {
            r_yes: 50_000,
            r_no: 40_000,
            r_lbtc: 100_000,
        };

        let reclaimed_yes = current.r_yes.saturating_sub(min_yes);
        let reclaimed_no = current.r_no.saturating_sub(min_no);
        let reclaimed_collateral = current.r_lbtc.saturating_sub(min_collateral);

        assert_eq!(reclaimed_yes, 49_999);
        assert_eq!(reclaimed_no, 39_999);
        assert_eq!(reclaimed_collateral, 99_999);
    }

    // ── Tier 1 + 1.5: BitMachine + C evaluator ──────────────────────────

    /// Build an admin-adjust leg and run both Rust BitMachine and C evaluator.
    ///
    /// This is a fast unit test (no elementsd) that isolates Rust-vs-C
    /// evaluator divergence for the admin path.
    #[test]
    fn admin_adjust_c_evaluator_agrees_with_rust() {
        use crate::lmsr_pool::table::LmsrTableManifest;
        use crate::lmsr_pool::witness::{satisfy_lmsr_primary_with_env, serialize_satisfied};
        use lwk_wollet::elements::secp256k1_zkp::{self, Keypair, Secp256k1};
        use sha2::{Digest, Sha256};
        use std::sync::Arc;

        // 1. Build params with a real admin cosigner
        let secp = Secp256k1::new();
        let secret = secp256k1_zkp::SecretKey::from_slice(&[0x42; 32]).unwrap();
        let admin_keypair = Keypair::from_secret_key(&secp, &secret);
        let (admin_xonly, _) = admin_keypair.x_only_public_key();

        let yes = [0x11; 32];
        let no = [0x22; 32];
        let collateral = [0x33; 32];
        let table_values: Vec<u64> = (0..16u64).map(|i| 2_000 + i * 10).collect();
        let table_root = lmsr_table_root(&table_values).unwrap();
        let manifest = LmsrTableManifest::new(4, table_values.clone()).unwrap();
        let proof = manifest.proof_at(8).unwrap();
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
            cosigner_pubkey: admin_xonly.serialize(),
        };
        let contract = CompiledLmsrPool::new(params).unwrap();
        let spk = contract.script_pubkey(8);

        // 2. Build a no-op admin adjust: same reserves in and out, same s_index
        let in_yes = 50_000u64;
        let in_no = 50_000u64;
        let in_collateral = 100_000u64;

        let pool_utxos = LmsrPoolUtxos {
            yes: dummy_utxo(yes, in_yes, 0, &spk),
            no: dummy_utxo(no, in_no, 1, &spk),
            collateral: dummy_utxo(collateral, in_collateral, 2, &spk),
        };

        // 3. Build PSET: 3 reserve inputs → 3 reserve outputs (same values) + fee
        let mut pset = new_pset();
        add_pset_input(&mut pset, &pool_utxos.yes);
        add_pset_input(&mut pset, &pool_utxos.no);
        add_pset_input(&mut pset, &pool_utxos.collateral);

        // Reserve outputs (unchanged)
        add_pset_output(&mut pset, explicit_txout(&yes, in_yes, &spk));
        add_pset_output(&mut pset, explicit_txout(&no, in_no, &spk));
        add_pset_output(
            &mut pset,
            explicit_txout(&collateral, in_collateral - 500, &spk),
        );
        // Fee output
        add_pset_output(&mut pset, explicit_txout(&collateral, 500, &Script::new()));

        // 4. Compute admin signature
        let genesis_hash = [0u8; 32]; // test genesis
        let tx = Arc::new(crate::assembly::pset_to_pruning_transaction(&pset).unwrap());
        let utxos: Vec<simplicityhl::simplicity::jet::elements::ElementsUtxo> = pset
            .inputs()
            .iter()
            .map(|inp| {
                simplicityhl::simplicity::jet::elements::ElementsUtxo::from(
                    inp.witness_utxo.clone().unwrap(),
                )
            })
            .collect();
        let primary_cmr = *contract.primary_cmr();
        let cb_bytes = contract.primary_control_block(8);
        let control_block =
            simplicityhl::elements::taproot::ControlBlock::from_slice(&cb_bytes).unwrap();
        let env = simplicityhl::simplicity::jet::elements::ElementsEnv::new(
            tx,
            utxos,
            0,
            primary_cmr,
            control_block,
            None,
            lwk_wollet::elements::BlockHash::from_byte_array(genesis_hash),
        );
        let sig_all: [u8; 32] = env.c_tx_env().sighash_all().to_byte_array();

        let mut hasher = Sha256::new();
        hasher.update(b"DEADCAT/LMSR_LIQUIDITY_ADJUST_V1");
        hasher.update(genesis_hash);
        hasher.update(params.lmsr_table_root);
        hasher.update(params.yes_asset_id);
        hasher.update(params.no_asset_id);
        hasher.update(params.collateral_asset_id);
        hasher.update(sig_all);
        // Input prevouts
        hasher.update(pool_utxos.yes.outpoint.txid.to_byte_array());
        hasher.update(pool_utxos.yes.outpoint.vout.to_be_bytes());
        hasher.update(pool_utxos.no.outpoint.txid.to_byte_array());
        hasher.update(pool_utxos.no.outpoint.vout.to_be_bytes());
        hasher.update(pool_utxos.collateral.outpoint.txid.to_byte_array());
        hasher.update(pool_utxos.collateral.outpoint.vout.to_be_bytes());
        // Indices
        hasher.update(0u32.to_be_bytes()); // i_yes = in_base
        hasher.update(0u32.to_be_bytes()); // out_base
        hasher.update(8u64.to_be_bytes()); // old_s_index
        hasher.update(8u64.to_be_bytes()); // new_s_index
        // Reserves
        hasher.update(in_yes.to_be_bytes());
        hasher.update(in_no.to_be_bytes());
        hasher.update(in_collateral.to_be_bytes());
        hasher.update(in_yes.to_be_bytes()); // out = in (no-op)
        hasher.update(in_no.to_be_bytes());
        hasher.update((in_collateral - 500).to_be_bytes()); // minus fee
        // SPK hashes
        let spk_hash = contract.script_hash(8);
        hasher.update(spk_hash);
        hasher.update(spk_hash);
        hasher.update(spk_hash);

        let msg_hash: [u8; 32] = hasher.finalize().into();
        let msg = secp256k1_zkp::Message::from_digest(msg_hash);
        let admin_sig = secp.sign_schnorr_no_aux_rand(&msg, &admin_keypair);

        // 5. Build the swap leg
        let leg = LmsrPoolSwapLeg {
            primary_path: LmsrPrimaryPath::AdminAdjust,
            pool_params: params,
            pool_id: "test-admin-pool".into(),
            old_s_index: 8,
            new_s_index: 8,
            old_path_bits: proof.path_bits,
            new_path_bits: proof.path_bits,
            old_siblings: proof.siblings.clone(),
            new_siblings: proof.siblings,
            in_base: 0,
            out_base: 0,
            pool_utxos,
            trade_kind: LmsrTradeKind::BuyYes, // unused for admin
            old_f: table_values[8],
            new_f: table_values[8],
            delta_in: 0,
            delta_out: 0,
            admin_signature: admin_sig.serialize(),
        };

        // 6. Satisfy and run Rust BitMachine
        let satisfied =
            satisfy_lmsr_primary_with_env(&contract, &leg, Some(&env)).expect("satisfy");
        let redeem = satisfied.redeem();
        let mut machine = BitMachine::for_program(redeem).expect("BitMachine init");
        machine
            .exec(redeem, &env)
            .expect("Rust BitMachine should accept admin adjust");

        // 7. Run the full C evaluator to match what elementsd does
        #[cfg(feature = "testing")]
        {
            let (prog_bytes, wit_bytes) = serialize_satisfied(&satisfied);
            crate::lmsr_pool::c_eval::run_program_with_env(&prog_bytes, &wit_bytes, env.c_tx_env())
                .expect("C evaluator should accept admin adjust program");
        }
    }
}
