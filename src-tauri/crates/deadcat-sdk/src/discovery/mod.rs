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

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};

use crate::maker_order::params::{MakerOrderParams, OrderDirection};
use crate::network::Network;

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

/// Tag value identifying a tagged order deletion request.
pub const ORDER_DELETE_TAG: &str = "order";

/// Tag value identifying a deadcat oracle attestation.
pub const ATTESTATION_TAG: &str = "deadcat-attestation";

/// Tag value identifying a deadcat pool announcement.
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

pub use pool::{DiscoveredPool, PoolAnnouncement, PoolParams, build_pool_event};

// ---------------------------------------------------------------------------
// Re-exports: config, events, service, store_trait
// ---------------------------------------------------------------------------

pub use config::DiscoveryConfig;
pub use events::DiscoveryEvent;
pub use service::{DiscoveryService, NoopStore, discovered_market_to_contract_params};
pub use store_trait::{
    ContractMetadataInput, DiscoveryStore, LmsrPoolIngestInput, LmsrPoolStateSource,
    LmsrPoolStateUpdateInput, NodeStore, OwnMakerOrderRecordInput, OwnOrderStatusChange,
    PendingOrderDeletion, PredictionMarketCandidateIngestInput, StoredOrderStatus,
};

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

pub fn order_addressable_id(market_id: &str, maker_base_pubkey: &str, order_nonce: &str) -> String {
    format!(
        "order:v1:{}:{}:{}",
        market_id.to_ascii_lowercase(),
        maker_base_pubkey.to_ascii_lowercase(),
        order_nonce.to_ascii_lowercase()
    )
}

fn parse_network_tag(network_tag: &str) -> Result<(), String> {
    network_tag
        .parse::<Network>()
        .map(|_| ())
        .map_err(|e| format!("unsupported network tag '{network_tag}': {e}"))
}

pub(crate) fn event_network_tag(event: &Event) -> Option<String> {
    event.tags.iter().find_map(|tag| {
        let fields = tag.as_slice();
        if fields.len() >= 2 && fields[0] == "network" {
            Some(fields[1].to_string())
        } else {
            None
        }
    })
}

fn build_order_tags(
    market_id: &str,
    maker_base_pubkey: &str,
    order_nonce: &str,
    direction_label: &str,
    price: u64,
    network_tag: &str,
    deleted: bool,
) -> Vec<Tag> {
    let mut tags = vec![
        Tag::identifier(order_addressable_id(
            market_id,
            maker_base_pubkey,
            order_nonce,
        )),
        Tag::hashtag(ORDER_TAG),
        Tag::hashtag(market_id),
        Tag::custom(TagKind::custom("network"), vec![network_tag.to_string()]),
        Tag::custom(
            TagKind::custom("direction"),
            vec![direction_label.to_string()],
        ),
        Tag::custom(TagKind::custom("price"), vec![price.to_string()]),
    ];
    if deleted {
        tags.push(Tag::custom(
            TagKind::custom("deleted"),
            vec!["true".to_string()],
        ));
    }
    tags
}

pub(crate) fn event_identifier(event: &Event) -> Option<String> {
    event.tags.iter().find_map(|tag| {
        let fields = tag.as_slice();
        if fields.len() >= 2 && fields[0] == "d" {
            Some(fields[1].to_string())
        } else {
            None
        }
    })
}

pub(crate) fn event_hashtags(event: &Event) -> Vec<String> {
    event
        .tags
        .iter()
        .filter_map(|tag| {
            let fields = tag.as_slice();
            if fields.len() >= 2 && fields[0] == "t" {
                Some(fields[1].to_string())
            } else {
                None
            }
        })
        .collect()
}

pub(crate) fn is_order_addressable_id(identifier: &str) -> bool {
    identifier.starts_with("order:v1:")
}

pub(crate) fn is_order_tombstone_event(event: &Event) -> bool {
    event.tags.iter().any(|tag| {
        let fields = tag.as_slice();
        fields.len() >= 2 && fields[0] == "deleted" && fields[1].eq_ignore_ascii_case("true")
    })
}

