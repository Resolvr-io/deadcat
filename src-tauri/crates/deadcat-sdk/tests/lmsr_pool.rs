mod common;

use common::{
    POLL_INTERVAL, POLL_TIMEOUT, SharedTestEnv, sync_node_until, wait_for_lmsr_transition_indexed,
};
use std::future::Future;
use std::sync::{Arc, Mutex, OnceLock};

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
use lwk_test_util::{generate_mnemonic, regtest_policy_asset};
use nostr_relay_builder::prelude::MockRelay;
use nostr_sdk::Keys;
use tempfile::TempDir;

fn asset_bytes(asset: AssetId) -> [u8; 32] {
    asset.into_inner().to_byte_array()
}

async fn wait_for_chain_tx(
    node: &DeadcatNode<TestStore>,
    env: &SharedTestEnv,
    blocks: u32,
    txid: deadcat_sdk::lwk_wollet::elements::Txid,
) {
    env.generate_blocks(blocks);
    wait_for("chain transaction visibility", || async {
        let _ = node.sync_wallet().await;
        node.fetch_transaction(txid).await.ok().map(|_| ())
    })
    .await;
}

async fn confirm_external_tx(
    node: &DeadcatNode<TestStore>,
    env: &SharedTestEnv,
    blocks: u32,
    txid: deadcat_sdk::lwk_wollet::elements::Txid,
    min_utxos: usize,
) {
    env.generate_blocks(blocks);
    sync_node_until(
        node,
        "external funding transaction confirmation",
        POLL_TIMEOUT,
        |node| {
            let txs = node.transactions().unwrap();
            let utxos = node.utxos().unwrap();
            let confirmed = txs.iter().any(|tx| tx.txid == txid && tx.height.is_some());
            (confirmed && utxos.len() >= min_utxos).then_some(())
        },
    )
    .await;
}

fn hold_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    match LOCK.get_or_init(|| Mutex::new(())).lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
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

async fn wait_for_pool_visibility(node: &DeadcatNode<TestStore>, market_id: &str) {
    wait_for("pool announcement to be fetchable", || async {
        let pools = node.fetch_pools(Some(market_id)).await.unwrap();
        (!pools.is_empty()).then_some(())
    })
    .await;
}

async fn quote_with_retry(
    node: &DeadcatNode<TestStore>,
    market_params: PredictionMarketParams,
    market_id: &str,
    side: TradeSide,
    direction: TradeDirection,
    amount: u64,
) -> deadcat_sdk::TradeQuote {
    let deadline = tokio::time::Instant::now() + POLL_TIMEOUT;
    loop {
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
                assert!(
                    tokio::time::Instant::now() < deadline,
                    "quote did not succeed before timeout; last electrum error: {msg}"
                );
                let _ = node.sync_wallet().await;
            }
            Err(err) => panic!("quote failed: {err:?}"),
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

struct Fixture {
    env: SharedTestEnv,
    _relay: MockRelay,
    _wallet_dir: TempDir,
    node: DeadcatNode<TestStore>,
    store: Arc<Mutex<TestStore>>,
}

impl Fixture {
    async fn new() -> Self {
        let env = SharedTestEnv::global();
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
        let mnemonic = generate_mnemonic();
        node.unlock_wallet(&mnemonic, env.electrum_url(), wallet_dir.path())
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

        let mut addresses = Vec::with_capacity(10);
        for index in 0..10 {
            addresses.push(node.address(Some(index)).await.expect("wallet address"));
        }
        let outputs: Vec<_> = addresses
            .iter()
            .map(|addr| (addr.address(), 200_000, None))
            .collect();
        let initial_utxos = node.utxos().unwrap().len();
        let txid = env.elementsd_send_outputs(&outputs);
        confirm_external_tx(&node, &env, 1, txid, initial_utxos + outputs.len()).await;

        Self {
            env,
            _relay: relay,
            _wallet_dir: wallet_dir,
            node,
            store,
        }
    }

    async fn fund_wallet_asset_splits(&self, asset_id: AssetId, amounts: &[u64]) {
        let mut addresses = Vec::with_capacity(amounts.len());
        for (index, _) in amounts.iter().enumerate() {
            addresses.push(
                self.node
                    .address(Some(index as u32))
                    .await
                    .expect("asset receive address"),
            );
        }
        let outputs: Vec<_> = addresses
            .iter()
            .zip(amounts.iter().copied())
            .map(|(addr, amount)| (addr.address(), amount, Some(asset_id)))
            .collect();
        let initial_utxos = self.node.utxos().unwrap().len();
        let txid = self.env.elementsd_send_outputs(&outputs);
        confirm_external_tx(
            &self.node,
            &self.env,
            1,
            txid,
            initial_utxos + outputs.len(),
        )
        .await;
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
    wait_for_chain_tx(&fixture.node, &fixture.env, 1, created.txid).await;
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
    let market_id = request.market_params.market_id().to_string();
    wait_for_pool_visibility(&fixture.node, &market_id).await;

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
    fixture.env.generate_blocks(1);
    let contract =
        deadcat_sdk::CompiledLmsrPool::new(created.snapshot.locator.params).expect("compile pool");
    let old_script = contract.script_pubkey(created.snapshot.current_s_index);
    wait_for_lmsr_transition_indexed(
        &fixture.env,
        &old_script,
        &created.snapshot.current_reserve_outpoints,
        result.txid,
    );

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
    wait_for_chain_tx(&fixture.node, &fixture.env, 1, created.txid).await;
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

    let contract = deadcat_sdk::CompiledLmsrPool::new(request.pool_params).expect("compile pool");
    let reserve_script = contract.script_pubkey(request.initial_s_index);
    // Mine and verify on-chain
    fixture.env.generate_blocks(1);
    wait_for_lmsr_transition_indexed(
        &fixture.env,
        &reserve_script,
        &snapshot.current_reserve_outpoints,
        result.txid,
    );
    let adjust_tx = fixture
        .node
        .fetch_transaction(result.txid)
        .await
        .expect("fetch adjust tx");

    // Reserve outputs are at indices 0, 1, 2
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

    fixture.env.generate_blocks(1);
    let contract = deadcat_sdk::CompiledLmsrPool::new(request.pool_params).expect("compile pool");
    let old_script = contract.script_pubkey(created.snapshot.current_s_index);
    wait_for_lmsr_transition_indexed(
        &fixture.env,
        &old_script,
        &created.snapshot.current_reserve_outpoints,
        result.txid,
    );

    let rescanned = fixture
        .node
        .scan_lmsr_pool(created.snapshot.locator.clone())
        .await
        .expect("rescan after decrease");
    assert_eq!(rescanned.reserves, adjust_req.new_reserves);
}
