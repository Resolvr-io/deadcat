use std::str::FromStr;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use bitcoincore_rpc::{Auth, Client as RpcClient, RpcApi};
use deadcat_sdk::lwk_wollet::elements::confidential::{Asset, Value as ConfValue};
use deadcat_sdk::lwk_wollet::elements::{Address, AddressParams, AssetId, Script, Txid};
use deadcat_sdk::taproot::NUMS_KEY_BYTES;
use deadcat_sdk::testing::TestStore;
use deadcat_sdk::{
    CompiledLmsrPool, DeadcatNode, DiscoveryConfig, Error, LiquiditySource, LmsrInitialOutpoint,
    LmsrPoolId, LmsrPoolIdInput, LmsrPoolParams, Network, NodeError, PoolAnnouncement, PoolParams,
    PoolReserves, PredictionMarketParams, TradeAmount, TradeDirection, TradeSide,
};
use lwk_test_util::{TEST_MNEMONIC, TestEnv, TestEnvBuilder, regtest_policy_asset};
use nostr_relay_builder::prelude::MockRelay;
use nostr_sdk::Keys;
use serde_json::{Value, json};
use tempfile::TempDir;

const WITNESS_SCHEMA_V2: &str = "DEADCAT/LMSR_WITNESS_SCHEMA_V2";
fn rpc_client(env: &TestEnv) -> RpcClient {
    let (user, pass) = env.elements_rpc_credentials();
    RpcClient::new(&env.elements_rpc_url(), Auth::UserPass(user, pass))
        .expect("create elements rpc client")
}

fn sats_to_btc_string(sats: u64) -> String {
    deadcat_sdk::lwk_wollet::elements::bitcoin::Amount::from_sat(sats)
        .to_string_in(deadcat_sdk::lwk_wollet::elements::bitcoin::Denomination::Bitcoin)
}

fn asset_bytes(asset: AssetId) -> [u8; 32] {
    asset.into_inner().to_byte_array()
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

fn derive_pool_id(announcement: &PoolAnnouncement) -> String {
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
        chain_genesis_hash: Network::LiquidRegtest.genesis_hash(),
        params,
        covenant_cmr: contract.primary_cmr().to_byte_array(),
        creation_txid,
        initial_yes_outpoint,
        initial_no_outpoint,
        initial_collateral_outpoint,
    })
    .expect("derive pool id")
    .to_hex()
}

