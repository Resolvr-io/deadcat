use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use bitcoincore_rpc::RpcApi;
use deadcat_sdk::lwk_wollet::elements::AssetId;
use deadcat_sdk::lwk_wollet::elements::confidential::{Asset, Value as ConfValue};
use deadcat_sdk::taproot::NUMS_KEY_BYTES;
use deadcat_sdk::testing::TestStore;
use deadcat_sdk::{
    CreateLmsrPoolRequest, DeadcatNode, DiscoveryConfig, Error, LiquiditySource, LmsrPoolId,
    LmsrPoolParams, Network, NodeError, PoolReserves, PredictionMarketParams, TradeAmount,
    TradeDirection, TradeSide,
};
use lwk_test_util::{TEST_MNEMONIC, TestEnv, TestEnvBuilder, regtest_policy_asset};
use nostr_relay_builder::prelude::MockRelay;
use nostr_sdk::Keys;
use tempfile::TempDir;

fn asset_bytes(asset: AssetId) -> [u8; 32] {
    asset.into_inner().to_byte_array()
}

async fn mine_and_sync(node: &DeadcatNode<TestStore>, env: &TestEnv, blocks: u32) {
    env.elementsd_generate(blocks);
    tokio::time::sleep(Duration::from_millis(900)).await;
    node.sync_wallet().await.expect("sync wallet");
}

fn hold_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    match LOCK.get_or_init(|| Mutex::new(())).lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

async fn quote_with_retry(
    node: &DeadcatNode<TestStore>,
    market_params: PredictionMarketParams,
    market_id: &str,
    side: TradeSide,
    direction: TradeDirection,
    amount: u64,
) -> deadcat_sdk::TradeQuote {
    let mut last_error = None;
    for _ in 0..16 {
        match node
            .quote_trade(
                market_params,
                market_id,
                side,
                direction,
                TradeAmount::ExactInput(amount),
            )
            .await
        {
            Ok(quote) => return quote,
            Err(NodeError::Sdk(Error::Electrum(msg))) if msg.contains("missing transaction") => {
                last_error = Some(msg);
                let _ = node.sync_wallet().await;
                tokio::time::sleep(Duration::from_millis(1_000)).await;
            }
            Err(err) => panic!("quote failed: {err:?}"),
        }
    }
    panic!(
        "quote did not succeed after retries; last electrum error: {:?}",
        last_error
    );
}

struct Fixture {
    env: TestEnv,
    _relay: MockRelay,
    _wallet_dir: TempDir,
    node: DeadcatNode<TestStore>,
    store: Arc<Mutex<TestStore>>,
}

impl Fixture {
    async fn new() -> Self {
        let env = TestEnvBuilder::from_env().with_electrum().build();
        let relay = MockRelay::run().await.expect("start mock relay");
        let keys = Keys::generate();
        let store = Arc::new(Mutex::new(TestStore::default()));
        let config = DiscoveryConfig {
            relays: vec![relay.url()],
            network_tag: "liquid-regtest".to_string(),
            ..Default::default()
        };
        let (node, _rx) =
            DeadcatNode::with_store(keys, Network::LiquidRegtest, store.clone(), config);
        let wallet_dir = tempfile::tempdir().expect("wallet tempdir");
        node.unlock_wallet(TEST_MNEMONIC, &env.electrum_url(), wallet_dir.path())
            .expect("unlock wallet");

        // Set the actual chain genesis hash (regtest has unique genesis per instance)
        {
            let rpc_url = env.elements_rpc_url();
            let (user, pass) = env.elements_rpc_credentials();
            let rpc =
                bitcoincore_rpc::Client::new(&rpc_url, bitcoincore_rpc::Auth::UserPass(user, pass))
                    .expect("rpc client");
            let genesis_hex: String = rpc
                .call("getblockhash", &[serde_json::json!(0)])
                .expect("getblockhash");
            let genesis_bytes = hex::decode(&genesis_hex).expect("decode genesis hex");
            // RPC returns display order (reversed). Convert to internal byte order.
            let mut genesis: [u8; 32] = genesis_bytes.try_into().expect("32 bytes");
            genesis.reverse();
            node.set_chain_genesis_hash(genesis)
                .await
                .expect("set genesis hash");
        }

        for _ in 0..10 {
            let addr = node.address(None).await.expect("wallet address");
            env.elementsd_sendtoaddress(addr.address(), 200_000, None);
        }
        mine_and_sync(&node, &env, 1).await;

        Self {
            env,
            _relay: relay,
            _wallet_dir: wallet_dir,
            node,
            store,
        }
    }

