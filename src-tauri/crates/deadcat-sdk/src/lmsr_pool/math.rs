use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::lmsr_pool::params::LmsrPoolParams;
use crate::lmsr_pool::table::LmsrTableManifest;

/// Basis-point denominator used by fee checks.
pub const FEE_DENOM: u64 = 10_000;

/// Supported swap trade kinds for LMSR v0.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum LmsrTradeKind {
    BuyYes = 0,
    SellYes = 1,
    BuyNo = 2,
    SellNo = 3,
}

impl LmsrTradeKind {
    /// Parse a trade kind from witness integer encoding.
    pub fn from_u8(v: u8) -> Result<Self> {
        match v {
            0 => Ok(Self::BuyYes),
            1 => Ok(Self::SellYes),
            2 => Ok(Self::BuyNo),
            3 => Ok(Self::SellNo),
            _ => Err(Error::LmsrPool(format!("invalid LMSR trade kind: {v}"))),
        }
    }

    /// `true` when collateral moves into the pool.
    pub fn is_buy(self) -> bool {
        matches!(self, Self::BuyYes | Self::BuyNo)
    }

    /// `true` when `NEW_S_INDEX` must be strictly greater than `OLD_S_INDEX`.
    pub fn requires_increasing_index(self) -> bool {
        matches!(self, Self::BuyYes | Self::SellNo)
    }
}

/// Deterministic quote output derived from table points + fee policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LmsrQuote {
    pub trade_kind: LmsrTradeKind,
    pub old_s_index: u64,
    pub new_s_index: u64,
    pub traded_lots: u64,
    /// `L = x * HALF_PAYOUT_SATS`.
    pub base_l: u64,
    /// Base cost/rebate before fee adjustment.
    pub base_notional: u64,
    /// Collateral amount respecting fee inequalities.
    pub collateral_amount: u64,
    /// `true` for buy paths (pool receives collateral).
    pub collateral_is_input: bool,
    /// Signed reserve delta in lots.
    pub yes_delta: i128,
    /// Signed reserve delta in lots.
    pub no_delta: i128,
    /// Signed collateral delta in sats.
    pub collateral_delta: i128,
}

/// Quote an LMSR transition from old/new table values and fee policy.
///
/// Inputs:
/// - `old_f/new_f`: table values for `old_s_index/new_s_index`
/// - `q_step_lots`: lots per state-index step
/// - `half_payout_sats`: payout scale (`U/2`)
///
/// Output:
/// - buy paths return minimum collateral-in required
/// - sell paths return maximum collateral-out allowed
#[allow(clippy::too_many_arguments)]
pub fn quote_from_table(
    trade_kind: LmsrTradeKind,
    old_s_index: u64,
    new_s_index: u64,
    old_f: u64,
    new_f: u64,
    q_step_lots: u64,
    half_payout_sats: u64,
    fee_bps: u64,
) -> Result<LmsrQuote> {
    if q_step_lots == 0 {
        return Err(Error::LmsrPool("q_step_lots must be > 0".into()));
    }
    if half_payout_sats == 0 {
        return Err(Error::LmsrPool("half_payout_sats must be > 0".into()));
    }
    validate_direction(trade_kind, old_s_index, new_s_index)?;

    let traded_lots = compute_traded_lots(old_s_index, new_s_index, q_step_lots)?;
    let base_l = traded_lots
        .checked_mul(half_payout_sats)
        .ok_or_else(|| Error::LmsrPool("L overflow (traded_lots * half_payout_sats)".into()))?;

    let base_notional = checked_base_notional(trade_kind.is_buy(), base_l, old_f, new_f)?;
    let collateral_amount = if trade_kind.is_buy() {
        min_collateral_in(base_notional, fee_bps)?
    } else {
        max_collateral_out(base_notional, fee_bps)?
    };

    let x = i128::from(traded_lots);
    let c = i128::from(collateral_amount);
    let (yes_delta, no_delta, collateral_delta) = match trade_kind {
        LmsrTradeKind::BuyYes => (-x, 0, c),
        LmsrTradeKind::SellYes => (x, 0, -c),
        LmsrTradeKind::BuyNo => (0, -x, c),
        LmsrTradeKind::SellNo => (0, x, -c),
    };

    Ok(LmsrQuote {
        trade_kind,
        old_s_index,
        new_s_index,
        traded_lots,
        base_l,
        base_notional,
        collateral_amount,
        collateral_is_input: trade_kind.is_buy(),
        yes_delta,
        no_delta,
        collateral_delta,
    })
}

