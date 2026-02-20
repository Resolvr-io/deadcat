use std::sync::Mutex;
use std::time::Duration;

use nostr_sdk::prelude::*;
use tauri::{Emitter, Manager};

use crate::discovery::{
    self, AttestationResult, ContractMetadata, CreateContractRequest,
    DiscoveredMarket, IdentityResponse,
};
use serde::{Deserialize, Serialize};

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
) -> Result<IdentityResponse, String> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("failed to get app data dir: {e}"))?;

    let keys = discovery::load_or_generate_keys(&app_data_dir)?;

    let response = IdentityResponse {
        pubkey_hex: keys.public_key().to_hex(),
        npub: keys.public_key().to_bech32().map_err(|e| format!("bech32 error: {e}"))?,
    };

    {
        let mut nostr_keys = state
            .nostr_keys
            .lock()
            .map_err(|_| "failed to lock nostr_keys".to_string())?;
        *nostr_keys = Some(keys);
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
            npub: keys.public_key().to_bech32().map_err(|e| format!("bech32 error: {e}"))?,
        })),
        None => Ok(None),
    }
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
        nostr_keys
            .clone()
            .ok_or_else(|| "nostr identity not initialized — call init_nostr_identity first".to_string())?
    };

    let oracle_pubkey_bytes: [u8; 32] = {
        let hex_str = keys.public_key().to_hex();
        let bytes = hex::decode(&hex_str).map_err(|e| format!("hex decode error: {e}"))?;
        bytes.try_into().map_err(|_| "pubkey must be 32 bytes".to_string())?
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

    let nevent = nostr_sdk::nips::nip19::Nip19Event::new(event_id, discovery::DEFAULT_RELAYS.iter().map(|r| r.to_string()))
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
        .kind(discovery::CONTRACT_EVENT_KIND)
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
        nostr_keys
            .clone()
            .ok_or_else(|| "nostr identity not initialized — call init_nostr_identity first".to_string())?
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
        let wallet = mgr.wallet_mut().ok_or_else(|| "wallet not initialized".to_string())?;

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

    let nevent = nostr_sdk::nips::nip19::Nip19Event::new(event_id, discovery::DEFAULT_RELAYS.iter().map(|r| r.to_string()))
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
