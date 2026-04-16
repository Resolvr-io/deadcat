# User Stories: Trader & Token Holder

Personas covered: **Trader (Taker)** and **Token Holder (Recovery)**. See [ux-design.md](../design.md) for persona definitions.

---

## US-T1: Browse and Discover Markets

**As a** trader, **I want to** browse active prediction markets by category, **so that** I can find opportunities that match my interests.

**Acceptance criteria**:
- Home view calls `list_markets(StateFilter::ActiveOnly, Pagination { after: None, limit: 20 })` on load
- Markets display: question text, YES probability (from pool spot price or `yes_price_bps`), 24h change, volume, liquidity
- Category tabs filter markets (Trending, Politics, Sports, Culture, Bitcoin, Weather, Macro)
- "Resolved" tab calls `list_markets(StateFilter::TerminalOnly, ...)` and shows outcome badges
- "My Markets" tab filters to markets where the user holds YES/NO tokens (wallet UTXO asset IDs matched via `identify_asset`)
- Search filters by question text (client-side on loaded markets)
- Pagination: infinite scroll triggers next page via `next_cursor`

**Interaction design**:
- **Featured market card**: The top trending market gets a large hero card with an embedded price chart (SVG, built from `PriceHistoryEntry` data). Clicking anywhere on the card opens the detail view.
- **Market grid**: Remaining markets in a responsive card grid. Each card shows question, probability pill, sparkline chart, and volume. Click opens detail.
- **Category row**: Horizontal scrollable tab bar. "Trending" is the default. Categories are static — not fetched from the engine.
- **Empty state**: When no markets exist, show "No markets discovered" with a prompt to create one (if market maker mode is on) or to check back later.

---

## US-T2: Buy YES or NO Tokens (Market Order)

**As a** trader, **I want to** buy YES or NO tokens on a market, **so that** I can profit if my prediction is correct.

**Acceptance criteria**:
- Detail view shows a trade composer with YES/NO toggle and amount input
- Selecting a side and entering an amount triggers `quote_trade(market_id, TradeSpec { side, direction: Buy, amount: ExactInput(sats) }, fee_rate)`
- Quote response displays: effective price (as probability %), tokens received (`total_output`), total cost (`total_input`), estimated fee
- "Confirm" button calls `build_trade_pset(quote, funding)` → sign → broadcast
- On `CoreError::StaleQuote`: auto re-quote, flash "Price updated" indicator, show new quote
- On `CoreError::InsufficientFunds { shortfalls }`: display each shortfall ("Need X more sats")
- On `CoreError::NoLiquidity`: display "No liquidity available" and disable the confirm button
- After broadcast: show pending toast via `interpret_transaction` on the unconfirmed tx
- After confirmation via `step`: update wallet balance, show success toast with txid

**Interaction design**:
- **Two-step pattern**: Mirrors core's `quote_trade` → `build_trade_pset` split. Step 1: user enters amount, sees live quote. Step 2: user reviews and confirms in a modal. This prevents accidental trades — the confirm modal shows exact amounts, not just the input.
- **Size input modes**: Toggle between "sats" (how much to spend) and "contracts" (how many tokens to buy). Both map to `TradeAmount::ExactInput` — contracts mode computes the sats equivalent from the current quote price.
- **Side selector**: Two large buttons — "YES" (emerald) and "NO" (rose). The selected side is visually prominent. Switching sides re-quotes automatically.
- **Quote staleness**: Quotes are ephemeral. The UI re-quotes on every input change (debounced 300ms). If the user waits >30s on the confirm modal, show "Quote may be stale — confirm to refresh."
- **Partial fills**: If `TradeQuote.filled_amount < requested_amount`, show warning: "Only X of Y sats can be filled. Proceed with partial fill?" The user explicitly accepts.

---

## US-T3: Sell Tokens (Close Position)

**As a** trader holding YES or NO tokens, **I want to** sell my tokens back to the market, **so that** I can take profit or cut losses.

**Acceptance criteria**:
- Trade composer has an "Open / Close" toggle (maps to `TradeIntent`)
- In "Close" mode, size input defaults to "contracts" mode showing current position size
- Sell quotes use `TradeSpec { side, direction: Sell, amount: ExactInput(tokens) }`
- Quote displays: collateral received (`total_output`), tokens sold (`total_input`), effective sell price
- Position display shows current holdings per side (YES tokens, NO tokens) from wallet UTXOs

**Interaction design**:
- **Intent toggle**: "Open" = buying tokens to open a new position. "Close" = selling tokens to close an existing position. The toggle changes the trade direction and default size mode.
- **Position awareness**: The close tab pre-fills with the user's current token balance for the selected side. "Max" button sets size to full position.
- **Sell price warning**: If the effective sell price is significantly below the current market price (>5% slippage), show an amber warning: "High slippage — you'll receive X% less than spot."

---

## US-T4: View Market Detail and Price Chart

**As a** trader, **I want to** see detailed market information and price history, **so that** I can make informed trading decisions.

