use std::sync::Mutex;
use std::time::Duration;

use deadcat_store::{ContractMetadataInput, MarketFilter};
use nostr_sdk::prelude::*;
use tauri::{Emitter, Manager};

use crate::discovery::{
    self, AttestationResult, ContractMetadata, CreateContractRequest, DiscoveredMarket,
    IdentityResponse, discovered_market_to_contract_params,
};
use serde::{Deserialize, Serialize};

use deadcat_sdk::ChainBackend;

use crate::state::AppStateManager;
use crate::SdkState;

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

async fn get_or_connect_nostr_client(sdk_state: &SdkState) -> Result<nostr_sdk::Client, String> {
    let mut nostr_client = sdk_state.nostr_client.lock().await;
    if nostr_client.is_none() {
        let c = discovery::connect_client(None).await?;
        *nostr_client = Some(c);
    }
    Ok(nostr_client.as_ref().unwrap().clone())
}

#[tauri::command]
pub async fn init_nostr_identity(
    state: tauri::State<'_, SdkState>,
    app_handle: tauri::AppHandle,
) -> Result<Option<IdentityResponse>, String> {
    let app_data_dir = app_handle
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

            {
                let mut nostr_keys = state
                    .nostr_keys
                    .lock()
                    .map_err(|_| "failed to lock nostr_keys".to_string())?;
                *nostr_keys = Some(keys);
            }

            Ok(Some(response))
        }
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn generate_nostr_identity(
    state: tauri::State<'_, SdkState>,
    app_handle: tauri::AppHandle,
) -> Result<IdentityResponse, String> {
    let app_data_dir = app_handle
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

    {
        let mut nostr_keys = state
            .nostr_keys
            .lock()
            .map_err(|_| "failed to lock nostr_keys".to_string())?;
        *nostr_keys = Some(keys);
    }

    // Reset Nostr client so it reconnects with new identity
    {
        let mut nostr_client = state.nostr_client.lock().await;
        *nostr_client = None;
    }

    Ok(response)
}

