use std::collections::HashMap;
use std::sync::Arc;

use simplicityhl::elements::Transaction;
use simplicityhl::num::U256;
use simplicityhl::simplicity::jet::elements::ElementsEnv;
use simplicityhl::str::WitnessName;
use simplicityhl::types::{ResolvedType, TypeConstructible};
use simplicityhl::value::ValueConstructible;
use simplicityhl::{SatisfiedProgram, Value, WitnessValues};

use super::contract::CompiledAmmPool;
use super::math::SwapPair;

/// Blinding factors for the LP reissuance token (input and output).
#[derive(Debug, Clone, Copy, Default)]
pub struct RtBlindingFactors {
    pub input_abf: [u8; 32],
    pub input_vbf: [u8; 32],
    pub output_abf: [u8; 32],
    pub output_vbf: [u8; 32],
}

/// The spending path for an AMM pool transaction.
#[derive(Debug, Clone)]
pub enum AmmPoolSpendingPath {
    /// Swap path: two reserves change, one unchanged.
    Swap {
        swap_pair: SwapPair,
        issued_lp: u64,
        blinding: RtBlindingFactors,
    },
    /// LP deposit or withdraw: all reserves may change, LP tokens minted/burned.
    LpDepositWithdraw {
        issued_lp: u64,
        blinding: RtBlindingFactors,
    },
    /// Secondary covenant input (indices 1, 2, 3).
    Secondary,
}

fn unit_ty() -> ResolvedType {
    ResolvedType::unit()
}

fn u256_val(bytes: &[u8; 32]) -> Value {
    Value::u256(U256::from_byte_array(*bytes))
}

/// Build the PATH witness value for the 3-path dispatch tree.
///
/// Tree structure: `Either<Either<(), ()>, ()>`
///   - `Left(Left(()))` = Path 1: Swap
///   - `Left(Right(()))` = Path 2: LP deposit/withdraw
///   - `Right(())` = Path 3: Secondary covenant input
fn build_path_value(path: &AmmPoolSpendingPath) -> Value {
    let u = || Value::unit();
    let ut = unit_ty;
    let pair_ty = || ResolvedType::either(ut(), ut());

    match path {
        AmmPoolSpendingPath::Swap { .. } => {
            // Left(Left(()))
            let inner = Value::left(u(), ut());
            Value::left(inner, ut())
        }
        AmmPoolSpendingPath::LpDepositWithdraw { .. } => {
            // Left(Right(()))
            let inner = Value::right(ut(), u());
            Value::left(inner, ut())
        }
        AmmPoolSpendingPath::Secondary => {
            // Right(())
            Value::right(pair_ty(), u())
        }
    }
}

/// Build the complete witness values for an AMM pool spending path.
///
/// All witnesses are set unconditionally (SimplicityHL requirement).
/// Unused paths get dummy/zero values that are pruned by Simplicity.
pub fn build_amm_pool_witness(path: &AmmPoolSpendingPath) -> WitnessValues {
    let mut map = HashMap::new();

    // PATH witness
    map.insert(
        WitnessName::from_str_unchecked("PATH"),
        build_path_value(path),
    );

    // SWAP_PAIR: u8 encoding of the pair
    let swap_pair_val: u8 = match path {
        AmmPoolSpendingPath::Swap { swap_pair, .. } => *swap_pair as u8,
        _ => 0, // dummy for non-swap paths
    };
    map.insert(
        WitnessName::from_str_unchecked("SWAP_PAIR"),
        Value::u8(swap_pair_val),
    );

    // ISSUED_LP: tapdata state
    let issued_lp_val: u64 = match path {
        AmmPoolSpendingPath::Swap { issued_lp, .. } => *issued_lp,
        AmmPoolSpendingPath::LpDepositWithdraw { issued_lp, .. } => *issued_lp,
        AmmPoolSpendingPath::Secondary => 0, // dummy
    };
    map.insert(
        WitnessName::from_str_unchecked("ISSUED_LP"),
        Value::u64(issued_lp_val),
    );

    // Blinding factors for reissuance token verification
    let bf = match path {
        AmmPoolSpendingPath::Swap { blinding, .. } => *blinding,
        AmmPoolSpendingPath::LpDepositWithdraw { blinding, .. } => *blinding,
        AmmPoolSpendingPath::Secondary => RtBlindingFactors::default(),
    };

    map.insert(
        WitnessName::from_str_unchecked("INPUT_ABF"),
        u256_val(&bf.input_abf),
    );
    map.insert(
        WitnessName::from_str_unchecked("INPUT_VBF"),
        u256_val(&bf.input_vbf),
    );
    map.insert(
        WitnessName::from_str_unchecked("OUTPUT_ABF"),
        u256_val(&bf.output_abf),
    );
    map.insert(
        WitnessName::from_str_unchecked("OUTPUT_VBF"),
        u256_val(&bf.output_vbf),
    );

    WitnessValues::from(map)
}

