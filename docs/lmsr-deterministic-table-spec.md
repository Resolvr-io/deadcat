# LMSR Deterministic Table Specification

**Status**: Skeleton — defines what needs to be specified. The exact algorithm, constants, and test vectors are to be filled in during implementation.

## Overview

The LMSR pool covenant commits to a Merkle root of precomputed cost function values (`F-values`). At swap time, the covenant verifies Merkle proofs for two F-values and uses them in integer arithmetic to enforce pricing. The covenant never evaluates the cost function itself — it only verifies hashes and performs integer comparisons.

This document specifies the off-chain algorithm that produces the F-values. Every implementation must produce bit-identical values from the same parameters, because the Merkle root is a covenant parameter — a different root means a different contract (different CMR, different addresses, incompatible swaps).

## Relationship to the Simplicity Contract

### What the covenant stores

The `LMSR_TABLE_ROOT` is a compile-time parameter baked into the Simplicity program. It's part of the contract's identity — changing it changes the CMR and all derived script pubkeys.

### What the covenant verifies at swap time

The swap witness provides six values: `old_s_index`, `new_s_index`, `f_old`, `f_new`, `old_proof`, `new_proof`. The covenant:

1. **Verifies two Merkle proofs** — confirms that `(old_s_index, f_old)` and `(new_s_index, f_new)` are committed leaves under `LMSR_TABLE_ROOT`:
   ```
   leaf = SHA256(0x00 || "LMSR_TBL_V1" || be64(index) || be64(value))
   node = SHA256(0x01 || left || right)
   ```
   Each proof is a list of `(sibling_hash, is_right)` pairs, folded from leaf to root. The covenant asserts the computed root equals `LMSR_TABLE_ROOT` and the proof depth equals `TABLE_DEPTH`.

2. **Uses f_old and f_new in the conservation equation** — the swap path computes `base_notional` from `f_old`, `f_new`, `traded_lots`, and `half_payout_sats`, then enforces fee-adjusted pricing via 128-bit integer inequality:
   ```
   buy:  delta_in  × fee_c    ≥ base_notional × FEE_DENOM
   sell: delta_out × FEE_DENOM ≤ base_notional × fee_c
   ```
   where `fee_c = FEE_DENOM - FEE_BPS` and all arithmetic is integer-only.

3. **Verifies trade direction** — `new_s_index > old_s_index` for buys, `<` for sells.

4. **Verifies reserve conservation** — the token and collateral deltas match the expected amounts for the trade direction.

### What the covenant does NOT do

- Never evaluates `exp`, `ln`, or any transcendental function
- Never generates F-values or Merkle trees
- Never performs floating-point arithmetic
- Has no concept of `b`, `max_loss_sats`, or the continuous cost function

The covenant is a pure integer verifier. The entire cost curve is encoded in the Merkle root at creation time.

### The off-chain / on-chain boundary

```
Off-chain (deadcat-core):                    On-chain (covenant):

max_loss_sats → b → q_step_lots
    ↓
F(0), F(1), ..., F(65535)
    ↓
Merkle root ──── baked into params ────────→ LMSR_TABLE_ROOT (constant)

At swap time:
compute optimal new_s_index
look up F(old_s), F(new_s)
generate proofs ─── witness data ──────────→ verify proofs against root
                                              use F-values in conservation eq
```

## Merkle Tree Format

**Already fully specified** — extractable from `lmsr_pool.simf` and matching Rust implementation in `table.rs`.

### Leaf hash
```
SHA256(0x00 || "LMSR_TBL_V1" || be64(index) || be64(value))
```
- `0x00` prefix distinguishes leaves from internal nodes
- `"LMSR_TBL_V1"` is 11 bytes of ASCII domain tag
- `index` and `value` are big-endian u64 (8 bytes each)
- Total preimage: 1 + 11 + 8 + 8 = 28 bytes

### Internal node hash
```
SHA256(0x01 || left_hash || right_hash)
```
- `0x01` prefix distinguishes internal nodes from leaves
- `left_hash` and `right_hash` are 32 bytes each
- Total preimage: 1 + 32 + 32 = 65 bytes

