# First Use Experience

The first-use experience prioritizes **zero-friction exploration**. A new user should see live markets within seconds of launching the app — no identity setup, no wallet creation, no password. Setup is deferred to the moment it's actually needed: when the user tries to trade, send, or receive funds.

This document specifies the guest mode, the deferred setup triggers, and the progressive upgrade path from anonymous browser to active trader.

---

## Design Principle: Browse First, Register When Needed

Traditional onboarding forces the user through identity + wallet setup before they see any content. This creates two problems:

1. **Drop-off**: Users who just want to explore leave before seeing a single market.
2. **Unmotivated setup**: Creating a wallet is meaningless before the user knows what they'd use it for. Seeing a market they want to trade on *is* the motivation.

The solution: **guest mode**. The app launches directly into the home view with markets loaded. Identity and wallet setup are triggered by user actions that require them. Neither step is forced on launch.

---

## Guest Mode

### What Works Without Identity or Wallet

| Feature | Available in guest mode | Reason |
| --- | --- | --- |
| Browse markets | Yes (if backend has cached data) | See market discovery note below |
| View market detail | Yes | All market data is public |
| View price charts | Yes | Chart data comes from pool history — public chain data |
| View orderbook | Yes | Order data is public (discovered via Nostr) |
| Search and filter | Yes | Client-side filtering on loaded data |
| View a trade quote | Yes | `quote_trade` is a read-only engine operation |
| Execute a trade | **No** — triggers identity + wallet setup | Requires signed transaction and wallet |
| Place a limit order | **No** — triggers identity + wallet setup | Requires signing + Nostr publishing |
| Create a market | **No** — triggers identity + wallet setup | Requires signing + Nostr publishing |
| Send/receive funds | **No** — triggers identity + wallet setup | Requires wallet |
| View wallet balance | **No** — requires wallet | Requires wallet |
| Publish to Nostr | **No** — requires identity | Requires Nostr keypair |

### Market Discovery in Guest Mode

Market discovery has a backend dependency on a Nostr node, which is only initialized after identity setup. The frontend handles this with a two-level fallback:

1. **`discover_contracts`** — primary path. Connects to relays and fetches market events. Requires an active Nostr node. Fails if no identity is loaded.
2. **`list_contracts`** — fallback path. Reads markets from the backend's local store. Works without a Nostr node. Returns whatever was previously discovered in prior sessions.

In practice:
- **Returning users who log out**: Their prior session populated the store. `list_contracts` returns those markets on next launch. Markets appear immediately.
- **Fresh install, no prior session**: The store is empty. `list_contracts` returns nothing. Guest sees an empty market list until they sign in.

Markets discovered in previous sessions are **not cleared on logout** — only the identity and wallet are deleted. The store persists across sessions.

### Quote Preview in Guest Mode

A guest user can enter a trade amount and see a live quote (`quote_trade` is a read-only engine operation, no wallet required). The quote modal shows the trade parameters and replaces the "Confirm trade" button with a locked state:

```
┌─────────────────────────────────────────┐
│ Buy YES — "Will BTC hit $200k?"         │
│                                         │
│ You pay:     10,000 sats                │
│ You receive: ~13.8 YES tokens           │
│ Price:       72.4%                      │
│ Est. fee:    ~250 sats                  │
│                                         │
│ ┌─────────────────────────────────────┐ │
│ │ Create a wallet to trade            │ │
│ │ Your quote will be preserved        │ │
│ │                                     │ │
│ │ [Set up wallet]                     │ │
│ └─────────────────────────────────────┘ │
└─────────────────────────────────────────┘
```

The user sees *exactly* what they'd get before being asked to commit. This is the key motivational moment. The "Set up wallet" button in the quote modal triggers `setup-wallet-from-quote`, which follows the same guard logic as any wallet-requiring action.

---

## Deferred Setup Triggers

Setup is triggered at the **point of need** — the exact moment the user tries to do something that requires identity or wallet. Identity and wallet setup happen in **separate modal overlays**. They run sequentially when both are needed (identity first, then wallet), or the wallet step opens on its own when the user is already signed in.

