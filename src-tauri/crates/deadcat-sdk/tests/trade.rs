//! End-to-end trade routing integration tests.
//!
//! These tests exercise the full `quote_trade` → `execute_trade` flow on
//! `DeadcatNode`, combining a regtest Elements chain with a mock Nostr relay.
//!
//! Requires `ELEMENTSD_EXEC` to be set (regtest environment).

use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bitcoincore_rpc::{Auth, Client as RpcClient, RpcApi};
use deadcat_sdk::taproot::NUMS_KEY_BYTES;
use deadcat_sdk::testing::TestStore;
use deadcat_sdk::{
    AmmPoolParams, ContractMetadata, DeadcatNode, DiscoveredMarket, DiscoveryConfig,
    OrderDirection, PredictionMarketParams,
};
use deadcat_sdk::{LiquiditySource, TradeAmount, TradeDirection, TradeSide};
use lwk_test_util::{TEST_MNEMONIC, TestEnv, TestEnvBuilder, generate_mnemonic};
use lwk_wollet::elements::AssetId;
use nostr_relay_builder::prelude::*;
use nostr_sdk::prelude::*;
use tempfile::TempDir;

// ── Helpers ─────────────────────────────────────────────────────────────

fn hex_to_32(hex: &str) -> [u8; 32] {
    let bytes = hex::decode(hex).unwrap();
    <[u8; 32]>::try_from(bytes.as_slice()).unwrap()
}

fn market_to_params(market: &DiscoveredMarket) -> PredictionMarketParams {
    PredictionMarketParams {
        oracle_public_key: hex_to_32(&market.oracle_pubkey),
        collateral_asset_id: hex_to_32(&market.collateral_asset_id),
        yes_token_asset: hex_to_32(&market.yes_asset_id),
        no_token_asset: hex_to_32(&market.no_asset_id),
        yes_reissuance_token: hex_to_32(&market.yes_reissuance_token),
        no_reissuance_token: hex_to_32(&market.no_reissuance_token),
        collateral_per_token: market.cpt_sats,
        expiry_time: market.expiry_height,
    }
}

fn test_metadata() -> ContractMetadata {
    ContractMetadata {
        question: "Trade test market".to_string(),
        description: "Used for trade integration tests".to_string(),
        category: "Test".to_string(),
        resolution_source: "Test oracle".to_string(),
        starting_yes_price: 50,
    }
}

fn generate_oracle_keypair() -> ([u8; 32], lwk_wollet::elements::secp256k1_zkp::Keypair) {
    use lwk_wollet::elements::secp256k1_zkp::{Keypair, Secp256k1};
    let secp = Secp256k1::new();
    let keypair = Keypair::new(&secp, &mut rand::thread_rng());
    let pubkey_bytes: [u8; 32] = keypair.x_only_public_key().0.serialize();
    (pubkey_bytes, keypair)
}

/// Create an elementsd RPC client from TestEnv credentials.
fn elementsd_rpc(env: &TestEnv) -> RpcClient {
    let url = env.elements_rpc_url();
    let (user, pass) = env.elements_rpc_credentials();
    RpcClient::new(&url, Auth::UserPass(user, pass)).unwrap()
}

/// Issue an asset with reissuance capability via elementsd RPC.
///
/// Returns `(asset_id, reissuance_token_id, issuance_txid)`. Both assets
/// land in elementsd's default wallet and must be transferred to the SDK
/// wallet via [`elementsd_sendtoaddress`](TestEnv::elementsd_sendtoaddress).
///
/// Uses `blind=false` because the Elements PSET blinder (`blind_last`) does
/// not blind issuance amounts — they stay explicit in the final transaction.
/// When elementsd validates a reissuance it derives the expected RT via
/// `CalculateReissuanceToken(entropy, nAmount.IsCommitment())`.  With an
/// explicit reissuance amount `fConfidential = false`, so the original
/// issuance must also use `blind=false` for the RT asset IDs to match.
fn issue_with_reissuance(rpc: &RpcClient) -> (AssetId, AssetId, lwk_wollet::elements::Txid) {
    let r: serde_json::Value = rpc
        .call("issueasset", &[0.001.into(), 0.001.into(), false.into()])
        .unwrap();
    let asset = AssetId::from_str(r["asset"].as_str().unwrap()).unwrap();
    let token = AssetId::from_str(r["token"].as_str().unwrap()).unwrap();
    let txid = lwk_wollet::elements::Txid::from_str(r["txid"].as_str().unwrap()).unwrap();
    (asset, token, txid)
}

