# Onboarding UX Standard

Canonical reference for the deadcat onboarding flow. All screens, copy, routing, and state behaviour are documented here. Changes to the onboarding must be reflected in this document.

---

## Overview

Onboarding is a sequential two-step flow, triggered at the point of need when the user attempts an action that requires it. Neither step is shown on first launch; the app opens directly to the home view in guest mode. See [ux-first-use.md](ux-first-use.md) for the guest mode specification.

```
Step 1 (Identity):  1a → 1b (generate) or 1c → 1d (import)
                         ↓ "Continue to wallet setup" — user becomes signed in here
Step 2 (Wallet):    2a (scan) → 2b → 2c/2d/2e/2f/2g → done
```

- **Step 1 — Identity modal**: Handles Nostr keypair generation or import only. Does not create a wallet, and does **not** sign the user in. The user is signed in only when they click "Continue to wallet setup" (`onboarding-nostr-continue`). If they go back before clicking that button, the generated/imported identity is discarded and the backend identity is deleted.
- **Step 2 — Wallet modal**: Requires a Nostr identity to already exist. Always begins with a backup scan (2a) before presenting setup options.

**Exception — wallet-only modal**: When a user who is already signed in opens wallet setup (via the "Set up wallet" header button, or by triggering any wallet-requiring action while signed in), the wallet step opens on its own — Step 1 is skipped, and `state.onboardingWalletOnly` is set to `true`. In this mode the step indicator and "Step 2 of 2" eyebrow are hidden. The wallet modal never navigates back into the identity modal regardless of `onboardingWalletOnly`.

### Step Indicator

The two-circle progress indicator (connected with a line, filled circle active, checkmark complete) is controlled by `state.onboardingWalletOnly`:
- **`false`** (default): user arrived at Step 2 by clicking "Continue to wallet setup" after completing Step 1 in this session. Step indicator is **shown**.
- **`true`**: user is already signed in and opened wallet setup directly (via header button or action guard). Step indicator is **hidden** — there is no Step 1 context to show progress against.

---

## Step 1 — Identity

### 1a. Set up your identity

**Route condition:** `onboardingStep === "nostr"`, `onboardingNostrMode === "generate"`, `onboardingNostrDone === false`

**Eyebrow:** Step 1 of 2  
**Heading:** Set up your identity  
**Body:** deadcat uses Nostr keypairs to publish markets and sign attestations. Generate a new identity or bring your own.

**Actions:**
- Primary: `Generate new identity` → calls `generate_nostr_identity`, then `export_nostr_nsec`; on success stores identity in `onboardingPendingPubkey`/`onboardingPendingNpub` (not yet committed to `nostrPubkey`/`nostrNpub`) and advances to 1b
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
- `npub — public key`: value read from `onboardingPendingNpub`, in `text-slate-600 mono`, Copy button
- `nsec — secret key`: hidden by default ("Hidden for your protection" in italic); Show button (amber) reveals it; once revealed, Copy button replaces Show. Revealed value in `text-rose-300 mono`.

**Checkbox (gates CTA):** "I have saved my secret key in a safe place" — rendered as a custom dark checkbox. The visual is a `flex h-4 w-4` span: when unchecked, `border border-slate-600 bg-slate-800`; when checked, `bg-emerald-400` with a white checkmark SVG. A native `<input type="checkbox" class="sr-only">` is used for accessibility. State is held in `onboardingNsecAcknowledged`.

**CTA:** `Continue to wallet setup` — disabled until checkbox is checked. On click:
1. Commits `onboardingPendingPubkey` → `state.nostrPubkey` and `onboardingPendingNpub` → `state.nostrNpub` — **this is the moment the user becomes signed in**
2. Clears `onboardingNsecAcknowledged` and `onboardingNsecRevealed` so checkbox state does not persist if the user navigates back in a future session
3. Clears pending fields
4. Advances to Step 2 (no backup scan for new-identity users)

