# Chain-Only Recovery

## Principle

All Deadcat contract positions are recoverable from a **mnemonic + chain connection** alone. No Nostr, no external services, no backups beyond the mnemonic. Only human-readable contract metadata (e.g., "Will BTC hit $200k by 2027?") requires the discovery layer — the cryptographic parameters that control funds are all recoverable from the chain.

This is achieved through three mechanisms:
1. **OP_RETURN recovery hints** in all contract creation transactions (market, pool, order)
2. **Elements asset issuance indexing** for token holder recovery
3. **Deterministic derivation** of all covenant parameters from a small set of mnemonic-derived keys

### Design Principle: Covenants Are Permissive, Builders Are Opinionated

The Simplicity covenants accept wide parameter ranges (u64 for prices, fees, collateral amounts). The `deadcat-core` PSET builders enforce tighter constraints — only parameter values that can be losslessly round-tripped through the OP_RETURN encoding are accepted. Non-conforming values produce `CoreError::InvalidParams`. This ensures every contract created through `deadcat-core` has a decodable recovery hint.

Convention compliance is enforced at three layers:
- **Derive functions** (`derive_order_params`, `derive_pool_params`): first line — catches convention violations at param construction time with the clearest error context
- **PSET builders** (all three creation builders): defense in depth — catches violations for manually-constructed params that bypass derive functions
- **Market ingestion** (`ingest_market`): protects all downstream users — rejects non-conforming markets, since non-conforming markets break the recovery chain for any child contracts (orders, pools) and any token holder tracing back to the market

| Contract | Creation enforcement | Ingestion enforcement | Why |
|---|---|---|---|
| Market | Builder rejects | `ingest_market` rejects | Non-conforming markets break all downstream users (token holders, orders, pools) |
| Order | `derive_order_params` + builder reject | No convention check | Only the creator needs recovery; takers just fill |
| Pool | `derive_pool_params` + builder reject | No convention check | Only the operator needs recovery; traders just swap |

Pool and order ingestion validates the parent market relationship (transitively ensuring the parent market is conforming) but does not enforce pool/order-specific conventions — a non-conforming pool or order is still fully functional for trading.

## Recovery Flows by User Type

### YES/NO Token Holders (Takers)

Token recovery is automatic — YES and NO tokens are standard Elements confidential assets at wallet addresses. Standard mnemonic-based rescan finds them.

**Labeling and redemption** require the market's `PredictionMarketParams`. The recovery path:

1. Wallet rescan finds YES/NO token UTXOs with asset IDs
2. For each unknown asset ID: query `ChainSource::issuance_transaction(asset_id)`
3. The returned transaction IS the market creation tx (the asset was first issued there)
4. Read the market OP_RETURN hint → reconstruct `PredictionMarketParams`
5. `ingest_market` with the reconstructed params + creation tx
6. `identify_asset` labels the tokens; `build_redemption_pset` enables redemption

One chain query per unique asset ID. Works despite blinded reissuance token outputs — the issuance entropy is always explicit in the transaction's input data (consensus requires it), and chain indexers (Esplora, Electrs) derive and index asset IDs from this entropy.

**Caveat**: This assumes the chain backend indexes zero-initial-supply issuances (the market creation tx issues RTs with zero token supply; actual tokens are minted later via reissuance). Esplora's `GET /asset/:asset_id` endpoint supports this. Verify with an integration test during implementation.

### Market Creators

Markets have no on-chain "owner" (taproot internal key is NUMS), but the creation transaction is wallet-funded. Recovery:

