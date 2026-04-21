# LMSR Pool Design

## Overview

LMSR (Logarithmic Market Scoring Rule) pools are automated market makers for binary prediction markets on Liquid/Elements. A pool holds YES tokens, NO tokens, and collateral (the market's collateral asset, e.g., L-BTC), and traders swap against it. The pool's pricing is governed by a mathematical cost function committed to at creation time via a Merkle root in the Simplicity covenant.

This document specifies the pool's conceptual model, parameter design, on-chain mechanics, and wallet-layer API. It is a satellite document referenced by [deadcat-core-design.md](../../architecture/deadcat-core-design.md).

## How Binary LMSR Works

### The Cost Function

The pool's state is a single integer: `s_index`. As `s_index` increases, the YES price increases (and NO decreases). The cost of moving the pool from state s1 to s2 is determined by a precomputed cost function:

```
C(s) = b × ln(exp(s/b) + exp(-s/b))
```

Where `b` is the liquidity parameter (in sats). This function is:
- **Convex and symmetric** — cheap to move near the center (50/50), expensive at the extremes (near 0% or 100%)
- **Bounded loss** — the pool's worst-case loss is `b × ln(2)` sats (~0.693 × b)

The implied YES price at state s is the logistic function: `p = 1 / (1 + exp(-2s/b))`, which naturally stays in (0, 1).

**UI visualization note**: The pricing curve is the standard logistic function — any UI framework can compute and plot it directly from `b` and `s_bias` without calling `deadcat-core`. The discrete LMSR table steps are invisible at UI rendering resolution; the continuous logistic curve is visually identical. More complex visualizations (price impact curves, depth charts, P&L scenarios) would depend on the actual LMSR math and could be added as core functions if needed.

### Pool as Inventory Manager

The pool holds three reserves:
- **YES tokens** — sold to traders who buy YES (s_index increases)
- **NO tokens** — sold to traders who buy NO (s_index decreases)
- **Collateral** (the market's collateral asset) — flows in when traders buy, flows out when traders sell

When a trader buys YES tokens, they pay collateral and receive YES tokens from the pool. The cost is `C(s2) - C(s1)` (plus fees), where s1 → s2 is the state movement. The pool's YES reserve decreases and collateral increases. The reverse for sells.

The reserves determine the pool's **capacity** — how many trades it can absorb before hitting minimum reserve limits. The cost function determines the **pricing** — how much each trade costs. These are independent: a pool can have deep pricing (high `b`) with limited capacity (low reserves), or vice versa.

### Discretization and the Merkle-Committed Curve

On-chain, the cost function is evaluated at discrete points. The F-values (`F(0), F(1), ..., F(65535)`) are the cost function evaluated at each `s_index`. These are precomputed as integers and committed to via a Merkle tree root in the covenant parameters.

Each swap transaction provides two Merkle proofs — `F(old_s_index)` and `F(new_s_index)` — and the covenant verifies:

1. Both proofs are valid against the committed Merkle root
2. The collateral amount satisfies the conservation equation (with fee adjustment)
3. The trade direction is correct (buying YES must increase s_index)
4. Reserve minimums are maintained after the trade

This approach avoids on-chain exp/ln computation entirely — the covenant only performs hash verification and integer arithmetic. The full curve is committed at creation; each trade reveals exactly two points with logarithmic-sized proofs.

**Curve well-formedness is caveat emptor at the covenant level**: the covenant verifies trades are *consistent with* the committed curve, not that the curve itself is well-formed (convex, monotonic, etc.). However, `deadcat-core` generates all curves deterministically from high-level parameters (see [Deterministic Table Generation](#deterministic-table-generation)), and `ingest_pool` with `PoolSnapshot::Creation` automatically verifies well-formedness: the engine derives `b` from the stored `max_loss_sats`, recomputes the table, and checks the Merkle root matches. Pools ingested via `PoolSnapshot::Current` skip this verification.

## Parameter Design

### Creator-Facing Parameters

A pool creator specifies exactly four values:

| Parameter | Type | Description |
|---|---|---|
| `max_loss_sats` | `u64` | Maximum possible loss for the pool (worst case). Determines market depth. |
| `fee_bps` | `u16` | Swap fee in basis points (0-9999). Pool operator's revenue per trade. |
| `half_payout_sats` | `u64` | Denomination — sats per "lot" of outcome tokens. Determines the monetary scale. |
| `starting_price_bps` | `u16` | Starting YES price in basis points (0-10000). Where the pool begins on the curve. |

Everything else is either derived or a protocol constant.

### Derived Parameters

| Parameter | Derived from | Formula / Logic |
|---|---|---|
| `b` | `max_loss_sats` | `b = max_loss_sats / ln(2)` (deterministic integer math) |
| `q_step_lots` | `b`, `half_payout_sats` | Derived to ensure the 0.1%-99.9% price range fits within the table. For most pools, `q_step_lots = 1`. See [lmsr-deterministic-table-spec.md](lmsr-deterministic-table-spec.md) for the canonical formula. |
| `s_index` (initial) | `starting_price_bps` | Nearest valid s_index for the requested price. Computed inside `estimate_bootstrap` and returned as `initial_s_index`; downstream (`derive_pool_params`, `build_lmsr_bootstrap_pset`, the OP_RETURN hint) consumes the snapped value directly — no inverse conversion lives anywhere. |
| `lmsr_table_root` | `b`, `half_payout_sats`, `q_step_lots` | Merkle root of the deterministically generated F-value table |
| Initial reserves | `b`, `starting_price_bps`, `half_payout_sats` | Balanced allocation — equal trading depth in both directions from starting price |

### Protocol Constants

| Constant | Value | Rationale |
|---|---|---|
| `TABLE_DEPTH` | 16 | 65,536 discrete price points. More than sufficient for any practical market. Fixed in the `.simf` — no metaprogramming needed. |
| `S_BIAS` | 32,768 | Always centered (s_max_index / 2). No advantage to asymmetry in a fair prediction market. |
| `S_MAX_INDEX` | 65,535 | Full table range. The LMSR cost function naturally makes extremes expensive — no need to artificially limit. |
| `MIN_POOL_RESERVE` | 1,000 sats | Applied to all three reserves (YES, NO, Collateral). Well above Liquid's dust limit (~546 sats), negligible locked capital (3,000 sats total per pool). |

**Encoding in `.simf`**: SimplicityHL lacks `const::` declarations, so protocol constants are encoded as zero-argument functions that return the literal value (e.g., `fn min_pool_reserve() -> u64 { 1000 }`), with call sites referencing `min_pool_reserve()` instead of the raw literal. This produces CMRs identical to inline literals (SimplicityHL inlines the function at compile time) while giving auditors a single named declaration per constant — easier to review than searching for `1000` throughout the program. All four constants are hard-baked covenant structure, not params: `TABLE_DEPTH` is structurally required (Merkle verification is unrolled 16 times in the program source because SimplicityHL has no loops); `S_BIAS` and `S_MAX_INDEX` derive from `TABLE_DEPTH` and mismatching them would be semantically nonsensical; `MIN_POOL_RESERVE` has no strategic decision to make at the per-pool level (it's a dust floor). The only amount-axis parameter is per-pool (`max_loss_sats`, `half_payout_sats`).

### Why Fixed Depth 16

The table depth determines the number of discrete price points (2^depth) and affects Merkle proof size:

| Depth | Price points | Proof size (2 per swap) | Table in memory | Generation time |
|---|---|---|---|---|
| 12 | 4,096 | ~784 B | 32 KB | ~5ms |
| 14 | 16,384 | ~912 B | 128 KB | ~20ms |
| **16** | **65,536** | **~1,040 B** | **512 KB** | **~80ms** |
| 18 | 262,144 | ~1,168 B | 2 MB | ~300ms |
| 20 | 1,048,576 | ~1,296 B | 8 MB | ~1s |

Depth 16 ensures `q_step_lots = 1` for pools up to ~33M sats max loss, at a negligible cost of ~26 extra sats per swap compared to depth 12. The pricing granularity near 50% is determined by `max_loss_sats`, not the table depth — when `q_step_lots = 1`, all depths produce identical pricing. The depth only matters for how large a pool can be before `q_step_lots` bumps above 1 (coarsening the minimum trade size). The 512 KB table is trivial to hold in memory.

A fixed depth means:
- Single `.simf` file with no metaprogramming or template-based code generation
- All pools share the same Merkle verification structure (same covenant program for the proof-checking logic)
- `table_depth` is not a covenant parameter — it's a protocol constant
- One fewer parameter in discovery payloads and OP_RETURN recovery hints

Variable depth would require either code generation (producing `.simf` source with unrolled Merkle verification for each depth) or SimplicityHL metaprogramming support that doesn't currently exist. The complexity isn't justified when depth 16 has massive headroom.

### Why `max_loss_sats` Instead of `b`

`b` is the mathematically correct LMSR liquidity parameter, but it's opaque — "set b to 500,000" means nothing without understanding the LMSR formula. `max_loss_sats` directly answers the pool creator's risk question: "What's the most I can lose?"

The conversion is trivial: `b = max_loss_sats / ln(2)`. The creator thinks "I'm willing to risk up to 100,000 sats" and gets a pool with the corresponding depth. `b` never appears in any public API — it's an internal implementation detail of the LMSR math.

The pricing granularity near 50% (where each discrete s_index step has the largest price impact) is determined entirely by `max_loss_sats` and `half_payout_sats`:

| `max_loss_sats` | Price step near 50% |
|---|---|
| 10,000 | ~17% |
| 50,000 | ~3.5% |
| 100,000 | ~1.7% |
| 1,000,000 | ~0.17% |
| 10,000,000 | ~0.017% |

(Assumes `half_payout_sats = 5,000`. Steps are finer near the extremes due to the logistic curve shape.)

Deeper pools require more capital but provide better trading experiences — finer pricing, smaller minimum trades, less slippage.

### Why `q_step_lots` Is Derived

With fixed depth 16 (65,536 entries), `q_step_lots` determines how many of those entries span the useful price range. The formula (defined in [lmsr-deterministic-table-spec.md](lmsr-deterministic-table-spec.md)) ensures the 0.1%-99.9% price range fits within the table. For most pools, `q_step_lots = 1`. Only very deep pools (approximately `max_loss_sats > 6,583 × half_payout_sats`) need larger values.

When `q_step_lots` bumps above 1, the minimum trade size increases proportionally (`q_step_lots × half_payout_sats` sats per index step) and the price step per trade becomes coarser. However, pools deep enough to trigger `q_step_lots > 1` already have extremely fine pricing from their depth — the coarsening is imperceptible in practice.

The pool creator has no meaningful reason to override this — it's purely a consequence of the depth and denomination choices.

### Why `min_r_*` Are Constants

The minimum reserves exist to prevent full drain of the pool. The "right" value is "above dust, small enough to be negligible" — there's no strategic decision here. Making them protocol constants (1,000 sats each) removes three parameters from the covenant, discovery payloads, and OP_RETURN recovery hints. The total locked capital per pool is 3,000 sats — trivially small for any liquidity provider.

If the pool creator wants higher effective minimums (e.g., the pool should always have at least 100,000 sats of each reserve), they achieve this through initial funding, not through min_r. The minimum reserves are a safety floor, not a business parameter.

## Deterministic Table Generation

### The Problem with Floating Point

The LMSR cost function `C(s) = b × ln(exp(s/b) + exp(-s/b))` involves transcendental functions (`exp`, `ln`). The existing SDK implementation uses `f64` for table generation. However, IEEE 754 only guarantees bit-identical results for basic arithmetic (+, -, ×, ÷) — transcendental functions like `exp()` and `ln()` can produce different results across platforms, compilers, and math libraries.

This matters because the F-values are committed to via a Merkle root. If two implementations produce different F-values from the same parameters, they produce different Merkle roots, and one of them won't match the on-chain commitment.

### The Solution: Deterministic Bignum Reference

`deadcat-core` generates F-values via arbitrary-precision bignum evaluation of the closed-form expression `F(i) = max_loss_sats + floor(b × ln(cosh(s/b)))`. Working precision is deliberately over-provisioned (nominally 200+ bits via `num-bigint` + `num-rational`) so that implementation details of the transcendental step do not affect output — any compliant implementation produces the identical `Vec<u64>` of F-values, and thus the identical Merkle root, for a given `(b, half_payout_sats, q_step_lots)`.

The full specification — derivation chain, Merkle tree format, precision budget, and reference fixtures — lives in [lmsr-deterministic-table-spec.md](lmsr-deterministic-table-spec.md). Cross-implementation conformance is anchored to a committed fixture set in `deadcat-codegen`: 256 Merkle roots (one per `(max_loss_sats, half_payout_sats)` combo) that any alternative implementation must reproduce byte-for-byte.

**v1 ships bignum-only.** A hybrid implementation (bignum at compile-time, fixed-point Taylor at runtime) is a non-breaking performance optimization for a future release — switching implementations only requires reproducing the same 256 committed Merkle roots. See the plan's [deferred items](../../architecture/deadcat-core-implementation-plan.md#deferred--out-of-scope-items).

### Implications

Deterministic generation eliminates several problems:

1. **No manifest storage needed**: The engine regenerates the table on demand from pool params. No 512 KB blob to store per pool in the `ContractStore`.
2. **No manifest in discovery payloads**: Discoverers regenerate the table from the announced params.
3. **OP_RETURN recovery works**: The hint contains enough params to regenerate the table, compute the Merkle root, and verify against the on-chain commitment.
4. **Caveat emptor resolved**: Anyone can verify a pool's curve is well-formed by regenerating the table from params and inspecting the F-values. No trust in the pool creator's off-chain claims.
5. **`interpret_transaction` works without stored manifests**: The engine regenerates the table for any pool whose params are known.

The generation cost (~5-10 seconds per `(max_loss_sats, half_payout_sats)` combo at bignum precision per [lmsr-deterministic-table-spec.md § Reference Fixtures](lmsr-deterministic-table-spec.md#reference-fixtures)) is acceptable for one-time operations amortized by caching. `deadcat-core` maintains an in-memory cache keyed by combo; subsequent lookups are O(1).

**Quoting via cached tables**: At bignum precision, individual F-value computations are ms-scale rather than microsecond-scale, so on-demand point evaluation during `quote_trade` would be prohibitively slow for multiple candidate pools. Instead, `deadcat-core` maintains an in-memory cache of full F-value tables keyed by `(max_loss_sats, half_payout_sats)` combos. The first quote for a given combo incurs the ~5-10s full-table generation cost; subsequent quotes (same pool or any pool sharing the combo) are O(1) lookups against the cache. With the 256-combo v1 param space and typical wallet usage, the cache amortizes well.

A fixed-point Taylor runtime (deferred to v2 per the [implementation plan](../../architecture/deadcat-core-implementation-plan.md#deferred--out-of-scope-items)) would restore microsecond-scale point evaluation and enable uncached quoting if needed. Since the committed Merkle roots are the cross-implementation conformance set, that switch is non-breaking.

## Pool Lifecycle

### Bootstrap

The pool creator:

1. Specifies `max_loss_sats`, `half_payout_sats`, `fee_bps`, `starting_price_bps`
2. Calls `estimate_bootstrap(max_loss_sats, half_payout_sats, starting_price_bps)` to see the required reserves (YES tokens, NO tokens, collateral) and the resulting `initial_s_index` — lightweight, called on every slider change (note: `fee_bps` does not affect reserves). The snap function (`starting_price_bps → initial_s_index`) lives here; every downstream consumer reads `initial_s_index` from this result.
3. Obtains the required YES and NO tokens by issuing pairs on the parent prediction market
4. Calls `derive_pool_params(deadcat_xprv, market_params, outcome, pool_index, max_loss_sats, half_payout_sats, fee_bps, initial_s_index)` to construct the full `LmsrPoolParams` with all derived fields (admin pubkey, table root, q_step_lots, asset IDs) and the XOR-masked pool index — heavier, called once when the user commits to creating
5. Calls `build_lmsr_bootstrap_pset(&params, initial_s_index, masked_index, &funding)` to build the transaction
6. Signs and broadcasts

`derive_pool_params` is a standalone pure function that takes the parent market's `MarketParams` umbrella (binary or multi-outcome), an `OutcomeIndex` selecting which outcome's YES/NO pair the pool serves (pass `OutcomeIndex::BINARY` for binary markets), the creator's four params plus `initial_s_index`, and derives the admin pubkey internally from the mnemonic. It derives `b`, `q_step_lots`, generates the F-value table deterministically, computes the Merkle root, and returns a fully-formed `LmsrPoolParams`. The builder then compiles the Simplicity covenant from these params and constructs the creation transaction with three reserve outputs (YES, NO, Collateral) and an OP_RETURN recovery hint.

Note: an integrator COULD construct `LmsrPoolParams` manually (it's a plain data struct with public fields), but `derive_pool_params` is strongly recommended because it guarantees the canonical deterministic table generation algorithm is used. A different implementation would produce a different Merkle root, and the covenant would reject all swaps.

`initial_s_index` represents the nearest valid discrete s_index for the requested starting price. It is computed by `estimate_bootstrap` (the UI uses the returned value for live feedback), passed through `derive_pool_params` and `build_lmsr_bootstrap_pset` unchanged, and stored directly in the pool OP_RETURN for recovery. The initial reserves are computed as a balanced allocation: equal trading depth in both directions from the starting price.

### Estimation

```rust
pub fn estimate_bootstrap(
    max_loss_sats: u64,
    half_payout_sats: u64,
    starting_price_bps: u16,
) -> BootstrapEstimate;

pub struct BootstrapEstimate {
    pub initial_yes_reserve: u64,
    pub initial_no_reserve: u64,
    pub initial_collateral_reserve: u64,
    pub initial_s_index: u64,
}
```

A standalone pure function (no engine needed). The UI calls this on every slider change for live feedback — sub-millisecond, just LMSR math. The three reserves tell the operator exactly how many tokens and how much collateral to provide. `initial_s_index` corresponds to the nearest valid LMSR curve point for the requested starting price (may differ slightly due to discretization). `starting_price_bps` must be in (0, 10000) exclusive — 0% and 100% are rejected (infinite reserve ratios).

The `initial_yes_reserve` and `initial_no_reserve` are determined by the pool's capacity in each direction from the starting price. At 50/50, they're roughly equal. At 70/30, more NO tokens are needed (more room to move toward 0%) and fewer YES tokens (less room toward 100%). The balanced allocation ensures equal trading depth in both directions.

### Param Derivation

```rust
pub fn derive_pool_params(
    deadcat_xprv: &Xpriv,
    market_params: &MarketParams,        // umbrella: binary or multi-outcome
    outcome: OutcomeIndex,                // which outcome's YES/NO pair the pool serves
    pool_index: u16,
    max_loss_sats: u64,
    half_payout_sats: u64,
    fee_bps: u16,
    initial_s_index: u16,
) -> Result<(LmsrPoolParams, u16 /* masked_index */), ConventionError>;
```

A standalone pure function that constructs the full `LmsrPoolParams` with all derived fields. Returns `ConventionError` if inputs violate OP_RETURN encoding conventions (`max_loss_sats` and `half_payout_sats` not in the 16-value 1-2-5 table, `fee_bps > 4095`, `initial_s_index` corresponding to an implied YES price outside `(0, 10000)` bps exclusive). Called once when the user commits to creating a pool — heavier than `estimate_bootstrap` because it generates the full 65K-entry F-value table and computes the Merkle root (~80ms). `initial_s_index` is sourced from `estimate_bootstrap` at creation time and directly from the pool OP_RETURN hint at recovery time — no inverse conversion from `starting_price_bps` is required. The resulting `LmsrPoolParams` is passed directly to `build_lmsr_bootstrap_pset`.

### Trading (Swaps)

Swaps are not built directly — they're part of trade transactions routed by the engine. See [trade-routing-algorithm.md](../../architecture/trade-routing-algorithm.md). The trade router evaluates pools alongside limit orders for best execution, factoring in both the pool's swap fee (`fee_bps`) and the transaction weight overhead.

The covenant's swap path enforces:
- `old_s_index != new_s_index` (state must change)
- Correct trade direction (BuyYes/SellNo must increase s_index; SellYes/BuyNo must decrease)
- Collateral conservation with fee inequality:
  - Buys: `collateral_in × (FEE_DENOM - fee_bps) >= base_cost × FEE_DENOM`
  - Sells: `collateral_out × FEE_DENOM <= base_rebate × (FEE_DENOM - fee_bps)`
- Reserve minimums maintained after the trade
- Valid Merkle proofs for F(old_s_index) and F(new_s_index)

Fee rounding always favors the pool: buyers pay ceiling, sellers receive floor.

### Admin Adjustments

The pool operator can adjust reserves without changing the s_index (and thus without changing the pricing curve). This is the admin path, authorized by the operator's admin key signature.

```rust
pub fn build_lmsr_adjust_pset(
    &self,
    contract_id: &ContractId,
    pair_delta: i64,
    collateral_delta: i64,
    funding: &WalletFunding,
) -> Result<PartiallySignedTransaction, CoreError<S::Error>>;
```

`pair_delta` is applied equally to both YES and NO reserves (the covenant enforces paired deltas). `collateral_delta` is independent. Both deltas being zero returns `CoreError::InvalidParams`.

**Use cases:**
- **Add liquidity**: Positive `pair_delta` + positive `collateral_delta`. The pool's capacity increases. The pricing curve doesn't change.
- **Remove liquidity / take profits**: Negative deltas. The operator extracts fee revenue accumulated as excess collateral.
- **Rebalance**: Adjust collateral without changing token reserves.

Admin adjustments change **capacity**, not **pricing**. The F-values (and thus the cost function) are fixed at creation — only the reserves change.

### Closure

The pool operator closes the pool via the dedicated close script path, atomically consuming all three reserve UTXOs. See [lmsr-pool-close-path.md](lmsr-pool-close-path.md).

```rust
pub fn build_lmsr_close_pset(
    &self,
    contract_id: &ContractId,
    funding: &WalletFunding,
) -> Result<PartiallySignedTransaction, CoreError<S::Error>>;
```

All reserve funds are returned to `funding.return_script`. The pool transitions to `LmsrPoolState::Closed`.

### Market Resolution

The pool covenant is **market-state-agnostic** — it doesn't know or care whether the parent prediction market has resolved. Swaps remain technically valid after resolution. However, no rational trader would swap after resolution (the outcome is known, so the token prices are known), so the pool naturally goes idle. The operator closes the pool when convenient, then redeems any winning tokens via the parent market's redemption path.

### Why the pool covenant can't feasibly gate post-resolution trading

A gated design would require the pool covenant to verify the parent market's state on every swap. Because a covenant can only introspect the current transaction, the only way for the pool to observe market state is to **co-spend the market covenant's UTXO as an input on every swap transaction** — the pool's spend path would require the market's Unresolved-phase collateral UTXO to be present in the same tx, and would reject the swap if the market wasn't in Trading state.

This is architecturally possible but prohibitively expensive:
- **Every swap tx grows by the full market co-spend** — adding the market's collateral input plus its witness (Simplicity program + control block) to the pool's own ~1,000-vbyte swap footprint. Realistically 1.5-2× the current swap size.
- **Every swap pays this overhead** — whether or not the market is near resolution. A trader swapping on a market with years left until expiry pays the same per-trade co-spend cost as one trading right before resolution.
- **Serialization constraint** — cross-outcome arb and other multi-pool patterns would compound: an N-pool-swap arb already co-spends the market in some directions; forcing co-spend on every pool swap regardless of direction makes these even heavier.
- **No covenant-cheap alternative** — there's no way for a pool covenant to check market state without seeing the market UTXO. Merkle inclusion proofs against a market-state commitment would require the market contract to emit such commitments, which they don't (and wouldn't in v1).

The cost falls on the 99%+ of swaps that happen during the market's active life — paying a permanent tax so that the <1% edge case (the informed-drainer attack right after resolution) is blocked. That trade is rejected: **operator-layer protection** (closing the pool via `build_lmsr_close_pset` after resolution) is the appropriate tool. Operators who keep pools open post-resolution are accepting the drain risk; those who don't, don't pay.

**`deadcat-core` mirrors this at the engine layer**: trading remains routable through `quote_trade` / `build_trade_pset` regardless of parent market state. The engine does not gate post-resolution trading — the covenant is market-state-agnostic by the above architectural choice, and engine-layer gating would provide only false safety (sophisticated actors fork `deadcat-core` or bypass it). See [`deadcat-core-design.md § Pool and Order Lifecycle at Market Resolution`](../../architecture/deadcat-core-design.md#pool-and-order-lifecycle-at-market-resolution) and [Design Principles § Engine gates covenant-invalidity and impossibility, not unfavorability](../../architecture/deadcat-core-design.md#engine-gates-covenant-invalidity-and-impossibility-not-unfavorability).

## Pool Operator Economics

### Revenue

The pool earns fee revenue on every swap. The fee (`fee_bps`) is the spread between the true LMSR cost and what the trader pays. Fee revenue accumulates as excess collateral in the pool's reserves. The operator extracts it via admin adjustments.

### Risk

The pool's maximum loss is `max_loss_sats` (= `b × ln(2)`). This worst case occurs when the market moves maximally in one direction from the pool's starting price. In practice, if the market moves and then returns, the pool profits from the round-trip fees.

The pool's net P&L = cumulative fee revenue - trading losses from directional movement. A pool in an active, balanced market (prices moving around rather than trending in one direction) typically profits from fees exceeding losses.

### Capital Efficiency

The total capital needed (sum of `initial_yes_reserve` + `initial_no_reserve` + `initial_collateral_reserve` from `BootstrapEstimate`, converted to collateral terms via the market's `collateral_per_pair`) is larger than `max_loss_sats` because the pool must hold token inventory, not just collateral to cover losses.

## On-Chain Covenant Parameters

With the simplifications above, `LmsrPoolParams` contains:

| Field | Size | Source |
|---|---|---|
| `yes_asset_id` | 32 bytes | From parent market |
| `no_asset_id` | 32 bytes | From parent market |
| `collateral_asset_id` | 32 bytes | From parent market |
| `lmsr_table_root` | 32 bytes | Derived (Merkle root of F-values) |
| `q_step_lots` | u64 | Derived from `b` and `half_payout_sats` |
| `half_payout_sats` | u64 | Creator-specified |
| `fee_bps` | u64 | Creator-specified (u64 for Simplicity arithmetic jets; validated < 10,000) |
| `admin_pubkey` | 32 bytes | From mnemonic |
| `max_loss_sats` | u64 | Creator-specified — NOT a covenant param (see below) |

The first 8 fields are covenant parameters (compiled into the Simplicity program). `max_loss_sats` is not a covenant parameter — the covenant only verifies Merkle proofs, never evaluates the cost function. It is included in the struct because all off-chain LMSR computation (point evaluation for quoting, table generation for Merkle proofs, spot price calculation) requires the liquidity parameter `b = max_loss_sats / ln(2)`, and `b` is not recoverable from the covenant params alone (the `ceil()` in the `max_loss_sats → q_step_lots` derivation is lossy). `q_step_lots` and `lmsr_table_root` are retained alongside `max_loss_sats` as compilation caches — recomputing `lmsr_table_root` requires ~80ms of table generation.

**Removed from params** (now constants in the `.simf`): `table_depth`, `s_bias`, `s_max_index`, `min_r_yes`, `min_r_no`, `min_r_collateral`.

**Not in params** (derived on demand from `max_loss_sats`): `b`. Unlike `q_step_lots` and `lmsr_table_root` (covenant params and compilation caches), `b` is only used transiently during LMSR math — deriving it from `max_loss_sats` is trivial.

## OP_RETURN Recovery Hint

The pool creation transaction includes a **40-byte** zero-value OP_RETURN output for mnemonic-based recovery. The hint uses compressed encoding: `max_loss_sats` and `half_payout_sats` as 4-bit 1-2-5 table indices each (shared with the market `base_payout` encoding, range 100 to 10,000,000 sats), `fee_bps` as u12 (0.01% granularity), `initial_s_index` as u16 (the starting table index, enabling direct script verification during creation-tx recovery), plus an XOR-masked pool operator derivation index.

All other covenant params are derived: `b` from `max_loss_sats`, `q_step_lots` from `b` and `half_payout_sats`, `lmsr_table_root` from deterministic F-value generation, token asset IDs from the parent market, admin pubkey from the mnemonic at `pool_index`. Protocol constants require no encoding.

See [chain-only-recovery.md](../../protocol/chain-only-recovery.md) for the exact byte layout, per-field justification, recovery flow, denomination convention specification, and XOR index masking details.

## Key Files

- `docs/architecture/deadcat-core-design.md` — main design doc (references this satellite doc)
- `docs/architecture/trade-routing-algorithm.md` — trade routing algorithm using LMSR pools + limit orders
- `docs/contracts/lmsr-pool/lmsr-pool-close-path.md` — close script path covenant design
- `src-tauri/crates/deadcat-sdk/src/lmsr_pool/math.rs` — current LMSR math (will move to `deadcat-core`)
- `src-tauri/crates/deadcat-sdk/contract/lmsr_pool.simf` — pool covenant source
