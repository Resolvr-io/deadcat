# Market Contract Principles

This document enumerates the design principles that both Deadcat market contracts — the **binary prediction market** and the **multi-outcome market** — must uphold. These are covenant-enforced properties: each one describes what the contract itself guarantees regardless of who builds the transaction or what tooling they use.

Usage conventions (1-2-5 mantissa for collateral amounts, 60-block expiry snapping, well-known collateral asset sets, OP_RETURN recovery hint format, `market_id` derivation formula, asset ordering) are not principles — they are builder-side rules enforceable only by `deadcat-core` itself. This document covers only the on-chain, covenant-level guarantees that hold for any client.

**Framing**: this is the shared market-contract implementation target for `deadcat-core`. The current `prediction_market.simf` source in `deadcat-sdk` does not yet implement all of these principles; see the [legacy source alignment checklist](contract-specification.md#legacy-source-alignment-checklist) in [contract-specification.md](contract-specification.md) for that delta. The intent is that the rewritten contracts meet every principle here.

## Scope

- **In scope**: the binary prediction market contract and the multi-outcome market contract.
- **Out of scope**: the LMSR pool contract and the maker order contract. These have their own principle sets (reserves, pricing integrity, maker-only cancellation) distinct from the market-contract principles below.

## Covenant self-enforcement

Every principle below is covenant-enforced: the Simplicity program verifies the property from transaction-observable data alone. PSET builders are off-chain conveniences; the covenant assumes an adversary constructs the spending transaction. Any constraint that protects funds, preserves solvency, or prevents griefing must live in the covenant, not in builder code.

This framing classifies every constraint the design relies on into one of three buckets:

1. **Covenant-enforced** — checked in the Simplicity program; safe against arbitrary builders.
2. **Builder-enforced, recovery-critical** — violation breaks chain-only recovery decode but cannot drain funds, alter resolution, or produce unauthorized issuance. Acceptable at the builder layer when explicitly documented as such. Examples: the 1-2-5 mantissa table for `collateral_per_pair`, OP_RETURN hint format, expiry snapping to 60-block boundaries.
3. **Builder-enforced, fund-critical** — **not acceptable.** Any constraint that, if violated, permits fund loss, solvency violation, or griefing must be promoted to covenant enforcement before release.

**Audit obligation**: every spend path in every `.simf` must be reviewed against this classification. Every constraint the covenant's correctness depends on belongs in bucket 1; none in bucket 3. Bucket-2 constraints must be named and their failure modes documented.

## Foundational principle

### 1. The contract's purpose is to uphold solvency across every possible resolution outcome

The market contract exists to guarantee that, no matter which outcome resolves, every token holder of the winning side can redeem their tokens at full value from the preserved collateral. The contract permits **any** transaction that preserves this invariant — it is not a whitelist of named operations, but an invariant enforcer.

The concrete operations the contract exposes (issue pair, cancel pair, split YES, merge YES, split NO, merge NO, cross swap, resolution, redemption, expiry) are **convenient bases** of the space of solvency-preserving transitions. They are enumerated for engineering reasons — simpler covenant logic, clearer PSET construction, lower witness overhead — but the underlying principle is the invariant, not the enumeration.

Equivalent statement: *For every outcome k and every reachable contract state, `C ≥ payout(k)` where `payout(k)` is the total collateral owed to winners on that outcome.* The contract only accepts transitions that preserve this bound.

## Authority and permissionlessness

### 2. No privileged role for market operations

Any party — including the oracle — can issue pairs, cancel pairs, split/merge sets, redeem winning tokens, and trigger expiry, with the only prerequisites being the required collateral (for mints) and the required tokens (for burns). No signature from the market creator, the oracle, or any admin is consulted by the covenant for these operations.

The oracle has no special privilege over token supply or collateral movement. If the oracle holds tokens, it participates as a regular user.

### 3. Oracle authority is narrow, pre-committed, and self-consistent

The oracle's sole covenant-granted power is a single on-chain transition: moving the contract from its active phase (Unresolved or Dormant) to a Resolved_k phase. This power is exercised via one BIP-340 Schnorr signature over a tagged hash:

```
message = tagged_hash("deadcat/oracle_attestation", market_id || outcome_index)
```

The tag string is hardcoded in the covenant. The oracle public key is committed into the covenant params at market creation and immutable thereafter. No other oracle action is recognized.

**Self-consistency guarantee (covenant-enforced within one contract)**: at most one outcome resolution lands on-chain, regardless of how many signatures the oracle produces. The first valid resolution consumes all RT UTXOs and the Unresolved collateral UTXO, leaving no spendable state for a second resolution. Both sides of a binary market cannot be simultaneously redeemable; for a multi-outcome market, at most one outcome's winning tokens become redeemable.

**Trust boundary (outside covenant scope)**: the contract cannot verify that the oracle's signed outcome matches reality — Liquid has no access to the outside world. Nor can it enforce coherence across *composed* multi-outcome events built from multiple binary contracts (e.g., "an election built from N per-candidate binary markets"). An oracle signing YES on two composed binary markets produces two covenant-coherent-per-market resolutions that are jointly incoherent; preventing this requires oracle discipline or app-layer arbitrage, not the covenant.

The contract's guarantee: *no matter what the oracle signs, within a single contract's scope the result is self-consistent and solvent.* Users of a single market trust the oracle on outcome truth. Users of composed events additionally trust the oracle to stay consistent across markets.

### 4. NUMS internal key, no key-spend path

The taproot internal key is a NUMS point ("nothing up my sleeve" — a curve point with no known discrete log). Key-spend is cryptographically infeasible. All spends go through the Simplicity script path, ensuring the covenant always runs.

## State machine completeness

### 5. Terminal paths are reachable from every non-terminal state

Markets must be able to reach a terminal state (Resolved_k or Expired) regardless of outstanding token supply. In particular, the zero-liquidity case — a market that was created and never used, or fully unwound back to zero outstanding — must be resolvable by the oracle and expirable by timelock.

Concretely: the Dormant phase (zero outstanding tokens, only RT UTXOs on-chain, no collateral locked) exposes oracle-resolution and timelock-expiry spend paths. Both consume all RT UTXOs, verify RT burn outputs, and produce no covenant continuation, immediately transitioning to Resolved_k or Expired with zero outstanding tokens (terminal).

Without this, an abandoned market's RT UTXOs would sit on-chain indefinitely, and a market with no traders could never be cleaned up — not a security bug, but a lifecycle completeness defect.

### 6. Resolution collapses the contract to a single collateral UTXO

A single valid resolution transition consumes every covenant UTXO except for the new Resolved_k collateral output. Specifically: all RTs are burned (see principle 9), the Unresolved collateral UTXO is consumed, and the only covenant UTXO remaining is the collateral at the Resolved_k script. Its asset and value are preserved from the pre-resolution state.

From this point, the contract only supports redemption spends against the Resolved_k collateral UTXO. No covenant path returns to Unresolved or crosses to a different Resolved_j.

### 7. No double resolution

Combined with principle 6 and principle 9 (RT destruction), a contract cannot be resolved twice. Any second resolution attempt would have to re-mint the Unresolved collateral UTXO and re-materialize the RT UTXOs, both of which are impossible: the covenant has no path back from Resolved_k, and the Elements consensus rule `nInflationKeys.IsNull() || assetBlindingNonce.IsNull()` makes additional RT creation impossible (see [enforcement-layers.md](../architecture/enforcement-layers.md), "RT Supply is Fixed at Creation").

## Solvency and conservation

### 8. Collateral conservation on every transition

Every mint or burn transition is gated by a covenant-checked relationship between the collateral delta and the token delta:

- Minting operations increase the locked collateral by exactly the amount required to back the new tokens under the solvency invariant.
- Burning operations release exactly the amount of collateral the burned tokens were backing.

The per-contract formulas differ (binary uses `collateral_per_pair`; multi-outcome uses `collateral_per_pair × ΔQ` where Q is the outcome-independent solvency quantity), but the principle is identical: the covenant evaluates the full transition against the invariant and rejects any transaction that would leave the contract under-collateralized for any possible outcome.

### 9. RT destruction on terminal transitions

Every resolution and expiry transition burns all reissuance tokens via a covenant-verified burn output (`ensure_blinded_reissuance_burn_output` or equivalent). The burn output uses bare `OP_RETURN` (consensus-level unspendability, pruned from the UTXO set) rather than a P2WSH-to-zero, and uses the correct asset ID, value, and deterministic blinding factors.

**Why this matters**: Elements-level reissuance operates below the covenant layer. Any party holding an RT UTXO and knowing its ABF (which is public under deterministic blinding — see principle 11) can mint new tokens of the original asset. If an RT ever escapes to a wallet address after resolution, the covenant no longer runs on it and the Elements protocol will accept reissuances against it. RT destruction is the defense. See [enforcement-layers.md](../architecture/enforcement-layers.md), "Gotcha 1: Elements Reissuance Bypasses Covenants."

### 10. No parasitic issuance

Every non-issuance covenant spend path calls `ensure_no_issuance` on every covenant input, rejecting any transaction that attaches issuance fields (`nAmount`, `nInflationKeys`, `assetBlindingNonce`, `assetEntropy`) to an input that is not exercising the covenant's issuance path.

**Why this matters**: Elements consensus allows any input to carry issuance fields. Without explicit opt-out, a malicious builder could attach token issuance to a resolution spend, a cancellation, or any other path, minting unbacked tokens alongside a legitimate transition.

### 11. Deterministic RT blinding

All RT outputs use covenant-enforced deterministic blinding factors:

- ABF derived via tagged hash from the defining outpoint (public, recomputable).
- CBF passed through unchanged from input to output across every transition (CBF is constant over an RT's lifetime).
- VBF computed as `VBF = CBF - ABF`.

Elements' traditional RT security relies on ABF secrecy; deterministic blinding deliberately makes ABFs public (for permissionless recovery and transaction construction). This removes the traditional Elements-layer safeguard and makes the covenant-enforced blinding scheme load-bearing against two attack classes:

1. **Griefing**: without covenant enforcement, a malicious issuer could use random ABFs/VBFs for new RT outputs, locking the market for all other participants (no one else can compute the VBFs needed for subsequent Pedersen balance). Covenant enforcement makes this impossible.
2. **Reissuance**: unauthorized reissuance is prevented by RT burn enforcement (principle 9), not ABF secrecy. The two defenses are complementary — not redundant.

See [deterministic-rt-blinding.md](../protocol/deterministic-rt-blinding.md).

### 12. Correct redemption rates

Redemption transitions release collateral at covenant-verified rates. Both market contracts parameterize on `base_payout` (the primary denomination) and derive `cp := base_payout × N` where N is the outcome count (`N = 2` for binary, `N ∈ [3, MAX_N]` for multi-outcome). All rates below are exact integers by construction:

- **Resolved**: winning tokens redeem for `cp = base_payout × N` each. All other tokens are inert.
- **Expired**: Binary: `base_payout` per token, symmetric across YES and NO (total `2 × base_payout = cp`). Multi-outcome: `base_payout` per YES token, `base_payout × (N-1)` per NO token.

The expired rates treat every outcome as equally probable (a uniform 1/N prior) — not because this is "correct" in any Bayesian sense, but because it is the solvency-preserving choice under the constraint that the covenant cannot run arbitrary dynamic computation.

**Exact redemption is structural, not asserted.** Parameterizing on `base_payout` and deriving `cp = base_payout × N` makes divisibility automatic: every expiry rate is an integer multiple of `base_payout`. The covenant performs no division at runtime, asserts no divisibility predicate, and rejects no markets for having "incompatible" denominations. See [multi-outcome-market-contract.md § Denomination model](multi-outcome/multi-outcome-market-contract.md#denomination-model) for the full rationale.

## UTXO identity and aliasing

### 13. Sibling UTXO check on co-spent covenant inputs

Every transition in the active phases (Unresolved, Dormant) that co-spends multiple covenant inputs requires that every covenant input in the set share the same `prev_txid` — i.e., was created by the same previous transition. This prevents collateral-substitution attacks where an attacker creates a fake UTXO at the covenant's collateral script address (the script is public and derivable from market params) and co-spends it with real RTs.

The check is **position-independent**: it validates a property of the input set, not where those inputs sit in the transaction's input list. Principle 15 (witness-parameterized indices) leverages this independence.

Every transition, including partial cancellation / partial burn, must co-spend all covenant inputs to maintain the sibling invariant across the market's lifecycle. See [enforcement-layers.md](../architecture/enforcement-layers.md), "Gotcha 5: Covenant Scripts Are Not Unique Per UTXO."

### 14. Asset identity on every constrained output

Every covenant-constrained output has its asset ID explicitly verified against the expected value committed in market params — collateral outputs against `collateral_asset_id`, token outputs against the appropriate `yes_token_asset_id` or `no_token_asset_id[k]`, RT continuation outputs against the appropriate reissuance token asset ID.

A covenant that only checks value and script pubkey (but not asset) can be satisfied by substituting a different asset at the same value, silently draining the market.

### 15. Witness-parameterized input and output indices

Both market contracts accept `in_base` and `out_base` from the Simplicity witness. The covenant asserts `current_index() == in_base + (my slot offset)` and validates a contiguous block of covenant inputs starting at `in_base` and a contiguous block of expected outputs starting at `out_base`.

This flexibility does **not** weaken covenant correctness. The safety argument is the same one already used for pool and order composition: the witness only chooses **where** the contract's input/output window sits in the transaction, not **what** the contract accepts. The covenant still verifies bounded contiguous windows, the expected script for every continuation output, and the expected asset on every constrained output. A malicious builder can move the window or overlap it with unrelated transaction structure, but cannot make the contract accept another contract's output or silently alias an output that fails the script/asset checks.

This enables flexible multi-contract transaction composition — a market transition can be co-spent with a binary LMSR pool swap, a maker order fill, or another market's operation in a single atomic transaction, with the PSET builder choosing where each contract's inputs and outputs sit. Key cases this unlocks:

- **Cross-outcome arb** on multi-outcome markets: the market's split-YES or merge-YES primitive co-spent with N pool swaps in one tx, closing `Σ p_YES_k = 1` coherence gaps atomically.
- **Pool + maker-order routing**: a trade that crosses both a pool and a resting maker order in one tx, taking the best aggregate price.
- **Atomic liquidity bootstrap**: pool creation co-spent with a market split operation that sources the pool's initial token reserves.

**No correctness sacrifice.** The covenant still verifies, per-position within its block: expected script pubkey, expected asset ID, expected value, expected issuance/burn, expected blinding factors. Aliasing across contracts is blocked by script uniqueness (different contract params → different script pubkeys). Aliasing within the contract's own output block is blocked because every slot has a distinct script in the set.

See [transaction-composability-model.md](../architecture/transaction-composability-model.md) for the general composition framework.

## Consensus-layer integration

### 16. Timelock-enforced expiry

Expiry transitions require `nLockTime ≥ expiry_time` where `expiry_time` is a covenant param fixed at market creation. The covenant uses the `check_lock_height` Simplicity jet to defer enforcement to Elements consensus — Layer 1 ensures the transaction's locktime is respected; Layer 2 checks that the locktime satisfies the expiry requirement.

### 17. Confidential-transaction compatibility

The covenant operates correctly with confidential inputs and outputs. RT outputs are always blinded (their deterministic CBF pass-through self-balances the RT portion of the Pedersen commitment equation, meaning transactions with zero or more additional blinded wallet outputs both work). Collateral continuation outputs are explicit-value to keep covenant introspection simple. User-facing token and collateral outputs can be blinded or explicit at the builder's choice.

## Summary: the covenant's contract with the world

Putting the principles together: the covenant guarantees that, for every market, the only thing the oracle can do on-chain is attest one outcome (narrow authority), and the only thing other actors can do is execute transactions that preserve solvency across every possible resolution outcome (permissionless within the invariant). When an outcome is attested, the contract collapses to a single collateral UTXO that pays winning tokens at full value and cannot be re-resolved or tampered with, even by the oracle. All of this holds against the full Elements attack surface (reissuance, parasitic issuance, fake UTXOs, asset substitution, blinding griefing) because every cross-layer escape hatch is explicitly closed.

Truthfulness of signed outcomes, and coherence across composed multi-outcome events, lie outside the covenant's enforcement scope and require oracle trust.

## Key Files

- [contract-specification.md](contract-specification.md) — top-level index with per-contract parameters, slot layouts, and spend paths for the binary and multi-outcome markets
- [multi-outcome/multi-outcome-market-contract.md](multi-outcome/multi-outcome-market-contract.md) — multi-outcome (2N token) market full spec
- [../architecture/enforcement-layers.md](../architecture/enforcement-layers.md) — cross-layer security framework (Layers 1-4, per-property enforcement table, cross-layer gotchas)
- [../architecture/transaction-composability-model.md](../architecture/transaction-composability-model.md) — witness-parameterized indices and multi-contract composition
- [../protocol/deterministic-rt-blinding.md](../protocol/deterministic-rt-blinding.md) — RT blinding scheme and covenant enforcement
- [../protocol/oracle-bip340-tagged-hash.md](../protocol/oracle-bip340-tagged-hash.md) — oracle attestation message format
- [../protocol/chain-only-recovery.md](../protocol/chain-only-recovery.md) — recovery flow that builds on these principles
