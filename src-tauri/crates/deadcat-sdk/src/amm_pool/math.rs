use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Which pair of assets is being swapped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum SwapPair {
    /// YES ↔ NO (L-BTC unchanged)
    YesNo = 0,
    /// YES ↔ L-BTC (NO unchanged)
    YesLbtc = 1,
    /// NO ↔ L-BTC (YES unchanged)
    NoLbtc = 2,
}

impl SwapPair {
    /// Parse a `SwapPair` from a u8.
    pub fn from_u8(v: u8) -> Result<Self> {
        match v {
            0 => Ok(Self::YesNo),
            1 => Ok(Self::YesLbtc),
            2 => Ok(Self::NoLbtc),
            _ => Err(Error::InvalidSwapPair(v)),
        }
    }
}

/// Current pool reserves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolReserves {
    pub r_yes: u64,
    pub r_no: u64,
    pub r_lbtc: u64,
}

/// Result of a swap calculation.
#[derive(Debug, Clone, Copy)]
pub struct SwapResult {
    /// Amount the trader puts in.
    pub delta_in: u64,
    /// Amount the trader receives.
    pub delta_out: u64,
    /// Reserves after the swap.
    pub new_reserves: PoolReserves,
    /// Price impact as a fraction (e.g. 0.05 = 5%).
    pub price_impact: f64,
}

/// FEE_DENOM is always 10000 (basis point denominator).
const FEE_DENOM: u64 = 10_000;

/// Select the two reserves involved in a swap pair and the unchanged one.
///
/// Returns `(reserve_a, reserve_b)` where A is the first-named asset and
/// B is the second-named asset in the pair.
fn select_reserves(reserves: &PoolReserves, pair: SwapPair) -> (u64, u64) {
    match pair {
        SwapPair::YesNo => (reserves.r_yes, reserves.r_no),
        SwapPair::YesLbtc => (reserves.r_yes, reserves.r_lbtc),
        SwapPair::NoLbtc => (reserves.r_no, reserves.r_lbtc),
    }
}

/// Rebuild reserves after a swap, given the pair and new values for A and B.
fn rebuild_reserves(
    original: &PoolReserves,
    pair: SwapPair,
    new_a: u64,
    new_b: u64,
) -> PoolReserves {
    match pair {
        SwapPair::YesNo => PoolReserves {
            r_yes: new_a,
            r_no: new_b,
            r_lbtc: original.r_lbtc,
        },
        SwapPair::YesLbtc => PoolReserves {
            r_yes: new_a,
            r_no: original.r_no,
            r_lbtc: new_b,
        },
        SwapPair::NoLbtc => PoolReserves {
            r_yes: original.r_yes,
            r_no: new_a,
            r_lbtc: new_b,
        },
    }
}

