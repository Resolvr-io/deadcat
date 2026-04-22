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

## Integration Contract

Correct chain-only recovery depends on the wallet integrator providing specific inputs to `deadcat-core`. This section enumerates every precondition, the failure mode if violated, and what the engine verifies on its own.

### Preconditions the integrator must satisfy

| Precondition | Failure mode if violated |
|---|---|
| **Deadcat xprv derived at `m/86'/1145258324'`** (see [HD Paths](#hd-paths)). The integrator is responsible for performing the derivation before passing the key to `derive_*_params` or engine construction. | Silent. Derive functions produce different keys, reconstructed covenant scripts do not match on-chain UTXOs, recovery reports "no matches" instead of an error. |
| **Complete wallet rescan.** The caller must present every wallet-funded transaction on the target network, from the wallet's first use through the current tip. Incremental rescans must not skip block ranges. | Silent. Orders and pools whose creation txs were missed are simply absent from the recovered state. The engine has no way to know about txs it was never given. |
| **Authoritative, tip-synced `ChainSource`.** Backends must return complete, current state — not filtered or stale results. | Latent. Stale tip produces stale state. Missing txs in `transactions_in_block` or equivalent queries produce the same silent gap as incomplete rescan. |
| **`ChainSource::issuance_transaction(asset_id)` returns the first-issuance transaction**, not a subsequent reissuance. Esplora's `/asset/:asset_id` endpoint and Electrs's asset index both return this directly. | Loud. `ingest_market` re-derivation fails the script-pubkey match and returns `CoreError::InvalidCreationTransaction`. The error does not obviously point at the integrator's `ChainSource` implementation — integrators should treat this error as a signal to verify their issuance lookup is returning the genesis tx. |
| **Correct `Network` at engine construction.** The well-known collateral asset index (see [Well-Known Collateral Asset Index](#well-known-collateral-asset-index-4-bits)) resolves against network-specific asset IDs — mainnet L-BTC ≠ testnet L-BTC ≠ regtest L-BTC, and the v1 well-known USDt entry exists only on Liquid mainnet. | Silent. Decoded collateral asset IDs resolve to the wrong chain's policy asset or treat a mainnet-only USDt index as valid on the wrong network; downstream operations fail with "unknown asset" rather than an explicit network-mismatch error. |

### What `deadcat-core` verifies

- **Creation tx / OP_RETURN authenticity** — ingestion re-derives the covenant script pubkey from the parsed params and requires it match the creation tx's output script. Spoofed hints or wrong creation txs are rejected with `CoreError::InvalidCreationTransaction`.
- **Asset identity on ingestion** — `identify_asset` cross-checks asset IDs against registered market params. Unknown asset IDs are reported, not silently accepted.
- **Covenant state transitions** — every tx presented to `step` / `interpret_transaction` is validated against the expected covenant spend paths; invalid transitions are rejected.

### What `deadcat-core` does not verify

- **Completeness of the transaction set the caller provided.** The engine cannot detect "you forgot to give me tx X." Integrators must independently guarantee rescan completeness.
- **Freshness of the chain tip.** The engine processes what it is given in the order it is given; a stale backend produces stale state with no warning.
- **Derivation path of the passed xprv.** The engine trusts the caller to have derived at `m/86'/1145258324'`. A wrong-path xprv produces usable-looking derived keys that silently fail to match on-chain data.

### Recommendation for integrators

After recovery, sanity-check the engine's state against an independent source before exposing it to the user. At minimum, query the `ChainSource` for the current chain height and confirm the wallet's latest processed height matches — this does not catch missing historical txs but catches obviously-stale backends.

A `verify_integration(xprv, chain_source)` helper is under consideration for a future release. It would exercise a known derivation + lookup path to convert several silent-failure integration bugs into fail-fast at construction time. Not committed for v1.

## Recovery Flows by User Type

### YES/NO Token Holders (Takers)

Token recovery is automatic — YES and NO tokens are standard Elements confidential assets at wallet addresses. Standard mnemonic-based rescan finds them.

**Labeling and redemption** require the market's `MarketParams` (binary or multi-outcome umbrella). The recovery path:

1. Wallet rescan finds YES/NO token UTXOs with asset IDs
2. For each unknown asset ID: query `ChainSource::issuance_transaction(asset_id)`
3. The returned transaction IS the market creation tx (the asset was first issued there)
4. Read the market OP_RETURN hint → reconstruct `MarketParams` (binary or multi-outcome, per the hint's type tag)
5. `ingest_market` with the reconstructed params + creation tx
6. `identify_asset` labels the tokens; `build_redemption_pset` enables redemption

One chain query per unique asset ID. Works despite blinded reissuance token outputs — the issuance entropy is always explicit in the transaction's input data (consensus requires it), and chain indexers (Esplora, Electrs) derive and index asset IDs from this entropy.

**Caveat**: This assumes the chain backend indexes zero-initial-supply issuances (the market creation tx issues RTs with zero token supply; actual tokens are minted later via reissuance). Esplora's `GET /asset/:asset_id` endpoint supports this. Verify with an integration test during implementation.

### Market Creators

Markets have no on-chain "owner" (taproot internal key is NUMS), but the creation transaction is wallet-funded. Recovery:

1. Wallet rescan finds the wallet-funded market creation tx
2. Read the market OP_RETURN → reconstruct `MarketParams` (non-derivable fields from hint + derivable asset IDs from the tx's issuance entropy; umbrella variant determined by the hint's type tag — binary or multi-outcome)
3. `ingest_market`

### Order Creators (Makers)

Maker order UTXOs are at covenant addresses — standard wallet rescan cannot find them. The covenant script depends on the full order params (market, price, direction, nonce-derived receive address).

1. Wallet rescan finds the wallet-funded order creation tx
2. Read the order OP_RETURN → extract `masked_index`, `market_creation_txid`, `price`, `side`, `direction`, `min_fill_lots`, `min_remainder_lots`
3. Derive mask: `HMAC(deadcat_secret_key, "deadcat/order_mask" || context)[0..2]` (context = all fields from step 2 except masked_index)
4. Unmask: `order_index = masked_index ^ mask`
5. Fetch the market creation tx by `market_creation_txid` → read market OP_RETURN → reconstruct `MarketParams` (binary or multi-outcome, determined by the market hint's type tag)
6. For each candidate `outcome` in the market's valid `OutcomeIndex` range:
   - For **binary** markets, the only valid index is `OutcomeIndex::BINARY` — a single candidate.
   - For **multi-outcome** markets with `MultiOutcomeMarketParams { outcome_count: N, .. }`, iterate `OutcomeIndex::new(k)` for `k in 0..N` — up to N candidates.
   - Call `derive_order_params(deadcat_xprv, market_params, outcome, order_index, side, direction, price, min_fill_lots, min_remainder_lots)` to reconstruct candidate `MakerOrderParams`.
   - Compile the covenant and check whether the script matches a creation tx output. First match wins.
7. `ingest_order`

Without the OP_RETURN, recovery requires brute-forcing `order_index x outcome x market x price x direction x min_fill x min_remainder` — each candidate requiring Simplicity compilation (~10-100ms). With the hint, up to `outcome_count` compilations per order to verify. See [Recovering without a hint](#recovering-without-a-hint-non-standard) for the non-standard fallback.

### Pool Operators

Pool reserve UTXOs are at covenant addresses — standard wallet rescan cannot find them. Same pattern as orders:

1. Wallet rescan finds the wallet-funded pool creation tx
2. Read the pool OP_RETURN → extract `masked_index`, `market_creation_txid`, `max_loss_sats`, `half_payout_sats`, `fee_bps`, `initial_s_index`
3. Derive mask: `HMAC(deadcat_secret_key, "deadcat/pool_mask" || context)[0..2]` (context = all fields from step 2 except masked_index)
4. Unmask: `pool_index = masked_index ^ mask`
5. Fetch market creation tx → reconstruct market params (binary or multi-outcome)
6. For each candidate `outcome` in the market's valid `OutcomeIndex` range:
   - For **binary** markets, the only valid index is `OutcomeIndex::BINARY` — a single candidate.
   - For **multi-outcome** markets with `MultiOutcomeMarketParams { outcome_count: N, .. }`, iterate `OutcomeIndex::new(k)` for `k in 0..N` — up to N candidates.
   - Call `derive_pool_params(deadcat_xprv, market_params, outcome, pool_index, max_loss_sats, half_payout_sats, fee_bps, initial_s_index)` to reconstruct candidate `LmsrPoolParams`. `initial_s_index` is passed directly from the hint — no inverse conversion.
   - Compile the covenant for `initial_s_index` and check whether the script matches a creation tx output. First match wins.
7. `ingest_pool`

### Recovering without a hint (non-standard)

The hint-based flows above assume every deadcat contract carries a parseable `deadcat-core`-format OP_RETURN. A creation transaction without such a hint — non-conforming contract built with custom tooling, format mismatch between recovery code and hint version, or pathological on-chain data loss — cannot be recovered via the fast path. The only fallback is brute-force index scanning: for each candidate `index` in `[0, 65535]` (and, for multi-outcome markets, each candidate `outcome`), derive the contract params with those values, compile the covenant, and check the script pubkey against known covenant-address UTXOs. At ~10-100 ms per Simplicity compilation, a full sweep costs 10-100 minutes per orphaned UTXO per outcome.

This path is **not supported** by `deadcat-core` v1. Integrators who need it can implement it against the public `derive_order_params` / `derive_pool_params` functions; it is a thin loop over indices and outcomes that compares compiled covenant scripts against the target UTXO set. Adding a shipped helper is non-breaking and can happen in a future release if real-world demand emerges. In practice, `deadcat-core`-built contracts always carry a hint, and convention enforcement at ingestion rejects non-conforming ones — so this fallback is relevant only when the contract was built by a tool that bypassed `deadcat-core`, in which case the authoring tool is responsible for its own recovery story.

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

Recovery requires deterministic derivation of keys and secrets from the mnemonic. The wallet derives the deadcat xprv at `m/86'/1145258324'` and passes it to `deadcat-core`'s derive functions, which handle all child derivations internally. The internal structure:

| Path | Derives | Used for |
|---|---|---|
| `m/86'/1145258324'/secret'` | `deadcat_secret_key` | Order nonce derivation + index masking (both orders and pools) |
| `m/86'/1145258324'/orders'/i` | Maker keypair at index `i` | `maker_pubkey` (covenant param) + cancel signing |
| `m/86'/1145258324'/pools'/i` | Admin keypair at index `i` | `admin_pubkey` (covenant) + admin/close signing |

A single `deadcat_secret_key` is used for all HMAC operations across both contract types. Different HMAC tags (`"deadcat/order_nonce"`, `"deadcat/order_mask"`, `"deadcat/pool_mask"`) provide full cryptographic domain separation — the outputs are independent PRF evaluations even with the same key.

**Path constants**. The `purpose'` value `86'` follows BIP-86 (single-key taproot) — deadcat covenants are taproot-based. The `coin_type'` value `1145258324'` is `0x44434154` = ASCII `"DCAT"`, self-documenting and within the hardened-index range (`< 2^31 - 1`). This follows the same pattern used by RGB-on-Liquid and other non-wallet protocols: claim a SLIP-0044 `coin_type` slot under a standard BIP purpose rather than introducing a new `purpose'` value. A SLIP-0044 registration PR for this coin_type is tracked as a pre-v1-ship action item.

**Migration from deadcat-sdk**. The existing `deadcat-sdk` code uses `m/84'/1776'/...` for maker-order and pool-admin key derivation. That path is superseded by this specification. Since `deadcat-core` is pre-implementation and no on-chain contracts use the new path yet, this is a clean break with no migration.

All paths use hardened derivation — compromising the deadcat xprv cannot affect non-deadcat wallet keys.

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

The context uses raw field values in standard big-endian encoding, independent of the compact OP_RETURN bit-packing. This makes the mask computation encoding-agnostic — a future OP_RETURN V2 format change would not affect mask computation for the same logical values.

**Order mask context (44 bytes):**
```
market_creation_txid  (32 bytes, raw)
price                 (8 bytes, u64 big-endian)
side                  (1 byte: 0x00 = Yes, 0x01 = No)
direction             (1 byte: 0x00 = SellBase, 0x01 = SellQuote)
min_fill_lots         (1 byte, u8)
min_remainder_lots    (1 byte, u8)
```

**Pool mask context (52 bytes):**
```
market_creation_txid  (32 bytes, raw)
max_loss_sats         (8 bytes, u64 big-endian)
half_payout_sats      (8 bytes, u64 big-endian)
fee_bps               (2 bytes, u16 big-endian)
initial_s_index       (2 bytes, u16 big-endian)
```

At recovery time, the decoder reads the OP_RETURN, decodes each field to its raw value (e.g., 4-bit 1-2-5 table index → u64 for `max_loss_sats`), then serializes in this format for the HMAC. The context length does not affect on-chain size — only the 2-byte mask output appears in the OP_RETURN.

Including contract-specific params in the context ensures different contracts get different masks. The recovery code recomputes the same mask from the decoded OP_RETURN data.

**Known property**: Two orders with identical non-index params on the same market share the same mask (leaking the XOR of their order indices). This is a negligible concern — distinct `order_index` values produce distinct `maker_pubkey` and `order_nonce` values (see [Key Derivation](#key-derivation) and [Order Nonce Derivation](#order-nonce-derivation)), so CMR collision is structurally prevented regardless of mask overlap. The leaked information (index XOR, not absolute indices) additionally requires the observer to have already linked both transactions to the same wallet.

## Standard Denomination Convention

### Market Denomination: 1-2-5 Table (4 bits)

`base_payout` — the primary covenant denomination, representing the per-outcome YES-expiry payout unit — is constrained to 16 values in the 1-2-5 series:

| Index | Value (sats) | Index | Value (sats) |
|---|---|---|---|
| 0 | 100 | 8 | 50,000 |
| 1 | 200 | 9 | 100,000 |
| 2 | 500 | 10 | 200,000 |
| 3 | 1,000 | 11 | 500,000 |
| 4 | 2,000 | 12 | 1,000,000 |
| 5 | 5,000 | 13 | 2,000,000 |
| 6 | 10,000 | 14 | 5,000,000 |
| 7 | 20,000 | 15 | 10,000,000 |

Binary markets derive `cp = base_payout × 2`. Multi-outcome markets derive `cp = base_payout × N`. This pair-cost is the total collateral backing one `(YES_i + NO_i)` pair. Parameterizing on `base_payout` rather than `cp` makes expiry-redemption rates exact integers by construction: a YES token always pays `base_payout`; a NO token always pays `base_payout × (N-1)` at expiry. No covenant-level divisibility check is needed, and every denomination-table index is usable for every supported N. See [multi-outcome-market-contract.md § Denomination model](../contracts/multi-outcome/multi-outcome-market-contract.md#denomination-model) for the full rationale.

This determines order price resolution: order `PRICE` is an integer bounded by `cp = base_payout × N`, so the number of distinct expressible probability values equals `cp`. For a binary market at `base_payout = 100` (`cp = 200`): 0.5% increments. At `base_payout = 10,000` (`cp = 20,000`): 0.005% increments. Markets with low `cp` have limited price resolution for limit orders but work fine for LMSR pool trading (pools use their own pricing curve).

### Pool Denomination: 1-2-5 Table (4 bits each)

Both `max_loss_sats` and `half_payout_sats` share the same 16-value 1-2-5 table used for market `base_payout`, encoded as a 4-bit index into the table above (see [Market Denomination](#market-denomination-1-2-5-table-4-bits)).

This gives 16 × 16 = 256 `(max_loss_sats, half_payout_sats)` combinations, encoded in 8 bits total (vs. 18 bits under the previous 26-mantissa × 16-exponent scheme). Value range: 100 to 10,000,000 sats per param.

**Why this range**: pools on L-BTC-denominated markets with `base_payout ≤ 10^7` sats and subsidies in the same range fit cleanly. Pools on larger-denomination markets (e.g., USDT with `base_payout = 10^8` or larger) would need an expanded table. Expanding the table in a future release is non-breaking: each new `(max_loss_sats, half_payout_sats)` combo gets its own Merkle root, and existing pools are unaffected by new table entries.

**Why not a separate table with wider range for pools**: consistency with the market encoding reduces the number of distinct denomination conventions in the protocol, simplifies decoders, and keeps the committed Merkle root set (one per combo) small. The 10^7 cap is a pragmatic v1 constraint, not a structural one.

### Well-Known Collateral Asset Index (4 bits)

The v1 mapping is keyed by network:

| Index | Liquid mainnet | Liquid testnet | Liquid regtest |
|---|---|---|---|
| `0` | L-BTC policy asset `6f0279e9ed041c3d710a9f57d0c02928416460c4b722ae3457a11eec381c526d` | Policy asset `144c654344aa716d6f3abcc1ca90e5641e4e2a7f633bc09fe3baf64585819a49` | Default regtest policy asset `5ac9f65c0efcc4775e0baec4ec03abdde22473cd3cf33c0419ca290e0751b225` |
| `1` | Liquid mainnet USDt `ce091c998b83c78bb71a632313ba3760f1763d9cfcffae02258ffa9865a37bd2` | **Unassigned in v1** — use escape `15` for non-policy collateral | **Unassigned in v1** — use escape `15` for non-policy collateral |
| `2-14` | Reserved for future well-known assets | Reserved for future well-known assets | Reserved for future well-known assets |
| `15` | Escape: full 32-byte asset ID follows | Escape: full 32-byte asset ID follows | Escape: full 32-byte asset ID follows |

Index `0` always means the selected network's policy asset. Index `1` is intentionally **Liquid-mainnet-only** in v1; builders on Liquid testnet and Liquid regtest must encode every non-policy collateral asset via escape `15`, and decoders should reject index `1` on those networks.

## OP_RETURN Encoding Specification

All recovery hints use zero-value OP_RETURN outputs. Data must be whole bytes (padded to byte boundaries). The type tag byte identifies the hint type and enables forward compatibility.

### Type Tag

V1 uses exact class-nibble assignments:

| Hint | `type_tag` | Meaning |
|---|---|---|
| Binary market | `0x10` | Market hint for the binary market contract family. Low nibble reserved, must be zero. |
| Multi-outcome market | `0x20` | Market hint for the multi-outcome contract family. Low nibble reserved, must be zero. |
| Pool | `0x30` | Pool hint. Low nibble reserved, must be zero. |
| Order: YES / SellBase | `0x40` | Order hint with class nibble `0x4`, side bit `0`, direction bit `0`, reserved bits `00`. |
| Order: YES / SellQuote | `0x44` | Order hint with class nibble `0x4`, side bit `0`, direction bit `1`, reserved bits `00`. |
| Order: NO / SellBase | `0x48` | Order hint with class nibble `0x4`, side bit `1`, direction bit `0`, reserved bits `00`. |
| Order: NO / SellQuote | `0x4C` | Order hint with class nibble `0x4`, side bit `1`, direction bit `1`, reserved bits `00`. |

The high nibble identifies the hint family. For market and pool hints, the low nibble is reserved and must be zero in v1. For order hints, the low nibble is structured as `[side(1)][direction(1)][reserved(2)]`, where `side = 0` means YES, `side = 1` means NO, `direction = 0` means SellBase, and `direction = 1` means SellQuote. All other byte values are reserved in v1.

The type tag is a **first-pass filter**, not a guarantee. Roughly 1 in 256 random OP_RETURNs match any given type tag value. Full verification (decode all fields, compile covenant, match script) is what confirms a hint is genuine.

### Market Hint

Binary and multi-outcome market hints share the same **37-byte** layout (69 bytes with exotic collateral). They are distinguished by the `type_tag` byte: `0x10` for binary markets and `0x20` for multi-outcome markets. The rest of the layout is identical:

```
Byte  0:     type_tag                                  --  8 bits
Bytes 1-32:  oracle_public_key                         -- 256 bits
Byte  33:    [collateral_asset(4)][base_payout(4)]          --  8 bits
Bytes 34-36: expiry_time (u24, big-endian)             -- 24 bits
                                                 Total: 296 bits = 37 bytes
```

If `collateral_asset` index = 15 (escape): 32 additional bytes of raw `collateral_asset_id` follow, totaling 69 bytes.

**Per-field justification:**

| Field | In hint | Size | Justification |
|---|---|---|---|
| `oracle_public_key` | Yes | 32 bytes | Not derivable — chosen by market creator |
| `collateral_asset_id` | Yes (indexed) | 4 bits | Not derivable — well-known index, escape for exotic |
| `base_payout` | Yes (indexed) | 4 bits | Not derivable — 1-2-5 convention, 16 values. Binary markets derive `cp = base_payout × 2`; multi-outcome markets derive `cp = base_payout × outcome_count`, with `outcome_count` recovered from the creation tx (see below). |
| `expiry_time` | Yes (absolute) | 3 bytes | Not derivable — u24 absolute encoding (see below) |
| `outcome_count` | **No** | — | **Derivable from creation tx issuance count** (multi-outcome only; binary is implicitly N=2) |
| `yes_token_asset_id(s)` | No | — | Derivable from creation tx issuance entropy |
| `no_token_asset_id(s)` | No | — | Derivable from creation tx issuance entropy |
| `yes_reissuance_token_id(s)` | No | — | Derivable from creation tx issuance entropy |
| `no_reissuance_token_id(s)` | No | — | Derivable from creation tx issuance entropy |

**Expiry time encoding** (u24): `encoded = expiry_time / 60` stored as 3 bytes big-endian. Recovery: `expiry_time = encoded × 60`. The PSET builder accepts any future height and **rounds `expiry_time` up to the next 60-block boundary** before constructing the covenant params; the covenant and returned params use that rounded value, making the encoding lossless. At Liquid's target rate of 1 block per minute, each unit represents approximately 1 hour. The u24 range (0 to 2^24 - 1 = 16,777,215) covers block heights from the Liquid genesis block (mined September 26, 2018) to approximately the year 3931, providing 1-hour granularity with ~1,900 years of headroom. This absolute encoding was chosen over a creation-block-relative delta because the creation block height is unknown at PSET build time (the transaction hasn't been broadcast yet), making delta-based recovery produce incorrect values due to confirmation drift.

**Deriving `outcome_count` for multi-outcome markets.** The creation tx mints 2N token pairs (one `AssetIssuance` per outcome's YES or NO leg), so `outcome_count = issuance_count / 2` where the count is over **new-issuance** `AssetIssuance` structures with both `amount` and `inflation_keys` non-null:

```rust
let issuance_count = creation_tx.input.iter()
    .filter(|inp| {
        inp.has_issuance()
        && inp.asset_issuance.asset_blinding_nonce == ZERO_TWEAK
        && !inp.asset_issuance.amount.is_null()
        && !inp.asset_issuance.inflation_keys.is_null()
    })
    .count();
let outcome_count = (issuance_count / 2) as u8;
```

The filter components:
- `has_issuance()` — returns true iff the input carries a non-null `AssetIssuance` record. Peg-ins use a separate bit on the prevout and are correctly excluded.
- `asset_blinding_nonce == ZERO_TWEAK` — selects *new* issuances. Reissuances (nonzero nonce) are excluded; same for any unrelated issuances that a non-conforming tx might carry.
- `amount` and `inflation_keys` both non-null — defensive filter that rules out asymmetric ("half-issuance") records. Elements consensus permits an `AssetIssuance` with one of the two null and the other set; the deadcat convention is that every market-creation issuance mints both an asset and its reissuance token. Asymmetric records would not be produced by `build_multi_outcome_market_creation_pset` and would fail covenant-script verification downstream, but the filter rejects them at count time for a clearer error.

The covenant script is the authoritative binding between N and the creation tx: if the derived `outcome_count` is wrong, the compiled covenant won't match any tx output and ingestion fails loudly. This makes the count-based derivation equivalent in correctness to storing `outcome_count` in the hint, but saves 1 byte and keeps binary and multi-outcome hint layouts unified.

**v1 support note**: the binary market remains a separate contract family. The multi-outcome contract supports `outcome_count ∈ {3, 4}` in v1; expansion to additional outcome counts later is non-breaking because each supported count gets its own generated contract artifact.

### Order Hint (40 bytes)

```
Byte  0:     [class=0x4][side(1)][direction(1)][reserved(2)] --  8 bits
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
| `price` | Yes | 3 bytes | Not derivable — u24, max ~16.8M; bounded by `cp = base_payout × N` for rational orders |
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

### Pool Hint (40 bytes)

```
Byte  0:      type_tag (`0x30`)                                  --  8 bits
Bytes 1-32:   market_creation_txid                               -- 256 bits
Byte  33:     [max_loss_idx(4)][half_payout_idx(4)]              --  8 bits
Byte  34:     fee_bps[11:4]                                      --  8 bits
Byte  35:     [fee_bps[3:0]][initial_s_index[15:12]]             --  8 bits
Byte  36:     initial_s_index[11:4]                              --  8 bits
Byte  37:     [initial_s_index[3:0]][masked_pool_index[15:12]]   --  8 bits
Byte  38:     masked_pool_index[11:4]                            --  8 bits
Byte  39:     [masked_pool_index[3:0]][reserved=0]               --  8 bits
                                                           Total: 320 bits = 40 bytes
```

Within each bracketed byte, the first nibble is the high nibble and the second nibble is the low nibble.

**Per-field justification:**

| Field | In hint | Size | Justification |
|---|---|---|---|
| `market_creation_txid` | Yes | 32 bytes | Chain-only recovery of market params |
| `max_loss_sats` | Yes (indexed) | 4 bits | Not derivable — 16-value 1-2-5 table (shared with market `base_payout` encoding) |
| `half_payout_sats` | Yes (indexed) | 4 bits | Not derivable — same 1-2-5 table |
| `fee_bps` | Yes | 12 bits | Not derivable — u12, 0.01% granularity, max 40.95% |
| `initial_s_index` | Yes | 16 bits | Not derivable without brute-force script matching (EC scalar mul per candidate); enables direct script verification during creation-tx ingestion |
| `pool_index` | Yes (masked) | 16 bits | Needed for admin key derivation; XOR-masked |
| `yes/no/collateral_asset_id` | No | — | Derivable from parent market params |
| `lmsr_table_root` | No | — | Derivable via deterministic table generation from `max_loss_sats` + `half_payout_sats` (see [lmsr-deterministic-table-spec.md](../contracts/lmsr-pool/lmsr-deterministic-table-spec.md)) |
| `q_step_lots` | No | — | Derivable from `b` and `half_payout_sats` |
| `admin_pubkey` | No | — | Derivable from mnemonic at `pool_index` |
| Protocol constants | No | — | `TABLE_DEPTH`, `S_BIAS`, `S_MAX_INDEX`, `MIN_POOL_RESERVE` are fixed in the `.simf` |

**Builder validation:**
- `max_loss_sats` and `half_payout_sats` must be valid indices into the 16-value 1-2-5 table
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

- `docs/architecture/deadcat-core-design.md` — main design doc (Wallet Recovery section references this doc)
- `docs/contracts/lmsr-pool/lmsr-pool-design.md` — LMSR pool design (deterministic table generation enables pool hint compression)
- `docs/protocol/deterministic-rt-blinding.md` — RT blinding factors are deterministic, eliminating anchor secrets from recovery
