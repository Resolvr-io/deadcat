use serde::{Deserialize, Serialize};

use crate::prediction_market::params::PredictionMarketParams;

/// Five-state lifecycle model for a binary prediction market.
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
    /// Oracle was not used; market was explicitly finalized as expired.
    Expired = 4,
}

/// Concrete covenant identities. A slot commits both lifecycle state and covenant role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum MarketSlot {
    DormantYesRt = 0,
    DormantNoRt = 1,
    UnresolvedYesRt = 2,
    UnresolvedNoRt = 3,
    UnresolvedCollateral = 4,
    ResolvedYesCollateral = 5,
    ResolvedNoCollateral = 6,
    ExpiredCollateral = 7,
}

impl MarketSlot {
    pub const ALL: [Self; 8] = [
        Self::DormantYesRt,
        Self::DormantNoRt,
        Self::UnresolvedYesRt,
        Self::UnresolvedNoRt,
        Self::UnresolvedCollateral,
        Self::ResolvedYesCollateral,
        Self::ResolvedNoCollateral,
        Self::ExpiredCollateral,
    ];

    pub const DORMANT: [Self; 2] = [Self::DormantYesRt, Self::DormantNoRt];
    pub const UNRESOLVED: [Self; 3] = [
        Self::UnresolvedYesRt,
        Self::UnresolvedNoRt,
        Self::UnresolvedCollateral,
    ];

    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::DormantYesRt),
            1 => Some(Self::DormantNoRt),
            2 => Some(Self::UnresolvedYesRt),
            3 => Some(Self::UnresolvedNoRt),
            4 => Some(Self::UnresolvedCollateral),
            5 => Some(Self::ResolvedYesCollateral),
            6 => Some(Self::ResolvedNoCollateral),
            7 => Some(Self::ExpiredCollateral),
            _ => None,
        }
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn as_u64(self) -> u64 {
        self as u64
    }

    pub fn state(self) -> MarketState {
        match self {
            Self::DormantYesRt | Self::DormantNoRt => MarketState::Dormant,
            Self::UnresolvedYesRt | Self::UnresolvedNoRt | Self::UnresolvedCollateral => {
                MarketState::Unresolved
            }
            Self::ResolvedYesCollateral => MarketState::ResolvedYes,
            Self::ResolvedNoCollateral => MarketState::ResolvedNo,
            Self::ExpiredCollateral => MarketState::Expired,
        }
    }

    pub fn is_yes_reissuance(self) -> bool {
        matches!(self, Self::DormantYesRt | Self::UnresolvedYesRt)
    }

    pub fn is_no_reissuance(self) -> bool {
        matches!(self, Self::DormantNoRt | Self::UnresolvedNoRt)
    }

    pub fn is_reissuance(self) -> bool {
        self.is_yes_reissuance() || self.is_no_reissuance()
    }

    pub fn is_collateral(self) -> bool {
        !self.is_reissuance()
    }

    pub fn collateral_slot_for_state(state: MarketState) -> Option<Self> {
        match state {
            MarketState::Dormant => None,
            MarketState::Unresolved => Some(Self::UnresolvedCollateral),
            MarketState::ResolvedYes => Some(Self::ResolvedYesCollateral),
            MarketState::ResolvedNo => Some(Self::ResolvedNoCollateral),
            MarketState::Expired => Some(Self::ExpiredCollateral),
        }
    }

    pub fn live_slots_for_state(state: MarketState) -> &'static [Self] {
        match state {
            MarketState::Dormant => &Self::DORMANT,
            MarketState::Unresolved => &Self::UNRESOLVED,
            MarketState::ResolvedYes => &[Self::ResolvedYesCollateral],
            MarketState::ResolvedNo => &[Self::ResolvedNoCollateral],
            MarketState::Expired => &[Self::ExpiredCollateral],
        }
    }

    pub fn derive_state_from_live_slots<I>(slots: I) -> Result<MarketState, String>
    where
        I: IntoIterator<Item = Self>,
    {
        let mut present = [false; 8];
        for slot in slots {
            present[slot.as_u8() as usize] = true;
        }

        let is_exact = |expected: &[Self]| {
            Self::ALL
                .iter()
                .all(|slot| present[slot.as_u8() as usize] == expected.contains(slot))
        };

        if is_exact(&Self::DORMANT) {
            return Ok(MarketState::Dormant);
        }
        if is_exact(&Self::UNRESOLVED) {
            return Ok(MarketState::Unresolved);
        }
        if is_exact(&[Self::ResolvedYesCollateral]) {
            return Ok(MarketState::ResolvedYes);
        }
        if is_exact(&[Self::ResolvedNoCollateral]) {
            return Ok(MarketState::ResolvedNo);
        }
        if is_exact(&[Self::ExpiredCollateral]) {
            return Ok(MarketState::Expired);
        }

        let present_slots: Vec<MarketSlot> = Self::ALL
            .into_iter()
            .filter(|slot| present[slot.as_u8() as usize])
            .collect();
        Err(format!("inconsistent live slot set: {present_slots:?}"))
    }
}

impl MarketState {
    pub fn from_u64(v: u64) -> Option<Self> {
        match v {
            0 => Some(Self::Dormant),
            1 => Some(Self::Unresolved),
            2 => Some(Self::ResolvedYes),
            3 => Some(Self::ResolvedNo),
            4 => Some(Self::Expired),
            _ => None,
        }
    }

    pub fn as_u64(self) -> u64 {
        self as u64
    }

    pub fn live_slots(self) -> &'static [MarketSlot] {
        MarketSlot::live_slots_for_state(self)
    }

    pub fn collateral_slot(self) -> Option<MarketSlot> {
        MarketSlot::collateral_slot_for_state(self)
    }

    /// Returns the winning token asset ID for a resolved state.
    /// YES asset if state == ResolvedYes, NO asset if state == ResolvedNo, None otherwise.
    pub fn winning_token_asset(self, params: &PredictionMarketParams) -> Option<[u8; 32]> {
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
    fn state_roundtrip() {
        for v in 0..=4 {
            let state = MarketState::from_u64(v).unwrap();
            assert_eq!(state.as_u64(), v);
        }
        assert!(MarketState::from_u64(5).is_none());
    }

    #[test]
    fn slot_roundtrip() {
        for v in 0..=7 {
            let slot = MarketSlot::from_u8(v).unwrap();
            assert_eq!(slot.as_u8(), v);
        }
        assert!(MarketSlot::from_u8(8).is_none());
    }

    #[test]
    fn derive_state_from_live_slots_is_strict() {
        assert_eq!(
            MarketSlot::derive_state_from_live_slots(MarketSlot::DORMANT).unwrap(),
            MarketState::Dormant
        );
        assert_eq!(
            MarketSlot::derive_state_from_live_slots(MarketSlot::UNRESOLVED).unwrap(),
            MarketState::Unresolved
        );
        assert!(
            MarketSlot::derive_state_from_live_slots([
                MarketSlot::DormantYesRt,
                MarketSlot::UnresolvedNoRt,
            ])
            .is_err()
        );
    }

    #[test]
    fn winning_token() {
        let params = PredictionMarketParams {
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
        assert_eq!(MarketState::Expired.winning_token_asset(&params), None);
    }
}
