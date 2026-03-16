use std::collections::HashMap;
use std::sync::Arc;

use simplicityhl::elements::Transaction;
use simplicityhl::num::U256;
use simplicityhl::simplicity::jet::elements::ElementsEnv;
use simplicityhl::str::WitnessName;
use simplicityhl::types::{ResolvedType, TypeConstructible};
use simplicityhl::value::ValueConstructible;
use simplicityhl::{SatisfiedProgram, Value, WitnessValues};

use crate::prediction_market::contract::CompiledPredictionMarket;
use crate::prediction_market::state::MarketSlot;

/// Blinding factors for a reissuance token (confidential input/output).
#[derive(Debug, Clone, Copy, Default)]
pub struct ReissuanceBlindingFactors {
    pub input_abf: [u8; 32],
    pub input_vbf: [u8; 32],
    pub output_abf: [u8; 32],
    pub output_vbf: [u8; 32],
}

/// All reissuance blinding factors needed for paths that cycle or inspect tokens.
#[derive(Debug, Clone, Copy, Default)]
pub struct AllBlindingFactors {
    pub yes: ReissuanceBlindingFactors,
    pub no: ReissuanceBlindingFactors,
}

/// Explicit prediction-market spend kinds.
#[derive(Debug, Clone)]
pub enum PredictionMarketSpendingPath {
    InitialIssuancePrimary {
        blinding: AllBlindingFactors,
    },
    InitialIssuanceSecondaryNoRt {
        blinding: AllBlindingFactors,
    },
    SubsequentIssuancePrimary {
        blinding: AllBlindingFactors,
    },
    SubsequentIssuanceSecondaryNoRt {
        blinding: AllBlindingFactors,
    },
    SubsequentIssuanceSecondaryCollateral,
    OracleResolvePrimary {
        outcome_yes: bool,
        oracle_signature: [u8; 64],
        blinding: AllBlindingFactors,
    },
    OracleResolveSecondaryNoRt {
        blinding: AllBlindingFactors,
    },
    OracleResolveSecondaryCollateral,
    PostResolutionRedemption {
        tokens_burned: u64,
    },
    ExpireTransitionPrimary {
        blinding: AllBlindingFactors,
    },
    ExpireTransitionSecondaryNoRt {
        blinding: AllBlindingFactors,
    },
    ExpireTransitionSecondaryCollateral,
    ExpiryRedemption {
        tokens_burned: u64,
        burn_token_asset: [u8; 32],
    },
    CancellationPartial {
        pairs_burned: u64,
    },
    CancellationFullPrimary {
        pairs_burned: u64,
        blinding: AllBlindingFactors,
    },
    CancellationFullSecondaryYesRt {
        blinding: AllBlindingFactors,
    },
    CancellationFullSecondaryNoRt {
        blinding: AllBlindingFactors,
    },
}

fn unit_ty() -> ResolvedType {
    ResolvedType::unit()
}

fn path1_or_2_ty() -> ResolvedType {
    ResolvedType::either(unit_ty(), unit_ty())
}

fn path3_or_4_ty() -> ResolvedType {
    ResolvedType::either(unit_ty(), unit_ty())
}

fn path5_or_6_ty() -> ResolvedType {
    ResolvedType::either(unit_ty(), unit_ty())
}

fn path7_or_8_ty() -> ResolvedType {
    ResolvedType::either(unit_ty(), unit_ty())
}

fn path1_to_4_ty() -> ResolvedType {
    ResolvedType::either(path1_or_2_ty(), path3_or_4_ty())
}

fn path5_to_8_ty() -> ResolvedType {
    ResolvedType::either(path5_or_6_ty(), path7_or_8_ty())
}

fn path1_to_8_ty() -> ResolvedType {
    ResolvedType::either(path1_to_4_ty(), path5_to_8_ty())
}

fn path9_or_10_ty() -> ResolvedType {
    ResolvedType::either(unit_ty(), unit_ty())
}

fn path11_or_12_ty() -> ResolvedType {
    ResolvedType::either(unit_ty(), unit_ty())
}