    async fn fund_wallet_asset_splits(&self, asset_id: AssetId, amounts: &[u64]) {
        for amount in amounts {
            let addr = self
                .node
                .address(None)
                .await
                .expect("asset receive address");
            self.env
                .elementsd_sendtoaddress(addr.address(), *amount, Some(asset_id));
        }
        mine_and_sync(&self.node, &self.env, 1).await;
    }
}

async fn bootstrap_pool(
    fixture: &Fixture,
) -> (CreateLmsrPoolRequest, deadcat_sdk::CreateLmsrPoolResult) {
    let yes_asset_id = fixture.env.elementsd_issueasset(1_000_000);
    let no_asset_id = fixture.env.elementsd_issueasset(1_000_000);
    fixture.env.elementsd_generate(1);

    fixture
        .fund_wallet_asset_splits(yes_asset_id, &[120_000, 110_000])
        .await;
    fixture
        .fund_wallet_asset_splits(no_asset_id, &[130_000, 100_000])
        .await;

    let yes_asset = asset_bytes(yes_asset_id);
    let no_asset = asset_bytes(no_asset_id);
    let lbtc_asset = asset_bytes(regtest_policy_asset());
    let table_values = vec![2_000, 2_010, 2_025, 2_045, 2_070, 2_100, 2_135, 2_175];
    let request = CreateLmsrPoolRequest {
        market_params: PredictionMarketParams {
            oracle_public_key: [0x42; 32],
            collateral_asset_id: lbtc_asset,
            yes_token_asset: yes_asset,
            no_token_asset: no_asset,
            yes_reissuance_token: [0x51; 32],
            no_reissuance_token: [0x52; 32],
            collateral_per_token: 100,
            expiry_time: 5_000_000,
        },
        pool_params: LmsrPoolParams {
            yes_asset_id: yes_asset,
            no_asset_id: no_asset,
            collateral_asset_id: lbtc_asset,
            lmsr_table_root: deadcat_sdk::lmsr_table_root(&table_values).expect("table root"),
            table_depth: 3,
            q_step_lots: 10,
            s_bias: 4,
            s_max_index: 7,
            half_payout_sats: 100,
            fee_bps: 30,
            min_r_yes: 1,
            min_r_no: 1,
            min_r_collateral: 1,
            cosigner_pubkey: NUMS_KEY_BYTES,
        },
        initial_s_index: 4,
        initial_reserves: PoolReserves {
            r_yes: 200_000,
            r_no: 200_000,
            r_lbtc: 300_000,
        },
        table_values: table_values.clone(),
        fee_amount: 500,
    };

    let created = fixture
        .node
        .create_lmsr_pool(request.clone())
        .await
        .expect("create lmsr pool");
    mine_and_sync(&fixture.node, &fixture.env, 1).await;
    (request, created)
}