### The Two Modals

**Identity modal** (`onboardingStep = "nostr"`): Opened when the user has no Nostr identity. Handles identity generation or import only. Does not create a wallet. After completing identity setup, the user returns to whatever they were doing. If the triggering action also requires a wallet, the wallet modal then opens separately.

**Wallet modal** (`onboardingStep = "wallet"`): Only available when the user **already has a Nostr identity**. A user without an identity cannot open the wallet modal — they see the identity modal first. The wallet modal always begins with a backup scan before presenting setup options. It never navigates back into the identity modal. When opened directly by a signed-in user (via the header button or an action guard), `state.onboardingWalletOnly` is set to `true` and the step indicator and "Step 2 of 2" eyebrow are hidden. When reached by completing Step 1 in the same session, `onboardingWalletOnly` is `false` and the full two-step progress UI is shown.

### Trigger Guard Logic

| User state | Action requiring wallet only | Action requiring identity + wallet |
| --- | --- | --- |
| No identity, no wallet | Identity modal opens | Identity modal opens |
| Has identity, no wallet | Wallet modal opens (scan first) | Wallet modal opens (scan first) |
| Has identity, wallet locked | Wallet unlock modal | Wallet unlock modal |
| Has identity, wallet unlocked | Proceeds | Proceeds |

### Trigger Table

| User action | Guard | What opens |
| --- | --- | --- |
| Execute a trade | `requiresWallet` | Identity modal (if no identity) or wallet modal (if signed in, no wallet) |
| Click "Set up wallet" in quote modal | `requiresWallet` | Same as above |
| Send or receive funds | `requiresWallet` | Same as above |
| Place a limit order | `requiresIdentityAndWallet` | Identity modal (if no identity) or wallet modal (if signed in, no wallet) |
| Create a market | `requiresIdentityAndWallet` | Same as above |
| Create a pool | `requiresIdentityAndWallet` | Same as above |
| Resolve a market | `requiresIdentityAndWallet` | Same as above |
| Click "Get started" in header | — | Identity modal |
| Click wallet icon (signed in, locked) | — | Wallet view (unlock screen) |

### Setup Flow: Identity Modal

When the user triggers any action without a Nostr identity:

1. The identity modal opens as an overlay over the current view
2. User generates a new keypair or imports an existing nsec. The backend identity is loaded at this point (needed for key export and profile fetch), but `state.nostrPubkey`/`state.nostrNpub` are **not yet set** — identity is held in `onboardingPendingPubkey`/`onboardingPendingNpub` until confirmed
3. User reviews their key on the backup screen (generate) or identity confirmation screen (import), then clicks **"Continue to wallet setup"** — this is the moment the identity is committed to `nostrPubkey`/`nostrNpub` and the user becomes signed in
4. If the user backs out before clicking "Continue to wallet setup", the pending identity is discarded and `delete_nostr_identity` is called on the backend
5. After completing identity setup, if the original action also required a wallet, the wallet modal now opens automatically

### Setup Flow: Wallet Modal (signed-in users, wallet-only mode)

When a signed-in user triggers a wallet-requiring action or clicks "Set up wallet" in the header, and has no wallet:

1. The wallet modal opens as an overlay (`onboardingWalletOnly = true` — no step indicator, no "Step 2 of 2")
2. **Backup scan runs immediately** — the modal shows a spinner while checking Nostr relays for existing encrypted wallet backups
3. After scan:
   - **Backup found**: The "Restore from Nostr backup" page is shown. Each discovered wallet is presented as a **selectable card** showing the wallet name and the primary relay hostname (e.g. `relay.primal.net` or `relay.primal.net and 1 other`). A secondary `Set up a new wallet` button is always shown below the primary CTA. There is no back button — the wallet modal never navigates back into the identity modal. Multiple named wallet backups may be listed — the user selects the one to restore.
   - **No backup**: "Set up your wallet" page is shown with create / restore-from-seed options. No back button — the wallet modal never navigates back to the identity modal.
