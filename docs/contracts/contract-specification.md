# Contract Specification

This document specifies the Deadcat covenant contracts from the perspective of a `deadcat-core` implementor: planned parameter types, covenant structure, spend paths, and witness data. It consolidates information from the `.simf` source files and multiple satellite refactor docs into a single reference.

**This document describes the planned end state** — after all pending refactors are applied. The current SDK source may differ. See [Pending Refactors](#pending-refactors) for the delta.

## Contract Types at a Glance

Deadcat has **two prediction market contracts** plus two supporting contracts:

| Contract | Purpose | Use when |
|---|---|---|
| **Prediction Market (binary)** | YES/NO on a single event | Outcomes are binary, or the set of outcomes can change over time (composed at the app layer with oracle-discipline-enforced inter-market coherence) |
| **Multi-Outcome Market** | N mutually exclusive, exhaustive outcomes with per-outcome YES/NO tokens. Provides atomic cross-outcome primitives (split-YES, split-NO, cross-outcome swap) for efficient arb, LP rotation, and basket operations. | Outcome set is fixed and known at creation (e.g., Fed rate decision: raise/flat/lower/other, sports brackets, Oscar categories) |
| **LMSR Pool** | Binary automated market maker. Serves binary markets directly and per-outcome YES/NO pairs within multi-outcome markets via **Option C composition** (N binary LMSR pools per multi-outcome market). | Providing liquidity on any market — single pool contract type for both binary and multi-outcome |
| **Maker Order** | Limit order against a binary market's token | Fills at a fixed price, cancellable by the maker |

The two prediction market contracts **coexist by design**. Creators pick by event characteristics:

- **Known, exhaustive outcome set** → multi-outcome market contract. Gets atomic cross-outcome primitives, stronger oracle containment, single shared collateral UTXO.
- **Dynamic or non-exhaustive outcomes** (candidates dropping out, open-ended questions) → composed binary market contracts at the app layer.

Regardless of which market contract is used, **AMM liquidity is always provided via binary LMSR pools**. For multi-outcome markets, N binary LMSR pools are composed (one per outcome's YES/NO pair) and cross-outcome AMM coherence (`Σ p_YES_k = 1`) is arb-enforced. The multi-outcome market contract's cross-outcome primitives make that arb atomic (single-transaction closure of coherence gaps). See [multi-outcome/multi-outcome-market-contract.md](multi-outcome/multi-outcome-market-contract.md) for the full spec and [multi-outcome/amm-scoring-rule-tradeoffs.md](multi-outcome/amm-scoring-rule-tradeoffs.md) for the pool design decision.

Both market contracts uphold the same set of covenant-enforced principles (permissionlessness within the solvency invariant, narrow oracle authority, terminal-path completeness, RT destruction, sibling UTXO check, witness-parameterized indices, etc.). These are specified once in [market-contract-principles.md](market-contract-principles.md) and implemented by each contract. The per-contract specs below describe what is specific to each — token model, spend paths, parameters — and inherit the shared principles by reference.

## Prediction Market (Binary)

### Parameters

```rust
pub struct BinaryMarketParams {
    pub oracle_public_key: XOnlyPublicKey,      // BIP-340 Schnorr pubkey for oracle attestation
    pub collateral_asset_id: AssetId,           // L-BTC, USDt, or other Elements asset
    pub yes_token_asset_id: AssetId,            // derivable from creation tx issuance entropy
    pub no_token_asset_id: AssetId,             // derivable from creation tx issuance entropy
    pub yes_reissuance_token_id: AssetId,       // derivable from creation tx issuance entropy
    pub no_reissuance_token_id: AssetId,        // derivable from creation tx issuance entropy
    pub base_payout: u64,                       // primary denomination (1-2-5 table); cp = base_payout × 2 is the pair cost
    pub expiry_time: u32,                       // block height deadline (convention: rounded up to next 60-block boundary)
}
```

4 of 8 fields are derivable from the creation transaction's issuance entropy. The remaining 4 are stored in the OP_RETURN recovery hint. See [chain-only-recovery.md](../protocol/chain-only-recovery.md).

**Denomination model**: The primary covenant param is `base_payout` — the per-outcome YES-expiry payout unit. Binary markets derive `cp = base_payout × 2`. Multi-outcome markets derive `cp = base_payout × outcome_count`. This keeps the two market kinds on one denomination model without conflating the binary API's single outcome with the binary settlement multiplier of 2. See [multi-outcome-market-contract.md § Denomination model](multi-outcome/multi-outcome-market-contract.md#denomination-model) for the rationale.

**Unit convention**: All amounts (`base_payout`, `max_loss_sats`, `half_payout_sats`, token counts, reserve values) are denominated in the **smallest indivisible unit** of the respective asset — satoshis for L-BTC (10^-8 BTC), 10^-8 for USDt on Liquid, etc. The `_sats` suffix on some fields reflects the L-BTC-as-canonical-example convention, not an L-BTC-only restriction. Protocol constants like `MIN_POOL_RESERVE = 1,000` are in smallest units regardless of asset.

Builder validates: `base_payout` in the 1-2-5 table, accepts any future `expiry_time`, rounds it up to the next 60-block boundary, and validates the rounded value against the recovery conventions. `collateral_asset_id` must be in the well-known set or exotic-escape-compatible.

### Covenant Structure

8 slots across 5 covenant phases:

| Slot | Phase | Purpose |
|---|---|---|
| 0: DormantYesRt | Dormant | YES reissuance token (0 outstanding pairs) |
| 1: DormantNoRt | Dormant | NO reissuance token (0 outstanding pairs) |
| 2: UnresolvedYesRt | Unresolved | YES reissuance token (>0 outstanding pairs) |
| 3: UnresolvedNoRt | Unresolved | NO reissuance token (>0 outstanding pairs) |
| 4: UnresolvedCollateral | Unresolved | Locked collateral |
| 5: ResolvedYesCollateral | ResolvedYes | Collateral (YES won, awaiting redemption) |
| 6: ResolvedNoCollateral | ResolvedNo | Collateral (NO won, awaiting redemption) |
| 7: ExpiredCollateral | Expired | Collateral (expired, awaiting redemption) |

Each slot has a unique script pubkey derived from the contract params + slot identity. All 8 scripts are static (computable at ingestion time) and pre-stored for script-based chain sync.

### Spend Paths

| Transition | From slots | To slots | Authorization | Covenant enforces |
|---|---|---|---|---|
| Initial issuance | 0, 1 | 2, 3, 4 | RT spend | Collateral = pairs × cp, where cp = base_payout × 2 |
| Subsequent issuance | 2, 3, 4 | 2, 3, 4 | RT spend | Collateral increased by pairs × cp; sibling UTXO check |
| Partial cancellation | 2, 3, 4 | 2, 3, 4 | RT spend + token burn | Collateral decreased, tokens burned; sibling UTXO check |
| Full cancellation | 2, 3, 4 | 0, 1 | RT spend + token burn | All collateral returned, all tokens burned; sibling UTXO check |
| Resolution (YES) | 2, 3, 4 | 5 | Oracle BIP-340 signature | Oracle signs tagged hash of market_id + outcome; RT burn outputs verified at unspendable script with correct commitment; sibling UTXO check |
| Resolution (NO) | 2, 3, 4 | 6 | Oracle BIP-340 signature | Same |
| Redemption (post-YES) | 5 | none | Token burn | YES tokens burned, collateral released at full value |
| Redemption (post-NO) | 6 | none | Token burn | NO tokens burned, collateral released at full value |
| Redemption (expired) | 7 | none | Token burn | Any tokens burned, collateral released at half value |
| Expiry | 2, 3, 4 | 7 | Timelock >= expiry_time | No signature required; RT burn outputs verified at unspendable script with correct commitment; sibling UTXO check |
| Dormant resolution (YES) | 0, 1 | none | Oracle BIP-340 signature | Both RTs consumed, no outputs |
| Dormant resolution (NO) | 0, 1 | none | Oracle BIP-340 signature | Both RTs consumed, no outputs |
| Dormant expiry | 0, 1 | none | Timelock >= expiry_time | Both RTs consumed, no outputs |

**Sibling UTXO check**: All transitions that co-spend RTs and collateral verify that the three covenant inputs were created in the same transaction (`input_prev_outpoint` txid match across all three). This prevents collateral substitution — an attacker cannot create a fake collateral UTXO at the covenant script address and swap it in for the real one, because the fake UTXO's `prev_txid` won't match the RTs'. See [enforcement-layers.md](../architecture/enforcement-layers.md) for the full attack analysis.

This is a pending refactor with two parts: (1) add the `prev_txid` check to all paths that co-spend RTs and collateral, and (2) change partial cancellation to co-spend RTs. Part 2 is required because the current partial cancellation only spends the collateral slot — after such a cancellation, the collateral's `prev_txid` would differ from the RTs' (it was created in the cancellation tx, while the RTs were last created in the prior issuance tx). Co-spending the RTs during partial cancellation ensures all three outputs are always born in the same transaction.

### Oracle Attestation

BIP-340 tagged hash with tag `"deadcat/oracle_attestation"`:
```
message = tagged_hash("deadcat/oracle_attestation", market_id || outcome_byte)
market_id = SHA256(yes_token_asset_id || no_token_asset_id)
outcome_byte = 0x01 (YES) or 0x00 (NO)
```

See [oracle-bip340-tagged-hash.md](../protocol/oracle-bip340-tagged-hash.md).

### Witness Data

For **dormant terminal paths** (resolution/expiry from 0 outstanding pairs): both RT inputs are spent with no covenant outputs. The three-way ambiguity (YES/NO/Expired) is resolved via `RedeemNode::decode` on the witness — the spend path identifies which transition occurred.

All other transitions are detectable from script pubkey matching alone (8 unique scripts).

## Multi-Outcome Market

Generalizes the binary market to N mutually exclusive, exhaustive outcomes with **per-outcome YES/NO tokens** (2N tokens total). This contract is appropriate when the outcome set is fixed and known at creation time. Its primary value is the **atomic cross-outcome primitives** (split-YES, split-NO, cross-outcome swap), which enable efficient arb, LP rotation, and basket operations that composed binary markets cannot provide atomically.

**AMM liquidity** for a multi-outcome market comes from **Option C composition**: N binary LMSR pools, one per outcome's YES/NO pair. Cross-outcome AMM coherence (`Σ p_YES_k = 1`) is arb-enforced, with arbitrageurs using the market contract's cross-outcome primitives to close gaps in a single atomic transaction.

For the full spec, design rationale, and alternatives considered, see [multi-outcome/multi-outcome-market-contract.md](multi-outcome/multi-outcome-market-contract.md).

### Parameters

```rust
pub struct MultiOutcomeMarketParams {
    pub oracle_public_key: XOnlyPublicKey,
    pub collateral_asset_id: AssetId,
    pub yes_token_asset_ids: [AssetId; N],       // derivable from creation tx
    pub no_token_asset_ids: [AssetId; N],        // derivable from creation tx
    pub yes_rt_asset_ids: [AssetId; N],          // derivable from creation tx
    pub no_rt_asset_ids: [AssetId; N],           // derivable from creation tx
    pub base_payout: u64,                        // primary denomination (1-2-5 table); cp = base_payout × N is the pair cost
    pub expiry_time: u32,                        // 60-block boundary
    pub outcome_count: u8,                       // N
}
```

Builder validates: `N` in supported range (**v1: N ∈ {3, 4}**), `base_payout` in 1-2-5 table, `expiry_time` on 60-block boundary. Expiry-redemption divisibility is structural under this model (`cp = base_payout × N` is trivially divisible by N), so no `cp mod N == 0` check is required at builder or covenant level.

### Covenant Structure

**5N + 2 slots**:
- 2N Dormant RTs (YES_i and NO_i, one per outcome)
- 2N Unresolved RTs (YES_i and NO_i)
- 1 Unresolved collateral
- N Resolved_k collateral slots (one per winning outcome)
- 1 Expired collateral

| N | Slot count |
|---|---|
| 3 | 17 |
| 5 | 27 |
| 8 | 42 |
| 10 | 52 |

### Token Model and Solvency Invariant

For outcome k winning, `YES_k` and `NO_j` (for all j ≠ k) each redeem for `cp = base_payout × N`. Pre-resolution, the contract maintains an outcome-independent quantity `Q = y_k + sum_{j≠k} n_j` (same value for every k), with collateral `C = cp × Q`. All supported operations preserve outcome-independence of Q.

### Spend Paths (summary)

Formulas below use `cp := base_payout × N` as a derivation shorthand.

Per-outcome operations:
- **Issue pair (outcome i)**: mint `sets` of YES_i and sets of NO_i, lock `sets × cp`.
- **Cancel pair (outcome i)**: burn sets of YES_i and sets of NO_i, release `sets × cp`.

Cross-outcome operations:
- **Split YES** / **Merge YES**: mint/burn `sets` of each YES_i, for `sets × cp`.
- **Split NO** / **Merge NO**: mint/burn `sets` of each NO_i, for `sets × (N-1) × cp`.

Resolution/redemption:
- **Resolution (outcome k)**: oracle signs u8 outcome_index; all 2N RTs burned; collateral moves to Resolved_k slot.
- **Redemption**: YES_k and NO_j (j ≠ k) each redeem for `cp = base_payout × N`.
- **Expiry redemption**: YES_i redeems for `base_payout`; NO_i redeems for `base_payout × (N-1)`. Both are exact integers by construction. Uniform 1/N outcome probability assumption keeps the rate solvency-preserving.

All Unresolved-phase transitions co-spend all **2N+1 covenant inputs** (all RTs + collateral) to maintain the sibling UTXO check.

### Oracle Attestation

```
message = tagged_hash("deadcat/oracle_attestation", market_id || outcome_index)
market_id = SHA256(yes_token_asset_ids[0] || no_token_asset_ids[0] || ... || yes_token_asset_ids[N-1] || no_token_asset_ids[N-1])
outcome_index = u8 in [0, N-1]
```

Tag string matches the binary market. Domain separation is via `market_id`.

### Code Generation

Each supported N has its own `.simf` file generated from a template by `deadcat-codegen` at **dev time**. The generated `.simf` sources are **committed to the repo** under `crates/deadcat-core/contracts/multi_outcome/`, and the compiled Simplicity artifacts (`src/artifacts/*.rs`, produced by `simplex build`) are also committed. Runtime reads these via `include_bytes!` and imports the typed artifact modules directly — no template instantiation or compilation happens at `cargo build` time. Regeneration is triggered explicitly by developers via `just generate-simf` + `just regenerate-artifacts` when an intentional change is being committed; drift tests catch stale committed outputs on every `cargo test`. **v1 supports N ∈ {3, 4}**. Expansion to larger N is non-breaking (new N → new template instantiation → new committed `.simf` + artifacts → new contract; existing contracts unaffected). Above N=10, transaction weight from the 2N+1-way covenant co-spend becomes uncomfortable; large-N events should use hierarchical composition (markets-of-markets) or binary-market composition with arbitrage-based coherency.

**The binary market stays separate from the multi-outcome template.** `prediction_market.simf` is the canonical binary contract and is not regenerated from the multi-outcome template. The template serves markets with 3 or more outcomes only in v1. See [multi-outcome-market-contract.md](multi-outcome/multi-outcome-market-contract.md).

## LMSR Pool

**The single AMM pool contract in deadcat.** One binary LMSR pool serves both single-event binary markets and per-outcome YES/NO pairs within multi-outcome markets (via Option C composition — N pools per multi-outcome market, one per outcome). The pool contract doesn't know or care which market contract type underlies its YES/NO tokens. See [multi-outcome/amm-scoring-rule-tradeoffs.md](multi-outcome/amm-scoring-rule-tradeoffs.md) for the pool design decision.

**Liquidity model**: admin-operated, permissionless creation. Each pool has a single operator who chooses parameters, provides subsidy, can adjust reserves and close via admin-signed spend paths, earns fees, and bears impermanent loss. Anyone can deploy a pool on any market with any parameters; multiple competing pools per market are expected. LP-tokenized pools are deferred to v2.

### Parameters

```rust
pub struct LmsrPoolParams {
    pub yes_asset_id: AssetId,              // from parent market
    pub no_asset_id: AssetId,               // from parent market
    pub collateral_asset_id: AssetId,       // from parent market
    pub lmsr_table_root: [u8; 32],          // derived: Merkle root of F-value table
    pub q_step_lots: u64,                   // derived from b and half_payout_sats
    pub half_payout_sats: u64,              // creator-specified (convention: 16-value 1-2-5 table, shared with market base_payout encoding)
    pub fee_bps: u16,                       // creator-specified public API type (convention: <= 4,095; widened internally for Simplicity arithmetic)
    pub admin_pubkey: XOnlyPublicKey,       // from mnemonic at pool_index
    pub max_loss_sats: u64,                 // NOT a covenant param — needed for off-chain LMSR math (b derivation, cached-table quoting, table generation)
}
```

9 fields (down from 14 in current SDK). See [lmsr-pool-design.md](lmsr-pool/lmsr-pool-design.md) for the full parameter design, derivation formulas, and protocol constants. Note: `max_loss_sats` is not a covenant parameter (the covenant only verifies Merkle proofs, never evaluates the cost function). It is included in the struct because all off-chain LMSR computation requires `b = max_loss_sats / ln(2)`, and `b` is not recoverable from the covenant params alone. The other two derived fields (`q_step_lots`, `lmsr_table_root`) are retained as compilation caches.

**Removed from params** (now protocol constants in the `.simf`):
- `table_depth` → `TABLE_DEPTH = 16`
- `s_bias` → `S_BIAS = 32,768`
- `s_max_index` → `S_MAX_INDEX = 65,535`
- `min_r_yes`, `min_r_no`, `min_r_collateral` → `MIN_POOL_RESERVE = 1,000` sats each

**Not in params** (derived on demand from `max_loss_sats`): `b`. Unlike `q_step_lots` and `lmsr_table_root` (which are covenant params and compilation caches), `b` is only used transiently during LMSR math and table generation — deriving it from `max_loss_sats` is trivial.

**Renamed**: `cosigner_pubkey` → `admin_pubkey`. See [maker-order-remove-cosigner.md](maker-order/maker-order-remove-cosigner.md).

### Covenant Structure

3 reserve UTXOs (YES, NO, Collateral) sharing a script pubkey that encodes the current `s_index`. The taproot internal key is NUMS (key-spend unspendable). The taproot tree has constant Simplicity program leaves (same CMR regardless of `s_index`) and a variable `tapdata_leaf = TaggedHash("TapData", s_index.to_be_bytes())`. When `s_index` changes (swap), only the tapdata leaf changes — the Simplicity programs and their CMRs are constant for given pool params. This means computing a pool's script pubkey for a given `s_index` requires one Simplicity compilation (to get the constant program CMRs) plus lightweight hashing and an EC scalar multiplication (for the taproot tweak).

Swap and admin paths produce three consecutive reserve outputs in fixed order, as enforced by the covenant: YES (index N), NO (index N+1), Collateral (index N+2), all sharing the same script pubkey encoding the current/new `s_index`.

### Spend Paths

| Path | Authorization | s_index | Covenant enforces |
|---|---|---|---|
| Swap | Permissionless | Changes | Merkle proofs for F(old_s) and F(new_s), collateral conservation with fee inequality, reserve minimums, correct trade direction |
| Admin adjust | Admin key signature | Frozen | YES and NO deltas must be equal, reserve minimums maintained |
| Close | Admin key signature | N/A | All 3 reserve UTXOs consumed atomically, no new covenant outputs |

See [lmsr-pool-close-path.md](lmsr-pool/lmsr-pool-close-path.md) for the close path specification.

### Witness Data

All pool transitions use **witness-based detection** via `RedeemNode::decode`:
- **Swap vs Admin**: Distinguished by spend path in the witness. s_index change confirmed by witness (swap: old != new, admin: old == new).
- **Close**: Spend path confirmed as close by the witness. No covenant outputs.
- **s_index extraction**: The witness contains the authoritative `old_s_index` and `new_s_index`. This is ground truth — reserve-based reverse lookup is fragile after admin adjustments.

### LMSR Math: Cached Tables

The v1 implementation uses the deterministic full-table output everywhere — quoting, proof generation, and ingestion verification. `deadcat-core` caches full F-value tables keyed by `(max_loss_sats, half_payout_sats)`: the first use of a combo incurs the bignum cold-start cost, and subsequent operations are O(1) lookups against the cached table. Routing still remains reserve-aware: the cached table determines curve pricing, while live reserves determine currently fillable volume. See [lmsr-pool-design.md](lmsr-pool/lmsr-pool-design.md).

## Maker Order

### Parameters

```rust
pub enum OrderDirection {
    SellBase,   // Maker offers outcome tokens, wants collateral
    SellQuote,  // Maker offers collateral, wants outcome tokens
}

pub struct MakerOrderParams {
    pub base_asset_id: AssetId,             // YES or NO token from parent market
    pub quote_asset_id: AssetId,            // collateral asset from parent market
    pub price: u64,                         // quote units per base unit (convention: <= 0xFFFFFF = u24 max)
    pub min_fill_lots: u64,                 // minimum base units per fill (convention: 1-255)
    pub min_remainder_lots: u64,            // minimum base units remaining after partial fill (convention: 1-255)
    pub direction: OrderDirection,
    pub maker_receive_spk_hash: [u8; 32],   // SHA256 of maker's P2TR receive scriptPubKey
    pub maker_pubkey: XOnlyPublicKey,       // maker's x-only pubkey (taproot internal key for cancel)
}
```

8 fields (down from 9 in current SDK). **Removed**: `cosigner_pubkey`. See [maker-order-remove-cosigner.md](maker-order/maker-order-remove-cosigner.md).

`maker_receive_spk_hash` is derived from a deterministic nonce chain: `deadcat_secret_key` + `order_index` → `order_nonce` → `order_uid` → `tweak` → `P_order` → scriptPubKey → SHA256. See [chain-only-recovery.md](../protocol/chain-only-recovery.md) for the full derivation.

### Covenant Structure

Single UTXO at a covenant address (taproot with Simplicity script). The taproot internal key is the maker's `maker_pubkey` — key-spend is available for cancellation.

### Spend Paths

| Path | Mechanism | Authorization | Covenant enforces |
|---|---|---|---|
| Fill (partial) | Simplicity script-spend | Permissionless | `consumed x PRICE` paid to maker, remainder locked at same script, min_fill and min_remainder constraints |
| Fill (complete) | Simplicity script-spend | Permissionless | `input_amount x PRICE` paid to maker, no remainder output, min_fill constraint |
| Cancel | Taproot key-spend | Maker signature | No constraints (maker reclaims funds freely) |

**Dependency**: This detection model (key-spend = cancel, script-spend = fill) requires the script-cancel refactor — see [maker-order-remove-script-cancel.md](maker-order/maker-order-remove-script-cancel.md).

### Witness Data

Order detection uses **taproot structural checks**, not Simplicity witness decoding:
- Strip optional annex (if >= 2 elements and last starts with byte `0x50`)
- 1 remaining element = key-spend = cancellation
- 3 remaining elements = Simplicity script-spend = fill (partial or complete determined by whether a new covenant output exists)

## Protocol Constants

| Constant | Value | Defined in |
|---|---|---|
| `TABLE_DEPTH` | 16 | `lmsr_pool.simf` |
| `S_BIAS` | 32,768 | `lmsr_pool.simf` |
| `S_MAX_INDEX` | 65,535 | `lmsr_pool.simf` |
| `MIN_POOL_RESERVE` | 1,000 sats | `lmsr_pool.simf` (applied to all 3 reserves) |
| NUMS key | `0x50929b74c1a04954b78b4b6035e97a5e078a5a0f28ec96d547bfee9ace803ac0` | All contracts (markets, pools) |

**NUMS key**: "Nothing Up My Sleeve" — a point on the secp256k1 curve with no known discrete logarithm, derived by hashing a fixed string to a curve point. This is the standard NUMS point used across the Bitcoin/Liquid ecosystem (same value used by BIP-341 for the unspendable internal key). Key-spend with this internal key is cryptographically infeasible, forcing all spends through the script path (Simplicity covenant). Maker orders use the maker's real public key instead — key-spend is their cancellation mechanism.

## Pending Refactors

These changes are specified in satellite docs but not yet applied to the `.simf` source files:

| Refactor | Satellite doc | Status | Blocks |
|---|---|---|---|
| `collateral_per_token` → `base_payout` (primary denomination; `cp = base_payout × N` derived) | [collateral-per-pair-refactor.md](prediction-market/collateral-per-pair-refactor.md) | Pending | Market contract, market params (binary and multi-outcome). Supersedes the intermediate `collateral_per_pair` rename — going directly to `base_payout` unifies binary and multi-outcome denomination and makes expiry-redemption divisibility structural. |
| Oracle BIP-340 tagged hash | [oracle-bip340-tagged-hash.md](../protocol/oracle-bip340-tagged-hash.md) | Pending | Market contract, oracle attestation |
| Remove cosigner from order fill path | [maker-order-remove-cosigner.md](maker-order/maker-order-remove-cosigner.md) | Pending | Order contract, order params |
| Rename pool `COSIGNER_PUBKEY` → `ADMIN_PUBKEY` | [maker-order-remove-cosigner.md](maker-order/maker-order-remove-cosigner.md) | Pending | Pool contract, pool params |
| Remove order script-cancel path | [maker-order-remove-script-cancel.md](maker-order/maker-order-remove-script-cancel.md) | Pending | Order contract |
| Add pool close script path | [lmsr-pool-close-path.md](lmsr-pool/lmsr-pool-close-path.md) | Pending | Pool contract |
| Pool params → protocol constants | [lmsr-pool-design.md](lmsr-pool/lmsr-pool-design.md) | Pending | Pool contract |
| Deterministic F-value computation (bignum runtime + committed reference Merkle roots) | [lmsr-deterministic-table-spec.md](lmsr-pool/lmsr-deterministic-table-spec.md) | Specified — see B1 resolution in the design docs. Runtime implementation in `deadcat-core` uses `num-bigint` + `num-rational` at arbitrary precision; reference generator and committed fixtures live in `deadcat-codegen`. | Pool math |
| Pool denomination: 26-mantissa × 16-exponent → 16-value 1-2-5 table (shared with market encoding) | [chain-only-recovery.md § Pool Denomination](../protocol/chain-only-recovery.md#pool-denomination-1-2-5-table-4-bits-each) | Specified | Pool params, OP_RETURN hint (41 → 40 bytes) |
| Covenant-enforced deterministic RT blinding | [deterministic-rt-blinding.md](../protocol/deterministic-rt-blinding.md) | Pending | Market contract (ABF enforcement, CBF pass-through, `verify_token_commitment` refactor) |
| Dormant terminal paths (resolution + expiry from zero pairs) | [market-dormant-terminal-paths.md](prediction-market/market-dormant-terminal-paths.md) | Pending | Market contract (DormantYesRt and DormantNoRt slot programs) |
| Order remainder witness-parameterization | [transaction-composability-model.md](../architecture/transaction-composability-model.md) | Pending | Order contract (`remainder_idx` from witness instead of `current_index() + 1`) |
| Sibling UTXO check + partial cancellation RT co-spend | [enforcement-layers.md](../architecture/enforcement-layers.md) | Pending | Market contract (add `prev_txid` match on all RT+collateral co-spend paths; partial cancellation must co-spend RTs to maintain sibling invariant) |
| Burn script: P2WSH → OP_RETURN | [enforcement-layers.md](../architecture/enforcement-layers.md) | Pending | Market contract (`ensure_blinded_reissuance_burn_output` checks bare OP_RETURN script hash instead of P2WSH hash). Rationale: consensus-level unspendability, UTXO set pruning. Blinded OP_RETURN confirmed supported on Elements. |

**Implementation order**: The `.simf` refactors should be applied before implementing `deadcat-core`. The core implementation is specified against the planned end state.

## Key Files

- `src-tauri/crates/deadcat-sdk/contract/prediction_market.simf` — market covenant source
- `src-tauri/crates/deadcat-sdk/contract/lmsr_pool.simf` — pool covenant source
- `src-tauri/crates/deadcat-sdk/contract/maker_order.simf` — order covenant source
- `src-tauri/crates/deadcat-sdk/src/prediction_market/params.rs` — current market params (pre-refactor)
- `src-tauri/crates/deadcat-sdk/src/lmsr_pool/params.rs` — current pool params (pre-refactor)
- `src-tauri/crates/deadcat-sdk/src/maker_order/params.rs` — current order params (pre-refactor)