// ── Test fixture ────────────────────────────────────────────────────────

struct TradeTestFixture {
    env: TestEnv,
    node: DeadcatNode<TestStore>,
    _store: Arc<Mutex<TestStore>>,
    _keys: Keys,
    _temp_dir: TempDir,
    _mock: MockRelay,
    rpc: RpcClient,
}

impl TradeTestFixture {
    async fn new() -> Self {
        let env = TestEnvBuilder::from_env().with_electrum().build();
        let mock = MockRelay::run().await.unwrap();
        let rpc = elementsd_rpc(&env);

        let keys = Keys::generate();
        let store = Arc::new(Mutex::new(TestStore::default()));
        let config = DiscoveryConfig {
            relays: vec![mock.url()],
            ..Default::default()
        };
        let (node, _rx) = DeadcatNode::with_store(
            keys.clone(),
            deadcat_sdk::Network::LiquidRegtest,
            store.clone(),
            config,
        );

        let temp_dir = tempfile::tempdir().unwrap();
        node.unlock_wallet(TEST_MNEMONIC, &env.electrum_url(), temp_dir.path())
            .unwrap();

        Self {
            env,
            node,
            _store: store,
            _keys: keys,
            _temp_dir: temp_dir,
            _mock: mock,
            rpc,
        }
    }

    /// Fund the node wallet with `count` L-BTC UTXOs of `sats_each`.
    ///
    /// Mines a block every 20 sends to stay within elementsd's mempool
    /// ancestor-chain limit (default 25).
    async fn fund(&self, count: u32, sats_each: u64) {
        for i in 0..count {
            let addr = self.node.address(None).await.unwrap();
            self.env
                .elementsd_sendtoaddress(addr.address(), sats_each, None);
            if (i + 1) % 20 == 0 {
                self.env.elementsd_generate(1);
            }
        }
        self.mine_and_sync().await;
    }

    /// Mine a block and sync the wallet.
    async fn mine_and_sync(&self) {
        self.env.elementsd_generate(1);
        tokio::time::sleep(Duration::from_millis(500)).await;
        self.node.sync_wallet().await.unwrap();
    }

    /// Create a market on-chain + Nostr, issue token pairs, return params + market_id.
    async fn create_market_and_issue(
        &self,
        pairs: u64,
    ) -> (PredictionMarketParams, String, lwk_wollet::elements::Txid) {
        let (oracle_pubkey, _keypair) = generate_oracle_keypair();

        let (market, txid) = self
            .node
            .create_market(
                oracle_pubkey,
                10_000,  // collateral_per_token
                500_000, // expiry_time
                1_000,   // min_utxo_value
                500,     // fee_amount
                test_metadata(),
            )
            .await
            .unwrap();

        self.mine_and_sync().await;

        let params = market_to_params(&market);
        let market_id = market.market_id.clone();

        let _issuance = self
            .node
            .issue_tokens(params, txid, pairs, 500)
            .await
            .unwrap();
        self.mine_and_sync().await;

        (params, market_id, txid)
    }

    /// Issue LP tokens with reissuance, transfer the reissuance token to the
    /// SDK wallet, and return `(lp_asset_id_bytes, rt_asset_id_bytes, issuance_txid)`.
    async fn setup_lp_token(&self) -> ([u8; 32], [u8; 32], lwk_wollet::elements::Txid) {
        let (lp_asset, rt_asset, issuance_txid) = issue_with_reissuance(&self.rpc);

        // Confirm the issuance so elementsd can spend the reissuance token
        self.env.elementsd_generate(1);
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Transfer the reissuance token to the SDK wallet
        let addr = self.node.address(None).await.unwrap();
        self.env
            .elementsd_sendtoaddress(addr.address(), 1_000, Some(rt_asset));

        self.mine_and_sync().await;

        (
            lp_asset.into_inner().to_byte_array(),
            rt_asset.into_inner().to_byte_array(),
            issuance_txid,
        )
    }

