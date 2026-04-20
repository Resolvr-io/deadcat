# UX Design Decisions Log

Each entry follows the format from [deadcat-core-design.md](../architecture/deadcat-core-design.md): **Chosen** / **Rejected** / **Why**. Entries are grouped by category.

---

## Rendering & Architecture

### React Over Vanilla HTML String Rendering

**Chosen**: React 18 with TypeScript (TSX). Components are React function components; state changes trigger fine-grained virtual DOM reconciliation — only affected components re-render.
**Rejected (previously attempted)**: Pure TypeScript functions returning HTML template strings. Full DOM replacement via `app.innerHTML = html` on each render cycle.
**Why**: The vanilla string rendering approach worked at small scale but created friction as the component surface grew: no JSX type safety, no composable component patterns, all event handlers had to flow through a single root dispatcher, and full re-renders became noticeable as the DOM grew beyond ~2000 nodes. React provides declarative composition, fine-grained updates via hooks, and a standard ecosystem for async state (React Query). The trade-off — build toolchain complexity (Vite + TSX compilation) — is accepted because Vite's HMR and TSX type safety outweigh the setup cost for a codebase this size. See also: Zustand and React Query decisions below.

### Zustand Over Manual Global State Object

**Chosen**: Zustand store (`src/store/index.ts`) with ~280 properties in flat slices. Components subscribe via `useStore((s) => s.property)`. State is mutated via `useStore.setState({ ... })`.
**Rejected**: (a) Plain global `state` object with manual `render()` calls. (b) Redux-style reducers with dispatched actions. (c) Distributed component-local state.
**Why**: Preserves the original single-source-of-truth philosophy — all UI state lives in one place, readable in one file — while gaining selective re-rendering. With the vanilla approach, any state mutation triggered a full `innerHTML` replacement. With Zustand, only components that subscribe to the changed slice re-render. The flat structure (all properties at the top level, no nested reducers) keeps the mental model simple: `useStore.setState({ tradeSizeSats: 10000 })` is exactly as legible as `state.tradeSizeSats = 10000` was before. Zustand's minimal API (`create`, `useStore`, `setState`) adds no indirection beyond what React itself requires.

### React Component Event Handlers Over Root Event Delegation

**Chosen**: Standard React `onClick`, `onChange`, `onSubmit` props on components. Mutations encapsulated in React Query hooks (`useTrading`, `useWalletOps`, etc.).
**Rejected**: Single click listener on root `#app` + `data-action` attribute dispatch pattern.
**Why**: The `data-action` pattern was designed to survive `innerHTML` re-renders — the root listener never needed re-attachment. With React, this constraint disappears: React manages listener lifecycle alongside component lifecycle. Per-component handlers are more discoverable (the handler is co-located with the element that triggers it), type-safe (no stringly-typed action names), and testable in isolation. The trade-off is losing the centralized action registry that `data-action` provided — mitigated by React Query's mutation hooks, which serve as the canonical list of all side-effectful operations.

### TanStack React Query for Server State

**Chosen**: React Query (`src/queries/`) for all data that originates from the Tauri backend: markets, wallet snapshots, price history, orders, pools. Queries are invalidated via Tauri events (`useTauriEvents`).
**Rejected**: Manual polling loops or Zustand for server state.
**Why**: Server state (markets, wallet data) has fundamentally different lifecycle requirements from UI state (which tab is selected, whether a modal is open). React Query's stale-while-revalidate model, automatic background refetching, and cache invalidation on mutation success eliminate entire categories of manual synchronization code that existed in the vanilla approach (polling intervals, manual `refreshWallet()` calls, etc.). The IPC bridge remains unchanged — `invoke()` calls are wrapped inside React Query's `queryFn`/`mutationFn` rather than called directly from event handlers.

---

## Trading Interaction Design

### Two-Step Quote-Confirm Pattern Over One-Click Trading

**Chosen**: User enters amount → sees live quote → clicks "Buy" → confirm modal with exact amounts → clicks "Confirm" → trade executes.
**Rejected**: Single "Buy X sats of YES" button that quotes and executes in one step.
**Why**: Directly mirrors the core engine's `quote_trade` → `build_trade_pset` two-step pattern. The quote step is read-only and cheap. The build step is irreversible (broadcasts a transaction). The confirm modal prevents accidental trades by showing the exact outcome before commitment. This is standard for any trading platform — even centralized exchanges show order confirmation. The extra click is a feature, not friction.

### Probability Display Over Raw Sats Price

**Chosen**: Display token prices as probabilities (0-100%) throughout the UI: "YES 72%", "NO 28%".
**Rejected**: Display prices in sats per contract: "YES: 3,600 sats", "NO: 1,400 sats".
**Why**: Users think in probabilities — "I think there's a 72% chance this happens" — not in sats-per-contract pricing. The conversion is trivial (`probability = price / collateral_per_pair`), but forcing users to do it mentally creates unnecessary cognitive load. The trade-off is that sats-per-contract is the "real" price unit (what `TradeQuote.total_input` reports), so the detail view shows both: probability prominently, sats in the quote breakdown.

