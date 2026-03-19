use std::sync::{Arc, Mutex};
use std::time::Duration;

use deadcat_sdk::PoolParams;
use deadcat_sdk::testing::{
    TestStore, oracle_pubkey_from_keys, test_market_announcement, test_market_params,
    test_order_announcement,
};
use deadcat_sdk::{
    CompiledLmsrPool, DeadcatNode, DiscoveryConfig, DiscoveryEvent, LmsrInitialOutpoint,
    LmsrPoolId, LmsrPoolIdInput, LmsrPoolParams, PoolAnnouncement, PoolReserves,
};
use deadcat_sdk::{NodeError, TradeAmount, TradeDirection, TradeSide};
use nostr_relay_builder::prelude::*;
use nostr_sdk::prelude::*;

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
        network_tag: "liquid-testnet".to_string(),
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

fn parse_lmsr_outpoint(outpoint: &str) -> LmsrInitialOutpoint {
    let (txid, vout) = outpoint
        .split_once(':')
        .expect("test outpoint must contain ':'");
    let txid: [u8; 32] = hex::decode(txid)
        .expect("test outpoint txid must be hex")
        .try_into()
        .expect("test outpoint txid must be 32 bytes");
    let vout = vout.parse::<u32>().expect("test outpoint vout must be u32");
    LmsrInitialOutpoint { txid, vout }
}