#[tokio::test]
async fn create_scan_lmsr_pool_and_route_trade_regtest() {
    let _guard = hold_test_lock();
    let fixture = Fixture::new().await;
    let (request, created) = bootstrap_pool(&fixture).await;

    assert_eq!(created.txid, created.snapshot.locator.creation_txid);
    assert_eq!(created.snapshot.current_s_index, request.initial_s_index);
    assert_eq!(created.snapshot.reserves, request.initial_reserves);
    assert_eq!(
        created.snapshot.current_reserve_outpoints[0].txid,
        created.txid
    );
    assert_eq!(created.snapshot.current_reserve_outpoints[0].vout, 0);
    assert_eq!(created.snapshot.current_reserve_outpoints[1].vout, 1);
    assert_eq!(created.snapshot.current_reserve_outpoints[2].vout, 2);
    assert_eq!(
        created.announcement.lmsr_table_values,
        Some(request.table_values.clone())
    );

    let creation_tx = fixture
        .node
        .fetch_transaction(created.txid)
        .await
        .expect("fetch creation tx");
    let reserve_script = deadcat_sdk::CompiledLmsrPool::new(request.pool_params)
        .expect("compile pool")
        .script_pubkey(request.initial_s_index);
    for (index, asset_id, amount) in [
        (
            0usize,
            request.pool_params.yes_asset_id,
            request.initial_reserves.r_yes,
        ),
        (
            1usize,
            request.pool_params.no_asset_id,
            request.initial_reserves.r_no,
        ),
        (
            2usize,
            request.pool_params.collateral_asset_id,
            request.initial_reserves.r_lbtc,
        ),
    ] {
        let output = &creation_tx.output[index];
        assert_eq!(output.script_pubkey, reserve_script);
        assert_eq!(
            output.asset,
            Asset::Explicit(AssetId::from_slice(&asset_id).unwrap())
        );
        assert_eq!(output.value, ConfValue::Explicit(amount));
    }
    assert_eq!(
        creation_tx.output[3].value,
        ConfValue::Explicit(request.fee_amount)
    );
    assert!(
        creation_tx
            .output
            .iter()
            .skip(4)
            .any(|output| match output.asset {
                Asset::Confidential(_) => true,
                Asset::Explicit(_) => !output.script_pubkey.is_empty(),
                _ => false,
            }),
        "wallet change outputs should exist after aggregated funding"
    );

    {
        let store = fixture.store.lock().unwrap();
        assert_eq!(store.pools.len(), 1);
        let pool = &store.pools[0];
        assert_eq!(pool.pool_id, created.snapshot.locator.pool_id.to_hex());
        assert_eq!(
            pool.market_id,
            created.snapshot.locator.market_id.to_string()
        );
        assert_eq!(
            pool.reserve_outpoints,
            created
                .snapshot
                .current_reserve_outpoints
                .map(|outpoint| outpoint.to_string())
        );
        assert_eq!(pool.reserve_yes, request.initial_reserves.r_yes);
        assert_eq!(pool.reserve_no, request.initial_reserves.r_no);
        assert_eq!(pool.reserve_collateral, request.initial_reserves.r_lbtc);
        assert_eq!(
            pool.state_source,
            deadcat_sdk::LmsrPoolStateSource::CanonicalScan
        );
    }

    fixture
        .node
        .announce_pool(&created.announcement)
        .await
        .expect("announce pool");
    tokio::time::sleep(Duration::from_millis(200)).await;

    let market_id = request.market_params.market_id().to_string();
    let quote = quote_with_retry(
        &fixture.node,
        request.market_params,
        &market_id,
        TradeSide::Yes,
        TradeDirection::Buy,
        10_000,
    )
    .await;
    assert!(
        quote
            .legs
            .iter()
            .any(|leg| matches!(leg.source, LiquiditySource::LmsrPool { .. })),
        "quote must include LMSR liquidity"
    );

    let result = fixture
        .node
        .execute_trade(quote, 500, &market_id)
        .await
        .expect("execute routed trade");
    assert!(result.pool_used, "execution should spend the LMSR pool");
    mine_and_sync(&fixture.node, &fixture.env, 1).await;

    let scanned = fixture
        .node
        .scan_lmsr_pool(created.snapshot.locator.clone())
        .await
        .expect("scan lmsr pool");
    assert_ne!(
        scanned.current_reserve_outpoints,
        created.snapshot.current_reserve_outpoints
    );
    assert!(scanned.last_transition_txid.is_some());

    let store = fixture.store.lock().unwrap();
    let state = store
        .pool_states
        .last()
        .expect("pool state should be persisted");
    assert_eq!(state.pool_id, created.snapshot.locator.pool_id.to_hex());
    assert_eq!(
        state.reserve_outpoints,
        scanned
            .current_reserve_outpoints
            .map(|outpoint| outpoint.to_string())
    );
    drop(store);

    let fetched_pools = fixture
        .node
        .fetch_pools(Some(&market_id))
        .await
        .expect("fetch pools");
    assert_eq!(fetched_pools.len(), 1);

    let store = fixture.store.lock().unwrap();
    let pool = store.pools.first().expect("pool should remain persisted");
    assert_eq!(
        pool.state_source,
        deadcat_sdk::LmsrPoolStateSource::CanonicalScan
    );
    assert_eq!(pool.reserve_yes, scanned.reserves.r_yes);
    assert_eq!(pool.reserve_no, scanned.reserves.r_no);
    assert_eq!(pool.reserve_collateral, scanned.reserves.r_lbtc);
    assert_eq!(
        pool.reserve_outpoints,
        scanned
            .current_reserve_outpoints
            .map(|outpoint| outpoint.to_string())
    );
    assert!(pool.nostr_event_id.is_some());
    assert!(pool.nostr_event_json.is_some());
}