    /// Create a second node using a fresh wallet, pointing at the same chain + relay.
    async fn create_second_node(&self) -> (DeadcatNode<TestStore>, TempDir) {
        let mnemonic = generate_mnemonic();
        let keys = Keys::generate();
        let store = Arc::new(Mutex::new(TestStore::default()));
        let config = DiscoveryConfig {
            relays: vec![self._mock.url()],
            ..Default::default()
        };
        let (node, _rx) =
            DeadcatNode::with_store(keys, deadcat_sdk::Network::LiquidRegtest, store, config);

        let temp_dir = tempfile::tempdir().unwrap();
        node.unlock_wallet(&mnemonic, &self.env.electrum_url(), temp_dir.path())
            .unwrap();

        (node, temp_dir)
    }

    /// Set up an AMM pool: issue LP tokens, construct pool params, create the
    /// pool on-chain, mine + sync.  Returns the `AmmPoolParams` for quoting.
    async fn setup_pool(
        &self,
        params: &PredictionMarketParams,
        lbtc_bytes: [u8; 32],
        market_id: &str,
    ) -> AmmPoolParams {
        let (lp_bytes, rt_bytes, lp_creation_txid) = self.setup_lp_token().await;

        let pool_params = AmmPoolParams {
            yes_asset_id: params.yes_token_asset,
            no_asset_id: params.no_token_asset,
            lbtc_asset_id: lbtc_bytes,
            lp_asset_id: lp_bytes,
            lp_reissuance_token_id: rt_bytes,
            fee_bps: 30,
            cosigner_pubkey: NUMS_KEY_BYTES,
        };

        let (_pool, _pool_txid) = self
            .node
            .create_pool(
                pool_params,
                50,      // initial_r_yes
                50,      // initial_r_no
                500_000, // initial_r_lbtc
                1_000,   // initial_issued_lp
                500,     // fee_amount
                market_id.to_string(),
                lp_creation_txid,
            )
            .await
            .unwrap();

        self.mine_and_sync().await;
        tokio::time::sleep(Duration::from_millis(500)).await;

        pool_params
    }

