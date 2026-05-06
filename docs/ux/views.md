# View Specifications & State Mapping

View-by-view interaction specs for Deadcat Live. Each view section documents the layout, data sources (mapped to `deadcat-core` types), state transitions, and conditional elements.

The app has four primary views (`ViewMode`): **home**, **detail**, **create**, **wallet** — plus optional **onboarding** and **profile** overlays. Navigation is state-driven: `useStore((s) => s.view)` determines which root component renders in `App.tsx`. Wallet, onboarding, and send/receive modals are overlaid on top of the current view.

**Component locations**:

| View | Component file |
| --- | --- |
| Home | `src/components/home/HomePage.tsx` |
| Detail | `src/components/detail/DetailPage.tsx` |
| Create | `src/components/create/CreateMarketPage.tsx` |
| Wallet overlay | `src/components/wallet/WalletPage.tsx` |
| Onboarding overlay | `src/components/onboarding/OnboardingOverlay.tsx` |
| Profile page | `src/components/profile/ProfilePage.tsx` |
| Shell/header | `src/components/layout/TopShell.tsx` |

---

## Onboarding Overlay

**Entry condition**: `setupModalOpen === true` in Zustand store. Triggered when the user attempts an action requiring identity or wallet. Never shown on first launch — the app opens directly to home in guest mode.

**Layout**: `OnboardingOverlay.tsx` renders a fixed backdrop (`z-50`) over the current view, routing to `NostrSetupStep` or `WalletSetupStep` based on `onboardingStep`.

**Step routing**:

| `onboardingStep` | Component | Shown when |
| --- | --- | --- |
| `"nostr"` | `NostrSetupStep.tsx` | User has no Nostr identity |
| `"wallet"` | `WalletSetupStep.tsx` | User has identity but no wallet |

### Step 1: Nostr Identity (`NostrSetupStep.tsx`)

| Element | Zustand source | Behavior |
| --- | --- | --- |
| Mode toggle | `onboardingNostrMode: "generate" \| "import"` | Switches between generate and import sub-views |
| Generate flow | — | Calls `invoke("generate_nostr_identity")` → stores in `onboardingPendingPubkey`/`onboardingPendingNpub`. Shows nsec reveal + acknowledgment checkbox |
| Import flow | `onboardingNostrNsec` | Paste field; validates via `invoke("import_nostr_nsec")`; shows derived npub |
| Nsec reveal | `onboardingNsecRevealed` | Amber warning + hidden value; `invoke("export_nostr_nsec")` on reveal |
| Acknowledgment checkbox | `onboardingNsecAcknowledged` | Required for generate flow to proceed |
| Continue button | Gated by mode-specific validation | Commits identity to `nostrPubkey`/`nostrNpub`; advances to wallet step if needed |
| Step indicator | `!onboardingWalletOnly` | Two-circle progress bar shown only for combined identity+wallet flows |

### Step 2: Wallet (`WalletSetupStep.tsx`)

| Element | Zustand source | Behavior |
| --- | --- | --- |
| Backup scan | `onboardingBackupScanning` | Runs automatically on open via `invoke("check_nostr_backup")`. Blocking spinner |
| Backup found | `onboardingBackupFound` | If true, shows restore-from-Nostr-backup card(s) as primary option |
| Mode selector | `onboardingWalletMode: "create" \| "restore" \| "nostr-restore"` | Options presented after scan completes |
| Password fields | `onboardingWalletPassword`, `onboardingWalletPasswordConfirm` | Min 8 chars. Confirm must match |
| Mnemonic display | Generated 12 words | Shown for create mode. Word verification step before continuing |
| Mnemonic input | `onboardingWalletMnemonic` | 12/24 word textarea for restore mode |
| Wallet-only indicator | `onboardingWalletOnly` | Hides step indicator when wallet modal opened independently by signed-in user |
| Finish button | Validates all fields | Calls `useCreateWallet` / `useRestoreWallet` mutation; on success: closes overlay, user returned to prior context |

**Exit**: On success, `setupModalOpen` is set to `false`. The user sees exactly what they left — same view, same market, same trade inputs — because Zustand preserves `selectedMarketId`, `tradeSizeSats`, `selectedSide`, etc. across the overlay.

---

## Home View (`view === "home"`)

**Component**: `src/components/home/HomePage.tsx`
**Data**: `useMarkets()` React Query hook — calls `discover_contracts` (with identity) or `list_contracts` (guest/fallback). Markets are filtered/sorted client-side.