pub(crate) fn order_invalidation_market_id(event: &Event) -> Option<String> {
    event_hashtags(event)
        .into_iter()
        .find(|hashtag| hashtag != ORDER_TAG && hashtag != ORDER_DELETE_TAG)
}

pub(crate) fn event_matches_network_tag(event: &Event, expected_network_tag: &str) -> bool {
    matches!(
        event_network_tag(event).as_deref(),
        Some(network_tag) if network_tag == expected_network_tag
    )
}

pub(crate) fn select_latest_order_events(events: &[Event]) -> Vec<Event> {
    let mut latest_by_coordinate: HashMap<(String, String), Event> = HashMap::new();
    let mut legacy = Vec::new();

    for event in events {
        let Some(identifier) = event_identifier(event) else {
            legacy.push(event.clone());
            continue;
        };
        if !is_order_addressable_id(&identifier) {
            legacy.push(event.clone());
            continue;
        }

        let key = (event.pubkey.to_hex(), identifier);
        match latest_by_coordinate.get_mut(&key) {
            None => {
                latest_by_coordinate.insert(key, event.clone());
            }
            Some(existing) => {
                let candidate_created_at = event.created_at.as_u64();
                let existing_created_at = existing.created_at.as_u64();
                let should_replace = candidate_created_at > existing_created_at
                    || (candidate_created_at == existing_created_at && event.id < existing.id);
                if should_replace {
                    *existing = event.clone();
                }
            }
        }
    }

    legacy.extend(latest_by_coordinate.into_values());
    legacy
}

pub(crate) fn select_fetchable_order_events(
    events: &[Event],
    expected_network_tag: &str,
) -> Result<Vec<Event>, String> {
    parse_network_tag(expected_network_tag)?;

    // Filter to the requested network before replaceable dedup so same-coordinate
    // announcements from other networks cannot shadow local orders.
    let network_events = events
        .iter()
        .filter(|event| event_matches_network_tag(event, expected_network_tag))
        .cloned()
        .collect::<Vec<_>>();

    Ok(select_latest_order_events(&network_events))
}

pub(crate) fn deleted_order_event_ids(
    candidate_events: &[Event],
    deletion_events: &[Event],
    expected_network_tag: &str,
) -> Result<HashSet<String>, String> {
    parse_network_tag(expected_network_tag)?;

    let order_authors: HashMap<String, String> = candidate_events
        .iter()
        .map(|event| (event.id.to_hex(), event.pubkey.to_hex()))
        .collect();

    let mut deleted = HashSet::new();
    for deletion in deletion_events {
        if !event_matches_network_tag(deletion, expected_network_tag) {
            continue;
        }
        let deletion_pubkey = deletion.pubkey.to_hex();
        for tag in deletion.tags.iter() {
            let fields = tag.as_slice();
            if fields.len() < 2 || fields[0] != "e" {
                continue;
            }
            let referenced = fields[1].to_string();
            if order_authors
                .get(&referenced)
                .is_some_and(|author| author == &deletion_pubkey)
            {
                deleted.insert(referenced);
            }
        }
    }

    Ok(deleted)
}

pub async fn fetch_order_deletion_events(
    client: &Client,
    order_events: &[Event],
    timeout: Duration,
) -> Result<Vec<Event>, String> {
    if order_events.is_empty() {
        return Ok(Vec::new());
    }

    let filter = Filter::new().kind(Kind::Custom(5)).events(
        order_events
            .iter()
            .map(|event| event.id)
            .collect::<Vec<_>>(),
    );
    let events = client
        .fetch_events(vec![filter], timeout)
        .await
        .map_err(|e| format!("failed to fetch order deletion events: {e}"))?;

    Ok(events.iter().cloned().collect())
}

/// Build a Nostr event for a limit order announcement.
pub fn build_order_event(
    keys: &Keys,
    announcement: &OrderAnnouncement,
    network_tag: &str,
) -> Result<Event, String> {
    parse_network_tag(network_tag)?;

    let content =
        serde_json::to_string(announcement).map_err(|e| format!("failed to serialize: {e}"))?;

    let tags = build_order_tags(
        &announcement.market_id,
        &announcement.maker_base_pubkey,
        &announcement.order_nonce,
        &announcement.direction_label,
        announcement.params.price,
        network_tag,
        false,
    );

    let event = EventBuilder::new(APP_EVENT_KIND, &content)
        .tags(tags)
        .sign_with_keys(keys)
        .map_err(|e| format!("failed to build event: {e}"))?;

    Ok(event)
}

