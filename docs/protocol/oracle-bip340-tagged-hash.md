# Oracle Attestation: BIP-340 Tagged Hash

This document is the authoritative oracle-attestation specification for **both** Deadcat market kinds: the binary prediction market and the multi-outcome market. Both use the same BIP-340 tagged-hash domain string and the same high-level message structure; they differ only in how `market_id` and `outcome_byte` are derived.

## Problem

The current oracle signature scheme uses a plain SHA256 hash for the attestation message:

```
message = SHA256(market_id || outcome_byte)
```

Where `market_id` identifies the market and `outcome_byte` identifies the attested resolution.

This lacks domain separation. If the oracle's signing key is used in another context that also signs `SHA256(32_bytes || 1_byte)`, a signature from that context could theoretically satisfy the covenant. Per BIP-340's rationale: "without tagged hashing a BIP340 signature could also be valid for a signature scheme where the only difference is that the arguments to the hash function are reordered."

## Proposed Change

Replace the plain SHA256 with a BIP-340 tagged hash:

```
message = SHA256(SHA256("deadcat/oracle_attestation") || SHA256("deadcat/oracle_attestation") || market_id || outcome_byte)
```

Where:
- The tag `"deadcat/oracle_attestation"` is UTF-8 encoded.
- `market_id` is the covenant-internal market identifier derived from the market's token asset IDs.
- `outcome_byte` is the one-byte resolution encoding for the specific market kind.
- The double `SHA256(tag)` prefix follows the BIP-340 tagged hash convention.

The BIP-340 tagged hash construction prefixes the data with `SHA256(tag) || SHA256(tag)` (64 bytes). This:
1. Creates a domain-separated hash function that cannot collide with untagged SHA256 or other tagged hashes with different tags
2. Fills exactly one SHA-256 block (64 bytes), enabling an optimization: implementations can precompute the SHA-256 internal state after the first block and reuse it for every call with the same tag

## Message Format

### Shared tagged-hash rule

Both market kinds sign the same tagged-hash envelope:

```
message = tagged_hash("deadcat/oracle_attestation", market_id || outcome_byte)
        = SHA256(SHA256("deadcat/oracle_attestation") || SHA256("deadcat/oracle_attestation") || market_id || outcome_byte)
```

### Binary market

```
market_id = SHA256(yes_token_asset_id || no_token_asset_id)
outcome_byte = 0x01 for YES, 0x00 for NO
```

Binary markets have one outcome and two sides. Oracle resolution therefore encodes the winning **side**, not an outcome index.

### Multi-outcome market

```
market_id = SHA256(yes_token_asset_ids[0] || no_token_asset_ids[0]
                || yes_token_asset_ids[1] || no_token_asset_ids[1]
                || ...
                || yes_token_asset_ids[N-1] || no_token_asset_ids[N-1])
outcome_byte = outcome_index as u8, in range [0, N-1]
```

Multi-outcome markets have `N` outcomes and encode the winning **outcome index** directly. The tag string is unchanged; domain separation between binary and multi-outcome markets comes from the different `market_id` derivation.

## Impact on Simplicity Covenants

### Affected function shape

The binary and multi-outcome market contracts both implement the same high-level rule:

```simplicity
fn verify_oracle_signature(outcome_byte: u8, signature: Signature) {
    let market_id: u256 = compute_market_id();
    let tag_hash: u256 = /* precomputed SHA256("deadcat/oracle_attestation") */;
    let ctx: Ctx8 = jet::sha_256_ctx_8_init();
    let ctx: Ctx8 = jet::sha_256_ctx_8_add_32(ctx, tag_hash);
    let ctx: Ctx8 = jet::sha_256_ctx_8_add_32(ctx, tag_hash);
    let ctx: Ctx8 = jet::sha_256_ctx_8_add_32(ctx, market_id);
    let ctx: Ctx8 = jet::sha_256_ctx_8_add_1(ctx, outcome_byte);
    let message: u256 = jet::sha_256_ctx_8_finalize(ctx);
    jet::bip_0340_verify((param::ORACLE_PUBLIC_KEY, message), signature);
}
```

The only market-specific part is how `compute_market_id()` and `outcome_byte` are formed.

### Binary-market before/after example

`verify_oracle_signature` in `prediction_market.simf` changes from the old plain SHA256 rule to the tagged-hash rule:

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

Note: `SHA256("deadcat/oracle_attestation")` is a constant. Implementations can precompute it and embed it directly in the `.simf` source.

### No Other Covenant Changes

- The oracle public key parameter (`ORACLE_PUBLIC_KEY`) is unchanged
- The signature format (64-byte BIP-340 Schnorr) is unchanged
- The `compute_market_id()` logic for each contract kind is unchanged except for being documented here as part of the shared protocol
- The binary and multi-outcome outcome-byte encodings are unchanged

## Impact on deadcat-core

### Canonical standalone function

```rust
/// Returns the 32-byte message an oracle must BIP-340 sign to attest to a market outcome.
/// Uses tagged hash over (market_id || outcome_byte), where both values are already
/// specific to the target market kind.
pub fn oracle_attestation_message(
    market_id: MarketId,
    resolution: MarketResolution,
) -> [u8; 32];
```

This is the protocol-level helper. Binary-specific convenience helpers may still exist, but they are specializations of this general rule rather than the other way around.

### Engine Convenience Method

```rust
pub fn oracle_attestation_spec(
    &self,
    contract_id: &ContractId,
    resolution: MarketResolution,
) -> Result<OracleAttestationSpec, CoreError<S::Error>>;

pub struct OracleAttestationSpec {
    pub market_id: MarketId,
    pub resolution: MarketResolution,
    pub message: [u8; 32],
    pub oracle_pubkey: XOnlyPublicKey,
}
```

Looks up the market's params from the store, derives the correct `market_id`, validates that the resolution variant matches the market kind, then calls `oracle_attestation_message` internally.

## Key Files

- `crates/deadcat-core/contracts/prediction_market.simf` — binary market oracle verification
- `crates/deadcat-core/contracts/multi_outcome/*.simf` — generated multi-outcome market oracle verification
- `crates/deadcat-core` market-oracle helper module — Rust-side attestation helper implementation target