#[tokio::test]
async fn scan_lmsr_pool_rejects_non_canonical_pool_id_regtest() {
    let _guard = hold_test_lock();
    let fixture = Fixture::new().await;
    let (_, created) = bootstrap_pool(&fixture).await;

    let mut locator = created.snapshot.locator.clone();
    locator.pool_id = LmsrPoolId([0xff; 32]);

    let err = fixture.node.scan_lmsr_pool(locator).await.unwrap_err();
    match err {
        NodeError::Sdk(Error::LmsrPool(msg)) => {
            assert!(msg.contains("does not match canonical pool_id"));
            assert!(msg.contains(&created.snapshot.locator.pool_id.to_hex()));
        }
        other => panic!("expected LMSR validation error, got {other:?}"),
    }
}

#[tokio::test]
async fn scan_lmsr_pool_rejects_non_canonical_market_id_regtest() {
    let _guard = hold_test_lock();
    let fixture = Fixture::new().await;
    let (_, created) = bootstrap_pool(&fixture).await;
    let original_pool_count = fixture.store.lock().unwrap().pools.len();
    let original_state_count = fixture.store.lock().unwrap().pool_states.len();

    let mut locator = created.snapshot.locator.clone();
    locator.market_id = deadcat_sdk::MarketId([0xff; 32]);

    let err = fixture.node.scan_lmsr_pool(locator).await.unwrap_err();
    match err {
        NodeError::Sdk(Error::LmsrPool(msg)) => {
            assert!(msg.contains("does not match canonical market_id"));
            assert!(msg.contains(&created.snapshot.locator.market_id.to_string()));
        }
        other => panic!("expected LMSR validation error, got {other:?}"),
    }

    let store = fixture.store.lock().unwrap();
    assert_eq!(store.pools.len(), original_pool_count);
    assert_eq!(store.pool_states.len(), original_state_count);
}

#[tokio::test]
async fn scan_lmsr_pool_bootstraps_empty_store_regtest() {
    let _guard = hold_test_lock();
    let fixture = Fixture::new().await;
    let (_, created) = bootstrap_pool(&fixture).await;

    {
        let mut store = fixture.store.lock().unwrap();
        store.pools.clear();
        store.pool_states.clear();
    }

    let scanned = fixture
        .node
        .scan_lmsr_pool(created.snapshot.locator.clone())
        .await
        .expect("scan lmsr pool");

    let store = fixture.store.lock().unwrap();
    assert_eq!(store.pools.len(), 1);
    assert_eq!(store.pool_states.len(), 1);
    assert_eq!(
        store.pools[0].pool_id,
        created.snapshot.locator.pool_id.to_hex()
    );
    assert_eq!(
        store.pools[0].market_id,
        created.snapshot.locator.market_id.to_string()
    );
    assert_eq!(
        store.pools[0].reserve_outpoints,
        scanned
            .current_reserve_outpoints
            .map(|outpoint| outpoint.to_string())
    );
    assert_eq!(store.pools[0].reserve_yes, scanned.reserves.r_yes);
    assert_eq!(store.pools[0].reserve_no, scanned.reserves.r_no);
    assert_eq!(store.pools[0].reserve_collateral, scanned.reserves.r_lbtc);
    assert_eq!(
        store.pools[0].state_source,
        deadcat_sdk::LmsrPoolStateSource::CanonicalScan
    );
    assert_eq!(
        store.pool_states[0].reserve_outpoints,
        scanned
            .current_reserve_outpoints
            .map(|outpoint| outpoint.to_string())
    );
}

