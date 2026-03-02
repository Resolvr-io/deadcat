use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use deadcat_store::MarketFilter;
use nostr_sdk::prelude::*;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

use crate::discovery::{
    self, ContractMetadata, CreateContractRequest, DiscoveredMarket, IdentityResponse,
};
use crate::state::AppStateManager;
use crate::{CachedTradeQuote, NodeState, NostrAppState};

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

// ── Helpers ──────────────────────────────────────────────────────────────

fn validate_request(request: &CreateContractRequest) -> Result<(), String> {
    if request.question.trim().is_empty() || request.question.len() > 140 {
        return Err("question must be 1-140 characters".to_string());
    }
    if request.description.trim().is_empty() || request.description.len() > 280 {
        return Err("description must be 1-280 characters".to_string());
    }
    if request.resolution_source.trim().is_empty() || request.resolution_source.len() > 120 {
        return Err("resolution_source must be 1-120 characters".to_string());
    }
    if request.collateral_per_token == 0 {
        return Err("collateral_per_token must be > 0".to_string());
    }
    Ok(())
}

fn decode_hex32(value: &str, field: &str) -> Result<[u8; 32], String> {
    hex::decode(value)
        .map_err(|e| format!("invalid {field}: {e}"))?
        .try_into()
        .map_err(|_| format!("{field} must be exactly 32 bytes"))
}

fn parse_order_direction(direction: &str) -> Result<deadcat_sdk::OrderDirection, String> {
    match direction {
        "sell-base" => Ok(deadcat_sdk::OrderDirection::SellBase),
        "sell-quote" => Ok(deadcat_sdk::OrderDirection::SellQuote),
        _ => Err("direction must be 'sell-base' or 'sell-quote'".to_string()),
    }
}

const MARKET_TRADE_QUOTE_TTL_SECS: u64 = 30;
const MARKET_TRADE_EXEC_FEE_SATS: u64 = 500;

fn parse_trade_side(side: &str) -> Result<deadcat_sdk::TradeSide, String> {
    match side {
        "yes" => Ok(deadcat_sdk::TradeSide::Yes),
        "no" => Ok(deadcat_sdk::TradeSide::No),
        _ => Err("side must be 'yes' or 'no'".to_string()),
    }
}

fn parse_trade_direction(direction: &str) -> Result<deadcat_sdk::TradeDirection, String> {
    match direction {
        "buy" => Ok(deadcat_sdk::TradeDirection::Buy),
        "sell" => Ok(deadcat_sdk::TradeDirection::Sell),
        _ => Err("direction must be 'buy' or 'sell'".to_string()),
    }
}

trait ExpiringEntry {
    fn expires_at(&self) -> Instant;
}

impl ExpiringEntry for CachedTradeQuote {
    fn expires_at(&self) -> Instant {
        self.expires_at
    }
}

fn prune_expired_entries<T: ExpiringEntry>(cache: &mut HashMap<String, T>, now: Instant) {
    cache.retain(|_, entry| entry.expires_at() > now);
}

fn take_unexpired_entry<T: ExpiringEntry>(
    cache: &mut HashMap<String, T>,
    key: &str,
    now: Instant,
    missing_error: &str,
) -> Result<T, String> {
    prune_expired_entries(cache, now);
    cache.remove(key).ok_or_else(|| missing_error.to_string())
}

fn market_quote_expires_at_unix(now_unix: u64, ttl_secs: u64) -> u64 {
    now_unix.saturating_add(ttl_secs)
}

fn market_quote_id() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

async fn compute_tip_and_now(
    network: crate::WalletNetwork,
) -> Result<(crate::ChainTipResponse, u64), String> {
    let tip = crate::fetch_chain_tip_inner(network).await?;
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| format!("time error: {e}"))?
        .as_secs();
    Ok((tip, now_unix))
}

/// Bump state revision and emit to frontend.
async fn bump_revision_and_emit(app: &tauri::AppHandle) -> Result<(), String> {
    let app_handle = app.clone();
    tokio::task::spawn_blocking(move || {
        let manager = app_handle.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        mgr.bump_revision();
        let state = mgr.snapshot();
        let _ = app_handle.emit(crate::APP_STATE_UPDATED_EVENT, &state);
        Ok::<_, String>(())
    })
    .await
    .map_err(|e| format!("task join: {e}"))??;
    Ok(())
}

async fn run_node_mutation<T, F>(app: &tauri::AppHandle, op: F) -> Result<T, String>
where
    F: for<'a> FnOnce(
        &'a deadcat_sdk::DeadcatNode<deadcat_store::DeadcatStore>,
    ) -> BoxFuture<'a, Result<T, String>>,
{
    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    let result = op(node).await?;
    drop(guard);

    bump_revision_and_emit(app).await?;
    Ok(result)
}

async fn run_node_query<T, F>(app: &tauri::AppHandle, op: F) -> Result<T, String>
where
    F: for<'a> FnOnce(
        &'a deadcat_sdk::DeadcatNode<deadcat_store::DeadcatStore>,
    ) -> BoxFuture<'a, Result<T, String>>,
{
    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    op(node).await
}

/// Get Nostr keys and a connected client from the node.
async fn get_keys_and_client(app: &tauri::AppHandle) -> Result<(Keys, nostr_sdk::Client), String> {
    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard
        .as_ref()
        .ok_or("Node not initialized — call init_nostr_identity first")?;
    let keys = node.keys().clone();
    let client = node.discovery().client().clone();
    drop(guard);

    // Ensure client has relays connected
    if client.relays().await.is_empty() {
        let nostr_state = app.state::<NostrAppState>();
        let relays = nostr_state
            .relay_list
            .read()
            .map_err(|_| "failed to read relay_list".to_string())?
            .clone();
        for url in &relays {
            let _ = client.add_relay(url.as_str()).await;
        }
        client.connect_with_timeout(Duration::from_secs(5)).await;
    }

    Ok((keys, client))
}

/// Subscribe a newly discovered market's scripts to the chain watcher.
async fn subscribe_discovered_market_to_watcher(
    app: &tauri::AppHandle,
    market: &deadcat_sdk::DiscoveredMarket,
) {
    use deadcat_sdk::ScriptOwner;

    let Ok(params) = deadcat_sdk::discovered_market_to_contract_params(market) else {
        return;
    };
    let market_id = params.market_id();

    let contract = match deadcat_sdk::CompiledPredictionMarket::new(params) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("subscribe_discovered_market: compile failed: {e}");
            return;
        }
    };

    let node_state = app.state::<NodeState>();
    let handle = node_state.watcher_handle.lock().await;
    let Some(ref watcher) = *handle else { return };

    for state in [
        deadcat_sdk::MarketState::Dormant,
        deadcat_sdk::MarketState::Unresolved,
        deadcat_sdk::MarketState::ResolvedYes,
        deadcat_sdk::MarketState::ResolvedNo,
        deadcat_sdk::MarketState::Expired,
    ] {
        let spk = contract.script_pubkey(state);
        watcher.subscribe(
            spk.to_bytes(),
            ScriptOwner::Market {
                market_id: market_id.0,
            },
        );
    }
}

/// Subscribe a newly discovered pool's script to the chain watcher.
async fn subscribe_discovered_pool_to_watcher(
    app: &tauri::AppHandle,
    pool: &deadcat_sdk::DiscoveredPool,
) {
    use deadcat_sdk::ScriptOwner;

    let Ok(params) = deadcat_sdk::discovered_pool_to_amm_params(pool) else {
        return;
    };
    let pool_id = deadcat_sdk::PoolId::from_params(&params);

    let contract = match deadcat_sdk::CompiledAmmPool::new(params) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("subscribe_discovered_pool: compile failed: {e}");
            return;
        }
    };

    let node_state = app.state::<NodeState>();
    let handle = node_state.watcher_handle.lock().await;
    let Some(ref watcher) = *handle else { return };

    let spk = contract.script_pubkey(pool.issued_lp);
    watcher.subscribe(spk.to_bytes(), ScriptOwner::Pool { pool_id });
}

