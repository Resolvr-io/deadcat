mod chain_adapter;
pub mod commands;
pub mod discovery;
pub mod identity_file;
pub mod nip46;
mod payments;
pub mod state;
pub mod wallet;
mod wallet_store;

use std::sync::Mutex;

use deadcat_sdk::elements::hashes::Hash as _;
use deadcat_store::ChainSource;
use serde::Deserialize;
use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Manager};

use state::{AppState, AppStateManager, PaymentSwap, AUTO_LOCK_TIMEOUT_SECS};

/// How often (in seconds) to sync the wallet in the background while unlocked.
const WALLET_SYNC_INTERVAL_SECS: u64 = 15;

/// Default confirmation target for fee estimation (Liquid blocks).
const DEFAULT_FEE_TARGET_BLOCKS: u16 = 6;

const APP_STATE_UPDATED_EVENT: &str = "app_state_updated";

/// Holds the DeadcatNode behind a tokio Mutex for async access.
/// The node is wrapped in `Arc` so that commands can clone it out of the
/// mutex guard and drop the guard immediately — this prevents long-running
/// operations like `sync()` from blocking short operations like address
/// generation.
///
/// NOTE: Commands should clone the Arc and drop the guard before calling
/// async node methods, especially before acquiring `AppStateManager`'s
/// std Mutex, to avoid holding both locks simultaneously.
pub struct NodeState {
    pub node: tokio::sync::Mutex<
        Option<std::sync::Arc<deadcat_sdk::DeadcatNode<deadcat_store::DeadcatStore>>>,
    >,
    /// JoinHandles for background tasks spawned per node (relay subscription,
    /// discovery event forwarding, wallet snapshot forwarding). When a node is
    /// replaced or dropped these are aborted so tasks don't leak.
    pub task_handles: tokio::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl Default for NodeState {
    fn default() -> Self {
        Self {
            node: tokio::sync::Mutex::new(None),
            task_handles: tokio::sync::Mutex::new(Vec::new()),
        }
    }
}

/// Holds a prepared send transaction awaiting user confirmation.
pub struct PendingSendState {
    pub prepared: tokio::sync::Mutex<Option<deadcat_sdk::PreparedSendLbtc>>,
}

impl Default for PendingSendState {
    fn default() -> Self {
        Self {
            prepared: tokio::sync::Mutex::new(None),
        }
    }
}

/// Minimal state for the legacy wallet_store commands.
#[derive(Default)]
pub struct WalletStoreState {
    pub wallet_store: wallet_store::WalletStore,
}

/// App-layer Nostr state: relay list and source npub (keys come from the node).
pub struct NostrAppState {
    pub relay_list: std::sync::RwLock<Vec<String>>,
    /// The npub whose market announcements we subscribe to.
    pub source_npub: std::sync::RwLock<String>,
}

impl Default for NostrAppState {
    fn default() -> Self {
        Self {
            relay_list: std::sync::RwLock::new(
                discovery::DEFAULT_RELAYS
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            ),
            source_npub: std::sync::RwLock::new(discovery::DEFAULT_SOURCE_NPUB.to_string()),
        }
    }
}

// ============================================================================
// Network type
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    Mainnet,
    Testnet,
    Regtest,
}

impl Network {
    pub fn as_str(&self) -> &'static str {
        match self {
            Network::Mainnet => "mainnet",
            Network::Testnet => "testnet",
            Network::Regtest => "regtest",
        }
    }

    pub fn is_mainnet(&self) -> bool {
        matches!(self, Network::Mainnet)
    }
}

impl std::str::FromStr for Network {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "mainnet" => Ok(Network::Mainnet),
            "testnet" => Ok(Network::Testnet),
            "regtest" => Ok(Network::Regtest),
            _ => Err(format!("Invalid network: {}", s)),
        }
    }
}

// ============================================================================
// Network Commands
// ============================================================================

#[tauri::command]
async fn is_first_launch(app: AppHandle) -> Result<bool, String> {
    tokio::task::spawn_blocking(move || {
        let manager = app.state::<Mutex<AppStateManager>>();
        let mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        Ok(mgr.is_first_launch())
    })
    .await
    .map_err(|e| format!("first_launch task failed: {e}"))?
}

#[tauri::command]
async fn set_network(network: Network, app: AppHandle) -> Result<AppState, String> {
    let app_handle = app.clone();
    tokio::task::spawn_blocking(move || {
        let manager = app_handle.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        let state = mgr.set_network(network);
        emit_state(&app_handle, &state);
        Ok(state)
    })
    .await
    .map_err(|e| format!("set_network task failed: {e}"))?
}

// ============================================================================
// App State Commands
// ============================================================================

#[tauri::command]
async fn get_app_state(app: AppHandle) -> Result<AppState, String> {
    let app_ref = app.clone();
    tokio::task::spawn_blocking(move || {
        let manager = app_ref.state::<Mutex<AppStateManager>>();
        let mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        if !mgr.is_initialized() {
            return Err("Not initialized - select a network first".to_string());
        }
        Ok(mgr.snapshot())
    })
    .await
    .map_err(|e| format!("state task failed: {e}"))?
}

// ============================================================================
// Wallet Commands
// ============================================================================

#[tauri::command]
async fn get_wallet_status(app: AppHandle) -> Result<wallet::types::WalletStatus, String> {
    tokio::task::spawn_blocking(move || {
        let manager = app.state::<Mutex<AppStateManager>>();
        let mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        Ok(mgr.wallet_status())
    })
    .await
    .map_err(|e| format!("wallet_status task failed: {e}"))?
}

#[tauri::command]
async fn generate_mnemonic(app: AppHandle) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let manager = app.state::<Mutex<AppStateManager>>();
        let mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        let network = mgr.network().ok_or("Network not initialized")?;
        let sdk_network = state::to_sdk_network(network);
        deadcat_sdk::DeadcatNode::<deadcat_sdk::NoopStore>::generate_mnemonic(sdk_network)
            .map_err(|e| format!("{e}"))
    })
    .await
    .map_err(|e| format!("generate_mnemonic task failed: {e}"))?
}

