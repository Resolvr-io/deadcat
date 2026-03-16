# Binary Prediction Market Covenant Smart Contract

**Design Document**

SimplicityHL on Liquid · Resolvr Inc. · March 2026

---

## 1. Overview

This document describes the current binary prediction market covenant implemented in `prediction_market.simf`.

The contract uses:

- a five-state lifecycle model for user-visible market status,
- an eight-slot covenant identity model for on-chain UTXOs,
- deterministic YES/NO asset issuance via Liquid reissuance tokens,
- a single collateral UTXO while the market is unresolved or terminal,
- explicit state transitions for oracle resolution, expiry, issuance, cancellation, and redemption.

The current design is slot-based. Lifecycle state still matters, but it is no longer sufficient to identify a covenant UTXO by itself. Every covenant UTXO is identified by a `MarketSlot`, and the Taproot address commits that slot directly.

## 2. Contract Parameters

The contract is parameterized at compile time with eight values. A ninth value, `MARKET_ID`, is derived deterministically from the YES and NO asset IDs.

| Parameter | Description |
|-----------|-------------|
| `ORACLE_PUBLIC_KEY` | X-only Schnorr pubkey used for oracle attestation verification. |
| `COLLATERAL_ASSET_ID` | Asset ID of the collateral asset, typically L-BTC. |
| `YES_TOKEN_ASSET` | Deterministic asset ID for YES outcome tokens. |
| `NO_TOKEN_ASSET` | Deterministic asset ID for NO outcome tokens. |
| `YES_REISSUANCE_TOKEN` | Reissuance token required to mint additional YES tokens. |
| `NO_REISSUANCE_TOKEN` | Reissuance token required to mint additional NO tokens. |
| `COLLATERAL_PER_TOKEN` | Satoshis backing each individual token. |
| `EXPIRY_TIME` | Block height used by expire-transition and expiry-redemption locktime checks. |

Derived value:

- `MARKET_ID = SHA256(YES_TOKEN_ASSET || NO_TOKEN_ASSET)`

## 3. Lifecycle State vs Covenant Slot

The implementation distinguishes between:

- `MarketState`: the five user-visible lifecycle states,
- `MarketSlot`: the eight concrete covenant identities that appear on chain.

### 3.1 Lifecycle States

| Value | State | Description |
|-------|-------|-------------|
| 0 | `Dormant` | Reissuance tokens exist, but no collateral has been deposited yet. |
| 1 | `Unresolved` | The market is live. Collateral exists and new issuance/cancellation is possible. |
| 2 | `ResolvedYes` | Oracle committed YES. Only resolved collateral remains. |
| 3 | `ResolvedNo` | Oracle committed NO. Only resolved collateral remains. |
| 4 | `Expired` | Oracle was not used; the market was finalized via expiry. |

### 3.2 Slots

| Value | Slot | State | Role |
|-------|------|-------|------|
| 0 | `DormantYesRt` | `Dormant` | YES reissuance token |
| 1 | `DormantNoRt` | `Dormant` | NO reissuance token |
| 2 | `UnresolvedYesRt` | `Unresolved` | YES reissuance token |
| 3 | `UnresolvedNoRt` | `Unresolved` | NO reissuance token |
| 4 | `UnresolvedCollateral` | `Unresolved` | collateral |
| 5 | `ResolvedYesCollateral` | `ResolvedYes` | collateral |
| 6 | `ResolvedNoCollateral` | `ResolvedNo` | collateral |
| 7 | `ExpiredCollateral` | `Expired` | collateral |

This means a market has eight covenant addresses, grouped as:

- `Dormant`: YES RT + NO RT
- `Unresolved`: YES RT + NO RT + collateral
- `ResolvedYes`: collateral only
- `ResolvedNo`: collateral only
- `Expired`: collateral only

### 3.3 Why the Slot Model Exists

The old state-address design allowed multiple roles to share the same covenant address inside one lifecycle state. That made it possible for the wrong covenant UTXO to masquerade as the right one on permissive multi-input paths.

The slot redesign fixes that class of problem by construction:

- the slot, not just the lifecycle state, is committed into Taproot metadata,
- every covenant path validates the exact slot it is allowed to spend,
- resolved and expired states do not carry reissuance-token covenant UTXOs at all.

## 4. Taproot Commitment Model

The covenant identity is committed via Taproot metadata.

- `MarketState` is still exposed at the SDK/API level.
- `MarketSlot` is what the contract commits and validates on chain.
- TapData for prediction-market slots is versioned as `[0x01, slot]`.

At address derivation time:

1. the Simplicity leaf is built from the program CMR,
2. a TapData leaf is built from the versioned slot encoding,
3. the two leaves are combined in a TapBranch,
4. the branch is tweaked onto the NUMS internal key,
5. the resulting P2TR output key defines the covenant address.

