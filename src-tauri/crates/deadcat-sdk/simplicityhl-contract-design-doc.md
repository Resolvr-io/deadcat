# Binary Prediction Market Covenant Smart Contract

**Design Document**

SimplicityHL on Liquid · Resolvr Inc. · February 2026

**DRAFT**

---

## 1. Overview

This document specifies the design of a Polymarket-style binary prediction market smart contract implemented in SimplicityHL, targeting the Liquid sidechain. The contract enables permissionless creation of binary outcome markets where participants buy and sell YES and NO tokens backed by collateral (L-BTC). An off-chain oracle resolves the market by committing an outcome on-chain via a state transition, after which winning token holders redeem collateral.

The design draws on two existing Blockstream reference contracts: the options contract (`options.simf`) for reissuance-based token minting and collateral management, and the Astrolabe contract for state commitment via taproot data embedding. It extends both with a four-state covenant model, single-UTXO collateral consolidation, and oracle signature verification using FROST threshold keys.

## 2. Design Goals and Constraints

**Permissionless issuance.** Anyone who deposits collateral can mint YES/NO token pairs. There is no issuer key or privileged minter. The covenant constraints alone enforce correctness.

**Oracle equivocation protection.** A dishonest oracle that signs both YES and NO outcomes cannot cause catastrophic loss. The state-commit model ensures that once an outcome is recorded on-chain, all winners on that side are treated equally. This converts a potential race-to-drain failure into a legible, disputable one.

**Deterministic redemption value.** After resolution, each winning token is worth a fixed, known amount of collateral. This enables risk-free off-chain swap services that absorb the on-chain serialization bottleneck.

**No division in contract arithmetic.** All collateral calculations use multiplication only, eliminating rounding bugs and integer division edge cases.

**Explicit amounts for issuance.** YES tokens, NO tokens, and collateral are required to use explicit (non-confidential) amounts during issuance, keeping covenant verification simple. Reissuance tokens remain confidential per Elements protocol requirements.

## 3. Contract Parameters

The contract is parameterized at compile time with eight values. A ninth value, MARKET_ID, is derived deterministically from the token asset IDs rather than supplied independently.

| Parameter | Description |
|-----------|-------------|
| `ORACLE_PUBLIC_KEY` | X-only Schnorr pubkey. Aggregate key from 2-of-3 FROST threshold signing performed off-chain. |
| `COLLATERAL_ASSET_ID` | Asset ID of the collateral (typically L-BTC). |
| `YES_TOKEN_ASSET` | Deterministic asset ID for YES tokens, derived from the outpoint used in the creation transaction. |
| `NO_TOKEN_ASSET` | Deterministic asset ID for NO tokens, derived from the outpoint used in the creation transaction. |
| `YES_REISSUANCE_TOKEN` | Reissuance token for YES asset. Required for minting additional YES tokens after creation. |
| `NO_REISSUANCE_TOKEN` | Reissuance token for NO asset. Required for minting additional NO tokens after creation. |
| `COLLATERAL_PER_TOKEN` | Satoshis backing each individual token. The unit of account for all collateral arithmetic. |
| `EXPIRY_TIME` | Block height deadline for oracle resolution. After this height, the market enters expiry mode. |

**Derived value:** `MARKET_ID = SHA256(YES_TOKEN_ASSET || NO_TOKEN_ASSET)`. This provides unique per-market domain separation for oracle signatures without requiring an additional parameter. Since token asset IDs are deterministically derived from outpoints, MARKET_ID is globally unique and verifiable by anyone who knows the contract parameters.

### 3.1 Design Decision: Eliminating MARKET_ID as a Parameter

Early designs included MARKET_ID as a standalone random 32-byte parameter. This was replaced with derivation from the token asset IDs because: (a) it eliminates a coordination step during bootstrapping—no need to generate and distribute a random value, (b) the oracle only needs to know which asset pair it is attesting for, which it already must know, and (c) it prevents stale pre-commitments since the oracle cannot pre-sign outcomes before the asset IDs exist. The constraint that the oracle cannot attest before asset creation is considered desirable, not limiting.

