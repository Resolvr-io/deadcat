use std::fs;
use std::path::{Path, PathBuf};

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde::{Deserialize, Serialize};
use thiserror::Error;

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
}

impl MnemonicPersister {
    pub fn new(app_data_dir: &Path, network: &str) -> Self {
        Self {
            file_path: app_data_dir.join(network).join(WALLET_FILE),
        }
    }

    pub fn exists(&self) -> bool {
        self.file_path.exists()
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

    pub fn load(&self, password: &str) -> Result<String, WalletPersistError> {
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

        String::from_utf8(plaintext).map_err(|e| WalletPersistError::Crypto(e.to_string()))
    }
}
