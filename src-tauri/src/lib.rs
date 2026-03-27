mod chain_adapter;
pub mod commands;
pub mod discovery;
mod payments;
pub mod state;
pub mod wallet;
mod wallet_store;

use std::sync::Mutex;

use deadcat_sdk::elements::hashes::Hash as _;
use deadcat_store::ChainSource;
use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager};

use state::{AppState, AppStateManager, PaymentSwap, AUTO_LOCK_TIMEOUT_SECS};

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
}

impl Default for NodeState {
    fn default() -> Self {
        Self {
            node: tokio::sync::Mutex::new(None),
        }
    }
}

/// Minimal state for the legacy wallet_store commands.
#[derive(Default)]
pub struct WalletStoreState {
    pub wallet_store: wallet_store::WalletStore,
}

/// App-layer Nostr state: relay list (keys come from the node).
pub struct NostrAppState {
    pub relay_list: std::sync::RwLock<Vec<String>>,
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
    tokio::task::spawn_blocking(move || {
        let manager = app.state::<Mutex<AppStateManager>>();
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
    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let is_unlocked = guard
        .as_ref()
        .map(|n| n.is_wallet_unlocked())
        .unwrap_or(false);
    drop(guard);

    tokio::task::spawn_blocking(move || {
        let manager = app.state::<Mutex<AppStateManager>>();
        let mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        Ok(mgr.wallet_status_with_unlock(is_unlocked))
    })
    .await
    .map_err(|e| format!("wallet_status task failed: {e}"))?
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
    let app_handle = app.clone();
    tokio::task::spawn_blocking(move || {
        let manager = app_handle.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;

        // Validate mnemonic
        let _: bip39::Mnemonic = mnemonic
            .parse()
            .map_err(|_| "Invalid mnemonic".to_string())?;

        let persister = mgr.persister_mut().ok_or("Persister not initialized")?;
        persister
            .save(&mnemonic, &password)
            .map_err(|e| e.to_string())?;

        mgr.bump_revision();
        let state = mgr.snapshot();
        emit_state(&app_handle, &state);
        Ok(state)
    })
    .await
    .map_err(|e| format!("restore_wallet task failed: {e}"))?
}

#[tauri::command]
async fn unlock_wallet(password: String, app: AppHandle) -> Result<AppState, String> {
    let app_handle = app.clone();

    // 1. Decrypt mnemonic (blocking — Argon2 KDF)
    let (mnemonic, network, data_dir) = tokio::task::spawn_blocking({
        let app_ref = app_handle.clone();
        move || {
            let manager = app_ref.state::<Mutex<AppStateManager>>();
            let mut mgr = manager
                .lock()
                .map_err(|_| "state lock failed".to_string())?;
            let network = mgr.network().ok_or("Network not initialized")?;

            let persister = mgr.persister_mut().ok_or("Persister not initialized")?;
            let mnemonic = if let Some(cached) = persister.cached() {
                cached.to_string()
            } else {
                persister.load(&password).map_err(|e| e.to_string())?
            };

            let data_dir = mgr.app_data_dir.clone();
            Ok::<_, String>((mnemonic, network, data_dir))
        }
    })
    .await
    .map_err(|e| format!("unlock task failed: {e}"))??;

    // 2. Unlock the wallet via the node (needs node lock)
    let node_state = app_handle.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard
        .as_ref()
        .ok_or("Node not initialized — call init_nostr_identity first")?;

    let sdk_network = state::to_sdk_network(network);
    let electrum_url = sdk_network.default_electrum_url();
    node.unlock_wallet(&mnemonic, electrum_url, &data_dir)
        .map_err(|e| format!("{e}"))?;
    drop(guard);

    // 3. Update app state
    let state = tokio::task::spawn_blocking({
        let app_ref = app_handle.clone();
        move || {
            let manager = app_ref.state::<Mutex<AppStateManager>>();
            let mut mgr = manager
                .lock()
                .map_err(|_| "state lock failed".to_string())?;
            mgr.set_wallet_unlocked(true);
            mgr.purge_stale_swaps();
            mgr.touch_activity();
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
    // Lock the node's wallet
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

        // Store sync + state emission in a single blocking task
        let _ = tokio::task::spawn_blocking(move || {
            let manager = bg_app.state::<Mutex<AppStateManager>>();

            // Sync store candidates against the chain
            let store_arc = {
                let mgr = match manager.lock() {
                    Ok(m) => m,
                    Err(_) => return,
                };
                mgr.store().cloned()
            };
            if let Some(store_arc) = store_arc {
                sync_store_candidates(&store_arc, &electrum_url);
            }

            // Emit final state with updated balance
            let mut mgr = match manager.lock() {
                Ok(m) => m,
                Err(_) => return,
            };
            mgr.bump_revision();
            let state = mgr.snapshot_with_balance(fresh_balance);
            emit_state(&bg_app, &state);
        })
        .await;
    });

    Ok(state)
}

#[tauri::command]
async fn get_wallet_balance(app: AppHandle) -> Result<wallet::types::WalletBalance, String> {
    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
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
    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    let policy_asset = node.policy_asset().await.map_err(|e| format!("{e}"))?;
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

// ============================================================================
// App Entry Point
// ============================================================================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Install the rustls CryptoProvider before any TLS connections.
    let _ = rustls::crypto::ring::default_provider().install_default();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
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

            app.manage(Mutex::new(manager));
            app.manage(NodeState::default());
            app.manage(NostrAppState::default());
            app.manage(WalletStoreState::default());

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
            get_mnemonic_word_count,
            get_mnemonic_word,
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
            // SDK / Nostr
            commands::init_nostr_identity,
            commands::generate_nostr_identity,
            commands::get_nostr_identity,
            commands::export_nostr_nsec,
            commands::delete_nostr_identity,
            commands::import_nostr_nsec,
            commands::discover_contracts,
            commands::publish_contract,
            commands::oracle_attest,
            commands::backup_mnemonic_to_nostr,
            commands::restore_mnemonic_from_nostr,
            commands::check_nostr_backup,
            commands::delete_nostr_backup,
            commands::get_relay_list,
            commands::set_relay_list,
            commands::fetch_nip65_relay_list,
            commands::add_relay,
            commands::remove_relay,
            commands::fetch_nostr_profile,
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
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