#[tauri::command]
async fn create_wallet(password: String, app: AppHandle) -> Result<String, String> {
    let app_handle = app.clone();
    tokio::task::spawn_blocking(move || {
        let manager = app_handle.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        let network = mgr.network().ok_or("Network not initialized")?;
        let sdk_network = state::to_sdk_network(network);

        let mnemonic =
            deadcat_sdk::DeadcatNode::<deadcat_sdk::NoopStore>::generate_mnemonic(sdk_network)
                .map_err(|e| format!("{e}"))?;

        let persister = mgr.persister_mut().ok_or("Persister not initialized")?;
        persister
            .save(&mnemonic, &password)
            .map_err(|e| e.to_string())?;

        mgr.bump_revision();
        let state = mgr.snapshot();
        emit_state(&app_handle, &state);
        Ok(mnemonic)
    })
    .await
    .map_err(|e| format!("create_wallet task failed: {e}"))?
}

#[tauri::command]
async fn restore_wallet(
    mnemonic: String,
    password: String,
    app: AppHandle,
) -> Result<AppState, String> {
    let _t0 = std::time::Instant::now();
    log::info!("[restore-trace] restore_wallet: start");
    let app_handle = app.clone();
    let result = tokio::task::spawn_blocking(move || {
        log::info!(
            "[restore-trace] restore_wallet: spawn_blocking entered at {:?}",
            _t0.elapsed()
        );
        let manager = app_handle.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        log::info!(
            "[restore-trace] restore_wallet: got AppStateManager lock at {:?}",
            _t0.elapsed()
        );

        // Validate mnemonic
        let _: bip39::Mnemonic = mnemonic
            .parse()
            .map_err(|_| "Invalid mnemonic".to_string())?;

        let persister = mgr.persister_mut().ok_or("Persister not initialized")?;
        persister
            .save(&mnemonic, &password)
            .map_err(|e| e.to_string())?;
        log::info!(
            "[restore-trace] restore_wallet: persister.save done at {:?}",
            _t0.elapsed()
        );

        mgr.bump_revision();
        let state = mgr.snapshot();
        emit_state(&app_handle, &state);
        log::info!(
            "[restore-trace] restore_wallet: emit_state done at {:?}",
            _t0.elapsed()
        );
        Ok(state)
    })
    .await
    .map_err(|e| format!("restore_wallet task failed: {e}"))?;
    log::info!(
        "[restore-trace] restore_wallet: complete in {:?}",
        _t0.elapsed()
    );
    result
}

#[tauri::command]
async fn unlock_wallet(password: String, app: AppHandle) -> Result<AppState, String> {
    let _t0 = std::time::Instant::now();
    log::info!("[restore-trace] unlock_wallet: start");
    let app_handle = app.clone();

    // 1. Decrypt mnemonic (blocking — Argon2 KDF). This is fast if cached.
    let (mnemonic, network, data_dir) = tokio::task::spawn_blocking({
        let app_ref = app_handle.clone();
        let t0 = _t0;
        move || {
            log::info!(
                "[restore-trace] unlock_wallet: spawn_blocking entered at {:?}",
                t0.elapsed()
            );
            let manager = app_ref.state::<Mutex<AppStateManager>>();
            let mut mgr = manager
                .lock()
                .map_err(|_| "state lock failed".to_string())?;
            log::info!(
                "[restore-trace] unlock_wallet: got AppStateManager lock at {:?}",
                t0.elapsed()
            );
            let network = mgr.network().ok_or("Network not initialized")?;

            let persister = mgr.persister_mut().ok_or("Persister not initialized")?;
            let mnemonic = if let Some(cached) = persister.cached() {
                log::info!("[restore-trace] unlock_wallet: using cached mnemonic");
                cached.to_string()
            } else {
                log::info!("[restore-trace] unlock_wallet: decrypting mnemonic (Argon2)...");
                let m = persister.load(&password).map_err(|e| e.to_string())?;
                log::info!(
                    "[restore-trace] unlock_wallet: Argon2 done at {:?}",
                    t0.elapsed()
                );
                m
            };

            let data_dir = mgr.app_data_dir.clone();
            Ok::<_, String>((mnemonic, network, data_dir))
        }
    })
    .await
    .map_err(|e| format!("unlock task failed: {e}"))??;
    log::info!(
        "[restore-trace] unlock_wallet: mnemonic ready at {:?}",
        _t0.elapsed()
    );

    // 2. Always return optimistic unlocked state immediately and run the
    //    heavy SDK initialization (Wollet DB open, electrum backend) on a
    //    dedicated OS thread so we never block tokio workers or the UI.
    let node = {
        let node_state = app_handle.state::<NodeState>();
        let guard = node_state.node.lock().await;
        guard
            .as_ref()
            .cloned()
            .ok_or("Node not initialized — call init_nostr_identity first")?
    };
    log::info!(
        "[restore-trace] unlock_wallet: got node at {:?}",
        _t0.elapsed()
    );

    let sdk_network = state::to_sdk_network(network);
    let electrum_url = sdk_network.default_electrum_url().to_string();

    // Fire SDK initialization on a dedicated OS thread (not spawn_blocking)
    let bg_app = app_handle.clone();
    std::thread::spawn(move || {
        log::info!("[restore-trace] unlock_wallet: bg thread — starting SDK init");
        let bg_t0 = std::time::Instant::now();
        if let Err(e) = node.unlock_wallet(&mnemonic, &electrum_url, &data_dir) {
            log::warn!("[restore-trace] unlock_wallet: bg thread — failed: {e}");
            return;
        }
        log::info!(
            "[restore-trace] unlock_wallet: bg thread — SDK init done in {:?}",
            bg_t0.elapsed()
        );
        let wb: Option<std::collections::HashMap<String, u64>> = node.balance().ok().map(|m| {
            m.into_iter()
                .filter(|(_, v)| *v > 0)
                .map(|(k, v)| (k.to_string(), v))
                .collect()
        });
        let manager = bg_app.state::<Mutex<AppStateManager>>();
        let mut mgr = match manager.lock() {
            Ok(m) => m,
            Err(_) => return,
        };
        mgr.set_wallet_unlocked(true);
        mgr.purge_stale_swaps();
        mgr.touch_activity();
        mgr.bump_revision();
        let state = mgr.snapshot_with_balance(wb);
        emit_state(&bg_app, &state);
    });

    // Return optimistic unlocked state immediately
    let state = tokio::task::spawn_blocking({
        let app_ref = app_handle.clone();
        move || {
            let manager = app_ref.state::<Mutex<AppStateManager>>();
            let mut mgr = manager
                .lock()
                .map_err(|_| "state lock failed".to_string())?;
            mgr.set_wallet_unlocked(true);
            mgr.bump_revision();
            let state = mgr.snapshot();
            let _ = app_ref.emit(APP_STATE_UPDATED_EVENT, &state);
            Ok::<_, String>(state)
        }
    })
    .await
    .map_err(|e| format!("unlock state task failed: {e}"))??;

    Ok(state)
}