#[tauri::command]
pub fn get_nostr_identity(
    state: tauri::State<'_, SdkState>,
) -> Result<Option<IdentityResponse>, String> {
    let nostr_keys = state
        .nostr_keys
        .lock()
        .map_err(|_| "failed to lock nostr_keys".to_string())?;

    match nostr_keys.as_ref() {
        Some(keys) => Ok(Some(IdentityResponse {
            pubkey_hex: keys.public_key().to_hex(),
            npub: keys
                .public_key()
                .to_bech32()
                .map_err(|e| format!("bech32 error: {e}"))?,
        })),
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn import_nostr_nsec(
    nsec: String,
    state: tauri::State<'_, SdkState>,
    app_handle: tauri::AppHandle,
) -> Result<IdentityResponse, String> {
    let secret_key = SecretKey::from_bech32(nsec.trim())
        .map_err(|e| format!("invalid nsec: {e}"))?;
    let keys = Keys::new(secret_key);

    // Persist to disk
    let app_data_dir = app_handle
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

    // Update in-memory keys
    {
        let mut nostr_keys = state
            .nostr_keys
            .lock()
            .map_err(|_| "failed to lock nostr_keys".to_string())?;
        *nostr_keys = Some(keys);
    }

    // Reset Nostr client so it reconnects with new identity
    {
        let mut nostr_client = state.nostr_client.lock().await;
        *nostr_client = None;
    }

    Ok(response)
}

#[tauri::command]
pub fn export_nostr_nsec(
    state: tauri::State<'_, SdkState>,
) -> Result<String, String> {
    let nostr_keys = state
        .nostr_keys
        .lock()
        .map_err(|_| "failed to lock nostr_keys".to_string())?;

    let keys = nostr_keys
        .as_ref()
        .ok_or_else(|| "Nostr identity not initialized".to_string())?;

    keys.secret_key()
        .to_bech32()
        .map_err(|e| format!("bech32 error: {e}"))
}

#[tauri::command]
pub async fn delete_nostr_identity(
    state: tauri::State<'_, SdkState>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("failed to get app data dir: {e}"))?;
    let key_path = app_data_dir.join("nostr_identity.key");
    if key_path.exists() {
        std::fs::remove_file(&key_path)
            .map_err(|e| format!("failed to delete key file: {e}"))?;
    }

    // Clear in-memory keys
    {
        let mut nostr_keys = state
            .nostr_keys
            .lock()
            .map_err(|_| "failed to lock nostr_keys".to_string())?;
        *nostr_keys = None;
    }

    // Reset Nostr client
    {
        let mut nostr_client = state.nostr_client.lock().await;
        *nostr_client = None;
    }

    Ok(())
}

#[tauri::command]
pub async fn discover_contracts(
    state: tauri::State<'_, SdkState>,
) -> Result<Vec<DiscoveredMarket>, String> {
    let client = get_or_connect_nostr_client(&state).await?;
    discovery::fetch_announcements(&client).await
}

/// Publish a contract to Nostr (Nostr-only mode — no on-chain tx).
#[tauri::command]
pub async fn publish_contract(
    state: tauri::State<'_, SdkState>,
    request: CreateContractRequest,
    app: tauri::AppHandle,
) -> Result<DiscoveredMarket, String> {
    validate_request(&request)?;

    let keys = {
        let nostr_keys = state
            .nostr_keys
            .lock()
            .map_err(|_| "failed to lock nostr_keys".to_string())?;
        nostr_keys.clone().ok_or_else(|| {
            "nostr identity not initialized — call init_nostr_identity first".to_string()
        })?
    };

    let oracle_pubkey_bytes: [u8; 32] = {
        let hex_str = keys.public_key().to_hex();
        let bytes = hex::decode(&hex_str).map_err(|e| format!("hex decode error: {e}"))?;
        bytes
            .try_into()
            .map_err(|_| "pubkey must be 32 bytes".to_string())?
    };

    let wallet_network: crate::WalletNetwork = {
        let state_handle = app.state::<Mutex<AppStateManager>>();
        let mgr = state_handle
            .lock()
            .map_err(|_| "wallet lock failed".to_string())?;
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

    let contract_params = deadcat_sdk::params::ContractParams {
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
        question: request.question,
        description: request.description,
        category: request.category,
        resolution_source: request.resolution_source,
        starting_yes_price: request.starting_yes_price,
    };

    let announcement = deadcat_sdk::announcement::ContractAnnouncement {
        version: 1,
        contract_params,
        metadata,
        creation_txid: None,
    };

    let event = discovery::build_announcement_event(&keys, &announcement)?;

    let client = get_or_connect_nostr_client(&state).await?;
    let event_id = discovery::publish_event(&client, event.clone()).await?;

    let market = discovery::parse_announcement_event(&event)?;

    let nevent = nostr_sdk::nips::nip19::Nip19Event::new(
        event_id,
        discovery::DEFAULT_RELAYS.iter().map(|r| r.to_string()),
    )
    .to_bech32()
    .unwrap_or_default();

    Ok(DiscoveredMarket {
        id: event_id.to_hex(),
        nevent,
        ..market
    })
}

#[tauri::command]
pub async fn oracle_attest(
    state: tauri::State<'_, SdkState>,
    market_id_hex: String,
    outcome_yes: bool,
) -> Result<AttestationResult, String> {
    let keys = {
        let nostr_keys = state
            .nostr_keys
            .lock()
            .map_err(|_| "failed to lock nostr_keys".to_string())?;
        nostr_keys
            .clone()
            .ok_or_else(|| "nostr identity not initialized".to_string())?
    };

    let market_id_bytes: [u8; 32] = hex::decode(&market_id_hex)
        .map_err(|e| format!("invalid market_id hex: {e}"))?
        .try_into()
        .map_err(|_| "market_id must be exactly 32 bytes".to_string())?;

    let market_id = deadcat_sdk::params::MarketId(market_id_bytes);

    let (sig_bytes, msg_bytes) = discovery::sign_attestation(&keys, &market_id, outcome_yes)?;

    let client = get_or_connect_nostr_client(&state).await?;

    let filter = nostr_sdk::Filter::new()
        .kind(discovery::APP_EVENT_KIND)
        .identifier(&market_id_hex)
        .hashtag(discovery::CONTRACT_TAG);

    let events = client
        .fetch_events(vec![filter], Duration::from_secs(15))
        .await
        .map_err(|e| format!("failed to fetch announcement: {e}"))?;

    let announcement_event_id = events
        .iter()
        .next()
        .map(|e| e.id.to_hex())
        .unwrap_or_default();

    let sig_hex = hex::encode(sig_bytes);
    let msg_hex = hex::encode(msg_bytes);

    let event = discovery::build_attestation_event(
        &keys,
        &market_id_hex,
        &announcement_event_id,
        outcome_yes,
        &sig_hex,
        &msg_hex,
    )?;

    let event_id = discovery::publish_event(&client, event).await?;

    Ok(AttestationResult {
        market_id: market_id_hex,
        outcome_yes,
        signature_hex: sig_hex,
        nostr_event_id: event_id.to_hex(),
    })
}

// ---------------------------------------------------------------------------
// On-chain contract creation command
// ---------------------------------------------------------------------------

/// Create a prediction market contract on-chain (Liquid creation tx + Nostr announcement).
#[tauri::command]
pub async fn create_contract_onchain(
    sdk_state: tauri::State<'_, SdkState>,
    request: CreateContractRequest,
    app: tauri::AppHandle,
) -> Result<DiscoveredMarket, String> {
    validate_request(&request)?;

    let keys = {
        let nostr_keys = sdk_state
            .nostr_keys
            .lock()
            .map_err(|_| "failed to lock nostr_keys".to_string())?;
        nostr_keys.clone().ok_or_else(|| {
            "nostr identity not initialized — call init_nostr_identity first".to_string()
        })?
    };

    let oracle_pubkey_bytes: [u8; 32] = {
        let hex_str = keys.public_key().to_hex();
        let bytes = hex::decode(&hex_str).map_err(|e| format!("hex decode: {e}"))?;
        bytes
            .try_into()
            .map_err(|_| "pubkey must be 32 bytes".to_string())?
    };

    let wallet_network: crate::WalletNetwork = {
        let state_handle = app.state::<Mutex<AppStateManager>>();
        let mgr = state_handle
            .lock()
            .map_err(|_| "wallet lock failed".to_string())?;
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

    let app_handle = app.clone();
    let collateral_per_token = request.collateral_per_token;

    let (creation_txid, contract_params) = tokio::task::spawn_blocking(move || {
        let manager = app_handle.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "wallet lock failed".to_string())?;
        let wallet = mgr
            .wallet_mut()
            .ok_or_else(|| "wallet not initialized".to_string())?;

        if wallet.status() != crate::wallet::types::WalletStatus::Unlocked {
            return Err("wallet must be unlocked to create a contract".to_string());
        }

        let sdk = wallet.sdk_mut().map_err(|e| format!("{e}"))?;
        let (txid, params) = sdk
            .create_contract_onchain(
                oracle_pubkey_bytes,
                collateral_per_token,
                expiry_time,
                300,
                300,
            )
            .map_err(|e| format!("contract creation: {e}"))?;

        mgr.bump_revision();
        let state = mgr.snapshot();
        let _ = app_handle.emit(crate::APP_STATE_UPDATED_EVENT, &state);

        Ok((txid.to_string(), params))
    })
    .await
    .map_err(|e| format!("task join: {e}"))??;

    let metadata = ContractMetadata {
        question: request.question,
        description: request.description,
        category: request.category,
        resolution_source: request.resolution_source,
        starting_yes_price: request.starting_yes_price,
    };

    let announcement = deadcat_sdk::announcement::ContractAnnouncement {
        version: 1,
        contract_params,
        metadata,
        creation_txid: Some(creation_txid),
    };

    let event = discovery::build_announcement_event(&keys, &announcement)?;

    let client = get_or_connect_nostr_client(&sdk_state).await?;
    let event_id = discovery::publish_event(&client, event.clone()).await?;
    let market = discovery::parse_announcement_event(&event)?;

    let nevent = nostr_sdk::nips::nip19::Nip19Event::new(
        event_id,
        discovery::DEFAULT_RELAYS.iter().map(|r| r.to_string()),
    )
    .to_bech32()
    .unwrap_or_default();

    Ok(DiscoveredMarket {
        id: event_id.to_hex(),
        nevent,
        ..market
    })
}

// ---------------------------------------------------------------------------
// Token issuance command
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct IssuanceResultResponse {
    pub txid: String,
    pub previous_state: u8,
    pub new_state: u8,
    pub pairs_issued: u64,
}

/// Issue prediction market token pairs for an existing on-chain contract.
///
/// Detects whether the market is in Dormant (initial issuance) or Unresolved
/// (subsequent issuance) state and builds the appropriate transaction.
#[tauri::command]
pub async fn issue_tokens(
    contract_params_json: String,
    creation_txid: String,
    pairs: u64,
    app: tauri::AppHandle,
) -> Result<IssuanceResultResponse, String> {
    let params: deadcat_sdk::params::ContractParams =
        serde_json::from_str(&contract_params_json)
            .map_err(|e| format!("invalid contract params: {e}"))?;

    let app_handle = app.clone();

    let result = tokio::task::spawn_blocking(move || {
        let txid: lwk_wollet::elements::Txid = creation_txid
            .parse()
            .map_err(|e| format!("invalid txid: {e}"))?;

        let manager = app_handle.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "wallet lock failed".to_string())?;
        let wallet = mgr
            .wallet_mut()
            .ok_or_else(|| "wallet not initialized".to_string())?;

        if wallet.status() != crate::wallet::types::WalletStatus::Unlocked {
            return Err("wallet must be unlocked to issue tokens".to_string());
        }

        let sdk = wallet.sdk_mut().map_err(|e| format!("{e}"))?;
        let result = sdk
            .issue_tokens(&params, &txid, pairs, 500)
            .map_err(|e| format!("issuance failed: {e}"))?;

        mgr.bump_revision();
        let state = mgr.snapshot();
        let _ = app_handle.emit(crate::APP_STATE_UPDATED_EVENT, &state);

        Ok(IssuanceResultResponse {
            txid: result.txid.to_string(),
            previous_state: result.previous_state as u8,
            new_state: result.new_state as u8,
            pairs_issued: result.pairs_issued,
        })
    })
    .await
    .map_err(|e| format!("task join: {e}"))??;

    Ok(result)
}

// ---------------------------------------------------------------------------
// Token cancellation command
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct CancellationResultResponse {
    pub txid: String,
    pub previous_state: u8,
    pub new_state: u8,
    pub pairs_burned: u64,
    pub is_full_cancellation: bool,
}

/// Cancel paired YES+NO tokens to reclaim collateral.
///
/// Partial cancellation keeps the market Unresolved; full cancellation
/// transitions back to Dormant.
#[tauri::command]
pub async fn cancel_tokens(
    contract_params_json: String,
    pairs: u64,
    app: tauri::AppHandle,
) -> Result<CancellationResultResponse, String> {
    let params: deadcat_sdk::params::ContractParams =
        serde_json::from_str(&contract_params_json)
            .map_err(|e| format!("invalid contract params: {e}"))?;

    let app_handle = app.clone();

    let result = tokio::task::spawn_blocking(move || {
        let manager = app_handle.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "wallet lock failed".to_string())?;
        let wallet = mgr
            .wallet_mut()
            .ok_or_else(|| "wallet not initialized".to_string())?;

        if wallet.status() != crate::wallet::types::WalletStatus::Unlocked {
            return Err("wallet must be unlocked to cancel tokens".to_string());
        }

        let sdk = wallet.sdk_mut().map_err(|e| format!("{e}"))?;
        let result = sdk
            .cancel_tokens(&params, pairs, 500)
            .map_err(|e| format!("cancellation failed: {e}"))?;

        mgr.bump_revision();
        let state = mgr.snapshot();
        let _ = app_handle.emit(crate::APP_STATE_UPDATED_EVENT, &state);

        Ok(CancellationResultResponse {
            txid: result.txid.to_string(),
            previous_state: result.previous_state as u8,
            new_state: result.new_state as u8,
            pairs_burned: result.pairs_burned,
            is_full_cancellation: result.is_full_cancellation,
        })
    })
    .await
    .map_err(|e| format!("task join: {e}"))??;

    Ok(result)
}

// ---------------------------------------------------------------------------
// Oracle resolution command
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct ResolutionResultResponse {
    pub txid: String,
    pub previous_state: u8,
    pub new_state: u8,
    pub outcome_yes: bool,
}

/// Execute on-chain oracle resolution (covenant state Unresolved → ResolvedYes/ResolvedNo).
#[tauri::command]
pub async fn resolve_market(
    contract_params_json: String,
    outcome_yes: bool,
    oracle_signature_hex: String,
    app: tauri::AppHandle,
) -> Result<ResolutionResultResponse, String> {
    let params: deadcat_sdk::params::ContractParams =
        serde_json::from_str(&contract_params_json)
            .map_err(|e| format!("invalid contract params: {e}"))?;

    let sig_bytes: [u8; 64] = hex::decode(&oracle_signature_hex)
        .map_err(|e| format!("invalid signature hex: {e}"))?
        .try_into()
        .map_err(|_| "oracle signature must be exactly 64 bytes".to_string())?;

    let app_handle = app.clone();

    let result = tokio::task::spawn_blocking(move || {
        let manager = app_handle.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "wallet lock failed".to_string())?;
        let wallet = mgr
            .wallet_mut()
            .ok_or_else(|| "wallet not initialized".to_string())?;

        if wallet.status() != crate::wallet::types::WalletStatus::Unlocked {
            return Err("wallet must be unlocked to resolve a market".to_string());
        }

        let sdk = wallet.sdk_mut().map_err(|e| format!("{e}"))?;
        let result = sdk
            .resolve_market(&params, outcome_yes, sig_bytes, 500)
            .map_err(|e| format!("resolution failed: {e}"))?;

        mgr.bump_revision();
        let state = mgr.snapshot();
        let _ = app_handle.emit(crate::APP_STATE_UPDATED_EVENT, &state);

        Ok(ResolutionResultResponse {
            txid: result.txid.to_string(),
            previous_state: result.previous_state as u8,
            new_state: result.new_state as u8,
            outcome_yes: result.outcome_yes,
        })
    })
    .await
    .map_err(|e| format!("task join: {e}"))??;

    Ok(result)
}

