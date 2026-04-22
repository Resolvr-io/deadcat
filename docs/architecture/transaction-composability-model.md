# Transaction Composability Model

## Overview

Trade transactions on Deadcat can co-spend multiple covenant inputs — LMSR pool reserves, maker order UTXOs, and, for an assisted pool leg, the parent market window — in a single transaction. Each covenant input independently runs its Simplicity program, introspecting the transaction's outputs to verify its constraints. This document specifies how the three covenant types introspect outputs, how the PSET builder arranges inputs and outputs to satisfy all covenants simultaneously, and what prevents output aliasing (two covenants both claiming the same output).

## Output Aliasing: The Core Risk

When two covenant inputs in the same transaction both inspect the same output and both believe it satisfies their requirements, the output is "aliased." Since the output only exists once, one covenant's state is effectively lost or one party is underpaid. The defense against aliasing operates at two levels:

1. **Script uniqueness** — Different contracts produce different script pubkeys. A covenant verifying its output against an expected script will reject outputs belonging to a different contract. This is the fundamental defense.
2. **Structural separation** — Covenants that reference outputs by position (tied to their input index) cannot inspect the same output as another covenant at a different input index. This is defense-in-depth that holds regardless of script uniqueness.

## Per-Contract Introspection Models

### LMSR Pool: Witness-Parameterized (Current — No Change Needed)

The pool covenant accepts `in_base` and `out_base` as witness data. It asserts the current input is at `in_base` (`current_index() == in_base`) and validates the three reserve inputs at `[in_base, in_base+1, in_base+2]` and outputs at `[out_base, out_base+1, out_base+2]`. It also validates `ensure_three_slot_window_in_range(out_base, num_outputs())`. All 3 output scripts are verified against `script_hash_for_state(new_s_index)`.

This is maximally flexible — the builder chooses where to place pool outputs. Two different pools always have different scripts (different params → different CMR → different `script_hash_for_state()` at any s_index), so aliasing between pools is impossible regardless of output placement.

The same public pool path can be used with `old_s_index != new_s_index` (ordinary swap or swap+paired-delta assist) or `old_s_index == new_s_index` (degenerate paired rebalance). Admin adjust remains a separate spend path. Witness flexibility here selects only the pool's inspection window; it does not weaken script verification or reserve checks.

### Maker Order: Hybrid Positional + Witness (Proposed Change)

**Current model:** The order covenant uses `current_index()` for both the maker receive output (at index `i`) and the remainder output (at index `i+1`). This creates rigid 2-slot windows that overlap when two partial-fill orders are at adjacent input indices.

**Proposed model:** The maker receive stays positional at `current_index()`. The remainder moves to a witness-parameterized index:

```simplicity
let i: u32 = jet::current_index();
let rem_idx: u32 = witness::REMAINDER_IDX;

// Maker receive — positional (structurally prevents payment aliasing)
let recv_spk: u256 = get_output_script_hash(i);
assert!(jet::eq_256(recv_spk, param::MAKER_RECEIVE_SPK_HASH));

// Remainder — witness-parameterized (covenant script uniqueness prevents aliasing)
let rem_spk: u256 = get_output_script_hash(rem_idx);
let input_spk: u256 = get_input_script_hash(i);
assert!(jet::eq_256(rem_spk, input_spk));
```

Each output uses the protection model best suited to its risk profile:

- **Maker receive (positional):** Payment aliasing requires only one shared field (`MAKER_RECEIVE_SPK_HASH` — a wallet address that a naive user might reuse across orders). Positional placement makes this structurally impossible — two orders at different input indices always check different output indices.
- **Remainder (witness-parameterized):** Covenant continuation aliasing requires ALL params to be identical (CMR collision — the entire covenant script must match). This is the strongest possible precondition, making witness flexibility safe.

**What this changes in the `.simf`:**
- `maker_order.simf`: the fill validation functions accept `remainder_idx` from witness data instead of computing `safe_add_32(i, 1)`. Remainder output script and value checks use the witness-provided index.
- `witness::REMAINDER_IDX` is added as a new witness declaration (replaces the implicit `i+1`).

