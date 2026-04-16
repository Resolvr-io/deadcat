# NIP-46 Remote Signing — Remaining Work

Status: Core implementation complete (branch `claude/nip46-remote-signing-BH3xu`).
The items below are polish/hardening tasks to address after manual testing.

---

## 1. auth_url Handler for Browser-Based Approval

**Problem:** Some NIP-46 signers respond to the `connect`
request with an `auth_url` challenge — the user must approve the connection
in a browser before the signer responds with `ack`. Without handling this,
`connect_nip46_bunker` will timeout after 60 seconds.

**Spec:** The `nostr-connect` crate already supports this via the
`AuthUrlHandler` trait:

```rust
#[async_trait]
pub trait AuthUrlHandler: Debug + Send + Sync {
    async fn on_auth_url(&self, auth_url: Url) -> Result<()>;
}
```

**Implementation:**

1. Create a `TauriAuthUrlHandler` struct in `nip46.rs` that holds an
   `AppHandle` and opens the auth URL in the system browser via
   `tauri_plugin_opener::open_url()`.

2. Wire it into `connect_from_bunker_uri`:
   ```rust
   let mut signer = NostrConnect::new(uri, app_keys, timeout, None)?;
   signer.auth_url_handler(TauriAuthUrlHandler { app: app_handle });
   ```

3. Frontend: show a "Waiting for approval in your browser..." message
   while `connect_nip46_bunker` is pending. The loading spinner already
   shows "Connecting..." — enhance with a secondary line after ~5 seconds
   suggesting the user check their signer app.

**Files:** `src-tauri/src/nip46.rs`, `src/components/onboarding/NostrSetupStep.tsx`

---

## 2. Wallet Backup (NIP-44) with Remote Signer

**Problem:** `backup_mnemonic_to_nostr` calls `nip44_encrypt_to_self(&keys, &mnemonic)`
which needs the local secret key. For NIP-46 users this fails because the
secret key isn't available.

**Option A — Route through signer trait (recommended):**

The `NostrConnect` crate implements `NostrSigner::nip44_encrypt`. Change
`backup_mnemonic_to_nostr` to use the signer trait:

```rust
// Instead of:
let encrypted = discovery::nip44_encrypt_to_self(&keys, &mnemonic)?;

// Use:
let (signer, client) = get_signer_and_client(&app).await?;
let pubkey = signer.get_public_key().await.map_err(|e| format!("{e}"))?;
let encrypted = signer.nip44_encrypt(&pubkey, &mnemonic).await
    .map_err(|e| format!("NIP-44 encrypt failed: {e}"))?;
```

This sends the mnemonic plaintext to the remote signer for encryption.
**Security consideration:** the mnemonic is transmitted over the NIP-46
relay channel (encrypted with NIP-44 between app and signer), but the
remote signer sees it. This is acceptable if the user trusts their signer
with the mnemonic — which they should, since the signer holds the nostr
private key.

**Option B — Encrypt locally with app keys:**

Use the ephemeral NIP-46 app keys (stored in `nip46_connection.json`) for
local encryption. The mnemonic never leaves the app. But restoring requires
the same app keys, tying the backup to the specific NIP-46 session.

**Option C — Disable for remote signers:**

Hide the "Back up to Nostr" button in the UI when using NIP-46. Users
rely on `.dcid` file backup or manual mnemonic backup instead.

**Recommendation:** Option A for simplicity. Option C as a quick stopgap.

**Files:** `src-tauri/src/commands.rs` (backup/restore commands),
`src/components/layout/SettingsPanel.tsx` (backup button visibility)

---

## 3. Identity File (.dcid) Export with Remote Signer

**Problem:** The `.dcid` export bundles `nsec + mnemonic + display_name`.
NIP-46 users have no nsec. The export button in the logout flow and
settings will fail.

**Spec:**