## 4. Collateral Mechanics

`COLLATERAL_PER_TOKEN` is the unit of account. Every issuance mints one YES token and one NO token per pair. The collateral deposited per pair is `2 * COLLATERAL_PER_TOKEN`—one unit backing the YES token, one backing the NO token. This means the contract never performs division; all paths use multiplication against COLLATERAL_PER_TOKEN.

### 4.1 Collateral Flows by Path

| Path | Formula | Rationale |
|------|---------|-----------|
| Initial issuance | `pairs * 2 * CPT` deposited | First issuance; no existing collateral. |
| Subsequent issuance | `pairs * 2 * CPT` added to existing | Each pair needs collateral for both outcomes. |
| Post-resolution redemption | `tokens * 2 * CPT` withdrawn | Winners collect full pair collateral; losers forfeit. |
| Expiry redemption | `tokens * CPT` withdrawn | Both sides redeem at equal rate; pool drains exactly. |
| Cancellation | `pairs * 2 * CPT` refunded | Matched pair burned; full deposit returned. |

### 4.2 Design Decision: Winners Receive Full Pair Collateral

An earlier design had winning tokens redeeming at `1 * COLLATERAL_PER_TOKEN`, which would leave the losing side's collateral permanently locked in the covenant. The revised design pays winners `2 * COLLATERAL_PER_TOKEN` per token, which correctly reflects prediction market economics: buying a YES token at 0.60 and winning pays out 1.00 (the combined collateral from both sides of the pair). This ensures the collateral pool drains exactly as all winners redeem, with no dead collateral.

### 4.3 Design Decision: Naming COLLATERAL_PER_TOKEN (Not COLLATERAL_PER_PAIR)

The parameter was initially considered as COLLATERAL_PER_PAIR with division by 2 for single-token operations. This was rejected because it would introduce integer division into the contract, creating potential for rounding errors (odd values of COLLATERAL_PER_PAIR would lose a satoshi). By defining the parameter as the per-token unit and multiplying by 2 for pair operations, the contract avoids division entirely. Wallet and UI software handles the conversion for display purposes.

## 5. State Model

The contract uses a four-state model encoded as a single `u64` value. State is committed via the tapdata-in-taproot pattern from the Astrolabe contract: the contract's tapleaf CMR and a state tapdata leaf are combined into a tapbranch, tweaked onto a NUMS (Nothing Up My Sleeve) key, producing a P2TR script hash. Each state value produces a different covenant address, yielding four possible addresses per market, all computable at compile time.

| Value | State | Description |
|-------|-------|-------------|
| 0 | DORMANT | Reissuance tokens deposited, no collateral. Awaiting initial issuance. |
| 1 | UNRESOLVED | At least one issuance has occurred. Market is live. |
| 2 | RESOLVED_YES | Oracle committed YES outcome. YES tokens are redeemable. |
| 3 | RESOLVED_NO | Oracle committed NO outcome. NO tokens are redeemable. |

### 5.1 State Validation (Fraud Prevention)

The contract validates its own state by comparing the witness-provided state value against the actual input script hash. In `main()`, the contract computes `script_hash_for_input_script(witness::STATE)` and asserts it equals `input_script_hash(current_index)`. If the caller lies about the state (e.g., claims state 0 when the UTXO is at the state-1 address), the computed script hash will not match, and the assertion fails. This makes state spoofing cryptographically impossible.

### 5.2 State Transitions

State transitions are strictly controlled:

- **0 → 1:** Initial issuance. First batch of tokens minted, collateral deposited, reissuance tokens move from Dormant to Unresolved.
- **1 → 1:** Subsequent issuance, partial cancellation, expiry redemption. Covenant UTXOs recycled at the same state-1 address.
- **1 → 0:** Full cancellation. All collateral withdrawn, reissuance tokens cycle back to Dormant, enabling future issuance.
- **1 → 2 or 1 → 3:** Oracle resolve. All covenant UTXOs move to state-2 or state-3 address.
- **2 → 2 or 3 → 3:** Post-resolution redemption. Covenant UTXOs stay at resolved address, collateral decreases.