// ---------------------------------------------------------------------------
// Post-resolution redemption command
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct RedemptionResultResponse {
    pub txid: String,
    pub previous_state: u8,
    pub tokens_redeemed: u64,
    pub payout_sats: u64,
}

/// Redeem winning tokens after oracle resolution (burn tokens → L-BTC payout).
#[tauri::command]
pub async fn redeem_tokens(
    contract_params_json: String,
    tokens: u64,
    app: tauri::AppHandle,
) -> Result<RedemptionResultResponse, String> {
    let params: deadcat_sdk::params::ContractParams =
        serde_json::from_str(&contract_params_json)
            .map_err(|e| format!("invalid contract params: {e}"))?;

    let app_handle = app.clone();

    let result = tokio::task::spawn_blocking(move || {
        let manager = app_handle.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "wallet lock failed".to_string())?;
        let wallet = mgr
            .wallet_mut()
            .ok_or_else(|| "wallet not initialized".to_string())?;

        if wallet.status() != crate::wallet::types::WalletStatus::Unlocked {
            return Err("wallet must be unlocked to redeem tokens".to_string());
        }

        let sdk = wallet.sdk_mut().map_err(|e| format!("{e}"))?;
        let result = sdk
            .redeem_tokens(&params, tokens, 500)
            .map_err(|e| format!("redemption failed: {e}"))?;

        mgr.bump_revision();
        let state = mgr.snapshot();
        let _ = app_handle.emit(crate::APP_STATE_UPDATED_EVENT, &state);

        Ok(RedemptionResultResponse {
            txid: result.txid.to_string(),
            previous_state: result.previous_state as u8,
            tokens_redeemed: result.tokens_redeemed,
            payout_sats: result.payout_sats,
        })
    })
    .await
    .map_err(|e| format!("task join: {e}"))??;

    Ok(result)
}