pub fn build_order_tombstone_event(
    keys: &Keys,
    market_id: &str,
    maker_base_pubkey: &str,
    order_nonce: &str,
    direction_label: &str,
    price: u64,
    network_tag: &str,
) -> Result<Event, String> {
    parse_network_tag(network_tag)?;

    let tags = build_order_tags(
        market_id,
        maker_base_pubkey,
        order_nonce,
        direction_label,
        price,
        network_tag,
        true,
    );

    EventBuilder::new(APP_EVENT_KIND, "")
        .tags(tags)
        .sign_with_keys(keys)
        .map_err(|e| format!("failed to build order tombstone event: {e}"))
}

pub fn build_order_deletion_request_event(
    keys: &Keys,
    original_event_id: &str,
    market_id: &str,
    network_tag: &str,
) -> Result<Event, String> {
    parse_network_tag(network_tag)?;
    let event_id =
        EventId::from_hex(original_event_id).map_err(|e| format!("invalid event id: {e}"))?;
    let tags = vec![
        Tag::event(event_id),
        Tag::custom(
            TagKind::custom("k"),
            vec![APP_EVENT_KIND.as_u16().to_string()],
        ),
        Tag::hashtag(ORDER_DELETE_TAG),
        Tag::hashtag(market_id),
        Tag::custom(TagKind::custom("network"), vec![network_tag.to_string()]),
    ];

    EventBuilder::new(Kind::Custom(5), "delete limit order announcement")
        .tags(tags)
        .sign_with_keys(keys)
        .map_err(|e| format!("failed to build order deletion event: {e}"))
}