/// Bootstrap a pool with a real admin cosigner (not NUMS).
///
/// Returns the pool_index used, the create request, and the create result.
async fn bootstrap_admin_pool(
    fixture: &Fixture,
) -> (
    u32,
    CreateLmsrPoolRequest,
    deadcat_sdk::CreateLmsrPoolResult,
) {
    let pool_index: u32 = 0;
    let admin_pubkey = fixture
        .node
        .pool_admin_pubkey(pool_index)
        .await
        .expect("derive admin pubkey");

    let yes_asset_id = fixture.env.elementsd_issueasset(1_000_000);
    let no_asset_id = fixture.env.elementsd_issueasset(1_000_000);
    fixture.env.elementsd_generate(1);

    fixture
        .fund_wallet_asset_splits(yes_asset_id, &[120_000, 110_000])
        .await;
    fixture
        .fund_wallet_asset_splits(no_asset_id, &[130_000, 100_000])
        .await;

    let yes_asset = asset_bytes(yes_asset_id);
    let no_asset = asset_bytes(no_asset_id);
    let lbtc_asset = asset_bytes(regtest_policy_asset());
    let table_values = vec![2_000, 2_010, 2_025, 2_045, 2_070, 2_100, 2_135, 2_175];
    let request = CreateLmsrPoolRequest {
        market_params: PredictionMarketParams {
            oracle_public_key: [0x42; 32],
            collateral_asset_id: lbtc_asset,
            yes_token_asset: yes_asset,
            no_token_asset: no_asset,
            yes_reissuance_token: [0x51; 32],
            no_reissuance_token: [0x52; 32],
            collateral_per_token: 100,
            expiry_time: 5_000_000,
        },
        pool_params: LmsrPoolParams {
            yes_asset_id: yes_asset,
            no_asset_id: no_asset,
            collateral_asset_id: lbtc_asset,
            lmsr_table_root: deadcat_sdk::lmsr_table_root(&table_values).expect("table root"),
            table_depth: 3,
            q_step_lots: 10,
            s_bias: 4,
            s_max_index: 7,
            half_payout_sats: 100,
            fee_bps: 30,
            min_r_yes: 1,
            min_r_no: 1,
            min_r_collateral: 1,
            cosigner_pubkey: admin_pubkey,
        },
        initial_s_index: 4,
        initial_reserves: PoolReserves {
            r_yes: 50_000,
            r_no: 50_000,
            r_lbtc: 100_000,
        },
        table_values: table_values.clone(),
        fee_amount: 500,
    };

    let created = fixture
        .node
        .create_lmsr_pool(request.clone())
        .await
        .expect("create admin-cosigned lmsr pool");
    mine_and_sync(&fixture.node, &fixture.env, 1).await;
    (pool_index, request, created)
}

