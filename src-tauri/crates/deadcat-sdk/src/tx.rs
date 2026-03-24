use serde::{Deserialize, Serialize};

use crate::elements::{Transaction, Txid};
use crate::error::{Error, Result};

const SATS_PER_BTC: f64 = 100_000_000.0;
const VBYTES_PER_KVB: f64 = 1000.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MinerFeePolicy {
    ConfirmationTargetBlocks { blocks: u16 },
    RateSatPerVb { sat_per_vb: f32 },
    ExactAmountSat { amount_sat: u64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TxOptions {
    pub fee_policy: MinerFeePolicy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedMinerFee {
    pub policy: MinerFeePolicy,
    pub amount_sat: u64,
    pub rate_sat_per_vb: f32,
    pub discount_vsize: u64,
}

#[derive(Debug, Clone)]
pub struct PreparedTransaction {
    pub tx: Transaction,
    pub txid: Txid,
    pub fee: ResolvedMinerFee,
}

impl PreparedTransaction {
    pub fn new(tx: Transaction, fee: ResolvedMinerFee) -> Self {
        let txid = tx.txid();
        Self { tx, txid, fee }
    }
}

pub fn btc_per_kvb_to_sat_per_vb(rate_btc_per_kvb: f64) -> Result<f32> {
    if !rate_btc_per_kvb.is_finite() || rate_btc_per_kvb <= 0.0 {
        return Err(Error::FeeEstimation(format!(
            "invalid fee estimate {rate_btc_per_kvb}"
        )));
    }
    Ok((rate_btc_per_kvb * SATS_PER_BTC / VBYTES_PER_KVB) as f32)
}

pub fn sat_per_vb_to_sat_per_kvb(rate_sat_per_vb: f32) -> Result<f32> {
    if !rate_sat_per_vb.is_finite() || rate_sat_per_vb <= 0.0 {
        return Err(Error::FeeResolution(format!(
            "invalid sat/vB rate {rate_sat_per_vb}"
        )));
    }
    Ok(rate_sat_per_vb * 1000.0)
}

pub fn fee_amount_for_discount_vsize(rate_sat_per_vb: f32, discount_vsize: u64) -> Result<u64> {
    if !rate_sat_per_vb.is_finite() || rate_sat_per_vb <= 0.0 {
        return Err(Error::FeeResolution(format!(
            "invalid sat/vB rate {rate_sat_per_vb}"
        )));
    }
    if discount_vsize == 0 {
        return Err(Error::FeeResolution(
            "cannot derive fee for zero discount_vsize".into(),
        ));
    }
    Ok((rate_sat_per_vb as f64 * discount_vsize as f64).ceil() as u64)
}

pub fn effective_rate_sat_per_vb(amount_sat: u64, discount_vsize: u64) -> Result<f32> {
    if discount_vsize == 0 {
        return Err(Error::FeeResolution(
            "cannot derive rate for zero discount_vsize".into(),
        ));
    }
    Ok(amount_sat as f32 / discount_vsize as f32)
}

pub fn resolved_miner_fee_for_tx(
    policy: MinerFeePolicy,
    amount_sat: u64,
    tx: &Transaction,
) -> Result<ResolvedMinerFee> {
    let discount_vsize = tx.discount_vsize() as u64;
    Ok(ResolvedMinerFee {
        policy,
        amount_sat,
        rate_sat_per_vb: effective_rate_sat_per_vb(amount_sat, discount_vsize)?,
        discount_vsize,
    })
}

#[cfg(test)]
mod tests {
    use super::{MinerFeePolicy, resolved_miner_fee_for_tx};
    use crate::elements::{
        AssetId, LockTime, Transaction, TxIn, TxOut,
        confidential::{Asset, Nonce, Value},
    };

    fn sample_fee_tx() -> Transaction {
        Transaction {
            version: 2,
            lock_time: LockTime::ZERO,
            input: vec![TxIn::default()],
            output: vec![TxOut {
                asset: Asset::Explicit(AssetId::from_slice(&[1u8; 32]).expect("asset id")),
                value: Value::Explicit(240),
                nonce: Nonce::Null,
                script_pubkey: Default::default(),
                witness: Default::default(),
            }],
        }
    }

    #[test]
    fn resolved_miner_fee_uses_effective_rate_for_final_tx() {
        let tx = sample_fee_tx();
        let fee =
            resolved_miner_fee_for_tx(MinerFeePolicy::RateSatPerVb { sat_per_vb: 10.0 }, 240, &tx)
                .expect("resolved fee");

        let expected_rate = 240.0 / fee.discount_vsize as f32;
        assert!((fee.rate_sat_per_vb - expected_rate).abs() < f32::EPSILON);
        assert_ne!(fee.rate_sat_per_vb, 10.0);
    }

    #[test]
    fn resolved_miner_fee_preserves_policy_and_amount() {
        let tx = sample_fee_tx();
        let fee = resolved_miner_fee_for_tx(
            MinerFeePolicy::ConfirmationTargetBlocks { blocks: 6 },
            240,
            &tx,
        )
        .expect("resolved fee");

        assert_eq!(
            fee.policy,
            MinerFeePolicy::ConfirmationTargetBlocks { blocks: 6 }
        );
        assert_eq!(fee.amount_sat, 240);
        assert!(fee.discount_vsize > 0);
    }
}
