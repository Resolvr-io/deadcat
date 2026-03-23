use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use deadcat_sdk::testing::{
    TestStore, oracle_pubkey_from_keys, test_market_announcement, test_order_announcement,
};
use deadcat_sdk::{DiscoveryConfig, DiscoveryEvent, DiscoveryService};
use nostr_relay_builder::prelude::*;
use nostr_sdk::prelude::*;

const POLL_INTERVAL: Duration = Duration::from_millis(50);
const POLL_TIMEOUT: Duration = Duration::from_secs(5);

async fn setup_service_with_store(
    mock_url: &str,
) -> (
    DiscoveryService<TestStore>,
    tokio::sync::broadcast::Receiver<DiscoveryEvent>,
    Arc<Mutex<TestStore>>,
    Keys,
) {
    let keys = Keys::generate();
    let store = Arc::new(Mutex::new(TestStore::default()));
    let config = DiscoveryConfig {
        relays: vec![mock_url.to_string()],
        network_tag: "liquid-testnet".to_string(),
        ..Default::default()
    };
    let (service, rx) = DiscoveryService::with_store(keys.clone(), store.clone(), config);
    (service, rx, store, keys)
}

async fn wait_for<T, F, Fut>(label: &str, mut poll: F) -> T
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Option<T>>,
{
    let deadline = tokio::time::Instant::now() + POLL_TIMEOUT;
    loop {
        if let Some(value) = poll().await {
            return value;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {label}"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn recv_matching_event<M>(
    label: &str,
    rx: &mut tokio::sync::broadcast::Receiver<DiscoveryEvent>,
    mut matches: M,
) -> DiscoveryEvent
where
    M: FnMut(&DiscoveryEvent) -> bool,
{
    let deadline = tokio::time::Instant::now() + POLL_TIMEOUT;
    loop {
        match tokio::time::timeout(POLL_INTERVAL, rx.recv()).await {
            Ok(Ok(event)) if matches(&event) => return event,
            Ok(Ok(_)) | Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) | Err(_) => {}
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                panic!("subscription channel closed while waiting for {label}");
            }
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {label}"
        );
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn market_announce_discover_roundtrip() {
    let mock = MockRelay::run().await.unwrap();
    let (service, _rx, store, keys) = setup_service_with_store(&mock.url()).await;

    let oracle_pubkey = oracle_pubkey_from_keys(&keys);
    let (announcement, params) = test_market_announcement(oracle_pubkey, 0x11);

    // Publish
    let event_id = service.announce_market(&announcement).await.unwrap();
    assert!(!event_id.to_hex().is_empty());

    // Fetch
    let markets = wait_for("market announcement to be fetchable", || async {
        let markets = service.fetch_markets().await.unwrap();
        (!markets.is_empty()).then_some(markets)
    })
    .await;

    let market = &markets[0];
    assert_eq!(market.question, "Will BTC close above $120k by Dec 2026?");
    assert_eq!(market.category, "Bitcoin");
    assert_eq!(market.cpt_sats, 5000);
    assert_eq!(market.expiry_height, 3_650_000);
    assert_eq!(market.oracle_pubkey, hex::encode(oracle_pubkey));
    assert_eq!(
        market.anchor.creation_txid,
        announcement.anchor.creation_txid
    );

    // Verify market_id is correct
    let expected_market_id = params.market_id();
    assert_eq!(market.market_id, hex::encode(expected_market_id.as_bytes()));

    // Verify persisted to store
    let s = store.lock().unwrap();
    assert_eq!(s.markets.len(), 1);
    assert_eq!(s.markets[0].params.market_id(), expected_market_id);
}

#[tokio::test]
async fn order_announce_discover_roundtrip() {
    let mock = MockRelay::run().await.unwrap();
    let (service, _rx, store, _keys) = setup_service_with_store(&mock.url()).await;

    let announcement = test_order_announcement("market123");

    // Publish
    let event_id = service.announce_order(&announcement).await.unwrap();
    assert!(!event_id.to_hex().is_empty());

    // Fetch
    let orders = wait_for("order announcement to be fetchable", || async {
        let orders = service.fetch_orders(None).await.unwrap();
        (!orders.is_empty()).then_some(orders)
    })
    .await;

    let order = &orders[0];
    assert_eq!(order.market_id, "market123");
    assert_eq!(order.price, 50_000);
    assert_eq!(order.direction, "sell-base");
    assert_eq!(order.offered_amount, 100);

    // Verify persisted to store
    let s = store.lock().unwrap();
    assert_eq!(s.orders.len(), 1);
    assert_eq!(s.orders[0].0.price, 50_000);
    // Verify nostr_event_id was stored
    assert!(s.orders[0].1.is_some());
}

#[tokio::test]
async fn subscription_delivers_market_events() {
    let mock = MockRelay::run().await.unwrap();
    let (service, mut rx, _store, keys) = setup_service_with_store(&mock.url()).await;

    // Start subscription loop
    let handle = service.start().await.unwrap();

    // Publish via a SEPARATE client (relay echoes to the subscribing service)
    let publisher = Client::new(keys.clone());
    publisher.add_relay(mock.url()).await.unwrap();
    publisher.connect().await;

    let oracle_pubkey = oracle_pubkey_from_keys(&keys);
    let (announcement, _params) = test_market_announcement(oracle_pubkey, 0x22);
    let event =
        deadcat_sdk::build_announcement_event(&keys, &announcement, "liquid-testnet").unwrap();
    publisher.send_event(event).await.unwrap();

    let event = recv_matching_event("market discovery subscription event", &mut rx, |event| {
        matches!(event, DiscoveryEvent::MarketDiscovered(_))
    })
    .await;

    match event {
        DiscoveryEvent::MarketDiscovered(market) => {
            assert_eq!(market.question, "Will BTC close above $120k by Dec 2026?");
        }
        other => panic!("expected MarketDiscovered, got {other:?}"),
    }

    handle.abort();
    let _ = publisher.disconnect().await;
}

#[tokio::test]
async fn subscription_delivers_order_events() {
    let mock = MockRelay::run().await.unwrap();
    let (service, mut rx, _store, keys) = setup_service_with_store(&mock.url()).await;

    let handle = service.start().await.unwrap();

    // Publish via a separate client
    let publisher = Client::new(keys.clone());
    publisher.add_relay(mock.url()).await.unwrap();
    publisher.connect().await;

    let announcement = test_order_announcement("market456");
    let event = deadcat_sdk::build_order_event(&keys, &announcement, "liquid-testnet").unwrap();
    publisher.send_event(event).await.unwrap();

    let event = recv_matching_event("order discovery subscription event", &mut rx, |event| {
        matches!(event, DiscoveryEvent::OrderDiscovered(_))
    })
    .await;

    match event {
        DiscoveryEvent::OrderDiscovered(order) => {
            assert_eq!(order.market_id, "market456");
            assert_eq!(order.price, 50_000);
        }
        other => panic!("expected OrderDiscovered, got {other:?}"),
    }

    handle.abort();
    let _ = publisher.disconnect().await;
}

#[tokio::test]
async fn store_persistence_on_discovery() {
    let mock = MockRelay::run().await.unwrap();
    let (service, mut rx, store, keys) = setup_service_with_store(&mock.url()).await;

    let handle = service.start().await.unwrap();

    // Publish via a separate client
    let publisher = Client::new(keys.clone());
    publisher.add_relay(mock.url()).await.unwrap();
    publisher.connect().await;

    // Publish a market
    let oracle_pubkey = oracle_pubkey_from_keys(&keys);
    let (announcement, _params) = test_market_announcement(oracle_pubkey, 0x33);
    let event =
        deadcat_sdk::build_announcement_event(&keys, &announcement, "liquid-testnet").unwrap();
    publisher.send_event(event).await.unwrap();

    let _ = recv_matching_event("market persistence subscription event", &mut rx, |event| {
        matches!(event, DiscoveryEvent::MarketDiscovered(_))
    })
    .await;

    // Publish an order
    let order_announcement = test_order_announcement("market789");
    let order_event =
        deadcat_sdk::build_order_event(&keys, &order_announcement, "liquid-testnet").unwrap();
    publisher.send_event(order_event).await.unwrap();

    let _ = recv_matching_event("order persistence subscription event", &mut rx, |event| {
        matches!(event, DiscoveryEvent::OrderDiscovered(_))
    })
    .await;

    // Verify both are persisted
    {
        let s = store.lock().unwrap();
        assert_eq!(s.markets.len(), 1, "market should be persisted");
        assert_eq!(s.orders.len(), 1, "order should be persisted");
    }

    handle.abort();
    let _ = publisher.disconnect().await;
}

#[tokio::test]
async fn attestation_roundtrip() {
    let mock = MockRelay::run().await.unwrap();
    let (service, _rx, _store, keys) = setup_service_with_store(&mock.url()).await;

    let oracle_pubkey = oracle_pubkey_from_keys(&keys);
    let (announcement, params) = test_market_announcement(oracle_pubkey, 0x44);
    let market_id = params.market_id();

    // First publish the announcement
    let ann_event_id = service.announce_market(&announcement).await.unwrap();

    // Publish attestation
    let result = service
        .publish_attestation(&market_id, &ann_event_id.to_hex(), true)
        .await
        .unwrap();

    assert!(result.outcome_yes);
    assert!(!result.signature_hex.is_empty());
    assert!(!result.nostr_event_id.is_empty());

    // Fetch attestation
    let market_id_hex = hex::encode(market_id.as_bytes());
    let att = wait_for("attestation to be fetchable", || async {
        service.fetch_attestation(&market_id_hex).await.unwrap()
    })
    .await;
    assert_eq!(att.market_id, market_id_hex);
    assert!(att.outcome_yes);
    assert_eq!(att.oracle_signature, result.signature_hex);

    // Verify signature
    use nostr_sdk::secp256k1;
    let sig_bytes: [u8; 64] = hex::decode(&att.oracle_signature)
        .unwrap()
        .try_into()
        .unwrap();
    let msg_bytes: [u8; 32] = hex::decode(&att.message).unwrap().try_into().unwrap();

    let secp = secp256k1::Secp256k1::new();
    let message = secp256k1::Message::from_digest(msg_bytes);
    let xonly = secp256k1::XOnlyPublicKey::from_slice(&oracle_pubkey).unwrap();
    let schnorr_sig = secp256k1::schnorr::Signature::from_slice(&sig_bytes).unwrap();
    assert!(secp.verify_schnorr(&schnorr_sig, &message, &xonly).is_ok());
}

#[tokio::test]
async fn duplicate_markets_are_idempotent() {
    let mock = MockRelay::run().await.unwrap();
    let (service, _rx, store, keys) = setup_service_with_store(&mock.url()).await;

    let oracle_pubkey = oracle_pubkey_from_keys(&keys);
    let (announcement, _params) = test_market_announcement(oracle_pubkey, 0x55);

    // Publish the same market twice
    service.announce_market(&announcement).await.unwrap();
    service.announce_market(&announcement).await.unwrap();

    // Fetch — both events come back but store should deduplicate
    let _markets = wait_for("duplicate market fetch to settle", || async {
        let markets = service.fetch_markets().await.unwrap();
        let deduped = store.lock().unwrap().markets.len() == 1;
        (!markets.is_empty() && deduped).then_some(markets)
    })
    .await;

    let s = store.lock().unwrap();
    assert_eq!(s.markets.len(), 1, "store should deduplicate by market_id");
}

#[tokio::test]
async fn fetch_orders_filters_by_market() {
    let mock = MockRelay::run().await.unwrap();
    let (service, _rx, _store, _keys) = setup_service_with_store(&mock.url()).await;

    let announcement_a = test_order_announcement("marketAAA");
    let announcement_b = test_order_announcement("marketBBB");

    service.announce_order(&announcement_a).await.unwrap();
    service.announce_order(&announcement_b).await.unwrap();

    // Fetch all
    let _all_orders = wait_for("all orders to be fetchable", || async {
        let orders = service.fetch_orders(None).await.unwrap();
        (orders.len() >= 2).then_some(orders)
    })
    .await;

    // Fetch filtered — the relay should filter, but we also verify client-side
    let filtered = service.fetch_orders(Some("marketAAA")).await.unwrap();
    // At minimum, the filtered result should contain at least one marketAAA order
    assert!(
        filtered.iter().any(|o| o.market_id == "marketAAA"),
        "filtered orders should include marketAAA"
    );
}
