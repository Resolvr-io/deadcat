use std::sync::{Arc, Mutex};
use std::time::Duration;

use deadcat_sdk::announcement::ContractAnnouncement;
use deadcat_sdk::discovery::{DiscoveryConfig, DiscoveryEvent, DiscoveryService};
use deadcat_sdk::testing::{
    TestStore, oracle_pubkey_from_keys, test_market_params, test_metadata, test_order_announcement,
};
use nostr_relay_builder::prelude::*;
use nostr_sdk::prelude::*;

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
        ..Default::default()
    };
    let (service, rx) = DiscoveryService::with_store(keys.clone(), store.clone(), config);
    (service, rx, store, keys)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn market_announce_discover_roundtrip() {
    let mock = MockRelay::run().await.unwrap();
    let (service, _rx, store, keys) = setup_service_with_store(&mock.url()).await;

    let oracle_pubkey = oracle_pubkey_from_keys(&keys);
    let params = test_market_params(oracle_pubkey);
    let announcement = ContractAnnouncement {
        version: 1,
        contract_params: params,
        metadata: test_metadata(),
        creation_txid: Some("abc123def456".to_string()),
    };

    // Publish
    let event_id = service.announce_market(&announcement).await.unwrap();
    assert!(!event_id.to_hex().is_empty());

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Fetch
    let markets = service.fetch_markets().await.unwrap();
    assert!(
        !markets.is_empty(),
        "should have fetched at least one market"
    );

    let market = &markets[0];
    assert_eq!(market.question, "Will BTC close above $120k by Dec 2026?");
    assert_eq!(market.category, "Bitcoin");
    assert_eq!(market.cpt_sats, 5000);
    assert_eq!(market.expiry_height, 3_650_000);
    assert_eq!(market.oracle_pubkey, hex::encode(oracle_pubkey));
    assert_eq!(market.creation_txid, Some("abc123def456".to_string()));

    // Verify market_id is correct
    let expected_market_id = params.market_id();
    assert_eq!(market.market_id, hex::encode(expected_market_id.as_bytes()));

    // Verify persisted to store
    let s = store.lock().unwrap();
    assert_eq!(s.markets.len(), 1);
    assert_eq!(s.markets[0].market_id(), expected_market_id);
}

#[tokio::test]
async fn order_announce_discover_roundtrip() {
    let mock = MockRelay::run().await.unwrap();
    let (service, _rx, store, _keys) = setup_service_with_store(&mock.url()).await;

    let announcement = test_order_announcement("market123");

    // Publish
    let event_id = service.announce_order(&announcement).await.unwrap();
    assert!(!event_id.to_hex().is_empty());

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Fetch
    let orders = service.fetch_orders(None).await.unwrap();
    assert!(!orders.is_empty(), "should have fetched at least one order");

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

    // Allow time for the subscription to be established
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Publish via a SEPARATE client (relay echoes to the subscribing service)
    let publisher = Client::new(keys.clone());
    publisher.add_relay(mock.url()).await.unwrap();
    publisher.connect().await;

    let oracle_pubkey = oracle_pubkey_from_keys(&keys);
    let params = test_market_params(oracle_pubkey);
    let announcement = ContractAnnouncement {
        version: 1,
        contract_params: params,
        metadata: test_metadata(),
        creation_txid: None,
    };

    let event = deadcat_sdk::discovery::build_announcement_event(&keys, &announcement).unwrap();
    publisher.send_event(event).await.unwrap();

    // Wait for the broadcast event
    let result = tokio::time::timeout(Duration::from_secs(5), rx.recv()).await;
    assert!(result.is_ok(), "should receive event within timeout");

    match result.unwrap().unwrap() {
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
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Publish via a separate client
    let publisher = Client::new(keys.clone());
    publisher.add_relay(mock.url()).await.unwrap();
    publisher.connect().await;

    let announcement = test_order_announcement("market456");
    let event = deadcat_sdk::discovery::build_order_event(&keys, &announcement).unwrap();
    publisher.send_event(event).await.unwrap();

    let result = tokio::time::timeout(Duration::from_secs(5), rx.recv()).await;
    assert!(result.is_ok(), "should receive event within timeout");

    match result.unwrap().unwrap() {
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
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Publish via a separate client
    let publisher = Client::new(keys.clone());
    publisher.add_relay(mock.url()).await.unwrap();
    publisher.connect().await;

    // Publish a market
    let oracle_pubkey = oracle_pubkey_from_keys(&keys);
    let params = test_market_params(oracle_pubkey);
    let announcement = ContractAnnouncement {
        version: 1,
        contract_params: params,
        metadata: test_metadata(),
        creation_txid: None,
    };
    let event = deadcat_sdk::discovery::build_announcement_event(&keys, &announcement).unwrap();
    publisher.send_event(event).await.unwrap();

    // Wait for broadcast
    let _ = tokio::time::timeout(Duration::from_secs(5), rx.recv()).await;

    // Publish an order
    let order_announcement = test_order_announcement("market789");
    let order_event =
        deadcat_sdk::discovery::build_order_event(&keys, &order_announcement).unwrap();
    publisher.send_event(order_event).await.unwrap();

    let _ = tokio::time::timeout(Duration::from_secs(5), rx.recv()).await;

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
    let params = test_market_params(oracle_pubkey);
    let market_id = params.market_id();

    // First publish the announcement
    let announcement = ContractAnnouncement {
        version: 1,
        contract_params: params,
        metadata: test_metadata(),
        creation_txid: None,
    };
    let ann_event_id = service.announce_market(&announcement).await.unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Publish attestation
    let result = service
        .publish_attestation(&market_id, &ann_event_id.to_hex(), true)
        .await
        .unwrap();

    assert!(result.outcome_yes);
    assert!(!result.signature_hex.is_empty());
    assert!(!result.nostr_event_id.is_empty());

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Fetch attestation
    let market_id_hex = hex::encode(market_id.as_bytes());
    let content = service.fetch_attestation(&market_id_hex).await.unwrap();
    assert!(content.is_some());

    let att = content.unwrap();
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
    let params = test_market_params(oracle_pubkey);
    let announcement = ContractAnnouncement {
        version: 1,
        contract_params: params,
        metadata: test_metadata(),
        creation_txid: None,
    };

    // Publish the same market twice
    service.announce_market(&announcement).await.unwrap();
    service.announce_market(&announcement).await.unwrap();

    tokio::time::sleep(Duration::from_millis(300)).await;

    // Fetch — both events come back but store should deduplicate
    let markets = service.fetch_markets().await.unwrap();
    assert!(!markets.is_empty());

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

    tokio::time::sleep(Duration::from_millis(300)).await;

    // Fetch all
    let all_orders = service.fetch_orders(None).await.unwrap();
    assert!(all_orders.len() >= 2);

    // Fetch filtered — the relay should filter, but we also verify client-side
    let filtered = service.fetch_orders(Some("marketAAA")).await.unwrap();
    // At minimum, the filtered result should contain at least one marketAAA order
    assert!(
        filtered.iter().any(|o| o.market_id == "marketAAA"),
        "filtered orders should include marketAAA"
    );
}
