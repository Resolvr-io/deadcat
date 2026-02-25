//! Helpers for converting Nostr discovery types into typed SDK structs.

use crate::amm_pool::params::AmmPoolParams;
use crate::discovery::{DiscoveredOrder, DiscoveredPool};
use crate::error::{Error, Result};
use crate::maker_order::params::{MakerOrderParams, OrderDirection};

/// Decode a hex string into a fixed 32-byte array.
pub(crate) fn hex_to_bytes32(hex: &str) -> Result<[u8; 32]> {
    let bytes =
        hex::decode(hex).map_err(|e| Error::TradeRouting(format!("invalid hex '{hex}': {e}")))?;
    if bytes.len() != 32 {
        return Err(Error::TradeRouting(format!(
            "expected 32 bytes, got {}",
            bytes.len()
        )));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(arr)
}

/// Parse a `DiscoveredPool` into `(AmmPoolParams, issued_lp, pool_id)`.
pub(crate) fn parse_discovered_pool(pool: &DiscoveredPool) -> Result<(AmmPoolParams, u64, String)> {
    let params = AmmPoolParams {
        yes_asset_id: hex_to_bytes32(&pool.yes_asset_id)?,
        no_asset_id: hex_to_bytes32(&pool.no_asset_id)?,
        lbtc_asset_id: hex_to_bytes32(&pool.lbtc_asset_id)?,
        lp_asset_id: hex_to_bytes32(&pool.lp_asset_id)?,
        lp_reissuance_token_id: hex_to_bytes32(&pool.lp_reissuance_token_id)?,
        fee_bps: pool.fee_bps,
        cosigner_pubkey: hex_to_bytes32(&pool.cosigner_pubkey)?,
    };
    Ok((params, pool.issued_lp, pool.pool_id.clone()))
}

/// Parse a `DiscoveredOrder` into `(MakerOrderParams, maker_base_pubkey, order_nonce)`.
pub(crate) fn parse_discovered_order(
    order: &DiscoveredOrder,
) -> Result<(MakerOrderParams, [u8; 32], [u8; 32])> {
    let direction = match order.direction.as_str() {
        "sell-base" => OrderDirection::SellBase,
        "sell-quote" => OrderDirection::SellQuote,
        other => {
            return Err(Error::TradeRouting(format!(
                "unknown order direction: {other}"
            )));
        }
    };
    let maker_base_pubkey = hex_to_bytes32(&order.maker_base_pubkey)?;
    let order_nonce = hex_to_bytes32(&order.order_nonce)?;

    let params = MakerOrderParams {
        base_asset_id: hex_to_bytes32(&order.base_asset_id)?,
        quote_asset_id: hex_to_bytes32(&order.quote_asset_id)?,
        price: order.price,
        min_fill_lots: order.min_fill_lots,
        min_remainder_lots: order.min_remainder_lots,
        direction,
        maker_receive_spk_hash: hex_to_bytes32(&order.maker_receive_spk_hash)?,
        cosigner_pubkey: hex_to_bytes32(&order.cosigner_pubkey)?,
        maker_pubkey: maker_base_pubkey,
    };
    Ok((params, maker_base_pubkey, order_nonce))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::amm_pool::math::PoolReserves;

    fn hex32(byte: u8) -> String {
        hex::encode([byte; 32])
    }

    // ── hex_to_bytes32 ──────────────────────────────────────────────────

    #[test]
    fn hex_to_bytes32_valid() {
        let input = hex::encode([0xab; 32]);
        assert_eq!(hex_to_bytes32(&input).unwrap(), [0xab; 32]);
    }

    #[test]
    fn hex_to_bytes32_wrong_length() {
        let short = hex::encode([0x01; 16]);
        let err = hex_to_bytes32(&short).unwrap_err();
        assert!(err.to_string().contains("expected 32 bytes"));
    }

    #[test]
    fn hex_to_bytes32_invalid_hex() {
        let err = hex_to_bytes32("zzzz").unwrap_err();
        assert!(err.to_string().contains("invalid hex"));
    }

    // ── parse_discovered_pool ───────────────────────────────────────────

    fn sample_pool() -> DiscoveredPool {
        DiscoveredPool {
            id: "evt1".into(),
            market_id: "mkt1".into(),
            pool_id: "pool1".into(),
            yes_asset_id: hex32(0x01),
            no_asset_id: hex32(0x02),
            lbtc_asset_id: hex32(0x03),
            lp_asset_id: hex32(0x04),
            lp_reissuance_token_id: hex32(0x05),
            fee_bps: 30,
            cosigner_pubkey: hex32(0x06),
            issued_lp: 1000,
            covenant_cmr: "ignored".into(),
            outpoints: vec![],
            reserves: PoolReserves {
                r_yes: 500,
                r_no: 500,
                r_lbtc: 1000,
            },
            creator_pubkey: "pubkey".into(),
            created_at: 0,
            nostr_event_json: None,
        }
    }

    #[test]
    fn parse_pool_round_trip() {
        let pool = sample_pool();
        let (params, issued_lp, pool_id) = parse_discovered_pool(&pool).unwrap();
        assert_eq!(params.yes_asset_id, [0x01; 32]);
        assert_eq!(params.no_asset_id, [0x02; 32]);
        assert_eq!(params.fee_bps, 30);
        assert_eq!(issued_lp, 1000);
        assert_eq!(pool_id, "pool1");
    }

    #[test]
    fn parse_pool_bad_hex_field() {
        let mut pool = sample_pool();
        pool.yes_asset_id = "not_hex".into();
        assert!(parse_discovered_pool(&pool).is_err());
    }

    // ── parse_discovered_order ──────────────────────────────────────────

    fn sample_order() -> DiscoveredOrder {
        DiscoveredOrder {
            id: "ord1".into(),
            market_id: "mkt1".into(),
            base_asset_id: hex32(0x01),
            quote_asset_id: hex32(0x02),
            price: 50_000,
            min_fill_lots: 1,
            min_remainder_lots: 1,
            direction: "sell-base".into(),
            direction_label: "sell-yes".into(),
            maker_base_pubkey: hex32(0xaa),
            order_nonce: hex32(0xbb),
            covenant_address: "tex1qtest".into(),
            offered_amount: 100,
            cosigner_pubkey: hex32(0xcc),
            maker_receive_spk_hash: hex32(0xdd),
            creator_pubkey: "pk".into(),
            created_at: 0,
            nostr_event_json: None,
        }
    }

    #[test]
    fn parse_order_sell_base() {
        let order = sample_order();
        let (params, maker_pk, nonce) = parse_discovered_order(&order).unwrap();
        assert_eq!(params.direction, OrderDirection::SellBase);
        assert_eq!(params.price, 50_000);
        assert_eq!(maker_pk, [0xaa; 32]);
        assert_eq!(nonce, [0xbb; 32]);
    }

    #[test]
    fn parse_order_sell_quote() {
        let mut order = sample_order();
        order.direction = "sell-quote".into();
        let (params, _, _) = parse_discovered_order(&order).unwrap();
        assert_eq!(params.direction, OrderDirection::SellQuote);
    }

    #[test]
    fn parse_order_unknown_direction() {
        let mut order = sample_order();
        order.direction = "buy-base".into();
        let err = parse_discovered_order(&order).unwrap_err();
        assert!(err.to_string().contains("unknown order direction"));
    }

    #[test]
    fn parse_order_bad_hex_nonce() {
        let mut order = sample_order();
        order.order_nonce = "short".into();
        assert!(parse_discovered_order(&order).is_err());
    }
}