    /// Fund a second node's wallet with L-BTC.
    async fn fund_second_node(&self, node: &DeadcatNode<TestStore>, count: u32, sats_each: u64) {
        for _ in 0..count {
            let addr = node.address(None).await.unwrap();
            self.env
                .elementsd_sendtoaddress(addr.address(), sats_each, None);
        }
        self.env.elementsd_generate(1);
        tokio::time::sleep(Duration::from_millis(500)).await;
        node.sync_wallet().await.unwrap();
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

/// Buy YES tokens routed entirely through a single limit order.
#[tokio::test]
async fn trade_buy_yes_via_limit_order() {
    let f = TradeTestFixture::new().await;
    f.fund(25, 500_000).await;

    let (params, market_id, _txid) = f.create_market_and_issue(10).await;

    let lbtc = f.node.policy_asset().await.unwrap();
    let lbtc_bytes: [u8; 32] = lbtc.into_inner().to_byte_array();
    let yes_asset = AssetId::from_slice(&params.yes_token_asset).unwrap();

    let balance_before = f.node.balance().unwrap();
    assert_eq!(*balance_before.get(&yes_asset).unwrap_or(&0), 10);

    // Create a SellBase limit order: sell 5 YES tokens at 1000 sats/token
    let (_create_result, _event_id) = f
        .node
        .create_limit_order(
            params.yes_token_asset,
            lbtc_bytes,
            1_000, // price: sats per token
            5,     // order_amount: 5 YES tokens
            OrderDirection::SellBase,
            1,   // min_fill_lots
            1,   // min_remainder_lots
            0,   // order_index
            500, // fee_amount
            market_id.clone(),
            "sell-yes".to_string(),
        )
        .await
        .unwrap();

    f.mine_and_sync().await;
    // Give mock relay time to index the event
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Quote: buy YES tokens with 3000 sats (should get 3 tokens at 1000 sats/token)
    let quote = f
        .node
        .quote_trade(
            params,
            &market_id,
            TradeSide::Yes,
            TradeDirection::Buy,
            TradeAmount::ExactInput(3_000),
        )
        .await
        .unwrap();

    assert_eq!(quote.legs.len(), 1);
    assert!(matches!(
        &quote.legs[0].source,
        LiquiditySource::LimitOrder { .. }
    ));
    assert_eq!(quote.total_input, 3_000);
    assert_eq!(quote.total_output, 3); // 3000 / 1000 = 3 tokens

    // Execute
    let result = f.node.execute_trade(quote, 500, &market_id).await.unwrap();

    f.mine_and_sync().await;

    assert_eq!(result.num_orders_filled, 1);
    assert!(!result.pool_used);

    // Balance: 5 kept + 3 received = 8 YES tokens
    let balance_after = f.node.balance().unwrap();
    assert_eq!(*balance_after.get(&yes_asset).unwrap_or(&0), 8);
}

/// When one node fills a limit order, a second node's previously-obtained
/// quote (referencing now-spent order UTXOs) should fail to execute.
#[tokio::test]
async fn trade_stale_quote_fails_on_execute() {
    let f = TradeTestFixture::new().await;
    f.fund(25, 500_000).await;

    let (params, market_id, _txid) = f.create_market_and_issue(10).await;

    let lbtc = f.node.policy_asset().await.unwrap();
    let lbtc_bytes: [u8; 32] = lbtc.into_inner().to_byte_array();

    // Create a SellBase limit order: sell 5 YES tokens at 1000 sats/token
    let (_create_result, _event_id) = f
        .node
        .create_limit_order(
            params.yes_token_asset,
            lbtc_bytes,
            1_000, // price
            5,     // order_amount
            OrderDirection::SellBase,
            1,
            1,
            0,
            500,
            market_id.clone(),
            "sell-yes".to_string(),
        )
        .await
        .unwrap();

    f.mine_and_sync().await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Create a second node with its own wallet
    let (node_b, _temp_b) = f.create_second_node().await;
    f.fund_second_node(&node_b, 10, 500_000).await;

    // Node A quotes first
    let quote_a = f
        .node
        .quote_trade(
            params,
            &market_id,
            TradeSide::Yes,
            TradeDirection::Buy,
            TradeAmount::ExactInput(3_000),
        )
        .await
        .unwrap();

    // Node B quotes and executes — this spends the order's UTXOs
    let quote_b = node_b
        .quote_trade(
            params,
            &market_id,
            TradeSide::Yes,
            TradeDirection::Buy,
            TradeAmount::ExactInput(3_000),
        )
        .await
        .unwrap();

    let _result_b = node_b
        .execute_trade(quote_b, 500, &market_id)
        .await
        .unwrap();

    f.mine_and_sync().await;
    node_b.sync_wallet().await.unwrap();

    // Node A tries to execute its stale quote — order UTXOs have been spent
    let result_a = f.node.execute_trade(quote_a, 500, &market_id).await;
    assert!(
        result_a.is_err(),
        "stale quote should fail: order UTXOs were spent by node B"
    );
}

// ── AMM pool tests ──────────────────────────────────────────────────────

/// Buy YES tokens routed entirely through an AMM pool.
#[tokio::test]
async fn trade_buy_yes_via_amm_pool() {
    let f = TradeTestFixture::new().await;
    f.fund(15, 3_000_000).await;

    let (params, market_id, _txid) = f.create_market_and_issue(60).await;

    let lbtc = f.node.policy_asset().await.unwrap();
    let lbtc_bytes: [u8; 32] = lbtc.into_inner().to_byte_array();
    let yes_asset = AssetId::from_slice(&params.yes_token_asset).unwrap();

    f.setup_pool(&params, lbtc_bytes, &market_id).await;

    let balance_pre_trade = f.node.balance().unwrap();
    let yes_pre = *balance_pre_trade.get(&yes_asset).unwrap_or(&0);

    let quote = f
        .node
        .quote_trade(
            params,
            &market_id,
            TradeSide::Yes,
            TradeDirection::Buy,
            TradeAmount::ExactInput(50_000),
        )
        .await
        .unwrap();

    assert_eq!(quote.legs.len(), 1);
    assert!(matches!(
        &quote.legs[0].source,
        LiquiditySource::AmmPool { .. }
    ));
    assert_eq!(quote.total_input, 50_000);
    assert!(quote.total_output > 0, "should receive some YES tokens");

    let expected_output = quote.total_output;

    let result = f.node.execute_trade(quote, 500, &market_id).await.unwrap();

    f.mine_and_sync().await;

    assert!(result.pool_used);
    assert_eq!(result.num_orders_filled, 0);
    assert!(result.new_reserves.is_some());

    let balance_post = f.node.balance().unwrap();
    let yes_post = *balance_post.get(&yes_asset).unwrap_or(&0);
    assert_eq!(yes_post, yes_pre + expected_output);
}

/// A trade that routes through both a limit order AND the AMM pool.
#[tokio::test]
async fn trade_buy_yes_combined_pool_and_order() {
    let f = TradeTestFixture::new().await;
    f.fund(15, 3_000_000).await;

    let (params, market_id, _txid) = f.create_market_and_issue(60).await;

    let lbtc = f.node.policy_asset().await.unwrap();
    let lbtc_bytes: [u8; 32] = lbtc.into_inner().to_byte_array();
    let yes_asset = AssetId::from_slice(&params.yes_token_asset).unwrap();

    f.setup_pool(&params, lbtc_bytes, &market_id).await;

    let (_create_result, _event_id) = f
        .node
        .create_limit_order(
            params.yes_token_asset,
            lbtc_bytes,
            5_000,
            3,
            OrderDirection::SellBase,
            1,
            1,
            0,
            500,
            market_id.clone(),
            "sell-yes".to_string(),
        )
        .await
        .unwrap();

    f.mine_and_sync().await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    let yes_pre = *f.node.balance().unwrap().get(&yes_asset).unwrap_or(&0);

    let quote = f
        .node
        .quote_trade(
            params,
            &market_id,
            TradeSide::Yes,
            TradeDirection::Buy,
            TradeAmount::ExactInput(50_000),
        )
        .await
        .unwrap();

    assert_eq!(quote.legs.len(), 2);
    let has_order_leg = quote
        .legs
        .iter()
        .any(|l| matches!(&l.source, LiquiditySource::LimitOrder { .. }));
    let has_pool_leg = quote
        .legs
        .iter()
        .any(|l| matches!(&l.source, LiquiditySource::AmmPool { .. }));
    assert!(has_order_leg, "should route through limit order");
    assert!(has_pool_leg, "should route remainder through pool");
    assert!(
        quote.total_output > 3,
        "should get more than just the order"
    );

    let expected_output = quote.total_output;

    let result = f.node.execute_trade(quote, 500, &market_id).await.unwrap();

    f.mine_and_sync().await;

    assert_eq!(result.num_orders_filled, 1);
    assert!(result.pool_used);

    let yes_post = *f.node.balance().unwrap().get(&yes_asset).unwrap_or(&0);
    assert_eq!(yes_post, yes_pre + expected_output);
}

/// Sell YES tokens routed through a SellQuote limit order.
///
/// The maker creates a SellQuote order (offering L-BTC, wanting YES tokens).
/// The taker sells YES tokens and receives L-BTC.
#[tokio::test]
async fn trade_sell_yes_via_limit_order() {
    let f = TradeTestFixture::new().await;
    f.fund(25, 500_000).await;

    let (params, market_id, _txid) = f.create_market_and_issue(10).await;

    let lbtc = f.node.policy_asset().await.unwrap();
    let lbtc_bytes: [u8; 32] = lbtc.into_inner().to_byte_array();
    let yes_asset = AssetId::from_slice(&params.yes_token_asset).unwrap();

    let balance_before = f.node.balance().unwrap();
    let yes_before = *balance_before.get(&yes_asset).unwrap_or(&0);
    assert_eq!(yes_before, 10);

    // Create a SellQuote limit order: maker offers 5000 sats at 1000 sats/token
    // (maker is buying up to 5 YES tokens)
    let (_create_result, _event_id) = f
        .node
        .create_limit_order(
            params.yes_token_asset,
            lbtc_bytes,
            1_000, // price: sats per token
            5_000, // order_amount: 5000 sats locked (buys up to 5 tokens)
            OrderDirection::SellQuote,
            1,   // min_fill_lots
            1,   // min_remainder_lots
            0,   // order_index
            500, // fee_amount
            market_id.clone(),
            "buy-yes".to_string(),
        )
        .await
        .unwrap();

    f.mine_and_sync().await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Quote: sell 3 YES tokens → should receive 3000 sats
    let quote = f
        .node
        .quote_trade(
            params,
            &market_id,
            TradeSide::Yes,
            TradeDirection::Sell,
            TradeAmount::ExactInput(3),
        )
        .await
        .unwrap();

    assert_eq!(quote.legs.len(), 1);
    assert!(matches!(
        &quote.legs[0].source,
        LiquiditySource::LimitOrder { .. }
    ));
    assert_eq!(quote.total_input, 3); // 3 tokens sent
    assert_eq!(quote.total_output, 3_000); // 3 * 1000 = 3000 sats received

    let result = f.node.execute_trade(quote, 500, &market_id).await.unwrap();

    f.mine_and_sync().await;

    assert_eq!(result.num_orders_filled, 1);
    assert!(!result.pool_used);

    let balance_after = f.node.balance().unwrap();
    let yes_after = *balance_after.get(&yes_asset).unwrap_or(&0);
    // SellQuote order locks L-BTC (not YES), so taker had 10 YES and sold 3.
    assert!(
        yes_after < yes_before,
        "should have fewer YES tokens after selling"
    );
}

/// Sell YES tokens routed entirely through an AMM pool.
#[tokio::test]
async fn trade_sell_yes_via_amm_pool() {
    let f = TradeTestFixture::new().await;
    f.fund(15, 3_000_000).await;

    let (params, market_id, _txid) = f.create_market_and_issue(60).await;

    let lbtc = f.node.policy_asset().await.unwrap();
    let lbtc_bytes: [u8; 32] = lbtc.into_inner().to_byte_array();
    let yes_asset = AssetId::from_slice(&params.yes_token_asset).unwrap();

    f.setup_pool(&params, lbtc_bytes, &market_id).await;

    let balance_pre = f.node.balance().unwrap();
    let yes_pre = *balance_pre.get(&yes_asset).unwrap_or(&0);
    let lbtc_pre = *balance_pre.get(&lbtc).unwrap_or(&0);

    // Sell 5 YES tokens through the AMM pool
    let quote = f
        .node
        .quote_trade(
            params,
            &market_id,
            TradeSide::Yes,
            TradeDirection::Sell,
            TradeAmount::ExactInput(5),
        )
        .await
        .unwrap();

    assert_eq!(quote.legs.len(), 1);
    assert!(matches!(
        &quote.legs[0].source,
        LiquiditySource::AmmPool { .. }
    ));
    assert_eq!(quote.total_input, 5);
    assert!(quote.total_output > 0, "should receive some L-BTC");

    let expected_lbtc_output = quote.total_output;

    let result = f.node.execute_trade(quote, 500, &market_id).await.unwrap();

    f.mine_and_sync().await;

    assert!(result.pool_used);
    assert_eq!(result.num_orders_filled, 0);
    assert!(result.new_reserves.is_some());

    let balance_post = f.node.balance().unwrap();
    let yes_post = *balance_post.get(&yes_asset).unwrap_or(&0);
    let lbtc_post = *balance_post.get(&lbtc).unwrap_or(&0);

    assert_eq!(yes_post, yes_pre - 5, "should have 5 fewer YES tokens");
    // L-BTC should increase by the expected output minus fees
    assert!(
        lbtc_post > lbtc_pre,
        "L-BTC balance should increase: pre={lbtc_pre}, post={lbtc_post}, expected_gain={expected_lbtc_output}"
    );
}

/// Buy NO tokens routed entirely through an AMM pool.
#[tokio::test]
async fn trade_buy_no_via_amm_pool() {
    let f = TradeTestFixture::new().await;
    f.fund(15, 3_000_000).await;

    let (params, market_id, _txid) = f.create_market_and_issue(60).await;

    let lbtc = f.node.policy_asset().await.unwrap();
    let lbtc_bytes: [u8; 32] = lbtc.into_inner().to_byte_array();
    let no_asset = AssetId::from_slice(&params.no_token_asset).unwrap();

    f.setup_pool(&params, lbtc_bytes, &market_id).await;

    let balance_pre = f.node.balance().unwrap();
    let no_pre = *balance_pre.get(&no_asset).unwrap_or(&0);

    // Buy NO tokens with 50,000 sats
    let quote = f
        .node
        .quote_trade(
            params,
            &market_id,
            TradeSide::No,
            TradeDirection::Buy,
            TradeAmount::ExactInput(50_000),
        )
        .await
        .unwrap();

    assert_eq!(quote.legs.len(), 1);
    assert!(matches!(
        &quote.legs[0].source,
        LiquiditySource::AmmPool { .. }
    ));
    assert_eq!(quote.total_input, 50_000);
    assert!(quote.total_output > 0, "should receive some NO tokens");

    let expected_output = quote.total_output;

    let result = f.node.execute_trade(quote, 500, &market_id).await.unwrap();

    f.mine_and_sync().await;

    assert!(result.pool_used);
    assert_eq!(result.num_orders_filled, 0);
    assert!(result.new_reserves.is_some());

    let balance_post = f.node.balance().unwrap();
    let no_post = *balance_post.get(&no_asset).unwrap_or(&0);
    assert_eq!(no_post, no_pre + expected_output);
}