### 5.3 Design Decision: Four States with Dormant State

An earlier design used three states (UNRESOLVED=0, RESOLVED_YES=1, RESOLVED_NO=2) where market creation was a single plain Elements transaction that issued YES/NO tokens, deposited reissuance tokens and collateral to the UNRESOLVED covenant address, and sent minted tokens to the creator. This was simpler but had a critical flaw: full cancellation (burning all tokens and withdrawing all collateral) stranded the reissuance tokens at the Unresolved covenant address with no collateral companion, making future issuance impossible without manual "revival."

The current design adds a Dormant state (state 0). Market creation is still a plain Elements transaction (no Simplicity validation), but it only deposits reissuance tokens to the Dormant covenant address—no tokens are minted and no collateral is deposited. A separate covenant-validated initial issuance transaction (0→1) mints the first batch of tokens and deposits collateral. Full cancellation (1→0) cycles reissuance tokens back to Dormant, enabling future issuance without workarounds. This adds one spending path and one confirmation wait to the bootstrapping flow, but eliminates the stranded-token problem entirely.

## 6. Single-UTXO Collateral Consolidation

A critical design requirement is that all collateral for a given market exists in a single UTXO at all times. This is necessary for the state-commit model to work: the oracle resolve transaction must transition ALL market assets from the state-1 (UNRESOLVED) address to the state-2 or state-3 (resolved) address atomically.

### 6.1 The Problem

The options contract (`options.simf`), which served as a reference implementation, does NOT consolidate collateral. Each funding operation creates a separate UTXO. This is acceptable for options (each position is independent), but breaks the prediction market's state model: if collateral is fragmented across N UTXOs, each with its own state commitment, the oracle resolve transaction would need to spend all N simultaneously, and any UTXO missed would remain in the UNRESOLVED state indefinitely.

### 6.2 The Solution

The issuance paths enforce consolidation by requiring that the existing collateral UTXO be consumed as an input and a new consolidated UTXO be produced as output. The creation transaction (a plain Elements transaction, not covenant-validated) deposits only reissuance tokens at the state-0 (DORMANT) address. The initial issuance transaction (0→1, covenant-validated) establishes the first collateral UTXO at the state-1 (UNRESOLVED) address. On each subsequent issuance (state stays 1), the existing collateral UTXO must be consumed (verified via script hash matching the state-1 address), and the output collateral equals the old amount plus the new deposit.

The reissuance tokens act as the enforcement mechanism: you cannot mint new YES or NO tokens without spending the reissuance token UTXOs, which only exist at the covenant address. The issuance paths that spend these tokens also require consuming and reconsolidating the collateral UTXO. Therefore, there is always exactly one collateral UTXO per active market (or zero when the market is in the Dormant state).

### 6.3 Design Decision: Consolidation Over Fragmentation

The alternative—allowing fragmented collateral and having the oracle resolve path sweep all UTXOs—was rejected for several reasons. The oracle would need to know the exact UTXO set at resolution time, which creates a coordination problem. Any UTXO created between the oracle's UTXO snapshot and the resolution transaction's confirmation would be missed. The single-UTXO model eliminates this entirely: there is always exactly one collateral UTXO, one YES reissuance token UTXO, and one NO reissuance token UTXO at the covenant address.

## 7. Spending Paths

The contract has seven spending paths, selected via a 7-way nested `Either` type in the witness. Paths 1–2 handle initial and subsequent issuance. Path 3 handles oracle resolution. Path 4 handles post-resolution redemption. Paths 5–6 handle expiry redemption and cancellation. Path 7 validates secondary covenant inputs when multiple covenant UTXOs are spent in the same transaction.

### 7.1 Initial Issuance (State 0 → 1)

First covenant-validated issuance. Transitions from Dormant to Unresolved.

**Preconditions:** State is 0 (DORMANT). Transaction lock_time is less than EXPIRY_TIME. Current input index is 0 (this covenant input is the primary input).

**Enforced constraints:**

