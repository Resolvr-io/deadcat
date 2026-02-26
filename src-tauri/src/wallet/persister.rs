use std::fs;
use std::path::{Path, PathBuf};

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use zeroize::Zeroizing;

const WALLET_FILE: &str = "wallet_encrypted.json";

#[derive(Error, Debug)]
pub enum WalletPersistError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Crypto error: {0}")]
    Crypto(String),

    #[error("Wrong password")]
    WrongPassword,
}

#[derive(Serialize, Deserialize)]
struct EncryptedWalletFile {
    salt: String,
    nonce: String,
    ciphertext: String,
}

pub struct MnemonicPersister {
    file_path: PathBuf,
    /// Cached mnemonic from a previous successful unlock (cleared on lock).
    /// Wrapped in `Zeroizing` so the backing memory is zeroed on drop/clear.
    cached_mnemonic: Option<Zeroizing<String>>,
}

impl MnemonicPersister {
    pub fn new(app_data_dir: &Path, network: &str) -> Self {
        Self {
            file_path: app_data_dir.join(network).join(WALLET_FILE),
            cached_mnemonic: None,
        }
    }

    pub fn exists(&self) -> bool {
        self.file_path.exists()
    }

    /// Return the cached mnemonic if available (skips Argon2 on repeat unlock).
    pub fn cached(&self) -> Option<&str> {
        self.cached_mnemonic.as_ref().map(|z| z.as_str())
    }

    /// Clear the cached mnemonic (call on lock).
    pub fn clear_cache(&mut self) {
        self.cached_mnemonic = None;
    }

    /// Return the word count of the cached mnemonic (12 or 24).
    pub fn cached_word_count(&self) -> Option<usize> {
        self.cached_mnemonic
            .as_ref()
            .map(|m| m.split_whitespace().count())
    }

    /// Return a single word from the cached mnemonic by zero-based index.
    pub fn cached_word(&self, index: usize) -> Option<&str> {
        self.cached_mnemonic
            .as_ref()
            .and_then(|m| m.split_whitespace().nth(index))
    }

    /// Remove the encrypted wallet file from disk and clear cache.
    pub fn delete(&mut self) -> Result<(), WalletPersistError> {
        if self.file_path.exists() {
            std::fs::remove_file(&self.file_path)?;
        }
        self.cached_mnemonic = None;
        Ok(())
    }

    pub fn save(&self, mnemonic: &str, password: &str) -> Result<(), WalletPersistError> {
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
            .encrypt(nonce, mnemonic.as_bytes())
            .map_err(|e| WalletPersistError::Crypto(e.to_string()))?;

        let file = EncryptedWalletFile {
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

    /// Decrypt and return a single word by index without caching the full mnemonic.
    pub fn load_word(
        &mut self,
        password: &str,
        index: usize,
    ) -> Result<String, WalletPersistError> {
        // If already cached, just return from cache
        if let Some(word) = self.cached_word(index) {
            return Ok(word.to_string());
        }
        // Otherwise decrypt, cache, and return the word
        let _mnemonic = self.load(password)?;
        self.cached_word(index)
            .map(|w| w.to_string())
            .ok_or_else(|| WalletPersistError::Crypto("word index out of range".to_string()))
    }

    /// Decrypt and return the word count without exposing the full mnemonic.
    pub fn load_word_count(&mut self, password: &str) -> Result<usize, WalletPersistError> {
        if let Some(count) = self.cached_word_count() {
            return Ok(count);
        }
        let _mnemonic = self.load(password)?;
        self.cached_word_count()
            .ok_or_else(|| WalletPersistError::Crypto("no mnemonic available".to_string()))
    }

    pub fn load(&mut self, password: &str) -> Result<String, WalletPersistError> {
        let contents = fs::read_to_string(&self.file_path)?;
        let file: EncryptedWalletFile = serde_json::from_str(&contents)?;

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

        let mnemonic_str =
            String::from_utf8(plaintext).map_err(|e| WalletPersistError::Crypto(e.to_string()))?;
        let ret = mnemonic_str.clone();
        self.cached_mnemonic = Some(Zeroizing::new(mnemonic_str));
        Ok(ret)
    }
}