// ---------------------------------------------------------------------------
// Expiry redemption command
// ---------------------------------------------------------------------------

/// Redeem tokens after market expiry (no oracle resolution, 1x CPT payout).
#[tauri::command]
pub async fn redeem_expired(
    contract_params_json: String,
    token_asset_hex: String,
    tokens: u64,
    app: tauri::AppHandle,
) -> Result<RedemptionResultResponse, String> {
    let params: deadcat_sdk::params::ContractParams =
        serde_json::from_str(&contract_params_json)
            .map_err(|e| format!("invalid contract params: {e}"))?;

    let token_asset: [u8; 32] = hex::decode(&token_asset_hex)
        .map_err(|e| format!("invalid token asset hex: {e}"))?
        .try_into()
        .map_err(|_| "token asset must be exactly 32 bytes".to_string())?;

    let app_handle = app.clone();

    let result = tokio::task::spawn_blocking(move || {
        let manager = app_handle.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "wallet lock failed".to_string())?;
        let wallet = mgr
            .wallet_mut()
            .ok_or_else(|| "wallet not initialized".to_string())?;

        if wallet.status() != crate::wallet::types::WalletStatus::Unlocked {
            return Err("wallet must be unlocked to redeem expired tokens".to_string());
        }

        let sdk = wallet.sdk_mut().map_err(|e| format!("{e}"))?;
        let result = sdk
            .redeem_expired(&params, token_asset, tokens, 500)
            .map_err(|e| format!("expiry redemption failed: {e}"))?;

        mgr.bump_revision();
        let state = mgr.snapshot();
        let _ = app_handle.emit(crate::APP_STATE_UPDATED_EVENT, &state);

        Ok(RedemptionResultResponse {
            txid: result.txid.to_string(),
            previous_state: result.previous_state as u8,
            tokens_redeemed: result.tokens_redeemed,
            payout_sats: result.payout_sats,
        })
    })
    .await
    .map_err(|e| format!("task join: {e}"))??;

    Ok(result)
}

