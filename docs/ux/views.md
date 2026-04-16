# View Specifications & State Mapping

View-by-view interaction specs for Deadcat Live. Each view section documents the layout, data sources (mapped to `deadcat-core` types), state transitions, and conditional elements.

The app has four primary views (`ViewMode`): **home**, **detail**, **create**, **wallet** — plus a pre-auth **onboarding** flow. Navigation is state-driven: `state.view` determines which root component renders.

---

## Onboarding Flow

**Entry condition**: `state.onboardingStep !== null` (no Nostr identity or no wallet).

**Layout**: Centered card on a minimal dark background. Two-step progress bar at top.

### Step 1: Nostr Identity (`onboardingStep === "nostr"`)

| Element | Source | Behavior |
| --- | --- | --- |
| Mode toggle | `onboardingNostrMode: "generate" \| "import"` | Switches between generate and import UI |
| Generated npub | `invoke("init_nostr_identity")` result | Displayed read-only after generation |
| Nsec reveal | `onboardingNsecRevealed` toggle | Hidden by default, revealed on click |
| Import input | `onboardingNostrNsec` | Free text, validated on change |
| Continue button | Gated by: nsec saved acknowledgment (generate) or valid nsec (import) | Advances to Step 2 |

### Step 2: Wallet (`onboardingStep === "wallet"`)

| Element | Source | Behavior |
| --- | --- | --- |
| Mode selector | `onboardingWalletMode: "create" \| "restore" \| "nostr-restore"` | Three radio options |
| Nostr backup indicator | `check_nostr_backup` result | If backup found, pre-selects "nostr-restore" |
| Password fields | `onboardingWalletPassword`, `onboardingWalletPasswordConfirm` | Required for create/restore |
| Mnemonic input | `onboardingWalletMnemonic` | 12/24 word textarea for restore mode |
| Finish button | Validates all fields, creates/restores wallet | Transitions to main app |

**Exit**: `finishOnboarding()` clears onboarding state, fetches wallet status, loads markets, dismisses splash.

---

## Home View (`state.view === "home"`)

**Layout**: Top shell (header + category row) + main content area.

### Header

| Element | Source | Behavior |
| --- | --- | --- |
| Logo | Static SVG | Click → `state.view = "home"` |
| Nav tabs | "Markets", "Live", "Social" | Markets is active; Live/Social are placeholders |
| Search input | `state.search` | Filters markets client-side by question text |
| Wallet button | `state.walletData?.balance` | Shows compact L-BTC balance; click → wallet view |
| User menu | `state.userMenuOpen` toggle | Dropdown: npub, currency selector, settings, logout |

### Category Row

Horizontal tab bar: Trending, Politics, Sports, Culture, Bitcoin, Weather, Macro, Resolved, My Markets.

| Category | Data source | Filter logic |
| --- | --- | --- |
| Trending | `getTrendingMarkets()` | Sorted by volume + change, top N |
| Topic categories | `getFilteredMarkets()` | `market.category === activeCategory` |
| Resolved | `list_markets(StateFilter::TerminalOnly)` | Markets in ResolvedYes/ResolvedNo/Expired states |
| My Markets | Wallet UTXOs matched via `identify_asset` | Markets where user holds YES or NO tokens |

### Content Area

**Trending view** (default):
- **Featured card**: Largest trending market. Full-width hero with embedded SVG price chart, question text, probability, 24h change indicator.
- **Top markets grid**: Next 6 markets in 2-column (desktop) or 1-column (mobile) card grid. Each card: question, probability pill, sparkline, volume badge.
- **Top movers sidebar**: 3 markets with highest absolute 24h change.

**Category view**: Paginated card grid filtered to the selected category. Same card layout as top markets grid.

**My Markets view**: Markets grouped by position type (YES holdings, NO holdings). Each shows position size and current estimated value.

### Market Card Anatomy

