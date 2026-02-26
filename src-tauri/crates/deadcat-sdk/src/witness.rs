use std::collections::HashMap;
use std::sync::Arc;

use simplicityhl::elements::Transaction;
use simplicityhl::num::U256;
use simplicityhl::simplicity::jet::elements::ElementsEnv;
use simplicityhl::str::WitnessName;
use simplicityhl::types::{ResolvedType, TypeConstructible};
use simplicityhl::value::ValueConstructible;
use simplicityhl::{SatisfiedProgram, Value, WitnessValues};

use crate::contract::CompiledContract;
use crate::state::MarketState;

/// Blinding factors for a reissuance token (confidential input/output).
#[derive(Debug, Clone, Copy, Default)]
pub struct ReissuanceBlindingFactors {
    pub input_abf: [u8; 32],
    pub input_vbf: [u8; 32],
    pub output_abf: [u8; 32],
    pub output_vbf: [u8; 32],
}

/// All reissuance blinding factors needed for paths that cycle tokens.
#[derive(Debug, Clone, Copy, Default)]
pub struct AllBlindingFactors {
    pub yes: ReissuanceBlindingFactors,
    pub no: ReissuanceBlindingFactors,
}

/// The spending path for a prediction market transaction.
#[derive(Debug, Clone)]
pub enum SpendingPath {
    InitialIssuance {
        blinding: AllBlindingFactors,
    },
    SubsequentIssuance {
        blinding: AllBlindingFactors,
    },
    OracleResolve {
        outcome_yes: bool,
        oracle_signature: [u8; 64],
        blinding: AllBlindingFactors,
    },
    PostResolutionRedemption {
        tokens_burned: u64,
    },
    ExpiryRedemption {
        tokens_burned: u64,
        burn_token_asset: [u8; 32],
    },
    Cancellation {
        pairs_burned: u64,
        blinding: Option<AllBlindingFactors>,
    },
    SecondaryCovenantInput,
}

/// The type used for the "other side" of an Either when building PATH values.
/// Since we just need `()` (unit) as the leaf payload, we use unit types.
fn unit_ty() -> ResolvedType {
    ResolvedType::unit()
}

/// Build the nested Either PATH value for the 7-path dispatch tree.
///
/// Tree structure (type):
///   Either<
///     Either<Either<(),()>, Either<(),()>>,   -- Left side (paths 1-4)
///     Either<Either<(),()>, ()>                -- Right side (paths 5-7)
///   >
///
/// Values:
///   Left(Left(Left(unit)))       = Path 1: Initial Issuance
///   Left(Left(Right(unit)))      = Path 2: Subsequent Issuance
///   Left(Right(Left(unit)))      = Path 3: Oracle Resolve
///   Left(Right(Right(unit)))     = Path 4: Post-Resolution Redemption
///   Right(Left(Left(unit)))      = Path 5: Expiry Redemption
///   Right(Left(Right(unit)))     = Path 6: Cancellation
///   Right(Right(unit))           = Path 7: Secondary Covenant Input
fn build_path_value(path: &SpendingPath) -> Value {
    let u = || Value::unit();
    let ut = unit_ty;

    // Type building helpers for the tree structure
    let pair_ty = || ResolvedType::either(ut(), ut()); // Either<(), ()>
    let left_ty = || ResolvedType::either(pair_ty(), pair_ty()); // Either<Either<(),()>, Either<(),()>>
    let right_ty = || ResolvedType::either(pair_ty(), ut()); // Either<Either<(),()>, ()>

    match path {
        SpendingPath::InitialIssuance { .. } => {
            // Left(Left(Left(unit)))
            let inner = Value::left(u(), ut());
            let inner = Value::left(inner, pair_ty());
            Value::left(inner, right_ty())
        }
        SpendingPath::SubsequentIssuance { .. } => {
            // Left(Left(Right(unit)))
            let inner = Value::right(ut(), u());
            let inner = Value::left(inner, pair_ty());
            Value::left(inner, right_ty())
        }
        SpendingPath::OracleResolve { .. } => {
            // Left(Right(Left(unit)))
            let inner = Value::left(u(), ut());
            let inner = Value::right(pair_ty(), inner);
            Value::left(inner, right_ty())
        }
        SpendingPath::PostResolutionRedemption { .. } => {
            // Left(Right(Right(unit)))
            let inner = Value::right(ut(), u());
            let inner = Value::right(pair_ty(), inner);
            Value::left(inner, right_ty())
        }
        SpendingPath::ExpiryRedemption { .. } => {
            // Right(Left(Left(unit)))
            let inner = Value::left(u(), ut());
            let inner = Value::left(inner, ut());
            Value::right(left_ty(), inner)
        }
        SpendingPath::Cancellation { .. } => {
            // Right(Left(Right(unit)))
            let inner = Value::right(ut(), u());
            let inner = Value::left(inner, ut());
            Value::right(left_ty(), inner)
        }
        SpendingPath::SecondaryCovenantInput => {
            // Right(Right(unit))
            let inner = Value::right(pair_ty(), u());
            Value::right(left_ty(), inner)
        }
    }
}

