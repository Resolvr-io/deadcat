use lwk_wollet::elements::confidential::{Asset, Value as ConfValue};
use lwk_wollet::elements::{OutPoint, Script, Transaction, Txid};

use crate::error::{Error, Result};
use crate::lmsr_pool::contract::CompiledLmsrPool;
use crate::lmsr_pool::params::LmsrPoolParams;
use crate::lmsr_pool::witness_decode::{
    DecodedPrimaryWitnessPayload, decode_primary_witness_payload_from_primary_input,
};
use crate::pool::PoolReserves;
use crate::pset::UnblindedUtxo;
use crate::trade::types::LmsrPoolUtxos;

/// Decode canonical LMSR primary witness payload from a transition transaction.
pub(crate) fn decode_primary_witness_payload_from_spend_tx(
    spend_tx: &Transaction,
    prior_yes_outpoint: OutPoint,
    contract: &CompiledLmsrPool,
    witness_schema_version: &str,
) -> Result<DecodedPrimaryWitnessPayload> {
    decode_primary_witness_payload_from_primary_input(
        spend_tx,
        prior_yes_outpoint,
        contract,
        witness_schema_version,
    )
}

/// Decode LMSR `OUT_BASE` from canonical schema payload.
#[allow(dead_code)]
pub(crate) fn decode_out_base_from_spend_tx(
    spend_tx: &Transaction,
    prior_yes_outpoint: OutPoint,
    contract: &CompiledLmsrPool,
    witness_schema_version: &str,
) -> Result<u32> {
    decode_primary_witness_payload_from_spend_tx(
        spend_tx,
        prior_yes_outpoint,
        contract,
        witness_schema_version,
    )
    .map(|payload| payload.out_base)
}

/// Extract canonical LMSR reserve window from `spend_tx` at `out_base`.
pub(crate) fn extract_reserve_window(
    spend_tx: &Transaction,
    out_base: u32,
    params: &LmsrPoolParams,
) -> Result<(LmsrPoolUtxos, PoolReserves, Script)> {
    let out_base = usize::try_from(out_base)
        .map_err(|_| Error::TradeRouting("LMSR OUT_BASE does not fit usize".into()))?;
    if out_base + 2 >= spend_tx.output.len() {
        return Err(Error::TradeRouting(format!(
            "LMSR OUT_BASE window [{out_base}..{}) exceeds tx output count {}",
            out_base + 3,
            spend_tx.output.len()
        )));
    }

    let txid = spend_tx.txid();
    let yes = reserve_unblinded_utxo(spend_tx, txid, out_base, 0, params.yes_asset_id)?;
    let no = reserve_unblinded_utxo(spend_tx, txid, out_base, 1, params.no_asset_id)?;
    let collateral =
        reserve_unblinded_utxo(spend_tx, txid, out_base, 2, params.collateral_asset_id)?;

    if yes.txout.script_pubkey != no.txout.script_pubkey
        || yes.txout.script_pubkey != collateral.txout.script_pubkey
    {
        return Err(Error::TradeRouting(
            "LMSR reserve output scripts do not match".into(),
        ));
    }

    let reserves = PoolReserves {
        r_yes: yes.value,
        r_no: no.value,
        r_lbtc: collateral.value,
    };

    Ok((
        LmsrPoolUtxos {
            yes,
            no,
            collateral,
        },
        reserves,
        spend_tx.output[out_base].script_pubkey.clone(),
    ))
}

