# Transaction Composability Model

## Overview

Trade transactions on Deadcat can co-spend multiple covenant inputs — LMSR pool reserves and maker order UTXOs — in a single transaction. Each covenant input independently runs its Simplicity program, introspecting the transaction's outputs to verify its constraints. This document specifies how the three covenant types introspect outputs, how the PSET builder arranges inputs and outputs to satisfy all covenants simultaneously, and what prevents output aliasing (two covenants both claiming the same output).

## Output Aliasing: The Core Risk

When two covenant inputs in the same transaction both inspect the same output and both believe it satisfies their requirements, the output is "aliased." Since the output only exists once, one covenant's state is effectively lost or one party is underpaid. The defense against aliasing operates at two levels:

1. **Script uniqueness** — Different contracts produce different script pubkeys. A covenant verifying its output against an expected script will reject outputs belonging to a different contract. This is the fundamental defense.
2. **Structural separation** — Covenants that reference outputs by position (tied to their input index) cannot inspect the same output as another covenant at a different input index. This is defense-in-depth that holds regardless of script uniqueness.

## Per-Contract Introspection Models

### LMSR Pool: Witness-Parameterized (Current — No Change Needed)

The pool covenant accepts `in_base` and `out_base` as witness data. It asserts the current input is at `in_base` (`current_index() == in_base`) and validates the three reserve inputs at `[in_base, in_base+1, in_base+2]` and outputs at `[out_base, out_base+1, out_base+2]`. It also validates `ensure_three_slot_window_in_range(out_base, num_outputs())`. All 3 output scripts are verified against `script_hash_for_state(new_s_index)`.

This is maximally flexible — the builder chooses where to place pool outputs. Two different pools always have different scripts (different params → different CMR → different `script_hash_for_state()` at any s_index), so aliasing between pools is impossible regardless of output placement.

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

### Prediction Market: Hardcoded Absolute Indices (No Change Needed)

The market covenant asserts `current_index() == 0` and checks outputs at fixed positions (0, 1, 2, 3, 4, 5...). This is acceptable because all market lifecycle operations (issuance, cancellation, resolution, redemption, expiry) are single-contract — there is no practical reason to co-spend two markets' covenant UTXOs in the same transaction. The rigid layout fully specifies the transaction structure for each operation.

**Future consideration:** The atomic issuance + pool bootstrap enhancement ([future-atomic-issuance-lmsr.md](future-atomic-issuance-lmsr.md)) would co-spend market RT UTXOs while creating pool reserve outputs. The market covenant's hardcoded indices may already accommodate this (the covenant doesn't constrain token output destinations, and extra outputs between the token outputs and the fee output may be tolerated). This requires verification against the exact `.simf` logic when that feature is scoped.

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

The `build_trade_pset` builder arranges inputs and outputs using the natural ordering:

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

### Layout Invariants

The builder must ensure:
1. Pool output windows (`[out_base, out_base+2]`) do not overlap with each other
2. Pool output windows do not overlap with order input indices (since order maker receives are positional at `current_index()`)
3. Order remainder witness indices do not collide with each other or with any positional output

The natural ordering (pools first, then orders, then wallet) satisfies all three invariants by construction. An incorrect layout produces a transaction that fails (covenant script mismatch on a contested output index) — not an aliasing exploit.

## Recovery and Duplicate Contracts

During mnemonic recovery, if the user creates new contracts before completing chain scanning, they may accidentally create a contract with the same params as an existing on-chain contract (by reusing an `order_index` or `pool_index`). This produces two UTXOs with the same covenant script.

**This is NOT a correctness issue for the engine** — it tracks contracts by outpoint, not script. Filling one order doesn't affect the other. But it IS an economic issue — the user has duplicate contracts they may not know about.

**Best practice:** Complete recovery scanning (wallet-funded transaction scan for OP_RETURN hints) before creating new contracts. The recovery flow naturally discovers existing contracts, which are re-ingested rather than recreated. The risk only materializes if the user creates new contracts mid-recovery before scanning is complete.

**Natural mitigation:** `derive_order_params` and `derive_pool_params` use sequential indices. Even if recovery misses the most recent contract, the user would typically increment the index, producing different params → different script → no collision. Collision requires exact index reuse, which the sequential pattern naturally avoids.

## Key Files

- `src-tauri/crates/deadcat-sdk/contract/maker_order.simf` — order covenant: change remainder from `current_index() + 1` to `witness::REMAINDER_IDX`
- `src-tauri/crates/deadcat-sdk/contract/lmsr_pool.simf` — pool covenant: already uses witness-based `in_base`/`out_base` (no changes needed)
- `src-tauri/crates/deadcat-sdk/contract/prediction_market.simf` — market covenant: hardcoded indices (no changes needed for v1)
- `docs/contract-specification.md` — pending refactors table: add order remainder witness-parameterization
- `docs/deadcat-core-design.md` — `build_trade_pset` output layout algorithm
