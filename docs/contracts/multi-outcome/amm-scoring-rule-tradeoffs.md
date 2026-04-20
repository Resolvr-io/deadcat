# AMM Scoring Rule Trade-offs

**Status**: Decision record. This document records the scoring rule comparison that informed deadcat's pool design decision. **See [Decision Summary](#decision-summary) for the final choice.** The comparative analysis below is retained as reference for the reasoning.

## Decision Summary

After working through the full scoring rule design space under deadcat's constraints (Simplicity covenants, no transcendental jets, 2N-token multi-outcome market contract, no per-trade co-spend with the market contract), the committed decision is:

- **Scoring rule**: **LMSR** (Hanson 2003), with the existing 1D Merkle-committed F-value table.
- **Pool dimensionality**: **Binary only** (N=2). No unified multi-outcome pool contract — even the feasible N=3 2D-table variant is declined for implementation simplicity.
- **Multi-outcome market liquidity**: **Option C composition** — N independent binary LMSR pools per market, one per outcome's YES/NO pair. Cross-outcome coherence is arb-enforced (not structural at the AMM layer).
- **Liquidity model**: **Admin-operated** pools with permissionless creation. No LP-tokenized pool in v1. Each pool has a single operator who commits subsidy, earns fees, and bears impermanent loss; anyone can create a new pool with any params.

The reasoning behind each of these decisions is distributed across the comparative analysis below. Key landing points:

- **QMSR was seriously considered** (polynomial inline, better subsidy efficiency, any-N support) but ultimately rejected because (a) it has no production deployments in prediction markets, (b) its linear cost curve produces weaker first-mover incentives than LMSR's exponential curve — meaningful for the AMM-as-price-oracle use case, (c) LMSR's Merkle-table overhead at N=2 is a one-time implementation cost rather than an ongoing one.
- **LS-QMSR was disqualified** by the 50% price-display bias (see the trilemma section).
- **LS-LMSR was disqualified** as strictly dominated by standard LMSR at every N in deadcat's environment.
- **Unified multi-outcome LMSR at N=3 (2D table)** is technically feasible with the same total entries as binary 1D, but was declined: the 2D-variant covenant, tooling, and audit overhead don't pay back enough given N=3 is a small fraction of real markets and composed binary works via arb.
- **FPMM / constant-product** (Gnosis-style) structurally requires co-spending the market contract on every trade, serializing all pool trades across all pools of a market on the market's collateral UTXO. This was the blocker — all pool trades on a market would serialize on the market's collateral UTXO. LMSR/QMSR don't have this issue because their pricing state is independent of reserves.

The **architectural orthogonality** is worth naming explicitly: the market contract layer (binary vs N-outcome) and the pool layer (always binary LMSR, always composed via Option C for multi-outcome markets) are independent choices. An N-outcome market contract is still used when the creator wants its cross-outcome primitives (atomic split-YES, split-NO, cross-outcome swap) for efficient arb, LP rotation, and complex trading. The pool layer doesn't know or care which market contract type is underneath.

## Purpose

This document compares automated market maker scoring rules that were considered for deadcat's pool layer:

- **LMSR** — Logarithmic Market Scoring Rule (Hanson 2003) — **chosen**
- **QMSR** — Quadratic Market Scoring Rule (Brier-derived) — considered, rejected
- **LS-LMSR** — Liquidity-Sensitive LMSR (Othman et al. 2013) — considered, rejected
- **LS-QMSR** — Liquidity-Sensitive QMSR (proposed) — considered, disqualified by price bias
- **FPMM / constant product** — considered as alternative family, rejected for per-trade market co-spend requirement

The comparison covers conceptual properties (properness, depth profile, bounded loss) and practical constraints (Simplicity covenant feasibility, witness sizes, interaction with the limit order book and the market contract layer). The original scope covered both single-event binary YES/NO markets and N-outcome markets with YES/NO per outcome as direct pool targets; the landed decision uses binary pools for both and composes for multi-outcome.

> **LS-QMSR verification status.** The LS-QMSR construction was independently analyzed with the following results:
>
> - **Path independence**: **Confirmed.** Follows directly from degree-1 homogeneity of `C(q) = Σ q_k · p_k(q)`.
> - **Bounded loss**: **Confirmed** as an upper bound. The formula `α(N-1)/(4N) · S` is tight for α ≤ 2 and conservative for larger α. Verified numerically at multiple (N, α) pairs.
> - **Strict properness**: **Disproven.** The LS construction causes the displayed price to diverge from the trader's true belief: at the trader's optimum, `p_k = π_k/2 + 1/(2N)` — a fixed 50% shrinkage toward uniform, independent of α. This is a fundamental consequence of the adaptive-properness-path-independence trilemma (see dedicated section below) and was the disqualifying finding for LS-QMSR.

## Background

### Why this matters for deadcat

deadcat's prediction market contracts use covenant-enforced primitives running on Liquid via Simplicity. The pool layer, which provides automated liquidity alongside the existing maker-order limit order book (LOB), can in principle use any of several scoring rules. The choice has implications for:

- **Operator economics**: how much locked collateral is required as subsidy to provide a given depth of liquidity.
- **Liquid transaction fees**: nearly every trade incurs a Liquid network fee proportional to transaction weight. Scoring rules with larger per-swap witnesses (e.g., LMSR's Merkle proofs, LS-LMSR's higher-dimension tables) produce heavier transactions and higher per-trade fees than scoring rules with inline polynomial arithmetic (QMSR, LS-QMSR). This affects every trade that touches the pool, which is most trades unless the LOB is exceptionally thick at the requested price — see the LOB interaction section.
- **Trader experience**: how prices respond to trades, where slippage concentrates, whether the market maker is always willing to trade.
- **Price signal accuracy**: the AMM is the source of truth for both current market price and historical price data. Scoring rule properties (properness, first-mover incentives) directly affect how quickly and accurately prices reflect trader beliefs — which in turn determines the quality of the historical price record the market produces.
- **Implementation complexity**: what arithmetic operations the covenant must perform, what auxiliary data structures are required, how large per-swap witnesses become.
- **Composability**: how the pool interacts with the limit order book, with the market contract, and with N-outcome events.

### Assumed environment

All comparisons assume:

- Pools are implemented as Simplicity covenants on Liquid/Elements. This directly implies integer-only arithmetic with no native `exp`/`ln` jets — the covenant-language constraint determines which scoring rules can be evaluated inline vs. which require pre-computed auxiliary data structures.
- Subsidy collateral is any Elements asset (L-BTC, USDt, or other) committed by the pool operator at creation.
- A maker-order LOB exists as a separate contract; pools compose with it via co-spending rather than depending on it.

### No published deployments

It is worth flagging upfront:

- **QMSR has no published production deployments** in any prediction market system, despite ~20 years of academic availability. Its non-adoption appears largely path-dependent (see Hanson 2003, Chen & Pennock 2007, Abramowicz 2007 for academic treatment).
- **LS-QMSR has no published literature whatsoever.** The construction here appears to be novel as far as informal search has determined. The pieces (QMSR, LS-LMSR's homogeneity trick) are well-known; combining them appears not to have been written up.
- **LMSR is widely deployed** (Augur v1, Gnosis, various corporate prediction markets).
- **LS-LMSR has limited deployment** (Microsoft internal, Inkling Markets, Consensus Point) but is well-studied academically.

This deployment asymmetry is a genuine consideration when weighing theoretical advantages against operational risk.

---

## Mathematical Foundations

### LMSR

**Cost function** (N outcomes):

```
C(q) = b · ln(Σ exp(q_k / b))
```

**Prices** (softmax):

```
p_k = exp(q_k / b) / Σ exp(q_j / b)
```

**Binary special case** (N=2, with q₁ = q_YES, q₂ = q_NO):

```
p_YES = σ((q_YES - q_NO) / b) = 1 / (1 + exp(-(q_YES - q_NO)/b))
```

**Properties**:
- Prices always strictly in (0, 1); sum to 1 by softmax normalization.
- Bounded loss: `b · ln(N)` for N outcomes; `b · ln(2) ≈ 0.693b` for binary.
- Price impact: `∂p_k/∂q_k = p_k(1 - p_k) / b` — state-dependent, maximized at p = 0.5 (independent of N).
- Path independent (cost function is well-defined; prices are exact gradient of C).
- Translation invariant (shifting all q_k by a constant preserves prices).

### QMSR

**Cost function** (N outcomes):

```
C(q) = (1/2b) · Σ q_k² - (1/2bN) · S² + S/N
```

where `S = Σ q_k`. The cross-coupling term `-S²/(2bN)` is essential — without it, prices do not sum to 1.

**Prices** (linear):

```
p_k = q_k/b - S/(bN) + 1/N
```

**Binary special case** (N=2):

```
p_YES = (q_YES - q_NO) / (2b) + 1/2
```

**Properties**:
- Prices sum to 1 by construction (the cross-term ensures this).
- Prices in [0, 1] only when domain constraint holds; can go negative if violated.
- Bounded loss: `b(N-1)/(2N)` — bounded by `b/2` for all N.
- Price impact: `∂p_k/∂q_k = (N-1)/(bN)` — constant, independent of state.
- Path independent.
- Translation invariant.

**Domain constraint**: `q_k ≥ (S - b)/N` for all k (equivalently, `q_min ≥ (S - b)/N`). The simpler sufficient condition `S ≤ b` is often quoted but is strictly stronger than necessary. Balanced trading (where all q_k are roughly equal) can have S > b without any price violations.

### LS-LMSR

**Construction**: replace fixed `b` with `b(q) = α · S` where `S = Σ q_k` and α > 0 is a fixed sensitivity parameter.

**Cost function** (closed form via homogeneity):

```
C(q) = Σ q_k · p_k(q)
```

where p_k uses the adaptive `b(q)`. This is the inner product of the quantity vector with the current price vector.

**Prices**:

```
p_k = exp(q_k / (α·S)) / Σ exp(q_j / (α·S))
```

Still softmax, sum to 1, in (0, 1).

**Properties**:
- Prices homogeneous of degree 0 in q (scaling all q by t preserves prices).
- C homogeneous of degree 1.
- Path independent (proven via homogeneity by Othman et al. 2013).
- Bounded loss: `α · ln(N) · S` — proportional to total volume, not a fixed amount.
- Loss ratio (loss / volume) is `α · ln(N)`, fixed.
- Adaptive depth: thin when S is small (responsive), thick when S is large (stable).
- Marginal cost ≠ instantaneous price (the gradient of C is not exactly p; an extra term arises from b varying with q).

### LS-QMSR

**Construction**: same homogeneity trick as LS-LMSR, applied to QMSR. Set `b(q) = α · S`.

**Prices**:

```
p_k = q_k / (α·S) - 1/(α·N) + 1/N
```

**Sum check**:

```
Σ p_k = S/(α·S) - N/(α·N) + N/N = 1/α - 1/α + 1 = 1  ✓
```

Prices sum to 1 (the cross-term cancels the scaling, just as in standard QMSR).

**Homogeneity check**:

```
p_k(t·q) = t·q_k/(α·t·S) - 1/(α·N) + 1/N = q_k/(α·S) - 1/(α·N) + 1/N = p_k(q)  ✓
```

Prices are degree-0 homogeneous, so by the same construction as LS-LMSR:

**Cost function**:

```
C(q) = Σ q_k · p_k(q) = (Σ q_k²)/(α·S) + S(α-1)/(α·N)
```

**Properties** (verification status updated):
- Prices sum to 1, homogeneous of degree 0.
- Path independent (via degree-1 homogeneity of C). **Verified.**
- **No domain restriction for α ≥ 1**: the condition `q_k ≥ S(1-α)/N` is automatically satisfied when α ≥ 1.
- Bounded loss: `α(N-1)/(4N) · S` — proportional to volume. **Verified** as an upper bound; tight for α ≤ 2. See note below.
- Adaptive depth: state-dependent. Outcomes with high volume share have low price impact (thick); outcomes with low volume share have high price impact (thin).
- Marginal cost ≠ instantaneous price (same caveat as LS-LMSR). **This has material consequences for price accuracy — see the trilemma section below.**
- **Displayed prices do not reflect true belief**: at the trader's optimum, `p_k = π_k/2 + 1/(2N)` — a fixed 50% shrinkage toward uniform, independent of α. See the trilemma section for derivation and implications.

> **Bounded loss verification**: the bound `α(N-1)/(4N) · S` was confirmed via constrained optimization of `max_{r,k} [r_k - (Σr_j²)/α - (α-1)/(αN)]` subject to `Σr_k = 1, r_k ≥ 0`. The unconstrained optimum `r_1* = [1 + (N-1)α/2]/N` is feasible when α ≤ 2; for larger α, the boundary constraint binds and the true worst case is strictly less than the formula. Verified numerically at (N=2, α=1): loss/S = 1/8 ✓; (N=3, α=1): loss/S = 1/6 ✓; (N=5, α=2): loss/S = 2/5 ✓.

---

## Properness

**Strict properness**: a market scoring rule is strictly proper if a risk-neutral trader's unique expected-profit-maximizing strategy is to move prices exactly to their true belief.

### LMSR

**Unconditionally strictly proper** (fixed b). Expected profit for moving prices from π to r given true belief q:

```
E[profit] = b · [KL(q ‖ π) - KL(q ‖ r)]
```

KL divergence is non-negative and equals zero iff arguments are identical, so the expected payoff is uniquely maximized at r = q. No constraints ever bind because LMSR prices approach 0 and 1 asymptotically but never reach them.

The KL identity has a strong interpretive consequence: **the LMSR market price is a sufficient statistic for the market's aggregate belief**. The cost function literally is a KL geometry over the probability simplex, and traders trading honestly is equivalent to performing Bayesian updates on the market's posterior.

**LS-LMSR**: subject to the trilemma (`∇C ≠ p`), so the displayed price at the trader's optimum is not exactly the true belief. However, the bias is proportional to `1/α` and small for practical α values. See the trilemma section for details.

### QMSR

**Strictly proper in the unconstrained case** (fixed b). Expected profit for binary QMSR (moving from π to r):

```
E[profit] = 2b(r-π)(q-π) - b(r-π)²
```

Setting ∂/∂r = 0 yields r = q. Unique optimum at the true belief.

**Properness can be compromised when boundary constraints bind**. If the unconstrained optimum r = q lies outside the valid domain, the trader is forced to stop at the boundary. The reported price is then the closest representable belief to q, not q itself. The cross-coupling term means a trader pushing one outcome's price up automatically suppresses other prices through `-S/(bN)`, which reduces (but does not eliminate) how often the unconstrained optimum lies outside the valid domain.

### LS-QMSR

**Not strictly proper in the price-display sense.** Although no domain constraints bind for α ≥ 1, the LS construction introduces a separate properness failure: the trader's optimal strategy moves displayed prices to `p_k = π_k/2 + 1/(2N)`, not to the true belief π_k. This is a fixed 50% shrinkage toward uniform, independent of α.

The underlying scoring rule is still proper in the decision-theoretic sense — the marginal cost `∂C/∂q_k` equals the true belief at the optimum. But the displayed price (which the system reports as "the market says X%") is systematically biased. See the trilemma section for the full derivation and comparison with LS-LMSR.

### Sharpness of the optimum

Both fixed-b families (LMSR and QMSR) are strictly proper, but they differ in how sharply the optimum is identified — i.e., the curvature of the expected-profit landscape at r = q.

For LMSR: curvature ≈ `1 / [q(1-q)]` per unit b.
For QMSR (binary): curvature = `2b` (constant).

| True belief q | LMSR curvature (per b) | QMSR curvature (per b) |
|---|---|---|
| 0.5 | 4 | 2 |
| 0.1 | 11 | 2 |
| 0.01 | 101 | 2 |

LMSR has a sharply identified optimum at extreme beliefs; QMSR's optimum is well-defined but the landscape is flatter. For practical traders facing budget constraints or risk aversion, sharper optima translate to more pressure to converge precisely on the true belief; flatter optima allow "good enough" reporting near the truth.

### First-mover incentives

LMSR's exponential cost curve creates strong first-mover incentives: the cost of moving prices grows steeply, so the second trader to act on the same information faces dramatically higher costs.

QMSR's linear cost curve creates weaker first-mover incentives: the second trader pays only proportionally more.

Whether stronger first-mover incentives are unambiguously good is debatable — they speed price discovery but can produce overshooting and may concentrate gains with the fastest traders rather than the most informed.

---

## The Adaptive-Properness-Path-Independence Trilemma

Any market scoring rule based on a cost function `C(q)` faces a three-way tradeoff among:

1. **Adaptive liquidity** — the depth parameter `b` adjusts automatically with market volume (`b = α·S`)
2. **Price accuracy** — the displayed price equals the trader's true belief at the optimum (`∇C = p`)
3. **Path independence** — the cost of a trade depends only on start and end states (`C(q') - C(q)`)

Standard scoring rules (LMSR, QMSR with fixed `b`) achieve (2) and (3): the cost function gradient IS the price vector, and C is well-defined. LS variants (LS-LMSR, LS-QMSR) achieve (1) and (3): the cost function is degree-1 homogeneous (path independent), and b adapts to volume. **Achieving all three is provably impossible** for QMSR-type prices: when `b` depends on the market state via any function `b(q)` with `∂b/∂q_k ≠ 0`, the price field has non-zero curl (`∂p_k/∂q_j ≠ ∂p_j/∂q_k` for `q_k ≠ q_j`), so no cost function `C` with `∇C = p` exists. The same argument applies to LMSR-type prices under any state-dependent `b`.

### Why ∇C ≠ p in LS variants

When `b = α·S` depends on the market state, differentiating the cost function `C(q) = Σ q_k · p_k(q)` produces extra terms:

```
∂C/∂q_k = p_k + Σ_j q_j · ∂p_j/∂q_k
```

The second term is non-zero because changing `q_k` changes `S`, which changes `b`, which changes all prices. For fixed-`b` scoring rules, `∂b/∂q_k = 0` and the extra terms vanish, giving `∇C = p`.

A risk-neutral trader maximizes `E[profit] = Σ π_k q'_k - C(q')`, yielding the first-order condition `∂C/∂q'_k = π_k`. This means the trader moves the state to where the **marginal cost** (not the displayed price) equals their true belief. The information is encoded in `∇C`, not in `p`.

### LS-QMSR: constant 50% shrinkage

For LS-QMSR, the trader's first-order condition can be solved in closed form. At the optimum:

```
optimal share:  r_k = α·π_k/2 + (2-α)/(2N)
displayed price: p_k = π_k/2 + 1/(2N)
```

The displayed price is a **50% shrinkage toward uniform**, independent of α. A trader with true belief π = 0.9 in a binary market moves the displayed price to 0.7, not 0.9.

**Derivation sketch**: the FOC `∂C/∂q_k = π_k` gives `2r_k/α - (Σr_j²)/α + (α-1)/(αN) = π_k`. Summing over k yields the constraint `Σr_j² = 1/N` at the optimum. Substituting back and computing `p_k = r_k/α - 1/(αN) + 1/N` produces `p_k = π_k/2 + 1/(2N)`.

The root cause is QMSR's **linear** price formula. The optimal `r_k` scales linearly with α (as `α·π_k/2`), causing α to cancel perfectly in `p_k = r_k/α + ...`, leaving a constant bias. This cancellation is specific to the linear structure.

Two additional structural consequences:

- **Prices stay biased regardless of volume.** Because prices are degree-0 homogeneous (they depend only on the ratios `r_k = q_k/S`, not on the scale `S`), additional trading at the optimal ratios adds volume without changing prices. A second trader with the same belief can profitably scale up at the same ratios — the profit function is degree-1 homogeneous, so expected profit scales linearly with trade size — but prices remain at `π/2 + 1/(2N)` throughout. The bias is a property of the ratio equilibrium, not a transient that washes out with volume.
- **Unbounded individual profit.** The degree-1 homogeneity of the profit function means a trader with non-uniform beliefs can achieve unbounded expected profit by scaling up their position. In practice this is bounded by capital, but it contrasts with standard MSRs where strict convexity of `C` ensures a unique finite optimum. The market maker's loss ratio remains bounded (the `α(N-1)/(4N)` bound holds regardless), but the absolute loss scales with volume.

### LS-LMSR: small, α-dependent shrinkage

For LS-LMSR, the same structural issue exists (`∇C ≠ p`), but the exponential (softmax) price formula means the optimal `r_k` scales **sub-linearly** with α. The α does not cancel from the price formula, and the bias shrinks as α grows. For practical α values, the displayed price is very close to the true belief.

This is the fundamental advantage of nonlinear price formulas under the LS construction: the more curvature the price function has, the smaller and more α-dependent the bias becomes.

The trilemma is mathematically strict — `∇C ≠ p` for any state-dependent `b` — but LS-LMSR makes it operationally soft. For practical α values, the price bias is negligible, giving LS-LMSR most of the benefits of all three properties simultaneously. The trilemma bites hard for LS-QMSR (constant 50% bias) but barely for LS-LMSR. This distinction is what makes the choice of base scoring rule matter so much: the exponential nonlinearity of LMSR nearly overcomes the trilemma, while the linearity of QMSR does not. It also suggests a research direction: a scoring rule with **intermediate** nonlinearity (e.g., quadratic prices from a cubic scoring rule) should produce intermediate bias under the LS construction — potentially small enough to be practical, while remaining polynomial and Simplicity-compatible. See open question 2.

### Implications

The trilemma has direct consequences for scoring rule selection:

| Scoring rule | Adaptive b | ∇C = p | Path independent | Price bias at optimum |
|---|---|---|---|---|
| LMSR (fixed b) | No | Yes | Yes | None |
| QMSR (fixed b) | No | Yes | Yes | None (unconstrained domain) |
| LS-LMSR | Yes | No | Yes | Small, ∝ 1/α |
| LS-QMSR | Yes | No | Yes | 50% shrinkage toward uniform |

For a system where the AMM's displayed price serves as the source of truth (as in deadcat), price accuracy is a first-order concern. The LS-QMSR's constant 50% bias means the price oracle systematically underreports the market's true belief — a market that "really believes" 90% displays 70%.

---

## Subsidy Efficiency / Bounded Loss

The operator's worst-case loss bounds how much subsidy capital must be locked at pool creation. For LS variants, the loss scales with volume; for fixed-b variants, it's a fixed amount.

| N | LMSR loss | QMSR loss | LS-LMSR loss/S | LS-QMSR loss/S (verified) |
|---|---|---|---|---|
| 2 | 0.693b | 0.250b | 0.693α | 0.125α |
| 3 | 1.099b | 0.333b | 1.099α | 0.167α |
| 5 | 1.609b | 0.400b | 1.609α | 0.200α |
| 10 | 2.303b | 0.450b | 2.303α | 0.225α |
| 20 | 2.996b | 0.475b | 2.996α | 0.238α |
| 100 | 4.605b | 0.495b | 4.605α | 0.248α |

**Comparison ratios** (efficiency advantage of QMSR-family over LMSR-family at equal subsidy):

| N | QMSR vs LMSR | LS-QMSR vs LS-LMSR (verified) |
|---|---|---|
| 2 | 2.77× | 5.54× |
| 3 | 3.30× | 6.59× |
| 5 | 4.02× | 8.05× |
| 10 | 5.12× | 10.24× |
| 100 | 9.30× | 18.61× |

For the same subsidy budget, QMSR-family scoring rules support meaningfully larger `b` (or smaller loss-per-unit-volume), enabling deeper liquidity per unit of locked collateral.

---

## Depth Profile

"Depth" at price p is the cost (per unit of probability shift) to move the price by an infinitesimal amount.

### LMSR / LS-LMSR

State-dependent: `∂p/∂q = p(1-p)/b`. Maximum impact (thinnest depth) at p = 0.5, independent of N. Minimum impact (thickest depth) at extremes (p → 0 or p → 1).

For binary, depth at price p is `b/(1-p)` for moving the price up; `b/p` for moving down.

**Depth profile shape**: thick at extremes (p → 0 or p → 1), thin at p = 0.5 (independent of N).

### QMSR

Constant price impact: `∂p/∂q = (N-1)/(bN)`, independent of state. A unit of trading moves the price by the same amount regardless of the current price level.

However, the cost per unit of probability shift (depth as defined above) is not constant: for binary, depth is `2bp` (moving up) or `2b(1-p)` (moving down). Directional depth varies linearly with p; the bidirectional average `b·[p + (1-p)] = b` is constant.

**Depth profile shape**: constant price impact; directional depth varies linearly with p (cheap to push prices toward 0 or 1, expensive to push away).

### LS-QMSR

State-dependent: `∂p_k/∂q_k = (1/α) · (1 - q_k/S) / S`.

Outcomes with high volume share (q_k/S near 1) have low price impact (thick depth). Outcomes with low volume share (q_k/S near 0) have high price impact (thin depth).

**Depth profile shape**: thick where volume is concentrated, thin where volume is sparse. Adapts to the market's actual structure.

### Crossover analysis (binary, equal subsidy)

For equal subsidy budget L, comparing LMSR depth to QMSR depth:

- QMSR is deeper than LMSR for **p ∈ (0.236, 0.764)** — the middle range
- LMSR is deeper than QMSR for **p < 0.236 or p > 0.764** — the tails

| Price | QMSR/LMSR depth ratio (equal subsidy) | Thicker side |
|---|---|---|
| 0.05 | 0.26 | LMSR (~3.8×) |
| 0.10 | 0.50 | LMSR (~2×) |
| 0.236 | 1.00 | Crossover |
| 0.30 | 1.17 | QMSR (~1.2×) |
| 0.50 | 1.39 | QMSR (~1.4×) |
| 0.764 | 1.00 | Crossover |
| 0.90 | 0.50 | LMSR (~2×) |
| 0.99 | 0.055 | LMSR (~18×) |

---

## Domain Constraints

### What "domain restriction" means

A scoring rule has a **domain restriction** when its price formula only produces valid prices (non-negative, summing to 1) for a subset of possible state vectors `q`. Outside that subset — the "domain" of valid states — the formula produces prices that violate basic axioms (e.g., negative probabilities).

Because an AMM must always expose valid prices to traders, a scoring rule with a domain restriction requires the covenant to **enforce the constraint on every trade**: reject trades that would push the state outside the valid domain. This has two consequences:

1. **Breaks the "always willing to trade" property**: the market maker refuses certain trades — specifically, trades that would push prices into the invalid region. A trader attempting such a trade sees their transaction fail.
2. **Creates a hard edge at the boundary**: as the state approaches the domain boundary, the pool still quotes prices (valid ones, from inside the domain), but the direction of trade that would cross the boundary becomes infeasible. The pool's responsiveness is asymmetric near the edge.

LMSR and LS-LMSR have no domain restriction — their softmax price formula is strictly positive for any `q`, so the market maker is unconditionally willing to trade. QMSR's linear price formula can produce negative prices when the state is skewed, requiring a domain constraint. LS-QMSR inherits QMSR's structure but the constraint disappears for α ≥ 1 because the adaptive `b` grows with volume fast enough to keep the formula valid.

The severity of a domain restriction in practice depends on how often the constraint actually binds. Mitigations (rebasing, pair-split, operator top-up) can substantially reduce the frequency of binding without eliminating the theoretical constraint.

### Summary table

| Scoring rule | Domain restriction | Mitigation |
|---|---|---|
| LMSR | None — prices always in (0, 1) | N/A |
| QMSR | `q_k ≥ (S-b)/N` for all k (strict); `S ≤ b` (sufficient simpler condition) | Rebasing reduces S without changing prices; pair-split provides liquidity at extremes; admin can increase b |
| LS-LMSR | None | N/A |
| LS-QMSR | None for α ≥ 1; `q_k ≥ S(1-α)/N` for α < 1 | Choose α ≥ 1 |

### Rebasing for QMSR

Subtracting a constant c from every q_k (with the operator simultaneously injecting c units of each outcome's tokens via primitive B basket-split) preserves all prices while reducing S by N·c. This converts the simple `S ≤ b` constraint into the exact `q_min ≥ (S-b)/N` constraint, and for balanced markets effectively removes the volume-cap concern.

The constraint binds only on **skew** (the spread between the most-traded and least-traded outcomes), not on total volume.

### Pair-split mitigation

In a YES/NO N-outcome market, the market contract's pair-split primitive (1 collateral → 1 YES_k + 1 NO_k) provides liquidity at the price extremes. When the QMSR pool approaches its domain boundary (price near 1 for the dominant outcome), traders can use pair-split to acquire YES_k at price ≈ 1 and sell NO_k at price ≈ 0 elsewhere. This routes around the pool's cap at near-zero economic cost.

---

## Implementation in Simplicity

Simplicity provides integer arithmetic but no transcendental functions (no `exp`, no `ln`). This significantly affects scoring rule feasibility.

### LMSR

Cannot be computed inline. The standard workaround is a **pre-computed Merkle-committed lookup table** mapping discretized state to F-values (cost-function values).

**For binary (N=2)**: state is 1-dimensional. A 1D table of ~65,536 entries suffices at depth 16. Pool creator generates the table at creation, commits to its Merkle root in the pool covenant, maintains an intermediate hash cache for proof generation. Each swap requires two Merkle proofs (~1 kB witness data) plus integer subtraction.

**For N ≥ 3**: the state is (N-1)-dimensional. A naive uniform lookup table would have `M^(N-1)` entries where M is per-axis discretization. At N=3 with M=256, this is 65,536 entries (still feasible — a 2D table). At N=4 with M=256, it's 16M entries (borderline — workable but uncomfortable on table generation cost and witness/proof size). At N=5+, the dimensional explosion makes the table approach impractical.

### QMSR

Computable inline using polynomial integer arithmetic. Pool state is an N-element q vector, committed via hash in the pool UTXO. Each swap:

1. Read the current q vector from witness, verify hash matches commitment
2. Compute new q vector based on trade
3. Compute `ΔC = C(q') - C(q)` using integer multiplications and divisions
4. Verify domain constraint (one comparison or simple inequality)
5. Hash and commit new q vector

Per-swap witness: ~8N bytes for the q vector plus a few hundred bytes overhead. No Merkle proofs required.

### LS-LMSR

Has the same exp/ln requirement as LMSR, but with an additional problem: **the Merkle table approach does not work because b changes with every trade**. Each table entry is a function of b, and when b varies, the entire table is invalidated.

Workarounds:
- **Inline exp/ln approximation**: not feasible without transcendental jets in Simplicity.
- **Table indexed by (q_diff_1, …, q_diff_{N-1}, S)**: adds one dimension vs. standard LMSR. Feasible for binary at 256×256 = 65k entries (2D). Borderline at N=3 (3D, ~16M entries). Impractical for N ≥ 4.
- **Off-chain computation with on-chain verification**: requires additional cryptographic infrastructure not currently part of the deadcat architecture.

LS-LMSR is **one dimensional step less practical than standard LMSR** at every N. Binary is feasible but requires a larger table; N=3 is borderline where standard LMSR is still comfortable; N ≥ 4 is impractical.

### LS-QMSR

Computable inline using polynomial integer arithmetic, same as standard QMSR. The only addition is the division by S (which becomes an inline computation per swap). Floor handling required when S is small (early trades): use `b = α · max(S, S_min)`.

Witness sizes and per-swap costs are essentially identical to standard QMSR.

### Summary

Each additional outcome adds a dimension to the state space. LS-LMSR effectively adds one more dimension on top of LMSR (the table must also index over varying S, since `b = α·S` changes per trade). The dimensional step from N to N+1 is the same for both LMSR and LS-LMSR; LS-LMSR is just offset by one dimension.

| Scoring rule | Binary (N=2) | N=3 | N=4 | N=5+ |
|---|---|---|---|---|
| LMSR | Feasible (1D Merkle table) | Feasible (2D Merkle table) | Borderline (3D table, ~16M entries) | Impractical |
| QMSR | Feasible (inline polynomial) | Feasible | Feasible | Feasible |
| LS-LMSR | Feasible (2D table) | Borderline (3D table, ~16M entries) | Impractical | Impractical |
| LS-QMSR | Feasible (inline polynomial) | Feasible | Feasible | Feasible |

---

## Interaction with the Limit Order Book

deadcat has a maker-order LOB as a separate covenant. The AMM pool and the LOB serve complementary roles; neither is strictly "primary" and neither is strictly "backup." Most trades are **single atomic transactions** that co-spend liquidity from both sources: a router (see `../../architecture/trade-routing-algorithm.md`) minimizes total cost — fill cost plus Liquid transaction fees — by selecting an optimal mix of resting maker orders and pool fill within one transaction. The pool and the LOB orders are complementary inputs to the routing optimization, not sequential venues.

### Complementary roles

**AMM (pool) roles:**
- **Always-available liquidity**: the AMM provides a quote at any price, at any time, for any trade size. Traders can always trade.
- **Current price source of truth**: the AMM's implied price is the authoritative current market price at any moment — the number displayed as "the market says X%".
- **Historical price source of truth**: the sequence of AMM states over time forms the canonical price history for the market. This is itself valuable data: prediction markets are often consulted for their historical pricing trajectory, not just their terminal outcome.
- **Price anchor for arbitrage**: LOB makers and takers use the AMM price as a reference point for posting and taking orders.

**LOB roles:**
- **Zero-subsidy liquidity from active makers**: professional market makers post orders at prices of their choosing without the operator needing to commit additional collateral.
- **Tighter spreads when makers are active**: makers can quote inside the AMM's implied spread when they have information, inventory, or capital to deploy.
- **Passive strategy support**: limit orders allow traders to specify fill conditions rather than taking immediate AMM prices.

### Why first-mover incentives still matter

Because the AMM is the source of truth for both current and historical prices, the AMM's price accuracy directly determines the quality of the prediction market as an information aggregation mechanism. For the AMM's price to track the market's true belief, informed traders must be incentivized to move the AMM's state promptly when they have new information.

This means the scoring rule's first-mover incentive structure matters **regardless of whether a LOB exists**:

- **Stronger first-mover incentive** (LMSR-family, exponential cost curve): the first trader to act on new information gets dramatically cheaper AMM prices; subsequent traders pay much more. Strong pressure to trade immediately, producing rapid price discovery and an accurate historical price record.
- **Weaker first-mover incentive** (QMSR-family, linear cost curve): the second trader pays proportionally more, not exponentially more. Price discovery is still directed toward truth (both families are strictly proper in the unconstrained case), but with less urgency. Historical prices may lag true belief more than under LMSR.

Both families still provide first-mover incentives — a trader always pays more to move a later state than an earlier one — but LMSR's are stronger by the ratio of exponential to linear cost growth.

The LOB does not eliminate this concern because informed traders who sweep the LOB and move the AMM are precisely the traders whose first-mover incentives we care about. If they delay, the AMM's price (and the historical record) lags the information they possess.

### Routing complexity

The router solves a joint optimization: given a requested trade size, it selects which LOB orders to take and how much to fill against which pool(s), minimizing total cost (fill price + cumulative transaction fee). This requires computing, for each candidate source, an effective price curve that the greedy can compare across sources. The mathematical form of the pool's marginal price curve directly affects how expensive this optimization is to perform.

- **LMSR**: the AMM's marginal price curve is logarithmic. Finding the crossover point between the pool's marginal price and a resting maker order's fixed price requires solving a transcendental equation. The current deadcat design uses binary search over the pool's state index to find the crossover — feasible but computationally involved per candidate source.
- **QMSR**: the AMM's marginal price curve is linear. The crossover with a resting maker order reduces to a linear equation — solvable in closed form.
- **LS-LMSR**: logarithmic marginal price curve with varying scale (b changes per trade). Even more complex than LMSR because the curve shape itself depends on cumulative volume.
- **LS-QMSR**: rational marginal price curve (polynomial numerator and denominator from the `q_k/(αS)` structure). More complex than QMSR but tractable without transcendentals.

### Per-scoring-rule summary

- **LMSR + LOB**: strong first-mover incentives (exponential cost), thick extremes (resists tail moves), transcendental routing. Best when accurate tail-probability pricing is critical and implementation overhead is acceptable.
- **QMSR + LOB**: moderate first-mover incentives (linear cost), uniform depth, linear routing. Better subsidy efficiency. Best when the active trading range dominates and operator collateral matters.
- **LS-LMSR + LOB**: adaptive overall scale on top of LMSR's shape. The LOB provides some organic adaptive liquidity (makers enter when volume justifies it), so LS-LMSR's adaptive-b advantage is partially redundant. Still provides stronger first-mover incentives than QMSR-family options.
- **LS-QMSR + LOB**: adaptive per-outcome depth, no domain constraint (α ≥ 1), best subsidy efficiency. The adaptive depth is orthogonal to the LOB's organic scaling — it provides per-outcome depth tuning that the LOB alone doesn't replicate (since makers concentrate on popular outcomes). **However**, the 50% price-display bias (see trilemma section) significantly undermines the AMM's role as source of truth for current and historical prices.

### Adaptive liquidity without the LS construction

The LS construction's appeal is automatic depth scaling — thin markets are thin, thick markets are thick, without operator intervention. The trilemma shows this comes at the cost of price accuracy. Several alternative mechanisms achieve adaptive liquidity while preserving `∇C = p` (strict properness):

- **LOB-provided organic depth**: the LOB already provides adaptive liquidity — makers enter when volume and spreads justify it. This scales naturally with market interest without touching the scoring rule. The AMM provides always-available baseline liquidity; the LOB thickens it when demand warrants.
- **Operator-managed b adjustments**: the pool admin mechanism (already part of the LMSR pool design) allows the operator to increase b as the market matures. Exogenous adaptation with no properness cost. The operator observes volume growth and adjusts; the LOB provides interim depth during any lag.
- **Time-based b growth**: `b(t) = b_0 + f(t)` where b increases with market age. Since b depends on time rather than market state, `∇C = p` is fully preserved — strict properness, path independence, polynomial arithmetic. Depth grows automatically without operator intervention. The tradeoff: depth increases even for markets nobody trades, locking up more subsidy capital. Manageable with a modest growth rate and a cap.

These mechanisms are less elegant than the LS construction but avoid the trilemma entirely. For deadcat's architecture, the combination of standard QMSR + active LOB + operator b-adjustment appears to achieve the practical goal of adaptive liquidity without sacrificing price accuracy.

---

## Use Case 1: Single-Event Binary YES/NO Markets

A single binary market has one YES token, one NO token, and resolves to one of two outcomes. This is the simplest and most common deadcat market type.

### LMSR

- **Implementable**: 1D Merkle table is the established design
- **Subsidy**: bounded loss `b · ln(2) ≈ 0.693b`
- **Depth profile**: thick at p ≈ 0 and p ≈ 1, thin at p ≈ 0.5
- **Properness**: unconditional
- **Domain**: no constraint
- **First-mover incentive**: strong (exponential cost curve)
- **Per-swap witness**: ~3-5 kB (Merkle proofs)
- **Notes**: requires table generation tooling, intermediate hash cache for proof generation

### QMSR

- **Implementable**: inline polynomial arithmetic, native to Simplicity
- **Subsidy**: bounded loss `b/4 = 0.25b` (~2.77× more efficient than LMSR for same depth)
- **Depth profile**: uniform; thicker than LMSR in (0.236, 0.764), thinner outside
- **Properness**: conditional (boundary `|q_YES - q_NO| ≤ b` may bind)
- **Domain**: hard cap on net skew; mitigated by rebasing
- **First-mover incentive**: moderate (linear cost curve)
- **Per-swap witness**: ~2-3 kB
- **Notes**: no table required, smaller witness, but requires boundary check

### LS-LMSR

- **Implementable**: 2D Merkle table (because b changes per trade) — feasible for binary but larger/more complex than standard LMSR's 1D table
- **Subsidy**: loss/volume = 0.693α (proportional to volume rather than fixed)
- **Depth profile**: same logarithmic shape as LMSR; overall scale adapts to volume
- **Properness**: subject to small α-dependent bias (see trilemma section); near-exact for practical α values
- **Domain**: no constraint
- **First-mover incentive**: strong
- **Per-swap witness**: larger than LMSR (table dimension is doubled)
- **Notes**: solves the b-guessing problem but at significant implementation cost (one dimension step beyond standard LMSR at every N)

### LS-QMSR

- **Implementable**: inline polynomial arithmetic, native to Simplicity
- **Subsidy**: loss/volume = α/8 (binary) (verified upper bound)
- **Depth profile**: state-dependent; adaptive thickness
- **Properness**: **biased** — displayed price = `π/2 + 1/4` (binary), a fixed 50% shrinkage toward uniform (see trilemma section)
- **Domain**: no constraint for α ≥ 1
- **First-mover incentive**: moderate (similar to QMSR)
- **Per-swap witness**: ~2-3 kB (similar to QMSR)
- **Notes**: the 50% price-display bias is a significant concern for the AMM's role as source of truth; see trilemma section and adaptive liquidity alternatives

---

## Use Case 2: N-Outcome Markets (Multi-Outcome)

In deadcat's multi-outcome market structure, each of N mutually exclusive outcomes has both a YES and a NO token (2N tokens total). The market contract provides permissionless solvency-preserving primitives: per-outcome pair issue/cancel, split-YES / merge-YES (full basket ↔ collateral_per_pair), split-NO / merge-NO (full NO basket ↔ (N-1)·collateral_per_pair), and cross-outcome swap (YES_i ↔ {NO_j : j≠i} + (N-2) collateral, derivable). See [`multi-outcome-market-contract.md`](multi-outcome-market-contract.md) for the contract design.

**The landed pool-layer decision for multi-outcome markets is Option C**: N independent binary LMSR pools per market, one per outcome's YES/NO pair. Cross-outcome AMM coherence is arb-enforced, with arbitrageurs exploiting the N-outcome market contract's native cross-outcome primitives to close coherence gaps atomically. The analysis below covers why this was chosen over unified multi-outcome pool designs.

### Option C (chosen): N binary LMSR pools per market

- **Pool type**: the same binary LMSR pool contract used for binary markets, instantiated N times per multi-outcome market.
- **Coherence**: `Σp_YES_k = 1` across pools is **arb-enforced**, not structural. Arb paths leverage the market contract's split-YES / merge-YES primitives (single atomic transaction when the N-outcome market contract is used; multi-tx sequence if the market is instead composed from N binary market contracts).
- **Subsidy scaling**: `N × b · ln(2)` across all pools (each pool independent). Higher per-outcome depth than unified LMSR for the same total subsidy because each pool's constraint is independent of the others.
- **Parallelism**: trades on different outcomes hit different pool UTXOs, so they parallelize. Only cross-outcome arb touches multiple pools atomically.
- **LP model**: admin-operated per pool. Creators pick which outcomes to provide liquidity for; a market may have some outcomes with deep pools and others with thin pools or no pool at all. Permissionless creation.
- **Implementation**: same `.simf` as binary LMSR pool. No multi-outcome-specific pool contract type.

### Why not unified LMSR (even at N=3 where feasible)?

LMSR unified pool is technically feasible for N=2 (1D table) and N=3 (2D table at same total entry count). The N=3 option was declined:

- **2D-variant tooling cost**: table generation, covenant lookup, state encoding, Merkle tree structure all differ from 1D in non-trivial ways. Realistic effort ratio ~1.3–1.5× binary, plus separate audit.
- **Small share of real markets**: N=3 prediction markets are a small fraction of total volume; binary dominates heavily.
- **Uniform implementation surface is valuable**: one pool contract type across all market shapes makes audit, indexing, routing, LP UX, and bug-fixing simpler.
- **N≥4 requires Option C anyway**: 3D table at ~16M entries is borderline; N≥5 is impractical. So unified pools would only ever cover N∈{2,3}, leaving N≥4 in Option C. Collapsing to "binary LMSR everywhere + Option C for multi-outcome" is architecturally cleaner.

### Why not unified QMSR (any N)?

QMSR's polynomial-inline feasibility at any N was attractive. Ultimately rejected because:

- **Zero production deployments** in prediction markets. 20 years of academic availability with no shipped system is a real (if path-dependent) operational risk signal.
- **Weaker first-mover incentives**: linear cost curve vs LMSR's exponential. For deadcat's AMM-as-price-oracle use case, faster information incorporation matters.
- **Domain constraint** (`Σ Δ_k ≤ b`) is benign in terms of semantics — at the boundary, pool's p hits 1 for some outcome, which is exactly where the market contract's pair-split takes over at equivalent cost — but it's still covenant complexity without clear offsetting value when LMSR+Option C covers the same use cases.

QMSR remains a viable fallback if future operational experience surfaces LMSR issues. The comparative analysis in this document supports either direction; the choice reduces to risk tolerance.

### LS-LMSR (rejected)

Shares LMSR's scaling problems plus the per-trade table invalidation issue (one dimension step more than standard LMSR at every N). Borderline at N=3 (3D table, ~16M entries), effectively impractical for N ≥ 4 in a Simplicity covenant. Under Option C composition, LMSR's already-feasible 1D binary table does the job without LS's overhead; LS-LMSR's adaptive-b advantage is largely subsumed by admin-bump + LOB organic depth.

### LS-QMSR (disqualified)

Shares QMSR's scaling properties (any N feasible) plus adaptive-depth advantages. Subsidy bound `α(N-1)/(4N) · S` (verified upper bound, tight for α ≤ 2).

For α ≥ 1, the domain constraint disappears entirely. **However**, the 50% price-display bias (see trilemma section) applies at every N, with displayed prices shrinking to `p_k = π_k/2 + 1/(2N)`. At N=10, the displayable range is only [0.05, 0.55]; a market with 100% consensus on one outcome displays at 55%. **Disqualified as a source-of-truth price oracle.**

### FPMM / constant-product (rejected due to per-trade co-spend)

The Gnosis-style FPMM is a natural alternative AMM family: `Π x_k = k` preserved on trades, polynomial inline, structurally Σp=1, no domain constraint, real production deployment history at Gnosis CTF.

The disqualifying issue is structural: FPMM's invariant only holds over **post-split** reserves. Every trade requires atomically splitting collateral through the market contract's split-YES primitive to preserve `Π x_k = k`. In deadcat's setup, this means every pool trade co-spends the market contract's collateral UTXO, serializing all pool trades across all pools of the same market on the market's collateral UTXO.

LMSR/QMSR don't have this problem: their pricing state is the q vector (tracked in tapdata), independent of physical reserves. Trades update q without touching uninvolved outcomes' reserves, and no market co-spend is required per trade.

This was the blocker for FPMM in deadcat. Not a property defect of FPMM — just a structural incompatibility with parallel pool trading.

---

## Trade-off Tables

### Single-Event Binary Markets

| Dimension | LMSR | QMSR | LS-LMSR | LS-QMSR |
|---|---|---|---|---|
| Properness | Unconditional | Conditional (boundary) | Small bias ∝ 1/α (trilemma) | **50% bias toward uniform** (trilemma) |
| Bounded loss | `b·ln(2) ≈ 0.693b` | `b/4 = 0.25b` | `α·ln(2)·S` | `α·S/8` (verified) |
| Subsidy efficiency vs LMSR | 1× | 2.77× better | Same as LMSR (per state) | 5.54× better than LS-LMSR |
| Depth at extremes (p=0.05) | Very thick | Thin | Very thick | State-dependent |
| Depth at center (p=0.5) | Thin | Uniform; thicker than LMSR for equal subsidy | Thin | Adaptive |
| First-mover incentive | Strong | Moderate | Strong | Moderate |
| Domain constraint | None | `\|q_Y - q_N\| ≤ b` | None | None for α≥1 |
| Simplicity feasibility | Feasible (1D Merkle table) | Feasible (inline polynomial) | Feasible (2D table) | Feasible (inline polynomial) |
| Per-swap witness size | ~3-5 kB | ~2-3 kB | Larger than LMSR | ~2-3 kB |
| Adaptive liquidity | No | No | Yes (per total volume) | Yes (per outcome) |
| Production deployment history | Extensive | None | Limited | None; no published literature |
| KL-divergence interpretation | Yes | No | Yes | No |

### N-Outcome YES/NO Markets

| Dimension | LMSR (unified) | LMSR (composed binary) | QMSR | LS-QMSR |
|---|---|---|---|---|
| Implementable in Simplicity | Up to N=3, borderline N=4 | Any N | Any N | Any N |
| Σp = 1 across YES tokens | Structural | Arbitrage-enforced | Structural | Structural |
| Bounded loss | `b·ln(N)` | `N · b · ln(2)` | `b(N-1)/(2N)` | `α(N-1)/(4N) · S` (verified) |
| Subsidy scaling with N | Logarithmic | Linear | Sub-linear | Sub-linear |
| Subsidy efficiency at N=10 | Baseline | ~3× worse than unified LMSR | ~5× better than unified LMSR | ~10× better than LS-LMSR |
| Per-outcome operator | No | Yes (independent pools) | No | No |
| Per-swap witness size | Grows with N (Merkle proof depth) | O(1) per pool | ~8N bytes | ~8N bytes |
| Domain constraint | None | None | Skew-bounded | None for α≥1 |
| Properness | Unconditional | Unconditional per pool | Conditional (boundary) | **50% bias toward uniform** (trilemma) |
| Deployment history | Limited (N=2 mostly) | Limited | None | None |

---

## Open Questions

### Resolved

1. ~~**LS-QMSR formal verification**~~: **Resolved.** Path independence confirmed (degree-1 homogeneity). Bounded loss confirmed as upper bound (tight for α ≤ 2). Strict properness **disproven** — displayed prices exhibit a fixed 50% shrinkage toward uniform. See the trilemma section and the LS-QMSR verification banner at the top of this document.

2. ~~**LS-QMSR loss analysis at high N**~~: **Resolved.** The bound `α(N-1)/(4N) · S` is confirmed as correct for all N. It is tight for α ≤ 2. For α > 2, the true worst case is strictly less. The bound is conservative (not violated) at any (α, N) combination.

3. ~~**Scoring rule choice**~~: **Resolved.** Binary LMSR chosen. Multi-outcome via Option C composition. See Decision Record.

4. ~~**Multi-outcome pool shape**~~: **Resolved.** Option C — N binary LMSR pools composed per market. No unified multi-outcome pool contract.

5. ~~**Liquidity model for v1**~~: **Resolved.** Admin-operated with permissionless creation. LP-tokenized pools deferred to v2.

### Open / deferred

6. ~~**Covenant implementation specifics for binary LMSR pool**~~: **Resolved.** Witness encoding, F-value generation (arbitrary-precision bignum), and Merkle proof format are specified in [`lmsr-pool/lmsr-deterministic-table-spec.md`](lmsr-pool/lmsr-deterministic-table-spec.md). Fixed-point Taylor is deferred to a post-v1 non-breaking optimization (committed Merkle roots are the conformance set).

7. **Operator economics in production**: theoretical subsidy efficiency is one factor; actual operator P&L depends on trader behavior, fee structure, adverse selection, and LOB-vs-pool routing dynamics. LMSR has production history in non-Liquid environments but not in deadcat's specific setup. Real-world data will inform future parameter defaults.

8. **Cross-outcome arb coherence in practice**: under Option C composition, `Σp_YES_k = 1` is arb-enforced. Empirical question: how tight do spreads stay in practice? Does it matter whether the underlying market is a single N-outcome contract (atomic arb via split-YES/merge-YES) vs. composed binary markets (multi-tx arb)? This will only be answered by real deployment data.

9. **LP-tokenized pools as v2**: if passive-capital demand materializes, design a tokenized-liquidity variant. Scale-invariance properties favor QMSR for LP-tokenization (LMSR's Merkle table doesn't play cleanly with dynamic b). Revisits the scoring rule choice.

10. **Higher-degree polynomial scoring rules with LS construction**: deferred indefinitely. A cubic scoring rule with quadratic prices could give LS-like adaptive liquidity with polynomial arithmetic and small price bias. Research territory; no urgency.

11. **Smooth QMSR variants**: Nueve & Waggoner (NeurIPS 2025) propose a smoothed QMSR with a log-barrier term. Polynomial-only smoothing? Deferred — only relevant if the project revisits QMSR.

12. **Multi-pool composition on the same market**: a single market may have multiple competing pools (possibly with different b, fee_bps). Routing optimization across them is a `deadcat-core` concern. Whether to build explicit router tooling for it in v1 is TBD.

---

## Decision Record

The committed pool design is **binary LMSR only, composed via Option C for multi-outcome markets, admin-operated with permissionless creation**. This section records the reasoning and acknowledged tradeoffs.

### For binary markets: LMSR chosen

**LMSR** (Hanson 2003) is the scoring rule. Core reasons:

- **Unconditionally strictly proper** — displayed prices equal true belief everywhere. No domain boundary, no bias. For deadcat's AMM-as-price-oracle role, this is a first-order property.
- **Strong first-mover incentives** via exponential cost curve. Informed traders face dramatically higher costs for delaying. This directly improves price discovery speed and the quality of the historical price record — a key deadcat value prop.
- **Extensive production track record**: Augur v1, Gnosis, corporate prediction markets. Known operational characteristics.
- **KL-divergence interpretation**: price is a sufficient statistic for the market's aggregate belief. Elegant theoretical grounding that survives into production.
- **1D Merkle table is tractable**: at depth 16, ~65k entries, ~3-5 kB witness per swap. The table-generation tooling and proof machinery are a one-time cost at pool creation and a manageable per-swap witness overhead.

Acknowledged costs:
- **Larger per-swap witness** (~3-5 kB) vs QMSR's ~2-3 kB. Ongoing per-trade chain fee, paid forever.
- **Worse subsidy efficiency** (~2.77× more collateral for equal depth at N=2). Operators commit more capital per pool.
- **Table generation tooling** must be maintained.

These were weighed against QMSR's advantages (smaller witness, better efficiency) and the decision went to LMSR on robustness grounds — proven math and stronger first-mover incentives matter more for a new platform where operational surprises are expensive.

### For multi-outcome markets: Option C composition (N binary LMSR pools)

For markets with N ≥ 3 outcomes, the pool layer composes **N independent binary LMSR pools**, one per outcome's YES/NO pair. Cross-outcome AMM coherence (`Σp_YES_k = 1`) is arb-enforced.

Rationale:
- **Uniform pool implementation**: one Simplicity contract, one audit, one `deadcat-core` integration, one LP mental model. This is architecturally significant.
- **Parallel trading**: trades on different outcomes hit different pool UTXOs. No serialization across outcomes.
- **Arb efficiency via the N-outcome market contract**: when the underlying market is an N-outcome contract (not N composed binary markets), arbitrageurs exploit its cross-outcome primitives (split-YES, merge-YES, cross-outcome swap) to close coherence gaps in a single atomic transaction. This keeps arb-enforced coherence tight.
- **Structural coherence at N=3 would have been "free" in table size** but costs meaningful implementation complexity (separate 2D covenant, tooling, audit). Given N=3 markets are a minority share of total volume, the ROI on a separate unified N=3 pool contract is weak.

Acknowledged costs:
- **Cross-outcome price coherence is arb-enforced, not structural**. Small price discrepancies persist between arb events.
- **Per-outcome liquidity may fragment**: popular outcomes get deep pools, obscure ones stay thin. Users compose trades across the available pools.

### Alternatives considered and rejected

- **QMSR (unified, any N)**: strong candidate. Polynomial-inline, better subsidy efficiency, any-N support, LP-scaling-invariant. Rejected primarily because it has **zero production deployments** in prediction markets and its linear cost curve produces **weaker first-mover incentives**. Remains a viable fallback if future operational issues with LMSR surface.
- **LS-LMSR**: adaptive b via `b = αS`. Rejected because the adaptive property requires an extra dimension in the Merkle table (one dimension less feasible than standard LMSR at every N), and its small price bias means it's not unconditionally proper. Under Option C binary pools, standard LMSR's fixed b + admin bumps + LOB organic depth substitute adequately.
- **LS-QMSR**: disqualified by the trilemma — at the trader's optimum, displayed prices exhibit a fixed 50% shrinkage toward uniform (`p_k = π_k/2 + 1/(2N)`), independent of α. Breaks the AMM's source-of-truth role.
- **FPMM / constant product** (Gnosis-style): rejected because every pool trade requires atomic market co-spend to preserve `Π x_k = k` over post-split reserves. This serializes all pool trades across all pools of the same market on the market's collateral UTXO. Fatal for parallel pool trading.
- **Unified LMSR at N=3 (2D table)**: feasible with same total entries as binary 1D but different tooling, covenant logic, state encoding, audit surface. Declined because of the uniform-implementation-surface win from "binary only."
- **Higher-degree polynomial scoring rules** (cubic, quartic with LS construction): genuinely novel research territory. Deferred indefinitely — not a v1 consideration.

### Liquidity model: admin-operated, permissionless creation

Each pool has a single operator who:
- Chooses parameters at creation (b, fee_bps, collateral asset, oracle pubkey inherited from market)
- Provides the subsidy capital
- Can adjust b via an admin spend path (permissioned bumps)
- Can close the pool via an admin spend path
- Earns all fees, bears all impermanent loss

Pool creation is **permissionless**: anyone can deploy a pool on any market with any parameters. Multiple competing pools per market are expected and welcomed. LPs with different opinions on fair b and fee rates compete.

LP-tokenized pools (shared ownership, deposit/withdraw mechanics, pro-rata fee distribution) were considered extensively but deferred to v2. Under admin-operation, the covenant stays simpler, the trust model is crisp, and operators have clear incentive alignment. LP-tokenization becomes an upgrade path if passive-capital demand materializes.

### Future directions

- **Revisit QMSR** if LMSR's subsidy efficiency or witness size become real constraints at volume.
- **LP-tokenized pools as v2**: QMSR's scale-invariance makes it the better candidate for LP-tokenization than LMSR (LMSR's Merkle table doesn't play cleanly with dynamic b). If/when LP-tokenized is prioritized, the scoring rule decision may revisit.
- **Cross-outcome arb tooling**: bots and incentive mechanisms to keep multi-outcome arb-enforced coherence tight. Not a contract change; an ecosystem tooling consideration.
- **LS-QMSR revival**: only if either (a) a display-layer correction is validated (showing `2p - 1/N` instead of `p`, with edge-case handling near 0 and 1), or (b) a higher-degree polynomial scoring rule with smaller LS bias is developed.

---

## Key Files

- `docs/contracts/lmsr-pool/lmsr-pool-design.md` — binary LMSR pool design (the chosen pool contract)
- `docs/contracts/lmsr-pool/lmsr-deterministic-table-spec.md` — Merkle table specification
- `docs/contracts/multi-outcome/multi-outcome-market-contract.md` — N-outcome market contract spec (pool liquidity via Option C composition)
- `docs/contracts/multi-outcome/design-journal-multi-outcome-amm.md` — design history record
- `docs/contracts/market-contract-principles.md` — covenant-enforced properties shared across both market contract types
- `docs/contracts/contract-specification.md` — top-level contract reference
- `docs/architecture/transaction-composability-model.md` — composability framework (witness-parameterized indices, atomic multi-contract PSETs)

## References

### Internal
- `../../architecture/deadcat-core-design.md` — main design document
- `../../architecture/enforcement-layers.md` — security framework

### External
- Hanson, R. (2003). "Combinatorial Information Market Design." *Information Systems Frontiers* 5(1).
- Chen, Y. & Pennock, D. (2007). "A Utility Framework for Bounded-Loss Market Makers." *UAI 2007*.
- Abramowicz, M. (2007). "The Hidden Beauty of the Quadratic Market Scoring Rule."
- Othman, A., Sandholm, T., Pennock, D., & Reeves, D. (2013). "A Practical Liquidity-Sensitive Automated Market Maker." *EC 2013*.
- Abernethy, J., Chen, Y., & Vaughan, J.W. (2013). "Efficient Market Making via Convex Optimization, and a Connection to Online Learning." *ACM TEAC* 1(2).
- Brier, G.W. (1950). "Verification of forecasts expressed in terms of probability." *Monthly Weather Review* 78(1).
- Selten, R. (1998). "Axiomatic Characterization of the Quadratic Scoring Rule." *Experimental Economics* 1.
- Nueve, J. & Waggoner, B. (2025). "Smooth Quadratic Prediction Markets." *NeurIPS 2025*.
