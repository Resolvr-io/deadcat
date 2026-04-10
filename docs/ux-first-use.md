# First Use Experience

The first-use experience prioritizes **zero-friction exploration**. A new user should see live markets within seconds of launching the app — no identity setup, no wallet creation, no password. Setup is deferred to the moment it's actually needed: when the user tries to trade, send, or receive funds.

This document specifies the guest mode, the deferred setup triggers, and the progressive upgrade path from anonymous browser to active trader.

---

## Design Principle: Browse First, Register When Needed

Traditional onboarding forces the user through identity + wallet setup before they see any content. This creates two problems:

1. **Drop-off**: Users who just want to explore leave before seeing a single market.
2. **Unmotivated setup**: Creating a wallet is meaningless before the user knows what they'd use it for. Seeing a market they want to trade on *is* the motivation.

The solution: **guest mode**. The app launches directly into the home view with markets loaded. Identity and wallet setup are triggered by user actions that require them.

---

## Guest Mode

### What Works Without Identity or Wallet

| Feature | Available in guest mode | Reason |
| --- | --- | --- |
| Browse markets | Yes | Markets are fetched from a default relay set — no user identity needed |
| View market detail | Yes | All market data is public |
| View price charts | Yes | Chart data comes from pool history — public chain data |
| View orderbook | Yes | Order data is public (discovered via Nostr) |
| Search and filter | Yes | Client-side filtering on loaded data |
| View a trade quote | Yes | `quote_trade` is a read-only engine operation |
| Execute a trade | **No** — triggers wallet setup | Requires signed transaction |
| Place a limit order | **No** — triggers wallet + identity setup | Requires signing + Nostr publishing |
| Create a market | **No** — triggers wallet + identity setup | Requires signing + Nostr publishing |
| Send/receive funds | **No** — triggers wallet setup | Requires wallet |
| View wallet balance | **No** — triggers wallet setup | Requires wallet |
| Publish to Nostr | **No** — triggers identity setup | Requires Nostr keypair |

### Market Discovery in Guest Mode

Guest mode uses a **hardcoded default relay set** for market discovery:

```
wss://relay.damus.io
wss://relay.primal.net
```

These are read-only connections — no publishing, no authentication. The app fetches market announcement events (Nostr kind for Deadcat markets) and displays them exactly as it would for an authenticated user. The user sees the same market cards, charts, and detail views.

**Why a default relay set works**: Market announcements are public events. Any relay that has them can serve them to anonymous clients. The user doesn't need their own relay list (NIP-65) to *read* markets — only to *publish* (create markets, post orders, announce pools). After identity setup, the app switches to the user's NIP-65 relay list and merges any additional relays.

### Quote Preview in Guest Mode