/// Compute a swap where the trader specifies the exact input amount.
///
/// Within the pair `(A, B)`, A is the first-named asset and B is the second.
///
/// - `sell_a = false` (default): trader deposits B, receives A.
///   For `YesLbtc`: deposit L-BTC, receive YES.
/// - `sell_a = true`: trader deposits A, receives B.
///   For `YesLbtc`: deposit YES, receive L-BTC.
///
/// Uses the formula from design doc §19.2:
///   `effective_in = delta_in * (FEE_DENOM - fee_bps) / FEE_DENOM`
///   `delta_out = floor(r_buy * effective_in / (r_sell + effective_in))`
pub fn compute_swap_exact_input(
    reserves: &PoolReserves,
    pair: SwapPair,
    delta_in: u64,
    fee_bps: u64,
    sell_a: bool,
) -> Result<SwapResult> {
    let (r_a, r_b) = select_reserves(reserves, pair);
    // r_sell = reserve the trader deposits into, r_buy = reserve trader withdraws from
    let (r_sell, r_buy) = if sell_a { (r_a, r_b) } else { (r_b, r_a) };

    if r_sell == 0 || r_buy == 0 {
        return Err(Error::ReserveDepleted);
    }
    if delta_in == 0 {
        return Err(Error::AmmPool("delta_in must be non-zero".into()));
    }

    let fee_complement = FEE_DENOM
        .checked_sub(fee_bps)
        .ok_or_else(|| Error::AmmPool("fee_bps exceeds FEE_DENOM".into()))?;

    // effective_in = delta_in * fee_complement / FEE_DENOM
    // Use u128 to avoid overflow
    let effective_in_num = (delta_in as u128) * (fee_complement as u128);
    let effective_in = effective_in_num / (FEE_DENOM as u128);

    // delta_out = floor(r_buy * effective_in / (r_sell + effective_in))
    let numerator = (r_buy as u128) * effective_in;
    let denominator = (r_sell as u128) + effective_in;

    if denominator == 0 {
        return Err(Error::ReserveDepleted);
    }

    let delta_out = (numerator / denominator) as u64;
    if delta_out == 0 {
        return Err(Error::AmmPool("swap output is zero".into()));
    }
    if delta_out >= r_buy {
        return Err(Error::InsufficientReserves);
    }

    let new_r_sell = r_sell + delta_in;
    let new_r_buy = r_buy - delta_out;
    let (new_a, new_b) = if sell_a {
        (new_r_sell, new_r_buy)
    } else {
        (new_r_buy, new_r_sell)
    };
    let new_reserves = rebuild_reserves(reserves, pair, new_a, new_b);

    // Spot price: r_sell / r_buy. Execution price: delta_in / delta_out.
    let spot_price = (r_sell as f64) / (r_buy as f64);
    let exec_price = (delta_in as f64) / (delta_out as f64);
    let price_impact = if spot_price > 0.0 {
        (exec_price - spot_price) / spot_price
    } else {
        0.0
    };

    Ok(SwapResult {
        delta_in,
        delta_out,
        new_reserves,
        price_impact,
    })
}

/// Compute a swap where the trader specifies the exact output amount.
///
/// - `sell_a = false` (default): trader pays B, receives `delta_out` of A.
/// - `sell_a = true`: trader pays A, receives `delta_out` of B.
///
/// Uses the formula from design doc §19.1:
///   `delta_in = ceil(r_sell * delta_out * FEE_DENOM / ((r_buy - delta_out) * (FEE_DENOM - fee_bps)))`
pub fn compute_swap_exact_output(
    reserves: &PoolReserves,
    pair: SwapPair,
    delta_out: u64,
    fee_bps: u64,
    sell_a: bool,
) -> Result<SwapResult> {
    let (r_a, r_b) = select_reserves(reserves, pair);
    let (r_sell, r_buy) = if sell_a { (r_a, r_b) } else { (r_b, r_a) };

    if r_sell == 0 || r_buy == 0 {
        return Err(Error::ReserveDepleted);
    }
    if delta_out == 0 {
        return Err(Error::AmmPool("delta_out must be non-zero".into()));
    }
    if delta_out >= r_buy {
        return Err(Error::InsufficientReserves);
    }

    let fee_complement = FEE_DENOM
        .checked_sub(fee_bps)
        .ok_or_else(|| Error::AmmPool("fee_bps exceeds FEE_DENOM".into()))?;

    // delta_in = ceil(r_sell * delta_out * FEE_DENOM / ((r_buy - delta_out) * fee_complement))
    let numerator = (r_sell as u128) * (delta_out as u128) * (FEE_DENOM as u128);
    let denominator = ((r_buy - delta_out) as u128) * (fee_complement as u128);

    if denominator == 0 {
        return Err(Error::ReserveDepleted);
    }

    // Ceiling division
    let delta_in = ((numerator + denominator - 1) / denominator) as u64;

    let new_r_sell = r_sell + delta_in;
    let new_r_buy = r_buy - delta_out;
    let (new_a, new_b) = if sell_a {
        (new_r_sell, new_r_buy)
    } else {
        (new_r_buy, new_r_sell)
    };
    let new_reserves = rebuild_reserves(reserves, pair, new_a, new_b);

    let spot_price = (r_sell as f64) / (r_buy as f64);
    let exec_price = (delta_in as f64) / (delta_out as f64);
    let price_impact = if spot_price > 0.0 {
        (exec_price - spot_price) / spot_price
    } else {
        0.0
    };

    Ok(SwapResult {
        delta_in,
        delta_out,
        new_reserves,
        price_impact,
    })
}