fn u256_val(bytes: &[u8; 32]) -> Value {
    Value::u256(U256::from_byte_array(*bytes))
}

/// Build the complete witness values for a spending path.
fn build_witness_values(path: &SpendingPath, state: MarketState) -> WitnessValues {
    let mut map = HashMap::new();

    // STATE witness
    map.insert(
        WitnessName::from_str_unchecked("STATE"),
        Value::u64(state.as_u64()),
    );

    // PATH witness (nested Either)
    map.insert(
        WitnessName::from_str_unchecked("PATH"),
        build_path_value(path),
    );

    let zero = [0u8; 32];

    // Budget padding witnesses (must match the .simf contract's BUDGET_PAD_A/B/C/D).
    map.insert(
        WitnessName::from_str_unchecked("BUDGET_PAD_A"),
        u256_val(&zero),
    );
    map.insert(
        WitnessName::from_str_unchecked("BUDGET_PAD_B"),
        u256_val(&zero),
    );
    map.insert(
        WitnessName::from_str_unchecked("BUDGET_PAD_C"),
        u256_val(&zero),
    );
    map.insert(
        WitnessName::from_str_unchecked("BUDGET_PAD_D"),
        u256_val(&zero),
    );

    match path {
        SpendingPath::InitialIssuance { blinding } => {
            set_blinding_map(&mut map, blinding);
            set_zero_oracle_map(&mut map);
            set_zero_redemption_map(&mut map);
        }
        SpendingPath::SubsequentIssuance { blinding } => {
            set_blinding_map(&mut map, blinding);
            set_zero_oracle_map(&mut map);
            set_zero_redemption_map(&mut map);
        }
        SpendingPath::OracleResolve {
            outcome_yes,
            oracle_signature,
            blinding,
        } => {
            set_blinding_map(&mut map, blinding);
            map.insert(
                WitnessName::from_str_unchecked("ORACLE_OUTCOME_YES"),
                Value::from(*outcome_yes),
            );
            map.insert(
                WitnessName::from_str_unchecked("ORACLE_SIGNATURE"),
                Value::byte_array(oracle_signature.iter().copied()),
            );
            set_zero_redemption_map(&mut map);
        }
        SpendingPath::PostResolutionRedemption { tokens_burned } => {
            set_zero_blinding_map(&mut map);
            set_zero_oracle_map(&mut map);
            map.insert(
                WitnessName::from_str_unchecked("TOKENS_BURNED"),
                Value::u64(*tokens_burned),
            );
            map.insert(
                WitnessName::from_str_unchecked("BURN_TOKEN_ASSET"),
                u256_val(&zero),
            );
            map.insert(
                WitnessName::from_str_unchecked("PAIRS_BURNED"),
                Value::u64(0),
            );
        }
        SpendingPath::ExpiryRedemption {
            tokens_burned,
            burn_token_asset,
        } => {
            set_zero_blinding_map(&mut map);
            set_zero_oracle_map(&mut map);
            map.insert(
                WitnessName::from_str_unchecked("TOKENS_BURNED"),
                Value::u64(*tokens_burned),
            );
            map.insert(
                WitnessName::from_str_unchecked("BURN_TOKEN_ASSET"),
                u256_val(burn_token_asset),
            );
            map.insert(
                WitnessName::from_str_unchecked("PAIRS_BURNED"),
                Value::u64(0),
            );
        }
        SpendingPath::Cancellation {
            pairs_burned,
            blinding,
        } => {
            if let Some(bf) = blinding {
                set_blinding_map(&mut map, bf);
            } else {
                set_zero_blinding_map(&mut map);
            }
            set_zero_oracle_map(&mut map);
            map.insert(
                WitnessName::from_str_unchecked("TOKENS_BURNED"),
                Value::u64(0),
            );
            map.insert(
                WitnessName::from_str_unchecked("BURN_TOKEN_ASSET"),
                u256_val(&zero),
            );
            map.insert(
                WitnessName::from_str_unchecked("PAIRS_BURNED"),
                Value::u64(*pairs_burned),
            );
        }
        SpendingPath::SecondaryCovenantInput => {
            set_zero_blinding_map(&mut map);
            set_zero_oracle_map(&mut map);
            set_zero_redemption_map(&mut map);
        }
    }

    WitnessValues::from(map)
}

