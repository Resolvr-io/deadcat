use std::collections::HashMap;
use std::path::{Path, PathBuf};

use deadcat_sdk::DeadcatSdk;
use deadcat_store::{ContractMetadataInput, DeadcatStore, MarketFilter, MarketInfo};
use thiserror::Error;

use crate::chain_adapter::ElectrumChainAdapter;
use crate::Network;

use super::persister::{MnemonicPersister, WalletPersistError};
use super::types::{
    LiquidSendResult, WalletAddress, WalletBalance, WalletStatus, WalletTransaction, WalletUtxo,
};

#[derive(Error, Debug)]
pub enum WalletError {
    #[error("Wallet already exists for this network")]
    AlreadyExists,

    #[error("Invalid mnemonic")]
    InvalidMnemonic,

    #[error("Wallet not unlocked")]
    NotUnlocked,

    #[error("SDK error: {0}")]
    Sdk(#[from] deadcat_sdk::Error),

    #[error("Persist error: {0}")]
    Persist(#[from] WalletPersistError),

    #[error("Store error: {0}")]
    Store(#[from] deadcat_store::StoreError),
}

/// Converts the app-layer `Network` to the SDK `Network`.
fn to_sdk_network(network: Network) -> deadcat_sdk::Network {
    match network {
        Network::Mainnet => deadcat_sdk::Network::Liquid,
        Network::Testnet => deadcat_sdk::Network::LiquidTestnet,
        Network::Regtest => deadcat_sdk::Network::LiquidRegtest,
    }
}

pub struct WalletManager {
    app_data_dir: PathBuf,
    network: Network,
    persister: MnemonicPersister,
    sdk: Option<DeadcatSdk>,
    store: DeadcatStore,
}

impl WalletManager {
    pub fn new(app_data_dir: &Path, network: Network) -> Self {
        let persister = MnemonicPersister::new(app_data_dir, network.as_str());

        // Open the store at <app_data_dir>/<network>/deadcat.db
        let store_dir = app_data_dir.join(network.as_str());
        std::fs::create_dir_all(&store_dir).ok();
        let db_path = store_dir.join("deadcat.db");
        let store = DeadcatStore::open(db_path.to_str().unwrap_or(":memory:"))
            .expect("failed to open deadcat store");

        Self {
            app_data_dir: app_data_dir.to_path_buf(),
            network,
            persister,
            sdk: None,
            store,
        }
    }

    pub fn electrum_url(&self) -> &str {
        self.sdk
            .as_ref()
            .map(|s| s.electrum_url())
            .unwrap_or_else(|| to_sdk_network(self.network).default_electrum_url())
    }

    pub fn status(&self) -> WalletStatus {
        if !self.persister.exists() {
            WalletStatus::NotCreated
        } else if self.sdk.is_none() {
            WalletStatus::Locked
        } else {
            WalletStatus::Unlocked
        }
    }

    pub fn create_wallet(&mut self, password: &str) -> Result<String, WalletError> {
        if self.persister.exists() {
            return Err(WalletError::AlreadyExists);
        }
        let sdk_network = to_sdk_network(self.network);
        let (mnemonic_str, _signer) = DeadcatSdk::generate_mnemonic(sdk_network.is_mainnet())?;
        self.persister.save(&mnemonic_str, password)?;
        self.init_sdk(&mnemonic_str)?;
        Ok(mnemonic_str)
    }

    pub fn restore_wallet(
        &mut self,
        mnemonic_str: &str,
        password: &str,
    ) -> Result<(), WalletError> {
        let _mnemonic: bip39::Mnemonic = mnemonic_str
            .parse()
            .map_err(|_| WalletError::InvalidMnemonic)?;
        self.persister.save(mnemonic_str, password)?;
        self.init_sdk(mnemonic_str)?;
        Ok(())
    }

    pub fn unlock(&mut self, password: &str) -> Result<(), WalletError> {
        // Use cached mnemonic if available (skips expensive Argon2 KDF)
        if let Some(cached) = self.persister.cached() {
            let cached = cached.to_string();
            self.init_sdk(&cached)?;
            return Ok(());
        }
        let mnemonic_str = self.persister.load(password)?;
        self.init_sdk(&mnemonic_str)?;
        Ok(())
    }

    pub fn lock(&mut self) {
        self.sdk = None;
        self.persister.clear_cache();
    }

    pub fn delete_wallet(&mut self) -> Result<(), WalletError> {
        self.sdk = None;
        self.persister.delete()?;
        Ok(())
    }

    fn init_sdk(&mut self, mnemonic_str: &str) -> Result<(), WalletError> {
        let sdk_network = to_sdk_network(self.network);
        let electrum_url = sdk_network.default_electrum_url();
        let sdk = DeadcatSdk::new(mnemonic_str, sdk_network, electrum_url, &self.app_data_dir)?;
        self.sdk = Some(sdk);
        Ok(())
    }

    pub fn sdk(&self) -> Result<&DeadcatSdk, WalletError> {
        self.sdk.as_ref().ok_or(WalletError::NotUnlocked)
    }

    pub fn sdk_mut(&mut self) -> Result<&mut DeadcatSdk, WalletError> {
        self.sdk.as_mut().ok_or(WalletError::NotUnlocked)
    }

    // ── Delegated wallet operations ──────────────────────────────────────

    pub fn policy_asset_id(&self) -> String {
        to_sdk_network(self.network)
            .into_lwk()
            .policy_asset()
            .to_string()
    }

    pub fn sync(&mut self) -> Result<(), WalletError> {
        self.sdk_mut()?.sync()?;
        self.sync_store()?;
        Ok(())
    }

    /// Sync the market store against the chain. Works independently of wallet
    /// unlock state — only needs an Electrum URL.
    pub fn sync_store(&mut self) -> Result<(), WalletError> {
        let electrum_url = self.electrum_url().to_string();
        let chain = ElectrumChainAdapter::new(&electrum_url);
        self.store.sync(&chain)?;
        Ok(())
    }

    pub fn balance(&self) -> Result<WalletBalance, WalletError> {
        let balance_map = self.sdk()?.balance()?;
        let mut assets = HashMap::new();
        for (asset_id, amount) in balance_map.iter() {
            if *amount > 0 {
                assets.insert(asset_id.to_string(), *amount);
            }
        }
        Ok(WalletBalance { assets })
    }

    pub fn address(&self, index: Option<u32>) -> Result<WalletAddress, WalletError> {
        let addr_result = self.sdk()?.address(index)?;
        Ok(WalletAddress {
            index: addr_result.index(),
            address: addr_result.address().to_string(),
        })
    }

    pub fn utxos(&self) -> Result<Vec<WalletUtxo>, WalletError> {
        let utxos = self.sdk()?.utxos()?;
        Ok(utxos
            .iter()
            .map(|u| WalletUtxo {
                txid: u.outpoint.txid.to_string(),
                vout: u.outpoint.vout,
                asset_id: u.unblinded.asset.to_string(),
                value: u.unblinded.value,
                height: u.height,
            })
            .collect())
    }

    pub fn transactions(&self) -> Result<Vec<WalletTransaction>, WalletError> {
        let sdk = self.sdk()?;
        let policy_asset = sdk.policy_asset();
        let txs = sdk.transactions()?;
        Ok(txs
            .iter()
            .map(|tx| {
                let balance_change = tx.balance.get(&policy_asset).copied().unwrap_or(0);
                WalletTransaction {
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

    pub fn send_lbtc(
        &mut self,
        address_str: &str,
        amount_sat: u64,
        fee_rate: Option<f32>,
    ) -> Result<LiquidSendResult, WalletError> {
        let (txid, fee_sat) = self
            .sdk_mut()?
            .send_lbtc(address_str, amount_sat, fee_rate)?;
        Ok(LiquidSendResult {
            txid: txid.to_string(),
            fee_sat,
        })
    }

    pub fn boltz_submarine_refund_pubkey_hex(&self) -> Result<String, WalletError> {
        Ok(self.sdk()?.boltz_submarine_refund_pubkey_hex()?)
    }

    pub fn boltz_reverse_claim_pubkey_hex(&self) -> Result<String, WalletError> {
        Ok(self.sdk()?.boltz_reverse_claim_pubkey_hex()?)
    }

    pub fn persister(&self) -> &MnemonicPersister {
        &self.persister
    }

    pub fn persister_mut(&mut self) -> &mut MnemonicPersister {
        &mut self.persister
    }

    // ── Market store delegation ──────────────────────────────────────────

    pub fn ingest_market(
        &mut self,
        params: &deadcat_sdk::ContractParams,
        metadata: Option<&ContractMetadataInput>,
    ) -> Result<deadcat_sdk::MarketId, WalletError> {
        Ok(self.store.ingest_market(params, metadata)?)
    }

    pub fn list_markets(&mut self, filter: &MarketFilter) -> Result<Vec<MarketInfo>, WalletError> {
        Ok(self.store.list_markets(filter)?)
    }

    pub fn get_market(
        &mut self,
        id: &deadcat_sdk::MarketId,
    ) -> Result<Option<MarketInfo>, WalletError> {
        Ok(self.store.get_market(id)?)
    }
}