/// Compute the maximum LP tokens mintable for a given deposit.
///
/// Given current reserves and `issued_lp`, and the new reserves after deposit,
/// returns the maximum `lp_mint` such that the cubic invariant holds:
///   `(issued_lp + lp_mint)^3 * old_product <= issued_lp^3 * new_product`
///
/// Uses floating-point cube root for the upper bound, then verifies with
/// integer arithmetic.
pub fn compute_lp_deposit(
    reserves: &PoolReserves,
    issued_lp: u64,
    new_reserves: &PoolReserves,
) -> Result<u64> {
    if issued_lp == 0 {
        return Err(Error::ZeroIssuedLp);
    }

    let old_product =
        (reserves.r_yes as f64) * (reserves.r_no as f64) * (reserves.r_lbtc as f64);
    let new_product = (new_reserves.r_yes as f64)
        * (new_reserves.r_no as f64)
        * (new_reserves.r_lbtc as f64);

    if old_product == 0.0 {
        return Err(Error::ReserveDepleted);
    }

    // new_issued_lp <= issued_lp * (new_product / old_product)^(1/3)
    // Use f64 for initial estimate, then verify with integer arithmetic.
    let ratio = new_product / old_product;
    let max_new_issued = (issued_lp as f64) * ratio.cbrt();
    let candidate = (max_new_issued.floor() as u64).saturating_sub(issued_lp);

    // Integer verification: check that (issued_lp + candidate)^3 * old_product
    // <= issued_lp^3 * new_product using u128 arithmetic.
    // If the candidate is too large (f64 rounding up), decrement.
    let lp_mint = verify_cubic_invariant(
        issued_lp,
        candidate,
        reserves,
        new_reserves,
    );

    Ok(lp_mint)
}

/// Wide 256-bit unsigned integer as (hi, lo) pair of u128.
/// Only implements the operations needed for cubic invariant checking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct U256 {
    hi: u128,
    lo: u128,
}

impl U256 {
    /// Multiply two u128 values, producing a full U256 result.
    fn widening_mul(a: u128, b: u128) -> Self {
        // Split each u128 into two u64 halves and do schoolbook multiplication.
        let a_lo = a as u64 as u128;
        let a_hi = a >> 64;
        let b_lo = b as u64 as u128;
        let b_hi = b >> 64;

        let ll = a_lo * b_lo;
        let lh = a_lo * b_hi;
        let hl = a_hi * b_lo;
        let hh = a_hi * b_hi;

        // Accumulate: result = hh << 128 + (lh + hl) << 64 + ll
        let mid = lh.wrapping_add(hl);
        let mid_overflow = if mid < lh { 1u128 } else { 0u128 };

        let lo = ll.wrapping_add(mid << 64);
        let carry = if lo < ll { 1u128 } else { 0u128 };
        let hi = hh + (mid >> 64) + (mid_overflow << 64) + carry;

        U256 { hi, lo }
    }

    /// Multiply a U256 by a u128, producing a U256 (truncated to 256 bits).
    /// This is sufficient when we know the mathematical result fits in 256 bits.
    fn mul_u128(self, rhs: u128) -> Self {
        // self.lo * rhs -> full U256
        let lo_prod = Self::widening_mul(self.lo, rhs);
        // self.hi * rhs -> only the low u128 matters (hi part would be >256 bits)
        let hi_prod_lo = self.hi.wrapping_mul(rhs);

        let hi = lo_prod.hi.wrapping_add(hi_prod_lo);
        U256 { hi, lo: lo_prod.lo }
    }

}