/// Construct a DeadcatNode from loaded keys and store it in NodeState.
/// Called whenever Nostr identity is loaded/generated/imported.
async fn construct_and_store_node(
    app: &tauri::AppHandle,
    keys: nostr_sdk::Keys,
) -> Result<(), String> {
    // Replacing identity should always tear down the previous chain watcher.
    let node_state = app.state::<NodeState>();
    node_state.shutdown_watcher().await;
    node_state.trade_quote_cache.lock().await.clear();

    let (sdk_network, store_arc) = {
        let manager = app.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        let network = mgr.network().ok_or("Network not initialized")?;
        let store = mgr.store().cloned().ok_or("Store not initialized")?;
        // Reset wallet state since we're constructing a new node
        mgr.set_wallet_unlocked(false);
        if let Some(persister) = mgr.persister_mut() {
            persister.clear_cache();
        }
        (crate::state::to_sdk_network(network), store)
    };

    let relays = {
        let nostr_state = app.state::<NostrAppState>();
        let guard = nostr_state
            .relay_list
            .read()
            .map_err(|_| "failed to read relay_list".to_string())?;
        guard.clone()
    };

    let config = deadcat_sdk::DiscoveryConfig {
        relays,
        ..Default::default()
    };

    let (node, mut rx) = deadcat_sdk::DeadcatNode::with_store(keys, sdk_network, store_arc, config);
    let mut snapshot_rx = node.subscribe_snapshot();

    // Cancel any previous periodic reconciliation task
    if let Some(handle) = node_state.reconcile_task.lock().await.take() {
        handle.abort();
    }
    // Replace any existing node (drops old node if any)
    let mut guard = node_state.node.lock().await;
    *guard = Some(node);

    // Start the background Nostr subscription loop
    if let Some(node) = guard.as_ref() {
        if let Err(e) = node.start_subscription().await {
            log::warn!("failed to start discovery subscription: {e}");
        }
    }
    drop(guard);

    // Background reconciliation: re-send stored Nostr events to relays.
    // A single task handles both the initial reconciliation (after a short
    // delay) and periodic re-sends every 30 minutes. One handle in
    // `reconcile_task` means identity-switch aborts everything cleanly.
    let app_reconcile = app.clone();
    let reconcile_handle = tokio::spawn(async move {
        // Small delay to let the subscription loop settle
        tokio::time::sleep(Duration::from_secs(5)).await;

        let node_state = app_reconcile.state::<NodeState>();

        // Startup reconciliation
        let prepared = {
            let guard = node_state.node.lock().await;
            guard.as_ref().and_then(|n| n.prepare_reconciliation().ok())
        };
        if let Some((client, events)) = prepared {
            let stats = deadcat_sdk::send_reconciliation_events(&client, &events).await;
            log::info!("startup reconciliation complete: {stats}");
        }

        // Then every 30 minutes
        let mut interval = tokio::time::interval(Duration::from_secs(30 * 60));
        loop {
            interval.tick().await;
            let prepared = {
                let guard = node_state.node.lock().await;
                guard.as_ref().and_then(|n| n.prepare_reconciliation().ok())
            };
            match prepared {
                Some((client, events)) => {
                    let stats = deadcat_sdk::send_reconciliation_events(&client, &events).await;
                    log::info!("periodic reconciliation: {stats}");
                }
                None => break, // Node was destroyed — stop the timer
            }
        }
    });

    *node_state.reconcile_task.lock().await = Some(reconcile_handle);

    // Forward discovery events to the frontend + subscribe new contracts to chain watcher
    let app_handle = app.clone();
    tokio::spawn(async move {
        use deadcat_sdk::DiscoveryEvent;
        while let Ok(event) = rx.recv().await {
            match event {
                DiscoveryEvent::MarketDiscovered(ref m) => {
                    let _ = app_handle.emit("discovery:market", m);
                    subscribe_discovered_market_to_watcher(&app_handle, m).await;
                }
                DiscoveryEvent::OrderDiscovered(o) => {
                    let _ = app_handle.emit("discovery:order", &o);
                }
                DiscoveryEvent::AttestationDiscovered(a) => {
                    let _ = app_handle.emit("discovery:attestation", &a);
                }
                DiscoveryEvent::PoolDiscovered(ref p) => {
                    let _ = app_handle.emit("discovery:pool", p);
                    subscribe_discovered_pool_to_watcher(&app_handle, p).await;
                }
            }
        }
        log::info!("discovery event forwarding loop ended");
    });

    // Forward wallet snapshot changes to the frontend
    let app_snapshot = app.clone();
    let policy_asset = sdk_network.into_lwk().policy_asset();
    tokio::spawn(async move {
        while snapshot_rx.changed().await.is_ok() {
            let payload = {
                let snap = snapshot_rx.borrow_and_update();
                snap.as_ref().map(|s| {
                    crate::wallet::types::WalletSnapshotEvent::from_snapshot(s, &policy_asset)
                })
            };
            let _ = app_snapshot.emit("wallet_snapshot", &payload);
        }
        log::info!("wallet snapshot forwarding loop ended");
    });

    Ok(())
}

fn market_state_to_u8(state: deadcat_sdk::MarketState) -> u8 {
    match state {
        deadcat_sdk::MarketState::Dormant => 0,
        deadcat_sdk::MarketState::Unresolved => 1,
        deadcat_sdk::MarketState::ResolvedYes => 2,
        deadcat_sdk::MarketState::ResolvedNo => 3,
        deadcat_sdk::MarketState::Expired => 4,
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct MakerOrderParamsPayload {
    pub base_asset_id_hex: String,
    pub quote_asset_id_hex: String,
    pub price: u64,
    pub min_fill_lots: u64,
    pub min_remainder_lots: u64,
    pub direction: String,
    pub maker_receive_spk_hash_hex: String,
    pub cosigner_pubkey_hex: String,
    pub maker_pubkey_hex: String,
}

impl MakerOrderParamsPayload {
    fn from_params(params: deadcat_sdk::MakerOrderParams) -> Self {
        Self {
            base_asset_id_hex: hex::encode(params.base_asset_id),
            quote_asset_id_hex: hex::encode(params.quote_asset_id),
            price: params.price,
            min_fill_lots: params.min_fill_lots,
            min_remainder_lots: params.min_remainder_lots,
            direction: match params.direction {
                deadcat_sdk::OrderDirection::SellBase => "sell-base".to_string(),
                deadcat_sdk::OrderDirection::SellQuote => "sell-quote".to_string(),
            },
            maker_receive_spk_hash_hex: hex::encode(params.maker_receive_spk_hash),
            cosigner_pubkey_hex: hex::encode(params.cosigner_pubkey),
            maker_pubkey_hex: hex::encode(params.maker_pubkey),
        }
    }

    fn try_into_params(self) -> Result<deadcat_sdk::MakerOrderParams, String> {
        Ok(deadcat_sdk::MakerOrderParams {
            base_asset_id: decode_hex32(&self.base_asset_id_hex, "base_asset_id_hex")?,
            quote_asset_id: decode_hex32(&self.quote_asset_id_hex, "quote_asset_id_hex")?,
            price: self.price,
            min_fill_lots: self.min_fill_lots,
            min_remainder_lots: self.min_remainder_lots,
            direction: parse_order_direction(&self.direction)?,
            maker_receive_spk_hash: decode_hex32(
                &self.maker_receive_spk_hash_hex,
                "maker_receive_spk_hash_hex",
            )?,
            cosigner_pubkey: decode_hex32(&self.cosigner_pubkey_hex, "cosigner_pubkey_hex")?,
            maker_pubkey: decode_hex32(&self.maker_pubkey_hex, "maker_pubkey_hex")?,
        })
    }
}

// =========================================================================
// Nostr identity commands
// =========================================================================

#[tauri::command]
pub async fn init_nostr_identity(
    app: tauri::AppHandle,
) -> Result<Option<IdentityResponse>, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("failed to get app data dir: {e}"))?;

    match discovery::load_keys(&app_data_dir)? {
        Some(keys) => {
            let response = IdentityResponse {
                pubkey_hex: keys.public_key().to_hex(),
                npub: keys
                    .public_key()
                    .to_bech32()
                    .map_err(|e| format!("bech32 error: {e}"))?,
            };
            construct_and_store_node(&app, keys).await?;
            Ok(Some(response))
        }
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn generate_nostr_identity(app: tauri::AppHandle) -> Result<IdentityResponse, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("failed to get app data dir: {e}"))?;

    let keys = discovery::generate_keys(&app_data_dir)?;

    let response = IdentityResponse {
        pubkey_hex: keys.public_key().to_hex(),
        npub: keys
            .public_key()
            .to_bech32()
            .map_err(|e| format!("bech32 error: {e}"))?,
    };

    construct_and_store_node(&app, keys).await?;
    Ok(response)
}

#[tauri::command]
pub async fn get_nostr_identity(app: tauri::AppHandle) -> Result<Option<IdentityResponse>, String> {
    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    match guard.as_ref() {
        Some(node) => {
            let keys = node.keys();
            Ok(Some(IdentityResponse {
                pubkey_hex: keys.public_key().to_hex(),
                npub: keys
                    .public_key()
                    .to_bech32()
                    .map_err(|e| format!("bech32 error: {e}"))?,
            }))
        }
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn import_nostr_nsec(
    nsec: String,
    app: tauri::AppHandle,
) -> Result<IdentityResponse, String> {
    let secret_key =
        SecretKey::from_bech32(nsec.trim()).map_err(|e| format!("invalid nsec: {e}"))?;
    let keys = Keys::new(secret_key);

    // Persist to disk
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("failed to get app data dir: {e}"))?;
    let key_path = app_data_dir.join("nostr_identity.key");
    std::fs::write(&key_path, keys.secret_key().to_secret_hex())
        .map_err(|e| format!("failed to write key file: {e}"))?;

    let response = IdentityResponse {
        pubkey_hex: keys.public_key().to_hex(),
        npub: keys
            .public_key()
            .to_bech32()
            .map_err(|e| format!("bech32 error: {e}"))?,
    };

    construct_and_store_node(&app, keys).await?;
    Ok(response)
}

#[tauri::command]
pub async fn export_nostr_nsec(app: tauri::AppHandle) -> Result<String, String> {
    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard
        .as_ref()
        .ok_or_else(|| "Nostr identity not initialized".to_string())?;

    node.keys()
        .secret_key()
        .to_bech32()
        .map_err(|e| format!("bech32 error: {e}"))
}

#[tauri::command]
pub async fn delete_nostr_identity(app: tauri::AppHandle) -> Result<(), String> {
    let node_state = app.state::<NodeState>();
    node_state.shutdown_watcher().await;

    // Lock wallet and drop node
    {
        let mut guard = node_state.node.lock().await;
        if let Some(node) = guard.as_ref() {
            node.lock_wallet();
        }
        *guard = None;
    }

    // Clear wallet state
    {
        let manager = app.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        mgr.set_wallet_unlocked(false);
        if let Some(persister) = mgr.persister_mut() {
            persister.clear_cache();
        }
    }

    // Delete key file
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("failed to get app data dir: {e}"))?;
    let key_path = app_data_dir.join("nostr_identity.key");
    if key_path.exists() {
        std::fs::remove_file(&key_path).map_err(|e| format!("failed to delete key file: {e}"))?;
    }

    bump_revision_and_emit(&app).await?;
    Ok(())
}

// =========================================================================
// NIP-44 wallet backup commands
// =========================================================================

