mod chain_adapter;
pub mod commands;
pub mod discovery;
mod payments;
mod state;
pub mod wallet;
mod wallet_store;

use std::sync::{Mutex, RwLock};

use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager};

use state::{AppState, AppStateManager, PaymentSwap};

const APP_STATE_UPDATED_EVENT: &str = "app_state_updated";

// SDK / Nostr state (managed alongside AppStateManager)
pub struct SdkState {
    pub wallet_store: wallet_store::WalletStore,
    pub nostr_keys: Mutex<Option<nostr_sdk::Keys>>,
    pub nostr_client: tokio::sync::Mutex<Option<nostr_sdk::Client>>,
    pub relay_list: RwLock<Vec<String>>,
}

impl Default for SdkState {
    fn default() -> Self {
        Self {
            wallet_store: wallet_store::WalletStore::default(),
            nostr_keys: Mutex::new(None),
            nostr_client: tokio::sync::Mutex::new(None),
            relay_list: RwLock::new(
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
        let mgr = manager.lock().map_err(|_| "state lock failed".to_string())?;
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
        let mut mgr = manager.lock().map_err(|_| "state lock failed".to_string())?;
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
        let mgr = manager.lock().map_err(|_| "state lock failed".to_string())?;
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
        let mgr = manager.lock().map_err(|_| "wallet lock failed".to_string())?;
        Ok(mgr
            .wallet()
            .map(|w| w.status())
            .unwrap_or(wallet::types::WalletStatus::NotCreated))
    })
    .await
    .map_err(|e| format!("wallet_status task failed: {e}"))?
}

#[tauri::command]
async fn create_wallet(password: String, app: AppHandle) -> Result<String, String> {
    let app_handle = app.clone();
    tokio::task::spawn_blocking(move || {
        let manager = app_handle.state::<Mutex<AppStateManager>>();
        let mut mgr = manager.lock().map_err(|_| "wallet lock failed".to_string())?;
        let wallet = mgr.wallet_mut().ok_or("Wallet not initialized")?;
        let mnemonic = wallet.create_wallet(&password).map_err(|e| e.to_string())?;
        mgr.bump_revision();
        let state = mgr.snapshot();
        emit_state(&app_handle, &state);
        Ok(mnemonic)
    })
    .await
    .map_err(|e| format!("create_wallet task failed: {e}"))?
}

#[tauri::command]
async fn restore_wallet(mnemonic: String, password: String, app: AppHandle) -> Result<AppState, String> {
    let app_handle = app.clone();
    tokio::task::spawn_blocking(move || {
        let manager = app_handle.state::<Mutex<AppStateManager>>();
        let mut mgr = manager.lock().map_err(|_| "wallet lock failed".to_string())?;
        let wallet = mgr.wallet_mut().ok_or("Wallet not initialized")?;
        wallet.restore_wallet(&mnemonic, &password).map_err(|e| e.to_string())?;
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
    tokio::task::spawn_blocking(move || {
        let manager = app_handle.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "wallet lock failed".to_string())?;
        let wallet = mgr
            .wallet_mut()
            .ok_or_else(|| "Wallet not initialized".to_string())?;
        wallet.unlock(&password).map_err(|e| e.to_string())?;
        mgr.bump_revision();
        let state = mgr.snapshot();
        let _ = app_handle.emit(APP_STATE_UPDATED_EVENT, &state);
        Ok(state)
    })
    .await
    .map_err(|e| format!("unlock task failed: {e}"))?
}

#[tauri::command]
async fn lock_wallet(app: AppHandle) -> Result<AppState, String> {
    let app_handle = app.clone();
    tokio::task::spawn_blocking(move || {
        let manager = app_handle.state::<Mutex<AppStateManager>>();
        let mut mgr = manager.lock().map_err(|_| "wallet lock failed".to_string())?;
        if let Some(wallet) = mgr.wallet_mut() {
            wallet.lock();
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
    let app_handle = app.clone();
    tokio::task::spawn_blocking(move || {
        let manager = app_handle.state::<Mutex<AppStateManager>>();
        let mut mgr = manager.lock().map_err(|_| "wallet lock failed".to_string())?;
        let wallet = mgr.wallet_mut().ok_or("Wallet not initialized")?;
        wallet.delete_wallet().map_err(|e| e.to_string())?;
        mgr.bump_revision();
        let state = mgr.snapshot();
        emit_state(&app_handle, &state);
        Ok(state)
    })
    .await
    .map_err(|e| format!("delete_wallet task failed: {e}"))?
}

#[tauri::command]
async fn sync_wallet(app: AppHandle) -> Result<AppState, String> {
    let app_handle = app.clone();
    tokio::task::spawn_blocking(move || {
        let manager = app_handle.state::<Mutex<AppStateManager>>();
        let mut mgr = manager
            .lock()
            .map_err(|_| "wallet lock failed".to_string())?;
        let wallet = mgr
            .wallet_mut()
            .ok_or_else(|| "Wallet not initialized".to_string())?;
        wallet.sync().map_err(|e| e.to_string())?;
        mgr.bump_revision();
        let state = mgr.snapshot();
        let _ = app_handle.emit(APP_STATE_UPDATED_EVENT, &state);
        Ok(state)
    })
    .await
    .map_err(|e| format!("sync task failed: {e}"))?
}

#[tauri::command]
async fn get_wallet_balance(app: AppHandle) -> Result<wallet::types::WalletBalance, String> {
    tokio::task::spawn_blocking(move || {
        let manager = app.state::<Mutex<AppStateManager>>();
        let mgr = manager.lock().map_err(|_| "wallet lock failed".to_string())?;
        let wallet = mgr.wallet().ok_or("Wallet not initialized")?;
        wallet.balance().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("balance task failed: {e}"))?
}

#[tauri::command]
async fn get_wallet_address(index: Option<u32>, app: AppHandle) -> Result<wallet::types::WalletAddress, String> {
    tokio::task::spawn_blocking(move || {
        let manager = app.state::<Mutex<AppStateManager>>();
        let mgr = manager.lock().map_err(|_| "wallet lock failed".to_string())?;
        let wallet = mgr.wallet().ok_or("Wallet not initialized")?;
        wallet.address(index).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("address task failed: {e}"))?
}

#[tauri::command]
async fn get_wallet_transactions(app: AppHandle) -> Result<Vec<wallet::types::WalletTransaction>, String> {
    tokio::task::spawn_blocking(move || {
        let manager = app.state::<Mutex<AppStateManager>>();
        let mgr = manager.lock().map_err(|_| "wallet lock failed".to_string())?;
        let wallet = mgr.wallet().ok_or("Wallet not initialized")?;
        wallet.transactions().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("transactions task failed: {e}"))?
}

#[tauri::command]
async fn send_lbtc(
    address: String,
    amount_sat: u64,
    fee_rate: Option<f32>,
    app: AppHandle,
) -> Result<wallet::types::LiquidSendResult, String> {
    let app_handle = app.clone();
    tokio::task::spawn_blocking(move || {
        let manager = app_handle.state::<Mutex<AppStateManager>>();
        let mut mgr = manager.lock().map_err(|_| "wallet lock failed".to_string())?;
        let wallet = mgr.wallet_mut().ok_or("Wallet not initialized")?;
        let result = wallet
            .send_lbtc(&address, amount_sat, fee_rate)
            .map_err(|e| e.to_string())?;
        mgr.bump_revision();
        let state = mgr.snapshot();
        emit_state(&app_handle, &state);
        Ok(result)
    })
    .await
    .map_err(|e| format!("send_lbtc task failed: {e}"))?
}

#[tauri::command]
async fn get_wallet_mnemonic(password: String, app: AppHandle) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let manager = app.state::<Mutex<AppStateManager>>();
        let mut mgr = manager.lock().map_err(|_| "wallet lock failed".to_string())?;
        let wallet = mgr.wallet_mut().ok_or("Wallet not initialized")?;
        let mnemonic = wallet
            .persister_mut()
            .load(&password)
            .map_err(|e| e.to_string())?;
        Ok(mnemonic)
    })
    .await
    .map_err(|e| format!("mnemonic task failed: {e}"))?
}

// ============================================================================
// Payment Commands (Boltz)
// ============================================================================

#[tauri::command]
async fn pay_lightning_invoice(
    invoice: String,
    app: AppHandle,
) -> Result<payments::boltz::BoltzSubmarineSwapCreated, String> {
    let app_ref = app.clone();
    let (network, refund_pubkey_hex) = tokio::task::spawn_blocking(move || {
        let manager = app_ref.state::<Mutex<AppStateManager>>();
        let mgr = manager.lock().map_err(|_| "wallet lock failed".to_string())?;
        let network = mgr
            .network()
            .ok_or("Not initialized - select a network first")?;
        let wallet = mgr.wallet().ok_or("Wallet not initialized")?;
        let refund_pubkey_hex = wallet
            .boltz_submarine_refund_pubkey_hex()
            .map_err(|e| format!("Wallet must be unlocked to initiate swap: {}", e))?;
        Ok::<_, String>((network, refund_pubkey_hex))
    })
    .await
    .map_err(|e| format!("pay_lightning task failed: {e}"))??;

    let boltz = payments::boltz::BoltzService::new(network, None);
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
        let mut mgr = manager.lock().map_err(|_| "wallet lock failed".to_string())?;
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
    let app_ref = app.clone();
    let (network, claim_pubkey_hex) = tokio::task::spawn_blocking(move || {
        let manager = app_ref.state::<Mutex<AppStateManager>>();
        let mgr = manager.lock().map_err(|_| "wallet lock failed".to_string())?;
        let network = mgr
            .network()
            .ok_or("Not initialized - select a network first")?;
        let wallet = mgr.wallet().ok_or("Wallet not initialized")?;
        let claim_pubkey_hex = wallet
            .boltz_reverse_claim_pubkey_hex()
            .map_err(|e| format!("Wallet must be unlocked to initiate swap: {}", e))?;
        Ok::<_, String>((network, claim_pubkey_hex))
    })
    .await
    .map_err(|e| format!("lightning_receive task failed: {e}"))??;

    let boltz = payments::boltz::BoltzService::new(network, None);
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
        let mut mgr = manager.lock().map_err(|_| "wallet lock failed".to_string())?;
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
    let app_ref = app.clone();
    let (network, claim_pubkey_hex, refund_pubkey_hex) = tokio::task::spawn_blocking(move || {
        let manager = app_ref.state::<Mutex<AppStateManager>>();
        let mgr = manager.lock().map_err(|_| "wallet lock failed".to_string())?;
        let network = mgr
            .network()
            .ok_or("Not initialized - select a network first")?;
        let wallet = mgr.wallet().ok_or("Wallet not initialized")?;
        let claim_pubkey_hex = wallet
            .boltz_reverse_claim_pubkey_hex()
            .map_err(|e| format!("Wallet must be unlocked to initiate swap: {}", e))?;
        let refund_pubkey_hex = wallet
            .boltz_submarine_refund_pubkey_hex()
            .map_err(|e| format!("Wallet must be unlocked to initiate swap: {}", e))?;
        Ok::<_, String>((network, claim_pubkey_hex, refund_pubkey_hex))
    })
    .await
    .map_err(|e| format!("bitcoin_receive task failed: {e}"))??;

    let boltz = payments::boltz::BoltzService::new(network, None);
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
        let mut mgr = manager.lock().map_err(|_| "wallet lock failed".to_string())?;
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
    let app_ref = app.clone();
    let (network, claim_pubkey_hex, refund_pubkey_hex) = tokio::task::spawn_blocking(move || {
        let manager = app_ref.state::<Mutex<AppStateManager>>();
        let mgr = manager.lock().map_err(|_| "wallet lock failed".to_string())?;
        let network = mgr
            .network()
            .ok_or("Not initialized - select a network first")?;
        let wallet = mgr.wallet().ok_or("Wallet not initialized")?;
        let claim_pubkey_hex = wallet
            .boltz_reverse_claim_pubkey_hex()
            .map_err(|e| format!("Wallet must be unlocked to initiate swap: {}", e))?;
        let refund_pubkey_hex = wallet
            .boltz_submarine_refund_pubkey_hex()
            .map_err(|e| format!("Wallet must be unlocked to initiate swap: {}", e))?;
        Ok::<_, String>((network, claim_pubkey_hex, refund_pubkey_hex))
    })
    .await
    .map_err(|e| format!("bitcoin_send task failed: {e}"))??;

    let boltz = payments::boltz::BoltzService::new(network, None);
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
        let mut mgr = manager.lock().map_err(|_| "wallet lock failed".to_string())?;
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
async fn get_chain_swap_pairs(app: AppHandle) -> Result<payments::boltz::BoltzChainSwapPairsInfo, String> {
    let app_ref = app.clone();
    let network = tokio::task::spawn_blocking(move || {
        let manager = app_ref.state::<Mutex<AppStateManager>>();
        let mgr = manager.lock().map_err(|_| "state lock failed".to_string())?;
        mgr.network()
            .ok_or("Not initialized - select a network first".to_string())
    })
    .await
    .map_err(|e| format!("chain_swap_pairs task failed: {e}"))??;

    let boltz = payments::boltz::BoltzService::new(network, None);
    boltz
        .get_chain_swap_pairs_info()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_payment_swaps(app: AppHandle) -> Result<Vec<PaymentSwap>, String> {
    tokio::task::spawn_blocking(move || {
        let manager = app.state::<Mutex<AppStateManager>>();
        let mgr = manager.lock().map_err(|_| "state lock failed".to_string())?;
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
    let app_ref = app.clone();
    let swap_id_clone = swap_id.clone();
    let network = tokio::task::spawn_blocking(move || {
        let manager = app_ref.state::<Mutex<AppStateManager>>();
        let mgr = manager.lock().map_err(|_| "state lock failed".to_string())?;
        mgr.network()
            .ok_or("Not initialized - select a network first".to_string())
    })
    .await
    .map_err(|e| format!("refresh_swap task failed: {e}"))??;

    let boltz = payments::boltz::BoltzService::new(network, None);
    let status = boltz
        .get_swap_status(&swap_id_clone)
        .await
        .map_err(|e| e.to_string())?;

    let app_ref = app.clone();
    let updated_swap = tokio::task::spawn_blocking(move || {
        let manager = app_ref.state::<Mutex<AppStateManager>>();
        let mut mgr = manager.lock().map_err(|_| "wallet lock failed".to_string())?;
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
    // electrum-client pulls in rustls 0.23 which requires an explicit provider.
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
            app.manage(SdkState::default());
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
            send_lbtc,
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
            commands::get_wallet_utxos,
            commands::ingest_discovered_markets,
            commands::list_contracts,
            // Wallet store (SDK)
            wallet_store::create_software_signer,
            wallet_store::create_wollet,
            wallet_store::wallet_new_address,
            wallet_store::wallet_signer_id,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