#[tokio::test]
async fn adjust_lmsr_pool_increases_reserves_regtest() {
    let _guard = hold_test_lock();
    let fixture = Fixture::new().await;
    let (pool_index, request, created) = bootstrap_admin_pool(&fixture).await;

    // Scan and build an adjust request with the live UTXOs
    let (snapshot, mut adjust_req) = fixture
        .node
        .scan_for_adjust(created.snapshot.locator.clone())
        .await
        .expect("scan_for_adjust");

    assert_eq!(snapshot.current_s_index, request.initial_s_index);
    assert_eq!(snapshot.reserves, request.initial_reserves);

    // Increase all reserves by 10_000
    adjust_req.new_reserves = PoolReserves {
        r_yes: snapshot.reserves.r_yes + 10_000,
        r_no: snapshot.reserves.r_no + 10_000,
        r_lbtc: snapshot.reserves.r_lbtc + 10_000,
    };
    adjust_req.table_values = request.table_values.clone();
    adjust_req.fee_amount = 500;
    adjust_req.pool_index = pool_index;

    let result = fixture
        .node
        .adjust_lmsr_pool(adjust_req.clone())
        .await
        .expect("adjust lmsr pool");

    // Verify the result snapshot
    assert_eq!(result.new_snapshot.reserves, adjust_req.new_reserves);
    assert_eq!(result.new_snapshot.current_s_index, request.initial_s_index);
    assert_eq!(
        result.new_snapshot.locator.pool_id,
        created.snapshot.locator.pool_id
    );
    assert_eq!(
        result.new_snapshot.locator.market_id,
        created.snapshot.locator.market_id
    );
    assert_eq!(
        result.new_snapshot.locator.creation_txid,
        created.snapshot.locator.creation_txid
    );

    // Mine and verify on-chain
    mine_and_sync(&fixture.node, &fixture.env, 1).await;
    let adjust_tx = fixture
        .node
        .fetch_transaction(result.txid)
        .await
        .expect("fetch adjust tx");

    // Reserve outputs are at indices 0, 1, 2
    let contract = deadcat_sdk::CompiledLmsrPool::new(request.pool_params).expect("compile pool");
    let reserve_script = contract.script_pubkey(request.initial_s_index);
    for (index, asset_id, amount) in [
        (
            0,
            request.pool_params.yes_asset_id,
            adjust_req.new_reserves.r_yes,
        ),
        (
            1,
            request.pool_params.no_asset_id,
            adjust_req.new_reserves.r_no,
        ),
        (
            2,
            request.pool_params.collateral_asset_id,
            adjust_req.new_reserves.r_lbtc,
        ),
    ] {
        let output = &adjust_tx.output[index];
        assert_eq!(
            output.script_pubkey, reserve_script,
            "output {index} script mismatch"
        );
        assert_eq!(
            output.asset,
            Asset::Explicit(AssetId::from_slice(&asset_id).unwrap()),
            "output {index} asset mismatch"
        );
        assert_eq!(
            output.value,
            ConfValue::Explicit(amount),
            "output {index} value mismatch"
        );
    }

    // Re-scan to verify the chain walk picks up the new state
    let rescanned = fixture
        .node
        .scan_lmsr_pool(created.snapshot.locator.clone())
        .await
        .expect("rescan after adjust");
    assert_eq!(rescanned.reserves, adjust_req.new_reserves);
    assert_eq!(rescanned.last_transition_txid, Some(result.txid));
}

#[tokio::test]
async fn adjust_lmsr_pool_decreases_reserves_returns_change_regtest() {
    let _guard = hold_test_lock();
    let fixture = Fixture::new().await;
    let (pool_index, request, created) = bootstrap_admin_pool(&fixture).await;

    let (_, mut adjust_req) = fixture
        .node
        .scan_for_adjust(created.snapshot.locator.clone())
        .await
        .expect("scan_for_adjust");

    // Decrease all reserves by 10_000 (removing liquidity)
    adjust_req.new_reserves = PoolReserves {
        r_yes: request.initial_reserves.r_yes - 10_000,
        r_no: request.initial_reserves.r_no - 10_000,
        r_lbtc: request.initial_reserves.r_lbtc - 10_000,
    };
    adjust_req.table_values = request.table_values.clone();
    adjust_req.fee_amount = 500;
    adjust_req.pool_index = pool_index;

    let result = match fixture.node.adjust_lmsr_pool(adjust_req.clone()).await {
        Ok(r) => r,
        Err(e) => {
            // Use RPC to get detailed rejection reason
            let rpc_url = fixture.env.elements_rpc_url();
            let (user, pass) = fixture.env.elements_rpc_credentials();
            eprintln!("RPC: {rpc_url} user={user}");
            let rpc =
                bitcoincore_rpc::Client::new(&rpc_url, bitcoincore_rpc::Auth::UserPass(user, pass))
                    .expect("rpc client");
            // Get the genesis block hash from the node
            let best = rpc
                .call::<serde_json::Value>("getblockhash", &[serde_json::json!(0)])
                .expect("getblockhash");
            eprintln!("Chain genesis hash: {best}");
            panic!("adjust lmsr pool (decrease): {e}");
        }
    };
    assert_eq!(result.new_snapshot.reserves, adjust_req.new_reserves);

    mine_and_sync(&fixture.node, &fixture.env, 1).await;

    let rescanned = fixture
        .node
        .scan_lmsr_pool(created.snapshot.locator.clone())
        .await
        .expect("rescan after decrease");
    assert_eq!(rescanned.reserves, adjust_req.new_reserves);
}