pub fn build_order_deletion_filter(market_id_hex: Option<&str>) -> Filter {
    let mut filter = Filter::new()
        .kind(Kind::Custom(5))
        .hashtag(ORDER_DELETE_TAG);

    if let Some(market_id) = market_id_hex {
        filter = filter.hashtag(market_id);
    }

    filter
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
pub fn parse_order_event(
    event: &Event,
    expected_network_tag: &str,
) -> Result<DiscoveredOrder, String> {
    parse_network_tag(expected_network_tag)?;
    if is_order_tombstone_event(event) {
        return Err("order tombstone event".to_string());
    }
    let network_tag = event_network_tag(event)
        .ok_or_else(|| "missing network tag for order event".to_string())?;
    if network_tag != expected_network_tag {
        return Err(format!(
            "unsupported network tag for order event: {network_tag}"
        ));
    }
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
pub async fn fetch_announcements(
    client: &Client,
    expected_network_tag: &str,
) -> Result<Vec<DiscoveredMarket>, String> {
    let filter = build_contract_filter();
    let events = client
        .fetch_events(vec![filter], Duration::from_secs(15))
        .await
        .map_err(|e| format!("failed to fetch events: {e}"))?;

    let mut markets = Vec::new();
    for event in events.iter() {
        match parse_announcement_event(event, expected_network_tag) {
            Ok(market) => markets.push(market),
            Err(e) => {
                if e.contains("unsupported contract announcement version") {
                    log::warn!("skipping market announcement {}: {e}", event.id);
                } else {
                    log::warn!("skipping unparseable announcement {}: {e}", event.id);
                }
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
    expected_network_tag: &str,
) -> Result<Vec<DiscoveredOrder>, String> {
    let filter = build_order_filter(market_id_hex);
    let events = client
        .fetch_events(vec![filter], Duration::from_secs(15))
        .await
        .map_err(|e| format!("failed to fetch order events: {e}"))?;
    let selected_events = select_fetchable_order_events(
        &events.iter().cloned().collect::<Vec<_>>(),
        expected_network_tag,
    )?;
    let deletion_events =
        fetch_order_deletion_events(client, &selected_events, Duration::from_secs(15)).await?;
    let deleted_ids =
        deleted_order_event_ids(&selected_events, &deletion_events, expected_network_tag)?;

    let mut orders = Vec::new();
    for event in selected_events {
        if deleted_ids.contains(&event.id.to_hex()) || is_order_tombstone_event(&event) {
            continue;
        }
        match parse_order_event(&event, expected_network_tag) {
            Ok(order) => orders.push(order),
            Err(e) => {
                if e != "order tombstone event" {
                    log::warn!("skipping unparseable order event {}: {e}", event.id);
                }
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
        let event = build_order_event(&keys, &announcement, "liquid-testnet").unwrap();
        let expected_id = order_addressable_id(
            &announcement.market_id,
            &announcement.maker_base_pubkey,
            &announcement.order_nonce,
        );
        assert_eq!(
            event_identifier(&event).as_deref(),
            Some(expected_id.as_str())
        );

        let discovered = parse_order_event(&event, "liquid-testnet").unwrap();
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

    #[test]
    fn order_deletion_filter_with_market() {
        let filter = build_order_deletion_filter(Some("abcd1234"));
        let debug = format!("{filter:?}");
        assert!(debug.contains("5"));
        assert!(debug.contains(ORDER_DELETE_TAG));
        assert!(debug.contains("abcd1234"));
    }

    #[test]
    fn parse_order_event_rejects_network_mismatch() {
        let keys = Keys::generate();
        let announcement = test_announcement();
        let event = build_order_event(&keys, &announcement, "liquid-testnet").unwrap();
        let err = parse_order_event(&event, "liquid-regtest").unwrap_err();
        assert!(
            err.contains("unsupported network tag for order event"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn tombstone_event_is_marked_deleted_and_not_parseable() {
        let keys = Keys::generate();
        let announcement = test_announcement();
        let event = build_order_tombstone_event(
            &keys,
            &announcement.market_id,
            &announcement.maker_base_pubkey,
            &announcement.order_nonce,
            &announcement.direction_label,
            announcement.params.price,
            "liquid-testnet",
        )
        .unwrap();

        assert!(is_order_tombstone_event(&event));
        assert_eq!(
            parse_order_event(&event, "liquid-testnet").unwrap_err(),
            "order tombstone event"
        );
    }

    #[test]
    fn latest_order_selection_replaces_new_format_only() {
        let keys = Keys::generate();
        let announcement = test_announcement();
        let older = build_order_event(&keys, &announcement, "liquid-testnet").unwrap();
        let tombstone = build_order_tombstone_event(
            &keys,
            &announcement.market_id,
            &announcement.maker_base_pubkey,
            &announcement.order_nonce,
            &announcement.direction_label,
            announcement.params.price,
            "liquid-testnet",
        )
        .unwrap();
        let newer = EventBuilder::new(APP_EVENT_KIND, "")
            .tags(tombstone.tags.clone())
            .custom_created_at(Timestamp::from(older.created_at.as_u64() + 1))
            .sign_with_keys(&keys)
            .unwrap();

        let legacy_content = serde_json::to_string(&announcement).unwrap();
        let legacy = EventBuilder::new(APP_EVENT_KIND, &legacy_content)
            .tags(vec![
                Tag::identifier(&announcement.market_id),
                Tag::hashtag(ORDER_TAG),
                Tag::hashtag(&announcement.market_id),
                Tag::custom(
                    TagKind::custom("network"),
                    vec!["liquid-testnet".to_string()],
                ),
            ])
            .custom_created_at(Timestamp::from(older.created_at.as_u64() + 10))
            .sign_with_keys(&keys)
            .unwrap();

        let selected = select_latest_order_events(&[older, newer.clone(), legacy.clone()]);
        assert_eq!(selected.len(), 2);
        assert!(selected.iter().any(|event| event.id == newer.id));
        assert!(selected.iter().any(|event| event.id == legacy.id));
    }

    #[test]
    fn select_fetchable_order_events_prefilters_wrong_network_replacements() {
        let keys = Keys::generate();
        let announcement = test_announcement();
        let order_event = build_order_event(&keys, &announcement, "liquid-testnet").unwrap();
        let wrong_network_event = EventBuilder::new(APP_EVENT_KIND, &order_event.content)
            .tags(vec![
                Tag::identifier(order_addressable_id(
                    &announcement.market_id,
                    &announcement.maker_base_pubkey,
                    &announcement.order_nonce,
                )),
                Tag::hashtag(ORDER_TAG),
                Tag::hashtag(&announcement.market_id),
                Tag::custom(
                    TagKind::custom("network"),
                    vec!["liquid-regtest".to_string()],
                ),
                Tag::custom(
                    TagKind::custom("direction"),
                    vec![announcement.direction_label.clone()],
                ),
                Tag::custom(
                    TagKind::custom("price"),
                    vec![announcement.params.price.to_string()],
                ),
            ])
            .custom_created_at(Timestamp::from(order_event.created_at.as_u64() + 1))
            .sign_with_keys(&keys)
            .unwrap();

        let selected = select_fetchable_order_events(
            &[order_event.clone(), wrong_network_event],
            "liquid-testnet",
        )
        .unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id, order_event.id);
    }

    #[test]
    fn deleted_order_event_ids_require_same_author_and_matching_network() {
        let author_keys = Keys::generate();
        let other_keys = Keys::generate();
        let announcement = test_announcement();
        let order_event = build_order_event(&author_keys, &announcement, "liquid-testnet").unwrap();
        let foreign_delete = build_order_deletion_request_event(
            &other_keys,
            &order_event.id.to_hex(),
            &announcement.market_id,
            "liquid-testnet",
        )
        .unwrap();
        let author_delete = build_order_deletion_request_event(
            &author_keys,
            &order_event.id.to_hex(),
            &announcement.market_id,
            "liquid-testnet",
        )
        .unwrap();
        let wrong_network_delete = build_order_deletion_request_event(
            &author_keys,
            &order_event.id.to_hex(),
            &announcement.market_id,
            "liquid-regtest",
        )
        .unwrap();
        let missing_network_delete =
            EventBuilder::new(Kind::Custom(5), "delete limit order announcement")
                .tags(vec![
                    Tag::event(order_event.id),
                    Tag::custom(
                        TagKind::custom("k"),
                        vec![APP_EVENT_KIND.as_u16().to_string()],
                    ),
                    Tag::hashtag(ORDER_DELETE_TAG),
                    Tag::hashtag(&announcement.market_id),
                ])
                .sign_with_keys(&author_keys)
                .unwrap();

        let foreign_deleted = deleted_order_event_ids(
            std::slice::from_ref(&order_event),
            &[foreign_delete],
            "liquid-testnet",
        )
        .unwrap();
        assert!(foreign_deleted.is_empty());

        let wrong_network_deleted = deleted_order_event_ids(
            std::slice::from_ref(&order_event),
            &[wrong_network_delete],
            "liquid-testnet",
        )
        .unwrap();
        assert!(wrong_network_deleted.is_empty());

        let missing_network_deleted = deleted_order_event_ids(
            std::slice::from_ref(&order_event),
            &[missing_network_delete],
            "liquid-testnet",
        )
        .unwrap();
        assert!(missing_network_deleted.is_empty());

        let author_deleted = deleted_order_event_ids(
            std::slice::from_ref(&order_event),
            &[author_delete],
            "liquid-testnet",
        )
        .unwrap();
        assert!(author_deleted.contains(&order_event.id.to_hex()));
    }

    #[test]
    fn deletion_request_event_carries_order_market_and_network_tags() {
        let keys = Keys::generate();
        let announcement = test_announcement();
        let order_event = build_order_event(&keys, &announcement, "liquid-testnet").unwrap();
        let delete_event = build_order_deletion_request_event(
            &keys,
            &order_event.id.to_hex(),
            &announcement.market_id,
            "liquid-testnet",
        )
        .unwrap();

        let hashtags = event_hashtags(&delete_event);
        assert!(hashtags.iter().any(|tag| tag == ORDER_DELETE_TAG));
        assert!(hashtags.iter().any(|tag| tag == &announcement.market_id));
        assert_eq!(
            event_network_tag(&delete_event).as_deref(),
            Some("liquid-testnet")
        );
        assert_eq!(
            order_invalidation_market_id(&delete_event).as_deref(),
            Some("abcd1234")
        );
    }
}