Every slot therefore has a distinct address even when multiple slots belong to the same lifecycle state.

## 5. Collateral and Token Economics

`COLLATERAL_PER_TOKEN` is the unit of account. Every issued pair consists of one YES token and one NO token, so issuance deposits:

- `pairs * 2 * COLLATERAL_PER_TOKEN`

Path payouts:

| Path | Formula |
|------|---------|
| Initial issuance | `pairs * 2 * CPT` deposited |
| Subsequent issuance | `pairs * 2 * CPT` added |
| Post-resolution redemption | `tokens * 2 * CPT` withdrawn |
| Expiry redemption | `tokens * CPT` withdrawn |
| Cancellation | `pairs * 2 * CPT` refunded |

Winning tokens redeem full pair collateral after resolution. Expiry redemption pays half that rate because both YES and NO sides redeem equally.

## 6. Live Slot Invariants

The implementation validates the proof-carrying dormant anchor, then derives
lifecycle state by walking canonical descendant transactions from that anchor.

Valid live sets are:

- `Dormant`: `DormantYesRt` + `DormantNoRt`
- `Unresolved`: `UnresolvedYesRt` + `UnresolvedNoRt` + `UnresolvedCollateral`
- `ResolvedYes`: `ResolvedYesCollateral`
- `ResolvedNo`: `ResolvedNoCollateral`
- `Expired`: `ExpiredCollateral`

Any partial or mixed live set is invalid. This rule is used by the SDK scan path and by `deadcat-store`.

## 7. Transaction Model

### 7.1 Creation

Market creation is a plain Elements transaction, not a covenant-validated spend.

It:

- consumes the two defining outpoints,
- uses native issuance to create the YES and NO reissuance tokens,
- deposits those reissuance tokens into `DormantYesRt` and `DormantNoRt`,
- does not mint YES/NO outcome tokens,
- does not deposit collateral.

### 7.2 Initial Issuance

Initial issuance transitions the market from `Dormant` to `Unresolved`.

Inputs:

- `DormantYesRt`
- `DormantNoRt`
- external collateral input
- fee input

Outputs:

- `UnresolvedYesRt`
- `UnresolvedNoRt`
- `UnresolvedCollateral`
- minted YES tokens
- minted NO tokens
- fee output

The YES and NO issuance amounts must match, and collateral must equal `pairs * 2 * CPT`.

### 7.3 Subsequent Issuance

Subsequent issuance keeps the market in `Unresolved`.

Inputs:

- `UnresolvedYesRt`
- `UnresolvedNoRt`
- `UnresolvedCollateral`
- external collateral input
- fee input

Outputs:

- `UnresolvedYesRt`
- `UnresolvedNoRt`
- consolidated `UnresolvedCollateral`
- newly minted YES tokens
- newly minted NO tokens
- fee output

The unresolved collateral UTXO is consumed and recreated with the old collateral plus the new deposit.

### 7.4 Oracle Resolve

Oracle resolution transitions from `Unresolved` to either `ResolvedYes` or `ResolvedNo`.

Inputs:

- `UnresolvedYesRt`
- `UnresolvedNoRt`
- `UnresolvedCollateral`
- fee input

Outputs:

- YES RT burn output
- NO RT burn output
- terminal collateral output at `ResolvedYesCollateral` or `ResolvedNoCollateral`
- fee output

The oracle signs `SHA256(MARKET_ID || outcome_byte)`, where `outcome_byte` is `0x01` for YES and `0x00` for NO.

Important property:

- reissuance tokens do **not** survive into terminal states,
- resolved states carry only collateral.

### 7.5 Expire Transition

Expiry finalization transitions from `Unresolved` to `Expired`.

Inputs:

- `UnresolvedYesRt`
- `UnresolvedNoRt`
- `UnresolvedCollateral`
- fee input

Outputs:

- YES RT burn output
- NO RT burn output
- `ExpiredCollateral`
- fee output

This path is gated by `check_lock_height(EXPIRY_TIME)`.

### 7.6 Post-Resolution Redemption

Post-resolution redemption spends terminal collateral only.

Inputs:

- `ResolvedYesCollateral` or `ResolvedNoCollateral`

Outputs:

- winning-token burn output
- payout output(s)
- remaining collateral at the same resolved collateral slot
- fee output

Reissuance-token covenant inputs are not part of this path.

### 7.7 Expiry Redemption

Expiry redemption also spends terminal collateral only.

Inputs:

- `ExpiredCollateral`

Outputs:

- YES or NO burn output
- payout output(s)
- remaining collateral at `ExpiredCollateral`
- fee output

This path is also gated by `check_lock_height(EXPIRY_TIME)`.