/// Find the largest valid LMSR transition that fits an `ExactInput` amount.
///
/// - Buy paths (`BuyYes`, `BuyNo`): `exact_input` is collateral sats.
/// - Sell paths (`SellYes`, `SellNo`): `exact_input` is token lots.
///
/// Returns `Ok(None)` when no valid non-zero transition fits.
pub fn quote_exact_input_from_manifest(
    manifest: &LmsrTableManifest,
    params: &LmsrPoolParams,
    trade_kind: LmsrTradeKind,
    old_s_index: u64,
    exact_input: u64,
) -> Result<Option<LmsrQuote>> {
    params
        .validate()
        .map_err(|e| Error::LmsrPool(format!("invalid LMSR params: {e}")))?;
    manifest.verify_matches_pool_params(params)?;

    if old_s_index > params.s_max_index {
        return Err(Error::LmsrPool(format!(
            "old_s_index {old_s_index} exceeds s_max_index {}",
            params.s_max_index
        )));
    }
    if exact_input == 0 {
        return Ok(None);
    }

    let old_f = manifest.value_at(old_s_index)?;
    let increasing = trade_kind.requires_increasing_index();
    let max_steps_by_index = if increasing {
        params.s_max_index - old_s_index
    } else {
        old_s_index
    };
    if max_steps_by_index == 0 {
        return Ok(None);
    }

    let max_steps = if trade_kind.is_buy() {
        max_steps_by_index
    } else {
        max_steps_by_index.min(exact_input / params.q_step_lots)
    };
    if max_steps == 0 {
        return Ok(None);
    }

    let mut best: Option<LmsrQuote> = None;
    for step in 1..=max_steps {
        let new_s_index = if increasing {
            old_s_index
                .checked_add(step)
                .ok_or_else(|| Error::LmsrPool("new_s_index overflow".into()))?
        } else {
            old_s_index
                .checked_sub(step)
                .ok_or_else(|| Error::LmsrPool("new_s_index underflow".into()))?
        };
        let new_f = manifest.value_at(new_s_index)?;
        let quote = quote_from_table(
            trade_kind,
            old_s_index,
            new_s_index,
            old_f,
            new_f,
            params.q_step_lots,
            params.half_payout_sats,
            params.fee_bps,
        )?;

        let fits = if trade_kind.is_buy() {
            quote.collateral_amount <= exact_input
        } else {
            quote.traded_lots <= exact_input
        };
        if fits {
            best = Some(quote);
        }
    }

    Ok(best)
}

/// Compute traded lots `x = abs(new-old) * q_step_lots`.
pub fn compute_traded_lots(old_s_index: u64, new_s_index: u64, q_step_lots: u64) -> Result<u64> {
    if q_step_lots == 0 {
        return Err(Error::LmsrPool("q_step_lots must be > 0".into()));
    }
    let steps = old_s_index.abs_diff(new_s_index);
    steps
        .checked_mul(q_step_lots)
        .ok_or_else(|| Error::LmsrPool("traded lots overflow".into()))
}

/// Buy-side minimum collateral input:
/// `ceil(base_cost * FEE_DENOM / (FEE_DENOM - fee_bps))`
pub fn min_collateral_in(base_cost: u64, fee_bps: u64) -> Result<u64> {
    if fee_bps >= FEE_DENOM {
        return Err(Error::LmsrPool(format!("fee_bps must be < {FEE_DENOM}")));
    }
    let fee_c = FEE_DENOM - fee_bps;
    let num = (base_cost as u128) * (FEE_DENOM as u128);
    let out = num.div_ceil(fee_c as u128);
    u64::try_from(out).map_err(|_| Error::LmsrPool("min_collateral_in overflow".into()))
}

