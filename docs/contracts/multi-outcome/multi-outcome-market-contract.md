# Multi-Outcome Prediction Market Contract

**Status**: Proposal — design specification for a new core contract type. Not yet implemented. Supersedes an earlier N-token (Arrow-Debreu) variant; see [Alternatives Considered](#alternatives-considered).

**Related**: this contract implements every principle in [market-contract-principles.md](../market-contract-principles.md) — permissionless operation within the solvency invariant, narrow oracle authority, terminal paths from every non-terminal state, RT destruction on resolution/expiry, sibling UTXO check, witness-parameterized indices, deterministic RT blinding, and the rest. The principles doc is the canonical specification of those shared properties; this doc focuses on what is specific to the multi-outcome contract (2N token model, per-outcome and cross-outcome operations, slot layout, code generation).

## Motivation

The existing prediction market contract (`prediction_market.simf`) supports exactly two outcomes (YES/NO). Many real-world markets require more: elections with multiple candidates, sports tournaments with many teams, awards with many nominees. Polymarket regularly runs multi-outcome events with 10-128 outcomes.

This document specifies a generalization of the binary market to N mutually exclusive and exhaustive outcomes (N ≥ 2), preserving the existing contract's core properties and extending its token model to first-class per-outcome YES/NO pairs:

- **Simple solvency invariant**: pre-resolution, the contract holds exactly enough collateral to cover the maximum possible payout across all N outcomes.
- **Per-outcome YES/NO tokens**: each outcome has both a YES token (pays on that outcome) and a NO token (pays on every other outcome), matching the mental model of the binary market N-fold.
- **Permissionless operation**: anyone can mint outcome pairs, burn pairs for collateral, split collateral into a full complement of YES (or NO) tokens across outcomes, or redeem winning tokens after resolution.
- **Oracle-only resolution**: a single BIP-340 signature attests which outcome won. Covenants enforce narrow oracle authority.
- **Composability**: tokens are standard Elements assets. Any pool design, maker order, or application can build on top.

## Design Rationale

**Why 2N tokens instead of N?**

A simpler variant — sometimes called the Arrow-Debreu approach — uses only N tokens, one per outcome. Holding `outcome_i` pays on outcome i and zero otherwise. A user who wants to bet *against* outcome i holds one of each other outcome's token (N-1 UTXOs).

This project rejected that variant for UX reasons:

1. **Negative positions require (N-1) UTXOs.** For anything but very small N, this is punishing: more UTXOs to manage, more dust, larger spend transactions, more mental overhead.
2. **Mental-model continuity with the binary market.** Users already understand YES/NO. Scaling that pattern to N-way is more intuitive than introducing "hold the complement of everything you want to bet against."
3. **AMM composition is cleaner.** A pool that makes markets in `YES_i` directly (with `NO_i` as the counter-asset) mirrors the binary pool design. With N-token, pool design gets asymmetric — long positions are single-token, short positions are multi-token.
4. **Liquid's asset model already requires one RT per token type.** The covenant overhead of supporting 2N tokens instead of N is 2× on RT slot count, not 2× on the fundamental design. Given (1)-(3), the UTXO overhead is worth it.

Trade-off accepted: transactions that co-spend all covenant I/O (split, merge) scale with 2N+1 instead of N+1, bounding practical N lower than the N-token design would have. The [Code Generation Strategy](#code-generation-strategy) discusses the range we intend to support.

## Overview

An N-outcome market issues **2N token types** (N YES tokens + N NO tokens) plus **2N reissuance tokens**. Token model:

- For each outcome i ∈ [0, N-1]: tokens `YES_i` and `NO_i` represent "outcome i wins" and "outcome i does not win" respectively.
- Pre-resolution invariant: the collateral held by the contract covers the maximum possible payout across all outcomes.
- On resolution of outcome k: all `YES_k` tokens redeem for `collateral_per_pair`; all `NO_i` tokens for i ≠ k redeem for `collateral_per_pair`; `YES_i` for i ≠ k and `NO_k` are worthless.

The fundamental per-outcome invariant carried over from the binary market: `YES_i + NO_i = collateral_per_pair`. Burning one of each redeems one unit of collateral.

Additionally, the multi-outcome contract enforces a cross-outcome invariant: `sum_i YES_i = collateral_per_pair`. Burning one YES of every outcome also redeems one unit of collateral.

Both invariants together imply `sum_i NO_i = (N-1) × collateral_per_pair`: burning one NO of every outcome redeems `(N-1) × collateral_per_pair`.

Binary market correspondence (N=2):

| Binary market | Multi-outcome market (N outcomes) |
|---|---|
| YES + NO tokens | 2N tokens (YES_i, NO_i for each outcome i) |
| `collateral_per_pair` | `collateral_per_pair` (per outcome, same convention) |
| Issue pair | Issue pair for outcome i (1 YES_i + 1 NO_i) |
| Cancel pair | Cancel pair for outcome i |
| (no analogue) | Split YES (1 of each YES_i for `collateral_per_pair`) |
| (no analogue) | Split NO (1 of each NO_i for `(N-1) × collateral_per_pair`) |
| 2 reissuance tokens | 2N reissuance tokens |
| 8 covenant slots | 5N + 2 covenant slots |
| Oracle attests outcome_byte | Oracle attests outcome_index (u8) |

### Solvency invariant (formal)

Let `y_i` = outstanding supply of `YES_i`, `n_i` = outstanding supply of `NO_i`, and `C` = collateral locked. For any resolution outcome k, the payout is:

```
payout(k) = collateral_per_pair × (y_k + sum_{j≠k} n_j)
```

The contract requires `C ≥ max_k payout(k)`. All supported operations preserve the tighter condition that this maximum is **outcome-independent** — i.e., `y_k + sum_{j≠k} n_j` is the same value `Q` for every outcome k. Therefore:

```
C = collateral_per_pair × Q
```

Operationally, preserving outcome-independence of Q constrains which `(Δy_i, Δn_i, Δc)` transitions the covenant permits. The [Operations](#operations) section enumerates the allowed primitives.

## Parameters

```rust
pub struct MultiOutcomeMarketParams {
    pub oracle_public_key: XOnlyPublicKey,       // BIP-340 Schnorr pubkey for oracle attestation
    pub collateral_asset_id: AssetId,            // L-BTC, USDt, or other Elements asset
    pub yes_token_asset_ids: [AssetId; N],       // YES_i asset IDs, derivable from creation tx
    pub no_token_asset_ids: [AssetId; N],        // NO_i asset IDs, derivable from creation tx
    pub yes_rt_asset_ids: [AssetId; N],          // YES_i reissuance tokens
    pub no_rt_asset_ids: [AssetId; N],           // NO_i reissuance tokens
    pub collateral_per_pair: u64,                // collateral backing one YES_i + NO_i pair
    pub expiry_time: u32,                        // block height deadline
    pub outcome_count: u8,                       // N — redundant with array length, included for discovery clarity
}
```

`4N` of the fields (the asset ID arrays) are derivable from the creation transaction's issuance entropy. The remaining `outcome_count + 4` fields are stored in the OP_RETURN recovery hint. See [OP_RETURN Recovery Hint](#op_return-recovery-hint).

**Unit convention**: same as binary market — all amounts in the smallest indivisible unit of the respective asset.

**Constraints** (enforced by builder / `derive_market_params`):
- `N ≥ 2` and `N ≤ MAX_N` per the set of generated `.simf` files.
- `collateral_per_pair` in the canonical 1-2-5 mantissa table.
- `collateral_per_pair % N == 0` to make expiry redemption rates exact (see [Expiry Redemption Rate](#expiry-redemption-rate)).
- `expiry_time` snapped to 60-block boundary.
- `collateral_asset_id` in the well-known set or exotic-escape-compatible.

## Covenant Structure

The market has **5N + 2** covenant slots:

| Slot range | Phase | Purpose |
|---|---|---|
| `0 .. N-1` | Dormant | Dormant RT for `YES_i` (no outstanding tokens) |
| `N .. 2N-1` | Dormant | Dormant RT for `NO_i` (no outstanding tokens) |
| `2N .. 3N-1` | Unresolved | Unresolved RT for `YES_i` |
| `3N .. 4N-1` | Unresolved | Unresolved RT for `NO_i` |
| `4N` | Unresolved | Unresolved collateral |
| `4N+1 .. 5N` | Resolved_k | Collateral (outcome k won, awaiting redemption), one slot per outcome |
| `5N+1` | Expired | Collateral (expired, awaiting redemption) |

Breakdown: 2N Dormant RTs + 2N Unresolved RTs + 1 Unresolved collateral + N Resolved_k collateral + 1 Expired collateral.

**Slot count by N**:

| N | Total slots | vs N-token variant |
|---|---|---|
| 2 | 12 | +4 |
| 3 | 17 | +6 |
| 5 | 27 | +10 |
| 8 | 42 | +16 |
| 10 | 52 | +20 |

All slot scripts are static (computable at ingestion time) and pre-stored for script-based chain sync.

**Phase semantics**:

- **Dormant**: zero outstanding pairs of any outcome. Only the 2N RT UTXOs exist on-chain; no collateral locked. Reachable at creation and after all tokens are burned back to zero. Transitions to Unresolved (first mint), Resolved_k (dormant oracle resolution), or Expired (dormant timelock expiry).
- **Unresolved**: nonzero outstanding supply of at least one token. 2N RT UTXOs + 1 collateral UTXO exist. Transitions to Unresolved (further mint/burn), Dormant (full burn), Resolved_k (oracle resolution), or Expired (timelock expiry).
- **Resolved_k**: outcome k has won. Only the collateral UTXO exists (at the Resolved_k script). `YES_k` holders and `NO_j` holders (j ≠ k) can redeem against it.
- **Expired**: timelock passed without resolution. Collateral UTXO at the Expired script. See [Expiry Redemption Rate](#expiry-redemption-rate).

## Operations

The covenant exposes the following operations in the Unresolved phase. All are permissionless (no oracle or admin signature required). Every listed operation preserves outcome-independence of Q (the solvency invariant stays tight).

### Per-outcome operations

**Issue pair for outcome i**: user pays `sets × collateral_per_pair`, receives `sets` of `YES_i` and `sets` of `NO_i`.
- `Δy_i = sets`, `Δn_i = sets`, all other deltas zero.
- `ΔQ = sets` uniformly across outcomes.
- `Δc = sets × collateral_per_pair`. ✓

**Cancel pair for outcome i**: user burns `sets` of `YES_i` and `sets` of `NO_i`, receives `sets × collateral_per_pair` collateral. Inverse of issue.

### Cross-outcome operations

**Split YES**: user pays `sets × collateral_per_pair`, receives `sets` of `YES_i` for every outcome i.
- `Δy_i = sets` for all i.
- `ΔQ = sets`. ✓

**Merge YES**: user burns `sets` of `YES_i` for every outcome i, receives `sets × collateral_per_pair` collateral.

**Split NO**: user pays `sets × (N-1) × collateral_per_pair`, receives `sets` of `NO_i` for every outcome i.
- `Δn_i = sets` for all i.
- `ΔQ = sets × (N-1)`. ✓

**Merge NO**: user burns `sets` of `NO_i` for every outcome i, receives `sets × (N-1) × collateral_per_pair` collateral.

### Derived operations (not first-class; expressible by composition)

**Cross swap `YES_i → {NO_j : j ≠ i}`** (Polymarket NegRiskAdapter's core invariant): `YES_i + (N-2) × collateral_per_pair ≡ {NO_j : j ≠ i}`.

Composition: cancel pair i (gives 1 collateral) + split NO (consumes N-1 collateral, produces 1 NO of each outcome including i) + cancel pair i using the new NO_i token (gives 1 collateral back) — net: -1 YES_i, +1 NO_j for j ≠ i, user paid N-2 collateral. ✓

The covenant does not expose this as a first-class operation; users and pools compose it from the primitives above.

## Spend Paths

| Transition | From slots | To slots | Authorization | Covenant enforces |
|---|---|---|---|---|
| Initial pair issue (outcome i) | All 2N Dormant RTs | All 2N Unresolved RTs, Unresolved collateral | RT spend (all 2N) | Collateral = sets × collateral_per_pair; issuance of `sets` tokens on YES_i RT and NO_i RT only; deterministic RT blinding |
| Subsequent pair issue (outcome i) | All 2N Unresolved RTs, Unresolved collateral | Same (continuation) | RT spend | Collateral increase = sets × collateral_per_pair; issuance on YES_i RT and NO_i RT only; sibling check across all 2N+1 covenant inputs |
| Pair cancel (outcome i) | All 2N Unresolved RTs, Unresolved collateral | Same (or transition to Dormant if last supply burned) | RT spend + token burn | Collateral decrease = sets × collateral_per_pair; burn outputs for sets of YES_i and sets of NO_i |
| Split YES | All 2N Unresolved RTs, collateral | Same | RT spend | Collateral increase = sets × collateral_per_pair; issuance of `sets` on each YES_i RT; no NO issuance |
| Merge YES | All 2N Unresolved RTs, collateral | Same (or → Dormant) | RT spend + token burn | Collateral decrease = sets × collateral_per_pair; burn outputs for `sets` of each YES_i |
| Split NO | All 2N Unresolved RTs, collateral | Same | RT spend | Collateral increase = sets × (N-1) × collateral_per_pair; issuance of `sets` on each NO_i RT |
| Merge NO | All 2N Unresolved RTs, collateral | Same (or → Dormant) | RT spend + token burn | Collateral decrease = sets × (N-1) × collateral_per_pair; burn outputs for `sets` of each NO_i |
| Resolution (outcome k) | All 2N Unresolved RTs, collateral | Resolved_k collateral | Oracle BIP-340 signature | Oracle signs tagged hash of market_id + outcome_index; all 2N RTs burned; collateral preserved at Resolved_k script |
| Redemption (resolved, winning YES_k) | Resolved_k | — | YES_k burn | YES_k tokens burned; collateral released at full value (1 token → `collateral_per_pair`) |
| Redemption (resolved, winning NO_j, j ≠ k) | Resolved_k | — | NO_j burn | NO_j tokens burned; collateral released at full value |
| Redemption (expired, YES_i) | Expired | — | YES_i burn | YES_i tokens burned; collateral released at yes_expiry_rate (see below) |
| Redemption (expired, NO_i) | Expired | — | NO_i burn | NO_i tokens burned; collateral released at no_expiry_rate (see below) |
| Expiry | All 2N Unresolved RTs, collateral | Expired | Timelock ≥ `expiry_time` | All 2N RTs burned; collateral preserved at Expired script |
| Dormant resolution (outcome k) | All 2N Dormant RTs | — | Oracle BIP-340 signature | All 2N RTs consumed, no covenant outputs |
| Dormant expiry | All 2N Dormant RTs | — | Timelock ≥ `expiry_time` | All 2N RTs consumed, no covenant outputs |

**Sibling UTXO check**: every transition in the Unresolved phase co-spends all 2N+1 covenant inputs and verifies they share the same `prev_txid`. This prevents collateral substitution attacks. See [enforcement-layers.md](../../architecture/enforcement-layers.md).

Per-outcome operations (issue pair, cancel pair) issue or burn only two RTs worth of tokens, but must still co-spend all 2N RTs in the witness to maintain the sibling invariant. The uninvolved RTs pass through unchanged (zero issuance, no burn).

## Oracle Attestation

Identical scheme to the binary market, with the outcome encoded as a u8 index:

```
message = tagged_hash("deadcat/oracle_attestation", market_id || outcome_index)
market_id = SHA256(yes_token_asset_ids[0] || no_token_asset_ids[0] || yes_token_asset_ids[1] || no_token_asset_ids[1] || ... || yes_token_asset_ids[N-1] || no_token_asset_ids[N-1])
outcome_index = u8, in range [0, N-1]
```

Tag string matches the binary market (`"deadcat/oracle_attestation"`). Domain separation comes from `market_id`.

## State Machine

```rust
pub enum MultiOutcomeMarketState {
    Trading {
        supplies: [PairSupply; N],   // y_i and n_i per outcome
    },
    Resolved {
        outcome_index: u8,
        winning_yes_supply: u64,      // supply of YES_{outcome_index}
        winning_no_supplies: [u64; N-1],  // supply of NO_j for j ≠ outcome_index
    },
    Expired {
        yes_supplies: [u64; N],
        no_supplies: [u64; N],
    },
}

pub struct PairSupply {
    pub yes: u64,
    pub no: u64,
}
```

`Trading` covers both Dormant (all supplies zero) and Unresolved phases. From the user's perspective, a market is either open for trading, resolved, or expired.

## Witness-Parameterized Output Indices

Same approach as the original multi-outcome proposal: the covenant accepts `out_base` from the witness and places outputs at `out_base..out_base + 2N` (2N RT outputs + 1 collateral output). See [transaction-composability-model.md](../../architecture/transaction-composability-model.md) for the general framework.

Aliasing defense: script uniqueness per slot + per-market script derivation means no output can alias another contract's output or another slot within this contract.

## Expiry Redemption Rate

If the oracle does not resolve by `expiry_time`, all outcome tokens become redeemable against the Expired collateral UTXO at a pre-computed rate. The rate treats every outcome as equally probable (1/N), which is the "no information" default and preserves solvency exactly.

Rates:
```
yes_expiry_rate = collateral_per_pair / N
no_expiry_rate  = collateral_per_pair × (N - 1) / N
```

**Solvency verification**: if all tokens redeem, total payout is:
```
sum(y_i) × yes_expiry_rate + sum(n_i) × no_expiry_rate
  = collateral_per_pair × [sum(y_i) / N + sum(n_i) × (N-1) / N]
  = collateral_per_pair × [Y + (N-1) × N_total] / N
```
where `Y = sum(y_i)` and `N_total = sum(n_i)`.

Using the outcome-independence constraint `y_k - n_k = D` (same constant D for all k), we have `Y = N_total + N×D`, so:
```
Y + (N-1) × N_total = N × N_total + N × D = N × (N_total + D)
```
Therefore total payout = `collateral_per_pair × (N_total + D)` which equals `C` exactly. ✓

Binary case (N=2): `yes_expiry_rate = no_expiry_rate = collateral_per_pair / 2` — matches the existing binary market.

**Rounding**: requiring `collateral_per_pair % N == 0` at builder time eliminates rounding residuals.

## Code Generation Strategy

Each supported N has its own `.simf` file generated from a template:

```
src-tauri/crates/deadcat-sdk/contract/
├── prediction_market.simf                # current binary market (N=2 legacy)
├── multi_outcome_market_n3.simf          # generated
├── multi_outcome_market_n4.simf          # generated
├── multi_outcome_market_n5.simf          # generated
├── ...
└── multi_outcome_market_nK.simf
```

**Supported N range**: proposed initial range is **N=3 through N=10**. N=10 caps split/merge transaction weight at a comfortable limit; higher N pushes against Liquid block constraints (2N+1 = 21 covenant I/Os at N=10). N values above this range should be expressed via hierarchical composition (markets-of-markets) or via binary-market composition with arbitrage-based coherency, not as a single flat multi-outcome contract. The design journal explores this split in detail.

N=2 remains served by the existing `prediction_market.simf`. Whether to regenerate N=2 from the template or keep the binary contract as a special case is deferred; see [Relationship to the Binary Market](#relationship-to-the-binary-market).

**Template structure**: a Rust build script (`build.rs` or a dedicated `codegen` crate) reads a template SimplicityHL file and produces concrete `.simf` files for each N. Parameterized sections include:
- 2N param declarations for asset IDs and RT asset IDs
- 2N RT slot programs (Dormant + Unresolved per RT)
- Loops over outcomes for issuance/burn checks (unrolled at codegen)
- Resolution dispatch: `match outcome_index { 0 => ..., N-1 => ... }`
- Redemption dispatch: winning-outcome selection for both YES_k and NO_j cases

**Audit & review**: generated `.simf` files are committed to the repo alongside the template so reviewers can verify the generator produces the expected structural generalization.

**Compilation caching**: each `.simf` file's compiled CMR is cached in `deadcat-core` build output. Per-N CMR is deterministic given a template version.

## OP_RETURN Recovery Hint

**Fixed portion** (independent of N, ~40 bytes, matching binary):
- `collateral_per_pair` (compressed mantissa + exponent: 2 bytes)
- `expiry_time` (per existing convention)
- `oracle_public_key` (32 bytes)
- `collateral_asset_id` (1 byte index into well-known set, or 32 bytes)
- `outcome_count` (1 byte)

**Variable portion**: the 4N asset IDs (2N tokens + 2N RTs) are derivable from the creation transaction's issuance entropy. Not stored in the hint.

Total hint size: ~40 bytes regardless of N.

See [chain-only-recovery.md](../../protocol/chain-only-recovery.md). Recovery flow: wallet scans for an asset ID that matches one of a market's `{yes,no}_token_asset_ids`, queries the issuance transaction, reads the OP_RETURN, reconstructs params, ingests the market.

## Relationship to the Binary Market

For N=2, the multi-outcome market is structurally very close to the binary market but not byte-for-byte identical:

- 2 YES tokens (YES_0, YES_1) + 2 NO tokens (NO_0, NO_1) = 4 token types. The binary market has 2 (YES, NO) because its single-outcome framing makes `YES = YES_0 = NO_1` and `NO = NO_0 = YES_1`. The multi-outcome contract still holds 4 distinct assets even when they'd be economically equivalent.
- `5N+2 = 12` slots vs. binary's 8.
- Oracle signs u8 outcome_index rather than a single outcome_byte.

**Migration question** (deferred):
- **(a) Keep `prediction_market.simf` canonical for N=2** and use `multi_outcome_market_nN.simf` for N ≥ 3. Binary stays battle-tested; the two-token-per-outcome redundancy is avoided at the common case.
- **(b) Regenerate N=2 from the template** and deprecate the binary contract. Single code-generation family, uniform core handling, at the cost of a heavier N=2 tx and the redundancy noted above.

This project leans toward option (a) — binary markets are the high-volume case and the existing contract is already deeply validated. This decision can be revisited after the generator ships and we measure real tx weights.

## Security Properties

| Property | Enforcement |
|---|---|
| Collateral conservation on per-outcome issue/cancel | Covenant checks `Δcollateral = sets × collateral_per_pair`, issuance on YES_i and NO_i RTs only |
| Collateral conservation on split/merge YES | Covenant checks `Δcollateral = sets × collateral_per_pair`, issuance across all YES RTs |
| Collateral conservation on split/merge NO | Covenant checks `Δcollateral = sets × (N-1) × collateral_per_pair`, issuance across all NO RTs |
| Outcome-independence of Q | Covenant only allows the enumerated operations, each of which preserves the invariant |
| Oracle-only resolution | BIP-340 signature verification against `oracle_public_key` |
| Correct redemption rate (resolved, winning YES_k) | Covenant releases `collateral_per_pair` per token |
| Correct redemption rate (resolved, winning NO_j, j ≠ k) | Covenant releases `collateral_per_pair` per token |
| Correct redemption rate (expired) | Covenant releases `collateral_per_pair / N` per YES token, `collateral_per_pair × (N-1) / N` per NO token |
| Deterministic RT blinding | Same scheme as binary market, applied to 2N RTs |
| RT destruction on terminal transitions | All 2N RTs burned on resolution and expiry |
| Collateral UTXO authenticity | Sibling UTXO check across all 2N+1 covenant inputs |
| No parasitic issuance | `ensure_no_issuance` on all non-issuance paths for all 2N+1 inputs |
| No double resolution | Resolution consumes all 2N RT UTXOs; no spend path back to Unresolved |

See [enforcement-layers.md](../../architecture/enforcement-layers.md) for the framework.

## Impact on deadcat-core

The existing `ContractEngine` API generalizes:

- `ingest_market` accepts `PredictionMarketParams` (binary) or `MultiOutcomeMarketParams` via a unified enum.
- `build_issue_pair_pset(market_id, outcome_index, sets)` — per-outcome issue.
- `build_cancel_pair_pset(market_id, outcome_index, sets)` — per-outcome cancel.
- `build_split_yes_pset(market_id, sets)` — cross-outcome YES split.
- `build_merge_yes_pset(market_id, sets)` — cross-outcome YES merge.
- `build_split_no_pset(market_id, sets)` — cross-outcome NO split.
- `build_merge_no_pset(market_id, sets)` — cross-outcome NO merge.
- `build_oracle_resolve_pset(market_id, outcome_index, signature)`.
- `build_redemption_pset(market_id, token_asset_id, amount)` — works for both YES_k (winning) and NO_j (winning) in Resolved_k, and for both YES_i and NO_i in Expired.
- `build_expire_transition_pset` — unchanged in shape.

The `Side` enum (`{ Yes, No }` in the binary contract) is preserved, now paired with `OutcomeIndex(u8)`:

```rust
pub struct OutcomeToken {
    pub outcome: OutcomeIndex,
    pub side: Side,  // Yes or No
}
```

Full `deadcat-core` API changes are out of scope for this doc — they'll be addressed in a subsequent pass over `../../architecture/deadcat-core-design.md`.

## Alternatives Considered

### N-token Arrow-Debreu variant

An earlier version of this spec used **N tokens** (one per outcome) instead of 2N. In that design:

- `outcome_i` pays 1 unit of collateral iff outcome i wins, 0 otherwise.
- Split: pay 1 collateral → receive 1 of each outcome token.
- Merge: burn 1 of each outcome token → receive 1 collateral.
- `NO_i` is implicit: holding one of every outcome token *except* i.

**Pros**:
- Fewer RTs (N instead of 2N). Smaller transactions on split/merge. Higher practical N ceiling (~15 vs ~10).
- Simpler expiry redemption rate (uniform 1/N per token).
- Conceptually closer to the financial literature (Arrow-Debreu securities).

**Cons (why we rejected it)**:
- Negative positions require (N-1) UTXOs. For N=5 that's 4 UTXOs per hedge; for N=10 that's 9. Dust, fees, and mental overhead scale badly.
- Mental-model discontinuity with the binary market. New users have to learn that "betting against X" means "buying everything else."
- AMM design asymmetry. A pool on a single outcome has a natural "long" side (the outcome token) but no natural "short" side — pools either serve only long positions or synthesize NO via an N-way bundle mechanism, both of which complicate the AMM.

The 2N design pays a 2× RT slot cost to make the YES/NO symmetry first-class. Given the binary market already trained users (and pool designs) on YES/NO thinking, this was judged worth the cost.

Reference: this alternative was the design specified by the pre-pivot version of this document and is reflected in the initial drafts of [design-journal-multi-outcome-amm.md](design-journal-multi-outcome-amm.md) and [amm-scoring-rule-tradeoffs.md](amm-scoring-rule-tradeoffs.md) — those docs need updating to reflect the 2N pivot.

### Binary-market composition via an event wrapper

Keep binary markets and introduce a new "event" contract that holds N binary markets' RTs and orchestrates cross-market operations atomically.

Rejected: infeasible without modifying the binary market to use witness-parameterized output indices (the current contract hardcodes positions 0/1/2). Even if we modified it, capital inefficiency is N:1 vs 1:1 for the native multi-outcome design.

### Application-only composition

Link N independent binary markets at the UI layer, with no new contracts. Coherency via oracle discipline and arbitrage.

Retained as an **option for very large N** (N > the 2N-contract ceiling) and for markets whose outcome set is not provably exhaustive (e.g., the 2024 US election where Biden dropped out). Documented in the design journal as the "soft-coherency" path.

## Pending Work

| Item | Purpose |
|---|---|
| Prototype the code generator | Produce generated `.simf` for N=3 and N=5. Verify all spend paths. |
| Compile prototype `.simf` files | Confirm SimplicityHL handles generated code. Measure program size and witness size per N. |
| Benchmark transaction weights | Measure actual vBytes for issue/cancel/split/merge/resolution at each N. Validate scaling. |
| Validate sibling UTXO check scaling | Confirm the 2N+1-way `prev_txid` check fits witness budget up to MAX_N. |
| Confirm MAX_N | Final decision on the upper N supported by a single multi-outcome contract (proposal: N=10). |
| Decide N=2 migration path | Keep `prediction_market.simf` as canonical N=2, or regenerate from template. |
| Write `.simf` template formally | Document template format, parameterization, generator algorithm. |
| Specify builder convention validation | Permitted N, outcome ordering, naming conventions. |
| Update `../contract-specification.md` | Add multi-outcome market as a new contract type. (Done — see that doc.) |
| Update `design-journal-multi-outcome-amm.md` | Reflect the 2N pivot; prior text assumes the N-token design. |
| Update `amm-scoring-rule-tradeoffs.md` | Re-evaluate scoring-rule analysis over the 2N token space. |
| Generate test vectors | Per-N vectors covering creation, each operation, resolution per outcome, redemption (winning YES and winning NO sides), expiry, and edge cases. |

## Key Files

- `docs/contracts/multi-outcome/multi-outcome-market-contract.md` — this document
- `docs/contracts/multi-outcome/design-journal-multi-outcome-amm.md` — design history (assumes N-token; pending update)
- `docs/contracts/multi-outcome/amm-scoring-rule-tradeoffs.md` — scoring-rule analysis (assumes N-token; pending update)
- `docs/contracts/contract-specification.md` — top-level contract index
- `docs/architecture/enforcement-layers.md` — security properties generalized to 2N
- `docs/protocol/chain-only-recovery.md` — recovery flow extends naturally
- `docs/protocol/deterministic-rt-blinding.md` — RT blinding applied per-token
- `docs/protocol/oracle-bip340-tagged-hash.md` — oracle attestation format extends to outcome_index
- `docs/architecture/transaction-composability-model.md` — witness-parameterized indices enable pool composition
- `docs/contracts/prediction-market/market-dormant-terminal-paths.md` — dormant terminal paths generalize to 2N RTs
- Future: `src-tauri/crates/deadcat-sdk/contract/multi_outcome_market_n{N}.simf` — generated contracts
- Future: `src-tauri/crates/deadcat-sdk/codegen/multi_outcome_market_template.simf` — generator input