- Reissuance tokens consumed from Dormant covenant and cycled.
- Collateral comes from an external input (not from the covenant—there is no existing collateral UTXO in the Dormant state).
- YES and NO tokens minted in equal amounts.
- Collateral output = `pairs_minted * 2 * COLLATERAL_PER_TOKEN`.
- All covenant outputs go to state-1 (UNRESOLVED) address.

### 7.2 Subsequent Issuance (State Stays 1)

Permissionless minting. Anyone depositing collateral can mint more pairs.

**Preconditions:** State is 1 (UNRESOLVED). Transaction lock_time is less than EXPIRY_TIME. Current input index is 0 (this covenant input is the primary input).

**Enforced constraints:**

- Existing collateral UTXO consumed as input 2 (script hash verified against state-1 address).
- Reissuance tokens consumed and cycled.
- YES and NO tokens minted in equal amounts.
- New collateral output = old collateral + `pairs_minted * 2 * COLLATERAL_PER_TOKEN`.
- All covenant outputs go to state-1 address.

### 7.3 Oracle Resolve (State 1 → 2 or 1 → 3)

The state-commit transaction. Posts the oracle's outcome attestation on-chain.

**Preconditions:** State is 1 (UNRESOLVED). Transaction lock_time is less than EXPIRY_TIME. Current input index is 0 (this covenant input is the primary input).

**Enforced constraints:**

- Witness provides outcome (true for YES, false for NO).
- Oracle signature over `SHA256(MARKET_ID || outcome_byte)` verifies against ORACLE_PUBLIC_KEY. Outcome byte is 0x01 for YES, 0x00 for NO.
- Existing collateral UTXO consumed as input 2 (script hash verified against state-1 address).
- Collateral amount preserved exactly (no tokens minted or burned).
- Reissuance tokens preserved.
- All covenant outputs move to the new state address (state 2 or 3).
- Transaction has exactly 4 outputs (3 covenant outputs + 1 fee).

### 7.4 Post-Resolution Redemption (State 2 or 3)

**Preconditions:** State is 2 or 3.

- Burned token asset matches the winning side (YES if state 2, NO if state 3).
- Tokens burned to an OP_RETURN output.
- Collateral withdrawn equals `tokens_burned * 2 * COLLATERAL_PER_TOKEN`.
- Remaining collateral stays at the resolved-state address. Reissuance tokens are not spent in this transaction and remain at the resolved-state address by Liquid consensus.

### 7.5 Expiry Redemption (State 1, Post-Expiry)

**Preconditions:** State is 1 (UNRESOLVED) and lock_time ≥ EXPIRY_TIME.

- Either YES or NO tokens accepted (both sides redeem equally).
- Tokens burned to an OP_RETURN output.
- Collateral withdrawn equals `tokens_burned * COLLATERAL_PER_TOKEN` (half the post-resolution rate, since both sides participate).
- Remaining collateral stays at state-1 address. Reissuance tokens are not spent in this transaction and remain at state-1 by Liquid consensus.

### 7.6 Cancellation (State 1 → 1 partial, State 1 → 0 full)

**Preconditions:** State is 1 (UNRESOLVED). No time constraint.

- Equal amounts of YES and NO tokens burned.
- Collateral returned equals `pairs_burned * 2 * COLLATERAL_PER_TOKEN`.

**Partial cancellation** (remaining collateral > 0):

- Remaining collateral stays at state-1 address. Reissuance tokens are not spent in this transaction and remain at state-1 by Liquid consensus.

**Full cancellation** (remaining collateral == 0):

- Reissuance token UTXOs consumed as inputs 1 (YES) and 2 (NO), verified via Pedersen commitment.
- Reissuance tokens cycled to outputs 0 and 1 at the state-0 (DORMANT) address.
- Token burns at outputs 2 and 3.
- This returns the market to the Dormant state, enabling future initial issuance without workarounds.

### 7.7 Secondary Covenant Input

**Preconditions:** None (works in any state).

This path validates covenant UTXOs that are spent as secondary inputs in a transaction where another covenant UTXO is the primary input (index 0). It is used when multiple covenant UTXOs—reissuance tokens and collateral—must be consumed in the same transaction (e.g., issuance, oracle resolve).