fn path9_to_12_ty() -> ResolvedType {
    ResolvedType::either(path9_or_10_ty(), path11_or_12_ty())
}

fn path13_or_14_ty() -> ResolvedType {
    ResolvedType::either(unit_ty(), unit_ty())
}

fn path16_or_17_ty() -> ResolvedType {
    ResolvedType::either(unit_ty(), unit_ty())
}

fn path15_to_17_ty() -> ResolvedType {
    ResolvedType::either(unit_ty(), path16_or_17_ty())
}

fn path13_to_17_ty() -> ResolvedType {
    ResolvedType::either(path13_or_14_ty(), path15_to_17_ty())
}

fn path9_to_17_ty() -> ResolvedType {
    ResolvedType::either(path9_to_12_ty(), path13_to_17_ty())
}

fn build_path_value(path: &PredictionMarketSpendingPath) -> Value {
    let u = || Value::unit();

    match path {
        PredictionMarketSpendingPath::InitialIssuancePrimary { .. } => {
            let v = Value::left(u(), unit_ty());
            let v = Value::left(v, path3_or_4_ty());
            let v = Value::left(v, path5_to_8_ty());
            Value::left(v, path9_to_17_ty())
        }
        PredictionMarketSpendingPath::InitialIssuanceSecondaryNoRt { .. } => {
            let v = Value::right(unit_ty(), u());
            let v = Value::left(v, path3_or_4_ty());
            let v = Value::left(v, path5_to_8_ty());
            Value::left(v, path9_to_17_ty())
        }
        PredictionMarketSpendingPath::SubsequentIssuancePrimary { .. } => {
            let v = Value::left(u(), unit_ty());
            let v = Value::right(path1_or_2_ty(), v);
            let v = Value::left(v, path5_to_8_ty());
            Value::left(v, path9_to_17_ty())
        }
        PredictionMarketSpendingPath::SubsequentIssuanceSecondaryNoRt { .. } => {
            let v = Value::right(unit_ty(), u());
            let v = Value::right(path1_or_2_ty(), v);
            let v = Value::left(v, path5_to_8_ty());
            Value::left(v, path9_to_17_ty())
        }
        PredictionMarketSpendingPath::SubsequentIssuanceSecondaryCollateral => {
            let v = Value::left(u(), unit_ty());
            let v = Value::left(v, path7_or_8_ty());
            let v = Value::right(path1_to_4_ty(), v);
            Value::left(v, path9_to_17_ty())
        }
        PredictionMarketSpendingPath::OracleResolvePrimary { .. } => {
            let v = Value::right(unit_ty(), u());
            let v = Value::left(v, path5_or_6_ty());
            let v = Value::right(path1_to_4_ty(), v);
            Value::left(v, path9_to_17_ty())
        }
        PredictionMarketSpendingPath::OracleResolveSecondaryNoRt { .. } => {
            let v = Value::left(u(), unit_ty());
            let v = Value::right(path1_or_2_ty(), v);
            let v = Value::right(path1_to_4_ty(), v);
            Value::left(v, path9_to_17_ty())
        }
        PredictionMarketSpendingPath::OracleResolveSecondaryCollateral => {
            let v = Value::right(unit_ty(), u());
            let v = Value::right(path1_or_2_ty(), v);
            let v = Value::right(path1_to_4_ty(), v);
            Value::left(v, path9_to_17_ty())
        }
        PredictionMarketSpendingPath::PostResolutionRedemption { .. } => {
            let v = Value::left(u(), unit_ty());
            let v = Value::left(v, path11_or_12_ty());
            Value::right(path1_to_8_ty(), Value::left(v, path13_to_17_ty()))
        }
        PredictionMarketSpendingPath::ExpireTransitionPrimary { .. } => {
            let v = Value::right(unit_ty(), u());
            let v = Value::left(v, path11_or_12_ty());
            Value::right(path1_to_8_ty(), Value::left(v, path13_to_17_ty()))
        }
        PredictionMarketSpendingPath::ExpireTransitionSecondaryNoRt { .. } => {
            let v = Value::left(u(), unit_ty());
            let v = Value::right(path9_or_10_ty(), v);
            Value::right(path1_to_8_ty(), Value::left(v, path13_to_17_ty()))
        }
        PredictionMarketSpendingPath::ExpireTransitionSecondaryCollateral => {
            let v = Value::right(unit_ty(), u());
            let v = Value::right(path9_or_10_ty(), v);
            Value::right(path1_to_8_ty(), Value::left(v, path13_to_17_ty()))
        }
        PredictionMarketSpendingPath::ExpiryRedemption { .. } => {
            let v = Value::left(u(), unit_ty());
            let v = Value::left(v, path15_to_17_ty());
            Value::right(path1_to_8_ty(), Value::right(path9_to_12_ty(), v))
        }
        PredictionMarketSpendingPath::CancellationPartial { .. } => {
            let v = Value::right(unit_ty(), u());
            let v = Value::left(v, path15_to_17_ty());
            Value::right(path1_to_8_ty(), Value::right(path9_to_12_ty(), v))
        }
        PredictionMarketSpendingPath::CancellationFullPrimary { .. } => {
            let v = Value::left(u(), path16_or_17_ty());
            let v = Value::right(path13_or_14_ty(), v);
            Value::right(path1_to_8_ty(), Value::right(path9_to_12_ty(), v))
        }
        PredictionMarketSpendingPath::CancellationFullSecondaryYesRt { .. } => {
            let v = Value::left(u(), unit_ty());
            let v = Value::right(unit_ty(), v);
            let v = Value::right(path13_or_14_ty(), v);
            Value::right(path1_to_8_ty(), Value::right(path9_to_12_ty(), v))
        }
        PredictionMarketSpendingPath::CancellationFullSecondaryNoRt { .. } => {
            let v = Value::right(unit_ty(), u());
            let v = Value::right(unit_ty(), v);
            let v = Value::right(path13_or_14_ty(), v);
            Value::right(path1_to_8_ty(), Value::right(path9_to_12_ty(), v))
        }
    }
}

