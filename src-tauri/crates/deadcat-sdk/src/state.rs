use serde::{Deserialize, Serialize};

use crate::params::ContractParams;

/// Four-state model for the binary prediction market covenant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u64)]
pub enum MarketState {
    /// Reissuance tokens deposited, no collateral. Awaiting initial issuance.
    Dormant = 0,
    /// At least one issuance has occurred. Market is live.
    Unresolved = 1,
    /// Oracle committed YES outcome. YES tokens are redeemable.
    ResolvedYes = 2,
    /// Oracle committed NO outcome. NO tokens are redeemable.
    ResolvedNo = 3,
}

impl MarketState {
    pub fn from_u64(v: u64) -> Option<Self> {
        match v {
            0 => Some(Self::Dormant),
            1 => Some(Self::Unresolved),
            2 => Some(Self::ResolvedYes),
            3 => Some(Self::ResolvedNo),
            _ => None,
        }
    }

    pub fn as_u64(self) -> u64 {
        self as u64
    }

    /// Returns the winning token asset ID for a resolved state.
    /// YES asset if state == ResolvedYes, NO asset if state == ResolvedNo, None otherwise.
    pub fn winning_token_asset(self, params: &ContractParams) -> Option<[u8; 32]> {
        match self {
            Self::ResolvedYes => Some(params.yes_token_asset),
            Self::ResolvedNo => Some(params.no_token_asset),
            _ => None,
        }
    }

    /// Returns true if this is a resolved state (YES or NO).
    pub fn is_resolved(self) -> bool {
        matches!(self, Self::ResolvedYes | Self::ResolvedNo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        for v in 0..=3 {
            let state = MarketState::from_u64(v).unwrap();
            assert_eq!(state.as_u64(), v);
        }
        assert!(MarketState::from_u64(4).is_none());
    }

    #[test]
    fn winning_token() {
        let params = ContractParams {
            oracle_public_key: [0; 32],
            collateral_asset_id: [0; 32],
            yes_token_asset: [0x01; 32],
            no_token_asset: [0x02; 32],
            yes_reissuance_token: [0; 32],
            no_reissuance_token: [0; 32],
            collateral_per_token: 100_000,
            expiry_time: 1_000_000,
        };
        assert_eq!(
            MarketState::ResolvedYes.winning_token_asset(&params),
            Some([0x01; 32])
        );
        assert_eq!(
            MarketState::ResolvedNo.winning_token_asset(&params),
            Some([0x02; 32])
        );
        assert_eq!(MarketState::Unresolved.winning_token_asset(&params), None);
        assert_eq!(MarketState::Dormant.winning_token_asset(&params), None);
    }
}