### Prediction Market: Witness-Parameterized (Cluster 1 Decision)

Both market contracts now follow the same composability model as the pool covenant: the witness provides `in_base` and `out_base`, the covenant asserts the current input is at `in_base + slot_offset`, and it validates a bounded contiguous input/output window rooted at those bases. This preserves covenant correctness while allowing the market to sit anywhere in a larger transaction.

For binary markets, this is a covenant-level capability decision, not a promise that every v1 builder uses arbitrary placement. The standard creation/issuance/cancellation/resolution/expiry/redemption builders may still choose a canonical layout for simplicity, but the committed contract semantics no longer hardcode absolute transaction positions. That preserves the option to add future multi-contract builders without changing already-created markets.

The anti-aliasing story matches the general model in [market-contract-principles.md](../contracts/market-contract-principles.md): witness flexibility only selects the contract's inspection window; it does not relax script verification, asset verification, or output-window bounds. A malicious builder can move the window, but cannot make the covenant accept another contract's outputs as its own.

## Script Uniqueness Guarantee

Every live maker order UTXO has a unique covenant script, and every live LMSR pool reserve UTXO has a unique covenant script (per reserve role). This is structurally guaranteed, not a convention:

- **Maker orders**: `maker_pubkey` and `order_nonce` are both deterministic functions of `order_index`, which the wallet increments per order. Two orders from the same wallet have different indices → different keys and nonces → different CMRs. Two orders from different wallets have different seeds → different keys → different CMRs. See [chain-only-recovery.md § Key Derivation](../protocol/chain-only-recovery.md#key-derivation) and [§ Order Nonce Derivation](../protocol/chain-only-recovery.md#order-nonce-derivation).
- **LMSR pools**: `admin_pubkey` is derived per `pool_index`, and the pool's Merkle root varies with `max_loss_sats` / `half_payout_sats`. Two pools with matching params still differ by admin pubkey → different CMR.
- **Prediction markets**: params include 2N issuance-derived asset IDs unique per creation tx, so two markets never share a CMR.

This guarantee is what makes the aliasing analysis below tractable: "same covenant script" attack preconditions are not reachable via `derive_order_params` / `derive_pool_params`, they would require manually-constructed params that bypass deterministic derivation.

## Aliasing Analysis

### Attack Vectors

| Attack | Precondition | Option A (fully witness) | Option B (hybrid — proposed) |
|---|---|---|---|
| **Maker receive aliasing** — two orders share a maker receive output, taker underpays | Same `MAKER_RECEIVE_SPK_HASH` (one field) | Vulnerable — taker pays `max(A, B)` instead of `A + B` | **Immune** — positional separation |
| **Remainder aliasing** — two orders share a remainder output, one order's locked value is lost | Same covenant script (ALL params identical — CMR collision) | Vulnerable | Vulnerable (same) |
| **Cross-type aliasing** — one order's maker receive overlaps another's remainder | N/A | Immune — wallet script (P2TR) ≠ covenant script (Simplicity) | Immune (same) |
| **Self-aliasing** — one order's maker receive and remainder point to same output | N/A | Immune — `MAKER_RECEIVE_SPK_HASH` ≠ covenant script | Immune (same) |
| **Pool-order aliasing** — order output overlaps pool reserve output | N/A | Immune — different contract types → different scripts | Immune (same) |
| **Pool-pool aliasing** — two pools share reserve outputs | N/A | Immune — different params → different `script_hash_for_state()` | Immune (same) |

### Why Maker Receive Aliasing Matters More Than Remainder Aliasing

Both aliasing types are prevented by script uniqueness when using `derive_order_params` (unique nonces per index). The difference is the precondition strength when params are manually constructed:

- **Remainder aliasing** requires ALL params identical. A user would need to manually construct two orders with the same market, price, direction, min_fill, min_remainder, maker_pubkey, AND maker_receive_spk_hash. This is essentially creating the same order twice — an obvious mistake.
- **Maker receive aliasing** requires only the same `MAKER_RECEIVE_SPK_HASH`. Two orders can differ in every other field (different markets, prices, directions) and still share a receive address. A naive user reusing a wallet address across orders triggers this naturally.

The proposed hybrid model (Option B) eliminates the more likely attack vector structurally, while accepting the less likely one's reliance on script uniqueness.

## Output Layout for Multi-Covenant Transactions

### Builder Layout Algorithm

The v1 builder uses two deterministic layouts:

- **Plain routes** (no assisted pool leg): pools first, then orders, then wallet.
- **Routes with one assisted pool leg**: the co-spent market window first, then that pool window, then any remaining pools, then orders, then wallet.

Placing the market window immediately before its assisted pool window localizes the only market co-spend in v1 and keeps witness-base assignment mechanical.

#### Plain Routes

**Inputs:**
```
[0..3P-1]      Pool reserve inputs (3 per pool, grouped by pool)
[3P..3P+K-1]   Order inputs (1 per order)
[3P+K..]       Wallet inputs (fee, collateral)
```

**Outputs:**
```
[0..3P-1]      Pool reserve outputs (3 per pool, at out_base = in_base for each pool)
[3P..3P+K-1]   Order maker receive outputs (1 per order, at current_index)
[3P+K..]       Order remainders (witness-specified), taker receive, fee, change
```

Each pool's witness sets `in_base` and `out_base` to the pool's starting input index. Each order's maker receive naturally lands at the output index matching its input index. Remainders float to witness-specified indices after all positional outputs.

#### Routes With One Assisted Pool Leg

Let `Wm` be the co-spent market window size:

- binary market, Unresolved phase: `Wm = 3`
- multi-outcome market, Unresolved phase: `Wm = 2N + 1`

**Inputs:**
```
[0..Wm-1]          Assisted market inputs
[Wm..Wm+2]         Assisted pool reserve inputs
[Wm+3..Wm+3+3P-1]  Remaining pool reserve inputs
[..]               Order inputs
[..]               Wallet inputs
```

**Outputs:**
```
[0..Wm-1]          Assisted market continuation outputs
[Wm..Wm+2]         Assisted pool reserve outputs
[Wm+3..Wm+3+3P-1]  Remaining pool reserve outputs
[..]               Order maker receive outputs
[..]               Order remainders, taker receive, fee, change, burn outputs
```

The market witness sets its own `in_base` / `out_base` to `0`. The assisted pool witness sets `in_base = Wm` and `out_base = Wm`. Any remaining pool windows follow after that in 3-slot groups.

### Layout Example: 2 Pools + 2 Partial-Fill Orders

```
Inputs:                              Outputs:
[0] Pool A YES reserve         →     [0] Pool A YES reserve (new s_index)
[1] Pool A NO reserve          →     [1] Pool A NO reserve (new s_index)
[2] Pool A Collateral          →     [2] Pool A Collateral (new s_index)
[3] Pool B YES reserve         →     [3] Pool B YES reserve (new s_index)
[4] Pool B NO reserve          →     [4] Pool B NO reserve (new s_index)
[5] Pool B Collateral          →     [5] Pool B Collateral (new s_index)
[6] Order A                    →     [6] Order A maker receive (positional)
[7] Order B                    →     [7] Order B maker receive (positional)
[8] Wallet (fee + collateral)        [8] Order A remainder (witness: rem_idx=8)
                                     [9] Order B remainder (witness: rem_idx=9)
                                     [10] Taker receive
                                     [11] Fee
                                     [12] Change

Pool A witness: in_base=0, out_base=0
Pool B witness: in_base=3, out_base=3
Order A witness: remainder_idx=8
Order B witness: remainder_idx=9
```

No overlapping windows. No aliasing. Each covenant's introspection is independently satisfied.

### Layout Example: Binary Buy With `IssuePairs`

An existing binary pool is short on depth for a YES buy, so the route co-spends the parent market's issuance path and mints `pairs` YES+NO directly into the pool while also moving the pool along the curve:

```
Inputs:                              Outputs:
[0] Market YES RT               →    [0] Market YES RT continuation
[1] Market NO RT                →    [1] Market NO RT continuation
[2] Market collateral           →    [2] Market collateral continuation (+ pairs × cp)
[3] Pool YES reserve            →    [3] Pool YES reserve (new s_index)
[4] Pool NO reserve             →    [4] Pool NO reserve (new s_index)
[5] Pool collateral reserve     →    [5] Pool collateral reserve (new s_index)
[6..] Wallet collateral inputs       [6] Taker YES receive
     (issuance collateral +          [7] Fee
      trade payment + fee)           [8..] Change

Market witness: in_base=0, out_base=0
Pool witness:   in_base=3, out_base=3
```

No temporary YES/NO outputs are needed. The newly issued tokens are paid directly into the pool reserve outputs at `[3]` and `[4]`; those outputs already represent the post-trade reserve state the pool covenant is checking.

### Layout Example: Binary Sell With `CancelPairs`

An existing binary pool buys YES from the taker, but the route also cancels `pairs` out of the pool reserves to release market collateral:

```
Inputs:                              Outputs:
[0] Market YES RT               →    [0] Market YES RT continuation
[1] Market NO RT                →    [1] Market NO RT continuation
[2] Market collateral           →    [2] Market collateral continuation (- pairs × cp)
[3] Pool YES reserve            →    [3] Pool YES reserve (new s_index)
[4] Pool NO reserve             →    [4] Pool NO reserve (new s_index)
[5] Pool collateral reserve     →    [5] Pool collateral reserve (new s_index)
[6] Wallet YES sold by taker         [6] YES burn output for cancelled pairs
[7..] Wallet fee inputs              [7] NO burn output for cancelled pairs
                                     [8] Taker collateral receive
                                     [9] Fee
                                     [10..] Change

Market witness: in_base=0, out_base=0
Pool witness:   in_base=3, out_base=3
```

The cancelled YES and NO tokens leave the pool via the burn outputs at `[6]` and `[7]`. The taker's collateral receive at `[8]` is the combined result of the pool-side rebate and the market-side cancellation release; `TradeQuote` presents this as one assisted pool leg even though two covenants contribute to the final amount.

### Layout Invariants

The builder must ensure:
1. Market and pool output windows do not overlap with each other
2. Pool output windows (`[out_base, out_base+2]`) do not overlap with each other
3. Contract output windows do not overlap with order input indices (since order maker receives are positional at `current_index()`)
4. Order remainder witness indices do not collide with each other or with any positional output

The chosen layouts satisfy all four invariants by construction. An incorrect layout produces a transaction that fails (covenant script mismatch on a contested output index) — not an aliasing exploit.

## Recovery and Duplicate Contracts

During mnemonic recovery, if the user creates new contracts before completing chain scanning, they may accidentally create a contract with the same params as an existing on-chain contract (by reusing an `order_index` or `pool_index`). This produces two UTXOs with the same covenant script.

**This is NOT a correctness issue for the engine** — it tracks contracts by outpoint, not script. Filling one order doesn't affect the other. But it IS an economic issue — the user has duplicate contracts they may not know about.

**Best practice:** Complete recovery scanning (wallet-funded transaction scan for OP_RETURN hints) before creating new contracts. The recovery flow naturally discovers existing contracts, which are re-ingested rather than recreated. The risk only materializes if the user creates new contracts mid-recovery before scanning is complete.

**Natural mitigation:** `derive_order_params` and `derive_pool_params` use sequential indices. Even if recovery misses the most recent contract, the user would typically increment the index, producing different params → different script → no collision. Collision requires exact index reuse, which the sequential pattern naturally avoids.

## Key Files

- `src-tauri/crates/deadcat-sdk/contract/maker_order.simf` — order covenant: change remainder from `current_index() + 1` to `witness::REMAINDER_IDX`
- `src-tauri/crates/deadcat-sdk/contract/lmsr_pool.simf` — pool covenant: already uses witness-based `in_base`/`out_base` (no changes needed)
- `src-tauri/crates/deadcat-sdk/contract/prediction_market.simf` — market covenant: witness-parameterized `in_base`/`out_base` for flexible transaction composition
- `docs/contracts/contract-specification.md` — pending refactors table: add order remainder witness-parameterization
- `docs/architecture/deadcat-core-design.md` — `build_trade_pset` output layout algorithm