**Enforced constraints:**

- The current input's script hash matches input 0's script hash (same covenant address).
- The current input index is not 0 (it is a secondary input, not the primary).

The primary input (index 0) runs one of paths 1–6 and enforces all transaction-level constraints. This path only ensures that the secondary input legitimately belongs to the same covenant instance.

## 8. Timelock Strategy

The contract uses EXPIRY_TIME (a block height) as the boundary between pre-expiry and post-expiry operations. Rather than introducing a separate EXPIRED state, expiry is enforced as a guard condition on each path.

### 8.1 Pre-Expiry Enforcement

SimplicityHL provides `jet::check_lock_height(threshold)` which asserts `lock_time >= threshold` (post-threshold). There is no complementary jet for asserting lock_time < threshold. For pre-expiry paths (issuance, oracle resolve), the contract reads the transaction's lock_time via `jet::lock_time()` and performs a manual comparison:

```rust
let tx_lock_time: u32 = jet::lock_time();
assert!(jet::lt_32(tx_lock_time, param::EXPIRY_TIME));
```

### 8.2 Post-Expiry Enforcement

For the expiry redemption path, `jet::check_lock_height(param::EXPIRY_TIME)` is used directly. This provides both script-level validation and consensus-level enforcement (the transaction literally cannot be mined before that block height).

### 8.3 Summary by Path

| Path | Time Check | Method |
|------|-----------|--------|
| Initial issuance | Pre-expiry | `jet::lock_time()` + `jet::lt_32()` |
| Subsequent issuance | Pre-expiry | `jet::lock_time()` + `jet::lt_32()` |
| Oracle resolve | Pre-expiry | `jet::lock_time()` + `jet::lt_32()` |
| Post-resolution redemption | None | Resolution implies pre-expiry already occurred. |
| Expiry redemption | Post-expiry | `jet::check_lock_height()` |
| Cancellation | None | Works at any time while unresolved. |
| Secondary covenant input | None | Delegates time enforcement to the primary input's path. |

## 9. Oracle Model

The oracle is a 2-of-3 FROST threshold signing committee that operates entirely off-chain. On-chain, the contract sees a single Schnorr public key (the FROST aggregate key) and verifies a single Schnorr signature via `jet::bip_0340_verify`.

### 9.1 Signature Scheme

The oracle signs the message `SHA256(MARKET_ID || outcome_byte)` where outcome_byte is 0x01 for YES or 0x00 for NO. The message is deterministic and independent of the transaction, meaning the oracle attestation can be produced off-chain and submitted by anyone. The oracle does not need to construct or sign the resolve transaction itself.

### 9.2 Equivocation Protection

The state-commit model provides the core equivocation protection. If the oracle signs both YES and NO (whether through collusion, key compromise, or operational failure), only the first attestation to be committed on-chain takes effect. The second attestation is useless because the covenant has already transitioned out of state 1 (UNRESOLVED), and there is no path between the two resolved states (2 and 3).

Without state commitment, a double-signing oracle would create a race condition: whoever submits their redemption transaction first drains the collateral pool, and later redeemers get nothing. This is catastrophically unfair. The state-commit model converts this into a legible, disputable event: the outcome was committed to one side, and all winners on that side are treated equally.

Note: the oracle's state commitment transitions the covenant from state 1 to state 2 or 3, and there is no path from state 2 to state 3 or vice versa.

## 10. Serialization and the Swap Service

### 10.1 The Serialization Bottleneck

The single-UTXO covenant model means all spending paths that touch the covenant UTXO are serialized: only one transaction at a time can spend it. This is inherent to the design and is the cost of equivocation protection via state commitment.

Issuance serialization is unavoidable (the reissuance token constraint is fundamental). Oracle resolution happens once and is not a concern. Redemption is the bottleneck: N winners must sequentially spend the same covenant UTXO. In the worst case, 500 individual redemptions on Liquid (≈1-minute blocks) would take approximately 8 hours.

### 10.2 Swap Service Mitigation (Clearinghouse Pattern)