fn reserve_unblinded_utxo(
    tx: &Transaction,
    txid: Txid,
    out_base: usize,
    rel: usize,
    expected_asset: [u8; 32],
) -> Result<UnblindedUtxo> {
    let out_index = out_base + rel;
    let txout = tx
        .output
        .get(out_index)
        .ok_or_else(|| {
            Error::TradeRouting(format!("missing LMSR reserve output at index {out_index}"))
        })?
        .clone();

    let asset = match txout.asset {
        Asset::Explicit(asset) => asset,
        _ => {
            return Err(Error::TradeRouting(format!(
                "LMSR reserve output {out_index} asset must be explicit"
            )));
        }
    };
    let value = match txout.value {
        ConfValue::Explicit(v) => v,
        _ => {
            return Err(Error::TradeRouting(format!(
                "LMSR reserve output {out_index} value must be explicit"
            )));
        }
    };

    let actual_asset = asset.into_inner().to_byte_array();
    if actual_asset != expected_asset {
        return Err(Error::TradeRouting(format!(
            "LMSR reserve output {out_index} asset mismatch"
        )));
    }

    Ok(UnblindedUtxo {
        outpoint: OutPoint::new(txid, out_index as u32),
        txout,
        asset_id: actual_asset,
        value,
        asset_blinding_factor: [0u8; 32],
        value_blinding_factor: [0u8; 32],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::pool::LMSR_WITNESS_SCHEMA_V2;
    use crate::lmsr_pool::contract::CompiledLmsrPool;
    use crate::lmsr_pool::math::LmsrTradeKind;
    use crate::lmsr_pool::table::{LmsrTableManifest, lmsr_table_root};
    use crate::lmsr_pool::witness::{satisfy_lmsr_primary_with_env, serialize_satisfied};
    use crate::pset::UnblindedUtxo;
    use crate::trade::types::{LmsrPoolSwapLeg, LmsrPoolUtxos, LmsrPrimaryPath};
    use lwk_wollet::elements::confidential::Nonce;
    use lwk_wollet::elements::hashes::Hash as _;
    use lwk_wollet::elements::{AssetId, TxIn, TxInWitness, TxOut, TxOutWitness};

    fn table_values() -> Vec<u64> {
        vec![2_000, 2_010, 2_025, 2_045, 2_070, 2_100, 2_135, 2_175]
    }

    fn params() -> LmsrPoolParams {
        let root = lmsr_table_root(&table_values()).unwrap();
        LmsrPoolParams {
            yes_asset_id: [0x01; 32],
            no_asset_id: [0x02; 32],
            collateral_asset_id: [0x03; 32],
            lmsr_table_root: root,
            table_depth: 3,
            q_step_lots: 10,
            s_bias: 4,
            s_max_index: 7,
            half_payout_sats: 100,
            fee_bps: 30,
            min_r_yes: 1,
            min_r_no: 1,
            min_r_collateral: 1,
            cosigner_pubkey: crate::taproot::NUMS_KEY_BYTES,
        }
    }

    fn dummy_utxo(asset: [u8; 32], value: u64, vout: u32, spk: &Script) -> UnblindedUtxo {
        UnblindedUtxo {
            outpoint: OutPoint::new(Txid::all_zeros(), vout),
            txout: explicit_txout(asset, value, spk),
            asset_id: asset,
            value,
            asset_blinding_factor: [0u8; 32],
            value_blinding_factor: [0u8; 32],
        }
    }

    fn valid_primary_stack(
        contract: &CompiledLmsrPool,
        params: &LmsrPoolParams,
        in_base: u32,
        out_base: u32,
    ) -> Vec<Vec<u8>> {
        let values = table_values();
        let manifest = LmsrTableManifest::new(params.table_depth, values).unwrap();
        let old_s_index = 3u64;
        let new_s_index = 4u64;
        let old_proof = manifest.proof_at(old_s_index).unwrap();
        let new_proof = manifest.proof_at(new_s_index).unwrap();
        let old_spk = contract.script_pubkey(old_s_index);
        let leg = LmsrPoolSwapLeg {
            primary_path: LmsrPrimaryPath::Swap,
            pool_params: *params,
            pool_id: "scan-test-pool".into(),
            old_s_index,
            new_s_index,
            old_path_bits: old_proof.path_bits,
            new_path_bits: new_proof.path_bits,
            old_siblings: old_proof.siblings,
            new_siblings: new_proof.siblings,
            in_base,
            out_base,
            pool_utxos: LmsrPoolUtxos {
                yes: dummy_utxo(params.yes_asset_id, 50_000, 0, &old_spk),
                no: dummy_utxo(params.no_asset_id, 50_000, 1, &old_spk),
                collateral: dummy_utxo(params.collateral_asset_id, 100_000, 2, &old_spk),
            },
            trade_kind: LmsrTradeKind::BuyYes,
            old_f: old_proof.value,
            new_f: new_proof.value,
            delta_in: 1_100,
            delta_out: 10,
            admin_signature: [0u8; 64],
        };
        let satisfied = satisfy_lmsr_primary_with_env(contract, &leg, None).unwrap();
        let (program_bytes, witness_bytes) = serialize_satisfied(&satisfied);
        vec![
            witness_bytes,
            program_bytes,
            contract.primary_cmr().to_byte_array().to_vec(),
            vec![0x03],
        ]
    }

    fn explicit_txout(asset: [u8; 32], value: u64, spk: &Script) -> TxOut {
        TxOut {
            asset: Asset::Explicit(AssetId::from_slice(&asset).unwrap()),
            value: ConfValue::Explicit(value),
            nonce: Nonce::Null,
            script_pubkey: spk.clone(),
            witness: TxOutWitness::default(),
        }
    }

    #[test]
    fn decode_out_base_from_yes_input_index() {
        let contract = CompiledLmsrPool::new(params()).unwrap();
        let yes_prev = OutPoint::new(Txid::from_byte_array([0x11; 32]), 0);
        let input0 = TxIn {
            previous_output: OutPoint::new(Txid::from_byte_array([0x10; 32]), 3),
            script_sig: Script::new(),
            sequence: lwk_wollet::elements::Sequence::MAX,
            is_pegin: false,
            asset_issuance: Default::default(),
            witness: TxInWitness::default(),
        };
        let mut input1 = TxIn {
            previous_output: yes_prev,
            script_sig: Script::new(),
            sequence: lwk_wollet::elements::Sequence::MAX,
            is_pegin: false,
            asset_issuance: Default::default(),
            witness: TxInWitness::default(),
        };
        input1.witness.script_witness = vec![vec![0x01]];

        let tx = Transaction {
            version: 2,
            lock_time: lwk_wollet::elements::LockTime::ZERO,
            input: vec![input0, input1],
            output: vec![],
        };

        let out_base =
            decode_out_base_from_spend_tx(&tx, yes_prev, &contract, LMSR_WITNESS_SCHEMA_V2)
                .unwrap_err();
        assert!(
            out_base
                .to_string()
                .contains("does not carry LMSR primary witness stack")
        );
    }

    #[test]
    fn decode_out_base_from_primary_input_stack() {
        let params = params();
        let contract = CompiledLmsrPool::new(params).unwrap();
        let yes_prev = OutPoint::new(Txid::from_byte_array([0x21; 32]), 0);
        let mut input0 = TxIn {
            previous_output: yes_prev,
            script_sig: Script::new(),
            sequence: lwk_wollet::elements::Sequence::MAX,
            is_pegin: false,
            asset_issuance: Default::default(),
            witness: TxInWitness::default(),
        };
        input0.witness.script_witness = valid_primary_stack(&contract, &params, 0, 0);
        let tx = Transaction {
            version: 2,
            lock_time: lwk_wollet::elements::LockTime::ZERO,
            input: vec![input0],
            output: vec![],
        };
        let out_base =
            decode_out_base_from_spend_tx(&tx, yes_prev, &contract, LMSR_WITNESS_SCHEMA_V2)
                .unwrap();
        assert_eq!(out_base, 0);
    }

    #[test]
    fn decode_out_base_can_differ_from_primary_input_index() {
        let params = params();
        let contract = CompiledLmsrPool::new(params).unwrap();
        let yes_prev = OutPoint::new(Txid::from_byte_array([0x22; 32]), 0);
        let mut input0 = TxIn {
            previous_output: yes_prev,
            script_sig: Script::new(),
            sequence: lwk_wollet::elements::Sequence::MAX,
            is_pegin: false,
            asset_issuance: Default::default(),
            witness: TxInWitness::default(),
        };
        input0.witness.script_witness = valid_primary_stack(&contract, &params, 0, 2);
        let tx = Transaction {
            version: 2,
            lock_time: lwk_wollet::elements::LockTime::ZERO,
            input: vec![input0],
            output: vec![],
        };
        let out_base =
            decode_out_base_from_spend_tx(&tx, yes_prev, &contract, LMSR_WITNESS_SCHEMA_V2)
                .unwrap();
        assert_eq!(out_base, 2);
    }

    #[test]
    fn decode_out_base_rejects_yes_mismatch() {
        let contract = CompiledLmsrPool::new(params()).unwrap();
        let yes_prev = OutPoint::new(Txid::from_byte_array([0x31; 32]), 0);
        let mut input0 = TxIn {
            previous_output: OutPoint::new(Txid::from_byte_array([0x32; 32]), 1),
            script_sig: Script::new(),
            sequence: lwk_wollet::elements::Sequence::MAX,
            is_pegin: false,
            asset_issuance: Default::default(),
            witness: TxInWitness::default(),
        };
        input0.witness.script_witness = vec![
            vec![0x01],
            vec![0x02],
            contract.primary_cmr().to_byte_array().to_vec(),
            vec![0x03],
        ];
        let tx = Transaction {
            version: 2,
            lock_time: lwk_wollet::elements::LockTime::ZERO,
            input: vec![input0],
            output: vec![],
        };
        let err = decode_out_base_from_spend_tx(&tx, yes_prev, &contract, LMSR_WITNESS_SCHEMA_V2)
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("missing canonical YES reserve input")
        );
    }

    #[test]
    fn decode_out_base_rejects_multiple_primary_inputs() {
        let contract = CompiledLmsrPool::new(params()).unwrap();
        let yes_prev = OutPoint::new(Txid::from_byte_array([0x41; 32]), 0);
        let mut input0 = TxIn {
            previous_output: yes_prev,
            script_sig: Script::new(),
            sequence: lwk_wollet::elements::Sequence::MAX,
            is_pegin: false,
            asset_issuance: Default::default(),
            witness: TxInWitness::default(),
        };
        input0.witness.script_witness = vec![
            vec![0x01],
            vec![0x02],
            contract.primary_cmr().to_byte_array().to_vec(),
            vec![0x03],
        ];
        let mut input1 = TxIn {
            previous_output: OutPoint::new(Txid::from_byte_array([0x42; 32]), 0),
            script_sig: Script::new(),
            sequence: lwk_wollet::elements::Sequence::MAX,
            is_pegin: false,
            asset_issuance: Default::default(),
            witness: TxInWitness::default(),
        };
        input1.witness.script_witness = vec![
            vec![0x11],
            vec![0x12],
            contract.primary_cmr().to_byte_array().to_vec(),
            vec![0x13],
        ];
        let tx = Transaction {
            version: 2,
            lock_time: lwk_wollet::elements::LockTime::ZERO,
            input: vec![input0, input1],
            output: vec![],
        };
        let err = decode_out_base_from_spend_tx(&tx, yes_prev, &contract, LMSR_WITNESS_SCHEMA_V2)
            .unwrap_err();
        assert!(err.to_string().contains("multiple primary witness inputs"));
    }

    #[test]
    fn decode_out_base_supports_schema_guard() {
        let contract = CompiledLmsrPool::new(params()).unwrap();
        let yes_prev = OutPoint::new(Txid::from_byte_array([0x51; 32]), 0);
        let mut input0 = TxIn {
            previous_output: yes_prev,
            script_sig: Script::new(),
            sequence: lwk_wollet::elements::Sequence::MAX,
            is_pegin: false,
            asset_issuance: Default::default(),
            witness: TxInWitness::default(),
        };
        input0.witness.script_witness = vec![
            vec![0x01],
            vec![0x02],
            contract.primary_cmr().to_byte_array().to_vec(),
            vec![0x03],
        ];
        let tx = Transaction {
            version: 2,
            lock_time: lwk_wollet::elements::LockTime::ZERO,
            input: vec![input0],
            output: vec![],
        };
        let err =
            decode_out_base_from_spend_tx(&tx, yes_prev, &contract, "UNSUPPORTED").unwrap_err();
        assert!(err.to_string().contains("unsupported LMSR witness schema"));
    }

    #[test]
    fn extract_reserve_window_validates_assets() {
        let p = params();
        let spk = Script::from(vec![0x51]);
        let tx = Transaction {
            version: 2,
            lock_time: lwk_wollet::elements::LockTime::ZERO,
            input: vec![],
            output: vec![
                explicit_txout([0x99; 32], 1, &spk),
                explicit_txout(p.yes_asset_id, 500, &spk),
                explicit_txout(p.no_asset_id, 400, &spk),
                explicit_txout(p.collateral_asset_id, 1000, &spk),
            ],
        };

        let (utxos, reserves, out_spk) = extract_reserve_window(&tx, 1, &p).unwrap();
        assert_eq!(utxos.yes.value, 500);
        assert_eq!(utxos.no.value, 400);
        assert_eq!(utxos.collateral.value, 1000);
        assert_eq!(reserves.r_yes, 500);
        assert_eq!(reserves.r_no, 400);
        assert_eq!(reserves.r_lbtc, 1000);
        assert_eq!(out_spk, spk);
    }

    #[test]
    fn extract_reserve_window_rejects_output_window_overflow() {
        let p = params();
        let spk = Script::from(vec![0x51]);
        let tx = Transaction {
            version: 2,
            lock_time: lwk_wollet::elements::LockTime::ZERO,
            input: vec![],
            output: vec![
                explicit_txout(p.yes_asset_id, 500, &spk),
                explicit_txout(p.no_asset_id, 400, &spk),
            ],
        };

        let err = extract_reserve_window(&tx, 0, &p).unwrap_err();
        assert!(err.to_string().contains("OUT_BASE window"));
    }

    #[test]
    fn extract_reserve_window_rejects_script_mismatch() {
        let p = params();
        let spk_a = Script::from(vec![0x51]);
        let spk_b = Script::from(vec![0x52]);
        let tx = Transaction {
            version: 2,
            lock_time: lwk_wollet::elements::LockTime::ZERO,
            input: vec![],
            output: vec![
                explicit_txout(p.yes_asset_id, 500, &spk_a),
                explicit_txout(p.no_asset_id, 400, &spk_b),
                explicit_txout(p.collateral_asset_id, 1000, &spk_a),
            ],
        };

        let err = extract_reserve_window(&tx, 0, &p).unwrap_err();
        assert!(
            err.to_string()
                .contains("reserve output scripts do not match")
        );
    }
}