**Back:** Returns to 1a. Discards the generated identity: clears `onboardingPendingPubkey`, `onboardingPendingNpub`, `onboardingNostrGeneratedNsec`, `onboardingNsecRevealed`, `onboardingNsecAcknowledged`, `onboardingNostrMode`, `onboardingNostrDone`. Calls `delete_nostr_identity` on the backend (fire-and-forget) to clean up the in-memory identity that was not committed.

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

**CTA:** `Import & continue` (loading state: "Importing...") — calls `import_nostr_nsec`. On success: stores identity in `onboardingPendingPubkey`/`onboardingPendingNpub` (not yet committed), sets `onboardingNostrDone = true`, starts fetching Nostr profile in background, renders 1d.

**Back:** Returns to 1a. Discards the imported identity: clears `onboardingPendingPubkey`, `onboardingPendingNpub`, `onboardingNostrDone`, `onboardingNostrMode`. Calls `delete_nostr_identity` on the backend.

---

### 1d. Confirm your identity (import flow)

**Route condition:** `onboardingNostrDone === true`, `onboardingNostrMode === "import"`

**Eyebrow:** Identity imported  
**Heading:** Confirm your identity  
**Body:** Your Nostr identity has been imported. Confirm your details below before continuing.

**Data row (divider list):**
- Single row: avatar (photo if available, fallback person icon) + display name in `text-sm font-semibold text-white` + npub (from `onboardingPendingNpub`) in `text-[11px] text-slate-600 italic mono` (truncated: first 10 chars + `...` + last 8)
- Profile photo and name load asynchronously after import; show fallbacks until resolved.

**No checkbox.** CTA is always enabled for import.

**CTA:** `Continue to wallet setup` — commits pending identity to `nostrPubkey`/`nostrNpub` (**user is now signed in**), clears pending fields, advances to Step 2 with backup scan.

**Back:** Returns to 1a. Discards pending identity, calls `delete_nostr_identity`.

---

## Step 2 — Wallet

Transition to Step 2 happens either:
- Via `onboarding-nostr-continue` (end of identity modal) — commits pending identity, sets `onboardingWalletOnly = false`
- Via `openWalletSetupModal()` — signed-in user opens wallet setup (header button or action guard). Sets `onboardingWalletOnly = true`. Skips Step 1 entirely.

A backup scan **always runs** at the start of the wallet modal. The scan shows the loading screen (2a) before landing on the wallet setup page (2b) or restore page (2e).

### 2a. Checking for backups (scanning state)

**Route condition:** `onboardingBackupScanning === true`

**Eyebrow:** Please wait  
**Heading:** Checking for backups  
**Body:** Scanning relays for an existing encrypted wallet backup.  
**Visual:** Centred spinner (`animate-spin`, emerald accent).

**No back button** on this screen (scan runs automatically).  
On scan complete: `onboardingBackupScanning = false`. If backup found: `onboardingBackupFound = true`, `onboardingWalletMode = "nostr-restore"`, `onboardingSelectedWalletDTag` set to first wallet's d-tag, advances to 2e. Otherwise advances to 2b.

---

### 2b. Set up your wallet

**Route condition:** `onboardingStep === "wallet"`, `onboardingWalletMode === "create"`, `onboardingBackupScanning === false`, `onboardingWalletPasswordStep === false`, `onboardingWalletMnemonic === ""`

**Eyebrow:** Step 2 of 2 — hidden when `onboardingWalletOnly === true`  
**Heading:** Set up your wallet  
**Body:** Create a new Liquid wallet or restore an existing one.

**Backup found card (conditional):** Shown when `onboardingBackupFound === true`. Displays the wallet name if one wallet found, or "N wallet backups found" if multiple. Clicking it sets `onboardingWalletMode = "nostr-restore"` → advances to 2e.

**Actions:**
- Primary: `Create new wallet` → `onboarding-wallet-continue` with mode `create` → advances to 2c
- Secondary: `Restore from seed` → `onboarding-set-wallet-mode` with `data-mode="restore"` → advances to 2d

