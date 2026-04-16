# Oracle Attestation: BIP-340 Tagged Hash Migration

## Problem

The current oracle signature scheme uses a plain SHA256 hash for the attestation message:

```
message = SHA256(market_id || outcome_byte)
```

Where `market_id = SHA256(yes_token_asset_id || no_token_asset_id)` and `outcome_byte` is `0x01` (YES) or `0x00` (NO).

This lacks domain separation. If the oracle's signing key is used in another context that also signs `SHA256(32_bytes || 1_byte)`, a signature from that context could theoretically satisfy the covenant. Per BIP-340's rationale: "without tagged hashing a BIP340 signature could also be valid for a signature scheme where the only difference is that the arguments to the hash function are reordered."

## Proposed Change

Replace the plain SHA256 with a BIP-340 tagged hash:

```
message = SHA256(SHA256("deadcat/oracle_attestation") || SHA256("deadcat/oracle_attestation") || market_id || outcome_byte)
```

Where:
- `market_id = SHA256(yes_token_asset_id || no_token_asset_id)` — unchanged
- `outcome_byte` is `0x01` for YES, `0x00` for NO — unchanged
- The tag `"deadcat/oracle_attestation"` is UTF-8 encoded
- The double `SHA256(tag)` prefix follows the BIP-340 tagged hash convention

The BIP-340 tagged hash construction prefixes the data with `SHA256(tag) || SHA256(tag)` (64 bytes). This:
1. Creates a domain-separated hash function that cannot collide with untagged SHA256 or other tagged hashes with different tags
2. Fills exactly one SHA-256 block (64 bytes), enabling an optimization: implementations can precompute the SHA-256 internal state after the first block and reuse it for every call with the same tag

## Impact on Simplicity Covenant

### Affected Function

`verify_oracle_signature` in `prediction_market.simf` (currently lines 248-259):

```simplicity
// Before
fn verify_oracle_signature(outcome_yes: bool, signature: Signature) {
    let market_id: u256 = compute_market_id();
    let outcome_byte: u8 = match outcome_yes {
        true => 1,
        false => 0,
    };
    let ctx: Ctx8 = jet::sha_256_ctx_8_init();
    let ctx: Ctx8 = jet::sha_256_ctx_8_add_32(ctx, market_id);
    let ctx: Ctx8 = jet::sha_256_ctx_8_add_1(ctx, outcome_byte);
    let message: u256 = jet::sha_256_ctx_8_finalize(ctx);
    jet::bip_0340_verify((param::ORACLE_PUBLIC_KEY, message), signature);
}

// After
fn verify_oracle_signature(outcome_yes: bool, signature: Signature) {
    let market_id: u256 = compute_market_id();
    let outcome_byte: u8 = match outcome_yes {
        true => 1,
        false => 0,
    };
    // BIP-340 tagged hash: SHA256(SHA256(tag) || SHA256(tag) || data)
    let tag_hash: u256 = {
        let ctx: Ctx8 = jet::sha_256_ctx_8_init();
        let ctx: Ctx8 = jet::sha_256_ctx_8_add_32(ctx, /* "deadcat/oracle_attestation" pre-hashed */);
        jet::sha_256_ctx_8_finalize(ctx)
    };
    let ctx: Ctx8 = jet::sha_256_ctx_8_init();
    let ctx: Ctx8 = jet::sha_256_ctx_8_add_32(ctx, tag_hash);  // first SHA256(tag)
    let ctx: Ctx8 = jet::sha_256_ctx_8_add_32(ctx, tag_hash);  // second SHA256(tag)
    let ctx: Ctx8 = jet::sha_256_ctx_8_add_32(ctx, market_id);
    let ctx: Ctx8 = jet::sha_256_ctx_8_add_1(ctx, outcome_byte);
    let message: u256 = jet::sha_256_ctx_8_finalize(ctx);
    jet::bip_0340_verify((param::ORACLE_PUBLIC_KEY, message), signature);
}
```

Note: The `SHA256("deadcat/oracle_attestation")` tag hash is a constant (deterministic from the tag string). It can be pre-computed and embedded as a literal in the `.simf` source to avoid hashing the tag string at runtime.

### No Other Covenant Changes

- The oracle public key parameter (`ORACLE_PUBLIC_KEY`) is unchanged
- The signature format (64-byte BIP-340 Schnorr) is unchanged
- The `compute_market_id()` function is unchanged
- The outcome byte encoding is unchanged

## Impact on deadcat-core

### Standalone Function

```rust
/// Returns the 32-byte message an oracle must BIP-340 sign to attest to a market outcome.
/// Uses tagged hash: SHA256(SHA256("deadcat/oracle_attestation") || SHA256("deadcat/oracle_attestation") || market_id || outcome_byte)
/// where market_id = SHA256(yes_token_asset_id || no_token_asset_id).
pub fn oracle_attestation_message(
    yes_asset_id: &AssetId,
    no_asset_id: &AssetId,
    outcome_yes: bool,
) -> [u8; 32];
```

This replaces the existing `oracle_message` function in the SDK (`prediction_market/oracle.rs`).

### Engine Convenience Method

```rust
pub fn oracle_attestation_spec(
    &self,
    contract_id: &ContractId,
    outcome_yes: bool,
) -> Result<OracleAttestationSpec, CoreError<S::Error>>;

pub struct OracleAttestationSpec {
    pub message: [u8; 32],
    pub oracle_pubkey: XOnlyPublicKey,
}
```

Looks up the market's params from the store, calls `oracle_attestation_message` internally, returns both the message and the expected oracle public key.

## Key Files

- `src-tauri/crates/deadcat-sdk/contract/prediction_market.simf` — `verify_oracle_signature` function
- `src-tauri/crates/deadcat-sdk/src/prediction_market/oracle.rs` — `oracle_message` function (to be updated)