#[tauri::command]
async fn lock_wallet(app: AppHandle) -> Result<AppState, String> {
    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;

    if let Some(node) = guard.as_ref() {
        node.lock_wallet();
    }
    drop(guard);

    let app_handle = app.clone();
    tokio::task::spawn_blocking(move || {
        let manager = app_handle.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        mgr.set_wallet_unlocked(false);
        if let Some(persister) = mgr.persister_mut() {
            persister.clear_cache();
        }
        mgr.bump_revision();
        let state = mgr.snapshot();
        emit_state(&app_handle, &state);
        Ok(state)
    })
    .await
    .map_err(|e| format!("lock_wallet task failed: {e}"))?
}

#[tauri::command]
async fn delete_wallet(app: AppHandle) -> Result<AppState, String> {
    // Lock/drop the wallet in the node
    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    if let Some(node) = guard.as_ref() {
        node.lock_wallet();
    }
    drop(guard);

    let app_handle = app.clone();
    tokio::task::spawn_blocking(move || {
        let manager = app_handle.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        mgr.set_wallet_unlocked(false);
        if let Some(persister) = mgr.persister_mut() {
            persister.delete().map_err(|e| e.to_string())?;
        }
        // Remove the LWK wallet database so a fresh restore doesn't reopen
        // stale data from a previous wallet (different descriptor).
        if let Some(network) = mgr.network() {
            let wallet_db_dir = mgr.app_data_dir.join(network.as_str()).join("wallet_db");
            if wallet_db_dir.exists() {
                if let Err(e) = std::fs::remove_dir_all(&wallet_db_dir) {
                    log::warn!("failed to remove wallet_db: {e}");
                }
            }
        }
        mgr.clear_payment_swaps();
        let state = mgr.snapshot();
        emit_state(&app_handle, &state);
        Ok(state)
    })
    .await
    .map_err(|e| format!("delete_wallet task failed: {e}"))?
}

/// Sync store candidates against the chain (promote confirmed, purge expired).
fn sync_store_candidates(
    store_arc: &std::sync::Arc<std::sync::Mutex<deadcat_store::DeadcatStore>>,
    electrum_url: &str,
) {
    let chain = match chain_adapter::ElectrumChainAdapter::new(electrum_url) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("failed to connect to electrum for store sync: {e}");
            return;
        }
    };
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let candidates = match store_arc.lock() {
        Ok(mut store) => store
            .list_unpromoted_prediction_market_candidates()
            .unwrap_or_else(|e| {
                log::warn!("failed to list unpromoted prediction market candidates: {e}");
                Vec::new()
            }),
        Err(_) => {
            log::warn!("failed to lock store for candidate listing");
            return;
        }
    };

    let best_height = match chain.best_block_height() {
        Ok(h) => h,
        Err(e) => {
            log::warn!("failed to fetch best block height from {electrum_url}: {e}");
            return;
        }
    };

    let confirmed: Vec<_> = candidates
        .into_iter()
        .filter_map(|candidate| {
            let txid =
                deadcat_sdk::parse_market_creation_txid(&candidate.anchor.creation_txid).ok()?;
            match chain.irreversible_confirmation_at(best_height, txid.as_byte_array()) {
                Ok(Some((height, block_hash))) => {
                    Some((candidate.candidate_id, height, block_hash))
                }
                Ok(None) => None,
                Err(e) => {
                    log::warn!(
                        "failed to check confirmation for candidate {} ({}): {e}",
                        candidate.candidate_id,
                        candidate.anchor.creation_txid,
                    );
                    None
                }
            }
        })
        .collect();

    if let Ok(mut store) = store_arc.lock() {
        for (id, height, block_hash) in confirmed {
            if let Err(e) =
                store.promote_prediction_market_candidate(id, now_unix, height, block_hash)
            {
                log::warn!("failed to promote prediction market candidate {id}: {e}");
            }
        }
        if let Err(e) = store.purge_expired_prediction_market_candidates(now_unix) {
            log::warn!("failed to purge expired prediction market candidates: {e}");
        }
    }
}

#[tauri::command]
async fn sync_wallet(app: AppHandle) -> Result<AppState, String> {
    // Clone the Arc and drop the guard immediately so other commands
    // (e.g. get_wallet_address) are not blocked during the slow sync.
    let node = {
        let node_state = app.state::<NodeState>();
        let guard = node_state.node.lock().await;
        guard.as_ref().cloned().ok_or("Node not initialized")?
    };

    // Return current cached state immediately — the UI gets instant feedback.
    let state = {
        let app_handle = app.clone();
        let cached_balance = node.balance().ok().map(|m| {
            m.into_iter()
                .filter(|(_, v)| *v > 0)
                .map(|(k, v)| (k.to_string(), v))
                .collect()
        });
        tokio::task::spawn_blocking(move || {
            let manager = app_handle.state::<Mutex<AppStateManager>>();
            let mut mgr = manager
                .lock()
                .map_err(|_| "state lock failed".to_string())?;
            mgr.purge_stale_swaps();
            Ok::<_, String>(mgr.snapshot_with_balance(cached_balance))
        })
        .await
        .map_err(|e| format!("sync task failed: {e}"))??
    };

    // Fire the entire sync in the background.
    // Updated balance/transactions arrive via the wallet_snapshot event listener.
    let bg_app = app.clone();
    tauri::async_runtime::spawn(async move {
        // Electrum wallet scan + LMSR pool sync
        if let Err(e) = node.sync().await {
            log::warn!("background sync failed: {e}");
        }

        let electrum_url = node
            .electrum_url()
            .unwrap_or_else(|| node.default_electrum_url().to_string());
        let fresh_balance: Option<std::collections::HashMap<String, u64>> =
            node.balance().ok().map(|m| {
                m.into_iter()
                    .filter(|(_, v)| *v > 0)
                    .map(|(k, v)| (k.to_string(), v))
                    .collect()
            });

        // Run store candidate sync on a dedicated OS thread (not spawn_blocking)
        // to avoid occupying a tokio blocking thread during slow electrum I/O.
        let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();
        std::thread::spawn(move || {
            let manager = bg_app.state::<Mutex<AppStateManager>>();

            // Sync store candidates against the chain
            let store_arc = {
                let mgr = match manager.lock() {
                    Ok(m) => m,
                    Err(_) => {
                        let _ = done_tx.send(());
                        return;
                    }
                };
                mgr.store().cloned()
            };
            if let Some(store_arc) = store_arc {
                sync_store_candidates(&store_arc, &electrum_url);
            }

            // Emit final state with updated balance
            let mut mgr = match manager.lock() {
                Ok(m) => m,
                Err(_) => {
                    let _ = done_tx.send(());
                    return;
                }
            };
            mgr.bump_revision();
            let state = mgr.snapshot_with_balance(fresh_balance);
            emit_state(&bg_app, &state);
            let _ = done_tx.send(());
        });
        let _ = done_rx.await;
    });

    Ok(state)
}

