# Onboarding UX Standard

Canonical reference for the deadcat onboarding flow. All screens, copy, routing, and state behaviour are documented here. Changes to the onboarding must be reflected in this document.

---

## Overview

Onboarding is a two-step, linear flow. Users cannot enter the app until both steps are complete.

```
Step 1: Identity  →  Step 2: Wallet  →  App
```

The progress indicator (two circles with a connector) is always visible. Step 1 shows a filled circle while active; a checkmark when complete. Step 2 activates when the user advances.

---

## Step 1 — Identity

### 1a. Set up your identity

**Route condition:** `onboardingStep === "nostr"`, `onboardingNostrMode === "generate"`, `onboardingNostrDone === false`

**Eyebrow:** Step 1 of 2  
**Heading:** Set up your identity  
**Body:** deadcat uses Nostr keypairs to publish markets and sign attestations. Generate a new identity or bring your own.

**Actions:**
- Primary: `Generate new identity` → calls `generate_nostr_identity`, then `export_nostr_nsec`; on success advances to 1b (generate)
- Secondary: `Import existing identity` → advances to 1c (import input)

**Back:** No back button on this screen (it is the entry point).

---

### 1b. Back up your secret key (generate flow)

**Route condition:** `onboardingNostrDone === true`, `onboardingNostrMode === "generate"`

**Eyebrow:** Identity created  
**Heading:** Back up your secret key  
**Body:** Your nsec is the only way to prove ownership of markets you create. Store it somewhere safe — it cannot be recovered if lost.

**Warning banner (amber):** Never share your nsec with anyone. Anyone who has it can act as you on Nostr.

**Data rows (divider list):**
- `npub — public key`: full npub in `text-slate-600 mono`, Copy button
- `nsec — secret key`: hidden by default ("Hidden for your protection" in italic); Show button (amber) reveals it; once revealed, Copy button replaces Show. Revealed value in `text-rose-300 mono`.

**Checkbox (gates CTA):** "I have saved my secret key in a safe place"

**CTA:** `Continue to wallet setup` — disabled until checkbox is checked. Calls `onboarding-nostr-continue`, advances to Step 2. No backup scan (new identity has no backup).

**Back:** Returns to 1a. Clears `onboardingNostrMode`, `onboardingNostrGeneratedNsec`, `onboardingNsecRevealed`, `onboardingNsecAcknowledged`.

---

### 1c. Import Nostr identity

**Route condition:** `onboardingNostrMode === "import"`, `onboardingNostrDone === false`

**Eyebrow:** Step 1 of 2  
**Heading:** Import Nostr identity  
**Body:** Paste your existing secret key (nsec) to restore your identity and access markets you've previously created.

**Input:** `id="onboarding-nostr-nsec"` — type text, mono, placeholder `nsec1...`

**Validation (inline, before action):**
- Empty → "Paste an nsec to import."
- Doesn't start with `nsec1` → "Invalid secret key. It should start with nsec1."

**CTA:** `Import & continue` (loading state: "Importing...") — calls `import_nostr_nsec`. On success: sets `onboardingNostrDone = true`, starts fetching Nostr profile in background, renders 1d.

**Back:** Returns to 1a.

---

### 1d. Confirm your identity (import flow)

**Route condition:** `onboardingNostrDone === true`, `onboardingNostrMode === "import"`

**Eyebrow:** Identity imported  
**Heading:** Confirm your identity  
**Body:** Your Nostr identity has been imported. Confirm your details below before continuing.

**Data row (divider list):**
- Single row: avatar (photo if available, fallback person icon) + display name in `text-sm font-semibold text-white` + npub in `text-[11px] text-slate-600 italic mono` (truncated: first 10 chars + `...` + last 8)
- Profile photo and name load asynchronously after import; show fallbacks until resolved.

**No checkbox.** CTA is always enabled for import.

**CTA:** `Continue to wallet setup` — calls `onboarding-nostr-continue`. Advances to Step 2 with backup scan.

**Back:** Returns to 1a. Clears nostr done state.

---

## Step 2 — Wallet

Transition to Step 2 happens via `onboarding-nostr-continue`. For the import flow, a relay backup scan runs immediately on transition and shows a loading screen (2a) before landing on the wallet setup page (2b).

### 2a. Checking for backups (scanning state)

**Route condition:** `onboardingBackupScanning === true`

**Eyebrow:** Please wait  
**Heading:** Checking for backups  
**Body:** Scanning relays for an existing encrypted wallet backup.  
**Visual:** Centred spinner (`animate-spin`, emerald accent).

**No back button** on this screen (scan runs automatically).  
On scan complete: `onboardingBackupScanning = false`. If backup found: `onboardingBackupFound = true`, `onboardingWalletMode = "nostr-restore"`.

