use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::wallet::persister::MnemonicPersister;
use crate::wallet::types::WalletStatus;
use crate::Network;

/// Duration of inactivity (in seconds) before the wallet auto-locks.
pub const AUTO_LOCK_TIMEOUT_SECS: u64 = 300; // 5 minutes

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
// SDK network conversion
// ============================================================================

pub fn to_sdk_network(network: Network) -> deadcat_sdk::Network {
    match network {
        Network::Mainnet => deadcat_sdk::Network::Liquid,
        Network::Testnet => deadcat_sdk::Network::LiquidTestnet,
        Network::Regtest => deadcat_sdk::Network::LiquidRegtest,
    }
}

// ============================================================================
// App state manager
// ============================================================================

pub struct AppStateManager {
    pub app_data_dir: PathBuf,
    network: Option<Network>,
    persister: Option<MnemonicPersister>,
    store: Option<Arc<std::sync::Mutex<deadcat_store::DeadcatStore>>>,
    /// Whether the node's wallet is currently unlocked.
    /// Updated by the caller after node operations.
    wallet_unlocked: bool,
    local_state: LocalState,
    revision: u64,
    /// Timestamp of last user activity (for auto-lock).
    last_activity: Instant,
}

impl AppStateManager {
    pub fn new(app_data_dir: PathBuf) -> Self {
        let local_state = Self::load_local_state(&app_data_dir).unwrap_or_default();
        Self {
            app_data_dir,
            network: None,
            persister: None,
            store: None,
            wallet_unlocked: false,
            local_state,
            revision: 0,
            last_activity: Instant::now(),
        }
    }

    /// Load saved network config and initialize persister if configured.
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
        self.persister = Some(MnemonicPersister::new(&self.app_data_dir, network.as_str()));

        // Open the store at <app_data_dir>/<network>/deadcat.db
        let store_dir = self.app_data_dir.join(network.as_str());
        std::fs::create_dir_all(&store_dir).ok();
        let db_path = store_dir.join("deadcat.db");
        let store = deadcat_store::DeadcatStore::open(db_path.to_str().unwrap_or(":memory:"))
            .expect("failed to open deadcat store");
        self.store = Some(Arc::new(std::sync::Mutex::new(store)));
    }

    pub fn persister(&self) -> Option<&MnemonicPersister> {
        self.persister.as_ref()
    }

    pub fn persister_mut(&mut self) -> Option<&mut MnemonicPersister> {
        self.persister.as_mut()
    }

    pub fn store(&self) -> Option<&Arc<std::sync::Mutex<deadcat_store::DeadcatStore>>> {
        self.store.as_ref()
    }

    /// Mark the wallet as unlocked/locked (synced from the NodeState).
    pub fn set_wallet_unlocked(&mut self, unlocked: bool) {
        self.wallet_unlocked = unlocked;
    }

    pub fn network_status(&self) -> NetworkStatus {
        let network = self.network;
        let sdk_network = network.map(to_sdk_network);
        NetworkStatus {
            network: network
                .map(|n| n.as_str().to_string())
                .unwrap_or_else(|| "unknown".into()),
            is_mainnet: network.map(|n| n.is_mainnet()).unwrap_or(false),
            electrum_url: sdk_network
                .map(|n| n.default_electrum_url().to_string())
                .unwrap_or_default(),
            policy_asset_id: sdk_network
                .map(|n| n.into_lwk().policy_asset().to_string())
                .unwrap_or_default(),
        }
    }

    pub fn wallet_status(&self) -> WalletStatus {
        self.wallet_status_for(self.wallet_unlocked)
    }

    /// Same as `wallet_status` but accepts an override for the unlock flag.
    /// Used when the caller knows the node's current unlock state.
    pub fn wallet_status_with_unlock(&self, is_unlocked: bool) -> WalletStatus {
        self.wallet_status_for(is_unlocked)
    }

    fn wallet_status_for(&self, is_unlocked: bool) -> WalletStatus {
        match &self.persister {
            Some(p) if !p.exists() => WalletStatus::NotCreated,
            Some(_) if is_unlocked => WalletStatus::Unlocked,
            Some(_) => WalletStatus::Locked,
            None => WalletStatus::NotCreated,
        }
    }

    pub fn snapshot(&self) -> AppState {
        self.snapshot_with_balance(None)
    }

    /// Build an `AppState` snapshot, optionally including wallet balance.
    pub fn snapshot_with_balance(&self, wallet_balance: Option<HashMap<String, u64>>) -> AppState {
        AppState {
            revision: self.revision,
            network_status: self.network_status(),
            wallet_status: self.wallet_status(),
            wallet_balance,
            payment_swaps: self.local_state.payment_swaps.clone(),
        }
    }

    pub fn bump_revision(&mut self) {
        self.revision += 1;
    }

    /// Record user activity (resets the auto-lock timer).
    pub fn touch_activity(&mut self) {
        self.last_activity = Instant::now();
    }

    /// Check if the auto-lock timeout has elapsed. If so, mark wallet locked
    /// and return `true` so the caller can lock the node and emit state.
    pub fn check_auto_lock(&mut self) -> bool {
        if self.last_activity.elapsed().as_secs() >= AUTO_LOCK_TIMEOUT_SECS && self.wallet_unlocked
        {
            self.wallet_unlocked = false;
            if let Some(persister) = self.persister.as_mut() {
                persister.clear_cache();
            }
            self.bump_revision();
            return true;
        }
        false
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
