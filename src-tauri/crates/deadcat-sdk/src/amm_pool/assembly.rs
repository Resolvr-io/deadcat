use std::sync::Arc;

use lwk_wollet::elements::pset::PartiallySignedTransaction;
use simplicityhl::elements::hashes::Hash;
use simplicityhl::elements::taproot::ControlBlock;
use simplicityhl::simplicity::jet::elements::{ElementsEnv, ElementsUtxo};

use crate::assembly::pset_to_pruning_transaction;
use crate::error::{Error, Result};

use super::contract::CompiledAmmPool;
use super::witness::{
    AmmPoolSpendingPath, satisfy_amm_pool_with_env, serialize_satisfied,
};

/// Result of assembling an AMM pool transaction (before signing).
pub struct AssembledPoolTransaction {
    pub pset: PartiallySignedTransaction,
    pub spending_path: AmmPoolSpendingPath,
}

/// Attach AMM pool Simplicity witness stacks to covenant inputs in the PSET.
///
/// - Input 0: primary path (Swap or LpDepositWithdraw)
/// - Inputs 1-3: secondary path (co-membership check)
pub fn attach_amm_pool_witnesses(
    pset: &mut PartiallySignedTransaction,
    contract: &CompiledAmmPool,
    issued_lp: u64,
    primary_path: AmmPoolSpendingPath,
) -> Result<AmmPoolSpendingPath> {
    if pset.inputs().len() < 4 {
        return Err(Error::Pset(format!(
            "AMM pool PSET requires at least 4 inputs (covenant), got {}",
            pset.inputs().len()
        )));
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

    let cb_bytes = contract.control_block(issued_lp);
    let cmr_bytes = contract.cmr().to_byte_array().to_vec();

    let build_witness_stack =
        |path: &AmmPoolSpendingPath, input_index: u32| -> Result<Vec<Vec<u8>>> {
            let control_block = ControlBlock::from_slice(&cb_bytes)
                .map_err(|e| Error::Witness(format!("control block: {e}")))?;

            let env = ElementsEnv::new(
                Arc::clone(&tx),
                utxos.clone(),
                input_index,
                *contract.cmr(),
                control_block,
                None,
                lwk_wollet::elements::BlockHash::all_zeros(),
            );

            let satisfied = satisfy_amm_pool_with_env(contract, path, Some(&env))
                .map_err(|e| Error::Witness(e.to_string()))?;
            let (program_bytes, witness_bytes) = serialize_satisfied(&satisfied);

            let stack = vec![
                witness_bytes,
                program_bytes,
                cmr_bytes.clone(),
                cb_bytes.clone(),
            ];

            debug_assert!(
                satisfied.redeem().bounds().cost.is_budget_valid(&stack),
                "input {input_index}: Simplicity program cost exceeds witness budget"
            );

            Ok(stack)
        };

    // Primary covenant input (index 0)
    pset.inputs_mut()[0].final_script_witness =
        Some(build_witness_stack(&primary_path, 0)?);

    // Secondary covenant inputs (indices 1-3)
    let secondary_path = AmmPoolSpendingPath::Secondary;
    for idx in 1..=3u32 {
        pset.inputs_mut()[idx as usize].final_script_witness =
            Some(build_witness_stack(&secondary_path, idx)?);
    }

    Ok(primary_path)
}