**Back:** No back button. The wallet modal never navigates back into the identity modal.

**Note:** When `onboarding-set-wallet-mode` is called with `data-mode="create"` (e.g. "Set up a new wallet" from 2e), `onboardingBackupFound` is also cleared so the backup card does not reappear.

---

### 2c. Save your recovery phrase

**Route condition:** `onboardingWalletMnemonic` is set, `onboardingWalletMode === "create"`, `onboardingMnemonicVerifyStep === false`, `onboardingWalletPasswordStep === false`

**Eyebrow:** Wallet created  
**Heading:** Save your recovery phrase  
**Body:** Write these 12 words down in order and store them somewhere safe. This is the only way to recover your wallet.

**Mnemonic display:** Divider-separated rows, 3 words per row. Each word is rendered as a number prefix (`text-xs text-slate-500`) and word (`mono text-sm text-slate-100`) with a `flex items-baseline gap-1.5` layout. Rows are separated by `border-t border-slate-700/60` dividers. Copy to clipboard button below.

**CTA:** `I've saved my recovery phrase` → sets `onboardingMnemonicVerifyStep = true` → advances to 2f (verify)

**Back:** Returns to 2b (wallet setup main). Clears mnemonic, verify state, passwords.

---

### 2d. Restore from seed

**Route condition:** `onboardingWalletMode === "restore"`, `onboardingWalletPasswordStep === false`

**Eyebrow:** Step 2 of 2 — hidden when `onboardingWalletOnly === true`  
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
**Body:** Select a wallet to restore.

**Wallet cards:** Each wallet found in the backup scan is shown as a selectable card. Cards are always shown — even for a single wallet. Each card shows:
- Wallet icon (emerald when selected, dim otherwise)
- Wallet name as primary text (emerald when selected, slate-300 otherwise)
- Primary relay hostname as subtitle (e.g. `relay.primal.net` or `relay.primal.net and 1 other`)
- Checkmark on the selected card

Clicking a card sets `onboardingSelectedWalletDTag` to that wallet's d-tag. If only one wallet exists it renders as pre-selected. The selected wallet's d-tag is passed to `restore_mnemonic_from_nostr` at 2g.

**CTA:** `Restore wallet` → `onboarding-wallet-continue` → advances to 2g (password) with mode `nostr-restore`

**Navigation:**
- Secondary button `Set up a new wallet` is **always shown** below the primary CTA. Clicking it sets `onboardingWalletMode = "create"`, clears `onboardingBackupFound`, and navigates to 2b.
- **No back button** on this screen regardless of how the user arrived. The wallet modal never navigates back into the identity modal.

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
**Body:** Your wallet will be encrypted with this password on this device. You'll need it every time you open deadcat to unlock your wallet.

**Fields:**
- Password: `id="onboarding-wallet-password"`, type password (toggleable via eye icon), `maxlength="32"`, no paste, placeholder "At least 8 characters"
- Confirm password: `id="onboarding-wallet-password-confirm"`, same type, no paste, placeholder "Repeat your password"

**CTA label:** `Create password` (all modes including restore). Loading: "Creating..." (create mode) / "Restoring..." (restore/nostr-restore).