**Acceptance criteria**:
- Detail view shows: question, description, resolution source, oracle pubkey (truncated), expiry block height (converted to estimated date), contract size (`collateral_per_pair`), covenant state badge
- Price chart built from `PriceHistoryEntry[]` data (pool transitions with `implied_yes_price_bps`)
- Chart supports timescale selection (10, 25, 50, 100 blocks)
- Hover on chart shows crosshair with price/block tooltip
- Order book section (toggleable) aggregates limit orders into bid/ask levels from `market.limitOrders`

**Interaction design**:
- **Chart**: SVG-based, rendered from `PriceHistoryEntry` data. YES probability on Y-axis (0-100%), block height on X-axis. The current price is highlighted with a pulsing indicator. Cat-paw markers at each data point reinforce the brand. Hover reveals a tooltip with exact probability and block number.
- **State badge**: Maps `MarketState` to visual badge — "Live" (green dot + pulse), "Resolved YES" (emerald check), "Resolved NO" (rose X), "Expired" (amber clock). The badge drives which action tabs are available.
- **Orderbook**: Collapsed by default. When expanded, shows aggregated bid/ask levels as horizontal bars (emerald for bids, rose for asks). Spread displayed between the two sides.
- **Info section**: Collapsible "Advanced details" panel showing oracle pubkey, asset IDs (YES, NO), creation txid, covenant state number. Targeted at advanced users who want to verify on-chain data.

---

## US-T5: Redeem Winning Tokens

**As a** trader holding winning tokens on a resolved market, **I want to** redeem them for collateral, **so that** I receive my payout.

**Acceptance criteria**:
- When `MarketState` is `ResolvedYes` or `ResolvedNo`, the "Redeem" action tab becomes active
- Redeem form shows: tokens to redeem (defaults to full balance), payout amount (`tokens * collateral_per_pair` for winning side)
- Calls `build_redemption_pset(contract_id, winning_side, tokens, funding)`
- On `CoreError::InvalidContractState`: show "This market hasn't been resolved yet"
- For `MarketState::Expired`: show half-value redemption with clear messaging ("Expired markets pay 50% of contract value")

**Interaction design**:
- **Auto-detection**: When the user opens a resolved market where they hold winning tokens, the action tab auto-switches to "Redeem" with a prominent "Claim your payout" banner.
- **Payout breakdown**: Show clearly: "X tokens x Y sats/contract = Z sats payout" (minus network fee). For expired markets: "X tokens x Y sats/contract x 50% = Z sats payout."
- **Losing side**: If the user holds losing tokens on a resolved market, show a muted message: "This market resolved [YES/NO]. Your [NO/YES] tokens have no redemption value." No redeem button.

---

## US-T6: View and Label Transaction History

**As a** trader, **I want to** see my transaction history with meaningful labels, **so that** I can track my trading activity.

**Acceptance criteria**:
- Wallet activity view shows all transactions from LWK wallet
- Each transaction is labeled via `interpret_transaction`: "Bought 10 YES on [market question]", "Redeemed 5 NO from [market]", "Issued 100 pairs on [market]"
- Labels use `TransitionDetails` variants: `MarketTransition::Issued` → "Issued", `PoolTransition::Swapped` → "Trade", `OrderTransition::Filled` → "Order filled"
- Unknown transactions (not matching any tracked contract) show as plain L-BTC transfers
- Labels persist across sessions via localStorage

**Interaction design**:
- **Labeling pipeline**: On each `StepReport`, iterate `ProcessedTransaction.interpretation.transitions` and generate human-readable labels. Store in `recentTxLabels` map (txid → label).
- **Pending transactions**: Show with amber "Pending" badge and spinner. Label from `interpret_transaction` on the unconfirmed tx. Badge clears on next `step` confirmation.
- **Explorer link**: Each transaction row has a small external-link icon that opens the txid in a Liquid block explorer.

---

## US-TH1: Recover Token Positions from Mnemonic

**As a** token holder restoring a wallet from mnemonic, **I want to** see and redeem my prediction market tokens, **so that** I can recover my funds without external services.

**Acceptance criteria**:
- After wallet restore, standard rescan finds YES/NO token UTXOs (they're normal confidential assets at wallet addresses)
- Unknown asset IDs trigger the recovery flow: `ChainSource::issuance_transaction(asset_id)` → market creation tx → read OP_RETURN → reconstruct `PredictionMarketParams` → `ingest_market` → `identify_asset`
- After recovery, tokens display with proper names: "YES — Will BTC hit $200k by 2027?"
- Redemption is available if the market is resolved/expired

**Interaction design**:
- **Unknown asset indicator**: Wallet balance shows unknown assets with a "?" icon and the raw asset ID (truncated). Tapping shows "This appears to be a prediction market token. Tap to identify."
- **Automatic recovery**: The app attempts automatic identification in the background on wallet restore. If successful, the "?" icon is replaced with the proper token name. No user action needed for the happy path.
- **Manual fallback**: If automatic recovery fails (e.g., chain source unavailable), the user can trigger manual identification from the asset detail view.