/// Encrypt the wallet mnemonic with NIP-44 and publish to relays.
#[tauri::command]
pub async fn backup_mnemonic_to_nostr(
    password: String,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let (keys, client) = get_keys_and_client(&app).await?;

    // Get mnemonic from persister
    let mnemonic = {
        let manager = app.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        let persister = mgr
            .persister_mut()
            .ok_or_else(|| "Persister not initialized".to_string())?;
        if let Some(cached) = persister.cached() {
            cached.to_string()
        } else {
            persister.load(&password).map_err(|e| e.to_string())?
        }
    };

    let encrypted = discovery::nip44_encrypt_to_self(&keys, &mnemonic)?;
    let event = discovery::build_wallet_backup_event(&keys, &encrypted)?;
    let event_id = discovery::publish_event(&client, event).await?;

    Ok(event_id.to_hex())
}

/// Fetch and decrypt wallet mnemonic backup from relays.
#[tauri::command]
pub async fn restore_mnemonic_from_nostr(app: tauri::AppHandle) -> Result<String, String> {
    let (keys, client) = get_keys_and_client(&app).await?;

    let filter = discovery::build_backup_query_filter(&keys.public_key());
    let events = client
        .fetch_events(vec![filter], Duration::from_secs(8))
        .await
        .map_err(|e| format!("failed to fetch backup: {e}"))?;

    let encrypted_content = events
        .iter()
        .find(|e| {
            !e.content.is_empty()
                && !e
                    .tags
                    .iter()
                    .any(|t| t.as_slice().first().map(|s| s.as_str()) == Some("deleted"))
        })
        .map(|e| e.content.clone())
        .ok_or_else(|| "No backup found on relays".to_string())?;

    discovery::nip44_decrypt_from_self(&keys, &encrypted_content)
}

#[tauri::command]
pub async fn check_nostr_backup(
    app: tauri::AppHandle,
) -> Result<discovery::NostrBackupStatus, String> {
    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    let keys = node.keys().clone();
    drop(guard);

    let relays = {
        let nostr_state = app.state::<NostrAppState>();
        let guard = nostr_state
            .relay_list
            .read()
            .map_err(|_| "failed to read relay_list".to_string())?;
        guard.clone()
    };

    let filter = discovery::build_backup_query_filter(&keys.public_key());

    let mut tasks = tokio::task::JoinSet::new();
    for url in relays {
        let f = filter.clone();
        tasks.spawn(async move {
            let found =
                match discovery::connect_multi_relay_client(std::slice::from_ref(&url)).await {
                    Ok(per_relay_client) => {
                        match per_relay_client
                            .fetch_events(vec![f], Duration::from_secs(8))
                            .await
                        {
                            Ok(events) => events.iter().any(|e| {
                                !e.content.is_empty()
                                    && !e.tags.iter().any(|t| {
                                        t.as_slice().first().map(|s| s.as_str()) == Some("deleted")
                                    })
                            }),
                            Err(_) => false,
                        }
                    }
                    Err(_) => false,
                };
            discovery::RelayBackupResult {
                url,
                has_backup: found,
            }
        });
    }

    let mut relay_results = Vec::new();
    let mut any_found = false;
    while let Some(result) = tasks.join_next().await {
        if let Ok(r) = result {
            if r.has_backup {
                any_found = true;
            }
            relay_results.push(r);
        }
    }

    Ok(discovery::NostrBackupStatus {
        has_backup: any_found,
        relay_results,
    })
}

#[tauri::command]
pub async fn delete_nostr_backup(app: tauri::AppHandle) -> Result<String, String> {
    let (keys, client) = get_keys_and_client(&app).await?;

    // Overwrite the addressable event with empty content first.
    // Relays that ignore NIP-09 deletion will still replace the backup
    // with this empty event, ensuring the mnemonic is no longer stored.
    let empty_event = discovery::build_backup_empty_replacement(&keys)?;
    let _ = discovery::publish_event(&client, empty_event).await;

    let event = discovery::build_backup_deletion_event(&keys)?;
    let event_id = discovery::publish_event(&client, event).await?;

    Ok(event_id.to_hex())
}

// =========================================================================
// NIP-65 relay management commands
// =========================================================================

#[tauri::command]
pub fn get_relay_list(app: tauri::AppHandle) -> Result<Vec<discovery::RelayEntry>, String> {
    let nostr_state = app.state::<NostrAppState>();
    let relays = nostr_state
        .relay_list
        .read()
        .map_err(|_| "failed to read relay_list".to_string())?
        .clone();

    Ok(relays
        .into_iter()
        .map(|url| discovery::RelayEntry {
            url,
            has_backup: false,
        })
        .collect())
}

#[tauri::command]
pub async fn set_relay_list(relays: Vec<String>, app: tauri::AppHandle) -> Result<(), String> {
    let normalized: Vec<String> = relays
        .iter()
        .map(|u| discovery::normalize_relay_url(u))
        .collect();

    // Update relay list
    {
        let nostr_state = app.state::<NostrAppState>();
        let mut list = nostr_state
            .relay_list
            .write()
            .map_err(|_| "failed to write relay_list".to_string())?;
        *list = normalized.clone();
    }

    // Publish kind 10002 if node is available
    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    if let Some(node) = guard.as_ref() {
        let keys = node.keys().clone();
        let client = node.discovery().client().clone();
        drop(guard);

        // Add new relays to the client
        for url in &normalized {
            let _ = client.add_relay(url.as_str()).await;
        }
        client.connect_with_timeout(Duration::from_secs(5)).await;

        let event = discovery::build_relay_list_event(&keys, &normalized)?;
        discovery::publish_event(&client, event).await?;
    }

    Ok(())
}

#[tauri::command]
pub async fn fetch_nip65_relay_list(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let (keys, client) = get_keys_and_client(&app).await?;

    match discovery::fetch_relay_list(&client, &keys.public_key()).await? {
        Some(relays) => {
            let nostr_state = app.state::<NostrAppState>();
            let mut list = nostr_state
                .relay_list
                .write()
                .map_err(|_| "failed to write relay_list".to_string())?;
            *list = relays.clone();
            Ok(relays)
        }
        None => {
            let nostr_state = app.state::<NostrAppState>();
            let relays = nostr_state
                .relay_list
                .read()
                .map_err(|_| "failed to read relay_list".to_string())?
                .clone();
            Ok(relays)
        }
    }
}

#[tauri::command]
pub async fn add_relay(url: String, app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let normalized = discovery::normalize_relay_url(&url);
    let new_list = {
        let nostr_state = app.state::<NostrAppState>();
        let mut list = nostr_state
            .relay_list
            .write()
            .map_err(|_| "failed to write relay_list".to_string())?;
        if !list.contains(&normalized) {
            list.push(normalized.clone());
        }
        list.clone()
    };

    // Add to client and publish if node is available
    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    if let Some(node) = guard.as_ref() {
        let keys = node.keys().clone();
        let client = node.discovery().client().clone();
        drop(guard);

        let _ = client.add_relay(normalized.as_str()).await;
        client.connect_with_timeout(Duration::from_secs(5)).await;

        let event = discovery::build_relay_list_event(&keys, &new_list)?;
        discovery::publish_event(&client, event).await?;
    }

    Ok(new_list)
}

#[tauri::command]
pub async fn remove_relay(url: String, app: tauri::AppHandle) -> Result<Vec<String>, String> {
    let normalized = discovery::normalize_relay_url(&url);
    let new_list = {
        let nostr_state = app.state::<NostrAppState>();
        let mut list = nostr_state
            .relay_list
            .write()
            .map_err(|_| "failed to write relay_list".to_string())?;
        list.retain(|u| u != &normalized);
        if list.is_empty() {
            *list = discovery::DEFAULT_RELAYS
                .iter()
                .map(|s| s.to_string())
                .collect();
        }
        list.clone()
    };

    // Publish if node is available
    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    if let Some(node) = guard.as_ref() {
        let keys = node.keys().clone();
        let client = node.discovery().client().clone();
        drop(guard);

        let event = discovery::build_relay_list_event(&keys, &new_list)?;
        discovery::publish_event(&client, event).await?;
    }

    Ok(new_list)
}

// =========================================================================
// Kind 0 profile command
// =========================================================================

#[tauri::command]
pub async fn fetch_nostr_profile(
    app: tauri::AppHandle,
) -> Result<Option<discovery::NostrProfile>, String> {
    let (keys, client) = get_keys_and_client(&app).await?;
    discovery::fetch_profile(&client, &keys.public_key()).await
}

// =========================================================================
// Contract discovery commands
// =========================================================================

#[tauri::command]
pub async fn discover_contracts(app: tauri::AppHandle) -> Result<Vec<DiscoveredMarket>, String> {
    // Fetch from Nostr (persists to store as side-effect)
    {
        let node_state = app.state::<NodeState>();
        let guard = node_state.node.lock().await;
        let node = guard.as_ref().ok_or("Node not initialized")?;
        if let Err(e) = node.fetch_markets().await {
            log::warn!("Nostr fetch failed (serving from store): {e}");
        }
    }
    // Return from store — single source of truth
    list_contracts(app)
}

#[tauri::command]
pub async fn discover_limit_orders(
    market_id: Option<String>,
    app: tauri::AppHandle,
) -> Result<Vec<deadcat_sdk::DiscoveredOrder>, String> {
    run_node_query(&app, |node| {
        Box::pin(async move {
            node.fetch_orders(market_id.as_deref())
                .await
                .map_err(|e| format!("{e}"))
        })
    })
    .await
}

