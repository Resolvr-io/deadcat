use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lwk_common::Signer;
use lwk_signer::SwSigner;
use lwk_wollet::blocking::BlockchainBackend;
use lwk_wollet::elements::pset::PartiallySignedTransaction;
use lwk_wollet::elements::secp256k1_zkp::{self, Keypair};
use lwk_wollet::elements::Transaction;
use lwk_wollet::{ElectrumClient, ElectrumUrl, TxBuilder, WalletTxOut, Wollet, WolletDescriptor};
use thiserror::Error;

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

    #[error("Signer error: {0}")]
    Signer(String),

    #[error("Descriptor error: {0}")]
    Descriptor(String),

    #[error("Wallet init error: {0}")]
    Init(String),

    #[error("Electrum error: {0}")]
    Electrum(String),

    #[error("Query error: {0}")]
    Query(String),

    #[error("Finalize error: {0}")]
    Finalize(String),

    #[error("Broadcast error: {0}")]
    Broadcast(String),

    #[error("Persist error: {0}")]
    Persist(#[from] WalletPersistError),
}

pub struct WalletManager {
    app_data_dir: PathBuf,
    network: Network,
    persister: MnemonicPersister,
    signer: Option<SwSigner>,
    wollet: Option<Wollet>,
    electrum_url_str: String,
}

impl WalletManager {
    pub fn new(app_data_dir: &Path, network: Network) -> Self {
        let electrum_url_str = match network {
            Network::Mainnet => "ssl://blockstream.info:995".to_string(),
            Network::Testnet => "ssl://blockstream.info:465".to_string(),
            Network::Regtest => "tcp://localhost:50001".to_string(),
        };
        let persister = MnemonicPersister::new(app_data_dir, network.as_str());
        Self {
            app_data_dir: app_data_dir.to_path_buf(),
            network,
            persister,
            signer: None,
            wollet: None,
            electrum_url_str,
        }
    }

    pub fn electrum_url(&self) -> &str {
        &self.electrum_url_str
    }

    pub fn status(&self) -> WalletStatus {
        if !self.persister.exists() {
            WalletStatus::NotCreated
        } else if self.signer.is_none() {
            WalletStatus::Locked
        } else {
            WalletStatus::Unlocked
        }
    }

    /// Create a new wallet with a random mnemonic, encrypt with password.
    /// Returns the mnemonic string for user backup.
    pub fn create_wallet(&mut self, password: &str) -> Result<String, WalletError> {
        if self.persister.exists() {
            return Err(WalletError::AlreadyExists);
        }
        let is_mainnet = self.network.is_mainnet();
        let (signer, mnemonic) =
            SwSigner::random(is_mainnet).map_err(|e| WalletError::Signer(e.to_string()))?;

        let mnemonic_str = mnemonic.to_string();
        self.persister.save(&mnemonic_str, password)?;
        self.init_wollet_from_signer(signer)?;
        Ok(mnemonic_str)
    }

    /// Restore wallet from existing mnemonic, encrypt with password.
    pub fn restore_wallet(
        &mut self,
        mnemonic_str: &str,
        password: &str,
    ) -> Result<(), WalletError> {
        let _mnemonic: bip39::Mnemonic = mnemonic_str
            .parse()
            .map_err(|_| WalletError::InvalidMnemonic)?;
        self.persister.save(mnemonic_str, password)?;
        self.init_from_mnemonic(mnemonic_str)?;
        Ok(())
    }

    /// Unlock existing wallet with password.
    pub fn unlock(&mut self, password: &str) -> Result<(), WalletError> {
        let mnemonic_str = self.persister.load(password)?;
        self.init_from_mnemonic(&mnemonic_str)?;
        Ok(())
    }

    /// Lock the wallet (clear signer and wollet from memory).
    pub fn lock(&mut self) {
        self.signer = None;
        self.wollet = None;
    }

    fn init_from_mnemonic(&mut self, mnemonic_str: &str) -> Result<(), WalletError> {
        let is_mainnet = self.network.is_mainnet();
        let signer = SwSigner::new(mnemonic_str, is_mainnet)
            .map_err(|e| WalletError::Signer(e.to_string()))?;
        self.init_wollet_from_signer(signer)
    }

    fn init_wollet_from_signer(&mut self, signer: SwSigner) -> Result<(), WalletError> {
        let slip77_key = signer
            .slip77_master_blinding_key()
            .map_err(|e| WalletError::Signer(e.to_string()))?;
        let xpub = signer.xpub();
        let descriptor_str = format!("ct(slip77({}),elwpkh({}/*))", slip77_key, xpub);
        let descriptor: WolletDescriptor = descriptor_str
            .parse()
            .map_err(|e: lwk_wollet::Error| WalletError::Descriptor(e.to_string()))?;

        let persist_dir = self
            .app_data_dir
            .join(self.network.as_str())
            .join("wallet_db");
        let lwk_network = self.lwk_network();

        let wollet = Wollet::with_fs_persist(lwk_network, descriptor, &persist_dir)
            .map_err(|e| WalletError::Init(e.to_string()))?;

        self.signer = Some(signer);
        self.wollet = Some(wollet);
        Ok(())
    }

