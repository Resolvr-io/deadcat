# Deadcat UX Design Document

## Purpose

This document specifies the user experience design for Deadcat Live — a desktop application for trading binary prediction markets on the Liquid Network. It translates the protocol-level abstractions defined in [deadcat-core-design.md](deadcat-core-design.md) into concrete interaction patterns, visual states, and user flows.

The primary design goal: make covenant-based prediction markets feel as intuitive as a centralized exchange, while preserving the self-custodial properties that make Deadcat distinct. Users should never need to understand Simplicity covenants, UTXO mechanics, reissuance tokens, or Nostr relay protocols to trade effectively.

**Companion documents**:
- User Stories: [Trader](ux-stories-trader.md) | [Creator & Oracle](ux-stories-creator.md) | [Operator & Maker](ux-stories-operator.md) | [Onboarding & Wallet](ux-stories-onboarding.md)
- [View Specifications](ux-views.md) — view-by-view interaction specs and state mapping
- [Design Decisions Log](ux-design-decisions.md) — chosen/rejected/why for every significant UX decision

**Relationship to `deadcat-core-design.md`**: This document is the UI counterpart. Where the core doc specifies `MarketState::Trading { outstanding_pairs }`, this doc specifies what the user sees: a green "Live" badge with a probability percentage. Where the core doc specifies `quote_trade` → `build_trade_pset`, this doc specifies the two-step "quote → confirm → execute" interaction pattern. Every UI state maps to a core type; every user action maps to a core API call.

## Design Principles

### 1. Protocol Complexity is an Implementation Detail

The user's mental model is: "I think YES is likely, so I buy YES tokens." They should never encounter terms like `CovenantPhase`, `SlotType`, `UnblindedPset`, or `s_index`. The UI translates protocol concepts into trading concepts:

| Protocol concept | User-facing concept |
| --- | --- |
| `MarketState::Trading` | "Live" market |
| `MarketState::ResolvedYes` / `ResolvedNo` | "Resolved YES" / "Resolved NO" |
| `MarketState::Expired` | "Expired" |
| `outstanding_pairs` | Total collateral locked (displayed as volume) |
| `collateral_per_pair` | Contract size (e.g., "5,000 sats per contract") |
| `TradeQuote.effective_price` | Probability (e.g., "72% YES") |
| `TradeQuote.total_input` / `total_output` | "You pay" / "You receive" |
| `RouteLeg` | Hidden — user sees aggregated totals only |
| `LmsrPoolState.s_index` | Implied probability on the price chart |
| `OrderState.total_filled` | "5,000 of 10,000 sats filled" progress bar |
| LMSR pool reserves | Liquidity depth indicator |
| Nostr relay list | Sync status icon |
| `WalletFunding` | Automatic — wallet UTXOs selected internally |

### 2. State Derives from Core Types

Every visual element maps to a specific field on a `deadcat-core` type. No UI state is invented that doesn't have a protocol counterpart. This principle prevents the UI from displaying information the engine can't verify:

| UI element | Core source | Core type |
| --- | --- | --- |
| Market probability | `TradeQuote.effective_price` or pool spot price | `f64` (display-only) |
| Market status badge | `MarketState` variant | `enum MarketState` |
| Position size | Token balance from wallet UTXOs | `UnblindedUtxo.value` |
| Order fill progress | `OrderState::Active { total_filled, offered_amount }` | `u64` / `u64` |
| Pool reserves | `LmsrPoolState::Active { reserves }` | `PoolReserves` |
| Price chart | `PoolHistoryEntry` sequence | `Vec<PoolHistoryEntry>` |
| Transaction label | `InterpretedTransaction.transitions[].details` | `TransitionDetails` |
| Asset name | `AssetInfo` from `identify_asset` | `enum AssetInfo` |

### 3. Optimistic UI with Honest Fallbacks

Liquid has ~1-minute block times. The UI shows pending operations immediately via `interpret_transaction` (read-only, works on unconfirmed txs), then reconciles when `step` confirms. If the optimistic state is wrong (e.g., a transaction fails), the UI reverts cleanly.

- **Pending trades**: Show "Pending" badge with spinner. Use `interpret_transaction` to display "Buying 10 YES @ 72%" before confirmation.
- **Stale quotes**: When `build_trade_pset` returns `CoreError::StaleQuote`, automatically re-quote and show "Price updated — confirm new quote?"
- **Failed broadcasts**: Show error toast with retry button. Do not auto-retry — the user may want to adjust parameters.

### 4. Progressive Disclosure