/// Check the cubic invariant: `new_lp^3 * old_product <= old_lp^3 * new_product`
/// using wide arithmetic to avoid overflow.
///
/// The product of three u64 reserves can be up to 192 bits (overflows u128),
/// so we compute `lp^3 * r_a * r_b * r_c` as a chain of wide multiplications
/// producing a 576-bit result (more than needed, but simple to implement).
///
/// We actually need: `new_lp^3 * old_rA * old_rB * old_rC <= old_lp^3 * new_rA * new_rB * new_rC`
/// Each side is at most 192 + 192 = 384 bits.
fn cubic_invariant_holds(
    old_lp: u64,
    new_lp: u64,
    old: &PoolReserves,
    new: &PoolReserves,
) -> bool {
    // Compute lp^3 as U256 (fits in 192 bits)
    let new_lp3 = cube_u64(new_lp);
    let old_lp3 = cube_u64(old_lp);

    // LHS = new_lp^3 * old_r_yes * old_r_no * old_r_lbtc
    // RHS = old_lp^3 * new_r_yes * new_r_no * new_r_lbtc
    // Each side: U256(≤192 bits) * u64 * u64 * u64 = up to 384 bits.
    // We use mul_u256_u128 producing (u128, U256) = 384 bits.
    // For the three u64 multiplications, we chain: U256 * u64 -> U256 (≤256),
    // then (u128, U256) for the final step.

    // LHS: new_lp3 * old.r_yes -> U256 (≤256 bits)
    let lhs_step1 = new_lp3.mul_u128(old.r_yes as u128);
    // lhs_step1 * old.r_no -> (u128, U256) = 384 bits
    let lhs_step2 = mul_u256_u128(lhs_step1, old.r_no as u128);
    // lhs_step2 * old.r_lbtc -> need to extend to ~448+ bits
    // Use mul_384_u128 for this final multiplication.
    let lhs = mul_384_u64(lhs_step2, old.r_lbtc);

    // RHS: old_lp3 * new.r_yes -> U256 (≤256 bits)
    let rhs_step1 = old_lp3.mul_u128(new.r_yes as u128);
    let rhs_step2 = mul_u256_u128(rhs_step1, new.r_no as u128);
    let rhs = mul_384_u64(rhs_step2, new.r_lbtc);

    le_512(lhs, rhs)
}

/// Compute n^3 as a U256 (result fits in 192 bits for u64 input).
fn cube_u64(n: u64) -> U256 {
    let v = n as u128;
    let sq = U256::widening_mul(v, v);
    sq.mul_u128(v)
}

/// Multiply a U256 by a u128, producing a 384-bit result as (u128_hi, U256_lo).
fn mul_u256_u128(a: U256, b: u128) -> (u128, U256) {
    // a.lo * b -> U256
    let lo_prod = U256::widening_mul(a.lo, b);
    // a.hi * b -> U256
    let hi_prod = U256::widening_mul(a.hi, b);

    // Result = hi_prod << 128 + lo_prod
    // lo part = lo_prod.lo
    // mid part = lo_prod.hi + hi_prod.lo (with carry)
    // hi part = hi_prod.hi + carry

    let mid = lo_prod.hi.wrapping_add(hi_prod.lo);
    let carry = if mid < lo_prod.hi { 1u128 } else { 0u128 };
    let top = hi_prod.hi.wrapping_add(carry);

    (top, U256 { hi: mid, lo: lo_prod.lo })
}

/// Multiply a 384-bit value (u128, U256) by a u64, producing a 512-bit value
/// stored as (U256_hi, U256_lo). Result is at most 384+64 = 448 bits, but we
/// store in 512 for simplicity.
fn mul_384_u64(a: (u128, U256), b: u64) -> (U256, U256) {
    let b128 = b as u128;

    // a = (a_top, a_mid, a_lo) where a = (a.0, a.1.hi, a.1.lo)
    // Multiply each part by b and accumulate with carries.
    let lo_prod = U256::widening_mul(a.1.lo, b128);      // 256 bits
    let mid_prod = U256::widening_mul(a.1.hi, b128);      // 256 bits
    let top_prod = U256::widening_mul(a.0, b128);          // 256 bits

    // Accumulate: result_lo = lo_prod.lo
    // result_mid = lo_prod.hi + mid_prod.lo (with carry)
    // result_hi = mid_prod.hi + top_prod.lo + carry
    // result_top = top_prod.hi + carry

    let result_lo_lo = lo_prod.lo;

    let (result_lo_hi, carry1) = carrying_add(lo_prod.hi, mid_prod.lo);
    let (result_hi_lo, carry2) = carrying_add(mid_prod.hi + carry1, top_prod.lo);
    let result_hi_hi = top_prod.hi + carry2;

    (
        U256 { hi: result_hi_hi, lo: result_hi_lo },
        U256 { hi: result_lo_hi, lo: result_lo_lo },
    )
}