/// Sell-side maximum collateral output:
/// `floor(base_rebate * (FEE_DENOM - fee_bps) / FEE_DENOM)`
pub fn max_collateral_out(base_rebate: u64, fee_bps: u64) -> Result<u64> {
    if fee_bps >= FEE_DENOM {
        return Err(Error::LmsrPool(format!("fee_bps must be < {FEE_DENOM}")));
    }
    let fee_c = FEE_DENOM - fee_bps;
    let out = ((base_rebate as u128) * (fee_c as u128)) / (FEE_DENOM as u128);
    u64::try_from(out).map_err(|_| Error::LmsrPool("max_collateral_out overflow".into()))
}

fn validate_direction(trade_kind: LmsrTradeKind, old_s_index: u64, new_s_index: u64) -> Result<()> {
    if trade_kind.requires_increasing_index() && new_s_index <= old_s_index {
        return Err(Error::LmsrPool(format!(
            "{trade_kind:?} requires NEW_S_INDEX > OLD_S_INDEX (old={old_s_index}, new={new_s_index})"
        )));
    }
    if !trade_kind.requires_increasing_index() && new_s_index >= old_s_index {
        return Err(Error::LmsrPool(format!(
            "{trade_kind:?} requires NEW_S_INDEX < OLD_S_INDEX (old={old_s_index}, new={new_s_index})"
        )));
    }
    Ok(())
}