#[tauri::command]
async fn get_wallet_balance(app: AppHandle) -> Result<wallet::types::WalletBalance, String> {
    let node = {
        let node_state = app.state::<NodeState>();
        let guard = node_state.node.lock().await;
        guard.as_ref().cloned().ok_or("Node not initialized")?
    };
    let balance_map = node.balance().map_err(|e| format!("{e}"))?;

    let mut assets = std::collections::HashMap::new();
    for (asset_id, amount) in balance_map.iter() {
        if *amount > 0 {
            assets.insert(asset_id.to_string(), *amount);
        }
    }
    Ok(wallet::types::WalletBalance { assets })
}

#[tauri::command]
async fn get_wallet_address(
    index: Option<u32>,
    app: AppHandle,
) -> Result<wallet::types::WalletAddress, String> {
    let node = {
        let node_state = app.state::<NodeState>();
        let guard = node_state.node.lock().await;
        guard.as_ref().cloned().ok_or("Node not initialized")?
    };
    let addr_result = node.address(index).await.map_err(|e| format!("{e}"))?;
    Ok(wallet::types::WalletAddress {
        index: addr_result.index(),
        address: addr_result.address().to_string(),
    })
}

#[tauri::command]
async fn get_wallet_transactions(
    app: AppHandle,
) -> Result<Vec<wallet::types::WalletTransaction>, String> {
    let node = {
        let node_state = app.state::<NodeState>();
        let guard = node_state.node.lock().await;
        guard.as_ref().cloned().ok_or("Node not initialized")?
    };
    let policy_asset = node.policy_asset();
    let txs = node.transactions().map_err(|e| format!("{e}"))?;
    Ok(txs
        .iter()
        .map(|tx| {
            let balance_change = tx.balance.get(&policy_asset).copied().unwrap_or(0);
            wallet::types::WalletTransaction {
                txid: tx.txid.to_string(),
                balance_change,
                fee: tx.fee,
                height: tx.height,
                timestamp: tx.timestamp,
                tx_type: tx.type_.clone(),
            }
        })
        .collect())
}

/// Drain the entire L-BTC balance to the given address, deducting the fee
/// from the send amount. No change output, no iteration — uses LWK's
/// native drain support. Stores the prepared tx for `confirm_send`.
#[tauri::command]
async fn estimate_max_send(
    address: String,
    app: AppHandle,
) -> Result<wallet::types::PrepareSendResult, String> {
    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;

    let tx_options = deadcat_sdk::TxOptions {
        fee_policy: deadcat_sdk::MinerFeePolicy::ConfirmationTargetBlocks {
            blocks: DEFAULT_FEE_TARGET_BLOCKS,
        },
    };

    let prepared = node
        .prepare_drain_lbtc(address, tx_options)
        .await
        .map_err(|e| format!("{e}"))?;

    let result = wallet::types::PrepareSendResult {
        address: prepared.address.clone(),
        amount_sat: prepared.amount_sat,
        fee_sat: prepared.prepared_tx.fee.amount_sat,
        fee: prepared.prepared_tx.fee.clone(),
    };

    drop(guard);
    let pending = app.state::<PendingSendState>();
    *pending.prepared.lock().await = Some(prepared);

    Ok(result)
}

/// Prepare a Liquid send transaction without broadcasting. Stores the
/// prepared tx in state and returns the fee breakdown for confirmation.
#[tauri::command]
async fn prepare_send(
    address: String,
    amount_sat: u64,
    tx_options: deadcat_sdk::TxOptions,
    app: AppHandle,
) -> Result<wallet::types::PrepareSendResult, String> {
    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    let prepared = node
        .prepare_send_lbtc(address, amount_sat, tx_options)
        .await
        .map_err(|e| format!("{e}"))?;

    let result = wallet::types::PrepareSendResult {
        address: prepared.address.clone(),
        amount_sat: prepared.amount_sat,
        fee_sat: prepared.prepared_tx.fee.amount_sat,
        fee: prepared.prepared_tx.fee.clone(),
    };
    drop(guard);

    let pending = app.state::<PendingSendState>();
    *pending.prepared.lock().await = Some(prepared);

    Ok(result)
}

/// Broadcast the previously prepared send transaction.
#[tauri::command]
async fn confirm_send(app: AppHandle) -> Result<wallet::types::LiquidSendResult, String> {
    let prepared = {
        let pending = app.state::<PendingSendState>();
        let mut guard = pending.prepared.lock().await;
        guard.take().ok_or("No pending send to confirm")?
    };

    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    let fee = prepared.prepared_tx.fee.clone();
    let (txid, fee_sat) = node
        .broadcast_prepared_send_lbtc(prepared)
        .await
        .map_err(|e| format!("{e}"))?;

    let wallet_balance = node.balance().ok().map(|m| {
        m.into_iter()
            .filter(|(_, v)| *v > 0)
            .map(|(k, v)| (k.to_string(), v))
            .collect()
    });
    drop(guard);

    let app_handle = app.clone();
    tokio::task::spawn_blocking(move || {
        let manager = app_handle.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        mgr.bump_revision();
        let state = mgr.snapshot_with_balance(wallet_balance);
        emit_state(&app_handle, &state);
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| format!("confirm_send state task failed: {e}"))??;

    Ok(wallet::types::LiquidSendResult {
        txid: txid.to_string(),
        fee_sat,
        fee,
    })
}

#[tauri::command]
async fn send_lbtc(
    address: String,
    amount_sat: u64,
    tx_options: deadcat_sdk::TxOptions,
    app: AppHandle,
) -> Result<wallet::types::LiquidSendResult, String> {
    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    let prepared = node
        .prepare_send_lbtc(address, amount_sat, tx_options)
        .await
        .map_err(|e| format!("{e}"))?;
    let fee = prepared.prepared_tx.fee.clone();
    let (txid, fee_sat) = node
        .broadcast_prepared_send_lbtc(prepared)
        .await
        .map_err(|e| format!("{e}"))?;

    // Grab updated balance from the snapshot (sync — no lock needed)
    let wallet_balance = node.balance().ok().map(|m| {
        m.into_iter()
            .filter(|(_, v)| *v > 0)
            .map(|(k, v)| (k.to_string(), v))
            .collect()
    });
    drop(guard);

    let app_handle = app.clone();
    tokio::task::spawn_blocking(move || {
        let manager = app_handle.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        mgr.bump_revision();
        let state = mgr.snapshot_with_balance(wallet_balance);
        emit_state(&app_handle, &state);
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| format!("send_lbtc state task failed: {e}"))??;

    Ok(wallet::types::LiquidSendResult {
        txid: txid.to_string(),
        fee_sat,
        fee,
    })
}

/// Return the cached mnemonic if the wallet is unlocked (no password needed).
#[tauri::command]
async fn get_cached_mnemonic(app: AppHandle) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let manager = app.state::<Mutex<AppStateManager>>();
        let mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        let persister = mgr.persister().ok_or("Persister not initialized")?;
        persister
            .cached()
            .map(|s| s.to_string())
            .ok_or_else(|| "Wallet is locked — mnemonic not cached".to_string())
    })
    .await
    .map_err(|e| format!("cached mnemonic task failed: {e}"))?
}

