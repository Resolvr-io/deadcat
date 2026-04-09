# User Stories: Onboarding, Wallet & Recovery

Cross-cutting stories that apply to all personas. See [ux-design.md](ux-design.md) for persona definitions.

---

## US-OB1: First Launch — Nostr Identity Setup

**As a** new user, **I want to** set up my identity on first launch, **so that** I can discover markets and publish my activity.

**Acceptance criteria**:
- On first launch, if no Nostr identity exists, the onboarding flow starts at the "Nostr" step
- Two options: "Generate new identity" (creates a new keypair) or "Import existing" (paste nsec)
- Generated identity: show the npub, allow revealing the nsec for backup. User must acknowledge they've saved it.
- Imported identity: validate the nsec format, derive the npub, display for confirmation
- After identity setup: fetch NIP-65 relay list and NIP-0 profile metadata in background
- Advance to wallet setup step (US-OB2)

**Interaction design**:
- **Two-step progress**: Visual progress indicator showing "1. Identity → 2. Wallet". The user always completes both steps before entering the app.
- **Generate flow**: Default selection. Show the npub prominently. The nsec is hidden behind a "Reveal" button with a warning: "Save this somewhere safe. It cannot be recovered." Checkbox: "I have saved my secret key" gates the "Continue" button.
- **Import flow**: Single text input for nsec. On paste, immediately validate and show the derived npub. Error state: "Invalid nsec format."
- **No skip**: Identity is required. The Nostr identity is used for market discovery, wallet backup, and key derivation.

---

## US-OB2: First Launch — Wallet Setup

**As a** new user, **I want to** create or restore a Liquid wallet, **so that** I can hold funds and trade.

**Acceptance criteria**:
- Three options: "Create new wallet", "Restore from mnemonic", "Restore from Nostr backup"
- Create: generate a new mnemonic, set a password, display backup words
- Restore from mnemonic: paste 12/24 words, set a password
- Restore from Nostr backup: if `check_nostr_backup` finds a backup on relays, decrypt with the Nostr key and restore automatically
- After wallet creation: transition to the main app (home view)
- Wallet is encrypted at rest with the user's password

**Interaction design**:
- **Nostr backup detection**: Before showing options, the app checks relays for an existing encrypted backup. If found, "Restore from Nostr backup" is pre-selected with a note: "We found an existing wallet backup on your Nostr relays."
- **Mnemonic display**: For new wallets, show the 12 words in a numbered grid. "Write these down" warning. Verification step: ask the user to confirm 3 random words.
- **Password requirements**: Minimum 8 characters. Confirm field. Show strength indicator.
- **Auto-lock**: After wallet creation, configure auto-lock timeout (default: 15 minutes of inactivity). The wallet locks (requires password to unlock) but the Nostr identity persists.

---

## US-W1: View Wallet Balance

**As a** user, **I want to** see my current balance, **so that** I know how much I can trade with.

**Acceptance criteria**:
- Wallet view shows L-BTC balance in the user's preferred denomination (sats or BTC) and fiat equivalent (configurable base currency)
- Token balances: each YES/NO token with market question label (via `identify_asset`), quantity, and current market value
- Pending swaps (Boltz Lightning/Bitcoin) show with status indicators
- Balance updates on each `refreshWallet` cycle

**Interaction design**:
- **Mini wallet**: Compact balance display in the header bar — just the L-BTC amount. Click opens the full wallet view.
- **Balance toggle**: Eye icon to hide/show balance (privacy in public). Persists via `walletBalanceHidden` state.
- **Token section**: Grouped by market. Each market shows YES and NO token holdings side by side, with current probability and estimated value: "10 YES tokens @ 72% = ~36,000 sats potential payout."
- **Unit toggle**: Switch between sats and BTC display. Applies globally.

---

## US-W2: Send Funds

**As a** user, **I want to** send L-BTC to another address or pay a Lightning invoice, **so that** I can transfer funds.

**Acceptance criteria**:
- Send modal with three tabs: Lightning, Liquid, Bitcoin
- Lightning: paste invoice, show decoded amount, send via Boltz submarine swap
- Liquid: paste address, enter amount, send directly via LWK wallet
- Bitcoin: paste address, enter amount, send via Boltz chain swap (Liquid → Bitcoin)
- Fee display before confirmation
- After send: show txid or swap ID with status tracking