**Layout**: Top shell (`TopShell.tsx`) + category tab row + main content area.

### Header (`TopShell.tsx`)

| Element | Zustand source | Behavior |
| --- | --- | --- |
| Logo | — | Click → `useStore.setState({ view: "home", selectedMarketId: "" })` |
| Search input | `search` | `SearchBar.tsx` filters markets client-side by question text |
| Portfolio icon | `activeCategory` | Icon button (bar chart SVG); visible only when `nostrPubkey` set; click → `activeCategory = "Portfolio"` |
| Wallet button | `walletStatus`, `walletData?.balance`, `walletNewTxids` | See wallet button states table below |
| User menu | `nostrPubkey` | Profile avatar → `UserMenu.tsx` dropdown; visible only when signed in |

**Wallet button states** (`WalletButton` component):

| State | Condition | Appearance | Action |
| --- | --- | --- | --- |
| Guest | `nostrPubkey === null` | Filled emerald "Get started" button | Opens setup modal at identity step |
| Identity-only | `nostrPubkey` set, `walletStatus === "not_created"` | Text link "Set up wallet" (emerald) | `walletOpen = true` |
| Locked | `walletStatus === "locked"` | Circular icon button with lock badge | `walletOpen = true` |
| Unlocked | `walletStatus === "unlocked"` | Circular icon button + compact sats balance | `walletOpen = true` |

The wallet button border animates when transactions are pending or new:
- **Pulsing emerald border**: unconfirmed incoming transaction (`height == null && balanceChange > 0`)
- **Pulsing white border**: unconfirmed outgoing transaction
- **Solid emerald border**: confirmed transaction not yet seen (`walletNewTxids.size > 0`); clears on open

**Header states (right side)**:

| User state | Right side |
| --- | --- |
| Guest (no identity) | `[Get started]` filled emerald button |
| Signed in, no wallet | `[Set up wallet]` text link |
| Signed in, wallet locked | Portfolio icon · Wallet icon (with lock) · Avatar |
| Signed in, wallet unlocked | Portfolio icon · Wallet icon + sats balance · Avatar |

**Logout modal** (`LogoutModal` component): Opens via user menu. Two variants:
- *Identity-only* (no wallet): simple confirmation, removes Nostr keys from device.
- *With wallet*: requires downloading a `.dcid` backup file (encrypted with wallet password) before the "Log Out" button enables. User must check "I have downloaded my backup" checkbox to confirm.

### Category Row (`CategoryBar`)

Horizontal scrollable pill bar. Active category stored in `activeCategory` (Zustand). Clicking a pill sets `activeCategory` and resets `selectedMarketId`.

| Category | Visibility | Filter / sort logic |
| --- | --- | --- |
| Trending | Always | `getTrendingMarkets()` — sorted by volume + 24h change; two-column layout with featured carousel + sidebar |
| Ending Soon | Always | `getEndingSoonMarkets()` — ascending `expiryHeight - currentHeight` |
| New | Always | `getNewMarkets()` — most recently created |
| Politics, Sports, Culture, Bitcoin, Weather, Macro | Always | `market.category === activeCategory`; category page three-column layout |
| My Markets | Only when `nostrPubkey && marketMakerMode` | Markets where `nostrPubkey === market.oraclePubkey` |
| Portfolio | Not in category bar — header icon button only | Markets where user holds YES/NO token positions |

### Trending View (`TrendingHomeView`)

Two-column layout (`xl:grid-cols-[1.618fr_1fr]`).

**Left column**:
- `FeaturedMarket.tsx`: Featured market carousel cycling through `getTrendingMarkets()` via `trendingIndex`. Prev/next arrows. Shows: category + LIVE badge + state badge, question, YES/NO buy buttons (or resolved/expired badge), volume · time-left · 24h change, description (2-line clamp), full `MarketChart.tsx`. Falls back to `generateMockPriceHistory(market)` when no real pool history exists.
- "Top Markets" grid: `md:grid-cols-2` grid of `MarketCard.tsx` — top 6 from `getFilteredMarkets("Trending")`, respects the `search` input.

**Right column** (sidebar):
- **Trending** panel: Top 3 from `getTrendingMarkets()` as `SidebarMarketItem` rows. Each row shows question, Yes%/No% buy buttons, 24h change indicator.
- **Top Movers** panel: Top 3 markets by `|change24h|` as `SidebarMarketItem` rows.

**Empty state**: If `trending.length === 0`, renders `EmptyState` (see below).

