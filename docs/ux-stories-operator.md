# User Stories: Pool Operator & Order Maker

Personas covered: **Pool Operator** and **Order Maker**. See [ux-design.md](ux-design.md) for persona definitions. Both personas require "Market maker mode" enabled in settings.

---

## US-PO1: Create an LMSR Liquidity Pool

**As a** pool operator, **I want to** bootstrap an LMSR pool for a market, **so that** traders have liquidity to trade against.

**Acceptance criteria**:
- Pool creation form (accessible from market detail in maker mode) collects: max loss (sats), half payout (sats), fee (bps), starting price (bps)
- `estimate_bootstrap(max_loss_sats, half_payout_sats, starting_price_bps)` shows required capital: initial YES reserve, initial NO reserve, initial collateral reserve
- `derive_pool_params(xprv, market_params, pool_index, max_loss_sats, half_payout_sats, fee_bps, starting_price_bps)` generates params + masked index
- On `ConventionError`: display which constraint was violated (e.g., "max_loss_sats must fit the 26-value mantissa encoding")
- `build_lmsr_bootstrap_pset(params, starting_price_bps, masked_index, funding)` → sign → broadcast
- After confirmation: `ingest_pool(params, PoolSnapshot::Creation(creation_tx))` to begin tracking with full history

**Interaction design**:
- **Capital preview**: Before creating, `estimate_bootstrap` shows a breakdown: "To start a pool at 50% YES price with 100k sats max loss, you need: 50 YES tokens, 50 NO tokens, 95,000 sats collateral. Total capital: ~195,000 sats." This helps the operator plan capital acquisition (they may need to issue token pairs first).
- **Parameter constraints**: All inputs are constrained to convention-valid values. Max loss and half payout use dropdowns or validated inputs matching the 26-value mantissa set. Fee uses a slider (0-40.95%, 0.01% steps). Starting price uses a slider (1-99%).
- **Risk disclosure**: Show max loss prominently: "Your maximum possible loss from this pool is X sats. This occurs if the price moves from Y% to 0% or 100%."

---

## US-PO2: Monitor Pool Performance

**As a** pool operator, **I want to** see my pool's current state and historical performance, **so that** I can decide whether to adjust or close it.

**Acceptance criteria**:
- "My Pools" section lists all pools the user operates (from `listLmsrPools`)
- Each pool shows: market question, current reserves (`PoolReserves`), current implied price (from `s_index`), pool state (Active/Closed)
- Pool detail shows `pool_history` entries: each swap (old/new s_index, reserve changes), each admin adjustment, close event
- Fee revenue is derivable from swap history (collateral increase from swap fees)

**Interaction design**:
- **Pool card**: Shows market question, current YES price, reserves bar chart (YES/NO/collateral as stacked horizontal bars), and a "net P&L" estimate.
- **History timeline**: Chronological list of pool transitions. Swaps show: "Swap: price moved 45% → 52%, +150 sats fee revenue." Adjustments show: "Added 5,000 sats collateral." Close shows: "Pool closed, reclaimed X/Y/Z."

---

## US-PO3: Adjust Pool Liquidity

**As a** pool operator, **I want to** add or remove liquidity from my pool, **so that** I can manage my capital exposure.

**Acceptance criteria**:
- Adjust form on pool detail shows current reserves and allows delta inputs
- `build_lmsr_adjust_pset(contract_id, pair_delta, collateral_delta, funding)` — pair_delta applied equally to YES and NO reserves
- The UI presents absolute target inputs and computes deltas internally: "Set YES/NO reserves to X" → `pair_delta = X - current`
- On `CoreError::InvalidParams` (zero deltas or below minimum reserve floor): show specific error
- After confirmation: reserves update in pool detail

**Interaction design**:
- **Absolute targets**: The user enters target reserve values, not deltas. The UI computes `pair_delta` and `collateral_delta` from the difference between current and target. This matches operator mental model ("I want 200 tokens in my pool") vs the covenant's delta model.
- **Reserve floor warning**: If the target would put any reserve below `MIN_POOL_RESERVE` (1,000 sats), show error: "Each reserve must be at least 1,000 sats."
- **Price impact preview**: Show how the adjustment changes the implied price. "Current price: 65%. After adjustment: 65% (no change)" — because admin adjustments preserve the s_index.

---

## US-PO4: Close a Pool

**As a** pool operator, **I want to** close my pool and reclaim all reserves, **so that** I can withdraw my capital.

**Acceptance criteria**:
- Close button on pool detail (only for Active pools)
- `build_lmsr_close_pset(contract_id, funding)` → sign → broadcast
- All three reserve UTXOs reclaimed atomically to `funding.return_script`
- After confirmation: pool state transitions to `Closed`

**Interaction design**:
- **Confirmation**: "Close this pool and reclaim: X YES tokens, Y NO tokens, Z sats collateral. The pool will no longer provide liquidity for this market."
- **Irreversibility**: "This cannot be undone. To provide liquidity again, you'll need to create a new pool."

---

## US-OM1: Place a Limit Order

**As an** order maker, **I want to** place a limit order at a specific price, **so that** I can buy or sell tokens at my preferred price.

**Acceptance criteria**:
- Limit order composer (on market detail, "Limit" order type tab) collects: side (YES/NO), direction (buy/sell), price (sats per contract), amount (sats to lock)
- `derive_order_params(xprv, market_params, order_index, side, direction, price, min_fill_lots, min_remainder_lots)` generates params + masked index
- On `ConventionError`: display constraint violation (e.g., "price must be ≤ 16,777,215 (u24 max)")
- `build_create_order_pset(params, offered_amount, masked_index, funding)` → sign → broadcast
- After confirmation: order appears in "My Orders" list. Also published as a Nostr event for discovery by takers.

**Interaction design**:
- **Price input**: Entered as a probability (1-99%). Internally converted to sats per contract (`price_pct * collateral_per_pair / 100`). The UI validates the result fits in u24.
- **Sell warning**: When selling tokens the user holds, check if the limit price is significantly below the current market price. If so, show warning: "Your limit price (X%) is Y% below the current market price. Your order may fill immediately at a loss."
- **Order summary**: Before confirmation: "Place limit [BUY/SELL] order: [X] [YES/NO] tokens at [Y]% ([Z sats/contract]). Locking [W sats]."

---

## US-OM2: Monitor and Cancel Orders

**As an** order maker, **I want to** monitor my open orders and cancel unfilled ones, **so that** I can manage my open positions.

**Acceptance criteria**:
- "My Orders" list shows all orders from `fetchOwnOrders`: market, direction, price, offered amount, fill status, order state
- Fill progress: `OrderState::Active { offered_amount, total_filled }` → progress bar showing "X of Y sats filled"
- Cancel button calls `build_cancel_order_pset(contract_id, funding)` → sign → broadcast
- After cancellation: `OrderState::Cancelled { total_filled }` — show "Cancelled (X of Y filled before cancellation)"
- Consumed orders (`OrderState::Consumed`): show "Fully filled" with green check

**Interaction design**:
- **Order card**: Shows market question, side badge (YES/NO), price, fill progress bar, and state badge (Active/Filled/Cancelled).
- **Fill notifications**: When `step` reports an `OrderTransition::Filled { fill_amount }`, show a toast: "Your [YES/NO] order was filled for [X sats]."
- **Cancel confirmation**: "Cancel this order? You'll reclaim [remaining_amount] sats. [filled_amount] sats were already filled."
- **Batch view**: Orders grouped by market. Within each market, sorted by price. This lets the maker see their full order book at a glance.
