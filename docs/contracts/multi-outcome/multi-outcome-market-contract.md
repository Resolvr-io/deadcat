# Multi-Outcome Prediction Market Contract

**Status**: Proposal — design specification for a new core contract type. Not yet implemented.

## Motivation

The existing prediction market contract (`prediction_market.simf`) supports exactly two outcomes (YES/NO). Many real-world prediction markets require more outcomes: elections with multiple candidates, sports tournaments with many teams, awards with many nominees. Polymarket routinely runs multi-outcome events with 50-128 outcomes.

This document specifies a generalization of the binary market to N mutually exclusive outcomes (N ≥ 2), preserving the existing contract's core properties:

- **Simple solvency invariant**: pre-resolution, there are equal numbers of outstanding tokens across all N outcomes, fully backed by collateral such that any single-outcome resolution leaves winners with their collateral to be claimed.
- **Permissionless operation**: anyone can split collateral into outcome tokens (issuance), merge a full set of outcome tokens back into collateral (cancellation), or redeem winning tokens after resolution. No admin key, no operator.
- **Oracle-only resolution**: a single BIP-340 signature from the pre-committed oracle key attests to which outcome won. Covenants enforce the oracle's narrow authority — attesting to an outcome, nothing else.
- **Composability**: the market contract exposes tokens as standard Elements assets. Any pool design, any maker order, any application can build on top.

The binary market is the N=2 special case of this design. In principle the two could be unified into a single code-generated family; in practice, whether to deprecate the existing binary contract or keep it alongside is a migration question deferred to a later decision.

## Overview

An N-outcome market issues N token types plus N reissuance tokens. Traders enter the market by **splitting** `collateral_per_set` collateral into one token of each of the N outcomes. They exit by **merging** one token of each outcome back into `collateral_per_set` collateral. At resolution, the oracle attests which outcome won; holders of that outcome's token redeem each token for `collateral_per_set` (full value). Losing tokens become worthless.

This generalizes the binary market's pair-issuance/pair-cancellation model:

| Binary market | Multi-outcome market |
|---|---|
| YES + NO tokens | N outcome tokens |
| `collateral_per_pair` | `collateral_per_set` |
| Issue pair (1 YES + 1 NO) | Split (1 of each outcome) |
| Cancel pair (burn 1 YES + 1 NO) | Merge (burn 1 of each outcome) |
| 2 reissuance tokens | N reissuance tokens |
| 8 covenant slots | 3N + 2 covenant slots |
| Oracle attests outcome_byte (0x00/0x01) | Oracle attests outcome_index (u8) |

The mathematical relationship between collateral and supply holds identically:

> **Solvency invariant**: for a market at any pre-resolution state, there are `S` outstanding tokens of each of the N outcomes, backed by exactly `S × collateral_per_set` collateral locked in the market covenant. When outcome `k` wins, `S` tokens of outcome `k` can each be redeemed for `collateral_per_set`. Total payout: `S × collateral_per_set`, exactly matching the locked collateral.