fn set_blinding_map(map: &mut HashMap<WitnessName, Value>, bf: &AllBlindingFactors) {
    map.insert(
        WitnessName::from_str_unchecked("YES_REISSUANCE_INPUT_ABF"),
        u256_val(&bf.yes.input_abf),
    );
    map.insert(
        WitnessName::from_str_unchecked("YES_REISSUANCE_INPUT_VBF"),
        u256_val(&bf.yes.input_vbf),
    );
    map.insert(
        WitnessName::from_str_unchecked("YES_REISSUANCE_OUTPUT_ABF"),
        u256_val(&bf.yes.output_abf),
    );
    map.insert(
        WitnessName::from_str_unchecked("YES_REISSUANCE_OUTPUT_VBF"),
        u256_val(&bf.yes.output_vbf),
    );
    map.insert(
        WitnessName::from_str_unchecked("NO_REISSUANCE_INPUT_ABF"),
        u256_val(&bf.no.input_abf),
    );
    map.insert(
        WitnessName::from_str_unchecked("NO_REISSUANCE_INPUT_VBF"),
        u256_val(&bf.no.input_vbf),
    );
    map.insert(
        WitnessName::from_str_unchecked("NO_REISSUANCE_OUTPUT_ABF"),
        u256_val(&bf.no.output_abf),
    );
    map.insert(
        WitnessName::from_str_unchecked("NO_REISSUANCE_OUTPUT_VBF"),
        u256_val(&bf.no.output_vbf),
    );
}

fn set_zero_blinding_map(map: &mut HashMap<WitnessName, Value>) {
    set_blinding_map(map, &AllBlindingFactors::default());
}

fn set_zero_oracle_map(map: &mut HashMap<WitnessName, Value>) {
    map.insert(
        WitnessName::from_str_unchecked("ORACLE_OUTCOME_YES"),
        Value::from(false),
    );
    map.insert(
        WitnessName::from_str_unchecked("ORACLE_SIGNATURE"),
        Value::byte_array([0u8; 64].iter().copied()),
    );
}

fn set_zero_redemption_map(map: &mut HashMap<WitnessName, Value>) {
    let zero = [0u8; 32];
    map.insert(
        WitnessName::from_str_unchecked("TOKENS_BURNED"),
        Value::u64(0),
    );
    map.insert(
        WitnessName::from_str_unchecked("BURN_TOKEN_ASSET"),
        u256_val(&zero),
    );
    map.insert(
        WitnessName::from_str_unchecked("PAIRS_BURNED"),
        Value::u64(0),
    );
}

/// Satisfy a compiled contract with the given spending path and state.
///
/// Builds witness values from the path and state, then calls `program.satisfy()`.
/// Note: this does NOT prune the program. Use `satisfy_contract_with_env` for
/// on-chain transactions that require pruning.
#[cfg(any(test, feature = "testing"))]
pub fn satisfy_contract(
    contract: &CompiledContract,
    path: &SpendingPath,
    state: MarketState,
) -> Result<SatisfiedProgram, String> {
    let witness_values = build_witness_values(path, state);
    contract.program().satisfy(witness_values)
}

/// Satisfy a compiled contract with pruning enabled via an ElementsEnv.
///
/// When `env` is `Some`, the program is pruned: un-taken case branches are
/// replaced with HIDDEN nodes containing only their CMR. This is required by
/// Simplicity's anti-DOS consensus rules (every visible node must be executed).
pub fn satisfy_contract_with_env(
    contract: &CompiledContract,
    path: &SpendingPath,
    state: MarketState,
    env: Option<&ElementsEnv<Arc<Transaction>>>,
) -> Result<SatisfiedProgram, String> {
    let witness_values = build_witness_values(path, state);
    contract.program().satisfy_with_env(witness_values, env)
}

/// Serialize a satisfied program into (program_bytes, witness_bytes).
///
/// Calls `redeem().encode_to_vec()` which returns the bit-encoded program
/// and witness data ready for inclusion in a transaction.
pub fn serialize_satisfied(satisfied: &SatisfiedProgram) -> (Vec<u8>, Vec<u8>) {
    satisfied.redeem().to_vec_with_witness()
}