A guest user can enter a trade amount and see a live quote (`quote_trade` is read-only). The quote modal shows:

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
│ │ 🔒 Create a wallet to trade        │ │
│ │                                     │ │
│ │ [Set Up Wallet]                     │ │
│ └─────────────────────────────────────┘ │
└─────────────────────────────────────────┘
```

The user sees *exactly* what they'd get before being asked to commit. This is the key motivational moment — "I can buy YES at 72% and it pays out at 100% if I'm right. Let me set up a wallet."

---

## Deferred Setup Triggers

Setup is triggered at the **point of need** — the exact moment the user tries to do something that requires identity or wallet. Each trigger leads to the minimal setup required, then returns the user to their original action.

### Trigger Table

| User action | Requires | Setup triggered | Return behavior |
| --- | --- | --- | --- |
| Click "Buy YES" / "Sell NO" (execute) | Wallet | Wallet setup flow | Return to trade confirm modal |
| Click "Send" or "Receive" | Wallet | Wallet setup flow | Return to send/receive modal |
| Click "Place Limit Order" | Wallet + Identity | Identity → Wallet setup flow | Return to limit order form |
| Click "Create Market" | Wallet + Identity | Identity → Wallet setup flow | Return to create market form |
| Click "Create Pool" | Wallet + Identity | Identity → Wallet setup flow | Return to pool creation form |
| Click "Resolve Market" | Wallet + Identity | Identity → Wallet setup flow | Return to resolution panel |

### Setup Flow: Wallet Only

When the user triggers a wallet-requiring action without a wallet:

1. **Inline prompt** replaces the action button: "Create a wallet to trade" with [Set Up Wallet] button
2. Clicking [Set Up Wallet] opens the wallet setup modal (not a full-page takeover)
3. Wallet setup modal offers: "Create new wallet" | "Restore from mnemonic"
4. User creates wallet (password + mnemonic backup) or restores
5. Modal closes. The user is returned to exactly where they were — same market, same trade parameters
6. The action button is now active. The user can complete their trade.

**Note**: "Restore from Nostr backup" is only available if the user already has a Nostr identity. If they don't, the wallet setup modal shows only "Create" and "Restore from mnemonic."

### Setup Flow: Identity + Wallet

When the user triggers an action requiring both (e.g., creating a market):

1. **Inline prompt**: "Set up your identity to create markets" with [Get Started] button
2. Step 1: Identity setup (generate or import Nostr keypair) — same as current onboarding but in a modal
3. Step 2: Wallet setup (create or restore) — with the addition of "Restore from Nostr backup" now that identity exists
4. Both modals close. User is returned to their original action.

The two-step flow preserves the dependency order: identity before wallet (needed for Nostr backup detection).

---

## Header State Indicators

The header adapts to the user's setup state:

| State | Wallet button | User menu |
| --- | --- | --- |
| Guest (no identity, no wallet) | [Set Up Wallet] text button | Generic user icon, click → "Set up identity" prompt |
| Identity only (no wallet) | [Set Up Wallet] text button | Nostr profile pic/npub, settings available |
| Wallet locked | Wallet icon (no balance) | Full menu, "Unlock" option |
| Wallet unlocked | Wallet icon + balance | Full menu |

### Guest Header Example

```
[Deadcat Logo]  Markets  Live  Social     [Search...]  [Set Up Wallet]  [👤]
```

The "Set Up Wallet" button is a clear call-to-action but not intrusive — it sits where the balance would normally appear. The user icon (👤) opens a minimal menu with "Set up your Nostr identity" as the primary option.

### Authenticated Header Example

```
[Deadcat Logo]  Markets  Live  Social     [Search...]  [💳 42,350 sats]  [Profile Pic]
```

---

## Progressive Upgrade Path

The user's state progresses naturally through engagement:

```
Guest → Browse markets → See a trade they want → Set up wallet → Trade
  │
  └→ Want to create a market → Set up identity → Set up wallet → Create
```

Each step is motivated by the user's own intent. No step is forced. The app remembers context across setup flows — if a user was viewing market X, entered 10,000 sats, and clicked "Buy YES" which triggered wallet setup, after setup they return to market X with 10,000 sats in the amount field and the "Buy YES" button now active.

### State Persistence Across Setup

When a setup flow is triggered from a specific context, the app preserves:

- `state.selectedMarketId` — which market they were viewing
- `state.selectedSide` — YES or NO
- `state.tradeSizeSats` / `state.tradeContracts` — their entered amount
- `state.limitPrice` — their limit price (if applicable)
- `state.view` — "detail" (they return to the detail view, not home)

This prevents the frustrating pattern of: enter trade parameters → get sent to setup → finish setup → land on home page → navigate back to market → re-enter parameters.

---

## Relationship to Existing Onboarding

This document **supersedes** the mandatory onboarding flow described in US-OB1 and US-OB2 of [ux-stories-onboarding.md](ux-stories-onboarding.md). The identity and wallet setup UIs are the same, but their trigger point changes:

| Before (mandatory) | After (deferred) |
| --- | --- |
| Forced on first launch before any content | Triggered by user action that requires it |
| Full-page takeover | Modal overlay (preserves context) |
| Must complete both steps to see markets | Markets visible immediately |
| "Set up your identity" is step 1 | "Set up your identity" is triggered by publishing actions |
| "Create wallet" is step 2 | "Create wallet" is triggered by trading/sending actions |

The setup components themselves (identity generation, mnemonic display, password creation, Nostr backup restore) are unchanged — only the entry point and presentation change.
