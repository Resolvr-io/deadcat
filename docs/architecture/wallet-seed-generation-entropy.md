# Deadcat Wallet Seed Generation Entropy - Technical Explanation

## Overview

Deadcat is a Tauri-based desktop application for trading prediction markets on the Liquid Network. It implements a non-custodial wallet with industry-standard entropy sources and encryption practices for seed generation and storage.

---

## Entropy Sources

### 1. Primary Entropy: BIP39 Mnemonic Generation

**Location**: `src-tauri/crates/deadcat-sdk/src/sdk.rs` (lines 454-458)

```rust
pub fn generate_mnemonic(is_mainnet: bool) -> Result<(String, SwSigner)> {
    let (signer, mnemonic) =
        SwSigner::random(is_mainnet).map_err(|e| Error::Signer(e.to_string()))?;
    Ok((mnemonic.to_string(), signer))
}
```

**How it works**:
- Calls `SwSigner::random()` from Blockstream's Liquid Wallet Kit (`lwk_signer 0.14`)
- The `lwk_signer` library uses Rust's `rand` crate (v0.8) internally
- The `rand` crate sources entropy from the operating system:
  - **Linux**: `getrandom` syscall or `/dev/urandom`
  - **macOS**: `SecRandomCopyBytes` (Security framework)
  - **Windows**: `RtlGenRandom` (BCryptGenRandom)
- Generates **128 bits** of entropy for a 12-word mnemonic (or 256 bits for 24 words)
- The entropy is converted to a BIP39 mnemonic using the standard word list

### 2. Encryption Entropy: Salt and Nonce Generation

**Location**: `src-tauri/src/wallet/persister.rs` (lines 88-116)

When the mnemonic is saved to disk, two additional random values are generated:

```rust
pub fn save(&self, mnemonic: &str, password: &str) -> Result<(), WalletPersistError> {
    let salt: [u8; 16] = rand::random();  // 128 bits of random salt
    
    // ... Argon2 key derivation ...
    
    let nonce_bytes: [u8; 12] = rand::random();  // 96 bits of random nonce
    // ... AES-256-GCM encryption ...
}
```

| Random Value | Size | Purpose |
|--------------|------|---------|
| Salt | 16 bytes (128 bits) | Argon2 key derivation function input |
| Nonce | 12 bytes (96 bits) | AES-GCM initialization vector |

Both use `rand::random()` which provides cryptographically secure random bytes from the OS.

---

## Seed-to-Key Derivation Process

### Step 1: Entropy to Mnemonic (BIP39)
1. 128 bits of random entropy are generated
2. A checksum (4 bits for 12 words) is appended
3. The 132 bits are split into 12 groups of 11 bits each
4. Each 11-bit value indexes into the BIP39 word list (2048 words)
5. Result: 12 human-readable words

### Step 2: Mnemonic to Seed (BIP39)
1. The mnemonic words + optional passphrase are fed into PBKDF2-HMAC-SHA512
2. 2048 iterations produce a 512-bit seed

### Step 3: Seed to HD Keys (BIP32)
1. The seed is used to derive a master private key and chain code
2. Hierarchical derivation creates child keys for different purposes
3. Liquid-specific derivation paths are used (via `lwk_wollet`)

---

## Mnemonic Encryption at Rest

When the wallet is created, the mnemonic is encrypted before storage:

### Encryption Pipeline

```
User Password
     │
     ▼
┌─────────────────────┐
│  Argon2 KDF         │ ← Random 16-byte salt
│  (memory-hard)      │
└─────────────────────┘
     │
     ▼
256-bit AES Key
     │
     ▼
┌─────────────────────┐
│  AES-256-GCM        │ ← Random 12-byte nonce
│  (authenticated)    │
└─────────────────────┘
     │
     ▼
Encrypted Mnemonic + Auth Tag
```

### Storage Format
Stored in `{app_data_dir}/{network}/wallet_encrypted.json`:
```json
{
  "salt": "base64-encoded-16-bytes",
  "nonce": "base64-encoded-12-bytes", 
  "ciphertext": "base64-encoded-encrypted-mnemonic"
}
```

---

## Cryptographic Library Versions

| Library | Version | Purpose |
|---------|---------|---------|
| `rand` | 0.8 | Cryptographic random number generation |
| `lwk_signer` | 0.14 | Blockstream's Liquid Wallet Kit (BIP39/BIP32) |
| `bip39` | 2.x | BIP39 mnemonic validation |
| `argon2` | 0.5 | Memory-hard password key derivation |
| `aes-gcm` | 0.10 | Authenticated encryption |

---

## Security Characteristics

### Strengths
1. **OS-level entropy**: Uses cryptographically secure random number generators from the operating system kernel
2. **Memory-hard KDF**: Argon2 resists brute-force attacks with GPU/ASIC hardware
3. **Authenticated encryption**: AES-GCM provides both confidentiality and integrity
4. **Unique salt per wallet**: Prevents rainbow table attacks
5. **Secure memory clearing**: Uses `Zeroizing<String>` wrapper to clear mnemonic from memory
6. **Industry-standard derivation**: BIP39/BIP32 compatibility allows recovery in other wallets

### Considerations
- Password minimum is 8 characters (enforced in UI) with no complexity requirements
- Entropy quality depends entirely on the underlying OS random source
- No hardware security module (HSM) integration for key storage

---

## Entropy Summary Table

| Stage | Source | Entropy (bits) | Purpose |
|-------|--------|----------------|---------|
| Mnemonic | OS RNG via `lwk_signer` | 128 (12 words) | Wallet master seed |
| Encryption salt | OS RNG via `rand::random()` | 128 | Argon2 KDF salt |
| Encryption nonce | OS RNG via `rand::random()` | 96 | AES-GCM IV |
| Address derivation | Deterministic from seed | N/A | HD key paths |

---

## Key Files

- **Mnemonic generation**: `src-tauri/crates/deadcat-sdk/src/sdk.rs:454-458`
- **Encryption/save**: `src-tauri/src/wallet/persister.rs:88-116`
- **Decryption/load**: `src-tauri/src/wallet/persister.rs:145-177`
- **Tauri commands**: `src-tauri/src/lib.rs:183-280`
