use lwk_wollet::elements::{OutPoint, Transaction};
use simplicityhl::Value as HlValue;
use simplicityhl::simplicity::dag::{DagLike, InternalSharing};
use simplicityhl::simplicity::jet::Elements;
use simplicityhl::simplicity::{BitIter, RedeemNode};
use simplicityhl::types::{ResolvedType, TypeConstructible};
use simplicityhl::value::{StructuralValue, UIntValue, ValueInner};

use crate::discovery::pool::LMSR_WITNESS_SCHEMA_V2;
use crate::error::{Error, Result};

use super::contract::CompiledLmsrPool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DecodedPrimaryWitnessPayload {
    pub out_base: u32,
    pub old_s_index: u64,
    pub new_s_index: u64,
    pub path_tag: u8,
}

fn scan_payload_type() -> ResolvedType {
    ResolvedType::tuple([
        ResolvedType::u32(),
        ResolvedType::tuple([
            ResolvedType::u64(),
            ResolvedType::tuple([ResolvedType::u64(), ResolvedType::u8()]),
        ]),
    ])
}

fn as_tuple2(value: &HlValue) -> Option<(&HlValue, &HlValue)> {
    match value.inner() {
        ValueInner::Tuple(elements) if elements.len() == 2 => Some((&elements[0], &elements[1])),
        _ => None,
    }
}

fn as_u8(value: &HlValue) -> Option<u8> {
    match value.inner() {
        ValueInner::UInt(UIntValue::U8(v)) => Some(*v),
        _ => None,
    }
}

fn as_u32(value: &HlValue) -> Option<u32> {
    match value.inner() {
        ValueInner::UInt(UIntValue::U32(v)) => Some(*v),
        _ => None,
    }
}

fn as_u64(value: &HlValue) -> Option<u64> {
    match value.inner() {
        ValueInner::UInt(UIntValue::U64(v)) => Some(*v),
        _ => None,
    }
}

fn decode_scan_payload_candidate(
    value: &simplicityhl::simplicity::Value,
) -> Option<DecodedPrimaryWitnessPayload> {
    let structural = StructuralValue::from(value.clone());
    let decoded = HlValue::reconstruct(&structural, &scan_payload_type())?;
    let (out_base_val, rest_0) = as_tuple2(&decoded)?;
    let (old_s_val, rest_1) = as_tuple2(rest_0)?;
    let (new_s_val, path_tag_val) = as_tuple2(rest_1)?;
    Some(DecodedPrimaryWitnessPayload {
        out_base: as_u32(out_base_val)?,
        old_s_index: as_u64(old_s_val)?,
        new_s_index: as_u64(new_s_val)?,
        path_tag: as_u8(path_tag_val)?,
    })
}

fn decode_scan_payload_from_stack(
    witness_bytes: &[u8],
    program_bytes: &[u8],
    expected_primary_cmr: [u8; 32],
) -> Result<DecodedPrimaryWitnessPayload> {
    let redeem = RedeemNode::<Elements>::decode(
        BitIter::from(program_bytes.iter().copied()),
        BitIter::from(witness_bytes.iter().copied()),
    )
    .map_err(|e| {
        Error::TradeRouting(format!("failed to decode LMSR primary witness stack: {e}"))
    })?;

    if redeem.cmr().to_byte_array() != expected_primary_cmr {
        return Err(Error::TradeRouting(
            "decoded LMSR witness program CMR does not match expected primary CMR".into(),
        ));
    }

    let mut decoded_payload: Option<DecodedPrimaryWitnessPayload> = None;
    for witness in redeem
        .as_ref()
        .post_order_iter::<InternalSharing>()
        .into_witnesses()
    {
        let Some(candidate) = decode_scan_payload_candidate(witness) else {
            continue;
        };
        match decoded_payload {
            None => decoded_payload = Some(candidate),
            Some(existing) if existing == candidate => {}
            Some(_) => {
                return Err(Error::TradeRouting(
                    "ambiguous LMSR primary witness: multiple SCAN_PAYLOAD candidates".into(),
                ));
            }
        }
    }

    let payload = decoded_payload.ok_or_else(|| {
        Error::TradeRouting(
            "missing SCAN_PAYLOAD in LMSR primary witness (schema v2 required)".into(),
        )
    })?;
    if payload.path_tag > 1 {
        return Err(Error::TradeRouting(format!(
            "invalid LMSR primary path tag in SCAN_PAYLOAD: {}",
            payload.path_tag
        )));
    }
    Ok(payload)
}

/// Decode canonical LMSR scan payload from the primary witness input.
pub(crate) fn decode_primary_witness_payload_from_primary_input(
    spend_tx: &Transaction,
    prior_yes_outpoint: OutPoint,
    contract: &CompiledLmsrPool,
    witness_schema_version: &str,
) -> Result<DecodedPrimaryWitnessPayload> {
    if witness_schema_version != LMSR_WITNESS_SCHEMA_V2 {
        return Err(Error::TradeRouting(format!(
            "unsupported LMSR witness schema: {witness_schema_version}"
        )));
    }

    let primary_cmr = contract.primary_cmr().to_byte_array();
    let yes_inputs: Vec<usize> = spend_tx
        .input
        .iter()
        .enumerate()
        .filter_map(|(idx, input)| (input.previous_output == prior_yes_outpoint).then_some(idx))
        .collect();

    let yes_idx = match yes_inputs.as_slice() {
        [] => {
            return Err(Error::TradeRouting(
                "missing canonical YES reserve input in transition tx".into(),
            ));
        }
        [idx] => *idx,
        _ => {
            return Err(Error::TradeRouting(
                "ambiguous LMSR transition: canonical YES outpoint appears multiple times".into(),
            ));
        }
    };

    let yes_stack = &spend_tx.input[yes_idx].witness.script_witness;
    if yes_stack.len() != 4 {
        return Err(Error::TradeRouting(
            "canonical YES reserve input does not carry LMSR primary witness stack".into(),
        ));
    }
    if yes_stack[0].is_empty() || yes_stack[1].is_empty() || yes_stack[3].is_empty() {
        return Err(Error::TradeRouting(
            "LMSR primary witness stack contains empty elements".into(),
        ));
    }
    if yes_stack[2].len() != 32 || yes_stack[2].as_slice() != primary_cmr {
        return Err(Error::TradeRouting(
            "canonical YES reserve input witness does not prove LMSR primary leaf".into(),
        ));
    }

    for (idx, input) in spend_tx.input.iter().enumerate() {
        if idx == yes_idx {
            continue;
        }
        let stack = &input.witness.script_witness;
        if stack.len() != 4 {
            continue;
        }
        let cmr_bytes = &stack[2];
        if cmr_bytes.len() != 32 {
            continue;
        }
        if cmr_bytes.as_slice() != primary_cmr {
            continue;
        }
        return Err(Error::TradeRouting(
            "ambiguous LMSR transition: multiple primary witness inputs".into(),
        ));
    }

    decode_scan_payload_from_stack(&yes_stack[0], &yes_stack[1], primary_cmr)
}