// ---------------------------------------------------------------------------
// Market state query command
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct MarketStateResponse {
    pub state: u8,
}

/// Query the live on-chain covenant state for a market.
#[tauri::command]
pub async fn get_market_state(
    contract_params_json: String,
    app: tauri::AppHandle,
) -> Result<MarketStateResponse, String> {
    let params: deadcat_sdk::params::ContractParams =
        serde_json::from_str(&contract_params_json)
            .map_err(|e| format!("invalid contract params: {e}"))?;

    let app_handle = app.clone();

    let result = tokio::task::spawn_blocking(move || {
        let manager = app_handle.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "wallet lock failed".to_string())?;
        let wallet = mgr
            .wallet_mut()
            .ok_or_else(|| "wallet not initialized".to_string())?;

        if wallet.status() != crate::wallet::types::WalletStatus::Unlocked {
            return Err("wallet must be unlocked to query market state".to_string());
        }

        let sdk = wallet.sdk_mut().map_err(|e| format!("{e}"))?;
        sdk.sync().map_err(|e| format!("sync failed: {e}"))?;

        let contract = deadcat_sdk::contract::CompiledContract::new(params)
            .map_err(|e| format!("contract compilation failed: {e}"))?;

        // Use the chain backend to scan covenant addresses
        let chain = sdk.chain();
        let dormant_spk = contract.script_pubkey(deadcat_sdk::state::MarketState::Dormant);
        let unresolved_spk = contract.script_pubkey(deadcat_sdk::state::MarketState::Unresolved);
        let resolved_yes_spk =
            contract.script_pubkey(deadcat_sdk::state::MarketState::ResolvedYes);
        let resolved_no_spk = contract.script_pubkey(deadcat_sdk::state::MarketState::ResolvedNo);

        let dormant = chain
            .scan_script_utxos(&dormant_spk)
            .map_err(|e| format!("{e}"))?;
        let unresolved = chain
            .scan_script_utxos(&unresolved_spk)
            .map_err(|e| format!("{e}"))?;
        let resolved_yes = chain
            .scan_script_utxos(&resolved_yes_spk)
            .map_err(|e| format!("{e}"))?;
        let resolved_no = chain
            .scan_script_utxos(&resolved_no_spk)
            .map_err(|e| format!("{e}"))?;

        let state = if !dormant.is_empty() {
            0u8
        } else if !unresolved.is_empty() {
            1
        } else if !resolved_yes.is_empty() {
            2
        } else if !resolved_no.is_empty() {
            3
        } else {
            return Err("no UTXOs found at any covenant address".to_string());
        };

        Ok(MarketStateResponse { state })
    })
    .await
    .map_err(|e| format!("task join: {e}"))??;

    Ok(result)
}