fn derive_test_lmsr_pool_id(announcement: &PoolAnnouncement) -> String {
    let params = LmsrPoolParams {
        yes_asset_id: announcement.params.yes_asset_id,
        no_asset_id: announcement.params.no_asset_id,
        collateral_asset_id: announcement.params.lbtc_asset_id,
        lmsr_table_root: hex::decode(&announcement.lmsr_table_root)
            .expect("table root hex")
            .try_into()
            .expect("table root len"),
        table_depth: announcement.table_depth,
        q_step_lots: announcement.q_step_lots,
        s_bias: announcement.s_bias,
        s_max_index: announcement.s_max_index,
        half_payout_sats: announcement.half_payout_sats,
        fee_bps: announcement.params.fee_bps,
        min_r_yes: announcement.params.min_r_yes,
        min_r_no: announcement.params.min_r_no,
        min_r_collateral: announcement.params.min_r_collateral,
        cosigner_pubkey: announcement.params.cosigner_pubkey,
    };
    let creation_txid: [u8; 32] = hex::decode(&announcement.creation_txid)
        .expect("creation txid hex")
        .try_into()
        .expect("creation txid len");
    let contract = CompiledLmsrPool::new(params).expect("compile test lmsr pool");
    let initial_yes_outpoint = parse_lmsr_outpoint(&announcement.initial_reserve_outpoints[0]);
    let initial_no_outpoint = parse_lmsr_outpoint(&announcement.initial_reserve_outpoints[1]);
    let initial_collateral_outpoint =
        parse_lmsr_outpoint(&announcement.initial_reserve_outpoints[2]);
    LmsrPoolId::derive_v1(&LmsrPoolIdInput {
        chain_genesis_hash: deadcat_sdk::Network::LiquidTestnet.genesis_hash(),
        params,
        covenant_cmr: contract.primary_cmr().to_byte_array(),
        creation_txid,
        initial_yes_outpoint,
        initial_no_outpoint,
        initial_collateral_outpoint,
    })
    .expect("derive test lmsr pool id")
    .to_hex()
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
    let (announcement, _params) = test_market_announcement(oracle_pubkey, 0x11);

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
    let (announcement, params) = test_market_announcement(oracle_pubkey, 0x22);
    let market_id = params.market_id();

    // First publish the announcement
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
    let (announcement, _params) = test_market_announcement(oracle_pubkey, 0x33);

    let event =
        deadcat_sdk::build_announcement_event(&keys, &announcement, "liquid-testnet").unwrap();
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
    let params = test_market_params(oracle_pubkey);

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
    let params = test_market_params(oracle_pubkey);

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
async fn quote_trade_requires_chain_access_for_lmsr_pool_scan() {
    let mock = MockRelay::run().await.unwrap();
    let (node, _rx, _store, keys) = setup_node_with_store(&mock.url()).await;

    // Unlock wallet with a valid mnemonic so with_sdk doesn't return WalletLocked.
    let datadir = tempfile::tempdir().unwrap();
    let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    node.unlock_wallet(mnemonic, "tcp://127.0.0.1:1", datadir.path())
        .unwrap();

    let table_values = vec![2_000, 2_010, 2_025, 2_045, 2_070, 2_100, 2_135, 2_175];
    let table_root = deadcat_sdk::lmsr_table_root(&table_values).unwrap();
    let creation_txid = hex::encode([0x88; 32]);
    let oracle_pubkey = oracle_pubkey_from_keys(&keys);
    let params = test_market_params(oracle_pubkey);
    let market_id = params.market_id().to_string();

    let mut announcement = PoolAnnouncement {
        version: 2,
        params: PoolParams {
            yes_asset_id: [0x01; 32],
            no_asset_id: [0x02; 32],
            lbtc_asset_id: [0x03; 32],
            fee_bps: 30,
            min_r_yes: 1,
            min_r_no: 1,
            min_r_collateral: 1,
            cosigner_pubkey: [0x06; 32],
        },
        market_id: market_id.clone(),
        reserves: PoolReserves {
            r_yes: 500,
            r_no: 500,
            r_lbtc: 1_000,
        },
        creation_txid: creation_txid.clone(),
        lmsr_pool_id: String::new(),
        lmsr_table_root: hex::encode(table_root),
        table_depth: 3,
        q_step_lots: 10,
        s_bias: 4,
        s_max_index: 7,
        half_payout_sats: 100,
        current_s_index: 4,
        initial_reserve_outpoints: vec![
            format!("{creation_txid}:0"),
            format!("{creation_txid}:1"),
            format!("{creation_txid}:2"),
        ],
        witness_schema_version: "DEADCAT/LMSR_WITNESS_SCHEMA_V2".to_string(),
        table_manifest_hash: Some(hex::encode([0xaa; 32])),
        lmsr_table_values: Some(table_values),
    };
    announcement.lmsr_pool_id = derive_test_lmsr_pool_id(&announcement);

    node.announce_pool(&announcement).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    let result = node
        .quote_trade(
            params,
            &market_id,
            TradeSide::Yes,
            TradeDirection::Buy,
            TradeAmount::ExactInput(10_000),
        )
        .await;

    match result {
        Err(NodeError::Sdk(deadcat_sdk::Error::Electrum(msg))) => {
            assert!(
                msg.contains("Connection refused")
                    || msg.contains("failed")
                    || msg.contains("error")
            );
        }
        other => panic!("expected electrum error while scanning LMSR reserves, got {other:?}"),
    }
}

#[tokio::test]
async fn quote_trade_ignores_malformed_lmsr_pool_payload() {
    let mock = MockRelay::run().await.unwrap();
    let (node, _rx, _store, keys) = setup_node_with_store(&mock.url()).await;

    // Unlock wallet with a valid mnemonic so with_sdk doesn't return WalletLocked.
    let datadir = tempfile::tempdir().unwrap();
    let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    node.unlock_wallet(mnemonic, "tcp://127.0.0.1:1", datadir.path())
        .unwrap();

    let oracle_pubkey = oracle_pubkey_from_keys(&keys);
    let params = test_market_params(oracle_pubkey);
    let market_id = params.market_id().to_string();

    // Deliberately malformed LMSR payload: invalid reserve outpoint encoding.
    let announcement = PoolAnnouncement {
        version: 2,
        params: PoolParams {
            yes_asset_id: [0x01; 32],
            no_asset_id: [0x02; 32],
            lbtc_asset_id: [0x03; 32],
            fee_bps: 30,
            min_r_yes: 1,
            min_r_no: 1,
            min_r_collateral: 1,
            cosigner_pubkey: [0x06; 32],
        },
        market_id: market_id.clone(),
        reserves: PoolReserves {
            r_yes: 500,
            r_no: 500,
            r_lbtc: 1_000,
        },
        creation_txid: hex::encode([0x88; 32]),
        lmsr_pool_id: hex::encode([0x22; 32]),
        lmsr_table_root: hex::encode([0x77; 32]),
        table_depth: 3,
        q_step_lots: 10,
        s_bias: 4,
        s_max_index: 7,
        half_payout_sats: 100,
        current_s_index: 4,
        initial_reserve_outpoints: vec![
            "not-an-outpoint".to_string(),
            format!("{}:1", hex::encode([0xb2; 32])),
            format!("{}:2", hex::encode([0xb3; 32])),
        ],
        witness_schema_version: "DEADCAT/LMSR_WITNESS_SCHEMA_V2".to_string(),
        table_manifest_hash: None,
        lmsr_table_values: None,
    };

    let announce_result = node.announce_pool(&announcement).await;
    match announce_result {
        Err(NodeError::Discovery(msg)) => {
            assert!(msg.contains("initial_reserve_outpoints[0]"));
        }
        other => panic!("expected malformed pool announcement rejection, got {other:?}"),
    }

    let result = node
        .quote_trade(
            params,
            &market_id,
            TradeSide::Yes,
            TradeDirection::Buy,
            TradeAmount::ExactInput(10_000),
        )
        .await;

    match result {
        Err(NodeError::Sdk(deadcat_sdk::Error::NoLiquidity)) => {}
        other => panic!("expected NoLiquidity after malformed pool rejection, got {other:?}"),
    }
}

#[tokio::test]
async fn quote_trade_rejects_lmsr_pool_without_table_values() {
    let mock = MockRelay::run().await.unwrap();
    let (node, _rx, _store, keys) = setup_node_with_store(&mock.url()).await;

    let datadir = tempfile::tempdir().unwrap();
    let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    node.unlock_wallet(mnemonic, "tcp://127.0.0.1:1", datadir.path())
        .unwrap();

    let table_values = vec![2_000, 2_010, 2_025, 2_045, 2_070, 2_100, 2_135, 2_175];
    let table_root = deadcat_sdk::lmsr_table_root(&table_values).unwrap();
    let creation_txid = hex::encode([0x88; 32]);
    let oracle_pubkey = oracle_pubkey_from_keys(&keys);
    let params = test_market_params(oracle_pubkey);
    let market_id = params.market_id().to_string();

    let mut announcement = PoolAnnouncement {
        version: 2,
        params: PoolParams {
            yes_asset_id: [0x01; 32],
            no_asset_id: [0x02; 32],
            lbtc_asset_id: [0x03; 32],
            fee_bps: 30,
            min_r_yes: 1,
            min_r_no: 1,
            min_r_collateral: 1,
            cosigner_pubkey: [0x06; 32],
        },
        market_id: market_id.clone(),
        reserves: PoolReserves {
            r_yes: 500,
            r_no: 500,
            r_lbtc: 1_000,
        },
        creation_txid: creation_txid.clone(),
        lmsr_pool_id: String::new(),
        lmsr_table_root: hex::encode(table_root),
        table_depth: 3,
        q_step_lots: 10,
        s_bias: 4,
        s_max_index: 7,
        half_payout_sats: 100,
        current_s_index: 4,
        initial_reserve_outpoints: vec![
            format!("{creation_txid}:0"),
            format!("{creation_txid}:1"),
            format!("{creation_txid}:2"),
        ],
        witness_schema_version: "DEADCAT/LMSR_WITNESS_SCHEMA_V2".to_string(),
        table_manifest_hash: Some(hex::encode([0xaa; 32])),
        lmsr_table_values: None,
    };
    announcement.lmsr_pool_id = derive_test_lmsr_pool_id(&announcement);

    node.announce_pool(&announcement).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    let result = node
        .quote_trade(
            params,
            &market_id,
            TradeSide::Yes,
            TradeDirection::Buy,
            TradeAmount::ExactInput(10_000),
        )
        .await;

    match result {
        Err(NodeError::Sdk(deadcat_sdk::Error::TradeRouting(msg))) => {
            assert!(msg.contains("lmsr_table_values"));
        }
        other => panic!("expected missing table-values error, got {other:?}"),
    }
}

#[tokio::test]
async fn quote_trade_requires_unlocked_wallet() {
    let mock = MockRelay::run().await.unwrap();
    let (node, _rx, _store, keys) = setup_node_with_store(&mock.url()).await;

    let oracle_pubkey = oracle_pubkey_from_keys(&keys);
    let params = test_market_params(oracle_pubkey);

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