### Tree structure
- Perfect binary tree with `2^TABLE_DEPTH` leaves
- `TABLE_DEPTH = 16` → 65,536 leaves
- Leaf at position `i` contains `(i, F(i))`
- Leaves are ordered left-to-right by index (leaf 0 is leftmost)

### Proof format
A proof for leaf `(index, value)` is a list of `TABLE_DEPTH` elements, each `(sibling_hash: [u8; 32], is_right: bool)`. `is_right = true` means the current node is the right child (sibling is left). Verification folds from leaf to root:

```
current = leaf_hash(index, value)
for (sibling, is_right) in proof:
    if is_right:
        current = node_hash(sibling, current)
    else:
        current = node_hash(current, sibling)
assert current == LMSR_TABLE_ROOT
```

## F-Value Computation Algorithm

**NOT YET SPECIFIED** — this is the section that requires implementation work.

### The mathematical definition

```
F(i) = floor(b × ln(exp(q_yes(i)/b) + exp(q_no(i)/b)))

where:
    b = max_loss_sats / ln(2)
    q_yes(i) = (i - S_BIAS) × q_step_lots × half_payout_sats
    q_no(i) = -q_yes(i)
    S_BIAS = 32,768  (protocol constant)
```

Since `q_no = -q_yes`, this simplifies to:
```
F(i) = floor(b × ln(exp(s/b) + exp(-s/b)))
     = floor(b × ln(2 × cosh(s/b)))
     = floor(b × (ln(2) + ln(cosh(s/b))))
     = floor(max_loss_sats + b × ln(cosh(s/b)))

where s = q_yes(i)
```

