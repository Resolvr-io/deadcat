use std::sync::{Arc, Mutex};
use std::time::Duration;

use deadcat_sdk::announcement::{ContractAnnouncement, ContractMetadata};
use deadcat_sdk::discovery::{DiscoveryConfig, DiscoveryEvent, OrderAnnouncement};
use deadcat_sdk::maker_order::params::{MakerOrderParams, OrderDirection};
use deadcat_sdk::node::DeadcatNode;
use deadcat_sdk::params::ContractParams;
use deadcat_sdk::taproot::NUMS_KEY_BYTES;
use deadcat_sdk::testing::TestStore;
use deadcat_sdk::{NodeError, TradeAmount, TradeDirection, TradeSide};
use nostr_relay_builder::prelude::*;
use nostr_sdk::prelude::*;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn test_metadata() -> ContractMetadata {
    ContractMetadata {
        question: "Will BTC close above $120k by Dec 2026?".to_string(),
        description: "Resolved using median close basket.".to_string(),
        category: "Bitcoin".to_string(),
        resolution_source: "Exchange close basket".to_string(),
        starting_yes_price: 57,
    }
}

fn test_params(oracle_pubkey: [u8; 32]) -> ContractParams {
    ContractParams {
        oracle_public_key: oracle_pubkey,
        collateral_asset_id: [0xbb; 32],
        yes_token_asset: [0x01; 32],
        no_token_asset: [0x02; 32],
        yes_reissuance_token: [0x03; 32],
        no_reissuance_token: [0x04; 32],
        collateral_per_token: 5000,
        expiry_time: 3_650_000,
    }
}

fn oracle_pubkey_from_keys(keys: &Keys) -> [u8; 32] {
    let h = keys.public_key().to_hex();
    let b = hex::decode(&h).unwrap();
    <[u8; 32]>::try_from(b.as_slice()).unwrap()
}

fn test_order_announcement(market_id: &str) -> OrderAnnouncement {
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
        market_id: market_id.to_string(),
        maker_base_pubkey: hex::encode([0xaa; 32]),
        order_nonce: hex::encode([0x11; 32]),
        covenant_address: "tex1qtest".to_string(),
        offered_amount: 100,
        direction_label: "sell-yes".to_string(),
    }
}

