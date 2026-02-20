mod payments;
mod state;
pub mod wallet;
pub mod commands;
pub mod discovery;
mod wallet_store;

use std::sync::Mutex;

use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager, State};

use state::{AppState, AppStateManager, PaymentSwap};

const APP_STATE_UPDATED_EVENT: &str = "app_state_updated";

// SDK / Nostr state (managed alongside AppStateManager)
pub struct SdkState {
    pub wallet_store: wallet_store::WalletStore,
    pub nostr_keys: Mutex<Option<nostr_sdk::Keys>>,
    pub nostr_client: tokio::sync::Mutex<Option<nostr_sdk::Client>>,
}

impl Default for SdkState {
    fn default() -> Self {
        Self {
            wallet_store: wallet_store::WalletStore::default(),
            nostr_keys: Mutex::new(None),
            nostr_client: tokio::sync::Mutex::new(None),
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
fn is_first_launch(manager: State<Mutex<AppStateManager>>) -> bool {
    let manager = manager.lock().expect("state manager mutex");
    manager.is_first_launch()
}

#[tauri::command]
fn set_network(
    network: Network,
    manager: State<Mutex<AppStateManager>>,
    app: AppHandle,
) -> Result<AppState, String> {
    let mut manager = manager.lock().expect("state manager mutex");
    let state = manager.set_network(network);
    emit_state(&app, &state);
    Ok(state)
}

// ============================================================================
// App State Commands
// ============================================================================

#[tauri::command]
fn get_app_state(manager: State<Mutex<AppStateManager>>) -> Result<AppState, String> {
    let manager = manager.lock().expect("state manager mutex");
    if !manager.is_initialized() {
        return Err("Not initialized - select a network first".to_string());
    }
    Ok(manager.snapshot())
}

// ============================================================================
// Wallet Commands
// ============================================================================

#[tauri::command]
fn get_wallet_status(
    manager: State<Mutex<AppStateManager>>,
) -> Result<wallet::types::WalletStatus, String> {
    let manager = manager.lock().expect("state manager mutex");
    Ok(manager
        .wallet()
        .map(|w| w.status())
        .unwrap_or(wallet::types::WalletStatus::NotCreated))
}

#[tauri::command]
fn create_wallet(
    password: String,
    manager: State<Mutex<AppStateManager>>,
    app: AppHandle,
) -> Result<String, String> {
    let mut manager = manager.lock().expect("state manager mutex");
    let wallet = manager.wallet_mut().ok_or("Wallet not initialized")?;
    let mnemonic = wallet.create_wallet(&password).map_err(|e| e.to_string())?;
    manager.bump_revision();
    let state = manager.snapshot();
    emit_state(&app, &state);
    Ok(mnemonic)
}

#[tauri::command]
fn restore_wallet(
    mnemonic: String,
    password: String,
    manager: State<Mutex<AppStateManager>>,
    app: AppHandle,
) -> Result<AppState, String> {
    let mut manager = manager.lock().expect("state manager mutex");
    let wallet = manager.wallet_mut().ok_or("Wallet not initialized")?;
    wallet
        .restore_wallet(&mnemonic, &password)
        .map_err(|e| e.to_string())?;
    manager.bump_revision();
    let state = manager.snapshot();
    emit_state(&app, &state);
    Ok(state)
}

#[tauri::command]
fn unlock_wallet(
    password: String,
    manager: State<Mutex<AppStateManager>>,
    app: AppHandle,
) -> Result<AppState, String> {
    let mut manager = manager.lock().expect("state manager mutex");
    let wallet = manager.wallet_mut().ok_or("Wallet not initialized")?;
    wallet.unlock(&password).map_err(|e| e.to_string())?;
    manager.bump_revision();
    let state = manager.snapshot();
    emit_state(&app, &state);
    Ok(state)
}

#[tauri::command]
fn lock_wallet(
    manager: State<Mutex<AppStateManager>>,
    app: AppHandle,
) -> Result<AppState, String> {
    let mut manager = manager.lock().expect("state manager mutex");
    if let Some(wallet) = manager.wallet_mut() {
        wallet.lock();
    }
    manager.bump_revision();
    let state = manager.snapshot();
    emit_state(&app, &state);
    Ok(state)
}

#[tauri::command]
fn sync_wallet(
    manager: State<Mutex<AppStateManager>>,
    app: AppHandle,
) -> Result<AppState, String> {
    let mut manager = manager.lock().expect("state manager mutex");
    let wallet = manager.wallet_mut().ok_or("Wallet not initialized")?;
    wallet.sync().map_err(|e| e.to_string())?;
    manager.bump_revision();
    let state = manager.snapshot();
    emit_state(&app, &state);
    Ok(state)
}

#[tauri::command]
fn get_wallet_balance(
    manager: State<Mutex<AppStateManager>>,
) -> Result<wallet::types::WalletBalance, String> {
    let manager = manager.lock().expect("state manager mutex");
    let wallet = manager.wallet().ok_or("Wallet not initialized")?;
    wallet.balance().map_err(|e| e.to_string())
}

#[tauri::command]
fn get_wallet_address(
    index: Option<u32>,
    manager: State<Mutex<AppStateManager>>,
) -> Result<wallet::types::WalletAddress, String> {
    let manager = manager.lock().expect("state manager mutex");
    let wallet = manager.wallet().ok_or("Wallet not initialized")?;
    wallet.address(index).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_wallet_transactions(
    manager: State<Mutex<AppStateManager>>,
) -> Result<Vec<wallet::types::WalletTransaction>, String> {
    let manager = manager.lock().expect("state manager mutex");
    let wallet = manager.wallet().ok_or("Wallet not initialized")?;
    wallet.transactions().map_err(|e| e.to_string())
}

#[tauri::command]
fn send_lbtc(
    address: String,
    amount_sat: u64,
    fee_rate: Option<f32>,
    manager: State<Mutex<AppStateManager>>,
    app: AppHandle,
) -> Result<wallet::types::LiquidSendResult, String> {
    let mut manager = manager.lock().expect("state manager mutex");
    let wallet = manager.wallet_mut().ok_or("Wallet not initialized")?;
    let result = wallet
        .send_lbtc(&address, amount_sat, fee_rate)
        .map_err(|e| e.to_string())?;
    manager.bump_revision();
    let state = manager.snapshot();
    emit_state(&app, &state);
    Ok(result)
}

#[tauri::command]
fn get_wallet_mnemonic(
    password: String,
    manager: State<Mutex<AppStateManager>>,
) -> Result<String, String> {
    let manager = manager.lock().expect("state manager mutex");
    let wallet = manager.wallet().ok_or("Wallet not initialized")?;
    let mnemonic = wallet
        .persister()
        .load(&password)
        .map_err(|e| e.to_string())?;
    Ok(mnemonic)
}

// ============================================================================
// Payment Commands (Boltz)
// ============================================================================

#[tauri::command]
async fn pay_lightning_invoice(
    invoice: String,
    manager: State<'_, Mutex<AppStateManager>>,
    app: AppHandle,
) -> Result<payments::boltz::BoltzSubmarineSwapCreated, String> {
    let (network, refund_pubkey_hex) = {
        let mgr = manager.lock().expect("state manager mutex");
        let network = mgr.network().ok_or("Not initialized - select a network first")?;
        let wallet = mgr.wallet().ok_or("Wallet not initialized")?;
        let refund_pubkey_hex = wallet
            .boltz_submarine_refund_pubkey_hex()
            .map_err(|e| format!("Wallet must be unlocked to initiate swap: {}", e))?;
        (network, refund_pubkey_hex)
    };

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

    {
        let mut mgr = manager.lock().expect("state manager mutex");
        mgr.upsert_payment_swap(saved_swap);
        let state = mgr.snapshot();
        emit_state(&app, &state);
    }

    Ok(created)
}

#[tauri::command]
async fn create_lightning_receive(
    amount_sat: u64,
    manager: State<'_, Mutex<AppStateManager>>,
    app: AppHandle,
) -> Result<payments::boltz::BoltzLightningReceiveCreated, String> {
    let (network, claim_pubkey_hex) = {
        let mgr = manager.lock().expect("state manager mutex");
        let network = mgr.network().ok_or("Not initialized - select a network first")?;
        let wallet = mgr.wallet().ok_or("Wallet not initialized")?;
        let claim_pubkey_hex = wallet
            .boltz_reverse_claim_pubkey_hex()
            .map_err(|e| format!("Wallet must be unlocked to initiate swap: {}", e))?;
        (network, claim_pubkey_hex)
    };

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

    {
        let mut mgr = manager.lock().expect("state manager mutex");
        mgr.upsert_payment_swap(saved_swap);
        let state = mgr.snapshot();
        emit_state(&app, &state);
    }

    Ok(created)
}

#[tauri::command]
async fn create_bitcoin_receive(
    amount_sat: u64,
    manager: State<'_, Mutex<AppStateManager>>,
    app: AppHandle,
) -> Result<payments::boltz::BoltzChainSwapCreated, String> {
    let (network, claim_pubkey_hex, refund_pubkey_hex) = {
        let mgr = manager.lock().expect("state manager mutex");
        let network = mgr.network().ok_or("Not initialized - select a network first")?;
        let wallet = mgr.wallet().ok_or("Wallet not initialized")?;
        let claim_pubkey_hex = wallet
            .boltz_reverse_claim_pubkey_hex()
            .map_err(|e| format!("Wallet must be unlocked to initiate swap: {}", e))?;
        let refund_pubkey_hex = wallet
            .boltz_submarine_refund_pubkey_hex()
            .map_err(|e| format!("Wallet must be unlocked to initiate swap: {}", e))?;
        (network, claim_pubkey_hex, refund_pubkey_hex)
    };

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

    {
        let mut mgr = manager.lock().expect("state manager mutex");
        mgr.upsert_payment_swap(saved_swap);
        let state = mgr.snapshot();
        emit_state(&app, &state);
    }

    Ok(created)
}

#[tauri::command]
async fn create_bitcoin_send(
    amount_sat: u64,
    manager: State<'_, Mutex<AppStateManager>>,
    app: AppHandle,
) -> Result<payments::boltz::BoltzChainSwapCreated, String> {
    let (network, claim_pubkey_hex, refund_pubkey_hex) = {
        let mgr = manager.lock().expect("state manager mutex");
        let network = mgr.network().ok_or("Not initialized - select a network first")?;
        let wallet = mgr.wallet().ok_or("Wallet not initialized")?;
        let claim_pubkey_hex = wallet
            .boltz_reverse_claim_pubkey_hex()
            .map_err(|e| format!("Wallet must be unlocked to initiate swap: {}", e))?;
        let refund_pubkey_hex = wallet
            .boltz_submarine_refund_pubkey_hex()
            .map_err(|e| format!("Wallet must be unlocked to initiate swap: {}", e))?;
        (network, claim_pubkey_hex, refund_pubkey_hex)
    };

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

    {
        let mut mgr = manager.lock().expect("state manager mutex");
        mgr.upsert_payment_swap(saved_swap);
        let state = mgr.snapshot();
        emit_state(&app, &state);
    }

    Ok(created)
}

#[tauri::command]
async fn get_chain_swap_pairs(
    manager: State<'_, Mutex<AppStateManager>>,
) -> Result<payments::boltz::BoltzChainSwapPairsInfo, String> {
    let network = {
        let mgr = manager.lock().expect("state manager mutex");
        mgr.network().ok_or("Not initialized - select a network first")?
    };

    let boltz = payments::boltz::BoltzService::new(network, None);
    boltz
        .get_chain_swap_pairs_info()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_payment_swaps(
    manager: State<'_, Mutex<AppStateManager>>,
) -> Result<Vec<PaymentSwap>, String> {
    let mgr = manager.lock().expect("state manager mutex");
    Ok(mgr.payment_swaps().to_vec())
}

#[tauri::command]
async fn refresh_payment_swap_status(
    swap_id: String,
    manager: State<'_, Mutex<AppStateManager>>,
    app: AppHandle,
) -> Result<PaymentSwap, String> {
    let network = {
        let mgr = manager.lock().expect("state manager mutex");
        mgr.network().ok_or("Not initialized - select a network first")?
    };

    let boltz = payments::boltz::BoltzService::new(network, None);
    let status = boltz
        .get_swap_status(&swap_id)
        .await
        .map_err(|e| e.to_string())?;

    let updated_swap = {
        let mut mgr = manager.lock().expect("state manager mutex");
        let existing = mgr
            .payment_swaps()
            .iter()
            .find(|swap| swap.id == swap_id)
            .cloned()
            .ok_or_else(|| format!("Payment swap not found: {}", swap_id))?;

        let mut updated = existing;
        updated.status = status.status;
        updated.lockup_txid = status.lockup_txid;
        updated.updated_at = chrono::Utc::now().to_rfc3339();

        mgr.upsert_payment_swap(updated.clone());
        let state = mgr.snapshot();
        emit_state(&app, &state);
        updated
    };

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
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_log::Builder::default().build())
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
            commands::get_nostr_identity,
            commands::discover_contracts,
            commands::publish_contract,
            commands::oracle_attest,
            commands::create_contract_onchain,
            // Wallet store (SDK)
            wallet_store::create_software_signer,
            wallet_store::create_wollet,
            wallet_store::wallet_new_address,
            wallet_store::wallet_signer_id,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