**Loading state**: While `isLoading && markets.length === 0`, renders `MarketLoader` — an animated cat-falls-into-bag SVG with "Fetching markets…" pulsing text. Not a skeleton — the full animation plays on first load only.

### Market Card Anatomy (`MarketCard.tsx`)

```
┌─────────────────────────────────────────┐
│ [Category] [· LIVE]        [time left]  │
│                                         │
│ Will BTC hit $200k by 2027?             │
│                                         │
│  72%                                    │
│                                         │
│ ──── sparkline (real price history) ─── │
│                                         │
│ [Yes 72%] [No 28%]          [↑ +4.2%]  │
│ 0.42 BTC vol · 1,240 traders · 3d left  │
└─────────────────────────────────────────┘
```

**Sparkline** (`ChartSparkline` component): Fetches `usePriceHistory(market.marketId)`. Falls back to `generateMockPriceHistory(market)` when no real pool history. Plots `implied_yes_price_bps / 10000` per entry as a polyline (80×24 SVG). Color: emerald when end ≥ start, rose when declining.

**Resolved / expired state**: YES/NO buy buttons are replaced by a single "Resolved YES" / "Resolved NO" / "Expired" badge.

**Footer**: `{volume} vol · {traderCount} traders · {timeLeft}`. `traderCount` row is omitted if `market.traderCount === 0`.

Click card → `openMarket(market)` → `useStore.setState({ view: "detail", selectedMarketId: market.id })`.

### Category Page View (`CategoryPageView`)

Rendered for named category tabs (Politics, Sports, Culture, Bitcoin, Weather, Macro). Three-column layout (`xl:grid-cols-[233px_1fr_320px]`).

**Left sidebar** (desktop only):
Three filter pills that set `categoryFilter` (Zustand):
- All markets (default)
- Live now (`m.isLive === true`)
- Ending soon (`m.isLive && expiryHeight - currentHeight ≤ 200 blocks`)

Active filter pill: `bg-slate-900/70 text-emerald-300`.

**Center column**:
- Heading + sort buttons: "Trending" (sort by `volumeBtc` desc) and "Frequency" (sort by `traderCount` desc). Active sort button: `border-slate-500 text-slate-100`. Sort stored in `categorySortMode` (Zustand).
- Stats row: Contracts (count after filter applied), Live now, 24h volume (sum of all category markets regardless of filter).
- `md:grid-cols-2` grid of `MarketCard.tsx` — filtered by `categoryFilter`, sorted by `categorySortMode`.

**Right sidebar**:
- **Live contracts**: Up to 4 live markets; each shows question, Yes% price, volume.
- **Highest liquidity**: Top 4 by `liquidityBtc`; ranked list with liquidity value.
- **Market states**: Breakdown by state using user-friendly labels: Dormant, Live (emerald), Resolved YES, Resolved NO, Expired (amber).

### Portfolio View (`PortfolioView`)

Shown when `activeCategory === "Portfolio"` (requires `nostrPubkey`). Only accessible via the header Portfolio icon — not in the category bar.

**Stats row** (3 cards): Portfolio Value (summed position values in L-BTC), Active Positions (count), Settled (count of resolved/expired positions held).

**Active Positions section**: Cards for each market where user holds YES or NO tokens. Each card shows:
- Question (click → opens market detail)
- Yes% price
- YES token quantity / NO token quantity
- Total value in sats (`tokenCount × pricePerToken`)

**Settled Positions section**: Cards for resolved/expired markets. Shows Won/Lost badge, YES/NO token quantities. "Won" when the market resolved in the direction the user holds.

**Empty state**: If no positions found, shows descriptive message and "Browse Markets" button.

### My Markets View (`MyMarketsView`)

Shown when `activeCategory === "My Markets"`. Only visible in the category bar when `nostrPubkey && marketMakerMode`. Displays markets where `nostrPubkey === market.oraclePubkey`.

**Stats row** (3 cards): Total markets, Active markets, Awaiting resolution (active markets past expiry).

**State sections** (each section only rendered if non-empty):
- "Dormant — needs initial issuance" (`state === 0`)
- "Active" (`state === 1`)
- "Expired" (`state === 4`)
- "Resolved" (`state === 2 || 3`)

Each market card shows: category, state badge (DORMANT / UNRESOLVED / RESOLVED YES / RESOLVED NO / EXPIRED), question, Yes%/No% prices.

**Empty state**: "No markets created yet" with optional "+ Create Market" button when `marketMakerMode`.