fn checked_base_notional(is_buy: bool, l: u64, old_f: u64, new_f: u64) -> Result<u64> {
    if new_f >= old_f {
        let d_up = new_f - old_f;
        if is_buy {
            l.checked_add(d_up)
                .ok_or_else(|| Error::LmsrPool("buy base_cost overflow".into()))
        } else {
            l.checked_sub(d_up)
                .ok_or_else(|| Error::LmsrPool("invalid swap: L < d_up on sell path".into()))
        }
    } else {
        let d_down = old_f - new_f;
        if is_buy {
            l.checked_sub(d_down)
                .ok_or_else(|| Error::LmsrPool("invalid swap: L < d_down on buy path".into()))
        } else {
            l.checked_add(d_down)
                .ok_or_else(|| Error::LmsrPool("sell base_rebate overflow".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lmsr_pool::table::lmsr_table_root;

    #[test]
    fn parse_trade_kind() {
        assert_eq!(LmsrTradeKind::from_u8(0).unwrap(), LmsrTradeKind::BuyYes);
        assert_eq!(LmsrTradeKind::from_u8(3).unwrap(), LmsrTradeKind::SellNo);
        assert!(LmsrTradeKind::from_u8(9).is_err());
    }

    #[test]
    fn buy_yes_quote_uses_up_branch() {
        // steps=3, x=30, L=3000; Fn-Fo=50 => base_cost=3050
        let q = quote_from_table(
            LmsrTradeKind::BuyYes,
            10,
            13,
            2_000,
            2_050,
            10,
            100,
            100, // 1%
        )
        .unwrap();
        assert_eq!(q.traded_lots, 30);
        assert_eq!(q.base_l, 3_000);
        assert_eq!(q.base_notional, 3_050);
        assert_eq!(q.collateral_amount, 3_081); // ceil(3050*10000/9900)
        assert_eq!(q.yes_delta, -30);
        assert_eq!(q.no_delta, 0);
        assert_eq!(q.collateral_delta, 3_081);
    }

    #[test]
    fn sell_yes_quote_uses_up_branch() {
        // steps=3, x=30, L=3000; Fn-Fo=50 => base_rebate=2950
        let q = quote_from_table(
            LmsrTradeKind::SellYes,
            13,
            10,
            2_000,
            2_050,
            10,
            100,
            100, // 1%
        )
        .unwrap();
        assert_eq!(q.base_notional, 2_950);
        assert_eq!(q.collateral_amount, 2_920); // floor(2950*9900/10000)
        assert_eq!(q.yes_delta, 30);
        assert_eq!(q.collateral_delta, -2_920);
    }

    #[test]
    fn buy_no_quote_uses_down_branch() {
        // steps=4, x=20, L=2000; Fo-Fn=150 => base_cost=1850
        let q = quote_from_table(
            LmsrTradeKind::BuyNo,
            20,
            16,
            1_250,
            1_100,
            5,
            100,
            50, // 0.5%
        )
        .unwrap();
        assert_eq!(q.traded_lots, 20);
        assert_eq!(q.base_notional, 1_850);
        assert_eq!(q.collateral_amount, 1_860); // ceil(1850*10000/9950)
        assert_eq!(q.no_delta, -20);
    }

    #[test]
    fn rejects_direction_mismatch() {
        let err =
            quote_from_table(LmsrTradeKind::BuyYes, 10, 10, 1_000, 1_000, 1, 100, 10).unwrap_err();
        assert!(
            err.to_string()
                .contains("requires NEW_S_INDEX > OLD_S_INDEX")
        );
    }

    #[test]
    fn rejects_invalid_subtraction_branch() {
        let err = quote_from_table(
            LmsrTradeKind::BuyYes,
            0,
            1,
            2_000,
            1_000, // d_down=1000
            1,     // x=1
            10,    // L=10, so L < d_down
            10,
        )
        .unwrap_err();
        assert!(err.to_string().contains("invalid swap"));
    }

    #[test]
    fn fee_helpers_enforce_rounding_policy() {
        assert_eq!(min_collateral_in(100, 1).unwrap(), 101);
        assert_eq!(max_collateral_out(100, 1).unwrap(), 99);
    }

    #[test]
    fn compute_traded_lots_overflow() {
        let err = compute_traded_lots(u64::MAX, 0, 2).unwrap_err();
        assert!(err.to_string().contains("overflow"));
    }

    fn sample_manifest_and_params(depth: u32) -> (LmsrTableManifest, LmsrPoolParams) {
        let leaf_count = 1usize << depth;
        let values: Vec<u64> = (0..leaf_count).map(|i| 2_000 + (i as u64 * 10)).collect();
        let root = lmsr_table_root(&values).unwrap();
        let manifest = LmsrTableManifest::new(depth, values).unwrap();
        let params = LmsrPoolParams {
            yes_asset_id: [0x01; 32],
            no_asset_id: [0x02; 32],
            collateral_asset_id: [0x03; 32],
            lmsr_table_root: root,
            table_depth: depth,
            q_step_lots: 10,
            s_bias: 100,
            s_max_index: (1u64 << depth) - 1,
            half_payout_sats: 100,
            fee_bps: 0,
            min_r_yes: 1,
            min_r_no: 1,
            min_r_collateral: 1,
            cosigner_pubkey: crate::taproot::NUMS_KEY_BYTES,
        };
        (manifest, params)
    }

    #[test]
    fn exact_input_buy_yes_picks_largest_affordable_step() {
        let (manifest, params) = sample_manifest_and_params(4);
        // At old=5: F_old=2050.
        // Step1 -> new=6, F_new=2060 => 1010 collateral.
        // Step2 -> new=7, F_new=2070 => 2020 collateral.
        // With 1500 sats input, step1 is best fit.
        let quote =
            quote_exact_input_from_manifest(&manifest, &params, LmsrTradeKind::BuyYes, 5, 1_500)
                .unwrap()
                .unwrap();
        assert_eq!(quote.old_s_index, 5);
        assert_eq!(quote.new_s_index, 6);
        assert_eq!(quote.traded_lots, 10);
        assert_eq!(quote.collateral_amount, 1_010);
    }

    #[test]
    fn exact_input_sell_yes_respects_token_budget() {
        let (manifest, params) = sample_manifest_and_params(4);
        // old=10, token input budget=25 lots, q_step=10 => max 2 steps.
        let quote =
            quote_exact_input_from_manifest(&manifest, &params, LmsrTradeKind::SellYes, 10, 25)
                .unwrap()
                .unwrap();
        assert_eq!(quote.old_s_index, 10);
        assert_eq!(quote.new_s_index, 8);
        assert_eq!(quote.traded_lots, 20);
    }

    #[test]
    fn exact_input_returns_none_when_budget_too_small() {
        let (manifest, params) = sample_manifest_and_params(4);
        let quote =
            quote_exact_input_from_manifest(&manifest, &params, LmsrTradeKind::BuyNo, 5, 500)
                .unwrap();
        assert!(quote.is_none());
    }
}
