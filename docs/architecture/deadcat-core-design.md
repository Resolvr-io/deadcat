# deadcat-core Design Document

## Purpose

`deadcat-core` is a pure computation library for interacting with Deadcat prediction market covenants on Liquid/Elements. It enables any wallet or application to create, track, interpret, and transact with prediction markets (binary and multi-outcome), LMSR pools, and limit orders — without prescribing how chain data is fetched, how state is persisted, or how keys are managed.

The primary motivating use case: integrating Deadcat functionality into existing wallets like Aqua, which already have their own wallet backend, chain connection, signer, and state management. These wallets need the covenant logic without an opinionated runtime.

**Contract scope**: `deadcat-core` supports two market contract types (binary and multi-outcome) plus one pool contract type (binary LMSR pool, used both directly for binary markets and via Option C composition for multi-outcome markets — see [amm-scoring-rule-tradeoffs.md](../contracts/multi-outcome/amm-scoring-rule-tradeoffs.md) and [multi-outcome-market-contract.md](../contracts/multi-outcome/multi-outcome-market-contract.md)) plus the maker order contract. The unified API (see [Option E decision in Design Decisions Log](#multi-outcome-market-support-option-e-unified-api-enum-dispatched-internals)) exposes per-market operations via the `Market` view type, with multi-outcome-specific operations accessible via `Market::as_multi_outcome()` type-level specialization.

**Implementation target**: This document is the implementation target for `deadcat-core`. Some legacy `deadcat-sdk` covenant/source files still need to be brought into line with this spec (collateral-per-pair rename, oracle BIP-340 tagged hash, cosigner removal, script-cancel removal, pool close path addition, pool param constants, plus the new multi-outcome market contract). See [contract-specification.md § Legacy Source Alignment Checklist](../contracts/contract-specification.md#legacy-source-alignment-checklist) for that migration checklist; it does not indicate unresolved protocol behavior in this document.

**Scope**: this document describes the unified API for both binary and multi-outcome markets. The type system uses umbrella enums (`MarketParams`, `MarketState`, `MarketTransition`) over Binary/MultiOutcome-paired inner types, a `MarketResolution` discriminated union for oracle APIs, view types (`Market<'a, S>`, `Pool<'a, S>`, `Order<'a, S>`) that cache state and enforce freshness via lifetimes, and a transition-classification model where each on-chain transaction maps to exactly one covenant spend path. Multi-outcome markets use a single generic solvency-preservation spend path for all Unresolved-phase operations (see [`multi-outcome-market-contract.md § Operations`](../contracts/multi-outcome/multi-outcome-market-contract.md#operations)); the engine pattern-matches observed tx deltas into named `MultiOutcomeMarketTransition` variants (including `CrossOutcomeSwap` as a single-tx primitive) or falls back to `Composite` for arbitrary delta shapes.

## Architecture Overview

### Layer Separation

```
deadcat-core     Pure computation. No IO, no wallet, no chain, no contract discovery.
                 Contains: contract compilation, PSET builders, state machine,
                 LMSR math, transaction interpretation, asset identification.
                 Fully encapsulates Simplicity — consumers never see compiled
                 contracts, CMRs, taproot trees, or witness encoding.

deadcat-sdk      Opinionated wallet integration. Wraps core with a specific
                 wallet backend (LWK), chain backend (Electrum), and signer.
                 Provides the "prepare" pattern: sync wallet -> provide UTXOs
                 + fee rate to core engine -> sign returned PSET -> broadcast.
                 Imports deadcat-core.

deadcat-node     Full batteries-included runtime. Wraps SDK with Nostr discovery,
                 SQLite persistence, background sync, Boltz Lightning integration.
                 What Deadcat Live uses. Imports deadcat-sdk.
```

### What Each Layer Owns

| Capability                             | deadcat-core   | deadcat-sdk    | deadcat-node       |
| -------------------------------------- | -------------- | -------------- | ------------------ |
| Simplicity contract compilation        | Yes (internal) |                |                    |
| PSET construction (incl. coin select.) | Yes            |                |                    |
| LMSR math (quotes, spot price, tables) | Yes            |                |                    |
| Contract state machine                 | Yes            |                |                    |
| Transaction interpretation             | Yes            |                |                    |
| Asset identification                   | Yes            |                |                    |
| Trade routing + quoting               | Yes            |                |                    |
| Fee computation (from fee rate)        | Yes            |                |                    |
| ContractStore trait (required)         | Yes (defines)  |                | Yes (SQLite impl)  |
| ContractHistory trait (optional)       | Yes (defines)  |                | Yes (SQLite impl)  |
| Wallet (UTXO source, signer)          |                | Yes (LWK)      |                    |
| Chain backend (scan, broadcast)        |                | Yes (Electrum) |                    |
| Fee rate estimation                    |                | Yes            |                    |
| Nostr discovery                        |                |                | Yes                |
| Background sync                        |                |                | Yes                |
| Boltz swap integration                 |                |                | Yes                |

Note: `deadcat-core` defines the `ContractStore` and `ContractHistory` traits. `deadcat-node` provides concrete SQLite implementations of both. The distinction: core defines the interfaces, node provides storage implementations. A consumer like Aqua would implement these traits against their own database.

## System Invariants

Load-bearing guarantees that the `deadcat-core` library exposes and depends on. Each is stated as a property of the system, with the enforcement mechanism cross-referenced. These are the checklist against which both the library and every `.simf` contract must be audited — if an implementation conflicts with an invariant, the implementation is wrong.

### Script uniqueness per live contract

Every live maker order UTXO has a unique covenant script; every live LMSR pool reserve UTXO has a unique covenant script (per reserve role); every market has unique covenant scripts across its slot layout. Orders and pools achieve uniqueness via per-index derivation of `maker_pubkey` / `admin_pubkey` and per-index HMAC-derived nonces — the wallet increments `order_index` / `pool_index` per contract. Markets achieve it via 2N issuance-entropy-derived asset IDs, unique by Elements consensus. CMR collision across live contracts is structurally prevented. See [chain-only-recovery.md § Key Derivation](../protocol/chain-only-recovery.md#key-derivation), [§ Order Nonce Derivation](../protocol/chain-only-recovery.md#order-nonce-derivation), and [transaction-composability-model.md § Script Uniqueness Guarantee](transaction-composability-model.md#script-uniqueness-guarantee).

### Covenants self-enforce

Covenants verify their own correctness against arbitrary transactions. Any constraint that protects funds, preserves solvency, or prevents griefing is enforced by the Simplicity program, not by builder conventions. Builder-layer constraints are confined to recovery decodability (bucket 2 of the self-enforcement classification) and never fund safety (bucket 3 is forbidden). See [market-contract-principles.md § Covenant self-enforcement](../contracts/market-contract-principles.md#covenant-self-enforcement).

### Sibling group atomicity

Every market transition in active phases (Unresolved, Dormant) spends all covenant UTXOs in its sibling group atomically. The covenant rejects partial spends that would leave orphaned RTs or orphaned collateral. Partial-cancel / partial-burn transitions are no exception — they still co-spend the full sibling set to maintain the `prev_txid`-equality invariant across the contract's lifetime. See [market-contract-principles.md § Principle 13](../contracts/market-contract-principles.md#13-sibling-utxo-check-on-co-spent-covenant-inputs) and [enforcement-layers.md](enforcement-layers.md).

### One covenant spend path per on-chain transaction

Each on-chain transaction that touches a Deadcat covenant exercises exactly one covenant spend path. Transition classification is deterministic: the engine pattern-matches the tx's observable effects (RT issuance, burn outputs, collateral delta) to a single named variant — binary primitives for the binary market, or one of the generic-path delta-shape classifications (`IssuedPair`, `SplitYes`, `CrossOutcomeSwap`, `Composite`, etc.) for the multi-outcome market. This enables unambiguous indexing, replay, and interpretation.

### Deterministic reconstructibility — owner-level

For any contract the user created (market, pool, order), mnemonic + authoritative chain data suffice to reconstruct all covenant parameters and private material (keys, nonces, masked indices) required to recover custody, sign cancellations, or exercise admin paths. No off-chain backup is required beyond the mnemonic. See [chain-only-recovery.md](../protocol/chain-only-recovery.md).

### Deterministic reconstructibility — non-owner-level

For any Deadcat contract on-chain, regardless of creator, the creation transaction plus its OP_RETURN hint suffice for any node to parameterize the contract, verify script pubkey authenticity via re-derivation, and construct transactions that interact with it (trade, redeem, observe). Non-owners cannot reconstruct the owner's private material but do not need it for interaction. This property enables permissionless discovery and participation.

### OP_RETURN authenticity is verifiable

Every covenant creation hint is reverse-verifiable: a parameter set parsed from an OP_RETURN, re-compiled to a covenant script, must match the UTXO's on-chain script pubkey. Spoofed or stale hints produce a compile-then-compare failure and are rejectable at the recovery layer before any downstream state is trusted.

### RT deterministic blinding

Reissuance token continuation outputs use covenant-enforced deterministic blinding factors (ABF derived from tagged hash of the defining outpoint; CBF passed through unchanged; VBF computed as `CBF - ABF`). This removes the traditional Elements-layer RT-secrecy safeguard deliberately — enabling permissionless recovery and transaction construction — and makes the covenant's enforcement the sole defense against blinding-griefing. See [market-contract-principles.md § Principle 11](../contracts/market-contract-principles.md#11-deterministic-rt-blinding) and [deterministic-rt-blinding.md](../protocol/deterministic-rt-blinding.md).

### View freshness via lifetimes

Public view types (`Market<'a, S>`, `Pool<'a, S>`, `Order<'a, S>`, `MultiOutcomeMarket<'a, S>`) carry the store's lifetime. The borrow checker prevents a caller from holding a view across a mutation of the underlying store, eliminating the stale-view class of bugs without runtime checks. No freshness flags, no revalidation API — the type system enforces it.

## Design Principles

Policies that shape the `deadcat-core` public API. Invariants (above) are correctness constraints the system must uphold; principles are discretion constraints — they govern what the API chooses to offer vs. what it intentionally omits.

### Engine gates covenant-invalidity and impossibility, not unfavorability

`deadcat-core` provides operations that are **covenant-valid, possible, and not strictly dominated for the caller's role in that invocation**. An operation is "strictly dominated" when there is always a better way to achieve the same goal — the engine's router, for example, picks the best-price path rather than offering suboptimal alternatives.

When a helper exists only to choose a default among multiple covenant-valid ways to reach substantially the same outcome, core may return a canonical recommendation. But it does **not** collapse distinct target states into one "approved" path. For LMSR pools, for example, `estimate_bootstrap` can recommend a lean default reserve vector while `build_lmsr_bootstrap_pset` still accepts explicit caller-chosen reserves.

Operations may be unfavorable for counterparties. An informed trader dumping post-resolution tokens harms the pool operator; a taker filling a maker's order may not be what the maker wants post-resolution. That's inherent to adversarial markets. The engine's job is to serve each caller's role in their invocation cleanly; counterparties protect themselves through their own actions (pool operators close pools after resolution; makers cancel stale orders) or through covenant-level invariants.

`CoreError` variants reflect this boundary:
- **Structural caller mismatch**: `InvalidParams` (wrong API-shape input such as a binary/multi-outcome resolution mismatch)
- **Canonicality / recovery boundary**: `ConventionViolation` (outside the canonical v1 recovery conventions), `ParentMarketNotTracked` (referenced market absent from the tracked set), `InvalidCreationTx` (on-chain tx doesn't match the claimed params)
- **Cryptographic / covenant validity**: `OracleSignatureInvalid`, `InvalidContractState`, `CovenantInvariantViolation`
- **Impossibility**: `NoLiquidity` (can't fill any positive amount), `InsufficientFunds` (wallet can't cover)
- **Information integrity**: `StaleQuote` (cached state no longer accurate), `ContractAlreadyTracked` (caller bug)

No variant exists for "this is valid and possible but we refuse because it's inadvisable." Such a refusal would provide false safety against adversarial actors (who fork or bypass core) while adding friction for legitimate edge cases.

### Multi-role patterns deferred to future versions

v1 focuses on single-role operations — one caller, one role, one intent per invocation. Compositions that span multiple roles within a single atomic transaction (trader + LP self-routing, take-and-post-only remainder, market maker rebalancing across orders and pools, cross-outcome arbitrage, atomic market + pool creation) are recognized as valuable but not covered by v1's APIs.

Until those compositions get first-class support, callers whose actual intent spans multiple roles construct PSETs directly against the covenant spec; the single-role APIs provide the ingredients (params, state inspection, contract compilation, LMSR math) but not the atomic composition. Specific named patterns deferred in the [implementation plan](deadcat-core-implementation-plan.md#deferred--out-of-scope-items) include cross-outcome arb and atomic issuance + pool bootstrap; other multi-role patterns are unnamed future work and will be added to the plan as concrete user needs emerge.

## ContractEngine

`ContractEngine` is the central type in `deadcat-core`. It owns the store, manages contract state, processes transactions, and provides interpretation and asset identification.

```rust
pub struct ContractEngine<S: ContractStore> {
    store: S,
    network: Network,
}
```

### Exclusive Store Ownership

The engine takes exclusive ownership of the store. The caller creates a `ContractStore` implementation, hands it to `ContractEngine::new()` along with the target network (testnet/mainnet), and never touches the store again. All reads and writes go through the engine's API. The network is a construction-time constant used for Simplicity compilation and address derivation.

**Why**: If the caller could mutate the store directly, they could advance a contract's outpoints without the engine knowing, or modify state in ways that break the engine's internal invariants. Exclusive ownership ensures the engine is always the single source of truth for contract state.

### API Overview

`ContractEngine` owns three responsibility clusters: **write operations** (ingestion, chain sync), **discovery** (listing, lookup, asset identification, interpretation), and **operations that don't have a tracked contract yet** (creation builders, trade routing). Per-contract operations live on **view types** (`Market`, `Pool`, `Order`, `MultiOutcomeMarket`) returned by engine accessors. See [View Types](#view-types) for the per-contract API surface.

```rust
impl<S: ContractStore> ContractEngine<S> {
    // ---- Construction ----
    pub fn new(store: S, network: Network) -> Self;

    // ---- Ingestion (writes — &mut self) ----
    pub fn ingest_market(
        &mut self,
        params: &MarketParams,                     // Binary(..) or MultiOutcome(..)
        creation_tx: &ChainTransaction,
    ) -> Result<ContractId, CoreError<S::Error>>;

    pub fn ingest_pool(
        &mut self,
        params: &LmsrPoolParams,
        snapshot: PoolSnapshot,
    ) -> Result<ContractId, CoreError<S::Error>>;

    /// Ingest for ownership monitoring: full history, persistent storage,
    /// no auto-cleanup on terminal state. Requires the creation tx.
    /// Sets `OrderState.tracking = OrderTracking::Persistent`.
    pub fn ingest_persistent_order(
        &mut self,
        params: &MakerOrderParams,
        creation_tx: &ChainTransaction,
    ) -> Result<ContractId, CoreError<S::Error>>;

    /// Ingest for routing/discovery: no history, auto-untracks past finality
    /// when terminal. Accepts either a `Creation` snapshot (accurate
    /// `offered_amount`) or a `Current` snapshot (baseline-at-discovery
    /// `offered_amount`). Sets `OrderState.tracking` to `EphemeralFresh` or
    /// `EphemeralMidLife` respectively.
    pub fn ingest_ephemeral_order(
        &mut self,
        params: &MakerOrderParams,
        snapshot: OrderSnapshot,
    ) -> Result<ContractId, CoreError<S::Error>>;

    pub fn untrack_contract(&mut self, contract_id: &ContractId) -> Result<(), CoreError<S::Error>>;

    // ---- Chain sync (writes — &mut self) ----
    pub fn step<C: ChainSource>(&mut self, chain: &mut C) -> Result<StepReport, CoreError<S::Error>>;
    pub fn rollback_to_height(&mut self, height: u32) -> Result<(), CoreError<S::Error>>;
    pub fn prune_finalized(&mut self, current_height: u32, finality_depth: u32) -> Result<(), CoreError<S::Error>>;

    // ---- Discovery (reads — &self) ----
    pub fn contract(&self, contract_id: &ContractId) -> Result<Option<Contract>, CoreError<S::Error>>;
    pub fn list_markets(&self, filter: StateFilter, page: Pagination) -> Result<Page<MarketEntry>, CoreError<S::Error>>;
    pub fn list_pools(&self, filter: StateFilter, page: Pagination) -> Result<Page<PoolEntry>, CoreError<S::Error>>;
    pub fn list_orders(&self, filter: StateFilter, page: Pagination) -> Result<Page<OrderEntry>, CoreError<S::Error>>;
    pub fn identify_asset(&self, asset_id: &AssetId) -> Result<Option<AssetInfo>, CoreError<S::Error>>;
    pub fn interpret_transaction(&self, tx: &Transaction) -> Result<InterpretedTransaction, CoreError<S::Error>>;

    // ---- View accessors (reads — &self) ----
    // Views cache (contract_id, params, state) at construction. Returns None if not tracked.
    pub fn market(&self, id: &ContractId) -> Result<Option<Market<'_, S>>, CoreError<S::Error>>;
    pub fn pool(&self, id: &ContractId) -> Result<Option<Pool<'_, S>>, CoreError<S::Error>>;
    pub fn order(&self, id: &ContractId) -> Result<Option<Order<'_, S>>, CoreError<S::Error>>;

    // ---- Creation builders (reads — &self; contract doesn't exist yet) ----
    pub fn build_binary_market_creation_pset(
        &self,
        params: &BinaryMarketCreationParams,
        funding: &WalletFunding,
    ) -> Result<(UnblindedPset, BinaryMarketParams), CoreError<S::Error>>;

    pub fn build_multi_outcome_market_creation_pset(
        &self,
        params: &MultiOutcomeMarketCreationParams,
        funding: &WalletFunding,
    ) -> Result<(UnblindedPset, MultiOutcomeMarketParams), CoreError<S::Error>>;

    pub fn build_lmsr_bootstrap_pset(
        &self,
        params: &LmsrPoolParams,
        initial_s_index: u16,
        initial_reserves: PoolReserves,
        masked_index: u16,
        funding: &WalletFunding,
    ) -> Result<PartiallySignedTransaction, CoreError<S::Error>>;

    pub fn build_create_order_pset(
        &self,
        params: &MakerOrderParams,
        offered_amount: u64,
        masked_index: u16,
        funding: &WalletFunding,
    ) -> Result<PartiallySignedTransaction, CoreError<S::Error>>;

    // ---- Trade routing (reads — &self) ----
    // Routes across all pools and maker orders for a given (market, outcome, side).
    // For multi-outcome markets, targets a single outcome's binary LMSR pool (and matching LOB orders).
    // Basket trades (cross-outcome splits/merges) are handled via MultiOutcomeMarket view;
    // cross-outcome arb (market + N pools atomic) is deferred to v2.
    pub fn quote_trade(
        &self,
        market_id: &ContractId,
        spec: TradeSpec,
        fee_rate: FeeRate,
    ) -> Result<TradeQuote, CoreError<S::Error>>;

    pub fn build_trade_pset(
        &self,
        quote: &TradeQuote,
        funding: &WalletFunding,
    ) -> Result<PartiallySignedTransaction, CoreError<S::Error>>;
}

// ---- Standalone pure functions (no engine needed) ----

/// CMR from contract params + network. Requires Simplicity compilation.
pub fn contract_cmr(params: &ContractParams, network: Network) -> Cmr;

/// Compute the market_id (32-byte tagged-hash input) from market params.
pub fn compute_market_id(params: &MarketParams) -> MarketId;

/// Compute the BIP-340 tagged-hash message the oracle needs to sign.
/// Usable by oracle services without a ContractEngine.
pub fn oracle_attestation_message(market_id: MarketId, resolution: MarketResolution) -> [u8; 32];

pub fn estimate_bootstrap(
    max_loss_sats: u64,
    half_payout_sats: u64,
    starting_price_bps: u16,
) -> Result<BootstrapEstimate, BootstrapError>;

pub fn derive_pool_params(
    deadcat_xprv: &Xpriv,
    market_params: &MarketParams,        // accepts binary or multi-outcome
    outcome: OutcomeIndex,                // which outcome's YES/NO pair the pool serves
    pool_index: u16,
    max_loss_sats: u64,
    half_payout_sats: u64,
    fee_bps: u16,
    initial_s_index: u16,                 // from estimate_bootstrap (creation) or hint (recovery)
) -> Result<(LmsrPoolParams, u16 /* masked_index */), ConventionError>;

pub fn derive_order_params(
    deadcat_xprv: &Xpriv,
    market_params: &MarketParams,        // accepts binary or multi-outcome
    outcome: OutcomeIndex,                // which outcome's YES/NO pair the order offers
    order_index: u16,
    side: Side,
    direction: OrderDirection,
    price: u64,
    min_fill_lots: u8,
    min_remainder_lots: u8,
) -> Result<(MakerOrderParams, u16 /* masked_index */), ConventionError>;
```

Write methods take `&mut self`. Read methods take `&self`. Rust's borrow rules enforce at compile time that only one writer OR multiple readers can access the engine at any given time — analogous to `RwLock` semantics without runtime overhead. While any view type (`Market`, `Pool`, `Order`) is alive, the engine holds an immutable borrow; mutations are blocked until the view is dropped.

**Per-contract operations live on views, not on the engine.** See [View Types](#view-types) for detailed per-view documentation:
- `Market<'a, S>` exposes issuance, cancellation, resolution, redemption, expiry builders plus oracle helpers, related-contract queries, and multi-outcome specialization.
- `MultiOutcomeMarket<'a, S>` exposes cross-outcome primitives (split-YES, merge-YES, split-NO, merge-NO). Obtained via `Market::as_multi_outcome()`; returns `None` for binary markets. Cross-outcome arb (market + N pools atomic) is deferred to v2.
- `Pool<'a, S>` exposes adjust and close builders plus parent-market navigation.
- `Order<'a, S>` exposes cancel builder plus parent-market navigation.

**Creation builders stay on the engine** because the contract doesn't exist yet — there's no view to operate on. They take concrete param types and, for markets, return the derived full-params alongside the PSET (the 4 / 4N token and RT asset IDs are derived from selected defining inputs). `build_lmsr_bootstrap_pset` takes `LmsrPoolParams` fully formed plus an explicit starting state (`initial_s_index`) and explicit starting reserves (`initial_reserves`). `estimate_bootstrap` is just the canonical default-policy helper for choosing those reserves; it is not the only valid bootstrap shape. `build_create_order_pset` takes `MakerOrderParams` similarly.

**Trade routing stays on the engine** because quoting inspects *multiple* contracts at once (the pool(s) and resting maker orders for a given market outcome). Putting `quote_trade` on the `Market` view would require the view to see other tracked contracts too, defeating the encapsulation — simpler to keep routing at the engine level where access to all tracked contracts is natural. `build_trade_pset` stays on the engine for the same reason.

**History methods — only available when the store implements `ContractHistory`. Exposed on view types**:

```rust
impl<'a, S: ContractHistory> Market<'a, S> {
    pub fn history(&self, after: Option<ChainPosition>, limit: u32)
        -> Result<Vec<MarketHistoryEntry>, CoreError<S::Error>>;
}

impl<'a, S: ContractHistory> Pool<'a, S> {
    pub fn history(&self, after: Option<ChainPosition>, limit: u32)
        -> Result<Vec<PoolHistoryEntry>, CoreError<S::Error>>;
}

impl<'a, S: ContractHistory> Order<'a, S> {
    pub fn history(&self, after: Option<ChainPosition>, limit: u32)
        -> Result<Vec<OrderHistoryEntry>, CoreError<S::Error>>;
}
```

**Maker order lifecycle vs taker trades**: the maker side of limit orders is directly exposed (`engine.build_create_order_pset`, `order.build_cancel_pset`). The taker side — filling orders — is handled through the trade system (`engine.quote_trade` + `engine.build_trade_pset`), which routes across pools and orders for best execution. There is intentionally no `build_fill_order_pset` — direct order targeting adds API complexity without improving execution, since the router always finds the best available fill. If explicit order targeting becomes a requested feature, a direct fill builder can be added later as a non-breaking change.

**Merged builders**: `Market::build_issuance_pset` handles both initial (Dormant → Unresolved) and subsequent (Unresolved → Unresolved) issuance — the view determines which from the cached state. `Market::build_redemption_pset` handles both post-resolution and post-expiry redemption. For binary markets, the `side` parameter specifies which token to burn; for resolved markets, the engine validates it matches the winning side; for expired markets, either side is valid. Multi-outcome redemption also takes `outcome: OutcomeIndex` to specify which outcome's token pair.

**No per-builder args structs**: PSET builders take operation-specific arguments as direct parameters alongside a shared `WalletFunding` struct (available UTXOs, fee rate, return script). This avoids a zoo of single-use parameter types — the function signature IS the documentation. See [WalletFunding](#walletfunding) and [PSET Construction](#pset-construction).

Note: The history `impl` block uses `S: ContractHistory` rather than `S: ContractStore + ContractHistory` because `ContractHistory` is a supertrait of `ContractStore` — the `ContractStore` bound is implied. See [ContractHistory](#optional-contracthistory).

### ContractId

Contract IDs uniquely identify a specific on-chain instance of a contract. They combine the program identity (CMR) with the instance identity (creation txid):

```rust
pub struct ContractId {
    pub cmr: Cmr,
    pub creation_txid: Txid,
}
```

`Cmr` is `simplicity_lang::Cmr`, re-exported from `deadcat-core`. It implements `AsRef<[u8]>` and `from_byte_array([u8; 32])`.

**Public dependencies**: `deadcat-core` has two public dependencies:
- **`simplicity_lang`** — provides `Cmr`
- **`elements`** — provides `Transaction`, `OutPoint`, `Txid`, `Script`, `AssetId` (all top-level), `elements::pset::PartiallySignedTransaction`, `elements::secp256k1_zkp::schnorr::Signature` (for oracle attestations), and `elements::bitcoin::bip32::Xpriv` (for the key derivation convenience functions). All Elements types used in the public API come from this crate.

**Why both fields**: The CMR identifies the program (same params + same covenant source = same CMR), but not the instance. Two on-chain instances with identical params produce the same CMR. While collisions are self-defeating in practice (pools: admin key=operator makes collision self-inflicted; orders: fresh nonces prevent it), the `creation_txid` component closes all theoretical collision vectors at minimal cost (32 extra bytes, already available from discovery). The struct preserves both fields: `cmr` for discovery dedup (O(1) "do I track anything with this CMR?"), `creation_txid` for instance uniqueness. See [Design Decisions Log](#design-decisions-log) for the full rationale.

The standalone function `contract_cmr(params, network)` returns only the CMR component — used for discovery dedup without requiring an engine. The full `ContractId` is only available after ingestion (when `creation_txid` is known from the creation transaction or snapshot).

### Contract Ingestion

Core uses per-type ingestion methods instead of a unified `ingest_contract`. Each contract type has genuinely different ingestion needs:

#### ingest_market

Markets are always ingested from their creation transaction:

```rust
pub fn ingest_market(
    &mut self,
    params: &MarketParams,                  // Binary(..) or MultiOutcome(..)
    creation_tx: &ChainTransaction,
) -> Result<ContractId, CoreError<S::Error>>;
```

The engine compiles the Simplicity contract from the parameters (for multi-outcome, selects the N-specific generated `.simf`), derives deterministic blinding factors for creation verification (see [Deterministic RT Blinding](../protocol/deterministic-rt-blinding.md)), verifies the creation transaction contains the expected covenant scripts and all expected token issuances (2 for binary, 2N for multi-outcome), derives the initial outpoints, indexes asset IDs and scripts, and begins tracking. Returns the `ContractId` (CMR + creation txid).

**No anchor required**: Prediction market creation transactions include blinded reissuance token outputs. The blinding factors for these outputs are derived deterministically from public on-chain data (the defining outpoints), so no out-of-band anchor data is needed.

#### ingest_pool

Pools support both creation-tx and non-initial ingestion:

```rust
pub fn ingest_pool(
    &mut self,
    params: &LmsrPoolParams,
    snapshot: PoolSnapshot,
) -> Result<ContractId, CoreError<S::Error>>;

pub struct ReserveOutpoints {
    pub yes: OutPoint,
    pub no: OutPoint,
    pub collateral: OutPoint,
}

pub enum PoolSnapshot {
    Creation(ChainTransaction),
    Current {
        creation_txid: Txid,
        outpoints: ReserveOutpoints,
        s_index: u64,
        reserves: PoolReserves,
        position: ChainPosition,
    },
}
```

With `PoolSnapshot::Creation`, the engine processes the creation transaction to derive initial state — same as market ingestion. The engine also verifies the pool's curve is well-formed: it derives `b` from `params.max_loss_sats`, recomputes `q_step_lots`, regenerates the full F-value table, and checks the Merkle root matches `params.lmsr_table_root`. A mismatch indicates the pool was created with a non-canonical table generation algorithm — the engine returns `CoreError::ConventionViolation { detail }`. This verification is a cold-cache table-generation step: the first use of a given `(max_loss_sats, half_payout_sats)` combo incurs the full bignum table cost (~5-10s), while later ingestions reuse the in-memory cache. With `PoolSnapshot::Current`, the engine starts tracking from the provided state without verifying history back to creation. The trade-off: `Current` = fast start (no history replay needed), but no prior transition history is recoverable. `Creation` = full history available via forward-sync from creation. Note: `Current` also sidesteps s_index derivation entirely (the caller provides `s_index` directly), making it useful for fast-start ingestion when the creation transaction is unavailable or the caller intentionally skips historical proof; the engine still enforces canonical supplied params and canonical-parent-market membership.

#### ingest_persistent_order and ingest_ephemeral_order

Orders have two ingestion methods corresponding to the two tracking modes. The method signals the caller's intent at call sites (is this an order I own and want to audit, or one I'm tracking for routing?):

```rust
/// Maker monitoring their own order: full history, persistent storage.
/// Requires the creation tx — accurate starting state is a prerequisite
/// for meaningful history. Sets tracking = Persistent.
pub fn ingest_persistent_order(
    &mut self,
    params: &MakerOrderParams,
    creation_tx: &ChainTransaction,
) -> Result<ContractId, CoreError<S::Error>>;

/// Taker / discoverer: no history, auto-untracks past finality when terminal.
/// Accepts either a Creation snapshot (accurate offered_amount, tracking
/// set to EphemeralFresh) or a Current snapshot (baseline-at-discovery
/// offered_amount, tracking set to EphemeralMidLife).
pub fn ingest_ephemeral_order(
    &mut self,
    params: &MakerOrderParams,
    snapshot: OrderSnapshot,
) -> Result<ContractId, CoreError<S::Error>>;

pub enum OrderSnapshot {
    Creation(ChainTransaction),
    Current {
        creation_txid: Txid,
        outpoint: OutPoint,
        locked_value: u64,
        position: ChainPosition,
    },
}
```

**Who calls which**:
- Makers ingest their own orders via `ingest_persistent_order`.
- Takers discovering orders for routing use `ingest_ephemeral_order` — typically with a `Current` snapshot (they don't have the creation tx), but `Creation` is accepted too when the discovery channel carries the creation tx (enabling honest fill-progress displays without the history storage cost).

**Tracking mode is immutable after ingestion**: to change tracking mode, call `untrack_contract` first, then re-ingest via the other method. This is slow for the `Ephemeral → Persistent` direction because the re-ingestion via `ingest_persistent_order` forward-syncs all fills from creation to rebuild history. Acceptable for the rare promotion scenario; not optimized further.

**Duplicate ingestion uniformly errors**: calling either method on an already-tracked `ContractId` returns `CoreError::ContractAlreadyTracked { contract_id }`, regardless of which method was used first. `ContractId` is derived from `CMR + creation_txid`, so the same underlying order produces the same `ContractId` through either method.

#### Common ingestion behavior

**Caller responsibility**: For `Creation` snapshots, the caller must ensure the creation transaction has already been confirmed on-chain. Core does not verify chain inclusion — it verifies that the transaction's outputs match the expected covenant scripts derived from the provided parameters.

**Not idempotent**: All ingestion methods are NOT idempotent. Re-ingesting an already-tracked contract returns `CoreError::ContractAlreadyTracked { contract_id }`. This is keyed on the full `ContractId` (CMR + creation_txid), so two instances with the same CMR but different creation_txids are distinct contracts. Callers who want idempotent behavior (e.g., crash recovery, multi-source discovery) can extract the contract ID from the error:

```rust
let contract_id = match engine.ingest_market(&params, &creation_tx) {
    Ok(id) => id,
    Err(CoreError::ContractAlreadyTracked { contract_id }) => contract_id,
    Err(e) => return Err(e),
};
// `params` is `MarketParams` (Binary(..) or MultiOutcome(..)); passed by reference consistent with
// ingest_pool / ingest_persistent_order / ingest_ephemeral_order.
```

**Parent market required**: `ingest_pool`, `ingest_persistent_order`, and `ingest_ephemeral_order` validate that the referenced token asset IDs correspond to a known market. If the parent market isn't tracked, the engine returns `CoreError::ParentMarketNotTracked { detail }`. `ingest_market` has no parent requirement.

### untrack_contract

Removes a contract from the engine, deleting the contract, all derived data (asset index, scripts), and any history:

```rust
pub fn untrack_contract(&mut self, contract_id: &ContractId) -> Result<(), CoreError<S::Error>>;
```

Primary uses: cleanup of terminal/unwanted contracts, and the "untrack + re-ingest" promotion pattern — upgrading from non-initial to creation-based ingestion. A contract ingested with `PoolSnapshot::Current` can be untracked and re-ingested with `PoolSnapshot::Creation` to gain full history.

### step

`step` is the primary sync method. It catches up all tracked contracts to the chain tip and processes any pending notifications, using a caller-provided `ChainSource` implementation for chain data access. See [Chain Sync](#chain-sync) for the full sync model, `ChainSource` trait definition, and internal sync strategies.

```rust
pub fn step<C: ChainSource>(
    &mut self,
    chain: &mut C,
) -> Result<StepReport, CoreError<S::Error>>;
```

The caller's sync loop is:

```rust
loop {
    let report = engine.step(&mut chain)?;
    for tx in &report.transactions {
        for t in &tx.interpretation.transitions {
            update_ui(&t.details);
        }
    }
    sleep(Duration::from_secs(60)); // or wait for block notification
}
```

`step` internally uses `process_transaction` (`pub(crate)`) to advance contract state. `process_transaction` is idempotent, durable-before-returning, and handles all contract types uniformly. It is not exposed publicly because the engine manages subscription state internally — direct external calls would cause subscription state to become stale. See [Chain Sync](#chain-sync) for details.

**Resumable**: If the chain source fails mid-sync, the engine has already persisted whatever it processed. The caller can retry `step` immediately — already-processed transactions are idempotent no-ops.

### interpret_transaction

`interpret_transaction` is the primary read method for wallet integration. It uses the same script matching and output value logic as `process_transaction` but does not modify state:

```rust
pub fn interpret_transaction(&self, tx: &Transaction) -> Result<InterpretedTransaction, CoreError<S::Error>>;
```

**Works for confirmed and unconfirmed transactions**: `interpret_transaction` accepts any transaction — confirmed or unconfirmed. It works as long as the transaction spends outpoints the engine currently tracks in its durable state. This enables "pending transaction" UX: a wallet can interpret an unconfirmed mempool transaction to display "Pending issuance" or "Pending trade" before the transaction confirms. Confirmed state updates happen through `step`, which uses the internal `process_transaction` — `interpret_transaction` is for read-only inspection, not durable state changes.

**Known limitation — chained unconfirmed transactions**: If two unconfirmed transactions form a chain (tx2 spends an output created by tx1), only tx1 is interpretable. Tx2 spends outpoints that the engine hasn't durably recorded (tx1 was never processed), so the engine doesn't recognize them. Once both confirm and `step` processes them, both are handled normally. This is rare in practice — Liquid has ~1-minute blocks, and chained unconfirmed covenant transactions require dependent operations within that window.

**No chain position metadata**: `interpret_transaction` takes a raw `elements::Transaction` without block height or tx index. The returned `InterpretedTransaction` omits chain position fields — contrast with `ProcessedTransaction` (returned by `step`) which includes `ChainPosition`. See [Transaction-Level Types](#transaction-level-types).

**Point-in-time query**: The results reflect what the engine currently knows. If a transaction spends UTXOs from a contract the engine hasn't ingested yet, those contracts are simply absent from the results. After ingesting the contract and catching it up, calling `interpret_transaction` on the same transaction returns additional results.

**Partial knowledge grows over time**: A trade transaction that spends a known limit order and an unknown pool would initially return only the order fill transition. After the pool is ingested, the same call would also include the pool swap transition. The caller should be prepared to re-interpret transactions as new contracts are ingested.

### contract

Returns the current state of a single tracked contract by ID:

```rust
pub fn contract(&self, contract_id: &ContractId) -> Result<Option<Contract>, CoreError<S::Error>>;
```

Returns `None` if the contract hasn't been ingested. The caller matches on the `Contract` enum to access the typed state. Since callers typically know the contract type (they ingested it), this is a single-variant match.

### Per-Type Listing Methods

The typed listing methods (`list_markets`, `list_pools`, `list_orders`) delegate to the store's per-type methods, which return typed results directly. All accept `StateFilter` and `Pagination` parameters.

```rust
pub fn list_markets(
    &self,
    filter: StateFilter,
    page: Pagination,
) -> Result<Page<MarketEntry>, CoreError<S::Error>>;
```

The store's listing methods return typed results (e.g., `Page<MarketEntry>` rather than `Page<(ContractId, Contract)>`). `MarketEntry` is a type alias for `ContractEntry<MarketParams, MarketState>` — see [ContractEntry](#contractentry). Using the umbrella types means `list_markets` returns both binary and multi-outcome markets in a single call. This enforces the type invariant at compile time — a `list_markets` implementation cannot accidentally return a pool or order. If the store's own data is corrupted (a "market" row deserializes to a different type), the error surfaces at the store layer via `Self::Error`, where data corruption errors belong.

**Pagination**: All listing methods use cursor-based pagination. See [Pagination Types](#pagination-types).

**State filtering**: `StateFilter::ActiveOnly` returns contracts with active outpoints. `StateFilter::TerminalOnly` returns contracts in terminal states (settled markets, closed pools, consumed/cancelled orders). `StateFilter::All` returns both. At Polymarket scale (thousands of markets), filtering at the store level avoids paging through thousands of irrelevant terminal contracts.

### Relationship Queries

Relationship queries live on view types, not on the engine. See [View Types § Market](#market): `market.pools(filter, page)` and `market.orders(filter, page)` return pools and orders associated with the market. These accept `StateFilter` to avoid paging through terminal contracts at scale — a popular market could accumulate thousands of consumed/cancelled orders.

The relationship is encoded in pool/order params (they reference the market's token asset IDs — binary markets: `yes_token_asset_id` / `no_token_asset_id`; multi-outcome markets: `yes_token_asset_ids[k]` / `no_token_asset_ids[k]` for a specific outcome k). The store maintains a secondary index on market_id for efficient lookups, built during ingestion by resolving the pool/order's token asset IDs via the store's own `find_by_asset_id` index.

**Ingestion ordering constraint**: Pools and orders require their parent market to be ingested first. During `ingest_pool` or either order-ingestion method, the engine validates that the referenced token asset IDs correspond to a known market. If the parent market isn't tracked, the engine returns `CoreError::ParentMarketNotTracked { detail }`. This is a natural constraint — you shouldn't track a pool for a market you don't know about, and discovery naturally produces markets before their pools/orders.

Pools and orders are split into separate accessors (`market.pools()` vs `market.orders()`) because they scale differently — a market typically has a handful of pools but potentially thousands of orders at Polymarket scale. Both are paginated.

**Reverse navigation**: `Pool::parent_market()` and `Order::parent_market()` return the parent market's `Market` view (if tracked). Implemented via the `asset_id → (contract_id, token_role)` index that the engine maintains for asset identification — looking up the pool's `yes_asset_id` yields the parent market's contract_id.

### History Methods

The three typed history methods (`Market::history`, `Pool::history`, `Order::history`) are exposed on the view types and are only available when the store implements `ContractHistory`. They delegate to the store's unified `transition_history` method internally, then unwrap the `TransitionDetails` enum to return typed results:

```rust
// Conceptually (each view type's impl block bound by ContractHistory):
impl<'a, S: ContractHistory> Market<'a, S> {
    pub fn history(&self, after: Option<ChainPosition>, limit: u32)
        -> Result<Vec<MarketHistoryEntry>, CoreError<S::Error>>
    {
        let raw: Vec<HistoryEntry> = self.engine.store().transition_history(&self.contract_id, after, limit)?;
        raw.into_iter().map(|u| {
            let TransitionDetails::Market(details) = u.details else {
                debug_assert!(false, "store returned non-market transition for market contract");
                // filter out mismatched entries
            };
            MarketHistoryEntry { contract_id: u.contract_id, txid: u.txid, /* ... */ details }
        }).collect()
    }
}
```

`Pool::history` and `Order::history` follow the same pattern but unwrap to `PoolHistoryEntry` / `OrderHistoryEntry` respectively.

**Ordering**: History is returned in ascending chain order (oldest first). This aligns with the primary use cases: price chart construction, audit trails, and catch-up from a checkpoint. The `after` parameter provides a precise cursor using `ChainPosition` (block height + tx index), which handles multiple transitions within the same block correctly. The caller paginates by passing the `position` from the last returned item as `after` in the next call.

**Why typed convenience methods on views**: the caller always knows the contract type when querying history (they're already holding a typed view). The unified `StateUpdate` with `TransitionDetails` enum would force an unnecessary match on a variant the caller already knows. The typed view-level `history` methods eliminate this ergonomic cost. The store trait stays simple (one `transition_history` method); the view does the trivial unwrapping.

**Invariant**: All transitions for a given `contract_id` always have the same `TransitionDetails` variant (a market contract only produces `Market` transitions). A mismatch indicates a bug in the store implementation — the view asserts in debug and filters in release.

## Core Types

### ContractId

```rust
pub struct ContractId {
    pub cmr: Cmr,
    pub creation_txid: Txid,
}
```

See [ContractId](#contractid) in the ContractEngine section for full description and rationale.

### ChainPosition

Chain position metadata for a confirmed transaction:

```rust
pub struct ChainPosition {
    pub block_height: u32,
    pub tx_index: u32,  // position within block, for ordering
}
```

Used by `ChainTransaction`, `ProcessedTransaction`, `StateUpdate`, and `TypedStateUpdate`. Groups the two fields that are always known or unknown together — a confirmed transaction always has both; an unconfirmed transaction has neither.

### ChainTransaction

The input to `process_transaction` and the per-type ingestion methods (via `Creation` snapshot variants). Represents a **confirmed** on-chain transaction. Core does not validate consensus — it interprets. This is a safe assumption because the caller gets transactions from their chain backend, which only provides consensus-valid data.

```rust
pub struct ChainTransaction {
    pub tx: elements::Transaction,
    pub position: ChainPosition,
}
```

**Confirmed only**: `process_transaction` and the ingestion methods require confirmed transactions with valid block height and tx index. For unconfirmed/mempool transactions, use `interpret_transaction` which takes a raw `elements::Transaction` without chain position metadata.

### ContractParams

The parameters that define a contract, from which core derives script pubkeys, compiles Simplicity contracts, and generates the CMR.

```rust
pub enum ContractParams {
    Market(MarketParams),
    LmsrPool(LmsrPoolParams),
    MakerOrder(MakerOrderParams),
}

/// Umbrella over the two market contract types. Binary and multi-outcome markets
/// share many properties (oracle, collateral asset, expiry) but have distinct
/// covenant layouts (8 vs 5N+2 slots) and distinct token models (2 vs 2N tokens).
pub enum MarketParams {
    Binary(BinaryMarketParams),
    MultiOutcome(MultiOutcomeMarketParams),
}
```

`ContractParams` is purely definitional — it contains only the data needed to derive the contract's identity (CMR) and addresses (covenant script pubkeys). No creation-time secrets or blinding factors. Given `ContractParams` + network + the Simplicity source code (built into `deadcat-core`), all covenant addresses for all states can be derived deterministically.

`MarketParams` is the umbrella consumers typically interact with via the `Market` view type (see [Market View Type](#market-view-type)). Common accessors (`outcome_count`, `oracle_public_key`, `collateral_asset_id`, `expiry_time`) are exposed on `Market` without requiring consumers to destructure the enum. Consumers can still match on the enum directly when they need type-specific fields.

**Naming**: `BinaryMarketParams` was previously called `PredictionMarketParams`. The rename aligns with the paired `BinaryMarketState` / `MultiOutcomeMarketState`, `BinaryMarketTransition` / `MultiOutcomeMarketTransition`, and `BinaryMarketCreationParams` / `MultiOutcomeMarketCreationParams` conventions introduced when the multi-outcome contract was added.

### Market Creation Params

Creation params differ meaningfully between binary and multi-outcome markets (multi-outcome adds `outcome_count`), and the creation PSET builders are correspondingly split. Each creation builder takes only the non-derivable fields; the remaining fields (token and reissuance token asset IDs) are derived from the creation transaction's issuance entropy, which depends on the UTXOs selected as defining inputs during coin selection.

```rust
pub struct BinaryMarketCreationParams {
    pub oracle_public_key: XOnlyPublicKey,
    pub collateral_asset_id: AssetId,
    pub base_payout: u64,           // primary denomination; cp = base_payout × 2 (pair cost) is derived
    pub expiry_time: u32,
}

pub struct MultiOutcomeMarketCreationParams {
    pub oracle_public_key: XOnlyPublicKey,
    pub collateral_asset_id: AssetId,
    pub base_payout: u64,           // primary denomination; cp = base_payout × outcome_count is derived
    pub expiry_time: u32,
    pub outcome_count: u8,          // N, validated in range per supported multi-outcome .simf files
}
```

The primary denomination field is `base_payout` — the per-outcome YES-expiry payout unit, drawn from the 1-2-5 table. Binary markets derive `cp = base_payout × 2`. Multi-outcome markets derive `cp = base_payout × outcome_count`. Formulas throughout this document use `collateral_per_pair` (or `cp`) as a derivation shorthand; implementations may expose it as an accessor method on the params struct. The unified-denomination rationale (divisibility becomes structural; no covenant `mod N` check needed; every denomination-table index is usable for every supported N) is specified in [multi-outcome-market-contract.md § Denomination model](../contracts/multi-outcome/multi-outcome-market-contract.md#denomination-model).

The binary creation builder selects 2 defining inputs, derives the 2 token and 2 reissuance-token asset IDs, compiles the covenant, builds the PSET, and returns the full `BinaryMarketParams`. The multi-outcome creation builder selects 2N defining inputs in canonical leg order `YES_0, NO_0, YES_1, NO_1, ..., YES_{N-1}, NO_{N-1}`, derives the 2N token and 2N reissuance-token asset IDs, compiles the N-specific `.simf` covenant, and returns the full `MultiOutcomeMarketParams`. In both cases the caller uses the returned params for subsequent `ingest_market` after the transaction confirms.

`BinaryMarketParams` and `MultiOutcomeMarketParams` define each market's covenant parameters (oracle key, expiry, asset IDs, etc.). `LmsrPoolParams` defines the pool's parameters (token asset IDs referencing the parent market, liquidity parameters, and `max_loss_sats` for off-chain LMSR math). `MakerOrderParams` defines the order's parameters (base/quote asset IDs, price, direction). These types map 1:1 to Simplicity covenant parameters, with one exception: `LmsrPoolParams.max_loss_sats` is not a covenant parameter but is included because all off-chain LMSR computation (cached-table quoting, table generation, spot price) requires the liquidity parameter `b = max_loss_sats / ln(2)`, and `b` is not recoverable from the covenant params alone (the `max_loss_sats → q_step_lots` derivation uses `ceil()`, which is lossy). The derivable fields `q_step_lots` and `lmsr_table_root` are retained alongside `max_loss_sats` as compilation caches — re-deriving `lmsr_table_root` is the cold-cache table-generation path. See [contract-specification.md](../contracts/contract-specification.md) for the planned field definitions per contract type, covenant structure, spend paths, and witness data.

**Pool layer composition**: a single `LmsrPoolParams` always refers to one outcome's YES/NO pair. For binary markets, the pool's YES/NO tokens come directly from the binary market's `yes_token_asset_id` / `no_token_asset_id`. For multi-outcome markets, the pool's YES/NO tokens are `yes_token_asset_ids[k]` / `no_token_asset_ids[k]` for a specific outcome k — multi-outcome markets compose their AMM liquidity from N independent binary LMSR pools (Option C composition). The pool contract doesn't know or care which market contract type underlies its tokens. See [amm-scoring-rule-tradeoffs.md](../contracts/multi-outcome/amm-scoring-rule-tradeoffs.md) for the pool design decision.

### Contract

The three contract kinds core tracks internally. This is an **internal type** managed by the engine and store — callers do not construct `Contract` values directly. Instead, they provide params + creation transaction (or snapshot) to the per-kind ingestion methods, and the engine derives the initial contract state.

```rust
pub enum Contract {
    Market {
        params: MarketParams,     // Binary(BinaryMarketParams) or MultiOutcome(MultiOutcomeMarketParams)
        state: MarketState,       // Binary(BinaryMarketState) or MultiOutcome(MultiOutcomeMarketState)
    },
    LmsrPool {
        params: LmsrPoolParams,
        state: LmsrPoolState,
    },
    MakerOrder {
        params: MakerOrderParams,
        state: OrderState,
    },
}
```

Each variant's mutable state (reserves, fill amounts) lives inside the state enum, not alongside it. This prevents stale field values when a contract reaches a terminal state. See [Contract State Enums](#contract-state-enums) below.

The `Market` variant is unified across binary and multi-outcome — both are markets from the engine's perspective, differing only in their inner params/state enum variants. Consumers interact with markets through the [Market view type](#market-view-type) (Stage 2), which exposes common accessors (`outcome_count`, `oracle_public_key`, etc.) without forcing callers to destructure the umbrella enum for every query. `Market::as_multi_outcome()` returns `Option<MultiOutcomeMarket>` for type-level access to multi-outcome-only operations.

### Contract State Enums

Each contract kind has a state enum representing its current tip state — the latest snapshot, not a history log. This is stored durably via `ContractStore` (required) and updated each time `process_transaction` advances the contract. The tip state carries enough information for basic wallet UX without requiring `ContractHistory`.

#### MarketState

Umbrella over binary and multi-outcome market states. Kept as separate inner enums because binary and multi-outcome resolution semantics genuinely differ — binary resolves via `Side` (YES or NO won the single event), multi-outcome resolves via `OutcomeIndex` (which of N mutually exclusive outcomes won).

```rust
pub enum MarketState {
    Binary(BinaryMarketState),
    MultiOutcome(MultiOutcomeMarketState),
}

pub enum BinaryMarketState {
    Trading { outstanding_pairs: u64 },
    ResolvedYes { outstanding_pairs: u64 },
    ResolvedNo { outstanding_pairs: u64 },
    Expired { outstanding_pairs: u64 },
}

pub enum MultiOutcomeMarketState {
    Trading {
        supplies: Vec<PairSupply>,      // length == outcome_count
    },
    Resolved {
        winning_outcome: OutcomeIndex,
        collateral_unredeemed: u64,     // when 0, terminal
    },
    Expired {
        collateral_unredeemed: u64,     // when 0, terminal
    },
}

pub struct PairSupply {
    pub yes: u64,
    pub no: u64,
}
```

`BinaryMarketState` is the state enum previously known simply as `MarketState`. It has been renamed to fit alongside `MultiOutcomeMarketState` under the umbrella. The enum's variants and field semantics are unchanged.

For both inner enums:
- **Trading** covers both Dormant (zero outstanding) and Unresolved (non-zero outstanding) covenant phases. The distinction is a covenant implementation detail.
- `outstanding_pairs` (binary) or the `supplies` vector (multi-outcome) is derived from on-chain state — from the collateral UTXO amount and the market params.
- **Terminal state** is any post-resolution/expiry variant with zero remaining unredeemed value. For binary: `ResolvedYes`/`ResolvedNo`/`Expired` with `outstanding_pairs == 0`. For multi-outcome: `Resolved`/`Expired` with `collateral_unredeemed == 0`. `Trading` with zero outstanding is NOT terminal (can still receive issuance, resolution, or expiry transitions).
- Resolution and expiry always produce the corresponding `Resolved*`/`Expired` variant, regardless of whether the market had outstanding supply. If there was nothing outstanding (dormant terminal path), the resulting state is immediately terminal.

**Multi-outcome specifics**:
- `Trading.supplies` is a `Vec<PairSupply>` of length `outcome_count`, indexed by outcome (`supplies[k]` is outcome k's YES/NO supply). `Vec` rather than `[PairSupply; N]` because `N` is a runtime value (per-pool), not a type parameter.
- `Resolved.collateral_unredeemed` tracks unredeemed collateral across all winning tokens (YES_k for winning outcome k plus NO_j for all j ≠ k). Detailed per-outcome redemption history is available via `ContractHistory` for consumers who need it.
- `Expired.collateral_unredeemed` tracks unredeemed collateral across all tokens redeemable at the expiry rate (all YES_k and NO_k).

Outpoints are not exposed in the public state — they are internal to the engine. Consumers use the [Market view type](#market-view-type) to access state; common accessors (`is_active()`, `is_resolved()`, `resolved_outcome()`, `outcome_count()`) work uniformly across market types without requiring consumers to destructure the umbrella enum.

See [collateral-per-pair-refactor.md](../contracts/prediction-market/collateral-per-pair-refactor.md) for the binary covenant parameter rename.

#### SlotIdentity and CovenantPhase

`SlotIdentity` is the public type that labels each tracked outpoint with its slot role — used at the engine↔store boundary for labeled outpoints (see [ContractMatch](#contractmatch), [InitialContractState](#initialcontractstate), [StateUpdate](#stateupdate), and [`ContractStore::contract_outpoints`](#required-contractstore)). `CovenantPhase` is internal (`pub(crate)`) and used for script matching and PSET routing.

```rust
pub enum SlotIdentity {
    BinaryMarket(BinaryMarketSlot),
    MultiOutcomeMarket(MultiOutcomeMarketSlot),
    Pool(PoolSlot),
    Order(OrderSlot),
}

pub enum BinaryMarketSlot {
    DormantYesRt,          // Dormant phase
    DormantNoRt,           // Dormant phase
    UnresolvedYesRt,       // Unresolved phase
    UnresolvedNoRt,        // Unresolved phase
    UnresolvedCollateral,  // Unresolved phase
    ResolvedYesCollateral, // ResolvedYes phase
    ResolvedNoCollateral,  // ResolvedNo phase
    ExpiredCollateral,     // Expired phase
}

pub enum MultiOutcomeMarketSlot {
    DormantYesRt(OutcomeIndex),
    DormantNoRt(OutcomeIndex),
    UnresolvedYesRt(OutcomeIndex),
    UnresolvedNoRt(OutcomeIndex),
    UnresolvedCollateral,
    ResolvedCollateral,     // single slot in the Resolved(k) phase
    ExpiredCollateral,
}

pub enum PoolSlot {
    YesReserve,
    NoReserve,
    CollateralReserve,
}

pub enum OrderSlot {
    Utxo,
}

// pub(crate) — internal to the engine. Market-only; pool and order phase/lifecycle
// tracking lives in their respective state enums (LmsrPoolState, OrderState).
pub(crate) enum CovenantPhase {
    Binary(BinaryCovenantPhase),
    MultiOutcome(MultiOutcomeCovenantPhase),
}

pub(crate) enum BinaryCovenantPhase {
    Dormant,
    Unresolved,
    ResolvedYes,
    ResolvedNo,
    Expired,
}

pub(crate) enum MultiOutcomeCovenantPhase {
    Dormant,
    Unresolved,
    Resolved(OutcomeIndex),   // which outcome won
    Expired,
}
```

`SlotIdentity` uniquely identifies a tracked UTXO's role. Within a contract, each `SlotIdentity` value appears at most once in the outpoint set (a hard invariant verified by compliance tests). Pairing each outpoint with its label at the store boundary replaces the earlier positional-ordering convention — it moves slot identity from "implicit in `Vec` index" to "explicit in the data," which eliminates a class of silent-mis-ordering bugs and handles variable-N multi-outcome markets cleanly.

Each `CovenantPhase` maps to a specific subset of slots. This mapping is the bridge between the state machine and the UTXO-following model — when a market transitions between phases, the engine knows which slots to expect in the new outputs.

**Binary** (8 slots, 1:1 with the existing covenant):

| BinaryCovenantPhase | Live Slots |
| ------------- | ---------- |
| Dormant       | DormantYesRt (0), DormantNoRt (1) |
| Unresolved    | UnresolvedYesRt (2), UnresolvedNoRt (3), UnresolvedCollateral (4) |
| ResolvedYes   | ResolvedYesCollateral (5) |
| ResolvedNo    | ResolvedNoCollateral (6) |
| Expired       | ExpiredCollateral (7) |

**Multi-outcome** (5N+2 slots, parameterized by `outcome_count`):

| MultiOutcomeCovenantPhase | Live Slots |
| --- | --- |
| Dormant       | DormantYesRt × N, DormantNoRt × N |
| Unresolved    | UnresolvedYesRt × N, UnresolvedNoRt × N, UnresolvedCollateral |
| Resolved(k)   | ResolvedCollateral(k) |
| Expired       | ExpiredCollateral |

The engine maps between the public `MarketState` and internal `CovenantPhase` as follows:
- Binary `Trading` with `outstanding_pairs == 0` → `BinaryCovenantPhase::Dormant`; with `outstanding_pairs > 0` → `BinaryCovenantPhase::Unresolved`.
- Multi-outcome `Trading` with all supplies zero → `MultiOutcomeCovenantPhase::Dormant`; with any non-zero supply → `MultiOutcomeCovenantPhase::Unresolved`.
- Binary `ResolvedYes`/`ResolvedNo`/`Expired` map directly to `BinaryCovenantPhase` variants.
- Multi-outcome `Resolved { winning_outcome }`/`Expired` map directly to `MultiOutcomeCovenantPhase` variants.

Internal dispatch (e.g., script derivation, output matching) uses a `pub(crate) trait MarketBehavior` implemented for `BinaryMarketParams` and `MultiOutcomeMarketParams` to avoid scattering `match` statements across the engine. This trait is internal and has no effect on the public API.

#### LmsrPoolState

```rust
pub struct PoolReserves {
    pub yes: u64,
    pub no: u64,
    pub collateral: u64,
}

pub enum LmsrPoolState {
    Active {
        s_index: u64,
        reserves: PoolReserves,
    },
    Closed {
        final_txid: Txid,
    },
}
```

Active pools track their reserves and s_index. Outpoints are internal to the engine and not exposed in the public state.

`Closed` represents a pool whose admin has reclaimed all reserve UTXOs via the dedicated Simplicity close script path. See [lmsr-pool-close-path.md](../contracts/lmsr-pool/lmsr-pool-close-path.md) for the covenant design. `StateFilter::TerminalOnly` returns closed pools.

#### OrderState

```rust
pub enum OrderState {
    Active {
        tracking: OrderTracking,
        active_txid: Txid,
        offered_amount: u64,
        total_filled: u64,
    },
    Consumed {
        tracking: OrderTracking,
        final_txid: Txid,
        offered_amount: u64,
    },
    Cancelled {
        tracking: OrderTracking,
        cancel_txid: Txid,
        offered_amount: u64,
        total_filled: u64,
    },
}

/// How the engine tracks an order. Controls both persistence behavior
/// and the accuracy of `offered_amount`.
pub enum OrderTracking {
    /// Ingested from the order's creation transaction, with full fill history
    /// and permanent storage. The engine persists every transition to
    /// `ContractHistory` (if implemented) and never auto-untracks the order
    /// upon reaching terminal state. `offered_amount` is the true original
    /// offered value.
    ///
    /// Use for orders you've created and want to audit (fill-by-fill history,
    /// cancellation detection, recovery).
    Persistent,

    /// Ingested from the creation transaction but tracked ephemerally.
    /// `offered_amount` is the true original. No history is persisted.
    /// The engine auto-untracks the order past finality when it reaches
    /// a terminal state (Consumed or Cancelled).
    ///
    /// Use for freshly-discovered orders (e.g. via Nostr announcement that
    /// includes the creation tx) where accurate fill-progress display matters
    /// but storing the full audit trail does not.
    EphemeralFresh,

    /// Ingested from a mid-lifecycle snapshot (no creation tx available).
    /// `offered_amount` reflects the value at discovery time, not the true
    /// original — any fills that happened before ingestion are unobservable.
    /// No history is persisted. Auto-untracks past finality when terminal.
    ///
    /// Use for discovered orders encountered mid-life (order books, routing).
    EphemeralMidLife,
}
```

`offered_amount` is the order's baseline value for fill-progress calculations. Under `Persistent` and `EphemeralFresh` tracking, it equals the true original offered amount (the creation tx's locked output value). Under `EphemeralMidLife`, it's the locked value at discovery — fills that occurred before ingestion aren't captured. `total_filled` is cumulative fills since ingestion. Both are denominated in the order's locked asset — the asset the maker offered (BASE for sell-base orders, QUOTE for sell-quote orders, per `MakerOrderParams.direction`). Remaining liquidity is always `offered_amount - total_filled` regardless of tracking mode.

Engine behavior forks on `tracking`: `ContractHistory` writes are skipped for the two `Ephemeral*` variants, and `prune_finalized` auto-untracks terminal `Ephemeral*` orders past finality. The `Persistent` variant behaves like other persistent contracts (markets, pools) — history written if the store supports it, terminal state preserved until explicitly untracked.

UX forks on the `offered_amount` accuracy implied by `tracking`: "X of Y filled" displays are honest under `Persistent` and `EphemeralFresh`; under `EphemeralMidLife`, the denominator is "baseline at discovery" rather than "true original," which UIs should clarify. View-layer helpers (`Order::has_full_origin()`, etc.) expose this distinction for callers. Outpoints are internal to the engine and not exposed in the public state.

### Pagination Types

All listing and bulk-read methods use cursor-based pagination. Cursors are opaque — only the store generates and interprets them. This avoids the offset-based pagination instability problem (items shifting between pages due to concurrent ingestion or state changes) and allows each store implementation to choose its own ordering strategy.

```rust
pub struct Cursor(String);  // opaque — only the store generates and interprets cursors

impl Cursor {
    pub fn from_string(s: String) -> Self { Self(s) }
    pub fn as_str(&self) -> &str { &self.0 }
}

pub struct Pagination {
    pub after: Option<Cursor>,  // None = first page
    pub limit: u32,             // 0 = return no items, only total count
}

pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<Cursor>,  // None = last page
    pub total: u64,                   // total matching items across all pages
}

pub enum StateFilter {
    All,
    ActiveOnly,
    TerminalOnly,
}
```

**Cursor opacity and validation**: The `Cursor` type has a private inner field — callers can extract the string via `as_str()` (for serialization, e.g., HTTP query params) and reconstruct via `from_string()` (for deserialization), but they cannot inspect or construct cursor values meaningfully. The store encodes whatever it needs into the cursor string (e.g., last seen contract_id, ordering position, method identity, filter). The caller passes cursors back without interpreting them. The store validates cursors on use: if a caller passes a cursor from one method to a different method, or reuses a cursor with different parameters (e.g., a cursor from `list_markets(ActiveOnly, ...)` used with `list_markets(TerminalOnly, ...)`), the store should return an error. Cursors encode a position within a specific filtered result set — changing the filter invalidates the position.

**Count-only queries**: Pass `limit: 0` to get just the `total` count without loading any items. No separate count methods needed. The response is `Page { items: vec![], next_cursor: None, total }` — `next_cursor` is `None` because no items were returned and there is no position to continue from. To fetch the first page after a count-only query, call again with `after: None` and a non-zero limit.

**`total` is always included**: Every paginated response includes the total count of matching items across all pages. This enables "showing 1-20 of 1,234 markets" UI without separate count calls. Implementors can cache counts if the overhead of `COUNT(*)` becomes significant.

### Side

```rust
pub enum Side { Yes, No }
```

Identifies which side of an outcome a token represents. For a binary market, the single outcome has YES (pays if the event happens) and NO (pays if it doesn't). For a multi-outcome market, each outcome k has YES_k (pays if outcome k wins) and NO_k (pays if outcome k doesn't win). Used in `OutputRole`, `TradeSpec`, `AssetInfo`, `BinaryMarketTransition`, `MultiOutcomeMarketTransition`.

### OutcomeIndex

```rust
pub struct OutcomeIndex(u8);

impl OutcomeIndex {
    /// The sole outcome of a binary market. Used wherever a unified signature
    /// requires an outcome parameter — for binary markets, pass `OutcomeIndex::BINARY`.
    pub const BINARY: OutcomeIndex = OutcomeIndex(0);

    pub const fn new(index: u8) -> Self { Self(index) }
    pub const fn as_u8(self) -> u8 { self.0 }
}
```

Identifies a specific outcome within a market.

- **Binary markets** have exactly one outcome (the single event being predicted). The sole valid index is `OutcomeIndex::BINARY`. APIs that take `OutcomeIndex` accept only this value for binary markets — other values return `CoreError::InvalidParams`.
- **Multi-outcome markets** have `outcome_count` outcomes (the N mutually exclusive events). Valid indices are `0..outcome_count`.

Used alongside `Side` to identify a specific token: `(OutcomeIndex, Side)` pinpoints YES_k or NO_k for outcome k. For binary markets, the `OutcomeIndex` is always `BINARY` and the `Side` distinguishes YES from NO.

The newtype prevents accidental conflation with other `u8` indices (output positions, byte values, etc.) and enables compile-time documentation at call sites.

### FeeRate

```rust
pub struct FeeRate(u64); // internal: sats per kvB

impl FeeRate {
    pub fn from_sat_per_kvb(rate: u64) -> Self;
    pub fn from_sat_per_vb(rate: u64) -> Self; // multiplies by 1000
    pub fn as_sat_per_kvb(&self) -> u64;
}
```

A newtype for fee rates with named constructors to eliminate unit ambiguity. The internal representation is sats per kvB (the Elements/Liquid convention). PSET builders accept `FeeRate` and compute the exact fee internally based on the actual transaction weight. The caller obtains the fee rate from their chain backend.

### Network

```rust
pub enum Network {
    Liquid,
    LiquidTestnet,
    ElementsRegtest,
}
```

Target network for Simplicity compilation and address derivation. Passed to `ContractEngine::new` and used internally — consumers interact with it only at construction time.

### WalletFunding

```rust
pub struct WalletFunding<'a> {
    pub available_utxos: &'a [UnblindedUtxo],
    pub fee_rate: FeeRate,
    pub return_script: &'a Script,
}
```

The wallet's contribution to a transaction. Shared across all PSET builders — the caller constructs one and passes it to any builder. `available_utxos` is the candidate UTXO pool (passing all wallet UTXOs is the expected usage — the builder selects the minimum needed). `fee_rate` is obtained from the chain backend. `return_script` receives all non-covenant, non-fee outputs: L-BTC change, collateral refunds, redemption payouts, trade proceeds, etc. Named `return_script` rather than `change_script` because it handles more than just change — it's the destination for all funds returning to the wallet.

**`available_utxos` contains wallet UTXOs only**: Covenant UTXOs (reissuance token slots, collateral, pool reserves, order locked value) are managed internally by the engine — it reads them from stored outpoints. Do not include covenant UTXOs in `available_utxos`. The builder uses both internally-sourced covenant UTXOs and caller-provided wallet UTXOs to construct the transaction.

The builder consolidates outputs to the same script and asset for efficiency — see [Output Consolidation](#output-consolidation).

### UnblindedUtxo

A wallet UTXO with unblinded (revealed) asset, value, and blinding factors. The caller obtains these from their wallet's UTXO set. `deadcat-core` defines this type; `deadcat-sdk` imports it from core.

```rust
pub struct UnblindedUtxo {
    pub outpoint: OutPoint,
    pub txout: elements::TxOut,  // full committed output (needed for PSET witness_utxo fields)
    pub asset_id: AssetId,
    pub value: u64,
    pub asset_blinding_factor: elements::confidential::AssetBlindingFactor,
    pub value_blinding_factor: elements::confidential::ValueBlindingFactor,
}
```

`txout` is the full committed transaction output (with blinded asset and value commitments), not just the script pubkey. PSET builders need the committed output for `witness_utxo` fields. The remaining fields are the unblinding of that committed output — the secrets that reveal the actual asset and value.

### StepReport

Result of a `step` call, containing any transactions that were processed:

```rust
pub struct StepReport {
    pub transactions: Vec<ProcessedTransaction>,
}
```

Empty `transactions` means no contracts were affected — either the engine was already current, or new blocks contained no relevant transactions. The caller iterates over transactions, then over each transaction's transitions, for UI updates (e.g., "Your order was filled").

### ContractEntry

```rust
pub struct ContractEntry<P, S> {
    pub contract_id: ContractId,
    pub params: P,
    pub state: S,
    pub synced_to: u32,
}

pub type MarketEntry = ContractEntry<MarketParams, MarketState>;
pub type PoolEntry = ContractEntry<LmsrPoolParams, LmsrPoolState>;
pub type OrderEntry = ContractEntry<MakerOrderParams, OrderState>;
```

`synced_to` indicates the block height through which this contract has been checked for chain activity. It advances during `step` even when no transitions are found for the contract. See [Chain Sync](#chain-sync).

Generic entry type used by all listing and relationship query methods. The type aliases provide ergonomic names (`Page<MarketEntry>` vs `Page<ContractEntry<MarketParams, MarketState>>`). `MarketEntry` uses the umbrella `MarketParams`/`MarketState`, so a single `list_markets` call returns both binary and multi-outcome markets; callers destructure the umbrella enum (or use the `Market` view type introduced in Stage 2) when they need type-specific fields.

### DerivedContractData

```rust
pub struct DerivedContractData {
    pub asset_ids: Vec<(AssetId, AssetInfo)>,
    pub covenant_scripts: Vec<Script>,
}
```

Permanent data derived from Simplicity compilation, passed to the store during contract tracking so it can build indexes without knowing about Simplicity. These fields never change after ingestion — they are static properties of the contract program. `asset_ids` maps each asset (YES/NO tokens, YES/NO reissuance tokens) to its `AssetInfo`. `covenant_scripts` is the set of covenant script pubkeys used for chain sync (catch-up scanning and steady-state subscription registration). Only prediction markets produce asset IDs; pools and orders have empty `asset_ids`.

**`covenant_scripts` per contract type**:
- **Binary markets**: 8 scripts across all covenant phases and slot types. Static — these scripts cover the market's entire lifecycle.
- **Multi-outcome markets**: `5N + 2` scripts, where `N = outcome_count`. All static — these scripts cover the market's entire lifecycle, with per-outcome variants for the 4N RT slots and the N resolved-collateral slots.
- **Orders**: The single covenant script. Static — the script does not change across partial fills.
- **Pools**: Empty. Pool scripts encode the s_index, which changes on every swap (unbounded), so pre-storing all possible scripts is impractical. Pool sync uses outpoint-based forward-chaining and structural output identification instead. See [Chain Sync](#chain-sync).

### InitialContractState

```rust
pub struct InitialContractState {
    pub outpoints: Vec<(SlotIdentity, OutPoint)>,
    pub position: ChainPosition,
}
```

The mutable initial state of a contract at ingestion time, passed to the store via `track_contract`. Groups the two fields that describe "where the contract is right now": `outpoints` are the contract's current tracked UTXOs (derived from the creation transaction or provided via a `Current` snapshot), each labeled with its [`SlotIdentity`](#slotidentity-and-covenantphase); `position` is the chain position at which those outpoints were confirmed.

This is intentionally separate from `DerivedContractData`, which contains permanent data derived from Simplicity compilation (scripts, asset IDs). `InitialContractState` contains mutable state — `outpoints` change with every transition (via `apply_transitions`), and `position` sets the initial `synced_to` height. The two structs represent different categories of data the engine pre-computes for the store.

**Outpoints per contract type** (each outpoint labeled with its [`SlotIdentity`](#slotidentity-and-covenantphase)):
- **Binary markets**: Dormant phase has 2 outpoints labeled `DormantYesRt` and `DormantNoRt`. Unresolved has 3 labeled `UnresolvedYesRt`, `UnresolvedNoRt`, `UnresolvedCollateral`. Terminal phases (ResolvedYes/ResolvedNo/Expired) have 1 labeled with the corresponding `*Collateral` slot.
- **Multi-outcome markets** (with `N = outcome_count`): Dormant has 2N outpoints labeled `DormantYesRt(k)` and `DormantNoRt(k)` for k ∈ [0, N). Unresolved has 2N+1 (the 2N per-outcome RTs plus `UnresolvedCollateral`). Terminal phases have 1 labeled `ResolvedCollateral` (with the winning outcome implicit from market state) or `ExpiredCollateral`.
- **Pools**: 3 outpoints labeled `YesReserve`, `NoReserve`, `CollateralReserve`.
- **Orders**: 1 outpoint labeled `Utxo`.

**Slot identity is explicit, not positional**: the engine, store, and PSET builders work with `Vec<(SlotIdentity, OutPoint)>` — each outpoint carries its own label rather than deriving identity from its position in a `Vec`. The engine produces labels deterministically from the current covenant phase during ingestion (`InitialContractState`) and transitions (`StateUpdate.new_outpoints`). The store persists the `(SlotIdentity, OutPoint)` pairs. `contract_outpoints` returns them. PSET builders look up the needed slot by `SlotIdentity` — no positional convention to maintain or accidentally violate. Within a single contract, each `SlotIdentity` value appears at most once (engine-enforced on write; compliance-tested at the store boundary).

### Oracle Attestation

The oracle resolve builder takes an `elements::secp256k1_zkp::schnorr::Signature` — a BIP-340 Schnorr signature from the oracle. The oracle signs a BIP-340 tagged hash message:

```
message = tagged_hash("deadcat/oracle_attestation", market_id || outcome_byte)
        = SHA256(SHA256("deadcat/oracle_attestation") || SHA256("deadcat/oracle_attestation") || market_id || outcome_byte)
```

The message structure is unified across binary and multi-outcome markets — `market_id` and `outcome_byte` differ based on market kind:

- **Binary**: `market_id = SHA256(yes_token_asset_id || no_token_asset_id)`. `outcome_byte` is `0x01` for YES (the event happened) or `0x00` for NO. Binary resolution picks a `Side`, not an outcome index — the single event has two sides, and the oracle attests which side won.
- **Multi-outcome**: `market_id = SHA256(yes_token_asset_ids[0] || no_token_asset_ids[0] || ... || yes_token_asset_ids[N-1] || no_token_asset_ids[N-1])`. `outcome_byte` is the u8 `outcome_index` of the winning outcome, in range `[0, N-1]`. Multi-outcome resolution picks an `OutcomeIndex` — N events compete and exactly one wins.

`market_id` is a covenant-internal identifier derived from the market's token asset IDs — it is NOT the same as `ContractId`. Domain separation across binary and multi-outcome markets is achieved via the different `market_id` derivations. See [oracle-bip340-tagged-hash.md](../protocol/oracle-bip340-tagged-hash.md) for the full specification.

**`MarketResolution` type**: because binary and multi-outcome resolutions are semantically different (binary attests a `Side`, multi-outcome attests an `OutcomeIndex`), the oracle API uses a discriminated union rather than conflating them under a single `OutcomeIndex` parameter:

```rust
pub enum MarketResolution {
    /// Binary market resolution: which side of the single event won.
    /// Encodes as outcome_byte = 0x01 for Yes, 0x00 for No.
    Binary(Side),

    /// Multi-outcome market resolution: which outcome event won.
    /// Encodes as outcome_byte = OutcomeIndex::as_u8().
    MultiOutcome(OutcomeIndex),
}
```

```rust
pub struct MarketId([u8; 32]);

impl MarketId {
    pub fn as_bytes(&self) -> &[u8; 32];
    pub fn from_bytes(bytes: [u8; 32]) -> Self;
}

pub struct OracleAttestationSpec {
    pub market_id: MarketId,
    pub resolution: MarketResolution,     // what the oracle is attesting to
    pub message: [u8; 32],                // tagged_hash, ready to sign
    pub oracle_pubkey: XOnlyPublicKey,    // elements::secp256k1_zkp::XOnlyPublicKey
}
```

Public API surface for oracle message construction and verification:

```rust
// Standalone pure functions (no engine needed):

/// Compute the market_id from market params (handles both binary and multi-outcome).
pub fn compute_market_id(params: &MarketParams) -> MarketId;

/// Compute the BIP-340 tagged hash message the oracle needs to sign.
/// Usable by oracle services without a ContractEngine.
///
/// The caller is responsible for pairing the correct MarketResolution variant
/// with the correct market_id (binary vs multi-outcome). A binary MarketResolution
/// paired with a multi-outcome market_id produces a valid-but-meaningless message.
pub fn oracle_attestation_message(market_id: MarketId, resolution: MarketResolution) -> [u8; 32];

// Engine methods (use contract_id; handle market_id lookup and resolution-variant validation internally):

impl<S: ContractStore> ContractEngine<S> {
    /// Returns { market_id, resolution, message, oracle_pubkey } for an oracle to sign.
    /// Validates that the MarketResolution variant matches the contract's kind:
    /// - Binary market requires MarketResolution::Binary(_); returns CoreError::InvalidParams otherwise.
    /// - Multi-outcome market requires MarketResolution::MultiOutcome(_) with outcome in 0..outcome_count.
    pub fn oracle_attestation_spec(
        &self,
        contract_id: &ContractId,
        resolution: MarketResolution,
    ) -> Result<OracleAttestationSpec, CoreError<S::Error>>;

    /// Verifies an oracle attestation against a specific (contract, resolution) pair.
    /// Useful for oracles to dry-run signatures before publishing, and for clients
    /// verifying attestations published out-of-band.
    pub fn verify_oracle_attestation(
        &self,
        contract_id: &ContractId,
        resolution: MarketResolution,
        signature: &schnorr::Signature,
    ) -> Result<bool, CoreError<S::Error>>;
}
```

The engine-level resolve builder accepts a raw signature and identifies the resolution by trial verification — for binary markets, verifies against both `MarketResolution::Binary(Side::Yes)` and `MarketResolution::Binary(Side::No)`; for multi-outcome markets, verifies against `MarketResolution::MultiOutcome(OutcomeIndex::new(k))` for each `k` in `0..outcome_count`. If the signature doesn't verify against any valid resolution, the engine returns `CoreError::OracleSignatureInvalid`.

### RedemptionKind

```rust
pub enum RedemptionKind {
    PostResolution,  // ResolvedYes/ResolvedNo with outstanding_pairs decremented (winning tokens redeemed at full value)
    Expiry,          // Expired with outstanding_pairs decremented (any tokens redeemed at half value)
}
```

Used as a discriminant in `MarketTransition::Redeemed`. Post-resolution vs expiry redemption is a meaningful user-facing distinction (full value vs half value).

Note: `IssuanceKind` (Initial vs Subsequent) is an internal type used by the engine for PSET routing. The public API simplifies issuance to just `{ pairs, collateral_locked }` — the distinction between initial and subsequent issuance is hidden from callers. See [TransitionDetails](#transitiondetails).

### Trade Types

```rust
pub enum TradeDirection { Buy, Sell }

pub enum TradeAmount {
    /// Taker specifies the exact amount they send.
    /// Buy: exact collateral to spend. Sell: exact tokens to sell.
    ExactInput(u64),
}

pub struct TradeSpec {
    pub outcome: OutcomeIndex,    // BINARY for binary markets; 0..N-1 for multi-outcome
    pub side: Side,
    pub direction: TradeDirection,
    pub amount: TradeAmount,
}
```

`TradeSpec` is the input to `quote_trade`. The four axes are orthogonal — any combination of outcome, side, direction, and amount mode is valid. For binary markets, `outcome` is always `OutcomeIndex::BINARY` (the single outcome); `side` picks YES or NO. For multi-outcome markets, `outcome` identifies which outcome's pool the trade targets, and `side` picks YES_k or NO_k within that outcome's pool.

**V1 is exact-input-only.** `TradeAmount` intentionally exposes only `ExactInput(u64)` in v1. An `ExactOutput` mode is deferred until the router's fill math, slippage semantics, and stale-quote checks are specified for exact-output routing; it is not part of the current public API.

**Basket trades are NOT part of `TradeSpec`.** Cross-outcome splits/merges (`MultiOutcomeMarket::build_split_yes_pset` etc.) are exposed as dedicated builders rather than routed through the trade quote system. Each `TradeQuote` corresponds to a single-outcome trade; multi-outcome traders issuing a basket construct a composition of single-outcome trades plus market-contract-native primitives. Cross-outcome arb (single-tx composition of a market split/merge with N pool swaps) is deferred to v2; see [Future: Cross-Outcome Arb API (v2)](#future-cross-outcome-arb-api-v2).

### TradeQuote and Related Types

```rust
pub struct TradeQuote {
    // Public — for display to the user:
    pub outcome: OutcomeIndex,  // which outcome was traded (BINARY for binary markets)
    pub side: Side,
    pub direction: TradeDirection,
    pub requested_amount: u64,
    pub filled_amount: u64,    // same units as requested_amount (input units for ExactInput, output units for future ExactOutput)
    pub total_input: u64,      // Buy: collateral spent. Sell: tokens sent.
    pub total_output: u64,     // Buy: tokens received. Sell: collateral received.
    pub estimated_fee: u64,    // Estimated transaction fee in sats, based on the route's weight model. Actual fee (computed at build time from real coin selection) may differ — see Trade PSET Builder.
    pub effective_price: f64,  // Display-only approximation. Do not use for computation. Use total_input/total_output for exact amounts.
    pub legs: Vec<RouteLeg>,

    // Crate-internal — consumed by build_trade_pset:
    pub(crate) route: TradeRoute,
}

pub struct RouteLeg {
    pub source: LiquiditySource,
    pub input_amount: u64,
    pub output_amount: u64,
}

pub enum MarketAssist {
    IssuePairs { pairs: u64 },
    CancelPairs { pairs: u64 },
}

pub enum LiquiditySource {
    LmsrPool {
        pool_id: ContractId,
        old_s_index: u64,
        new_s_index: u64,
        market_assist: Option<MarketAssist>,
    },
    LimitOrder {
        order_id: ContractId,
        price: u64,         // quote asset units per base asset unit (from MakerOrderParams.price)
        base_filled: u64,   // base asset units filled in this leg
    },
}
```

`TradeQuote` represents the best available fill. If `filled_amount < requested_amount`, the fill is partial — the wallet decides whether to proceed or warn the user. `quote_trade` returns `Err(CoreError::NoLiquidity)` only when zero liquidity is available (no pools, no orders); any positive fill returns `Ok`.

The `pub(crate)` field `route` makes `TradeQuote` non-constructable by external consumers — they can only receive one from the engine and pass it to `build_trade_pset`. See [Trade PSET Builder](#trade-pset-builder).

`TradeRoute` is a crate-internal type capturing the route plan (contract IDs, leg amounts, outpoint snapshots) needed by `build_trade_pset`. External consumers cannot inspect or construct it. For an assisted pool leg, `TradeRoute` carries the full parent-market continuation and burn/issuance bookkeeping; the public `market_assist` field is intentionally just a display summary.

`RouteLeg` breaks down how the trade is routed across liquidity sources. `LiquiditySource::LmsrPool` includes s-index movement for "pool moved from 50 to 55" display and an optional `market_assist` summary. `IssuePairs` means the route co-spends the parent market's issuance path for this same `(market, outcome)` and mints `pairs` YES+NO directly into the pool's reserves. `CancelPairs` means the route co-spends the parent market's cancellation path, burns `pairs` YES+NO out of the pool reserves, and releases the corresponding market collateral. `LiquiditySource::LimitOrder` includes the matched price and base fill amount.

For multi-outcome markets, the assist always refers to the same `outcome` as the pool leg — no cross-outcome behavior is implied by `TradeQuote`. In v1, at most one `LmsrPool` leg in a route may carry `market_assist: Some(_)`; if an assisted and non-assisted route tie on taker outcome, the non-assisted route wins. Degenerate fixed-`s_index` public pair rebalances remain covenant-valid but are not intentionally emitted by `quote_trade`.

### BootstrapEstimate

Result of `estimate_bootstrap` — the canonical default bootstrap plan for a pool creation flow:

```rust
pub struct BootstrapEstimate {
    pub initial_yes_reserve: u64,
    pub initial_no_reserve: u64,
    pub initial_collateral_reserve: u64,
    pub initial_s_index: u16,
}

pub enum BootstrapError {
    InvalidStartingPriceBps { starting_price_bps: u16 },
    ArithmeticOverflow,
}
```

The three reserves are in different assets (YES tokens, NO tokens, collateral). The operator uses these to plan capital acquisition — e.g., issuing `max(yes, no)` token pairs (which costs `max * collateral_per_pair` collateral from the parent market) plus providing `initial_collateral_reserve` directly. Total capital outlay depends on the market's `collateral_per_pair` and what the operator does with leftover tokens, which are wallet-layer concerns outside this function's scope.

`estimate_bootstrap` returns the **canonical default** reserve vector for a given curve and starting price; it does **not** define the only valid reserve vector for the pool. The helper:

1. Snaps `starting_price_bps` to the nearest valid `initial_s_index`.
2. Computes the inward-snapped "useful band" bounds: the lowest and highest table indices whose fee-free YES spot prices remain within `[10, 9990]` bps (0.1%-99.9%).
3. Returns the smallest reserve vector that lets the pool move from `initial_s_index` to those useful-band bounds while preserving `MIN_POOL_RESERVE` on all three reserves.

This inward snap matters because `q_step_lots` is ceil-rounded: the literal table edges can lie outside the useful 0.1%-99.9% band, so funding the full table by default would pre-load dead tail liquidity. The recommended flow is `estimate_bootstrap` → let the operator accept or override the reserves → pass the chosen reserves into `build_lmsr_bootstrap_pset`. Explicit over-funded or under-funded starting inventories remain covenant-valid as long as they satisfy the covenant minimums and the transaction is fundable.

`estimate_bootstrap` is a standalone pure helper, so it returns `BootstrapError`, not `CoreError`. `starting_price_bps` must be in the range `(0, 10000)` exclusive — 0 and 10000 return `BootstrapError::InvalidStartingPriceBps` because they represent 0% and 100% probabilities with infinite reserve ratios. Values that overflow the reserve computation return `BootstrapError::ArithmeticOverflow`.

### ContractMatch

Returned by `ContractStore::find_by_outpoints`. Used internally by the engine to identify which tracked contracts are affected by a transaction and which specific outpoints (and their slot roles) matched. Callers of the engine never see this type — it exists at the engine-store boundary only.

```rust
pub struct ContractMatch {
    pub contract_id: ContractId,
    pub matched_outpoints: Vec<(SlotIdentity, OutPoint)>,
}
```

`matched_outpoints` carries slot labels so the engine can dispatch per-slot logic (e.g., "the UnresolvedCollateral slot was spent, this is a cancellation or resolution") without cross-referencing against `contract_outpoints`. The store indexes outpoints alongside their `SlotIdentity` labels; lookup returns both.

### Transaction-Level Types

Transaction interpretation and processing results are grouped at the **transaction level**, not the per-contract level. A single transaction can affect multiple contracts (e.g., a trade routing through a pool and filling an order), and its non-covenant outputs are a property of the transaction, not of any individual contract's transition.

```rust
pub struct Transition {
    pub contract_id: ContractId,
    pub details: TransitionDetails,
}

pub struct InterpretedTransaction {
    pub txid: Txid,
    pub transitions: Vec<Transition>,
    pub external_outputs: Vec<ExternalOutput>,
}

pub struct ProcessedTransaction {
    pub interpretation: InterpretedTransaction,
    pub position: ChainPosition,
}
```

`Transition` is the per-contract view — which contract was affected and what happened. It carries no transaction-level data (txid, outputs, position). `InterpretedTransaction` groups all transitions from a single transaction with the transaction's non-covenant outputs. `ProcessedTransaction` wraps `InterpretedTransaction` with `ChainPosition` for confirmed transactions.

`interpret_transaction` returns `InterpretedTransaction` (works for confirmed and unconfirmed). `step` returns `StepReport { transactions: Vec<ProcessedTransaction> }` (always confirmed). The composition avoids `Option<ChainPosition>` — `step` callers never unwrap a field that's always `Some`.

Outpoints are intentionally omitted — they are internal to the engine's UTXO-following state machine. The `txid` provides sufficient correlation for block explorer lookups and transaction graph traversal.

**Why `external_outputs` is transaction-level, not per-transition**: A key invariant of the Deadcat protocol is that while a transaction can compose multiple contracts (co-spending pool reserves and order UTXOs), each non-covenant output is associated with at most one contract. A `MakerReceive` output belongs to a specific order (positional at `current_index()`). A `TradeReceive` output is the taker's consolidated receive. A `Fee` output is transaction-global. No output serves dual roles for two different contracts. This means output classification never conflicts across contracts — an output is either `Unknown` from a contract's perspective or has exactly one role, and when multiple contracts can classify the same output they always agree (e.g., both the pool and order classify the taker receive as `TradeReceive`). The engine exploits this by computing a single merged classification at the transaction level, eliminating per-contract duplication and the merge boilerplate every wallet integrator would otherwise need. When a wallet needs to attribute a `MakerReceive` output to a specific order (e.g., two orders filled in the same trade produce two `MakerReceive` outputs), it matches the output's `script_pubkey` against the filled orders' `maker_receive_spk_hash` from their params.

#### Multi-Contract Transaction Patterns

Single-contract transitions (one market, one pool, one order) are fully described by their respective `TransitionDetails` variant. Some transactions legitimately span multiple contracts atomically — a trade (pool + maybe maker orders), an atomic issuance + pool bootstrap (market + new pool creation). For these, the raw per-contract transitions are always preserved in `InterpretedTransaction.transitions`; additional helper methods on `InterpretedTransaction` classify recognized multi-contract patterns without collapsing the raw data:

```rust
impl InterpretedTransaction {
    /// Returns trade details if this transaction realizes a single-outcome
    /// taker trade: one pool swap AND/OR one-or-more resting LOB order fills,
    /// all targeting the same `(market_id, outcome, side)`.
    ///
    /// Returns `None` for transactions that don't match this shape:
    /// - Pool state change with no taker (admin adjustment, close)
    /// - Market-only transitions (issuance, resolution, redemption)
    /// - Cross-outcome arb patterns (market split/merge + pool swaps);
    ///   use `as_cross_outcome_arb()` in v2
    /// - Multi-outcome bundled trades (pool swaps on different outcomes
    ///   in one tx without a market leg)
    /// - Multi-pool same-outcome (unusual; not produced by `build_trade_pset`)
    ///
    /// Raw per-contract transitions are always available via `self.transitions`
    /// regardless of whether this helper matches. See [`TradeRealized`](#traderealized)
    /// for invariants on the returned value.
    pub fn as_trade(&self) -> Option<TradeRealized>;

    /// Net change in token and collateral balances attributable to a specific contract
    /// from this transaction. Rolls up per-contract transitions and external outputs.
    /// Returns None if the contract had no involvement in this transaction.
    ///
    /// `as_trade()` is pattern-classification (returns structured aggregate);
    /// `net_effect_for()` is per-contract rollup (returns wallet-relevant deltas).
    /// They answer different questions. A caller asking "what did this trade do
    /// to my pool balance?" uses `net_effect_for(pool_id)`; a caller asking
    /// "was this a trade and what were its legs?" uses `as_trade()`.
    pub fn net_effect_for(&self, contract_id: &ContractId) -> Option<ContractNetEffect>;
}

// Cross-outcome arb classification (`as_cross_outcome_arb`, `CrossOutcomeArb`) is
// deferred to v2 alongside the arb PSET builder. See "Future: Cross-Outcome Arb API"
// for the deferred surface.

/// Aggregated details of a single-outcome taker trade.
///
/// ## Structure
/// - `market_id`, `outcome`, `side`: all legs target this triple.
/// - `pool_leg`: at most one pool swap (the router targets one pool per
///   outcome; multi-pool-same-outcome patterns don't classify as trades).
/// - `order_legs`: zero or more LOB fills on resting orders at this
///   `(outcome, side)`, in fill order.
/// - `total_input` / `total_output`: aggregated across all legs.
///
/// ## Invariants (engine-enforced at construction)
/// - At least one leg present (`pool_leg.is_some()` OR `!order_legs.is_empty()`).
/// - All `OrderLegRealized.market_id == market_id` and `.outcome == outcome`.
/// - `PoolLegRealized.market_id == market_id` if present.
///
/// ## Future (v2)
/// Multi-market and cross-outcome arb patterns will NOT extend this type;
/// they get their own aggregation helpers (e.g. `CrossOutcomeArb`).
/// `TradeRealized` remains strictly single-outcome.
pub struct TradeRealized {
    pub market_id: ContractId,
    pub outcome: OutcomeIndex,                   // BINARY for binary markets
    pub side: Side,
    pub direction: TradeDirection,
    pub total_input: u64,
    pub total_output: u64,
    pub pool_leg: Option<PoolLegRealized>,       // Some if pool was hit
    pub order_legs: Vec<OrderLegRealized>,       // One per LOB order filled
}

pub struct ContractNetEffect {
    pub token_deltas: Vec<(AssetId, i64)>,       // asset → signed delta in user's tokens
    pub collateral_delta: i64,                   // signed change in user's collateral
}
```

**Single-contract transactions** still have `as_*` helpers return `None` — they return `Some` only when the tx exactly matches the multi-contract pattern. A tx that moved a pool's s_index but did nothing else (no market co-spend) produces a single `PoolTransition::Swapped` and `as_trade()` returns `None` (no taker was involved). A tx that combines a market split/merge with N pool swaps (what would be an arb in v2) still ingests cleanly — the raw `MarketTransition::SplitYes` + N `PoolTransition::Swapped` are preserved in `self.transitions`, just without an aggregate arb classification until v2.

**Transactions are interpreted independently.** The engine does not pattern-match across transactions to recognize user-level behaviors that span multiple txs. For example, a user-level "cross-outcome swap" (a SplitNo in tx 1 followed by a CancelledPair in tx 2 on the same market) produces two independent single-primitive interpretations. Higher-level tools that want to aggregate across tx history can do so externally; the core engine's job is single-tx classification.

### StateUpdate

The write-path type passed to the store via `apply_transitions`. Contains the full state-advancement delta: old + new contract state, labeled old + new outpoints, and the transition details. Not used on the read path (history queries return `HistoryEntry` which omits outpoints). Does not include the computed output classification (`external_outputs`) since those are derived from the transaction at query time and do not need to be persisted:

```rust
pub struct StateUpdate {
    pub contract_id: ContractId,
    pub txid: Txid,
    pub position: ChainPosition,
    pub old_state: Contract,
    pub new_state: Contract,
    pub old_outpoints: Vec<(SlotIdentity, OutPoint)>,
    pub new_outpoints: Vec<(SlotIdentity, OutPoint)>,
    pub details: TransitionDetails,
}
```

**Why `old_state` and `new_state`**: rollback needs the pre-transition state to restore (several transitions aren't reversible from `TransitionDetails` alone — e.g., `PoolTransition::Closed` doesn't carry the old `s_index`). The engine computes both and passes them along; the store persists whatever it needs for its durability and rollback requirements. Having the engine compute `new_state` (rather than the store deriving it from `(old_state, details)`) keeps all domain logic in the engine — the store is a plain persistence layer without Simplicity or contract-math knowledge.

**Why two types**: `ProcessedTransaction` (and its inner `InterpretedTransaction`) is the caller-facing view (full data, including ephemeral computed fields like output roles). `StateUpdate` is the storage-facing view (only what needs to be persisted). The engine converts between them internally. This prevents store implementors from accidentally persisting wallet-specific data (output roles, classifications) alongside contract state, while ensuring callers always get the full picture.

### TypedStateUpdate

Generic transition record carrying the caller-facing fields from a state transition (omitting internal outpoint data). Used in two contexts: the store returns `HistoryEntry` (`TypedStateUpdate<TransitionDetails>`) from `transition_history`, and the engine unwraps to typed aliases (`MarketHistoryEntry`, etc.) for caller convenience:

```rust
pub struct TypedStateUpdate<D> {
    pub contract_id: ContractId,
    pub txid: Txid,
    pub position: ChainPosition,
    pub details: D,
}
```

Type aliases for ergonomic use:

```rust
pub type HistoryEntry = TypedStateUpdate<TransitionDetails>;
pub type MarketHistoryEntry = TypedStateUpdate<MarketTransition>;
pub type PoolHistoryEntry = TypedStateUpdate<PoolTransition>;
pub type OrderHistoryEntry = TypedStateUpdate<OrderTransition>;
```

`HistoryEntry` is used by the store's `transition_history` method. The typed aliases are used by the view types' `history()` methods (`Market::history`, `Pool::history`, `Order::history`), which return these typed entries after unwrapping the store's `TransitionDetails` enum.

### TransitionDetails

Nested by contract type. Each variant carries the decoded details of what happened:

```rust
pub enum TransitionDetails {
    Market(MarketTransition),
    Pool(PoolTransition),
    Order(OrderTransition),
}

pub enum MarketTransition {
    Binary(BinaryMarketTransition),
    MultiOutcome(MultiOutcomeMarketTransition),
}

pub enum BinaryMarketTransition {
    Issued { pairs: u64, collateral_locked: u64 },
    Cancelled { pairs_burned: u64, collateral_returned: u64 },
    Resolved { outcome: Side },
    Redeemed { kind: RedemptionKind, side: Side, tokens_burned: u64, payout_sats: u64 },
    Expired,
}

pub enum MultiOutcomeMarketTransition {
    // Classified delta shapes (engine pattern-matches tx deltas against these common
    // shapes for display convenience). All pass through the same generic covenant path.
    IssuedPair { outcome: OutcomeIndex, pairs: u64, collateral_locked: u64 },
    CancelledPair { outcome: OutcomeIndex, pairs_burned: u64, collateral_returned: u64 },
    SplitYes { sets: u64, collateral_locked: u64 },
    MergeYes { sets: u64, collateral_returned: u64 },
    SplitNo { sets: u64, collateral_locked: u64 },
    MergeNo { sets: u64, collateral_returned: u64 },

    /// Cross-outcome swap: 1 YES_i in, 1 NO_j out for each j ≠ i, paying
    /// (N − 2) × collateral_per_pair. Possible as a single transaction under the
    /// generic spend path; the engine pattern-matches this canonical shape.
    CrossOutcomeSwap {
        from_outcome: OutcomeIndex,
        sets: u64,
        collateral_cost: u64,
    },

    /// Arbitrary solvency-preserving delta composition that doesn't match any named
    /// classification above. Raw deltas are preserved for consumers that want
    /// granular detail; helper methods on the transition can classify common
    /// sub-patterns.
    Composite {
        delta_yes: Vec<i64>,        // length outcome_count
        delta_no: Vec<i64>,         // length outcome_count
        delta_collateral: i64,      // signed
    },

    // Resolution / expiry / redemption (unchanged):
    Resolved { outcome: OutcomeIndex },
    Redeemed { kind: RedemptionKind, outcome: OutcomeIndex, side: Side, tokens_burned: u64, payout_sats: u64 },
    Expired,
}

pub enum PoolTransition {
    Swapped { old_s_index: u64, new_s_index: u64, old_reserves: PoolReserves, new_reserves: PoolReserves },
    Adjusted { s_index: u64, old_reserves: PoolReserves, new_reserves: PoolReserves },
    Closed { final_reserves: PoolReserves },
}

pub enum OrderTransition {
    Filled { fill_amount: u64 },  // locked asset units consumed in this fill (delta, not cumulative)
    Cancelled,
}
```

`BinaryMarketTransition::Issued` (and `MultiOutcomeMarketTransition::IssuedPair` / `SplitYes` / `SplitNo`) carry only the user-facing amounts (`pairs`/`sets` and `collateral_locked`) without an `IssuanceKind` discriminant. The engine still knows internally whether it was initial or subsequent issuance (for PSET routing), but this distinction is hidden from callers — it is a covenant implementation detail.

**Each market transaction is exactly one covenant spend path.** The multi-outcome market covenant uses a single generic spend path for all Unresolved-phase transitions (see [`multi-outcome-market-contract.md § Operations`](../contracts/multi-outcome/multi-outcome-market-contract.md#operations)). That one spend path accepts any `(Δy, Δn, Δc)` preserving the solvency invariant — so a single on-chain transaction may represent a pure named primitive (IssuedPair, SplitYes, etc.), a classified cross-outcome swap, or an arbitrary composition of delta shapes.

The engine classifies the tx's delta shape into a `MultiOutcomeMarketTransition` variant. Each named variant corresponds to a canonical delta shape defined by the covenant (see [`multi-outcome-market-contract.md § Operations`](../contracts/multi-outcome/multi-outcome-market-contract.md#operations) for the covenant-level coefficients). The variants' shapes are pairwise disjoint by construction, so matching order is a formality — but the engine uses a consistent order for implementation clarity.

**Canonical shape table** (all entries expressed in Δ-per-outcome for YES/NO token supplies and Δ for collateral; `cp := base_payout × N`, `cp_yes_basket := cp`, `cp_no_basket := (N - 1) × cp`, `cp_cross_swap := (N - 2) × cp`, matching [contract-specification.md § Spend Paths](../contracts/contract-specification.md#spend-paths-summary)):

| Variant | Δy shape | Δn shape | Δc shape |
|---|---|---|---|
| `IssuedPair { outcome: i, pairs: p }` | Δy[i] = +p; all others 0 | Δn[i] = +p; all others 0 | +p × cp |
| `CancelledPair { outcome: i, pairs_burned: p }` | Δy[i] = −p; all others 0 | Δn[i] = −p; all others 0 | −p × cp |
| `SplitYes { sets: s }` | Δy[k] = +s for all k | all Δn = 0 | +s × cp |
| `MergeYes { sets: s }` | Δy[k] = −s for all k | all Δn = 0 | −s × cp |
| `SplitNo { sets: s }` | all Δy = 0 | Δn[k] = +s for all k | +s × ((N - 1) × cp) |
| `MergeNo { sets: s }` | all Δy = 0 | Δn[k] = −s for all k | −s × ((N - 1) × cp) |
| `CrossOutcomeSwap { from_outcome: i, sets: s }` | Δy[i] = −s; all others 0 | Δn[j] = +s for j ≠ i; Δn[i] = 0 | +s × ((N - 2) × cp) |

In the typed Rust surface, `params.cp_yes_basket()`, `params.cp_no_basket()`, and `params.cp_cross_swap()` are just accessors for those exact formulas; no alternate coefficient definitions exist.

**Classification algorithm**:

```rust
fn classify_multi_outcome_transition(
    delta_yes: &[i64],       // length N
    delta_no: &[i64],        // length N
    delta_collateral: i64,
    params: &MultiOutcomeMarketParams,
) -> MultiOutcomeMarketTransition {
    let n = params.outcome_count as usize;
    let yes_nz: Vec<(usize, i64)> = delta_yes.iter().enumerate()
        .filter(|(_, &v)| v != 0).map(|(i, &v)| (i, v)).collect();
    let no_nz: Vec<(usize, i64)> = delta_no.iter().enumerate()
        .filter(|(_, &v)| v != 0).map(|(i, &v)| (i, v)).collect();

    // 1. Pair (issue or cancel) — exactly one YES and one NO nonzero, same index, same value
    if yes_nz.len() == 1 && no_nz.len() == 1 && yes_nz[0] == no_nz[0] {
        let (i, d) = yes_nz[0];
        let expected_dc = d * params.cp() as i64;
        if delta_collateral == expected_dc {
            return if d > 0 { IssuedPair { outcome: i.into(), pairs: d as u64, collateral_locked: expected_dc as u64 } }
                   else     { CancelledPair { outcome: i.into(), pairs_burned: (-d) as u64, collateral_returned: (-expected_dc) as u64 } };
        }
    }

    // 2. SplitYes / MergeYes — all N YES move together by the same amount, no NO change
    if no_nz.is_empty() && yes_nz.len() == n {
        let first = yes_nz[0].1;
        if yes_nz.iter().all(|&(_, v)| v == first) && delta_collateral == first * params.cp_yes_basket() as i64 {
            return if first > 0 { SplitYes { sets: first as u64, collateral_locked: delta_collateral as u64 } }
                   else         { MergeYes { sets: (-first) as u64, collateral_returned: (-delta_collateral) as u64 } };
        }
    }

    // 3. SplitNo / MergeNo — symmetric (all N NO move together, no YES change)
    if yes_nz.is_empty() && no_nz.len() == n {
        let first = no_nz[0].1;
        if no_nz.iter().all(|&(_, v)| v == first) && delta_collateral == first * params.cp_no_basket() as i64 {
            return if first > 0 { SplitNo { sets: first as u64, collateral_locked: delta_collateral as u64 } }
                   else         { MergeNo { sets: (-first) as u64, collateral_returned: (-delta_collateral) as u64 } };
        }
    }

    // 4. CrossOutcomeSwap — Δy[i] = -s (unique); Δn[j] = +s for each j ≠ i
    if yes_nz.len() == 1 && no_nz.len() == n - 1 {
        let (i, dy_i) = yes_nz[0];
        let s = -dy_i;
        if s > 0
            && no_nz.iter().all(|&(j, v)| j != i && v == s)
            && delta_collateral == s * params.cp_cross_swap() as i64 {
            return CrossOutcomeSwap {
                from_outcome: i.into(),
                sets: s as u64,
                collateral_cost: delta_collateral as u64,
            };
        }
    }

    // 5. Composite fallback — shape didn't match any named primitive. Preserves raw deltas.
    Composite {
        delta_yes: delta_yes.to_vec(),
        delta_no: delta_no.to_vec(),
        delta_collateral,
    }
}
```

**Matching precedence** (pairwise disjoint shapes mean order doesn't affect correctness, but the engine pins this order for consistency): Pair → SplitYes/MergeYes → SplitNo/MergeNo → CrossOutcomeSwap → Composite.

**Shape match with wrong coefficient falls through to Composite.** If deltas satisfy a named variant's YES/NO shape but `delta_collateral` doesn't equal the expected covenant-computed amount, the algorithm falls through rather than emitting a misclassified named variant with incorrect numbers. For consensus-valid txs this shouldn't occur (covenant enforces coefficients); Composite is the safe default if a covenant bug ever let a mismatch through.

**Edge cases**:
- All-zero deltas: consensus-valid txs always change something, so this shouldn't occur. If it does, falls through to `Composite { all zeros }` — non-harmful.
- Single-outcome Δy or Δn with zero Δc: doesn't match any shape → Composite.
- Deltas with the shape of a named variant but numerically out-of-bounds (e.g., cancelling more than outstanding): covenant rejects at spend time; this code only sees consensus-valid txs.

**Cross-outcome swap is now a single-transaction operation.** Under the generic spend path, a user (or wallet builder) constructs one transaction with the cross-outcome-swap delta shape and the covenant accepts it atomically. This is a change from an earlier design iteration where cross-outcome swap was necessarily a two-transaction composition (split-NO + pair-cancel); that was tied to the enumerated-primitives covenant design, which has since been replaced with the generic-path design.

**Multi-contract patterns in a single transaction** (e.g., trades that combine a pool swap with maker order fills) are detected at the `InterpretedTransaction` level via helper methods. Each participating contract still emits one primitive transition; the tx-level helpers recognize recurring multi-contract patterns without collapsing the per-contract transitions. See [Multi-Contract Transaction Patterns](#multi-contract-transaction-patterns) below. Cross-outcome arb (market + N pools atomic) is deferred to v2 — see [Future: Cross-Outcome Arb API (v2)](#future-cross-outcome-arb-api-v2).

`PoolTransition::Swapped` corresponds to the pool covenant's **public** path with `old_s_index != new_s_index` — someone traded through the pool, possibly with a paired reserve assist, moving the s-index. `PoolTransition::Adjusted` covers both the admin path and the degenerate public path with `old_s_index == new_s_index`. The public API intentionally does not preserve which authorization path produced an `Adjusted` transition; it just records the reserve change. `PoolTransition::Closed` indicates the pool admin reclaimed all reserve UTXOs via the close script path. See [lmsr-pool-close-path.md](../contracts/lmsr-pool/lmsr-pool-close-path.md).

**Why nested by contract type**: When processing a market transition, the caller wants to match on market-specific variants without wading through pool and order cases. A flat enum mixing all contract types would force exhaustive matching across unrelated variants.

### ExternalOutput

Non-covenant outputs in a transaction. Each output carries shared fields (index, script, role) directly, with asset and value available only when the output is explicit (unblinded):

```rust
pub struct ExternalOutput {
    pub index: u32,
    pub script_pubkey: Script,

    /// Semantic purpose of this output (CollateralReturn, MakerReceive, Burn, etc.).
    ///
    /// **Role identifies *which* output serves a purpose; it does not encode
    /// precise amounts.** When a covenant return and wallet change share a
    /// script (both going to `WalletFunding::return_script`), they are
    /// consolidated into a single output — the `role` reflects the primary
    /// covenant purpose, but `explicit.value` (if present) is the *aggregate*
    /// of all consolidated amounts. For per-role semantic amounts (collateral
    /// released, payout received, tokens burned, etc.), consult
    /// `TransitionDetails` — it is authoritative.
    pub role: OutputRole,

    /// Explicit asset and value when the output is unblinded, else `None`.
    ///
    /// **Consolidation caveat**: `value` may aggregate multiple semantic roles
    /// (most commonly: covenant return + wallet L-BTC change both going to
    /// `return_script`). For per-role semantic amounts, use `TransitionDetails`.
    pub explicit: Option<ExplicitValues>,
}

pub struct ExplicitValues {
    pub asset: AssetId,
    pub value: u64,
}

impl ExternalOutput {
    pub fn is_explicit(&self) -> bool { self.explicit.is_some() }
    pub fn is_confidential(&self) -> bool { self.explicit.is_none() }
}
```

**Why a struct with `Option<ExplicitValues>`**: Asset and value are bundled in `Option<ExplicitValues>` so they're always known together or not at all — the impossible state "asset known but value unknown" is unrepresentable. Shared fields (`index`, `script_pubkey`, `role`) live on the struct directly, accessible without matching on blinding status. `role` is an independent axis from asset/value observability — the engine can sometimes determine an output's purpose from the script alone (e.g., burn outputs at the known unspendable OP_RETURN script) even when asset and value are blinded. An output that core can see but can't classify gets `role: OutputRole::Unknown`.

For confidential outputs (`explicit: None`), the wallet uses its own blinding keys to determine asset and value. Core provides the output index, script pubkey, and role so the wallet can correlate and label.

```rust
pub enum OutputRole {
    IssuedTokens,
    CollateralReturn,
    TradeReceive,
    MakerReceive,
    OrderReturn,
    PoolReturn,
    Burn,
    Fee,
    Unknown,
}
```

`OutputRole` is purely semantic — it labels what the output represents in the transaction, not its asset or value. The asset and value are already available via `ExplicitValues` when the output is explicit, so the role does not duplicate them. No role variant carries asset or value data — the wallet uses `identify_asset` when it needs to distinguish assets (e.g., YES_2 vs NO_5 within a multi-outcome market's issued tokens) within a role.

| Role | Meaning | Appears in |
| ---- | ------- | ---------- |
| `IssuedTokens` | Newly minted outcome tokens (YES or NO, any outcome) | Pair issuance, split-YES, split-NO |
| `CollateralReturn` | Collateral released from covenant to user | Redemption, pair cancellation, merge-YES, merge-NO |
| `TradeReceive` | Tokens or L-BTC received by the taker | Trade, fill order |
| `MakerReceive` | Payment sent to the maker | Fill order, trade |
| `OrderReturn` | Order's locked asset returned to maker | Cancel order |
| `PoolReturn` | Pool reserves returned to operator | Pool closure |
| `Burn` | Tokens or RTs destroyed (unspendable OP_RETURN script) | Pair cancellation, merge-YES, merge-NO, resolution, expiry |
| `Fee` | Transaction fee | All |
| `Unknown` | Core can see asset/value but can't classify | Any (wallet labels via key ownership) |

**Why `IssuedTokens` is semantic rather than outcome-indexed**: the asset ID already carries outcome identity (YES_3 has a different asset ID from YES_0 or NO_3). A wallet that needs to know "which outcome was issued" calls `identify_asset(asset_id)` and inspects the returned `AssetInfo::OutcomeToken { outcome, side, ... }`. The role is about transaction-level purpose, not asset-level identity.

**Burn outputs** use bare OP_RETURN (`0x6a`) — an unspendable script by consensus rule. The engine recognizes the burn script (a known constant) and assigns `OutputRole::Burn` regardless of whether the output is explicit or confidential. For explicit burns (YES/NO tokens during cancellation), the engine provides full `ExplicitValues`. For blinded burns (RT destruction during resolution/expiry), the output is confidential (`explicit: None`) but the engine still assigns `Burn` from the script match. See [enforcement-layers.md](enforcement-layers.md) for the rationale behind OP_RETURN over P2WSH for burns.

**Output consolidation**: PSET builders consolidate outputs that share the same script and asset into a single output for efficiency and privacy (see [Output Consolidation](#output-consolidation)). A `CollateralReturn` output may therefore include wallet L-BTC change when both land at `WalletFunding::return_script`. **This means `ExternalOutput.explicit.value` may aggregate multiple semantic roles.** When exact amounts matter — accounting, per-outcome breakdowns, cost-basis tracking — use `TransitionDetails`; it is authoritative for per-role semantic amounts (payout, tokens burned, collateral locked, etc.) regardless of consolidation or blinding. `OutputRole` identifies *which* output serves a purpose; `TransitionDetails` provides *the precise numbers*. `Fee` outputs on Elements are structurally separate (explicit fee outputs with no script) and are never consolidated with any other role.

### AssetInfo

Result of asset identification. Unified across binary and multi-outcome markets: `OutcomeToken` and `ReissuanceToken` each carry `outcome: OutcomeIndex` (which is `BINARY` for binary markets) and `side: Side` (YES or NO). Callers who care about the market kind can destructure the embedded `params` enum.

```rust
pub enum AssetInfo {
    /// An outcome token (YES_k or NO_k). For binary markets, `outcome` is always `OutcomeIndex::BINARY`.
    OutcomeToken {
        market_id: ContractId,
        outcome: OutcomeIndex,
        side: Side,
        params: MarketParams,     // umbrella: Binary(BinaryMarketParams) or MultiOutcome(MultiOutcomeMarketParams)
    },
    /// A reissuance token associated with one specific outcome token.
    ReissuanceToken {
        market_id: ContractId,
        outcome: OutcomeIndex,
        side: Side,
    },
    /// The collateral asset used by the market (e.g., L-BTC, USDt). Shared across all outcomes.
    Collateral {
        market_id: ContractId,
        asset_id: AssetId,
    },
}
```

Under the hood, the engine maintains an `asset_id → (contract_id, token_role)` index where `token_role ∈ { OutcomeToken(outcome, side), ReissuanceToken(outcome, side), Collateral }`. Lookups are O(1). For multi-outcome markets, the index holds 4N + 1 entries per market (2N token assets + 2N reissuance-token assets + 1 collateral asset); for binary markets, it holds 5 entries. The index supports both `identify_asset` (asset_id → AssetInfo) and internal reverse lookup during transaction interpretation (which tracked contract owns this asset?).

### CoreError

The error type for all engine operations. Generic over the store's error type, which piggybacks on the engine's existing `S: ContractStore` generic — no additional type parameter burden for consumers.

```rust
pub struct Shortfall {
    pub asset_id: AssetId,
    pub available: u64,
    pub required: u64,
}

pub enum CoreError<E: std::error::Error> {
    Store(E),
    ChainSource(Box<dyn std::error::Error + Send + Sync>),
    InvalidCreationTx { reason: String },
    InvalidParams { detail: String },
    ConventionViolation { detail: String },
    ParentMarketNotTracked { detail: String },
    OracleSignatureInvalid,
    InvalidContractState { contract_id: ContractId, kind: InvalidStateKind },
    ContractNotFound { contract_id: ContractId },
    ContractAlreadyTracked { contract_id: ContractId },
    InsufficientFunds { shortfalls: Vec<Shortfall> },
    NoLiquidity {
        market_id: ContractId,
        outcome: OutcomeIndex,
        side: Side,
        direction: TradeDirection,
    },
    StaleQuote { reason: StaleQuoteReason },
    CovenantInvariantViolation {
        contract_id: ContractId,
        kind: InvariantViolationKind,
    },
}

/// Structured reason a builder rejected a contract's state for the
/// requested operation. Distinguishes "wrong variant" from "right variant
/// but unmet condition." See [State Machine Summary](#state-machine-summary)
/// for the full valid-transition matrix.
pub enum InvalidStateKind {
    /// The contract's state variant is incompatible with the requested
    /// operation (e.g., calling `build_issuance_pset` on a Resolved market).
    WrongVariant {
        expected: &'static [&'static str],
        actual: &'static str,
    },
    /// The state variant is compatible but a runtime precondition failed
    /// (e.g., `build_merge_yes_pset` with insufficient basket supply;
    /// `build_expire_transition_pset` called before the timelock height).
    ConditionFailed {
        condition: &'static str,
        detail: String,
    },
}

/// Why `build_trade_pset` rejected a quote as stale. Callers should
/// re-quote and re-confirm with the user before retrying (prices may
/// have moved).
pub enum StaleQuoteReason {
    /// A referenced contract advanced (pool swap, pool admin adjust,
    /// order fill, order cancel) between quote and build, producing
    /// new outpoints the snapshot doesn't match.
    OutpointsChanged { contract_id: ContractId },
    /// A referenced contract was untracked between quote and build.
    ContractUntracked { contract_id: ContractId },
    /// A referenced contract was removed by `rollback_to_height`
    /// between quote and build (e.g., its creation tx was reorged out).
    ContractRemoved { contract_id: ContractId },
}

/// A consensus-valid transaction violated a covenant-enforced invariant
/// that should have made the transaction impossible. Indicates either a
/// covenant bug, chain-data corruption, or a version mismatch between the
/// running `deadcat-core` and the on-chain covenant. Should not occur in
/// correct operation; callers should treat this as a signal to investigate
/// (and re-ingest from scratch if necessary) rather than retry.
///
/// Kept as an explicit variant even post-covenant-proof as defense-in-depth
/// against bugs outside the proof's scope (interpretation layer, chain
/// backend, version mismatch). Can be removed once an engine-level proof
/// covers all pathways.
pub enum InvariantViolationKind {
    /// A pool transition was observed but the expected covenant-enforced
    /// output window (3 consecutive outputs with `[YES, NO, Collateral]`
    /// asset IDs sharing a script pubkey) was absent or malformed.
    PoolWindowMalformed { detail: String },
    /// A binary or multi-outcome market transition produced outputs that
    /// don't match any recognized covenant phase layout.
    MarketOutputLayoutInvalid { detail: String },
    /// A maker order transition produced outputs that don't match the
    /// covenant's expected fill or cancellation shape.
    OrderOutputLayoutInvalid { detail: String },
}
```

`ChainSource` wraps errors from the `ChainSource` trait implementation during `step`. The chain error type is boxed rather than generic to keep the engine at a single generic parameter (`S`) — `step` introduces `C: ChainSource` only at the call site. The `ChainSource::Error` bound includes `Send + Sync + 'static` to enable boxing into `Box<dyn Error + Send + Sync>`. Integrators can display/debug the error or downcast if they need the concrete type. `InvalidParams` covers structural caller mistakes at the API boundary (e.g., invalid outcome index for the target market kind, calling `oracle_attestation_spec` with a `MarketResolution` variant that does not match the market kind). `ConventionViolation` covers the strict-canonical policy boundary — parameters or externally-supplied contract data that fall outside the canonical v1 recovery conventions even if the underlying covenant could be consensus-valid. `ParentMarketNotTracked` is returned by pool/order ingestion when the referenced YES/NO assets do not resolve to any tracked market. `OracleSignatureInvalid` is returned by resolve-building paths when a raw oracle signature does not verify against any valid resolution for the target market. `InvalidContractState` is returned by PSET builders when the contract is in the wrong state for the requested operation; `InvalidStateKind` distinguishes `WrongVariant` (state machine rejection) from `ConditionFailed` (runtime precondition unmet, e.g., insufficient basket supply). `InsufficientFunds` is returned by PSET builders when the caller's available UTXOs don't cover the required amounts — the `shortfalls` vec reports all insufficient assets at once (e.g., "need 50 more YES tokens AND 3,000 more sats"), enabling wallet UX that shows all missing resources rather than one at a time. `NoLiquidity` is returned by `quote_trade` only when the router can't fill any positive amount (all pools exhausted, all orders dust, or no tracked sources); it carries the trade's target tuple so UIs can show "No liquidity to BUY YES on outcome 2 of market X." Partial fills do not return `NoLiquidity` — any `filled_amount > 0` returns `Ok(TradeQuote)` and the caller decides. `StaleQuote` is returned by `build_trade_pset` when the quote's snapshot is no longer current; `StaleQuoteReason` identifies the specific cause for both UI messaging and diagnostics. `CovenantInvariantViolation` indicates a should-be-impossible consensus-valid transaction violated a covenant-enforced property; this is bug-adjacent and callers should not loop-retry. Internal construction errors (e.g., Pedersen commitment math failure) indicate bugs in core and panic rather than returning an error — every `CoreError` variant represents a condition the caller can meaningfully respond to.

**Why generic over the store error**: The engine is already generic over `S: ContractStore`, so `CoreError<S::Error>` adds no new generic parameters. Store error types are preserved — consumers can match on `CoreError::Store(e)` and handle their specific store error without downcasting. Store implementors define their own error type independently via an associated type on the trait.

### UnblindedPset and PreparedPset

See [Confidential Transaction Blinding](#confidential-transaction-blinding) for the full design and rationale. Summarized here for type reference:

```rust
/// Returned by the 5 market builders that involve reissuance token outputs.
/// Private fields prevent extracting the PSET without going through a blinding method.
pub struct UnblindedPset { /* private: pset, rt_blinding_factors, input_secrets, output_classification */ }

impl UnblindedPset {
    pub fn prepare(self, wallet_blinding_pubkey: &PublicKey) -> Result<PreparedPset, BlindingError>;
    pub fn finalize(self) -> Result<PartiallySignedTransaction, BlindingError>;
}

/// Returned by UnblindedPset::prepare(). The caller must call
/// pset.blind_last(rng, secp, &input_secrets) before signing.
pub struct PreparedPset {
    pub pset: PartiallySignedTransaction,
    pub input_secrets: HashMap<usize, TxOutSecrets>,
}
```

`BlindingError` is a simple error type for cryptographic failures during blinding (proof generation, commitment construction). It is separate from `CoreError` — blinding is a post-builder step decoupled from the engine's store generic. The builder returns `Result<UnblindedPset, CoreError<S::Error>>`; the blinding methods return `Result<_, BlindingError>`.

## View Types

`ContractEngine` exposes per-contract operations via **view types**: lightweight handles returned by `engine.market(id)`, `engine.pool(id)`, `engine.order(id)`, and `Market::as_multi_outcome()`. Each view caches the contract's `(params, state)` at construction (a single store read per view creation) and bundles per-contract operations as methods on the view.

**Why view types**: operations on a market, pool, or order naturally cluster by the object they operate on. Putting them on the engine would mean ~25 methods at the top level, mostly varying by which `contract_id` they take. View types group them and improve discoverability — IDE autocomplete on a `Market` shows exactly what you can do with a market, without wading through unrelated pool/order methods. The view-type specialization (`Market::as_multi_outcome()`) also provides type-level dispatch: operations that exist only for multi-outcome markets live on `MultiOutcomeMarket` and are unreachable from binary markets without going through `as_multi_outcome()` (which returns `None` for binary).

**Borrow semantics and staleness**: a view type holds `&'a ContractEngine<S>` (immutable borrow). While any view is alive, the engine cannot be `&mut self`-borrowed — so `step`, `rollback_to_height`, `prune_finalized`, and any `ingest_*` call are blocked by the borrow checker until all views are dropped. Because contract params are immutable over the contract's lifetime and state transitions only happen through `&mut self` engine methods, the cached `(params, state)` within a view are **provably fresh** for the view's entire lifetime. No runtime staleness checks needed.

**View lifetimes**: the `'a` lifetime ties the view to the engine's borrow. Views aren't meant to be long-lived — typical usage is `engine.market(&id)?.build_issuance_pset(...)` in one expression. Views CAN be held across multiple method calls (for composability) as long as nothing tries to mutate the engine during that time.

### Market

The unified view for both binary and multi-outcome markets. Common accessors work uniformly; type-specific behavior goes through `as_multi_outcome()` or direct matching on `params()` / `state()`.

```rust
pub struct Market<'a, S: ContractStore> {
    // Private. Holds engine reference + contract_id + cached (params, state).
}

impl<'a, S: ContractStore> Market<'a, S> {
    // ---- Identity and state accessors ----
    pub fn contract_id(&self) -> &ContractId;
    pub fn params(&self) -> &MarketParams;
    pub fn state(&self) -> &MarketState;

    // ---- Common property accessors (work uniformly for binary + multi-outcome) ----
    pub fn outcome_count(&self) -> u8;                    // 1 for binary, N for multi-outcome
    pub fn oracle_public_key(&self) -> XOnlyPublicKey;
    pub fn collateral_asset_id(&self) -> AssetId;
    pub fn collateral_per_pair(&self) -> u64;
    pub fn expiry_time(&self) -> u32;

    // ---- Phase predicates ----
    pub fn is_active(&self) -> bool;                       // Trading variant (either kind)
    pub fn is_resolved(&self) -> bool;
    pub fn is_expired(&self) -> bool;
    pub fn is_terminal(&self) -> bool;                     // Resolved/Expired with zero unredeemed
    pub fn resolution(&self) -> Option<MarketResolution>;  // Some if resolved, None otherwise

    // ---- Unified PSET builders (work for both market kinds) ----
    // All builders take `outcome: OutcomeIndex` to identify which outcome's YES/NO pair
    // is being operated on. For binary markets, `outcome` must be `OutcomeIndex::BINARY`;
    // other values return CoreError::InvalidParams.

    /// Mint `pairs` new YES + NO tokens for the given outcome. Locks `pairs × collateral_per_pair`.
    pub fn build_issuance_pset(
        &self,
        outcome: OutcomeIndex,
        pairs: u64,
        yes_dest: &Script,
        no_dest: &Script,
        funding: &WalletFunding,
    ) -> Result<UnblindedPset, CoreError<S::Error>>;

    /// Burn `pairs_to_burn` YES + NO pairs for the given outcome; releases collateral.
    /// `pairs_to_burn: None` means "burn all outstanding pairs for this outcome" (full cancellation).
    pub fn build_cancellation_pset(
        &self,
        outcome: OutcomeIndex,
        pairs_to_burn: Option<u64>,
        funding: &WalletFunding,
    ) -> Result<UnblindedPset, CoreError<S::Error>>;

    /// Resolve the market via an oracle attestation. For binary: single signature
    /// over (market_id || outcome_byte) where outcome_byte ∈ {0x00, 0x01}. For multi-outcome:
    /// signature over (market_id || outcome_index). The engine identifies which outcome
    /// was attested to by trial verification.
    pub fn build_oracle_resolve_pset(
        &self,
        attestation: &schnorr::Signature,
        funding: &WalletFunding,
    ) -> Result<UnblindedPset, CoreError<S::Error>>;

    /// Redeem winning tokens (post-resolution) or any tokens (post-expiry) for collateral.
    /// For binary post-resolution: `side` must match the winning side; for expired markets,
    /// either side is valid. For multi-outcome: `outcome` identifies which pair; the winning
    /// YES_k and any NO_j (j ≠ k) are redeemable at full value post-resolution; all tokens
    /// at the fractional rate post-expiry.
    pub fn build_redemption_pset(
        &self,
        outcome: OutcomeIndex,
        side: Side,
        tokens_to_redeem: u64,
        funding: &WalletFunding,
    ) -> Result<PartiallySignedTransaction, CoreError<S::Error>>;

    /// Move the market from Unresolved/Dormant to Expired once `nLockTime >= expiry_time`.
    /// Does not require any tokens; any party can invoke.
    pub fn build_expire_transition_pset(
        &self,
        funding: &WalletFunding,
    ) -> Result<UnblindedPset, CoreError<S::Error>>;

    // ---- Oracle helpers ----

    /// Returns the message, oracle pubkey, and related data for a given resolution.
    /// Useful for oracle services: compute the message, sign it off-band, pass the
    /// signature back to `build_oracle_resolve_pset`.
    pub fn oracle_attestation_spec(
        &self,
        resolution: MarketResolution,
    ) -> Result<OracleAttestationSpec, CoreError<S::Error>>;

    /// Verify an oracle signature against a specific resolution without broadcasting.
    /// Useful for oracles to dry-run their signatures before publishing.
    pub fn verify_oracle_attestation(
        &self,
        resolution: MarketResolution,
        signature: &schnorr::Signature,
    ) -> Result<bool, CoreError<S::Error>>;

    // ---- Probability / implied-price accessors ----

    /// Liquidity-weighted probability that `outcome` wins, in basis points (0..=10000).
    /// Weighting is by each pool's `b` parameter (LMSR depth). Returns None if no pool
    /// exists for that outcome.
    ///
    /// Within any single LMSR pool, p_YES + p_NO = 10000 bps by construction (softmax),
    /// so per-outcome probability is a single value — the YES side's price in bps; the
    /// NO side's price derives as `10000 - probability_bps`.
    pub fn probability_bps(&self, outcome: OutcomeIndex)
        -> Result<Option<u16>, CoreError<S::Error>>;

    /// Implied token cost in collateral sats at the current probability. For real trade
    /// prices including fees and slippage, use `engine.quote_trade`. Derived as:
    ///   Side::Yes → probability_bps × collateral_per_pair / 10000
    ///   Side::No  → (10000 - probability_bps) × collateral_per_pair / 10000
    pub fn implied_token_cost_sats(
        &self,
        outcome: OutcomeIndex,
        side: Side,
    ) -> Result<Option<u64>, CoreError<S::Error>>;

    // ---- Related contracts (replaces engine.pools_for_market / engine.orders_for_market) ----

    /// All pools associated with this market across all outcomes.
    ///
    /// For binary markets (N=1), this is a single outcome-scoped store call under the hood.
    /// For multi-outcome markets, the view iterates over `0..outcome_count` store calls
    /// (one per outcome) and merges results in outcome-index order. Pagination is
    /// supported via an opaque cursor that encodes `(outcome_index, inner_cursor)` —
    /// callers don't need to manage the iteration themselves.
    ///
    /// Use `pools_for_outcome` when scoping to a single outcome (routing, per-outcome
    /// display). Use `pools` for the "all pools for this market" display case.
    pub fn pools(&self, filter: StateFilter, page: Pagination)
        -> Result<Page<PoolEntry>, CoreError<S::Error>>;

    /// All orders associated with this market across all outcomes. Same iteration
    /// semantics as `pools` for multi-outcome markets.
    pub fn orders(&self, filter: StateFilter, page: Pagination)
        -> Result<Page<OrderEntry>, CoreError<S::Error>>;

    /// Pools for a specific outcome. Direct delegate to `store.pools_for_market` (single
    /// indexed store call, no iteration). For binary markets, pass `OutcomeIndex::BINARY`;
    /// any other index returns an empty page.
    pub fn pools_for_outcome(&self, outcome: OutcomeIndex, filter: StateFilter, page: Pagination)
        -> Result<Page<PoolEntry>, CoreError<S::Error>>;

    /// Orders for a specific outcome (both sides). Direct delegate to
    /// `store.orders_for_market` (single indexed store call, no iteration). Side/direction
    /// filtering can be applied by the caller on the returned data, or use
    /// `engine.quote_trade` for routing-focused access to best orders.
    pub fn orders_for_outcome(&self, outcome: OutcomeIndex, filter: StateFilter, page: Pagination)
        -> Result<Page<OrderEntry>, CoreError<S::Error>>;

    // ---- Type-level specialization ----
    /// Returns a `MultiOutcomeMarket` view for multi-outcome-specific operations
    /// (cross-outcome splits/merges). Returns `None` for binary markets.
    /// Cross-outcome arb builders are deferred to v2.
    pub fn as_multi_outcome(&self) -> Option<MultiOutcomeMarket<'a, S>>;
}

impl<'a, S: ContractHistory> Market<'a, S> {
    pub fn history(&self, after: Option<ChainPosition>, limit: u32)
        -> Result<Vec<MarketHistoryEntry>, CoreError<S::Error>>;
}
```

### MultiOutcomeMarket

Specialization of `Market` for multi-outcome markets. Obtained via `Market::as_multi_outcome()` — returns `Some` for multi-outcome markets, `None` for binary. Exposes cross-outcome primitives that don't exist in the binary market (since binary has only one outcome).

```rust
pub struct MultiOutcomeMarket<'a, S: ContractStore> {
    // Private. Holds engine reference + contract_id + cached typed (params, state).
    // `params: &MultiOutcomeMarketParams`, `state: &MultiOutcomeMarketState` — not the umbrella.
}

impl<'a, S: ContractStore> MultiOutcomeMarket<'a, S> {
    // ---- Identity and state accessors (typed to multi-outcome) ----
    pub fn contract_id(&self) -> &ContractId;
    pub fn params(&self) -> &MultiOutcomeMarketParams;
    pub fn state(&self) -> &MultiOutcomeMarketState;

    // ---- Cross-outcome primitives (multi-outcome only) ----

    /// Mint a complete YES set (1 of each outcome's YES) for `sets × collateral_per_pair` collateral.
    /// `destinations` has length `outcome_count`; destinations[k] receives `sets` of YES_k tokens.
    pub fn build_split_yes_pset(
        &self,
        sets: u64,
        destinations: &[Script],
        funding: &WalletFunding,
    ) -> Result<UnblindedPset, CoreError<S::Error>>;

    /// Burn a complete YES set for `sets × collateral_per_pair` collateral released.
    pub fn build_merge_yes_pset(
        &self,
        sets: u64,
        funding: &WalletFunding,
    ) -> Result<UnblindedPset, CoreError<S::Error>>;

    /// Mint a complete NO set (1 of each outcome's NO) for `sets × (N-1) × collateral_per_pair` collateral.
    pub fn build_split_no_pset(
        &self,
        sets: u64,
        destinations: &[Script],
        funding: &WalletFunding,
    ) -> Result<UnblindedPset, CoreError<S::Error>>;

    /// Burn a complete NO set for `sets × (N-1) × collateral_per_pair` collateral released.
    pub fn build_merge_no_pset(
        &self,
        sets: u64,
        funding: &WalletFunding,
    ) -> Result<UnblindedPset, CoreError<S::Error>>;

    // ---- Probability aggregates ----

    /// Length `outcome_count`. Entry k is `Some(probability_bps)` if any pool exists for
    /// outcome k, else `None`. Each entry is a liquidity-weighted average across all pools
    /// for that outcome (same as `Market::probability_bps`).
    pub fn probabilities_bps(&self) -> Result<Vec<Option<u16>>, CoreError<S::Error>>;

    /// Sum of per-outcome probabilities across all outcomes with at least one pool.
    /// Equals ~10000 under perfect cross-outcome arb; deviation indicates an arb
    /// opportunity. Outcomes with no pool are omitted from the sum.
    pub fn sum_of_probabilities_bps(&self) -> Result<u64, CoreError<S::Error>>;

    // Cross-outcome arb (quote/build API) is deferred to v2. The covenant's generic
    // solvency-preservation path already makes arb permissionless, so external bots
    // can close coherence gaps without a built-in builder. See the "Future:
    // Cross-Outcome Arb API" section below for the deferred surface and open
    // design questions.

    // ---- Conversion back ----
    /// Returns the general `Market` view, for operations that apply to both market kinds.
    pub fn as_market(&self) -> Market<'a, S>;
}
```

### Future: Cross-Outcome Arb API (v2)

Cross-outcome arb closes coherence gaps among a multi-outcome market's N pools: when `Σ p_YES_k ≠ 1` (or symmetrically `Σ p_NO_k ≠ N−1`), an arbitrageur can profit by co-spending a market split/merge primitive with N pool swaps in one atomic transaction. Four directions exist — split-YES + sell-yes-to-pools, buy-yes-from-pools + merge-YES, and the NO analogues.

**Scope decision**: deferred to v2. Rationale:

- **Not safety-critical.** Coherence gaps are pricing drift, not solvency violations — the covenant's invariants hold regardless of whether arb is run. Markets stay solvent; users can still trade.
- **Permissionless by construction.** The multi-outcome market's generic solvency-preservation spend path ([see multi-outcome-market-contract.md § Operations](../contracts/multi-outcome/multi-outcome-market-contract.md#operations)) admits cross-outcome arb as one of its delta shapes. External arb bots can construct and broadcast these txs directly against the covenant spec without a `deadcat-core`-provided builder.
- **Advanced-actor-facing.** Arbitrageurs and keepers, not retail users. That audience tolerates external tooling while v1 ships.

**v1 core builders do not compose into arb PSETs.** The market's single-contract builders (`build_split_yes_pset`, `build_merge_yes_pset`, etc.) and the engine-managed trade builder (`build_trade_pset`) each construct complete transactions for their own scope — they cannot be merged into one atomic multi-contract arb transaction. Arb requires one bespoke PSET co-spending the market's generic spend path with N pool public-path spends simultaneously; external tooling must construct that transaction directly against the covenant spec in v1.

**What external arb tooling can leverage from `deadcat-core` v1**:

| Available | Not available (must re-implement externally) |
|---|---|
| `engine.market(id).as_multi_outcome()` state inspection (supplies, asset IDs, params) | PSET construction for multi-contract atomic spends |
| `engine.pool(id)` state (s_index, reserves, params) | Combined fee calculation across market + N pool inputs |
| `LmsrPoolParams` + `MultiOutcomeMarketParams` full params for script derivation | Witness-stack layout for the generic solvency-preservation spend path |
| LMSR F-value runtime (`pub` in v1) for quoting and Merkle proof generation | Cross-contract tx atomicity management |
| `interpret_transaction` to verify a constructed arb tx before broadcast | Arb opportunity detection (coherence gap scanning) |

The LMSR F-value runtime is specifically exposed as `pub` in v1 (not `pub(crate)`) so external tooling can compute identical Merkle proofs to what the covenant verifies, avoiding reimplementation of the bignum algorithm specified in [lmsr-deterministic-table-spec.md](../contracts/lmsr-pool/lmsr-deterministic-table-spec.md). Cross-implementation conformance remains anchored to the committed Merkle roots — tooling that reproduces them is provably equivalent.

**Deferred API surface** (names reserved; signatures to be finalized before v2):

- `MultiOutcomeMarket::quote_cross_outcome_arb(...) -> Result<Option<ArbQuote<'a>>, _>` — quote the best available arb direction.
- `MultiOutcomeMarket::build_cross_outcome_arb_pset(quote, funding, fee_rate) -> Result<UnblindedPset, _>` — build the atomic PSET.
- `InterpretedTransaction::as_cross_outcome_arb() -> Option<&CrossOutcomeArb>` — classify an observed arb tx.
- Types: `ArbQuote`, `ArbDirection`, `ArbPoolLeg`, `CrossOutcomeArb` (observed-tx form).

**Open design questions for v2** (these do not need to be settled for v1):

1. Scope of directions in v2 — just the four split/merge directions, or also cross-outcome swap as an arb primitive?
2. Quote API shape — caller specifies `ArbDirection` explicitly, or engine auto-detects the most profitable?
3. Sizing model — engine picks max-profit sets vs caller-specified `sets` vs break-even sets?
4. `ArbQuote` fields — staleness via lifetime binding to `MultiOutcomeMarket` reference? Pool state snapshot granularity?
5. Classification rule — what exact delta-shape + pool-swap pattern counts as "arb" vs falling through to generic `Composite`?

**v1 behavior on observed arb-shaped txs**: an arb tx broadcast by an external bot will ingest normally. Its per-contract transitions (market `SplitYes` / `MergeYes` / etc. + N pool `Swapped`) remain available in `InterpretedTransaction.transitions`. The aggregate multi-contract classification is deferred — such txs fall through to the generic classification until v2 lands the `as_cross_outcome_arb` helper.

### Pool

Per-pool view. All pools in deadcat are binary LMSR pools (see [amm-scoring-rule-tradeoffs.md](../contracts/multi-outcome/amm-scoring-rule-tradeoffs.md)); `Pool` is the single view type. For multi-outcome markets, the parent market has N pools under Option C composition — one per outcome's YES/NO pair; each is a standalone `Pool`.

```rust
pub struct Pool<'a, S: ContractStore> {
    // Private. Holds engine reference + contract_id + cached (params, state).
}

impl<'a, S: ContractStore> Pool<'a, S> {
    pub fn contract_id(&self) -> &ContractId;
    pub fn params(&self) -> &LmsrPoolParams;
    pub fn state(&self) -> &LmsrPoolState;

    pub fn is_active(&self) -> bool;
    pub fn is_closed(&self) -> bool;

    /// Adjust the pool's reserves (admin operation — requires admin signature).
    /// `pair_delta` and `collateral_delta` are signed: positive = injection, negative = withdrawal.
    /// The covenant enforces that YES and NO deltas are equal (pair_delta applies to both).
    pub fn build_adjust_pset(
        &self,
        pair_delta: i64,
        collateral_delta: i64,
        funding: &WalletFunding,
    ) -> Result<PartiallySignedTransaction, CoreError<S::Error>>;

    /// Close the pool (admin operation — requires admin signature). Consumes all 3 reserve UTXOs
    /// atomically, releasing all assets to the operator.
    pub fn build_close_pset(
        &self,
        funding: &WalletFunding,
    ) -> Result<PartiallySignedTransaction, CoreError<S::Error>>;

    /// Returns the parent market's view, if tracked. A pool's params reference specific asset
    /// IDs (yes_asset_id, no_asset_id, collateral_asset_id) that belong to some market; this
    /// navigates back to that market.
    pub fn parent_market(&self) -> Result<Option<Market<'a, S>>, CoreError<S::Error>>;
}

impl<'a, S: ContractHistory> Pool<'a, S> {
    pub fn history(&self, after: Option<ChainPosition>, limit: u32)
        -> Result<Vec<PoolHistoryEntry>, CoreError<S::Error>>;
}
```

### Order

Per-maker-order view. Only the maker-lifecycle operation (cancellation) lives on this view; filling orders is a taker operation routed through `engine.quote_trade` + `engine.build_trade_pset`.

```rust
pub struct Order<'a, S: ContractStore> {
    // Private. Holds engine reference + contract_id + cached (params, state).
}

impl<'a, S: ContractStore> Order<'a, S> {
    pub fn contract_id(&self) -> &ContractId;
    pub fn params(&self) -> &MakerOrderParams;
    pub fn state(&self) -> &OrderState;

    pub fn is_active(&self) -> bool;
    pub fn is_consumed(&self) -> bool;
    pub fn is_cancelled(&self) -> bool;
    pub fn remaining_liquidity(&self) -> u64;   // offered_amount - total_filled; 0 if terminal

    /// Cancel the order (maker operation — requires maker signature via taproot key-spend).
    pub fn build_cancel_pset(
        &self,
        funding: &WalletFunding,
    ) -> Result<PartiallySignedTransaction, CoreError<S::Error>>;

    /// Returns the parent market's view, if tracked.
    pub fn parent_market(&self) -> Result<Option<Market<'a, S>>, CoreError<S::Error>>;
}

impl<'a, S: ContractHistory> Order<'a, S> {
    pub fn history(&self, after: Option<ChainPosition>, limit: u32)
        -> Result<Vec<OrderHistoryEntry>, CoreError<S::Error>>;
}
```

### Builder naming convention on views

Within each view type, builder methods drop the contract-type prefix that was previously needed to disambiguate when all builders lived on the engine:
- Engine: `build_lmsr_adjust_pset` → Pool view: `build_adjust_pset`
- Engine: `build_lmsr_close_pset` → Pool view: `build_close_pset`
- Engine: `build_cancel_order_pset` → Order view: `build_cancel_pset`

Market and multi-outcome market builder names are unchanged (`build_issuance_pset`, `build_split_yes_pset`, etc.) because the "market" aspect isn't in the name — the action is. They're already scoped by which view they live on.

Creation builders stay on the engine (no view exists yet pre-creation) and retain their descriptive names:
- `engine.build_binary_market_creation_pset`
- `engine.build_multi_outcome_market_creation_pset`
- `engine.build_lmsr_bootstrap_pset` (kept — "bootstrap" captures LMSR's initial-price-setting semantics)
- `engine.build_create_order_pset`

## Core Design: UTXO-Following State Machine

### Fundamental Loop

The core of `deadcat-core` is a state machine that tracks covenant UTXOs:

1. **Ingest**: Register a contract via the per-type ingestion method (core derives initial outpoints or accepts a snapshot)
2. **Spend**: When a transaction spends any tracked UTXO, process it
3. **Advance**: Determine the contract's new state from the transaction's outputs and witness data
4. **Repeat**: The new UTXOs become the tracked set

This is a UTXO-following model, not a transaction classifier. The distinction is important:

- **Transaction classifier** (rejected): "What kind of transaction is this?" — fragile, must recognize every possible tx shape, breaks on unknown combinations
- **UTXO-following state machine** (chosen): "My contract was at UTXO X. X got spent. Where is my contract now?" — robust, works for any valid covenant transition

### Why UTXO-Following Over Classification

The Simplicity covenant enforces that outputs follow a known pattern. If input UTXO X was a prediction market's UnresolvedCollateral slot, the covenant guarantees the spending transaction's outputs contain the next valid state. Core doesn't need to classify the transaction — it just needs to find its contract's new outputs.

This also handles unknown transaction combinations gracefully. If a future version combines an issuance with a pool adjustment in one transaction (something no current PSET builder produces), the UTXO-following model still works — each contract independently follows its own outpoints through the transaction. The classifier model would fail because it wouldn't recognize the combined shape.

### Why Input-Based, Not Output-Based

The UTXO-following model detects transitions by finding tracked outpoints in a transaction's **inputs** (spends), not by scanning **outputs** for matching scripts. Input-based tracking is more precise:

- **Input-based**: "My tracked UTXO X was spent in this tx. My contract transitioned." Precise and unambiguous — an outpoint is either in the inputs or not.
- **Output-based**: "This tx has an output matching a script I track. Something happened." Requires scanning all scripts x all states x all contracts. Can produce false positives if someone sends coins to a covenant address without going through the covenant spend path.

Contract creation is the one case with no prior outpoints, which is why the per-type ingestion methods have their own output-scanning logic (see [Contract Ingestion](#contract-ingestion)). All subsequent state transitions use input-based tracking via `process_transaction`.

### Processing a Transaction

When `process_transaction` is called (internally by `step`):

1. Collect all input outpoints from the transaction
2. Check which tracked contracts own any of those outpoints (via `ContractMatch`)
3. For each affected contract, identify new outputs using a per-type strategy:
   - **Markets/orders**: match outputs against expected covenant scripts from the store's persisted script index
   - **Pools**: identify the contiguous 3-slot reserve output window by asset ID, derive the new s_index from explicit reserve values via the LMSR table (see [LMSR Pools](#lmsr-pools) below)
4. Derive transition details from the current state, new state, and output values
5. Compute external output roles for non-covenant outputs
6. Durably persist the state updates
7. Return the full `ProcessedTransaction`

**Important**: A single transaction can affect multiple contracts. For example, a routed trade can spend LMSR pool reserves AND fill a limit order. Each contract advances independently based on its own outpoints — they don't need to know about each other.

### Output Matching via Script Pubkeys

Core identifies which outputs belong to which contract by matching `script_pubkey` values, not by assuming fixed output indices. This is robust across all transaction shapes:

- **Explicit covenant outputs** (collateral, reserves): script pubkey is the covenant address derived from contract params + state. Asset and value are readable.
- **Reissuance token outputs**: `Asset::Null` and `Value::Null` on-chain, but `script_pubkey` is set to the covenant address for the target slot. Core matches by script pubkey alone.
- **Confidential wallet outputs** (change, payouts): script pubkey is readable but asset/value are confidential. Core identifies these as "not covenant" and reports them as `ExternalOutput` with `explicit: None`, the output index, script pubkey, and `role: OutputRole::Unknown`.

Core relies on the caller providing consensus-valid transactions. Since Liquid consensus has already verified reissuance token validity, confidential proofs, and Simplicity covenant witnesses, core does not need to re-verify — it only interprets.

### Transaction Ordering

Liquid transactions within a block have a strict serial order. Even if a contract's UTXO is spent and the resulting UTXO is re-spent within the same block, the two transactions have a defined order.

Core requires the caller to feed transactions in chain order. As long as this guarantee holds, core processes one transaction at a time sequentially — no concurrent writes, no locking needed. The "atomic write" for a single transaction is simply "write all state updates from this transaction before processing the next one."

### State Advancement via Script Matching and Output Values

Core determines transitions primarily through the current contract state, script pubkey matching (against the store's persisted script index), and explicit output values. This works because the covenant design encodes state into the script pubkey — different states produce different addresses — so the new state is usually identifiable from the transaction's outputs alone.

Two specific transitions produce no new covenant outputs, making the spend path indistinguishable from outputs alone. For these cases, the engine uses lightweight Simplicity witness path detection — see [Detection Strategy and Robustness](#detection-strategy-and-robustness).

#### Prediction Markets

The internal `CovenantPhase` maps to a unique set of slot script pubkeys (see [SlotIdentity and CovenantPhase](#slotidentity-and-covenantphase)). The transition type is determined by which slot scripts the new outputs match, combined with witness-based path detection where output matching is ambiguous.

**Binary markets** (8 slots):

- **Issuance** (Trading with 0 pairs to Trading with >0 pairs, or Trading to Trading with more pairs): Old outputs match Dormant or Unresolved slots; new outputs match Unresolved slots. `pairs` = new collateral value / `collateral_per_pair`. This division is always exact — the covenant enforces that collateral is a multiple of the pair cost. Implementations should assert exactness rather than silently truncating. `collateral_locked` = new collateral value - old collateral value. For initial issuance from Dormant, old collateral value is zero, so `collateral_locked` equals the full new collateral value. `IssuanceKind` is determined internally (`Initial` if old phase was Dormant, `Subsequent` if Unresolved) but not exposed.
- **Resolution** (Trading with >0 pairs → ResolvedYes/ResolvedNo): New output matches either `ResolvedYesCollateral` or `ResolvedNoCollateral` script. Which one determines the `outcome`.
- **Redemption** (ResolvedYes/ResolvedNo/Expired with outstanding_pairs decremented, terminal when reaching 0): No new covenant outputs. `payout_sats` is derived from the old collateral value. `side` from which token burn outputs are present. `RedemptionKind` is `PostResolution` if old state was Resolved*, `Expiry` if Expired.
- **Cancellation** (Trading → Trading with fewer pairs): New outputs match Unresolved or Dormant slots. `pairs_burned` and `collateral_returned` from the value differences.
- **Expiry** (Trading with >0 pairs → Expired): New output matches `ExpiredCollateral` script.
- **Dormant terminal paths** (Trading with 0 pairs → ResolvedYes/ResolvedNo/Expired with 0 pairs): Both RT outpoints consumed, no new covenant outputs. Output-only detection can't disambiguate — all three paths produce identical observable outputs. Engine uses witness-based path detection via `RedeemNode::decode`.

**Multi-outcome markets** (5N+2 slots, detection rules analogous but scaled):

- **Per-outcome pair issuance / cancellation**: detected by slot script match + witness path selector. Because the covenant always co-spends all 2N+1 Unresolved UTXOs and outputs always include all 2N+1 Unresolved slot continuations, the output-only signature isn't enough to distinguish "issue pair for outcome i" from "issue pair for outcome j". The engine examines the witness (`RedeemNode::decode`) to identify which outcome's spend path was selected. Supply deltas computed from RT-issuance amounts and/or token burn outputs.
- **Split-YES / Merge-YES / Split-NO / Merge-NO**: also witness-identified (all four produce similar output layouts — all 2N+1 covenant continuations). The collateral delta distinguishes (split-YES: +collateral_per_pair; merge-YES: -collateral_per_pair; split-NO: +(N-1)·collateral_per_pair; merge-NO: -(N-1)·collateral_per_pair). The engine can cross-check witness path against collateral delta for defensive verification.
- **Resolution** (Trading → Resolved(k)): new output matches one of the N `ResolvedCollateral(k)` scripts (one per possible winning outcome). The matched slot identifies `winning_outcome`.
- **Redemption**: no new covenant outputs; token burn outputs identify which `(outcome, side)` was redeemed; payout derived from the old resolved-collateral value.
- **Expiry**: new output matches `ExpiredCollateral` script (single script, not outcome-indexed since expiry is pre-resolution and doesn't pick an outcome).
- **Dormant terminal paths** (all 2N Dormant RTs → Resolved(k) or Expired, no continuation): analogous to binary. Witness-based path detection identifies which of the `N + 1` terminal paths was taken.

**Scaling implications for multi-outcome detection**: the engine makes **one `RedeemNode::decode` call per transaction per multi-outcome market transition** (same as binary — just with a richer set of possible paths). Cost remains negligible (~<1ms). The N outcome-pair slot scripts per phase are pre-stored in the script index during ingestion, so slot-match lookups remain O(1).

**Detection strategy summary (binary)**:

| Transition | Detection method | Airtight? |
|---|---|---|
| Issuance (initial) | Dormant input scripts → Unresolved output scripts | Yes — unique scripts per phase |
| Issuance (subsequent) | Unresolved input scripts → Unresolved output scripts, collateral increased | Yes — value direction distinguishes from cancellation |
| Resolution (non-dormant) | Unresolved inputs → ResolvedYes or ResolvedNo output script | Yes — unique scripts for slots 5 and 6 |
| Redemption | Resolved/Expired inputs → no covenant outputs, old state was Resolved/Expired | Yes — old state distinguishes from dormant terminal |
| Partial cancellation | Unresolved inputs → Unresolved outputs, collateral decreased | Yes — value direction distinguishes from issuance |
| Full cancellation | Unresolved inputs → Dormant output scripts | Yes — unique scripts |
| Expiry (non-dormant) | Unresolved inputs → ExpiredCollateral output script | Yes — unique script for slot 7 |
| Dormant terminal | Dormant RT inputs → no covenant outputs, witness path detection | Yes — witness is ground truth |

**Detection strategy summary (multi-outcome)**:

| Transition | Detection method | Airtight? |
|---|---|---|
| Issue / cancel pair (outcome k) | Script match (Unresolved continuation) + witness path (identifies outcome index k) + collateral delta | Yes — witness is ground truth |
| Split-YES / Merge-YES / Split-NO / Merge-NO | Script match (Unresolved continuation) + witness path (identifies primitive) + collateral delta (cross-checked against primitive) | Yes — witness is ground truth; collateral delta is defensive cross-check |
| Resolution (non-dormant, outcome k) | Unresolved inputs → ResolvedCollateral(k) script (1 of N possible) | Yes — unique scripts |
| Redemption | Resolved(k)/Expired inputs → no covenant outputs, token burn outputs identify (outcome, side) | Yes — old state + burn outputs disambiguate |
| Expiry (non-dormant) | Unresolved inputs → ExpiredCollateral script | Yes — unique script |
| Dormant terminal | All 2N Dormant RT inputs → no covenant outputs, witness path detection (N+1 possible paths: Resolved for each outcome + Expired) | Yes — witness is ground truth |

#### LMSR Pools

Different `s_index` values produce different covenant addresses (the s_index is a parameter in the script derivation). Unlike markets and orders, pools cannot use pre-stored scripts for output matching because the unbounded s_index makes full script enumeration impractical. The pool's taproot tree has constant Simplicity program leaves (same CMR regardless of `s_index`) and a variable `tapdata_leaf = TaggedHash("TapData", s_index.to_be_bytes())` — only the tapdata leaf changes when `s_index` changes, but computing the full script pubkey still requires an EC scalar multiplication per candidate (the taproot tweak), making brute-force script enumeration prohibitively slow (~3-7 seconds for all 65K values). Pool transition detection uses **witness-based path and s_index extraction** for all transitions, combined with output scanning for reserve values.

**Why witness-based for all pool transitions**: The engine needs the new `s_index` on every pool transition (it's stored in `LmsrPoolState::Active`). Deriving s_index from reserve values (reverse LMSR table lookup) is fragile — admin adjustments change reserves without moving along the LMSR curve, so the reserves no longer correspond to a single point on the curve. The witness contains the exact `old_s_index` and `new_s_index` used in the covenant verification — this is ground truth, not a derived estimate. Additionally, output-only detection cannot reliably distinguish close from public/admin (wallet outputs can mimic the covenant window pattern). Witness parsing resolves all ambiguities definitively for a negligible cost (~<1ms per `RedeemNode::decode` call, at most once per pool per block).

**Pool transition detection algorithm**:

1. **Parse witness**: Extract the Simplicity program bytes and witness bytes from the spending transaction's witness stack for the input that spent a tracked pool outpoint. Call `RedeemNode::decode` to identify the spend path (public, admin, or close) and extract `old_s_index` and `new_s_index`.
2. **Switch on spend path**:
   - **Public or Admin**: Find the covenant output window — three consecutive explicit outputs (as enforced by the covenant) where index N has the pool's YES asset ID, N+1 has the NO asset ID, N+2 has the Collateral asset ID, and all three share the same script pubkey (co-membership). The window must exist (covenant-enforced for public/admin paths). Read reserve values from the explicit outputs. Classify: public path with `new_s_index != old_s_index` → `Swapped`; public path with `new_s_index == old_s_index` or admin path → `Adjusted`.
   - **Close**: No covenant output window expected. The pool transitions to `Closed`. `final_reserves` from the stored state at time of closure.

Transition details:

- **Swap**: public spend path with `old_s_index != new_s_index`. `old_s_index` and `new_s_index` from the witness. `old_reserves` from stored state. `new_reserves` from explicit output values.
- **Adjustment**: either the admin path, or the degenerate public path with `old_s_index == new_s_index`. `old_reserves` and `new_reserves` from stored state and output values. The public API does not preserve the authorization source in `PoolTransition`; callers that care inspect the interpreted witness data directly.
- **Closure**: Spend path confirmed as close by the witness. All pool outpoints consumed, no new covenant outputs. `final_reserves` from the stored state at time of closure.

**Detection strategy summary:**

| Transition | Detection method | Airtight? |
|---|---|---|
| Public path | Witness: public spend path + s_index extraction. Outputs: reserve values from covenant window. `old_s != new_s` => `Swapped`; `old_s == new_s` => `Adjusted`. | Yes — witness is ground truth |
| Admin adjust | Witness: admin spend path. Outputs: reserve values from covenant window. | Yes — witness is ground truth |
| Close | Witness: close spend path confirmed | Yes — witness is ground truth |

#### Maker Orders

Order transition detection uses a structural witness check — key-spend vs script-spend is distinguishable from the taproot witness stack element count. Per BIP 341: strip the optional annex (if ≥2 elements and the last starts with byte `0x50`), then count remaining elements. One element = key-spend (the signature). Three elements = Simplicity script-spend (witness bytes, program bytes, control block). This is a Bitcoin/Elements-level structural check, not Simplicity witness decoding. **Dependency**: this detection requires the script-cancel refactor (see [maker-order-remove-script-cancel.md](../contracts/maker-order/maker-order-remove-script-cancel.md)) — post-refactor, the Simplicity program handles fills only, and cancellation is exclusively via key-spend. Without the refactor, both fill and cancel are script-spends and cannot be distinguished by element count.

- **Partial fill** (Active → Active): Order outpoint spent via script-spend, new covenant output exists with the same script pubkey. `fill_amount` = old locked value - new locked value.
- **Complete fill** (Active → Consumed): Order outpoint spent via script-spend, no new covenant output. The Simplicity covenant enforced valid payment to the maker. `fill_amount` = total locked value.
- **Cancellation** (Active → Cancelled): Order outpoint spent via key-spend, no new covenant output. The maker reclaimed their funds without covenant constraints.

**Detection strategy summary:**

| Transition | Detection method | Airtight? |
|---|---|---|
| Partial fill | Script-spend + new covenant output at same script with lower value | Yes — unique script, value decreased |
| Complete fill | Script-spend + no new covenant output | Yes — script-spend rules out cancellation |
| Cancellation | Key-spend (single witness stack element) | Yes — taproot structural check |

#### Detection Strategy and Robustness

Each contract type uses the detection method best suited to its structural characteristics — not a uniform approach, but the right tool for each type:

| Contract type | Key characteristic | Detection method | Why this is the right tool |
|---|---|---|---|
| Markets (non-dormant) | 8 bounded, pre-storable scripts | Script pubkey matching | Each phase has unique scripts — byte comparison is O(1) and trivially airtight |
| Markets (dormant terminal) | No covenant outputs produced | Witness path detection | No scripts to match against — spend path only exists in the witness |
| Orders | Two spend types at taproot level | Taproot structural check | Witness element count (1 vs 3) is the simplest possible distinguisher |
| Pools (all transitions) | Unbounded s_index, need s_index value on every transition | Witness-based | s_index only in witness; scripts can't be pre-stored; reserve-based derivation is fragile after admin adjustments |

**The underlying principle**: Simplicity covenants encode state into the script pubkey — each unique state produces a unique script. For markets and orders, this enables output-based detection (script matching, structural checks). For pools, the unbounded s_index makes script enumeration impractical, and the engine needs the s_index value on every transition, so witness-based extraction is the natural fit. For dormant market terminals, no covenant outputs are produced, leaving no scripts to match — the witness is the only source of truth.

**Witness-based detection uses `RedeemNode::decode`** from the `simplicity_lang` crate. Key properties:

- **No compilation needed**: `RedeemNode::decode` takes raw bytes from the transaction's witness stack — it parses the serialized program, not a `CompiledProgram`. This is lighter than full Simplicity compilation (no type inference, no commitment computation).
- **No storage needed**: The program bytes are already in the transaction being processed. Nothing needs to be pre-stored or cached.
- **Works for both `process_transaction` and `interpret_transaction`**: Both receive the full transaction, so both have access to the witness stack.
- **Acceptable performance**: Pool transitions occur at most once per pool per block (~1 minute on Liquid). Dormant market terminals occur at most once per market lifetime. The `RedeemNode::decode` cost (~<1ms) is negligible at these frequencies.
- **Authoritative**: The witness bytes are what was actually executed on-chain. This is ground truth, not a heuristic.

**Calling convention**: The taproot script-spend witness stack contains (after stripping the optional annex and control block) the Simplicity program bytes and witness bytes as separate elements. These are passed to `RedeemNode::decode` as two separate bit streams:

```rust
let program = RedeemNode::<Elements>::decode(
    BitIter::from(&program_bytes[..]),
    BitIter::from(&witness_bytes[..]),
)?;
```

**Spend path detection**: Simplicity uses `case` combinators for branching. At redemption time, the untaken branch is **pruned** — replaced by its CMR hash. The decoded tree reveals which branch was taken via `Inner::AssertL` (left taken, right pruned) or `Inner::AssertR` (left pruned, right taken). The engine maps the `.simf` source's branching structure to these variants — e.g., for pools, the primary program's top-level split distinguishes swap (left) from admin/close (right), with a secondary split distinguishing admin from close.

**Witness value extraction**: Witness nodes in the decoded program carry `Value` objects. The engine extracts them via post-order traversal (`post_order_iter().into_witnesses()`). Each `Value` can be read as raw bytes and converted to the expected type (e.g., `u64` for s_index). The witness layout mirrors the `.simf` witness declarations — values appear in the same order they're declared in the source.

The no-cache architecture is preserved: output-based detection works from persisted data (scripts, state, output values) without compiled contracts. Witness-based detection reads from the transaction itself without compiled contracts. Only PSET builders need the full compiled contract (for witness *encoding*).

## Contract Ingestion

Core doesn't know or care about the discovery layer (Nostr, manual import, QR code, etc.). To start tracking a contract, the caller provides contract parameters and either a creation transaction or a current-state snapshot, depending on the contract type.

### Per-Type Ingestion

The three ingestion methods handle the different needs of each contract type:

**Markets** (`ingest_market`): Always ingested from the creation transaction. Markets have few transitions (bounded by the number of covenant phases) and fast catch-up, so there is no benefit to non-initial ingestion. Ingestion handles both binary (2 token issuances, 2 RT issuances, 3-output covenant window) and multi-outcome (2N token issuances, 2N RT issuances, (2N+1)-output covenant window) markets via the `MarketParams` enum. Verification cost scales linearly with N — each token and RT asset ID is reconstructed and matched against the creation tx's issuance metadata.

**Pools** (`ingest_pool`): Support both creation-tx and non-initial ingestion via `PoolSnapshot`. Pools can accumulate thousands of state transitions (one per swap), making forward-sync from creation expensive. Non-initial ingestion via `PoolSnapshot::Current` allows a trader to start using a pool immediately from its current state without replaying history.

**Orders** (`ingest_persistent_order` / `ingest_ephemeral_order`): Makers monitoring their own orders use `ingest_persistent_order` (creation tx required, full history). Takers discovering orders for routing use `ingest_ephemeral_order` — accepts either a Creation snapshot (accurate `offered_amount`, no history, auto-cleanup past finality) or a Current snapshot (mid-life discovery, baseline-at-ingestion `offered_amount`, same auto-cleanup). Tracking mode is immutable post-ingestion; untrack + re-ingest to switch.

### What each snapshot variant provides

| Snapshot | History | Verification | Use case |
| -------- | ------- | ------------ | -------- |
| `Creation(ChainTransaction)` | Full — forward-sync from creation recovers all transitions | Creation tx verified against params and, for tracked contracts, against the canonical recovery conventions | Makers, pool operators, anyone needing price history |
| `Current { ... }` | None — no prior transitions recoverable | No verification back to creation; canonical param shape still enforced on the supplied params | Takers, traders who only need current state |

The trade-off is explicit in the type system: `Current` = fast start, no history; `Creation` = full history + verified.

### Catching Up New Contracts

When a contract is ingested that was created in the past, it needs to be "caught up" to the chain tip. The engine handles this automatically via `step` — the caller simply ingests the contract and calls `step`. The engine determines the per-contract-type catch-up strategy internally (script scan for markets/orders, forward-chaining for pools). See [Chain Sync](#chain-sync) for the full sync model.

Each contract tracks its own `synced_to` height independently. Existing fully-synced contracts are unaffected when a new contract is ingested — the engine efficiently targets only stale contracts during catch-up.

### Consumer Flow

With `step` managing all sync internally, the consumer flow is uniform across contract types:

```rust
// 1. Ingest contracts (per-kind methods).
//    `market_params` is `MarketParams` (Binary(..) or MultiOutcome(..)); all three ingestion
//    methods take params by reference.
let market_id = engine.ingest_market(&market_params, &creation_tx)?;
let pool_id = engine.ingest_pool(&pool_params, PoolSnapshot::Creation(pool_creation_tx))?;
let order_id = engine.ingest_ephemeral_order(&order_params, OrderSnapshot::Current { ... })?;

// 2. Sync — step handles catch-up, subscription setup, and steady-state
engine.step(&mut chain)?;

// 3. Ongoing sync loop
loop {
    let report = engine.step(&mut chain)?;
    for tx in &report.transactions {
        for t in &tx.interpretation.transitions {
            update_ui(&t.details);
        }
    }
    sleep(Duration::from_secs(60)); // or wait for block notification
}
```

The caller never manages scripts, outpoints, subscriptions, or per-contract sync strategies. `step` handles all of this internally based on contract type. New contracts ingested between `step` calls are automatically caught up on the next call.

**Trading** (same as before — unrelated to sync):
```rust
let quote = engine.quote_trade(&market_id, spec, fee_rate)?;
let pset = engine.build_trade_pset(&quote, &funding)?;
let signed = signer.sign(pset)?;
chain.broadcast(signed)?;
// interpret_transaction for pending UX; step processes on confirmation
```

**Order creation** (maker):
```rust
// derive_order_params takes MarketParams (umbrella) + outcome: OutcomeIndex.
// For binary markets, pass OutcomeIndex::BINARY.
let (params, masked_index) = derive_order_params(
    &deadcat_xprv,
    &market_params,            // MarketParams umbrella
    OutcomeIndex::BINARY,       // or OutcomeIndex::new(k) for multi-outcome
    order_index,
    Side::Yes,
    OrderDirection::SellBase,
    price,
    1, 1,
)?;
let pset = engine.build_create_order_pset(&params, offered_amount, masked_index, &funding)?;
let signed = signer.sign(pset)?;
chain.broadcast(signed)?;
// After confirmation: ingest as persistent (maker wants full history) and step catches it up
let order_id = engine.ingest_persistent_order(&params, &creation_tx)?;
engine.step(&mut chain)?;
```

## Persistence: Store Trait

Core defines traits for state persistence. The implementor controls what to store and how. All methods are required (no default implementations) — the store trait is designed for Polymarket-scale operation where every query needs efficient indexing. If an implementor wants to take shortcuts (e.g., implement `list_markets` by scanning all contracts and filtering), the performance cost is visible in their own code, not hidden behind a default.

### Required: ContractStore

```rust
pub trait ContractStore {
    type Error: std::error::Error;

    // Single contract lookup — &self
    fn current_state(&self, contract_id: &ContractId) -> Result<Option<Contract>, Self::Error>;

    // Bulk reads — &self
    fn find_by_outpoints(&self, outpoints: &[OutPoint]) -> Result<Vec<ContractMatch>, Self::Error>;

    // Index lookups — &self (populated at ingestion via DerivedContractData)
    fn find_by_asset_id(&self, asset_id: &AssetId) -> Result<Option<AssetInfo>, Self::Error>;
    fn covenant_scripts(&self, contract_id: &ContractId) -> Result<Vec<Script>, Self::Error>;

    // Sync support — &self (used by step internally)
    fn stale_contracts(&self, tip_height: u32) -> Result<StaleContracts, Self::Error>;
    fn contract_outpoints(&self, contract_id: &ContractId) -> Result<Vec<(SlotIdentity, OutPoint)>, Self::Error>;

    // Per-type listing — &self (typed results, not Contract enum)
    fn list_markets(&self, filter: StateFilter, page: Pagination) -> Result<Page<MarketEntry>, Self::Error>;
    fn list_pools(&self, filter: StateFilter, page: Pagination) -> Result<Page<PoolEntry>, Self::Error>;
    fn list_orders(&self, filter: StateFilter, page: Pagination) -> Result<Page<OrderEntry>, Self::Error>;

    // Relationship queries — &self (typed results; outcome-scoped)
    // For binary markets, callers pass `OutcomeIndex::BINARY`. For multi-outcome
    // markets, callers pass the specific outcome they care about. Store implementations
    // should index by (market_id, outcome) for efficient lookup. To get "all pools/orders
    // across all outcomes" of a multi-outcome market, the caller (or the engine's Market
    // view) iterates over `0..outcome_count` and merges results.
    fn pools_for_market(&self, market_id: &ContractId, outcome: OutcomeIndex, filter: StateFilter, page: Pagination) -> Result<Page<PoolEntry>, Self::Error>;
    fn orders_for_market(&self, market_id: &ContractId, outcome: OutcomeIndex, filter: StateFilter, page: Pagination) -> Result<Page<OrderEntry>, Self::Error>;

    // Trade routing support — &self (used by quote_trade internally; outcome-scoped)
    fn best_orders_for_market(&self, market_id: &ContractId, outcome: OutcomeIndex, side: Side, direction: OrderDirection, ascending: bool, min_remaining: u64, limit: u32) -> Result<Vec<OrderEntry>, Self::Error>;

    // Writes — &mut self
    fn track_contract(&mut self, contract_id: ContractId, contract: Contract, derived: DerivedContractData, initial: InitialContractState) -> Result<(), Self::Error>;
    fn untrack_contract(&mut self, contract_id: &ContractId) -> Result<(), Self::Error>;
    fn apply_transitions(&mut self, transitions: &[StateUpdate]) -> Result<(), Self::Error>;
    fn advance_synced_heights(&mut self, updates: &[(ContractId, u32)]) -> Result<(), Self::Error>;
    fn rollback_to_height(&mut self, height: u32) -> Result<(), Self::Error>;
    fn prune_finalized(&mut self, current_height: u32, finality_depth: u32) -> Result<(), Self::Error>;
}

pub struct StaleContracts {
    pub script_contracts: Vec<ScriptContractInfo>,
    pub outpoint_contracts: Vec<OutpointContractInfo>,
}

pub struct ScriptContractInfo {
    pub contract_id: ContractId,
    pub scripts: Vec<Script>,
    pub synced_to: u32,
}

pub struct OutpointContractInfo {
    pub contract_id: ContractId,
    pub outpoints: Vec<(SlotIdentity, OutPoint)>,
    pub synced_to: u32,
}
```

Every consumer must implement this. Read methods take `&self`, write methods take `&mut self` — mirroring the engine's own borrow semantics. The engine calls read methods during interpretation (`&self` on the engine borrows the store as `&self`) and write methods during processing (`&mut self` on the engine borrows the store as `&mut self`).

`apply_transitions` applies the full `&[StateUpdate]` slice produced from a single chain transaction. Required semantics:

- **Per-transaction atomic**: the slice commits entirely or not at all. Typically implemented with a single database transaction around the body. See the "Atomicity requirements" paragraph later in this section for full error-handling semantics.
- **Durable on return**: the engine depends on this for crash safety — once the call returns `Ok`, the state must survive a process crash without further action.
- **Idempotent per `(contract_id, txid)`**: calling `apply_transitions` twice with a `StateUpdate` sharing the same `(contract_id, txid)` is a no-op on the second call. Required because `process_transaction` is idempotent (for crash recovery and multi-step sync); that idempotency propagates through this method. For stores implementing `ContractHistory`, idempotency extends to history writes — don't create duplicate entries. The standard implementation checks whether a transition for `(contract_id, txid)` already exists before inserting.

`find_by_outpoints` is the hot-path method called on every internal `process_transaction`. It is not paginated because its input is bounded by the transaction's input count (constrained by Liquid's transaction size limits).

`find_by_asset_id` and `covenant_scripts` are index lookups populated at ingestion time. The engine passes `DerivedContractData` (asset IDs + scripts) to `track_contract`, and the store indexes this data for fast lookups. `find_by_asset_id` backs the engine's `identify_asset` method. `covenant_scripts` is used by `step` internally for building catch-up scan queries and subscription registrations. Neither requires Simplicity knowledge — the engine pre-computes the data and hands it over.

`stale_contracts` is the primary sync-support method. It returns all contracts with `synced_to < tip_height`, pre-grouped by sync strategy (script-based vs outpoint-based) with their scripts/outpoints included. This allows the engine to build catch-up queries and subscription registrations in a single store call. The store determines which group a contract belongs to based on its `DerivedContractData`: contracts with non-empty `covenant_scripts` go into `script_contracts` (markets and orders); contracts with empty `covenant_scripts` go into `outpoint_contracts` (pools). A SQLite implementation does this with one JOIN + WHERE query. In steady-state (all contracts at tip), it returns empty — no pagination needed.

`contract_outpoints` returns the current tracked outpoints for a contract. Used by `step` for pool forward-chaining. The store already tracks outpoints internally (for `find_by_outpoints`); this method exposes them per-contract.

`best_orders_for_market` returns orders for a market in `Active` state with `offered_amount - total_filled >= min_remaining`, sorted by price (ascending or descending as specified). Orders at the same price are returned in creation order (ascending `ChainPosition`) — FIFO prioritizes maker fairness over minimal fee cost (in rare cases, the taker pays marginally higher network fees because a smaller, earlier order is activated before a larger, later one; the fee-aware greedy mitigates this by accounting for per-order activation cost). The Active filter is implicit — consumed and cancelled orders cannot participate in routing. Used by `quote_trade` internally for trade routing — not a user-facing listing method. The `side` parameter filters by which outcome token is the order's base asset (the engine resolves `Side` to the market's YES or NO asset ID). The `direction` parameter selects which order direction to match (the engine translates from the taker's `TradeSpec`). Together, `side` + `direction` select exactly one of the four order types (e.g., YES-SellBase, NO-SellQuote). The `min_remaining` parameter filters dust orders at the store level. The `limit` bounds the result count (e.g., 50). No cursor pagination — the router processes all returned orders in a single pass. See [trade-routing-algorithm.md](trade-routing-algorithm.md) for the full routing algorithm.

`advance_synced_heights` bulk-advances `synced_to` for multiple contracts. Called by `step` after processing a batch of transactions or after confirming that subscriptions have covered through the tip height.

`track_contract` initializes `synced_to` from `initial.position.block_height` and records `initial.outpoints` for outpoint tracking (`find_by_outpoints`, `contract_outpoints`). `rollback_to_height(N)` resets `synced_to = min(synced_to, N)` for all contracts.

`untrack_contract` removes the contract, all derived data (asset ID index entries, covenant scripts), and any history. Store implementations must clean up all references.

**Discovery dedup**: Discovery payloads always include `creation_txid`, so the caller can construct the full `ContractId` and use `engine.contract(&contract_id)` to check if a contract is already tracked. No CMR-only lookup is needed — full `ContractId` dedup is both correct and efficient. CMR-only dedup would be incorrect in the rare case of two legitimate instances sharing params (same CMR, different `creation_txid`).

**Processing log**: The store must persist enough rollback metadata during `apply_transitions` for `rollback_to_height` to reverse transitions. At minimum: the contract ID, old outpoints, new outpoints, old contract state, and block height for each processed transition. The old contract state (the `MarketState`, `LmsrPoolState`, or `OrderState` value before the transition) is required because several transitions are not reversible from `TransitionDetails` alone — e.g., `PoolTransition::Closed` doesn't carry the old `s_index`, `OrderTransition::Cancelled` doesn't carry the old `total_filled`. Persisting the old state makes rollback mechanical (restore old state + old outpoints) regardless of transition type. `prune_finalized` removes this metadata for transitions below the finality threshold. This processing log is separate from `ContractHistory`'s transition history — it exists for rollback, not for user-facing queries. `rollback_to_height` must also clean up derived data (asset ID index, covenant scripts) for contracts removed during rollback.

**Atomicity requirements**: `apply_transitions` is **per-transaction atomic** — the full `&[StateUpdate]` slice passed in a single call commits as a unit or not at all. The engine invokes `apply_transitions` once per processed chain transaction; cross-contract transactions (e.g., routed trades) produce slices with more than one `StateUpdate`. Per-transaction atomicity is required (not merely recommended) because the engine's error semantics rely on it: on `CovenantInvariantViolation` during a multi-contract transaction, the current transaction's batch must roll back as a unit so the engine's retry and rollback logic sees a consistent state. Store implementations typically achieve this with a single database transaction around the `apply_transitions` body. Within a batch, contract-level consistency follows from per-batch atomicity — a single contract's state update (old outpoints → new outpoints + state change) commits atomically by construction.

Across multiple transactions processed in one `step` call, each transaction's batch commits independently. On error mid-step, transactions processed before the error stay committed; the erroring transaction's batch is rolled back; unprocessed transactions remain to be processed on retry. Retrying `step` picks up where it left off via `apply_transitions`'s per-`(contract_id, txid)` idempotency.

### Optional: ContractHistory

```rust
pub trait ContractHistory: ContractStore {
    fn transition_history(
        &self,
        contract_id: &ContractId,
        after: Option<ChainPosition>,
        limit: u32,
    ) -> Result<Vec<HistoryEntry>, Self::Error>;
}
```

`ContractHistory` is a supertrait of `ContractStore` — implementing it requires also implementing `ContractStore`. This means `Self::Error` is the same associated type from `ContractStore`, eliminating any error type mismatch. The engine's history methods can wrap the error in `CoreError::Store(e)` without ambiguity.

Only implement if the consumer wants price charts, audit trails, etc. Core never depends on history for processing — it only needs current state. History returns `HistoryEntry` (a type alias for `TypedStateUpdate<TransitionDetails>`) — the caller-facing fields without internal outpoints. To get full output classification for a historical transaction, the caller can call `interpret_transaction`.

History is exposed through typed convenience methods on the view types (`Market::history`, `Pool::history`, `Order::history`), only available when the store implements `ContractHistory`. The store trait itself has a single unified `transition_history` method — the typed unwrapping happens inside each view's `history()` method. See [History Methods](#history-methods).

### Implementor Controls Retention

The engine always passes full `StateUpdate` details to `apply_transitions`. The implementor decides what to keep:

- **Minimal (e.g., Aqua)**: Update current outpoints, discard old state. Doesn't implement `ContractHistory`.
- **Full (e.g., Deadcat Live)**: Update current state AND append to history table. Implements `ContractHistory`. Supports price charts and audit trails.
- **Selective**: Keep LMSR pool history (for price charts) but discard order fill history (not needed).

This is an implementation detail — core doesn't need per-contract configuration flags.

### Tip State Principle

The current contract state (stored via `ContractStore`) carries enough information for basic wallet UX without requiring `ContractHistory`. A minimal consumer that only implements `ContractStore` can still answer:

- "Did this binary market resolve YES or NO?" -> `MarketState::Binary(BinaryMarketState::ResolvedYes { outstanding_pairs: 0 })` (terminal) or `ResolvedYes { outstanding_pairs: 500 }` (awaiting redemption)
- "Which outcome won this multi-outcome market?" -> `MarketState::MultiOutcome(MultiOutcomeMarketState::Resolved { winning_outcome, collateral_unredeemed })` — `collateral_unredeemed == 0` indicates terminal
- "How much of my order has been filled?" -> `OrderState::Active { total_filled }` or `OrderState::Cancelled { total_filled }`
- "What are my pool's current reserves?" -> `LmsrPoolState::Active { reserves, .. }`

Transition history is for richer features: price charts, fill-by-fill order breakdowns, full audit trails.

### ContractStore Compliance Test Kit

Store correctness is enforced by a separate crate, `deadcat-core-store-testkit`, that integrators depend on as a dev-dependency. The crate exposes `pub fn run_store_compliance(store: &mut impl ContractStore) -> TestResult` (plus `pub fn run_chain_source_compliance(chain: &mut impl ChainSource) -> TestResult` for `ChainSource` implementations). Integrators call one function per trait in their own test suite and get automated conformance checking. Pattern matches `sqlx::testing`, Diesel backend compliance suites, iroh's blob store kit — a well-established approach for trait-based APIs with pluggable backends.

The test kit enforces the following invariant categories. Each category maps to one or more concrete test cases in the kit:

**Outpoint tracking and slot identity**
- Outpoint round-trip: after `apply_transitions` writes `(slot, outpoint)`, `find_by_outpoints(&[outpoint])` returns a `ContractMatch` with the same `(slot, outpoint)` pair.
- Outpoint uniqueness across contracts: no two tracked contracts share an outpoint.
- Slot label uniqueness within a contract: `contract_outpoints` returns at most one entry per `SlotIdentity` value.
- Slot-type containment: `PoolSlot` values only appear in pool contracts, `MarketSlot` only in market contracts, etc.

**Contract lifecycle**
- `track_contract` on an already-tracked `ContractId` errors with `ContractAlreadyTracked`.
- `untrack_contract` removes the contract and all derived data (asset-id index entries, covenant scripts, processing log entries, history entries for `ContractHistory` implementors).
- `DerivedContractData` is immutable after `track_contract`.

**Sync state**
- `synced_to` monotonically advances per contract; `advance_synced_heights` with a lower value is rejected or no-op.
- `stale_contracts` groups by emptiness of `DerivedContractData.covenant_scripts` (empty → `outpoint_contracts`, non-empty → `script_contracts`) and returns only contracts with `synced_to < tip_height`.

**Write-path atomicity and idempotency**
- `apply_transitions` is per-transaction atomic: the full slice commits as a unit or not at all.
- `apply_transitions` is idempotent on `(contract_id, txid)`: the second call is a no-op, with no duplicate history entries for `ContractHistory` implementors.
- `apply_transitions` is durable on return.

**Indexing**
- Asset-ID index consistency: `find_by_asset_id(a)` returns contract X iff X's `DerivedContractData.asset_ids` includes `a`.
- `covenant_scripts(contract_id)` returns exactly the scripts from `DerivedContractData.covenant_scripts`.

**Rollback**
- `rollback_to_height(N)` restores each contract's state to its most recent transition at or below N.
- Contracts whose creation transaction was in blocks strictly above N are removed.
- After rollback, `synced_to = min(old_synced_to, N)` for remaining contracts.
- Rollback is idempotent; `rollback_to_height(N)` for N ≥ current tip is a no-op.

**Pagination**
- Cursor stability under concurrent writes: opaque cursors continue to function correctly when new contracts are ingested between pages (no duplicates, no missed items for contracts ordered before the cursor position).
- Cursor scope: a cursor from one method is rejected when passed to a different method or with different filters.

**Query-level ordering**
- `best_orders_for_market` returns orders in price order with FIFO by `ChainPosition` among ties; filtered to `Active` state with sufficient remaining liquidity.
- `transition_history` returns in ascending `ChainPosition` (oldest-first).

**Processing log vs. history separation**
- `prune_finalized` removes rollback metadata but does NOT remove `ContractHistory` entries.
- Stores that don't implement `ContractHistory` can still roll back correctly (processing log is independent).

**Tracking mode behavior (order-specific)**
- `OrderTracking::Persistent` orders accumulate history if the store implements `ContractHistory`.
- `OrderTracking::EphemeralFresh` and `OrderTracking::EphemeralMidLife` orders produce no `ContractHistory` entries regardless of whether the store implements the trait.
- `prune_finalized` auto-untracks terminal Ephemeral orders past the finality depth.

**ChainSource invariants** (for `run_chain_source_compliance`)
- `register_*` is idempotent with set semantics: re-registering same scripts/outpoints collapses to one active watch; `from_height` re-registration uses `min(existing, new)` to widen coverage.
- `unregister_*` of never-registered items is a no-op, not an error.
- Notifications delivered per registered item, not per call (no duplicate notifications from duplicate registrations).
- `transactions_by_scripts` returns results in chain order with complete-block guarantees.

This is the source of truth for compliance. The categories above are the user-facing overview; the crate's source code enumerates the specific test cases, fixtures, and edge cases. As new invariants surface during implementation (Phases 3-6), they land in the kit first and in this list second.

## Separation of Concerns: Wallet vs Contract Layer

A key design principle: the contract layer and wallet layer have complementary, non-overlapping views of the same transaction.

| Output type    | Tracked by         | Example                          |
| -------------- | ------------------ | -------------------------------- |
| Covenant state | Contract engine    | Market collateral, pool reserves |
| Wallet balance | Wallet (LWK/GDK)  | L-BTC change, received tokens    |
| Fee            | Neither explicitly | Implicit from tx structure       |

The contract engine doesn't track wallet outputs. The wallet doesn't track covenant outputs. They correlate on txid when labeling is needed.

The `Transition.external_outputs` bridges the gap — core identifies which outputs are NOT contract state, and labels their roles where possible, so the wallet can display "Received 10 YES tokens from issuance" without understanding covenant mechanics. For confidential external outputs, core provides the output index and script pubkey; the wallet uses its own blinding keys to determine asset and value.

## PSET Construction

All PSET builders are engine methods — no wallet access, no chain queries, no signing. The caller provides operation-specific arguments and a `WalletFunding` struct. The engine handles Simplicity contract compilation, script derivation, taproot tree construction, coin selection, and fee computation internally. Builders that involve reissuance token (RT) outputs return `UnblindedPset` — a newtype that enforces covenant blinding before the caller can sign (see [Confidential Transaction Blinding](#confidential-transaction-blinding)). All other builders return `PartiallySignedTransaction` directly.

**Simplicity is fully encapsulated**: Consumers never see compiled contracts (`CompiledPredictionMarket`, `CompiledLmsrPool`, `CompiledMakerOrder`), Commitment Merkle Roots (CMRs), taproot trees, or witness encoding. These are internal to the engine. Consumers provide contract params (plain data: oracle keys, asset IDs, prices, expiry times) and receive PSETs back. The word "Simplicity" need not appear in consumer code.

**State preconditions**: every transition-producing builder requires the contract to be in a specific set of states (e.g., `build_issuance_pset` requires `Trading`; `build_redemption_pset` requires a terminal variant with unredeemed supply). Violating the state precondition returns `CoreError::InvalidContractState { contract_id, kind: InvalidStateKind::WrongVariant { expected, actual } }`. Runtime preconditions beyond state (e.g., sufficient basket supply for `build_merge_yes_pset`; chain height ≥ expiry for `build_expire_transition_pset`) produce `InvalidStateKind::ConditionFailed { condition, detail }`. See [State Machine Summary](#state-machine-summary) for the full valid-transition matrix. Per-builder rustdoc states the "Valid from:" precondition concisely and references this matrix for the complete picture.

### Coin Selection and Fee Computation

PSET builders perform coin selection internally. The caller provides `available_utxos` via `WalletFunding` — their full candidate pool (or a pre-filtered subset if they want to exclude specific UTXOs). The builder selects the minimum needed. Passing all wallet UTXOs is the expected usage.

Fee computation is also internal. The caller provides `fee_rate: FeeRate` via `WalletFunding` (obtained from their chain backend). The builder constructs the transaction, measures its weight, and computes `fee = rate * weight`. This eliminates the chicken-and-egg problem of needing to know the transaction size to estimate the fee.

If the available UTXOs don't cover the required amount plus fee, the builder returns `CoreError::InsufficientFunds { shortfalls: Vec<Shortfall> }`.

### UnblindedUtxo

PSET builders accept `UnblindedUtxo` (via `WalletFunding`) — a UTXO with unblinded (revealed) asset, value, and blinding factors. This type is defined in `deadcat-core` and carries: outpoint, script pubkey, explicit asset ID, explicit value, asset blinding factor, and value blinding factor. `deadcat-sdk` imports this type from core (not the other way around — core has no dependency on SDK). The caller obtains these from their wallet's UTXO set.

### Output Consolidation

PSET builders consolidate outputs that share the same destination script and asset into a single output. For example, a redemption payout (collateral returned) and fee change (excess L-BTC from the fee input) both go to `funding.return_script` — the builder merges them into one output. This reduces transaction size (~4.3 KB per confidential output on Liquid), lowers fees, reduces UTXO bloat in the wallet, and improves privacy (fewer outputs = less structural fingerprinting).

Because consolidated outputs may include fee change alongside their primary purpose, `OutputRole` identifies *which* output serves a purpose, but `TransitionDetails` is authoritative for *exact semantic amounts* (payout, tokens burned, collateral locked, etc.). The wallet should always use `TransitionDetails` for display amounts.

### No Per-Builder Args Structs

PSET builders take operation-specific arguments as direct function parameters alongside the shared `WalletFunding` struct. This avoids a proliferation of single-use parameter types — the function signature IS the documentation. Every builder needs wallet funding (every Liquid transaction requires an explicit fee). The `WalletFunding` struct carries the three common fields; operation-specific arguments are direct parameters. See [WalletFunding](#walletfunding).

### Prediction Market Builders

Canonical builder signatures are in [View Types § Market](#market) (common to both market kinds) and [View Types § MultiOutcomeMarket](#multioutcomemarket) (cross-outcome primitives for multi-outcome only). The creation builders are on the engine (see [API Overview](#api-overview)). This section covers the per-builder transitions and semantics.

**Common market builders (Market view, both kinds)**:

| Builder | Transaction | Covenant Transition |
| ------- | ----------- | ------------------- |
| `build_issuance_pset` | Mint pair for outcome k | Trading → Trading (more pairs) |
| `build_cancellation_pset` | Burn pair for outcome k | Trading → Trading (fewer pairs) or → Trading (0) |
| `build_oracle_resolve_pset` | Oracle resolution | Trading → Resolved* |
| `build_expire_transition_pset` | Expire market | Trading → Expired |
| `build_redemption_pset` | Redeem tokens (post-resolution or post-expiry) | Resolved*/Expired → same variant with fewer unredeemed (terminal at 0) |

**Multi-outcome cross-outcome primitives (MultiOutcomeMarket view)**:

| Builder | Transaction | Covenant Path |
| ------- | ----------- | ------------- |
| `build_split_yes_pset` | Mint complete YES basket | Trading → Trading (all supplies.yes ↑ by sets) |
| `build_merge_yes_pset` | Burn complete YES basket | Trading → Trading (all supplies.yes ↓ by sets) |
| `build_split_no_pset` | Mint complete NO basket | Trading → Trading (all supplies.no ↑ by sets) |
| `build_merge_no_pset` | Burn complete NO basket | Trading → Trading (all supplies.no ↓ by sets) |

`build_cross_outcome_arb_pset` (multi-contract atomic arb co-spending market + N pools) is deferred to v2. See [Future: Cross-Outcome Arb API](#future-cross-outcome-arb-api-v2).

**Creation builders (engine)**:

| Builder | Transaction | Params |
| ------- | ----------- | ------ |
| `build_binary_market_creation_pset` | Binary market creation (2 YES/NO assets, 2 RTs) | `BinaryMarketCreationParams` |
| `build_multi_outcome_market_creation_pset` | Multi-outcome market creation (2N assets, 2N RTs) | `MultiOutcomeMarketCreationParams` |

**Semantic notes**:

- **Issuance**: `build_issuance_pset(outcome, pairs, yes_dest, no_dest, funding)` handles both initial (Dormant → Unresolved) and subsequent (Unresolved → Unresolved) issuance — the view determines which from the cached state. For binary markets, `outcome` must be `OutcomeIndex::BINARY`; other values return `CoreError::InvalidParams`. For multi-outcome, `outcome` selects which of the N outcome-pair RT slots to draw from.
- **Cancellation**: `build_cancellation_pset(outcome, pairs_to_burn, funding)` — if `pairs_to_burn` is `None`, the engine computes the maximum cancellable from available YES + NO tokens for the given outcome in `funding.available_utxos` (minimum of the two token balances).
- **Oracle resolution and expiry**: both `build_oracle_resolve_pset` and `build_expire_transition_pset` branch internally based on outstanding supply — when called on a market with zero outstanding pairs/sets (Dormant), they handle the dormant terminal paths (all RT UTXOs consumed atomically, market reaches terminal state). No new builder methods are needed for this case. See [market-dormant-terminal-paths.md](../contracts/prediction-market/market-dormant-terminal-paths.md).
- **Redemption**: `build_redemption_pset(outcome, side, tokens_to_redeem, funding)` handles both post-resolution and post-expiry redemption; the view determines which from the cached state. Post-resolution: for binary markets the engine validates `side` matches the winning side; for multi-outcome it validates either `(outcome == winning_outcome, side == Yes)` (winning YES_k) or `(outcome != winning_outcome, side == No)` (winning NO_j). Post-expiry: any `(outcome, side)` combination is valid at the fractional rate.
- **Split/merge YES (multi-outcome)**: atomically mints or burns one of each `YES_k` for the market's N outcomes, with collateral flow of `sets × collateral_per_pair`. The `destinations: &[Script]` slice on `build_split_yes_pset` has length `outcome_count`; `destinations[k]` receives `sets` units of `YES_k`.
- **Split/merge NO (multi-outcome)**: same pattern but for NO tokens; collateral flow is `sets × (N-1) × collateral_per_pair`.
- **Cross-outcome arb** (multi-outcome): single atomic transaction that co-spends the market contract's split-YES (or merge-YES) path with each outcome's binary LMSR pool public path (`old_s_index != new_s_index`). Closes cross-outcome price coherence gaps (`Σ p_YES_k ≠ 1`) in one tx. The engine constructs the full multi-contract PSET internally.
- **Creation builders**: return the full derived params (`BinaryMarketParams` / `MultiOutcomeMarketParams`) alongside the PSET so the caller can `ingest_market` after the creation transaction confirms. `build_binary_market_creation_pset` selects 2 defining inputs; `build_multi_outcome_market_creation_pset` selects 2N defining inputs and compiles the N-specific generated `.simf` covenant.

**OP_RETURN recovery hints**: Both `build_binary_market_creation_pset` and `build_multi_outcome_market_creation_pset` include a **37-byte** zero-value OP_RETURN hint (69 bytes with exotic collateral). Binary and multi-outcome markets share the same layout, distinguished by the hint's type tag byte. For multi-outcome markets, `outcome_count` is **not stored** — it is derived at recovery time from the creation tx's new-issuance count (2N issuances → N outcomes), with a defensive filter on `AssetIssuance` records (both `amount` and `inflation_keys` non-null) to rule out asymmetric issuances. The covenant script is the authoritative binding between N and the tx shape; a wrong derived N produces a script mismatch at ingestion, which is a loud failure. All 4N asset IDs for multi-outcome markets are derivable from the creation transaction's issuance entropy — the hint doesn't scale with N. See [chain-only-recovery.md](../protocol/chain-only-recovery.md) and [multi-outcome-market-contract.md](../contracts/multi-outcome/multi-outcome-market-contract.md).

### LMSR Pool Builders

Canonical builder signatures are in [View Types § Pool](#pool). The bootstrap builder is on the engine (see [API Overview](#api-overview)). This section covers the per-builder transitions and semantics.

| Builder | Location | Transaction | Covenant Path |
| ------- | -------- | ----------- | ------------- |
| `build_lmsr_bootstrap_pset` | `engine` | Pool creation (fund initial reserves) | — (creates initial state) |
| `build_adjust_pset` | `Pool` view | Admin liquidity adjustment | Admin path (s_index unchanged) |
| `build_close_pset` | `Pool` view | Pool closure (reclaim all reserves) | Close script path |

**Semantic notes**:

- **`build_adjust_pset(pair_delta, collateral_delta, funding)`**: `pair_delta` is applied equally to both YES and NO reserves (signed: positive = injection, negative = withdrawal). `collateral_delta` is applied to the collateral reserve independently. The API shape makes the covenant's paired-delta constraint (YES and NO must move equally on the admin path) unrepresentable as an error — the caller cannot express asymmetric deltas. The engine validates that the resulting reserves meet the covenant's minimum reserve floor (`MIN_POOL_RESERVE` — a protocol constant, 1,000 sats per reserve, hardcoded in the covenant) and returns `CoreError::InvalidParams` if violated. If both deltas are zero, the engine returns `CoreError::InvalidParams` — a no-op adjustment would waste fees. The wallet can present an absolute-target UI ("set pool to 1000 YES/NO") by computing the delta from current reserves on their side. See [lmsr-pool-design.md](../contracts/lmsr-pool/lmsr-pool-design.md) for the full pool parameter design.
- **`build_close_pset(funding)`**: atomically consumes all three reserve UTXOs via the dedicated Simplicity close script path (NUMS internal key makes key-spend unspendable). All reserve funds are returned to `funding.return_script`. See [lmsr-pool-close-path.md](../contracts/lmsr-pool/lmsr-pool-close-path.md).
- **`build_lmsr_bootstrap_pset(params, initial_s_index, initial_reserves, masked_index, funding)`**: creates the pool at the chosen starting state with the caller-specified starting reserves. The recommended UX flow is to seed `initial_reserves` from `estimate_bootstrap`, but the builder does not silently re-derive or canonicalize reserves internally — explicit reserve vectors are part of the caller's chosen end state. The creation transaction includes a **40-byte** zero-value OP_RETURN recovery hint containing: market creation txid, `max_loss_sats` and `half_payout_sats` (4-bit 1-2-5 table indices each, shared with the market `base_payout` encoding), `fee_bps` (u12, 0.01% granularity), `initial_s_index` (u16, the starting table index for script verification during recovery), and XOR-masked pool operator derivation index. The hint does not encode reserves; recovery learns the actual starting reserves from the creation transaction outputs themselves. All other covenant params are derived via deterministic table generation. See [Wallet Recovery](#wallet-recovery), [chain-only-recovery.md](../protocol/chain-only-recovery.md), and [lmsr-pool-design.md](../contracts/lmsr-pool/lmsr-pool-design.md).

**Multi-outcome pool composition**: a pool always serves one outcome's YES/NO pair. For binary markets that's the single event's YES/NO. For multi-outcome markets under Option C composition, each outcome has its own independent binary LMSR pool (created via `derive_pool_params` with an `outcome: OutcomeIndex` parameter, then bootstrapped via `build_lmsr_bootstrap_pset`). The pool contract doesn't know or care which market kind underlies its YES/NO tokens.

**Signing note**: Pool adjust and close PSETs require signing with both the wallet key (for fee inputs) and the pool's admin key (for the covenant spend authorization). Both keys are controlled by the pool operator. Pool swaps (via trade PSETs) are permissionless and require only the taker's wallet key.

Pool swaps are not built directly — they are part of trade transactions (see [Trade PSET Builder](#trade-pset-builder) below).

### Maker Order Builders

Canonical builder signature for cancellation is in [View Types § Order](#order). The creation builder is on the engine (see [API Overview](#api-overview)). The maker's lifecycle is directly exposed here; the taker side (filling orders) is handled through trade transactions — see [Trade PSET Builder](#trade-pset-builder).

| Builder | Location | Transaction | State Change |
| ------- | -------- | ----------- | ------------ |
| `build_create_order_pset` | `engine` | Create limit order | — (creates initial state) |
| `build_cancel_pset` | `Order` view | Cancel order | Active → Cancelled |

**Semantic notes**:

- **`build_create_order_pset(params, offered_amount, masked_index, funding)`**: takes `MakerOrderParams` fully formed (params aren't derived from issuance entropy — they're committed directly). Includes a 40-byte zero-value OP_RETURN recovery hint (masked derivation index, market txid, compressed price/outcome/side/direction/min_fill/min_remainder). The `masked_index` parameter is computed by the caller via `derive_order_params`. See [Wallet Recovery](#wallet-recovery) and [chain-only-recovery.md](../protocol/chain-only-recovery.md).
- **`build_cancel_pset(funding)`**: the cancel path uses taproot key-spend with the maker's real public key (not NUMS). Requires the maker's signature; no script-path covenant execution. See [maker-order-remove-script-cancel.md](../contracts/maker-order/maker-order-remove-script-cancel.md).

**Multi-outcome order composition**: like pools, orders are per-outcome. `derive_order_params` takes an `outcome: OutcomeIndex` parameter that selects which YES/NO pair the order offers. Orders on different outcomes of the same multi-outcome market are independent contracts. Routing (`quote_trade`) targets orders that match the trade's `(outcome, side)`.

### Trade PSET Builder

Trade transactions are unique: they route across multiple contracts (LMSR pools and/or maker orders) in a single transaction. This requires cross-contract route optimization — choosing which pools and orders to hit, in what amounts, for best execution. The engine has the state needed for this (pool reserves, order books, LMSR math), so trade PSET construction uses a two-step pattern:

**Step 1: Quote** (engine method, read-only):

```rust
pub fn quote_trade(
    &self,
    market_id: &ContractId,
    spec: TradeSpec,
    fee_rate: FeeRate,
) -> Result<TradeQuote, CoreError<S::Error>>;
```

The engine computes the optimal route across all available pools and orders for the market, minimizing total cost to the taker including transaction fee overhead. The `fee_rate` parameter is required because the routing algorithm uses fee-adjusted effective prices — each liquidity source's activation cost (transaction weight) is weighted by the fee rate to determine whether including it improves the route. The routing algorithm uses pool-subset enumeration combined with fee-aware greedy order selection — see [trade-routing-algorithm.md](trade-routing-algorithm.md) for the full specification. Returns a `TradeQuote` representing the best available fill, including `estimated_fee` computed from the route's total transaction weight and the provided fee rate.

For existing pools, the router may also choose a market-assisted pool leg when that improves fillability or taker price. Assisted legs still surface as `LiquiditySource::LmsrPool` in the quote; the exact parent-market co-spend stays internal in `TradeRoute`. In v1, assisted routing is limited to at most one pool leg per route, uses `IssuePairs` on buys and `CancelPairs` on sells, and is considered only while the parent market still supports the required issuance/cancellation path.

Returns `Err(CoreError::NoLiquidity { market_id, outcome, side, direction })` only when the router cannot fill any positive amount for the target `(market, outcome, side, direction)` — all pools at minimum reserves in the trade direction, all orders dust, or no tracked sources. Any `filled_amount > 0` returns `Ok(TradeQuote)`, including heavily partial fills where the caller may want to abandon. Partial-fill decision-making is the caller's responsibility — inspect `TradeQuote.filled_amount` vs `TradeQuote.requested_amount`. See [TradeQuote](#tradequote-and-related-types) for details.

Post-resolution trading is not gated — `quote_trade` succeeds regardless of the parent market's state as long as routable liquidity exists. See [Pool and Order Lifecycle at Market Resolution](#pool-and-order-lifecycle-at-market-resolution).

**Step 2: Build** (engine method):

```rust
pub fn build_trade_pset(
    &self,
    quote: &TradeQuote,
    funding: &WalletFunding,
) -> Result<PartiallySignedTransaction, CoreError<S::Error>>;
```

Takes the accepted quote and the caller's wallet funding. The engine validates that `funding.fee_rate` matches the fee rate used during quoting — if they differ, it returns `CoreError::InvalidParams` because the route was optimized for the quote's fee rate (a different rate could make the route suboptimal; the caller should re-quote with the current rate). The engine recompiles contracts from stored params, selects the needed UTXOs, computes the actual fee from the real transaction weight, and builds the PSET. The actual fee may differ from `TradeQuote.estimated_fee` because the quote's weight model assumes a single wallet input, while coin selection may add more — display the quote's fee as an estimate, not a guarantee.

**Input and output layout**: the trade PSET arranges inputs and outputs using a deterministic contract-window ordering to satisfy each covenant's introspection rules simultaneously: witness-parameterized pool windows, an optional witness-parameterized parent-market window for one assisted pool leg, positional maker receives, and witness-specified order remainders. The full layout algorithm — including output-index assignment, witness `in_base` / `out_base` / `remainder_idx` construction, and the aliasing-prevention invariants the builder must uphold — is specified in [transaction-composability-model.md § Output Layout for Multi-Covenant Transactions](transaction-composability-model.md#output-layout-for-multi-covenant-transactions). Implementers of `build_trade_pset` should consult that doc as the authoritative layout spec.

**Freshness check algorithm**: the quote captures outpoint snapshots at quote time for every contract the route touches (one `SlotIdentity`-labeled set per pool/order leg, plus the parent market window for an assisted pool leg). `build_trade_pset` verifies each snapshot is still current by comparing against `store.contract_outpoints(contract_id)`. If any snapshotted outpoint is no longer in that contract's current outpoint set, returns `CoreError::StaleQuote { reason }` with one of:

- `StaleQuoteReason::OutpointsChanged { contract_id }` — another `process_transaction` advanced the contract (pool swap/adjust, order fill, order cancel).
- `StaleQuoteReason::ContractUntracked { contract_id }` — the contract was untracked between quote and build.
- `StaleQuoteReason::ContractRemoved { contract_id }` — the contract was removed by `rollback_to_height` between quote and build (e.g., reorged out).

The check is O(L) store reads where L is the number of legs (typically 1 pool + ~5 orders) — negligible overhead. Partial freshness is not supported: if any leg is stale, the entire quote is, and the caller re-quotes. Note the subtle case of pool admin adjust — the pricing curve doesn't change (s_index stays put), but the covenant still produces new outpoints, so a quote made pre-adjust goes stale despite the LMSR math being identical.

If the quote is still valid at build time but the transaction later fails on-chain (spent inputs due to a block arriving between build and broadcast), the caller re-quotes. This is standard trading UX — quotes are inherently ephemeral.

See [Trade Types](#trade-types) and [TradeQuote](#tradequote-and-related-types) for full type definitions.

**Why trades use a two-step pattern**: Trade is the only operation requiring cross-contract route optimization from engine state. The two-step pattern enables the standard trading UX of "show quote, user confirms, then build." All other PSET builders are single-step — the caller provides the operation params directly and gets a PSET back. They don't need a quoting step because they operate on a single contract whose state the caller already knows.

### Confidential Transaction Blinding

On Liquid, transaction outputs can be **explicit** (asset and value visible) or **confidential** (hidden behind Pedersen commitments with range and surjection proofs). The three Deadcat covenants require all covenant outputs (collateral, reserves, order locked value) to be **explicit** — the Simplicity programs use `unwrap_right()` on output introspection jets, which fails on confidential outputs. The one exception is reissuance token (RT) outputs, which Elements requires to be blinded for reissuance mechanics to work.

**Which builders need RT blinding**: The 6 prediction market builders that involve RT outputs (`build_binary_market_creation_pset`, `build_multi_outcome_market_creation_pset`, `build_issuance_pset`, `build_cancellation_pset`, `build_oracle_resolve_pset`, `build_expire_transition_pset`). The remaining builders have no RT involvement — their covenant outputs are all explicit and require no blinding by core.

**Deterministic RT blinding**: RT blinding factors are derived deterministically from public on-chain data (see [deterministic-rt-blinding.md](../protocol/deterministic-rt-blinding.md)), not generated randomly. This is essential for core's architecture: the engine internally manages RT outpoints and must reconstruct blinding factors when building future PSETs that spend those outpoints. With deterministic derivation, the engine recomputes the factors on demand without needing to persist blinding secrets.

**`UnblindedPset` newtype**: The 6 RT-involving builders return `UnblindedPset` — an opaque type whose private fields capture the explicit PSET, deterministic RT blinding factors, and all input secrets (both covenant inputs with zero blinding factors and wallet inputs with real blinding factors from `UnblindedUtxo`). The type enforces that the caller cannot extract a `PartiallySignedTransaction` without going through a blinding method, making "forgot to blind" unrepresentable at the type level.

```rust
pub struct UnblindedPset { /* private */ }

impl UnblindedPset {
    /// Blind covenant RT outputs, mark wallet outputs for confidential blinding.
    /// Returns a PreparedPset. Caller must then call
    /// `pset.blind_last(rng, secp, &input_secrets)` to blind wallet outputs,
    /// then sign.
    pub fn prepare(
        self,
        wallet_blinding_pubkey: &PublicKey,
    ) -> Result<PreparedPset, BlindingError>;

    /// Blind covenant RT outputs with VBF balancing. Wallet outputs remain
    /// explicit (unblinded). Returns a ready-to-sign PSET.
    /// **Precondition**: All wallet inputs must be explicit (zero blinding factors).
    /// The CBF pass-through self-balances the RT portion, but wallet inputs with
    /// non-zero blinding factors would unbalance the equation. Naturally satisfied
    /// on regtest where all UTXOs are explicit.
    pub fn finalize(self) -> Result<PartiallySignedTransaction, BlindingError>;
}

pub struct PreparedPset {
    pub pset: PartiallySignedTransaction,
    /// Complete input secrets for all inputs (covenant + wallet).
    /// Pass directly to `pset.blind_last()`.
    pub input_secrets: HashMap<usize, TxOutSecrets>,
}
```

**`prepare` vs `finalize`**: Both methods blind the RT outputs identically using deterministic factors. The difference is a **privacy decision** about wallet outputs:

- **`prepare(pubkey)`**: For callers who want confidential wallet outputs (the common case on Liquid mainnet). Blinds RT outputs using non-last semantics (pushes VBF delta to `global.scalars` for later balancing). Marks wallet outputs with the provided blinding public key. Returns `PreparedPset` with the PSET and complete input secrets map. The caller then calls `pset.blind_last(rng, secp, &input_secrets)` which blinds the wallet outputs and balances VBFs. After `blind_last`, the PSET is ready to sign.

- **`finalize()`**: For callers who don't need wallet output privacy (regtest, testing, explicit-only workflows). Blinds RT outputs using deterministic factors (ABF from tagged hash, VBF derived from CBF − ABF). The CBF pass-through scheme ensures RT blinding factors self-balance, so no wallet output is needed for VBF absorption. Wallet outputs stay explicit. Returns a `PartiallySignedTransaction` ready to sign immediately. **Requires all wallet inputs to be explicit** (zero blinding factors) — the CBF pass-through balances the RT portion of the blinding equation, but confidential wallet inputs would contribute non-zero blinding factors with no wallet output to absorb them. This precondition is naturally satisfied on regtest where all UTXOs are explicit; for mainnet transactions with confidential wallet inputs, use `prepare()` instead. See [deterministic-rt-blinding.md](../protocol/deterministic-rt-blinding.md) for the derivation scheme.

**Signing must happen after blinding**: The sighash commits to output commitments. If outputs are blinded after signing, the commitments change and signatures become invalid.

**Implementation**: Core implements deterministic RT blinding using public APIs from `elements` (PSET output fields: `amount_comm`, `asset_comm`, `value_rangeproof`, `asset_surjection_proof`, etc.) and `secp256k1-zkp` (Pedersen commitments, range proof generation, surjection proof generation). The `global.scalars` field (used for VBF delta tracking in the `prepare` path) is a serialized PSET field that survives cross-process serialization/deserialization. No fork of the `elements` crate is needed.

**Non-RT builders**: The 7 builders without RT involvement (`build_redemption_pset`, all pool builders, all order builders, `build_trade_pset`) return `PartiallySignedTransaction` directly with all outputs explicit. If the caller wants confidential wallet outputs, they handle blinding using their standard Elements wallet workflow — this is a general Liquid concern, not deadcat-specific.

**Caller flow — RT builders** (on `Market` view):
```rust
let market = engine.market(&id)?.expect("market tracked");

// Confidential wallet outputs (Liquid mainnet)
let unblinded = market.build_issuance_pset(OutcomeIndex::BINARY, pairs, &yes, &no, &funding)?;
let prepared = unblinded.prepare(&wallet_blinding_pubkey)?;
prepared.pset.blind_last(&mut rng, &secp, &prepared.input_secrets)?;
signer.sign(&mut prepared.pset)?;

// Explicit wallet outputs (regtest / testing — all wallet inputs must be explicit)
let unblinded = market.build_issuance_pset(OutcomeIndex::BINARY, pairs, &yes, &no, &funding)?;
let mut pset = unblinded.finalize()?;
signer.sign(&mut pset)?;
```

**Caller flow — non-RT builders** (on view types, or engine for trades and creation):
```rust
let pool = engine.pool(&pool_id)?.expect("pool tracked");
let mut pset = pool.build_close_pset(&funding)?;
// Optional: standard wallet blinding if desired
signer.sign(&mut pset)?;
```

## Chain Sync

### ChainSource Trait

The `ChainSource` trait defines the chain data access capabilities the engine needs for sync. The caller provides an implementation — `deadcat-core` defines the trait, separate crates ship implementations for common backends (Esplora, Electrs, Waterfalls).

```rust
pub trait ChainSource {
    type Error: std::error::Error + Send + Sync + 'static;

    // Catch-up (pull — historical data retrieval)
    fn tip_height(&self) -> Result<u32, Self::Error>;
    fn transactions_by_scripts(&self, scripts: &[Script], from_height: u32, limit: u32)
        -> Result<Vec<ChainTransaction>, Self::Error>;
    fn spending_transaction(&self, outpoint: &OutPoint)
        -> Result<Option<ChainTransaction>, Self::Error>;
    fn issuance_transaction(&self, asset_id: &AssetId)
        -> Result<Option<ChainTransaction>, Self::Error>;

    // Steady-state (push registration + drain)
    fn register_scripts(&mut self, scripts: &[Script], from_height: u32) -> Result<(), Self::Error>;
    fn register_spends(&mut self, outpoints: &[OutPoint]) -> Result<(), Self::Error>;
    fn unregister_scripts(&mut self, scripts: &[Script]) -> Result<(), Self::Error>;
    fn unregister_spends(&mut self, outpoints: &[OutPoint]) -> Result<(), Self::Error>;
    fn drain_notifications(&mut self) -> Result<Vec<ChainTransaction>, Self::Error>;
}
```

**9 methods, split into two groups:**

The **catch-up** methods are synchronous pull queries. `tip_height` returns the current chain tip. `transactions_by_scripts` returns confirmed transactions *involving* any of the given scripts from `from_height` onwards, in chain order. Each transaction appears at most once in the result, even if it involves multiple scripts from the input set. `issuance_transaction` returns the transaction that first issued a given asset ID — used for token holder recovery (see [chain-only-recovery.md](../protocol/chain-only-recovery.md)). For Esplora backends, this maps to `GET /asset/:asset_id` → `issuance_txin.txid`. Works despite blinded reissuance token outputs because the issuance entropy is always explicit in the transaction's input data. "Involving" means both transactions that create outputs paying to those scripts AND transactions that spend outputs from those scripts — the engine needs both for catch-up (creation of covenant UTXOs and their subsequent spends). This matches the standard behavior of Electrum's `blockchain.scripthash.get_history` and Esplora's `/scripthash/:hash/txs`. `limit` sets a target result count. The implementation **MUST** return results as complete blocks — it is never valid to return a partial block. If the `limit`-th result falls within a block, the implementation **MUST** include all remaining matching transactions from that block before stopping. Results **MUST** be in chain order (ascending by `ChainPosition`). The actual result count may exceed `limit` by up to the number of matching transactions in the final block. `spending_transaction` returns the transaction that spent a given outpoint, or `None` if unspent.

The **steady-state** methods manage a notification registration system. `register_scripts` and `register_spends` tell the chain source to watch for activity. `drain_notifications` returns any confirmed transactions that matched since the last drain. `unregister_scripts` and `unregister_spends` clean up when contracts reach terminal states or are untracked.

**Gap-free handoff**: `register_scripts` takes `from_height` — the chain source guarantees delivery of all matching transactions at or above this height. The engine registers with `from_height = synced_to`, creating overlap with the catch-up scan rather than a gap. Overlap is harmless (`process_transaction` is idempotent). `register_spends` does NOT take `from_height` — instead, the chain source checks if the outpoint is already spent and includes the spending transaction in the next `drain_notifications` call if so. This binary spent/unspent check is sufficient because outpoints (unlike scripts) have a single possible event.

**Subscription semantics — idempotent set operations**:

- `register_scripts(scripts, from_height)` adds each script to the active watch set. If a script is already registered, the effective `from_height` becomes `min(existing, new)` — re-registration only widens coverage, never narrows it. This matters for the engine's re-initialization path (after rollback or subscription-state invalidation): `step` calls `register_scripts` for all applicable contracts again; already-registered scripts are not an error.
- `register_spends(outpoints)` adds each outpoint to the active watch set. No `from_height` concern (binary spent/unspent check). Duplicate registrations collapse to one active watch.
- `unregister_scripts(scripts)` and `unregister_spends(outpoints)` remove each from the active watch set. Unregistering a script or outpoint that was never registered (or was already unregistered) is a **no-op, not an error** — makes cleanup and rollback handling forgiving.
- Notifications are delivered **per registered item**, not per registration call. Registering the same script or outpoint twice does not produce duplicate notifications.

These semantics free the engine from bookkeeping "have I already registered this?" state. The engine calls `register_*` when it wants coverage and `unregister_*` when it wants to release coverage; the chain source handles the set-membership details.

**The trait is a read-only data source, not a service.** It makes no writes to the chain, does no broadcasting, and performs no fee estimation. The engine treats it as an immutable data accessor. The `&mut self` on registration methods reflects internal state management (tracking what's registered), not external side effects.

**`drain_notifications` returns confirmed transactions only** (with `ChainPosition`), **in chain order** (ascending by `ChainPosition`). This matches the ordering guarantee of `transactions_by_scripts`. The engine processes notifications sequentially — out-of-order delivery could cause it to miss a transaction whose inputs reference outpoints created by a not-yet-processed earlier transaction. Mempool/unconfirmed transactions are out of scope — the caller handles mempool awareness separately via `interpret_transaction` if they want pending UX.

### Sync Model

The engine tracks a `synced_to` height per contract — the block height through which the contract has been checked for chain activity. `synced_to` advances during `step` even when no transitions are found. This allows the engine to distinguish "never checked past height 1000" from "checked to height 2000, nothing happened."

Contracts fall into two categories based on whether their covenant scripts are static or dynamic:

**Static scripts** (markets: 8 scripts, orders: 1 script): Scripts don't change across transitions. The same scripts cover the contract's entire lifecycle. This enables both batch catch-up (one `transactions_by_scripts` call) and one-time subscription registration (`register_scripts` once, never re-register).

**Dynamic scripts** (pools): Scripts encode the s_index, which changes on every swap. Pre-storing all possible scripts is impractical. Catch-up uses outpoint-based forward-chaining (`spending_transaction`). Steady-state uses outpoint subscription (`register_spends` / `unregister_spends`), re-registered after each transition.

### Sync Strategies by Contract Type

```
                    Catch-up                    Steady-state              Re-subscribe?
                    ─────────────────────────   ─────────────────────     ─────────────
Markets             Script scan (8 scripts)     Script notifications      No
                    One batch query gets all     Same 8 scripts cover
                    historical txs               all future states

Orders              Script scan (1 script)      Script notifications      No
                    One batch query gets all     Same script covers all
                    fills + cancellation         future fills + cancel

Pools (creation)    Forward-chain or            Outpoint notifications    Yes — after
                    batched TXIDs                                         every transition

Pools (current)     None — already current      Outpoint notifications    Yes — after
                                                                          every transition
```

The root cause of the split: pool scripts encode the `s_index`, which changes on every swap (unbounded). Markets have 8 bounded phases. Orders have 1 unchanging script regardless of fill level.

### step Internals

`step` orchestrates the full sync cycle internally. The caller never sees catch-up vs steady-state — they just call `step`.

**First call (or after subscription state invalidation):**

1. `chain.tip_height()` to determine the target
2. Read stale contracts from the store (contracts with `synced_to < tip`)
3. **Script-based catch-up** (markets + orders): Batch all stale contracts' scripts into one `chain.transactions_by_scripts(all_scripts, from_height, BATCH_SIZE)` call (initial `from_height = min_synced_to`). Process results via `process_transaction` (internal). Advance `from_height` to `max_block_height_in_batch + 1` after each batch. Loop until the scan returns fewer than `BATCH_SIZE` results (indicating no more data). The complete-block guarantee (see `ChainSource`) ensures no transactions are skipped at block boundaries.
4. **Outpoint-based catch-up** (pools): For each stale pool, forward-chain via `chain.spending_transaction(outpoint)` until `None` (unspent = caught up). The engine picks one representative outpoint per pool (the covenant guarantees all 3 are spent together).
5. **Set up subscriptions**: `chain.register_scripts(scripts, synced_to)` for markets/orders. `chain.register_spends(outpoints)` for pools.
6. **Advance `synced_to`** for all contracts to tip.

**Subsequent calls (steady-state):**

1. `chain.drain_notifications()` → process any matching transactions
2. For pool transitions: `chain.unregister_spends(old_outpoints)`, `chain.register_spends(new_outpoints)`
3. For contracts reaching terminal states (markets: ResolvedYes/ResolvedNo/Expired with outstanding_pairs == 0; pools: Closed; orders: Consumed/Cancelled): `chain.unregister_scripts(scripts)` for markets/orders, `chain.unregister_spends(outpoints)` for pools. Prevents stale subscriptions from producing irrelevant notifications.
4. `chain.tip_height()` → advance `synced_to` for all contracts (even if no notifications — the scan/subscription covered everything)

**Subscription state invalidation**: Ingesting a contract, untracking a contract, or calling `rollback_to_height` invalidates the engine's internal subscription state. The engine tracks internally whether subscriptions are initialized; these operations reset the flag, causing the next `step` to re-initialize (re-reads from store, re-registers subscriptions). Invalidation is rare — ingestion and untracking are infrequent, rollbacks are exceptional on Liquid.

### `synced_to` Tracking

`synced_to` is a per-contract block height stored alongside the contract state. Initialized from `initial.position.block_height` during `track_contract`. Advanced by `step` via `advance_synced_heights`. Reset by `rollback_to_height(N)` → `synced_to = min(synced_to, N)` for all contracts.

The store exposes this through `synced_to` on `ContractEntry` (readable by the caller for informational purposes like "last synced: block 2000") and through `stale_contracts(tip_height)` (used by the engine to efficiently find contracts needing work). See [ContractStore](#required-contractstore).

## Pool and Order Lifecycle at Market Resolution

The pool and order covenants are **market-state-agnostic** — they accept swaps and fills regardless of whether the parent market has resolved or expired. See [lmsr-pool-design.md § Market Resolution](../contracts/lmsr-pool/lmsr-pool-design.md#market-resolution). `deadcat-core` mirrors this at the policy layer: **trading through resolved-parent pools and orders is not gated**.

### What remains available regardless of parent market state

- `quote_trade` and `build_trade_pset` route through pools and orders on resolved-parent markets normally. Quotes return `Ok(TradeQuote)` if routable liquidity exists; PSETs build successfully.
- `build_adjust_pset` and `build_close_pset` on the `Pool` view remain callable.
- `build_cancel_pset` on the `Order` view remains callable.
- Market operations (`build_oracle_resolve_pset`, `build_expire_transition_pset`, `build_redemption_pset`) proceed according to the market's own state-machine transitions.

### Pool operator responsibilities

The operator closes the pool via `build_close_pset` when convenient after market resolution. Until they do, the pool remains tradable at stale prices (YES ≈ 1 at YES-resolved markets, half each at expiry), and an informed trader could drain reserves by buying out winning-token inventory. This is **not protected by `deadcat-core`** — the engine's policy is that pool operators manage their own liquidity carefully, including closing pools at terminal market states.

**Why this isn't enforced at the covenant layer**: airtight protection would require the pool covenant to observe the parent market's state on every swap, and a covenant can only introspect the current transaction — so the only mechanism is to **co-spend the market covenant's UTXO as an input on every swap transaction**. That would make every swap substantially heavier (adding the market's collateral input + witness to the pool's ~1,000-vbyte footprint), and the cost would fall on every trade, not just ones near resolution. Paying a permanent per-trade tax to block the informed-drainer attack in the narrow window between resolution and operator-close isn't a trade worth making. See [lmsr-pool-design.md § Why the pool covenant can't feasibly gate post-resolution trading](../contracts/lmsr-pool/lmsr-pool-design.md#why-the-pool-covenant-cant-feasibly-gate-post-resolution-trading) for the full analysis.

### Order maker responsibilities

Order makers cancel unfilled orders via `build_cancel_pset` when convenient; otherwise takers may fill them post-resolution. As with pools, the engine does not gate this.

### Ephemeral orders and rollback

For orders tracked as `OrderTracking::EphemeralFresh` or `OrderTracking::EphemeralMidLife`, terminal states (`Consumed`, `Cancelled`) remain visible in storage during the finality window and auto-untrack at `prune_finalized` past finality (depth 2 on Liquid). Rollback interacts with this as follows:

- **Rollback within finality**: processing log still contains the transitions. `rollback_to_height(N)` reverses them normally — a Consumed Ephemeral order returns to Active state if the terminal transition occurred in blocks above N, same as any other rollback.
- **Rollback past a finalized terminal transition**: the order was auto-untracked at `prune_finalized`; its processing log entries and stored state are gone. Rollback cannot restore it. On the new canonical chain, if the order exists again (e.g., its creation tx didn't get reorged), the caller must re-discover from Nostr and re-ingest. This matches the general "contracts above rollback height are removed; caller re-discovers" rule for Ephemeral orders specifically.

Persistent orders never auto-untrack, so they rollback cleanly within the finality window and otherwise behave identically to markets and pools.

### UI-layer warnings are appropriate

Wallet UIs concerned about post-resolution trading risk can warn users before routing trades by checking `Market::state()` independently. This provides the honest-user protection without a false sense of airtight gating — adversarial actors would fork or bypass core regardless. See [Design Principles § Engine gates covenant-invalidity and impossibility, not unfavorability](#engine-gates-covenant-invalidity-and-impossibility-not-unfavorability) for the full rationale.

## Simplicity Contracts (Internal)

Core contains the `.simf` Simplicity contract source code and the compiler integration. Given contract parameters and a network type (testnet/mainnet), core internally:

- Compiles Simplicity contracts into committed programs with Commitment Merkle Roots (CMRs)
- Derives taproot script pubkeys for any contract state
- Generates control blocks for spending witnesses
- Decodes witness data from spending transactions

This is necessary for both PSET construction (building covenant outputs with correct scripts) and state advancement (matching output scripts to determine new state).

**Compilation model**: The Simplicity source templates are parsed once (process-wide `OnceLock` cache). Per-contract instantiation (binding parameters to the template) and commitment are performed on demand — there is no in-memory compiled contract cache. During ingestion, the engine compiles the contract, passes pre-computed scripts and asset IDs to the store as `DerivedContractData` for indexing, and discards the compiled result. PSET builders recompile from stored params on each call — the cost is moderate (~10-100ms, dominated by instantiation + commitment; template parsing is already cached). `process_transaction` and `interpret_transaction` do not need compiled contracts — they determine transitions from script pubkey matching (using the store's persisted script index) and output values, without witness decoding. `ContractEngine::new` is O(1) — it does not iterate existing contracts or compile anything at construction time.

**Why no compiled contract cache**: The only operation requiring a compiled contract is PSET construction (specifically, witness encoding for spending covenant inputs). The simplicityhl library's `CompiledProgram` type is opaque with no serialization API, so compiled contracts cannot be persisted to disk. An in-memory cache would only save recompilation across multiple PSET builds for the same contract within a single engine lifetime — a rare scenario that doesn't justify the cache's complexity (eviction during rollback, interior mutability for `&self` methods). If simplicityhl adds `CompiledProgram` serialization in the future, persisting compiled contracts at ingestion time would eliminate recompilation entirely — a transparent internal optimization with no API change. See [simplicityhl-compiled-program-serialization.md](../upstream-simplicity/simplicityhl-compiled-program-serialization.md) for the upstream request.

**None of this is exposed in the public API.** Consumers interact with contract params (plain data) and the engine's methods. Compiled contracts, CMRs, taproot tree structures, and witness encoding are implementation details.

## Reorg Handling

Core maintains a processing log: for each processed transaction, the contract ID, old outpoints, new outpoints, and block height. Rolling back to height N means:

1. Find all transitions from blocks strictly above N
2. Reverse them: restore old outpoints, remove new outpoints
3. Delete the processing records
4. Remove contracts whose creation transaction was in blocks strictly above N

Contracts created at height N are kept. Contracts created at height N+1 or above are removed — their creation transactions may no longer exist on the canonical chain after the reorg.

**Uniform rollback for all ingestion types**: Contracts ingested via `PoolSnapshot::Current` or `OrderSnapshot::Current` include a `ChainPosition` indicating when the snapshot's outpoints were confirmed. The engine passes this via `InitialContractState` to `track_contract`, and rollback treats it identically to a creation transaction's position — if the initial position is strictly above height N, the contract is removed.

**`synced_to` reset**: `rollback_to_height(N)` resets `synced_to = min(synced_to, N)` for all remaining contracts. This ensures the next `step` call re-scans from the rollback height, catching any new chain data on the canonical fork. The engine also invalidates its internal subscription state, so the next `step` re-initializes subscriptions.

The caller detects the reorg (their chain backend tells them), calls `rollback_to_height`, then calls `step` to re-sync:

**History cleanup**: For stores implementing `ContractHistory`, `rollback_to_height` must also remove any persisted transition history records above the rollback height. These records reference a chain that may no longer exist after the reorg. **Important**: Both state rollback and history cleanup must be atomic — a single database transaction, not two separate operations. A crash between rolling back state and rolling back history would leave the store in an inconsistent state (current state reflects the rollback but history still contains records from the pre-rollback chain). Since the store implements both `ContractStore` (with `rollback_to_height`) and `ContractHistory` (with its history table), it has everything needed to clean up both in a single operation. This is a `ContractStore::rollback_to_height` implementation concern, not a separate method on `ContractHistory`, because atomicity requires both cleanups to happen in one call.

**Known limitation**: `rollback_to_height` removes contracts ingested above the rollback height. The caller must re-discover and re-ingest these contracts if they reappear on the new canonical chain. Since discovery happens over Nostr, contracts can always be re-fetched. This is the correct behavior — a contract whose creation transaction was reorged out is not a valid contract on the current chain.

**Typical usage**: After rolling back, the caller calls `step` which handles re-scanning automatically (synced_to was reset, so step catches up from the rollback height):

```rust
engine.rollback_to_height(reorg_height)?;
engine.step(&mut chain)?;  // re-syncs all contracts from rollback height
```

### Finality-Based Pruning

On Liquid, transactions are considered absolutely irreversible after 2 confirmations. Processing log entries for finalized transactions can never be needed for reorg recovery and can be safely pruned:

```rust
fn prune_finalized(&mut self, current_height: u32, finality_depth: u32) -> Result<(), CoreError<S::Error>>;
```

The caller periodically calls `prune_finalized` with the current chain tip and the network's finality depth (2 for Liquid). This keeps the processing log bounded without sacrificing correctness.

**Important**: Pruning the processing log (for reorg rollback) is independent from retaining transition history (for price charts, audit trails). The `ContractHistory` trait stores historical transitions permanently — `prune_finalized` only removes the rollback metadata that's no longer needed.

## State Machine Summary

This section consolidates the valid-transition matrices for each contract type into a single reference. Per-builder rustdoc references this section via "Valid from: [state list]" clauses, and `InvalidContractState { kind: WrongVariant { expected, actual } }` surfaces mismatches programmatically. The matrices are the source of truth; per-state-enum definitions and per-builder rustdoc comments defer to these tables.

### Creation Builders

Creation builders don't operate on existing state — they produce brand-new contracts. Listed separately from the transition matrices:

| Builder | Produces | Preconditions |
|---|---|---|
| `build_binary_market_creation_pset` | `Trading { outstanding_pairs: 0 }` binary market | Convention-valid params (`base_payout` in 1-2-5 table, expiry on 60-block boundary) |
| `build_multi_outcome_market_creation_pset` | `Trading { supplies: [empty; N] }` multi-outcome market | Same plus `N ∈ {3, 4}` for v1 |
| `build_lmsr_bootstrap_pset` | `Active { ... }` LMSR pool | Parent market tracked; convention-valid params; reserves available |
| `build_create_order_pset` | `Active { tracking, offered_amount, total_filled: 0 }` order | Parent market tracked; convention-valid params |

After the creation tx confirms on-chain, the caller ingests via the corresponding `ingest_*` method to begin tracking.

### Binary Market transitions

| From state | Valid builder | To state | Additional condition |
|---|---|---|---|
| `Trading` | `build_issuance_pset` | `Trading` (outstanding + Δ) | — |
| `Trading { outstanding > 0 }` | `build_cancellation_pset(Some(Δ))` | `Trading` (outstanding − Δ) | Δ < outstanding |
| `Trading { outstanding > 0 }` | `build_cancellation_pset(None)` | `Trading { outstanding: 0 }` | caller supplies all outstanding YES/NO pairs for that outcome |
| `Trading` | `build_oracle_resolve_pset` | `ResolvedYes` or `ResolvedNo` | valid oracle BIP-340 sig |
| `Trading` | `build_expire_transition_pset` | `Expired` | chain height ≥ `expiry_block_height` |
| `ResolvedYes \| ResolvedNo \| Expired` (outstanding > 0) | `build_redemption_pset` | same variant, outstanding decremented (terminal if 0) | outstanding > 0 |

All other (builder, state) pairs return `InvalidContractState { kind: WrongVariant { ... } }`. Terminal states (`outstanding_pairs == 0` on any non-`Trading` variant) admit no further transitions.

### Multi-Outcome Market transitions

| From state | Valid builder | To state | Condition |
|---|---|---|---|
| `Trading` | `build_issuance_pset(outcome)` | `Trading` (supply[outcome] +p) | — |
| `Trading` | `build_split_yes_pset` \| `build_split_no_pset` | `Trading` (all supplies +s) | — |
| `Trading` | `build_merge_yes_pset` | `Trading` (all yes supplies −s) | all `supplies[k].yes ≥ s` |
| `Trading` | `build_merge_no_pset` | `Trading` (all no supplies −s) | all `supplies[k].no ≥ s` |
| `Trading` | `build_oracle_resolve_pset(k)` | `Resolved { winning_outcome: k, ... }` | valid oracle sig |
| `Trading` | `build_expire_transition_pset` | `Expired` | chain height ≥ expiry |
| `Resolved \| Expired` (unredeemed > 0) | `build_redemption_pset` | same variant, `collateral_unredeemed` decremented (terminal if 0) | collateral_unredeemed > 0 |

Cross-outcome swap is not a builder in v1 (it's a `CrossOutcomeSwap` transition classification for observed txs, and v2 gets a dedicated arb quote/build API). See [Future: Cross-Outcome Arb API (v2)](#future-cross-outcome-arb-api-v2).

### LMSR Pool transitions

| From state | Valid builder | To state | Condition |
|---|---|---|---|
| `Active` | (public pool path via `engine.build_trade_pset`; plain or market-assisted) | `Active` (new s_index, new reserves) | s_index within table, reserves ≥ MIN_POOL_RESERVE |
| `Active` | `build_adjust_pset` | `Active` (new reserves, same s_index) | admin key signature, non-zero delta |
| `Active` | `build_close_pset` | `Closed { final_txid }` | admin key signature |

Pool operations remain valid regardless of parent market state — see [Pool and Order Lifecycle at Market Resolution](#pool-and-order-lifecycle-at-market-resolution). Market-assisted pool legs disappear once the parent market no longer supports issuance/cancellation, but plain pool trading and admin operations remain callable. Closed pools admit no further transitions.

### Order transitions

| From state | Valid builder | To state | Condition |
|---|---|---|---|
| `Active` | (fill via `engine.build_trade_pset`) | `Active` (partial) or `Consumed` (full) | sufficient remaining liquidity |
| `Active` | `build_cancel_pset` | `Cancelled` | maker key signature |

`Consumed` and `Cancelled` are terminal. Under `tracking: EphemeralFresh` or `EphemeralMidLife`, the engine auto-untracks the order past finality via `prune_finalized` (see [OrderState](#orderstate) for details).

### Error reporting for matrix violations

Every (builder, invalid-state) pair returns `CoreError::InvalidContractState { contract_id, kind }` where:
- `InvalidStateKind::WrongVariant { expected, actual }` — the state variant itself is wrong for this builder (e.g., `build_issuance_pset` on `ResolvedYes`).
- `InvalidStateKind::ConditionFailed { condition, detail }` — state variant is fine but a runtime precondition failed (e.g., `build_cancellation_pset(Some(Δ))` with Δ > outstanding; `build_expire_transition_pset` before the timelock height).

Callers can pattern-match on the kind to distinguish "fundamentally wrong call" from "temporarily unmet condition" for UX purposes.

## Thread Safety

Write methods on the engine (`step`, `ingest_market`, `ingest_pool`, `ingest_persistent_order`, `ingest_ephemeral_order`, `untrack_contract`, `rollback_to_height`, `prune_finalized`) take `&mut self`. Read methods on the engine (`interpret_transaction`, `identify_asset`, `contract`, `list_markets`, `list_pools`, `list_orders`, `market`, `pool`, `order`, `quote_trade`, and all engine-level PSET builders — creation and trade) take `&self`. View types (`Market`, `MultiOutcomeMarket`, `Pool`, `Order`) are constructed from `&self` engine methods and hold `&'a ContractEngine<S>` internally — all their methods (state accessors, PSET builders, oracle helpers, relationship queries, history) are effectively `&self` reads as far as the engine is concerned. While any view is alive, the engine cannot be mutably borrowed.

Rust's borrow rules provide compile-time `RwLock` semantics: multiple concurrent readers OR one exclusive writer, enforced without runtime overhead. For single-threaded consumers this is invisible. For multi-threaded consumers who need concurrent access, wrap the engine in `RwLock<ContractEngine<S>>`:

```rust
let mut chain = EsploraChainSource::new("...");
let engine = Arc::new(RwLock::new(ContractEngine::new(store, Network::Liquid)));

// Writer thread (sync)
let engine_w = engine.clone();
std::thread::spawn(move || {
    loop {
        engine_w.write().unwrap().step(&mut chain).unwrap();
        std::thread::sleep(Duration::from_secs(60));
    }
});

// Reader thread (can run concurrently with other readers)
let interpretations = engine.read().unwrap().interpret_transaction(&some_tx);
```

Core does not add `Send` or `Sync` bounds on the `ContractStore` trait. If a store implementation is `Send`, `ContractEngine<S>` is automatically `Send`. This lets single-threaded consumers use non-thread-safe stores without penalty, while multi-threaded consumers choose thread-safe implementations.

## LMSR Math

Core provides pure LMSR computation functions for pricing, quoting, and table generation. See [lmsr-pool-design.md](../contracts/lmsr-pool/lmsr-pool-design.md) for the full pool design, parameter simplification rationale, and Merkle-committed curve approach.

The key functions live in the `deadcat-core` LMSR math module:

- `fee_free_yes_spot_price_bps(manifest, params, s_index)` — implied probability at a given state
- `quote_from_table(trade_kind, old_s_index, new_s_index, ...)` — deterministic quote from F-value table lookup
- `quote_exact_input_from_manifest(manifest, params, trade_kind, s_index, input)` — best trade for a given input amount
- `generate_lmsr_table(b, half_payout_sats, q_step_lots)` — deterministic bignum F-value generation (see [Deterministic Table Generation](../contracts/lmsr-pool/lmsr-pool-design.md#deterministic-table-generation) and the authoritative [lmsr-deterministic-table-spec.md](../contracts/lmsr-pool/lmsr-deterministic-table-spec.md))
- `lmsr_table_root(values)` — Merkle root from table values

Types: `LmsrTradeKind` (BuyYes, SellYes, BuyNo, SellNo), `LmsrQuote` (full trade result with reserve deltas), `LmsrTableManifest` (in-memory table: depth + F-values vector).

These have zero dependencies beyond basic math — no wallet, chain, or state. All functions that require `b` derive it internally from `LmsrPoolParams.max_loss_sats` — callers never provide `b` directly.

**Runtime model: cached full tables**: `quote_trade` and the build/ingest paths all consume the same deterministic table output. `deadcat-core` maintains an in-memory cache of full F-value tables keyed by `(max_loss_sats, half_payout_sats)`. The first use of a combo incurs the bignum cold-start cost (~5-10s); subsequent operations — quoting, Merkle proof generation, and ingestion verification — are O(1) lookups against the cached table. The router combines those cached curve lookups with live pool state: `s_index` determines where the pool sits on the curve, while current reserves determine how much volume is still fillable before a reserve floor is hit.

The authoritative deterministic algorithm and Merkle format are specified in [lmsr-deterministic-table-spec.md](../contracts/lmsr-pool/lmsr-deterministic-table-spec.md). The liquidity parameter `b` is derived from `LmsrPoolParams.max_loss_sats` via `b = max_loss_sats / ln(2)` at bignum precision. All LMSR functions that need `b` derive it from the stored `max_loss_sats` — it is never stored or passed separately.

## Key Derivation Convenience Functions

`derive_order_params` and `derive_pool_params` are standalone functions (not engine methods) that accept the deadcat xprv (`elements::bitcoin::bip32::Xpriv` at HD path `m/86'/1145258324'`) and encapsulate all key derivation, nonce computation, and index masking internally:

```rust
pub fn derive_order_params(
    deadcat_xprv: &Xpriv,
    market_params: &MarketParams,        // umbrella: binary or multi-outcome
    outcome: OutcomeIndex,                // which outcome's YES/NO pair the order offers
    order_index: u16,
    side: Side, direction: OrderDirection,
    price: u64, min_fill_lots: u8, min_remainder_lots: u8,
) -> Result<(MakerOrderParams, u16 /* masked_index */), ConventionError>;

pub fn derive_pool_params(
    deadcat_xprv: &Xpriv,
    market_params: &MarketParams,        // umbrella: binary or multi-outcome
    outcome: OutcomeIndex,                // which outcome's YES/NO pair the pool serves
    pool_index: u16,
    max_loss_sats: u64, half_payout_sats: u64, fee_bps: u16,
    initial_s_index: u16,                 // from estimate_bootstrap (creation) or hint (recovery)
) -> Result<(LmsrPoolParams, u16 /* masked_index */), ConventionError>;
```

Both functions validate OP_RETURN convention constraints before deriving parameters, returning `ConventionError` if the inputs cannot be losslessly encoded in the recovery hint. `derive_order_params` validates: `price <= 0xFFFFFF` (u24), `min_fill_lots >= 1`, `min_remainder_lots >= 1`. `derive_pool_params` validates: `max_loss_sats` and `half_payout_sats` in the 16-value 1-2-5 table (shared with market `base_payout` encoding), `fee_bps <= 4095` (u12), `initial_s_index` in `[0, 65535]` with the constraint that the resulting implied YES price is in `(0, 10000)` bps exclusive (0% and 100% starting prices are rejected). `ConventionError` is a simple error type (separate from `CoreError`) with a descriptive message indicating which constraint was violated.

**`initial_s_index` sourcing**: `derive_pool_params` takes `initial_s_index` directly — it is not computed from `starting_price_bps` internally. The UI flow obtains `initial_s_index` from `estimate_bootstrap`, which takes `starting_price_bps` and returns the nearest valid `initial_s_index` (see [estimate_bootstrap](../contracts/lmsr-pool/lmsr-pool-design.md)). On recovery, `initial_s_index` is read directly from the pool OP_RETURN hint and passed through — no inverse conversion needed. The snap function (`bps → s_index`) lives in exactly one place (`estimate_bootstrap`); `build_lmsr_bootstrap_pset` and `derive_pool_params` consume the snapped index as-is. This eliminates the earlier three-way coupling surface where divergence between forward and inverse snap implementations could silently break recovery.

Internally, each function derives from the xprv:
- **`deadcat_secret_key`** at `m/86'/1145258324'/secret'` — a single key used for all HMAC operations (nonce derivation, index masking). Different HMAC tags (`"deadcat/order_nonce"`, `"deadcat/order_mask"`, `"deadcat/pool_mask"`) provide full domain separation.
- **Per-instance public key** at `m/86'/1145258324'/orders'/i` or `m/86'/1145258324'/pools'/i` — the `maker_pubkey` or `admin_pubkey` baked into the covenant.

The functions are standalone (not engine methods) and stateless — the xprv is passed in, child keys are derived, public parameters are extracted, and all private key material is dropped on return. The engine never touches private keys; only these two convenience functions do.

**Why core accepts private key material here**: HD derivation is pure computation (HMAC-SHA512 + secp256k1 point multiplication) with no IO, state, or signing — the same category as Simplicity compilation and taproot tree construction. Encapsulating the derivation makes the internal HD path structure (`secret'`, `orders'/i`, `pools'/i`) an implementation detail rather than a public interoperability standard. The derivation spec is documented publicly in [chain-only-recovery.md](../protocol/chain-only-recovery.md) for independent audit and cross-language implementations, but Rust integrators can use these functions directly.

**Hardened derivation blast radius**: All paths use hardened derivation (`'`). If the deadcat xprv is compromised, only deadcat-related keys are affected — the wallet's Bitcoin, L-BTC, and other non-deadcat keys are unreachable from the deadcat subtree.

## Sync Patterns and Discovery

Sync is handled by the engine via `step` — the caller's only responsibility is calling `step` with a `ChainSource` implementation. This section describes aspects of sync that are visible to the caller: ingestion choices that affect what history is available, and discovery payload shapes.

### Per-Persona Ingestion Tables

The caller's ingestion choice (Creation vs Current snapshot) determines what history is available. `step` handles the sync strategy internally regardless of the choice.

**Pool ingestion by persona:**

| Persona | Ingestion | History available? |
| ------- | --------- | --------------- |
| Trader (taker) | `PoolSnapshot::Current` | No — only current price matters |
| Pool operator (maker) | `PoolSnapshot::Creation` | Yes — fee revenue, adjustment audit |
| Price chart viewer | `PoolSnapshot::Creation` | Yes — full price history |

**Order ingestion by persona:**

| Persona | Ingestion | History available? |
| ------- | --------- | --------------- |
| Taker | `OrderSnapshot::Current` | No — only current fill level matters |
| Maker (monitoring) | `OrderSnapshot::Creation` | Yes — fill-by-fill history |
| Maker (recovery) | `OrderSnapshot::Creation` | Yes — full fill history |

### Discovery Payload Shapes

These are illustrative — core doesn't own discovery. The shapes show what data the API is designed to consume:

```rust
// Pool discovery
struct PoolDiscoveryPayload {
    params: LmsrPoolParams,
    creation_txid: Txid,
    // Optional acceleration:
    snapshot: Option<PoolCurrentState>,     // for non-initial ingestion
    transition_txids: Option<Vec<Txid>>,    // for batched forward-sync
}

// Order discovery
struct OrderDiscoveryPayload {
    params: MakerOrderParams,
    creation_txid: Txid,
    snapshot: Option<OrderCurrentState>,    // for non-initial ingestion (takers)
}
```

Note: Market discovery payloads include `BinaryMarketParams` + `creation_txid`. Markets always use creation-tx ingestion, so no snapshot is needed.

### Untrack + Re-Ingest Promotion

A contract ingested with a `Current` snapshot (no history) can be promoted to full-history mode:

1. Call `untrack_contract` to remove the contract and all derived data
2. Re-ingest with `PoolSnapshot::Creation` or `OrderSnapshot::Creation`
3. Call `step` — the engine forward-syncs from creation to rebuild full history

This is useful when a trader initially ingested a pool for quick trading (non-initial) and later wants price history (e.g., for charting).

## Wallet Recovery

When a wallet is restored from a mnemonic, Deadcat positions need to be rediscovered. Fund recovery is **fully mnemonic-driven and stateless** — all three contract creation transactions (markets, pools, orders) embed OP_RETURN recovery hints that, combined with the mnemonic and chain data, enable complete reconstruction of contract params without any external services. Pure token holders (takers who only traded through existing pools) recover via Elements asset issuance indexing — tracing the token's asset ID back to the market creation transaction. Only human-readable contract metadata (e.g., "Will BTC hit $200k by 2027?") requires the discovery layer — the cryptographic parameters that control funds are all recoverable from the chain.

**Convention enforcement** operates at three layers, each catching violations at a different point in the workflow:

| Layer | Enforcement point | What it catches |
|---|---|---|
| Derive functions | `derive_order_params`, `derive_pool_params` | Convention violations at param construction time (`ConventionError`; first line — best error UX) |
| PSET builders | All three creation builders | Convention violations for manually-constructed params (`CoreError::ConventionViolation`; defense in depth) |
| Ingestion | `ingest_market`, `ingest_pool`, `ingest_persistent_order`, `ingest_ephemeral_order` | Strict-canonical tracking boundary: reject non-conforming supplied params for any tracked contract; `Creation` snapshots additionally verify the creation tx against those params |

`deadcat-core` adopts a **strict-canonical tracking policy**: if the engine agrees to track a contract, the supplied params must conform to the published v1 recovery conventions. This avoids a mixed universe of "tracked but foreign" contracts whose recovery or downstream UX semantics differ from the canonical path. Markets remain the strongest case because every token holder traces back to the market creation tx, but the same policy is applied to pools and orders for API consistency and easier reasoning. The one remaining trust trade-off is `PoolSnapshot::Current` / `OrderSnapshot::Current`: because those variants intentionally omit the creation transaction, they cannot prove that the historical on-chain hint was present. They still reject non-conforming supplied params and require a canonical parent market, but the omitted creation-time proof remains the caller's responsibility. See [chain-only-recovery.md](../protocol/chain-only-recovery.md) for the full recovery specification.

**Wallet-funded prerequisite**: OP_RETURN recovery hints are found by scanning wallet-funded transactions. Token holder recovery uses `ChainSource::issuance_transaction` to trace asset IDs back to their creation transactions. Both paths are chain-only — no external services.

**OP_RETURN properties**: All recovery hints use zero-value OP_RETURN outputs, which are both consensus-valid and relay-standard on Elements. Hints use compressed encodings (standard denomination conventions, well-known asset indices, hybrid time encoding) to minimize size. The hints are always included; there is no opt-out. Derivation indices are XOR-masked for privacy. See [chain-only-recovery.md](../protocol/chain-only-recovery.md) for encoding details.

### YES/NO Token Positions

Token recovery is automatic. YES and NO tokens are standard Elements confidential assets held at the wallet's own addresses. The wallet's normal mnemonic-based rescan (gap-limit scan over derived scriptpubkeys) finds them the same way it finds L-BTC UTXOs. No deadcat-specific recovery logic is needed.

**Labeling and redemption** require market ingestion. The wallet discovers it holds a UTXO with an unfamiliar asset ID, but doesn't know it's a "YES token for market X" until the market's `BinaryMarketParams` are available and the market is ingested. The recovery path: `asset_id` → `ChainSource::issuance_transaction(asset_id)` → market creation tx → read OP_RETURN → reconstruct market params → `ingest_market` → `identify_asset` for labeling, `build_redemption_pset` for redemption. One chain query per unique asset ID. This works for **all** token holders, including pure takers who only traded through existing pools and never created any contracts. See [chain-only-recovery.md](../protocol/chain-only-recovery.md) for details.

### Prediction Market Positions

Markets have no on-chain "owner" — the taproot internal key is NUMS. However, the market creation builders include an OP_RETURN recovery hint in the market creation transaction. This serves two purposes: (1) enabling the market creator to re-discover and re-announce their market, and (2) providing the anchor for chain-only pool and order recovery — pool and order hints point to the market creation transaction by txid. It also enables token holder recovery: `issuance_transaction(asset_id)` traces any YES/NO token back to this transaction.

**37 bytes** (known collateral asset) / **69 bytes** (exotic collateral). Uses compressed encoding: 4-bit well-known collateral asset index (network policy asset = `0`, Liquid-mainnet USDt = `1`, escape = `15`), 4-bit 1-2-5 denomination convention for `base_payout`, and absolute `expiry_time` as u24 (block height divided by 60, giving hour-level granularity with range from the Liquid genesis block to approximately the year 3931). The builder accepts any future height, rounds `expiry_time` up to the next 60-block boundary, and commits that rounded value into the covenant params — making the encoding lossless. Only 4 of 8 `BinaryMarketParams` fields need encoding — the other 4 (token and RT asset IDs) are derivable from the creation transaction's issuance entropy. See [chain-only-recovery.md](../protocol/chain-only-recovery.md) for the exact byte layout, network-specific asset mapping, and per-field justification.

### Maker Order Positions

Maker order UTXOs are NOT at wallet addresses — they're at covenant addresses (taproot scripts derived from the Simplicity covenant with order params baked in). The wallet's standard rescan does not find them. The maker's key is derived from the mnemonic, but the covenant script depends on the full order params (market, price, direction, deterministic nonce). Without the params, the output key cannot be computed.

Maker orders are the only contract type directly "owned" by regular end users (as opposed to markets and pools, which are created by operators in managerial roles). This makes mnemonic-only recovery especially important — regular users should not be expected to maintain stateful backups.

**40 bytes**. Includes: XOR-masked derivation index (for O(1) key recovery + observer privacy), market creation txid (chain-only market param recovery), price (u24, bounded by `collateral_per_pair`), min_fill_lots and min_remainder_lots (u8 each, range 1-255), and side + direction packed into the type tag byte. The `derive_order_params` function encapsulates all key derivation — callers pass the deadcat xprv + `order_index`, and the maker pubkey, canonical nonce, and masked index are computed internally (see [Key Derivation Convenience Functions](#key-derivation-convenience-functions)).

The builder validates: `price <= 0xFFFFFF` (u24 max, 16,777,215), `min_fill_lots` and `min_remainder_lots` in range 1-255, `order_index <= 65535`, and parent market conforms to conventions. See [chain-only-recovery.md](../protocol/chain-only-recovery.md) for the exact byte layout, per-field inclusion/compression justification, recovery flow, and XOR masking specification.

### LMSR Pool Positions

Like maker orders, pool reserve UTXOs are at covenant addresses — the wallet's standard rescan does not find them. The operator derives their admin key from the mnemonic, but the admin pubkey alone is insufficient to find the pool on-chain — the covenant scripts also depend on liquidity parameters and the s_index (which changes on every swap, making script enumeration impractical).

**40 bytes**. Uses compressed encoding: `max_loss_sats` and `half_payout_sats` as 4-bit 1-2-5 table indices each (shared with the market `base_payout` encoding, range 100 to 10,000,000 sats), `fee_bps` as u12 (0.01% granularity, max 40.95%), `initial_s_index` as u16 (the starting table index — enables direct script verification during recovery without reverse-deriving from reserves), plus XOR-masked pool operator derivation index. All other covenant params are derived: `b` from `max_loss_sats`, `q_step_lots` from `b` and `half_payout_sats`, `lmsr_table_root` from deterministic F-value generation, token asset IDs from the parent market, admin pubkey from the mnemonic at `pool_index`. Protocol constants (`TABLE_DEPTH`, `S_BIAS`, `S_MAX_INDEX`, `MIN_POOL_RESERVE`) require no encoding. See [chain-only-recovery.md](../protocol/chain-only-recovery.md) for the exact byte layout, per-field justification, and recovery flow.

### Oracle Market Discovery

An oracle derives their key from the mnemonic and re-discovers markets referencing that key via Nostr (filtering market announcements by `oracle_public_key`), or by scanning market OP_RETURN hints in wallet-funded transactions for their oracle pubkey. The oracle's role is limited to resolution — a managerial role where maintaining backups of market params is a reasonable expectation.

### Cost Amortization

Market and pool creations are infrequent lifecycle events — the OP_RETURN cost (37-40 bytes at typical Liquid fee rates, or up to 69 bytes for markets with exotic collateral) is paid once and amortized over the entire lifetime of the contract (every trade, fill, adjustment, and redemption that follows). Maker order creation is the most frequent user-facing operation with an OP_RETURN, but the cost is negligible relative to the order value and trade fees. The OP_RETURN cost is never paid by market takers or regular traders — only by contract creators.

### Recovery Summary

| Position | Recovery mechanism | Hint size | Chain-only? |
|---|---|---|---|
| YES/NO tokens | Standard wallet rescan + `issuance_transaction` for labeling/redemption | — | Yes |
| Prediction markets | OP_RETURN in creation tx | 37 bytes (known asset) / 69 bytes (exotic) | Yes |
| Maker orders | OP_RETURN in creation tx → market hint chain | 40 bytes | Yes |
| LMSR pools | OP_RETURN in creation tx → market hint chain | 40 bytes | Yes |

All user types — market creators, order makers, pool operators, and pure token holders — achieve chain-only recovery. Discovery (Nostr) is only needed for human-readable metadata, not fund recovery.

**Core's role in recovery**: Core provides `identify_asset` for token labeling, `derive_order_params` and `derive_pool_params` for deterministic param reconstruction (both accept the deadcat xprv and encapsulate all key derivation, nonce computation, and index masking internally — see [Key Derivation Convenience Functions](#key-derivation-convenience-functions)), Simplicity compilation for contract verification, `ingest_*` for re-tracking, and convention enforcement (builder validation + strict-canonical ingestion rejection of non-conforming supplied params). The `ChainSource::issuance_transaction` method enables token holder recovery. See [chain-only-recovery.md](../protocol/chain-only-recovery.md) for the complete specification.

## Example Integration: Aqua Wallet

```rust
use deadcat_core::{
    ContractEngine, MarketParams, BinaryMarketParams, FeeRate, WalletFunding,
    Network, Pagination, StateFilter, Side, OutcomeIndex,
    TradeSpec, TradeDirection, TradeAmount,
};

// 1. Initialize engine with a store implementation and network.
//    The store is exclusively owned by the engine from this point.
//    Construction is O(1) — no iteration, no compilation.
let mut engine = ContractEngine::new(aqua_deadcat_store, Network::Liquid);

// 2. Set up the chain source (Aqua uses Esplora)
let mut chain = EsploraChainSource::new("https://blockstream.info/liquid/api");

// 3. Ingest a market (discovered via Nostr, import, etc.)
//    `ingest_market` takes a reference to the umbrella `MarketParams` enum
//    (Binary(..) or MultiOutcome(..)).
//    No anchor needed — core derives blinding factors deterministically.
//    Core compiles the contract, verifies the creation tx, and indexes asset IDs + scripts.
//    Returns ContractId (CMR + creation_txid).
let market_params = MarketParams::Binary(binary_market_params);
let market_id = engine.ingest_market(&market_params, &creation_tx)?;

// 4. Sync — step handles catch-up and subscription setup automatically.
//    All contracts (including the just-ingested market) are brought to the chain tip.
engine.step(&mut chain)?;

// 5. Ongoing sync loop — call step periodically or on block notifications.
loop {
    let report = engine.step(&mut chain)?;
    for tx in &report.transactions {
        for t in &tx.interpretation.transitions {
            log::info!("Contract {:?} transitioned at block {}", t.contract_id, tx.position.block_height);
        }
    }
    sleep(Duration::from_secs(60));
}

// 6. Pending UX: interpret unconfirmed mempool transactions (read-only)
if let Some(mempool_tx) = aqua_chain.get_mempool_tx(txid) {
    let result = engine.interpret_transaction(&mempool_tx)?;
    for t in &result.transitions {
        render_pending_label(&t.details);
    }
}

// 7. Label wallet history (read-only, can be called anytime)
for wallet_tx in wallet_history {
    let result = engine.interpret_transaction(&wallet_tx)?;
    for output in &result.external_outputs {
        if output.index == my_utxo_index {
            render_label(&output.role);
        }
    }
}

// 8. Identify assets in wallet balance (delegates to store's asset index)
for (asset_id, amount) in wallet_balance {
    if let Some(info) = engine.identify_asset(&asset_id)? {
        render_token_position(&info, amount);
    }
}

// 9. Browse markets with pagination
let page = engine.list_markets(
    StateFilter::ActiveOnly,
    Pagination { after: None, limit: 20 },
)?;
for entry in &page.items {
    render_market_card(&entry.contract_id, &entry.params, &entry.state);
}

// 10. Deduplicate during discovery (full ContractId check — discovery payloads include creation_txid)
let cmr = contract_cmr(
    &ContractParams::Market(MarketParams::Binary(announced_params.clone())),
    Network::Liquid,
);
let contract_id = ContractId { cmr, creation_txid: announced_creation_txid };
if engine.contract(&contract_id)?.is_some() { continue; }

// 11. Trade: two-step quote + build (engine handles routing, coin selection, fee computation)
//     TradeSpec identifies (outcome, side). For binary markets, outcome is OutcomeIndex::BINARY.
let spec = TradeSpec {
    outcome: OutcomeIndex::BINARY,
    side: Side::Yes,
    direction: TradeDirection::Buy,
    amount: TradeAmount::ExactInput(5000),
};
let fee_rate = FeeRate::from_sat_per_vb(aqua_chain.estimate_fee_rate());
let quote = engine.quote_trade(&market_id, spec, fee_rate)?;
if user_confirms(&quote) {
    let funding = WalletFunding {
        available_utxos: &aqua_wallet.list_utxos(),
        fee_rate,
        return_script: &aqua_wallet.next_return_script(),
    };
    let pset = engine.build_trade_pset(&quote, &funding)?;
    let signed = aqua_signer.sign(pset)?;
    aqua_chain.broadcast(signed)?;
}

// 12. Build an RT-involving transaction through the Market view.
//     engine.market(id) returns a Market view caching the contract's (params, state).
//     build_issuance_pset lives on the view; it takes `outcome: OutcomeIndex` (BINARY for binary markets).
let funding = WalletFunding {
    available_utxos: &aqua_wallet.list_utxos(),
    fee_rate: FeeRate::from_sat_per_vb(aqua_chain.estimate_fee_rate()),
    return_script: &aqua_wallet.next_return_script(),
};
let market = engine.market(&market_id)?.expect("market tracked");
let unblinded = market.build_issuance_pset(
    OutcomeIndex::BINARY,
    100,
    &token_dest,
    &token_dest,
    &funding,
)?;
let prepared = unblinded.prepare(&aqua_wallet.blinding_pubkey())?;
prepared.pset.blind_last(&mut rng, &secp, &prepared.input_secrets)?;
let signed = aqua_signer.sign(prepared.pset)?;
aqua_chain.broadcast(signed)?;
```

## Testing Strategy

`deadcat-core` is a pure computation library with no IO, making it highly testable without external infrastructure. The testing strategy uses five tiers, each targeting a specific correctness property at the lowest possible cost. The goal: comprehensive coverage in under 3 minutes total, with regtest reserved for a handful of end-to-end smoke tests.

### Tier 1: Pure Function Tests

**What**: Every standalone function and deterministic computation — LMSR math (table generation, cached-table quoting, spot-price helpers), key derivation (`derive_order_params`, `derive_pool_params`), oracle attestation messages, OP_RETURN encoding/decoding (byte layout round-trips), expiry time snapping (block height → u24 → block height), XOR index masking/unmasking, CBF derivation chain, `BootstrapEstimate` computation, `FeeRate` conversions, pagination cursor encoding.

**How**: Standard `#[test]` functions, zero dependencies beyond core itself. Property-based tests (e.g., `proptest`) for encoding invariants — "for any valid params, `decode(encode(params)) == params`" generates thousands of random inputs and provides much stronger guarantees than hand-picked examples. Particularly valuable for OP_RETURN round-trips, LMSR quote/proof consistency against the cached tables, and XOR masking/unmasking.

**Speed**: Instant (<1ms per test, hundreds of tests).

### Tier 2: State Machine Tests

**What**: The `ContractEngine` state machine — feed synthetic `ChainTransaction`s and verify state transitions, `StepReport` contents, and error handling.

**How**: In-memory `ContractStore` implementation (mock store). Mock `ChainSource` that returns pre-constructed transactions. The key pattern is **builders as test data generators**: use PSET builders to produce structurally valid transactions, extract the unsigned transaction, wrap it as a `ChainTransaction` with a synthetic `ChainPosition`, and feed it to `process_transaction` via `step`. This creates a self-reinforcing loop — the builders generate test data for the state machine, and the state machine validates the builders' output.

**Coverage**:
- All market transitions: Trading → issuance → resolution → redemption (terminal); Trading → expiry → redemption; Trading → cancellation; dormant terminal paths
- All pool transitions: Active → swap → admin adjust → close
- All order transitions: Active → partial fill → complete fill; Active → cancel
- Multi-contract transactions (routed trades affecting pool + order)
- Reorg handling: rollback, re-sync, contract removal
- Idempotent processing (same tx twice = no-op)
- Edge cases: ingestion ordering constraints, parent market required, duplicate ingestion error, stale quotes
- `interpret_transaction` (read-only) vs `process_transaction` (durable) consistency

**Speed**: Instant (<10ms per test, no IO).

### Tier 3: PSET Builder Tests

**What**: Structural correctness of built PSETs — output scripts, values, OP_RETURN encoding, coin selection, fee computation, RT blinding, error cases.

**How**: Build PSETs from mock `WalletFunding` inputs and inspect the resulting PSET structure. No broadcasting, no signing, no chain. Verify: correct covenant script pubkeys on outputs, correct collateral/reserve amounts, OP_RETURN present with decodable content, coin selection chose appropriate UTXOs, fee matches `weight × rate`, `UnblindedPset` → `prepare()`/`finalize()` produces valid PSET structure.

**Error case coverage**: `InsufficientFunds` (with correct `shortfalls`), `InvalidParams` (non-encodable params, out-of-range values), `InvalidContractState` (wrong state for operation), `StaleQuote` (outpoints changed), fee rate mismatch on `build_trade_pset`.

**Speed**: Fast (<100ms per test — includes Simplicity compilation for script derivation).

### Tier 4: Covenant Execution Tests

**What**: Verify that built PSETs produce valid Simplicity witnesses that the covenant accepts, and that invalid transactions are rejected.

**How**: Uses Simplicity's `ElementsEnv` — a mock transaction execution context that runs the full Simplicity program through a `BitMachine` with real C-backed cryptographic jets. The existing SDK has extensive infrastructure for this pattern (see `src-tauri/crates/deadcat-sdk/tests/elements_env_execution.rs` and `src/testing.rs`).

**What `ElementsEnv` validates**:
- Full Simplicity program logic (every combinator node evaluated)
- BIP-340 signature verification (real libsecp256k1 via C FFI — oracle and admin signatures cryptographically verified)
- Output/input introspection (scripts, assets, values, issuance data — all jets read from real transaction data)
- Pedersen commitment math (the contract's `verify_token_commitment` runs against actual output commitments)
- Locktime/timelock checks
- All assertion failures (wrong witnesses, wrong scripts, wrong amounts → jet failure)

**What `ElementsEnv` does NOT validate** (deferred to Tier 5):
- Value balance (outputs ≤ inputs + fee) — you can construct an inflating transaction and the BitMachine won't notice
- Range proofs and surjection proofs — serialized but not verified
- Taproot control block verification — the program runs directly without the outer script interpreter
- Serialization roundtrip — the BitMachine operates on in-memory `RedeemNode`, not serialized bytes

**Critical test cases**:
- Each covenant spend path (all market, pool, and order transitions)
- **Deterministic RT blinding enforcement**: construct transactions with non-deterministic ABFs → covenant rejects; wrong CBF (not passed through) → covenant rejects. These directly test the griefing prevention.
- Negative tests for every spend path (wrong signature, wrong collateral, wrong script, inflation attempt)
- The `ElementsEnv` is constructed per-input — multi-covenant-input transactions require one execution per covenant input

**Speed**: ~1-2 seconds per test (dominated by Simplicity compilation + BitMachine execution). Dev-dependency on `simplicity-sys` with `test-utils` feature for the C FFI jets.

### Tier 5: Regtest Smoke Tests

**What**: End-to-end validation that PSETs built by core are actually valid when broadcast to a real Elements chain.

**How**: Requires `elementsd` (Liquid regtest node) + `electrs` (Electrum indexer). Build PSET → sign → broadcast → mine block → verify confirmation → sync engine → verify state. A small number of tests covering the gaps that `ElementsEnv` cannot validate: value balance, range/surjection proof validity, taproot verification, serialization roundtrip, genesis hash correctness.

**Coverage** (minimal — everything else is covered by Tiers 1-4):
- One full market lifecycle: create → issue → trade → resolve → redeem
- One pool lifecycle: bootstrap → swap → admin adjust → close
- One order lifecycle: create → partial fill → complete fill (or cancel)
- RT blinding round-trip: create with deterministic blinding → issue → verify reissuance works with derived ABFs
- OP_RETURN RT burn: create market → resolve → verify blinded OP_RETURN RT burn output is consensus-valid and accepted by the node

**Speed**: ~10-30 seconds per test (node startup, block mining, electrum sync with retry). ~60-90 seconds total for the minimal set.

### Test Infrastructure

**Mock `ContractStore`**: In-memory implementation using `HashMap`s. Supports all trait methods. No persistence, no database. Used by Tiers 2 and 3.

**Mock `ChainSource`**: Pre-loaded with transactions, returns them on demand. Supports both pull queries (`transactions_by_scripts`, `spending_transaction`) and push notifications (`register_scripts` → `drain_notifications`). Used by Tier 2.

**`testing` module**: Core should include a `pub(crate)` testing module with helpers for constructing synthetic transactions, mock UTXOs, standard test params, and pre-computed test data. The SDK's existing `testing.rs` (19KB) provides a good template, adapted for core's types.

### Test Count Estimates

| Tier | Est. count | Est. time | What it proves |
|---|---|---|---|
| Pure functions | 200+ | <5s | Math, derivation, encoding correct |
| State machine | 50+ | <10s | All transitions work for all tx shapes |
| PSET builders | 30+ | <30s | PSETs structurally correct, error cases handled |
| Covenant execution | 15-20 | <30s | Covenants accept valid txs, reject invalid ones |
| Regtest smoke | 3-5 | ~60-90s | Real chain accepts built transactions |

**Total: ~300+ tests in under 3 minutes**, with the bulk of correctness confidence coming from Tiers 1-4 (no external processes, <1 minute). Regtest tests are a safety net, not the primary correctness mechanism.

## Security Model

### Trust Assumptions

The protocol has two external trust points. Integrators should understand these before building on `deadcat-core`.

| Assumption | Impact if violated | Mitigation |
|---|---|---|
| **Oracle honesty** | A compromised oracle can resolve markets incorrectly — winning token holders lose their full-value redemption. | Token holders can wait for expiry (half-value redemption) as a fallback. Multi-oracle schemes (M-of-N signatures) would reduce single-point-of-failure risk but are out of scope for v1. |
| **Liquid federation honest block production** | Federation members could front-run trades (insert own transactions before user transactions), censor specific transactions, or reorder transactions within a block. | Standard Liquid trust model — not Deadcat-specific. No public mempool reduces the front-running surface compared to Bitcoin. |

Everything else is covenant-enforced — a malicious actor who modifies `deadcat-core` (or builds transactions manually) cannot violate the properties below.

### Covenant-Enforced Properties

Each row is a property the Simplicity covenants enforce on-chain. These also serve as the **negative test case spec for Tier 4 covenant execution tests** — each row should have a corresponding test that constructs the attack transaction and verifies the covenant rejects it.

| Property | Attack prevented | Enforced by |
|---|---|---|
| Collateral conservation on issuance | Issue tokens without providing sufficient collateral | Market covenant checks `collateral = pairs × collateral_per_pair` |
| Oracle-only resolution | Resolve a market without the oracle's signature | Market covenant verifies BIP-340 signature against `ORACLE_PUBLIC_KEY` |
| Correct redemption rates | Redeem at full value on an expired market (should be half) | Market covenant enforces half-value for Expired, full-value for ResolvedYes/ResolvedNo |
| Deterministic RT blinding | Grief a market by using non-deterministic blinding on RT outputs, locking it for all other participants | Market covenant enforces deterministic ABFs + CBF pass-through (see [deterministic-rt-blinding.md](../protocol/deterministic-rt-blinding.md)) |
| Swap pricing integrity | Get more tokens from a pool than the LMSR curve allows | Pool covenant verifies Merkle proofs for F(old_s) and F(new_s), enforces conservation equation with fee inequality |
| Pool reserve minimums | Drain a pool below minimum reserves via swap or admin adjustment | Pool covenant enforces `MIN_POOL_RESERVE` on all 3 reserves for swap and admin paths |
| Correct trade direction | Buy YES tokens while moving s_index down (getting a better price) | Pool covenant enforces `new_s_index > old_s_index` for buys, `<` for sells |
| Maker payment on order fill | Fill an order without paying the maker the correct amount in the correct asset | Order covenant checks `output[i].value >= fill_amount × PRICE` with asset verified against `QUOTE_ASSET_ID` (SellBase) or `BASE_ASSET_ID` (SellQuote) |
| Order remainder integrity | Steal the remainder of a partially-filled order | Order covenant checks remainder output has the order's covenant script and correct asset, with `min_remainder_lots` floor |
| Asset identity on all covenant outputs | Substitute one asset for another at the same value (e.g., replace YES tokens in a pool reserve with L-BTC) | All three covenants verify output asset IDs — pool: all 3 reserves, market: all paths, order: maker receive + remainder |
| Admin-only pool operations | Adjust or close a pool without the operator's key | Pool covenant requires BIP-340 signature from `ADMIN_PUBKEY` for admin and close paths |
| Maker-only order cancellation | Cancel someone else's order to steal their locked tokens | Order cancellation is taproot key-spend only — requires maker's private key |
| No double resolution | Resolve a market twice (once YES, once NO) to profit from both sides | Resolution consumes the RT UTXOs — no spend path exists from resolved state back to unresolved |
| Timelock-enforced expiry | Expire a market before the deadline to force half-value redemptions on winning token holders | Covenant checks `nLockTime >= expiry_time` (consensus-enforced — transaction cannot be included in a block before this height) |
| Correct issuance amounts | Mint more tokens than the collateral covers via the reissuance mechanism | Market covenant introspects `issuance_asset_amount` and validates against collateral |
| RT destruction on terminal transitions | Redirect RT tokens to a wallet address during resolution/expiry, then use the Elements reissuance mechanism to mint unbacked tokens (bypassing the covenant entirely) | Market covenant verifies RT burn outputs at the unspendable burn script with correct commitment (`ensure_blinded_reissuance_burn_output`) on all resolution and expiry paths. This is critical because deterministic blinding makes ABFs public — the traditional Elements safeguard (ABF secrecy) is absent, making covenant-enforced burns the sole protection against unauthorized reissuance. |
| Collateral UTXO authenticity (sibling check) | Create a fake collateral UTXO at the covenant script address (same script hash, tiny value), then co-spend it with the real RTs during issuance — orphaning the real collateral and making it inaccessible through normal resolution/redemption | All paths that co-spend RTs and collateral verify that all three covenant inputs share the same `prev_txid` (were created in the same transaction). Partial cancellation co-spends RTs to maintain this invariant. See [enforcement-layers.md](enforcement-layers.md). |

### Transaction Composability

Trade transactions co-spend multiple covenant inputs (LMSR pools + maker orders). Output aliasing — where two covenants both claim the same output — is prevented by two mechanisms: script uniqueness (different contracts produce different scripts) and structural separation (positional output references tied to input index). See [transaction-composability-model.md](transaction-composability-model.md) for the full analysis, output layout algorithm, and the proposed order covenant change that enables flexible multi-source trade transactions.

## Design Decisions Log

### Store Trait Relationship Queries Are Outcome-Scoped; View Adds All-Outcomes Companions

**Chosen**: the store trait's `pools_for_market`, `orders_for_market`, and `best_orders_for_market` all take `outcome: OutcomeIndex` as a required parameter. Callers must specify which outcome's YES/NO pair they want. Binary markets always pass `OutcomeIndex::BINARY`.

For the display case of "all pools/orders across all outcomes of a multi-outcome market," the `Market` view exposes `pools(filter, page)` and `orders(filter, page)` that iterate internally over `0..outcome_count` and merge results. Outcome-scoped companions `pools_for_outcome` and `orders_for_outcome` delegate directly to the store for a single outcome.

**Rejected**:
- **Store trait without `outcome`, engine filters in memory**: fine for pools (few per market) but wasteful for orders at scale (a 10-outcome market with thousands of orders per outcome would return tens of thousands of rows when routing wants 20-50). Store-level indexed filtering is much cheaper.
- **Two store trait variants (`pools_for_market` unscoped and `pools_for_outcome` scoped)**: doubles the trait surface. The unscoped variant is a thin wrapper around iteration; no real value in having both at the store layer.
- **`Option<OutcomeIndex>` parameter with `None` meaning all outcomes**: mixes two semantics in one method. Option A (required outcome) keeps each store method single-purpose; the view layer handles the all-outcomes aggregation where pagination logic naturally lives.

**Why**: scale pressure on orders drives this. Indexed `(market_id, outcome)` lookups at the store are fundamental to sustaining Polymarket-scale multi-outcome markets. Binary markets pay a syntactic tax of one extra parameter (`OutcomeIndex::BINARY`) but get the same behavior they would have had. The view-layer aggregation pattern (iterate outcomes for the all-outcomes case) is bounded (N ≤ 10 in practice) and lives in one place — the `Market` view — rather than being duplicated across consumers.

### `MultiOutcomeMarketTransition` Is a Classification of the Tx's Delta Shape

**Chosen**: `MultiOutcomeMarketTransition` variants name common delta-shape patterns (`IssuedPair`, `SplitYes`, `CrossOutcomeSwap`, etc.) plus a `Composite { delta_yes, delta_no, delta_collateral }` escape hatch for arbitrary solvency-preserving delta shapes that don't match a named pattern. Each market transaction produces exactly one variant (the covenant executes one spend path per tx — the generic solvency-preserving path — and the engine classifies the observed deltas).

**Rejected**:
- **Single-primitive-only variants** (as originally specified during Stage 3): assumed the covenant enumerated primitives (pair-issue, split-YES, merge-YES, split-NO, merge-NO) as separate spend paths, making each tx exactly one named primitive. That design has since been superseded by the generic solvency-preservation spend path (see [`multi-outcome-market-contract.md § Operations`](../contracts/multi-outcome/multi-outcome-market-contract.md#operations)), which accepts any `(Δy, Δn, Δc)` preserving the invariant. Under the generic path, a single tx can represent any composition of named primitives plus novel delta shapes, so `Composite` is required to represent compositions that don't match named classifications.
- **Bare `Composite` variant only, no named patterns**: loses display convenience. Consumers have to decode raw deltas to recognize common operations. Named variants provide ergonomic matching for the common cases; `Composite` catches the rest.

**Why**: matches the covenant's actual structure. The generic spend path makes single-tx compositions possible, and the classification enum reflects that: common patterns get named variants (wallet UI can match on `IssuedPair { outcome, pairs, ... }` directly), while arbitrary compositions get captured as `Composite` with raw deltas preserved. Multi-contract patterns (trades) remain detected at the `InterpretedTransaction` level via helper methods; those are orthogonal to per-contract transition classification. Cross-outcome arb classification (market + N pools atomic) is deferred to v2.

### Multi-Contract Patterns Detected at the Transaction Level

Multi-contract patterns — a single transaction co-spending multiple contracts atomically (trade: pool + LOB orders) — are surfaced via helper methods on `InterpretedTransaction` (`as_trade`, `net_effect_for`). Raw per-contract transitions remain available in `InterpretedTransaction.transitions` for consumers that want granular detail. Cross-outcome arb (market + N pools) is deferred to v2; see [Future: Cross-Outcome Arb API (v2)](#future-cross-outcome-arb-api-v2).

### Single-Transaction Interpretation (No Cross-Transaction Inference)

**Chosen**: the engine interprets each transaction independently. It does not pattern-match across a user's transaction history to recognize user-level behaviors that span multiple transactions (e.g., "these two txs together were a cross-outcome swap").

**Rejected**: cross-transaction inference in the engine. Higher-level tools (wallets, explorers, analytics) can aggregate across user tx history externally.

**Why**: the UTXO-following state machine processes one tx at a time. Each tx produces a complete interpretation given the current contract state. Cross-tx inference would require maintaining stateful heuristics (which sequences count as "semantically atomic" from the user's perspective?) and would pollute the core's otherwise mechanical interpretation logic. Keep the core mechanical; let higher layers add semantic aggregation.

### Liquidity-Weighted Probability; Single Value Per Outcome

**Chosen**: `Market::probability_bps(outcome)` returns a single liquidity-weighted probability in basis points (0..=10000) — weighted by each pool's `b` parameter (LMSR depth).

**Rejected**:
- Return separate YES and NO prices per outcome. Redundant: within any single LMSR pool, p_YES + p_NO = 10000 bps by construction, so per-outcome probability is a single number; the NO side's price derives as `10000 - probability_bps`.
- Return per-pool prices without aggregation. Available separately via the `Pool` view (`pool.params()` + `pool.state()`); the Market view exposes the canonical single value.
- Simple average (unweighted). Deep pools should dominate thin ones in the canonical "what does the market think?" number.
- Best-priced pool only. Biases toward outliers; a single thin pool at an extreme price would dominate despite low confidence.

**Why**: weighting by `b` matches intuition — a pool with 10× the subsidy is "10× more confident" in its price because it can absorb 10× the volume before moving significantly. Single-value-per-outcome collapses redundancy while remaining honest about cross-outcome coherence via `sum_of_probabilities_bps()` (expected to equal 10000; deviation surfaces arb opportunity).

### Cross-Outcome Arb: Deferred to v2

**Chosen**: defer the cross-outcome arb quote + build API (`quote_cross_outcome_arb`, `build_cross_outcome_arb_pset`, `ArbQuote`, `ArbDirection`, `ArbPoolLeg`, `as_cross_outcome_arb`) to v2. v1 ships multi-outcome markets (if B3 resolves that way) without a built-in arb builder; external bots can construct arb txs directly against the covenant's generic solvency-preservation spend path.

**Rejected**: shipping the full quote + build + classification surface in v1. The API design has several unresolved questions (scope of directions, sizing model, staleness representation, classification rule) and landing it prematurely would bake in choices before the arb ecosystem exists to inform them.

**Why**: cross-outcome arb is not safety-critical — coherence gaps are pricing drift, not solvency violations, and the covenant's invariants hold regardless of whether arb runs. The generic solvency-preservation spend path makes arb permissionless by construction, so external tooling can close gaps without a core-layer builder. The audience for arb is advanced actors (bots, keepers) who tolerate external tooling while v1 stabilizes. See [Future: Cross-Outcome Arb API (v2)](#future-cross-outcome-arb-api-v2) for the deferred surface and open design questions.

### Multi-Outcome `.simf` Code Generation

**Chosen**: Rust-based generator using MiniJinja templates, emitting one hand-committed `.simf` file per supported N. The generator lives in a separate `deadcat-codegen` workspace crate (dev-only). `deadcat-core` reads the committed files via `include_bytes!` and has no dependency on the generator or its templating library — downstream consumers of the published crate receive pre-embedded `.simf` files in their dep graph. Generator invocation is explicit (`just generate-simf`); drift detection runs as part of `cargo test` by regenerating in-memory, asserting byte-exact equality against committed files, and invoking the SimplicityHL compiler to verify the output is semantically valid. v1 supports N ∈ {3, 4}.

**Rejected**:
- **`build.rs`-driven regeneration**: cargo's conventional `OUT_DIR` target isn't committed; silently regenerating on every `cargo build` risks clobbering developer edits and hides the provenance of committed files.
- **Per-N CMR caching**: a reasonable-sounding optimization that doesn't work. CMR depends on the full param set (oracle pubkey, asset IDs, `base_payout`, `expiry_time`), so there is no stable "per-N CMR" to cache — every market instance has a distinct CMR. What we commit is `.simf` source text; drift is detected via byte-exact source match, not a CMR regression.
- **Handlebars, Tera, and plain-Rust string concatenation** as the templating approach: Handlebars lacks native range loops; Tera is functional but has a materially heavier dependency tree (regex, pest, etc.) than MiniJinja for no win in our use case; plain-Rust string building would work but loses the readability benefit of a template file where the N-dependent structure is visible.
- **Single parameterized `.simf` with N as witness**: Simplicity is a total language without general recursion, so loops over 2N inputs/outputs must be unrolled at compile time. N cannot be a runtime witness.
- **Hand-written `.simf` per N without codegen**: feasible at N=3, but each additional N added manually is new audit surface and risks divergence between files. Writing a small generator now is the cheaper long-term path.

**Why**: the generator + committed-output + drift-test pattern gives three wins together — inspectability (auditors review committed `.simf` text directly), zero-drift guarantee (CI catches any mismatch between generator and committed output), and clean crate separation (no runtime deps leak to downstream). Adding new N values in future releases is non-breaking — each N has its own CMR-committed program, so existing markets are unaffected. Shrinking the supported range is breaking and should not be done once markets are live. See [multi-outcome-market-contract.md § Code Generation Strategy](../contracts/multi-outcome/multi-outcome-market-contract.md#code-generation-strategy) for implementation-level detail (file layout, template structure, verification test).

### LMSR F-Value Computation: Bignum Runtime, Reference Merkle Roots as Fixtures

**Chosen**: `deadcat-core` computes F-values at runtime using arbitrary-precision bignum (`num-bigint` + `num-rational`) directly from the closed-form expression `F(i) = max_loss_sats + floor(b × ln(cosh(s/b)))`. Per-pool F-value tables are cached in memory (and optionally on disk) after first computation. `deadcat-codegen` hosts the reference bignum generator and a committed fixture file with the canonical Merkle root and anchor F-values for each of the 256 valid `(max_loss_sats, half_payout_sats)` parameter combinations. A regression test on every `cargo test` re-runs the bignum reference and asserts all 256 committed roots reproduce byte-for-byte. Pool denomination uses the 16-value 1-2-5 table shared with market `base_payout` encoding (4 bits per param, 256 combinations total).

**Rejected**:
- **Fixed-point Taylor series runtime**: faster (~100ms table gen, ~1μs point eval vs bignum's ~5–10s / ~100μs) but requires specifying a fixed-point representation (Q64.64 in u128), choosing a transcendental algorithm (Taylor + range reduction, CORDIC), committing precomputed irrational constants, writing a worked example, and maintaining correctness tests that assert Taylor matches bignum. Substantial spec surface area for marginal runtime-performance gains on operations (pool creation, first-ingest) that are infrequent relative to trading activity.
- **Embedding the full 65,536-entry F-value tables in the `deadcat-core` binary for all 256 param combos**: infeasible. 256 × 65,536 × 8 bytes = 128 MB uncompressed, ~20–30 MB with aggressive delta-varint compression. Rejected as library bloat.
- **Embedding just the 256 Merkle roots in `deadcat-core`**: 8 KB cost is trivial, but with bignum as the runtime algorithm the roots aren't needed at runtime — bignum always produces the correct value. Moving the roots to `deadcat-codegen` as test fixtures preserves the regression-protection and cross-implementation-conformance properties without paying any `deadcat-core` binary cost.
- **Hybrid (bignum compile-time for roots, Taylor runtime for F-values)**: reasonable long-term optimization but delays the v1 spec behind a second algorithm. Can be added as a non-breaking follow-up in a future release — the committed reference Merkle roots serve as the acceptance criterion for any alternative runtime implementation.
- **Pool denomination at 26-mantissa × 16-exponent (previous design)**: 416 values per param × 2 params = 173,056 combinations → 5.5 MB of Merkle root fixtures (if committed) or a much larger param space (if not). The 1-2-5 × 16-value table reduces the fixture set 675× while retaining adequate granularity for v1 pool sizes. Range is capped at 10^7 sats per param — expanding is non-breaking in a future release (adds new table entries without invalidating existing pools' Merkle roots).

**Why**: bignum-only runtime eliminates an entire class of spec complexity (fixed-point precision analysis, Taylor term-count bounds, precomputed transcendental constants, worked examples) at the cost of 5–10 seconds of cold-start compute per new pool parameter combo per user per install. That cost is amortized across subsequent trading on the pool (cached) and is paid at deliberate multi-step actions (pool creation, first-time pool ingestion). Correctness is structural: bignum is the reference, committed Merkle roots are the regression guard, and any future faster implementation can be validated against the exact same fixtures.

See [lmsr-deterministic-table-spec.md § F-Value Computation Algorithm](../contracts/lmsr-pool/lmsr-deterministic-table-spec.md#f-value-computation-algorithm) for the full algorithm specification and [chain-only-recovery.md § Pool Denomination](../protocol/chain-only-recovery.md#pool-denomination-1-2-5-table-4-bits-each) for the encoding.

### View Types for Per-Contract Operations

**Chosen**: `ContractEngine` exposes per-contract operations via view types (`Market<'a, S>`, `Pool<'a, S>`, `Order<'a, S>`, `MultiOutcomeMarket<'a, S>`) returned by engine accessors (`engine.market(id)`, etc.). Each view holds `&'a ContractEngine<S>` plus cached `(params, state)`. Per-contract PSET builders, state accessors, oracle helpers, and relationship queries live on the views; engine surface shrinks to ingestion, chain sync, discovery/listing, asset identification, transaction interpretation, creation builders, and trade routing.

**Rejected**:
- **Flat engine surface with `contract_id` on every method**: original design. 25+ methods on the engine, mostly disambiguated by `contract_id` as first argument. IDE autocomplete is noisy, operations are mixed across contract kinds, discoverability suffers, `contract_id` repeated at every call site.
- **Free functions with `&engine` parameter**: e.g. `build_issuance_pset(&engine, &contract_id, pairs, ...)`. Avoids the engine-method bloat but loses method-call ergonomics and bundled-context benefits.
- **Trait-based dispatch (e.g. `ContractOps::build_issuance_pset`)**: adds type machinery for minimal gain over direct `impl` blocks on concrete view structs.

**Why**: operations naturally cluster by the object they operate on. A `Market` view groups everything you can do with a market (issue, cancel, resolve, expire, redeem, query state, fetch pools, fetch orders, oracle attestation helpers, and for multi-outcome via `as_multi_outcome()`: split/merge YES/NO — with cross-outcome arb deferred to v2). This is idiomatic Rust API design (similar patterns in `std::fs::File`, `hyper::Client`, etc.) and it enables type-level dispatch for specializations (`MultiOutcomeMarket` only exists for multi-outcome markets — binary markets can't accidentally call `build_split_yes_pset`).

### View Caching and Borrow-Checker-Enforced Freshness

**Chosen**: view types cache `(params, state)` at construction time via a single `ContractStore` read. Methods on the view use the cached values without re-reading from the store.

**Rejected**: re-read `(params, state)` from the store on every method call. Would guarantee freshness but adds 1 store read per method call.

**Why**: Rust's borrow checker makes view caching provably safe. A view holds `&'a ContractEngine<S>` (immutable borrow), which blocks any `&mut self` engine method from running while the view is alive. Contract params are immutable over the contract's lifetime. State transitions only happen through `&mut self` engine methods (`step`, `rollback_to_height`, etc.). Therefore, within the view's lifetime, no state change is possible — the cached values are guaranteed fresh. No runtime staleness check needed. Zero ongoing cost for caching; just a single initial store read.

### Relationship Queries on the Market View

**Chosen**: `Market::pools(filter, page)` and `Market::orders(filter, page)` replace the engine-level `engine.pools_for_market(market_id, ...)` and `engine.orders_for_market(market_id, ...)`.

**Rejected**: keep relationship queries on the engine alongside `list_pools` / `list_orders`.

**Why**: "pools belonging to this market" is a per-market operation, naturally scoped by the market's identity. Moving it to the `Market` view means consumers already holding a market reference don't need to pass the market's ID back to the engine. Discoverability: "what's associated with this market?" is answered by the view's methods, not by scanning the engine's namespace. The engine-level `list_pools` / `list_orders` stay for the global "list all pools regardless of market" case.

### Trade Routing Stays on the Engine

**Chosen**: `quote_trade(market_id, spec, fee_rate)` and `build_trade_pset(quote, funding)` stay on `ContractEngine`, not on the `Market` view.

**Rejected**: `market.quote_trade(spec, fee_rate)` and `market.build_trade_pset(quote, funding)` on the `Market` view.

**Why**: trade routing needs access to *multiple* contracts simultaneously — the target market's pool(s) and any maker orders on the target market's tokens. Putting `quote_trade` on the `Market` view would require the view to transitively see other tracked contracts (pools, orders), breaking the encapsulation the view-type pattern is meant to establish. Keeping routing at the engine level — which naturally has access to all tracked contracts — is both simpler and more honest about the operation's scope. The engine is the right home for operations that inspect or compose across multiple contracts.

### Builder Naming on Views

**Chosen**: on view types, drop the contract-type prefix from builder names where it would be redundant with the view's type:

- `ContractEngine::build_lmsr_adjust_pset` → `Pool::build_adjust_pset`
- `ContractEngine::build_lmsr_close_pset` → `Pool::build_close_pset`
- `ContractEngine::build_cancel_order_pset` → `Order::build_cancel_pset`

Market and multi-outcome market builder names are unchanged — the action (issuance, cancellation, split-YES, etc.) is already specific without needing a "market" prefix.

**Why**: scope is already conveyed by the type (`Pool::build_adjust_pset` is unambiguous; the `lmsr_` prefix was needed when it was one of many `build_*` methods on the engine). Shorter names, cleaner code at call sites.

### Creation Builders Stay on the Engine

**Chosen**: creation builders (`build_binary_market_creation_pset`, `build_multi_outcome_market_creation_pset`, `build_lmsr_bootstrap_pset`, `build_create_order_pset`) stay on `ContractEngine`, not on any view type.

**Rejected**: factory-pattern types like `engine.market_factory().build_binary_creation_pset(...)`.

**Why**: creation operates on a contract that doesn't exist yet — there's no view to hold cached state. Putting these on the engine alongside ingestion (`ingest_*`) keeps the "introduce a new contract to the engine" path together. Also: creation builders need to return the newly-derived full params (for `ingest_market` after confirmation), which would be awkward on a factory type. Engine-level keeps it simple.

### `derive_pool_params` / `derive_order_params` Take `OutcomeIndex`

**Chosen**: the helper functions take `market_params: &MarketParams` (the umbrella) plus `outcome: OutcomeIndex` to identify which outcome's YES/NO pair the pool/order is for. Binary markets pass `OutcomeIndex::BINARY`; multi-outcome markets pass any valid index in `[0, outcome_count)`.

**Rejected**: keep `market_params: &BinaryMarketParams` (binary-only helpers) and introduce parallel `derive_pool_params_multi_outcome` / `derive_order_params_multi_outcome` functions.

**Why**: pools and orders for multi-outcome markets are first-class and need derivation support just like binary ones. The existing helper already takes market params; adding `outcome: OutcomeIndex` is the minimal extension. A single function for both kinds keeps the surface small; internal dispatch based on the `MarketParams` variant handles the per-kind asset-ID lookup. For binary callers, passing `OutcomeIndex::BINARY` is a minor annotation; for multi-outcome callers, the parameter is load-bearing.

### Multi-Outcome Market Support: Option E (Unified API, Enum-Dispatched Internals)

**Chosen**: Expose a unified public API across binary and multi-outcome markets where the operations are conceptually shared, with enum-based internal dispatch. Binary- and multi-outcome-specific types are siblings under umbrella enums (`MarketParams = { Binary(BinaryMarketParams), MultiOutcome(MultiOutcomeMarketParams) }`, same pattern for `MarketState`, `MarketTransition`). Consumers interact with markets via the `Market` view type (see Stage 2); operations that exist only for multi-outcome (split-YES, merge-YES, split-NO, merge-NO, cross-outcome swap) live on a `MultiOutcomeMarket` specialization accessible via `Market::as_multi_outcome() -> Option<MultiOutcomeMarket>`.

**Rejected**:
- **Option A — separate APIs**: `ingest_market` + `ingest_multi_outcome_market`, separate listings, separate state types at the surface. Leaks the binary/multi-outcome distinction into consumer code at every operation. Consumers iterating over all markets would need two listings and merge.
- **Option B — flat unified enum everywhere**: force-unify `MarketState` variants (`Resolved{outcome_index: 0}` for binary YES, `Resolved{outcome_index: 1}` for binary NO) to one enum. Rejected because binary and multi-outcome resolution semantics genuinely differ (binary's `Side` = which side of one event won; multi-outcome's `OutcomeIndex` = which of N events happened). Forcing them into one shape obscures the model.
- **Option C — binary as multi-outcome with N=1**: use the multi-outcome contract for everything. Rejected because the two contracts have different token layouts (2 tokens vs 4 tokens at N=2 in the 2N model), different slot counts, and different covenant source files. The existing binary contract is already implemented and deployed; collapsing binary into the multi-outcome code path would require regenerating it from the multi-outcome template and accepting the 2-tokens-per-outcome overhead.

**Why**: Consumers think "I'm tracking markets" not "I'm tracking binary and multi-outcome markets as distinct categories." The unified API reflects that mental model. Internal dispatch (trait `MarketBehavior` implemented for each params type) keeps engine code clean without leaking dispatch choices into the public surface. The view-type pattern isolates per-market operations into a cohesive API (`Market`) while allowing type-level specialization for multi-outcome-only operations (`MultiOutcomeMarket`).

### Binary/Multi-Outcome Naming Convention

**Chosen**: `BinaryMarketParams` / `MultiOutcomeMarketParams`, `BinaryMarketState` / `MultiOutcomeMarketState`, `BinaryMarketTransition` / `MultiOutcomeMarketTransition`, `BinaryMarketCreationParams` / `MultiOutcomeMarketCreationParams`. Umbrella enums drop the kind prefix: `MarketParams`, `MarketState`, `MarketTransition`.

**Rejected**: Keeping the legacy `PredictionMarketParams` / `MarketState` names for binary alongside `MultiOutcomeMarketParams` / `MultiOutcomeMarketState`. Rejected because the asymmetry is confusing — readers would ask "is `MarketState` the umbrella or the binary type?"

**Why**: Uniform naming makes pattern-matching intuitive. The rename is a no-op for the covenant layer (same `.simf` file, same types at the wire level) and localized to `deadcat-core` type definitions.

### OutcomeIndex Newtype

**Chosen**: `OutcomeIndex(u8)` with `OutcomeIndex::BINARY = OutcomeIndex(0)` as a public constant. Used in APIs that take an outcome identifier (`TradeSpec`, issuance builders, redemption builders, oracle attestation).

**Rejected**: Bare `u8`. Rejected because `u8` is ambiguous in the type system (could be a byte value, an output index, a side discriminant) and provides no compile-time documentation at call sites.

**Why**: Type safety + self-documentation. `OutcomeIndex::BINARY` is explicit at call sites: `spec.build_issuance_pset(OutcomeIndex::BINARY, 5, ...)` for binary markets vs. `spec.build_issuance_pset(OutcomeIndex::new(2), 5, ...)` for multi-outcome. Zero runtime cost.

### Oracle Attestation Uses `MarketResolution` Discriminated Union, Not `OutcomeIndex` Alone

**Chosen**: The unified oracle attestation API (`oracle_attestation_message`, `oracle_attestation_spec`, `verify_oracle_attestation`) takes a `MarketResolution` discriminated union:

```rust
pub enum MarketResolution {
    Binary(Side),             // encoded as outcome_byte 0x01 (Yes) or 0x00 (No)
    MultiOutcome(OutcomeIndex), // encoded as outcome_byte OutcomeIndex::as_u8()
}
```

**Rejected**: Taking `OutcomeIndex` uniformly and treating binary markets as having `OutcomeIndex::BINARY = 0` = YES, `OutcomeIndex(1)` = NO.

**Why**: Binary market resolution and multi-outcome resolution are semantically different. Binary markets have one event with two sides; resolution picks a `Side`. Multi-outcome markets have N competing events; resolution picks an `OutcomeIndex`. Conflating them under `OutcomeIndex` would mean `OutcomeIndex::BINARY` (the value 0) maps to outcome_byte 0x00 = binary NO, which is semantically confusing — readers would ask "why does the canonical binary outcome index mean NO?"

The discriminated union preserves the semantic distinction at compile time: a binary market requires `MarketResolution::Binary(_)` and rejects `MarketResolution::MultiOutcome(_)` at the API boundary (and vice versa). The `outcome_byte` encoding happens inside the engine and is consistent with the covenant's expectation.

This is the same pattern used elsewhere in the codebase: keep per-kind semantic types where the semantics differ (binary uses `Side` internally in `BinaryMarketTransition::Resolved`; multi-outcome uses `OutcomeIndex` in `MultiOutcomeMarketTransition::Resolved`). The umbrella `MarketResolution` appears only where a single API must serve both kinds.

### Multi-Outcome State Tracks Per-Outcome Supplies in Trading, Collateral Sum at Terminal Phases

**Chosen**: `MultiOutcomeMarketState::Trading { supplies: Vec<PairSupply> }` tracks each outcome's YES/NO supply individually. `Resolved { winning_outcome, collateral_unredeemed }` and `Expired { collateral_unredeemed }` track only total unredeemed collateral.

**Rejected**:
- `[PairSupply; N]` const-generic arrays in the state enum: rejected because `N` is a runtime value (per-market) and would force the entire `MarketState` type to be generic over `N`, breaking the umbrella enum. `Vec` runs at runtime with minor heap overhead but stays representable in a single enum.
- Full per-outcome supply tracking in `Resolved`/`Expired` terminal phases: rejected because granular post-resolution redemption data is better surfaced via `ContractHistory`. Tip state should stay tight; history methods serve detailed-audit use cases.

**Why**: `Trading.supplies` is necessary for per-outcome price and volume display. Terminal-phase tracking collapses to a single `collateral_unredeemed` u64, which fully determines the terminal condition (reaches 0 when all claimable tokens have been burned).

### Staging the Multi-Outcome Integration

**Chosen**: the multi-outcome integration into `deadcat-core` is landed across three sequential stages.

- **Stage 1 (complete)**: type definitions — umbrella enums, paired Binary/MultiOutcome inner types, `OutcomeIndex` newtype, `MarketResolution` discriminated union, generalized `AssetInfo` / `TradeSpec`, unified `OracleAttestationSpec`.
- **Stage 2 (complete)**: API surface — `Market<'a, S>`, `MultiOutcomeMarket<'a, S>`, `Pool<'a, S>`, `Order<'a, S>` view types with per-contract operations; engine surface shrunk to ingestion, chain sync, discovery, view accessors, creation builders, and trade routing; relationship queries moved to views; builder naming rationalized (`build_lmsr_adjust_pset` → `build_adjust_pset` on `Pool` view, etc.).
- **Stage 3 (complete)**: behavior — multi-outcome transaction interpretation (delta-shape classification into `MultiOutcomeMarketTransition` variants), multi-contract pattern detection on `InterpretedTransaction` (`as_trade`, `net_effect_for`) while preserving raw per-contract transitions, probability accessors on views (liquidity-weighted by LMSR `b`), chain sync notes for N-scaling. Cross-outcome arb quote/build and `as_cross_outcome_arb` classification were originally specified here but have since been deferred to v2 — see [Future: Cross-Outcome Arb API (v2)](#future-cross-outcome-arb-api-v2).

**Why**: staging keeps each reviewable chunk focused. Stage 1 establishes the vocabulary; Stage 2 uses that vocabulary in API signatures and view-type design; Stage 3 elaborates behavior (and corrects a Stage 1 design error surfaced by deeper interpretation analysis). Staging avoids touch-every-section-at-once commits that are hard to review.

### UTXO-following vs Transaction Classification

**Chosen**: UTXO-following state machine.
**Rejected**: Transaction classifier (enum of known tx types).
**Why**: A single transaction can affect multiple contracts (e.g., a trade that routes through a pool AND fills limit orders). A flat classification enum can't represent this. More importantly, the UTXO model handles unknown transaction shapes — if a future version combines operations in ways current PSET builders don't produce, the state machine still works because each contract independently follows its own outpoints.

### Input-Based Over Output-Based Tracking

**Chosen**: Detect transitions by finding tracked outpoints in transaction inputs (spends).
**Rejected**: Detect transitions by scanning transaction outputs for matching scripts.
**Why**: Input-based tracking is precise — a tracked outpoint is either spent or not. Output-based scanning requires checking against all scripts x all states x all contracts, and can false-positive when someone sends coins to a covenant address without going through the covenant spend path. Contract creation is the one exception (no prior outpoints), handled by the per-type ingestion methods' output-scanning logic.

### Core Owns No IO

**Chosen**: Core is pure computation; caller provides chain data.
**Rejected**: Core takes a chain backend trait and fetches data internally.
**Why**: Different consumers have fundamentally different IO capabilities. LWK uses Electrum. GDK uses its own backend. Esplora-based wallets use HTTP. Core shouldn't prescribe the IO layer. The caller converts their chain data into core's input format (`ChainTransaction`) and feeds it.

### No Global Sync Tip

**Chosen**: Each contract tracks independently; caller determines sync status.
**Rejected**: Core maintains a global sync tip that gates processing.
**Why**: A global tip creates unnecessary coordination. When a new contract is ingested, existing synced contracts would be blocked until the new one catches up. Without a global tip, synced contracts continue processing tip transactions while new contracts catch up independently. Whether a contract is "caught up" is the caller's domain — only they know the chain tip.

### Engine Exclusively Owns the Store

**Chosen**: `ContractEngine` takes ownership of the `ContractStore` implementation. The caller never accesses the store directly after handing it to the engine.
**Rejected**: Engine borrows the store, or engine and caller share access.
**Why**: If the caller could mutate the store directly, they could advance contract outpoints or modify state without the engine's knowledge, breaking internal invariants. Exclusive ownership ensures the engine is the single source of truth. Reads go through the engine's `&self` methods; writes go through `&mut self` methods.

### Deterministic Contract IDs

**Chosen**: Contract IDs are deterministically derived by the engine from contract parameters.
**Rejected**: Caller-assigned IDs or store-generated IDs.
**Why**: The same contract always produces the same ID regardless of who ingests it. Two different wallets ingesting the same market get the same ID, enabling cross-wallet coordination without requiring callers to know the hashing scheme.

### Creation-Specific Ingestion Logic

**Chosen**: Per-type ingestion methods have their own output-scanning logic, separate from `process_transaction`.
**Rejected**: Unified method that handles both creation and state transitions.
**Why**: Contract creation and state transition are fundamentally different operations. Creation scans transaction **outputs** to find initial covenant UTXOs (there are no prior outpoints to track). State transitions scan transaction **inputs** to find spent tracked outpoints. A unified method would need to handle both paths, muddying the clean UTXO-following model. Separating them keeps each method focused on its actual semantics.

### Transaction-Level Grouping with Composition

**Chosen**: Results are grouped at the transaction level. `interpret_transaction` returns `InterpretedTransaction` (txid + transitions + external_outputs). `step` returns `StepReport { transactions: Vec<ProcessedTransaction> }` where `ProcessedTransaction` wraps `InterpretedTransaction` with `ChainPosition`. Per-contract `Transition` carries only `contract_id` and `details`.
**Rejected**: (a) Flat list of per-contract transitions, each carrying its own txid, external_outputs, and optional chain position. (b) Per-contract external_outputs requiring wallet-side merge.
**Why**: A transaction can compose multiple contracts (e.g., a trade routing through a pool and filling an order), but each non-covenant output is associated with at most one contract — no output serves dual roles for two different contracts. This means output classification never conflicts across contracts, so the engine computes a single merged classification at the transaction level. Grouping at the transaction level eliminates three kinds of wallet boilerplate: grouping transitions by txid, merging output classifications across transitions, and handling the `Option<ChainPosition>` that a flat list would require (processing always has position, interpretation never does). The composition approach (`ProcessedTransaction` wraps `InterpretedTransaction` + `ChainPosition`) gives each method a return type that's fully precise — no optionals, no sentinel values.

### Store Trait with Optional History (Supertrait)

**Chosen**: Required `ContractStore` trait + optional `ContractHistory` trait. `ContractHistory` is a supertrait of `ContractStore`. Engine exposes typed history methods only when the store implements `ContractHistory`.
**Rejected**: (a) Separate traits with independent error types. (b) Single trait with history methods that return empty vecs.
**Why**: The supertrait relationship ensures `ContractHistory` shares `ContractStore`'s `Error` associated type, eliminating error type mismatch. The engine's history methods can wrap the error in `CoreError::Store(e)` without ambiguity. Separating the traits makes the contract clearer: a minimal consumer (Aqua) only implements `ContractStore`. A full consumer (Deadcat Live) implements both. Core's processing pipeline never calls history methods — they're purely for consumer-facing features like price charts.

### Typed History Convenience Methods

**Chosen**: Engine exposes per-contract-type history methods that delegate to the store's unified `transition_history` and unwrap the result.
**Rejected**: Three separate history methods on the store trait.
**Why**: The store writes unified `StateUpdate` values (a single transaction can affect multiple contract types). Splitting the read path at the store level would create a mismatch: writes are unified, reads are split. Instead, the store keeps one method, and the engine (which knows each contract's type) does the trivial unwrapping. This keeps the store trait simple while giving consumers typed results.

### Persistence Owned by Caller, Not Core

**Chosen**: Core defines traits; caller implements persistence.
**Rejected**: Core owns a database or storage engine.
**Why**: Aqua already has its own database. Deadcat Live uses SQLite. A CLI tool might use flat files. Core shouldn't prescribe storage. The trait approach lets each consumer integrate deadcat state into their existing persistence layer.

### Process and Persist Atomically

**Chosen**: `process_transaction` computes transitions and durably persists them in one call, returning results only after the write succeeds.
**Rejected**: Two-step pattern where the engine returns results and the caller manually triggers persistence.
**Why**: Since the engine exclusively owns the store, there's no reason for the caller to inspect results before deciding to persist — the transitions are deterministic from the transaction. Splitting compute and persist would create a window where a crash could leave the engine's in-memory state ahead of the store. The single-call pattern eliminates this by design.

### Per-Transaction Atomicity Required

**Chosen**: `apply_transitions` is per-transaction atomic — the full `&[StateUpdate]` slice passed in one call commits as a unit or not at all. Typically implemented with a single database transaction around the method body.

**Rejected**: Contract-level atomicity with transaction-level merely "recommended" (an earlier iteration). Under that weaker contract, a `CovenantInvariantViolation` mid-multi-contract-transaction could leave one contract advanced while another rolled back, producing a torn state the engine's retry logic couldn't safely recover from.

**Why**: Cross-contract transactions (routed trades, and in v2 cross-outcome arbs) produce multiple `StateUpdate` values that must commit together for the engine's error semantics to work. On `CovenantInvariantViolation` during a multi-contract transaction, the current transaction's batch must roll back as a unit so the engine sees a consistent state; partial commits would leave the store in a configuration the engine can't reach via normal transitions. Across multiple transactions within one `step` call, each transaction's batch commits independently — prior transactions stay committed on error, the erroring transaction rolls back, unprocessed transactions remain to be processed on retry. Per-batch atomicity is the minimum guarantee required; it's also cheap to implement (one SQLite transaction per call) and compatible with integrators who already have transactional backends. See the "Atomicity requirements" paragraph under the Store Trait section for the full error-handling semantics.

### Idempotent Transaction Processing

**Chosen**: `process_transaction` is idempotent — calling it twice with the same transaction is a no-op.
**Rejected**: Requiring the caller to deduplicate transactions before calling.
**Why**: During catch-up and crash recovery, the same transaction may be fed to the engine more than once. Idempotency means the caller doesn't need to track what's already been processed — they can safely replay blocks without double-counting.

### Interpretation as Read-Only Replay

**Chosen**: `interpret_transaction` uses same logic as `process_transaction` but read-only. Works for both confirmed and unconfirmed transactions.
**Rejected**: Separate classification system for interpretation.
**Why**: The same script matching and output value logic serves both purposes. Having two systems would mean two places to update when covenant logic changes. The only difference is mutability — processing writes new state, interpretation just reads.

### Advance Logic Uses Script Matching and Output Values

**Chosen**: Use the detection method best suited to each contract type's structural characteristics. Markets: script pubkey matching (8 bounded, pre-storable scripts). Orders: taproot structural check (key-spend vs script-spend element count). Pools: witness-based path and s_index extraction via `RedeemNode::decode` for all transitions. Dormant market terminals: witness-based path detection for the three-way ambiguity.
**Rejected**: (a) Pattern-match transaction structure (input/output counts). (b) Uniform witness decoding on all transitions for all contract types. (c) Output-only detection for all transitions (no witness inspection). (d) Reserve-based s_index derivation for pool public/admin transitions.
**Why**: Each contract type has a naturally fitting detection method. Markets have 8 bounded phase scripts — byte comparison is O(1) and trivially airtight for all non-dormant transitions. Orders have a taproot-level key-spend/script-spend split — element count is the simplest possible check. Pools have unbounded s_index (scripts can't be pre-stored), and the engine needs the s_index value on every transition — the witness is the only reliable source, since reserve-based derivation (option d) is fragile after admin adjustments (reserves change without moving along the LMSR curve). Dormant market terminals produce no covenant outputs, creating a three-way ambiguity (resolution YES/NO vs expiry) only resolvable from the witness. Uniform witness decoding (option b) was rejected because markets and orders have simpler, equally correct methods — adding `RedeemNode::decode` overhead to script matching or element counting would be strictly worse. Pure output-based detection (option c) was rejected because pool s_index derivation from reserves is unreliable and dormant terminal ambiguities produce wrong state variants (e.g., `Expired` when the market actually resolved YES). `RedeemNode::decode` takes raw bytes from the transaction — no `CompiledProgram` or compilation needed, no storage needed. See [Detection Strategy and Robustness](#detection-strategy-and-robustness) for the full analysis.

### Output Identification via Script Pubkey Matching

**Chosen**: Identify contract outputs by matching `script_pubkey` against expected covenant scripts.
**Rejected**: Assume fixed output indices based on known PSET layouts.
**Why**: Multi-contract transactions (routed trades, combined operations) have variable output indices. LMSR pool reserve inputs can be interleaved with order inputs at arbitrary positions. Script pubkey matching works regardless of output ordering and handles reissuance token outputs (which have `Asset::Null, Value::Null` but valid script pubkeys) correctly.

### Interpretation Reflects Current Knowledge

**Chosen**: `interpret_transaction` returns results based on currently-ingested contracts. Unknown contracts are silently absent from results. Results grow as more contracts are ingested.
**Rejected**: Returning "unknown contract detected" markers or requiring all contracts to be ingested before interpretation works.
**Why**: Core can't know what it doesn't know. A transaction might touch contracts from markets the caller hasn't discovered yet. Returning empty for unknown contracts is honest and composable — the caller re-interprets after ingesting new contracts. Markers would require heuristic detection of "looks like a covenant" which is fragile. The caller's mental model is simple: "interpretation results may improve over time as I discover more contracts."

### Consensus-Validity Assumption

**Chosen**: Core assumes all transactions passed to it are consensus-valid. `process_transaction` and the per-type ingestion methods require confirmed transactions. `interpret_transaction` accepts unconfirmed transactions but still assumes consensus validity (mempool-accepted).
**Rejected**: Core validates Simplicity witnesses, confidential proofs, and reissuance token derivation internally.
**Why**: The caller's chain backend only provides consensus-valid data. Liquid consensus has already verified everything — Simplicity covenant witnesses satisfy the script, confidential range proofs are valid, reissuance tokens match their issuance entropy. Re-verifying would duplicate work the network already did. This assumption is what allows core to identify reissuance token outputs (which have `Asset::Null, Value::Null` on-chain) purely by script pubkey matching, and to classify confidential outputs as "not covenant" without needing to unblind them.

### ExternalOutput as Struct with Optional ExplicitValues

**Chosen**: `ExternalOutput` is a struct with shared fields (`index`, `script_pubkey`, `role`) directly accessible, and `explicit: Option<ExplicitValues>` bundling asset and value.
**Rejected**: (a) Enum with `Explicit` and `Confidential` variants (duplicates shared fields, couples role to asset/value observability). (b) Flat struct with separate `Option<AssetId>` and `Option<u64>` (allows "asset known but value unknown").
**Why**: Asset and value are always known together or not at all — `Option<ExplicitValues>` preserves this invariant. `role` is an independent axis: the engine can determine purpose from the script alone (e.g., burn outputs at the unspendable OP_RETURN script) even when asset and value are blinded. The struct makes shared fields (`index`, `script_pubkey`, `role`) directly accessible without matching — the common case ("what role is this output?") is `output.role` regardless of blinding. `is_explicit()` and `is_confidential()` convenience methods provide the blinding check when needed.

### OutputRole as Pure Semantic Label

**Chosen**: `OutputRole` variants carry only semantic information (e.g., `Side` for token outputs). Asset and value are NOT duplicated from `ExplicitValues`.
**Rejected**: `OutputRole` variants with `amount: u64` and `asset: AssetId` fields.
**Why**: In Elements, the output `value` field IS the raw unit count — token count for YES/NO tokens, satoshis for L-BTC. There is no denomination layer. Since `ExplicitValues` already carries `asset` and `value`, duplicating them in `OutputRole` would be pure redundancy, with the risk of inconsistency between the two copies.

### Self-Sufficient Tip State

**Chosen**: Each contract's current state (stored via `ContractStore`) carries enough information for basic wallet UX without requiring `ContractHistory`.
**Rejected**: Minimal tip state that requires history for common queries like "what was the market outcome?"
**Why**: `ContractHistory` is optional — minimal consumers (Aqua) may not implement it. The tip state must answer basic wallet questions independently: market outcome (YES/NO/cancelled/expired), order fill progress, pool current reserves. Richer features (price charts, fill-by-fill breakdowns, audit trails) belong in transition history.

### Generic Error Type Over Boxed Errors

**Chosen**: `CoreError<E>` is generic over the store's error type via an associated type on the trait.
**Rejected**: `CoreError` with `Store(Box<dyn Error>)` that erases the store error type.
**Why**: The engine is already generic over `S: ContractStore`, so `CoreError<S::Error>` adds no new generic parameter burden. Store error types are preserved — consumers can match on `CoreError::Store(e)` and handle their specific store error without downcasting. Store implementors define their own error type independently.

### No Thread Safety Constraints on Store Trait

**Chosen**: `ContractStore` has no `Send`/`Sync` bounds. Thread safety is determined by the implementor's choice.
**Rejected**: Requiring `Send + Sync` on the store trait.
**Why**: Single-threaded consumers (the common case) shouldn't pay for thread safety they don't need. If a consumer wraps the engine in `RwLock` for multi-threaded access, Rust automatically requires their store to be `Send` — the constraint propagates naturally without being forced on everyone.

### Cursor-Based Pagination Over Offset

**Chosen**: Opaque cursor-based pagination for all listing and bulk-read methods.
**Rejected**: Offset + limit pagination.
**Why**: Offset-based pagination is unstable under concurrent modifications — if a contract is ingested or reaches terminal state between page fetches, items shift and the caller sees duplicates or misses entries. At Polymarket scale with background sync continuously advancing contracts, this is a real possibility. Cursor-based pagination is stable: "give me items after X" is unaffected by insertions or deletions before X. It also maps efficiently to database index seeks rather than counting and discarding rows.

### All Store Methods Required (No Defaults)

**Chosen**: Every `ContractStore` method is required — no default implementations.
**Rejected**: Default implementations that scan all contracts and filter (Iterator pattern).
**Why**: Deadcat targets Polymarket-scale operation (thousands of markets, potentially tens of thousands of orders). Default implementations that scan all contracts would hide O(N) performance cliffs behind a convenient API. By requiring every method, the performance characteristics of each query are explicit in the implementor's code. If an implementor wants to shortcut (e.g., implement `list_markets` by scanning all contracts), the cost is visible in their own code, not a hidden gotcha.

### Rollback Removes Contracts Created Above Rollback Height

**Chosen**: `rollback_to_height(N)` removes contracts whose creation transaction was in blocks strictly above N, in addition to reversing transitions.
**Rejected**: (a) Rollback ignores ingested contracts. (b) Rollback marks contracts as "unverified."
**Why**: After a reorg, a contract's creation transaction may no longer exist on the canonical chain. Keeping a phantom contract with invalid outpoints would corrupt the engine's state — the engine would track outpoints that don't exist, and `step` would silently fail to match. Removing the contract is correct: the caller re-discovers and re-ingests from Nostr if the creation tx reappears on the new chain. The "unverified" approach adds unnecessary state complexity for a problem that re-ingestion solves cleanly.

### Deterministic RT Blinding (Anchor Elimination)

**Chosen**: Prediction market creation transactions blind reissuance token outputs with deterministic blinding factors derived from public on-chain data. No out-of-band anchor data needed.
**Rejected**: Random blinding factors for RT outputs, requiring an anchor (blinding factors) to be shared via Nostr.
**Why**: The Elements protocol requires reissuance token outputs to be blinded (ABF != 0) for reissuance to work. Traditionally, random ABFs are used as an authorization mechanism — only someone who knows the ABF can reissue. With Simplicity covenants, authorization is enforced by the covenant itself, not by ABF secrecy. Using deterministic ABFs derived from public data (defining outpoints via tagged hash) satisfies the protocol requirement while eliminating the need for anchor distribution. This simplifies the ingestion API (no anchor parameter), the Nostr announcement format, and removes the "lost anchor" failure mode. See [Deterministic RT Blinding](../protocol/deterministic-rt-blinding.md) for the derivation spec.

### UnblindedPset Newtype for RT-Involving Builders

**Chosen**: The 6 prediction market builders that involve reissuance token outputs return `UnblindedPset` — an opaque newtype with `prepare(pubkey)` and `finalize()` methods. The 7 remaining builders return `PartiallySignedTransaction` directly.
**Rejected**: (a) All 12 builders return `UnblindedPset` (uniform but unnecessary wrapping for RT-free builders). (b) All builders return raw `PartiallySignedTransaction` (no enforcement). (c) Builders take blinding parameters and handle all blinding internally (conflates construction with wallet-level blinding, requires RNG/secp context parameters). (d) Two builder functions per RT-involving transaction type — one fully-blinded, one partially-blinded (doubles API surface, the fully-blinded variant has a hidden precondition about wallet input confidentiality).
**Why**: RT blinding is deadcat-specific, non-standard, and easy to forget — the newtype makes "forgot to blind" a compile error. Wallet output blinding for RT-free builders is standard Elements wallet behavior that every integrator already handles — wrapping it adds ceremony without preventing a novel mistake. The `prepare`/`finalize` choice is a simple privacy decision (confidential vs explicit wallet outputs), not a technical one about input types. The `UnblindedPset` captures all needed state at build time (PSET, deterministic RT factors, input secrets from `WalletFunding`), so neither method requires additional crypto parameters from the caller. Core implements deterministic blinding using public `elements`/`secp256k1-zkp` APIs — no fork needed.

### Store Returns Typed Results for Listing Methods

**Chosen**: Store's `list_markets`, `list_pools`, `list_orders`, `pools_for_market`, and `orders_for_market` return typed results (e.g., `Page<MarketEntry>`) rather than the generic `Contract` enum. Results use `ContractEntry<P, S>`, a generic struct with type aliases for ergonomics.
**Rejected**: Store returns `Page<(ContractId, Contract)>` and the engine unwraps.
**Why**: The methods are already type-specific — `list_markets` only returns markets. Returning the generic enum would allow the store to accidentally return wrong variants, with the mismatch only caught at runtime. Typed return types enforce the invariant at compile time. If the store's data is corrupted (a market row deserializes to a pool), the error surfaces at the store layer via `Self::Error`, where data corruption belongs.

### Two-Step Trade PSET Pattern

**Chosen**: Trade PSET construction uses a two-step pattern: `engine.quote_trade()` (read-only, returns `TradeQuote`) then `engine.build_trade_pset(quote, funding)`.
**Rejected**: (a) Single engine method that takes wallet UTXOs directly without showing a quote first. (b) Standalone builder that requires the caller to manually specify the route.
**Why**: Trade is the only operation requiring cross-contract route optimization — the engine has the pool/order state and LMSR math needed to compute optimal routes. The two-step pattern enables the standard trading UX of "show quote, user confirms, then build." `TradeQuote` uses `pub(crate)` internal fields so external consumers cannot construct one — they can only receive quotes from the engine and pass them to the builder. All other PSET builders are single-step: the caller provides operation params and gets a PSET back immediately, no quoting needed.

### Assisted Pool Liquidity Stays Inside Trade API

**Chosen**: Existing-pool issuance/cancellation assist is exposed only through `quote_trade` + `build_trade_pset`. Public `TradeQuote` displays it as `LiquiditySource::LmsrPool { market_assist: Option<MarketAssist> }`; the exact parent-market continuation remains internal in `TradeRoute`. v1 allows at most one assisted pool leg per route and prefers non-assisted routes on ties.
**Rejected**: (a) Dedicated public `build_issue_into_pool_trade_pset` / `build_cancel_from_pool_trade_pset` builders. (b) Hiding assisted liquidity entirely from `TradeQuote`.
**Why**: Assisted pool liquidity is still a taker trade — it belongs behind the same quote/confirm/build flow as every other routed trade. Separate builders would leak router internals into the public surface and create a second taker API for what is conceptually the same operation. Hiding assist entirely would make quotes misleading, because the on-chain transaction weight and the taker's net capital flows differ materially from a plain swap. The chosen shape exposes only the user-relevant summary and keeps the complicated covenant bookkeeping internal.

### Non-Idempotent Contract Ingestion

**Chosen**: Per-type ingestion methods return `CoreError::ContractAlreadyTracked { contract_id }` on duplicate ingestion.
**Rejected**: Silent no-op (idempotent) like `process_transaction`.
**Why**: `process_transaction` replays are routine (catch-up, crash recovery), so idempotency is essential. Ingestion duplicates typically indicate a caller bug. The error surfaces it while still enabling crash-safe recovery: the error carries the `contract_id`, so callers who want idempotent behavior can extract the ID from the error variant. A silent no-op could mask real issues and would not provide the contract ID to the caller.

### Public Contract CMR Derivation

**Chosen**: Standalone `contract_cmr(params, network)` function available without an engine.
**Rejected**: CMR derivation only available through ingestion.
**Why**: Callers need CMRs before ingestion — primarily for deduplication during discovery. When Nostr pushes 50 market announcements, the caller checks CMRs against tracked contracts to skip already-known ones before fetching creation transactions. Requiring an engine (and a creation transaction) just to get a CMR would force unnecessary work. The full `ContractId` is only available after ingestion (when `creation_txid` is known).

### Simplicity Fully Encapsulated

**Chosen**: Compiled contracts, CMRs, taproot trees, and witness encoding are internal to the engine. Consumers never see these types. PSET builders are engine methods that recompile from stored params on each call.
**Rejected**: (a) Public compiled contract types with engine getters. (b) Standalone PSET builder functions that take compiled contracts as parameters.
**Why**: Compiled contracts are a "how" detail, not a "what." Wallet integrators care about building transactions, not about Simplicity compilation. Making PSET builders engine methods eliminates an entire type family from the public API (`CompiledPredictionMarket`, `CompiledLmsrPool`, `CompiledMakerOrder`) and the question of how to obtain them. All builders compile on demand — template parsing is cached process-wide via `OnceLock`, only instantiation + commitment is per-call (~10-100ms). The only trade-off is that PSET builders are no longer independently testable as pure functions — but they can be tested with an engine backed by a test store.

### Internal Coin Selection

**Chosen**: PSET builders perform coin selection internally. The caller provides their available UTXO pool. No public `select_utxos` function.
**Rejected**: (a) Public `select_utxos` function returning a newtype that builders require. (b) Public `select_utxos` alongside builders that also select internally (redundant).
**Why**: The builder knows exactly what it needs (from the operation params and/or trade quote). Exposing coin selection as a separate step forces the caller to extract target asset and amount from the params — duplicating knowledge the builder already has and creating a surface for bugs. Internal selection also eliminates a footgun: if the caller accidentally passed their entire UTXO set directly to a builder (bypassing selection), every UTXO would become a transaction input. With internal selection, passing all UTXOs is the expected usage — the builder selects the minimum needed. Pre-filtering the candidate pool remains the caller's mechanism for controlling which UTXOs are eligible.

### FeeRate Newtype Over Raw Integer

**Chosen**: `FeeRate` newtype with named constructors (`from_sat_per_kvb`, `from_sat_per_vb`) instead of a raw `u64`.
**Rejected**: `fee_amount: u64` (caller pre-computes the fee) or `fee_rate: u64` (ambiguous units).
**Why**: A raw `u64` for fee rate invites unit confusion (sats/vB vs sats/kvB vs millisats/vB). The newtype with named constructors makes the unit unambiguous at the call site. PSET builders accept `FeeRate` and compute the exact fee internally based on the actual transaction weight, eliminating the chicken-and-egg problem of needing to know the transaction size to estimate the fee. The caller's only responsibility is querying their chain backend for the current fee rate.

### History Pagination via ChainPosition

**Chosen**: History methods use `after: Option<ChainPosition>` + `limit: u32` for pagination. Results are ordered oldest-first (ascending).
**Rejected**: (a) `since_height: Option<u32>` — not a precise cursor when multiple transitions share the same block height. (b) Opaque `Cursor`-based pagination (used by listing methods) — unnecessary indirection for a naturally-ordered data set.
**Why**: `ChainPosition` (block height + tx index) is a precise, transparent cursor that handles multiple transitions within the same block correctly. The caller paginates by passing the last returned item's `position` as `after`. Oldest-first ordering aligns with the primary use cases (price chart construction, audit trails, catch-up from a checkpoint). For "recent activity" UX, the caller passes a recent `ChainPosition` as `after` rather than paging from the beginning.

### Merged Issuance and Redemption Builders

**Chosen**: `build_issuance_pset` handles both initial and subsequent issuance. `build_redemption_pset` handles both post-resolution and post-expiry redemption. The engine determines the correct covenant path from the contract's current state.
**Rejected**: Separate builders for each state transition (`build_initial_issuance_pset`, `build_subsequent_issuance_pset`, `build_post_resolution_redemption_pset`, `build_expiry_redemption_pset`).
**Why**: The caller-provided data is identical for both paths within each pair. The distinction is the covenant path, which the engine knows from the contract's current state. Separate builders would force the caller to determine the contract's phase before calling the correct builder — duplicating knowledge the engine already has. Merging reduces the public API surface and eliminates a class of "called the wrong builder for the current state" errors.

### WalletFunding Struct Over Per-Builder Args Structs

**Chosen**: PSET builders take operation-specific arguments as direct parameters alongside a shared `WalletFunding` struct. No per-builder args structs.
**Rejected**: (a) One args struct per builder (12 single-use types). (b) All arguments as direct parameters (7-8 args on some builders).
**Why**: Every builder needs the same three wallet fields (`available_utxos`, `fee_rate`, `return_script`). Bundling them into `WalletFunding` reduces argument count by 2 across all builders while avoiding the proliferation of single-use structs. The function signature documents each builder's operation-specific parameters directly. The caller constructs one `WalletFunding` and reuses it across multiple builder calls.

### Creation Builders Take Concrete Param Types

**Chosen**: Creation builders take concrete param types (`&MarketCreationParams`, `&LmsrPoolParams`, `&MakerOrderParams`) instead of the `ContractParams` enum. `build_binary_market_creation_pset` takes `MarketCreationParams` (only non-derivable fields) rather than full `BinaryMarketParams` because the 4 token/RT asset IDs depend on coin selection (see [MarketCreationParams](#marketcreationparams)).
**Rejected**: All creation builders take `&ContractParams`, with runtime validation of the variant.
**Why**: Passing the wrong variant (e.g., `ContractParams::LmsrPool` to `build_binary_market_creation_pset`) would only be caught at runtime. Taking concrete types makes wrong-variant errors compile-time errors. The standalone `contract_cmr()` still takes `ContractParams` (the enum) since it is genuinely polymorphic.

### Single Return Script for All Non-Covenant Outputs

**Chosen**: `WalletFunding.return_script` is the sole destination for all non-covenant, non-fee outputs: change, collateral refunds, redemption payouts, trade proceeds, order refunds, etc. No separate `payout_script` or `refund_script` parameters.
**Rejected**: Per-builder destination scripts (e.g., `refund_script` for cancellation, `payout_script` for redemption).
**Why**: In practice, wallets send all funds to the same set of addresses. Separate destination scripts add API surface without changing behavior for the common case. Using a single `return_script` simplifies every builder signature and enables output consolidation (same script + same asset → merge). Named `return_script` rather than `change_script` to reflect its broader purpose. If a future use case requires separate destinations (e.g., institutional custody with per-purpose addresses), this can be added as an optional override without breaking the API.
**Exception**: `build_issuance_pset` takes `yes_dest: &Script` and `no_dest: &Script` for newly minted token delivery. These are justified because the caller is creating two distinct assets that may need separate destinations (e.g., YES tokens to a pool, NO tokens to cold storage). This is fundamentally different from L-BTC fund flows where a single destination suffices.

### Output Consolidation Over Separate Outputs

**Chosen**: PSET builders consolidate outputs that share the same destination script and asset into a single output. `OutputRole` labels the consolidated output with its primary role. `TransitionDetails` is authoritative for exact semantic amounts.
**Rejected**: Separate outputs for each logical purpose (e.g., separate collateral payout and fee change).
**Why**: On Liquid, each extra confidential output adds ~4.3 KB to the transaction (range proof + surjection proof). This increases fees both now (larger transaction) and later (extra UTXO to spend). It also increases UTXO bloat and slightly worsens privacy (more outputs = more structural fingerprinting). Consolidation avoids all of this. The trade-off is that `OutputRole` can't distinguish the fee change portion of a consolidated output — but `TransitionDetails` provides exact semantic amounts, so wallets can always display precise numbers.

### Store-Persisted Indexes, No In-Memory Cache

**Chosen**: Asset ID → contract mapping and covenant scripts are persisted in the store (populated at ingestion via `DerivedContractData`). Compiled contracts are not cached — they are recompiled on demand for PSET builders only.
**Rejected**: (a) All indexes in memory (requires eager reconstruction at startup). (b) In-memory compiled contract cache (adds complexity for minimal benefit). (c) Persisted compiled contracts (simplicityhl's `CompiledProgram` has no serialization API).
**Why**: Asset ID lookups and covenant script lookups are hot-path operations (called by `identify_asset` and `step`'s internal sync logic). Persisting them in the store ensures O(1) construction of `ContractEngine::new` — no iteration or compilation at startup. The internal `process_transaction` and `interpret_transaction` determine transitions from script pubkey matching and output values — no compiled contracts needed. Only PSET builders require compilation (for witness encoding), and their ~10-100ms recompilation cost is acceptable for a user-initiated operation. An in-memory cache would only save recompilation across multiple PSET builds for the same contract within a single engine lifetime — too rare to justify the complexity (eviction during rollback, interior mutability). If simplicityhl adds `CompiledProgram` serialization, persisting at ingestion time would eliminate recompilation entirely.

Note: Covenant scripts are used internally by `step` for catch-up scanning and subscription registration. Markets have all 8 bounded scripts, orders have 1 static script, pools have empty scripts (they use outpoint-based sync instead). See [Chain Sync](#chain-sync).

### CMR + Creation Txid as Contract ID

**Chosen**: `ContractId { cmr: Cmr, creation_txid: Txid }` — both fields extractable.
**Rejected**: (a) CMR alone. (b) Hash of CMR + creation_txid.
**Why**: CMR alone identifies the program, not the instance. Two on-chain instances with identical params produce the same CMR. While collisions are self-defeating in practice (pools: admin key=operator makes collision self-inflicted; orders: fresh nonces prevent it), the `creation_txid` component closes all theoretical collision vectors at minimal cost (32 extra bytes, already available from discovery). The struct preserves both fields: `cmr` for discovery dedup (O(1) "do I track anything with this CMR?"), `creation_txid` for instance uniqueness. A hash would discard this extractability.
**Trade-off**: Discovery payloads must include `creation_txid`. The standalone `contract_cmr()` returns only the CMR component — the full `ContractId` is only available after ingestion (when `creation_txid` is known).

### Per-Type Ingestion Methods

**Chosen**: `ingest_market`, `ingest_pool`, and the split `ingest_persistent_order` / `ingest_ephemeral_order` with type-specific snapshot enums. Orders have two ingestion methods because persistence behavior is distinct (maker-owned orders keep history and persist through terminal states; taker-tracked orders skip history and auto-untrack past finality).
**Rejected**: Unified `ingest_contract(ContractParams, ChainTransaction)`.
**Why**: Different contract types have genuinely different ingestion needs. Markets always need the creation tx (few transitions, fast catch-up). Pools and orders benefit from non-initial ingestion (pools can have thousands of transitions; order takers don't need history). Per-type methods make each contract's recommended pattern explicit in the type system. The snapshot enums (`PoolSnapshot`, `OrderSnapshot`) document the trade-off at the type level: `Creation` = full history + verified; `Current` = fast start, no prior history, no verification back to creation.

### Non-Initial Ingestion for Pools and Orders

**Chosen**: `PoolSnapshot::Current` and `OrderSnapshot::Current` allow ingestion at any point in a contract's lifecycle.
**Rejected**: Always requiring the creation transaction.
**Why**: LMSR pools can accumulate thousands of state transitions (one per swap). Forward-syncing from creation requires sequential chain queries for each transition. Non-initial ingestion allows a trader to start using a pool immediately from its current state, without replaying its entire history. For limit orders, takers need only the current state — order history is irrelevant for filling. Makers who need history (monitoring, recovery) use `OrderSnapshot::Creation`.
**Trade-off**: Ingesting at `Current` means no prior history is recoverable. This is a one-way choice — the caller cannot "upgrade" to full history without untracking and re-ingesting from creation. The API makes this trade-off explicit via the snapshot enum variants. `Current` snapshots include a `ChainPosition` indicating when the outpoints were confirmed, enabling uniform rollback behavior.

### Forward-Only Sync (Backward-Sync Deferred)

**Chosen**: v1 supports only forward-sync. No backward-sync or parallel multi-checkpoint sync.
**Rejected**: Backward-sync for pool price history in v1.
**Why**: Forward-sync from creation (with optional discovery-batched TXID acceleration) covers v1 price history needs. Discovery payloads can include `Vec<Txid>` of transition history, enabling parallel batch-fetching. The engine just processes transactions in chain order — the optimization lives entirely in the discovery/caller layer. Backward-sync adds engine API surface (history backfill method), store trait complexity (out-of-order insertion), and a verified/unverified history distinction that isn't needed when sync is forward-only. All of this can be added later as a non-breaking addition.

### Discovery-Batched Forward Sync

**Chosen**: Pool discovery payloads can include `Vec<Txid>` of historical transitions. The caller pre-fetches all transactions in parallel and provides them through a `ChainSource` implementation that returns pre-cached results for `spending_transaction` calls. The engine's forward-chaining logic (via `step`) is unchanged — it still calls `spending_transaction` sequentially, but each call returns instantly from the cache instead of making a network request.
**Rejected**: Sequential forward-chaining as the only catch-up mechanism.
**Why**: A pool with 1000 transitions requires 1000 sequential `spending_transaction` queries when forward-chaining. With bundled TXIDs, the caller's `ChainSource` pre-fetches all 1000 transactions in parallel (one batch request to their chain backend) and caches them. The engine's `step` call then forward-chains through the cache at memory speed. The optimization lives entirely in the `ChainSource` implementation — the engine's internal logic is unchanged. The engine verifies chain of custody during processing (each tx must spend the previous outpoints), so pre-cached transactions are verified, not blindly trusted.

### Discoverability Trust Gap (OP_RETURN Deferred)

An LMSR pool operator could create a pool, manipulate its price privately (no one can arbitrage because no one knows about it), then announce it on Nostr. The historical price data looks legitimate (all real on-chain transactions) but wasn't subject to market pressure during the private period. The same attack extends to markets: an undiscoverable market + discoverable pool means only the operator can issue tokens and trade.
The ideal solution: embed full contract params in an OP_RETURN output in the creation transaction, making the contract provably discoverable from the chain from the moment of creation. However, `LmsrPoolParams` is 228 bytes and `BinaryMarketParams` is 204 bytes — both exceed Liquid's default 80-byte OP_RETURN relay policy. This is a policy limit (configurable by federation, not a consensus constraint), and Bitcoin Core has recently removed it entirely. When Elements merges this change, OP_RETURN-based discoverability becomes viable. Deferred until then.

Note: The recovery hints described in [Wallet Recovery](#wallet-recovery) and [chain-only-recovery.md](../protocol/chain-only-recovery.md) are distinct from the full-params discoverability discussed here. Recovery hints use compressed encodings (standard denomination conventions, well-known asset indices, hybrid time encoding) and omit derivable fields — they enable fund recovery (reconstructing params when combined with a mnemonic and chain data), not public discoverability (making params available to anyone scanning the chain). Full discoverability requires embedding complete params, which exceeds the current OP_RETURN policy limit.

### Dormant/Unresolved Hidden in Trading

**Chosen**: The public `MarketState` has 4 variants (`Trading`, `ResolvedYes`, `ResolvedNo`, `Expired`). Dormant (0 pairs) and Unresolved (>0 pairs) are both `Trading`. Terminal state = `outstanding_pairs == 0` on any non-Trading variant. No `Settled` variant.
**Rejected**: (a) Exposing `CovenantPhase` with Dormant/Unresolved. (b) Separate `Settled { final_txid, outcome }` terminal variant with `MarketOutcome` type.
**Why**: The Dormant/Unresolved distinction is a covenant implementation detail. The `Settled` variant was removed because it created a routing ambiguity: resolution from non-dormant markets produced intermediate `ResolvedYes/ResolvedNo` states, while resolution from dormant markets had to route directly to `Settled` — a special case an implementor could miss. Without `Settled`, resolution/expiry always produce the corresponding variant regardless of outstanding pairs, and `outstanding_pairs` naturally reaches 0 through redemption (or starts at 0 for dormant terminals). See the dedicated "Flat MarketState (No Settled Variant)" entry below for the `Settled` rationale.

### Pool Closure via Simplicity Script Path

**Chosen**: Dedicated Simplicity close script path with NUMS internal key (key-spend unspendable).
**Rejected**: Taproot key-spend for pool closure.
**Why**: The LMSR pool already uses NUMS as its internal key — key-spend was never available. A Simplicity close path provides atomic enforcement (all three reserve UTXOs must be consumed together), auditability (the close operation is visible in the witness), and eliminates the partial-spend edge case. See [lmsr-pool-close-path.md](../contracts/lmsr-pool/lmsr-pool-close-path.md).

### Dormant Terminal Paths

**Chosen**: Oracle resolution and timelock expiry are available from zero-pair state (both RT UTXOs consumed, market reaches terminal state with outstanding_pairs: 0).
**Rejected**: Only allowing resolution/expiry from non-zero-pair state.
**Why**: The same terminal states should be reachable regardless of outstanding pairs. Without this, abandoned or fully-cancelled markets have RT UTXOs that sit on-chain forever. The existing PSET builders (`build_oracle_resolve_pset`, `build_expire_transition_pset`) branch internally based on outstanding pairs — no new builder methods needed. See [market-dormant-terminal-paths.md](../contracts/prediction-market/market-dormant-terminal-paths.md).

### Untrack Contract

**Chosen**: `untrack_contract` method for removing contracts from the engine.
**Rejected**: No untrack mechanism (deferred indefinitely).
**Why**: With non-initial ingestion, the "untrack + re-ingest from creation" pattern enables promoting a contract from fast-start (no history) to full-history mode. Also serves general cleanup of terminal or unwanted contracts. Simple to implement (delete contract + derived data + history from store).

### Deterministic Nonces Prevent Order CMR Collisions

The `derive_order_params` function derives a unique nonce for each order from `deadcat_secret_key` + `order_index`: `HMAC(secret_key, "deadcat/order_nonce" || order_index)`. Different indices always produce different nonces, ensuring unique `maker_receive_spk_hash` values and preventing CMR collisions between orders from the same maker at the same price/direction. This is enforced by the API — callers cannot provide a non-deterministic nonce because the derivation is encapsulated.

### OP_RETURN Recovery Hints in All Contract Creation Transactions

**Chosen**: All three creation builders always include a zero-value OP_RETURN output with a compact recovery hint. Markets: 37 bytes (compressed non-derivable params using well-known asset index, 1-2-5 denomination, absolute u24 expiry). Binary and multi-outcome markets share the same 37-byte layout, distinguished by the type tag byte; `outcome_count` is derived from the creation tx's issuance count rather than stored. Orders: 40 bytes (XOR-masked index, market txid, u24 price, u8 min_fill/remainder, side+direction in type tag). Pools: 40 bytes (market txid, 4-bit 1-2-5 index each for max_loss/half_payout, u12 fee_bps, u16 initial_s_index, XOR-masked index). No opt-out. All fit within a single 80-byte OP_RETURN.
**Rejected**: (a) No on-chain hints (orders require brute-force scanning, tokens require Nostr for labeling/redemption). (b) Optional hint via builder flag (risk of users opting out). (c) Uncompressed params (wastes bytes). (d) Hints for orders and pools only, not markets (breaks the recovery chain for all user types including pure token holders).
**Why**: Chain-only recovery for ALL user types — market creators, order makers, pool operators, and pure token holders (via `issuance_transaction` → market creation tx → OP_RETURN). Maker orders are the only contract type directly "owned" by regular end users, making compression especially important. The OP_RETURN encoding uses standard denomination conventions (1-2-5 tables shared between market and pool encoding), well-known collateral asset indices, and XOR-masked derivation indices for privacy. Convention compliance is enforced at three layers: derive functions (first line), builders (defense in depth), and ingestion as the strict-canonical tracking boundary. See [chain-only-recovery.md](../protocol/chain-only-recovery.md) for the complete encoding specification and recovery flows.

### Taker Order Fills Via Trade Router

**Chosen**: Order filling (taker side) is handled exclusively through the trade system (`quote_trade` + `build_trade_pset`). No `build_fill_order_pset`.
**Rejected**: Direct `build_fill_order_pset` builder for explicit single-order fills.
**Why**: The trade router optimizes across all available pools and orders for best execution. A direct fill builder would allow suboptimal execution and create an inconsistency (pool trading already goes through the trade router; there is no standalone pool swap builder). The maker's lifecycle is directly exposed (`build_create_order_pset`, `build_cancel_pset`) because those are single-contract operations that don't benefit from routing. If explicit order targeting becomes a requested feature, a direct fill builder can be added as a non-breaking change (new engine method, no store or type changes).

### LMSR Adjust API Uses Deltas

**Chosen**: `Pool::build_adjust_pset` takes `pair_delta: i64` (applied equally to YES and NO) and `collateral_delta: i64`, not `target_reserves: &PoolReserves`.
**Rejected**: Absolute target reserves with runtime validation of the paired-delta constraint.
**Why**: The LMSR covenant enforces that YES and NO reserve deltas are equal on the admin path. By taking a single `pair_delta` parameter, the API makes this constraint unrepresentable as an error — the caller cannot express asymmetric deltas. The only remaining validation is reserve floors (computed targets must meet minimums), which is a meaningful constraint rather than an input formatting error. Wallets can present absolute-target UIs by computing deltas from current reserves on their side.

### One Permissionless Public Pool Path

**Chosen**: The pool covenant's permissionless path allows both ordinary swaps and equal YES/NO paired reserve deltas, including the degenerate `old_s_index == new_s_index` case. Admin adjust and close remain separate paths.
**Rejected**: Separate permissionless swap and permissionless pair-rebalance spend paths.
**Why**: One public path preserves future transaction composability for both binary and multi-outcome markets without needing to change already-created markets. The paired delta is derived from the reserve vector change itself rather than supplied as an independent witness scalar, so the extra flexibility does not weaken covenant correctness. v1 `quote_trade` intentionally emits only plain swaps and swap+market-assist routes; pure public pair rebalances remain covenant-valid for future composition without forcing a second taker API today.

### Pool/Order Ingestion Requires Parent Market

**Chosen**: `ingest_pool`, `ingest_persistent_order`, and `ingest_ephemeral_order` validate that the referenced token asset IDs correspond to a known market. If the parent market isn't tracked, returns `CoreError::ParentMarketNotTracked { detail }`. `ingest_market` has no parent requirement.
**Rejected**: (a) Allow orphaned pools/orders with later backfill. (b) Engine pre-computes parent ID and passes it to store.
**Why**: The store builds its own parent-market index by resolving token asset IDs via `find_by_asset_id` during `track_contract`. This requires the parent market to already be in the asset index. Allowing orphans would require backfill machinery (scan for orphans when a market is ingested) — significant complexity for a case that shouldn't happen. Discovery naturally produces markets before their pools/orders. The simpler option (engine pre-computes parent ID and passes it via `DerivedContractData`) was rejected because it duplicates work the store can already do with its existing asset index.

### Engine-Managed Sync via step and ChainSource

**Chosen**: The engine manages all sync internally via `step`, using a `ChainSource` trait for chain data access. Catch-up vs steady-state, script scanning vs outpoint forward-chaining, and subscription management are all hidden from the caller. `process_transaction` is `pub(crate)`.
**Rejected**: (a) Caller-managed sync with exposed building blocks (`market_catchup_scripts`, `watched_outpoints`, public `process_transaction`). (b) Engine owns the chain source (second generic parameter). (c) Pure polling without subscriptions.
**Why**: (a) Leaks per-contract-type sync strategies to the caller — every integrator must reimplement the same orchestration logic. (b) Adds a second generic parameter to the engine type, affecting all method signatures even for integrators who don't need sync. (c) Polling scales with total contracts (O(N) scripts per block); subscriptions scale with active contracts (O(active) per block). The chosen design keeps the engine at one generic (`S`), introduces `C: ChainSource` only on `step`, and manages subscriptions internally. `process_transaction` is internal because external calls would cause subscription state to become stale.

### ChainSource as a Read-Only Data Source

**Chosen**: `ChainSource` has 9 methods: 4 pull queries (catch-up + asset issuance lookup), 4 registration methods (steady-state), and 1 drain method (notification retrieval). The trait is sync, stateless (from the engine's perspective), and read-only.
**Rejected**: (a) Full chain backend trait with broadcasting, fee estimation, mempool queries. (b) Async trait. (c) Fewer methods with combined register/unregister semantics.
**Why**: The engine needs exactly two capabilities: historical data retrieval and change notification. Broadcasting, fee estimation, and mempool access vary wildly across backends and belong to the caller. Sync keeps core simple and embeddable. Separate register/unregister methods are more efficient than replace-all semantics for the common case (pool advances change 3 outpoints out of thousands of registrations).

### Gap-Free Catch-Up to Subscription Handoff

**Chosen**: `register_scripts` takes `from_height` to guarantee no gap. `register_spends` uses an "already spent" check (no height needed).
**Rejected**: Registration without height, relying on the caller to handle the gap.
**Why**: Between catch-up completing and subscriptions starting, a block could arrive. `from_height` on `register_scripts` creates overlap (the chain source delivers from `synced_to` onward), and overlap is harmless (idempotent processing). For outpoints, the binary spent/unspent check is sufficient — `register_spends` delivers the spending transaction immediately if the outpoint is already spent.

### Per-Contract synced_to Tracking

**Chosen**: Each contract has a `synced_to` block height, advanced by `step` even when no transitions are found.
**Rejected**: (a) No sync tracking — engine re-scans from last transition height (stuck `from_height` for inactive contracts). (b) Global sync tip (blocks independent contract catch-up).
**Why**: Without `synced_to`, a contract whose last transition was at height 1000 would be re-scanned from 1000 on every `step` call, even if the engine checked through height 2000 and found nothing. `synced_to` records "checked through 2000," so the next scan starts from 2000. Per-contract (not global) because contracts are ingested at different times and catch up independently.

### Denomination: `base_payout` as Primary Param

**Chosen**: Covenant param is `BASE_PAYOUT` — the per-outcome YES-expiry payout unit. Binary markets derive `cp = base_payout × 2`. Multi-outcome markets derive `cp = base_payout × outcome_count`. Both contract types share the same 1-2-5 denomination table, indexed by `base_payout`.
**Rejected**: (a) `COLLATERAL_PER_PAIR` as primary (pair cost) — requires covenant-level `cp mod N == 0` assertion for multi-outcome, restricts the 1-2-5 table to N-compatible values (empty for N ∈ {3, 6, 7, 9}), creates asymmetry between binary and multi-outcome denomination. (b) Per-N denomination tables — 8 separate tables, complex decoder. (c) `COLLATERAL_PER_TOKEN` (original) — required `× 2` in every formula, caused documentation bugs.
**Why**: Parameterizing on the per-outcome unit rather than the pair cost makes expiry-redemption divisibility automatic by construction. In binary, `cp = base_payout × 2`; in multi-outcome, `cp = base_payout × outcome_count`, so the covenant performs no division at runtime and needs no divisibility assertion. The 1-2-5 table (unchanged) is usable for every supported market shape. Binary and multi-outcome markets share one denomination model. See [multi-outcome-market-contract.md § Denomination model](../contracts/multi-outcome/multi-outcome-market-contract.md#denomination-model) and [collateral-per-pair-refactor.md](../contracts/prediction-market/collateral-per-pair-refactor.md) (the latter is superseded by this decision but retained for historical context on the earlier `collateral_per_token → collateral_per_pair` step).

### Key-Spend-Only Order Cancellation

**Chosen**: Maker order cancellation is exclusively via taproot key-spend. The Simplicity program handles fills only.
**Rejected**: Both key-spend and Simplicity cancel path (which existed in the original covenant).
**Why**: The script cancel path was functionally identical to key-spend (maker signature, no output constraints) but heavier. More importantly, having both paths made it impossible to reliably distinguish complete fills from cancellations using structural witness checks. With key-spend as the sole cancel mechanism, the engine uses a watertight 3-step detection algorithm: script-spend + new covenant output = partial fill, script-spend + no covenant output = complete fill, key-spend = cancel. See [maker-order-remove-script-cancel.md](../contracts/maker-order/maker-order-remove-script-cancel.md).

### BIP-340 Tagged Hash for Oracle Attestations

**Chosen**: Oracle attestation messages use BIP-340 tagged hash with tag `"deadcat/oracle_attestation"`.
**Rejected**: Plain SHA256 without domain separation (the original implementation).
**Why**: Without tagged hashing, a signature valid in one context could satisfy the covenant if the oracle's key is used in another protocol that signs `SHA256(32_bytes || 1_byte)`. The BIP-340 tagged hash convention creates a domain-separated hash function that cannot collide with untagged SHA256 or other tagged hashes. The double `SHA256(tag)` prefix fills one SHA-256 block (64 bytes), enabling constant-time precomputation. See [oracle-bip340-tagged-hash.md](../protocol/oracle-bip340-tagged-hash.md).

### Oracle Attestation as Standalone Function + Engine Convenience

**Chosen**: `oracle_attestation_message` is a standalone public function (no engine needed). `oracle_attestation_spec` is an engine convenience method that also returns the oracle pubkey.
**Rejected**: Engine method only.
**Why**: Oracle services that sign attestations may not run a `ContractEngine` — they just need the asset IDs and outcome to compute the message. The standalone function serves this use case. The engine method is a convenience for wallet integrators who have an engine and don't want to manually extract asset IDs from params (the engine exclusively owns the store, so the caller can't look up params directly).

### Snapshot Position for Uniform Rollback

**Chosen**: `PoolSnapshot::Current` and `OrderSnapshot::Current` include a `ChainPosition` field. `track_contract` takes `InitialContractState` which groups outpoints and position. Rollback treats all contracts uniformly — if initial position is above rollback height, the contract is removed.
**Rejected**: (a) Separate `known_at_height` field (over-aggressive — uses ingestion time, not outpoint confirmation time). (b) Caller responsible for post-rollback cleanup (latent-bug surface if caller forgets).
**Why**: The `ChainPosition` in the snapshot represents when the outpoints were confirmed — precise, not over-aggressive. Rollback becomes uniform: creation-tx contracts and snapshot contracts are both removed if their initial position is above the rollback height. The engine's tracked outpoints are always correct after rollback.

### Cosigner Removal and Admin Key Rename

**Chosen**: Remove the optional cosigner check from the maker order fill path. Rename `COSIGNER_PUBKEY` to `ADMIN_PUBKEY` in the LMSR pool covenant.
**Rejected**: Keeping the cosigner mechanism on orders (speculative complexity) and the misleading "cosigner" name on pools.
**Why**: The order cosigner was a no-op in practice (NUMS bypass was the expected default). On Liquid, there's no anti-MEV or batch-matching reason to gate-keep fills. The pool rename aligns with the actual role: `ADMIN_PUBKEY` is the sole authorization for admin/close operations, not a co-signature. See [maker-order-remove-cosigner.md](../contracts/maker-order/maker-order-remove-cosigner.md).

### Flat MarketState (No Settled Variant)

**Chosen**: `MarketState` has 4 variants: `Trading`, `ResolvedYes`, `ResolvedNo`, `Expired`. Terminal state = `outstanding_pairs == 0` on any non-Trading variant. No `Settled` variant, no `MarketOutcome` type.
**Rejected**: 5-variant enum with explicit `Settled { final_txid, outcome }`.
**Why**: Resolution and expiry always produce the same variant regardless of outstanding pairs. The uniform progression (`ResolvedYes(1000)` → redeem → `ResolvedYes(0)`) eliminates a routing ambiguity: without `Settled`, there is no special case for dormant terminals (0-pair markets resolved/expired directly to a different variant). Terminal detection is `outstanding_pairs == 0 && !Trading` — slightly more code than `matches!(Settled)` but eliminates an entire class of state-routing bugs. The `final_txid` (previously on `Settled`) is available from transition history.

### Chain-Only Recovery via Issuance Indexing

**Chosen**: `ChainSource::issuance_transaction(asset_id)` enables pure token holders (takers who only traded, never created contracts) to recover by tracing token asset IDs back to the market creation transaction.
**Rejected**: Token holder recovery only via Nostr discovery.
**Why**: Token holders are the most common user type. Requiring Nostr for fund recovery (labeling + redemption) would make the most common recovery scenario depend on an external service. Elements chain backends (Esplora, Electrs) natively support asset issuance indexing. One chain query per unique asset ID — simple and sufficient.

### Strict-Canonical Contract Tracking

**Chosen**: `deadcat-core` rejects non-conforming contracts at ingestion across all contract kinds. `ingest_market` verifies market conventions against the creation tx. `ingest_pool`, `ingest_persistent_order`, and `ingest_ephemeral_order` reject non-conforming supplied params on all snapshot variants; `Creation` snapshots additionally verify the creation tx, while `Current` snapshots intentionally keep their no-backfill trust trade-off.
**Rejected**: (a) Accept all pools/orders for routing while only markets are strict. (b) Accept all contracts and warn but do not reject. (c) Reject only at creation-builder time and let ingestion be permissive.
**Why**: A single tracked-contract class is easier to reason about than a mixed universe of canonical and foreign contracts. If `deadcat-core` tracks a contract, callers can assume it sits on the canonical v1 recovery surface rather than asking per-contract whether recovery or UX guarantees degrade. This is obviously required for markets, because even pure token holders trace back to the market creation tx, but the same policy is worthwhile for pools and orders because it keeps the routing and recovery model uniform. The remaining trust trade-off is explicit: `Current` snapshots skip verification back to creation, so they cannot prove that an omitted historical hint existed on-chain. They still enforce canonical param shape and a canonical parent market, preserving as much of the strict-canonical boundary as the fast-start snapshot model allows.

### Standard Denomination Conventions

**Chosen**: `base_payout` constrained to 16-value 1-2-5 table (4 bits); binary markets derive `cp = base_payout × 2`, while multi-outcome markets derive `cp = base_payout × outcome_count`. Pool `max_loss_sats` and `half_payout_sats` are constrained to the **same** 16-value 1-2-5 table (4 bits each), sharing the encoding with market `base_payout`. Well-known collateral asset index (4 bits: network policy asset = `0`, Liquid-mainnet USDt = `1`, escape = `15`).
**Rejected**: (a) Uncompressed u64 values in OP_RETURN (wastes bytes). (b) Separate 26-value mantissa × 10^exponent encoding for pools (previous design — 9 bits each, wider range but adds encoding complexity and a second convention to learn). (c) Per-N denomination tables (complex decoder).
**Why**: The conventions compress OP_RETURN hints (market: 77→37 bytes, pool: 51→40 bytes) while constraining parameters to "round numbers" that market creators naturally pick. Using a single 16-value 1-2-5 table for both market and pool denomination reduces the number of distinct encodings in the protocol, simplifies decoders, and keeps the committed LMSR Merkle-root fixture space small (16×16 = 256 combinations). The 10^7-sat range ceiling is a pragmatic v1 constraint, not a structural one — expansion to wider ranges (e.g., for pools on USDt-denominated markets with larger subsidies) is non-breaking via table extension. See [chain-only-recovery.md](../protocol/chain-only-recovery.md).

### XOR Index Masking for Privacy

**Chosen**: Derivation indices (`order_index`, `pool_index`) are XOR-masked in the OP_RETURN using `HMAC(deadcat_secret_key, tag || context)[0..2]` where `deadcat_secret_key` is a single key derived from `m/86'/1145258324'/secret'`. The mask context includes all other OP_RETURN fields, serialized as raw values in standard big-endian encoding (not the compact OP_RETURN bit-packing). Different HMAC tags (`"deadcat/order_mask"`, `"deadcat/pool_mask"`) provide domain separation.
**Rejected**: (a) Unmasked indices (reveals derivation order and contract count to observers). (b) No index in OP_RETURN (forces gap-limit scanning during recovery). (c) Mask context using OP_RETURN-encoded bytes (couples the mask to the OP_RETURN encoding format — a future V2 format change would break mask computation for V1 hints).
**Why**: The mask is deterministic from the mnemonic + public OP_RETURN data, so recovery is still O(1). Observers see random-looking u16 values. The privacy cost of unmasked indices is small (only meaningful if an observer can link two transactions to the same wallet), but the masking cost is zero (one HMAC computation). Using raw values for the context (not OP_RETURN encodings) makes the mask encoding-agnostic — the context length doesn't affect on-chain size (only the 2-byte mask output appears in the OP_RETURN). Known property: identical-param orders on the same market share a mask — a negligible concern in an already-pathological scenario. See [chain-only-recovery.md](../protocol/chain-only-recovery.md) for the exact byte-level context serialization.

### Deterministic Derivation via `derive_order_params` and `derive_pool_params`

**Chosen**: Both `derive_order_params` and `derive_pool_params` take `deadcat_xprv` (the xprv at `m/86'/1145258324'`) + a `MarketParams` umbrella + an `OutcomeIndex` + an index and derive all keys, nonces, and masks internally. Both return `Result<(Params, u16 /* masked_index */), ConventionError>` — validating OP_RETURN convention constraints before deriving. A single `deadcat_secret_key` (derived from the xprv) is used for all HMAC operations (nonce derivation, index masking) with different HMAC tags providing domain separation. `derive_pool_params` additionally takes `initial_s_index: u16` directly (not `starting_price_bps`) — the mask context includes `initial_s_index`, and sourcing it directly from the hint at recovery time (or from `estimate_bootstrap` at creation time) eliminates the non-injective inversion that a `starting_price_bps → initial_s_index` conversion would require.
**Rejected**: (a) Caller passes a pre-derived nonce or separate secret key + pubkey (foot-gun — non-deterministic nonces break recovery; separate keys require callers to manage multiple HD paths). (b) Separate `order_secret_key` and `pool_secret_key` at different HD paths (HMAC tags already provide full domain separation; a second secret key adds an HD path with no security benefit). (c) Omit `initial_s_index` from the pool mask context (weaker mask, and two pools with identical params but different starting prices would share the same mask). (d) Take `starting_price_bps` and re-derive `initial_s_index` internally (requires a bit-identical inverse logistic implementation on recovery; the `bps → s_index` snap is non-injective, so multiple `bps` values produce the same `s_index` — silent-failure surface if forward and inverse drift apart).
**Why**: The nonce and mask MUST be deterministic from the mnemonic for chain-only recovery. Encapsulating all derivation in these functions eliminates foot-guns (wrong key, non-deterministic nonce, mismatched pubkey). Taking `initial_s_index` directly means the `bps → s_index` snap function lives in exactly one place (`estimate_bootstrap`) and the hint's stored value is passed through unchanged on recovery. Accepting the `MarketParams` umbrella + `OutcomeIndex` lets a single pair of functions serve both binary and multi-outcome markets. See [Key Derivation Convenience Functions](#key-derivation-convenience-functions) and [chain-only-recovery.md](../protocol/chain-only-recovery.md).

### Convenience Derive Functions Accept Private Key Material

**Chosen**: `derive_order_params` and `derive_pool_params` accept the deadcat xprv (`elements::bitcoin::bip32::Xpriv`) and perform HD derivation internally. The internal HD path structure (`secret'`, `orders'/i`, `pools'/i`) becomes an implementation detail.
**Rejected**: (a) Derive functions take raw crypto inputs only — `&[u8; 32]` secret key + `&XOnlyPublicKey` pubkey (forces callers to manage HD paths externally, making the path structure a public interoperability standard). (b) Engine methods that accept xprv (violates "engine never touches private keys" more broadly).
**Why**: HD derivation is pure computation (HMAC-SHA512 + secp256k1 point multiplication) with no IO, state, or signing — the same category as Simplicity compilation and taproot tree construction. Encapsulating it in standalone functions (not engine methods) makes the HD path structure an internal detail. The derivation spec is documented publicly in chain-only-recovery.md for independent audit and cross-language implementations, but Rust integrators can use the convenience functions directly. The functions are stateless — the xprv is passed in, child keys are derived, and all private material is dropped on return. Hardened derivation ensures compromising the deadcat xprv cannot affect non-deadcat wallet keys.

### Covenant-Enforced Deterministic RT Blinding (CBF Pass-Through)

**Chosen**: The prediction market covenant enforces deterministic ABFs (derived from the input outpoint via tagged hash) and CBF pass-through (output CBF = input CBF) for all transitions that produce new RT outputs. The `verify_token_commitment` function is refactored from `(ABF, VBF)` to `(ABF, CBF)`.
**Rejected**: (a) Application-convention-only deterministic blinding (griefable — a malicious issuer uses non-deterministic blinding, locking the market). (b) Enforce both ABF and VBF independently (requires either a blinded wallet output for VBF balancing or 256-bit modular arithmetic in the covenant). (c) Enforce ABFs only, extract VBFs from witness data (recoverable but requires witness parsing and additional engine storage). (d) VBF pass-through instead of CBF (doesn't self-balance — the Pedersen balance equation involves `v*abf + vbf`, not just `vbf`).
**Why**: Without covenant enforcement, permissionless issuance enables a griefing attack: anyone can issue tokens and use non-deterministic blinding for the new RT outputs, making the market's RT UTXOs unspendable by others (unknown VBFs prevent Pedersen commitment balancing in future transactions). The CBF pass-through scheme self-balances across transitions without constraining wallet outputs — both `prepare()` and `finalize()` work. See [deterministic-rt-blinding.md](../protocol/deterministic-rt-blinding.md).

### OutputRole as Pure Purpose Labels

**Chosen**: `OutputRole` variants carry no asset or value data. Every variant is a simple label (`IssuedTokens`, `CollateralReturn`, `PoolReturn`, etc.). The wallet uses `identify_asset` when it needs to distinguish assets within a role.
**Rejected**: (a) `IssuedTokens { side: Side }` with a discriminant (saves an asset lookup but breaks the "roles don't carry asset/value data" principle; if one variant carries data, the others should too for consistency). (b) All variants carry full context data (forces artificial padding on variants with no natural non-duplicate data).
**Why**: Asset and value are already available via `ExplicitValues` on the parent struct. Duplicating them in the role creates inconsistency risk and muddies the type's purpose. One consistent principle (roles label purpose, the parent carries data) eliminates the need for case-by-case decisions about which variants deserve extra fields.

### FIFO Order Tie-Breaking

**Chosen**: `best_orders_for_market` returns orders at the same price in creation order (ascending `ChainPosition`).
**Rejected**: (a) Largest-first (minimizes leg count, saves marginal network fees). (b) Implementation-defined (non-deterministic routing).
**Why**: FIFO prioritizes maker fairness — the first maker to post an order at a given price gets filled first. The fee savings from largest-first are negligible (~tens of sats on Liquid in rare interleaving scenarios) because the fee-aware greedy already accounts for per-order activation cost in its effective price formula. FIFO is standard in traditional order books and creates the right incentive (post early).

### Fee Rate Validation on build_trade_pset

**Chosen**: `build_trade_pset` validates that `funding.fee_rate` matches the fee rate used during `quote_trade`. Returns `CoreError::InvalidParams` if they differ.
**Rejected**: (a) Silently use the quote's fee rate (surprising — WalletFunding.fee_rate ignored contradicts every other builder). (b) Document the mismatch as a caveat (silent suboptimality). (c) Allow intentional mismatch for fee overpayment (not relevant on Liquid, which has no fee market).
**Why**: The route was optimized for the quote's fee rate — a different rate could make included orders suboptimal. If the fee rate changed, the correct response is to re-quote (the route may change). The validation turns a silent suboptimality into an explicit prompt to do the right thing.

### Absolute Expiry Time Encoding (u24)

**Chosen**: Market OP_RETURN encodes `expiry_time` as `block_height / 60` in a u24 (3 bytes, big-endian). Recovery: `expiry_time = encoded × 60`. Hour-level granularity, range from Liquid genesis (September 26, 2018) to approximately the year 3931.
**Rejected**: (a) Creation-block-relative delta (u16, hybrid hour/day) — the creation block height is unknown at PSET build time, so the delta cannot be computed correctly; confirmation drift causes recovery to produce wrong values. (b) Well-known-epoch-relative delta (u16) — the epoch choice is arbitrary and hour-mode doesn't fit current Liquid heights in 15 bits. (c) Raw u32 block height (4 bytes) — one extra byte for block-level precision that adds no practical value over hour-level.
**Why**: The absolute encoding eliminates the chicken-and-egg problem entirely. At build time, `expiry_time` is a known block height — the builder divides by 60 and stores the result. At recovery time, the result is multiplied by 60. No estimation, no drift, no edge cases. The 1 extra byte (vs the broken u16 delta) is negligible.

### MarketCreationParams for Market Creation Builder

**Chosen**: `build_binary_market_creation_pset` takes `&MarketCreationParams` (4 non-derivable fields) and returns `(UnblindedPset, BinaryMarketParams)`. The builder derives the 4 token/RT asset IDs from the selected defining inputs.
**Rejected**: (a) Builder takes full `&BinaryMarketParams` (caller can't fill in the 4 derivable asset ID fields because they depend on coin selection, which happens inside the builder). (b) Caller pre-selects defining inputs (breaks the "pass all UTXOs, builder selects" pattern).
**Why**: The 4 asset IDs are derived from issuance entropy = `hash(defining_outpoint || contract_hash)`. The defining outpoints are UTXOs selected by the builder during coin selection. Since coin selection happens inside the builder, the caller can't know the asset IDs beforehand. `MarketCreationParams` makes the API honest about what data flows in which direction — the caller provides what they know, the builder returns what it computed.

### LMSR Deterministic Table Specification Required

**Chosen**: The deterministic integer-only F-value generation algorithm requires a formal specification in a separate satellite document, to be written after extracting the Merkle tree format from the `.simf` verification code.
**Why**: The derivation chain `max_loss_sats → b → q_step_lots → F-values → Merkle root` involves transcendental constants (`1/ln(2)`, `ln(999)`) and transcendental functions (`exp`, `ln`). Different rational approximations or different fixed-point precisions produce different F-values, different Merkle roots, and incompatible pools. Cross-implementation recovery (including the Rust implementation itself) requires exact constants, a defined algorithm, and test vectors. The `.simf` covenant defines the Merkle tree format (hash function, leaf encoding) via its verification code — the spec must extract and document this alongside the generation algorithm.

### Pool OP_RETURN Includes initial_s_index

**Chosen**: The pool OP_RETURN hint includes `initial_s_index` as u16 (2 bytes; pool hint is 40 bytes total, would be 38 without this field).
**Rejected**: (a) Derive s_index from reserve values via reverse LMSR lookup (fragile: bootstrap reserves are explicit caller-chosen inputs rather than uniquely determined by starting price, and adversarial reserve values make reverse inference unreliable). (b) Brute-force script matching over all 65K s_index candidates (requires EC scalar multiplication per candidate, ~3-7 seconds worst case).
**Why**: The pool's taproot tree structure means each s_index candidate requires a full taproot tweak (EC scalar multiplication) to verify — hashing alone is insufficient. Including `initial_s_index` directly in the hint eliminates all reverse-derivation complexity: compile for one s_index, verify script matches, done. The 2-byte cost is negligible relative to the hint's total size and is amortized over the pool's entire lifetime.

### HD Path Constants: BIP-86 Purpose + ASCII "DCAT" Coin Type

**Chosen**: deadcat uses the HD path `m/86'/1145258324'/{secret'|orders'/i|pools'/i}`. The `purpose'` value `86'` follows BIP-86 (single-key taproot) — deadcat covenants are taproot-based. The `coin_type'` value `1145258324'` is `0x44434154` = ASCII `"DCAT"`, self-documenting and within the hardened-index range (`< 2^31 - 1`).
**Rejected**: (a) Claim a new BIP-43 `purpose'` value by publishing a BIP (slow, no real-world precedent for non-coin protocols). (b) Use a phone-keypad constant like `3228'` (ambiguous — "DCAT"/"DAAT"/"FCAT"/"EBBT" all map to 3228). (c) Reuse the existing `deadcat-sdk` path `m/84'/1776'/...` (BIP-84 is P2WPKH, not taproot; LBTC's coin_type `1776'` is for L-BTC wallets, not for a layered protocol). (d) Pick an arbitrary low number (small collision risk with existing or future registrations).
**Why**: The SLIP-0044-coin-type-under-standard-BIP-purpose pattern is the community-expected path for Liquid-layered protocols (RGB-on-Liquid registered coin_type `828942'` under this model). ASCII-derived constants are self-documenting — anyone can verify `0x44434154 = "DCAT"` — and SLIP-0044 has precedent for ASCII-like coin_types (e.g., `0x80616263 = "abc"`). BIP-86 is the right purpose because covenants are taproot. `deadcat-core` is pre-implementation, so migrating off the old SDK path is a clean break with no on-chain state to preserve. A SLIP-0044 registration PR is tracked as a pre-v1-ship action item in [deadcat-core-implementation-plan.md](deadcat-core-implementation-plan.md).

### Multi-Outcome `outcome_count` Derived from Creation Tx Issuance Count

**Chosen**: Multi-outcome market hints do NOT store `outcome_count`. Recovery derives it by counting `AssetIssuance` structures in the creation tx whose `asset_blinding_nonce` is zero AND whose `amount` and `inflation_keys` are both non-null; `outcome_count = issuance_count / 2`. Binary and multi-outcome market hints share the same 37-byte layout (69 bytes with exotic collateral), distinguished only by the type tag byte.
**Rejected**: (a) Store `outcome_count` as a u8 in byte 1 of the hint (38 bytes — redundant since the covenant script is the authoritative binding). (b) Store `outcome_count` in 4 bits of the type-tag byte (packed format — couples "which hint type" with "how many outcomes"). (c) Leave the encoding question to each supported N (new type tag per N — breaks forward extension).
**Why**: The creation tx's issuance count is already the authoritative on-chain fact that determines `outcome_count` — storing it separately in the hint creates a redundant consistency surface without new information. The covenant script is the ultimate source of truth: a wrong derived N produces a compiled script that doesn't match any creation-tx output, and ingestion fails loudly. The defensive filter on `AssetIssuance` amount/inflation_keys null-ness rules out the one Elements edge case (asymmetric half-issuances) that could confuse a naive count. Saves 1 byte per multi-outcome hint; keeps binary and multi-outcome hint layouts unified.

### Convention Validation in Derive Functions

**Chosen**: `derive_order_params` and `derive_pool_params` return `Result<_, ConventionError>` and validate that all inputs conform to OP_RETURN encoding conventions before deriving parameters. Builders also validate (defense in depth).
**Rejected**: (a) Derive functions are infallible, validation only at builder time (error surfaces far from the logical mistake — caller gets valid-looking params back, wires up UI, hits wall at build time). (b) Derive functions validate but panic (convention violations are input errors, not bugs).
**Why**: The derive functions are the natural first line of defense — the caller is making the parameter decision at this point. Catching `fee_bps = 5000` at derivation time ("this value exceeds the u12 OP_RETURN encoding limit") is clearer than catching it at build time ("PSET construction failed"). Three enforcement layers total: derive functions → builders → ingestion as the strict-canonical tracking boundary (see [Wallet Recovery](#wallet-recovery)).

### `max_loss_sats` in `LmsrPoolParams`

**Chosen**: `LmsrPoolParams` includes `max_loss_sats: u64` alongside the covenant parameters, even though it is not itself a covenant parameter.
**Rejected**: (a) Store `max_loss_sats` in a separate `PoolConfig` wrapper (cleaner separation but ripples through the entire API — store trait, `Contract` enum, ingestion methods, discovery types). (b) Pass `max_loss_sats` as a separate parameter on `ingest_pool` (ad-hoc, no natural place to store it). (c) Recover `b` from `q_step_lots + half_payout_sats` (impossible — the `ceil()` in the derivation is lossy).
**Why**: All off-chain LMSR computation — cached-table quoting, full table generation for Merkle proofs, spot price calculation — requires the liquidity parameter `b = max_loss_sats / ln(2)`. Without `max_loss_sats`, the engine literally cannot evaluate the LMSR cost function after ingestion. The struct already contains two derived fields (`q_step_lots`, `lmsr_table_root`) as compilation caches, so adding a third non-covenant field is consistent. Including `max_loss_sats` also enables automatic curve well-formedness verification at `Creation` ingestion: the engine derives `b`, recomputes the table, and verifies the Merkle root matches — catching misconfigured or adversarially-constructed pools.

### `estimate_bootstrap` Does Not Take `fee_bps`

**Chosen**: `estimate_bootstrap` takes `max_loss_sats`, `half_payout_sats`, and `starting_price_bps` — no `fee_bps`.
**Rejected**: Including `fee_bps` for API symmetry with `derive_pool_params`.
**Why**: `estimate_bootstrap` computes only the snapped starting state and the canonical default reserve vector for that curve. The fee has no effect on the cost function, the useful-band bounds, the default reserve calculation, or the `starting_price_bps → initial_s_index` mapping — it's a per-swap spread applied by the covenant, not a curve-shape parameter. Accepting an unused parameter misleads callers into thinking fees affect bootstrap capital planning.

### Labeled Outpoints at the Engine↔Store Boundary

**Chosen**: `Vec<(SlotIdentity, OutPoint)>` at every engine↔store boundary where outpoint sets appear (`InitialContractState.outpoints`, `ContractMatch.matched_outpoints`, `StateUpdate.old_outpoints` / `new_outpoints`, `OutpointContractInfo.outpoints`, `ContractStore::contract_outpoints` return). Slot identity lives in the data, not in the `Vec` index.

**Rejected**:
- (a) Convention-only positional ordering — documentation-enforced invariant that the store implementor could silently violate (e.g., with a hash-backed index that scrambles insertion order).
- (b) Compliance tests alone — documentation plus testkit, but still convention at the type level. Better than (a) but still requires store implementors to opt in.
- (c) Type-level fixed shapes (`[OutPoint; 3]` for pools, typed structs like `PoolOutpoints { yes, no, collateral }` per contract type) — clean for pools and orders but forces a parallel shape for variable-N multi-outcome markets. Partial type-level enforcement is worse than picking a lane.
- (d-unrefined) Dropping ordering entirely with raw `Vec<OutPoint>` — fails because `OutPoint` alone is `(txid, vout)` with no way to determine which slot the engine meant without re-fetching the previous output from chain. Labels are the refinement that makes (d) work.

**Why**: (a) is fragile. (b) helps but is opt-in. (c) partially type-enforces but forces awkward variable-N handling. (d) with labels is strictly better: the store's contract becomes unambiguous ("persist these labeled pairs"), the engine/builders look up slots by `SlotIdentity` without positional conventions, and variable-N markets fit naturally via the `u8` outcome-index field in `MultiOutcomeMarketSlot` variants. Slot-label uniqueness within a contract is an engine-enforced invariant and is compliance-tested at the store boundary. See [SlotIdentity](#slotidentity-and-covenantphase) and the [ContractStore Compliance Test Kit](#contractstore-compliance-test-kit).

### Two Order Ingestion Methods, Not One

**Chosen**: `ingest_persistent_order(params, creation_tx)` and `ingest_ephemeral_order(params, snapshot)` — two distinct engine methods that signal caller intent at the call site.

**Rejected**:
- Single `ingest_order(params, snapshot, tracking: OrderTracking)` with a tracking-mode parameter. Less self-documenting at call sites (readers must trace the `tracking` argument to understand whether this is a maker-monitoring or taker-discovery call).
- Single `ingest_order(params, snapshot)` inferring tracking mode from snapshot type. Loses the `EphemeralFresh` case (creation tx discovered from Nostr, but no history desired) — that combination doesn't get expressed.
- Keeping tracking mode implicit and supplying only one method — taker and maker use cases diverge too much; a single method forces callers to remember which behaviors they get.

**Why**: The two methods make caller intent explicit at the call site — `ingest_persistent_order(params, tx)` obviously means "I own this and want full history," `ingest_ephemeral_order(params, snapshot)` obviously means "I'm tracking this for routing or display." The tracking-mode field in `OrderState` is still needed because it governs downstream engine behavior (history writes, `prune_finalized` cleanup), but callers don't pass a mode argument — the method-name-as-intent signal is cleaner. See [OrderState](#orderstate), [OrderTracking](#orderstate), and the engine's [Ingestion](#contract-ingestion) section.

### No Atomic Order Promotion Method

**Chosen**: Changing an order's tracking mode requires `untrack_contract` followed by re-ingestion via the other method. No dedicated `promote_to_owned` / `demote_to_discovered` engine method.

**Rejected**: A `promote_to_owned(contract_id, creation_tx)` engine method that updates tracking mode in place (cheap, no history backfill) or triggers forward-sync from creation (slow, full history rebuild).

**Why**: YAGNI. The promotion use case (a taker who initially tracked ephemerally decides they want to audit a specific order's history) is genuinely rare. The demotion case (a maker who owns an order decides they don't want history anymore, for storage cleanup) is even rarer — and they can just untrack the order entirely if storage cleanup is the goal. Dedicated promotion/demotion methods would add API surface and engine-level complexity for a use case nobody has asked for. If concrete demand surfaces post-v1, adding such methods is a pure non-breaking API addition. Until then, the two-step `untrack_contract` → re-ingest path is documented as the explicit migration for callers who need it.

### Post-Resolution Trading Not Gated

**Chosen**: `deadcat-core` does not gate trades through pools or orders whose parent market has resolved or expired. `quote_trade`, `build_trade_pset`, and all pool/order admin operations remain callable regardless of parent market state.

**Rejected**:
- Halting `quote_trade` / `build_trade_pset` with a `MarketNotTrading` error variant when the parent market is non-`Trading`.
- Adding a safety-mode flag that callers can toggle to opt in or out of the gate.

**Why**: The covenants are market-state-agnostic — they accept swaps and fills indefinitely, not by oversight but by architectural necessity. Covenants can only introspect the current transaction, so the only way for the pool covenant to verify the parent market's state is to **co-spend the market covenant's UTXO as an input on every swap transaction**. That would roughly double every swap's on-chain footprint (adding the market's collateral input + Simplicity witness to the pool's ~1,000 vbytes) and impose the cost on every legitimate trade — not just ones near resolution. Paying a permanent per-trade tax to block the informed-drainer attack in the narrow post-resolution / pre-operator-close window isn't a trade worth making. See [lmsr-pool-design.md § Why the pool covenant can't feasibly gate post-resolution trading](../contracts/lmsr-pool/lmsr-pool-design.md#why-the-pool-covenant-cant-feasibly-gate-post-resolution-trading) for the full analysis.

Given that the covenant can't feasibly enforce the gate, engine-layer gating would provide only false safety (sophisticated actors fork or bypass `deadcat-core`), while adding friction for legitimate edge cases (an informed trader dumping now-worthless tokens benefits from the covenant-valid trade even though it's bad for the pool operator). The engine's responsibility is covenant-validity and impossibility, not unfavorability. See the broader principle at [Design Principles § Engine gates covenant-invalidity and impossibility, not unfavorability](#engine-gates-covenant-invalidity-and-impossibility-not-unfavorability) and the operational consequences at [Pool and Order Lifecycle at Market Resolution](#pool-and-order-lifecycle-at-market-resolution). Pool operators protect themselves by closing pools after resolution (`build_close_pset`); UI-layer warnings handle the honest-user protection case.

### CovenantInvariantViolation Retained as Defense-in-Depth

**Chosen**: Keep the `CoreError::CovenantInvariantViolation { contract_id, kind }` variant even after a covenant-level formal proof lands, annotated in rustdoc as "unreachable post-proof in the covenant layer but retained as defense-in-depth for layers the proof doesn't cover (interpretation, chain-source, version-mismatch)."

**Rejected**: Removing the variant once a covenant proof is published, on the grounds that the proof renders the violation unreachable.

**Why**: A covenant-level proof eliminates the possibility that the covenant accepts a malformed transaction — but doesn't guarantee the interpretation layer correctly identifies well-formed outputs, that `RedeemNode::decode` has no bugs, or that the `ChainSource` backend isn't returning spoofed or truncated data. Failure modes at those layers still surface as "malformed covenant window" at the engine. Removing the variant post-covenant-proof would require an engine-level end-to-end proof (much larger lift) to be fully justified. Keeping the variant as defense-in-depth costs little (dead-code-post-proof) and catches real-world bugs outside the proof's scope. When/if an engine-level proof does land, the variant can be removed at that point with genuine code-simplicity gain.