#[derive(Serialize, Deserialize)]
pub struct RecoveredOwnLimitOrderResponse {
    pub txid: String,
    pub vout: u32,
    pub outpoint: String,
    pub offered_asset_id_hex: String,
    pub offered_amount: u64,
    pub order_index: Option<u32>,
    pub maker_base_pubkey_hex: Option<String>,
    pub order_nonce_hex: Option<String>,
    pub order_params: Option<MakerOrderParamsPayload>,
    pub status: String,
    pub ambiguity_count: u32,
    pub is_cancelable: bool,
}

fn collect_candidate_base_assets(
    app: &tauri::AppHandle,
    market_id: Option<[u8; 32]>,
) -> Result<Vec<[u8; 32]>, String> {
    let store_arc = {
        let state_handle = app.state::<Mutex<AppStateManager>>();
        let mgr = state_handle
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        mgr.store()
            .cloned()
            .ok_or_else(|| "Store not initialized".to_string())?
    };

    let mut store = store_arc
        .lock()
        .map_err(|_| "store lock failed".to_string())?;
    let infos = store
        .list_markets(&MarketFilter::default())
        .map_err(|e| format!("list markets: {e}"))?;

    let mut set = HashSet::<[u8; 32]>::new();
    for info in infos {
        if let Some(mid) = market_id {
            if *info.market_id.as_bytes() != mid {
                continue;
            }
        }
        set.insert(info.params.yes_token_asset);
        set.insert(info.params.no_token_asset);
    }

    Ok(set.into_iter().collect())
}

fn map_recovered_order_status(status: deadcat_sdk::RecoveredOwnOrderStatus) -> String {
    match status {
        deadcat_sdk::RecoveredOwnOrderStatus::ActiveConfirmed => "active_confirmed",
        deadcat_sdk::RecoveredOwnOrderStatus::ActiveMempool => "active_mempool",
        deadcat_sdk::RecoveredOwnOrderStatus::SpentOrFilled => "spent_or_filled",
        deadcat_sdk::RecoveredOwnOrderStatus::Ambiguous => "ambiguous",
    }
    .to_string()
}

#[tauri::command]
pub async fn recover_own_limit_orders(
    market_id: Option<String>,
    app: tauri::AppHandle,
) -> Result<Vec<RecoveredOwnLimitOrderResponse>, String> {
    let market_id_bytes = match market_id {
        Some(mid) => Some(decode_hex32(&mid, "market_id")?),
        None => None,
    };
    let candidate_base_assets = collect_candidate_base_assets(&app, market_id_bytes)?;

    let recovered = run_node_query(&app, |node| {
        Box::pin(async move {
            node.recover_own_limit_orders(candidate_base_assets)
                .await
                .map_err(|e| format!("{e}"))
        })
    })
    .await?;

    Ok(recovered
        .into_iter()
        .map(|o| RecoveredOwnLimitOrderResponse {
            txid: o.txid.to_string(),
            vout: o.vout,
            outpoint: format!("{}:{}", o.outpoint.txid, o.outpoint.vout),
            offered_asset_id_hex: hex::encode(o.offered_asset_id),
            offered_amount: o.offered_amount,
            order_index: o.order_index,
            maker_base_pubkey_hex: o.maker_base_pubkey.map(hex::encode),
            order_nonce_hex: o.order_nonce.map(hex::encode),
            order_params: o.params.map(MakerOrderParamsPayload::from_params),
            status: map_recovered_order_status(o.status),
            ambiguity_count: o.ambiguity_count,
            is_cancelable: o.is_cancelable(),
        })
        .collect())
}

/// Publish a contract to Nostr (Nostr-only mode — no on-chain tx).
#[tauri::command]
pub async fn publish_contract(
    request: CreateContractRequest,
    app: tauri::AppHandle,
) -> Result<DiscoveredMarket, String> {
    validate_request(&request)?;

    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard
        .as_ref()
        .ok_or("Node not initialized — call init_nostr_identity first")?;

    let oracle_pubkey_bytes: [u8; 32] = {
        let hex_str = node.keys().public_key().to_hex();
        let bytes = hex::decode(&hex_str).map_err(|e| format!("hex decode error: {e}"))?;
        bytes
            .try_into()
            .map_err(|_| "pubkey must be 32 bytes".to_string())?
    };

    let wallet_network: crate::WalletNetwork = {
        let state_handle = app.state::<Mutex<AppStateManager>>();
        let mgr = state_handle
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        mgr.network()
            .ok_or_else(|| "network not configured".to_string())?
            .into()
    };
    let (tip, now_unix) = compute_tip_and_now(wallet_network).await?;

    let expiry_time = if request.settlement_deadline_unix > now_unix {
        let seconds_until = request.settlement_deadline_unix - now_unix;
        let blocks_until = (seconds_until / 60) as u32;
        tip.height + blocks_until
    } else {
        return Err("settlement deadline must be in the future".to_string());
    };

    // Asset IDs are zero — no on-chain issuance has occurred yet.
    // They get populated when the market is created on-chain via create_contract_onchain.
    let contract_params = deadcat_sdk::PredictionMarketParams {
        oracle_public_key: oracle_pubkey_bytes,
        collateral_asset_id: [0u8; 32],
        yes_token_asset: [0u8; 32],
        no_token_asset: [0u8; 32],
        yes_reissuance_token: [0u8; 32],
        no_reissuance_token: [0u8; 32],
        collateral_per_token: request.collateral_per_token,
        expiry_time,
    };

    let metadata = ContractMetadata {
        question: request.question.clone(),
        description: request.description.clone(),
        category: request.category.clone(),
        resolution_source: request.resolution_source.clone(),
    };

    let announcement = deadcat_sdk::ContractAnnouncement {
        version: 2,
        contract_params,
        metadata: metadata.clone(),
        creation_txid: None,
    };

    let event_id = node
        .announce_market(&announcement)
        .await
        .map_err(|e| format!("{e}"))?;

    let market_id = contract_params.market_id();
    let nevent = nostr_sdk::nips::nip19::Nip19Event::new(
        event_id,
        discovery::DEFAULT_RELAYS.iter().map(|r| r.to_string()),
    )
    .to_bech32()
    .unwrap_or_default();

    let creator_pubkey = node.keys().public_key().to_hex();

    Ok(DiscoveredMarket {
        id: event_id.to_hex(),
        nevent,
        market_id: hex::encode(market_id.as_bytes()),
        question: metadata.question,
        category: metadata.category,
        description: metadata.description,
        resolution_source: metadata.resolution_source,
        oracle_pubkey: hex::encode(oracle_pubkey_bytes),
        expiry_height: expiry_time,
        cpt_sats: request.collateral_per_token,
        collateral_asset_id: hex::encode([0u8; 32]),
        yes_asset_id: hex::encode([0u8; 32]),
        no_asset_id: hex::encode([0u8; 32]),
        yes_reissuance_token: hex::encode([0u8; 32]),
        no_reissuance_token: hex::encode([0u8; 32]),
        creator_pubkey,
        created_at: nostr_sdk::Timestamp::now().as_u64(),
        creation_txid: None,
        state: 0,
        nostr_event_json: None,
        yes_price_bps: None,
        no_price_bps: None,
    })
}

#[tauri::command]
pub async fn oracle_attest(
    market_id_hex: String,
    outcome_yes: bool,
    app: tauri::AppHandle,
) -> Result<discovery::AttestationResult, String> {
    let market_id_bytes: [u8; 32] = hex::decode(&market_id_hex)
        .map_err(|e| format!("invalid market_id hex: {e}"))?
        .try_into()
        .map_err(|_| "market_id must be exactly 32 bytes".to_string())?;
    let market_id = deadcat_sdk::MarketId(market_id_bytes);

    // Get a connected client (handles relay connection)
    let (_keys, client) = get_keys_and_client(&app).await?;

    // Fetch the announcement to get its event ID
    let filter = nostr_sdk::Filter::new()
        .kind(discovery::APP_EVENT_KIND)
        .identifier(&market_id_hex)
        .hashtag(discovery::CONTRACT_TAG);

    let events = client
        .fetch_events(vec![filter], Duration::from_secs(8))
        .await
        .map_err(|e| format!("failed to fetch announcement: {e}"))?;

    let announcement_event_id = events
        .iter()
        .next()
        .map(|e| e.id.to_hex())
        .unwrap_or_default();

    // Lock node only for the attestation call
    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    let result = node
        .attest_market(&market_id, &announcement_event_id, outcome_yes)
        .await
        .map_err(|e| format!("{e}"))?;

    Ok(result)
}

// =========================================================================
// On-chain contract creation command
// =========================================================================

#[tauri::command]
pub async fn create_contract_onchain(
    request: CreateContractRequest,
    app: tauri::AppHandle,
) -> Result<DiscoveredMarket, String> {
    validate_request(&request)?;

    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard
        .as_ref()
        .ok_or("Node not initialized — call init_nostr_identity first")?;

    let oracle_pubkey_bytes: [u8; 32] = {
        let hex_str = node.keys().public_key().to_hex();
        let bytes = hex::decode(&hex_str).map_err(|e| format!("hex decode: {e}"))?;
        bytes
            .try_into()
            .map_err(|_| "pubkey must be 32 bytes".to_string())?
    };

    let wallet_network: crate::WalletNetwork = {
        let state_handle = app.state::<Mutex<AppStateManager>>();
        let mgr = state_handle
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        mgr.network()
            .ok_or_else(|| "network not configured".to_string())?
            .into()
    };
    let (tip, now_unix) = compute_tip_and_now(wallet_network).await?;

    let expiry_time = if request.settlement_deadline_unix > now_unix {
        let seconds_until = request.settlement_deadline_unix - now_unix;
        let blocks_until = (seconds_until / 60) as u32;
        tip.height + blocks_until
    } else {
        return Err("settlement deadline must be in the future".into());
    };

    let metadata = ContractMetadata {
        question: request.question,
        description: request.description,
        category: request.category,
        resolution_source: request.resolution_source,
    };

    let (market, _txid) = node
        .create_market(
            oracle_pubkey_bytes,
            request.collateral_per_token,
            expiry_time,
            300,
            300,
            metadata,
        )
        .await
        .map_err(|e| format!("{e}"))?;
    drop(guard);

    bump_revision_and_emit(&app).await?;

    Ok(market)
}

