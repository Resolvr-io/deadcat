use std::collections::HashMap;

use simplicityhl::str::WitnessName;
use simplicityhl::types::{ResolvedType, TypeConstructible};
use simplicityhl::value::ValueConstructible;
use simplicityhl::{SatisfiedProgram, Value, WitnessValues};

use super::contract::CompiledMakerOrder;

/// Build the witness values for a maker order **fill** (Left path).
///
/// Provides all 3 witnesses: PATH = Left(()), COSIGNER_SIGNATURE, MAKER_CANCEL_SIGNATURE.
/// Unused witnesses get dummy values (pruned by Simplicity).
pub fn build_maker_order_fill_witness(cosigner_signature: &[u8; 64]) -> WitnessValues {
    let mut map = HashMap::new();

    // PATH = Left(()) — fill path
    let ut = || ResolvedType::unit();
    map.insert(
        WitnessName::from_str_unchecked("PATH"),
        Value::left(Value::unit(), ut()),
    );

    map.insert(
        WitnessName::from_str_unchecked("COSIGNER_SIGNATURE"),
        Value::byte_array(cosigner_signature.iter().copied()),
    );

    // Dummy cancel signature (pruned on the fill path)
    map.insert(
        WitnessName::from_str_unchecked("MAKER_CANCEL_SIGNATURE"),
        Value::byte_array([0u8; 64].iter().copied()),
    );

    WitnessValues::from(map)
}

/// Build the witness values for a maker order **cancel** (Right path).
///
/// Provides all 3 witnesses: PATH = Right(()), COSIGNER_SIGNATURE, MAKER_CANCEL_SIGNATURE.
/// Unused witnesses get dummy values (pruned by Simplicity).
pub fn build_maker_order_cancel_witness(maker_cancel_sig: &[u8; 64]) -> WitnessValues {
    let mut map = HashMap::new();

    // PATH = Right(()) — cancel path
    let ut = || ResolvedType::unit();
    map.insert(
        WitnessName::from_str_unchecked("PATH"),
        Value::right(ut(), Value::unit()),
    );

    // Dummy cosigner signature (pruned on the cancel path)
    map.insert(
        WitnessName::from_str_unchecked("COSIGNER_SIGNATURE"),
        Value::byte_array([0u8; 64].iter().copied()),
    );

    map.insert(
        WitnessName::from_str_unchecked("MAKER_CANCEL_SIGNATURE"),
        Value::byte_array(maker_cancel_sig.iter().copied()),
    );

    WitnessValues::from(map)
}

/// Satisfy a compiled maker order contract with the given cosigner signature (fill path).
#[cfg_attr(not(any(test, feature = "testing")), allow(dead_code))]
pub fn satisfy_maker_order(
    contract: &CompiledMakerOrder,
    cosigner_signature: &[u8; 64],
) -> crate::error::Result<SatisfiedProgram> {
    let witness_values = build_maker_order_fill_witness(cosigner_signature);
    contract
        .program()
        .satisfy(witness_values)
        .map_err(|e| crate::error::Error::Compilation(format!("witness satisfaction: {e}")))
}

/// Serialize a satisfied program into (program_bytes, witness_bytes).
pub fn serialize_satisfied(satisfied: &SatisfiedProgram) -> (Vec<u8>, Vec<u8>) {
    satisfied.redeem().to_vec_with_witness()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::maker_order::params::{MakerOrderParams, OrderDirection};
    use crate::taproot::NUMS_KEY_BYTES;

    fn test_params() -> MakerOrderParams {
        let (params, _) = MakerOrderParams::new(
            [0x01; 32],
            [0xbb; 32],
            50_000,
            1,
            1,
            OrderDirection::SellBase,
            NUMS_KEY_BYTES,
            &[0xaa; 32],
            &[0x11; 32],
        );
        params
    }

    #[test]
    fn fill_witness_satisfies_contract() {
        let contract = CompiledMakerOrder::new(test_params()).unwrap();
        let witness = build_maker_order_fill_witness(&[0u8; 64]);
        let satisfied = contract
            .program()
            .satisfy(witness)
            .expect("fill witness should satisfy");
        let (prog, wit) = serialize_satisfied(&satisfied);
        assert!(!prog.is_empty());
        assert!(!wit.is_empty());
    }

    #[test]
    fn cancel_witness_satisfies_contract() {
        let contract = CompiledMakerOrder::new(test_params()).unwrap();
        let witness = build_maker_order_cancel_witness(&[0xab; 64]);
        let satisfied = contract
            .program()
            .satisfy(witness)
            .expect("cancel witness should satisfy");
        let (prog, wit) = serialize_satisfied(&satisfied);
        assert!(!prog.is_empty());
        assert!(!wit.is_empty());
    }
}