1. Wallet rescan finds the wallet-funded market creation tx
2. Read the market OP_RETURN → reconstruct `PredictionMarketParams` (non-derivable fields from hint + derivable asset IDs from the tx's issuance entropy)
3. `ingest_market`

### Order Creators (Makers)

Maker order UTXOs are at covenant addresses — standard wallet rescan cannot find them. The covenant script depends on the full order params (market, price, direction, nonce-derived receive address).

1. Wallet rescan finds the wallet-funded order creation tx
2. Read the order OP_RETURN → extract `masked_index`, `market_creation_txid`, `price`, `side`, `direction`, `min_fill_lots`, `min_remainder_lots`
3. Derive mask: `HMAC(deadcat_secret_key, "deadcat/order_mask" || context)[0..2]` (context = all fields from step 2 except masked_index)
4. Unmask: `order_index = masked_index ^ mask`
5. Fetch the market creation tx by `market_creation_txid` → read market OP_RETURN → reconstruct `PredictionMarketParams`
6. Call `derive_order_params(deadcat_xprv, market_params, order_index, side, direction, price, min_fill_lots, min_remainder_lots)` to reconstruct full `MakerOrderParams` (derives maker pubkey, nonce, and everything else internally)
7. Compile the covenant, verify the script matches a creation tx output
8. `ingest_order`

Without the OP_RETURN, recovery requires brute-forcing `order_index x market x price x direction x min_fill x min_remainder` — each candidate requiring Simplicity compilation (~10-100ms). With the hint, one compilation per order to verify.

### Pool Operators

Pool reserve UTXOs are at covenant addresses — standard wallet rescan cannot find them. Same pattern as orders:

1. Wallet rescan finds the wallet-funded pool creation tx
2. Read the pool OP_RETURN → extract `masked_index`, `market_creation_txid`, `max_loss_sats`, `half_payout_sats`, `fee_bps`, `initial_s_index`
3. Derive mask: `HMAC(deadcat_secret_key, "deadcat/pool_mask" || context)[0..2]` (context = all fields from step 2 except masked_index)
4. Unmask: `pool_index = masked_index ^ mask`
5. Fetch market creation tx → reconstruct market params
6. Call `derive_pool_params(deadcat_xprv, market_params, pool_index, max_loss_sats, half_payout_sats, fee_bps)` to reconstruct full `LmsrPoolParams` (derives admin pubkey, Merkle root, and everything else internally)
7. Compile the covenant for `initial_s_index`, verify the script matches a creation tx output
8. `ingest_pool`

## ChainSource Addition

Token holder recovery requires a new method on the `ChainSource` trait:

```rust
fn issuance_transaction(&self, asset_id: &AssetId)
    -> Result<Option<ChainTransaction>, Self::Error>;
```

Returns the transaction that first issued the given asset. For Esplora backends, this maps to `GET /asset/:asset_id` → `issuance_txin.txid` → fetch transaction. For Electrs, the asset index provides the same capability.

This works despite blinded reissuance token outputs because the `AssetIssuance` structure on the issuing input always contains the explicit entropy. Asset IDs are derived deterministically: `asset_id = SHA256mid(SHA256mid(entropy || 0))`, `rt_asset_id = SHA256mid(SHA256mid(entropy || 1))`. Chain indexers compute and index both IDs from the entropy regardless of output blinding.

## Key Derivation

### HD Paths

Recovery requires deterministic derivation of keys and secrets from the mnemonic. The wallet derives the deadcat xprv at `m/purpose'/deadcat'` and passes it to `deadcat-core`'s derive functions, which handle all child derivations internally. The internal structure:

| Path | Derives | Used for |
|---|---|---|
| `m/purpose'/deadcat'/secret'` | `deadcat_secret_key` | Order nonce derivation + index masking (both orders and pools) |
| `m/purpose'/deadcat'/orders'/i` | Maker keypair at index `i` | `maker_pubkey` (covenant param) + cancel signing |
| `m/purpose'/deadcat'/pools'/i` | Admin keypair at index `i` | `admin_pubkey` (covenant) + admin/close signing |

A single `deadcat_secret_key` is used for all HMAC operations across both contract types. Different HMAC tags (`"deadcat/order_nonce"`, `"deadcat/order_mask"`, `"deadcat/pool_mask"`) provide full cryptographic domain separation — the outputs are independent PRF evaluations even with the same key.

The exact `purpose'` value is TBD (BIP-43 registration or application-specific constant). All paths use hardened derivation — compromising the deadcat xprv cannot affect non-deadcat wallet keys.

**Interoperability**: This derivation spec is the public interoperability standard for cross-wallet recovery. Any wallet implementing Deadcat must follow these paths. `deadcat-core` provides convenience functions (`derive_order_params`, `derive_pool_params`) that accept the deadcat xprv and handle all child derivations internally — Rust integrators can use these directly. Cross-language implementations (JavaScript, Swift) must implement the derivation independently from this spec.

### Order Nonce Derivation

The maker order covenant requires `maker_receive_spk_hash` — derived from a nonce-tweaked public key unique to each order. The nonce MUST be deterministic from the mnemonic for recovery to work:

```
order_nonce = HMAC-SHA256(deadcat_secret_key, "deadcat/order_nonce" || order_index)
```

This nonce feeds into the existing derivation chain: `order_nonce` + other params → `order_uid` → `tweak` → `P_order` → `maker_receive_spk_hash`. Different `order_index` values produce different nonces, guaranteeing unique covenant scripts even for orders with otherwise identical params.

The `derive_order_params` function encapsulates this derivation — callers pass the deadcat xprv + `order_index`, and the maker pubkey, canonical nonce, and masked index are all computed internally. This eliminates the foot-gun of passing a non-deterministic nonce or mismatched keys.

### XOR Index Masking

Both `order_index` and `pool_index` are XOR-masked in the OP_RETURN to prevent observers from reading derivation indices, linking contracts to the same operator, or inferring how many contracts an operator has created.

```
mask = HMAC-SHA256(secret_key, tag || context)[0..2]
masked_index = index ^ mask
```

Where:
- `secret_key` is `deadcat_secret_key` (the same key for both orders and pools)
- `tag` is `"deadcat/order_mask"` or `"deadcat/pool_mask"`
- `context` is all other OP_RETURN fields (available before unmasking during recovery)

For orders: `context = market_creation_txid || price || side || direction || min_fill_lots || min_remainder_lots`
For pools: `context = market_creation_txid || max_loss_sats || half_payout_sats || fee_bps`

Including contract-specific params in the context ensures different contracts get different masks. The recovery code recomputes the same mask from the OP_RETURN data.

**Known property**: Two orders with completely identical params on the same market share the same mask (leaking the XOR of their indices). This is a negligible concern — identical params already imply a CMR collision scenario the spec warns against, and the leaked information (index difference, not absolute values) requires the observer to already have linked the two transactions to the same wallet.

## Standard Denomination Convention

### Market Denomination: 1-2-5 Table (4 bits)

`collateral_per_pair` is constrained to 16 values in the 1-2-5 series:

| Index | Value | ~USD (L-BTC) | Index | Value | ~USD (L-BTC) |
|---|---|---|---|---|---|
| 0 | 100 | $0.10 | 8 | 50,000 | $50 |
| 1 | 200 | $0.20 | 9 | 100,000 | $100 |
| 2 | 500 | $0.50 | 10 | 200,000 | $200 |
| 3 | 1,000 | $1 | 11 | 500,000 | $500 |
| 4 | 2,000 | $2 | 12 | 1,000,000 | $1,000 |
| 5 | 5,000 | $5 | 13 | 2,000,000 | $2,000 |
| 6 | 10,000 | $10 | 14 | 5,000,000 | $5,000 |
| 7 | 20,000 | $20 | 15 | 10,000,000 | $10,000 |

This determines order price resolution: `PRICE` is an integer bounded by `collateral_per_pair`, so the number of distinct expressible probability values equals `collateral_per_pair`. At 100: 1% increments (coarse). At 10,000: 0.01% increments (fine). Markets below 1,000 have limited price resolution for limit orders but work fine for LMSR pool trading (pools use their own pricing curve).

### Pool Denomination: 26-Value Mantissa x 10^Exponent (9 bits)

`max_loss_sats` and `half_payout_sats` use a two-significant-digit encoding:

**26 mantissa values** (5 bits):
```
10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,
25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 75, 80, 85, 90, 95
```

Fine granularity at the low end (10-20: ~5-10% steps), medium in the middle (25-50: ~10-20% steps), adequate at the high end (50-95: ~5-17% steps). 6 unused 5-bit slots reserved for future expansion.

**4-bit exponent** (0-15): `value = mantissa x 10^exponent`. The 4-bit exponent (not 3-bit) is necessary because non-L-BTC assets (USDT on Liquid = 10^8 units per dollar) need exponent 8+ for moderate pool sizes.

Range: 10 x 10^0 = 10 through 95 x 10^15. Covers all practical pool parameters for any collateral asset.

### Well-Known Collateral Asset Index (4 bits)

```
0 = L-BTC (mainnet)
1 = USDt (Liquid)
2-14 = reserved for future well-known assets
15 = escape: full 32-byte asset ID follows
```

The lookup table is network-specific — L-BTC has different asset IDs on mainnet, testnet, and regtest. The engine knows the network from construction time.

## OP_RETURN Encoding Specification

All recovery hints use zero-value OP_RETURN outputs. Data must be whole bytes (padded to byte boundaries). The type tag byte identifies the hint type and enables forward compatibility.

### Type Tag

The first byte of every hint. It identifies:
1. Whether this is a deadcat hint (vs other protocols' OP_RETURNs)
2. Which contract type (market, pool, order)
3. Format version
4. For orders: side and direction flags in the low bits

The type tag is a **first-pass filter**, not a guarantee. Roughly 1 in 256 random OP_RETURNs match any given type tag value. Full verification (decode all fields, compile covenant, match script) is what confirms a hint is genuine.

### Market Hint

**37 bytes** (known collateral asset) / **69 bytes** (exotic collateral with escape code):

```
Byte  0:     type_tag                                  --  8 bits
Bytes 1-32:  oracle_public_key                         -- 256 bits
Byte  33:    [collateral_asset(4)][collateral_per_pair(4)]  --  8 bits
Bytes 34-36: expiry_time (u24, big-endian)             -- 24 bits
                                                 Total: 296 bits = 37 bytes
```

If `collateral_asset` index = 15 (escape): 32 additional bytes of raw `collateral_asset_id` follow, totaling 69 bytes.

**Per-field justification:**

| Field | In hint | Size | Justification |
|---|---|---|---|
| `oracle_public_key` | Yes | 32 bytes | Not derivable — chosen by market creator |
| `collateral_asset_id` | Yes (indexed) | 4 bits | Not derivable — well-known index, escape for exotic |
| `collateral_per_pair` | Yes (indexed) | 4 bits | Not derivable — 1-2-5 convention, 16 values |
| `expiry_time` | Yes (absolute) | 3 bytes | Not derivable — u24 absolute encoding (see below) |
| `yes_token_asset_id` | No | — | Derivable from creation tx issuance entropy |
| `no_token_asset_id` | No | — | Derivable from creation tx issuance entropy |
| `yes_reissuance_token_id` | No | — | Derivable from creation tx issuance entropy |
| `no_reissuance_token_id` | No | — | Derivable from creation tx issuance entropy |

**Expiry time encoding** (u24): `encoded = expiry_time / 60` stored as 3 bytes big-endian. Recovery: `expiry_time = encoded × 60`. The PSET builder **snaps** `expiry_time` to the nearest 60-block boundary (the covenant uses the snapped value, making the encoding lossless). At Liquid's target rate of 1 block per minute, each unit represents approximately 1 hour. The u24 range (0 to 2^24 - 1 = 16,777,215) covers block heights from the Liquid genesis block (mined September 26, 2018) to approximately the year 3931, providing 1-hour granularity with ~1,900 years of headroom. This absolute encoding was chosen over a creation-block-relative delta because the creation block height is unknown at PSET build time (the transaction hasn't been broadcast yet), making delta-based recovery produce incorrect values due to confirmation drift.

### Order Hint (40 bytes)

```
Byte  0:     [format(4)][side(1)][direction(1)][reserved(2)]  --  8 bits
Bytes 1-2:   masked_order_index (u16)                          -- 16 bits
Bytes 3-34:  market_creation_txid                              -- 256 bits
Bytes 35-37: price (u24, big-endian)                           -- 24 bits
Byte  38:    min_fill_lots (u8)                                --  8 bits
Byte  39:    min_remainder_lots (u8)                           --  8 bits
                                                         Total: 320 bits = 40 bytes
```

**Per-field justification:**

| Field | In hint | Size | Justification |
|---|---|---|---|
| `side` | Yes (in type_tag) | 1 bit | Not derivable — YES or NO as base token |
| `direction` | Yes (in type_tag) | 1 bit | Not derivable — SellBase or SellQuote |
| `order_index` | Yes (masked) | 2 bytes | Needed for key derivation; XOR-masked for privacy |
| `market_creation_txid` | Yes | 32 bytes | Chain-only recovery of market params |
| `price` | Yes | 3 bytes | Not derivable — u24, max ~16.8M; bounded by `collateral_per_pair` for rational orders |
| `min_fill_lots` | Yes | 1 byte | Not derivable — u8, range 1-255; baked into covenant script |
| `min_remainder_lots` | Yes | 1 byte | Not derivable — u8, range 1-255; baked into covenant script |
| `base_asset_id` | No | — | Derivable: `side` + market params → YES or NO asset ID |
| `quote_asset_id` | No | — | Derivable: market params → collateral asset ID |
| `maker_receive_spk_hash` | No | — | Derivable: mnemonic → nonce → tweak → P_order → hash |
| `maker_pubkey` | No | — | Derivable: mnemonic at `order_index` |

**Builder validation** (returns `CoreError::InvalidParams` if violated):
- `price <= 0xFFFFFF` (16,777,215)
- `min_fill_lots` in range 1-255
- `min_remainder_lots` in range 1-255
- `order_index <= 65535`
- Parent market conforms to market conventions

### Pool Hint (41 bytes)

```
Byte  0:      type_tag                                          --  8 bits
Bytes 1-32:   market_creation_txid                              -- 256 bits
Bits 264-272: max_loss_sats (9 bits: 5 mantissa + 4 exponent)   \
Bits 273-281: half_payout_sats (9 bits: 5 mantissa + 4 exponent) |-- 64 bits = 8 bytes
Bits 282-293: fee_bps (u12)                                      |   (exact bit-level packing
Bits 294-309: initial_s_index (u16)                              |    across bytes is an
Bits 310-325: masked_pool_index (u16)                            |    implementation detail)
Bits 326-327: reserved (must be zero)                            /
                                                          Total: 328 bits = 41 bytes
```

**Per-field justification:**

| Field | In hint | Size | Justification |
|---|---|---|---|
| `market_creation_txid` | Yes | 32 bytes | Chain-only recovery of market params |
| `max_loss_sats` | Yes (encoded) | 9 bits | Not derivable — 26-value mantissa x 10^exp |
| `half_payout_sats` | Yes (encoded) | 9 bits | Not derivable — same encoding |
| `fee_bps` | Yes | 12 bits | Not derivable — u12, 0.01% granularity, max 40.95% |
| `initial_s_index` | Yes | 16 bits | Not derivable without brute-force script matching (EC scalar mul per candidate); enables direct script verification during creation-tx ingestion |
| `pool_index` | Yes (masked) | 16 bits | Needed for admin key derivation; XOR-masked |
| `yes/no/collateral_asset_id` | No | — | Derivable from parent market params |
| `lmsr_table_root` | No | — | Derivable via deterministic table generation from `max_loss_sats` + `half_payout_sats` |
| `q_step_lots` | No | — | Derivable from `b` and `half_payout_sats` |
| `admin_pubkey` | No | — | Derivable from mnemonic at `pool_index` |
| Protocol constants | No | — | `TABLE_DEPTH`, `S_BIAS`, `S_MAX_INDEX`, `MIN_POOL_RESERVE` are fixed in the `.simf` |

**Builder validation:**
- `max_loss_sats` and `half_payout_sats` must be in the 26-value mantissa x exponent set
- `fee_bps <= 4095` (40.95%)
- `initial_s_index <= 65535`
- `pool_index <= 65535`
- Parent market conforms to market conventions

## Forward Compatibility

### Type Tag Versioning

New OP_RETURN formats can be introduced via new type tag values. This is a non-breaking change:
- New software reads old formats (backwards compatible)
- Old software encountering an unknown type tag logs "unknown hint format — update your software" and skips the hint
- No hints are silently lost — unrecognized hints are flagged, not ignored

### Reserved Bits

Reserved bits in all hint formats must be zero. Future format versions may assign meaning to these bits. Decoders encountering non-zero reserved bits in a known format version should still attempt decoding (forward-compatible) but may log a warning.

### Position-Based Market Reference (Future V2)

A future compression optimization: encode `market_creation_txid` as `(block_height: u32, tx_index: u16)` = 6 bytes instead of 32 bytes. This would save 26 bytes on every pool and order hint. Not implemented in V1 due to: (a) chain backend must support "get tx at block position" (new `ChainSource` capability), (b) relies on Liquid's 2-confirmation finality guarantee, (c) adds a failure mode if the recovery software lacks full block data. Noted here as a viable future path.

## Cost Amortization

Market and pool creations are infrequent lifecycle events — the OP_RETURN cost is paid once and amortized over the contract's entire lifetime. Maker order creation is the most frequent user-facing operation with an OP_RETURN, but the cost (37-40 bytes at typical Liquid fee rates) is negligible relative to order value and trade fees. The OP_RETURN cost is never paid by market takers or regular traders — only by contract creators.

## Key Files

- `docs/deadcat-core-design.md` — main design doc (Wallet Recovery section references this doc)
- `docs/lmsr-pool-design.md` — LMSR pool design (deterministic table generation enables pool hint compression)
- `docs/deterministic-rt-blinding.md` — RT blinding factors are deterministic, eliminating anchor secrets from recovery