fn u256_val(bytes: &[u8; 32]) -> Value {
    Value::u256(U256::from_byte_array(*bytes))
}

fn build_witness_values(path: &PredictionMarketSpendingPath, slot: MarketSlot) -> WitnessValues {
    let mut map = HashMap::new();

    map.insert(
        WitnessName::from_str_unchecked("SLOT"),
        Value::u8(slot.as_u8()),
    );
    map.insert(
        WitnessName::from_str_unchecked("PATH"),
        build_path_value(path),
    );

    let zero = [0u8; 32];
    for name in [
        "BUDGET_PAD_A",
        "BUDGET_PAD_B",
        "BUDGET_PAD_C",
        "BUDGET_PAD_D",
        "BUDGET_PAD_E",
        "BUDGET_PAD_F",
        "BUDGET_PAD_G",
        "BUDGET_PAD_H",
        "BUDGET_PAD_I",
        "BUDGET_PAD_J",
        "BUDGET_PAD_K",
        "BUDGET_PAD_L",
        "BUDGET_PAD_M",
        "BUDGET_PAD_N",
        "BUDGET_PAD_O",
        "BUDGET_PAD_P",
        "BUDGET_PAD_Q",
        "BUDGET_PAD_R",
        "BUDGET_PAD_S",
        "BUDGET_PAD_T",
        "BUDGET_PAD_U",
        "BUDGET_PAD_V",
        "BUDGET_PAD_W",
        "BUDGET_PAD_X",
    ] {
        map.insert(WitnessName::from_str_unchecked(name), u256_val(&zero));
    }

    match path {
        PredictionMarketSpendingPath::InitialIssuancePrimary { blinding }
        | PredictionMarketSpendingPath::InitialIssuanceSecondaryNoRt { blinding }
        | PredictionMarketSpendingPath::SubsequentIssuancePrimary { blinding }
        | PredictionMarketSpendingPath::SubsequentIssuanceSecondaryNoRt { blinding }
        | PredictionMarketSpendingPath::OracleResolveSecondaryNoRt { blinding }
        | PredictionMarketSpendingPath::ExpireTransitionPrimary { blinding }
        | PredictionMarketSpendingPath::ExpireTransitionSecondaryNoRt { blinding }
        | PredictionMarketSpendingPath::CancellationFullPrimary { blinding, .. }
        | PredictionMarketSpendingPath::CancellationFullSecondaryYesRt { blinding }
        | PredictionMarketSpendingPath::CancellationFullSecondaryNoRt { blinding } => {
            set_blinding_map(&mut map, blinding);
        }
        PredictionMarketSpendingPath::OracleResolvePrimary { blinding, .. } => {
            set_blinding_map(&mut map, blinding);
        }
        _ => set_zero_blinding_map(&mut map),
    }

    match path {
        PredictionMarketSpendingPath::OracleResolvePrimary {
            outcome_yes,
            oracle_signature,
            ..
        } => {
            map.insert(
                WitnessName::from_str_unchecked("ORACLE_OUTCOME_YES"),
                Value::from(*outcome_yes),
            );
            map.insert(
                WitnessName::from_str_unchecked("ORACLE_SIGNATURE"),
                Value::byte_array(oracle_signature.iter().copied()),
            );
        }
        _ => set_zero_oracle_map(&mut map),
    }

    match path {
        PredictionMarketSpendingPath::PostResolutionRedemption { tokens_burned }
        | PredictionMarketSpendingPath::ExpiryRedemption { tokens_burned, .. } => {
            map.insert(
                WitnessName::from_str_unchecked("TOKENS_BURNED"),
                Value::u64(*tokens_burned),
            );
        }
        _ => {
            map.insert(
                WitnessName::from_str_unchecked("TOKENS_BURNED"),
                Value::u64(0),
            );
        }
    }

    match path {
        PredictionMarketSpendingPath::ExpiryRedemption {
            burn_token_asset, ..
        } => {
            map.insert(
                WitnessName::from_str_unchecked("BURN_TOKEN_ASSET"),
                u256_val(burn_token_asset),
            );
        }
        _ => {
            map.insert(
                WitnessName::from_str_unchecked("BURN_TOKEN_ASSET"),
                u256_val(&zero),
            );
        }
    }

    match path {
        PredictionMarketSpendingPath::CancellationPartial { pairs_burned }
        | PredictionMarketSpendingPath::CancellationFullPrimary { pairs_burned, .. } => {
            map.insert(
                WitnessName::from_str_unchecked("PAIRS_BURNED"),
                Value::u64(*pairs_burned),
            );
        }
        _ => {
            map.insert(
                WitnessName::from_str_unchecked("PAIRS_BURNED"),
                Value::u64(0),
            );
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

/// Satisfy a compiled contract with the given spending path and slot.
#[cfg(any(test, feature = "testing"))]
pub fn satisfy_contract(
    contract: &CompiledPredictionMarket,
    path: &PredictionMarketSpendingPath,
    slot: MarketSlot,
) -> Result<SatisfiedProgram, String> {
    let witness_values = build_witness_values(path, slot);
    contract.program().satisfy(witness_values)
}

/// Satisfy a compiled contract with pruning enabled via an ElementsEnv.
pub fn satisfy_contract_with_env(
    contract: &CompiledPredictionMarket,
    path: &PredictionMarketSpendingPath,
    slot: MarketSlot,
    env: Option<&ElementsEnv<Arc<Transaction>>>,
) -> Result<SatisfiedProgram, String> {
    let witness_values = build_witness_values(path, slot);
    contract.program().satisfy_with_env(witness_values, env)
}

/// Serialize a satisfied program into (program_bytes, witness_bytes).
pub fn serialize_satisfied(satisfied: &SatisfiedProgram) -> (Vec<u8>, Vec<u8>) {
    satisfied.redeem().to_vec_with_witness()
}