**Interaction design**:
- **Tab selection**: Icons for each network. Lightning bolt, Liquid droplet, Bitcoin symbol.
- **Amount input**: For Liquid sends, show available balance and "Max" button. Validate: amount > 0, amount ≤ balance - estimated fee.
- **Confirmation**: Modal showing: recipient (truncated address/invoice), amount, network fee, total deducted. "Confirm Send" button.
- **Status tracking**: For Boltz swaps, show real-time status updates (lockup confirmed, claim pending, completed). For Liquid sends, show pending → confirmed.

---

## US-W3: Receive Funds

**As a** user, **I want to** receive L-BTC via various methods, **so that** I can fund my wallet for trading.

**Acceptance criteria**:
- Receive modal with three tabs: Lightning, Liquid, Bitcoin
- Lightning: enter amount → create Boltz reverse swap → show Lightning invoice + QR
- Liquid: generate a receive address from LWK wallet → show address + QR
- Bitcoin: enter amount → create Boltz chain swap (Bitcoin → Liquid) → show Bitcoin address + QR
- Auto-detect when payment arrives and show success

**Interaction design**:
- **QR code**: Large, centered QR code with the address/invoice below (truncated, with copy button).
- **Amount input**: For Lightning and Bitcoin swaps, amount is required (Boltz needs it upfront). For Liquid, amount is optional (any amount accepted).
- **Fee disclosure**: For swap methods, show Boltz fees upfront: "Service fee: X sats. You'll receive: Y sats."
- **Expiry timer**: Lightning invoices and Boltz swaps expire. Show countdown timer. On expiry: "Invoice expired. Generate a new one?"

---

## US-W4: Lock and Unlock Wallet

**As a** user, **I want to** lock my wallet when away and unlock with my password, **so that** my funds are protected.

**Acceptance criteria**:
- Auto-lock after configurable inactivity timeout (default 15 min)
- Manual lock via user menu → "Log out"
- Locked state: show lock icon, password input, "Unlock" button
- Unlock: verify password, decrypt wallet, resume normal operation
- While locked: Nostr identity persists (markets are still visible), but wallet operations are disabled

**Interaction design**:
- **Locked view**: Centered lock icon with password field. No wallet data visible. Market browsing still works (read-only from Nostr).
- **Activity tracking**: Mouse, keyboard, scroll, and click events reset the inactivity timer. Throttled to one reset per 30 seconds to avoid excessive processing.
- **Wrong password**: Shake animation on the input field. "Incorrect password" message. No lockout (the encryption key is derived from the password — wrong password simply fails to decrypt).

---

## US-R1: Wallet Recovery Flow

**As a** user who lost their device, **I want to** recover my full wallet state from my mnemonic, **so that** I can regain access to all my funds and positions.

**Acceptance criteria**:
- Mnemonic restore triggers: standard wallet rescan (finds L-BTC + token UTXOs), then deadcat-specific recovery
- Market positions: token asset IDs → `issuance_transaction` → market creation tx → OP_RETURN → reconstruct params → `ingest_market`
- Maker orders: scan wallet-funded transactions for OP_RETURN hints → reconstruct `MakerOrderParams` via `derive_order_params` → `ingest_order`
- Pool positions: scan wallet-funded transactions for OP_RETURN hints → reconstruct `LmsrPoolParams` via `derive_pool_params` → `ingest_pool`
- All recovery is chain-only — no Nostr dependency for fund recovery

**Interaction design**:
- **Progress indicator**: Recovery can take a few minutes (chain scanning). Show a progress bar with status messages: "Scanning wallet addresses...", "Identifying token assets...", "Recovering market positions...", "Recovering maker orders...", "Recovering pool positions..."
- **Recovery summary**: After completion, show what was found: "Recovered: 3 markets, 12 token positions, 2 maker orders, 1 pool. Total estimated value: X sats."
- **Partial recovery**: If some positions can't be recovered (e.g., chain source unavailable), show what succeeded and what needs manual attention.
