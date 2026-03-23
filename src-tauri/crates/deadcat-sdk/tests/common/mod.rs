use std::collections::HashSet;
use std::str::FromStr;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use bitcoincore_rpc::{Auth, Client as RpcClient, RpcApi};
use deadcat_sdk::testing::TestStore;
use deadcat_sdk::{DeadcatNode, DeadcatSdk, Error, NodeError};
use electrum_client::{ElectrumApi, Param};
use lwk_test_util::{TestEnv, TestEnvBuilder};
use lwk_wollet::blocking::BlockchainBackend;
use lwk_wollet::elements::bitcoin::{Amount, Denomination};
use lwk_wollet::elements::{Address, AssetId, OutPoint, Script, Txid};
use lwk_wollet::{ElectrumClient, ElectrumUrl};
use serde_json::{Value, json};

pub const POLL_INTERVAL: Duration = Duration::from_millis(50);
pub const POLL_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub struct SharedTestEnv {
    inner: Arc<Mutex<TestEnv>>,
    electrum_url: String,
}

impl SharedTestEnv {
    pub fn global() -> Self {
        static SHARED_ENV: OnceLock<Arc<Mutex<TestEnv>>> = OnceLock::new();

        let inner = SHARED_ENV
            .get_or_init(|| {
                Arc::new(Mutex::new(
                    TestEnvBuilder::from_env().with_electrum().build(),
                ))
            })
            .clone();
        let electrum_url = with_mutex(&inner, |env| env.electrum_url());

        Self {
            inner,
            electrum_url,
        }
    }

    pub fn electrum_url(&self) -> &str {
        &self.electrum_url
    }

    pub fn elements_rpc_url(&self) -> String {
        self.with_env(|env| env.elements_rpc_url())
    }

    pub fn elements_rpc_credentials(&self) -> (String, String) {
        self.with_env(|env| env.elements_rpc_credentials())
    }

    pub fn elementsd_send_outputs(&self, outputs: &[(&Address, u64, Option<AssetId>)]) -> Txid {
        assert!(
            !outputs.is_empty(),
            "elementsd_send_outputs requires at least one output"
        );

        let rpc = self.rpc_client();
        let outputs = Value::Array(
            outputs
                .iter()
                .map(|(address, satoshi, asset)| {
                    let mut map = serde_json::Map::new();
                    map.insert(
                        address.to_string(),
                        Value::String(amount_to_btc_string(*satoshi)),
                    );
                    if let Some(asset_id) = asset {
                        map.insert("asset".to_string(), Value::String(asset_id.to_string()));
                    }
                    Value::Object(map)
                })
                .collect(),
        );

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

        let txid = rpc
            .call::<String>("sendrawtransaction", &[json!(tx_hex)])
            .expect("sendrawtransaction");
        Txid::from_str(&txid).expect("broadcast txid")
    }

    #[allow(dead_code)]
    pub fn elementsd_issueasset(&self, satoshi: u64) -> AssetId {
        self.with_env(|env| env.elementsd_issueasset(satoshi))
    }

    #[allow(dead_code)]
    pub fn elementsd_height(&self) -> u64 {
        self.with_env(|env| env.elementsd_height())
    }

    #[allow(dead_code)]
    pub fn elementsd_generate(&self, blocks: u32) {
        self.with_env(|env| env.elementsd_generate(blocks));
    }

    pub fn generate_blocks(&self, blocks: u32) -> u64 {
        self.with_env(|env| {
            env.elementsd_generate(blocks);
            env.elementsd_height()
        })
    }

    /// Wait for electrum to report the given chain tip height.
    ///
    /// This is only suitable for chain-tip dependent flows. Wallet visibility
    /// must use the wallet-aware polling helpers below instead.
    #[allow(dead_code)]
    pub fn wait_for_electrum_tip(&self, min_height: u64) {
        let electrum_url = ElectrumUrl::from_str(&self.electrum_url).expect("parse electrum url");
        let mut client = ElectrumClient::new(&electrum_url).expect("create electrum client");
        let deadline = Instant::now() + POLL_TIMEOUT;

        loop {
            let tip_height = u64::from(client.tip().expect("query electrum tip").height);
            if tip_height >= min_height {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "electrum tip did not reach height {min_height} before timeout"
            );
            std::thread::sleep(POLL_INTERVAL);
        }
    }

    fn with_env<T>(&self, f: impl FnOnce(&TestEnv) -> T) -> T {
        with_mutex(&self.inner, f)
    }

    fn rpc_client(&self) -> RpcClient {
        let (user, pass) = self.elements_rpc_credentials();
        RpcClient::new(&self.elements_rpc_url(), Auth::UserPass(user, pass))
            .expect("create elements rpc client")
    }
}

fn with_mutex<T, R>(mutex: &Mutex<T>, f: impl FnOnce(&T) -> R) -> R {
    match mutex.lock() {
        Ok(guard) => f(&guard),
        Err(poisoned) => f(&poisoned.into_inner()),
    }
}

fn amount_to_btc_string(satoshi: u64) -> String {
    Amount::from_sat(satoshi).to_string_in(Denomination::Bitcoin)
}