/// Add two u128 values, returning (sum, carry) where carry is 0 or 1.
fn carrying_add(a: u128, b: u128) -> (u128, u128) {
    let sum = a.wrapping_add(b);
    let carry = if sum < a { 1u128 } else { 0u128 };
    (sum, carry)
}

/// Compare two 512-bit values stored as (U256_hi, U256_lo).
fn le_512(a: (U256, U256), b: (U256, U256)) -> bool {
    if a.0.hi != b.0.hi { return a.0.hi < b.0.hi; }
    if a.0.lo != b.0.lo { return a.0.lo < b.0.lo; }
    if a.1.hi != b.1.hi { return a.1.hi < b.1.hi; }
    a.1.lo <= b.1.lo
}

/// Verify and adjust the LP mint amount so the cubic invariant holds.
///
/// Invariant: `new_lp^3 * old_product <= old_lp^3 * new_product`
/// Uses 384-bit wide arithmetic to avoid overflow.
///
/// Starting from the f64-estimated `candidate`, searches in both directions
/// to find the maximum valid mint amount.
///
/// The f64 estimate has at most ~1 ULP of error, so the search terminates in
/// a small number of iterations. A safety bound of 100 iterations is imposed
/// to prevent unbounded loops on pathological inputs.
fn verify_cubic_invariant(
    issued_lp: u64,
    candidate: u64,
    old: &PoolReserves,
    new: &PoolReserves,
) -> u64 {
    const MAX_ITERATIONS: u32 = 100;

    // First try the candidate as-is
    if cubic_invariant_holds(issued_lp, issued_lp + candidate, old, new) {
        // f64 estimate was valid or conservative — try incrementing to find the true max
        let mut mint = candidate;
        for _ in 0..MAX_ITERATIONS {
            let next = mint + 1;
            if !cubic_invariant_holds(issued_lp, issued_lp + next, old, new) {
                return mint;
            }
            mint = next;
        }
        mint
    } else {
        // f64 estimate was too high — decrement until valid
        let mut mint = candidate;
        for _ in 0..MAX_ITERATIONS {
            if mint == 0 {
                return 0;
            }
            mint -= 1;
            if cubic_invariant_holds(issued_lp, issued_lp + mint, old, new) {
                return mint;
            }
        }
        0
    }
}

/// Compute a proportional withdrawal: burn `lp_burn` LP tokens and receive
/// a proportional share of all three reserves.
///
/// Returns the amounts withdrawn from each reserve.
pub fn compute_lp_proportional_withdraw(
    reserves: &PoolReserves,
    issued_lp: u64,
    lp_burn: u64,
) -> Result<PoolReserves> {
    if issued_lp == 0 {
        return Err(Error::ZeroIssuedLp);
    }
    if lp_burn >= issued_lp {
        return Err(Error::AmmPool(
            "cannot burn all LP tokens (minimum 1 must remain)".into(),
        ));
    }

    // Proportional share: withdrawn_X = floor(r_X * lp_burn / issued_lp)
    let withdraw_yes =
        ((reserves.r_yes as u128) * (lp_burn as u128) / (issued_lp as u128)) as u64;
    let withdraw_no =
        ((reserves.r_no as u128) * (lp_burn as u128) / (issued_lp as u128)) as u64;
    let withdraw_lbtc =
        ((reserves.r_lbtc as u128) * (lp_burn as u128) / (issued_lp as u128)) as u64;

    Ok(PoolReserves {
        r_yes: withdraw_yes,
        r_no: withdraw_no,
        r_lbtc: withdraw_lbtc,
    })
}

/// Spot price of YES tokens in L-BTC terms.
pub fn spot_price_yes_lbtc(reserves: &PoolReserves) -> f64 {
    if reserves.r_yes == 0 {
        return 0.0;
    }
    (reserves.r_lbtc as f64) / (reserves.r_yes as f64)
}

/// Spot price of NO tokens in L-BTC terms.
pub fn spot_price_no_lbtc(reserves: &PoolReserves) -> f64 {
    if reserves.r_no == 0 {
        return 0.0;
    }
    (reserves.r_lbtc as f64) / (reserves.r_no as f64)
}

