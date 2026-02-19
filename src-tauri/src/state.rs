use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::wallet::types::WalletStatus;
use crate::wallet::WalletManager;
use crate::Network;

const LOCAL_STATE_FILE: &str = "deadcat_state.json";
const CONFIG_FILE: &str = "network_config.json";

// ============================================================================
// Persisted local state (payment swaps)
// ============================================================================

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalState {
    #[serde(default)]
    payment_swaps: Vec<PaymentSwap>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentSwap {
    pub id: String,
    pub flow: String,
    pub network: String,
    pub status: String,
    pub invoice_amount_sat: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_amount_sat: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lockup_address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_block_height: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pair_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invoice: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invoice_expiry_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invoice_expires_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lockup_txid: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// ============================================================================
// Network status & app state (sent to frontend)
// ============================================================================

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkStatus {
    pub network: String,
    pub is_mainnet: bool,
    pub electrum_url: String,
    pub policy_asset_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    pub revision: u64,
    pub network_status: NetworkStatus,
    pub wallet_status: WalletStatus,
    pub wallet_balance: Option<HashMap<String, u64>>,
    pub payment_swaps: Vec<PaymentSwap>,
}

// ============================================================================
// App state manager
// ============================================================================

pub struct AppStateManager {
    app_data_dir: PathBuf,
    network: Option<Network>,
    wallet: Option<WalletManager>,
    local_state: LocalState,
    revision: u64,
}

impl AppStateManager {
    pub fn new(app_data_dir: PathBuf) -> Self {
        let local_state = Self::load_local_state(&app_data_dir).unwrap_or_default();
        Self {
            app_data_dir,
            network: None,
            wallet: None,
            local_state,
            revision: 0,
        }
    }

    /// Load saved network config and initialize wallet manager if configured.
    pub fn initialize(&mut self) {
        if let Some(network) = self.load_network_config() {
            self.init_with_network(network);
        }
    }

    pub fn is_first_launch(&self) -> bool {
        !self.app_data_dir.join(CONFIG_FILE).exists()
    }

    pub fn is_initialized(&self) -> bool {
        self.network.is_some()
    }

    pub fn network(&self) -> Option<Network> {
        self.network
    }

    pub fn set_network(&mut self, network: Network) -> AppState {
        self.save_network_config(network);
        self.init_with_network(network);
        self.bump_revision();
        self.snapshot()
    }

    fn init_with_network(&mut self, network: Network) {
        self.network = Some(network);
        self.wallet = Some(WalletManager::new(&self.app_data_dir, network));
    }

    pub fn wallet(&self) -> Option<&WalletManager> {
        self.wallet.as_ref()
    }

    pub fn wallet_mut(&mut self) -> Option<&mut WalletManager> {
        self.wallet.as_mut()
    }

    pub fn network_status(&self) -> NetworkStatus {
        let network = self.network;
        NetworkStatus {
            network: network
                .map(|n| n.as_str().to_string())
                .unwrap_or_else(|| "unknown".into()),
            is_mainnet: network.map(|n| n.is_mainnet()).unwrap_or(false),
            electrum_url: self
                .wallet
                .as_ref()
                .map(|w| w.electrum_url().to_string())
                .unwrap_or_default(),
            policy_asset_id: self
                .wallet
                .as_ref()
                .map(|w| w.policy_asset_id())
                .unwrap_or_default(),
        }
    }

    pub fn snapshot(&self) -> AppState {
        let network_status = self.network_status();

        let wallet_status = self
            .wallet
            .as_ref()
            .map(|w| w.status())
            .unwrap_or(WalletStatus::NotCreated);

        let wallet_balance = if wallet_status == WalletStatus::Unlocked {
            self.wallet
                .as_ref()
                .and_then(|w| w.balance().ok())
                .map(|b| b.assets)
        } else {
            None
        };

        AppState {
            revision: self.revision,
            network_status,
            wallet_status,
            wallet_balance,
            payment_swaps: self.local_state.payment_swaps.clone(),
        }
    }

    pub fn bump_revision(&mut self) {
        self.revision += 1;
    }

    pub fn payment_swaps(&self) -> &[PaymentSwap] {
        &self.local_state.payment_swaps
    }

    pub fn upsert_payment_swap(&mut self, swap: PaymentSwap) {
        match self
            .local_state
            .payment_swaps
            .iter_mut()
            .find(|s| s.id == swap.id)
        {
            Some(existing) => *existing = swap,
            None => self.local_state.payment_swaps.push(swap),
        }
        self.save_local_state();
        self.bump_revision();
    }

    // --- Persistence helpers ---

    fn load_network_config(&self) -> Option<Network> {
        let path = self.app_data_dir.join(CONFIG_FILE);
        let contents = fs::read_to_string(path).ok()?;
        let config: serde_json::Value = serde_json::from_str(&contents).ok()?;
        let network_str = config.get("network")?.as_str()?;
        network_str.parse().ok()
    }

    fn save_network_config(&self, network: Network) {
        let path = self.app_data_dir.join(CONFIG_FILE);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let config = serde_json::json!({ "network": network.as_str() });
        if let Ok(json) = serde_json::to_string_pretty(&config) {
            let _ = fs::write(path, json);
        }
    }

    fn load_local_state(dir: &Path) -> Option<LocalState> {
        let path = dir.join(LOCAL_STATE_FILE);
        let contents = fs::read_to_string(path).ok()?;
        serde_json::from_str(&contents).ok()
    }

    fn save_local_state(&self) {
        let path = self.app_data_dir.join(LOCAL_STATE_FILE);
        if let Ok(json) = serde_json::to_string_pretty(&self.local_state) {
            let _ = fs::write(path, json);
        }
    }
}