At `i = S_BIAS`: `s = 0`, `cosh(0) = 1`, `ln(1) = 0`, so `F(S_BIAS) = floor(max_loss_sats)` = the minimum F-value (the pool's maximum loss).

### What the algorithm must define

Each of the following must be specified with exact precision, rounding mode, and intermediate representation:

1. **Computation of `b` from `max_loss_sats`**
   - Mathematical: `b = max_loss_sats / ln(2)`
   - Requires: exact rational approximation of `1/ln(2)` (or `ln(2)` for division)
   - Output: `b` as a fixed-point value with defined precision
   - Rounding: specified (e.g., round-to-nearest, truncate)

2. **Computation of `q_step_lots` from `b` and `half_payout_sats`**
   - Mathematical: `q_step_lots = max(1, ceil(ln(999) × b / (65536 × half_payout_sats)))`
   - Equivalent: `q_step_lots = max(1, ceil(ln(999) × max_loss_sats / (65536 × ln(2) × half_payout_sats)))`
   - Output: u64
   - Rounding: ceiling, floored at 1
   - `ln(999) ≈ 6.9078` is a **protocol-fixed derivation constant** encoding the 0.1%-99.9% price range target. The table's edge indices (`i = 0` and `i = S_MAX_INDEX`) correspond to implied YES prices of approximately 0.1% and 99.9% when `q_step_lots` equals this formula's result. The exact rational approximation of `ln(999)` is part of the deterministic integer algorithm specification.
   - For most practical pool parameters, `q_step_lots = 1`. The formula only produces values > 1 for very deep pools (approximately `max_loss_sats > 6,583 × half_payout_sats`).

3. **Evaluation of `F(i)` for each `i` in `[0, 2^TABLE_DEPTH)`**
   - Mathematical: `F(i) = floor(b × ln(exp(s/b) + exp(-s/b)))` where `s = (i - S_BIAS) × q_step_lots × half_payout_sats`
   - Requires: deterministic evaluation of `exp` and `ln` (or `cosh` and `ln`) at sufficient precision
   - All intermediates must use defined-precision integer/fixed-point arithmetic
   - No `f64` — IEEE 754 does not guarantee deterministic `exp`/`ln` across platforms
   - Output: u64 (the `floor()` of the high-precision result)

4. **Point evaluation (same algorithm, single index)**
   - Used by `quote_trade` for quoting (~1μs per evaluation)
   - Must produce bit-identical results to the table generation path
   - Performance target: ~16 evaluations in ~16μs (binary search over s_index range)

### Algorithm design considerations

**Approach options** (to be evaluated during implementation):

- **Fixed-point Taylor series for `exp(x)`**: Express `exp(x) = 2^k × exp(r)` where `r` is small, compute `exp(r)` via truncated Taylor series with defined term count and precision. `ln` via inverse or separate series.
- **Fixed-point `cosh` directly**: `cosh(x) = (exp(x) + exp(-x))/2`. Avoids the log-sum-exp decomposition. Even Taylor terms only: `cosh(x) = 1 + x²/2! + x⁴/4! + ...`
- **CORDIC**: Iterative shift-and-add algorithm for hyperbolic functions. Deterministic by construction but potentially slower.
- **High-precision rational arithmetic**: Use a bignum library with exact rational intermediates, convert to u64 at the end. Simplest to reason about correctness but may be slow for 65K evaluations.

**Key constraint**: The algorithm must be fast enough that generating 65,536 F-values takes ≤ ~100ms. Point evaluation must be ≤ ~1μs.

**Precision requirement**: The `floor()` to u64 must be correct. This means the high-precision intermediate must have enough fractional bits that the error is < 1.0 at the final step. For the parameter ranges in practice (`max_loss_sats` in the 26-value set, `half_payout_sats` similarly), the F-values range from `max_loss_sats` (minimum, at `S_BIAS`) to roughly `max_loss_sats + b × ln(999)` (maximum, at the table edges). The absolute values are in the millions-to-billions range (sats), so 64-bit integer part + ~32 fractional bits should suffice. To be verified during implementation.

## Derivation Chain Summary

```
Input:      max_loss_sats (u64), half_payout_sats (u64)
                │
                ▼
Step 1:     b = max_loss_sats / ln(2)           [fixed-point, precision TBD]
                │
                ▼
Step 2:     q_step_lots = max(1, ceil(ln(999) × b / (65536 × half_payout_sats)))   [u64]
                │
                ▼
Step 3:     For i in 0..65536:                   [fixed-point, precision TBD]
              s = (i - 32768) × q_step_lots × half_payout_sats
              F(i) = floor(b × ln(exp(s/b) + exp(-s/b)))
                │
                ▼
Step 4:     Merkle root from F-values            [SHA256, already specified]
```

Steps 1-3 need the deterministic integer algorithm. Step 4 is already implemented and matches the `.simf`.

## Test Vectors

**To be generated after the algorithm is implemented.** The test vector set should cover:

1. **Minimum viable params**: smallest `max_loss_sats` and `half_payout_sats` in the 26-value convention set
2. **Typical params**: mid-range values representative of real pools
3. **Maximum params**: largest values in the convention set
4. **Edge indices**: `F(0)`, `F(S_BIAS)`, `F(S_MAX_INDEX)` — the extremes and the minimum
5. **Symmetry check**: `F(S_BIAS - k)` should equal `F(S_BIAS + k)` for all valid `k` (the cost function is symmetric around the bias point)
6. **Full Merkle root**: at least one complete set of `(max_loss_sats, half_payout_sats) → q_step_lots → all 65536 F-values → Merkle root`
7. **Point evaluation consistency**: verify that evaluating `F(i)` individually produces the same value as the table generation path for all `i` in a test case

## Key Files

- `src-tauri/crates/deadcat-sdk/contract/lmsr_pool.simf` — `lmsr_table_leaf_hash`, `lmsr_table_node_hash`, `merkle_proof_step_fn` (the on-chain verification code — the authoritative definition of the Merkle format)
- `src-tauri/crates/deadcat-sdk/src/lmsr_pool/table.rs` — Rust-side Merkle tree (leaf hash, node hash, root, proof generation/verification — matches `.simf` byte-for-byte)
- `src-tauri/crates/deadcat-sdk/src/lmsr_pool/math.rs` — quoting logic (`quote_from_table`, `quote_exact_input_from_manifest`, `fee_free_yes_spot_price_bps`)
- `docs/lmsr-pool-design.md` — pool parameter design, derivation formulas, deterministic generation rationale
- `docs/deadcat-core-design.md` — LMSR Math section, point evaluation vs full table distinction
