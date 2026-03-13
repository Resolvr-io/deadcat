use std::collections::HashMap;
use std::sync::Arc;

use simplicityhl::elements::Transaction;
use simplicityhl::num::{NonZeroPow2Usize, U256};
use simplicityhl::simplicity::jet::elements::ElementsEnv;
use simplicityhl::str::WitnessName;
use simplicityhl::types::{ResolvedType, TypeConstructible};
use simplicityhl::value::ValueConstructible;
use simplicityhl::{SatisfiedProgram, Value, WitnessValues};

use crate::error::{Error, Result};
use crate::trade::types::{LmsrPoolSwapLeg, LmsrPrimaryPath};

use super::contract::CompiledLmsrPool;

const LMSR_MAX_PROOF_DEPTH: usize = 63;
const LMSR_PROOF_BOUND: usize = 64;

fn build_proof_list(path_bits: u64, siblings: &[[u8; 32]]) -> Result<Value> {
    let element_ty = ResolvedType::tuple([ResolvedType::u256(), ResolvedType::boolean()]);
    let bound =
        NonZeroPow2Usize::new(LMSR_PROOF_BOUND).expect("LMSR proof bound must be power-of-two");
    if siblings.len() > LMSR_MAX_PROOF_DEPTH {
        return Err(Error::Witness(format!(
            "LMSR proof sibling count {} exceeds supported depth {}",
            siblings.len(),
            LMSR_MAX_PROOF_DEPTH
        )));
    }
    Ok(Value::list(
        siblings.iter().enumerate().map(|(level, sibling)| {
            let is_right = ((path_bits >> level) & 1) == 1;
            Value::tuple([
                Value::u256(U256::from_byte_array(*sibling)),
                Value::from(is_right),
            ])
        }),
        element_ty,
        bound,
    ))
}

fn unit_ty() -> ResolvedType {
    ResolvedType::unit()
}

fn build_primary_path_value(path: LmsrPrimaryPath) -> Value {
    match path {
        LmsrPrimaryPath::Swap => Value::left(Value::unit(), unit_ty()),
        LmsrPrimaryPath::AdminAdjust => Value::right(unit_ty(), Value::unit()),
    }
}

fn build_scan_payload(leg: &LmsrPoolSwapLeg) -> Value {
    let path_tag = match leg.primary_path {
        LmsrPrimaryPath::Swap => 0u8,
        LmsrPrimaryPath::AdminAdjust => 1u8,
    };
    Value::product(
        Value::u32(leg.out_base),
        Value::product(
            Value::u64(leg.old_s_index),
            Value::product(Value::u64(leg.new_s_index), Value::u8(path_tag)),
        ),
    )
}

/// Build witness values for the LMSR primary leaf program.
pub fn build_lmsr_primary_witness(leg: &LmsrPoolSwapLeg) -> Result<WitnessValues> {
    let mut map = HashMap::new();
    map.insert(
        WitnessName::from_str_unchecked("PATH_PRIMARY"),
        build_primary_path_value(leg.primary_path),
    );
    map.insert(
        WitnessName::from_str_unchecked("IN_BASE"),
        Value::u32(leg.in_base),
    );
    map.insert(
        WitnessName::from_str_unchecked("OUT_BASE"),
        Value::u32(leg.out_base),
    );
    map.insert(
        WitnessName::from_str_unchecked("TRADE_KIND"),
        Value::u8(leg.trade_kind as u8),
    );
    map.insert(
        WitnessName::from_str_unchecked("OLD_S_INDEX"),
        Value::u64(leg.old_s_index),
    );
    map.insert(
        WitnessName::from_str_unchecked("NEW_S_INDEX"),
        Value::u64(leg.new_s_index),
    );
    map.insert(
        WitnessName::from_str_unchecked("F_OLD"),
        Value::u64(leg.old_f),
    );
    map.insert(
        WitnessName::from_str_unchecked("F_NEW"),
        Value::u64(leg.new_f),
    );
    map.insert(
        WitnessName::from_str_unchecked("OLD_PROOF"),
        build_proof_list(leg.old_path_bits, &leg.old_siblings)?,
    );
    map.insert(
        WitnessName::from_str_unchecked("NEW_PROOF"),
        build_proof_list(leg.new_path_bits, &leg.new_siblings)?,
    );
    map.insert(
        WitnessName::from_str_unchecked("DELTA_IN"),
        Value::u64(leg.delta_in),
    );
    map.insert(
        WitnessName::from_str_unchecked("DELTA_OUT"),
        Value::u64(leg.delta_out),
    );
    map.insert(
        WitnessName::from_str_unchecked("ADMIN_SIGNATURE"),
        Value::byte_array(leg.admin_signature.iter().copied()),
    );
    map.insert(
        WitnessName::from_str_unchecked("SCAN_PAYLOAD"),
        build_scan_payload(leg),
    );
    Ok(WitnessValues::from(map))
}

/// Build witness values for the LMSR secondary leaf program.
pub fn build_lmsr_secondary_witness(in_base: u32) -> WitnessValues {
    let mut map = HashMap::new();
    map.insert(
        WitnessName::from_str_unchecked("IN_BASE"),
        Value::u32(in_base),
    );
    WitnessValues::from(map)
}

/// Satisfy the LMSR primary leaf program with pruning enabled via `ElementsEnv`.
pub fn satisfy_lmsr_primary_with_env(
    contract: &CompiledLmsrPool,
    leg: &LmsrPoolSwapLeg,
    env: Option<&ElementsEnv<Arc<Transaction>>>,
) -> Result<SatisfiedProgram> {
    let witness_values = build_lmsr_primary_witness(leg)?;
    contract
        .primary_program()?
        .satisfy_with_env(witness_values, env)
        .map_err(|e| Error::Witness(format!("lmsr primary witness satisfaction: {e}")))
}

/// Satisfy the LMSR secondary leaf program with pruning enabled via `ElementsEnv`.
pub fn satisfy_lmsr_secondary_with_env(
    contract: &CompiledLmsrPool,
    in_base: u32,
    env: Option<&ElementsEnv<Arc<Transaction>>>,
) -> Result<SatisfiedProgram> {
    let witness_values = build_lmsr_secondary_witness(in_base);
    contract
        .secondary_program()?
        .satisfy_with_env(witness_values, env)
        .map_err(|e| Error::Witness(format!("lmsr secondary witness satisfaction: {e}")))
}

/// Serialize a satisfied program into `(program_bytes, witness_bytes)`.
pub fn serialize_satisfied(satisfied: &SatisfiedProgram) -> (Vec<u8>, Vec<u8>) {
    satisfied.redeem().to_vec_with_witness()
}

#[cfg(test)]
mod tests {
    use super::{LMSR_MAX_PROOF_DEPTH, build_proof_list};

    #[test]
    fn build_proof_list_rejects_excessive_depth() {
        let siblings = vec![[0u8; 32]; LMSR_MAX_PROOF_DEPTH + 1];
        let err = build_proof_list(0, &siblings).unwrap_err();
        assert!(err.to_string().contains("exceeds supported depth"));
    }
}