#[tauri::command]
async fn get_wallet_mnemonic(password: String, app: AppHandle) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let manager = app.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        let persister = mgr.persister_mut().ok_or("Persister not initialized")?;
        let mnemonic = persister.load(&password).map_err(|e| e.to_string())?;
        Ok(mnemonic)
    })
    .await
    .map_err(|e| format!("mnemonic task failed: {e}"))?
}

/// Return the word count of the mnemonic (12 or 24) after verifying password.
#[tauri::command]
async fn get_mnemonic_word_count(password: String, app: AppHandle) -> Result<usize, String> {
    tokio::task::spawn_blocking(move || {
        let manager = app.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        let persister = mgr.persister_mut().ok_or("Persister not initialized")?;
        persister
            .load_word_count(&password)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("mnemonic_word_count task failed: {e}"))?
}

/// Return a single mnemonic word by zero-based index after verifying password.
#[tauri::command]
async fn get_mnemonic_word(
    password: String,
    index: usize,
    app: AppHandle,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let manager = app.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        let persister = mgr.persister_mut().ok_or("Persister not initialized")?;
        persister
            .load_word(&password, index)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("mnemonic_word task failed: {e}"))?
}

// ============================================================================
// Payment Commands (Boltz)
// ============================================================================

#[tauri::command]
async fn pay_lightning_invoice(
    invoice: String,
    app: AppHandle,
) -> Result<payments::boltz::BoltzSubmarineSwapCreated, String> {
    let node = {
        let node_state = app.state::<NodeState>();
        let guard = node_state.node.lock().await;
        guard.as_ref().cloned().ok_or("Node not initialized")?
    };
    let refund_pubkey_hex = node
        .boltz_submarine_refund_pubkey_hex()
        .await
        .map_err(|e| format!("Wallet must be unlocked to initiate swap: {e}"))?;

    let boltz = {
        let manager = app.state::<Mutex<AppStateManager>>();
        let mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        mgr.boltz_service()
            .ok_or("Not initialized - select a network first")?
    };

    let created = boltz
        .create_submarine_swap(&invoice, &refund_pubkey_hex)
        .await
        .map_err(|e| e.to_string())?;

    let now = chrono::Utc::now().to_rfc3339();
    let saved_swap = PaymentSwap {
        id: created.id.clone(),
        flow: created.flow.clone(),
        network: created.network.clone(),
        status: created.status.clone(),
        invoice_amount_sat: created.invoice_amount_sat,
        expected_amount_sat: Some(created.expected_amount_sat),
        lockup_address: Some(created.lockup_address.clone()),
        timeout_block_height: Some(created.timeout_block_height),
        pair_hash: Some(created.pair_hash.clone()),
        invoice: Some(invoice),
        invoice_expiry_seconds: Some(created.invoice_expiry_seconds),
        invoice_expires_at: Some(created.invoice_expires_at.clone()),
        lockup_txid: None,
        created_at: now.clone(),
        updated_at: now,
    };

    let app_ref = app.clone();
    tokio::task::spawn_blocking(move || {
        let manager = app_ref.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        mgr.upsert_payment_swap(saved_swap);
        let state = mgr.snapshot();
        emit_state(&app_ref, &state);
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| format!("pay_lightning save task failed: {e}"))??;

    Ok(created)
}

#[tauri::command]
async fn create_lightning_receive(
    amount_sat: u64,
    app: AppHandle,
) -> Result<payments::boltz::BoltzLightningReceiveCreated, String> {
    let node = {
        let node_state = app.state::<NodeState>();
        let guard = node_state.node.lock().await;
        guard.as_ref().cloned().ok_or("Node not initialized")?
    };
    let claim_pubkey_hex = node
        .boltz_reverse_claim_pubkey_hex()
        .await
        .map_err(|e| format!("Wallet must be unlocked to initiate swap: {e}"))?;

    let boltz = {
        let manager = app.state::<Mutex<AppStateManager>>();
        let mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        mgr.boltz_service()
            .ok_or("Not initialized - select a network first")?
    };

    let created = boltz
        .create_lightning_receive(amount_sat, &claim_pubkey_hex)
        .await
        .map_err(|e| e.to_string())?;

    let now = chrono::Utc::now().to_rfc3339();
    let saved_swap = PaymentSwap {
        id: created.id.clone(),
        flow: created.flow.clone(),
        network: created.network.clone(),
        status: created.status.clone(),
        invoice_amount_sat: created.invoice_amount_sat,
        expected_amount_sat: Some(created.expected_onchain_amount_sat),
        lockup_address: Some(created.lockup_address.clone()),
        timeout_block_height: Some(created.timeout_block_height),
        pair_hash: Some(created.pair_hash.clone()),
        invoice: Some(created.invoice.clone()),
        invoice_expiry_seconds: Some(created.invoice_expiry_seconds),
        invoice_expires_at: Some(created.invoice_expires_at.clone()),
        lockup_txid: None,
        created_at: now.clone(),
        updated_at: now,
    };

    let app_ref = app.clone();
    tokio::task::spawn_blocking(move || {
        let manager = app_ref.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        mgr.upsert_payment_swap(saved_swap);
        let state = mgr.snapshot();
        emit_state(&app_ref, &state);
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| format!("lightning_receive save task failed: {e}"))??;

    Ok(created)
}

#[tauri::command]
async fn create_bitcoin_receive(
    amount_sat: u64,
    app: AppHandle,
) -> Result<payments::boltz::BoltzChainSwapCreated, String> {
    let node = {
        let node_state = app.state::<NodeState>();
        let guard = node_state.node.lock().await;
        guard.as_ref().cloned().ok_or("Node not initialized")?
    };
    let claim_pubkey_hex = node
        .boltz_reverse_claim_pubkey_hex()
        .await
        .map_err(|e| format!("Wallet must be unlocked to initiate swap: {e}"))?;
    let refund_pubkey_hex = node
        .boltz_submarine_refund_pubkey_hex()
        .await
        .map_err(|e| format!("Wallet must be unlocked to initiate swap: {e}"))?;

    let boltz = {
        let manager = app.state::<Mutex<AppStateManager>>();
        let mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        mgr.boltz_service()
            .ok_or("Not initialized - select a network first")?
    };

    let created = boltz
        .create_chain_swap_btc_to_lbtc(amount_sat, &claim_pubkey_hex, &refund_pubkey_hex)
        .await
        .map_err(|e| e.to_string())?;

    let now = chrono::Utc::now().to_rfc3339();
    let saved_swap = PaymentSwap {
        id: created.id.clone(),
        flow: created.flow.clone(),
        network: created.network.clone(),
        status: created.status.clone(),
        invoice_amount_sat: created.amount_sat,
        expected_amount_sat: Some(created.expected_amount_sat),
        lockup_address: Some(created.lockup_address.clone()),
        timeout_block_height: Some(created.timeout_block_height),
        pair_hash: Some(created.pair_hash.clone()),
        invoice: None,
        invoice_expiry_seconds: None,
        invoice_expires_at: None,
        lockup_txid: None,
        created_at: now.clone(),
        updated_at: now,
    };

    let app_ref = app.clone();
    tokio::task::spawn_blocking(move || {
        let manager = app_ref.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        mgr.upsert_payment_swap(saved_swap);
        let state = mgr.snapshot();
        emit_state(&app_ref, &state);
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| format!("bitcoin_receive save task failed: {e}"))??;

    Ok(created)
}

#[tauri::command]
async fn create_bitcoin_send(
    amount_sat: u64,
    app: AppHandle,
) -> Result<payments::boltz::BoltzChainSwapCreated, String> {
    let node = {
        let node_state = app.state::<NodeState>();
        let guard = node_state.node.lock().await;
        guard.as_ref().cloned().ok_or("Node not initialized")?
    };
    let claim_pubkey_hex = node
        .boltz_reverse_claim_pubkey_hex()
        .await
        .map_err(|e| format!("Wallet must be unlocked to initiate swap: {e}"))?;
    let refund_pubkey_hex = node
        .boltz_submarine_refund_pubkey_hex()
        .await
        .map_err(|e| format!("Wallet must be unlocked to initiate swap: {e}"))?;

    let boltz = {
        let manager = app.state::<Mutex<AppStateManager>>();
        let mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        mgr.boltz_service()
            .ok_or("Not initialized - select a network first")?
    };

    let created = boltz
        .create_chain_swap_lbtc_to_btc(amount_sat, &claim_pubkey_hex, &refund_pubkey_hex)
        .await
        .map_err(|e| e.to_string())?;

    let now = chrono::Utc::now().to_rfc3339();
    let saved_swap = PaymentSwap {
        id: created.id.clone(),
        flow: created.flow.clone(),
        network: created.network.clone(),
        status: created.status.clone(),
        invoice_amount_sat: created.amount_sat,
        expected_amount_sat: Some(created.expected_amount_sat),
        lockup_address: Some(created.lockup_address.clone()),
        timeout_block_height: Some(created.timeout_block_height),
        pair_hash: Some(created.pair_hash.clone()),
        invoice: None,
        invoice_expiry_seconds: None,
        invoice_expires_at: None,
        lockup_txid: None,
        created_at: now.clone(),
        updated_at: now,
    };

    let app_ref = app.clone();
    tokio::task::spawn_blocking(move || {
        let manager = app_ref.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        mgr.upsert_payment_swap(saved_swap);
        let state = mgr.snapshot();
        emit_state(&app_ref, &state);
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| format!("bitcoin_send save task failed: {e}"))??;

    Ok(created)
}

#[tauri::command]
async fn get_chain_swap_pairs(
    app: AppHandle,
) -> Result<payments::boltz::BoltzChainSwapPairsInfo, String> {
    let boltz = {
        let manager = app.state::<Mutex<AppStateManager>>();
        let mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        mgr.boltz_service()
            .ok_or("Not initialized - select a network first".to_string())?
    };

    boltz
        .get_chain_swap_pairs_info()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_payment_swaps(app: AppHandle) -> Result<Vec<PaymentSwap>, String> {
    tokio::task::spawn_blocking(move || {
        let manager = app.state::<Mutex<AppStateManager>>();
        let mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        Ok(mgr.payment_swaps().to_vec())
    })
    .await
    .map_err(|e| format!("list_swaps task failed: {e}"))?
}

#[tauri::command]
async fn refresh_payment_swap_status(
    swap_id: String,
    app: AppHandle,
) -> Result<PaymentSwap, String> {
    let swap_id_clone = swap_id.clone();
    let boltz = {
        let manager = app.state::<Mutex<AppStateManager>>();
        let mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        mgr.boltz_service()
            .ok_or("Not initialized - select a network first".to_string())?
    };

    let status = boltz
        .get_swap_status(&swap_id_clone)
        .await
        .map_err(|e| e.to_string())?;

    let app_ref = app.clone();
    let updated_swap = tokio::task::spawn_blocking(move || {
        let manager = app_ref.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        let existing = mgr
            .payment_swaps()
            .iter()
            .find(|swap| swap.id == swap_id_clone)
            .cloned()
            .ok_or_else(|| format!("Payment swap not found: {}", swap_id_clone))?;

        let mut updated = existing;
        updated.status = status.status;
        updated.lockup_txid = status.lockup_txid;
        updated.updated_at = chrono::Utc::now().to_rfc3339();

        mgr.upsert_payment_swap(updated.clone());
        let state = mgr.snapshot();
        emit_state(&app_ref, &state);
        Ok::<_, String>(updated)
    })
    .await
    .map_err(|e| format!("refresh_swap save task failed: {e}"))??;

    Ok(updated_swap)
}

// ============================================================================
// Legacy Commands (backward compatibility)
// ============================================================================

#[derive(serde::Serialize)]
pub struct ChainTipResponse {
    height: u32,
    block_hash: String,
    timestamp: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WalletNetwork {
    Liquid,
    LiquidTestnet,
    LiquidRegtest,
}

impl WalletNetwork {
    pub fn into_lwk(self) -> lwk_wollet::ElementsNetwork {
        match self {
            WalletNetwork::Liquid => lwk_wollet::ElementsNetwork::Liquid,
            WalletNetwork::LiquidTestnet => lwk_wollet::ElementsNetwork::LiquidTestnet,
            WalletNetwork::LiquidRegtest => lwk_wollet::ElementsNetwork::default_regtest(),
        }
    }
}

impl From<Network> for WalletNetwork {
    fn from(n: Network) -> Self {
        match n {
            Network::Mainnet => WalletNetwork::Liquid,
            Network::Testnet => WalletNetwork::LiquidTestnet,
            Network::Regtest => WalletNetwork::LiquidRegtest,
        }
    }
}

pub async fn fetch_chain_tip_inner(network: WalletNetwork) -> Result<ChainTipResponse, String> {
    let url = match network {
        WalletNetwork::Liquid => "https://blockstream.info/liquid/api",
        WalletNetwork::LiquidTestnet => "https://blockstream.info/liquidtestnet/api",
        WalletNetwork::LiquidRegtest => {
            return Err(
                "liquid-regtest tip fetch is not configured; use liquid or liquid-testnet"
                    .to_string(),
            )
        }
    };

    let mut client = lwk_wollet::asyncr::EsploraClient::new(network.into_lwk(), url);
    let tip = client
        .tip()
        .await
        .map_err(|e| format!("failed to fetch chain tip from LWK esplora: {e}"))?;

    Ok(ChainTipResponse {
        height: tip.height,
        block_hash: tip.block_hash().to_string(),
        timestamp: tip.time,
    })
}

#[tauri::command]
async fn fetch_chain_tip(network: WalletNetwork) -> Result<ChainTipResponse, String> {
    fetch_chain_tip_inner(network).await
}

// ============================================================================
// Auto-lock / activity commands
// ============================================================================

/// Record user activity to reset the auto-lock timer.
#[tauri::command]
async fn record_activity(app: AppHandle) -> Result<(), String> {
    let manager = app.state::<Mutex<AppStateManager>>();
    let mut mgr = manager
        .lock()
        .map_err(|_| "state lock failed".to_string())?;
    mgr.touch_activity();
    Ok(())
}

// ============================================================================
// Helpers
// ============================================================================

fn emit_state(app: &AppHandle, state: &AppState) {
    let _ = app.emit(APP_STATE_UPDATED_EVENT, state);
}

/// Exit the application (called after user confirms quit).
#[tauri::command]
fn confirm_quit(app: AppHandle) {
    app.exit(0);
}

#[derive(serde::Serialize)]
struct AppVersion {
    version: &'static str,
    commit: &'static str,
    modified: bool,
    /// SemVer build-metadata form: `0.1.0+abc1234` (or `+abc1234.modified`).
    display: String,
}

/// Version string assembled from Cargo package version + git short hash
/// embedded at build time. Shown in Settings.
#[tauri::command]
fn get_app_version() -> AppVersion {
    let version = env!("CARGO_PKG_VERSION");
    let commit = env!("GIT_HASH");
    let modified = env!("GIT_DIRTY") == "1";
    let display = if modified {
        format!("{version}+{commit}.modified")
    } else {
        format!("{version}+{commit}")
    };
    AppVersion {
        version,
        commit,
        modified,
        display,
    }
}

// ============================================================================
// App Entry Point
// ============================================================================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Install the rustls CryptoProvider before any TLS connections.
    let _ = rustls::crypto::ring::default_provider().install_default();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Info)
                .level_for("rustls", log::LevelFilter::Warn)
                .level_for("tungstenite", log::LevelFilter::Warn)
                .level_for("tokio_tungstenite", log::LevelFilter::Warn)
                .level_for("reqwest", log::LevelFilter::Warn)
                .level_for("tao", log::LevelFilter::Warn)
                .level_for("lwk_wollet", log::LevelFilter::Warn)
                .build(),
        )
        .setup(|app| {
            // Bridge `tracing` events (nostr-connect / nostr-sdk internals)
            // to the `log` crate so tauri-plugin-log captures them. Useful
            // for diagnosing NIP-46 handshake stalls.
            let _ = tracing_log::LogTracer::init();
            let filter =
                tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                    tracing_subscriber::EnvFilter::new(
                        "nostr=debug,nostr_connect=debug,nostr_sdk=info",
                    )
                });
            let _ = tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_writer(std::io::stderr)
                .try_init();

            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to get app data directory");

            let mut manager = AppStateManager::new(app_data_dir);
            manager.initialize();

            // Default to Testnet on first launch
            if manager.is_first_launch() {
                eprintln!("First launch detected - defaulting to Testnet network");
                manager.set_network(Network::Testnet);
            }

            // Load source npub from config file (creates default if missing)
            let source_npub = manager.load_source_npub();

            let nostr_state = NostrAppState {
                relay_list: std::sync::RwLock::new(
                    discovery::DEFAULT_RELAYS
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                ),
                source_npub: std::sync::RwLock::new(source_npub),
            };

            app.manage(Mutex::new(manager));
            app.manage(NodeState::default());
            app.manage(PendingSendState::default());
            app.manage(nostr_state);
            app.manage(WalletStoreState::default());

            // Load persisted relay market cache for instant cold-start display
            commands::load_relay_cache(app.handle());

            // Custom macOS menu — Cmd+Q routes through frontend quit confirmation
            let quit_item = MenuItemBuilder::with_id("confirm-quit", "Quit Deadcat Live")
                .accelerator("CmdOrCtrl+Q")
                .build(app)?;
            let app_submenu = SubmenuBuilder::new(app, "Deadcat Live")
                .items(&[&quit_item])
                .build()?;
            let edit_submenu = SubmenuBuilder::new(app, "Edit")
                .items(&[
                    &PredefinedMenuItem::undo(app, None)?,
                    &PredefinedMenuItem::redo(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::cut(app, None)?,
                    &PredefinedMenuItem::copy(app, None)?,
                    &PredefinedMenuItem::paste(app, None)?,
                    &PredefinedMenuItem::select_all(app, None)?,
                ])
                .build()?;
            let menu = MenuBuilder::new(app)
                .item(&app_submenu)
                .item(&edit_submenu)
                .build()?;
            app.set_menu(menu)?;
            app.on_menu_event(move |app, event| {
                if event.id() == "confirm-quit" {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.emit("close-requested", ());
                    }
                }
            });

            // Spawn auto-lock background timer
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let interval_secs = std::cmp::max(AUTO_LOCK_TIMEOUT_SECS / 5, 10);
                let mut interval =
                    tokio::time::interval(std::time::Duration::from_secs(interval_secs));
                loop {
                    interval.tick().await;

                    // Check auto-lock: lock the node's wallet if timeout elapsed
                    let should_lock = {
                        let manager = app_handle.state::<Mutex<AppStateManager>>();
                        let mut mgr = match manager.lock() {
                            Ok(m) => m,
                            Err(_) => continue,
                        };
                        mgr.check_auto_lock()
                    };

                    if should_lock {
                        // Also lock via the node
                        let node_state = app_handle.state::<NodeState>();
                        let guard = node_state.node.lock().await;
                        if let Some(node) = guard.as_ref() {
                            node.lock_wallet();
                        }
                        drop(guard);

                        log::info!("auto-lock: wallet locked after inactivity");
                        let snapshot = {
                            let manager = app_handle.state::<Mutex<AppStateManager>>();
                            manager.lock().ok().map(|mgr| mgr.snapshot())
                        };
                        if let Some(state) = snapshot {
                            emit_state(&app_handle, &state);
                        }
                    }
                }
            });

            // Spawn background wallet sync loop — keeps balances and
            // transaction confirmations up to date while the wallet is
            // unlocked, without requiring a manual sync button press.
            let sync_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(
                    WALLET_SYNC_INTERVAL_SECS,
                ));
                // The first tick fires immediately; skip it so we don't
                // race with the initial unlock sync.
                interval.tick().await;

                loop {
                    interval.tick().await;

                    // Clone the Arc out of the mutex immediately so we
                    // don't hold the node lock during the slow sync.
                    let node = {
                        let node_state = sync_handle.state::<NodeState>();
                        let guard = node_state.node.lock().await;
                        match guard.as_ref().cloned() {
                            Some(n) => n,
                            None => continue,
                        }
                    };

                    if !node.is_wallet_unlocked() {
                        continue;
                    }

                    if let Err(e) = node.sync().await {
                        log::debug!("background wallet sync: {e}");
                        continue;
                    }

                    // Emit updated state so the frontend picks up new
                    // confirmations and balance changes.
                    let fresh_balance: Option<std::collections::HashMap<String, u64>> =
                        node.balance().ok().map(|m| {
                            m.into_iter()
                                .filter(|(_, v)| *v > 0)
                                .map(|(k, v)| (k.to_string(), v))
                                .collect()
                        });

                    let bg_app = sync_handle.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        let manager = bg_app.state::<Mutex<AppStateManager>>();
                        let mut mgr = match manager.lock() {
                            Ok(m) => m,
                            Err(_) => return,
                        };
                        mgr.bump_revision();
                        let state = mgr.snapshot_with_balance(fresh_balance);
                        emit_state(&bg_app, &state);
                    })
                    .await;
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Network
            is_first_launch,
            set_network,
            // App state
            get_app_state,
            // Wallet
            get_wallet_status,
            generate_mnemonic,
            create_wallet,
            restore_wallet,
            unlock_wallet,
            lock_wallet,
            delete_wallet,
            sync_wallet,
            get_wallet_balance,
            get_wallet_address,
            get_wallet_transactions,
            get_wallet_mnemonic,
            get_cached_mnemonic,
            get_mnemonic_word_count,
            get_mnemonic_word,
            estimate_max_send,
            prepare_send,
            confirm_send,
            send_lbtc,
            // Activity / auto-lock
            record_activity,
            // Payments (Boltz)
            pay_lightning_invoice,
            create_lightning_receive,
            create_bitcoin_receive,
            create_bitcoin_send,
            get_chain_swap_pairs,
            list_payment_swaps,
            refresh_payment_swap_status,
            // Legacy
            fetch_chain_tip,
            confirm_quit,
            get_app_version,
            // SDK / Nostr
            commands::init_nostr_identity,
            commands::generate_nostr_identity,
            commands::preview_nostr_identity,
            commands::get_nostr_identity,
            commands::export_nostr_nsec,
            commands::delete_nostr_identity,
            commands::import_nostr_nsec,
            // NIP-46 remote signing
            commands::initiate_nostrconnect,
            commands::connect_nip46_bunker,
            commands::disconnect_nip46,
            commands::get_nip46_status,
            commands::discover_contracts,
            commands::publish_contract,
            commands::oracle_attest,
            commands::backup_mnemonic_to_nostr,
            commands::restore_mnemonic_from_nostr,
            commands::check_nostr_backup,
            commands::delete_nostr_backup,
            commands::get_source_npub,
            commands::set_source_npub,
            commands::get_default_source_npub,
            commands::get_relay_list,
            commands::set_relay_list,
            commands::fetch_nip65_relay_list,
            commands::add_relay,
            commands::remove_relay,
            commands::fetch_nostr_profile,
            commands::preview_nostr_profile,
            commands::derive_npub_from_nsec,
            commands::publish_nostr_profile,
            commands::create_nip98_auth,
            commands::fetch_market_comments,
            commands::publish_market_comment,
            commands::delete_market_comment,
            commands::sign_zap_request,
            commands::export_identity_file,
            commands::open_downloads_folder,
            commands::import_identity_file,
            commands::create_contract_onchain,
            commands::issue_tokens,
            commands::cancel_tokens,
            commands::resolve_market,
            commands::redeem_tokens,
            commands::redeem_expired,
            commands::get_market_state,
            commands::quote_trade,
            commands::execute_trade,
            commands::get_wallet_utxos,
            commands::list_contracts,
            commands::fetch_orders,
            commands::create_limit_order,
            commands::cancel_limit_order,
            commands::list_own_orders,
            // LMSR Pools
            commands::generate_lmsr_table,
            commands::build_pool_params_json,
            commands::create_lmsr_pool,
            commands::scan_lmsr_pool,
            commands::adjust_lmsr_pool,
            commands::close_lmsr_pool,
            commands::list_lmsr_pools,
            commands::get_price_history,
            commands::get_pool_price_history,
            // Wallet store (SDK)
            wallet_store::create_software_signer,
            wallet_store::create_wollet,
            wallet_store::wallet_new_address,
            wallet_store::wallet_signer_id,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Prevent immediate close — let the frontend handle confirmation
                api.prevent_close();
                let _ = window.emit("close-requested", ());
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
