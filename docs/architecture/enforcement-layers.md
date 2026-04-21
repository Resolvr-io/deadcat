# Enforcement Layers

## Purpose

Deadcat's security properties are enforced across multiple independent layers — Elements consensus, Simplicity covenants, taproot structure, and application conventions. Each layer has different capabilities and limitations. Security analysis that considers only one layer can miss attacks that exploit mechanisms at a different layer.

This document defines the layers, maps each security property to its enforcement layer(s), and provides a methodology for analyzing new properties. It exists because of a concrete lesson: the RT burn enforcement on resolution/expiry was initially dismissed as "unnecessary plumbing" when reasoning only at the Simplicity covenant layer, but is in fact security-critical because Elements consensus has a reissuance mechanism that operates below the covenant.

## The Four Layers

### Layer 1: Elements Consensus

The base protocol layer. Enforced by all validating nodes. A transaction that violates Layer 1 rules is rejected at the network level — it cannot be included in a block.

**Mechanisms:**
- **Per-asset value balance**: For each asset ID, the sum of input values must equal the sum of output values plus fees. No asset can be inflated or destroyed without explicit issuance/burn mechanisms.
- **Pedersen commitment balance**: `sum(input_cbf) = sum(output_cbf)` where `cbf = value × abf + vbf`. Ensures confidential transactions preserve value without revealing amounts.
- **Confidential output requirement**: If at least one input is confidential, at least one output must also be confidential. This is a structural rule — there must be a free variable (a confidential output's VBF) to absorb the input blinding factors.
- **Reissuance authorization**: Any input spending a UTXO that contains a reissuance token can trigger reissuance (minting new tokens of the original asset) by providing the RT UTXO's ABF in the `assetBlindingNonce` field. This mechanism is entirely independent of script validation.
- **Issuance on any input**: Any transaction input can carry issuance fields (`nAmount`, `nInflationKeys`) to mint new tokens or create new reissuance tokens. This is also independent of script validation.
- **Timelock enforcement**: `nLockTime` is checked by block validation — a transaction with a future locktime cannot be included in a block before that height/time.
- **Range proofs**: Prove that confidential output values lie in [0, 2^52 - 1], preventing negative values that would break the value balance.
- **Surjection proofs**: Prove that each confidential output's asset tag matches at least one input's asset tag, preventing cross-asset inflation.

**Key property**: Layer 1 mechanisms operate unconditionally. They do not know about Simplicity covenants, taproot structure, or application conventions. A mechanism like reissuance works the same way whether the RT is at a covenant address or a wallet address.

### Layer 2: Simplicity Covenant (Per-Input)

The contract logic layer. A Simplicity program executes when a covenant UTXO is spent. The program introspects the spending transaction (inputs, outputs, scripts, assets, values, commitments, issuance data) and either succeeds (spend is valid) or fails (transaction is rejected).

**Capabilities:**
- Verify signatures (BIP-340 Schnorr via jets)
- Introspect any input or output (script, asset, value, commitment)
- Verify Pedersen commitments against expected blinding factors
- Check issuance data on inputs (`issuance_asset_amount`, `issuance_token_amount`)
- Enforce timelocks (via `check_lock_height` / `check_lock_time` jets)
- Constrain the number of outputs (`num_outputs()`)

**Limitations:**
- **Only runs when a covenant UTXO is spent.** If an RT escapes to a wallet address, the covenant never executes on it again. The wallet owner can spend it freely, including for Elements-level reissuance.
- **Per-input execution.** Each input runs its own program independently. Cross-input coordination requires each program to introspect the other inputs' properties (co-membership checks).
- **Cannot enforce transaction-level properties directly.** The covenant can check individual outputs but cannot enforce properties like "the total number of RT tokens across all outputs is exactly 2." It enforces per-output constraints that collectively achieve transaction-level invariants.
- **Cannot prevent future transactions.** The covenant can constrain the current spend but cannot prevent what happens to non-covenant outputs in future transactions.

### Layer 3: Taproot Structure

The Bitcoin/Elements script framework layer. Taproot's key-spend vs script-spend distinction provides structural properties independent of the Simplicity program content.

**Mechanisms:**
- **NUMS internal key**: Markets and pools use a Nothing Up My Sleeve point as the taproot internal key, making key-spend cryptographically unspendable. All spends must go through the script path (Simplicity covenant).
- **Real internal key**: Maker orders use the maker's public key as the internal key. Key-spend is available for cancellation — no covenant constraints, the maker reclaims funds freely.
- **Witness structure**: Key-spend produces a single 64-byte witness element. Script-spend produces multiple elements (witness data, program bytes, control block). This structural difference enables transition detection without Simplicity witness decoding.

**Key property**: Taproot structure determines WHO can spend and WHAT detection mechanisms are available, independent of the Simplicity program logic.

### Layer 4: Application Convention (deadcat-core)

The builder/application layer. Conventions enforced by `deadcat-core` during PSET construction, parameter derivation, and contract ingestion. NOT enforced on-chain — a custom tool can violate these conventions.

**Mechanisms:**
- OP_RETURN recovery hints (chain-only recovery)
- Standard denomination conventions (shared 16-value 1-2-5 table for market `base_payout` and pool `max_loss_sats` / `half_payout_sats`)
- Deterministic key and nonce derivation
- Output layout conventions for multi-covenant transactions
- Convention validation in derive functions and PSET builders
- Market ingestion rejection of non-conforming markets

**Key property**: Layer 4 conventions protect users who use `deadcat-core` but cannot protect against transactions built by custom tools. Security properties MUST NOT depend solely on Layer 4 — they must be enforced at Layer 1, 2, or 3.

## Elements Issuance and Reissuance Mechanics

This section documents the Elements issuance model in detail, because the interaction between issuance, reissuance, and Simplicity covenants is a recurring source of cross-layer reasoning errors.

### Initial Issuance vs Reissuance

Elements has two types of issuance, distinguished by the `assetBlindingNonce` field on the input:

| | Initial issuance | Reissuance |
|---|---|---|
| `assetBlindingNonce` | `0x00...00` (null) | ABF of the RT UTXO being spent (non-null) |
| `assetEntropy` | Contract hash (chosen by creator) | Asset entropy (from original issuance) |
| `nAmount` | Tokens to mint | New tokens to mint |
| `nInflationKeys` | Reissuance tokens to create | **Must be null** (consensus rule) |
| Creates new asset? | Yes | No — reissues existing asset |
| Creates new RTs? | Yes (if `nInflationKeys > 0`) | **No — consensus rejects non-null `nInflationKeys`** |

### RT Supply is Fixed at Creation

The Elements consensus rule (`src/confidential_validation.cpp:270-278`) explicitly rejects any transaction where `nInflationKeys` is non-null AND `assetBlindingNonce` is non-null (reissuance):

```cpp
if (!issuance.nInflationKeys.IsNull()) {
    if (!issuance.assetBlindingNonce.IsNull()) {
        return false;  // Consensus rejection
    }
}
```

This means: **the total supply of reissuance tokens for an asset is permanently fixed at initial issuance time.** No additional RTs can ever be created through any transaction. The only way to reduce RT supply is to burn them (send to an unspendable script).

### Burn Script: OP_RETURN Over P2WSH

Burn outputs use bare OP_RETURN (`0x6a`) rather than the alternative P2WSH-with-all-zero-hash approach. Both are unspendable, but OP_RETURN is superior for two reasons:

1. **Consensus-level unspendability**: OP_RETURN is unspendable by consensus rule — absolute, not dependent on computational assumptions. P2WSH(0x00...00) relies on SHA256 preimage resistance (computationally secure but not consensus-enforced).
2. **UTXO set hygiene**: Nodes prune OP_RETURN outputs from the UTXO set (they're known-unspendable). P2WSH burn outputs remain in the UTXO set forever because nodes can't determine they're unspendable.

**Blinded OP_RETURN is supported on Elements.** RT burn outputs must be blinded (the covenant verifies deterministic blinding factors). Elements handles this correctly: `blind.cpp` sets `min_value = 0` for unspendable scripts, `blind_tests.cpp` explicitly tests blinded OP_RETURN outputs, and `VerifyAmounts` processes blinded OP_RETURN identically to other blinded outputs. The fee mechanism already depends on explicit non-zero-value OP_RETURN (every transaction's fee output is an OP_RETURN with the fee amount).

### Implications for Deadcat

1. **A market creator who creates exactly 1 YES RT and 1 NO RT in the creation transaction has permanently fixed the RT supply.** No covenant enforcement is needed to prevent additional RT creation — Elements consensus handles it.

2. **The covenant's job is to keep existing RTs locked**, not to prevent new ones. The RT burn enforcement on resolution/expiry prevents RTs from escaping to wallet addresses where they could be used for unauthorized reissuance. The `ensure_no_issuance` calls on non-issuance paths prevent parasitic TOKEN minting (not RT minting — that's already impossible).

3. **Deterministic blinding makes ABFs public**, which means the traditional Elements safeguard for reissuance (ABF secrecy) is absent. Covenant-enforced burns are the sole protection against unauthorized reissuance — anyone who holds an RT and knows the ABF can reissue. See [deterministic-rt-blinding.md](../protocol/deterministic-rt-blinding.md).

4. **`ensure_no_issuance` on non-issuance paths prevents parasitic token minting.** Even though additional RTs can't be created, a malicious builder could attach `nAmount` (token minting) to a non-issuance covenant spend. The `ensure_no_issuance` check prevents this by verifying that all three issuance fields are null on non-issuance inputs.

## Security Properties by Enforcement Layer

Each row maps a security property to the layer(s) that enforce it and notes any cross-layer dependencies.

| Property | Primary enforcement | Cross-layer dependency | What goes wrong without it |
|---|---|---|---|
| **Token supply = collateral / cp** (where `cp = base_payout × N`) | L2: covenant checks collateral on issuance | L2 must also enforce RT burns (see below) and `ensure_no_issuance` on non-issuance paths, because L1 reissuance/issuance mechanisms bypass L2 | Unbacked tokens dilute legitimate holders |
| **RT destruction on terminal transitions** | L2: `ensure_blinded_reissuance_burn_output` verifies burn script + commitment | Required because L1 reissuance uses RT + ABF (public with deterministic blinding) — if RTs escape to wallet addresses, L1 reissuance bypasses L2 entirely | Attacker mints unbacked tokens via Elements reissuance |
| **No parasitic issuance** | L2: `ensure_no_issuance` on every covenant input for non-issuance paths | L1 allows issuance fields on any input — without L2 checks, a builder could attach issuance to a resolution/swap/fill spend | Attacker mints tokens alongside a legitimate covenant transition |
| **Oracle-only resolution** | L2: BIP-340 signature verification against `ORACLE_PUBLIC_KEY` | L1 provides the Schnorr verification primitive (secp256k1 jet) | Anyone can resolve markets, stealing from token holders |
| **Collateral conservation** | L2: covenant checks `collateral = pairs × cp` (where `cp = base_payout × N`) | None — purely L2 | Issue tokens without backing |
| **Correct redemption rates** | L2: covenant enforces half-value (expired) or full-value (resolved) | None — purely L2 | Expired-market holders redeem at full value |
| **Deterministic RT blinding** | L2: covenant verifies commitments match deterministic ABF + CBF pass-through | L1 Pedersen balance constrains transaction structure (need confidential outputs for confidential inputs). L2 enforcement is the griefing defense — without it, L4 convention alone is insufficient | Malicious issuer locks the market for all participants |
| **Swap pricing integrity** | L2: Merkle proofs for F(old_s) and F(new_s), conservation equation | None — purely L2 | Extract more tokens than the LMSR curve allows |
| **Pool reserve minimums** | L2: `MIN_POOL_RESERVE` checked for swap and admin paths | None — purely L2 | Drain pool to dust |
| **Correct trade direction** | L2: s_index direction matches trade direction | None — purely L2 | Get a better price by moving s_index the wrong way |
| **Maker payment on fill** | L2: output value ≥ fill_amount × PRICE with correct asset | None — purely L2 | Fill an order without paying the maker |
| **Order remainder integrity** | L2: remainder output has covenant script, correct asset, min_remainder floor | None — purely L2 | Steal locked tokens from a partially-filled order |
| **Asset identity on all outputs** | L2: all covenants verify output asset IDs | None — purely L2 | Substitute one asset for another at the same value |
| **Admin-only pool operations** | L2: BIP-340 signature from `ADMIN_PUBKEY` | L1 provides the Schnorr verification primitive | Unauthorized pool adjustment or closure |
| **Maker-only cancellation** | L3: taproot key-spend requires maker's private key | L3 NUMS impossibility ensures markets/pools can't be key-spent. L3 real key on orders ensures only maker can cancel | Anyone cancels orders, stealing locked tokens |
| **No double resolution** | L2: resolution consumes RT UTXOs (state machine has no path back) + L2: RT burns prevent reissuance | L1 reissuance is the mechanism that could theoretically "undo" RT consumption — burn enforcement prevents this | Resolve a market twice, profit from both outcomes |
| **Timelock-enforced expiry** | L1: `nLockTime` consensus rule + L2: `check_lock_height` jet | L2 delegates to L1 — the jet checks the transaction's locktime, and L1 ensures the locktime is respected | Expire a market early, forcing half-value redemptions |
| **Pedersen balance for RT transactions** | L1: commitment balance equation | L2 must produce correctly-blinded burn/continuation outputs. L4 (`finalize`/`prepare`) must construct transactions that satisfy L1's confidential output requirement | Invalid transaction (consensus rejection) |
| **Recovery from mnemonic** | L4: OP_RETURN hints, deterministic derivation, convention enforcement | L4 only — no on-chain enforcement. Non-conforming contracts from custom tools are not recoverable | Lost funds after wallet restore |
| **Output aliasing prevention** | L2: script uniqueness (different contracts → different scripts) + L2: structural separation (positional output refs) | None — purely L2 (different contracts' covenants independently verify their outputs) | Two covenants both claim the same output, one contract's state is lost |
| **Collateral UTXO authenticity** | L2: sibling UTXO check — all three covenant inputs (YES RT, NO RT, collateral) must share the same `prev_txid`, proving they were created in the same transaction | L1 allows anyone to send value to a covenant script address, creating duplicate UTXOs at the same script. Without the sibling check, L2 cannot distinguish canonical from fake collateral. Partial cancellation must co-spend RTs to maintain the sibling invariant across all transitions. | Attacker creates a fake collateral UTXO, co-spends it with real RTs during issuance, orphaning the real collateral (inaccessible through normal resolution/redemption) |

## Cross-Layer Gotchas

These are the patterns where single-layer reasoning leads to incorrect conclusions. Each is a concrete lesson.

### Gotcha 1: Elements Reissuance Bypasses Covenants

**The trap**: "RTs are worthless outside the covenant — once the market resolves, nobody can use them." This is true at Layer 2 (no covenant operation accepts escaped RTs). It is false at Layer 1 (Elements reissuance uses RTs as authorization, independent of covenants).

**Made worse by**: Deterministic blinding. Traditional Elements RT security relies on ABF secrecy. Deterministic blinding makes ABFs public, removing the traditional safeguard and making covenant-enforced burns the sole protection.

**The fix**: `ensure_blinded_reissuance_burn_output` on all resolution/expiry paths.

**The principle**: When reasoning about what happens to assets that leave the covenant's control, consider ALL Layer 1 mechanisms that operate on those assets — not just the covenant-level state machine.

### Gotcha 2: Issuance Fields on Any Input

**The trap**: "The issuance path is the only way to mint tokens." This is true at Layer 2 (only the issuance covenant path creates tokens). It is false at Layer 1 (any transaction input can carry issuance fields).

**The fix**: `ensure_no_issuance` on every covenant input for non-issuance spend paths.

**The principle**: Layer 1 attaches capabilities to transaction inputs that the covenant must explicitly opt out of. If the covenant doesn't check, the capability is available to the transaction builder.

### Gotcha 3: Confidential Output Requirement

**The trap**: "Just make all outputs explicit for the regtest/`finalize()` path." This ignores the Layer 1 consensus rule that confidential inputs require at least one confidential output.

**The fix**: RT burn outputs (or continuation outputs) are always blinded, satisfying the rule. The `finalize()` path blinds them deterministically; the `prepare()` path lets `blind_last` handle them.

**The principle**: Layer 1 structural rules constrain transaction shape. Layer 4 builder logic must satisfy these constraints even when the application would prefer simpler output construction.

### Gotcha 4: NUMS Unspendability is Layer 3, Not Layer 2

**The trap**: "The covenant enforces all market/pool spend rules." The covenant only runs on script-spend. Key-spend bypass is prevented by NUMS at Layer 3, not by the Simplicity program.

**Not currently a problem**: Markets and pools correctly use NUMS. But if a future contract accidentally used a real internal key, the covenant would be bypassable via key-spend — and no Layer 2 analysis would catch this.

**The principle**: The covenant's authority depends on Layer 3 (taproot) preventing alternative spend paths. Verify the internal key choice for each contract type.

### Gotcha 5: Covenant Scripts Are Not Unique Per UTXO

**The trap**: "The collateral is at the Unresolved Collateral script, so it must be the real collateral." The covenant script hash is derived from market params + slot type — it's the same for ALL UTXOs at that slot. Anyone can create a UTXO at a covenant script address by simply sending value to it.

**The attack**: An attacker sends a tiny amount of L-BTC to the Unresolved Collateral script address (derivable from public market params). They then build a subsequent issuance transaction co-spending this fake collateral with the real RT UTXOs. The covenant checks the script hash (matches), reads the old collateral amount (tiny), and accepts the transition. The real collateral UTXO is orphaned — the RTs have moved on to new outpoints associated with the fake collateral. The orphaned collateral cannot be resolved or expired (those paths require RT co-membership). It can only be drained via partial cancellation (burning equal YES + NO token pairs), which is impractical after market resolution.

**The fix**: The sibling UTXO check. All transitions that co-spend RTs and collateral verify that the three covenant inputs share the same `prev_txid` — i.e., they were created in the same transaction. A fake collateral UTXO would have a different `prev_txid` (it was created in the attacker's funding transaction, not in the last market transition).

**Why partial cancellation must co-spend RTs**: The current `.simf` has partial cancellation spending only the collateral slot (no RT co-membership). This is the lightest path but it breaks the sibling invariant. After a collateral-only partial cancellation:
- RTs remain at outpoints created in transaction A (the last issuance)
- New collateral is created in transaction B (the cancellation)
- The next co-spending operation (subsequent issuance, resolution, expiry) sees: `RT.prev_txid = A`, `collateral.prev_txid = B` → sibling check fails

If partial cancellation co-spends the RTs (producing new RT outputs at the same scripts with deterministic blinding), all three outputs are born in transaction B, and the sibling invariant is restored. The cost: partial cancellation becomes as complex as issuance (3 covenant inputs, RT blinding in witness). The benefit: the sibling invariant holds across the entire market lifecycle.

**The principle**: When a covenant slot has a static script (same script regardless of state), any UTXO at that script is interchangeable from the covenant's perspective. If interchangeability is dangerous, the covenant needs a UTXO identity check beyond script hash matching. The `prev_txid` sibling check provides this without encoding state in the script.

**Contrast with pools**: The LMSR pool doesn't have this vulnerability because the pool script encodes the `s_index` — each state has a unique script. An attacker cannot create a UTXO at the correct script for the current state without knowing the current `s_index` and performing the full taproot tweak computation. Even then, the pool's co-membership check verifies all three inputs share the same script, and the pool would need the s_index to be in the tapdata leaf.

## Analysis Checklist for New Properties

When adding a new security property or modifying an existing one:

1. **Identify the invariant.** State the property precisely. "Token supply is collateral-backed" is better than "issuance is secure."

2. **For each layer, ask: does this layer have a mechanism that could VIOLATE the invariant?**
   - L1: Does Elements consensus have a mechanism (reissuance, issuance, value transfer) that could break it?
   - L2: Could a malicious covenant witness satisfy the program while violating the intent?
   - L3: Could key-spend bypass the covenant and violate the property?
   - L4: Is the property only enforced by application convention? If so, it's NOT a security property — it's a convention.

3. **For each identified mechanism, ask: is it defended against?**
   - If L1 has a mechanism, does L2 explicitly defend? (e.g., `ensure_no_issuance`, burn enforcement)
   - If L3 could bypass, is the internal key NUMS?
   - If L4 is the only enforcement, escalate to L2 or accept the limitation.

4. **Consider what happens to assets that LEAVE the covenant's control.** Burn outputs, wallet outputs, fee outputs — the covenant can constrain them at spend time, but cannot control what happens to them in future transactions. If an asset has Layer 1 capabilities (like reissuance authorization), the covenant must ensure it goes somewhere safe (burn script) or loses those capabilities.

5. **Document the cross-layer dependencies** in this document's security properties table.

## Key Files

- `src-tauri/crates/deadcat-sdk/contract/prediction_market.simf` — `ensure_blinded_reissuance_burn_output`, `ensure_no_issuance`, `verify_token_commitment`
- `src-tauri/crates/deadcat-sdk/contract/lmsr_pool.simf` — pool covenant (swap, admin, close paths)
- `src-tauri/crates/deadcat-sdk/contract/maker_order.simf` — order covenant (fill path only)
- `docs/protocol/deterministic-rt-blinding.md` — RT blinding scheme and covenant enforcement
- `docs/contracts/contract-specification.md` — spend paths and covenant constraints
- `docs/architecture/deadcat-core-design.md` — security model section, covenant-enforced properties table
