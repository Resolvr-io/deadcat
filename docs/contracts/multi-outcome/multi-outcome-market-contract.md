# Multi-Outcome Prediction Market Contract

**Status**: Proposal — design specification for a new core contract type. Not yet implemented. Supersedes an earlier N-token (Arrow-Debreu) variant (see [Alternatives Considered](#alternatives-considered)). The Unresolved-phase operations design was revised from an enumerated-primitives approach (6 specific spend paths) to a single generic solvency-preservation spend path — the generic path accepts any `(Δy, Δn, Δc)` that preserves the invariant, enabling atomic cross-outcome compositions like the cross-outcome swap.

**Related**: this contract implements every principle in [market-contract-principles.md](../market-contract-principles.md) — permissionless operation within the solvency invariant, narrow oracle authority, terminal paths from every non-terminal state, RT destruction on resolution/expiry, sibling UTXO check, witness-parameterized indices, deterministic RT blinding, and the rest. The principles doc is the canonical specification of those shared properties; this doc focuses on what is specific to the multi-outcome contract (2N token model, generic Unresolved-phase transition check, slot layout, code generation).

## Motivation

Many real-world markets require more than two outcomes: Fed rate decisions (raise/flat/lower/other), elections with multiple candidates, sports tournaments, awards with many nominees. Polymarket regularly runs multi-outcome events with 10-128 outcomes. The existing binary prediction market contract (`prediction_market.simf`) covers single-event YES/NO and can be composed into multi-outcome events at the application layer (one binary market per candidate/outcome), but that composition has structural limitations this contract addresses natively.

**The primary value proposition of this contract is operational efficiency for multi-outcome trading.** The covenant exposes a richer set of solvency-preserving operations than composed binary markets can provide, and those operations become the arbitrage paths, hedging paths, and liquidity-rotation paths that traders actually use:

- **Atomic cross-outcome arbitrage**: when AMM liquidity comes from composed per-outcome pools (see [Pool Composition](#pool-composition) below), cross-outcome price coherence (`Σ p_YES_k = 1`) is arb-enforced. The covenant-native `split-YES` and `merge-YES` primitives let arbitrageurs close coherence gaps in a **single atomic transaction**, keeping arb-enforced coherence tight. Composed binary markets would require multi-tx arb sequences.
- **Efficient LP/maker liquidity rotation**: a maker rotating from YES_raise exposure to YES_flat exposure can use the cross-outcome swap primitive (`YES_i + (N-2) × collateral ≡ {NO_j : j≠i}`) or split-YES + per-outcome pair-cancel atomically. Composed binary markets require multi-tx dances through each underlying market.
- **Efficient bootstrap**: a pool creator seeding N per-outcome pools can source all outcome tokens via a single split-YES / split-NO rather than N separate pair-issues.
- **Complex trading strategies**: basket positions, conditional structures, outcome-rotation trades — all lower-friction with native cross-outcome primitives.
- **Shared collateral and single market identity**: one collateral UTXO serving all outcomes; single `market_id` for oracle attestation, discovery, and indexing. Operational conveniences on top of the above.

### Secondary value: oracle containment

As a secondary benefit, the N-outcome contract **structurally contains oracle misbehavior**: even if the oracle signs multiple outcomes, at most one resolution can land on-chain (the first signature consumed by the covenant transitions the contract to a terminal Resolved_k state; the others have no valid spend path). With composed binary markets, an oracle that signs YES for multiple markets causes economic insolvency across the composition.

This is real but less important in practice than the operational efficiency argument. Market users have to trust their oracle either way; structural containment only limits the blast radius on rare failure modes (key compromise, operator error). The cross-outcome primitives, by contrast, deliver continuous operational value on every trade.

### Design properties preserved from the binary market

- **Solvency invariant**: pre-resolution, the contract holds exactly enough collateral to cover the maximum possible payout across all N outcomes.
- **Per-outcome YES/NO tokens**: each outcome has both a YES (pays on that outcome) and a NO (pays on every other outcome), matching the mental model of the binary market N-fold.
- **Permissionless operation**: anyone can mint outcome pairs, burn pairs for collateral, split collateral into a full complement of YES (or NO) tokens, or redeem winning tokens after resolution.
- **Oracle-only resolution**: a single BIP-340 signature attests which outcome won. Covenant enforces narrow oracle authority.
- **Composability**: tokens are standard Elements assets. Any pool design, maker order, or application can build on top.

## Pool Composition

**AMM liquidity for this contract is provided via Option C composition**: N independent binary LMSR pools per market, one per outcome's YES/NO pair. See [`amm-scoring-rule-tradeoffs.md`](amm-scoring-rule-tradeoffs.md) for the full analysis of why Option C was chosen over unified multi-outcome pool designs.

Briefly:
- **One pool contract type** — the binary LMSR pool — used for both binary markets and per-outcome pairs within N-outcome markets. Single Simplicity covenant to audit and maintain.
- **Parallelism**: trades on different outcomes hit different pool UTXOs.
- **Arb-enforced cross-outcome coherence** (`Σ p_YES_k = 1`), closed atomically via the market contract's cross-outcome primitives.
- **Per-outcome liquidity may fragment** (popular outcomes get deep pools, obscure ones stay thin) — accepted as the cost of Option C.

This contract's cross-outcome primitives are what make Option C's arb-enforced coherence tight in practice. The market contract and the pool layer are designed to work together: the market contract provides the solvency-preserving operations; the pool layer provides per-outcome price discovery; arbitrage closes the loop.

### Architectural orthogonality

The market contract layer and the pool layer are **independent design choices**:

- **Binary market contract** can be used alone (single YES/NO event) or composed (N binary markets per multi-outcome event, at the application layer, with oracle-discipline-enforced coherence).
- **N-outcome market contract** (this contract) provides structural market-level coherence and cross-outcome primitives. AMM liquidity comes from N binary LMSR pools composed per market.
- **Binary LMSR pool** doesn't know or care which market contract type underlies its tokens. It takes `(yes_asset, no_asset, collateral_asset)` and makes a market; those assets can come from either contract type.

Creators pick market contract type based on their event characteristics:

- **Known, exhaustive outcome set** (Fed rate decisions, sports brackets, Oscar categories) → N-outcome market contract. Gets atomic cross-outcome primitives, stronger oracle containment, single market identity.
- **Dynamic or non-exhaustive outcomes** (elections where candidates may drop out, open-ended questions with potential "other" cases) → composed binary markets. Trades off atomic primitives for flexibility in the outcome set.

## Design Rationale

**Why 2N tokens instead of N?**

A simpler variant — sometimes called the Arrow-Debreu approach — uses only N tokens, one per outcome. Holding `outcome_i` pays on outcome i and zero otherwise. A user who wants to bet *against* outcome i holds one of each other outcome's token (N-1 UTXOs).

This project rejected that variant for UX reasons:

1. **Negative positions require (N-1) UTXOs.** For anything but very small N, this is punishing: more UTXOs to manage, more dust, larger spend transactions, more mental overhead.
2. **Mental-model continuity with the binary market.** Users already understand YES/NO. Scaling that pattern to N-way is more intuitive than introducing "hold the complement of everything you want to bet against."
3. **AMM composition is cleaner.** Per-outcome binary LMSR pools (Option C) make markets in `YES_k` with `NO_k` as the counter-asset per pool — mirrors the binary pool design exactly. N-token would make pool design asymmetric (long positions single-token, short positions multi-token).
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

The contract requires `C ≥ max_k payout(k)`. The invariant tightens this to **outcome-independence** — i.e., `y_k + sum_{j≠k} n_j` is the same value `Q` for every outcome k. Therefore:

```
C = collateral_per_pair × Q
```

Equivalently (and more useful for delta-based covenant checks): `y_k − n_k = D` for some constant `D` across all k, and `C = collateral_per_pair × Q`. The two formulations are equivalent given positive supplies.

**Operationally**, the covenant exposes a single generic spend path for Unresolved-phase transitions: any transaction whose `(Δy, Δn, Δc)` preserves the invariant is accepted. The covenant derives the deltas from the transaction's issuance fields and burn outputs (no witness-declared state), then checks invariant preservation directly via two arithmetic conditions (see [Operations](#operations)). This makes any solvency-preserving combination of supply changes atomic in a single transaction, including compositions like cross-outcome swap that would otherwise require multi-tx sequences.

## Parameters

```rust
pub struct MultiOutcomeMarketParams {
    pub oracle_public_key: XOnlyPublicKey,       // BIP-340 Schnorr pubkey for oracle attestation
    pub collateral_asset_id: AssetId,            // L-BTC, USDt, or other Elements asset
    pub yes_token_asset_ids: [AssetId; N],       // YES_i asset IDs, derivable from creation tx
    pub no_token_asset_ids: [AssetId; N],        // NO_i asset IDs, derivable from creation tx
    pub yes_rt_asset_ids: [AssetId; N],          // YES_i reissuance tokens
    pub no_rt_asset_ids: [AssetId; N],           // NO_i reissuance tokens
    pub base_payout: u64,                        // primary denomination: the per-outcome YES-expiry payout
    pub expiry_time: u32,                        // block height deadline
    pub outcome_count: u8,                       // N — redundant with array length, included for discovery clarity
}
```

`4N` of the fields (the asset ID arrays) are derivable from the creation transaction's issuance entropy. The remaining 4 non-derivable fields (`oracle_public_key`, `collateral_asset_id`, `base_payout`, `expiry_time`) are stored in the OP_RETURN recovery hint. See [OP_RETURN Recovery Hint](#op_return-recovery-hint).

**Unit convention**: same as binary market — all amounts in the smallest indivisible unit of the respective asset.

### Denomination model

The primary covenant param is `base_payout` — the amount of collateral one YES token returns on expiry redemption. The derived quantity `cp := base_payout × N` is the total collateral backing one `(YES_i + NO_i)` pair. All issuance, cancellation, and redemption formulas in this document use `cp` for readability; implementations compute `cp = base_payout × N` inline (with `N` a file-level literal in each generated `.simf`).

This model is unified across the binary and multi-outcome market contracts: both parameterize on `base_payout` drawn from the same 1-2-5 table. Binary markets derive `cp = base_payout × 2`; multi-outcome markets derive `cp = base_payout × N`. See [market-contract-principles.md § 12. Correct redemption rates](../market-contract-principles.md#12-correct-redemption-rates).

**Why this model rather than `cp` as the primary param**: parameterizing on `cp` directly would require the covenant to enforce `cp mod N == 0` to prevent integer-division rounding losses on expiry redemption (losses that scale linearly with pairs issued). Parameterizing on `base_payout` makes the divisibility automatic by construction — `cp = base_payout × N` is trivially divisible by N — so no runtime assertion is needed and no denomination table entry is unreachable for any supported N. The 4-bit OP_RETURN encoding is unchanged; only the semantic of the indexed value shifts from "pair cost" to "per-outcome payout unit."

**Constraints**:
- **v1 supports `N ∈ {3, 4}`**. Each supported N uses its own generated `.simf` file. The binary market stays a separate hand-written contract; the multi-outcome template begins at 3 outcomes in v1. Expanding to additional N values later is non-breaking because each new N is a new contract artifact.
- `base_payout` drawn from the canonical 1-2-5 mantissa table. *(builder-enforced; recovery decodability — bucket 2 of the [self-enforcement classification](../market-contract-principles.md#covenant-self-enforcement).)*
- `expiry_time` rounded up to the next 60-block boundary. *(builder-enforced; recovery decodability.)*
- `collateral_asset_id` in the well-known set or exotic-escape-compatible. *(builder-enforced; recovery decodability.)*

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

The covenant exposes **a single generic spend path** for Unresolved-phase transitions, plus the terminal phase-change paths (resolution, expiry, redemption). The generic path accepts any permissionless transaction whose `(Δy, Δn, Δc)` preserves the solvency invariant.

### The generic solvency-preserving transition

On every Unresolved-phase transition, the covenant derives the per-outcome deltas from the transaction's observable fields:

- `Δy_k = (issuance amount on YES_k RT input) − (amount at YES_k burn output)` for each k
- `Δn_k = (issuance amount on NO_k RT input) − (amount at NO_k burn output)` for each k
- `Δc = (new collateral output amount) − (old collateral input amount)`

All of these are directly observable: RT issuance via Elements issuance fields, burn amounts via OP_RETURN outputs with specific asset IDs, collateral via explicit output values. No witness-declared state, no tapdata supply tracking.

The covenant then verifies exactly two invariant-preservation checks:

**Check 1 — uniform side shift:**
```
S := Δy_0 − Δn_0
for each k in 1..N:
    assert Δy_k − Δn_k == S
```
This preserves the invariant `y_k − n_k = D` (constant across k), which is equivalent to outcome-independence of `Q`.

**Check 2 — collateral matches ΔQ:**
```
SumDeltaN := Σ_k Δn_k
ΔQ := S + SumDeltaN
assert Δc == ΔQ × collateral_per_pair
```
This ties `C` to `Q` so that `C = collateral_per_pair × Q` remains exact post-transition.

Any `(Δy, Δn, Δc)` satisfying both checks is accepted. The covenant additionally enforces the orthogonal structural invariants: sibling UTXO check across all 2N+1 covenant inputs, deterministic RT blinding on continuation RT outputs, no parasitic issuance on non-issuance inputs. These are unchanged from the standard multi-input covenant design.

### Common operations as specific delta shapes

All the operations a user or pool builder might want are specific instances of the generic check. These names are **wallet-layer ergonomics** (PSET builders construct txs with these specific delta shapes) — from the covenant's perspective, every one of these is the same generic spend path:

| Operation | Δ shape | Passes check? |
|---|---|---|
| Issue pair outcome i | `Δy_i = Δn_i = sets`, all other 0, `Δc = sets·cp` | Check 1: S=0 uniformly ✓; Check 2: Δc = (0 + sets)·cp ✓ |
| Cancel pair outcome i | `Δy_i = Δn_i = −sets`, all other 0, `Δc = −sets·cp` | Symmetric ✓ |
| Split YES | `Δy_k = sets ∀k`, `Δn = 0`, `Δc = sets·cp` | S = sets uniformly ✓; Δc = (sets + 0)·cp ✓ |
| Merge YES | `Δy_k = −sets ∀k`, `Δn = 0`, `Δc = −sets·cp` | ✓ |
| Split NO | `Δn_k = sets ∀k`, `Δy = 0`, `Δc = sets·(N−1)·cp` | S = −sets uniformly ✓; Δc = (−sets + N·sets)·cp ✓ |
| Merge NO | Symmetric ✓ | ✓ |
| **Cross-outcome swap** (YES_i → {NO_j : j≠i}) | `Δy_i = −sets`, `Δn_j = sets ∀j≠i`, `Δc = sets·(N−2)·cp` | S = −sets uniformly (k=i: −sets − 0 = −sets; j≠i: 0 − sets = −sets) ✓; ΣΔn = (N−1)·sets; Δc = (−sets + (N−1)·sets)·cp = (N−2)·sets·cp ✓ |
| **Arbitrary combination** in one tx | Linear sum of above | ✓ if each component preserves invariant; compositions are also invariant-preserving |

Under this design, **cross-outcome swap is a single-transaction primitive use of the generic path**, not a multi-tx composition. Any wallet can construct a tx with the cross-outcome-swap delta shape and the covenant accepts it.

### What about cp, N, and numerical bounds?

- `cp := base_payout × N` — derived from the primary covenant param `base_payout` (committed at creation) and the file-level literal `N`. `cp` is the pair cost; `collateral_per_pair` is used as a synonym in formulas below.
- `N = outcome_count` — committed at creation, fixed for the market's lifetime. Determines the iteration range for Check 1.
- Supply deltas fit in i64 (signed, bounded by reasonable token supplies).
- `ΔQ × collateral_per_pair` may require u128 intermediate for large markets — Simplicity handles u128 arithmetic via jets.

### Why this design (over enumerated primitives)

**Atomicity**: any composition of solvency-preserving operations happens in one transaction. Cross-outcome swap is the canonical example, but the same flexibility applies to any future operation that preserves the invariant.

**Extensibility**: new operations don't require covenant changes. If a wallet or router wants to compose a novel sequence of deltas in one tx, the covenant accepts it as long as the invariant is preserved. No new spend paths to enumerate, test, and audit.

**Simpler covenant code**: one spend path (the generic invariant check) replaces six enumerated primitives. N−1 equality checks + 1 value check vs. N specific primitive-verification blocks.

**Cleaner audit**: the invariant-preservation proof is mathematical and universally quantified — prove that "if Check 1 and Check 2 pass, the post-state is invariant-preserving" once, and it covers every accepted transition. Compare to enumerated primitives, where each primitive needs its own "this operation preserves the invariant" proof.

**No state growth**: the delta-based check uses only tx-observable data. Tapdata does not need to track per-outcome supplies, matching the existing design's minimal state commitment.

## Spend Paths

| Transition | From slots | To slots | Authorization | Covenant enforces |
|---|---|---|---|---|
| **Generic solvency-preserving transition** (Unresolved ↔ Unresolved, Dormant → Unresolved, Unresolved → Dormant) | All 2N+1 Unresolved covenant UTXOs, or all 2N Dormant RTs if pre-state is Dormant | All 2N+1 Unresolved covenant UTXOs, or all 2N Dormant RTs if post-state reaches Q=0 | RT spend (all 2N RTs) + token burns (for negative deltas) | Sibling check across all covenant inputs; deterministic RT blinding on continuation RTs; `no_parasitic_issuance` on all inputs that aren't legitimately issuing; Check 1 (Δy_k − Δn_k uniform across k); Check 2 (Δc = (S + ΣΔn_k) × collateral_per_pair). Dormant pre-state treats all supplies as 0; Dormant post-state requires all supplies to reach 0. |
| Resolution (outcome k, from Unresolved) | All 2N Unresolved RTs, collateral | Resolved_k collateral | Oracle BIP-340 signature | Oracle signs tagged hash of market_id + outcome_index; all 2N RTs burned; collateral preserved at Resolved_k script |
| Redemption (resolved, winning YES_k) | Resolved_k | — | YES_k burn | YES_k tokens burned; collateral released at full value (1 token → `collateral_per_pair`) |
| Redemption (resolved, winning NO_j, j ≠ k) | Resolved_k | — | NO_j burn | NO_j tokens burned; collateral released at full value |
| Redemption (expired, YES_i) | Expired | — | YES_i burn | YES_i tokens burned; collateral released at yes_expiry_rate (see below) |
| Redemption (expired, NO_i) | Expired | — | NO_i burn | NO_i tokens burned; collateral released at no_expiry_rate (see below) |
| Expiry (from Unresolved) | All 2N Unresolved RTs, collateral | Expired | Timelock ≥ `expiry_time` | All 2N RTs burned; collateral preserved at Expired script |
| Dormant resolution (outcome k) | All 2N Dormant RTs | — | Oracle BIP-340 signature | All 2N RTs consumed, no covenant outputs |
| Dormant expiry | All 2N Dormant RTs | — | Timelock ≥ `expiry_time` | All 2N RTs consumed, no covenant outputs |

**Sibling UTXO check**: every transition in the Unresolved phase co-spends all 2N+1 covenant inputs and verifies they share the same `prev_txid`. This prevents collateral substitution attacks. See [enforcement-layers.md](../../architecture/enforcement-layers.md).

Per-outcome operations (issue pair, cancel pair) issue or burn only two RTs worth of tokens, but must still co-spend all 2N RTs in the witness to maintain the sibling invariant. The uninvolved RTs pass through unchanged (zero issuance, no burn).

## Oracle Attestation

Uses the shared tagged-hash protocol defined in [oracle-bip340-tagged-hash.md](../../protocol/oracle-bip340-tagged-hash.md). For the multi-outcome contract, the market-specific pieces are:

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

## Witness-Parameterized Input and Output Indices

The covenant accepts both `in_base` and `out_base` from the witness. It asserts that the current input sits at `in_base + slot_offset`, validates the full `2N + 1` covenant-input window rooted at `in_base`, and places continuation outputs at `out_base..out_base + 2N` (2N RT outputs + 1 collateral output). This gives the contract covenant-level flexibility for future multi-contract composition while preserving correctness through bounded-window checks plus explicit script/asset verification. See [transaction-composability-model.md](../../architecture/transaction-composability-model.md) for the general framework.

Aliasing defense: script uniqueness per slot + per-market script derivation means no output can alias another contract's output or another slot within this contract.

## Expiry Redemption Rate

If the oracle does not resolve by `expiry_time`, all outcome tokens become redeemable against the Expired collateral UTXO at a pre-computed rate. The rate treats every outcome as equally probable (1/N), which is the "no information" default and preserves solvency exactly.

Rates (expressed in terms of the primary denomination `base_payout`):
```
yes_expiry_rate = base_payout          = cp / N
no_expiry_rate  = base_payout × (N-1)  = cp × (N-1) / N
```

Both rates are exact integers by construction, because `cp = base_payout × N`. No division is performed at covenant runtime — the covenant's expiry spend path uses `base_payout` and `base_payout × (N-1)` directly.

**Solvency verification**: if all tokens redeem, total payout is:
```
sum(y_i) × base_payout + sum(n_i) × base_payout × (N-1)
  = base_payout × [Y + (N-1) × N_total]
```
where `Y = sum(y_i)` and `N_total = sum(n_i)`.

Using the outcome-independence constraint `y_k - n_k = D` (same constant D for all k), we have `Y = N_total + N × D`, so:
```
Y + (N-1) × N_total = N × N_total + N × D = N × (N_total + D)
```
Therefore total payout = `base_payout × N × (N_total + D) = cp × (N_total + D) = C` exactly. ✓

Binary case (N=2): `yes_expiry_rate = no_expiry_rate = base_payout = cp / 2` — matches the unified denomination model shared with the binary market.

**Exact redemption is structural, not asserted.** Because `base_payout` is the primary param and `cp = base_payout × N` is derived at covenant compile time, every expiry redemption rate is an integer multiple of `base_payout` — no rounding residuals can arise. The alternative (primary `cp` param + covenant `cp mod N == 0` assertion) would have required rejecting any creation with non-divisible `cp` and restricted the denomination table to N-compatible values. The primary-`base_payout` model avoids both complications.

## Code Generation Strategy

Each supported N has its own hand-committed `.simf` file, produced by a Rust-based generator that applies a MiniJinja template to a per-N context. The generator runs at dev time, not at build or runtime.

### Supported N range (v1)

**v1 supports N ∈ {3, 4}.** N=2 continues to use the existing `prediction_market.simf` (the binary market contract, which has been deeply reviewed and is refactored in place rather than regenerated from the multi-outcome template).

The {3, 4} range is a deliberately conservative v1 scope. Transaction size scales roughly quadratically with N (per-input witness grows with N, and the number of 2N+1 covenant inputs grows with N), and Liquid's block weight limit bounds the practical ceiling. We have not yet benchmarked the compiled binary (N=2) witness size against the generic-path multi-outcome contract, so we can't predict the exact cutoff. Starting at {3, 4} unlocks simple multi-outcome use cases without committing to larger N values whose tx weights we haven't measured.

**Expansion is non-breaking.** Adding support for N=5, N=6, etc. in a future release only introduces new per-N `.simf` files (and their CMRs). Existing N=3 and N=4 markets are unaffected because each N's covenant is its own CMR-committed program; a market created against one `.simf` file has no dependence on any other. **Shrinking the supported range is breaking** (would invalidate existing markets' spend paths if their N is removed) and should not be done once markets exist in the wild.

### Crate architecture

The generator is fully decoupled from the `deadcat-core` runtime surface:

- **`deadcat-codegen`** (new workspace crate, dev-only): pulls MiniJinja as a regular dep. Exposes a function (and a CLI binary for `just generate-simf`) that takes `N` and writes `multi_outcome_market_nN.simf` to the expected path inside `deadcat-core`'s contract directory.
- **`deadcat-core`**: reads the committed `.simf` files at compile time via `include_bytes!`. Has no dependency on `deadcat-codegen` or MiniJinja. Downstream consumers of `deadcat-core` receive pre-embedded `.simf` files bundled with the published crate and never see the generator's dependency tree.
- **Workspace CI**: `cargo test` at the workspace root runs `deadcat-codegen`'s drift-detection test, which regenerates each supported N's `.simf` source in-memory and asserts byte-exact equality against the committed files (including that no extra files exist in the target directory and no expected files are missing).

File layout:

```
crates/deadcat-codegen/
  src/
    lib.rs                                       # fn generate(n: usize) -> String
    templates/
      multi_outcome_market.simf.j2               # MiniJinja template
    bin/
      generate-simf.rs                           # CLI entry point (just generate-simf)
  tests/
    drift.rs                                     # in-memory regen + compile + byte-match test

crates/deadcat-core/
  contracts/
    prediction_market.simf                       # binary market (hand-maintained, separate contract family)
    multi_outcome/
      multi_outcome_market_n3.simf               # committed, generator output
      multi_outcome_market_n4.simf               # committed, generator output
```

### Template structure

The MiniJinja template parameterizes the SimplicityHL source on `N`. Parameterized sections include:

- 2N param declarations for YES/NO token asset IDs and their reissuance tokens
- 2N RT slot programs (Dormant + Unresolved per RT)
- Loops over outcomes for issuance/burn checks (unrolled at codegen time via `{% for k in range(n=N) %}`)
- Resolution dispatch: `match outcome_index { 0 => ..., N-1 => ... }` (unrolled)
- Redemption dispatch: winning-outcome selection for both YES_k and NO_j cases

Template syntax uses MiniJinja's Jinja2-compatible delimiters (`{{ N }}` for substitution, `{% for ... %}...{% endfor %}` for loops, `{% if ... %}...{% endif %}` for conditionals). These do not conflict with SimplicityHL syntax, which uses bare `{ }` and `[ ]` with adjacent tokens.

### Verification test

The drift-detection test in `deadcat-codegen` runs on every `cargo test` invocation and performs, for each supported N:

1. **Byte-match check**: regenerate the `.simf` source in-memory via the generator and assert it matches the committed file byte-for-byte.
2. **Directory consistency**: assert the committed contracts directory contains exactly the expected set of files (no drift, no stragglers, no missing entries).
3. **Compile check**: invoke the SimplicityHL compiler (as a Rust library — the same compiler `deadcat-core` uses at runtime) on the generated source with a fixed canonical test param set, asserting compilation succeeds. This catches template bugs that produce syntactically valid but semantically broken SimplicityHL.

The compile check uses fixed test params rather than per-market params because CMR depends on the full param set (see [CMR and params](#cmr-and-params) below) — a fixed canonical param set gives a reproducible compile but its CMR is not a deployment artifact.

Cost: the compile check runs the full SimplicityHL compilation pipeline per N, adding some time to `cargo test`. Acceptable at N=2 (N=3, N=4 in v1); worth watching as the range expands.

### CMR and params

CMR (Commitment Merkle Root) in Simplicity commits to the program's combinator tree, which includes compile-time constants. In deadcat's model, covenant params (oracle pubkey, asset IDs, `base_payout`, `expiry_time`, etc.) are inlined as constants during SimplicityHL compilation — so **every distinct param set produces a distinct CMR**. This is consistent with `deadcat-core-design.md`'s `fn contract_cmr(params, network) -> Cmr` signature and the cross-contract CMR-uniqueness discussion in `transaction-composability-model.md § Script Uniqueness Guarantee`.

Consequence for codegen: we do **not** cache per-N CMRs at build time. The only CMR that matters is computed at market creation (and stored in `ContractId.cmr`). What we commit and verify at codegen time is the `.simf` source text, not a compiled CMR.

**Audit-workflow TODO**: a reproducible recipe for "given the committed `.simf` at commit X and canonical test params Y, here's CMR Z" is useful for security-audit sign-off but is a tooling polish item, not a correctness requirement. Can ship alongside the audit pass rather than blocking v1.

## OP_RETURN Recovery Hint

**Fixed portion** (independent of N, 37 bytes total with well-known collateral, matching binary):
- `base_payout` (4-bit index into the 1-2-5 denomination table; `cp = base_payout × outcome_count` is derived at decode time)
- `expiry_time` (per existing convention)
- `oracle_public_key` (32 bytes)
- `collateral_asset_id` (1 byte index into well-known set, or 32 bytes)

`outcome_count` is **not stored** in the hint. Recovery derives it from the creation transaction's new-issuance count (`2N` issuances → `N` outcomes), keeping the market hint layout identical to the binary market's layout aside from the type-tag byte.

**Variable portion**: the 4N asset IDs (2N tokens + 2N RTs) are derivable from the creation transaction's issuance entropy. Not stored in the hint.

Total hint size: 37 bytes with well-known collateral, or 69 bytes with exotic collateral.

See [chain-only-recovery.md](../../protocol/chain-only-recovery.md). Recovery flow: wallet scans for an asset ID that matches one of a market's `{yes,no}_token_asset_ids`, queries the issuance transaction, reads the OP_RETURN, reconstructs params, ingests the market.

## Relationship to the Binary Market

For the hypothetical 2-outcome member of the multi-outcome family, the structure would be very close to the binary market but not byte-for-byte identical:

- 2 YES tokens (YES_0, YES_1) + 2 NO tokens (NO_0, NO_1) = 4 token types. The binary market has 2 (YES, NO) because its single-outcome framing makes `YES = YES_0 = NO_1` and `NO = NO_0 = YES_1`. The multi-outcome contract still holds 4 distinct assets even when they'd be economically equivalent.
- `5N+2 = 12` slots vs. binary's 8.
- Oracle signs u8 outcome_index rather than a single outcome_byte.

**Chosen for v1: `prediction_market.simf` stays the canonical binary market contract.** The hypothetical 2-outcome member of the multi-outcome family is not used in v1; the template serves markets with 3 or more outcomes only. Binary remains the high-volume case and the existing contract is already deeply validated; the two-token-per-outcome redundancy of forcing binary through the multi-outcome template would cost tx weight at the common case for no structural benefit. The decision can be revisited after the generator ships and we measure real tx weights, but the path of least risk is to keep the two contracts independent.

## Security Properties

| Property | Enforcement |
|---|---|
| Outcome-independence of Q (equivalent: `y_k − n_k = D` constant across k) | Generic Unresolved-phase spend path's Check 1: `Δy_k − Δn_k` uniform across k for every transition. Inductive from Dormant pre-state (all zero, invariant trivially holds). |
| Collateral matches Q | Generic Unresolved-phase spend path's Check 2: `Δc = (S + ΣΔn_k) × collateral_per_pair`. Inductive from Dormant pre-state (C=0, Q=0). |
| Oracle-only resolution | BIP-340 signature verification against `oracle_public_key` in the resolution spend path (separate from the generic Unresolved-phase path) |
| Correct redemption rate (resolved, winning YES_k) | Resolved_k slot's spend path releases `collateral_per_pair` per winning token burned |
| Correct redemption rate (resolved, winning NO_j, j ≠ k) | Same Resolved_k slot; covenant distinguishes winning YES_k burn from winning NO_j burn by asset ID |
| Correct redemption rate (expired) | Expired slot releases `base_payout` per YES token, `base_payout × (N-1)` per NO token. Both are exact integers by construction (primary param is `base_payout`; `cp = base_payout × N` is derived), so no rounding residuals arise and no covenant-level divisibility assertion is needed. |
| Deterministic RT blinding | Same scheme as binary market, applied to 2N RTs, on all continuation RT outputs |
| RT destruction on terminal transitions | All 2N RTs burned on resolution and expiry spend paths |
| Collateral UTXO authenticity | Sibling UTXO check across all 2N+1 covenant inputs on the generic Unresolved-phase path and on resolution/expiry paths |
| No parasitic issuance | `ensure_no_issuance` on inputs that the generic path's delta derivation doesn't account for (i.e., inputs other than RT issuance and collateral spend) |
| No double resolution | Resolution consumes all 2N RT UTXOs; no spend path from Resolved_k back to Unresolved exists |
| Invariant preservation across any composition of deltas | Linearity: if each component of a composed transition individually preserves Check 1 and Check 2, their sum does too. Formally proven once, covers every transaction that the generic path accepts. |

See [enforcement-layers.md](../../architecture/enforcement-layers.md) for the framework.

## Impact on deadcat-core

The `ContractEngine` API generalizes naturally to this covenant shape. Wallet-layer PSET builders expose named operations for ergonomics (`build_issuance_pset`, `build_split_yes_pset`, etc.), each constructing a tx with a specific `(Δy, Δn, Δc)` delta shape. All of these builders produce transactions that route through the same generic covenant spend path — the covenant doesn't see the builder name, only the tx's observable deltas.

See `../../architecture/deadcat-core-design.md` for the full `Market` and `MultiOutcomeMarket` view-type APIs (unified `build_issuance_pset`, cross-outcome-specific `build_split_yes_pset` / `build_merge_yes_pset` / `build_split_no_pset` / `build_merge_no_pset`). Cross-outcome arb quote/build is deferred to v2; see [deadcat-core-design.md § Future: Cross-Outcome Arb API (v2)](../../architecture/deadcat-core-design.md#future-cross-outcome-arb-api-v2). The builders are unchanged in shape by the generic-path design; what changes is the covenant's internal verification logic and the ability to compose novel delta shapes in a single transaction within the same generic solvency-preserving path.

The `Side` enum (`{ Yes, No }` in the binary contract) is preserved, now paired with `OutcomeIndex(u8)`:

```rust
pub struct OutcomeToken {
    pub outcome: OutcomeIndex,
    pub side: Side,  // Yes or No
}
```

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

Reference: this alternative was the design specified by the pre-pivot version of this document. [`amm-scoring-rule-tradeoffs.md`](amm-scoring-rule-tradeoffs.md) has been updated to reflect the 2N pivot and the Option C pool composition decision. [`design-journal-multi-outcome-amm.md`](design-journal-multi-outcome-amm.md) records the design history.

### Binary-market composition via an event wrapper

Keep binary markets and introduce a new "event" contract that holds N binary markets' RTs and orchestrates cross-market operations atomically.

Rejected: infeasible without modifying the binary market to use witness-parameterized output indices (the current contract hardcodes positions 0/1/2). Even if we modified it, capital inefficiency is N:1 vs 1:1 for the native multi-outcome design.

### Application-only composition

Link N independent binary markets at the UI layer, with no new contracts. Coherency via oracle discipline and arbitrage.

Retained as an **option for very large N** (N > the 2N-contract ceiling) and for markets whose outcome set is not provably exhaustive (e.g., the 2024 US election where Biden dropped out). Documented in the design journal as the "soft-coherency" path.

## Pending Work

| Item | Purpose |
|---|---|
| Prototype the code generator | Produce generated `.simf` for N=3 and N=4. Verify all spend paths. |
| Compile prototype `.simf` files | Confirm SimplicityHL handles generated code. Measure program size and witness size per N. |
| Benchmark transaction weights | Measure actual vBytes for issue/cancel/split/merge/resolution at each N. Validate scaling. |
| Validate sibling UTXO check scaling | Confirm the 2N+1-way `prev_txid` check fits witness budget for the v1 set `{3, 4}` and characterize headroom for future N expansion. |
| Write `.simf` template formally | Document template format, parameterization, generator algorithm. |
| Specify builder convention validation | Permitted N, outcome ordering, naming conventions. |
| Generate test vectors | Per-N vectors covering creation, each operation, resolution per outcome, redemption (winning YES and winning NO sides), expiry, and edge cases. |

## Key Files

- `docs/contracts/multi-outcome/multi-outcome-market-contract.md` — this document
- `docs/contracts/multi-outcome/amm-scoring-rule-tradeoffs.md` — scoring-rule analysis and the pool design decision (binary LMSR + Option C composition)
- `docs/contracts/multi-outcome/design-journal-multi-outcome-amm.md` — design history record
- `docs/contracts/contract-specification.md` — top-level contract index
- `docs/contracts/market-contract-principles.md` — covenant-enforced properties shared by both market contract types
- `docs/architecture/enforcement-layers.md` — security properties generalized to 2N
- `docs/protocol/chain-only-recovery.md` — recovery flow extends naturally
- `docs/protocol/deterministic-rt-blinding.md` — RT blinding applied per-token
- `docs/protocol/oracle-bip340-tagged-hash.md` — oracle attestation format extends to outcome_index
- `docs/architecture/transaction-composability-model.md` — witness-parameterized indices enable atomic multi-contract PSETs (including cross-outcome arb via pool co-spend with market's split-YES / merge-YES)
- `docs/contracts/prediction-market/market-dormant-terminal-paths.md` — dormant terminal paths generalize to 2N RTs
- Future: `src-tauri/crates/deadcat-core/contract/multi_outcome_market_n{N}.simf` — generated contracts
- Future: `src-tauri/crates/deadcat-core/codegen/multi_outcome_market_template.simf` — generator input
