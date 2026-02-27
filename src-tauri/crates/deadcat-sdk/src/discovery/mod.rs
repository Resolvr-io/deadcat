//! Unified Nostr discovery service for markets, orders, and attestations.
//!
//! This module consolidates all Nostr-based discovery (previously split between
//! `order_announcement`, `order_discovery`, and the app-layer `discovery.rs`)
//! into a single SDK-owned module.

pub(crate) mod attestation;
pub(crate) mod config;
pub(crate) mod events;
pub(crate) mod market;
pub(crate) mod pool;
pub(crate) mod service;
pub(crate) mod store_trait;

use std::time::Duration;

use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

use crate::maker_order::params::{MakerOrderParams, OrderDirection};

// ---------------------------------------------------------------------------
// Shared constants
// ---------------------------------------------------------------------------

/// Nostr event kind for app-specific data (NIP-78).
/// Used for both contract announcements and order announcements.
pub const APP_EVENT_KIND: Kind = Kind::Custom(30078);

/// Tag value identifying a deadcat contract announcement.
pub const CONTRACT_TAG: &str = "deadcat-contract";

/// Tag value identifying a deadcat limit order.
pub const ORDER_TAG: &str = "deadcat-order";

/// Tag value identifying a deadcat oracle attestation.
pub const ATTESTATION_TAG: &str = "deadcat-attestation";

/// Tag value identifying a deadcat AMM pool.
pub const POOL_TAG: &str = "deadcat-pool";

/// Network tag value for Liquid Testnet.
pub const NETWORK_TAG: &str = "liquid-testnet";

/// Default relay URLs.
pub const DEFAULT_RELAYS: &[&str] = &["wss://relay.damus.io", "wss://relay.primal.net"];

// ---------------------------------------------------------------------------
// Re-exports: market
// ---------------------------------------------------------------------------

pub use market::{
    DiscoveredMarket, build_announcement_event, build_contract_filter, parse_announcement_event,
};

// ---------------------------------------------------------------------------
// Re-exports: attestation
// ---------------------------------------------------------------------------

pub use attestation::{
    AttestationContent, AttestationResult, build_attestation_event, build_attestation_filter,
    sign_attestation,
};

// ---------------------------------------------------------------------------
// Re-exports: pool
// ---------------------------------------------------------------------------

pub use pool::{DiscoveredPool, PoolAnnouncement};

// ---------------------------------------------------------------------------
// Re-exports: config, events, service, store_trait
// ---------------------------------------------------------------------------

pub use config::DiscoveryConfig;
pub use events::DiscoveryEvent;
pub use service::{DiscoveryService, NoopStore, discovered_market_to_contract_params};
pub use store_trait::{DiscoveredMarketMetadata, DiscoveryStore, PoolInfo, PoolSnapshot};

// ---------------------------------------------------------------------------
// Order types (moved from order_announcement.rs)
// ---------------------------------------------------------------------------

/// Published to Nostr, contains maker order params + discovery metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderAnnouncement {
    pub version: u8,
    pub params: MakerOrderParams,
    pub market_id: String,
    pub maker_base_pubkey: String,
    pub order_nonce: String,
    pub covenant_address: String,
    pub offered_amount: u64,
    pub direction_label: String,
}

/// Parsed from a Nostr event — what the taker sees.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredOrder {
    pub id: String,
    pub market_id: String,
    pub base_asset_id: String,
    pub quote_asset_id: String,
    pub price: u64,
    pub min_fill_lots: u64,
    pub min_remainder_lots: u64,
    pub direction: String,
    pub direction_label: String,
    pub maker_base_pubkey: String,
    pub order_nonce: String,
    pub covenant_address: String,
    pub offered_amount: u64,
    pub cosigner_pubkey: String,
    pub maker_receive_spk_hash: String,
    pub creator_pubkey: String,
    pub created_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nostr_event_json: Option<String>,
}

// ---------------------------------------------------------------------------
// Order event building / parsing (moved from order_discovery.rs)
// ---------------------------------------------------------------------------

pub(crate) fn bytes_to_hex(bytes: &[u8]) -> String {
    hex::encode(bytes)
}

/// Build a Nostr event for a limit order announcement.
pub fn build_order_event(keys: &Keys, announcement: &OrderAnnouncement) -> Result<Event, String> {
    let order_uid_hex = &announcement.market_id;

    let content =
        serde_json::to_string(announcement).map_err(|e| format!("failed to serialize: {e}"))?;

    let tags = vec![
        Tag::identifier(order_uid_hex),
        Tag::hashtag(ORDER_TAG),
        Tag::hashtag(&announcement.market_id),
        Tag::custom(TagKind::custom("network"), vec![NETWORK_TAG.to_string()]),
        Tag::custom(
            TagKind::custom("direction"),
            vec![announcement.direction_label.clone()],
        ),
        Tag::custom(
            TagKind::custom("price"),
            vec![announcement.params.price.to_string()],
        ),
    ];

    let event = EventBuilder::new(APP_EVENT_KIND, &content)
        .tags(tags)
        .sign_with_keys(keys)
        .map_err(|e| format!("failed to build event: {e}"))?;

    Ok(event)
}

/// Build a Nostr filter for fetching limit order announcements.
///
/// If `market_id_hex` is provided, filters to orders for that specific market.
pub fn build_order_filter(market_id_hex: Option<&str>) -> Filter {
    let mut filter = Filter::new().kind(APP_EVENT_KIND).hashtag(ORDER_TAG);

    if let Some(market_id) = market_id_hex {
        filter = filter.hashtag(market_id);
    }

    filter
}

