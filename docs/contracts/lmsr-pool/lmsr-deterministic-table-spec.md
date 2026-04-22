# LMSR Deterministic Table Specification

**Status**: Specified — runtime algorithm is arbitrary-precision bignum computation of the closed-form F-value expression, with committed reference Merkle roots as regression fixtures in `deadcat-codegen`. No fixed-point precision tuning, no Taylor-series term-count bound, no precomputed transcendental constants are needed.

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

### Mathematical definition

```
F(i) = floor(b × ln(exp(q_yes(i)/b) + exp(q_no(i)/b)))

where:
    b = max_loss_sats / ln(2)
    q_yes(i) = (i - S_BIAS) × q_step_lots × half_payout_sats
    q_no(i) = -q_yes(i)
    S_BIAS = 32,768  (protocol constant)
```

Since `q_no = -q_yes`, this simplifies (via `exp(x) + exp(-x) = 2 × cosh(x)` and `b × ln(2) = max_loss_sats`) to:

```
F(i) = max_loss_sats + floor(b × ln(cosh(s/b)))
where s = q_yes(i)
```

At `i = S_BIAS`: `s = 0`, `cosh(0) = 1`, `ln(1) = 0`, so `F(S_BIAS) = max_loss_sats` (the minimum F-value, representing the pool's maximum loss).

The per-index range of `s/b` is bounded by construction of `q_step_lots`: `|s/b| ≤ ln(999)/2 ≈ 3.45`. This bounds the working range of the transcendental evaluation.

### Runtime algorithm: arbitrary-precision bignum

The runtime algorithm is direct arbitrary-precision evaluation of the closed-form expression above. Specifically:

1. **Dependencies**: `num-bigint` for arbitrary-precision integer arithmetic and `num-rational` for exact rational arithmetic. For transcendental evaluation (`cosh`, `ln`), compute to sufficient working precision (nominally 200+ bits) via any deterministic method — Taylor series at high precision, continued fractions, or a deterministic MPFR-backed implementation are all acceptable choices. The reference implementation in `deadcat-codegen` picks one method; other implementations must match the reference's F-values byte-for-byte.

2. **Precision budget**: working precision must be sufficient that the final `floor()` to u64 is correct across the entire param space. For `max_loss_sats ≤ 10^16` and `|s/b| ≤ 3.45`, 200 bits of working precision in rational/high-precision floating intermediate yields `floor()` correctness with ~50 bits of margin. No fixed-point analysis required.

3. **Derivation chain** (see [Derivation Chain Summary](#derivation-chain-summary) for the full pipeline):
   - `b = max_loss_sats / ln(2)` — stored as a high-precision rational or arbitrary-precision float.
   - `q_step_lots = max(1, ceil(ln(999) × b / (65536 × half_payout_sats)))` — output is u64; ceiling rounding.
   - For each `i ∈ [0, 65536)`: compute `s`, then `F(i) = max_loss_sats + floor(b × ln(cosh(s/b)))` with final `floor()` rounding to u64.

4. **No fixed-point precision tuning, no Taylor term-count tuning, no precomputed irrational constants.** The bignum precision budget is deliberately over-provisioned so that implementation details of the transcendental step do not affect output correctness. Two compliant implementations using different precision levels (as long as both exceed the budget) produce identical F-values.

5. **Caching strategy**: `deadcat-core` caches generated F-value tables per unique `(max_loss_sats, half_payout_sats)` pair to amortize the bignum cost. First-use cost per pool parameter combination is on the order of 5–10 seconds (bignum is slow); subsequent lookups on the cached table are O(1). Cache is stored in memory and optionally persisted to disk by the consuming wallet layer.

### Protocol constants

- `ln(2)`, `ln(999)` — no precomputed approximation is committed at the protocol level. Implementations compute or embed these at their own chosen precision, provided the overall F-value output matches the reference.
- `S_BIAS = 32,768`, `S_MAX_INDEX = 65,535`, `TABLE_DEPTH = 16` — unchanged, integer constants.
- `ln(999)` encodes the 0.1%–99.9% price range target (the table edges correspond to implied YES prices of ~0.1% and ~99.9%).

### Why bignum over fixed-point Taylor

Several alternatives were considered and rejected in favor of bignum:

- **Fixed-point Taylor + range reduction**: fast at runtime (~100ms for a full table, ~1μs per point evaluation) but requires careful precision analysis, worked examples, and a correctness proof ("Taylor matches reference for all 256 combos"). Substantial spec surface area for a marginal runtime-performance benefit.
- **CORDIC**: similar complexity concerns; no clear win over Taylor.
- **Hybrid (bignum for compile-time Merkle root, Taylor for runtime)**: can be added later as a pure optimization. The committed reference Merkle roots (see [Reference Fixtures](#reference-fixtures)) serve as the canonical acceptance criterion for any alternative implementation; switching to Taylor in a future release is non-breaking provided all reference roots reproduce byte-for-byte.

For v1, bignum-only prioritizes correctness and spec simplicity over runtime performance. The cold-start cost (5–10 seconds per new pool combo, one-time per install) is acceptable given that pool creation and first-ingest events are infrequent compared to trading activity.

## Derivation Chain Summary

```
Input:      max_loss_sats (u64), half_payout_sats (u64)
                │
                ▼
Step 1:     b = max_loss_sats / ln(2)                        [bignum rational / high-precision float]
                │
                ▼
Step 2:     q_step_lots = max(1, ceil(ln(999) × b / (65536 × half_payout_sats)))   [u64, ceiling rounding]
                │
                ▼
Step 3:     For i in 0..65536:                               [bignum rational / high-precision float]
              s = (i - 32768) × q_step_lots × half_payout_sats
              F(i) = max_loss_sats + floor(b × ln(cosh(s/b)))   [final floor to u64]
                │
                ▼
Step 4:     Merkle root from F-values                        [SHA256, already specified — see Merkle Tree Format]
```

All arithmetic operations use the same bignum precision budget. No intermediate step uses a lower precision than the rest.

## Reference Fixtures

Correctness validation uses the committed Merkle root approach rather than per-index test vectors:

- **`deadcat-codegen`** (dev-only crate) contains the bignum reference implementation and a committed fixture file mapping each of the 256 `(max_loss_sats, half_payout_sats)` parameter combinations to:
  - Its canonical Merkle root (32 bytes).
  - Anchor F-values at key indices (`F(0)`, `F(S_BIAS)`, `F(S_MAX_INDEX)`) for human inspection and debugging.
  - The resolved `q_step_lots`.
- **Regression test** runs on every `cargo test`: re-execute the bignum reference for each of the 256 combos, assert each computed Merkle root matches the committed fixture value. If the bignum implementation or its dependencies ever produce different output (e.g., `num-bigint` version bump changes rounding behavior), this test fails loudly.
- **Regeneration**: `just regenerate-lmsr-fixtures` invokes the reference generator to emit fresh fixture content. Run by developers when an intentional algorithm change (or parameter-space expansion) is being committed.
- **Cross-implementation conformance**: future alternative implementations (Taylor, CORDIC, cross-language ports) target the same committed Merkle roots. Any implementation reproducing all 256 roots byte-for-byte is provably equivalent to the bignum reference over the entire valid parameter space.
- **No per-index test vector is committed** — the Merkle root over 65,536 F-values is a stronger equivalence check than any finite sample of individual F-values. If two implementations agree on the root, they agree on every F-value.

## Precision Calibration

The "200+ bits working precision is sufficient" claim is empirically validated, not assumed. During `deadcat-codegen` development, a one-time calibration run establishes the minimum safe precision for the chosen bignum method:

1. **Establish ground truth**: generate all 256 Merkle roots at a very high precision (e.g., 512 bits). Re-generate at 1024 bits and assert the results match byte-for-byte. If they don't, the starting precision is too low — go higher. Once two consecutive precision levels agree, the higher one is "trusted truth."
2. **Binary-search downward**: halve the precision repeatedly (512 → 256 → 128 → 64 → …). At each level, regenerate all 256 roots plus their underlying 16.7M F-values (65,536 per combo × 256 combos) and compare to ground truth.
3. **Record the threshold**: the first precision where any root or F-value disagrees with ground truth is the "minimum safe precision" for this bignum method. The precision budget pinned in the spec (nominally 200 bits) must exceed this threshold by a comfortable margin.

The calibration is a **one-time development artifact**, not a per-CI regression. Its output is a documented fact pinned in the satellite spec (e.g., "empirical minimum for our bignum method is X bits; we use 200 bits for Y bits of safety margin"). The ongoing regression check remains the committed Merkle roots — any implementation that reproduces them byte-for-byte is provably equivalent.

The threshold is **specific to the chosen bignum method** (Taylor series at high precision, continued fractions, MPFR-backed evaluation, etc.). An alternative implementation with a different method may have a different minimum; it only needs to reproduce the committed roots, not the threshold. Implementations whose published conformance demands that they document their precision strategy (for auditability) may cite this calibration result as the justification for their chosen working precision.

A CLI subcommand in `deadcat-codegen` (`just calibrate-precision`) runs the calibration and emits a report. Integrators and auditors can re-run it against a fork or alternative implementation to validate the precision claim.

## Key Files

- `src-tauri/crates/deadcat-sdk/contract/lmsr_pool.simf` — `lmsr_table_leaf_hash`, `lmsr_table_node_hash`, `merkle_proof_step_fn` (the on-chain verification code — the authoritative definition of the Merkle format)
- `src-tauri/crates/deadcat-sdk/src/lmsr_pool/table.rs` — Rust-side Merkle tree (leaf hash, node hash, root, proof generation/verification — matches `.simf` byte-for-byte)
- `src-tauri/crates/deadcat-sdk/src/lmsr_pool/math.rs` — quoting logic (`quote_from_table`, `quote_exact_input_from_manifest`, `fee_free_yes_spot_price_bps`)
- `crates/deadcat-codegen/` (planned, shared with the multi-outcome `.simf` generator) — bignum reference implementation and committed fixture file with per-combo Merkle roots and anchor F-values
- `crates/deadcat-core/` — runtime F-value computation (bignum-based), per-pool-combo in-memory cache
- `docs/contracts/lmsr-pool/lmsr-pool-design.md` — pool parameter design, derivation formulas, deterministic generation rationale
- `docs/architecture/deadcat-core-design.md` — LMSR Math section, cached-table runtime model and reserve-aware routing notes