// ---------------------------------------------------------------------------
// Wallet UTXO query command
// ---------------------------------------------------------------------------

/// Expose the wallet's raw UTXO list (needed for position tracking of YES/NO tokens).
#[tauri::command]
pub fn get_wallet_utxos(
    app: tauri::AppHandle,
) -> Result<Vec<crate::wallet::types::WalletUtxo>, String> {
    let state_handle = app.state::<Mutex<AppStateManager>>();
    let mgr = state_handle
        .lock()
        .map_err(|_| "state lock failed".to_string())?;
    let wallet = mgr.wallet().ok_or_else(|| "wallet not initialized".to_string())?;
    wallet.utxos().map_err(|e| format!("{e}"))
}

// ---------------------------------------------------------------------------
// Market store commands
// ---------------------------------------------------------------------------

/// Ingest discovered markets into the persistent store.
///
/// For each market, compiles the contract — markets that fail compilation are
/// silently dropped. Returns the number of markets successfully ingested.
#[tauri::command]
pub fn ingest_discovered_markets(
    markets: Vec<DiscoveredMarket>,
    app: tauri::AppHandle,
) -> Result<u32, String> {
    let state_handle = app.state::<Mutex<AppStateManager>>();
    let mut mgr = state_handle
        .lock()
        .map_err(|_| "state lock failed".to_string())?;
    let wallet = mgr
        .wallet_mut()
        .ok_or_else(|| "wallet not initialized".to_string())?;

    let mut count = 0u32;
    for market in &markets {
        let params = match discovered_market_to_contract_params(market) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("skipping market {}: {e}", market.market_id);
                continue;
            }
        };

        let metadata = ContractMetadataInput {
            question: Some(market.question.clone()),
            description: Some(market.description.clone()),
            category: Some(market.category.clone()),
            resolution_source: Some(market.resolution_source.clone()),
            starting_yes_price: Some(market.starting_yes_price),
            creator_pubkey: hex::decode(&market.creator_pubkey).ok(),
            creation_txid: market.creation_txid.clone(),
            nevent: Some(market.nevent.clone()),
            nostr_event_id: Some(market.id.clone()),
            nostr_event_json: market.nostr_event_json.clone(),
        };

        match wallet.ingest_market(&params, Some(&metadata)) {
            Ok(_) => count += 1,
            Err(e) => {
                log::warn!("failed to ingest market {}: {e}", market.market_id);
            }
        }
    }

    Ok(count)
}

