use std::sync::Mutex;
use std::time::Duration;

use deadcat_store::MarketFilter;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

use crate::discovery::{
    self, ContractMetadata, CreateContractRequest, DiscoveredMarket, DiscoveredOrder,
    IdentityResponse,
};
use crate::state::AppStateManager;
use crate::{NodeState, NostrAppState};

// ── Helpers ──────────────────────────────────────────────────────────────

const ORDER_INDEX_AUTO_RESOLVE_SENTINEL: u32 = u32::MAX;

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

#[derive(Serialize)]
struct OrdersInvalidatedPayload {
    market_id: Option<String>,
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
async fn bump_revision_and_emit<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<(), String> {
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

/// Construct a DeadcatNode from loaded keys and store it in NodeState.
/// Called whenever Nostr identity is loaded/generated/imported.
async fn construct_and_store_node(
    app: &tauri::AppHandle,
    keys: nostr_sdk::Keys,
) -> Result<(), String> {
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
        network_tag: sdk_network.discovery_tag().to_string(),
        ..Default::default()
    };

    let (node, mut rx) = deadcat_sdk::DeadcatNode::with_store(keys, sdk_network, store_arc, config);
    let mut snapshot_rx = node.subscribe_snapshot();

    // Replace any existing node (drops old node if any)
    let node_state = app.state::<NodeState>();
    let mut guard = node_state.node.lock().await;
    *guard = Some(node);

    // Start the background Nostr subscription loop
    if let Some(node) = guard.as_ref() {
        if let Err(e) = node.start_subscription().await {
            log::warn!("failed to start discovery subscription: {e}");
        }
    }
    drop(guard);

    // Forward discovery events to the frontend
    let app_handle = app.clone();
    tokio::spawn(async move {
        use deadcat_sdk::DiscoveryEvent;
        while let Ok(event) = rx.recv().await {
            match event {
                DiscoveryEvent::MarketDiscovered(m) => {
                    let _ = app_handle.emit("discovery:market", &m);
                }
                DiscoveryEvent::OrderDiscovered(o) => {
                    let _ = app_handle.emit("discovery:order", &o);
                }
                DiscoveryEvent::OrdersInvalidated { market_id } => {
                    let payload = OrdersInvalidatedPayload { market_id };
                    let _ = app_handle.emit("discovery:orders-invalidated", &payload);
                }
                DiscoveryEvent::AttestationDiscovered(a) => {
                    let _ = app_handle.emit("discovery:attestation", &a);
                }
                DiscoveryEvent::PoolDiscovered(p) => {
                    let _ = app_handle.emit("discovery:pool", &p);
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
    // Lock wallet and drop node
    {
        let node_state = app.state::<NodeState>();
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

    let encrypted_content = {
        let mut iter = events.iter();
        let event = iter
            .next()
            .ok_or_else(|| "No wallet backup found on relays".to_string())?;
        event.content.clone()
    };

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
                            Ok(events) => events.iter().next().is_some(),
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
pub async fn fetch_orders(
    market_id: Option<String>,
    app: tauri::AppHandle,
) -> Result<Vec<DiscoveredOrder>, String> {
    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    match node.fetch_orders(market_id.as_deref()).await {
        Ok(orders) => Ok(orders),
        Err(e) => {
            log::warn!("Nostr order fetch failed: {e}");
            Ok(vec![])
        }
    }
}

/// Publish a contract to Nostr (Nostr-only mode — no on-chain tx).
#[tauri::command]
pub async fn publish_contract(
    _request: CreateContractRequest,
    _app: tauri::AppHandle,
) -> Result<DiscoveredMarket, String> {
    Err(
        "publish_contract without an on-chain dormant anchor is no longer supported; use create_contract_onchain"
            .to_string(),
    )
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

    let market = node
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
    contract_params_json: String,
    anchor: deadcat_sdk::PredictionMarketAnchor,
    pairs: u64,
    app: tauri::AppHandle,
) -> Result<IssuanceResultResponse, String> {
    let params: deadcat_sdk::PredictionMarketParams =
        serde_json::from_str(&contract_params_json)
            .map_err(|e| format!("invalid contract params: {e}"))?;

    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    let result = node
        .issue_tokens(params, anchor, pairs, 500)
        .await
        .map_err(|e| format!("{e}"))?;
    drop(guard);

    bump_revision_and_emit(&app).await?;

    Ok(IssuanceResultResponse {
        txid: result.txid.to_string(),
        previous_state: result.previous_state as u8,
        new_state: result.new_state as u8,
        pairs_issued: result.pairs_issued,
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
    contract_params_json: String,
    anchor: deadcat_sdk::PredictionMarketAnchor,
    pairs: u64,
    app: tauri::AppHandle,
) -> Result<CancellationResultResponse, String> {
    let params: deadcat_sdk::PredictionMarketParams =
        serde_json::from_str(&contract_params_json)
            .map_err(|e| format!("invalid contract params: {e}"))?;

    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    let result = node
        .cancel_tokens(params, anchor, pairs, 500)
        .await
        .map_err(|e| format!("{e}"))?;
    drop(guard);

    bump_revision_and_emit(&app).await?;

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
    contract_params_json: String,
    anchor: deadcat_sdk::PredictionMarketAnchor,
    outcome_yes: bool,
    oracle_signature_hex: String,
    app: tauri::AppHandle,
) -> Result<ResolutionResultResponse, String> {
    let params: deadcat_sdk::PredictionMarketParams =
        serde_json::from_str(&contract_params_json)
            .map_err(|e| format!("invalid contract params: {e}"))?;

    let sig_bytes: [u8; 64] = hex::decode(&oracle_signature_hex)
        .map_err(|e| format!("invalid signature hex: {e}"))?
        .try_into()
        .map_err(|_| "oracle signature must be exactly 64 bytes".to_string())?;

    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    let result = node
        .resolve_market(params, anchor, outcome_yes, sig_bytes, 500)
        .await
        .map_err(|e| format!("{e}"))?;
    drop(guard);

    bump_revision_and_emit(&app).await?;

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
    contract_params_json: String,
    anchor: deadcat_sdk::PredictionMarketAnchor,
    tokens: u64,
    app: tauri::AppHandle,
) -> Result<RedemptionResultResponse, String> {
    let params: deadcat_sdk::PredictionMarketParams =
        serde_json::from_str(&contract_params_json)
            .map_err(|e| format!("invalid contract params: {e}"))?;

    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    let result = node
        .redeem_tokens(params, anchor, tokens, 500)
        .await
        .map_err(|e| format!("{e}"))?;
    drop(guard);

    bump_revision_and_emit(&app).await?;

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
    contract_params_json: String,
    anchor: deadcat_sdk::PredictionMarketAnchor,
    token_asset_hex: String,
    tokens: u64,
    app: tauri::AppHandle,
) -> Result<RedemptionResultResponse, String> {
    let params: deadcat_sdk::PredictionMarketParams =
        serde_json::from_str(&contract_params_json)
            .map_err(|e| format!("invalid contract params: {e}"))?;

    let token_asset: [u8; 32] = hex::decode(&token_asset_hex)
        .map_err(|e| format!("invalid token asset hex: {e}"))?
        .try_into()
        .map_err(|_| "token asset must be exactly 32 bytes".to_string())?;

    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    let result = node
        .redeem_expired(params, anchor, token_asset, tokens, 500)
        .await
        .map_err(|e| format!("{e}"))?;
    drop(guard);

    bump_revision_and_emit(&app).await?;

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
    contract_params_json: String,
    anchor: deadcat_sdk::PredictionMarketAnchor,
    app: tauri::AppHandle,
) -> Result<MarketStateResponse, String> {
    let params: deadcat_sdk::PredictionMarketParams =
        serde_json::from_str(&contract_params_json)
            .map_err(|e| format!("invalid contract params: {e}"))?;

    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    let state = node
        .market_state(params, anchor)
        .await
        .map_err(|e| format!("{e}"))?;

    Ok(MarketStateResponse {
        state: market_state_to_u8(state),
    })
}

// =========================================================================
// Trade quote / execute commands
// =========================================================================

#[derive(Serialize, Deserialize)]
pub struct TradeQuoteRequest {
    pub contract_params_json: String,
    pub market_id: String,
    pub side: String,
    pub direction: String,
    pub exact_input: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TradeQuoteResponse {
    pub total_input: u64,
    pub total_output: u64,
    pub effective_price: f64,
    pub legs: Vec<RouteLegResponse>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RouteLegSourceResponse {
    LmsrPool {
        pool_id: String,
        old_s_index: u64,
        new_s_index: u64,
    },
    LimitOrder {
        order_id: String,
        price: u64,
        lots: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteLegResponse {
    pub source: RouteLegSourceResponse,
    pub input_amount: u64,
    pub output_amount: u64,
}

#[derive(Serialize, Deserialize)]
pub struct ExecuteTradeRequest {
    pub contract_params_json: String,
    pub market_id: String,
    pub side: String,
    pub direction: String,
    pub exact_input: u64,
    #[serde(default)]
    pub fee_amount: Option<u64>,
    #[serde(default)]
    pub expected_quote: Option<TradeQuoteResponse>,
}

#[derive(Serialize, Deserialize)]
pub struct ExecuteTradeResponse {
    pub txid: String,
    pub total_input: u64,
    pub total_output: u64,
    pub num_orders_filled: usize,
    pub pool_used: bool,
    pub new_reserves: Option<deadcat_sdk::PoolReserves>,
}

fn parse_trade_side(side: &str) -> Result<deadcat_sdk::TradeSide, String> {
    match side.trim().to_ascii_lowercase().as_str() {
        "yes" => Ok(deadcat_sdk::TradeSide::Yes),
        "no" => Ok(deadcat_sdk::TradeSide::No),
        other => Err(format!("invalid side '{other}', expected 'yes' or 'no'")),
    }
}

fn parse_trade_direction(direction: &str) -> Result<deadcat_sdk::TradeDirection, String> {
    match direction.trim().to_ascii_lowercase().as_str() {
        "buy" => Ok(deadcat_sdk::TradeDirection::Buy),
        "sell" => Ok(deadcat_sdk::TradeDirection::Sell),
        other => Err(format!(
            "invalid direction '{other}', expected 'buy' or 'sell'"
        )),
    }
}

fn map_route_leg(leg: deadcat_sdk::RouteLeg) -> RouteLegResponse {
    let source = match leg.source {
        deadcat_sdk::LiquiditySource::LmsrPool {
            pool_id,
            old_s_index,
            new_s_index,
        } => RouteLegSourceResponse::LmsrPool {
            pool_id,
            old_s_index,
            new_s_index,
        },
        deadcat_sdk::LiquiditySource::LimitOrder {
            order_id,
            price,
            lots,
        } => RouteLegSourceResponse::LimitOrder {
            order_id,
            price,
            lots,
        },
    };
    RouteLegResponse {
        source,
        input_amount: leg.input_amount,
        output_amount: leg.output_amount,
    }
}

fn map_trade_quote(quote: &deadcat_sdk::TradeQuote) -> TradeQuoteResponse {
    TradeQuoteResponse {
        total_input: quote.total_input,
        total_output: quote.total_output,
        effective_price: quote.effective_price,
        legs: quote.legs.iter().cloned().map(map_route_leg).collect(),
    }
}

fn quote_matches_expected(actual: &TradeQuoteResponse, expected: &TradeQuoteResponse) -> bool {
    actual.total_input == expected.total_input
        && actual.total_output == expected.total_output
        && actual.legs == expected.legs
}

fn validate_expected_quote(
    live_quote: &TradeQuoteResponse,
    expected_quote: Option<&TradeQuoteResponse>,
) -> Result<(), String> {
    if let Some(expected_quote) = expected_quote {
        if !quote_matches_expected(live_quote, expected_quote) {
            return Err(
                "quote changed before execution; request a fresh quote and confirm again".into(),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod trade_command_tests {
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        execute_trade_inner, get_pool_price_history_inner, get_price_history_inner,
        parse_trade_direction, parse_trade_side, quote_matches_expected, quote_trade_inner,
        scan_lmsr_pool_inner, validate_expected_quote, ExecuteTradeRequest, ExecuteTradeResponse,
        RouteLegResponse, RouteLegSourceResponse, TradeQuoteRequest, TradeQuoteResponse,
    };
    use crate::state::AppStateManager;
    use crate::NodeState;
    use nostr_sdk::Keys;
    use tauri::test::{mock_builder, mock_context, noop_assets};
    use tauri::Manager;

    fn sample_quote(effective_price: f64) -> TradeQuoteResponse {
        TradeQuoteResponse {
            total_input: 10_000,
            total_output: 123,
            effective_price,
            legs: vec![
                RouteLegResponse {
                    source: RouteLegSourceResponse::LimitOrder {
                        order_id: "order-a".to_string(),
                        price: 81,
                        lots: 100,
                    },
                    input_amount: 7_000,
                    output_amount: 90,
                },
                RouteLegResponse {
                    source: RouteLegSourceResponse::LmsrPool {
                        pool_id: "pool-a".to_string(),
                        old_s_index: 10,
                        new_s_index: 11,
                    },
                    input_amount: 3_000,
                    output_amount: 33,
                },
            ],
        }
    }

    fn sample_contract_params_json() -> String {
        serde_json::json!({
            "oracle_public_key": vec![1u8; 32],
            "collateral_asset_id": vec![2u8; 32],
            "yes_token_asset": vec![3u8; 32],
            "no_token_asset": vec![4u8; 32],
            "yes_reissuance_token": vec![5u8; 32],
            "no_reissuance_token": vec![6u8; 32],
            "collateral_per_token": 1000u64,
            "expiry_time": 12345u32
        })
        .to_string()
    }

    fn mock_trade_app() -> tauri::App<tauri::test::MockRuntime> {
        mock_builder()
            .manage(NodeState::default())
            .build(mock_context(noop_assets()))
            .expect("build mock tauri app")
    }

    fn unique_test_app_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after unix epoch")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("deadcat-{label}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&path).expect("create test app dir");
        path
    }

    fn mock_scan_app() -> (
        tauri::App<tauri::test::MockRuntime>,
        Arc<Mutex<deadcat_store::DeadcatStore>>,
    ) {
        let mut manager = AppStateManager::new(unique_test_app_dir("scan-lmsr"));
        manager.set_network(crate::Network::Testnet);
        let store = manager.store().cloned().expect("store initialized");
        let app = mock_builder()
            .manage(NodeState::default())
            .manage(Mutex::new(manager))
            .build(mock_context(noop_assets()))
            .expect("build mock tauri app");
        (app, store)
    }

    fn sample_price_transition(
        pool_id: &str,
        market_id: &str,
        transition_txid: &str,
        block_height: u32,
    ) -> deadcat_sdk::LmsrPriceTransitionInput {
        deadcat_sdk::LmsrPriceTransitionInput {
            pool_id: pool_id.to_string(),
            market_id: market_id.to_string(),
            transition_txid: transition_txid.to_string(),
            old_s_index: 10,
            new_s_index: 11,
            reserve_yes: 1_000,
            reserve_no: 900,
            reserve_collateral: 2_000,
            implied_yes_price_bps: 5_100,
            block_height,
        }
    }

    #[test]
    fn parse_trade_side_accepts_yes_no() {
        assert!(parse_trade_side("yes").is_ok());
        assert!(parse_trade_side("no").is_ok());
        assert!(parse_trade_side("YES").is_ok());
    }

    #[test]
    fn parse_trade_side_rejects_unknown() {
        assert!(parse_trade_side("maybe").is_err());
    }

    #[test]
    fn parse_trade_direction_accepts_buy_sell() {
        assert!(parse_trade_direction("buy").is_ok());
        assert!(parse_trade_direction("sell").is_ok());
        assert!(parse_trade_direction("BUY").is_ok());
    }

    #[test]
    fn parse_trade_direction_rejects_unknown() {
        assert!(parse_trade_direction("hold").is_err());
    }

    #[test]
    fn quote_match_ignores_effective_price() {
        let expected = sample_quote(1.0);
        let actual = sample_quote(1.2345);
        assert!(quote_matches_expected(&actual, &expected));
    }

    #[test]
    fn quote_match_rejects_leg_differences() {
        let expected = sample_quote(1.0);
        let mut actual = sample_quote(1.0);
        actual.legs[0].output_amount += 1;
        assert!(!quote_matches_expected(&actual, &expected));
    }

    #[test]
    fn validate_expected_quote_rejects_mismatch() {
        let live = sample_quote(1.0);
        let expected = sample_quote(1.0);
        let mut mismatched = expected.clone();
        mismatched.total_output += 1;
        let err = validate_expected_quote(&live, Some(&mismatched)).unwrap_err();
        assert!(err.contains("quote changed before execution"));
    }

    #[test]
    fn trade_quote_request_roundtrip() {
        let request = TradeQuoteRequest {
            contract_params_json: "{\"oracle_public_key\":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],\"collateral_asset_id\":[2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2],\"yes_token_asset\":[3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3],\"no_token_asset\":[4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4],\"yes_reissuance_token\":[5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5],\"no_reissuance_token\":[6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6],\"collateral_per_token\":1000,\"expiry_time\":12345}".to_string(),
            market_id: "market-a".to_string(),
            side: "yes".to_string(),
            direction: "buy".to_string(),
            exact_input: 1000,
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: TradeQuoteRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.market_id, "market-a");
        assert_eq!(parsed.side, "yes");
        assert_eq!(parsed.direction, "buy");
        assert_eq!(parsed.exact_input, 1000);
    }

    #[test]
    fn trade_quote_response_roundtrip() {
        let response = sample_quote(12.34);
        let json = serde_json::to_string(&response).unwrap();
        let parsed: TradeQuoteResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.total_input, response.total_input);
        assert_eq!(parsed.total_output, response.total_output);
        assert_eq!(parsed.legs, response.legs);
    }

    #[test]
    fn execute_trade_request_roundtrip() {
        let expected_quote = sample_quote(9.87);
        let request = ExecuteTradeRequest {
            contract_params_json: "{\"oracle_public_key\":[1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1],\"collateral_asset_id\":[2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2],\"yes_token_asset\":[3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3,3],\"no_token_asset\":[4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4,4],\"yes_reissuance_token\":[5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5,5],\"no_reissuance_token\":[6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6,6],\"collateral_per_token\":1000,\"expiry_time\":12345}".to_string(),
            market_id: "market-b".to_string(),
            side: "no".to_string(),
            direction: "sell".to_string(),
            exact_input: 2000,
            fee_amount: Some(600),
            expected_quote: Some(expected_quote.clone()),
        };
        let json = serde_json::to_string(&request).unwrap();
        let parsed: ExecuteTradeRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.market_id, "market-b");
        assert_eq!(parsed.side, "no");
        assert_eq!(parsed.direction, "sell");
        assert_eq!(parsed.fee_amount, Some(600));
        assert_eq!(parsed.expected_quote.unwrap().legs, expected_quote.legs);
    }

    #[test]
    fn execute_trade_response_roundtrip() {
        let response = ExecuteTradeResponse {
            txid: "abc123".to_string(),
            total_input: 1000,
            total_output: 99,
            num_orders_filled: 1,
            pool_used: true,
            new_reserves: Some(deadcat_sdk::PoolReserves {
                r_yes: 10,
                r_no: 20,
                r_lbtc: 30,
            }),
        };
        let json = serde_json::to_string(&response).unwrap();
        let parsed: ExecuteTradeResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.txid, "abc123");
        assert_eq!(parsed.total_input, 1000);
        assert_eq!(parsed.total_output, 99);
        assert_eq!(parsed.num_orders_filled, 1);
        assert!(parsed.pool_used);
        assert_eq!(
            parsed.new_reserves.unwrap(),
            deadcat_sdk::PoolReserves {
                r_yes: 10,
                r_no: 20,
                r_lbtc: 30,
            }
        );
    }

    #[tokio::test]
    async fn quote_trade_command_path_rejects_uninitialized_node() {
        let app = mock_trade_app();
        let request = TradeQuoteRequest {
            contract_params_json: sample_contract_params_json(),
            market_id: "market-a".to_string(),
            side: "yes".to_string(),
            direction: "buy".to_string(),
            exact_input: 10_000,
        };
        let err = quote_trade_inner(request, app.handle().clone())
            .await
            .expect_err("expected quote_trade error");
        assert!(err.contains("Node not initialized"));
    }

    #[tokio::test]
    async fn quote_trade_command_path_rejects_invalid_side() {
        let app = mock_trade_app();
        let request = TradeQuoteRequest {
            contract_params_json: sample_contract_params_json(),
            market_id: "market-a".to_string(),
            side: "maybe".to_string(),
            direction: "buy".to_string(),
            exact_input: 10_000,
        };
        let err = quote_trade_inner(request, app.handle().clone())
            .await
            .expect_err("expected quote_trade error");
        assert!(err.contains("invalid side"));
    }

    #[tokio::test]
    async fn execute_trade_command_path_rejects_uninitialized_node() {
        let app = mock_trade_app();
        let request = ExecuteTradeRequest {
            contract_params_json: sample_contract_params_json(),
            market_id: "market-b".to_string(),
            side: "yes".to_string(),
            direction: "buy".to_string(),
            exact_input: 10_000,
            fee_amount: Some(500),
            expected_quote: None,
        };
        let result = execute_trade_inner(request, app.handle().clone()).await;
        let err = match result {
            Ok(_) => panic!("expected execute_trade error"),
            Err(err) => err,
        };
        assert!(err.contains("Node not initialized"));
    }

    #[tokio::test]
    async fn execute_trade_command_path_rejects_invalid_direction() {
        let app = mock_trade_app();
        let request = ExecuteTradeRequest {
            contract_params_json: sample_contract_params_json(),
            market_id: "market-b".to_string(),
            side: "yes".to_string(),
            direction: "hold".to_string(),
            exact_input: 10_000,
            fee_amount: Some(500),
            expected_quote: None,
        };
        let result = execute_trade_inner(request, app.handle().clone()).await;
        let err = match result {
            Ok(_) => panic!("expected execute_trade error"),
            Err(err) => err,
        };
        assert!(err.contains("invalid direction"));
    }

    #[tokio::test]
    async fn scan_lmsr_pool_repairs_store_metadata_before_wallet_check() {
        let (app, store) = mock_scan_app();
        let keys = Keys::generate();
        let (node, _rx) = deadcat_sdk::DeadcatNode::with_store(
            keys.clone(),
            deadcat_sdk::Network::LiquidTestnet,
            store.clone(),
            deadcat_sdk::DiscoveryConfig {
                relays: vec![],
                network_tag: "liquid-testnet".to_string(),
                ..Default::default()
            },
        );
        {
            let node_state = app.state::<NodeState>();
            let mut guard = node_state.node.lock().await;
            *guard = Some(node);
        }

        let announcement = deadcat_sdk::testing::test_lmsr_pool_announcement(
            deadcat_sdk::Network::LiquidTestnet,
            0x51,
        );
        let event = deadcat_sdk::build_pool_event(
            &keys,
            &announcement,
            deadcat_sdk::Network::LiquidTestnet.discovery_tag(),
        )
        .unwrap();
        let mut ingest = deadcat_sdk::testing::test_lmsr_pool_ingest_input(
            deadcat_sdk::Network::LiquidTestnet,
            0x51,
        );
        ingest.initial_reserve_outpoints = [
            format!("{}:7", ingest.creation_txid),
            format!("{}:8", ingest.creation_txid),
            format!("{}:9", ingest.creation_txid),
        ];
        ingest.lmsr_table_values = None;
        ingest.nostr_event_id = Some(event.id.to_hex());
        ingest.nostr_event_json = Some(serde_json::to_string(&event).unwrap());
        store.lock().unwrap().ingest_lmsr_pool(&ingest).unwrap();

        let err = match scan_lmsr_pool_inner(ingest.pool_id.clone(), app.handle().clone()).await {
            Ok(_) => panic!("scan should stop at wallet lock after metadata resolution"),
            Err(err) => err,
        };
        assert!(err.contains("wallet is locked"));

        let repaired = store
            .lock()
            .unwrap()
            .list_lmsr_pool_sync_info()
            .unwrap()
            .into_iter()
            .find(|pool| pool.pool_id == ingest.pool_id)
            .unwrap();
        assert_eq!(repaired.market_id, announcement.market_id);
        assert_eq!(
            repaired
                .stored_initial_reserve_outpoints
                .unwrap()
                .as_slice(),
            announcement.initial_reserve_outpoints.as_slice()
        );
    }

    #[tokio::test]
    async fn get_price_history_reads_from_store_when_node_is_not_initialized() {
        let (app, store) = mock_scan_app();
        store
            .lock()
            .unwrap()
            .record_price_transition(&sample_price_transition("pool-a", "market-a", "tx-a", 101))
            .unwrap();

        let entries =
            get_price_history_inner("market-a".to_string(), Some(10), app.handle().clone())
                .await
                .expect("market history from store");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].pool_id, "pool-a");
        assert_eq!(entries[0].market_id, "market-a");
        assert_eq!(entries[0].block_height, 101);
    }

    #[tokio::test]
    async fn get_pool_price_history_reads_from_store_when_node_is_not_initialized() {
        let (app, store) = mock_scan_app();
        store
            .lock()
            .unwrap()
            .record_price_transition(&sample_price_transition("pool-b", "market-b", "tx-b", 202))
            .unwrap();

        let entries = get_pool_price_history_inner(
            "pool-b".to_string(),
            Some(10),
            Some(200),
            app.handle().clone(),
        )
        .await
        .expect("pool history from store");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].pool_id, "pool-b");
        assert_eq!(entries[0].market_id, "market-b");
        assert_eq!(entries[0].block_height, 202);
    }

    #[tokio::test]
    async fn get_price_history_errors_when_neither_node_nor_store_is_initialized() {
        let app = mock_trade_app();

        let err = match get_price_history_inner("market-a".to_string(), None, app.handle().clone())
            .await
        {
            Ok(_) => panic!("expected get_price_history error"),
            Err(err) => err,
        };

        assert!(err.contains("Store not initialized"));
    }

    #[tokio::test]
    async fn get_pool_price_history_errors_when_neither_node_nor_store_is_initialized() {
        let app = mock_trade_app();

        let err = match get_pool_price_history_inner(
            "pool-a".to_string(),
            None,
            None,
            app.handle().clone(),
        )
        .await
        {
            Ok(_) => panic!("expected get_pool_price_history error"),
            Err(err) => err,
        };

        assert!(err.contains("Store not initialized"));
    }
}

#[cfg(test)]
mod limit_order_command_tests {
    use std::sync::{Arc, Mutex};

    use super::resolve_create_limit_order_index;

    #[tokio::test]
    async fn resolve_create_limit_order_index_returns_zero_for_empty_wallet() {
        let keys = nostr_sdk::prelude::Keys::generate();
        let store = Arc::new(Mutex::new(
            deadcat_store::DeadcatStore::open_in_memory().unwrap(),
        ));
        let config = deadcat_sdk::DiscoveryConfig {
            relays: Vec::new(),
            network_tag: "liquid-testnet".to_string(),
            ..Default::default()
        };
        let (node, _rx) = deadcat_sdk::DeadcatNode::with_store(
            keys,
            deadcat_sdk::Network::LiquidTestnet,
            store.clone(),
            config,
        );

        let wallet_dir = std::env::temp_dir().join(format!(
            "deadcat-limit-order-index-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&wallet_dir).unwrap();
        node.unlock_wallet(
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
            "tcp://127.0.0.1:1",
            &wallet_dir,
        )
        .unwrap();

        assert_eq!(resolve_create_limit_order_index(&node).await.unwrap(), 0);

        let _ = std::fs::remove_dir_all(wallet_dir);
    }
}

async fn quote_trade_inner<R: tauri::Runtime>(
    request: TradeQuoteRequest,
    app: tauri::AppHandle<R>,
) -> Result<TradeQuoteResponse, String> {
    let params: deadcat_sdk::PredictionMarketParams =
        serde_json::from_str(&request.contract_params_json)
            .map_err(|e| format!("invalid contract params: {e}"))?;
    let side = parse_trade_side(&request.side)?;
    let direction = parse_trade_direction(&request.direction)?;

    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    let quote = node
        .quote_trade(
            params,
            &request.market_id,
            side,
            direction,
            deadcat_sdk::TradeAmount::ExactInput(request.exact_input),
        )
        .await
        .map_err(|e| format!("{e}"))?;

    Ok(map_trade_quote(&quote))
}

async fn execute_trade_inner<R: tauri::Runtime>(
    request: ExecuteTradeRequest,
    app: tauri::AppHandle<R>,
) -> Result<ExecuteTradeResponse, String> {
    let params: deadcat_sdk::PredictionMarketParams =
        serde_json::from_str(&request.contract_params_json)
            .map_err(|e| format!("invalid contract params: {e}"))?;
    let side = parse_trade_side(&request.side)?;
    let direction = parse_trade_direction(&request.direction)?;
    let fee_amount = request.fee_amount.unwrap_or(500);

    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    let quote = node
        .quote_trade(
            params,
            &request.market_id,
            side,
            direction,
            deadcat_sdk::TradeAmount::ExactInput(request.exact_input),
        )
        .await
        .map_err(|e| format!("{e}"))?;
    let live_quote = map_trade_quote(&quote);
    validate_expected_quote(&live_quote, request.expected_quote.as_ref())?;
    let result = node
        .execute_trade(quote, fee_amount, &request.market_id)
        .await
        .map_err(|e| format!("{e}"))?;
    drop(guard);

    bump_revision_and_emit(&app).await?;

    Ok(ExecuteTradeResponse {
        txid: result.txid.to_string(),
        total_input: result.total_input,
        total_output: result.total_output,
        num_orders_filled: result.num_orders_filled,
        pool_used: result.pool_used,
        new_reserves: result.new_reserves,
    })
}

#[tauri::command]
pub async fn quote_trade(
    request: TradeQuoteRequest,
    app: tauri::AppHandle,
) -> Result<TradeQuoteResponse, String> {
    quote_trade_inner(request, app).await
}

#[tauri::command]
pub async fn execute_trade(
    request: ExecuteTradeRequest,
    app: tauri::AppHandle,
) -> Result<ExecuteTradeResponse, String> {
    execute_trade_inner(request, app).await
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
        result.push(market_info_to_discovered(info, None, None));
    }
    Ok(result)
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
        anchor: info.anchor.clone(),
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
// Limit order commands
// =========================================================================

#[derive(Serialize, Deserialize)]
pub struct CreateLimitOrderRequest {
    pub contract_params_json: String,
    pub market_id: String,
    pub side: String,
    pub direction: String,
    pub price: u64,
    pub amount: u64,
    #[serde(default)]
    pub fee_amount: Option<u64>,
}

#[derive(Serialize, Deserialize)]
pub struct CreateLimitOrderResponse {
    pub txid: String,
    pub nostr_event_id: String,
    pub covenant_address: String,
    pub order_amount: u64,
    pub order_index: u32,
}

async fn resolve_create_limit_order_index(
    node: &deadcat_sdk::DeadcatNode<deadcat_store::DeadcatStore>,
) -> Result<u32, String> {
    node.next_maker_order_index()
        .await
        .map_err(|e| format!("{e}"))
}

#[tauri::command]
pub async fn create_limit_order(
    request: CreateLimitOrderRequest,
    app: tauri::AppHandle,
) -> Result<CreateLimitOrderResponse, String> {
    let params: deadcat_sdk::PredictionMarketParams =
        serde_json::from_str(&request.contract_params_json)
            .map_err(|e| format!("invalid contract params: {e}"))?;
    let side = parse_trade_side(&request.side)?;
    let direction = parse_trade_direction(&request.direction)?;

    let base_asset_id = match side {
        deadcat_sdk::TradeSide::Yes => params.yes_token_asset,
        deadcat_sdk::TradeSide::No => params.no_token_asset,
    };
    let quote_asset_id = params.collateral_asset_id;

    let order_direction = match direction {
        deadcat_sdk::TradeDirection::Buy => deadcat_sdk::OrderDirection::SellQuote,
        deadcat_sdk::TradeDirection::Sell => deadcat_sdk::OrderDirection::SellBase,
    };

    let direction_label = format!(
        "{}-{}",
        match direction {
            deadcat_sdk::TradeDirection::Buy => "buy",
            deadcat_sdk::TradeDirection::Sell => "sell",
        },
        match side {
            deadcat_sdk::TradeSide::Yes => "yes",
            deadcat_sdk::TradeSide::No => "no",
        }
    );

    let fee_amount = request.fee_amount.unwrap_or(500);

    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    let order_index = resolve_create_limit_order_index(node).await?;

    let (result, event_id) = node
        .create_limit_order(
            base_asset_id,
            quote_asset_id,
            request.price,
            request.amount,
            order_direction,
            1,
            1,
            order_index,
            fee_amount,
            request.market_id,
            direction_label,
        )
        .await
        .map_err(|e| format!("{e}"))?;
    drop(guard);

    bump_revision_and_emit(&app).await?;

    Ok(CreateLimitOrderResponse {
        txid: result.txid.to_string(),
        nostr_event_id: event_id.to_hex(),
        covenant_address: result.covenant_address,
        order_amount: result.order_amount,
        order_index,
    })
}

#[derive(Serialize, Deserialize)]
pub struct CancelLimitOrderRequest {
    pub market_id: String,
    pub base_asset_id: String,
    pub quote_asset_id: String,
    pub price: u64,
    pub min_fill_lots: u64,
    pub min_remainder_lots: u64,
    pub direction: String,
    pub maker_base_pubkey: String,
    pub order_nonce: String,
    pub cosigner_pubkey: String,
    pub maker_receive_spk_hash: String,
    #[serde(default)]
    pub fee_amount: Option<u64>,
    #[serde(default)]
    pub order_index: Option<u32>,
}

#[derive(Serialize, Deserialize)]
pub struct CancelLimitOrderResponse {
    pub txid: String,
    pub refunded_amount: u64,
}

fn decode_hex_32(hex_str: &str, field: &str) -> Result<[u8; 32], String> {
    hex::decode(hex_str)
        .map_err(|e| format!("invalid {field} hex: {e}"))?
        .try_into()
        .map_err(|_| format!("{field} must be exactly 32 bytes"))
}

fn parse_order_direction(direction: &str) -> Result<deadcat_sdk::OrderDirection, String> {
    match direction.trim().to_ascii_lowercase().as_str() {
        "sell-base" => Ok(deadcat_sdk::OrderDirection::SellBase),
        "sell-quote" => Ok(deadcat_sdk::OrderDirection::SellQuote),
        other => Err(format!(
            "invalid order direction '{other}', expected 'sell-base' or 'sell-quote'"
        )),
    }
}

#[tauri::command]
pub async fn cancel_limit_order(
    request: CancelLimitOrderRequest,
    app: tauri::AppHandle,
) -> Result<CancelLimitOrderResponse, String> {
    let base_asset_id = decode_hex_32(&request.base_asset_id, "base_asset_id")?;
    let quote_asset_id = decode_hex_32(&request.quote_asset_id, "quote_asset_id")?;
    let maker_pubkey = decode_hex_32(&request.maker_base_pubkey, "maker_base_pubkey")?;
    let cosigner_pubkey = decode_hex_32(&request.cosigner_pubkey, "cosigner_pubkey")?;
    let maker_receive_spk_hash =
        decode_hex_32(&request.maker_receive_spk_hash, "maker_receive_spk_hash")?;
    let direction = parse_order_direction(&request.direction)?;

    let params = deadcat_sdk::MakerOrderParams {
        base_asset_id,
        quote_asset_id,
        price: request.price,
        min_fill_lots: request.min_fill_lots,
        min_remainder_lots: request.min_remainder_lots,
        direction,
        maker_receive_spk_hash,
        cosigner_pubkey,
        maker_pubkey,
    };

    let fee_amount = request.fee_amount.unwrap_or(500);

    let order_index = request
        .order_index
        .unwrap_or(ORDER_INDEX_AUTO_RESOLVE_SENTINEL);

    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    let result = node
        .cancel_limit_order(params, maker_pubkey, order_index, fee_amount)
        .await
        .map_err(|e| format!("{e}"))?;
    drop(guard);

    bump_revision_and_emit(&app).await?;

    Ok(CancelLimitOrderResponse {
        txid: result.txid.to_string(),
        refunded_amount: result.refunded_amount,
    })
}

// =========================================================================
// Own order listing (for transaction labeling)
// =========================================================================

#[derive(Serialize)]
pub struct OwnOrderSummary {
    pub creation_txid: Option<String>,
    pub market_id: Option<String>,
    pub direction_label: Option<String>,
    pub price: u64,
    pub offered_amount: Option<u64>,
    pub order_status: String,
}

#[tauri::command]
pub fn list_own_orders(app: tauri::AppHandle) -> Result<Vec<OwnOrderSummary>, String> {
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

    // Only return orders that have local creation metadata (creation_txid IS NOT NULL)
    let all_orders = store
        .list_maker_orders(&deadcat_store::OrderFilter::default())
        .map_err(|e| format!("list orders: {e}"))?;

    let own: Vec<OwnOrderSummary> = all_orders
        .into_iter()
        .filter(|o| o.creation_txid.is_some())
        .map(|o| OwnOrderSummary {
            creation_txid: o.creation_txid,
            market_id: o.market_id,
            direction_label: o.direction_label,
            price: o.params.price,
            offered_amount: o.offered_amount,
            order_status: format!("{:?}", o.status),
        })
        .collect();

    Ok(own)
}

// =========================================================================
// LMSR Pool commands
// =========================================================================

#[tauri::command]
pub fn generate_lmsr_table(
    liquidity_param: f64,
    table_depth: u32,
    q_step_lots: u64,
    s_bias: u64,
    half_payout_sats: u64,
) -> Result<Vec<u64>, String> {
    deadcat_sdk::generate_lmsr_table(
        liquidity_param,
        table_depth,
        q_step_lots,
        s_bias,
        half_payout_sats,
    )
    .map_err(|e| format!("{e}"))
}

#[derive(Deserialize)]
pub struct CreateLmsrPoolRequest {
    pub market_params_json: String,
    pub pool_params_json: String,
    pub initial_s_index: u64,
    pub initial_reserves_yes: u64,
    pub initial_reserves_no: u64,
    pub initial_reserves_lbtc: u64,
    pub table_values: Vec<u64>,
    pub fee_amount: Option<u64>,
}

#[derive(Serialize)]
pub struct CreateLmsrPoolResponse {
    pub txid: String,
    pub pool_id: String,
}

#[tauri::command]
pub async fn create_lmsr_pool(
    request: CreateLmsrPoolRequest,
    app: tauri::AppHandle,
) -> Result<CreateLmsrPoolResponse, String> {
    let market_params: deadcat_sdk::PredictionMarketParams =
        serde_json::from_str(&request.market_params_json)
            .map_err(|e| format!("invalid market params: {e}"))?;
    let pool_params: deadcat_sdk::LmsrPoolParams = serde_json::from_str(&request.pool_params_json)
        .map_err(|e| format!("invalid pool params: {e}"))?;

    let sdk_request = deadcat_sdk::CreateLmsrPoolRequest {
        market_params,
        pool_params,
        initial_s_index: request.initial_s_index,
        initial_reserves: deadcat_sdk::PoolReserves {
            r_yes: request.initial_reserves_yes,
            r_no: request.initial_reserves_no,
            r_lbtc: request.initial_reserves_lbtc,
        },
        table_values: request.table_values,
        fee_amount: request.fee_amount.unwrap_or(500),
    };

    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    let result = node
        .create_lmsr_pool(sdk_request)
        .await
        .map_err(|e| format!("{e}"))?;
    drop(guard);

    bump_revision_and_emit(&app).await?;

    Ok(CreateLmsrPoolResponse {
        txid: result.txid.to_string(),
        pool_id: result.snapshot.locator.pool_id.to_hex(),
    })
}

#[derive(Serialize)]
pub struct ScanLmsrPoolResponse {
    pub pool_id: String,
    pub current_s_index: u64,
    pub reserve_yes: u64,
    pub reserve_no: u64,
    pub reserve_collateral: u64,
}

async fn scan_lmsr_pool_inner<R: tauri::Runtime>(
    pool_id: String,
    app: tauri::AppHandle<R>,
) -> Result<ScanLmsrPoolResponse, String> {
    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    let locator = node
        .resolve_lmsr_pool_locator(&pool_id)
        .map_err(|e| format!("{e}"))?;
    let snapshot = node
        .scan_lmsr_pool(locator)
        .await
        .map_err(|e| format!("{e}"))?;
    drop(guard);

    Ok(ScanLmsrPoolResponse {
        pool_id,
        current_s_index: snapshot.current_s_index,
        reserve_yes: snapshot.reserves.r_yes,
        reserve_no: snapshot.reserves.r_no,
        reserve_collateral: snapshot.reserves.r_lbtc,
    })
}

#[tauri::command]
pub async fn scan_lmsr_pool(
    pool_id: String,
    app: tauri::AppHandle,
) -> Result<ScanLmsrPoolResponse, String> {
    scan_lmsr_pool_inner(pool_id, app).await
}

#[derive(Deserialize)]
pub struct AdjustLmsrPoolTauriRequest {
    pub pool_id: String,
    pub new_reserves_yes: u64,
    pub new_reserves_no: u64,
    pub new_reserves_lbtc: u64,
    pub table_values: Vec<u64>,
    pub fee_amount: Option<u64>,
    pub pool_index: Option<u32>,
}

#[derive(Serialize)]
pub struct AdjustLmsrPoolResponse {
    pub txid: String,
    pub pool_id: String,
    pub current_s_index: u64,
    pub reserve_yes: u64,
    pub reserve_no: u64,
    pub reserve_collateral: u64,
}

/// Adjust an LMSR pool's reserves via AdminAdjust transition.
///
/// Requires the pool scan to return unblinded UTXOs — not yet wired.
/// Returns an error until full pool-UTXO passthrough is implemented.
#[tauri::command]
pub async fn adjust_lmsr_pool(
    _request: AdjustLmsrPoolTauriRequest,
    _app: tauri::AppHandle,
) -> Result<AdjustLmsrPoolResponse, String> {
    Err("adjust_lmsr_pool is not yet fully wired — pool UTXO passthrough required".to_string())
}

#[derive(Deserialize)]
pub struct CloseLmsrPoolTauriRequest {
    pub pool_id: String,
    pub table_values: Vec<u64>,
    pub fee_amount: Option<u64>,
    pub pool_index: Option<u32>,
}

#[derive(Serialize)]
pub struct CloseLmsrPoolResponse {
    pub txid: String,
    pub reclaimed_yes: u64,
    pub reclaimed_no: u64,
    pub reclaimed_collateral: u64,
}

/// Close an LMSR pool by adjusting reserves to covenant minimums.
///
/// Requires the pool scan to return unblinded UTXOs — not yet wired.
/// Returns an error until full pool-UTXO passthrough is implemented.
#[tauri::command]
pub async fn close_lmsr_pool(
    _request: CloseLmsrPoolTauriRequest,
    _app: tauri::AppHandle,
) -> Result<CloseLmsrPoolResponse, String> {
    Err("close_lmsr_pool is not yet fully wired — pool UTXO passthrough required".to_string())
}

#[derive(Serialize)]
pub struct LmsrPoolInfoResponse {
    pub pool_id: String,
    pub market_id: String,
    pub creation_txid: String,
    pub current_s_index: u64,
    pub reserve_yes: u64,
    pub reserve_no: u64,
    pub reserve_collateral: u64,
    pub state_source: String,
    pub params_json: String,
    pub created_at: String,
    pub updated_at: String,
}

#[tauri::command]
pub fn list_lmsr_pools(
    market_id: Option<String>,
    app: tauri::AppHandle,
) -> Result<Vec<LmsrPoolInfoResponse>, String> {
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

    let pools = store
        .list_lmsr_pools(&deadcat_store::LmsrPoolFilter {
            market_id,
            ..Default::default()
        })
        .map_err(|e| format!("list pools: {e}"))?;

    Ok(pools
        .into_iter()
        .map(|p| LmsrPoolInfoResponse {
            pool_id: p.pool_id,
            market_id: p.market_id,
            creation_txid: p.creation_txid,
            current_s_index: p.current_s_index,
            reserve_yes: p.reserve_yes,
            reserve_no: p.reserve_no,
            reserve_collateral: p.reserve_collateral,
            state_source: p.state_source,
            params_json: p.params_json,
            created_at: p.created_at,
            updated_at: p.updated_at,
        })
        .collect())
}

#[derive(Serialize)]
pub struct PriceHistoryEntryResponse {
    pub pool_id: String,
    pub market_id: String,
    pub transition_txid: String,
    pub old_s_index: u64,
    pub new_s_index: u64,
    pub reserve_yes: u64,
    pub reserve_no: u64,
    pub reserve_collateral: u64,
    pub implied_yes_price_bps: u16,
    pub block_height: u32,
}

fn map_price_history_entries(
    entries: Vec<deadcat_sdk::LmsrPriceHistoryEntry>,
) -> Vec<PriceHistoryEntryResponse> {
    entries
        .into_iter()
        .map(|e| PriceHistoryEntryResponse {
            pool_id: e.pool_id,
            market_id: e.market_id,
            transition_txid: e.transition_txid,
            old_s_index: e.old_s_index,
            new_s_index: e.new_s_index,
            reserve_yes: e.reserve_yes,
            reserve_no: e.reserve_no,
            reserve_collateral: e.reserve_collateral,
            implied_yes_price_bps: e.implied_yes_price_bps,
            block_height: e.block_height,
        })
        .collect()
}

fn get_store<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> Result<std::sync::Arc<std::sync::Mutex<deadcat_store::DeadcatStore>>, String> {
    let state_handle = app
        .try_state::<Mutex<AppStateManager>>()
        .ok_or_else(|| "Store not initialized".to_string())?;
    Ok({
        let mgr = state_handle
            .lock()
            .map_err(|_| "state lock failed".to_string())?;
        mgr.store()
            .cloned()
            .ok_or_else(|| "Store not initialized".to_string())?
    })
}

// Read-only LMSR history stays available before node init by falling back to
// the confirmed rows already persisted in the store.
fn get_market_price_history_from_store<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    market_id: &str,
    limit: Option<i64>,
) -> Result<Vec<deadcat_sdk::LmsrPriceHistoryEntry>, String> {
    let store_arc = get_store(app)?;
    let mut store = store_arc
        .lock()
        .map_err(|_| "store lock failed".to_string())?;
    store
        .get_market_price_history(market_id, None, limit)
        .map_err(|e| format!("get price history: {e}"))
}

fn get_pool_price_history_from_store<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    pool_id: &str,
    since_block_height: Option<u32>,
    limit: Option<i64>,
) -> Result<Vec<deadcat_sdk::LmsrPriceHistoryEntry>, String> {
    let store_arc = get_store(app)?;
    let mut store = store_arc
        .lock()
        .map_err(|_| "store lock failed".to_string())?;
    store
        .get_pool_price_history(pool_id, since_block_height, limit)
        .map_err(|e| format!("get pool price history: {e}"))
}

#[tauri::command]
pub async fn get_price_history(
    market_id: String,
    limit: Option<i64>,
    app: tauri::AppHandle,
) -> Result<Vec<PriceHistoryEntryResponse>, String> {
    get_price_history_inner(market_id, limit, app).await
}

async fn get_price_history_inner<R: tauri::Runtime>(
    market_id: String,
    limit: Option<i64>,
    app: tauri::AppHandle<R>,
) -> Result<Vec<PriceHistoryEntryResponse>, String> {
    let entries = {
        let node_state = app.state::<NodeState>();
        let guard = node_state.node.lock().await;
        if let Some(node) = guard.as_ref() {
            node.get_market_price_history(&market_id, None, limit)
                .map_err(|e| format!("get price history: {e}"))?
        } else {
            drop(guard);
            get_market_price_history_from_store(&app, &market_id, limit)?
        }
    };

    Ok(map_price_history_entries(entries))
}

#[tauri::command]
pub async fn get_pool_price_history(
    pool_id: String,
    limit: Option<i64>,
    since_block_height: Option<u32>,
    app: tauri::AppHandle,
) -> Result<Vec<PriceHistoryEntryResponse>, String> {
    get_pool_price_history_inner(pool_id, limit, since_block_height, app).await
}

async fn get_pool_price_history_inner<R: tauri::Runtime>(
    pool_id: String,
    limit: Option<i64>,
    since_block_height: Option<u32>,
    app: tauri::AppHandle<R>,
) -> Result<Vec<PriceHistoryEntryResponse>, String> {
    let entries = {
        let node_state = app.state::<NodeState>();
        let guard = node_state.node.lock().await;
        if let Some(node) = guard.as_ref() {
            node.get_pool_price_history(&pool_id, since_block_height, limit)
                .map_err(|e| format!("get pool price history: {e}"))?
        } else {
            drop(guard);
            get_pool_price_history_from_store(&app, &pool_id, since_block_height, limit)?
        }
    };

    Ok(map_price_history_entries(entries))
}
