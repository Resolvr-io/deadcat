# Prediction Market: Terminal Paths from Dormant State

## Problem

A prediction market with zero outstanding token pairs (the state previously called "Dormant") has only two reissuance token UTXOs on-chain and no collateral. Currently, the only transition available from this state is issuance (minting new token pairs). There is no path to resolve, expire, or otherwise terminate the market.

This means:
- An abandoned market (created but never used) has RT UTXOs that sit on-chain forever
- A fully cancelled market (all pairs burned, returned to zero outstanding) cannot be cleaned up
- The oracle cannot attest on a market with no outstanding pairs
- The market can never reach a terminal state without first issuing tokens

The design principle: **the same terminal states should be reachable whether or not there are outstanding token pairs.** A market with 1000 pairs and a market with 0 pairs should both be resolvable by the oracle and expirable by timelock.

## Proposed Changes

Add two new spend paths to the Dormant RT slot covenant programs, mirroring the resolution and expiry paths that exist for the Unresolved state.

### 1. Oracle Resolution from Zero-Pair State

**Authorization**: Oracle BIP-340 Schnorr signature on `SHA256(market_id || outcome_byte)` — identical to the existing resolution path from Unresolved.

**Constraints**:
- Both RT UTXOs (YES RT and NO RT) must be consumed atomically in the same transaction
- No new covenant outputs produced (both RTs extinguished)
- Oracle signature verified against `oracle_public_key` from market params

**Result**: Market transitions directly to `ResolvedYes { outstanding_pairs: 0 }` or `ResolvedNo { outstanding_pairs: 0 }` depending on the attestation — immediately terminal.

**Why this is safe**: There are no outstanding tokens, so there is nothing to redeem. The outcome is recorded in the market's state variant for historical reference — "this market resolved YES even though no one had open positions."

### 2. Expiry from Zero-Pair State

**Authorization**: Timelock — the transaction's timelock must be ≥ `expiry_time` from market params. Same mechanism as the existing Unresolved → Expired transition.

**Constraints**:
- Both RT UTXOs (YES RT and NO RT) must be consumed atomically in the same transaction
- No new covenant outputs produced (both RTs extinguished)
- Timelock validated against `expiry_time` parameter

**Result**: Market transitions directly to `Expired { outstanding_pairs: 0 }` — immediately terminal.

**Why this is safe**: The expiry time has passed, no tokens are outstanding, and no collateral is locked. The market is dead — this just formalizes it on-chain.

## Impact on deadcat-core

### State Machine

The market state machine gains two new transitions:

| From | To | Trigger | Condition |
|---|---|---|---|
| Trading (0 pairs) | ResolvedYes (0 pairs) | Oracle YES attestation | outstanding_pairs == 0 |
| Trading (0 pairs) | ResolvedNo (0 pairs) | Oracle NO attestation | outstanding_pairs == 0 |
| Trading (0 pairs) | Expired (0 pairs) | Timelock passed | outstanding_pairs == 0 |

These are in addition to the existing transitions from Trading with outstanding pairs (which go through ResolvedYes/ResolvedNo/Expired phases with collateral, becoming terminal when outstanding_pairs reaches 0 through redemption).

### PSET Builders

No new PSET builder methods needed. The existing builders handle this:

- `build_oracle_resolve_pset`: Engine checks outstanding pairs. If > 0, builds the standard Unresolved → Resolved PSET. If == 0, builds the zero-pair resolution PSET (consumes only RT UTXOs, no collateral input).
- `build_expire_transition_pset`: Same branching. If outstanding pairs > 0, standard Unresolved → Expired. If == 0, zero-pair expiry (consumes only RT UTXOs).

The caller doesn't need to know which path is taken — the engine determines it from the contract's current state.

### State Advancement

`process_transaction` identifies these transitions by: both RT outpoints spent + no new covenant outputs + market had zero outstanding pairs. Since all three dormant terminal paths produce identical observable outputs (no covenant outputs), the engine uses **witness-based path detection** — extracting the Simplicity program and witness bytes from the spending transaction's witness stack and calling `RedeemNode::decode` to determine which spend path was taken. This yields `MarketTransition::Resolved { outcome: Side }` or `MarketTransition::Expired` as appropriate. See the main design doc's [Detection Strategy and Robustness](../../architecture/deadcat-core-design.md#detection-strategy-and-robustness) section.

### Transition Details

From the public API perspective, these transitions look the same as their non-zero-pair counterparts:

- `MarketTransition::Resolved { outcome: Side }` — whether triggered from zero or non-zero pairs
- `MarketTransition::Expired` — same

The distinction (zero pairs vs non-zero pairs) is an internal routing detail for PSET construction, not a public API concern. This is consistent with hiding the Dormant/Unresolved distinction.

## Covenant Changes

### Affected Slots

Both Dormant RT slots need new spend paths:

- **DormantYesRt (slot 0)**: Add oracle resolution path + expiry path
- **DormantNoRt (slot 1)**: Add oracle resolution path + expiry path

Both paths require atomic consumption of BOTH RT UTXOs (co-membership enforcement, same pattern used by full cancellation).

### Signature Domain Strings

The oracle resolution from zero-pair state uses the same oracle signature as resolution from Unresolved — the oracle signs the same BIP-340 tagged hash message (`tagged_hash("deadcat/oracle_attestation", market_id || outcome_byte)`) regardless of the market's pair count. No new domain string needed. See [oracle-bip340-tagged-hash.md](../../protocol/oracle-bip340-tagged-hash.md).

### Legacy Source Touchpoints

These are the current `deadcat-sdk` files where this legacy-source delta exists today. The `deadcat-core` implementation should realize the same behavior in its new market contract modules.

- `src-tauri/crates/deadcat-sdk/contract/prediction_market.simf` — add resolution and expiry paths to Dormant RT slot programs