### 7.8 Cancellation

Cancellation is available only while the market is unresolved.

Partial cancellation:

- spends `UnresolvedCollateral` only,
- burns equal YES and NO amounts,
- recreates `UnresolvedCollateral` with the remaining collateral.

Full cancellation:

- spends `UnresolvedCollateral`, `UnresolvedYesRt`, and `UnresolvedNoRt`,
- burns equal YES and NO amounts,
- returns the RTs to `DormantYesRt` and `DormantNoRt`,
- returns collateral to external outputs,
- leaves the market in `Dormant`.

## 8. Spend Kinds and Witness Model

The old generic “secondary covenant input” path no longer exists.

Instead, each covenant input is satisfied with an explicit spend kind plus a `SLOT` witness. The current witness model includes the following path variants:

- initial issuance primary
- initial issuance secondary no-RT
- subsequent issuance primary
- subsequent issuance secondary no-RT
- subsequent issuance secondary collateral
- oracle resolve primary
- oracle resolve secondary no-RT
- oracle resolve secondary collateral
- post-resolution redemption
- expire transition primary
- expire transition secondary no-RT
- expire transition secondary collateral
- expiry redemption
- cancellation partial
- cancellation full primary
- cancellation full secondary YES RT
- cancellation full secondary NO RT

This design makes multi-input validation explicit:

- primary paths assert the exact primary slot,
- secondary paths assert both their own slot and the expected primary slot at input 0,
- non-issuance paths explicitly reject issuance fields,
- issuance is allowed only on the issuance paths.

## 9. Locktime Strategy

The contract uses `EXPIRY_TIME` only for post-threshold checks.

- `ExpireTransition` requires `lock_time >= EXPIRY_TIME`
- `ExpiryRedemption` requires `lock_time >= EXPIRY_TIME`

Other behaviors are governed by slot/state, not by attempting pre-expiry script guards.

## 10. Single-Collateral-UTXO Model

While a market is unresolved or terminal, there is exactly one live collateral covenant UTXO.

This remains a core design requirement:

- unresolved issuance must consume and recreate `UnresolvedCollateral`,
- oracle resolve must consume the entire unresolved collateral position at once,
- expire transition must consume the entire unresolved collateral position at once,
- resolved and expired states each carry exactly one collateral slot.

The slot redesign does not change this invariant; it strengthens the role separation around it.

## 11. Bootstrapping Sequence

1. Choose the YES and NO defining outpoints.
2. Compile the contract parameters. This yields the program CMR and eight slot addresses.
3. Build the creation transaction so the two RT UTXOs land at `DormantYesRt` and `DormantNoRt`.
4. Build the initial issuance transaction to consume the dormant RT slots, mint YES/NO tokens, and create the unresolved slot set.
5. While unresolved, anyone can issue more pairs, cancel, or wait for resolution/expiry.
6. Oracle resolution or expiry finalization consumes the unresolved slot set and leaves exactly one terminal collateral slot.
7. Redemption spends the terminal collateral slot until the market is fully drained.

## 12. Output Layout Notes

The key fixed layouts are:

- oracle resolve: 4 outputs = YES RT burn, NO RT burn, terminal collateral, fee
- expire transition: 4 outputs = YES RT burn, NO RT burn, expired collateral, fee

Issuance and full cancellation also have fixed covenant-role layouts, but may have additional non-covenant outputs such as token recipients, collateral refund recipients, or change.

Redemption and partial cancellation have variable output counts; the fee output is the last output in those flows.

## 13. Store and Scan Semantics

The SDK and `deadcat-store` no longer derive market state from raw slot-address
occupancy alone.

Instead, they start from the proof-carrying dormant anchor:

- `creation_txid`
- YES dormant output opening
- NO dormant output opening

The creation tx is validated against that anchor, then the scanner walks only
canonical descendant transactions of the validated dormant bundle.

This is important for correctness:

- foreign dust at slot scripts is ignored because it is not descended from the
  validated anchor,
- malformed or conflicting canonical transitions are rejected during the
  lineage walk,
- storage persists only canonical live slot outpoints and tags them by slot,
  not only by lifecycle state.

## 14. Summary

The current prediction-market covenant is a slot-based redesign of the earlier state-address model.

The important properties are:

- five lifecycle states remain the user-facing model,
- eight Taproot-committed slots identify concrete covenant UTXOs,
- issuance and full cancellation move RTs between dormant and unresolved slots,
- resolve/expire burn RTs and leave only terminal collateral,
- redemption and partial cancellation are collateral-slot-only flows,
- state is derived from canonical anchor lineage, not from loose address matching
  or raw slot occupancy.

This is the canonical architecture implemented by the current SDK, contract, and store.