N is a compile-time constant per contract. Each supported N value has its own `.simf` file, generated from a template. See [Code Generation Strategy](#code-generation-strategy).

## Parameters

```rust
pub struct MultiOutcomeMarketParams {
    pub oracle_public_key: XOnlyPublicKey,       // BIP-340 Schnorr pubkey for oracle attestation
    pub collateral_asset_id: AssetId,            // L-BTC, USDt, or other Elements asset
    pub outcome_token_asset_ids: [AssetId; N],   // N outcome token asset IDs (derivable from creation tx)
    pub outcome_rt_asset_ids: [AssetId; N],      // N reissuance token asset IDs (derivable from creation tx)
    pub collateral_per_set: u64,                 // collateral backing one full set of N outcome tokens
    pub expiry_time: u32,                        // block height deadline
    pub outcome_count: u8,                       // N — redundant with array length, included for clarity in discovery
}
```

`2N` of the fields are derivable from the creation transaction's issuance entropy (the asset IDs). The remaining `outcome_count + 4` fields are stored in the OP_RETURN recovery hint. See [OP_RETURN Recovery Hint](#op_return-recovery-hint).

**Unit convention**: Same as binary market — all amounts in the smallest indivisible unit of the respective asset (satoshis for L-BTC, 10^-8 for USDt, etc.).

**Constraints** (enforced by builder / `derive_market_params`):
- `N ≥ 2` and `N ≤ MAX_N` where `MAX_N` is determined by the set of generated `.simf` files (see [Code Generation Strategy](#code-generation-strategy)).
- `collateral_per_set` in the canonical 1-2-5 mantissa table.
- `expiry_time` snapped to 60-block boundary.
- `collateral_asset_id` in the well-known set or exotic-escape-compatible.

## Covenant Structure

The market has **3N + 2** covenant slots, each with a unique script pubkey derivable from the contract params + slot identity:

| Slot index | Phase | Purpose |
|---|---|---|
| 0 | Dormant | Dormant RT for outcome 0 (0 outstanding sets) |
| 1 | Dormant | Dormant RT for outcome 1 (0 outstanding sets) |
| ... | Dormant | ... |
| N-1 | Dormant | Dormant RT for outcome N-1 (0 outstanding sets) |
| N | Unresolved | Unresolved RT for outcome 0 (>0 outstanding sets) |
| N+1 | Unresolved | Unresolved RT for outcome 1 (>0 outstanding sets) |
| ... | Unresolved | ... |
| 2N-1 | Unresolved | Unresolved RT for outcome N-1 (>0 outstanding sets) |
| 2N | Unresolved | Unresolved collateral slot (>0 outstanding sets) |
| 2N+1 | Resolved_0 | Collateral (outcome 0 won, awaiting redemption) |
| 2N+2 | Resolved_1 | Collateral (outcome 1 won, awaiting redemption) |
| ... | Resolved_k | ... |
| 3N | Resolved_{N-1} | Collateral (outcome N-1 won, awaiting redemption) |
| 3N+1 | Expired | Collateral (expired, awaiting redemption) |

**Slot count by N**:

| N | Total slots |
|---|---|
| 2 | 8 (matches current binary) |
| 3 | 11 |
| 5 | 17 |
| 10 | 32 |
| 15 | 47 |

All slot scripts are static (computable at ingestion time) and can be pre-stored for script-based chain sync. This matches the current binary market's approach.

**Phase semantics** (equivalent to the binary market's covenant phases):
- **Dormant**: zero outstanding sets. Only RTs exist on-chain; no collateral is locked. Reachable at creation and after full cancellation. Can transition to Unresolved (split), Resolved (dormant oracle resolution), or Expired (dormant timelock expiry).
- **Unresolved**: nonzero outstanding sets. N RT UTXOs + 1 collateral UTXO exist. Can transition to Unresolved (subsequent split or partial merge), Dormant (full merge), Resolved_k (oracle resolution), or Expired (timelock expiry).
- **Resolved_k**: outcome k has won. Only the collateral UTXO exists (at the Resolved_k script). Winning outcome k tokens can be redeemed against it at full value.
- **Expired**: timelock has passed with no resolution. Collateral UTXO at the Expired script. See [Expiry Redemption Rate](#expiry-redemption-rate) for the redemption semantics.

## Spend Paths

| Transition | From slots | To slots | Authorization | Covenant enforces |
|---|---|---|---|---|
| Initial split | 0..N-1 (all Dormant RTs) | N..2N (all Unresolved RTs), 2N (collateral) | RT spend | Collateral = sets × `collateral_per_set`; N issuances, one per RT input; deterministic RT blinding |
| Subsequent split | N..2N-1, 2N | N..2N-1, 2N | RT spend | Collateral increased by sets × `collateral_per_set`; N issuances; sibling UTXO check across all N+1 inputs |
| Partial merge | N..2N-1, 2N | N..2N-1, 2N | RT spend + token burn | Collateral decreased; 1 of each outcome token burned per set; sibling UTXO check |
| Full merge | N..2N-1, 2N | 0..N-1 | RT spend + token burn | All collateral returned; all outstanding sets burned; sibling UTXO check |
| Resolution (outcome k) | N..2N-1, 2N | 2N+1+k | Oracle BIP-340 signature | Oracle signs tagged hash of market_id + outcome_index; all N RTs burned; collateral preserved at Resolved_k script; sibling UTXO check |
| Redemption (resolved) | 2N+1+k | — | Token burn | Winning outcome k tokens burned; collateral released at full value (1 token → `collateral_per_set`) |
| Redemption (expired) | 3N+1 | — | Token burn | Any outcome token burned; collateral released at expiry redemption rate (see below) |
| Expiry | N..2N-1, 2N | 3N+1 | Timelock ≥ `expiry_time` | All N RTs burned; collateral preserved at Expired script |
| Dormant resolution | 0..N-1 | — | Oracle BIP-340 signature | All N dormant RTs consumed; no covenant outputs |
| Dormant expiry | 0..N-1 | — | Timelock ≥ `expiry_time` | All N dormant RTs consumed; no covenant outputs |

**Sibling UTXO check** (generalization of the binary market's check): all transitions that co-spend RTs and collateral verify that all N+1 covenant inputs share the same `prev_txid`. This prevents collateral substitution attacks (see [enforcement-layers.md](../../architecture/enforcement-layers.md)).

Partial merge must co-spend all N RTs to maintain the sibling invariant, same as the binary market's partial cancellation refactor.

## Oracle Attestation

The oracle signs a BIP-340 tagged hash:

```
message = tagged_hash("deadcat/oracle_attestation", market_id || outcome_index)
market_id = SHA256(outcome_token_asset_ids[0] || outcome_token_asset_ids[1] || ... || outcome_token_asset_ids[N-1])
outcome_index = u8, in range [0, N-1]
```

The tag string (`"deadcat/oracle_attestation"`) matches the binary market's tag — the hash construction is identical, just with a u8 outcome_index replacing the 0x00/0x01 outcome_byte. For N=2, the hashes are distinct from the binary market (because `market_id` is computed from the two asset IDs concatenated, not from the binary-specific YES/NO pair) but the signature scheme is otherwise identical.

Oracles signing for both binary markets and multi-outcome markets use the same key and the same tag. Domain separation is achieved via the `market_id` — a given `market_id` uniquely identifies one market (binary or multi-outcome), and the covenant verifies the signature against its specific `oracle_public_key` parameter.

## State Machine

From the perspective of `deadcat-core`, the market state is one of:

```rust
pub enum MultiOutcomeMarketState {
    Trading {
        outstanding_sets: u64,
    },
    Resolved {
        outcome_index: u8,
        outstanding_sets: u64,
    },
    Expired {
        outstanding_sets: u64,
    },
}
```

`Trading` covers both Dormant (outstanding_sets = 0) and Unresolved (outstanding_sets > 0) covenant phases — the distinction is a covenant implementation detail. From the user's perspective, a market is either open for trading, resolved, or expired.

`Resolved { outstanding_sets: 0 }` and `Expired { outstanding_sets: 0 }` are terminal — all collateral has been redeemed.

Transition diagram (simplified):

```
           ┌─────────────┐
           │   Trading   │
           │ (sets = 0)  │ ── split ──> Trading (sets > 0)
           └──────┬──────┘
                  │
           ┌──────▼──────┐
           │   Trading   │ ── split ──> Trading (more sets)
           │ (sets > 0)  │ ── merge ──> Trading (fewer sets or 0)
           └──┬───┬───┬──┘
              │   │   │
     oracle───┘   │   └───expiry
       │          │         │
       ▼          ▼         ▼
    Resolved   Resolved   Expired
    outcome_0  outcome_k
    ...          ...
    outcome_{N-1}        ─────── each redeems to outcome-k tokens
```

The outstanding_sets count changes via split (increase) and merge (decrease). Resolution and expiry transitions preserve outstanding_sets (the count at transition time), and post-resolution/expiry redemptions decrement it to zero.

## Witness-Parameterized Output Indices

The current binary market contract uses hardcoded absolute output indices: `jet::current_index() == 0/1/2` and outputs at positions 0, 1, 2. This is simple and unambiguous for single-contract transactions.

**For the multi-outcome market, we propose witness-parameterized output indices** (as used in the current LMSR pool). The covenant accepts `out_base` from the witness and places outputs at positions `out_base`, `out_base+1`, ..., `out_base+N` (for the N RT outputs + 1 collateral output).

### Why witness-parameterized

Composability with pools (e.g., the QMSR pool proposal) requires the market's split/merge operations to co-exist with pool swaps in a single transaction. With hardcoded indices, two different contracts cannot both have their inputs at index 0. Witness-parameterized indices allow flexible arrangement.

This matches the design pattern already established by the LMSR pool and the order contract's remainder output. See [transaction-composability-model.md](../../architecture/transaction-composability-model.md) for the general framework.

### Aliasing defense

Output aliasing is prevented by **script uniqueness** — the 3N+2 slot scripts are unique per contract (derived from all params including the N asset IDs). Two different markets have entirely different script pubkeys. A witness-parameterized output cannot be aliased with another contract's output because the covenant verifies the exact script pubkey at the specified index.

Within a single market transition (e.g., a split), the N+1 output slots each have a distinct script. The covenant verifies each at its respective position (out_base, out_base+1, ..., out_base+N). No two outputs in the transition share a script, so no within-transition aliasing is possible.

## Split and Merge Semantics

### Split

A split transaction creates `sets` new sets of outcome tokens, locking `sets × collateral_per_set` additional collateral in the market.

**Initial split** (from Dormant, outstanding_sets = 0):

```
Inputs:
  [in_base]     DormantRT for outcome 0 (carries issuance: nAmount = sets, outcome_0 asset)
  [in_base+1]   DormantRT for outcome 1 (carries issuance: nAmount = sets, outcome_1 asset)
  ...
  [in_base+N-1] DormantRT for outcome N-1
  wallet input (collateral for sets × collateral_per_set, plus fees)

Outputs:
  [out_base]     UnresolvedRT for outcome 0 (blinded, covenant continuation)
  [out_base+1]   UnresolvedRT for outcome 1 (blinded, covenant continuation)
  ...
  [out_base+N-1] UnresolvedRT for outcome N-1
  [out_base+N]   UnresolvedCollateral (explicit, value = sets × collateral_per_set)
  [token_dest_0] Outcome 0 tokens (nAmount = sets)
  ...
  [token_dest_{N-1}] Outcome N-1 tokens
  fee, change
```

Each RT input carries an issuance of `sets` tokens of the corresponding outcome asset. The covenant verifies:

- Each RT input `i` ∈ [in_base, in_base+N) carries issuance of exactly `sets` tokens of `outcome_token_asset_ids[i - in_base]`.
- The collateral output at `out_base+N` has asset `collateral_asset_id` and value ≥ sets × collateral_per_set (the excess is the wallet's contribution; the exact amount is verified via Elements per-asset balance).
- The N RT outputs at `out_base..out_base+N-1` have the correct Unresolved slot scripts and correct RTs, with deterministic blinding (see [deterministic-rt-blinding.md](../../protocol/deterministic-rt-blinding.md)).
- Sibling UTXO check: all N+1 covenant inputs share the same `prev_txid`.

**Subsequent split** (from Unresolved, outstanding_sets > 0): same structure, but the N+1 inputs come from Unresolved slots (N..2N), and the collateral input already has `existing_sets × collateral_per_set`. The covenant verifies the collateral increase equals `sets × collateral_per_set`.

### Merge

A merge transaction burns `sets` complete sets of outcome tokens (one of each outcome), releasing `sets × collateral_per_set` collateral.

**Partial merge** (Unresolved → Unresolved with fewer sets):

```
Inputs:
  [in_base..in_base+N-1] UnresolvedRTs (N inputs)
  [in_base+N]            UnresolvedCollateral
  wallet inputs: sets × N outcome tokens (one of each outcome, to burn)

Outputs:
  [out_base..out_base+N-1] UnresolvedRTs (continuation)
  [out_base+N]             UnresolvedCollateral (value decreased by sets × collateral_per_set)
  wallet output:           sets × collateral_per_set collateral (to user)
  N token burn outputs:    each OP_RETURN with sets of one outcome asset
  fee, change
```

The covenant verifies:
- N token burn outputs, each burning exactly `sets` tokens of one distinct outcome asset.
- Collateral decrease equals `sets × collateral_per_set`.
- N+1 covenant input/output continuations with correct scripts and preserved RT amounts.
- Sibling UTXO check.

**Full merge** (Unresolved → Dormant): same structure, but all outstanding sets are burned and the collateral UTXO is consumed entirely. Outputs go to Dormant RT slots (0..N-1), no collateral continuation.

## Expiry Redemption Rate

**Open question**: what redemption rate should expired markets pay?

The binary market pays half value (1 token → `collateral_per_pair / 2`) on expiry redemption. This is symmetric — both YES and NO holders get the same payout, splitting the collateral equally regardless of which side they held.

For N outcomes, the natural generalization is `1 / N` value:

```
expiry_redemption_rate = collateral_per_set / N
```

Each outcome token holder gets `collateral_per_set / N` per token. If all `S` sets' worth of tokens (across all outcomes) are redeemed, the total payout is `S × N × (collateral_per_set / N) = S × collateral_per_set` — exactly matching the locked collateral. Solvency is preserved.

Considerations:
- **Rounding**: `collateral_per_set / N` may not be an integer. Round down to preserve solvency (small rounding residual stays in the market and can be reclaimed after all redemptions, possibly via a final sweep).
- **Fairness**: the 1/N rate is neutral — it reflects "no information" about which outcome would have won. Alternatively, the market could distribute the collateral to ALL holders regardless of outcome, which equates to 1/N anyway.
- **Asymmetric alternatives**: we could require `collateral_per_set` to be divisible by N (via builder constraint), eliminating the rounding residual. This is a minor constraint but simplifies the covenant.

**Recommendation**: enforce `collateral_per_set % N == 0` via builder validation. Redemption rate is `collateral_per_set / N` per token. This keeps the covenant math clean.

## Code Generation Strategy

The multi-outcome market covenant is **code-generated** from a template. Each supported N has its own `.simf` file:

```
src-tauri/crates/deadcat-sdk/contract/
├── prediction_market.simf              # current binary market (N=2 legacy)
├── multi_outcome_market_n3.simf        # generated
├── multi_outcome_market_n4.simf        # generated
├── multi_outcome_market_n5.simf        # generated
├── ...
└── multi_outcome_market_n15.simf       # generated
```

### Template structure

A Rust build script (`build.rs` or a dedicated `codegen` crate) reads a template SimplicityHL file and produces concrete `.simf` files for each N in the supported range.

The template has parameterized sections:
- N param declarations for outcome token asset IDs
- N param declarations for outcome RT asset IDs
- N RT slot programs (each handles the Dormant and Unresolved phases for its outcome)
- N-way loops for verifying token issuances on split
- N-way loops for verifying token burns on merge
- Resolution dispatch: `match outcome_index { 0 => ..., 1 => ..., ..., N-1 => ... }`
- Redemption dispatch: similar match over the resolved-outcome slot

The generator unrolls the loops at build time (since SimplicityHL has no loops). Each generated `.simf` is a self-contained, hand-readable program.

### Supported N values

Proposed initial range: **N=3 through N=15**.

- N=2 is deferred to the migration decision: whether to regenerate the binary market from this template or keep `prediction_market.simf` as a special case.
- N=15 covers Polymarket-scale events with room to spare. Higher N values can be added later as needed.
- Each (N) adds roughly linear complexity to the generated program. Witness size scales with N. Transaction weight for split/merge scales with N (since all N+1 covenant I/O must be co-spent).

### Audit & review

Generated `.simf` files are committed to the repo alongside the template. This ensures:
- Auditors can review the exact programs that will be compiled, not a meta-description.
- Diffs between N values are clear — reviewers can verify the generator produces the expected structural generalization.
- Changes to the template trigger regeneration as part of CI, with review required for the diff.

### Compilation caching

Each `.simf` file's compiled CMR is cached in the `deadcat-core` build output. The per-N CMR is deterministic (given a template version). Consumers that need a specific CMR (e.g., for ingestion verification) use the cache rather than recompiling.

## OP_RETURN Recovery Hint

The binary market's OP_RETURN hint is ~40 bytes (fixed). The multi-outcome hint is slightly larger and variable by N:

**Fixed portion** (independent of N):
- `collateral_per_set` (u9 mantissa + exponent: 2 bytes compressed)
- `expiry_time` (u24 or absolute: see existing binary convention)
- `oracle_public_key` (32 bytes)
- `collateral_asset_id` (32 bytes, or index into well-known set: 1 byte)
- `outcome_count` (u8: 1 byte)

**Variable portion** (scales with N):
- The 2N asset IDs (N outcome tokens + N RTs) are derivable from the creation transaction's issuance entropy, not stored in the hint.

Total hint size: ~40 bytes regardless of N (matching the binary hint). The variable-N data is entirely recovered from the on-chain issuance metadata.

See [chain-only-recovery.md](../../protocol/chain-only-recovery.md) for the recovery flow. The extension to multi-outcome markets is natural: a wallet scanning for asset IDs that match one of a market's `outcome_token_asset_ids` queries the issuance transaction, reads the OP_RETURN, reconstructs the params, and ingests the market.

## Relationship to the Binary Market

For N=2, the multi-outcome market contract is structurally equivalent to the binary market:

- 2 outcome tokens ≈ YES + NO
- 2 reissuance tokens ≈ YES RT + NO RT
- 8 slots (3N + 2 = 8 for N=2)
- Oracle signs outcome_index (0 or 1) ≈ outcome_byte (0x00 or 0x01)

The differences:
- **`market_id` differs**: the multi-outcome market's market_id is `SHA256(asset_id_0 || asset_id_1)` while the binary market's is `SHA256(yes_asset || no_asset)`. For N=2 deployments, these would produce identical hashes if the asset ordering matches (which it does by convention: outcome 0 = YES, outcome 1 = NO). This is a cosmetic point — same input, same hash.
- **Witness-parameterized indices**: the multi-outcome market uses out_base from witness; the binary market uses hardcoded positions. The former is more flexible (enables composition with pools) at the cost of a slightly more complex spend path.
- **Partial merge requires co-spend**: the multi-outcome market always co-spends all N+1 covenant inputs on partial merge (for the sibling check). The binary market's original design had partial cancellation spending only the collateral slot; the [refactor](../../architecture/enforcement-layers.md) added RT co-spend for the sibling check. The multi-outcome contract starts with this property built in.

**Migration question** (deferred): do we:

- **(a) Keep `prediction_market.simf` as the canonical N=2 contract** and use `multi_outcome_market_n{N}.simf` for N ≥ 3? The binary contract is battle-tested; new code generation introduces risk we don't need to take for the already-working case.
- **(b) Regenerate `multi_outcome_market_n2.simf` from the template** and deprecate `prediction_market.simf`? Uniform code generation, single contract family, no special-case logic in `deadcat-core`.

Option (b) is cleaner architecturally but requires validating that the generated N=2 contract is behaviorally equivalent to the current binary market (and handling any CMR/address differences in the ecosystem). This decision can be made after the generator is built and tested — we don't need to commit now.

## Security Properties

The multi-outcome market preserves all the security properties of the binary market, generalized from 2 to N:

| Property | Enforcement |
|---|---|
| Collateral conservation on split | Covenant checks `collateral_increase = sets × collateral_per_set` |
| Equal token supply across outcomes | Covenant verifies N token issuances of equal amount on split; N token burns of equal amount on merge |
| Oracle-only resolution | BIP-340 signature verification against `oracle_public_key` |
| Correct redemption rate (resolved) | Covenant releases `collateral_per_set` per winning token |
| Correct redemption rate (expired) | Covenant releases `collateral_per_set / N` per any outcome token |
| Deterministic RT blinding | Same scheme as binary market, applied to N RTs |
| RT destruction on terminal transitions | All N RTs burned on resolution and expiry |
| Collateral UTXO authenticity | Sibling UTXO check across all N+1 covenant inputs |
| No parasitic issuance | `ensure_no_issuance` on all non-issuance paths for all N+1 inputs |
| No double resolution | Resolution consumes all N RT UTXOs; no spend path back to Unresolved |

See [enforcement-layers.md](../../architecture/enforcement-layers.md) for the framework and the cross-layer analysis (which generalizes directly from 2 to N).

## Impact on deadcat-core

The existing `ContractEngine` API generalizes naturally:

- `ingest_market` accepts either `PredictionMarketParams` (binary, legacy) or `MultiOutcomeMarketParams` (new), via a unified `MarketParams` enum or separate methods.
- `build_split_pset` (renamed from `build_issuance_pset`) takes a market ID and a `sets` count.
- `build_merge_pset` (renamed from `build_cancellation_pset`) takes a market ID and a `sets` count.
- `build_oracle_resolve_pset` takes a market ID, `outcome_index`, and the oracle signature.
- `build_redemption_pset` takes a market ID, `outcome_index` (must match the resolved outcome), and `tokens_to_redeem`.
- `build_expire_transition_pset` and `build_expired_redemption_pset` are unchanged in shape.

Rename recommendations:
- `issuance` → `split` (more accurate for N outcomes; reads naturally for N=2 too)
- `cancellation` → `merge`
- `pairs` → `sets` everywhere
- `collateral_per_pair` → `collateral_per_set`

The `Side` enum (currently `{ Yes, No }`) generalizes to `OutcomeIndex(u8)` with convenience constants `pub const YES: OutcomeIndex = OutcomeIndex(0);` and `pub const NO: OutcomeIndex = OutcomeIndex(1);` for binary markets.

Full `deadcat-core` API changes are out of scope for this doc — they'll be addressed in a subsequent pass over `../../architecture/deadcat-core-design.md`.

## Pending Work

Validation and design-completion work needed before this proposal graduates from "proposal" to "committed design":

| Item | Purpose |
|---|---|
| Prototype the code generator | Produce a generated `.simf` for N=3 and N=5. Verify the template handles all spend paths correctly. |
| Compile prototype `.simf` files | Confirm SimplicityHL compiler handles the generated code without errors. Measure program size and witness size per N. |
| Benchmark transaction weights | Measure actual vBytes for split/merge/resolution transactions at each N. Validate scaling matches theoretical estimates. |
| Validate sibling UTXO check scaling | Confirm the N+1-way prev_txid check fits in the witness budget for N up to MAX_N. |
| Decide expiry redemption rate | Confirm the 1/N rate + `collateral_per_set % N == 0` constraint is acceptable. Alternatively specify and implement rounding-residual handling. |
| Decide N=2 migration path | Choose between keeping `prediction_market.simf` and regenerating from template. |
| Write `.simf` template formally | Document the template format, parameterization mechanism, and generator algorithm. |
| Specify builder convention validation | What N values are permitted? What ordering of outcomes? What naming convention? |
| Update `../contract-specification.md` | Add the multi-outcome market as a third contract type alongside the binary market, LMSR pool, and maker order. |
| Generate test vectors | Per-N test vectors covering creation, split, merge, resolution (each outcome), redemption, expiry, and edge cases (N=2 boundary, max N, outstanding_sets = 0 terminal paths). |

## Key Files

- `docs/contracts/multi-outcome/multi-outcome-market-contract.md` — this document
- `docs/contracts/contract-specification.md` — to be updated with the multi-outcome market spec
- `docs/architecture/enforcement-layers.md` — security properties generalized from binary to N
- `docs/protocol/chain-only-recovery.md` — recovery flow extends to multi-outcome via issuance indexing
- `docs/protocol/deterministic-rt-blinding.md` — RT blinding applied per-outcome
- `docs/protocol/oracle-bip340-tagged-hash.md` — oracle attestation format extends to outcome_index
- `docs/architecture/transaction-composability-model.md` — witness-parameterized indices enable composition with pools
- `docs/contracts/prediction-market/market-dormant-terminal-paths.md` — dormant terminal paths generalize from 2 to N RTs
- Future: `src-tauri/crates/deadcat-sdk/contract/multi_outcome_market_n{N}.simf` — generated contract files
- Future: `src-tauri/crates/deadcat-sdk/codegen/multi_outcome_market_template.simf` — the generator input