### Sats and Contracts Size Modes Over Sats-Only

**Chosen**: Toggle between "sats" mode (how much to spend) and "contracts" mode (how many tokens to buy).
**Rejected**: Sats-only input, or contracts-only input.
**Why**: Different mental models for different intents. Opening a position: "I want to risk 10,000 sats" (sats mode). Closing a position: "I want to sell all 15 of my contracts" (contracts mode). Both map to `TradeAmount::ExactInput` — contracts mode computes the sats equivalent from the current effective price. The default mode switches based on trade intent: "Open" defaults to sats, "Close" defaults to contracts.

### Auto-Requote on Stale Quote Over Manual Refresh

**Chosen**: When `build_trade_pset` returns `CoreError::StaleQuote`, automatically re-quote and display the updated quote with a "Price updated" flash indicator.
**Rejected**: Show an error: "Quote expired. Please refresh." with a manual button.
**Why**: Stale quotes are a normal event, not an error. Between quoting and confirming, another trader may have moved the pool's s_index. Auto-requoting maintains flow — the user sees the updated price and can immediately confirm. The flash indicator draws attention to the price change without blocking the flow. If the new price is significantly different (>5%), the confirm button is disabled for 2 seconds to prevent accidental confirmation.

### Partial Fill Warning Over Silent Acceptance

**Chosen**: When `TradeQuote.filled_amount < requested_amount`, show an explicit warning: "Only X of Y sats can be filled. Proceed with partial fill?"
**Rejected**: Silently execute the partial fill.
**Why**: A user who enters 100,000 sats and only gets 40,000 filled may not notice. The warning respects the user's intent — they asked for X and are getting less. The core engine returns partial fills as `Ok` (not an error), so the UI must add this check. The user explicitly accepts the partial fill, or adjusts their amount.

---

## Market State Display

### Flat State Badges Over Phase-Encoded Display

**Chosen**: Four visual states: "Live" (green), "Resolved YES" (emerald), "Resolved NO" (rose), "Expired" (amber). The Dormant/Unresolved distinction within `MarketState::Trading` is hidden.
**Rejected**: Showing "Dormant" vs "Active" vs "Unresolved" as separate badges.
**Why**: Mirrors the core's own design decision to use a flat `MarketState` — the Dormant/Unresolved split is a covenant implementation detail. A market with zero outstanding pairs is just "a market where no one has issued yet" — showing "Dormant" would confuse users who don't understand covenant phases. The `outstanding_pairs` value is available in the detail view for advanced users.

### Conditional Action Tabs Over Universal Tabs

**Chosen**: Action tabs (Trade, Issue, Redeem, Cancel) are shown/hidden based on market state and user role. Users see only the actions available to them.
**Rejected**: All tabs always visible, with disabled states for unavailable actions.
**Why**: Showing a disabled "Redeem" tab on a live market clutters the interface and raises questions ("Why can't I redeem?"). Conditional visibility means every visible tab is actionable. The tab set acts as an implicit state indicator — the appearance of "Redeem" after resolution is itself a notification that the market has settled. The mapping is documented in [ux-views.md](views.md) § Trade Composer tab visibility.

---

## Wallet & Onboarding

### Browse-First Guest Mode Over Mandatory Onboarding

**Chosen**: The app launches directly into the home view with markets loaded. No identity or wallet required to browse. Setup is deferred to the moment the user tries to trade, create, or send/receive funds.
**Rejected**: (a) Mandatory two-step onboarding (identity + wallet) before any content is visible. (b) Always-visible "Set up wallet" banners on every page.
**Why**: The primary conversion bottleneck is showing users *why* they should set up a wallet. A user who sees live markets with real probabilities and price charts is motivated — "I think YES at 72% is wrong, I want to buy NO." That motivation doesn't exist on a blank setup screen. Market discovery works without identity (public Nostr events from a default relay set). Quote preview works without a wallet (`quote_trade` is read-only). The setup trigger is contextual: "Buy YES" button shows an inline "Create a wallet to trade" prompt, not a full-page redirect. After setup, the user returns to exactly where they were — same market, same trade parameters. The dependency order (identity before wallet for Nostr backup detection) is preserved when both are needed. See [ux-first-use.md](first-use.md) for the full specification.

### Nostr-Based Wallet Backup Over Manual Export

**Chosen**: Wallet mnemonic encrypted with the user's Nostr key and published to their relays (NIP-44 + NIP-78). Automatic detection and restore during onboarding.
**Rejected**: Manual mnemonic export/import only.
**Why**: Users lose mnemonics. Nostr relay backup provides seamless cross-device recovery without a centralized backup service. The encryption (NIP-44 self-encryption) ensures only the user can decrypt. The UX win: "Restore from Nostr backup" pre-selected when a backup is detected, reducing wallet restoration to a single click. The trade-off is relay availability — if all relays lose the backup, the user falls back to mnemonic restore (the manual path is always available).