Once the oracle attestation is confirmed on-chain, the value of winning tokens is deterministic and risk-free: each winning token is worth exactly `2 * COLLATERAL_PER_TOKEN`. A swap service can offer to buy winning tokens at face value with zero market risk. Its only costs are operational (transaction fees, amortized across batched redemptions).

The swap service accumulates winning tokens off-chain or via Liquid atomic swaps, then batch-redeems against the covenant in large transactions. This transforms the bottleneck from "N individual redemptions serialized on-chain" to "one sophisticated actor draining the pool in a few large transactions."

This pattern mirrors traditional finance clearinghouses: net positions and settle in bulk. The on-chain covenant is the settlement layer; the swap service is the clearing layer. The service is a natural role for the market operator, a liquidity provider, or any arbitrageur.

### 10.3 Design Decision: State Commit Despite Serialization

The serialization cost was weighed against the stateless alternative (verifying the oracle signature at redemption time with no state commitment). The stateless model allows parallel redemption but fails catastrophically under oracle equivocation: the oracle signs both sides, and a race to drain collateral begins. First redeemers win; later redeemers lose everything. The state-commit model's serialization cost, mitigated by the swap service, was judged far preferable to the stateless model's catastrophic failure mode.

## 11. Contract Structure

### 11.1 main() Entry Point

The contract's `main()` function reads all witness values, validates state, then dispatches to the appropriate spending path via a 7-way nested `Either` match. All witnesses are read unconditionally in `main()` before dispatch—this is a SimplicityHL requirement, as witness values must be bound at the top level even if only a subset is used by any given path.

```rust
fn main() {
    let state: u64 = witness::STATE;

    // Validate state commitment
    assert!(jet::eq_256(
        script_hash_for_input_script(state),
        input_script_hash(current_index)
    ));

    // Read all witnesses (SimplicityHL requirement)
    // ... blinding factors, oracle sig, tokens_burned, etc.

    match witness::PATH {
        Left(path_1_to_4) => match path_1_to_4 {
            Left(path_1_or_2) => match path_1_or_2 {
                Left(_)  => { /* 1. Initial issuance            */ },
                Right(_) => { /* 2. Subsequent issuance          */ },
            },
            Right(path_3_or_4) => match path_3_or_4 {
                Left(_)  => { /* 3. Oracle resolve               */ },
                Right(_) => { /* 4. Post-resolution redemption   */ },
            },
        },
        Right(path_5_to_7) => match path_5_to_7 {
            Left(path_5_or_6) => match path_5_or_6 {
                Left(_)  => { /* 5. Expiry redemption            */ },
                Right(_) => { /* 6. Cancellation                 */ },
            },
            Right(_) => { /* 7. Secondary covenant input      */ },
        },
    }
}
```

### 11.2 Utility Functions

The contract reuses utility functions from the Astrolabe and options contracts:

- `script_hash_for_input_script` — computes the P2TR script hash for a given state value.
- `compute_p2tr_script_hash_from_output_key`, `covenant_nums_key` — taproot address computation.
- `verify_token_commitment`, `verify_input_reissuance_token`, `verify_output_reissuance_token` — reissuance token validation.
- `get_input_explicit_asset_amount`, `get_output_explicit_asset_amount` — explicit amount introspection.
- `ensure_output_script_hash_eq`, `ensure_input_script_hash_eq` — address enforcement.
- `ensure_output_is_op_return` — token burn verification.

**New function:** Oracle signature verification—constructs `SHA256(MARKET_ID || outcome_byte)` and calls `jet::bip_0340_verify`.

## 12. Bootstrapping Sequence

