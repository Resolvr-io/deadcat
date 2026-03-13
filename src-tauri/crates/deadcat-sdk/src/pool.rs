use serde::{Deserialize, Serialize};

/// Generic reserve bundle used by LMSR discovery, routing, and price snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolReserves {
    pub r_yes: u64,
    pub r_no: u64,
    pub r_lbtc: u64,
}

/// Compute implied YES/NO probabilities from YES/NO reserve weights.
pub fn implied_probability_bps(reserves: &PoolReserves) -> Option<(u16, u16)> {
    let denom = reserves.r_yes.checked_add(reserves.r_no)?;
    if denom == 0 {
        return None;
    }
    let yes = ((u128::from(reserves.r_yes) * 10_000) / u128::from(denom)) as u16;
    let no = 10_000u16.saturating_sub(yes);
    Some((yes, no))
}
