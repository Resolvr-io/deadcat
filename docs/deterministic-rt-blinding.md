# Deterministic RT Blinding: Deadcat Implementation Plan

For the general problem statement, protocol constraints, and security analysis, see [Deterministic Reissuance Token Blinding for Simplicity Covenants](simplicity-deterministic-reissuance-blinding.md). This document covers the deadcat-specific implementation details, assuming the `elements` crate's PSET blinding API has NOT been expanded to support predetermined blinding factors — all confidential output construction for RT outputs must be hand-rolled.

## Derivation Spec

Deadcat prediction market creation transactions issue two reissuance tokens (YES and NO). Their blinding factors are derived via BIP-340-style tagged hashes from the defining outpoints:

```
YES_RT_ABF = SHA256(SHA256("deadcat/rt_abf") || SHA256("deadcat/rt_abf") || yes_defining_outpoint)
NO_RT_ABF  = SHA256(SHA256("deadcat/rt_abf") || SHA256("deadcat/rt_abf") || no_defining_outpoint)

YES_RT_VBF = SHA256(SHA256("deadcat/rt_vbf") || SHA256("deadcat/rt_vbf") || yes_defining_outpoint)
NO_RT_VBF  = SHA256(SHA256("deadcat/rt_vbf") || SHA256("deadcat/rt_vbf") || no_defining_outpoint)
```

Where:
- `yes_defining_outpoint` — serialized outpoint of input 0 of the creation transaction (YES defining UTXO)
- `no_defining_outpoint` — serialized outpoint of input 1 of the creation transaction (NO defining UTXO)

RT outputs always hold exactly 1 satoshi. Both ABF and VBF are publicly derivable.

**VBF balancing caveat**: The last blinded output in a transaction must have its VBF adjusted to balance all blinding factors. If the RT outputs are blinded independently (before `blind_last` handles wallet outputs), the predetermined VBFs apply directly to the RT outputs and `blind_last` balances the remaining outputs via its own VBF adjustment. If no other outputs are blinded, the second RT output's VBF must be computed as the balancing factor rather than using the tagged hash derivation.

## Issuance Token Asset ID

Deadcat markets issue with `nAmount = Null` (no asset minted, only RTs) and `blinded_issuance = 0x00` (unblinded). Therefore `nAmount.IsCommitment()` is false, and the RT asset ID uses `reissuance_token_from_entropy(entropy, false)` — i.e., `H(E || 1)`. This does not change with deterministic blinding — the issuance input fields remain the same; only the RT output blinding changes.

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
| `build_creation_pset` | Marks RT outputs for `blind_last` | Manually blinds RT outputs with deterministic ABFs |
| `recover_creation_anchor` | Extracts blinding factors from blinded outputs | **Eliminated** |
| `compute_issuance_entropy` | Takes ABFs from anchor | Derives ABFs from defining outpoints |
| `validate_prediction_market_creation_tx` | Uses anchor blinding factors for verification | Derives blinding factors from creation tx |
| Market Nostr announcement | Includes anchor payload | Drops anchor — only params + creation_txid |
| `ingest_contract` (deadcat-core) | Takes `ContractParams` + `PredictionMarketAnchor` + `ChainTransaction` | Takes `ContractParams` + `ChainTransaction` only |

## Key Files

- `src-tauri/crates/deadcat-sdk/src/prediction_market/pset/creation.rs` — creation PSET builder
- `src-tauri/crates/deadcat-sdk/src/prediction_market/assembly.rs` — `compute_issuance_entropy()`, blinding
- `src-tauri/crates/deadcat-sdk/src/prediction_market/anchor.rs` — **to be removed entirely**
- `src-tauri/crates/deadcat-sdk/src/prediction_market_scan.rs` — market validation
- `src-tauri/crates/deadcat-sdk/src/sdk.rs` — market creation flow, anchor recovery
- `src-tauri/crates/deadcat-sdk/src/announcement.rs` — Nostr announcement format

## Impact on deadcat-core API

The anchor elimination simplifies the `ingest_contract` API:

```rust
// Before: 3 parameters
pub fn ingest_contract(
    &mut self,
    params: ContractParams,
    anchor: PredictionMarketAnchor,
    creation_tx: &ChainTransaction,
) -> Result<String, CoreError<S::Error>>;

// After: 2 parameters
pub fn ingest_contract(
    &mut self,
    params: ContractParams,
    creation_tx: &ChainTransaction,
) -> Result<String, CoreError<S::Error>>;
```

`ContractParams` stays pure — only data needed to derive the contract's identity and addresses. No creation-time secrets. The `PredictionMarketAnchor` type and the `anchor` field on `Contract::PredictionMarket` are both eliminated. See [deadcat-core design doc](deadcat-core-design.md) for the full API.