---

### 2b. Set up your wallet

**Route condition:** `onboardingStep === "wallet"`, `onboardingWalletMode === "create"`, `onboardingBackupScanning === false`, `onboardingWalletPasswordStep === false`, `onboardingWalletMnemonic === ""`

**Eyebrow:** Step 2 of 2  
**Heading:** Set up your wallet  
**Body:** Create a new Liquid wallet or restore an existing one.

**Backup found card (conditional, import flow only):** Shown when `onboardingBackupFound === true`. Displays relay name + others count (e.g., "relay.damus.io and 2 others"). Action: `onboarding-set-wallet-mode` with `data-mode="nostr-restore"` → advances to 2e.

**Actions:**
- Primary: `Create new wallet` → `onboarding-wallet-continue` with mode `create` → advances to 2c
- Secondary: `Restore from seed` → `onboarding-set-wallet-mode` with `data-mode="restore"` → advances to 2d

**Back:** Returns to Step 1. Resets `onboardingNostrDone = false`, `onboardingNostrMode = "generate"`.

---

### 2c. Save your recovery phrase

**Route condition:** `onboardingWalletMnemonic` is set, `onboardingWalletMode === "create"`, `onboardingMnemonicVerifyStep === false`, `onboardingWalletPasswordStep === false`

**Eyebrow:** Wallet created  
**Heading:** Save your recovery phrase  
**Body:** Write these 12 words down in order and store them somewhere safe. This is the only way to recover your wallet.

**Mnemonic grid:** numbered word grid (12 words). Copy to clipboard button below the grid.

**CTA:** `I've saved my recovery phrase` → sets `onboardingMnemonicVerifyStep = true` → advances to 2f (verify)

**Back:** Returns to 2b (wallet setup main). Clears mnemonic, verify state, passwords.

---

### 2d. Restore from seed

**Route condition:** `onboardingWalletMode === "restore"`, `onboardingWalletPasswordStep === false`

**Eyebrow:** Step 2 of 2  
**Heading:** Restore from seed  
**Body:** Enter your 12-word recovery phrase to restore your existing wallet.

**Input:** `id="onboarding-wallet-mnemonic"` — textarea, mono, placeholder `word1 word2 ... word12`, `rows="3"`, no resize.

**Validation (on action):** Empty → "Please enter your recovery phrase." (BIP39 validity checked by backend.)

**CTA:** `Restore wallet` → `onboarding-wallet-continue` → advances to 2g (password) with mode `restore`

**Back:** Returns to 2b.

---

### 2e. Restore from Nostr backup

**Route condition:** `onboardingWalletMode === "nostr-restore"`, `onboardingWalletPasswordStep === false`

**Eyebrow:** Backup found  
**Heading:** Restore from Nostr backup  
**Body:** An encrypted wallet backup was found. It will be decrypted locally on this device.

**Data row (divider list):**
- `Source`: primary relay hostname (strip `wss://`, trailing slash); if multiple, append "and N other(s)" in `text-slate-600`

**Balance notice card:** "Restore your wallet to view your balance." (No balance is available pre-restore; do not show placeholders.)

**CTA:** `Restore wallet` → `onboarding-wallet-continue` → advances to 2g (password) with mode `nostr-restore`

**Back:** Returns to Step 1 identity confirmation screen (`onboardingStep = "nostr"`, `onboardingNostrDone = true`). Resets `onboardingWalletMode = "create"`, `onboardingBackupFound = false`.

---

### 2f. Confirm your recovery phrase (verify)

**Route condition:** `onboardingWalletMnemonic` is set, `onboardingWalletMode === "create"`, `onboardingMnemonicVerifyStep === true`, `onboardingWalletPasswordStep === false`

**Eyebrow:** Verify backup  
**Heading:** Confirm your recovery phrase  
**Body:** Enter the 3 words below to confirm you've written them down correctly.

**Inputs:** Three word inputs (`id="onboarding-verify-word-0/1/2"`), each labelled with the 1-based word number. No `value=` binding — inputs are uncontrolled to preserve focus on re-render.

**Error:** Displayed inline below inputs when words are incorrect.

**CTA:** `Confirm` → `onboarding-verify-mnemonic`. On success: sets `onboardingWalletPasswordStep = true`, `onboardingMnemonicVerifyStep = false` → advances to 2g. On failure: shows error message, stays on page.

**Back:** Returns to 2c (mnemonic display). Restores `onboardingMnemonicVerifyStep = true → false`.

---

### 2g. Set a password

**Route condition:** `onboardingWalletPasswordStep === true`

