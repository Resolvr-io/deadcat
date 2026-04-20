# Trade Routing Algorithm

## Overview

The `quote_trade` engine method computes the optimal route for a trade across all available LMSR pools and limit orders for a **specific `(market, outcome, side)` combination**. The algorithm minimizes total cost to the taker, including transaction fees — which depend on how many liquidity sources are included in the route.

The external interface is simple: `TradeSpec { outcome, side, direction, amount }` in, `TradeQuote` out. This document specifies the internal routing algorithm.

**Scoping**: routing operates on a single outcome's YES/NO pair at a time. For binary markets, there is only one outcome (`OutcomeIndex::BINARY`), so the `outcome` axis is trivial. For multi-outcome markets composed via Option C (N independent binary LMSR pools per market, one per outcome's YES/NO pair — see [`amm-scoring-rule-tradeoffs.md`](../contracts/multi-outcome/amm-scoring-rule-tradeoffs.md)), each trade targets one outcome's liquidity: the pools and maker orders for that outcome's YES_k / NO_k assets. The algorithm is identical per-outcome — this document is "the algorithm for one outcome's liquidity" with no special multi-outcome semantics at the routing layer itself.

**Multi-outcome operations that don't route through this algorithm**: basket trades (minting or burning complete YES/NO sets) are direct primitives on `MultiOutcomeMarket`. See [`deadcat-core-design.md § MultiOutcomeMarket`](deadcat-core-design.md#multioutcomemarket) for details. Cross-outcome arbitrage (closing `Σ p_YES_k ≠ 1` gaps by atomically composing the market's split-YES/merge-YES primitive with per-outcome pool swaps) will get its own quote/build flow on `MultiOutcomeMarket` in v2; v1 treats externally-broadcast arb txs as ordinary multi-contract transactions (per-contract transitions preserved, no aggregate arb classification). See [`deadcat-core-design.md § Future: Cross-Outcome Arb API (v2)`](deadcat-core-design.md#future-cross-outcome-arb-api-v2). Neither case is routed through `quote_trade`, which handles only single-outcome trades.

## Algorithm Structure

The router uses **pool-subset enumeration × fee-aware greedy order selection**, scoped to the trade's target outcome:

1. **Pre-select candidate pools**: Rank all active pools for the target outcome by estimated average fill price for the requested amount (one LMSR computation per pool). Take the top N (N = 5).
2. **Enumerate pool subsets**: For each subset of the N candidate pools (including the empty set — no pools), run the fee-aware greedy order selection.
3. **Pick the best result**: The subset that produces the lowest total cost (fill cost + transaction fee) wins.

Pool subset enumeration ensures the pool inclusion decision is optimal — no greedy heuristic for whether to activate a pool. Order selection within each subset is greedy, which is near-optimal for fixed-price sources with independent per-source costs.

## Fee-Aware Greedy Order Selection

For a given pool subset, the greedy fills the requested amount by iterating over candidate sources (included pools + top-K price-sorted orders) and selecting the source with the best **fee-adjusted effective price** at each step:

```
effective_price(source) = (fill_cost + marginal_tx_weight × fee_rate) / fill_amount
```

Where:
- `fill_cost` = raw cost of filling `fill_amount` from this source at its price
- `marginal_tx_weight` = incremental transaction weight from adding this source
- `fee_rate` = from `WalletFunding.fee_rate`

The marginal weight captures the fixed cost of activating each source:
- **Pool (first use)**: ~1000 vbytes (3 reserve inputs + 3 reserve outputs + Simplicity witnesses)
- **Pool (already in route)**: 0 vbytes (inputs/outputs already accounted for — additional volume is free)
- **Limit order**: ~400 vbytes (1 order input + 1-2 outputs + witness)

This formula naturally handles the "one large order at a slightly worse price beats five small orders at a better price" case — the per-source fee amortized over a small fill dominates the price advantage.

### Natural stopping condition

The greedy stops adding sources when:
- The request is fully filled, or
- No remaining source has a fee-adjusted effective price that improves the route, or
- The transaction weight cap is reached (see below)

A partial fill (filled_amount < requested_amount) is returned to the caller via `TradeQuote`. The caller decides whether to proceed.

## Pool Pre-Selection

With P active pools for the trade's target outcome, full subset enumeration costs 2^P. To bound this:

1. For each pool serving the target outcome, compute the **estimated average fill price** for the full requested amount via cached F-value lookups. This is O(1) per pool assuming the combo's table is cached (see [Integer Precision and Caching](#integer-precision-and-caching) for the bignum caching strategy; first-use per combo incurs a ~5-10s table generation cost).
2. Rank pools by this estimate (lower = better).
3. Take the top N pools (N = 5 constant). Discard the rest.

Ranking by average fill price (not spot price) ensures deep pools with slightly worse spot prices are preferred over shallow pools with great spot prices for large trades. A shallow pool at 50.00 that slips to 55.00 over 1000 tokens ranks below a deep pool at 50.50 that barely moves.

**Pool flooding defense**: An attacker creating 100 pools for a single outcome to slow routing only causes 100 LMSR lookups in the pre-selection step (microseconds). The top-5 filter bounds the enumeration at 2^5 = 32 regardless of total pool count. For multi-outcome markets, the attacker can't flood "all outcomes at once" to amplify the attack — routing scopes to one outcome's pool set, so flooding other outcomes' pool sets has no effect on this trade's routing cost.

## Order Constraints

### Minimum fill and remainder

Each maker order has `min_fill_lots` (minimum per fill) and `min_remainder_lots` (minimum remaining after a partial fill). The greedy computes the **actual fillable amount** before evaluating an order's fee-adjusted price:

```
available = order.remaining_locked
desired = min(remaining_request, available)

if desired < order.min_fill_lots:
    skip — can't meet minimum fill

if desired < available:
    // Partial fill — check remainder constraint
    remainder_after = available - desired
    if remainder_after < order.min_remainder_lots:
        // Partial fill would violate remainder. Options:
        reduced = available - order.min_remainder_lots
        if reduced >= order.min_fill_lots and reduced <= remaining_request:
            actual_fill = reduced  // leave exactly min_remainder
        else if remaining_request >= available:
            actual_fill = available  // full consumption (no remainder constraint)
        else:
            skip — can't satisfy both constraints
    else:
        actual_fill = desired
else:
    actual_fill = available  // full consumption
```

The fee-adjusted price uses `actual_fill`, not `desired`. An order with tight constraints that can only be partially filled for a small amount gets a proportionally worse fee-adjusted price — the greedy naturally deprioritizes it.

### Dust order defense

`best_orders_for_market` takes a `min_remaining` parameter, computed by the engine from the fee rate:

```
min_remaining = ORDER_MARGINAL_WEIGHT × fee_rate × DUST_MULTIPLIER
```

Where `DUST_MULTIPLIER` is a constant (e.g., 2-3×) ensuring the order's depth meaningfully exceeds its activation cost. This filters dust orders at the store level, preventing an attacker from filling the top-K query slots with tiny orders that crowd out legitimate liquidity.

## Quote Staleness

A `TradeQuote` captures snapshots of the pool and order outpoints referenced by its legs at quote time. `build_trade_pset` verifies these snapshots are still current against the store's tracked outpoints; if any have changed, it returns `CoreError::StaleQuote` and the caller re-quotes. Causes:

- **Pool swap**: another trade advanced the pool's s_index and produced new reserve outpoints.
- **Pool admin adjust**: the operator changed reserves without changing s_index — the pricing curve is unchanged, but new outpoints are still produced, so the quote goes stale even though the LMSR math would be identical. This is a subtle case: a taker with a stale quote might expect it to still be valid because "nothing about the price changed," but the outpoint-level snapshot is what the freshness check uses.
- **Order fill**: another taker filled the order leg, either partially (new active outpoint) or fully (outpoint consumed).
- **Order cancel**: the maker cancelled, outpoint consumed.
- **Parent market resolution**: this doesn't directly invalidate the quote — `deadcat-core` doesn't gate post-resolution trading. The pool/order outpoints themselves only change if the pool/order has also transitioned (e.g., an admin close following resolution). Note that the non-gating is an architectural consequence (covenant-level enforcement would require co-spending the market on every pool swap, doubling every swap's weight), not a policy oversight. See [`deadcat-core-design.md § Pool and Order Lifecycle at Market Resolution`](deadcat-core-design.md#pool-and-order-lifecycle-at-market-resolution) and [`lmsr-pool-design.md § Why the pool covenant can't feasibly gate post-resolution trading`](../contracts/lmsr-pool/lmsr-pool-design.md#why-the-pool-covenant-cant-feasibly-gate-post-resolution-trading).

A stale quote is not an error in the usual sense — it's an information-integrity signal that the world changed. Wallet UX is standard "re-quote and re-confirm."

## Transaction Weight Model

The router tracks cumulative transaction weight to compute fees and enforce the weight cap:

| Component | Estimated weight |
|---|---|
| Base (wallet input + confidential change + fee output) | ~4,500 vbytes |
| Per pool (3 reserve inputs + 3 reserve outputs + witnesses) | ~1,000 vbytes |
| Per limit order fill (order input + maker receive output + witness) | ~400 vbytes |

Marginal weight is the incremental weight from adding a source to an existing route. A pool's marginal weight is ~1,000 vbytes on first activation, 0 on subsequent fills (already in the transaction). An order's marginal weight is always ~400 vbytes.

### Weight cap

A hard ceiling on transaction weight (e.g., 100,000 vbytes) prevents pathological routes. The greedy skips sources whose marginal weight would exceed the remaining budget. If the cap is reached before the request is filled, the route is a partial fill.

In practice, the fee-aware pricing limits leg count long before the weight cap — each additional leg must provide enough price improvement to justify its weight. The cap is a safety bound, not a typical constraint.

## Integer Precision and Caching

All fill amounts and costs computed by the router must use the **exact same deterministic LMSR algorithm** that the covenant verifies on-chain — the arbitrary-precision bignum algorithm specified in [lmsr-deterministic-table-spec.md](../contracts/lmsr-pool/lmsr-deterministic-table-spec.md). If the router uses floating-point approximations or independent LMSR reimplementations, the quoted amounts may differ from what the covenant enforces, causing on-chain transaction failure.

**Caching strategy at bignum speeds**: Individual F-value computations are ms-scale at arbitrary precision, so on-demand point evaluation during routing would be prohibitively slow when multiple candidate pools need evaluation. `deadcat-core` maintains an in-memory cache of full F-value tables keyed by `(max_loss_sats, half_payout_sats)` combos; the router serves all lookups from the cache. First-use cost per combo is ~5-10s (full table generation); subsequent operations — pool pre-selection, fee-aware greedy evaluation, Merkle proof generation at build time — are O(1) cache hits. The 256-combo v1 param space means the full cache stabilizes quickly under typical wallet usage.

A fixed-point Taylor runtime (deferred to v2 — see [implementation plan § deferred items](deadcat-core-implementation-plan.md#deferred--out-of-scope-items)) would restore microsecond-scale point evaluation and enable truly uncached quoting. Committed Merkle roots are the cross-implementation conformance set, so that switch is non-breaking.

Caching applies to:
- **Pool fill computation**: tokens received for a given input amount (lookup of `F(new_s) - F(old_s)` from cached table)
- **Crossover binary search**: finding the s_index where a pool's marginal price exceeds the next alternative (binary search over s_index range using cached lookups at each candidate)

## Full Algorithm

```
function quote_trade(market_id, spec, fee_rate):
    // spec = TradeSpec { outcome, side, direction, amount }
    // For binary markets, spec.outcome == OutcomeIndex::BINARY.
    // For multi-outcome markets, spec.outcome picks which outcome's liquidity to route against.

    // 1. Load candidates — scoped to the trade's target outcome
    all_pools = store.pools_for_market(market_id, spec.outcome, ActiveOnly)
    orders = store.best_orders_for_market(
        market_id, spec.outcome, spec.side, matching_direction,
        ascending, min_remaining, K=50,
    )
    // orders are filtered by (outcome, side, direction), sorted by (price, creation_position)
    // — FIFO within same price

    // 2. Pre-select top-N pools by average fill price (cached F-value lookup)
    for each pool in all_pools:
        pool.estimated_cost = lmsr_cached_lookup_exact_input(pool, requested_amount)
    candidate_pools = top_n_by_estimated_cost(all_pools, N=5)

    // 3. Enumerate pool subsets × fee-aware greedy
    best_result = None

    for each pool_subset in powerset(candidate_pools):  // 2^N iterations (max 32)
        result = fee_aware_greedy(pool_subset, orders, requested_amount, fee_rate)
        if result.total_cost < best_result.total_cost:
            best_result = result

    // 4. Return quote
    if best_result.filled_amount == 0:
        return Err(NoLiquidity)
    return Ok(TradeQuote from best_result)


function fee_aware_greedy(pools, orders, requested_amount, fee_rate):
    remaining = requested_amount
    weight = BASE_TX_WEIGHT
    legs = []
    total_cost = 0
    order_cursor = 0  // index into price-sorted orders

    loop:
        best = None  // (source, fill_amount, fill_cost, marginal_weight)

        // Evaluate each pool
        for each pool in pools:
            if pool is exhausted: continue
            marginal_w = if pool already in legs { 0 } else { POOL_WEIGHT }
            if weight + marginal_w > MAX_TX_WEIGHT: continue

            // Use cached LMSR F-value lookups: fill pool up to crossover point
            // Crossover = s_index where pool marginal price exceeds next best alternative
            // Binary search over s_index range using cached lookups at each candidate
            next_best_price = best_alternative_price(orders, order_cursor, fee_rate)
            (fill_amt, fill_cost) = lmsr_cached_fill_to_crossover(pool, remaining, next_best_price)
            if fill_amt == 0: continue

            eff_price = (fill_cost + marginal_w × fee_rate) / fill_amt
            if eff_price < best.eff_price:
                best = (pool, fill_amt, fill_cost, marginal_w)

        // Evaluate next order
        while order_cursor < orders.len():
            order = orders[order_cursor]
            if weight + ORDER_WEIGHT > MAX_TX_WEIGHT: break

            actual_fill = constrained_fill(order, remaining)  // respects min_fill/min_remainder
            if actual_fill == 0:
                order_cursor += 1
                continue

            fill_cost = actual_fill × order.price
            eff_price = (fill_cost + ORDER_WEIGHT × fee_rate) / actual_fill
            if eff_price < best.eff_price:
                best = (order, actual_fill, fill_cost, ORDER_WEIGHT)
            break  // only evaluate the top unconsumed order

        if best is None: break  // no viable sources

        // Execute best fill
        legs.append(best)
        remaining -= best.fill_amount
        total_cost += best.fill_cost
        weight += best.marginal_weight
        if best.source is order: order_cursor += 1

        if remaining == 0: break

    total_fee = weight × fee_rate
    return (legs, total_cost, total_fee, filled_amount = requested_amount - remaining)
```

## Complexity

**Time**: O(2^N × K × (P + log S))
- N = candidate pool cap (5) → 2^N = 32 subset iterations
- K = candidate order limit (50) → greedy loop iterations
- P = pools in subset (≤5) → per-iteration pool evaluations
- log S = LMSR point evaluation binary search depth (~16 for 16-bit s_index range)

For typical values with warm caches: 32 × 50 × (5 + 16) ≈ 33,600 cache lookups ≈ sub-millisecond worst case, typically much faster (most greedy iterations terminate early). Cold-start (first use of a given `(max_loss_sats, half_payout_sats)` combo) incurs a ~5-10s full-table generation; this is a one-time cost per combo amortized across all subsequent quotes.

**Space**: O(K + P) for the candidate lists.

**Store query cost**: One `pools_for_market` call + one `best_orders_for_market` indexed query. Both are single database lookups. This dominates the real-world latency, not the routing math.

## Key Files

- `src-tauri/crates/deadcat-sdk/src/amm_pool/math.rs` — LMSR math functions (will move to `deadcat-core`); binary LMSR is the only scoring rule, used per-outcome for multi-outcome markets under Option C composition
- `docs/architecture/deadcat-core-design.md` — `ContractStore` trait (outcome-scoped `pools_for_market` / `best_orders_for_market`), `quote_trade` engine API, `TradeSpec` / `TradeQuote` types
- `docs/contracts/multi-outcome/amm-scoring-rule-tradeoffs.md` — pool design decision (binary LMSR + Option C composition for multi-outcome)
- `docs/contracts/multi-outcome/multi-outcome-market-contract.md` — market contract providing the cross-outcome primitives (split/merge YES/NO) that basket trades and future v2 arb compose with