#[allow(dead_code)]
pub fn wait_until<T>(label: &str, timeout: Duration, mut poll: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(value) = poll() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {label}");
        std::thread::sleep(POLL_INTERVAL);
    }
}

#[allow(dead_code)]
pub fn sync_sdk_until<T>(
    sdk: &mut DeadcatSdk,
    label: &str,
    timeout: Duration,
    mut poll: impl FnMut(&DeadcatSdk) -> Option<T>,
) -> T {
    let deadline = Instant::now() + timeout;
    loop {
        match sdk.sync() {
            Ok(()) => {
                if let Some(value) = poll(sdk) {
                    return value;
                }
            }
            Err(err) if is_transient_sdk_sync_error(&err) => {}
            Err(err) => panic!("sync sdk failed while waiting for {label}: {err}"),
        }
        assert!(Instant::now() < deadline, "timed out waiting for {label}");
        std::thread::sleep(POLL_INTERVAL);
    }
}

#[allow(dead_code)]
pub async fn sync_node_until<T, F>(
    node: &DeadcatNode<TestStore>,
    label: &str,
    timeout: Duration,
    mut poll: F,
) -> T
where
    F: FnMut(&DeadcatNode<TestStore>) -> Option<T>,
{
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        match node.sync_wallet().await {
            Ok(()) => {
                if let Some(value) = poll(node) {
                    return value;
                }
            }
            Err(err) if is_transient_node_sync_error(&err) => {}
            Err(err) => panic!("sync wallet failed while waiting for {label}: {err}"),
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {label}"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

fn is_transient_sdk_sync_error(err: &Error) -> bool {
    match err {
        Error::Electrum(msg) => is_transient_electrum_message(msg),
        _ => false,
    }
}

fn is_transient_node_sync_error(err: &NodeError) -> bool {
    match err {
        NodeError::Sdk(inner) => is_transient_sdk_sync_error(inner),
        _ => false,
    }
}

fn is_transient_electrum_message(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("missing transaction")
        || lower.contains("no such mempool or blockchain transaction")
}

#[allow(dead_code)]
pub fn wait_for_lmsr_transition_indexed(
    env: &SharedTestEnv,
    old_script: &Script,
    previous_bundle: &[OutPoint],
    transition_txid: Txid,
) {
    let client =
        electrum_client::Client::new(env.electrum_url()).expect("create electrum polling client");
    let script_hash = electrum_script_hash_hex(old_script);
    let deadline = Instant::now() + POLL_TIMEOUT;

    loop {
        match lmsr_transition_indexed(&client, &script_hash, previous_bundle, transition_txid) {
            Ok(true) => return,
            Ok(false) => {}
            Err(err) if is_transient_electrum_poll_error(&err) => {}
            Err(err) => panic!("failed while waiting for indexed LMSR transition: {err}"),
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for LMSR transition {transition_txid} to be indexed"
        );
        std::thread::sleep(POLL_INTERVAL);
    }
}

fn lmsr_transition_indexed(
    client: &electrum_client::Client,
    script_hash: &str,
    previous_bundle: &[OutPoint],
    transition_txid: Txid,
) -> std::result::Result<bool, String> {
    let history = client
        .raw_call(
            "blockchain.scripthash.get_history",
            [Param::String(script_hash.to_string())],
        )
        .map_err(|e| e.to_string())?;
    let history_entries = history
        .as_array()
        .ok_or_else(|| "expected array response from get_history".to_string())?;
    let transition_txid = transition_txid.to_string();
    let history_contains_transition = history_entries.iter().any(|entry| {
        entry["tx_hash"]
            .as_str()
            .map(|tx_hash| tx_hash == transition_txid)
            .unwrap_or(false)
    });
    if !history_contains_transition {
        return Ok(false);
    }

    let utxos = client
        .raw_call(
            "blockchain.scripthash.listunspent",
            [Param::String(script_hash.to_string())],
        )
        .map_err(|e| e.to_string())?;
    let utxo_entries = utxos
        .as_array()
        .ok_or_else(|| "expected array response from listunspent".to_string())?;
    let previous_bundle: HashSet<_> = previous_bundle.iter().copied().collect();
    let any_previous_live = utxo_entries.iter().any(|entry| {
        let tx_hash = entry["tx_hash"].as_str();
        let tx_pos = entry["tx_pos"].as_u64();
        match (tx_hash, tx_pos) {
            (Some(tx_hash), Some(tx_pos)) => tx_hash
                .parse::<Txid>()
                .ok()
                .map(|txid| previous_bundle.contains(&OutPoint::new(txid, tx_pos as u32)))
                .unwrap_or(false),
            _ => false,
        }
    });

    Ok(!any_previous_live)
}

fn electrum_script_hash_hex(script_pubkey: &Script) -> String {
    use sha2::{Digest, Sha256};

    let mut hash = Sha256::digest(script_pubkey.as_bytes()).to_vec();
    hash.reverse();
    hex::encode(hash)
}

fn is_transient_electrum_poll_error(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    is_transient_electrum_message(msg) || lower.contains("timeout") || lower.contains("timed out")
}