fn create_lmsr_anchor_tx(
    env: &TestEnv,
    script: &Script,
    yes_asset: AssetId,
    no_asset: AssetId,
    collateral_asset: AssetId,
    reserve_yes: u64,
    reserve_no: u64,
    reserve_collateral: u64,
) -> Txid {
    let rpc = rpc_client(env);
    let reserve_address = Address::from_script(script, None, &AddressParams::ELEMENTS)
        .expect("reserve script to address");
    let reserve_addr = reserve_address.to_string();
    let output = |asset: AssetId, amount_sats: u64| -> Value {
        let mut map = serde_json::Map::new();
        map.insert(
            reserve_addr.clone(),
            Value::String(sats_to_btc_string(amount_sats)),
        );
        map.insert("asset".to_string(), Value::String(asset.to_string()));
        Value::Object(map)
    };
    let outputs = Value::Array(vec![
        output(yes_asset, reserve_yes),
        output(no_asset, reserve_no),
        output(collateral_asset, reserve_collateral),
    ]);

    let funded: Value = rpc
        .call(
            "walletcreatefundedpsbt",
            &[json!([]), outputs, json!(0), json!({}), json!(true)],
        )
        .expect("walletcreatefundedpsbt");
    let funded_psbt = funded
        .get("psbt")
        .and_then(Value::as_str)
        .expect("funded psbt");

    let processed: Value = rpc
        .call("walletprocesspsbt", &[json!(funded_psbt)])
        .expect("walletprocesspsbt");
    let signed_psbt = processed
        .get("psbt")
        .and_then(Value::as_str)
        .expect("signed psbt");

    let finalized: Value = rpc
        .call("finalizepsbt", &[json!(signed_psbt)])
        .expect("finalizepsbt");
    assert!(
        finalized
            .get("complete")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "finalized psbt must be complete"
    );
    let tx_hex = finalized
        .get("hex")
        .and_then(Value::as_str)
        .expect("finalized hex");

    let txid_hex: String = rpc
        .call("sendrawtransaction", &[json!(tx_hex)])
        .expect("sendrawtransaction");
    Txid::from_str(&txid_hex).expect("anchor txid")
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
    fixture: &RegtestFixture,
    side: TradeSide,
    direction: TradeDirection,
    amount: u64,
) -> deadcat_sdk::TradeQuote {
    let mut last_error = None;
    for _ in 0..16 {
        match fixture
            .node
            .quote_trade(
                fixture.market_params,
                &fixture.market_id,
                side,
                direction,
                TradeAmount::ExactInput(amount),
            )
            .await
        {
            Ok(quote) => return quote,
            Err(NodeError::Sdk(Error::Electrum(msg))) if msg.contains("missing transaction") => {
                last_error = Some(msg);
                let _ = fixture.node.sync_wallet().await;
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

fn test_market_params(
    yes_asset: [u8; 32],
    no_asset: [u8; 32],
    lbtc_asset: [u8; 32],
) -> PredictionMarketParams {
    PredictionMarketParams {
        oracle_public_key: [0x42; 32],
        collateral_asset_id: lbtc_asset,
        yes_token_asset: yes_asset,
        no_token_asset: no_asset,
        yes_reissuance_token: [0x99; 32],
        no_reissuance_token: [0x98; 32],
        collateral_per_token: 1_000,
        expiry_time: 5_000_000,
    }
}

struct RegtestFixture {
    env: TestEnv,
    _relay: MockRelay,
    _store: Arc<Mutex<TestStore>>,
    _wallet_dir: TempDir,
    node: DeadcatNode<TestStore>,
    market_id: String,
    market_params: PredictionMarketParams,
    yes_asset: [u8; 32],
    lbtc_asset: [u8; 32],
}

impl RegtestFixture {
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

        for _ in 0..10 {
            let addr = node.address(None).await.expect("wallet address");
            env.elementsd_sendtoaddress(addr.address(), 200_000, None);
        }
        mine_and_sync(&node, &env, 1).await;

        let yes_asset_id = env.elementsd_issueasset(2_000_000);
        let no_asset_id = env.elementsd_issueasset(2_000_000);
        env.elementsd_generate(1);

        // Fund taker with YES/NO assets so sell paths are executable.
        let yes_addr = node.address(None).await.expect("yes receive address");
        env.elementsd_sendtoaddress(yes_addr.address(), 50_000, Some(yes_asset_id));
        let no_addr = node.address(None).await.expect("no receive address");
        env.elementsd_sendtoaddress(no_addr.address(), 50_000, Some(no_asset_id));
        mine_and_sync(&node, &env, 1).await;

        let lbtc_asset_id = regtest_policy_asset();
        let yes_asset = asset_bytes(yes_asset_id);
        let no_asset = asset_bytes(no_asset_id);
        let lbtc_asset = asset_bytes(lbtc_asset_id);

        let table_values = vec![2_000, 2_010, 2_025, 2_045, 2_070, 2_100, 2_135, 2_175];
        let table_root = deadcat_sdk::lmsr_table_root(&table_values).expect("table root");
        let lmsr_params = LmsrPoolParams {
            yes_asset_id: yes_asset,
            no_asset_id: no_asset,
            collateral_asset_id: lbtc_asset,
            lmsr_table_root: table_root,
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
        };

        let current_s_index = 4;
        let reserve_yes = 200_000;
        let reserve_no = 200_000;
        let reserve_collateral = 400_000;

        let contract = CompiledLmsrPool::new(lmsr_params).expect("compile lmsr pool");
        let reserve_script = contract.script_pubkey(current_s_index);

        let creation_txid = create_lmsr_anchor_tx(
            &env,
            &reserve_script,
            yes_asset_id,
            no_asset_id,
            lbtc_asset_id,
            reserve_yes,
            reserve_no,
            reserve_collateral,
        );
        mine_and_sync(&node, &env, 1).await;

        let creation_tx = node
            .fetch_transaction(creation_txid)
            .await
            .expect("fetch creation tx");
        let mut yes_vout = None;
        let mut no_vout = None;
        let mut collateral_vout = None;
        for (idx, output) in creation_tx.output.iter().enumerate() {
            if output.script_pubkey != reserve_script {
                continue;
            }
            let Asset::Explicit(asset_id) = output.asset else {
                continue;
            };
            let amount = output.value;
            let amount_sat = match amount {
                ConfValue::Explicit(v) => v,
                _ => continue,
            };
            let asset = asset_id.into_inner().to_byte_array();
            if asset == yes_asset && amount_sat == reserve_yes {
                yes_vout = Some(idx as u32);
            } else if asset == no_asset && amount_sat == reserve_no {
                no_vout = Some(idx as u32);
            } else if asset == lbtc_asset && amount_sat == reserve_collateral {
                collateral_vout = Some(idx as u32);
            }
        }
        let yes_vout = yes_vout.expect("YES reserve vout");
        let no_vout = no_vout.expect("NO reserve vout");
        let collateral_vout = collateral_vout.expect("collateral reserve vout");

        let creation_txid_hex = creation_txid.to_string();
        let market_params = test_market_params(yes_asset, no_asset, lbtc_asset);
        let market_id = market_params.market_id().to_string();
        let mut announcement = PoolAnnouncement {
            version: 2,
            params: PoolParams {
                yes_asset_id: yes_asset,
                no_asset_id: no_asset,
                lbtc_asset_id: lbtc_asset,
                fee_bps: lmsr_params.fee_bps,
                min_r_yes: lmsr_params.min_r_yes,
                min_r_no: lmsr_params.min_r_no,
                min_r_collateral: lmsr_params.min_r_collateral,
                cosigner_pubkey: NUMS_KEY_BYTES,
            },
            market_id: market_id.clone(),
            reserves: PoolReserves {
                r_yes: reserve_yes,
                r_no: reserve_no,
                r_lbtc: reserve_collateral,
            },
            creation_txid: creation_txid_hex.clone(),
            lmsr_pool_id: String::new(),
            lmsr_table_root: hex::encode(table_root),
            table_depth: lmsr_params.table_depth,
            q_step_lots: lmsr_params.q_step_lots,
            s_bias: lmsr_params.s_bias,
            s_max_index: lmsr_params.s_max_index,
            half_payout_sats: lmsr_params.half_payout_sats,
            current_s_index,
            initial_reserve_outpoints: vec![
                format!("{creation_txid_hex}:{yes_vout}"),
                format!("{creation_txid_hex}:{no_vout}"),
                format!("{creation_txid_hex}:{collateral_vout}"),
            ],
            witness_schema_version: WITNESS_SCHEMA_V2.to_string(),
            table_manifest_hash: None,
            lmsr_table_values: Some(table_values),
        };
        announcement.lmsr_pool_id = derive_pool_id(&announcement);

        node.announce_pool(&announcement)
            .await
            .expect("announce LMSR pool");
        tokio::time::sleep(Duration::from_millis(200)).await;

        Self {
            env,
            _relay: relay,
            _store: store,
            _wallet_dir: wallet_dir,
            node,
            market_id,
            market_params,
            yes_asset,
            lbtc_asset,
        }
    }
}

#[tokio::test]
async fn lmsr_quote_execute_buy_yes_and_no_regtest() {
    let _guard = hold_test_lock();
    let fixture = RegtestFixture::new().await;

    for side in [TradeSide::Yes, TradeSide::No] {
        let quote = quote_with_retry(&fixture, side, TradeDirection::Buy, 10_000).await;
        assert!(
            quote.total_output > 0,
            "buy quote should produce taker output"
        );
        assert!(
            quote
                .legs
                .iter()
                .any(|leg| matches!(leg.source, LiquiditySource::LmsrPool { .. })),
            "quote must include LMSR leg"
        );

        let result = fixture
            .node
            .execute_trade(quote, 500, &fixture.market_id)
            .await
            .expect("execute buy trade");
        assert!(result.pool_used, "execution should use LMSR pool");

        mine_and_sync(&fixture.node, &fixture.env, 1).await;
    }
}

#[tokio::test]
async fn lmsr_quote_execute_sell_yes_and_no_regtest() {
    let _guard = hold_test_lock();
    let fixture = RegtestFixture::new().await;

    for side in [TradeSide::Yes, TradeSide::No] {
        let quote = quote_with_retry(&fixture, side, TradeDirection::Sell, 2_000).await;
        assert!(
            quote.total_output > 0,
            "sell quote should produce collateral output"
        );
        assert!(
            quote
                .legs
                .iter()
                .any(|leg| matches!(leg.source, LiquiditySource::LmsrPool { .. })),
            "quote must include LMSR leg"
        );

        let result = fixture
            .node
            .execute_trade(quote, 500, &fixture.market_id)
            .await
            .expect("execute sell trade");
        assert!(result.pool_used, "execution should use LMSR pool");

        mine_and_sync(&fixture.node, &fixture.env, 1).await;
    }
}

#[tokio::test]
async fn lmsr_mixed_maker_and_pool_route_executes_regtest() {
    let _guard = hold_test_lock();
    let fixture = RegtestFixture::new().await;

    let (_create_result, _event_id) = fixture
        .node
        .create_limit_order(
            fixture.yes_asset,
            fixture.lbtc_asset,
            1, // intentionally cheap: deterministic maker-first fill
            1_000,
            deadcat_sdk::OrderDirection::SellBase,
            1,
            1,
            44,
            500,
            fixture.market_id.clone(),
            "sell-yes".to_string(),
        )
        .await
        .expect("create limit order");
    mine_and_sync(&fixture.node, &fixture.env, 1).await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    let quote = quote_with_retry(&fixture, TradeSide::Yes, TradeDirection::Buy, 8_000).await;

    let has_limit_order_leg = quote
        .legs
        .iter()
        .any(|leg| matches!(leg.source, LiquiditySource::LimitOrder { .. }));
    let has_lmsr_leg = quote
        .legs
        .iter()
        .any(|leg| matches!(leg.source, LiquiditySource::LmsrPool { .. }));
    assert!(
        has_limit_order_leg && has_lmsr_leg,
        "quote should include both maker and LMSR legs"
    );

    let result = fixture
        .node
        .execute_trade(quote, 500, &fixture.market_id)
        .await
        .expect("execute mixed route");
    assert!(result.pool_used, "mixed route should use LMSR pool");
    assert!(
        result.num_orders_filled >= 1,
        "mixed route should fill at least one maker order"
    );
}

#[tokio::test]
async fn lmsr_execute_rejects_stale_quote_after_pool_transition() {
    let _guard = hold_test_lock();
    let fixture = RegtestFixture::new().await;

    let stale_quote = quote_with_retry(&fixture, TradeSide::Yes, TradeDirection::Buy, 7_000).await;
    let fresh_quote = quote_with_retry(&fixture, TradeSide::Yes, TradeDirection::Buy, 7_000).await;

    fixture
        .node
        .execute_trade(fresh_quote, 500, &fixture.market_id)
        .await
        .expect("execute fresh quote");
    mine_and_sync(&fixture.node, &fixture.env, 1).await;

    let err = fixture
        .node
        .execute_trade(stale_quote, 500, &fixture.market_id)
        .await
        .expect_err("stale quote must be rejected");
    match err {
        NodeError::Sdk(Error::TradeRouting(msg)) => {
            assert!(msg.contains("stale/non-canonical LMSR reserve anchors"));
        }
        other => panic!("expected stale quote rejection, got {other:?}"),
    }
}