async fn setup_node_with_store(
    mock_url: &str,
) -> (
    DeadcatNode<TestStore>,
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
    let (node, rx) = DeadcatNode::with_store(
        keys.clone(),
        deadcat_sdk::Network::LiquidTestnet,
        store.clone(),
        config,
    );
    (node, rx, store, keys)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn node_wallet_lifecycle() {
    let mock = MockRelay::run().await.unwrap();
    let (node, _rx, _store, _keys) = setup_node_with_store(&mock.url()).await;

    // Initially locked
    assert!(!node.is_wallet_unlocked());

    // Lock when already locked is a no-op
    node.lock_wallet();
    assert!(!node.is_wallet_unlocked());

    // SDK-dependent methods should fail when locked
    let result = node.sync_wallet().await;
    assert!(result.is_err());
    match result.unwrap_err() {
        deadcat_sdk::NodeError::WalletLocked => {}
        other => panic!("expected WalletLocked, got {other}"),
    }
}

#[tokio::test]
async fn node_announce_and_fetch_market() {
    let mock = MockRelay::run().await.unwrap();
    let (node, _rx, store, keys) = setup_node_with_store(&mock.url()).await;

    let oracle_pubkey = oracle_pubkey_from_keys(&keys);
    let params = test_params(oracle_pubkey);
    let announcement = ContractAnnouncement {
        version: 1,
        contract_params: params,
        metadata: test_metadata(),
        creation_txid: Some("abc123def456".to_string()),
    };

    // Announce via node (Nostr-only, no on-chain)
    let event_id = node.announce_market(&announcement).await.unwrap();
    assert!(!event_id.to_hex().is_empty());

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Fetch back
    let markets = node.fetch_markets().await.unwrap();
    assert!(
        !markets.is_empty(),
        "should have fetched at least one market"
    );

    let market = &markets[0];
    assert_eq!(market.question, "Will BTC close above $120k by Dec 2026?");
    assert_eq!(market.category, "Bitcoin");
    assert_eq!(market.cpt_sats, 5000);
    assert_eq!(market.oracle_pubkey, hex::encode(oracle_pubkey));

    // Verify persisted to store
    let s = store.lock().unwrap();
    assert_eq!(s.markets.len(), 1);
}

#[tokio::test]
async fn node_announce_and_fetch_order() {
    let mock = MockRelay::run().await.unwrap();
    let (node, _rx, store, _keys) = setup_node_with_store(&mock.url()).await;

    let announcement = test_order_announcement("market123");

    // Announce via discovery (delegated through node)
    let event_id = node
        .discovery()
        .announce_order(&announcement)
        .await
        .unwrap();
    assert!(!event_id.to_hex().is_empty());

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Fetch back via node
    let orders = node.fetch_orders(None).await.unwrap();
    assert!(!orders.is_empty(), "should have fetched at least one order");

    let order = &orders[0];
    assert_eq!(order.market_id, "market123");
    assert_eq!(order.price, 50_000);

    // Verify persisted
    let s = store.lock().unwrap();
    assert_eq!(s.orders.len(), 1);
}

#[tokio::test]
async fn node_attestation() {
    let mock = MockRelay::run().await.unwrap();
    let (node, _rx, _store, keys) = setup_node_with_store(&mock.url()).await;

    let oracle_pubkey = oracle_pubkey_from_keys(&keys);
    let params = test_params(oracle_pubkey);
    let market_id = params.market_id();

    // First publish the announcement
    let announcement = ContractAnnouncement {
        version: 1,
        contract_params: params,
        metadata: test_metadata(),
        creation_txid: None,
    };
    let ann_event_id = node.announce_market(&announcement).await.unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Attest
    let result = node
        .attest_market(&market_id, &ann_event_id.to_hex(), true)
        .await
        .unwrap();
    assert!(result.outcome_yes);
    assert!(!result.signature_hex.is_empty());

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Fetch attestation
    let market_id_hex = hex::encode(market_id.as_bytes());
    let content = node.fetch_attestation(&market_id_hex).await.unwrap();
    assert!(content.is_some());

    let att = content.unwrap();
    assert_eq!(att.market_id, market_id_hex);
    assert!(att.outcome_yes);
}

#[tokio::test]
async fn node_subscription_delivers_events() {
    let mock = MockRelay::run().await.unwrap();
    let (node, mut rx, _store, keys) = setup_node_with_store(&mock.url()).await;

    // Start subscription loop
    let handle = node.start_subscription().await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Publish via a SEPARATE client
    let publisher = Client::new(keys.clone());
    publisher.add_relay(mock.url()).await.unwrap();
    publisher.connect().await;

    let oracle_pubkey = oracle_pubkey_from_keys(&keys);
    let params = test_params(oracle_pubkey);
    let announcement = ContractAnnouncement {
        version: 1,
        contract_params: params,
        metadata: test_metadata(),
        creation_txid: None,
    };

    let event = deadcat_sdk::discovery::build_announcement_event(&keys, &announcement).unwrap();
    publisher.send_event(event).await.unwrap();

    // Wait for broadcast event
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
async fn node_discovery_delegates_to_service() {
    let mock = MockRelay::run().await.unwrap();
    let (node, _rx, _store, keys) = setup_node_with_store(&mock.url()).await;

    // Verify the discovery service reference is accessible
    let service = node.discovery();
    assert_eq!(service.keys().public_key(), keys.public_key());

    // Verify accessors
    assert_eq!(node.keys().public_key(), keys.public_key());
    assert_eq!(node.network(), deadcat_sdk::Network::LiquidTestnet);
}

#[tokio::test]
async fn node_subscribe_returns_receiver() {
    let mock = MockRelay::run().await.unwrap();
    let (node, _rx, _store, _keys) = setup_node_with_store(&mock.url()).await;

    // Get an additional receiver
    let _rx2 = node.subscribe();
    // No panic — receiver is valid
}

// ---------------------------------------------------------------------------
// Trade routing: quote_trade
// ---------------------------------------------------------------------------

#[tokio::test]
async fn quote_trade_exact_output_unsupported() {
    let mock = MockRelay::run().await.unwrap();
    let (node, _rx, _store, keys) = setup_node_with_store(&mock.url()).await;

    let oracle_pubkey = oracle_pubkey_from_keys(&keys);
    let params = test_params(oracle_pubkey);

    let result = node
        .quote_trade(
            params,
            "mkt1",
            TradeSide::Yes,
            TradeDirection::Buy,
            TradeAmount::ExactOutput(1000),
        )
        .await;

    match result {
        Err(NodeError::Sdk(deadcat_sdk::Error::ExactOutputUnsupported)) => {}
        other => panic!("expected ExactOutputUnsupported, got {other:?}"),
    }
}

#[tokio::test]
async fn quote_trade_no_liquidity() {
    let mock = MockRelay::run().await.unwrap();
    let (node, _rx, _store, keys) = setup_node_with_store(&mock.url()).await;

    // Unlock wallet with a valid mnemonic so with_sdk doesn't return WalletLocked.
    // The electrum URL won't be contacted — no pool/order UTXOs to scan.
    let datadir = tempfile::tempdir().unwrap();
    let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    node.unlock_wallet(mnemonic, "tcp://127.0.0.1:1", datadir.path())
        .unwrap();

    let oracle_pubkey = oracle_pubkey_from_keys(&keys);
    let params = test_params(oracle_pubkey);

    // Mock relay has no pools or orders — should get NoLiquidity
    let result = node
        .quote_trade(
            params,
            "mkt1",
            TradeSide::Yes,
            TradeDirection::Buy,
            TradeAmount::ExactInput(10_000),
        )
        .await;

    match result {
        Err(NodeError::Sdk(deadcat_sdk::Error::NoLiquidity)) => {}
        other => panic!("expected NoLiquidity, got {other:?}"),
    }
}

#[tokio::test]
async fn quote_trade_requires_unlocked_wallet() {
    let mock = MockRelay::run().await.unwrap();
    let (node, _rx, _store, keys) = setup_node_with_store(&mock.url()).await;

    let oracle_pubkey = oracle_pubkey_from_keys(&keys);
    let params = test_params(oracle_pubkey);

    // Wallet is locked — should get WalletLocked
    let result = node
        .quote_trade(
            params,
            "mkt1",
            TradeSide::Yes,
            TradeDirection::Buy,
            TradeAmount::ExactInput(10_000),
        )
        .await;

    match result {
        Err(NodeError::WalletLocked) => {}
        other => panic!("expected WalletLocked, got {other:?}"),
    }
}
