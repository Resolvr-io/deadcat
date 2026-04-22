# Design Journal: Multi-Outcome Markets and AMM Exploration

**Status**: Historical design record. All open questions in this journal have been resolved. **See [Final Decisions](#final-decisions) for the committed landings.** The rest of the journal preserves the exploration trail for future reference.

## Final Decisions

After extended design iteration documented below (and further discussion in subsequent sessions), the following are the committed decisions:

### Multi-outcome market contract: 2N tokens (YES/NO per outcome)

The original Approach A in this journal proposed N tokens (one per outcome, Arrow-Debreu style). **This was subsequently pivoted to 2N tokens**: each outcome has both a YES token and a NO token, mirroring the binary market's token model. Reasons:

- **Negative positions don't require (N-1) UTXOs**: with NO_k as a first-class token, users can bet against outcome k by holding a single NO_k, not a bundle of N-1 "everything-else" tokens.
- **AMM symmetry**: per-outcome binary pools (Option C composition, see below) naturally take (YES_k, NO_k) pairs, mirroring the binary LMSR pool structure.
- **Mental-model continuity**: users already understand YES/NO from the binary market.

Trade-off accepted: 5N+2 covenant slots (vs 3N+2 for the N-token variant), 2N+1-way sibling check on unresolved-phase transitions.

See [`multi-outcome-market-contract.md`](multi-outcome-market-contract.md) for the full spec.

### Scoring rule: LMSR (not QMSR)

The journal's tentative lean was "QMSR-for-all." **This was reversed.** Final decision: binary LMSR.

Reasons the reversal happened:
- **Production track record**: LMSR has 20 years of real deployments. QMSR has zero. For a new platform, operational unknowns are expensive.
- **First-mover incentives**: LMSR's exponential cost curve creates dramatically stronger pressure to trade immediately on new information than QMSR's linear curve. For deadcat's AMM-as-price-oracle role, this matters.
- **LS-QMSR disqualified by trilemma**: analyzed in [`amm-scoring-rule-tradeoffs.md`](amm-scoring-rule-tradeoffs.md). The LS construction causes a fixed 50% shrinkage of displayed prices toward uniform, independent of α. Breaks the price-oracle role.

QMSR remains a viable fallback if LMSR's subsidy efficiency or witness size become real operational constraints.

### Pool shape: binary only (no unified multi-outcome pool)

The journal's "LMSR pool scaling investigation" noted that N=3 is feasible with a 2D table of the same entry count as the binary 1D table, and N=4 is borderline with 3D tables. **The final decision is to only support binary (N=2) LMSR pools**, even though N=3 unified is technically feasible.

Reasons:
- **Uniform implementation surface**: one pool contract type across all market shapes simplifies audit, indexing, routing, LP UX, and bug-fixing.
- **2D-variant cost isn't "free"**: same table entries, but different covenant logic, state encoding, tooling, audit overhead.
- **N=3 markets are a minority share of volume**: the ROI on a separate unified N=3 pool contract is weak.
- **N≥4 needs composition anyway**: collapsing to "binary everywhere + Option C for multi-outcome" is architecturally cleaner than "unified for N∈{2,3} + composed for N≥4."

### Multi-outcome pool liquidity: Option C composition

For a multi-outcome market with N ≥ 3 outcomes, AMM liquidity comes from **N independent binary LMSR pools**, one per outcome's YES/NO pair. Cross-outcome AMM coherence (`Σ p_YES_k = 1`) is **arb-enforced**, not structural.

The multi-outcome market contract's cross-outcome primitives (split-YES, merge-YES, cross-outcome swap) enable arbitrageurs to close coherence gaps in a **single atomic transaction**, keeping arb-enforced coherence tight. This is the primary operational value of the multi-outcome market contract, surpassing the secondary oracle-containment value initially emphasized.

### Liquidity model: admin-operated pools, permissionless creation

Each pool has a single operator who commits subsidy, earns fees, bears impermanent loss, and can adjust reserves / close via admin-signed spend paths. **Pool creation itself is permissionless** — anyone can deploy a pool on any market with any parameters, and multiple competing pools per market are expected.

LP-tokenized pools (shared ownership, deposit/withdraw mechanics, pro-rata fees) were considered in depth and deferred to v2.

### Contracts kept

- **Binary prediction market contract** — single-event YES/NO, also used as the building block for app-layer composed multi-outcome events where the outcome set can evolve.
- **Multi-outcome market contract** — 2N tokens, structural solvency, cross-outcome primitives for efficient arb and LP rotation.
- **Binary LMSR pool contract** — one pool type, serves both market contract types (directly for binary markets, composed via Option C for multi-outcome).
- **Maker order contract** — limit orders on any market.

### Alternatives considered and rejected

- **Unified multi-outcome LMSR at N=3** (2D table) — feasible but declined for implementation simplicity.
- **Unified multi-outcome QMSR** (any N via polynomial inline) — rejected for lack of production history and weaker first-mover incentives.
- **LS-LMSR** — dominated by standard LMSR at every N in Simplicity environment.
- **LS-QMSR** — disqualified by the properness trilemma (50% price bias).
- **FPMM / constant product** (Gnosis-style) — requires per-trade market co-spend, serializing all pool trades on the market's collateral UTXO. Rejected.
- **LP-tokenized pools in v1** — deferred to v2; admin-operated is simpler and the v1 permissionless-creation story is preserved.
- **Higher-degree polynomial scoring rules with LS construction** — genuinely novel research territory, deferred indefinitely.

---

**The remainder of this document preserves the original exploration trail.** Labels like "still under validation" and "tentative" refer to the state at the time of writing, not the current state.

---

## What we set out to answer

Starting question: the current deadcat contracts support binary (YES/NO) prediction markets only. How should we extend them to support multi-outcome events (e.g., "who will win the election" with many candidates), matching the capabilities Polymarket provides? Specifically:

- How does Polymarket compose binary markets into multi-outcome events?
- How can we provide liquidity for multi-outcome events without introducing structural arbitrage?
- How would any new design fit into deadcat's UTXO + Simplicity covenant architecture?

## Polymarket findings

- **Polymarket uses binary markets as the atomic primitive.** Every underlying market is YES/NO on the Gnosis Conditional Token Framework (CTF).
- Multi-outcome events are built by **composing multiple binary markets** with a contract called the **NegRiskAdapter**, which handles:
  - The split/merge mechanism (collateral ↔ set of outcome tokens)
  - The NO-token equivalence trick: 1 NO_A + 1 NO_B ≡ 1 USDC + 1 YES_C (in a 3-outcome event), which creates a structural arbitrage-prevention mechanism.
  - Enforcement that only one outcome can resolve YES.
- **AMM history**: LMSR → FPMM (constant product) → CLOB. Polymarket abandoned AMMs entirely at scale in favor of an off-chain order book. Their reasons were capital efficiency at extreme prices and professional market maker onboarding — not AMM correctness issues.
- **Scale**: Polymarket runs events with **up to 128 outcomes** (e.g., 2028 presidential primary markets). Hard technical limit is 256 (uint8 index in the NegRiskAdapter). Typical multi-outcome events are 10-100 outcomes.

## Three architectural approaches considered

### Approach A: Multi-outcome market contract (N outcomes as a primitive)

A new contract type that natively handles N outcomes:
- N outcome tokens, N reissuance tokens
- 3N + 2 covenant slots (generalizing the binary market's 8 slots)
- Split: collateral → 1 of each outcome token
- Merge: 1 of each outcome token → collateral
- Oracle attests which outcome won (u8 index)

**Pros**: Clean primitive. Split/merge enforced at covenant level. Structural arbitrage impossible by construction.

**Cons**: Each N requires a separate `.simf` (code generation). Transactions scale with N.

### Approach B: Event wrapper + N independent binary markets

Keep binary markets. Add a new "event" contract that holds N binary markets' RTs and orchestrates atomic cross-market operations.

**Verdict: still unattractive.** An earlier version of this analysis assumed the binary market used hardcoded output indices (`current_index() == 0/1/2`), which would have blocked co-spending multiple markets in one transaction. That premise is now superseded by the witness-parameterized market design. Even with that fix, Approach B remains worse than A because it adds wrapper complexity while still suffering capital inefficiency (N:1 capital lockup vs 1:1 for A).

### Approach C: Application-only composition

Link N independent binary markets at the UI layer. No new contracts.

**Verdict: weak structural guarantees.** Oracle discipline is the only thing preventing arbitrage. Can work as an MVP but isn't a long-term answer.

### Decision: Approach A is the winner

Approach A is strictly better than B on capital efficiency, transaction weight, and architectural cleanliness. It's strictly better than C on structural guarantees.

## Transaction weight analysis

Worked out per-PSET weight for each approach across N values. Key findings:

- Approach A split/merge/resolution scales as ~O(N) (all covenant I/O must be co-spent).
- For N=10: split ~17 kB, resolution ~12 kB — well within Liquid's limits.
- Approach A's "trade via split" atomic transaction (market covenant + N-1 pool swaps) is ~20 kB at N=10, ~27 kB at N=15.
- Per-outcome pool swaps (direct trades, not via split) are O(1) — constant regardless of N.

Approach A chosen as the backbone for multi-outcome markets.

## Hierarchical composition for large N

For N > ~15 (where single-contract transactions get uncomfortably heavy), we can **nest Approach A contracts**: a 10-outcome top-level market where each outcome is itself a 10-outcome sub-market.

- Within-tier arbitrage is covenant-enforced.
- Between-tier arbitrage is soft (like Approach C).
- Max single-tx weight stays bounded at the per-tier limit.

Covers Polymarket-scale events (128 outcomes) with a 2-tier structure.

## LMSR pool scaling investigation

The current binary LMSR pool uses a 65,536-entry Merkle-committed F-value table (1D, for binary state). Can we extend LMSR to multi-outcome markets?

- **N=3**: a 2D table works (e.g., 256×256 = 65,536 entries). On-chain verification is IDENTICAL to binary — two Merkle proofs, integer subtraction. Just different interpretation of leaf index.
- **N=4**: 3D table. At depth 24-28 (16M-268M entries), ~100-645 steps per dimension. Borderline.
- **N≥5**: dimensional explosion. At depth 32 (4B entries), N=5 gives only 256 steps per dimension; N=10 is impractical.

### Merkle cache depth vs storage

The pool operator generates the full table once at creation, but only needs to store an **intermediate hash cache** to generate proofs per swap. Deeper caches = faster proofs:

| Cache level | Cache size (D=32) | Leaves recomputed per proof | Proof gen time |
|---|---|---|---|
| 12 | 32 MB | 4,096 | ~4 ms |
| 16 | 2 MB | 65,536 | ~72 ms |
| 20 | 128 KB | 1M | ~1.2 sec |

Standardized parameter sets (pre-computing the roots + caches for common `max_loss_sats`/`half_payout_sats` combinations) can make pool creation instant.

### Per-swap fee overhead

Deeper Merkle trees add ~32 bytes per depth level per proof. Depth 32 vs depth 16 adds ~400 vB per swap. Negligible fee impact.

## AMM design space research

Sidebar: what alternative AMM designs could work for N>4?

**Constant product (CP)** with N+1 reserves (shared collateral):
- Each swap changes 2 reserves → O(1) pairwise constant product check (two 64-bit multiplications).
- Probabilities `p_i = (1/r_i) / Σ(1/r_j)` always sum to 1.
- Permissionless, simple verification.
- Unbounded impermanent loss for operator (vs LMSR's bounded loss).

**Curve StableSwap**: polynomial invariant, concentrates liquidity near balance. Verification requires big-integer arithmetic (~30 operations at N=10 with wide-int multiply). Capital-efficient but complex.

**Quadratic Market Scoring Rule (QMSR)**:
- Cost function: C(q) = (1/2b)·Σq_i² − (1/2bN)·S² + (1/N)·S
- Prices: p_i = q_i/b − S/(bN) + 1/N
- Prices sum to 1 by construction
- Bounded loss: b(N-1)/(2N)
- Only polynomial arithmetic, no transcendental functions
- **Never deployed in production despite 20+ years of academic advocacy**

## QMSR deep dive

### Why QMSR was never deployed

Research revealed QMSR's non-adoption was largely path-dependent, not evidence of fatal flaws:

1. Hanson published LMSR first (2002). His unique-locality theorem applies to combinatorial markets.
2. EVM environments have fixed-point exp/ln, making LMSR tractable.
3. Gnosis chose FPMM over LMSR for LP-crowdfunding reasons, not pricing-quality reasons.
4. Polymarket moved to CLOB for scale, not because LMSR or QMSR was broken.
5. **The environment where QMSR is uniquely valuable (integer-only arithmetic, no transcendentals) didn't exist in prior deployments.**

Abramowicz (2007) explicitly advocates for QMSR with a "uniform liquidity" argument. Nueve & Waggoner (NeurIPS 2025) builds on QMSR with a "smooth" variant — not a bug fix, just a tighter loss bound.

### The negative-price issue

QMSR's unconstrained price formula can produce p_i < 0 when one outcome's q_i is far below the mean. Two solutions:

- **Domain restriction**: enforce `S ≤ b` as a covenant invariant (mathematically proven sufficient).
- **Simplex projection**: standard DCFMM approach; handles out-of-domain states by clipping prices to the simplex boundary. Requires sort + threshold in the covenant (feasible but more complex).

The domain-restricted version is simpler and preferred for initial deployment.

### QMSR unification question (eventually rejected)

Initial exploration suggested QMSR might require **unifying market and pool** into a single contract. Reasoning: QMSR mints tokens on demand, requires RT ownership, and the market contract is the RT holder in the current architecture.

**Correction**: QMSR doesn't require mint-on-demand. It can work with pre-stocked token reserves (exactly like the current LMSR pool). The q vector just reinterprets as "tokens depleted from reserve" rather than "tokens minted."

This preserves the clean separation:
- **Market contract**: holds RTs, enforces token conservation, plain-English solvency invariants.
- **QMSR pool**: holds outcome token reserves + subsidy collateral, uses QMSR pricing.

Composability preserved. Market contract stays simple.

### Ownership model discussion

Three candidate models for the QMSR pool:

- **Model A (operator-owned)**: admin key, adjustable b, early close. Traditional.
- **Model B (bonded creator)**: creator locks subsidy, no admin controls, subsidy returned at resolution. Ownerless during operation.
- **Model C (tokenized liquidity)**: LP tokens. Maximum permissionlessness. Significant complexity.

For the **separable** QMSR pool (as opposed to the rejected unified version), Model A is appropriate — it mirrors the current LMSR pool's operator model. The market contract stays ownerless; the pool is operator-controlled. Clean separation of concerns.

## Operator-signed pool variant

For any N, an operator-signed pool provides a UX optimization:
- Operator co-signs every swap.
- Covenant verifies signature + basic conservation only (no pricing math).
- O(1) verification, any N.
- Requires trust: operator can censor swaps, could Sybil-attack price history by self-trading exclusively.

Best used as an **optional variant alongside covenant-verified pools**, not a replacement. Hybrid taproot tree:

```
Leaf 0: Operator-cosigned swap (fast path, needs operator online)
Leaf 1: Permissionless covenant swap (fallback, full on-chain verification)
Leaf 2: Admin adjust
Leaf 3: Close
Leaf 4: Timelock recovery
```

## The big question: QMSR-for-all vs hybrid

Extended dialectic:

**Initial enthusiasm for QMSR-for-all** (one pool type across all N):
- Single implementation, single auditing surface
- Works at any N without modification
- Sum-to-1 by construction
- Polynomial arithmetic (no Merkle tables)
- Better bounded-loss ratio than LMSR

**Steelman against QMSR-for-all**:
- Zero production history (20 years of academic availability, no deployments)
- LMSR's Bayesian/KL-divergence interpretation has theoretical value
- LMSR is more liquid at extremes (per-token price impact smaller) — which may actually be good for manipulation resistance
- Hard cap on outstanding tokens (S ≤ b) limits expressiveness at extreme probabilities
- Future combinatorial markets favor LMSR (Hanson's locality theorem)
- No liquidity-sensitive variant exists (LS-LMSR exists)

**Counter-steelman (defending QMSR-for-all)**:
- Non-adoption has identifiable causes that don't apply to UTXO + Simplicity environments
- Hard cap is a feature (explicit risk bound) — LMSR has the same practical cap via finite capital
- Bayesian interpretation is aesthetically pretty but never actually leveraged in production
- QMSR's uniform liquidity is defensible for information aggregation near resolution
- Simpler math = smaller attack surface, easier audit
- Hybrid design has real costs: two implementations, doubled surface area, arbitrary N boundary

**Current convergence** (tentative):
- QMSR-for-all is likely the better answer, but the case isn't airtight
- The hybrid approach is a local minimum — takes on QMSR risk without fully committing
- Either trust QMSR everywhere or don't use it at all
- The honest uncertainty is about how much weight to give the "20 years of non-deployment" signal

## Architectural decisions made

These are settled:

- **Multi-outcome market contract (Approach A)** is the primitive for N ≥ 3. Separate contract from binary, code-generated per N.
- **Witness-parameterized output indices** for the multi-outcome market (for composability with pools). Departs from the current binary market's hardcoded indices.
- **Market contract stays ownerless** with plain-English solvency invariants. Pool design choice is orthogonal.
- **Pool is a separate contract**, operator-owned (Model A). Any pricing model works.
- **Operator-signed pool** is an optional variant, not a replacement.
- **Hierarchical composition** for very high N (>15).
- **Rename throughout**: `pair` → `set`, `issuance` → `split`, `cancellation` → `merge`, `collateral_per_pair` → `collateral_per_set`.
- **Side enum generalizes to OutcomeIndex(u8)** with `YES` = 0, `NO` = 1 convenience constants.

These are still under validation:

- **QMSR as the primary pool design for N ≥ 3** (or some subset). The "for-all vs hybrid" question remains technically open but is tentatively resolved in favor of QMSR-for-all for simplicity.
- **Expiry redemption rate**: proposed `collateral_per_set / N` with divisibility constraint.
- **Code generation specifics**: template format, supported N range (tentatively N=3 to N=15), build process.
- **N=2 migration**: keep `prediction_market.simf` or regenerate from template?

## Documentation strategy

Rather than rewrite the existing `../../architecture/deadcat-core-design.md` (2900+ lines) and satellite docs, we agreed to:

1. **Write new satellite docs first** as design proposals.
2. **Preserve the existing docs** as-is until the new designs prove themselves in implementation.
3. **Defer the main doc rewrite** until we have implementation confidence.

### Planned satellite docs

- **Doc 1 — `multi-outcome-market-contract.md`** (DRAFTED). The stable, high-confidence part: the generalization of the binary market to N outcomes. Covers parameters, covenant structure, spend paths, oracle attestation, state machine, code generation strategy, OP_RETURN recovery, security properties, and the validation checklist.

- **Doc 2 — `qmsr-pool-proposal.md`** (NOT YET WRITTEN). The QMSR pool design as a proposal. Will cover cost function, domain restriction, contract architecture, integer arithmetic details, operator economics, comparison to alternatives, open questions.

### Blast-radius assessment for future main-doc rewrites

Rough estimate (from the conversation): ~40% of total doc content will need meaningful changes when we commit to the new designs. Split roughly as:
- ~15% terminology/naming updates (Side → OutcomeIndex, pair → set, etc.)
- ~15% structural generalization (types, state enums, API signatures)
- ~10% genuinely new content (code generation, 2-reserve pool, event composition, trade-via-split routing)

The existing architecture (UTXO-following state machine, store trait design, chain sync model, Simplicity encapsulation, LMSR math, coin selection, four-layer enforcement) all survives intact. What changes is the parameterization from 2 to N.

## Current state

- **`docs/contracts/multi-outcome/multi-outcome-market-contract.md` drafted** (Doc 1). Ready for review.
- **QMSR pool proposal (Doc 2) not yet written**.
- **Existing docs unchanged**.
- **No code written yet**. All at the design / specification level.

## Suggested next steps

In priority order:

1. **Review and iterate on Doc 1** (multi-outcome market contract). Confirm design decisions: witness-parameterized indices, expiry redemption rate, code generation strategy, N range, naming changes.

2. **Codegen validation spike**: prototype the `.simf` generator and produce an N=3 contract. Compile it. Measure sizes. Confirm the template approach works and the generated code is readable/auditable.

3. **Draft Doc 2** (QMSR pool proposal). Now that the foundation is in place, the QMSR pool design can be concretely specified.

4. **Small updates to existing docs**: forward-reference the new docs from `../contract-specification.md` and `../../architecture/deadcat-core-design.md`'s pending refactors tables.

5. **QMSR implementation spike**: write an actual `.simf` for the QMSR pool to validate the math works in integer arithmetic with acceptable witness sizes.

6. **Decision point**: based on implementation validation, commit to QMSR-for-all or hybrid. Plan the main-doc rewrite accordingly.

## Key insights to carry forward

1. **The current architecture's composability is valuable.** Market + pool + orders as separate contracts is clean. Don't unify unless forced to.

2. **QMSR doesn't require unification.** It can work as a separable pool with pre-stocked reserves, same architectural shape as the current LMSR pool.

3. **Approach A (multi-outcome market contract) is independent of pool choice.** It's needed regardless of whether we pick LMSR, QMSR, CP, or something else for the pool layer.

4. **Witness-parameterized indices are the right default** for new contracts that need to compose.

5. **Code generation per N is feasible and clean.** Template + per-N generated files committed to repo.

6. **The "20 years of non-deployment" signal for QMSR is real but weaker than it seems.** The environments where QMSR uniquely shines didn't exist before now.

7. **Hybrid designs that split "easy cases" from "hard cases" by N are usually local minima.** If a design is safe for the hard cases, it's safe for the easy ones too. Pick one and commit.

8. **"Ownerless market, operator-owned pool" is the right trust model.** The market contract is public infrastructure. The pool is a private service. Both are fine.

## References

### Internal docs
- `docs/contracts/multi-outcome/multi-outcome-market-contract.md` — Doc 1 (drafted)
- `docs/contracts/contract-specification.md` — current binary contracts
- `docs/architecture/deadcat-core-design.md` — current main design doc
- `docs/architecture/enforcement-layers.md` — security framework
- `docs/contracts/lmsr-pool/lmsr-pool-design.md` — current LMSR pool
- `docs/architecture/transaction-composability-model.md` — composability patterns
- `docs/protocol/chain-only-recovery.md` — recovery flows

### External references (from research agents)
- Hanson 2002/2003 — LMSR origination and combinatorial locality
- Chen & Pennock 2007 — utility framework for bounded-loss market makers
- Abernethy, Chen, Vaughan 2013 — convex optimization framework (DCFMM)
- Abramowicz 2007 — "The Hidden Beauty of the Quadratic Market Scoring Rule"
- Nueve & Waggoner 2025 (NeurIPS) — "Smooth Quadratic Prediction Markets"
- Othman et al. 2013 — "A Practical Liquidity-Sensitive Automated Market Maker"
- Gnosis CTF documentation — conditional tokens framework
- Polymarket NegRiskAdapter — neg-risk market composition