1. Choose two UTXOs whose outpoints will define the deterministic YES and NO asset IDs and reissuance token IDs.
2. Compile the contract with all eight parameters. This produces the CMR and the four covenant addresses (one per state).
3. Construct the creation transaction: a plain Elements transaction that spends the two asset-defining UTXOs (with native issuance). Issues reissuance tokens only (no YES/NO tokens, no collateral) and deposits them to the state-0 (DORMANT) covenant address. No Simplicity validation occurs on this transaction.
4. Construct the initial issuance transaction (state 0 → 1): a covenant-validated transaction that consumes the reissuance tokens from the Dormant address, deposits collateral, mints the first batch of YES/NO tokens, and moves all covenant UTXOs to the state-1 (UNRESOLVED) address.
5. Market is live at state 1. Anyone can mint more pairs, cancel, or wait for resolution.
6. Oracle resolves by posting the state-commit transaction (state 1 → 2 or 1 → 3).
7. Winning token holders redeem directly or sell to a swap service.

*Note: The creation transaction is not covenant-validated—a malformed creation would create unspendable UTXOs, but the SDK validates amounts and computes deterministic asset IDs. Anyone evaluating a market should verify the creation transaction to confirm correct reissuance token issuance and covenant address outputs. The covenant enforces correctness of all subsequent transactions starting from initial issuance.*

## 13. Explicit vs. Confidential Amounts

Issuance transactions require explicit (non-confidential) amounts for YES tokens, NO tokens, and collateral. This simplifies covenant verification: the contract can directly read and compare amounts via `get_output_explicit_asset_amount` without needing to verify Pedersen commitment arithmetic or handle blinding factors.

Reissuance tokens remain confidential per Elements protocol requirements. The existing `verify_token_commitment` function from the options contract handles confidential reissuance token verification using asset/value blinding factors provided as witness data.

### 13.1 Design Decision: Explicit Amounts

The alternative—supporting confidential amounts for minted tokens and collateral—would require the SimplicityHL script to verify that confidential YES and NO token amounts are equal, and that the confidential collateral amount matches the expected deposit. This would involve Pedersen commitment arithmetic in the contract, significantly increasing complexity and program weight. Since issuance is a public operation (anyone can see that tokens are being minted), confidentiality provides limited benefit. The complexity cost of confidential verification was judged not worth the marginal privacy gain.

## 14. Resolved Implementation Details

The following items were identified during design and have been resolved in the implementation.

**Reissuance token input/output layout.** The creation transaction creates reissuance tokens at output indices 0 (YES) and 1 (NO) via native Liquid issuance from inputs 0 and 1 (reissuance tokens only, no asset value). The covenant-validated issuance paths consume existing reissuance tokens from inputs 0 and 1, and cycle them to outputs 0 and 1 via reissuance.

**Fee output handling.** For paths with fixed transaction layouts (issuance, oracle resolve), the fee output index is hardcoded (index 5 for issuance, index 3 for oracle resolve). For paths with variable layouts (redemption, cancellation), the fee output is dynamically computed as `num_outputs - 1`. All fee outputs are verified to have an empty script hash (OP_RETURN).

**Output count enforcement.** Oracle resolve enforces exactly 4 outputs (`num_outputs == 4`): three covenant outputs (YES reissuance token, NO reissuance token, collateral) plus one fee output. Other paths use flexible output counts to accommodate partial vs. full redemption scenarios.

## 15. Summary of Design Decisions

| Decision | Chosen | Rejected Alternative |
|----------|--------|---------------------|
| Collateral naming | COLLATERAL_PER_TOKEN with multiplication | COLLATERAL_PER_PAIR with division |
| State model | Four states (0/1/2/3) with Dormant + plain creation tx | Three states without Dormant (strands tokens on full cancellation) |
| Collateral structure | Single UTXO, consolidated on each issuance | Fragmented UTXOs (options contract pattern) |
| Oracle state commitment | On-chain state transition (covenant) | Stateless oracle sig check at redemption |
| Winning payout | 2 * CPT per token (full pair collateral) | 1 * CPT per token (dead collateral) |
| MARKET_ID | Derived from token asset IDs | Standalone random parameter |
| Issuance amounts | Explicit (non-confidential) | Confidential with Pedersen verification |
| Pre-expiry check | `jet::lock_time()` + manual compare | `check_lock_height` (only does ≥) |
| Serialization mitigation | Off-chain swap service (clearinghouse) | Fragmented collateral (breaks state model) |

---

*End of design document.*