/// Parse a Nostr event into a DiscoveredOrder.
pub fn parse_order_event(event: &Event) -> Result<DiscoveredOrder, String> {
    let announcement: OrderAnnouncement = serde_json::from_str(&event.content)
        .map_err(|e| format!("failed to parse order announcement: {e}"))?;

    let direction_str = match announcement.params.direction {
        OrderDirection::SellBase => "sell-base",
        OrderDirection::SellQuote => "sell-quote",
    };

    Ok(DiscoveredOrder {
        id: event.id.to_hex(),
        market_id: announcement.market_id,
        base_asset_id: bytes_to_hex(&announcement.params.base_asset_id),
        quote_asset_id: bytes_to_hex(&announcement.params.quote_asset_id),
        price: announcement.params.price,
        min_fill_lots: announcement.params.min_fill_lots,
        min_remainder_lots: announcement.params.min_remainder_lots,
        direction: direction_str.to_string(),
        direction_label: announcement.direction_label,
        maker_base_pubkey: announcement.maker_base_pubkey,
        order_nonce: announcement.order_nonce,
        covenant_address: announcement.covenant_address,
        offered_amount: announcement.offered_amount,
        cosigner_pubkey: bytes_to_hex(&announcement.params.cosigner_pubkey),
        maker_receive_spk_hash: bytes_to_hex(&announcement.params.maker_receive_spk_hash),
        creator_pubkey: event.pubkey.to_hex(),
        created_at: event.created_at.as_u64(),
        nostr_event_json: None,
    })
}

// ---------------------------------------------------------------------------
// Relay interaction helpers
// ---------------------------------------------------------------------------

/// Connect a Nostr client to the default relays (or a custom one).
pub async fn connect_client(relay_url: Option<&str>) -> Result<Client, String> {
    let client = Client::default();
    if let Some(url) = relay_url {
        client
            .add_relay(url)
            .await
            .map_err(|e| format!("failed to add relay {url}: {e}"))?;
    } else {
        for url in DEFAULT_RELAYS {
            client
                .add_relay(*url)
                .await
                .map_err(|e| format!("failed to add relay {url}: {e}"))?;
        }
    }
    client.connect().await;
    Ok(client)
}

/// Publish an event to the connected relays.
pub async fn publish_event(client: &Client, event: Event) -> Result<EventId, String> {
    let output = client
        .send_event(event)
        .await
        .map_err(|e| format!("failed to send event: {e}"))?;
    Ok(*output.id())
}

/// Fetch contract announcements from relays.
pub async fn fetch_announcements(client: &Client) -> Result<Vec<DiscoveredMarket>, String> {
    let filter = build_contract_filter();
    let events = client
        .fetch_events(vec![filter], Duration::from_secs(15))
        .await
        .map_err(|e| format!("failed to fetch events: {e}"))?;

    let mut markets = Vec::new();
    for event in events.iter() {
        match parse_announcement_event(event) {
            Ok(market) => markets.push(market),
            Err(e) => {
                log::warn!("skipping unparseable announcement {}: {e}", event.id);
            }
        }
    }

    Ok(markets)
}

/// Fetch limit orders from relays, optionally filtered by market ID.
#[allow(dead_code)]
pub async fn fetch_orders(
    client: &Client,
    market_id_hex: Option<&str>,
) -> Result<Vec<DiscoveredOrder>, String> {
    let filter = build_order_filter(market_id_hex);
    let events = client
        .fetch_events(vec![filter], Duration::from_secs(15))
        .await
        .map_err(|e| format!("failed to fetch order events: {e}"))?;

    let mut orders = Vec::new();
    for event in events.iter() {
        match parse_order_event(event) {
            Ok(order) => orders.push(order),
            Err(e) => {
                log::warn!("skipping unparseable order event {}: {e}", event.id);
            }
        }
    }

    Ok(orders)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taproot::NUMS_KEY_BYTES;

    fn test_announcement() -> OrderAnnouncement {
        let (params, _) = MakerOrderParams::new(
            [0x01; 32],
            [0xbb; 32],
            50_000,
            1,
            1,
            OrderDirection::SellBase,
            NUMS_KEY_BYTES,
            &[0xaa; 32],
            &[0x11; 32],
        );
        OrderAnnouncement {
            version: 1,
            params,
            market_id: "abcd1234".to_string(),
            maker_base_pubkey: hex::encode([0xaa; 32]),
            order_nonce: hex::encode([0x11; 32]),
            covenant_address: "tex1qtest".to_string(),
            offered_amount: 100,
            direction_label: "sell-yes".to_string(),
        }
    }

    #[test]
    fn order_announcement_serde_roundtrip() {
        let announcement = test_announcement();
        let json = serde_json::to_string(&announcement).unwrap();
        let parsed: OrderAnnouncement = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.params, announcement.params);
        assert_eq!(parsed.market_id, "abcd1234");
        assert_eq!(parsed.offered_amount, 100);
    }

    #[test]
    fn build_and_parse_order_event() {
        let keys = Keys::generate();
        let announcement = test_announcement();
        let event = build_order_event(&keys, &announcement).unwrap();

        let discovered = parse_order_event(&event).unwrap();
        assert_eq!(discovered.market_id, "abcd1234");
        assert_eq!(discovered.price, 50_000);
        assert_eq!(discovered.direction, "sell-base");
        assert_eq!(discovered.direction_label, "sell-yes");
        assert_eq!(discovered.offered_amount, 100);
        assert_eq!(discovered.creator_pubkey, keys.public_key().to_hex());
    }

    #[test]
    fn order_filter_without_market() {
        let filter = build_order_filter(None);
        let debug = format!("{filter:?}");
        assert!(debug.contains("30078"));
    }

    #[test]
    fn order_filter_with_market() {
        let filter = build_order_filter(Some("abcd1234"));
        let debug = format!("{filter:?}");
        assert!(debug.contains("abcd1234"));
    }
}
