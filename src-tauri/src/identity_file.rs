//! Encrypted Deadcat identity file (.dcid).
//!
//! Format: JSON envelope with Argon2-derived AES-256-GCM encryption.
//! Contains the Nostr nsec + wallet mnemonic + display name.

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde::{Deserialize, Serialize};

const IDENTITY_FILE_VERSION: u8 = 1;

/// The encrypted file envelope (written to disk / downloaded).
#[derive(Serialize, Deserialize)]
pub struct IdentityFileEnvelope {
    pub app: String,
    pub url: String,
    pub description: String,
    pub version: u8,
    pub salt: String,
    pub nonce: String,
    pub ciphertext: String,
}

/// The plaintext payload inside the encrypted envelope.
#[derive(Serialize, Deserialize)]
pub struct IdentityFilePayload {
    pub nsec: String,
    pub mnemonic: String,
    pub display_name: String,
    pub created_at: String,
}

/// Encrypt an identity payload with a password, returning the JSON envelope string.
pub fn export_identity_file(
    payload: &IdentityFilePayload,
    password: &str,
) -> Result<String, String> {
    let plaintext =
        serde_json::to_string(payload).map_err(|e| format!("serialize payload: {e}"))?;

    let salt: [u8; 16] = rand::random();
    let mut key_bytes = [0u8; 32];
    argon2::Argon2::default()
        .hash_password_into(password.as_bytes(), &salt, &mut key_bytes)
        .map_err(|e| format!("argon2 KDF: {e}"))?;

    let cipher =
        Aes256Gcm::new_from_slice(&key_bytes).map_err(|e| format!("cipher init: {e}"))?;
    let nonce_bytes: [u8; 12] = rand::random();
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("encrypt: {e}"))?;

    let envelope = IdentityFileEnvelope {
        app: "Deadcat Live".to_string(),
        url: "https://deadcat.live/".to_string(),
        description: "Encrypted Deadcat Live identity backup. Contains Nostr keys and wallet recovery phrase, protected by your account password. Download Deadcat Live at https://deadcat.live/ to restore your account.".to_string(),
        version: IDENTITY_FILE_VERSION,
        salt: BASE64.encode(salt),
        nonce: BASE64.encode(nonce_bytes),
        ciphertext: BASE64.encode(ciphertext),
    };

    serde_json::to_string_pretty(&envelope).map_err(|e| format!("serialize envelope: {e}"))
}

/// Decrypt an identity file envelope with a password, returning the payload.
pub fn import_identity_file(
    envelope_json: &str,
    password: &str,
) -> Result<IdentityFilePayload, String> {
    let envelope: IdentityFileEnvelope =
        serde_json::from_str(envelope_json).map_err(|e| format!("invalid identity file: {e}"))?;

    if envelope.version != IDENTITY_FILE_VERSION {
        return Err(format!(
            "unsupported identity file version: {}",
            envelope.version
        ));
    }

    let salt = BASE64
        .decode(&envelope.salt)
        .map_err(|e| format!("decode salt: {e}"))?;
    let mut key_bytes = [0u8; 32];
    argon2::Argon2::default()
        .hash_password_into(password.as_bytes(), &salt, &mut key_bytes)
        .map_err(|e| format!("argon2 KDF: {e}"))?;

    let cipher =
        Aes256Gcm::new_from_slice(&key_bytes).map_err(|e| format!("cipher init: {e}"))?;
    let nonce_bytes = BASE64
        .decode(&envelope.nonce)
        .map_err(|e| format!("decode nonce: {e}"))?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = BASE64
        .decode(&envelope.ciphertext)
        .map_err(|e| format!("decode ciphertext: {e}"))?;

    let plaintext = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|_| "wrong password or corrupted file".to_string())?;

    let payload_str =
        String::from_utf8(plaintext).map_err(|e| format!("invalid UTF-8: {e}"))?;

    serde_json::from_str(&payload_str).map_err(|e| format!("invalid payload: {e}"))
}