/// Spot price of YES tokens in NO terms.
pub fn spot_price_yes_no(reserves: &PoolReserves) -> f64 {
    if reserves.r_yes == 0 {
        return 0.0;
    }
    (reserves.r_no as f64) / (reserves.r_yes as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_reserves() -> PoolReserves {
        PoolReserves {
            r_yes: 1_000_000,
            r_no: 1_000_000,
            r_lbtc: 500_000,
        }
    }

    #[test]
    fn swap_pair_from_u8() {
        assert_eq!(SwapPair::from_u8(0).unwrap(), SwapPair::YesNo);
        assert_eq!(SwapPair::from_u8(1).unwrap(), SwapPair::YesLbtc);
        assert_eq!(SwapPair::from_u8(2).unwrap(), SwapPair::NoLbtc);
        assert!(SwapPair::from_u8(3).is_err());
    }

    #[test]
    fn exact_input_swap_yes_lbtc() {
        // Design doc §7.1 example:
        // R_yes = 1,000,000, R_lbtc = 500,000, fee = 30 bps
        // Trader deposits 10,000 L-BTC, receives YES tokens
        let reserves = default_reserves();
        let result =
            compute_swap_exact_input(&reserves, SwapPair::YesLbtc, 10_000, 30, false).unwrap();

        // effective_input = 10,000 * 9970 / 10000 = 9,970
        // delta_out = floor(1,000,000 * 9970 / (500,000 + 9970)) = floor(9,970,000,000 / 509,970) ≈ 19,550
        assert!(result.delta_out > 19_000 && result.delta_out < 20_000);
        assert_eq!(result.delta_in, 10_000);
        assert_eq!(result.new_reserves.r_lbtc, 510_000);
        assert!(result.new_reserves.r_yes < 1_000_000);
        // NO unchanged
        assert_eq!(result.new_reserves.r_no, 1_000_000);
        assert!(result.price_impact > 0.0);
    }

    #[test]
    fn exact_output_swap_yes_lbtc() {
        let reserves = default_reserves();
        // Trader wants exactly 19,000 YES tokens, paying L-BTC
        let result =
            compute_swap_exact_output(&reserves, SwapPair::YesLbtc, 19_000, 30, false).unwrap();

        assert_eq!(result.delta_out, 19_000);
        assert!(result.delta_in > 9_000 && result.delta_in < 11_000);
        assert_eq!(result.new_reserves.r_yes, 1_000_000 - 19_000);
        assert_eq!(result.new_reserves.r_no, 1_000_000);
    }

    #[test]
    fn swap_depleted_reserve_fails() {
        let reserves = PoolReserves {
            r_yes: 0,
            r_no: 1000,
            r_lbtc: 1000,
        };
        assert!(compute_swap_exact_input(&reserves, SwapPair::YesLbtc, 100, 30, false).is_err());
    }

    #[test]
    fn swap_output_exceeds_reserve_fails() {
        let reserves = default_reserves();
        assert!(
            compute_swap_exact_output(&reserves, SwapPair::YesLbtc, 1_000_001, 30, false).is_err()
        );
    }

    #[test]
    fn lp_deposit_single_sided() {
        // Design doc §13.6:
        // R_yes = 10,000, R_no = 10,000, R_lbtc = 10,000,000, issued_lp = 1,000
        // Deposit 1,000,000 L-BTC single-sided → max ~32 LP tokens
        let reserves = PoolReserves {
            r_yes: 10_000,
            r_no: 10_000,
            r_lbtc: 10_000_000,
        };
        let new_reserves = PoolReserves {
            r_yes: 10_000,
            r_no: 10_000,
            r_lbtc: 11_000_000,
        };
        let lp_mint = compute_lp_deposit(&reserves, 1_000, &new_reserves).unwrap();
        assert!(lp_mint <= 32, "lp_mint was {lp_mint}, expected <= 32");
        assert!(lp_mint >= 30, "lp_mint was {lp_mint}, expected >= 30");
    }

    #[test]
    fn proportional_withdraw() {
        let reserves = PoolReserves {
            r_yes: 10_000,
            r_no: 10_000,
            r_lbtc: 10_000_000,
        };
        let withdrawn = compute_lp_proportional_withdraw(&reserves, 1_000, 100).unwrap();
        // 10% of each reserve
        assert_eq!(withdrawn.r_yes, 1_000);
        assert_eq!(withdrawn.r_no, 1_000);
        assert_eq!(withdrawn.r_lbtc, 1_000_000);
    }

    #[test]
    fn withdraw_all_fails() {
        let reserves = PoolReserves {
            r_yes: 10_000,
            r_no: 10_000,
            r_lbtc: 10_000_000,
        };
        assert!(compute_lp_proportional_withdraw(&reserves, 1_000, 1_000).is_err());
    }

    #[test]
    fn spot_prices_50_50() {
        let reserves = PoolReserves {
            r_yes: 10_000,
            r_no: 10_000,
            r_lbtc: 5_000_000,
        };
        let yes_price = spot_price_yes_lbtc(&reserves);
        let no_price = spot_price_no_lbtc(&reserves);
        let yes_no = spot_price_yes_no(&reserves);

        assert!((yes_price - 500.0).abs() < 0.01);
        assert!((no_price - 500.0).abs() < 0.01);
        assert!((yes_no - 1.0).abs() < 0.01);
    }

    #[test]
    fn spot_prices_70_30() {
        let reserves = PoolReserves {
            r_yes: 5_000,
            r_no: 15_000,
            r_lbtc: 5_000_000,
        };
        let yes_price = spot_price_yes_lbtc(&reserves);
        let no_price = spot_price_no_lbtc(&reserves);

        // YES is scarcer → more expensive
        assert!(yes_price > no_price);
        assert!((yes_price - 1000.0).abs() < 0.01);
        assert!((no_price - 333.333).abs() < 0.5);
    }

    #[test]
    fn u256_widening_mul_basic() {
        // Small values should match regular multiplication.
        let r = U256::widening_mul(100, 200);
        assert_eq!(r.hi, 0);
        assert_eq!(r.lo, 20_000);
    }

    #[test]
    fn u256_widening_mul_overflow() {
        // u128::MAX * 2 should produce a valid U256
        let r = U256::widening_mul(u128::MAX, 2);
        assert_eq!(r.hi, 1);
        assert_eq!(r.lo, u128::MAX - 1);
    }

    #[test]
    fn cubic_invariant_holds_trivial() {
        // Proportional deposit: double all reserves, LP should double.
        let old = PoolReserves { r_yes: 1000, r_no: 1000, r_lbtc: 1000 };
        let new = PoolReserves { r_yes: 2000, r_no: 2000, r_lbtc: 2000 };
        let mint = compute_lp_deposit(&old, 100, &new).unwrap();
        // 100 * (8/1)^(1/3) = 100 * 2 = 200, so mint should be exactly 100
        assert_eq!(mint, 100);
    }

    #[test]
    fn cubic_invariant_large_reserves_no_overflow() {
        // Large reserves that would overflow u128 in the old implementation.
        // Each reserve ~2^50, product ~2^150, lp ~2^40, lp^3 ~2^120.
        // lp^3 * product ~2^270 — way beyond u128.
        let old = PoolReserves {
            r_yes: 1 << 50,
            r_no: 1 << 50,
            r_lbtc: 1 << 50,
        };
        // 10% deposit on each side
        let new = PoolReserves {
            r_yes: old.r_yes + old.r_yes / 10,
            r_no: old.r_no + old.r_no / 10,
            r_lbtc: old.r_lbtc + old.r_lbtc / 10,
        };
        let issued_lp = 1u64 << 40;
        let mint = compute_lp_deposit(&old, issued_lp, &new).unwrap();
        // Proportional 10% increase → ~10% more LP tokens
        let expected_approx = issued_lp / 10;
        let tolerance = expected_approx / 100; // 1% tolerance
        assert!(
            (mint as i128 - expected_approx as i128).unsigned_abs() <= tolerance as u128,
            "mint = {mint}, expected ~{expected_approx} ± {tolerance}"
        );
    }

    #[test]
    fn cubic_invariant_maximality() {
        // Verify that the returned mint is the maximum valid value:
        // the invariant holds for mint, but not for mint+1.
        let old = PoolReserves { r_yes: 10_000, r_no: 10_000, r_lbtc: 10_000 };
        let new = PoolReserves { r_yes: 11_000, r_no: 10_000, r_lbtc: 10_000 };
        let mint = compute_lp_deposit(&old, 1_000, &new).unwrap();
        assert!(mint > 0, "10% single-sided deposit should mint LP tokens");
        assert!(cubic_invariant_holds(1_000, 1_000 + mint, &old, &new));
        assert!(!cubic_invariant_holds(1_000, 1_000 + mint + 1, &old, &new));
    }

    #[test]
    fn cubic_invariant_tiny_deposit_zero_mint() {
        // A deposit too small to justify minting 1 LP token should return 0.
        let old = PoolReserves { r_yes: 10_000, r_no: 10_000, r_lbtc: 10_000 };
        let new = PoolReserves { r_yes: 10_001, r_no: 10_000, r_lbtc: 10_000 };
        let mint = compute_lp_deposit(&old, 30_000, &new).unwrap();
        assert_eq!(mint, 0, "deposit too small to mint even 1 LP token");
    }

    // ── sell_a (reverse direction) tests ────────────────────────────────

    #[test]
    fn exact_input_swap_sell_a_yes_lbtc() {
        // sell_a=true with YesLbtc: trader sells YES, receives L-BTC
        let reserves = default_reserves();
        let result = compute_swap_exact_input(
            &reserves,
            SwapPair::YesLbtc,
            10_000,
            30,
            true, // sell YES
        )
        .unwrap();

        // YES reserve increases, LBTC reserve decreases
        assert!(result.new_reserves.r_yes > reserves.r_yes);
        assert!(result.new_reserves.r_lbtc < reserves.r_lbtc);
        // NO unchanged
        assert_eq!(result.new_reserves.r_no, reserves.r_no);
        assert!(result.delta_out > 0);
    }

    #[test]
    fn exact_input_swap_sell_a_yes_no() {
        // sell_a=true with YesNo: trader sells YES, receives NO
        let reserves = default_reserves();
        let result = compute_swap_exact_input(
            &reserves,
            SwapPair::YesNo,
            5_000,
            30,
            true, // sell YES
        )
        .unwrap();

        assert!(result.new_reserves.r_yes > reserves.r_yes);
        assert!(result.new_reserves.r_no < reserves.r_no);
        assert_eq!(result.new_reserves.r_lbtc, reserves.r_lbtc);
    }

    #[test]
    fn exact_output_swap_sell_a() {
        // sell_a=true with NoLbtc: trader sells NO, receives exactly 1000 L-BTC
        let reserves = default_reserves();
        let result = compute_swap_exact_output(
            &reserves,
            SwapPair::NoLbtc,
            1_000,
            30,
            true, // sell NO
        )
        .unwrap();

        assert_eq!(result.delta_out, 1_000);
        assert!(result.new_reserves.r_no > reserves.r_no);
        assert_eq!(result.new_reserves.r_lbtc, reserves.r_lbtc - 1_000);
        assert_eq!(result.new_reserves.r_yes, reserves.r_yes);
    }

    #[test]
    fn sell_a_false_matches_original_behavior() {
        // Verify sell_a=false produces the same results as the old API
        let reserves = default_reserves();
        let result = compute_swap_exact_input(
            &reserves,
            SwapPair::YesLbtc,
            10_000,
            30,
            false,
        )
        .unwrap();

        // Original convention: deposit B (LBTC), receive A (YES)
        assert_eq!(result.new_reserves.r_lbtc, 510_000);
        assert!(result.new_reserves.r_yes < 1_000_000);
        assert_eq!(result.new_reserves.r_no, 1_000_000);
    }

    #[test]
    fn sell_a_both_directions_consistent() {
        // If trader sells 10k YES for LBTC (sell_a=true), then sells the LBTC
        // back for YES (sell_a=false), they should end up with less than 10k YES
        // due to fees.
        let reserves = default_reserves();
        let step1 = compute_swap_exact_input(
            &reserves,
            SwapPair::YesLbtc,
            10_000,
            30,
            true, // sell YES, get LBTC
        )
        .unwrap();

        let step2 = compute_swap_exact_input(
            &step1.new_reserves,
            SwapPair::YesLbtc,
            step1.delta_out,
            30,
            false, // sell LBTC, get YES
        )
        .unwrap();

        // Roundtrip should lose value to fees
        assert!(step2.delta_out < 10_000, "roundtrip should lose to fees");
    }
}