The default view shows only what a casual trader needs. Advanced features (limit orders, issuance, pool management, oracle tools) are revealed through explicit user actions:

| Audience | Default visible | Revealed on demand |
| --- | --- | --- |
| Casual trader | Market list, buy/sell YES/NO, wallet balance | — |
| Active trader | + Order book, limit orders, position management | Toggle "Show orderbook" |
| Market creator | + Create market form | Enable "Market maker mode" in settings |
| Pool operator | + Pool management panel | Via "My Pools" in market maker mode |
| Oracle | + Resolution panel | Only shown when user's pubkey matches `oracle_public_key` |

### 5. Fail Loudly at Boundaries, Trust Internally

User input is validated at entry points (form submission, trade confirmation). Once data enters the core engine, the UI trusts core's error types for feedback:

- `CoreError::InsufficientFunds { shortfalls }` → Display all missing assets at once: "Need 5,000 more sats and 10 more YES tokens"
- `CoreError::NoLiquidity` → "No liquidity available for this market"
- `CoreError::InvalidContractState` → "This market has already been resolved" (prevent stale UI from allowing invalid actions)
- `CoreError::StaleQuote` → Silent re-quote, then re-display

## User Personas

Derived from the per-persona ingestion tables in `deadcat-core-design.md` § Sync Patterns and Discovery. Each persona maps to specific core API usage patterns and UI feature gates.

### Trader (Taker)

The most common user. Browses markets, buys/sells YES or NO tokens through the trade router (`quote_trade` + `build_trade_pset`). Never creates contracts. Ingests pools via `PoolSnapshot::Current` (no history needed — only current price matters). Discovers markets and pools via Nostr.

**Key UI surfaces**: Home (market discovery), Detail (trade composer), Wallet (balance, send/receive)

**Core API touchpoints**: `list_markets`, `quote_trade`, `build_trade_pset`, `interpret_transaction`, `identify_asset`

### Market Creator

Creates prediction markets via `build_creation_pset`. Defines the question, oracle, collateral asset, settlement date. Also issues initial token pairs via `build_issuance_pset`. May also act as Trader. Requires "Market maker mode" enabled.

**Key UI surfaces**: Create Market form, Detail (issue/cancel tabs), Home (My Markets filter)

**Core API touchpoints**: `build_creation_pset`, `build_issuance_pset`, `build_cancellation_pset`, `ingest_market`

### Pool Operator

Creates and manages LMSR liquidity pools via `build_lmsr_bootstrap_pset`. Adjusts liquidity (`build_lmsr_adjust_pset`), monitors reserves, closes pools (`build_lmsr_close_pset`). Ingests pools via `PoolSnapshot::Creation` (needs full history for fee revenue tracking). Requires "Market maker mode" enabled.

**Key UI surfaces**: Pool management panel, Detail (pool reserves display), Home (My Pools)

**Core API touchpoints**: `derive_pool_params`, `estimate_bootstrap`, `build_lmsr_bootstrap_pset`, `build_lmsr_adjust_pset`, `build_lmsr_close_pset`, `pool_history`

### Order Maker

Places limit orders via `build_create_order_pset`. Monitors fill progress (`OrderState::Active { total_filled }`). Cancels unfilled orders (`build_cancel_order_pset`). Ingests own orders via `OrderSnapshot::Creation` (needs fill history). Requires "Market maker mode" enabled.

**Key UI surfaces**: Detail (limit order composer), My Orders list, Order fill notifications

**Core API touchpoints**: `derive_order_params`, `build_create_order_pset`, `build_cancel_order_pset`, `order_history`

### Oracle

Resolves markets by signing attestations. The UI shows a resolution panel only when the logged-in user's Nostr pubkey matches a market's `oracle_public_key`. Signs via `oracle_attestation_message`, then broadcasts the signed attestation to resolve the market (`build_oracle_resolve_pset`).

**Key UI surfaces**: Detail (resolution panel — conditional), Attestation signing flow

**Core API touchpoints**: `oracle_attestation_spec`, `build_oracle_resolve_pset`

### Token Holder (Recovery)

A user restoring from mnemonic who holds YES/NO tokens but hasn't ingested the parent market yet. The wallet finds unfamiliar asset IDs during rescan. The UI guides them through the recovery flow: `issuance_transaction(asset_id)` → market creation tx → OP_RETURN → reconstruct params → `ingest_market` → `identify_asset` for labeling.

**Key UI surfaces**: Wallet (unknown asset display), Recovery wizard

