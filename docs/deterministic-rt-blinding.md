# Deterministic RT Blinding: Deadcat Implementation Plan

For the general problem statement, protocol constraints, and security analysis, see [Deterministic Reissuance Token Blinding for Simplicity Covenants](simplicity-deterministic-reissuance-blinding.md). This document covers the deadcat-specific implementation details, assuming the `elements` crate's PSET blinding API has NOT been expanded to support predetermined blinding factors — all confidential output construction for RT outputs must be hand-rolled.

## Motivation: Griefing Attack Prevention

Deterministic RT blinding is NOT just an application convention — it MUST be enforced at the covenant level. Without covenant enforcement, a malicious third party can grief any prediction market:

1. Issuance is permissionless — anyone who provides collateral can issue token pairs
2. The attacker computes the current RT outputs' deterministic ABFs (public data), performs a valid issuance
3. The attacker uses **non-deterministic** blinding factors for the new RT outputs
4. Now nobody else knows the ABFs/VBFs of the current RT outputs
5. Every remaining market transition (resolution, expiry, further issuance, cancellation) requires spending the RT UTXOs as inputs — which requires knowing the VBFs for transaction-level Pedersen commitment balancing
6. The market is locked. Token holders cannot redeem. Only the attacker can advance the market.

The covenant must enforce that new RT outputs use deterministic blinding, making the attack impossible regardless of who builds the transaction. See [Covenant Enforcement](#covenant-enforcement) below.

## Derivation Spec

### Creation Transaction (Initial RT Outputs)

The market creation transaction issues two reissuance tokens (YES and NO). Their blinding factors are derived via BIP-340-style tagged hashes from the defining outpoints:

```
YES_RT_ABF = tagged_hash("deadcat/rt_abf", yes_defining_outpoint)
NO_RT_ABF  = tagged_hash("deadcat/rt_abf", no_defining_outpoint)

YES_RT_VBF = tagged_hash("deadcat/rt_vbf", yes_defining_outpoint)
NO_RT_VBF  = tagged_hash("deadcat/rt_vbf", no_defining_outpoint)
```

Where:
- `tagged_hash(tag, data) = SHA256(SHA256(tag) || SHA256(tag) || data)` (BIP-340 convention)
- `yes_defining_outpoint` — serialized outpoint of input 0 of the creation transaction (YES defining UTXO)
- `no_defining_outpoint` — serialized outpoint of input 1 of the creation transaction (NO defining UTXO)

RT outputs always hold exactly 1 satoshi. Both ABF and VBF are publicly derivable.

### Combined Blinding Factor (CBF)

For RT outputs with value = 1, the Elements Pedersen commitment balance equation requires:

```
sum(cbf_outputs) = sum(cbf_inputs)
where cbf = v * abf + vbf = abf + vbf  (for v = 1)
```

The **combined blinding factor** `cbf = abf + vbf` (mod secp256k1 group order) is the quantity that must balance across inputs and outputs. This is the key insight for the cross-transition blinding scheme.

At creation time:
```
YES_RT_CBF = YES_RT_ABF + YES_RT_VBF (mod n)
NO_RT_CBF  = NO_RT_ABF  + NO_RT_VBF  (mod n)
```

### Subsequent Transitions (CBF Pass-Through)

For all transitions that produce new RT outputs (issuance, cancellation), the blinding scheme is:

- **ABF**: Independently deterministic per transition — derived from the input outpoint being consumed:
  ```
  out_abf = tagged_hash("deadcat/rt_abf", spent_rt_outpoint)
  ```
- **CBF**: Passed through unchanged from the input: `out_cbf = in_cbf`
- **VBF**: Implied: `out_vbf = out_cbf - out_abf` (mod n). Not stored or transmitted — computed by `deadcat-core` in Rust when needed for PSET construction.

The CBF is constant for the entire lifetime of each RT (YES and NO independently). It is set at creation time and never changes. The ABF changes on every transition (different input outpoint → different hash), and the VBF adjusts accordingly to maintain the same CBF.

**Why CBF pass-through self-balances**: Since `out_cbf = in_cbf` for both YES and NO RTs:
```
cbf_yes_out + cbf_no_out = cbf_yes_in + cbf_no_in  ✓ (identical values)
```

The RT portion of the blinding factor balance always holds, regardless of how many or few blinded wallet outputs the transaction has. The wallet's `blind_last` handles the wallet-side balance independently. This means:
- Transactions with **zero** blinded non-RT outputs work (the `finalize()` path)
- Transactions with **one or more** blinded non-RT outputs work (the `prepare()` path)

**Why VBF pass-through would NOT work**: A simpler scheme would pass through the VBF instead of the CBF. But the balance equation involves `cbf = abf + vbf`, not just `vbf`. Passing through VBFs while changing ABFs leaves the CBF sum unbalanced:
```
(new_abf_yes + old_vbf_yes) + (new_abf_no + old_vbf_no)
≠ (old_abf_yes + old_vbf_yes) + (old_abf_no + old_vbf_no)
```

### Recovery

To reconstruct blinding factors for any RT output in the market's history:

1. Find the market creation tx (via OP_RETURN or `issuance_transaction`)
2. Derive creation ABFs and VBFs from the defining outpoints (tagged hashes)
3. Compute `cbf = abf + vbf` (mod n) at creation time — this is constant forever
4. For any specific RT output: `abf = tagged_hash("deadcat/rt_abf", input_outpoint_that_created_it)` — derivable from chain
5. `vbf = cbf - abf` (mod n) — simple modular arithmetic in Rust

No witness parsing needed for VBF recovery. No chain of derivations to follow. One-shot CBF computation from creation data, then ABF + modular subtraction for any specific output.

## Issuance Token Asset ID

Deadcat markets issue with `nAmount = Null` (no asset minted, only RTs) and `blinded_issuance = 0x00` (unblinded). Therefore `nAmount.IsCommitment()` is false, and the RT asset ID uses `reissuance_token_from_entropy(entropy, false)` — i.e., `H(E || 1)`. This does not change with deterministic blinding — the issuance input fields remain the same; only the RT output blinding changes.

## Covenant Enforcement

The prediction market covenant already verifies RT output commitments using EC point arithmetic (lines 122-161 of `prediction_market.simf`). The existing `verify_token_commitment` function:

1. Accepts ABF and a second blinding factor as witness data
2. Computes the expected Pedersen commitments via `jet::hash_to_curve`, `jet::generate`, `jet::gej_ge_add`
3. Compares against the actual on-chain output commitments via `jet::output_asset` / `jet::output_amount`
4. Rejects the transaction if they don't match

### Required Changes

**Refactor `verify_token_commitment`** to accept `(ABF, CBF)` instead of `(ABF, VBF)`. The value commitment computation changes from `V = A' + vbf*G` to `V = H + cbf*G` (mathematically identical since `H + cbf*G = H + (abf+vbf)*G = (H + abf*G) + vbf*G = A' + vbf*G`):

```simplicity
fn verify_token_commitment(
    asset_commitment, amount_commitment, token_id,
    abf: u256,   // for asset: A' = H + abf*G
    cbf: u256    // for value: V = H + cbf*G  (was: V = A' + vbf*G)
) {
    let h_point: Ge = jet::hash_to_curve(token_id);

    // Asset commitment: A' = H + abf*G (unchanged)
    let abf_point: Gej = jet::generate(abf);
    let asset_gen: Gej = jet::gej_ge_add(abf_point, h_point);
    // ... verify asset_gen matches asset_commitment ...

    // Value commitment: V = H + cbf*G (was: A' + vbf*G — same point)
    let cbf_point: Gej = jet::generate(cbf);
    let value_gen: Gej = jet::gej_ge_add(cbf_point, h_point);
    // ... verify value_gen matches amount_commitment ...
}
```

**Enforce deterministic ABFs** in every spend path that produces new RT outputs. The output ABF is computed by the covenant from the input outpoint (no longer free witness data):

```simplicity
fn compute_deterministic_abf(input_index: u32) -> u256 {
    let (txid, vout): (u256, u32) = jet::input_prev_outpoint(input_index);
    let tag_hash: u256 = 0x...; // precomputed SHA256("deadcat/rt_abf")
    let ctx: Ctx8 = jet::sha_256_ctx_8_init();
    let ctx: Ctx8 = jet::sha_256_ctx_8_add_32(ctx, tag_hash);
    let ctx: Ctx8 = jet::sha_256_ctx_8_add_32(ctx, tag_hash);
    let ctx: Ctx8 = jet::sha_256_ctx_8_add_32(ctx, txid);
    let ctx: Ctx8 = jet::sha_256_ctx_8_add_4(ctx, vout);
    jet::sha_256_ctx_8_finalize(ctx)
}
```

**Enforce CBF pass-through** by using the verified input CBF directly as the output CBF:

```simplicity
// In issuance/cancellation paths:
// 1. Verify input RT commitments (ABF + CBF from witness)
verify_input_rt(0, YES_RT, yes_in_abf, yes_in_cbf);
verify_input_rt(1, NO_RT, no_in_abf, no_in_cbf);

// 2. Compute deterministic output ABFs
let yes_out_abf: u256 = compute_deterministic_abf(0);
let no_out_abf: u256 = compute_deterministic_abf(1);

// 3. CBF passes through — covenant enforces this
verify_output_rt(0, YES_RT, yes_out_abf, yes_in_cbf);  // cbf_out = cbf_in
verify_output_rt(1, NO_RT, no_out_abf, no_in_cbf);      // cbf_out = cbf_in
```

All required jets are already in use in the existing covenant: `input_prev_outpoint`, SHA256 context APIs, `generate`, `gej_ge_add`, `gej_normalize`, `eq_256`. No new Simplicity capabilities required.

### Witness Data Changes

The `BlindingQuad` type `(in_abf, in_vbf, out_abf, out_vbf)` shrinks. Output blinding factors are no longer free witness data — the covenant computes/passes them through:
- **Input ABF**: remains witness data (verified against input commitment)
- **Input CBF**: replaces input VBF as witness data (verified against input commitment)
- **Output ABF**: computed by covenant (deterministic tagged hash of input outpoint)
- **Output CBF**: passed through from input CBF (not in witness)

### Affected Spend Paths

**Transitions producing new RT continuation outputs** (covenant enforces deterministic ABFs + CBF pass-through on the continuation outputs):
- **Initial issuance** (Dormant → Unresolved)
- **Subsequent issuance** (Unresolved → Unresolved)
- **Partial cancellation** (Unresolved → Unresolved)
- **Full cancellation** (Unresolved → Dormant)

**Transitions consuming RTs without continuation** (covenant enforces RT burn outputs at the unspendable burn script with verified commitment):
- **Resolution** (Unresolved → ResolvedYes/ResolvedNo)
- **Expiry** (Unresolved → Expired)

These transitions use `ensure_blinded_reissuance_burn_output`, which verifies the output commitment matches the expected RT asset (same `verify_token_commitment` pattern) AND verifies the output script is the burn script. This is security-critical: deterministic blinding makes ABFs public, so the traditional Elements safeguard (ABF secrecy prevents unauthorized reissuance) is absent. Without covenant-enforced burns, a malicious transaction builder could redirect RT tokens to a wallet address and use the Elements consensus-level reissuance mechanism to mint unbacked tokens — bypassing the Simplicity covenant entirely.

**Dormant terminal transitions** (resolution/expiry from zero outstanding pairs) consume both DormantRT slots with no outputs. These are specified in [market-dormant-terminal-paths.md](market-dormant-terminal-paths.md) and will also require burn output enforcement — to be added when those paths are implemented.

### What the Covenant Does NOT Enforce

- **Creation-time blinding**: The covenant doesn't run during creation (the creation transaction CREATES the covenant UTXOs; the covenant first executes when those UTXOs are spent). Creation-time blinding is the market creator's responsibility via `deadcat-core`. A market creator who uses non-deterministic blinding breaks their own market — the threat model protects honest creators from malicious third parties, not creators from themselves.
- **Wallet output blinding**: Non-RT outputs (wallet change, fee, collateral, token destinations) are outside the covenant's scope. The wallet handles its own blinding independently.
- **Burn output blinding factor choice**: The covenant verifies burn output commitments using witness-provided ABF/VBF (same `verify_token_commitment` pattern), but the specific blinding factors on burn outputs are not required to follow the CBF pass-through scheme. Any valid commitment that matches the expected RT asset and value is accepted. In practice, `deadcat-core` uses CBF pass-through for burn outputs (same mechanism as continuation outputs) because it self-balances the Pedersen commitment equation without requiring a blinded wallet output — but this is an implementation choice, not a covenant constraint.

## Implementation: Hand-Rolled Confidential Outputs

Since `blind_last` and `blind_non_last` always generate random `AssetBlindingFactor` values and provide no option for predetermined factors, RT outputs must be blinded manually using `secp256k1-zkp` primitives.

### Creation PSET Builder

**Current flow** (`build_creation_pset` + blinding in `sdk.rs`):
1. Creates RT outputs as unblinded placeholders
2. Marks outputs 0, 1 with `blinding_key` for `blind_last`
3. `blind_last` generates random ABFs/VBFs, constructs Pedersen commitments, range proofs, surjection proofs

**New flow**:
1. Create RT outputs as unblinded placeholders (same as before)
2. **Do NOT set `blinding_key`** on outputs 0, 1 — exclude them from `blind_last`
3. Compute deterministic ABFs/VBFs from the defining outpoints via tagged hash
4. For each RT output, manually:
   - Construct the blinded asset generator: `secp256k1_generator_generate_blinded(token_asset_id, abf)`
   - Construct the value commitment: `secp256k1_pedersen_commit(vbf, value=1, generator)`
   - Generate a range proof: `secp256k1_rangeproof_sign(value=1, vbf, generator, ...)`
   - Generate a surjection proof from the transaction's input assets
   - Set `asset_comm`, `value_comm`, rangeproof, and surjection proof on the PSET output
5. Mark remaining wallet outputs (change, etc.) for `blind_last` as usual
6. Call `blind_last` — it handles only the marked outputs and balances VBFs for those independently

The codebase already supports selective blinding — outputs without `blinding_key` are untouched by `blind_last`. This pattern is used today for fee outputs (always explicit) and covenant reserve outputs (always explicit).

### Subsequent Transition PSET Builders (Issuance, Cancellation)

Same hand-rolled blinding for RT outputs, but the ABF is derived from the input outpoint (not the defining outpoint), and the VBF is computed as `cbf - abf` (mod n) where `cbf` is the creation-time CBF (constant, derivable from the defining outpoints).

### Issuance PSET Builder

**Current flow** (`compute_issuance_entropy` in `assembly.rs`):
1. Reads ABF from the anchor's `DormantOutputOpening`
2. Uses it as `blinding_nonce` in the reissuance input

**New flow**:
1. Compute ABF from the defining outpoint via tagged hash
2. Use it as `blinding_nonce` (same structure, different source)

### Market Scanning / Validation

**Current flow** (`validate_prediction_market_creation_tx` in `prediction_market_scan.rs`):
1. Receives anchor with blinding factors via Nostr
2. Reconstructs expected Pedersen commitments from anchor blinding factors
3. Verifies on-chain commitments match

**New flow**:
1. Derive blinding factors from defining outpoints (visible in creation tx inputs)
2. Same verification, just derived differently

### Nostr Announcement Format

**Current**: Includes `PredictionMarketAnchor` payload (creation_txid + 4 blinding factors)
**New**: Drops anchor entirely — only `PredictionMarketParams` + creation_txid needed. Discoverers derive everything from public on-chain data.

## Functions Affected

| Function | Current | After |
| -------- | ------- | ----- |
| `build_creation_pset` | Marks RT outputs for `blind_last` | Manually blinds RT outputs with deterministic ABFs/VBFs |
| `build_issuance_pset` | Marks RT outputs for `blind_last` | Manually blinds with deterministic ABF + CBF-derived VBF |
| `build_cancellation_pset` | Marks RT outputs for `blind_last` | Same as issuance |
| `recover_creation_anchor` | Extracts blinding factors from blinded outputs | **Eliminated** |
| `compute_issuance_entropy` | Takes ABFs from anchor | Derives ABFs from defining outpoints |
| `validate_prediction_market_creation_tx` | Uses anchor blinding factors for verification | Derives blinding factors from creation tx |
| Market Nostr announcement | Includes anchor payload | Drops anchor — only params + creation_txid |
| `ingest_market` (deadcat-core) | Takes `PredictionMarketParams` + `PredictionMarketAnchor` + `ChainTransaction` | Takes `PredictionMarketParams` + `ChainTransaction` only |
| `verify_token_commitment` (.simf) | Takes `(ABF, VBF)` from witness | Takes `(ABF, CBF)`, output ABF computed by covenant |
| Issuance/cancellation paths (.simf) | Output blinding from free witness data | Output ABF enforced deterministic, CBF passed through |

## Security Properties

- **Griefing prevention**: The covenant enforces deterministic ABFs and CBF pass-through. A malicious issuer cannot use non-deterministic blinding — the commitment check fails and the transaction is rejected.
- **Permissionless issuance preserved**: Anyone can compute the deterministic ABFs (public data) and provide the correct input witness data (uniquely determined by on-chain commitments). Issuance remains permissionless.
- **No privacy loss**: Both ABFs and VBFs were already publicly derivable (from public on-chain data). The covenant enforcement doesn't change the privacy model.
- **No constraint on wallet outputs**: The CBF pass-through self-balances the RT portion. Wallet outputs can be explicit or confidential — `blind_last` handles the wallet-side balance independently.

## Key Files

- `src-tauri/crates/deadcat-sdk/contract/prediction_market.simf` — `verify_token_commitment` refactor, deterministic ABF computation, CBF pass-through enforcement in issuance/cancellation paths
- `src-tauri/crates/deadcat-sdk/src/prediction_market/pset/creation.rs` — creation PSET builder
- `src-tauri/crates/deadcat-sdk/src/prediction_market/assembly.rs` — `compute_issuance_entropy()`, blinding
- `src-tauri/crates/deadcat-sdk/src/prediction_market/anchor.rs` — **to be removed entirely**
- `src-tauri/crates/deadcat-sdk/src/prediction_market_scan.rs` — market validation
- `src-tauri/crates/deadcat-sdk/src/sdk.rs` — market creation flow, anchor recovery
- `src-tauri/crates/deadcat-sdk/src/announcement.rs` — Nostr announcement format

## Impact on deadcat-core API

The anchor elimination simplifies the `ingest_market` API:

```rust
// Before: 3 parameters
pub fn ingest_market(
    &mut self,
    params: &PredictionMarketParams,
    anchor: PredictionMarketAnchor,
    creation_tx: &ChainTransaction,
) -> Result<ContractId, CoreError<S::Error>>;

// After: 2 parameters
pub fn ingest_market(
    &mut self,
    params: &PredictionMarketParams,
    creation_tx: &ChainTransaction,
) -> Result<ContractId, CoreError<S::Error>>;
```

`PredictionMarketParams` stays pure — only data needed to derive the contract's identity and addresses. No creation-time secrets. The `PredictionMarketAnchor` type and the `anchor` field on `Contract::PredictionMarket` are both eliminated. See [deadcat-core design doc](deadcat-core-design.md) for the full API.