### Three-Tab Send/Receive Over Single Address

**Chosen**: Send and receive modals each have three tabs: Lightning, Liquid, Bitcoin. Powered by Boltz swaps for cross-network transfers.
**Rejected**: Liquid-only send/receive.
**Why**: Users receive funds from various sources — Lightning payments, on-chain Bitcoin, and direct Liquid transfers. Supporting all three via Boltz integration makes Deadcat a viable primary wallet. The three-tab pattern matches user mental model: "I have Bitcoin on-chain, how do I get it into Deadcat?" → Bitcoin tab → enter amount → get a Bitcoin address → send → funds arrive as L-BTC. Without this, users need an external swap service — additional friction before they can start trading.

---

## Data Visualization

### SVG Chart Over Canvas or Library

**Chosen**: Hand-crafted SVG polyline charts with custom markers and hover interactions.
**Rejected**: Chart.js, D3.js, or HTML Canvas-based charts.
**Why**: The chart requirements are narrow: a single polyline with hover tooltip, cat-paw markers, and a pulsing live indicator. A charting library would add 100KB+ to the bundle for features we don't use. SVG integrates naturally with the HTML string rendering model (it's just more markup in the template). Canvas would require imperative drawing code that doesn't fit the declarative template pattern. The trade-off is that complex chart features (zoom, pan, multiple overlays) would need custom implementation — acceptable because the current feature set doesn't require them.

### Cat-Paw Data Point Markers Over Standard Dots

**Chosen**: Each data point on the price chart is rendered as a small cat-paw SVG silhouette.
**Rejected**: Standard circular dot markers.
**Why**: Brand identity. The Deadcat brand is built around the cat motif — logo, splash screen animations, and chart markers all reinforce it. The paw markers are subtle enough not to distract from the data but distinctive enough to be memorable. The paw SVG path is a single `<path>` element — no rendering cost difference from a `<circle>`.

### Block Height X-Axis Over Timestamps

**Chosen**: Chart X-axis uses block heights (10B, 25B, 50B, 100B timescales) rather than wall-clock timestamps.
**Rejected**: Time-based X-axis (1h, 6h, 1d, 1w).
**Why**: Liquid blocks are the ground truth for market state transitions — each pool swap is exactly one transition per block. Block-based timescales produce evenly-spaced data points (one per transition), while time-based scales would show irregular spacing (blocks are ~1 minute but not exact). The trade-off is that casual users are less familiar with "last 50 blocks" than "last hour" — the UI helps by showing estimated time ranges alongside block counts.

---

## Progressive Disclosure

### Market Maker Mode Toggle Over Role-Based Accounts

**Chosen**: A single "Market maker mode" toggle in settings that reveals advanced features (market creation, pool management, limit orders, issuance/cancellation).
**Rejected**: Separate account types (trader vs maker) or always-visible advanced features.
**Why**: Most users are traders who never create markets or pools. Showing creation forms and pool management to everyone adds clutter and confusion. A single toggle is simpler than account types (no sign-up distinction, no permission system). The toggle persists across sessions. When enabled, the Create Market button appears in the header, the Issue/Cancel tabs appear on market detail, and the pool management section becomes accessible.

### Collapsible Advanced Details Over Flat Layout

**Chosen**: Market details like asset IDs, oracle pubkey, covenant state, and creation txid are hidden behind a "Show advanced details" toggle.
**Rejected**: Always-visible advanced info.
**Why**: 95% of users don't need raw asset IDs or covenant state numbers. Showing them by default creates visual noise that obscures the important information (probability, volume, settlement date). The collapsible pattern lets advanced users verify on-chain data without penalizing casual users. The toggle state resets when navigating away — advanced details are not "sticky" across markets.

---

## Error Handling

### All-at-Once Shortfall Display Over Sequential Errors

**Chosen**: When `CoreError::InsufficientFunds { shortfalls }` is returned, display ALL missing assets simultaneously: "Need 5,000 more sats AND 10 more YES tokens."
**Rejected**: Show one error at a time: "Need 5,000 more sats." (User adds sats, tries again.) "Need 10 more YES tokens."
**Why**: The core engine returns all shortfalls in a single error (the `shortfalls: Vec<Shortfall>` field exists precisely for this UX). Sequential errors waste user time and create frustration ("Why didn't it tell me about the tokens the first time?"). The UI renders each shortfall as a separate line item with the asset name (via `identify_asset` for token IDs) and the deficit amount.

### Inline Errors Over Modal Error Dialogs

**Chosen**: Validation errors and core errors display inline near the relevant input or action button.
**Rejected**: Modal error dialogs that require dismissal.
**Why**: Modal errors interrupt flow and force an extra click to dismiss. Inline errors are contextual — "Need 5,000 more sats" appears next to the amount input, not in a dialog. The user can adjust the input and retry without closing anything. Exceptions: broadcast failures and network errors use toast notifications (bottom-right, auto-dismiss after 5 seconds) because they're not tied to a specific input field.