```
┌─────────────────────────────────────────┐
│ [Category badge]              [24h ±X%] │
│                                         │
│ Will BTC hit $200k by 2027?             │
│                                         │
│ ┌─────────────────────────────────────┐ │
│ │ ~~~ sparkline chart ~~~             │ │
│ └─────────────────────────────────────┘ │
│                                         │
│ YES 72%                  Vol: 0.5 BTC   │
└─────────────────────────────────────────┘
```

Click anywhere → `openMarket(marketId)` → transitions to detail view.

---

## Detail View (`state.view === "detail"`)

**Layout**: Two-column on desktop (chart + info left, trade composer right). Single column on mobile (chart → info → trade).

### Left Column: Market Info + Chart

| Element | Source | Behavior |
| --- | --- | --- |
| Question | `market.question` | Large heading text |
| State badge | `MarketState` variant mapping | Green "Live", emerald "Resolved YES", rose "Resolved NO", amber "Expired" |
| Description | `market.description` | Collapsible paragraph |
| Resolution source | `market.resolutionSource` | Link or text describing how outcome is determined |
| Settlement date | `market.expiryHeight` → estimated date | "Settles ~June 15, 2027 (block 2,150,400)" |
| Contract size | `market.cptSats` | "5,000 sats per contract" |
| Price chart | `PriceHistoryEntry[]` from `getPriceHistory` | SVG chart with hover tooltip |
| Orderbook | `getFullOrderbook(market)` | Toggleable bid/ask visualization |

### Price Chart Spec

- **Axes**: Y = YES probability (0-100%), X = block height
- **Data**: Each `PriceHistoryEntry` maps to a point: `(block_height, implied_yes_price_bps / 100)`
- **Rendering**: SVG polyline with gradient fill below the line (emerald). Cat-paw SVG markers at data points.
- **Live indicator**: Pulsing dot at the latest data point with current probability label.
- **Hover**: Vertical crosshair line. Tooltip shows exact probability and block height. Tooltip tracks mouse via `chartHoverX` state.
- **Timescale**: Buttons for 10B/25B/50B/100B (blocks). Filters visible data range.
- **Aspect ratio**: Dynamically measured from container via `syncChartAspectFromLayout()`.

### Right Column: Trade Composer

**Tab bar**: Trade | Issue | Redeem | Cancel

Tab visibility is conditional on market state and user role:

| Tab | Visible when | Maps to |
| --- | --- | --- |
| Trade | Always (state is `Trading`) | `quote_trade` + `build_trade_pset` |
| Issue | Market maker mode + market is `Trading` | `build_issuance_pset` |
| Redeem | Market is `ResolvedYes`/`ResolvedNo`/`Expired` AND user holds tokens | `build_redemption_pset` |
| Cancel | Market maker mode + market is `Trading` + user holds both YES and NO tokens | `build_cancellation_pset` |

### Trade Tab

```
┌─────────────────────────────────────┐
│  [YES ██████]  [NO ░░░░░░]          │  ← Side selector
│                                     │
│  [Market ▼] [Limit]                 │  ← Order type
│                                     │
│  [Open ○] [Close ○]                 │  ← Trade intent
│                                     │
│  Amount: [________] [sats|contracts]│  ← Size input + mode toggle
│                                     │
│  ┌─────────────────────────────────┐│
│  │ Price:     72%                  ││
│  │ You pay:   10,000 sats          ││  ← Live quote
│  │ You get:   ~13.8 contracts      ││
│  │ Est. fee:  ~250 sats            ││
│  └─────────────────────────────────┘│
│                                     │
│  [     Buy YES     ]                │  ← Action button (emerald)
└─────────────────────────────────────┘
```

**State flow**: Input change → debounce 300ms → `quote_trade` → display quote → user clicks Buy → confirm modal → `build_trade_pset` → sign → broadcast → pending toast → `step` confirms → success toast.

### Quote Confirm Modal

Triggered by the Buy/Sell button. Full-screen overlay with:

