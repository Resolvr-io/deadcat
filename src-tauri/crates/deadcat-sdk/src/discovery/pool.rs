use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

use crate::amm_pool::math::PoolReserves;
use crate::amm_pool::params::AmmPoolParams;

use super::{APP_EVENT_KIND, NETWORK_TAG, POOL_TAG, bytes_to_hex};

/// Published to Nostr — contains AMM pool params + discovery metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolAnnouncement {
    pub version: u8,
    pub params: AmmPoolParams,
    pub market_id: String,
    pub issued_lp: u64,
    pub covenant_cmr: String,
    pub outpoints: Vec<String>,
    pub reserves: PoolReserves,
}

/// Parsed from a Nostr event — what a trader or LP sees.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredPool {
    pub id: String,
    pub market_id: String,
    pub pool_id: String,
    pub yes_asset_id: String,
    pub no_asset_id: String,
    pub lbtc_asset_id: String,
    pub lp_asset_id: String,
    pub lp_reissuance_token_id: String,
    pub fee_bps: u64,
    pub cosigner_pubkey: String,
    pub issued_lp: u64,
    pub covenant_cmr: String,
    pub outpoints: Vec<String>,
    pub reserves: PoolReserves,
    pub creator_pubkey: String,
    pub created_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nostr_event_json: Option<String>,
}

/// Build a Nostr event for an AMM pool announcement.
///
/// Uses NIP-33 replaceable events with `d` tag = pool_id hex, so subsequent
/// announcements (e.g. after LP deposit/withdraw) replace the previous one.
pub fn build_pool_event(keys: &Keys, announcement: &PoolAnnouncement) -> Result<Event, String> {
    let pool_id = crate::amm_pool::params::PoolId::from_params(&announcement.params);
    let pool_id_hex = pool_id.to_hex();

    let content =
        serde_json::to_string(announcement).map_err(|e| format!("failed to serialize: {e}"))?;

    let tags = vec![
        Tag::identifier(&pool_id_hex),
        Tag::hashtag(POOL_TAG),
        Tag::hashtag(&announcement.market_id),
        Tag::custom(TagKind::custom("network"), vec![NETWORK_TAG.to_string()]),
    ];

    let event = EventBuilder::new(APP_EVENT_KIND, &content)
        .tags(tags)
        .sign_with_keys(keys)
        .map_err(|e| format!("failed to build event: {e}"))?;

    Ok(event)
}

/// Build a Nostr filter for fetching AMM pool announcements.
///
/// If `market_id_hex` is provided, filters to pools for that specific market.
pub fn build_pool_filter(market_id_hex: Option<&str>) -> Filter {
    let mut filter = Filter::new().kind(APP_EVENT_KIND).hashtag(POOL_TAG);

    if let Some(market_id) = market_id_hex {
        filter = filter.hashtag(market_id);
    }

    filter
}

/// Parse a Nostr event into a `DiscoveredPool`.
pub fn parse_pool_event(event: &Event) -> Result<DiscoveredPool, String> {
    let announcement: PoolAnnouncement = serde_json::from_str(&event.content)
        .map_err(|e| format!("failed to parse pool announcement: {e}"))?;

    let pool_id = crate::amm_pool::params::PoolId::from_params(&announcement.params);

    Ok(DiscoveredPool {
        id: event.id.to_hex(),
        market_id: announcement.market_id,
        pool_id: pool_id.to_hex(),
        yes_asset_id: bytes_to_hex(&announcement.params.yes_asset_id),
        no_asset_id: bytes_to_hex(&announcement.params.no_asset_id),
        lbtc_asset_id: bytes_to_hex(&announcement.params.lbtc_asset_id),
        lp_asset_id: bytes_to_hex(&announcement.params.lp_asset_id),
        lp_reissuance_token_id: bytes_to_hex(&announcement.params.lp_reissuance_token_id),
        fee_bps: announcement.params.fee_bps,
        cosigner_pubkey: bytes_to_hex(&announcement.params.cosigner_pubkey),
        issued_lp: announcement.issued_lp,
        covenant_cmr: announcement.covenant_cmr,
        outpoints: announcement.outpoints,
        reserves: announcement.reserves,
        creator_pubkey: event.pubkey.to_hex(),
        created_at: event.created_at.as_u64(),
        nostr_event_json: None,
    })
}

/// Fetch AMM pool announcements from relays.
pub async fn fetch_pools(
    client: &Client,
    market_id_hex: Option<&str>,
) -> Result<Vec<DiscoveredPool>, String> {
    let filter = build_pool_filter(market_id_hex);
    let events = client
        .fetch_events(vec![filter], std::time::Duration::from_secs(15))
        .await
        .map_err(|e| format!("failed to fetch pool events: {e}"))?;

    let mut pools = Vec::new();
    for event in events.iter() {
        match parse_pool_event(event) {
            Ok(pool) => pools.push(pool),
            Err(e) => {
                log::warn!("skipping unparseable pool event {}: {e}", event.id);
            }
        }
    }

    Ok(pools)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taproot::NUMS_KEY_BYTES;

    fn test_pool_params() -> AmmPoolParams {
        AmmPoolParams {
            yes_asset_id: [0x01; 32],
            no_asset_id: [0x02; 32],
            lbtc_asset_id: [0x03; 32],
            lp_asset_id: [0x04; 32],
            lp_reissuance_token_id: [0x05; 32],
            fee_bps: 30,
            cosigner_pubkey: NUMS_KEY_BYTES,
        }
    }

    fn test_announcement() -> PoolAnnouncement {
        PoolAnnouncement {
            version: 1,
            params: test_pool_params(),
            market_id: "abcd1234".to_string(),
            issued_lp: 1_000_000,
            covenant_cmr: hex::encode([0xcc; 32]),
            outpoints: vec![
                "aabb:0".to_string(),
                "aabb:1".to_string(),
                "aabb:2".to_string(),
                "aabb:3".to_string(),
            ],
            reserves: PoolReserves {
                r_yes: 500_000,
                r_no: 500_000,
                r_lbtc: 250_000,
            },
        }
    }

    #[test]
    fn pool_announcement_serde_roundtrip() {
        let announcement = test_announcement();
        let json = serde_json::to_string(&announcement).unwrap();
        let parsed: PoolAnnouncement = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.params, announcement.params);
        assert_eq!(parsed.market_id, "abcd1234");
        assert_eq!(parsed.issued_lp, 1_000_000);
        assert_eq!(parsed.reserves.r_yes, 500_000);
    }

    #[test]
    fn build_and_parse_pool_event() {
        let keys = Keys::generate();
        let announcement = test_announcement();
        let event = build_pool_event(&keys, &announcement).unwrap();

        let discovered = parse_pool_event(&event).unwrap();
        assert_eq!(discovered.market_id, "abcd1234");
        assert_eq!(discovered.fee_bps, 30);
        assert_eq!(discovered.issued_lp, 1_000_000);
        assert_eq!(discovered.reserves.r_yes, 500_000);
        assert_eq!(discovered.reserves.r_no, 500_000);
        assert_eq!(discovered.reserves.r_lbtc, 250_000);
        assert_eq!(discovered.creator_pubkey, keys.public_key().to_hex());
        assert_eq!(discovered.outpoints.len(), 4);
    }

    #[test]
    fn pool_filter_without_market() {
        let filter = build_pool_filter(None);
        let debug = format!("{filter:?}");
        assert!(debug.contains("30078"));
    }

    #[test]
    fn pool_filter_with_market() {
        let filter = build_pool_filter(Some("abcd1234"));
        let debug = format!("{filter:?}");
        assert!(debug.contains("abcd1234"));
    }
}