4. User creates or restores a wallet (password required)
5. Modal closes. The user is returned to exactly where they were.

---

## Header States

The header adapts to the user's setup state. Identity and wallet status are reflected independently.

| State | Right side of header |
| --- | --- |
| Guest (no identity, no wallet) | [Get started] filled button |
| Signed in, no wallet | [Set up wallet] text button + profile avatar + full user menu |
| Signed in, wallet locked | Wallet icon (no balance) + profile avatar + full user menu |
| Signed in, wallet unlocked | Wallet icon + balance + profile avatar + full user menu |

### Guest Header

```
[Deadcat Logo]  Markets  Live  Social     [Search...]  [Get started]
```

"Get started" opens the identity modal (`open-wallet` action with `onboardingStep = "nostr"`). The user chooses between "Generate new identity" and "Import existing identity" inside the modal.

### Signed-In, No Wallet

```
[Deadcat Logo]  Markets  Live  Social     [Search...]  [Set up wallet]  [Profile Pic]
```

"Set up wallet" opens the wallet modal which immediately begins a backup scan.

### Authenticated Header

```
[Deadcat Logo]  Markets  Live  Social     [Search...]  [💳 42,350 sats]  [Profile Pic]
```

---

## Logout Behaviour

Logging out deletes **both** the Nostr identity and the wallet from disk. After logout:

- `walletStatus = "not_created"` (wallet file deleted)
- `nostrPubkey = null`, `nostrNpub = null`
- Header reverts to guest state (Log In / Sign Up)
- Markets from the last session remain visible via the local store (`list_contracts` fallback)
- On next sign-in, the wallet modal will scan for a Nostr backup before offering create/restore

This ensures a clean slate — no password, no key material, no wallet data persists after logout.

---

## Progressive Upgrade Path

The user's state progresses naturally through engagement:

```
Guest → Browse markets → See a trade they want
  → Click "Buy YES" → Identity modal → Sign in
  → Wallet modal → Scan (backup found?) → Create or restore wallet
  → Trade executes
  │
  └→ Click "Create Market" → Identity modal (if not signed in) → Sign in
     → Wallet modal → Scan → Set up wallet → Create market
```

Each step is motivated by the user's own intent. No step is forced.

### State Persistence Across Setup

When a setup flow is triggered from a specific context, the app preserves the following across both modals:

- `state.selectedMarketId` — which market they were viewing
- `state.selectedSide` — YES or NO
- `state.tradeSizeSats` / `state.tradeContracts` — their entered amount
- `state.limitPrice` — their limit price (if applicable)
- `state.view` — they return to "detail" view, not home

State is preserved because neither modal wipes these fields — they remain untouched until the user explicitly changes them. No explicit context-save-and-restore mechanism is needed.

---

## Relationship to Existing Onboarding Documentation

This document **supersedes** the mandatory onboarding flow described in the original US-OB1 and US-OB2. The identity and wallet setup UIs are largely the same, but entry point, presentation, and sequencing have changed:

| Before (mandatory) | After (deferred) |
| --- | --- |
| Forced on first launch before any content | Triggered by the specific action that needs it |
| Full-page takeover — no content visible | Modal overlay — user can see context behind |
| Must complete both steps to see markets | Markets visible immediately (from store) |
| Identity and wallet are a single sequential flow | Identity and wallet are separate modal overlays; wallet-only when already signed in |
| Wallet modal shown to all users | Wallet modal only shown to signed-in users |
| No backup scan for new-identity users | Backup scan always runs when wallet modal opens |
| User signed in at keypair generate/import | User signed in only at "Continue to wallet setup"; pending identity held in `onboardingPendingPubkey`/`onboardingPendingNpub` until confirmed |
| Backup restore shown as data rows with relay name | Backup restore shown as selectable wallet cards with wallet name; multiple named wallets supported |
| Back button on backup restore page (2e) | No back button anywhere in the wallet modal; secondary `Set up a new wallet` button always shown on 2e |