/// Satisfy an AMM pool contract with the given spending path (no pruning).
pub fn satisfy_amm_pool(
    contract: &CompiledAmmPool,
    path: &AmmPoolSpendingPath,
) -> crate::error::Result<SatisfiedProgram> {
    let witness_values = build_amm_pool_witness(path);
    contract
        .program()
        .satisfy(witness_values)
        .map_err(|e| crate::error::Error::Witness(format!("amm pool witness satisfaction: {e}")))
}

/// Satisfy an AMM pool contract with pruning enabled via an ElementsEnv.
pub fn satisfy_amm_pool_with_env(
    contract: &CompiledAmmPool,
    path: &AmmPoolSpendingPath,
    env: Option<&ElementsEnv<Arc<Transaction>>>,
) -> crate::error::Result<SatisfiedProgram> {
    let witness_values = build_amm_pool_witness(path);
    contract
        .program()
        .satisfy_with_env(witness_values, env)
        .map_err(|e| crate::error::Error::Witness(format!("amm pool witness satisfaction: {e}")))
}

/// Serialize a satisfied program into (program_bytes, witness_bytes).
pub fn serialize_satisfied(satisfied: &SatisfiedProgram) -> (Vec<u8>, Vec<u8>) {
    satisfied.redeem().to_vec_with_witness()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::amm_pool::params::AmmPoolParams;
    use crate::taproot::NUMS_KEY_BYTES;

    fn test_params() -> AmmPoolParams {
        AmmPoolParams {
            yes_asset_id: [0x01; 32],
            no_asset_id: [0x02; 32],
            lbtc_asset_id: [0x03; 32],
            lp_asset_id: [0x04; 32],
            lp_reissuance_token_id: [0x05; 32],
            fee_bps: 30,
            cosigner_pubkey: NUMS_KEY_BYTES,
        }
    }

    #[test]
    fn swap_witness_satisfies_contract() {
        let contract = CompiledAmmPool::new(test_params()).unwrap();
        let path = AmmPoolSpendingPath::Swap {
            swap_pair: SwapPair::YesLbtc,
            issued_lp: 1000,
            blinding: RtBlindingFactors::default(),
        };
        let witness = build_amm_pool_witness(&path);
        let satisfied = contract
            .program()
            .satisfy(witness)
            .expect("swap witness should satisfy");
        let (prog, wit) = serialize_satisfied(&satisfied);
        assert!(!prog.is_empty());
        assert!(!wit.is_empty());
    }

    #[test]
    fn lp_witness_satisfies_contract() {
        let contract = CompiledAmmPool::new(test_params()).unwrap();
        let path = AmmPoolSpendingPath::LpDepositWithdraw {
            issued_lp: 1000,
            blinding: RtBlindingFactors::default(),
        };
        let witness = build_amm_pool_witness(&path);
        let satisfied = contract
            .program()
            .satisfy(witness)
            .expect("lp witness should satisfy");
        let (prog, wit) = serialize_satisfied(&satisfied);
        assert!(!prog.is_empty());
        assert!(!wit.is_empty());
    }

    #[test]
    fn secondary_witness_satisfies_contract() {
        let contract = CompiledAmmPool::new(test_params()).unwrap();
        let path = AmmPoolSpendingPath::Secondary;
        let witness = build_amm_pool_witness(&path);
        let satisfied = contract
            .program()
            .satisfy(witness)
            .expect("secondary witness should satisfy");
        let (prog, wit) = serialize_satisfied(&satisfied);
        assert!(!prog.is_empty());
        assert!(!wit.is_empty());
    }
}