1. **Quick fix:** Hide the "Download backup" and "Export identity file"
   buttons when `get_nip46_status` returns a connected session. The
   `.dcid` format is inherently local-keys-only.

2. **Future (.dcid v2):** Extend `IdentityFilePayload` to support:
   ```rust
   pub struct IdentityFilePayloadV2 {
       pub nsec: Option<String>,
       pub mnemonic: Option<String>,
       pub bunker_uri: Option<String>,
       pub display_name: String,
       pub user_pubkey: String,
       pub created_at: String,
   }
   ```
   This allows NIP-46 users to export a file containing the bunker URI +
   mnemonic, which can be restored on another device by reconnecting to
   the same bunker and restoring the wallet.

**Files:** `src-tauri/src/identity_file.rs`, `src-tauri/src/commands.rs`
(export/import commands), `src/components/onboarding/NostrSetupStep.tsx`
(backup download screen after account creation)

---

## 4. NIP-44 Wallet Restore from Nostr with Remote Signer

**Problem:** `restore_mnemonic_from_nostr` calls
`nip44_decrypt_from_self(&keys, &ciphertext)` which needs local keys.

**Spec:** Same approach as item 2 — route through the signer trait:

```rust
let decrypted = signer.nip44_decrypt(&pubkey, &ciphertext).await
    .map_err(|e| format!("NIP-44 decrypt failed: {e}"))?;
```

**Files:** `src-tauri/src/commands.rs` (`restore_mnemonic_from_nostr`)

---

## 5. Relay List Publishing with Remote Signer

**Problem:** `build_relay_list_event(&keys, &relay_list)` in
`src-tauri/src/discovery.rs` takes `&Keys` and calls `.sign_with_keys()`.
The `add_relay`, `remove_relay`, and `set_relay_list` commands load keys
from disk, which fails for NIP-46 users.

**Spec:** Change `build_relay_list_event` to return an `EventBuilder`
(same pattern as the SDK refactor), then sign via the client's signer:

```rust
pub fn build_relay_list_event(relays: &[String]) -> Result<EventBuilder, String> {
    // ... build tags ...
    Ok(EventBuilder::new(Kind::Custom(10002), "").tags(tags))
}
```

Then in the command:
```rust
let (signer, client) = get_signer_and_client(&app).await?;
let builder = discovery::build_relay_list_event(&new_list)?;
let event = builder.sign(&*signer).await.map_err(|e| format!("{e}"))?;
discovery::publish_event(&client, event).await?;
```

**Files:** `src-tauri/src/discovery.rs`, `src-tauri/src/commands.rs`

---

## 6. NIP-98 Auth with Remote Signer

**Problem:** `create_nip98_auth` in `commands.rs` uses `get_keys_and_client`
which requires local keys.

**Spec:** Change to use `get_signer_and_client` and sign the NIP-98 event
via the signer trait instead of `sign_with_keys`.

**Files:** `src-tauri/src/commands.rs`, `src-tauri/src/discovery.rs`

---

## 7. Profile Publishing with Remote Signer

**Problem:** `publish_nostr_profile` uses `get_keys_and_client` for event
signing.

**Spec:** Change to use `get_signer_and_client` + `builder.sign(&*signer).await`.

**Files:** `src-tauri/src/commands.rs`

---

## Future Milestones (Not In Scope)

### Milestone 3: WalletSigner Trait for Hardware Wallets

Abstract the Liquid wallet signer (`lwk_signer::SwSigner`) behind a
`WalletSigner` trait. Concrete implementations: `SoftwareWalletSigner`
(current) and `HardwareWalletSigner` (Jade/Ledger). Separate operational
sub-keys (Boltz swaps, LMSR admin, limit orders) from custody keys.

### Milestone 4: Unified Account Model

`AccountConfig` struct tracking `NostrSignerType` + `WalletSignerType`.
`.dcid` v2 format. Fully decoupled onboarding steps where any nostr
option works with any wallet option.