    /// Get the policy asset ID (L-BTC) for the current network.
    pub fn policy_asset_id(&self) -> String {
        self.lwk_network().policy_asset().to_string()
    }

    fn lwk_network(&self) -> lwk_wollet::ElementsNetwork {
        match self.network {
            Network::Mainnet => lwk_wollet::ElementsNetwork::Liquid,
            Network::Testnet => lwk_wollet::ElementsNetwork::LiquidTestnet,
            Network::Regtest => lwk_wollet::ElementsNetwork::default_regtest(),
        }
    }

    /// Sync wallet with electrum server.
    pub fn sync(&mut self) -> Result<(), WalletError> {
        let wollet = self.wollet.as_mut().ok_or(WalletError::NotUnlocked)?;
        let url: ElectrumUrl = self
            .electrum_url_str
            .parse()
            .map_err(|e| WalletError::Electrum(format!("{:?}", e)))?;
        let mut client =
            ElectrumClient::new(&url).map_err(|e| WalletError::Electrum(e.to_string()))?;
        lwk_wollet::full_scan_with_electrum_client(wollet, &mut client)
            .map_err(|e| WalletError::Electrum(e.to_string()))?;
        Ok(())
    }

    /// Get current balance as a map of asset_id hex -> satoshis.
    pub fn balance(&self) -> Result<WalletBalance, WalletError> {
        let wollet = self.wollet.as_ref().ok_or(WalletError::NotUnlocked)?;
        let balance_map = wollet
            .balance()
            .map_err(|e| WalletError::Query(e.to_string()))?;
        let mut assets = HashMap::new();
        for (asset_id, amount) in balance_map.iter() {
            if *amount > 0 {
                assets.insert(asset_id.to_string(), *amount);
            }
        }
        Ok(WalletBalance { assets })
    }

    /// Get a receive address.
    pub fn address(&self, index: Option<u32>) -> Result<WalletAddress, WalletError> {
        let wollet = self.wollet.as_ref().ok_or(WalletError::NotUnlocked)?;
        let addr_result = wollet
            .address(index)
            .map_err(|e| WalletError::Query(e.to_string()))?;
        Ok(WalletAddress {
            index: addr_result.index(),
            address: addr_result.address().to_string(),
        })
    }