**Header action**: "+ Create New Market" button appears at top-right when `marketMakerMode`.

### Empty State (`EmptyState`)

Shown when `trending.length === 0` on the Trending view. Icon + "No markets yet" heading + description. Conditional CTAs:
- If no identity: "Set Up Account" → opens setup modal at identity step.
- If `marketMakerMode`: "Create Market" → `view = "create"`.

---

## Detail View (`view === "detail"`)

**Component**: `src/components/detail/DetailPage.tsx`
**Data**: Market from `useMarkets()` filtered by `selectedMarketId`. Price history from `usePriceHistory(marketId)`. Orders from `useMarketOrders(marketId)`.

**Layout**: Two-column on desktop (chart + info left, trading panel right). Single column on mobile.

### Left Column: Market Info + Chart (`MarketHeader.tsx`, `MarketChart.tsx`)

| Element | Source | Behavior |
| --- | --- | --- |
| Question | `market.question` | Large heading |
| State badge | `market.state` (`MarketState` variant) | "Live" (green), "Resolved YES" (emerald), "Resolved NO" (rose), "Expired" (amber) |
| Description | `market.description` | Collapsible paragraph |
| Resolution source | `market.resolutionSource` | Link or text |
| Settlement date | `market.expiryHeight` → estimated date | "Settles ~June 15, 2027 (block 2,150,400)" |
| Contract size | `market.cptSats` | "5,000 sats per contract" |
| Price chart | `PriceHistoryEntry[]` from `usePriceHistory()` | `MarketChart.tsx` SVG with hover, timescale buttons |
| Orderbook | `useMarketOrders()` | `OrderbookPanel.tsx` — toggleable bid/ask levels |

### Price Chart Spec (`MarketChart.tsx`)

- **Axes**: Y = YES/NO probability (0–100%), X = block height
- **Series**: Two polylines — YES (emerald) and NO (rose) — with gradient fill
- **Markers**: Cat-paw SVG silhouettes along each series; cat-logo markers at endpoints
- **Live indicator**: Pulsing animation at the latest data point
- **Hover**: Vertical crosshair + tooltip (probability + block height). Tracked via `chartHoverMarketId` + `chartHoverX` in Zustand
- **Timescale**: 10B / 25B / 50B / 100B buttons. Updates `chartTimescale` in store
- **Aspect ratio**: Measured from container via `ResizeObserver` → `chartAspectDetail` in store

### Right Column: Trading Panel (`TradingPanel.tsx`)

**Action tabs** (conditional on market state and user role):

| Tab | Visible when | Core operation |
| --- | --- | --- |
| Trade | Market is `Trading` | `useQuoteTrade` + `useExecuteTrade` |
| Issue | `marketMakerMode` + `Trading` | `useIssueTokens` |
| Redeem | `ResolvedYes`/`ResolvedNo`/`Expired` + user holds tokens | `useRedeemTokens` |
| Cancel | `marketMakerMode` + `Trading` + user holds YES+NO tokens | `useCancelTokens` |

**Trade tab layout**:

```
┌─────────────────────────────────────┐
│  [YES ██████]  [NO ░░░░░░]          │  ← Side selector
│                                     │
│  [Market ▼] [Limit]                 │  ← Order type (marketMakerMode)
│                                     │
│  [Open ○] [Close ○]                 │  ← Trade intent
│                                     │
│  Amount: [________] [sats|contracts]│  ← Size input + mode toggle
│                                     │
│  ┌─────────────────────────────────┐│
│  │ Price:     72%                  ││
│  │ You pay:   10,000 sats          ││  ← Quote result
│  │ You get:   ~13.8 contracts      ││
│  │ Est. fee:  ~250 sats            ││
│  └─────────────────────────────────┘│
│                                     │
│  [     Buy YES     ]                │  ← Opens QuoteModal
└─────────────────────────────────────┘
```

**State flow**: Input change → debounce 300ms → `quote_trade` → display quote → user clicks Buy → confirm modal → `build_trade_pset` → prepare/blind → sign → broadcast → pending toast → `step` confirms → success toast.

### Quote Confirm Modal (`QuoteModal.tsx`)

