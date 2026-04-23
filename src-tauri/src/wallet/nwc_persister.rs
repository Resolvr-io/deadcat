//! Encrypted persistence for the user's NWC (Nostr Wallet Connect)
//! connection string. Mirrors `MnemonicPersister`'s crypto choices —
//! Argon2 → AES-256-GCM → base64-wrapped JSON on disk — so the
//! wallet unlock password protects both secrets with no second
//! password to manage.
//!
//! No in-memory cache: the URL is only read at boot (to reconnect
//! Bitcoin Connect) and cleared on disconnect. No need to keep it
//! resident.
//!
//! See `wallet/persister.rs` for the parallel mnemonic-storage
//! implementation.

use std::fs;
use std::path::{Path, PathBuf};

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde::{Deserialize, Serialize};

use super::persister::WalletPersistError;

const NWC_FILE: &str = "nwc_encrypted.json";

#[derive(Serialize, Deserialize)]
struct EncryptedNwcFile {
    salt: String,
    nonce: String,
    ciphertext: String,
}

pub struct NwcPersister {
    file_path: PathBuf,
}

impl NwcPersister {
    pub fn new(app_data_dir: &Path, network: &str) -> Self {
        Self {
            file_path: app_data_dir.join(network).join(NWC_FILE),
        }
    }

    pub fn exists(&self) -> bool {
        self.file_path.exists()
    }

    pub fn delete(&self) -> Result<(), WalletPersistError> {
        if self.file_path.exists() {
            fs::remove_file(&self.file_path)?;
        }
        Ok(())
    }

    pub fn save(&self, nwc_url: &str, password: &str) -> Result<(), WalletPersistError> {
        let salt: [u8; 16] = rand::random();

        let mut key_bytes = [0u8; 32];
        argon2::Argon2::default()
            .hash_password_into(password.as_bytes(), &salt, &mut key_bytes)
            .map_err(|e| WalletPersistError::Crypto(e.to_string()))?;

        let cipher = Aes256Gcm::new_from_slice(&key_bytes)
            .map_err(|e| WalletPersistError::Crypto(e.to_string()))?;
        let nonce_bytes: [u8; 12] = rand::random();
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, nwc_url.as_bytes())
            .map_err(|e| WalletPersistError::Crypto(e.to_string()))?;

        let file = EncryptedNwcFile {
            salt: BASE64.encode(salt),
            nonce: BASE64.encode(nonce_bytes),
            ciphertext: BASE64.encode(ciphertext),
        };

        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&file)?;
        fs::write(&self.file_path, json)?;
        Ok(())
    }

    pub fn load(&self, password: &str) -> Result<String, WalletPersistError> {
        let contents = fs::read_to_string(&self.file_path)?;
        let file: EncryptedNwcFile = serde_json::from_str(&contents)?;

        let salt = BASE64
            .decode(&file.salt)
            .map_err(|e| WalletPersistError::Crypto(e.to_string()))?;

        let mut key_bytes = [0u8; 32];
        argon2::Argon2::default()
            .hash_password_into(password.as_bytes(), &salt, &mut key_bytes)
            .map_err(|e| WalletPersistError::Crypto(e.to_string()))?;

        let cipher = Aes256Gcm::new_from_slice(&key_bytes)
            .map_err(|e| WalletPersistError::Crypto(e.to_string()))?;
        let nonce_bytes = BASE64
            .decode(&file.nonce)
            .map_err(|e| WalletPersistError::Crypto(e.to_string()))?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = BASE64
            .decode(&file.ciphertext)
            .map_err(|e| WalletPersistError::Crypto(e.to_string()))?;

        let plaintext = cipher
            .decrypt(nonce, ciphertext.as_ref())
            .map_err(|_| WalletPersistError::WrongPassword)?;

        String::from_utf8(plaintext).map_err(|e| WalletPersistError::Crypto(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn save_and_load_round_trip() {
        let dir = tempdir().unwrap();
        let persister = NwcPersister::new(dir.path(), "mainnet");
        let url = "nostr+walletconnect://abc?relay=wss://r&secret=deadbeef";
        persister.save(url, "correcthorse").unwrap();
        assert!(persister.exists());
        let loaded = persister.load("correcthorse").unwrap();
        assert_eq!(loaded, url);
    }

    #[test]
    fn wrong_password_errors() {
        let dir = tempdir().unwrap();
        let persister = NwcPersister::new(dir.path(), "mainnet");
        persister.save("nostr+walletconnect://x", "a").unwrap();
        let err = persister.load("b").unwrap_err();
        assert!(matches!(err, WalletPersistError::WrongPassword));
    }

    #[test]
    fn delete_removes_file() {
        let dir = tempdir().unwrap();
        let persister = NwcPersister::new(dir.path(), "mainnet");
        persister.save("nostr+walletconnect://x", "a").unwrap();
        assert!(persister.exists());
        persister.delete().unwrap();
        assert!(!persister.exists());
    }
}