| Field | Source |
| --- | --- |
| Direction | "Buying YES" / "Selling NO" etc. |
| Amount | `TradeQuote.total_input` (formatted) |
| Tokens | `TradeQuote.total_output` (formatted) |
| Price | `TradeQuote.effective_price` as probability % |
| Fee | `TradeQuote.estimated_fee` (with "estimate" disclaimer) |
| Route legs | `TradeQuote.legs` — shown only in advanced mode |
| Confirm button | Triggers `build_trade_pset` + sign + broadcast |
| Cancel button | Dismisses modal |

### Oracle Resolution Panel

**Visible only when**: `state.nostrPubkey === market.oraclePubkey` AND market state is `Trading`.

```
┌─────────────────────────────────────┐
│ ⚖ You are the oracle for this      │
│   market.                           │
│                                     │
│ [Resolve YES]    [Resolve NO]       │
└─────────────────────────────────────┘
```

---

## Create View (`state.view === "create"`)

**Entry**: "Create Market" button (visible in market maker mode).

**Layout**: Centered form card.

| Field | Input type | Validation | Maps to |
| --- | --- | --- | --- |
| Question | Text input (required) | Min 10 chars | Nostr event content |
| Description | Textarea | Optional | Nostr event content |
| Category | Dropdown | Required, from `MarketCategory` | Nostr event tag |
| Resolution source | Text input | Optional | Nostr event content |
| Collateral per pair | Constrained dropdown (1-2-5 table) | Must be valid denomination | `MarketCreationParams.collateral_per_pair` |
| Settlement date | Calendar + time picker | Must be in the future | `MarketCreationParams.expiry_time` (snapped to 60-block boundary) |

**Submit flow**: Validate → `build_creation_pset(params, funding)` → sign `UnblindedPset` → broadcast → await confirmation → `ingest_market` → publish Nostr event → redirect to detail view.

---

## Wallet View (`state.view === "wallet"`)

**Layout**: Routed by `state.walletStatus`:

| Status | Component | Content |
| --- | --- | --- |
| `not_created` | Setup | Password + mnemonic creation form |
| `locked` | Locked | Lock icon + password input |
| `unlocked` | Unlocked | Full wallet dashboard |

### Unlocked Wallet Dashboard

| Section | Source | Content |
| --- | --- | --- |
| Balance header | `walletData.balance[policyAssetId]` | L-BTC balance + fiat equivalent + hide toggle |
| Action buttons | — | "Receive" and "Send" buttons opening modals |
| Activity list | `walletData.transactions` + `recentTxLabels` | Chronological tx list with deadcat labels |
| UTXO explorer | `walletData.utxos` (toggleable) | Raw UTXO list for advanced users |

### State Mapping: Core Types → UI States

| Core type | UI representation |
| --- | --- |
| `MarketState::Trading { outstanding_pairs: 0 }` | "Live" badge, no volume indicator |
| `MarketState::Trading { outstanding_pairs: N }` | "Live" badge, volume = `N * collateral_per_pair` |
| `MarketState::ResolvedYes` | Emerald "Resolved YES" badge + check icon |
| `MarketState::ResolvedNo` | Rose "Resolved NO" badge + X icon |
| `MarketState::Expired` | Amber "Expired" badge + clock icon |
| `LmsrPoolState::Active { reserves, s_index }` | Green dot, reserve bars, implied price from s_index |
| `LmsrPoolState::Closed` | Gray "Closed" badge |
| `OrderState::Active { offered, total_filled }` | Progress bar: `total_filled / offered` with percentage |
| `OrderState::Consumed` | Green "Filled" badge |
| `OrderState::Cancelled { total_filled }` | Gray "Cancelled" badge + partial fill note if `total_filled > 0` |
| `TradeQuote` (partial fill) | Amber warning: "Only X of Y can be filled" |
| `CoreError::InsufficientFunds` | Red inline errors listing each shortfall |
| `CoreError::NoLiquidity` | "No liquidity" message, trade button disabled |
| `CoreError::StaleQuote` | Auto re-quote + flash "Price updated" |
| `ProcessedTransaction` (from `step`) | Success toast with txid + label from `TransitionDetails` |
| `InterpretedTransaction` (unconfirmed) | Amber "Pending" badge with spinner |
