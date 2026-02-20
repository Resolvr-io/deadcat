use std::collections::HashMap;

use simplicityhl::str::WitnessName;
use simplicityhl::value::ValueConstructible;
use simplicityhl::{SatisfiedProgram, Value, WitnessValues};

use super::contract::CompiledMakerOrder;

/// Build the witness values for a maker order fill.
///
/// The only witness is the cosigner signature (64-byte Schnorr).
/// Pass `[0u8; 64]` when no cosigner is configured (COSIGNER_PUBKEY == NUMS).
pub fn build_maker_order_witness(cosigner_signature: &[u8; 64]) -> WitnessValues {
    let mut map = HashMap::new();
    map.insert(
        WitnessName::from_str_unchecked("COSIGNER_SIGNATURE"),
        Value::byte_array(cosigner_signature.iter().copied()),
    );
    WitnessValues::from(map)
}

/// Satisfy a compiled maker order contract with the given cosigner signature.
pub fn satisfy_maker_order(
    contract: &CompiledMakerOrder,
    cosigner_signature: &[u8; 64],
) -> crate::error::Result<SatisfiedProgram> {
    let witness_values = build_maker_order_witness(cosigner_signature);
    contract
        .program()
        .satisfy(witness_values)
        .map_err(|e| crate::error::Error::Compilation(format!("witness satisfaction: {e}")))
}

/// Serialize a satisfied program into (program_bytes, witness_bytes).
pub fn serialize_satisfied(satisfied: &SatisfiedProgram) -> (Vec<u8>, Vec<u8>) {
    satisfied.redeem().to_vec_with_witness()
}
