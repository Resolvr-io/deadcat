use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use lwk_signer::SwSigner;
use lwk_wollet::{NoPersist, Wollet, WolletDescriptor};
use serde::Serialize;

use crate::WalletNetwork;
use crate::WalletStoreState;

#[derive(Default)]
pub struct WalletStore {
    pub signers: Mutex<HashMap<String, SwSigner>>,
    pub wallets: Mutex<HashMap<String, WalletContext>>,
}

pub struct WalletContext {
    pub signer_id: String,
    pub wollet: Wollet,
}

#[derive(Serialize)]
pub struct SoftwareSignerResponse {
    signer_id: String,
    mnemonic: String,
    xpub: String,
    fingerprint: String,
}

#[derive(Serialize)]
pub struct WolletResponse {
    wallet_id: String,
    signer_id: String,
    first_address: String,
    address_index: u32,
}

#[derive(Serialize)]
pub struct AddressResponse {
    wallet_id: String,
    address: String,
    address_index: u32,
}

static ID_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_id(prefix: &str) -> String {
    let id = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{id}")
}

#[tauri::command]
pub fn create_software_signer(
    state: tauri::State<'_, WalletStoreState>,
    mnemonic: Option<String>,
    is_mainnet: Option<bool>,
) -> Result<SoftwareSignerResponse, String> {
    let use_mainnet = is_mainnet.unwrap_or(false);
    let (signer, mnemonic_phrase) = match mnemonic {
        Some(phrase) => {
            let signer = SwSigner::new(&phrase, use_mainnet)
                .map_err(|e| format!("failed to create software signer: {e}"))?;
            (signer, phrase)
        }
        None => {
            let (signer, random_mnemonic) = SwSigner::random(use_mainnet)
                .map_err(|e| format!("failed to generate software signer: {e}"))?;
            (signer, random_mnemonic.to_string())
        }
    };

    let signer_id = next_id("signer");
    let response = SoftwareSignerResponse {
        signer_id: signer_id.clone(),
        mnemonic: mnemonic_phrase,
        xpub: signer.xpub().to_string(),
        fingerprint: signer.fingerprint().to_string(),
    };

    let mut signers = state
        .wallet_store
        .signers
        .lock()
        .map_err(|_| "failed to lock signer store".to_string())?;
    signers.insert(signer_id, signer);

    Ok(response)
}

#[tauri::command]
pub fn create_wollet(
    state: tauri::State<'_, WalletStoreState>,
    signer_id: String,
    descriptor: String,
    network: WalletNetwork,
    wallet_id: Option<String>,
) -> Result<WolletResponse, String> {
    {
        let signers = state
            .wallet_store
            .signers
            .lock()
            .map_err(|_| "failed to lock signer store".to_string())?;
        if !signers.contains_key(&signer_id) {
            return Err(format!("unknown signer_id: {signer_id}"));
        }
    }

    let parsed_descriptor = WolletDescriptor::from_str(&descriptor)
        .map_err(|e| format!("invalid wollet descriptor: {e}"))?;
    let wollet = Wollet::new(network.into_lwk(), NoPersist::new(), parsed_descriptor)
        .map_err(|e| format!("failed to build wollet: {e}"))?;

    let first = wollet
        .address(None)
        .map_err(|e| format!("failed to derive first address: {e}"))?;

    let assigned_wallet_id = wallet_id.unwrap_or_else(|| next_id("wallet"));
    let response = WolletResponse {
        wallet_id: assigned_wallet_id.clone(),
        signer_id: signer_id.clone(),
        first_address: first.address().to_string(),
        address_index: first.index(),
    };

    let mut wallets = state
        .wallet_store
        .wallets
        .lock()
        .map_err(|_| "failed to lock wallet store".to_string())?;
    wallets.insert(assigned_wallet_id, WalletContext { signer_id, wollet });

    Ok(response)
}

#[tauri::command]
pub fn wallet_new_address(
    state: tauri::State<'_, WalletStoreState>,
    wallet_id: String,
) -> Result<AddressResponse, String> {
    let mut wallets = state
        .wallet_store
        .wallets
        .lock()
        .map_err(|_| "failed to lock wallet store".to_string())?;
    let wallet = wallets
        .get_mut(&wallet_id)
        .ok_or_else(|| format!("unknown wallet_id: {wallet_id}"))?;

    let details = wallet
        .wollet
        .address(None)
        .map_err(|e| format!("failed to derive address: {e}"))?;

    Ok(AddressResponse {
        wallet_id,
        address: details.address().to_string(),
        address_index: details.index(),
    })
}

#[tauri::command]
pub fn wallet_signer_id(
    state: tauri::State<'_, WalletStoreState>,
    wallet_id: String,
) -> Result<String, String> {
    let wallets = state
        .wallet_store
        .wallets
        .lock()
        .map_err(|_| "failed to lock wallet store".to_string())?;
    let wallet = wallets
        .get(&wallet_id)
        .ok_or_else(|| format!("unknown wallet_id: {wallet_id}"))?;
    Ok(wallet.signer_id.clone())
}