    /// List UTXOs.
    pub fn utxos(&self) -> Result<Vec<WalletUtxo>, WalletError> {
        let wollet = self.wollet.as_ref().ok_or(WalletError::NotUnlocked)?;
        let utxos = wollet
            .utxos()
            .map_err(|e| WalletError::Query(e.to_string()))?;
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

    /// List wallet transactions with net L-BTC balance changes.
    pub fn transactions(&self) -> Result<Vec<WalletTransaction>, WalletError> {
        let wollet = self.wollet.as_ref().ok_or(WalletError::NotUnlocked)?;
        let policy_asset = self.lwk_network().policy_asset();
        let txs = wollet
            .transactions()
            .map_err(|e| WalletError::Query(e.to_string()))?;
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
                }
            })
            .collect())
    }

    /// Sign a PSET and finalize it into a Transaction.
    pub fn sign_pset(
        &self,
        mut pset: PartiallySignedTransaction,
    ) -> Result<Transaction, WalletError> {
        let signer = self.signer.as_ref().ok_or(WalletError::NotUnlocked)?;
        let wollet = self.wollet.as_ref().ok_or(WalletError::NotUnlocked)?;
        wollet
            .add_details(&mut pset)
            .map_err(|e| WalletError::Signer(format!("add_details: {}", e)))?;
        signer
            .sign(&mut pset)
            .map_err(|e| WalletError::Signer(format!("{:?}", e)))?;
        wollet
            .finalize(&mut pset)
            .map_err(|e| WalletError::Finalize(e.to_string()))
    }

    /// Send L-BTC to a Liquid address.
    pub fn send_lbtc(
        &mut self,
        address_str: &str,
        amount_sat: u64,
        fee_rate: Option<f32>,
    ) -> Result<LiquidSendResult, WalletError> {
        let wollet = self.wollet.as_ref().ok_or(WalletError::NotUnlocked)?;

        let address: lwk_wollet::elements::Address = address_str
            .parse()
            .map_err(|e| WalletError::Query(format!("Invalid address: {}", e)))?;

        let pset = TxBuilder::new(self.lwk_network())
            .add_lbtc_recipient(&address, amount_sat)
            .map_err(|e| WalletError::Query(format!("add_lbtc_recipient: {}", e)))?
            .fee_rate(fee_rate)
            .finish(wollet)
            .map_err(|e| WalletError::Query(format!("TxBuilder finish: {}", e)))?;

        let tx = self.sign_pset(pset)?;

        let fee_sat: u64 = tx
            .output
            .iter()
            .filter(|o| o.script_pubkey.is_empty())
            .map(|o| o.value.explicit().unwrap_or(0))
            .sum();

        let txid = tx.txid().to_string();

        // Broadcast
        let url: ElectrumUrl = self
            .electrum_url_str
            .parse()
            .map_err(|e| WalletError::Electrum(format!("{:?}", e)))?;
        let mut client =
            ElectrumClient::new(&url).map_err(|e| WalletError::Electrum(e.to_string()))?;
        client
            .broadcast(&tx)
            .map_err(|e| WalletError::Broadcast(e.to_string()))?;

        // Sync wallet to update balance/tx list
        let wollet = self.wollet.as_mut().ok_or(WalletError::NotUnlocked)?;
        lwk_wollet::full_scan_with_electrum_client(wollet, &mut client)
            .map_err(|e| WalletError::Electrum(e.to_string()))?;

        Ok(LiquidSendResult { txid, fee_sat })
    }

    /// Raw UTXOs with blinding factors for SDK consumption.
    pub fn raw_utxos(&self) -> Result<Vec<WalletTxOut>, WalletError> {
        let wollet = self.wollet.as_ref().ok_or(WalletError::NotUnlocked)?;
        wollet
            .utxos()
            .map_err(|e| WalletError::Query(e.to_string()))
    }

    /// Fetch a raw transaction by txid from electrum.
    pub fn fetch_transaction(
        &self,
        txid: &lwk_wollet::elements::Txid,
    ) -> Result<Transaction, WalletError> {
        let url: ElectrumUrl = self
            .electrum_url_str
            .parse()
            .map_err(|e| WalletError::Electrum(format!("{:?}", e)))?;
        let client =
            ElectrumClient::new(&url).map_err(|e| WalletError::Electrum(e.to_string()))?;
        let txs = client
            .get_transactions(&[*txid])
            .map_err(|e| WalletError::Electrum(e.to_string()))?;
        txs.into_iter()
            .next()
            .ok_or_else(|| WalletError::Query(format!("transaction {} not found", txid)))
    }

    /// Broadcast a pre-signed transaction and sync the wallet afterward.
    pub fn broadcast_and_sync(
        &mut self,
        tx: &Transaction,
    ) -> Result<lwk_wollet::elements::Txid, WalletError> {
        let url: ElectrumUrl = self
            .electrum_url_str
            .parse()
            .map_err(|e| WalletError::Electrum(format!("{:?}", e)))?;
        let mut client =
            ElectrumClient::new(&url).map_err(|e| WalletError::Electrum(e.to_string()))?;
        let txid = client
            .broadcast(tx)
            .map_err(|e| WalletError::Broadcast(e.to_string()))?;

        let wollet = self.wollet.as_mut().ok_or(WalletError::NotUnlocked)?;
        lwk_wollet::full_scan_with_electrum_client(wollet, &mut client)
            .map_err(|e| WalletError::Electrum(e.to_string()))?;

        Ok(txid)
    }

    /// Get the persister reference (for password-gated operations like backup).
    pub fn persister(&self) -> &MnemonicPersister {
        &self.persister
    }

    /// Derive a Boltz-compatible compressed refund public key hex string.
    /// Uses a deterministic path for submarine swap refunds.
    pub fn boltz_submarine_refund_pubkey_hex(&self) -> Result<String, WalletError> {
        let signer = self.signer.as_ref().ok_or(WalletError::NotUnlocked)?;
        let network_path = if self.network.is_mainnet() { 1776 } else { 1 };
        let path_str = format!("m/49'/{network_path}'/21'/0/0");
        let path: lwk_wollet::bitcoin::bip32::DerivationPath = path_str
            .parse()
            .map_err(|e| WalletError::Signer(format!("{}", e)))?;
        let derived = signer
            .derive_xprv(&path)
            .map_err(|e| WalletError::Signer(format!("{:?}", e)))?;
        let secp = secp256k1_zkp::Secp256k1::new();
        let secret = secp256k1_zkp::SecretKey::from_slice(&derived.private_key.secret_bytes())
            .map_err(|e| WalletError::Signer(format!("{}", e)))?;
        let keypair = Keypair::from_secret_key(&secp, &secret);
        Ok(keypair.public_key().to_string())
    }

    /// Derive a Boltz-compatible compressed claim public key hex string.
    /// Uses the reverse-swap derivation path.
    pub fn boltz_reverse_claim_pubkey_hex(&self) -> Result<String, WalletError> {
        let signer = self.signer.as_ref().ok_or(WalletError::NotUnlocked)?;
        let network_path = if self.network.is_mainnet() { 1776 } else { 1 };
        let path_str = format!("m/84'/{network_path}'/42'/0/0");
        let path: lwk_wollet::bitcoin::bip32::DerivationPath = path_str
            .parse()
            .map_err(|e| WalletError::Signer(format!("{}", e)))?;
        let derived = signer
            .derive_xprv(&path)
            .map_err(|e| WalletError::Signer(format!("{:?}", e)))?;
        let secp = secp256k1_zkp::Secp256k1::new();
        let secret = secp256k1_zkp::SecretKey::from_slice(&derived.private_key.secret_bytes())
            .map_err(|e| WalletError::Signer(format!("{}", e)))?;
        let keypair = Keypair::from_secret_key(&secp, &secret);
        Ok(keypair.public_key().to_string())
    }
}
