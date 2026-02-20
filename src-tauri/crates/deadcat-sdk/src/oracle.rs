use sha2::{Digest, Sha256};

use crate::params::MarketId;

/// Construct the oracle message: SHA256(market_id || outcome_byte).
/// outcome_byte is 0x01 for YES, 0x00 for NO.
pub fn oracle_message(market_id: &MarketId, outcome_yes: bool) -> [u8; 32] {
    let outcome_byte: u8 = if outcome_yes { 0x01 } else { 0x00 };
    let mut hasher = Sha256::new();
    hasher.update(market_id.as_bytes());
    hasher.update([outcome_byte]);
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oracle_message_deterministic() {
        let id = MarketId([0xab; 32]);
        let msg1 = oracle_message(&id, true);
        let msg2 = oracle_message(&id, true);
        assert_eq!(msg1, msg2);
    }

    #[test]
    fn oracle_message_differs_by_outcome() {
        let id = MarketId([0xab; 32]);
        let yes_msg = oracle_message(&id, true);
        let no_msg = oracle_message(&id, false);
        assert_ne!(yes_msg, no_msg);
    }

    #[test]
    fn oracle_message_differs_by_market() {
        let id1 = MarketId([0x01; 32]);
        let id2 = MarketId([0x02; 32]);
        let msg1 = oracle_message(&id1, true);
        let msg2 = oracle_message(&id2, true);
        assert_ne!(msg1, msg2);
    }
}
