use std::sync::Mutex;
use std::time::Duration;

use deadcat_store::MarketFilter;
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

use crate::discovery::{
    self, ContractMetadata, CreateContractRequest, DiscoveredMarket, IdentityResponse,
};
use crate::state::AppStateManager;
use crate::{NodeState, NostrAppState};

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
    if request.starting_yes_price < 1 || request.starting_yes_price > 99 {
        return Err("starting_yes_price must be 1-99".to_string());
    }
    if request.collateral_per_token == 0 {
        return Err("collateral_per_token must be > 0".to_string());
    }
    Ok(())
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
        starting_yes_price: request.starting_yes_price,
    };

    let announcement = deadcat_sdk::ContractAnnouncement {
        version: 1,
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
        starting_yes_price: request.starting_yes_price,
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
        starting_yes_price: request.starting_yes_price,
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
    creation_txid: String,
    pairs: u64,
    app: tauri::AppHandle,
) -> Result<IssuanceResultResponse, String> {
    let params: deadcat_sdk::PredictionMarketParams =
        serde_json::from_str(&contract_params_json)
            .map_err(|e| format!("invalid contract params: {e}"))?;

    let txid: lwk_wollet::elements::Txid = creation_txid
        .parse()
        .map_err(|e| format!("invalid txid: {e}"))?;

    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    let result = node
        .issue_tokens(params, txid, pairs, 500)
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
        .cancel_tokens(params, pairs, 500)
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
        .resolve_market(params, outcome_yes, sig_bytes, 500)
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
        .redeem_tokens(params, tokens, 500)
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
        .redeem_expired(params, token_asset, tokens, 500)
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
    app: tauri::AppHandle,
) -> Result<MarketStateResponse, String> {
    let params: deadcat_sdk::PredictionMarketParams =
        serde_json::from_str(&contract_params_json)
            .map_err(|e| format!("invalid contract params: {e}"))?;

    let node_state = app.state::<NodeState>();
    let guard = node_state.node.lock().await;
    let node = guard.as_ref().ok_or("Node not initialized")?;
    let state = node
        .market_state(params)
        .await
        .map_err(|e| format!("{e}"))?;

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
        starting_yes_price: info.starting_yes_price.unwrap_or(50),
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