| Field | Source |
| --- | --- |
| Direction | "Buying YES" / "Selling NO" etc. |
| Amount | `TradeQuote.total_input` (formatted) |
| Tokens | `TradeQuote.total_output` (formatted) |
| Price | `TradeQuote.effective_price` as probability % |
| Fee | `TradeQuote.estimated_fee` (with "estimate" disclaimer) |
| Timer | 30-second countdown on quote validity. On expiry: auto-re-quote + "Price updated" flash |
| Route legs | `TradeQuote.legs` — advanced mode toggle |
| Confirm button | Triggers `useExecuteTrade` mutation |
| Cancel button | Closes modal, clears `tradeQuoteSnapshot` |

### Oracle Resolution Panel

**Visible only when**: `nostrPubkey === market.oraclePubkey` AND market state is `Trading`.

```
┌─────────────────────────────────────┐
│ ⚖ You are the oracle for this      │
│   market.                           │
│                                     │
│ [Resolve YES]    [Resolve NO]       │
└─────────────────────────────────────┘
```

Calls `useMarketOps` mutation (oracle attest + execute resolution).

---

## Create View (`view === "create"`)

**Component**: `src/components/create/CreateMarketPage.tsx`
**Entry**: "Create Market" button in header (visible when `marketMakerMode`). Requires identity + wallet.

**Layout**: Centered form card with sub-components `CategoryDropdown.tsx` and `SettlementPicker.tsx`.

| Field | Input type | Validation | Maps to |
| --- | --- | --- | --- |
| Question | Text input (required) | Min 10 chars | Nostr event content |
| Description | Textarea | Optional | Nostr event content |
| Category | `CategoryDropdown` | Required | Nostr event tag |
| Resolution source | Text input | Optional | Nostr event content |
| Collateral per pair | Constrained dropdown (1-2-5 table) | Must be valid denomination | `MarketCreationParams.collateral_per_pair` |
| Settlement date | `SettlementPicker` (calendar + time) | Must be in the future | `MarketCreationParams.expiry_time` (rounded up to 60-block boundary) |

**Submit flow**: Validate → `build_binary_market_creation_pset(params, funding)` → prepare/blind `PreBlindedPset` → sign → broadcast → await confirmation → `ingest_market` → publish Nostr event → redirect to detail view.

---

## Wallet Overlay (`walletOpen === true`)

**Component**: `src/components/wallet/WalletPage.tsx`
**Layout**: Modal overlay. Routes by `walletStatus`:

| Status | Component | Content |
| --- | --- | --- |
| `not_created` | `WalletSetup.tsx` | Create or restore wallet (same UI as onboarding step 2, but standalone) |
| `locked` | `WalletLocked.tsx` | Lock icon + password input → `useUnlockWallet` mutation |
| `unlocked` | `WalletUnlocked.tsx` | Full wallet dashboard |

### Unlocked Wallet Dashboard (`WalletUnlocked.tsx`)

| Section | Data source | Content |
| --- | --- | --- |
| Balance header | `useWalletSnapshot()` → `walletData.balance[policyAssetId]` | L-BTC amount + unit toggle (sats/BTC) + hide toggle |
| Token balances | `walletData.utxos` + `identify_asset` | YES/NO token positions grouped by market |
| Send / Receive | — | Buttons open `SendModal.tsx` / `ReceiveModal.tsx` |
| Activity list | `ActivityList.tsx` | `walletData.transactions` + `recentTxLabels` — chronological with deadcat labels + pagination |
| UTXO list | `UtxoList.tsx` (toggleable) | Raw UTXO list for advanced users |
| Pending swaps | `walletData` Boltz swap entries | Status indicators for in-progress Lightning/Bitcoin swaps |

### Send Modal (`SendModal.tsx`) — Three tabs

| Tab | Operation | Flow |
| --- | --- | --- |
| Lightning | Pay LN invoice via Boltz submarine swap | Paste invoice → decode amount → confirm → `invoke("pay_lightning_invoice")` |
| Liquid | Direct L-BTC transfer | Enter address + amount → confirm → `invoke("send_liquid")` |
| Bitcoin | On-chain BTC via Boltz chain swap | Enter address + amount → confirm → `invoke("create_bitcoin_send")` |

### Receive Modal (`ReceiveModal.tsx`) — Three tabs

| Tab | Operation | Flow |
| --- | --- | --- |
| Lightning | Generate LN invoice via Boltz reverse swap | Enter amount → `invoke("create_lightning_receive")` → show QR + invoice |
| Liquid | L-BTC address | `invoke("generate_liquid_address")` → show QR + address |
| Bitcoin | On-chain BTC via Boltz chain swap | Enter amount → `invoke("create_bitcoin_receive")` → show QR + BTC address |

---

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
