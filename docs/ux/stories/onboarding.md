# User Stories: Onboarding, Wallet & Recovery

Cross-cutting stories that apply to all personas. See [ux-design.md](../design.md) for persona definitions.

**Important**: Identity and wallet setup are **not** forced on first launch. The app opens directly to the home view in guest mode. Setup is triggered when the user attempts an action that requires it. See [ux-first-use.md](../first-use.md) for the guest mode specification and deferred setup triggers.

---

## US-OB1: Nostr Identity Setup (Deferred)

**As a** user who wants to publish content (create markets, place orders, announce pools), **I want to** set up my Nostr identity, **so that** my activity is attributed to me and discoverable by others.

**Trigger**: User attempts an action requiring Nostr publishing (create market, place limit order, create pool, resolve market). NOT triggered on first launch.

**Acceptance criteria**:
- Identity setup opens as a **modal overlay** (not a full-page takeover), preserving the user's current context
- Two options: "Generate new identity" (creates a new keypair) or "Import existing" (paste nsec)
- Generated identity: show the npub, allow revealing the nsec for backup. User must acknowledge they've saved it.
- Imported identity: validate the nsec format, derive the npub, display for confirmation
- After identity setup: fetch NIP-65 relay list and NIP-0 profile metadata in background
- If the triggering action also requires a wallet, advance to wallet setup (US-OB2) before returning to the action
- After setup completes, the user is returned to exactly where they were with their prior inputs preserved

**Interaction design**:
- **Modal, not page**: The setup modal overlays the current view. The user can see the market they were looking at behind the modal. This maintains motivation and context.
- **Generate flow**: Default selection. Show the npub prominently. The nsec is hidden behind a "Reveal" button with a warning: "Save this somewhere safe. It cannot be recovered." Checkbox: "I have saved my secret key" gates the "Continue" button.
- **Import flow**: Single text input for nsec. On paste, immediately validate and show the derived npub. Error state: "Invalid nsec format."
- **Context preservation**: `state.selectedMarketId`, `state.selectedSide`, `state.tradeSizeSats`, `state.limitPrice`, and `state.view` are all preserved across the setup flow.

---

## US-OB2: Wallet Setup (Deferred, Identity Required)

**As a** user who wants to trade, send, or receive funds, **I want to** create or restore a Liquid wallet, **so that** I can hold funds and execute transactions.

**Trigger**: User attempts an action requiring a wallet (execute trade, send/receive funds, issue tokens, redeem). NOT triggered on first launch. If the user has no Nostr identity, the identity modal runs first; the wallet step follows automatically. If the user is already signed in but has no wallet, the wallet step opens directly (skipping identity setup). The "Set up wallet" button in the header for a signed-in user with no wallet also opens it directly.

**Acceptance criteria**:
- Wallet setup opens as a **modal overlay**, preserving the user's current context
- Requires a Nostr identity — guests see the identity modal first; wallet setup is only reached once an identity exists
- Always begins with a backup scan (2a loading state) before presenting any options
- Options after scan: "Create new wallet" | "Restore from seed" | "Restore from Nostr backup" (if backup found)
- Create: generate a new mnemonic, set a password, display backup words
- Restore from seed: paste 12/24 words, set a password
- Restore from Nostr backup: if `check_nostr_backup` finds a backup on relays, the backup-found page is shown automatically; user enters password to decrypt
- After wallet creation: modal closes, user returns to their prior context with the action button now enabled
- Wallet is encrypted at rest with the user's password
- On logout: wallet file is deleted from disk. No wallet data or password persists after logout. On next sign-in, the wallet modal starts fresh (backup scan, then create or restore).

**Interaction design**:
- **Nostr backup detection**: The modal always runs a backup scan immediately on open, showing a spinner (2a). This is a blocking step — the user waits for the scan to complete before seeing any options. If a backup is found, the "Restore from Nostr backup" page is shown directly. If not, the main setup page (create / restore from seed) is shown.
- **No step indicator when signed in**: The two-circle step indicator shown during the combined identity+wallet flow is hidden when the wallet modal is opened independently by a signed-in user. "Step 2 of 2" eyebrow labels are also hidden. There is no back button to the identity step.
- **Mnemonic display**: For new wallets, show the 12 words in divider-separated rows (3 words per row), each word with a number prefix. "Write these down" warning. Verification step: ask the user to confirm 3 random words.
- **Password requirements**: Minimum 8 characters. Confirm field.
- **Auto-lock**: After wallet creation, configure auto-lock timeout (default: 15 minutes of inactivity). The wallet locks (requires password to unlock) but the Nostr identity persists.
- **Return to action**: After wallet creation completes, the modal closes and the trade/send/receive action that triggered it is now available. The user does not need to re-navigate or re-enter parameters.

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