/// List all markets from the persistent store.
///
/// Returns markets as `DiscoveredMarket` (the frontend type), with `state`
/// reflecting real on-chain state from the store's sync.
#[tauri::command]
pub fn list_contracts(app: tauri::AppHandle) -> Result<Vec<DiscoveredMarket>, String> {
    let state_handle = app.state::<Mutex<AppStateManager>>();
    let mut mgr = state_handle
        .lock()
        .map_err(|_| "state lock failed".to_string())?;
    let wallet = mgr
        .wallet_mut()
        .ok_or_else(|| "wallet not initialized".to_string())?;

    let infos = wallet
        .list_markets(&MarketFilter::default())
        .map_err(|e| format!("list markets: {e}"))?;

    let mut result = Vec::with_capacity(infos.len());
    for info in &infos {
        result.push(market_info_to_discovered(info));
    }
    Ok(result)
}

/// Convert a `MarketInfo` (store type) back to `DiscoveredMarket` (frontend type).
fn market_info_to_discovered(info: &deadcat_store::MarketInfo) -> DiscoveredMarket {
    let p = &info.params;
    let market_id_hex = hex::encode(info.market_id.as_bytes());
    DiscoveredMarket {
        // Use market_id as stable unique identifier (the store doesn't persist Nostr event IDs)
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
    }
}

/// Parse an ISO datetime string (e.g. "2026-02-21 12:34:56") into a unix timestamp.
fn parse_iso_datetime_to_unix(s: &str) -> u64 {
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .map(|dt| dt.and_utc().timestamp() as u64)
        .unwrap_or(0)
}