**Core API touchpoints**: `ChainSource::issuance_transaction`, `ingest_market`, `identify_asset`, `build_redemption_pset`

## Architecture Overview

### Rendering Model

Deadcat Live uses a Tauri v2 desktop shell with a vanilla TypeScript frontend. Components are pure functions returning HTML strings. State mutations trigger a synchronous full re-render via `app.innerHTML`. This is intentional — the DOM is small enough that full re-renders are fast (<16ms), and the simplicity eliminates an entire class of stale-state bugs that plague virtual DOM diffing.

```
┌─────────────────────────────────────────────────┐
│  Tauri Shell (Rust)                             │
│  ┌───────────────────────────────────────────┐  │
│  │  deadcat-node (Rust)                      │  │
│  │  ContractEngine + LWK Wallet + Nostr      │  │
│  │  ─── IPC boundary (invoke / events) ───   │  │
│  │  Frontend (TypeScript)                    │  │
│  │  state.ts → components/*.ts → render()    │  │
│  └───────────────────────────────────────────┘  │
└─────────────────────────────────────────────────┘
```

### State Management

A single global `state` object holds all UI state. No stores, no reducers, no subscriptions. Event handlers mutate `state` directly, then call `render()`. This maps naturally to the core engine's own model: the engine exclusively owns its store, and all reads/writes go through the engine API. The UI mirrors this — `state` is the single source of truth, and all reads/writes go through handler functions.

### Event Delegation

A single click listener on the root `#app` element handles all interactions via `data-action` attributes. This eliminates listener cleanup on re-render (innerHTML destroys old listeners, but the root listener persists). Input, keydown, and focusout handlers follow the same delegation pattern.

### IPC Bridge

All Tauri commands (`invoke`) map 1:1 to core engine operations. The frontend never constructs PSETs, selects coins, or manages UTXOs — it sends intent ("buy 10 YES tokens on market X") and receives results ("txid, fee paid, tokens received"). The IPC boundary is the natural place for the protocol-to-UX translation described in Principle 1.

## Visual Design Language

### Color System

| Role | Color | Tailwind | Usage |
| --- | --- | --- | --- |
| YES / Positive | Emerald | `emerald-300` / `emerald-400` | YES prices, buy buttons, positive changes, brand accent |
| NO / Negative | Rose | `rose-300` / `rose-400` | NO prices, sell buttons, negative changes |
| Warning | Amber | `amber-300` | Insufficient funds, expiry approaching, unconfirmed tx |
| Background | Slate 950 | `slate-950` | App background |
| Surface | Slate 900 | `slate-900` | Cards, modals, dropdowns |
| Border | Slate 800 | `slate-800` | Dividers, card borders |
| Primary text | Slate 100 | `slate-100` | Headlines, values |
| Secondary text | Slate 400 | `slate-400` | Labels, descriptions |
| Muted text | Slate 500 | `slate-500` | Timestamps, IDs |

### Typography

Monospace font (`mono` class) for all numerical values: prices, amounts, txids, pubkeys. This ensures digit alignment in tables and prevents layout shift when values update. System sans-serif for all other text.

### Spacing

Golden ratio-based spacing system using `--phi: 1.618` CSS custom property. Container widths use `phi-container` utility. This creates visually harmonious proportions without arbitrary magic numbers.

### Iconography

Inline SVG icons throughout — no icon font dependency. Feather icon style (24x24 viewBox, 2px stroke, round caps). The Deadcat logo (cat silhouette) doubles as a chart data point marker and loading indicator.

## Document Index

| Document | Contents |
| --- | --- |
| **User Stories** | |
| [Trader & Token Holder](ux-stories-trader.md) | Browse markets, buy/sell tokens, redeem, view history, recover positions |
| [Market Creator & Oracle](ux-stories-creator.md) | Create markets, issue/cancel pairs, resolve/expire markets |
| [Pool Operator & Order Maker](ux-stories-operator.md) | Bootstrap/adjust/close pools, place/monitor/cancel limit orders |
| [Onboarding, Wallet & Recovery](ux-stories-onboarding.md) | First launch, identity setup, wallet create/restore, send/receive, recovery |
| **Specifications** | |
| [View Specifications](ux-views.md) | View-by-view interaction specs, layout, and core type → UI state mapping |
| [Design Decisions](ux-design-decisions.md) | Chosen/rejected/why for every significant UX decision |
| **Reference** | |
| [Core Design](deadcat-core-design.md) | Protocol-level computation library design |
| [Contract Specification](contract-specification.md) | Simplicity covenant structure and spend paths |