**CTA action per mode:**
- `create` → `onboarding-create-wallet`: calls `restore_wallet(mnemonic, password)` → `unlock_wallet` → `sync_wallet` → `finishOnboarding()`
- `restore` → `onboarding-restore-wallet`: same sequence, mnemonic from user input
- `nostr-restore` → `onboarding-nostr-restore-wallet`: calls `restore_mnemonic_from_nostr(walletName)` first (using the selected wallet's name from `onboardingSelectedWalletDTag`), then same sequence

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
| `onboarding-generate-nostr` success | Sets `onboardingPendingPubkey`, `onboardingPendingNpub`, `onboardingNostrGeneratedNsec` — does **not** set `nostrPubkey`/`nostrNpub` |
| `onboarding-import-nostr` success | Sets `onboardingPendingPubkey`, `onboardingPendingNpub` — does **not** set `nostrPubkey`/`nostrNpub` |
| `onboarding-nostr-continue` | Commits `onboardingPendingPubkey` → `nostrPubkey`, `onboardingPendingNpub` → `nostrNpub`. Clears pending fields, `onboardingNsecAcknowledged`, `onboardingNsecRevealed`. **User is now signed in.** |
| Back from any nostr sub-page to 1a | Clears `onboardingNostrMode`, `onboardingNostrDone`, `onboardingNostrGeneratedNsec`, `onboardingNsecRevealed`, `onboardingNsecAcknowledged`, `onboardingPendingPubkey`, `onboardingPendingNpub`, `onboardingError`. Calls `delete_nostr_identity` on backend. |
| `openWalletSetupModal()` called | Sets `onboardingWalletOnly = true`, `onboardingSelectedWalletDTag` to first found wallet's d-tag after scan |
| `onboarding-nostr-continue` fired | Sets `onboardingWalletOnly = false` — user just completed Step 1, full progress UI shown |
| `onboarding-set-wallet-mode` with `data-mode="create"` | Clears `onboardingBackupFound = false` so backup card does not reappear on 2b |
| Back from wallet setup main (2b) to Step 1 | Not possible — the wallet modal never navigates back to the identity modal. |
| Back from any wallet sub-page to 2b | `onboardingWalletMode = "create"`, `onboardingWalletMnemonic`, `onboardingWalletPassword`, `onboardingWalletPasswordConfirm`, `onboardingWalletPasswordStep`, `onboardingMnemonicVerifyStep`, `onboardingMnemonicVerifyIndices`, `onboardingMnemonicVerifyInputs`, `onboardingBackupFound`, `onboardingError` |
| Back from 2g (create) to 2f | `onboardingWalletPasswordStep = false`, `onboardingMnemonicVerifyStep = true`, `onboardingWalletPassword`, `onboardingWalletPasswordConfirm`, `onboardingError` |
| Back from 2g (restore/nostr-restore) to sub-page | `onboardingWalletPasswordStep = false`, `onboardingWalletPassword`, `onboardingWalletPasswordConfirm`, `onboardingError` |
| Back from 2e (nostr-restore) | Not possible — no back button on 2e. Use "Set up a new wallet" to navigate to 2b instead. |
| "Set up a new wallet" from 2e (any mode) | `onboardingWalletMode = "create"`, `onboardingBackupFound = false`, `onboardingError` — navigates to 2b. |
| `onboarding-back` (any page) | Always clears `onboardingPasswordRevealed` |
| `finishOnboarding()` | Clears all onboarding state fields, sets `setupModalOpen = false`, `setupRequires = null`, `onboardingWalletOnly = false`, `onboardingPendingPubkey = ""`, `onboardingPendingNpub = ""` |
| Logout | Deletes wallet file from disk (`delete_wallet`), sets `walletStatus = "not_created"`, clears Nostr identity. No wallet or key material persists. |

---

## Async behaviour

| Action | Async pattern |
|---|---|
| Generate identity | Overlay loader ("Generating...") while calling backend. Identity stored in pending fields on success. Error shown in-screen on failure. |
| Import nsec | Button disabled + "Importing..." label. Identity stored in pending fields on success. Profile fetched in background after success (no loader). |
| Create/restore wallet | Overlay loader with phase messages: "Creating wallet...", "Unlocking wallet...", "Scanning blockchain...", "Loading markets...". |
| Nostr restore | Overlay loader: "Fetching backup from relays...", "Restoring wallet...", "Unlocking wallet...", "Scanning blockchain...", "Loading markets...". Uses selected wallet name to fetch the correct named backup. |
| Backup scan | Full-page scanning card (2a) — shown at the start of every wallet modal open. Blocks progression until complete. No overlay loader. |
| Profile load (import) | No loader. Name/photo appear when the background fetch resolves; "No display name" shown until then. |