// =========================================================================
// Market trade quote commands
// =========================================================================

#[derive(Serialize, Deserialize, Clone)]
pub struct QuoteMarketTradeRequest {
    pub contract_params: deadcat_sdk::PredictionMarketParams,
    pub market_id: String,
    pub side: String,
    pub direction: String,
    pub exact_input: u64,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TradeQuoteLegSourceResponse {
    AmmPool {
        pool_id: String,
    },
    LimitOrder {
        order_id: String,
        price: u64,
        lots: u64,
    },
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TradeQuoteLegResponse {
    pub source: TradeQuoteLegSourceResponse,
    pub input_amount: u64,
    pub output_amount: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct QuoteMarketTradeResponse {
    pub quote_id: String,
    pub market_id: String,
    pub side: String,
    pub direction: String,
    pub exact_input: u64,
    pub total_input: u64,
    pub total_output: u64,
    pub effective_price: f64,
    pub expires_at_unix: u64,
    pub legs: Vec<TradeQuoteLegResponse>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PreviewMarketTradeResponse {
    pub market_id: String,
    pub side: String,
    pub direction: String,
    pub exact_input: u64,
    pub total_input: u64,
    pub total_output: u64,
    pub effective_price: f64,
    pub legs: Vec<TradeQuoteLegResponse>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ExecuteMarketTradeQuoteRequest {
    pub quote_id: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ExecuteMarketTradeQuoteResponse {
    pub txid: String,
    pub total_input: u64,
    pub total_output: u64,
    pub num_orders_filled: usize,
    pub pool_used: bool,
}

fn trade_side_label(side: deadcat_sdk::TradeSide) -> &'static str {
    match side {
        deadcat_sdk::TradeSide::Yes => "yes",
        deadcat_sdk::TradeSide::No => "no",
    }
}

fn trade_direction_label(direction: deadcat_sdk::TradeDirection) -> &'static str {
    match direction {
        deadcat_sdk::TradeDirection::Buy => "buy",
        deadcat_sdk::TradeDirection::Sell => "sell",
    }
}

fn map_route_leg(leg: &deadcat_sdk::RouteLeg) -> TradeQuoteLegResponse {
    let source = match &leg.source {
        deadcat_sdk::LiquiditySource::AmmPool { pool_id } => TradeQuoteLegSourceResponse::AmmPool {
            pool_id: pool_id.clone(),
        },
        deadcat_sdk::LiquiditySource::LimitOrder {
            order_id,
            price,
            lots,
        } => TradeQuoteLegSourceResponse::LimitOrder {
            order_id: order_id.clone(),
            price: *price,
            lots: *lots,
        },
    };
    TradeQuoteLegResponse {
        source,
        input_amount: leg.input_amount,
        output_amount: leg.output_amount,
    }
}

fn build_preview_market_trade_response(
    request: &QuoteMarketTradeRequest,
    side: deadcat_sdk::TradeSide,
    direction: deadcat_sdk::TradeDirection,
    quote: &deadcat_sdk::TradeQuote,
) -> PreviewMarketTradeResponse {
    PreviewMarketTradeResponse {
        market_id: request.market_id.clone(),
        side: trade_side_label(side).to_string(),
        direction: trade_direction_label(direction).to_string(),
        exact_input: request.exact_input,
        total_input: quote.total_input,
        total_output: quote.total_output,
        effective_price: quote.effective_price,
        legs: quote.legs.iter().map(map_route_leg).collect(),
    }
}

fn validate_quote_market_trade_request(request: &QuoteMarketTradeRequest) -> Result<(), String> {
    if request.market_id.trim().is_empty() {
        return Err("market_id is required".to_string());
    }
    if request.exact_input == 0 {
        return Err("exact_input must be > 0".to_string());
    }
    Ok(())
}

#[tauri::command]
pub async fn quote_market_trade(
    request: QuoteMarketTradeRequest,
    app: tauri::AppHandle,
) -> Result<QuoteMarketTradeResponse, String> {
    validate_quote_market_trade_request(&request)?;

    let side = parse_trade_side(&request.side)?;
    let direction = parse_trade_direction(&request.direction)?;
    let market_id = request.market_id.clone();
    let contract_params = request.contract_params;
    let exact_input = request.exact_input;

    let quote = run_node_query(&app, |node| {
        Box::pin(async move {
            node.quote_trade(
                contract_params,
                &market_id,
                side,
                direction,
                deadcat_sdk::TradeAmount::ExactInput(exact_input),
            )
            .await
            .map_err(|e| format!("{e}"))
        })
    })
    .await?;

    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("time error: {e}"))?
        .as_secs();
    let expires_at_unix = market_quote_expires_at_unix(now_unix, MARKET_TRADE_QUOTE_TTL_SECS);
    let now = Instant::now();
    let expires_at = now + Duration::from_secs(MARKET_TRADE_QUOTE_TTL_SECS);
    let quote_id = market_quote_id();

    let response = QuoteMarketTradeResponse {
        quote_id: quote_id.clone(),
        market_id: request.market_id.clone(),
        side: trade_side_label(side).to_string(),
        direction: trade_direction_label(direction).to_string(),
        exact_input: request.exact_input,
        total_input: quote.total_input,
        total_output: quote.total_output,
        effective_price: quote.effective_price,
        expires_at_unix,
        legs: quote.legs.iter().map(map_route_leg).collect(),
    };

    let node_state = app.state::<NodeState>();
    let mut cache = node_state.trade_quote_cache.lock().await;
    prune_expired_entries(&mut cache, now);
    cache.insert(
        quote_id,
        CachedTradeQuote {
            quote,
            market_id: request.market_id,
            expires_at,
        },
    );

    Ok(response)
}

#[tauri::command]
pub async fn preview_market_trade(
    request: QuoteMarketTradeRequest,
    app: tauri::AppHandle,
) -> Result<PreviewMarketTradeResponse, String> {
    validate_quote_market_trade_request(&request)?;

    let side = parse_trade_side(&request.side)?;
    let direction = parse_trade_direction(&request.direction)?;
    let market_id = request.market_id.clone();
    let contract_params = request.contract_params;
    let exact_input = request.exact_input;

    let quote = run_node_query(&app, |node| {
        Box::pin(async move {
            // TODO(deadcat-sdk): expose a dedicated non-binding preview quote API.
            // For now this intentionally reuses quote_trade and returns no executable quote_id.
            node.quote_trade(
                contract_params,
                &market_id,
                side,
                direction,
                deadcat_sdk::TradeAmount::ExactInput(exact_input),
            )
            .await
            .map_err(|e| format!("{e}"))
        })
    })
    .await?;

    Ok(build_preview_market_trade_response(
        &request, side, direction, &quote,
    ))
}

#[tauri::command]
pub async fn execute_market_trade_quote(
    request: ExecuteMarketTradeQuoteRequest,
    app: tauri::AppHandle,
) -> Result<ExecuteMarketTradeQuoteResponse, String> {
    if request.quote_id.trim().is_empty() {
        return Err("quote_id is required".to_string());
    }

    let cached = {
        let node_state = app.state::<NodeState>();
        let mut cache = node_state.trade_quote_cache.lock().await;
        take_unexpired_entry(
            &mut cache,
            &request.quote_id,
            Instant::now(),
            "Quote not found or expired",
        )?
    };

    let market_id = cached.market_id;
    let quote = cached.quote;
    let result = run_node_mutation(&app, |node| {
        Box::pin(async move {
            node.execute_trade(quote, MARKET_TRADE_EXEC_FEE_SATS, &market_id)
                .await
                .map_err(|e| format!("{e}"))
        })
    })
    .await?;

    Ok(ExecuteMarketTradeQuoteResponse {
        txid: result.txid.to_string(),
        total_input: result.total_input,
        total_output: result.total_output,
        num_orders_filled: result.num_orders_filled,
        pool_used: result.pool_used,
    })
}

// =========================================================================
// Token issuance command
// =========================================================================

#[derive(Serialize, Deserialize)]
pub struct IssuanceResultResponse {
    pub txid: String,
    pub previous_state: u8,
    pub new_state: u8,
    pub pairs_issued: u64,
}

/// Issue new YES+NO token pairs by locking collateral.
#[tauri::command]
pub async fn issue_tokens(
    contract_params: deadcat_sdk::PredictionMarketParams,
    creation_txid: String,
    pairs: u64,
    app: tauri::AppHandle,
) -> Result<IssuanceResultResponse, String> {
    let txid: lwk_wollet::elements::Txid = creation_txid
        .parse()
        .map_err(|e| format!("invalid txid: {e}"))?;

    let result = run_node_mutation(&app, |node| {
        Box::pin(async move {
            node.issue_tokens(contract_params, txid, pairs, 500)
                .await
                .map_err(|e| format!("{e}"))
        })
    })
    .await?;

    Ok(IssuanceResultResponse {
        txid: result.txid.to_string(),
        previous_state: result.previous_state as u8,
        new_state: result.new_state as u8,
        pairs_issued: result.pairs_issued,
    })
}

// =========================================================================
// Limit order commands
// =========================================================================

#[derive(Serialize, Deserialize, Clone)]
pub struct CreateLimitOrderRequest {
    pub base_asset_id_hex: String,
    pub quote_asset_id_hex: String,
    pub price: u64,
    pub order_amount: u64,
    pub direction: String,
    pub min_fill_lots: u64,
    pub min_remainder_lots: u64,
    pub market_id: String,
    pub direction_label: String,
}

#[derive(Serialize, Deserialize)]
pub struct CreateLimitOrderResponse {
    pub txid: String,
    pub order_event_id: String,
    pub order_uid: String,
    pub order_params: MakerOrderParamsPayload,
    pub maker_base_pubkey_hex: String,
    pub order_nonce_hex: String,
    pub covenant_address: String,
    pub order_amount: u64,
}

#[tauri::command]
pub async fn create_limit_order(
    request: CreateLimitOrderRequest,
    app: tauri::AppHandle,
) -> Result<CreateLimitOrderResponse, String> {
    if !(deadcat_sdk::LIMIT_ORDER_PRICE_MIN..=deadcat_sdk::LIMIT_ORDER_PRICE_MAX)
        .contains(&request.price)
    {
        return Err(format!(
            "price must be in range {}..={}",
            deadcat_sdk::LIMIT_ORDER_PRICE_MIN,
            deadcat_sdk::LIMIT_ORDER_PRICE_MAX
        ));
    }
    if request.order_amount == 0 {
        return Err("order_amount must be > 0".to_string());
    }
    if request.min_fill_lots != deadcat_sdk::LIMIT_ORDER_MIN_FILL_LOTS_V2 {
        return Err("v2 requires min_fill_lots = 1".to_string());
    }
    if request.min_remainder_lots != deadcat_sdk::LIMIT_ORDER_MIN_REMAINDER_LOTS_V2 {
        return Err("v2 requires min_remainder_lots = 1".to_string());
    }

    let base_asset_id = decode_hex32(&request.base_asset_id_hex, "base_asset_id_hex")?;
    let quote_asset_id = decode_hex32(&request.quote_asset_id_hex, "quote_asset_id_hex")?;
    let policy_asset = {
        let state_handle = app.state::<Mutex<AppStateManager>>();
        let mgr = state_handle
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        let network = mgr
            .network()
            .ok_or_else(|| "network not configured".to_string())?;
        crate::state::to_sdk_network(network)
            .into_lwk()
            .policy_asset()
            .into_inner()
            .to_byte_array()
    };
    if quote_asset_id != policy_asset {
        return Err("v2 requires quote_asset_id_hex to be policy asset (L-BTC)".to_string());
    }
    let direction = parse_order_direction(&request.direction)?;

    let market_id = request.market_id;
    let market_id_for_uid = market_id.clone();
    let direction_label = request.direction_label;
    let order_amount = request.order_amount;
    let min_fill_lots = request.min_fill_lots;
    let min_remainder_lots = request.min_remainder_lots;
    let price = request.price;

    let (result, event_id) = run_node_mutation(&app, |node| {
        Box::pin(async move {
            node.create_limit_order(
                base_asset_id,
                quote_asset_id,
                price,
                order_amount,
                direction,
                min_fill_lots,
                min_remainder_lots,
                500,
                market_id,
                direction_label,
            )
            .await
            .map_err(|e| format!("{e}"))
        })
    })
    .await?;

    let maker_base_pubkey_hex = hex::encode(result.maker_base_pubkey);
    let order_nonce_hex = hex::encode(result.order_nonce);
    let order_uid =
        deadcat_sdk::derive_order_uid(&market_id_for_uid, &maker_base_pubkey_hex, &order_nonce_hex);

    Ok(CreateLimitOrderResponse {
        txid: result.txid.to_string(),
        order_event_id: event_id.to_hex(),
        order_uid,
        order_params: MakerOrderParamsPayload::from_params(result.order_params),
        maker_base_pubkey_hex,
        order_nonce_hex,
        covenant_address: result.covenant_address,
        order_amount: result.order_amount,
    })
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CancelLimitOrderRequest {
    pub order_params: MakerOrderParamsPayload,
    pub maker_base_pubkey_hex: String,
    pub order_nonce_hex: String,
}

#[derive(Serialize, Deserialize)]
pub struct CancelLimitOrderResponse {
    pub txid: String,
    pub refunded_amount: u64,
}

#[tauri::command]
pub async fn cancel_limit_order(
    request: CancelLimitOrderRequest,
    app: tauri::AppHandle,
) -> Result<CancelLimitOrderResponse, String> {
    let maker_base_pubkey = decode_hex32(&request.maker_base_pubkey_hex, "maker_base_pubkey_hex")?;
    let order_nonce = decode_hex32(&request.order_nonce_hex, "order_nonce_hex")?;
    let order_params = request.order_params.try_into_params()?;

    let result = run_node_mutation(&app, |node| {
        Box::pin(async move {
            node.cancel_limit_order(order_params, maker_base_pubkey, order_nonce, 500)
                .await
                .map_err(|e| format!("{e}"))
        })
    })
    .await?;

    Ok(CancelLimitOrderResponse {
        txid: result.txid.to_string(),
        refunded_amount: result.refunded_amount,
    })
}

#[derive(Serialize, Deserialize, Clone)]
pub struct FillLimitOrderRequest {
    pub order_params: MakerOrderParamsPayload,
    pub maker_base_pubkey_hex: String,
    pub order_nonce_hex: String,
    pub lots_to_fill: u64,
}

#[derive(Serialize, Deserialize)]
pub struct FillLimitOrderResponse {
    pub txid: String,
    pub lots_filled: u64,
    pub is_partial: bool,
}

#[tauri::command]
pub async fn fill_limit_order(
    request: FillLimitOrderRequest,
    app: tauri::AppHandle,
) -> Result<FillLimitOrderResponse, String> {
    if request.lots_to_fill == 0 {
        return Err("lots_to_fill must be > 0".to_string());
    }

    let maker_base_pubkey = decode_hex32(&request.maker_base_pubkey_hex, "maker_base_pubkey_hex")?;
    let order_nonce = decode_hex32(&request.order_nonce_hex, "order_nonce_hex")?;
    let order_params = request.order_params.try_into_params()?;

    let result = run_node_mutation(&app, |node| {
        Box::pin(async move {
            node.fill_limit_order(
                order_params,
                maker_base_pubkey,
                order_nonce,
                request.lots_to_fill,
                500,
            )
            .await
            .map_err(|e| format!("{e}"))
        })
    })
    .await?;

    Ok(FillLimitOrderResponse {
        txid: result.txid.to_string(),
        lots_filled: result.lots_filled,
        is_partial: result.is_partial,
    })
}

// =========================================================================
// Token cancellation command
// =========================================================================

#[derive(Serialize, Deserialize)]
pub struct CancellationResultResponse {
    pub txid: String,
    pub previous_state: u8,
    pub new_state: u8,
    pub pairs_burned: u64,
    pub is_full_cancellation: bool,
}

/// Cancel paired YES+NO tokens back into collateral.
#[tauri::command]
pub async fn cancel_tokens(
    contract_params: deadcat_sdk::PredictionMarketParams,
    pairs: u64,
    app: tauri::AppHandle,
) -> Result<CancellationResultResponse, String> {
    let result = run_node_mutation(&app, |node| {
        Box::pin(async move {
            node.cancel_tokens(contract_params, pairs, 500)
                .await
                .map_err(|e| format!("{e}"))
        })
    })
    .await?;

    Ok(CancellationResultResponse {
        txid: result.txid.to_string(),
        previous_state: result.previous_state as u8,
        new_state: result.new_state as u8,
        pairs_burned: result.pairs_burned,
        is_full_cancellation: result.is_full_cancellation,
    })
}

// =========================================================================
// Oracle resolution command
// =========================================================================

#[derive(Serialize, Deserialize)]
pub struct ResolutionResultResponse {
    pub txid: String,
    pub previous_state: u8,
    pub new_state: u8,
    pub outcome_yes: bool,
}

/// Resolve a market with an oracle signature.
#[tauri::command]
pub async fn resolve_market(
    contract_params: deadcat_sdk::PredictionMarketParams,
    outcome_yes: bool,
    oracle_signature_hex: String,
    app: tauri::AppHandle,
) -> Result<ResolutionResultResponse, String> {
    let sig_bytes: [u8; 64] = hex::decode(&oracle_signature_hex)
        .map_err(|e| format!("invalid signature hex: {e}"))?
        .try_into()
        .map_err(|_| "oracle signature must be exactly 64 bytes".to_string())?;

    let result = run_node_mutation(&app, |node| {
        Box::pin(async move {
            node.resolve_market(contract_params, outcome_yes, sig_bytes, 500)
                .await
                .map_err(|e| format!("{e}"))
        })
    })
    .await?;

    Ok(ResolutionResultResponse {
        txid: result.txid.to_string(),
        previous_state: result.previous_state as u8,
        new_state: result.new_state as u8,
        outcome_yes: result.outcome_yes,
    })
}

// =========================================================================
// Post-resolution redemption command
// =========================================================================

#[derive(Serialize, Deserialize)]
pub struct RedemptionResultResponse {
    pub txid: String,
    pub previous_state: u8,
    pub tokens_redeemed: u64,
    pub payout_sats: u64,
}

/// Redeem winning tokens after market resolution.
#[tauri::command]
pub async fn redeem_tokens(
    contract_params: deadcat_sdk::PredictionMarketParams,
    tokens: u64,
    app: tauri::AppHandle,
) -> Result<RedemptionResultResponse, String> {
    let result = run_node_mutation(&app, |node| {
        Box::pin(async move {
            node.redeem_tokens(contract_params, tokens, 500)
                .await
                .map_err(|e| format!("{e}"))
        })
    })
    .await?;

    Ok(RedemptionResultResponse {
        txid: result.txid.to_string(),
        previous_state: result.previous_state as u8,
        tokens_redeemed: result.tokens_redeemed,
        payout_sats: result.payout_sats,
    })
}

// =========================================================================
// Expiry redemption command
// =========================================================================

/// Redeem tokens via the expiry path after the locktime has passed.
#[tauri::command]
pub async fn redeem_expired(
    contract_params: deadcat_sdk::PredictionMarketParams,
    token_asset_hex: String,
    tokens: u64,
    app: tauri::AppHandle,
) -> Result<RedemptionResultResponse, String> {
    let token_asset: [u8; 32] = hex::decode(&token_asset_hex)
        .map_err(|e| format!("invalid token asset hex: {e}"))?
        .try_into()
        .map_err(|_| "token asset must be exactly 32 bytes".to_string())?;

    let result = run_node_mutation(&app, |node| {
        Box::pin(async move {
            node.redeem_expired(contract_params, token_asset, tokens, 500)
                .await
                .map_err(|e| format!("{e}"))
        })
    })
    .await?;

    Ok(RedemptionResultResponse {
        txid: result.txid.to_string(),
        previous_state: result.previous_state as u8,
        tokens_redeemed: result.tokens_redeemed,
        payout_sats: result.payout_sats,
    })
}

// =========================================================================
// Market state query command
// =========================================================================

#[derive(Serialize, Deserialize)]
pub struct MarketStateResponse {
    pub state: u8,
}

#[tauri::command]
pub async fn get_market_state(
    contract_params: deadcat_sdk::PredictionMarketParams,
    app: tauri::AppHandle,
) -> Result<MarketStateResponse, String> {
    let state = run_node_query(&app, |node| {
        Box::pin(async move {
            node.market_state(contract_params)
                .await
                .map_err(|e| format!("{e}"))
        })
    })
    .await?;

    Ok(MarketStateResponse {
        state: market_state_to_u8(state),
    })
}

// =========================================================================
// Wallet UTXO query command
// =========================================================================

#[tauri::command]
pub async fn get_wallet_utxos(
    app: tauri::AppHandle,
) -> Result<Vec<crate::wallet::types::WalletUtxo>, String> {
    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    let utxos = node.utxos().map_err(|e| format!("{e}"))?;
    Ok(utxos
        .iter()
        .map(|u| crate::wallet::types::WalletUtxo {
            txid: u.outpoint.txid.to_string(),
            vout: u.outpoint.vout,
            asset_id: u.unblinded.asset.to_string(),
            value: u.unblinded.value,
            height: u.height,
        })
        .collect())
}

// =========================================================================
// Market store commands
// =========================================================================

#[tauri::command]
pub fn list_contracts(app: tauri::AppHandle) -> Result<Vec<DiscoveredMarket>, String> {
    let store_arc = {
        let state_handle = app.state::<Mutex<AppStateManager>>();
        let mgr = state_handle
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        mgr.store()
            .cloned()
            .ok_or_else(|| "Store not initialized".to_string())?
    };

    let mut store = store_arc
        .lock()
        .map_err(|_| "store lock failed".to_string())?;

    let infos = store
        .list_markets(&MarketFilter::default())
        .map_err(|e| format!("list markets: {e}"))?;

    let mut result = Vec::with_capacity(infos.len());
    for info in &infos {
        // Look up pool price from latest snapshot
        let (yes_bps, no_bps) = pool_price_for_market(&mut store, info.market_id.as_bytes());
        result.push(market_info_to_discovered(info, yes_bps, no_bps));
    }
    Ok(result)
}

/// Look up pool price from the latest snapshot for a market.
fn pool_price_for_market(
    store: &mut deadcat_store::DeadcatStore,
    market_id: &[u8; 32],
) -> (Option<u16>, Option<u16>) {
    let pool = match store.get_pool_for_market(market_id) {
        Ok(Some(p)) => p,
        _ => return (None, None),
    };
    let snap = match store.get_latest_pool_snapshot(&pool.pool_id.0) {
        Ok(Some(s)) => s,
        _ => return (None, None),
    };
    let reserves = deadcat_sdk::PoolReserves {
        r_yes: snap.r_yes,
        r_no: snap.r_no,
        r_lbtc: snap.r_lbtc,
    };
    deadcat_sdk::implied_probability_bps(&reserves)
        .map(|(y, n)| (Some(y), Some(n)))
        .unwrap_or((None, None))
}

/// Convert a `MarketInfo` (store type) back to `DiscoveredMarket` (frontend type).
fn market_info_to_discovered(
    info: &deadcat_store::MarketInfo,
    yes_price_bps: Option<u16>,
    no_price_bps: Option<u16>,
) -> DiscoveredMarket {
    let p = &info.params;
    let market_id_hex = hex::encode(info.market_id.as_bytes());
    DiscoveredMarket {
        id: market_id_hex.clone(),
        nevent: info.nevent.clone().unwrap_or_default(),
        market_id: market_id_hex,
        question: info.question.clone().unwrap_or_default(),
        category: info.category.clone().unwrap_or_default(),
        description: info.description.clone().unwrap_or_default(),
        resolution_source: info.resolution_source.clone().unwrap_or_default(),
        oracle_pubkey: hex::encode(p.oracle_public_key),
        expiry_height: p.expiry_time,
        cpt_sats: p.collateral_per_token,
        collateral_asset_id: hex::encode(p.collateral_asset_id),
        yes_asset_id: hex::encode(p.yes_token_asset),
        no_asset_id: hex::encode(p.no_token_asset),
        yes_reissuance_token: hex::encode(p.yes_reissuance_token),
        no_reissuance_token: hex::encode(p.no_reissuance_token),
        creator_pubkey: info
            .creator_pubkey
            .as_ref()
            .map(hex::encode)
            .unwrap_or_default(),
        created_at: parse_iso_datetime_to_unix(&info.created_at),
        creation_txid: info.creation_txid.clone(),
        state: info.state.as_u64() as u8,
        nostr_event_json: info.nostr_event_json.clone(),
        yes_price_bps,
        no_price_bps,
    }
}

fn parse_iso_datetime_to_unix(s: &str) -> u64 {
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .map(|dt| dt.and_utc().timestamp() as u64)
        .unwrap_or(0)
}

// =========================================================================
// Pool chain-walk commands
// =========================================================================

/// Sync a pool's on-chain state history (chain walk).
#[tauri::command]
pub async fn sync_pool(pool_id: String, app: tauri::AppHandle) -> Result<(), String> {
    let pool_id_bytes: [u8; 32] = hex::decode(&pool_id)
        .map_err(|e| format!("invalid pool_id hex: {e}"))?
        .try_into()
        .map_err(|_| "pool_id must be exactly 32 bytes".to_string())?;

    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    node.sync_pool_chain(&deadcat_sdk::PoolId(pool_id_bytes))
        .await
        .map_err(|e| format!("{e}"))?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
pub struct PricePoint {
    pub block_height: Option<i32>,
    pub yes_price_bps: u16,
    pub no_price_bps: u16,
    pub r_yes: u64,
    pub r_no: u64,
    pub r_lbtc: u64,
}

/// Get price history for a market's pool (all snapshots as price points).
#[tauri::command]
pub async fn get_pool_price_history(
    market_id: String,
    app: tauri::AppHandle,
) -> Result<Vec<PricePoint>, String> {
    let market_id_bytes: [u8; 32] = hex::decode(&market_id)
        .map_err(|e| format!("invalid market_id hex: {e}"))?
        .try_into()
        .map_err(|_| "market_id must be exactly 32 bytes".to_string())?;
    let mid = deadcat_sdk::MarketId(market_id_bytes);

    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;

    let history = node
        .market_price_history(&mid)
        .map_err(|e| format!("{e}"))?;

    Ok(history
        .into_iter()
        .map(|p| PricePoint {
            block_height: p.block_height,
            yes_price_bps: p.yes_bps,
            no_price_bps: p.no_bps,
            r_yes: p.reserves.r_yes,
            r_no: p.reserves.r_no,
            r_lbtc: p.reserves.r_lbtc,
        })
        .collect())
}

/// Manually trigger Nostr discovery reconciliation.
///
/// Re-sends all stored Nostr events to connected relays, ensuring relay
/// availability. Idempotent thanks to NIP-33 replaceable events.
#[tauri::command]
pub async fn reconcile_nostr(
    app: tauri::AppHandle,
) -> Result<deadcat_sdk::ReconciliationStats, String> {
    let node_state = app.state::<NodeState>();
    let (client, events) = {
        let guard = node_state.node.lock().await;
        let node = guard.as_ref().ok_or("Node not initialized")?;
        node.prepare_reconciliation().map_err(|e| format!("{e}"))?
    };
    Ok(deadcat_sdk::send_reconciliation_events(&client, &events).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    fn valid_request() -> CreateContractRequest {
        CreateContractRequest {
            question: "Will BTC close above $100k this year?".to_string(),
            description: "Uses a predefined exchange basket at year-end close.".to_string(),
            category: "Bitcoin".to_string(),
            resolution_source: "Exchange basket".to_string(),
            settlement_deadline_unix: 1_800_000_000,
            collateral_per_token: 5_000,
        }
    }

    fn sample_order_params(
        direction: deadcat_sdk::OrderDirection,
    ) -> deadcat_sdk::MakerOrderParams {
        deadcat_sdk::MakerOrderParams {
            base_asset_id: [0x11; 32],
            quote_asset_id: [0x22; 32],
            price: 42,
            min_fill_lots: 1,
            min_remainder_lots: 1,
            direction,
            maker_receive_spk_hash: [0x33; 32],
            cosigner_pubkey: [0x44; 32],
            maker_pubkey: [0x55; 32],
        }
    }

    fn sample_contract_params() -> deadcat_sdk::PredictionMarketParams {
        deadcat_sdk::PredictionMarketParams {
            oracle_public_key: [0x10; 32],
            collateral_asset_id: [0x20; 32],
            yes_token_asset: [0x30; 32],
            no_token_asset: [0x40; 32],
            yes_reissuance_token: [0x50; 32],
            no_reissuance_token: [0x60; 32],
            collateral_per_token: 100,
            expiry_time: 120_000,
        }
    }

    fn valid_quote_request() -> QuoteMarketTradeRequest {
        QuoteMarketTradeRequest {
            contract_params: sample_contract_params(),
            market_id: "ab".repeat(32),
            side: "yes".to_string(),
            direction: "buy".to_string(),
            exact_input: 1_000,
        }
    }

    #[test]
    fn validate_request_accepts_valid_payload() {
        let request = valid_request();
        assert!(validate_request(&request).is_ok());
    }

    #[test]
    fn validate_request_rejects_empty_question() {
        let mut request = valid_request();
        request.question = "   ".to_string();
        let error = validate_request(&request).expect_err("request should fail");
        assert!(error.contains("question"));
    }

    #[test]
    fn validate_request_rejects_zero_collateral() {
        let mut request = valid_request();
        request.collateral_per_token = 0;
        let error = validate_request(&request).expect_err("request should fail");
        assert!(error.contains("collateral_per_token"));
    }

    #[test]
    fn decode_hex32_accepts_valid_input() {
        let value = "aa".repeat(32);
        let decoded = decode_hex32(&value, "test_field").expect("valid hex32 should parse");
        assert_eq!(decoded, [0xaa; 32]);
    }

    #[test]
    fn decode_hex32_rejects_invalid_hex() {
        let err = decode_hex32("zz", "test_field").expect_err("invalid hex should fail");
        assert!(err.contains("invalid test_field"));
    }

    #[test]
    fn decode_hex32_rejects_wrong_length() {
        let err = decode_hex32("aa", "test_field").expect_err("short input should fail");
        assert!(err.contains("test_field must be exactly 32 bytes"));
    }

    #[test]
    fn parse_order_direction_accepts_known_values() {
        assert_eq!(
            parse_order_direction("sell-base").expect("sell-base should parse"),
            deadcat_sdk::OrderDirection::SellBase
        );
        assert_eq!(
            parse_order_direction("sell-quote").expect("sell-quote should parse"),
            deadcat_sdk::OrderDirection::SellQuote
        );
    }

    #[test]
    fn parse_order_direction_rejects_unknown_value() {
        let err = parse_order_direction("buy-base").expect_err("unknown direction should fail");
        assert!(err.contains("direction must be 'sell-base' or 'sell-quote'"));
    }

    #[test]
    fn parse_trade_side_accepts_known_values() {
        assert_eq!(
            parse_trade_side("yes").expect("yes should parse"),
            deadcat_sdk::TradeSide::Yes
        );
        assert_eq!(
            parse_trade_side("no").expect("no should parse"),
            deadcat_sdk::TradeSide::No
        );
    }

    #[test]
    fn parse_trade_side_rejects_unknown_value() {
        let err = parse_trade_side("maybe").expect_err("unknown side should fail");
        assert!(err.contains("side must be 'yes' or 'no'"));
    }

    #[test]
    fn parse_trade_direction_accepts_known_values() {
        assert_eq!(
            parse_trade_direction("buy").expect("buy should parse"),
            deadcat_sdk::TradeDirection::Buy
        );
        assert_eq!(
            parse_trade_direction("sell").expect("sell should parse"),
            deadcat_sdk::TradeDirection::Sell
        );
    }

    #[test]
    fn parse_trade_direction_rejects_unknown_value() {
        let err = parse_trade_direction("hold").expect_err("unknown direction should fail");
        assert!(err.contains("direction must be 'buy' or 'sell'"));
    }

    #[test]
    fn validate_quote_market_trade_request_rejects_zero_exact_input() {
        let mut request = valid_quote_request();
        request.exact_input = 0;
        let err = validate_quote_market_trade_request(&request)
            .expect_err("zero exact_input should fail");
        assert!(err.contains("exact_input must be > 0"));
    }

    #[test]
    fn validate_quote_market_trade_request_rejects_missing_market_id() {
        let mut request = valid_quote_request();
        request.market_id = "   ".to_string();
        let err =
            validate_quote_market_trade_request(&request).expect_err("empty market_id should fail");
        assert!(err.contains("market_id is required"));
    }

    #[test]
    fn preview_market_trade_request_validation_reuses_quote_rules() {
        let mut request = valid_quote_request();
        request.exact_input = 0;
        let err = validate_quote_market_trade_request(&request)
            .expect_err("preview request should reject zero exact_input");
        assert!(err.contains("exact_input must be > 0"));
    }

    #[test]
    fn market_quote_expires_at_unix_adds_ttl() {
        assert_eq!(market_quote_expires_at_unix(1_000, 30), 1_030);
    }

    #[test]
    fn prune_expired_entries_removes_expired() {
        struct DummyEntry {
            expires_at: Instant,
        }
        impl ExpiringEntry for DummyEntry {
            fn expires_at(&self) -> Instant {
                self.expires_at
            }
        }

        let now = Instant::now();
        let mut cache = HashMap::<String, DummyEntry>::new();
        cache.insert(
            "expired".to_string(),
            DummyEntry {
                expires_at: now - Duration::from_secs(1),
            },
        );
        cache.insert(
            "fresh".to_string(),
            DummyEntry {
                expires_at: now + Duration::from_secs(1),
            },
        );

        prune_expired_entries(&mut cache, now);
        assert_eq!(cache.len(), 1);
        assert!(cache.contains_key("fresh"));
    }

    #[test]
    fn take_unexpired_entry_is_single_use() {
        struct DummyEntry {
            expires_at: Instant,
            value: u64,
        }
        impl ExpiringEntry for DummyEntry {
            fn expires_at(&self) -> Instant {
                self.expires_at
            }
        }

        let now = Instant::now();
        let mut cache = HashMap::<String, DummyEntry>::new();
        cache.insert(
            "q1".to_string(),
            DummyEntry {
                expires_at: now + Duration::from_secs(10),
                value: 7,
            },
        );

        let first = take_unexpired_entry(&mut cache, "q1", now, "missing")
            .expect("first lookup should succeed");
        assert_eq!(first.value, 7);
        let second = take_unexpired_entry(&mut cache, "q1", now, "missing");
        assert!(second.is_err());
    }

    #[test]
    fn map_route_leg_maps_limit_order_fields() {
        let leg = deadcat_sdk::RouteLeg {
            source: deadcat_sdk::LiquiditySource::LimitOrder {
                order_id: "o-1".to_string(),
                price: 42,
                lots: 5,
            },
            input_amount: 210,
            output_amount: 5,
        };

        let mapped = map_route_leg(&leg);
        match mapped.source {
            TradeQuoteLegSourceResponse::LimitOrder {
                order_id,
                price,
                lots,
            } => {
                assert_eq!(order_id, "o-1");
                assert_eq!(price, 42);
                assert_eq!(lots, 5);
            }
            _ => panic!("expected limit-order source"),
        }
        assert_eq!(mapped.input_amount, 210);
        assert_eq!(mapped.output_amount, 5);
    }

    #[test]
    fn maker_order_params_payload_roundtrip_sell_base() {
        let expected = sample_order_params(deadcat_sdk::OrderDirection::SellBase);
        let payload = MakerOrderParamsPayload::from_params(sample_order_params(
            deadcat_sdk::OrderDirection::SellBase,
        ));
        let parsed = payload
            .try_into_params()
            .expect("payload should roundtrip to params");
        assert_eq!(parsed, expected);
    }

    #[test]
    fn maker_order_params_payload_roundtrip_sell_quote() {
        let expected = sample_order_params(deadcat_sdk::OrderDirection::SellQuote);
        let payload = MakerOrderParamsPayload::from_params(sample_order_params(
            deadcat_sdk::OrderDirection::SellQuote,
        ));
        let parsed = payload
            .try_into_params()
            .expect("payload should roundtrip to params");
        assert_eq!(parsed, expected);
    }

    #[test]
    fn maker_order_params_payload_rejects_malformed_field() {
        let mut payload = MakerOrderParamsPayload::from_params(sample_order_params(
            deadcat_sdk::OrderDirection::SellBase,
        ));
        payload.base_asset_id_hex = "abcd".to_string();
        let err = payload
            .try_into_params()
            .expect_err("malformed base_asset_id_hex should fail");
        assert!(err.contains("base_asset_id_hex must be exactly 32 bytes"));
    }

    #[test]
    fn map_recovered_order_status_maps_all_variants() {
        assert_eq!(
            map_recovered_order_status(deadcat_sdk::RecoveredOwnOrderStatus::ActiveConfirmed),
            "active_confirmed"
        );
        assert_eq!(
            map_recovered_order_status(deadcat_sdk::RecoveredOwnOrderStatus::ActiveMempool),
            "active_mempool"
        );
        assert_eq!(
            map_recovered_order_status(deadcat_sdk::RecoveredOwnOrderStatus::SpentOrFilled),
            "spent_or_filled"
        );
        assert_eq!(
            map_recovered_order_status(deadcat_sdk::RecoveredOwnOrderStatus::Ambiguous),
            "ambiguous"
        );
    }

    #[test]
    fn parse_iso_datetime_to_unix_parses_valid_datetime() {
        assert_eq!(parse_iso_datetime_to_unix("1970-01-01 00:00:00"), 0);
        assert_eq!(parse_iso_datetime_to_unix("1970-01-01 00:00:01"), 1);
    }

    #[test]
    fn parse_iso_datetime_to_unix_returns_zero_for_invalid_input() {
        assert_eq!(parse_iso_datetime_to_unix("not-a-datetime"), 0);
    }
}