**Eyebrow:** Protect your wallet  
**Heading:** Set a password  
**Body:** Your wallet will be encrypted with this password on this device. You'll need it every time you unlock the app.

**Fields:**
- Password: `id="onboarding-wallet-password"`, type password (toggleable via eye icon), `maxlength="32"`, no paste, placeholder "At least 8 characters"
- Confirm password: `id="onboarding-wallet-password-confirm"`, same type, no paste, placeholder "Repeat your password"

**CTA label:** `Create password` (all modes including restore). Loading: "Creating..." (create mode) / "Restoring..." (restore/nostr-restore).

**CTA action per mode:**
- `create` → `onboarding-create-wallet`: calls `restore_wallet(mnemonic, password)` → `unlock_wallet` → `sync_wallet` → `finishOnboarding()`
- `restore` → `onboarding-restore-wallet`: same sequence, mnemonic from user input
- `nostr-restore` → `onboarding-nostr-restore-wallet`: calls `restore_mnemonic_from_nostr()` first, then same sequence

**Validation (all modes):** password required, minimum 8 characters, passwords must match.

**Back behaviour per mode:**
- `create`: returns to 2f (verify step), restores `onboardingMnemonicVerifyStep = true`
- `restore`: returns to 2d (restore from seed)
- `nostr-restore`: returns to 2e (nostr restore sub-page)

---

## Copy rules

| Rule | Detail |
|---|---|
| Eyebrow labels | Sentence case in source; uppercased by CSS. E.g. "Verify backup" not "Verify Backup". |
| Headings | Sentence case. E.g. "Set up your identity." |
| Button labels | Sentence case, imperative verb. E.g. "Generate new identity", "Import & continue". |
| Relay display | Always show hostname (strip `wss://`, trailing slash). If multiple, "relay.name and N other(s)". Never "your relays". |
| Secret key | nsec displayed in `text-rose-300`. Hidden by default. "Show" reveals, "Copy" copies. nsec is never cleared from state after reveal. |
| npub | `text-slate-600 mono`. Not italic in generate flow. Italic and truncated (10+…+8) in import confirmation. |
| Error messages | Inline, `text-red-400`, in a `border-red-500/30 bg-red-500/10` pill. One error shown at a time. |

---

## State reset rules

| Event | State cleared |
|---|---|
| Back from any nostr sub-page to 1a | `onboardingNostrMode`, `onboardingNostrDone`, `onboardingNostrGeneratedNsec`, `onboardingNsecRevealed`, `onboardingNsecAcknowledged`, `onboardingError` |
| Back from wallet setup main to Step 1 | `onboardingNostrDone = false`, `onboardingNostrMode = "generate"`, `onboardingError` |
| Back from any wallet sub-page to 2b | `onboardingWalletMode = "create"`, `onboardingWalletMnemonic`, `onboardingWalletPassword`, `onboardingWalletPasswordConfirm`, `onboardingWalletPasswordStep`, `onboardingMnemonicVerifyStep`, `onboardingMnemonicVerifyIndices`, `onboardingMnemonicVerifyInputs`, `onboardingBackupFound`, `onboardingError` |
| Back from 2g (create) to 2f | `onboardingWalletPasswordStep = false`, `onboardingMnemonicVerifyStep = true`, `onboardingWalletPassword`, `onboardingWalletPasswordConfirm`, `onboardingError` |
| Back from 2g (restore/nostr-restore) to sub-page | `onboardingWalletPasswordStep = false`, `onboardingWalletPassword`, `onboardingWalletPasswordConfirm`, `onboardingError` |
| Back from 2e (nostr-restore) to Step 1 | `onboardingStep = "nostr"`, `onboardingNostrDone = true`, `onboardingWalletMode = "create"`, `onboardingBackupFound = false`, `onboardingError` |
| `onboarding-back` (any page) | Always clears `onboardingPasswordRevealed` |
| `finishOnboarding()` | Clears all onboarding state fields |

---

## Async behaviour

| Action | Async pattern |
|---|---|
| Generate identity | Overlay loader ("Generating...") while calling backend. Error shown in-screen on failure. |
| Import nsec | Button disabled + "Importing..." label. Profile fetched in background after success (no loader). |
| Create/restore wallet | Overlay loader with phase messages: "Creating wallet...", "Unlocking wallet...", "Scanning blockchain...", "Loading markets...". |
| Nostr restore | Overlay loader: "Fetching backup from relays...", "Restoring wallet...", "Unlocking wallet...", "Scanning blockchain...", "Loading markets...". |
| Backup scan | Full-page scanning card (2a) replaces wallet setup during scan. No overlay loader. |
| Profile load (import) | No loader. Name/photo appear when the background fetch resolves; "No display name" shown until then. |
