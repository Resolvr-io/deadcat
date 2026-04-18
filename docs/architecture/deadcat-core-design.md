# deadcat-core Design Document

## Purpose

`deadcat-core` is a pure computation library for interacting with Deadcat prediction market covenants on Liquid/Elements. It enables any wallet or application to create, track, interpret, and transact with prediction markets (binary and multi-outcome), LMSR pools, and limit orders — without prescribing how chain data is fetched, how state is persisted, or how keys are managed.

The primary motivating use case: integrating Deadcat functionality into existing wallets like Aqua, which already have their own wallet backend, chain connection, signer, and state management. These wallets need the covenant logic without an opinionated runtime.

**Contract scope**: `deadcat-core` supports two market contract types (binary and multi-outcome) plus one pool contract type (binary LMSR pool, used both directly for binary markets and via Option C composition for multi-outcome markets — see [amm-scoring-rule-tradeoffs.md](../contracts/multi-outcome/amm-scoring-rule-tradeoffs.md) and [multi-outcome-market-contract.md](../contracts/multi-outcome/multi-outcome-market-contract.md)) plus the maker order contract. The unified API (see [Option E decision in Design Decisions Log](#multi-outcome-market-support-option-e-unified-api-enum-dispatched-internals)) exposes per-market operations via the `Market` view type, with multi-outcome-specific operations accessible via `Market::as_multi_outcome()` type-level specialization.

**Implementation prerequisite**: This document specifies the planned end state — after several pending `.simf` covenant refactors (collateral-per-pair rename, oracle BIP-340 tagged hash, cosigner removal, script-cancel removal, pool close path addition, pool param constants) plus the as-yet-unimplemented multi-outcome market contract. These refactors and new contracts should be applied before implementing `deadcat-core`. See [contract-specification.md § Pending Refactors](../contracts/contract-specification.md#pending-refactors) for the complete list and status.

**Documentation staging**: this document was updated across three stages to integrate multi-outcome support. **Stage 1 (complete)**: core type definitions — umbrella enums (`MarketParams`, `MarketState`, `MarketTransition`), Binary/MultiOutcome-paired inner types, `OutcomeIndex` newtype, `MarketResolution` discriminated union for oracle APIs, `MarketId` newtype, generalized `AssetInfo`/`TradeSpec`, unified `OracleAttestationSpec`. **Stage 2 (complete)**: API surface — `Market<'a, S>`, `MultiOutcomeMarket<'a, S>`, `Pool<'a, S>`, `Order<'a, S>` view types; per-contract operations moved from `ContractEngine` methods to view types; engine shrunk to ingestion, chain sync, discovery, view accessors, creation builders, and trade routing; relationship queries moved to `Market` view. **Stage 3 (complete)**: behavior — multi-outcome transaction interpretation (per-primitive transitions, witness-based path disambiguation for the new spend paths), multi-contract pattern detection via `InterpretedTransaction::as_cross_outcome_arb` / `as_trade` / `net_effect_for`, probability accessors on views (`probability_bps`, `sum_of_probabilities_bps`, `implied_token_cost_sats` — liquidity-weighted by LMSR `b`), cross-outcome arb quote/build pattern (`quote_cross_outcome_arb` + `build_cross_outcome_arb_pset` with `ArbQuote` staleness-checked snapshot and `break_even_sets` helper), chain sync scaling notes for 2N+1 sibling-checked inputs and N-specific slot indexing. Also: removed the erroneous `CrossOutcomeSwap` variant from `MultiOutcomeMarketTransition` — at the single-market-contract layer, every transaction is exactly one primitive; cross-outcome rotation is a 2-transaction user-level composition, not a single market transition.

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

    pub fn ingest_order(
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
        starting_price_bps: u16,
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
    // Basket trades and cross-outcome arb are NOT handled here — see MultiOutcomeMarket view.
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

pub fn estimate_bootstrap(max_loss_sats: u64, half_payout_sats: u64, starting_price_bps: u16) -> BootstrapEstimate;

pub fn derive_pool_params(
    deadcat_xprv: &Xpriv,
    market_params: &MarketParams,        // accepts binary or multi-outcome
    outcome: OutcomeIndex,                // which outcome's YES/NO pair the pool serves
    pool_index: u16,
    max_loss_sats: u64,
    half_payout_sats: u64,
    fee_bps: u16,
    starting_price_bps: u16,
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
- `MultiOutcomeMarket<'a, S>` exposes cross-outcome primitives (split-YES, merge-YES, split-NO, merge-NO, cross-outcome arb). Obtained via `Market::as_multi_outcome()`; returns `None` for binary markets.
- `Pool<'a, S>` exposes adjust and close builders plus parent-market navigation.
- `Order<'a, S>` exposes cancel builder plus parent-market navigation.

**Creation builders stay on the engine** because the contract doesn't exist yet — there's no view to operate on. They take concrete param types and, for markets, return the derived full-params alongside the PSET (the 4 / 4N token and RT asset IDs are derived from selected defining inputs). `build_lmsr_bootstrap_pset` takes `LmsrPoolParams` fully formed (params aren't derived from transaction entropy — they're committed directly as operator subsidy parameters). `build_create_order_pset` takes `MakerOrderParams` similarly.

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

With `PoolSnapshot::Creation`, the engine processes the creation transaction to derive initial state — same as market ingestion. The engine also verifies the pool's curve is well-formed: it derives `b` from `params.max_loss_sats`, recomputes `q_step_lots`, regenerates the full F-value table, and checks the Merkle root matches `params.lmsr_table_root`. A mismatch indicates the pool was created with a non-canonical table generation algorithm — the engine returns `CoreError::InvalidParams`. This verification costs ~80ms (table generation) and runs once at ingestion. With `PoolSnapshot::Current`, the engine starts tracking from the provided state without verifying history back to creation. The trade-off: `Current` = fast start (no history replay needed), but no prior transition history is recoverable. `Creation` = full history available via forward-sync from creation. Note: `Current` also sidesteps s_index derivation entirely (the caller provides `s_index` directly), making it useful for ingesting untrusted pools from unknown operators where the creation transaction's OP_RETURN may not be available or trustworthy.

#### ingest_order

Orders support both creation-tx and non-initial ingestion:

```rust
pub fn ingest_order(
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

Same trade-off as pools: `Creation` gives full fill history; `Current` gives fast start with no history. Takers typically use `Current` (they only care about the current state for filling). Makers who want fill history use `Creation`.

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
// ingest_pool / ingest_order.
```

**Parent market required**: `ingest_pool` and `ingest_order` validate that the referenced token asset IDs correspond to a known market. If the parent market isn't tracked, the engine returns `CoreError::InvalidParams`. `ingest_market` has no parent requirement.

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

**Ingestion ordering constraint**: Pools and orders require their parent market to be ingested first. During `ingest_pool` or `ingest_order`, the engine validates that the referenced token asset IDs correspond to a known market. If the parent market isn't tracked, the engine returns `CoreError::InvalidParams`. This is a natural constraint — you shouldn't track a pool for a market you don't know about, and discovery naturally produces markets before their pools/orders.

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
    pub collateral_per_pair: u64,
    pub expiry_time: u32,
}

pub struct MultiOutcomeMarketCreationParams {
    pub oracle_public_key: XOnlyPublicKey,
    pub collateral_asset_id: AssetId,
    pub collateral_per_pair: u64,   // must be divisible by outcome_count (exact expiry redemption)
    pub expiry_time: u32,
    pub outcome_count: u8,          // N, validated in range per supported multi-outcome .simf files
}
```

The binary creation builder selects 2 defining inputs, derives the 2 token and 2 reissuance-token asset IDs, compiles the covenant, builds the PSET, and returns the full `BinaryMarketParams`. The multi-outcome creation builder selects 2N defining inputs, derives the 2N token and 2N reissuance-token asset IDs, compiles the N-specific `.simf` covenant, and returns the full `MultiOutcomeMarketParams`. In both cases the caller uses the returned params for subsequent `ingest_market` after the transaction confirms.

`BinaryMarketParams` and `MultiOutcomeMarketParams` define each market's covenant parameters (oracle key, expiry, asset IDs, etc.). `LmsrPoolParams` defines the pool's parameters (token asset IDs referencing the parent market, liquidity parameters, and `max_loss_sats` for off-chain LMSR math). `MakerOrderParams` defines the order's parameters (base/quote asset IDs, price, direction). These types map 1:1 to Simplicity covenant parameters, with one exception: `LmsrPoolParams.max_loss_sats` is not a covenant parameter but is included because all off-chain LMSR computation (point evaluation, table generation, spot price) requires the liquidity parameter `b = max_loss_sats / ln(2)`, and `b` is not recoverable from the covenant params alone (the `max_loss_sats → q_step_lots` derivation uses `ceil()`, which is lossy). The derivable fields `q_step_lots` and `lmsr_table_root` are retained alongside `max_loss_sats` as compilation caches — recomputing `lmsr_table_root` requires ~80ms of table generation. See [contract-specification.md](../contracts/contract-specification.md) for the planned field definitions per contract type, covenant structure, spend paths, and witness data.

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

#### SlotType and CovenantPhase (Internal)

`SlotType` and `CovenantPhase` are internal types (`pub(crate)`) used for script matching and PSET routing. They are not exposed in the public API but are described here as an implementation spec. Split per market kind because slot layouts differ (binary has 8 slots, multi-outcome has 5N+2 slots that are parameterized by outcome index):

```rust
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

pub(crate) enum SlotType {
    Binary(BinarySlotType),
    MultiOutcome(MultiOutcomeSlotType),
}

pub(crate) enum BinarySlotType {
    DormantYesRt,          // slot 0
    DormantNoRt,           // slot 1
    UnresolvedYesRt,       // slot 2
    UnresolvedNoRt,        // slot 3
    UnresolvedCollateral,  // slot 4
    ResolvedYesCollateral, // slot 5
    ResolvedNoCollateral,  // slot 6
    ExpiredCollateral,     // slot 7
}

pub(crate) enum MultiOutcomeSlotType {
    DormantYesRt(OutcomeIndex),      // slots 0..N-1
    DormantNoRt(OutcomeIndex),       // slots N..2N-1
    UnresolvedYesRt(OutcomeIndex),   // slots 2N..3N-1
    UnresolvedNoRt(OutcomeIndex),    // slots 3N..4N-1
    UnresolvedCollateral,            // slot 4N
    ResolvedCollateral(OutcomeIndex),// slots 4N+1..5N — one per outcome that could win
    ExpiredCollateral,               // slot 5N+1
}
```

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
        offered_amount: u64,
        total_filled: u64,
    },
    Consumed {
        final_txid: Txid,
        offered_amount: u64,
    },
    Cancelled {
        cancel_txid: Txid,
        offered_amount: u64,
        total_filled: u64,
    },
}
```

`offered_amount` is the total value the maker locked when the order was created. For `Creation` ingestion, this is the initial UTXO value (the true original). For `Current` ingestion, this is `locked_value` from the snapshot (the remaining at discovery time — the best available without history). `total_filled` is cumulative fills since ingestion. Both are denominated in the order's locked asset — the asset the maker offered (BASE for sell-base orders, QUOTE for sell-quote orders, per `MakerOrderParams.direction`). Remaining liquidity is `offered_amount - total_filled`.

`Active` enables "5,000 of 10,000 sats filled" display. `Consumed` stores `offered_amount` (which equals `total_filled` by definition — only one is needed). `Cancelled` enables "5,000 of 10,000 filled, then cancelled" display (if `total_filled == 0`, it was a clean cancellation). Outpoints are internal to the engine and not exposed in the public state.

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
    pub outpoints: Vec<OutPoint>,
    pub position: ChainPosition,
}
```

The mutable initial state of a contract at ingestion time, passed to the store via `track_contract`. Groups the two fields that describe "where the contract is right now": `outpoints` are the contract's current tracked UTXOs (derived from the creation transaction or provided via a `Current` snapshot), and `position` is the chain position at which those outpoints were confirmed.

This is intentionally separate from `DerivedContractData`, which contains permanent data derived from Simplicity compilation (scripts, asset IDs). `InitialContractState` contains mutable state — `outpoints` change with every transition (via `apply_transitions`), and `position` sets the initial `synced_to` height. The two structs represent different categories of data the engine pre-computes for the store.

**Outpoints per contract type** (positional ordering — index = slot identity):
- **Binary markets**: 2 outpoints `[DormantYesRt, DormantNoRt]` for initial Dormant state. In Unresolved: `[UnresolvedYesRt, UnresolvedNoRt, UnresolvedCollateral]`. In ResolvedYes/No/Expired: `[collateral_slot]`.
- **Multi-outcome markets** (with `N = outcome_count`): 2N outpoints `[DormantYesRt(0), ..., DormantYesRt(N-1), DormantNoRt(0), ..., DormantNoRt(N-1)]` for initial Dormant state. In Unresolved: 2N+1 outpoints `[UnresolvedYesRt(0..N-1), UnresolvedNoRt(0..N-1), UnresolvedCollateral]`. In Resolved(k) or Expired: `[collateral_slot]`.
- **Pools**: 3 outpoints `[YES reserve, NO reserve, Collateral reserve]`.
- **Orders**: 1 outpoint `[order UTXO]`.

**Positional ordering is a hard invariant**: The engine, store, and PSET builders all depend on `Vec<OutPoint>` index positions matching slot identity. The engine produces outpoints in this canonical order during ingestion (`InitialContractState`) and transitions (`StateUpdate.new_outpoints`). The store must preserve insertion order. `contract_outpoints` must return outpoints in the same positional order they were stored. PSET builders use the index to place the correct outpoint at the correct transaction input position for Simplicity witness encoding.

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

The engine-level resolve builder accepts a raw signature and identifies the resolution by trial verification — for binary markets, verifies against both `MarketResolution::Binary(Side::Yes)` and `MarketResolution::Binary(Side::No)`; for multi-outcome markets, verifies against `MarketResolution::MultiOutcome(OutcomeIndex::new(k))` for each `k` in `0..outcome_count`. If the signature doesn't verify against any valid resolution, the engine returns `CoreError::InvalidParams { detail: "oracle attestation does not verify against any resolution" }`.

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

// TODO: add ExactOutput(u64) variant when routing math supports it
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

**Basket trades and cross-outcome arb are NOT part of `TradeSpec`.** These are multi-outcome-specific operations exposed as dedicated builders (e.g., `MultiOutcomeMarket::build_split_yes_pset`, `build_cross_outcome_arb_pset`) rather than routed through the trade quote system. Each `TradeQuote` corresponds to a single-outcome trade; multi-outcome traders issuing a basket construct a composition of single-outcome trades plus market-contract-native primitives.

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

pub enum LiquiditySource {
    LmsrPool {
        pool_id: ContractId,
        old_s_index: u64,
        new_s_index: u64,
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

`TradeRoute` is a crate-internal type capturing the route plan (contract IDs, leg amounts, outpoint snapshots) needed by `build_trade_pset`. External consumers cannot inspect or construct it.

`RouteLeg` breaks down how the trade is routed across liquidity sources. `LiquiditySource::LmsrPool` includes s-index movement for "pool moved from 50 to 55" display. `LiquiditySource::LimitOrder` includes the matched price and base fill amount.

### BootstrapEstimate

Result of `estimate_bootstrap` — tells the operator how much capital they need before creating a pool:

```rust
pub struct BootstrapEstimate {
    pub initial_yes_reserve: u64,
    pub initial_no_reserve: u64,
    pub initial_collateral_reserve: u64,
    pub initial_s_index: u64,
}
```

The three reserves are in different assets (YES tokens, NO tokens, collateral). The operator uses these to plan capital acquisition — e.g., issuing `max(yes, no)` token pairs (which costs `max * collateral_per_pair` collateral from the parent market) plus providing `initial_collateral_reserve` directly. Total capital outlay depends on the market's `collateral_per_pair` and what the operator does with leftover tokens, which are wallet-layer concerns outside this function's scope.

`starting_price_bps` must be in the range (0, 10000) exclusive — 0 and 10000 are rejected (`CoreError::InvalidParams`) because they represent 0% and 100% probabilities with infinite reserve ratios. Values that cause integer overflow in the LMSR computation are also rejected. No further range restriction — the purpose of `estimate_bootstrap` is to let the caller evaluate capital requirements and decide for themselves whether the reserves are practical.

### ContractMatch

Returned by `ContractStore::find_by_outpoints`. Used internally by the engine to identify which tracked contracts are affected by a transaction and which specific outpoints matched. Callers of the engine never see this type — it exists at the engine-store boundary only.

```rust
pub struct ContractMatch {
    pub contract_id: ContractId,
    pub matched_outpoints: Vec<OutPoint>,
}
```

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

Single-contract transitions (one market, one pool, one order) are fully described by their respective `TransitionDetails` variant. Some transactions legitimately span multiple contracts atomically — a trade (pool + maybe maker orders), a cross-outcome arb (market + N pools), an atomic issuance + pool bootstrap (market + new pool creation). For these, the raw per-contract transitions are always preserved in `InterpretedTransaction.transitions`; additional helper methods on `InterpretedTransaction` classify recognized multi-contract patterns without collapsing the raw data:

```rust
impl InterpretedTransaction {
    /// If this tx realizes a cross-outcome arb (market's SplitYes or MergeYes atomically
    /// co-spent with matching swaps on every outcome's pool), returns structured arb
    /// details. Raw transitions remain available via `self.transitions`.
    pub fn as_cross_outcome_arb(&self) -> Option<CrossOutcomeArbRealized>;

    /// If this tx realizes a trade (pool swap plus zero or more LOB order fills),
    /// returns structured trade details.
    pub fn as_trade(&self) -> Option<TradeRealized>;

    /// Net change in token and collateral balances attributable to a specific contract
    /// from this transaction. Rolls up per-contract transitions and external outputs.
    /// Returns None if the contract had no involvement in this transaction.
    pub fn net_effect_for(&self, contract_id: &ContractId) -> Option<ContractNetEffect>;
}

pub struct CrossOutcomeArbRealized {
    pub market_id: ContractId,
    pub direction: ArbDirection,
    pub sets: u64,
    pub pool_legs: Vec<ArbPoolLegRealized>,     // one per outcome — which pool, what s-index move
    pub realized_profit_sats: i64,              // includes fee
}

pub struct ArbPoolLegRealized {
    pub outcome: OutcomeIndex,
    pub pool_id: ContractId,
    pub side_traded: Side,
    pub old_s_index: u64,
    pub new_s_index: u64,
    pub tokens_moved: u64,
}

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

**Single-contract transactions** still have `as_*` helpers return `None` — they return `Some` only when the tx exactly matches the multi-contract pattern. A tx that moved a pool's s_index but did nothing else (no market co-spend) produces a single `PoolTransition::Swapped` and `as_trade()` returns `None` (no taker was involved). A tx that consolidates an arb via market + pools returns `Some(CrossOutcomeArbRealized)` AND preserves the individual `MarketTransition::SplitYes` + N `PoolTransition::Swapped` in `self.transitions`.

**Transactions are interpreted independently.** The engine does not pattern-match across transactions to recognize user-level behaviors that span multiple txs. For example, a user-level "cross-outcome swap" (a SplitNo in tx 1 followed by a CancelledPair in tx 2 on the same market) produces two independent single-primitive interpretations. Higher-level tools that want to aggregate across tx history can do so externally; the core engine's job is single-tx classification.

### StateUpdate

The write-path type passed to the store via `apply_transitions`. Contains outpoints needed for state advancement and rollback, but NOT used on the read path (history queries return `HistoryEntry` which omits outpoints). Does not include the computed output classification (`external_outputs`) since those are derived from the transaction at query time and do not need to be persisted:

```rust
pub struct StateUpdate {
    pub contract_id: ContractId,
    pub txid: Txid,
    pub position: ChainPosition,
    pub old_outpoints: Vec<OutPoint>,
    pub new_outpoints: Vec<OutPoint>,
    pub details: TransitionDetails,
}
```

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
    // Per-outcome primitives (binary-analogous, outcome-indexed):
    IssuedPair { outcome: OutcomeIndex, pairs: u64, collateral_locked: u64 },
    CancelledPair { outcome: OutcomeIndex, pairs_burned: u64, collateral_returned: u64 },

    // Cross-outcome primitives (multi-outcome only):
    SplitYes { sets: u64, collateral_locked: u64 },
    MergeYes { sets: u64, collateral_returned: u64 },
    SplitNo { sets: u64, collateral_locked: u64 },
    MergeNo { sets: u64, collateral_returned: u64 },

    // Resolution / expiry / redemption:
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

**Each market transaction is exactly one primitive.** The multi-outcome market covenant enforces that every Unresolved-phase transition co-spends all 2N+1 covenant inputs and selects exactly one spend path from the witness. This means each on-chain transaction produces exactly one `MultiOutcomeMarketTransition` for the market contract — there is no in-transaction composition at the market-contract layer.

**User-level cross-outcome swaps are two-transaction compositions.** A user rotating 1 YES_i into `{NO_j : j ≠ i}` at a net cost of `(N-2) × collateral_per_pair` executes this across two separate transactions: (1) `SplitNo` (pay `(N-1) × collateral_per_pair`, receive 1 NO_k for each k) and (2) `CancelledPair { outcome: i }` (burn 1 YES_i + 1 NO_i, release `collateral_per_pair`). Each is interpreted independently as a single primitive transition. The engine does not pattern-match across transactions to emit a derived "cross-outcome swap" — each tx is processed individually by the UTXO-following state machine. Higher-level tools (wallets, explorers) can aggregate across a user's tx history to recognize the pattern if desired.

**Multi-contract patterns in a single transaction** (e.g., cross-outcome arb: market's split-YES atomically co-spent with N pool swaps) are detected at the `InterpretedTransaction` level via helper methods like `InterpretedTransaction::as_cross_outcome_arb()`. Each participating contract still emits one primitive transition; the tx-level helpers recognize recurring multi-contract patterns without collapsing the per-contract transitions. See [Multi-Contract Transaction Patterns](#multi-contract-transaction-patterns) below.

`PoolTransition::Swapped` corresponds to the LMSR covenant's swap path — someone traded through the pool, moving the s-index. `PoolTransition::Adjusted` corresponds to the admin path — the pool operator (with admin key signature) adjusted liquidity without changing the s-index. The covenant enforces that YES and NO token deltas are equal on the admin path; collateral can change independently. `PoolTransition::Closed` indicates the pool admin reclaimed all reserve UTXOs via the close script path. See [lmsr-pool-close-path.md](../contracts/lmsr-pool/lmsr-pool-close-path.md).

**Why nested by contract type**: When processing a market transition, the caller wants to match on market-specific variants without wading through pool and order cases. A flat enum mixing all contract types would force exhaustive matching across unrelated variants.

### ExternalOutput

Non-covenant outputs in a transaction. Each output carries shared fields (index, script, role) directly, with asset and value available only when the output is explicit (unblinded):

```rust
pub struct ExternalOutput {
    pub index: u32,
    pub script_pubkey: Script,
    pub role: OutputRole,
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

**Output consolidation**: PSET builders consolidate outputs that share the same script and asset into a single output for efficiency and privacy (see [Output Consolidation](#output-consolidation)). A `CollateralReturn` output may therefore include fee change. When exact amounts matter, use `TransitionDetails` — it is authoritative for semantic amounts (payout, tokens burned, collateral locked, etc.). `OutputRole` identifies *which* output serves a purpose; `TransitionDetails` provides *the precise numbers*.

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
    InvalidContractState { contract_id: ContractId, detail: String },
    ContractNotFound { contract_id: ContractId },
    ContractAlreadyTracked { contract_id: ContractId },
    InsufficientFunds { shortfalls: Vec<Shortfall> },
    NoLiquidity { market_id: ContractId },
    StaleQuote { detail: String },
}
```

`ChainSource` wraps errors from the `ChainSource` trait implementation during `step`. The chain error type is boxed rather than generic to keep the engine at a single generic parameter (`S`) — `step` introduces `C: ChainSource` only at the call site. The `ChainSource::Error` bound includes `Send + Sync + 'static` to enable boxing into `Box<dyn Error + Send + Sync>`. Integrators can display/debug the error or downcast if they need the concrete type. `InvalidParams` covers caller-provided inputs that violate covenant constraints (e.g., issuance amount exceeds limits, invalid collateral asset, pool/order referencing an unknown parent market, calling `oracle_attestation_spec` on a non-market contract). `InvalidContractState` is returned by PSET builders when the contract is in the wrong state for the requested operation (e.g., `build_issuance_pset` on a settled market, `build_redemption_pset` on a trading market). `InsufficientFunds` is returned by PSET builders when the caller's available UTXOs don't cover the required amounts — the `shortfalls` vec reports all insufficient assets at once (e.g., "need 50 more YES tokens AND 3,000 more sats"), enabling wallet UX that shows all missing resources rather than one at a time. `StaleQuote` is returned by `build_trade_pset` when the quote's snapshotted outpoints are no longer current (a `step` call consumed them between quoting and building) — the caller should re-quote. Internal construction errors (e.g., Pedersen commitment math failure) indicate bugs in core and panic rather than returning an error — every `CoreError` variant represents a condition the caller can meaningfully respond to.

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
    pub fn outcome_count(&self) -> u8;                    // 2 for binary, N for multi-outcome
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
    /// (cross-outcome splits/merges, cross-outcome arb). Returns `None` for binary markets.
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

    // ---- Cross-outcome arb (multi-contract atomic composition) ----

    /// Quote a cross-outcome arb opportunity if one exists. Returns `Ok(None)` if
    /// `profit_per_set ≤ 0` (i.e., cross-outcome prices already coherent). Returns
    /// `Ok(Some(quote))` with a snapshot of pool outpoints and the arb direction
    /// otherwise. Mirrors the `engine.quote_trade` → `engine.build_trade_pset` pattern.
    ///
    /// The engine picks the best-priced pool per outcome for the arb direction
    /// (cheapest for buys, most valuable for sells). Consumers wanting a specific
    /// pool selection can route manually via individual pool swap builders.
    pub fn quote_cross_outcome_arb(&self, fee_rate: FeeRate)
        -> Result<Option<ArbQuote>, CoreError<S::Error>>;

    /// Build the atomic PSET for an arb quote. Fails with `CoreError::StaleQuote` if the
    /// snapshotted pool outpoints in `quote` are no longer current (a `step` consumed them
    /// between quote and build).
    ///
    /// `sets` scales the arb volume; total profit is approximately
    /// `quote.profit_per_set_sats × sets - fee`. Caller can use
    /// `quote.break_even_sets()` to pick a minimum-viable `sets`.
    pub fn build_cross_outcome_arb_pset(
        &self,
        quote: &ArbQuote,
        sets: u64,
        funding: &WalletFunding,
    ) -> Result<UnblindedPset, CoreError<S::Error>>;

    // ---- Conversion back ----
    /// Returns the general `Market` view, for operations that apply to both market kinds.
    pub fn as_market(&self) -> Market<'a, S>;
}

pub enum ArbDirection {
    /// Σ p_YES_k > 10000 bps: split-YES via market (pay 1×cp), sell each YES_k to its
    /// pool. Receive Σ p_YES_k × cp. Profit: (Σ p − 10000) × cp / 10000 per set.
    SplitAndSell,

    /// Σ p_YES_k < 10000 bps: buy each YES_k from its pool (pay Σ p × cp), merge-YES
    /// via market (receive 1×cp). Profit: (10000 − Σ p) × cp / 10000 per set.
    BuyAndMerge,
}

pub struct ArbQuote {
    // Public — for display / decision-making:
    pub direction: ArbDirection,
    pub sum_of_probabilities_bps: u64,       // > 10000 → SplitAndSell; < 10000 → BuyAndMerge
    pub profit_per_set_sats: i64,            // > 0 if quote returned Ok(Some); caller scales by `sets`
    pub estimated_fee_sats: u64,             // tx weight × fee_rate, single-tx cost
    pub pool_legs: Vec<ArbPoolLeg>,          // length outcome_count — shows which pool per outcome

    // Crate-internal snapshot (pool outpoints + s-indices) — staleness-checked at build time.
    pub(crate) snapshot: ArbSnapshot,
}

pub struct ArbPoolLeg {
    pub outcome: OutcomeIndex,
    pub pool_id: ContractId,
    pub side_traded: Side,            // Yes for SplitAndSell, No inferred on BuyAndMerge
    pub pool_price_bps: u16,          // this pool's contribution to Σ p
}

impl ArbQuote {
    /// Minimum `sets` required for net profit ≥ 0 after fees. Returns None if fees
    /// exceed profit at all scales (shouldn't happen since the quote returned Ok(Some),
    /// but guards against edge cases at extremely large fees).
    pub fn break_even_sets(&self) -> Option<u64>;

    /// Net profit in collateral sats at a chosen `sets` count, including estimated fees.
    /// Returns signed — may be negative below break-even.
    pub fn net_profit_at_sets(&self, sets: u64) -> i64;
}
```

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

The internal `CovenantPhase` maps to a unique set of slot script pubkeys (see [SlotType and CovenantPhase](#slottype-and-covenantphase-internal)). The transition type is determined by which slot scripts the new outputs match, combined with witness-based path detection where output matching is ambiguous.

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

**Why witness-based for all pool transitions**: The engine needs the new `s_index` on every pool transition (it's stored in `LmsrPoolState::Active`). Deriving s_index from reserve values (reverse LMSR table lookup) is fragile — admin adjustments change reserves without moving along the LMSR curve, so the reserves no longer correspond to a single point on the curve. The witness contains the exact `old_s_index` and `new_s_index` used in the covenant verification — this is ground truth, not a derived estimate. Additionally, output-only detection cannot reliably distinguish close from swap/admin (wallet outputs can mimic the covenant window pattern). Witness parsing resolves all ambiguities definitively for a negligible cost (~<1ms per `RedeemNode::decode` call, at most once per pool per block).

**Pool transition detection algorithm**:

1. **Parse witness**: Extract the Simplicity program bytes and witness bytes from the spending transaction's witness stack for the input that spent a tracked pool outpoint. Call `RedeemNode::decode` to identify the spend path (swap, admin, or close) and extract `old_s_index` and `new_s_index`.
2. **Switch on spend path**:
   - **Swap or Admin**: Find the covenant output window — three consecutive explicit outputs (as enforced by the covenant) where index N has the pool's YES asset ID, N+1 has the NO asset ID, N+2 has the Collateral asset ID, and all three share the same script pubkey (co-membership). The window must exist (covenant-enforced for swap/admin paths). Read reserve values from the explicit outputs. Classify: `new_s_index != old_s_index` → `Swapped`, `new_s_index == old_s_index` → `Adjusted`.
   - **Close**: No covenant output window expected. The pool transitions to `Closed`. `final_reserves` from the stored state at time of closure.

Transition details:

- **Swap**: `old_s_index` and `new_s_index` from the witness. `old_reserves` from stored state. `new_reserves` from explicit output values.
- **Adjustment**: `old_s_index == new_s_index` confirmed by the witness (s_index frozen on admin path). `old_reserves` and `new_reserves` from stored state and output values.
- **Closure**: Spend path confirmed as close by the witness. All pool outpoints consumed, no new covenant outputs. `final_reserves` from the stored state at time of closure.

**Detection strategy summary:**

| Transition | Detection method | Airtight? |
|---|---|---|
| Swap | Witness: spend path + s_index extraction. Outputs: reserve values from covenant window. | Yes — witness is ground truth |
| Admin adjust | Witness: spend path + s_index unchanged. Outputs: reserve values from covenant window. | Yes — witness is ground truth |
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

**Orders** (`ingest_order`): Support both creation-tx and non-initial ingestion via `OrderSnapshot`. Takers need only the current state for filling — order history is irrelevant. Makers who need fill history (monitoring, recovery) use `OrderSnapshot::Creation`.

### What each snapshot variant provides

| Snapshot | History | Verification | Use case |
| -------- | ------- | ------------ | -------- |
| `Creation(ChainTransaction)` | Full — forward-sync from creation recovers all transitions | Creation tx verified against params | Makers, pool operators, anyone needing price history |
| `Current { ... }` | None — no prior transitions recoverable | No verification back to creation | Takers, traders who only need current state |

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
let order_id = engine.ingest_order(&order_params, OrderSnapshot::Current { ... })?;

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
// After confirmation: ingest and step catches it up
let order_id = engine.ingest_order(&params, OrderSnapshot::Creation(creation_tx))?;
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
    fn contract_outpoints(&self, contract_id: &ContractId) -> Result<Vec<OutPoint>, Self::Error>;

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
    pub outpoints: Vec<OutPoint>,
    pub synced_to: u32,
}
```

Every consumer must implement this. Read methods take `&self`, write methods take `&mut self` — mirroring the engine's own borrow semantics. The engine calls read methods during interpretation (`&self` on the engine borrows the store as `&self`) and write methods during processing (`&mut self` on the engine borrows the store as `&mut self`).

`apply_transitions` must be durable when it returns — the engine depends on this for crash safety. It must also be **idempotent**: calling it twice with the same `StateUpdate` (same `contract_id` + `txid`) must be a no-op on the second call. This is required because `process_transaction` is idempotent, which flows through to `apply_transitions`. For stores implementing `ContractHistory`, idempotency means avoiding duplicate history entries — the store should check whether a transition for the given `(contract_id, txid)` already exists before inserting.

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

**Atomicity requirements**: Contract-level atomicity is a hard requirement — a single contract's state update (old outpoints -> new outpoints + state change) must be all-or-nothing. A half-updated contract is corrupted state. Transaction-level atomicity (all contracts updated together for a multi-contract transaction) is recommended but not strictly required for correctness. A "jagged" state where one contract has processed a transaction but another hasn't is indistinguishable from staggered ingestion — which is already a normal condition when contracts are discovered at different times. The system self-heals: re-processing the transaction advances the remaining contracts while already-processed contracts are a no-op (idempotency). Transaction-level atomicity is recommended because it's typically not much extra burden on top of the already-required contract-level atomicity (e.g., a single SQLite transaction) and avoids the jagged-view window.

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
| `build_cross_outcome_arb_pset` | Multi-contract atomic arb (co-spends market + N pools) | Market Trading → Trading + pool s_index moves |

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
- **Cross-outcome arb** (multi-outcome): single atomic transaction that co-spends the market contract's split-YES (or merge-YES) path with each outcome's binary LMSR pool swap path. Closes cross-outcome price coherence gaps (`Σ p_YES_k ≠ 1`) in one tx. The engine constructs the full multi-contract PSET internally.
- **Creation builders**: return the full derived params (`BinaryMarketParams` / `MultiOutcomeMarketParams`) alongside the PSET so the caller can `ingest_market` after the creation transaction confirms. `build_binary_market_creation_pset` selects 2 defining inputs; `build_multi_outcome_market_creation_pset` selects 2N defining inputs and compiles the N-specific generated `.simf` covenant.

**OP_RETURN recovery hints**: `build_binary_market_creation_pset` includes a ~37-byte zero-value OP_RETURN hint; `build_multi_outcome_market_creation_pset` includes a ~40-byte hint (with `outcome_count` field). All the 4N asset IDs for multi-outcome markets are derivable from the creation transaction's issuance entropy — the hint doesn't scale with N. See [chain-only-recovery.md](../protocol/chain-only-recovery.md) and [multi-outcome-market-contract.md](../contracts/multi-outcome/multi-outcome-market-contract.md).

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
- **`build_lmsr_bootstrap_pset`**: includes a **41-byte** zero-value OP_RETURN recovery hint containing: market creation txid, `max_loss_sats` and `half_payout_sats` (9-bit encoded: 26-value mantissa x 10^exponent, supporting non-L-BTC assets), `fee_bps` (u12, 0.01% granularity), `initial_s_index` (u16, the starting table index for script verification during recovery), and XOR-masked pool operator derivation index. All other covenant params are derived via deterministic table generation. See [Wallet Recovery](#wallet-recovery), [chain-only-recovery.md](../protocol/chain-only-recovery.md), and [lmsr-pool-design.md](../contracts/lmsr-pool/lmsr-pool-design.md).

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

The engine computes the optimal route across all available pools and orders for the market, minimizing total cost to the taker including transaction fee overhead. The `fee_rate` parameter is required because the routing algorithm uses fee-adjusted effective prices — each liquidity source's activation cost (transaction weight) is weighted by the fee rate to determine whether including it improves the route. The routing algorithm uses pool-subset enumeration combined with fee-aware greedy order selection — see [trade-routing-algorithm.md](trade-routing-algorithm.md) for the full specification. Returns a `TradeQuote` representing the best available fill, including `estimated_fee` computed from the route's total transaction weight and the provided fee rate. Returns `Err(CoreError::NoLiquidity)` only when zero liquidity is available; any positive fill returns `Ok` (see [TradeQuote](#tradequote-and-related-types) for partial fill handling).

**Step 2: Build** (engine method):

```rust
pub fn build_trade_pset(
    &self,
    quote: &TradeQuote,
    funding: &WalletFunding,
) -> Result<PartiallySignedTransaction, CoreError<S::Error>>;
```

Takes the accepted quote and the caller's wallet funding. The engine validates that `funding.fee_rate` matches the fee rate used during quoting — if they differ, it returns `CoreError::InvalidParams` because the route was optimized for the quote's fee rate (a different rate could make the route suboptimal; the caller should re-quote with the current rate). The engine recompiles contracts from stored params, selects the needed UTXOs, computes the actual fee from the real transaction weight, and builds the PSET. The actual fee may differ from `TradeQuote.estimated_fee` because the quote's weight model assumes a single wallet input, while coin selection may add more — display the quote's fee as an estimate, not a guarantee. The quote captures a snapshot of all contract state needed at quote time (outpoints, route parameters). If the underlying contracts change between quoting and building (a `process_transaction` call consumed the snapshotted outpoints), the engine returns `CoreError::StaleQuote` — the caller should re-quote. If the quote is still valid at build time but the transaction later fails on-chain (spent inputs due to a block arriving between build and broadcast), the caller re-quotes. This is standard trading UX — quotes are inherently ephemeral.

See [Trade Types](#trade-types) and [TradeQuote](#tradequote-and-related-types) for full type definitions.

**Why trades use a two-step pattern**: Trade is the only operation requiring cross-contract route optimization from engine state. The two-step pattern enables the standard trading UX of "show quote, user confirms, then build." All other PSET builders are single-step — the caller provides the operation params directly and gets a PSET back. They don't need a quoting step because they operate on a single contract whose state the caller already knows.

### Confidential Transaction Blinding

On Liquid, transaction outputs can be **explicit** (asset and value visible) or **confidential** (hidden behind Pedersen commitments with range and surjection proofs). The three Deadcat covenants require all covenant outputs (collateral, reserves, order locked value) to be **explicit** — the Simplicity programs use `unwrap_right()` on output introspection jets, which fails on confidential outputs. The one exception is reissuance token (RT) outputs, which Elements requires to be blinded for reissuance mechanics to work.

**Which builders need RT blinding**: The 5 prediction market builders that involve RT outputs (`build_creation_pset`, `build_issuance_pset`, `build_cancellation_pset`, `build_oracle_resolve_pset`, `build_expire_transition_pset`). The remaining 7 builders have no RT involvement — their covenant outputs are all explicit and require no blinding by core.

**Deterministic RT blinding**: RT blinding factors are derived deterministically from public on-chain data (see [deterministic-rt-blinding.md](../protocol/deterministic-rt-blinding.md)), not generated randomly. This is essential for core's architecture: the engine internally manages RT outpoints and must reconstruct blinding factors when building future PSETs that spend those outpoints. With deterministic derivation, the engine recomputes the factors on demand without needing to persist blinding secrets.

**`UnblindedPset` newtype**: The 5 RT-involving builders return `UnblindedPset` — an opaque type whose private fields capture the explicit PSET, deterministic RT blinding factors, and all input secrets (both covenant inputs with zero blinding factors and wallet inputs with real blinding factors from `UnblindedUtxo`). The type enforces that the caller cannot extract a `PartiallySignedTransaction` without going through a blinding method, making "forgot to blind" unrepresentable at the type level.

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

## Thread Safety

Write methods on the engine (`step`, `ingest_market`, `ingest_pool`, `ingest_order`, `untrack_contract`, `rollback_to_height`, `prune_finalized`) take `&mut self`. Read methods on the engine (`interpret_transaction`, `identify_asset`, `contract`, `list_markets`, `list_pools`, `list_orders`, `market`, `pool`, `order`, `quote_trade`, and all engine-level PSET builders — creation and trade) take `&self`. View types (`Market`, `MultiOutcomeMarket`, `Pool`, `Order`) are constructed from `&self` engine methods and hold `&'a ContractEngine<S>` internally — all their methods (state accessors, PSET builders, oracle helpers, relationship queries, history) are effectively `&self` reads as far as the engine is concerned. While any view is alive, the engine cannot be mutably borrowed.

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

The key functions (currently in `src-tauri/crates/deadcat-sdk/src/lmsr_pool/math.rs`, will move to `deadcat-core`):

- `fee_free_yes_spot_price_bps(manifest, params, s_index)` — implied probability at a given state
- `quote_from_table(trade_kind, old_s_index, new_s_index, ...)` — deterministic quote from F-value table lookup
- `quote_exact_input_from_manifest(manifest, params, trade_kind, s_index, input)` — best trade for a given input amount
- `generate_lmsr_table(b, half_payout_sats, q_step_lots)` — deterministic integer-only F-value generation (see [Deterministic Table Generation](../contracts/lmsr-pool/lmsr-pool-design.md#deterministic-table-generation))
- `lmsr_table_root(values)` — Merkle root from table values

Types: `LmsrTradeKind` (BuyYes, SellYes, BuyNo, SellNo), `LmsrQuote` (full trade result with reserve deltas), `LmsrTableManifest` (in-memory table: depth + F-values vector).

These have zero dependencies beyond basic math — no wallet, chain, or state. All functions that require `b` derive it internally from `LmsrPoolParams.max_loss_sats` — callers never provide `b` directly.

**Point evaluation vs full table**: The quoting hot path (`quote_trade`) does NOT need the full 65K-entry F-value table. It evaluates the cost function at specific points (~1us per evaluation, ~16us for a binary search over the table index range). The full table is only needed for Merkle proof generation (`build_trade_pset`, `build_lmsr_bootstrap_pset`) and pool ingestion verification — infrequent, user-initiated operations where ~80ms generation time is acceptable. This means `quote_trade` evaluating 5 candidate pools costs ~80us total, not ~400ms. No table caching is needed for the quoting path.

**Note for implementors**: After the move to `deadcat-core`, this section should be updated with the final type definitions and full function signatures. The `generate_lmsr_table` function must use a deterministic integer-only algorithm (no floating point) to ensure bit-identical F-values across all platforms — the specific algorithm is defined in the implementation. The SDK path above will no longer be valid post-migration. The liquidity parameter `b` is derived from `LmsrPoolParams.max_loss_sats` via `b = max_loss_sats / ln(2)` (using the deterministic integer algorithm). All LMSR functions that need `b` derive it from the stored `max_loss_sats` — it is never stored or passed separately.

**Deterministic table specification required**: The derivation chain `max_loss_sats → b → q_step_lots → F-values → Merkle root` involves transcendental constants (`1/ln(2)`, `ln(999)`) and a cost function (`b × ln(exp(s/b) + exp(-s/b))`) that must be evaluated using integer-only arithmetic. Cross-implementation determinism requires a formal specification defining: exact rational approximations for all transcendental constants, the fixed-point algorithm for F-value computation (precision, series terms, rounding mode), the Merkle tree construction algorithm (hash function, leaf encoding, extracted from the `.simf` verification code), and test vectors. This will be a separate satellite document — see [lmsr-pool-design.md](../contracts/lmsr-pool/lmsr-pool-design.md) for background.

## Key Derivation Convenience Functions

`derive_order_params` and `derive_pool_params` are standalone functions (not engine methods) that accept the deadcat xprv (`elements::bitcoin::bip32::Xpriv` at HD path `m/purpose'/deadcat'`) and encapsulate all key derivation, nonce computation, and index masking internally:

```rust
pub fn derive_order_params(
    deadcat_xprv: &Xpriv,
    market_params: &BinaryMarketParams,
    order_index: u16,
    side: Side, direction: OrderDirection,
    price: u64, min_fill_lots: u8, min_remainder_lots: u8,
) -> Result<(MakerOrderParams, u16 /* masked_index */), ConventionError>;

pub fn derive_pool_params(
    deadcat_xprv: &Xpriv,
    market_params: &BinaryMarketParams,
    pool_index: u16,
    max_loss_sats: u64, half_payout_sats: u64, fee_bps: u16,
    starting_price_bps: u16,
) -> Result<(LmsrPoolParams, u16 /* masked_index */), ConventionError>;
```

Both functions validate OP_RETURN convention constraints before deriving parameters, returning `ConventionError` if the inputs cannot be losslessly encoded in the recovery hint. `derive_order_params` validates: `price <= 0xFFFFFF` (u24), `min_fill_lots >= 1`, `min_remainder_lots >= 1`. `derive_pool_params` validates: `max_loss_sats` and `half_payout_sats` in the 26-value mantissa × 10^exponent set, `fee_bps <= 4095` (u12), `starting_price_bps` in (0, 10000) exclusive. The `starting_price_bps` parameter is needed to compute `initial_s_index` for the XOR mask context (see [chain-only-recovery.md](../protocol/chain-only-recovery.md)) — the mask includes `initial_s_index`, which is derived from `starting_price_bps` via the inverse logistic function. `ConventionError` is a simple error type (separate from `CoreError`) with a descriptive message indicating which constraint was violated.

**`initial_s_index` coupling**: Three functions compute `initial_s_index` from `starting_price_bps`: `estimate_bootstrap` (for UI display), `derive_pool_params` (for the mask context), and `build_lmsr_bootstrap_pset` (for the covenant script and OP_RETURN). All three **must** use a single canonical internal function to guarantee bit-identical results. A divergence would cause silent recovery failure (mask mismatch between creation and recovery). The PSET builders also validate these constraints (defense in depth for manually-constructed params), but the derive functions are the natural first line — catching violations at the point where the caller is making the decision.

Internally, each function derives from the xprv:
- **`deadcat_secret_key`** at `m/purpose'/deadcat'/secret'` — a single key used for all HMAC operations (nonce derivation, index masking). Different HMAC tags (`"deadcat/order_nonce"`, `"deadcat/order_mask"`, `"deadcat/pool_mask"`) provide full domain separation.
- **Per-instance public key** at `m/purpose'/deadcat'/orders'/i` or `m/purpose'/deadcat'/pools'/i` — the `maker_pubkey` or `admin_pubkey` baked into the covenant.

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
| Derive functions | `derive_order_params`, `derive_pool_params` | Convention violations at param construction time (first line — best error UX) |
| PSET builders | All three creation builders | Convention violations for manually-constructed params (defense in depth) |
| Market ingestion | `ingest_market` | Non-conforming markets from external sources (protects all downstream users) |

Market conventions are enforced at ingestion because non-conforming markets break ALL downstream users — token holders, order makers, and pool operators all trace back to the market's OP_RETURN for chain-only recovery. Pool and order conventions are enforced at creation time only (derive functions + builders) because they affect only the creator's own recovery. `ingest_pool` and `ingest_order` do NOT enforce pool/order-specific conventions — a non-conforming pool or order created by a custom tool is still fully functional for trading, and rejecting it at ingestion would prevent takers from using valid liquidity. Pool and order ingestion does validate the parent market relationship (transitively ensuring the parent market is conforming). See [chain-only-recovery.md](../protocol/chain-only-recovery.md) for the full recovery specification.

**Wallet-funded prerequisite**: OP_RETURN recovery hints are found by scanning wallet-funded transactions. Token holder recovery uses `ChainSource::issuance_transaction` to trace asset IDs back to their creation transactions. Both paths are chain-only — no external services.

**OP_RETURN properties**: All recovery hints use zero-value OP_RETURN outputs, which are both consensus-valid and relay-standard on Elements. Hints use compressed encodings (standard denomination conventions, well-known asset indices, hybrid time encoding) to minimize size. The hints are always included; there is no opt-out. Derivation indices are XOR-masked for privacy. See [chain-only-recovery.md](../protocol/chain-only-recovery.md) for encoding details.

### YES/NO Token Positions

Token recovery is automatic. YES and NO tokens are standard Elements confidential assets held at the wallet's own addresses. The wallet's normal mnemonic-based rescan (gap-limit scan over derived scriptpubkeys) finds them the same way it finds L-BTC UTXOs. No deadcat-specific recovery logic is needed.

**Labeling and redemption** require market ingestion. The wallet discovers it holds a UTXO with an unfamiliar asset ID, but doesn't know it's a "YES token for market X" until the market's `BinaryMarketParams` are available and the market is ingested. The recovery path: `asset_id` → `ChainSource::issuance_transaction(asset_id)` → market creation tx → read OP_RETURN → reconstruct market params → `ingest_market` → `identify_asset` for labeling, `build_redemption_pset` for redemption. One chain query per unique asset ID. This works for **all** token holders, including pure takers who only traded through existing pools and never created any contracts. See [chain-only-recovery.md](../protocol/chain-only-recovery.md) for details.

### Prediction Market Positions

Markets have no on-chain "owner" — the taproot internal key is NUMS. However, `build_creation_pset` includes an OP_RETURN recovery hint in the market creation transaction. This serves two purposes: (1) enabling the market creator to re-discover and re-announce their market, and (2) providing the anchor for chain-only pool and order recovery — pool and order hints point to the market creation transaction by txid. It also enables token holder recovery: `issuance_transaction(asset_id)` traces any YES/NO token back to this transaction.

**37 bytes** (known collateral asset) / **69 bytes** (exotic collateral). Uses compressed encoding: 4-bit well-known collateral asset index (L-BTC=0, USDt=1, escape=15), 4-bit 1-2-5 denomination convention for `collateral_per_pair`, and absolute `expiry_time` as u24 (block height divided by 60, giving hour-level granularity with range from the Liquid genesis block to approximately the year 3931). The builder snaps `expiry_time` to the nearest 60-block boundary — the covenant uses the snapped value, making the encoding lossless. Only 4 of 8 `BinaryMarketParams` fields need encoding — the other 4 (token and RT asset IDs) are derivable from the creation transaction's issuance entropy. See [chain-only-recovery.md](../protocol/chain-only-recovery.md) for the exact byte layout and per-field justification.

### Maker Order Positions

Maker order UTXOs are NOT at wallet addresses — they're at covenant addresses (taproot scripts derived from the Simplicity covenant with order params baked in). The wallet's standard rescan does not find them. The maker's key is derived from the mnemonic, but the covenant script depends on the full order params (market, price, direction, deterministic nonce). Without the params, the output key cannot be computed.

Maker orders are the only contract type directly "owned" by regular end users (as opposed to markets and pools, which are created by operators in managerial roles). This makes mnemonic-only recovery especially important — regular users should not be expected to maintain stateful backups.

**40 bytes**. Includes: XOR-masked derivation index (for O(1) key recovery + observer privacy), market creation txid (chain-only market param recovery), price (u24, bounded by `collateral_per_pair`), min_fill_lots and min_remainder_lots (u8 each, range 1-255), and side + direction packed into the type tag byte. The `derive_order_params` function encapsulates all key derivation — callers pass the deadcat xprv + `order_index`, and the maker pubkey, canonical nonce, and masked index are computed internally (see [Key Derivation Convenience Functions](#key-derivation-convenience-functions)).

The builder validates: `price <= 2^24`, `min_fill_lots` and `min_remainder_lots` in range 1-255, `order_index <= 65535`, and parent market conforms to conventions. See [chain-only-recovery.md](../protocol/chain-only-recovery.md) for the exact byte layout, per-field inclusion/compression justification, recovery flow, and XOR masking specification.

### LMSR Pool Positions

Like maker orders, pool reserve UTXOs are at covenant addresses — the wallet's standard rescan does not find them. The operator derives their admin key from the mnemonic, but the admin pubkey alone is insufficient to find the pool on-chain — the covenant scripts also depend on liquidity parameters and the s_index (which changes on every swap, making script enumeration impractical).

**41 bytes**. Uses compressed encoding: `max_loss_sats` and `half_payout_sats` as 9-bit 26-value mantissa x 10^exponent (supports non-L-BTC collateral assets like USDT), `fee_bps` as u12 (0.01% granularity, max 40.95%), `initial_s_index` as u16 (the starting table index — enables direct script verification during recovery without reverse-deriving from reserves), plus XOR-masked pool operator derivation index. All other covenant params are derived: `b` from `max_loss_sats`, `q_step_lots` from `b` and `half_payout_sats`, `lmsr_table_root` from deterministic F-value generation, token asset IDs from the parent market, admin pubkey from the mnemonic at `pool_index`. Protocol constants (`TABLE_DEPTH`, `S_BIAS`, `S_MAX_INDEX`, `MIN_POOL_RESERVE`) require no encoding. See [chain-only-recovery.md](../protocol/chain-only-recovery.md) for the exact byte layout, per-field justification, and recovery flow.

### Oracle Market Discovery

An oracle derives their key from the mnemonic and re-discovers markets referencing that key via Nostr (filtering market announcements by `oracle_public_key`), or by scanning market OP_RETURN hints in wallet-funded transactions for their oracle pubkey. The oracle's role is limited to resolution — a managerial role where maintaining backups of market params is a reasonable expectation.

### Cost Amortization

Market and pool creations are infrequent lifecycle events — the OP_RETURN cost (37-41 bytes at typical Liquid fee rates) is paid once and amortized over the entire lifetime of the contract (every trade, fill, adjustment, and redemption that follows). Maker order creation is the most frequent user-facing operation with an OP_RETURN, but the cost is negligible relative to the order value and trade fees. The OP_RETURN cost is never paid by market takers or regular traders — only by contract creators.

### Recovery Summary

| Position | Recovery mechanism | Hint size | Chain-only? |
|---|---|---|---|
| YES/NO tokens | Standard wallet rescan + `issuance_transaction` for labeling/redemption | — | Yes |
| Prediction markets | OP_RETURN in creation tx | 37 bytes (known asset) / 69 bytes (exotic) | Yes |
| Maker orders | OP_RETURN in creation tx → market hint chain | 40 bytes | Yes |
| LMSR pools | OP_RETURN in creation tx → market hint chain | 41 bytes | Yes |

All user types — market creators, order makers, pool operators, and pure token holders — achieve chain-only recovery. Discovery (Nostr) is only needed for human-readable metadata, not fund recovery.

**Core's role in recovery**: Core provides `identify_asset` for token labeling, `derive_order_params` and `derive_pool_params` for deterministic param reconstruction (both accept the deadcat xprv and encapsulate all key derivation, nonce computation, and index masking internally — see [Key Derivation Convenience Functions](#key-derivation-convenience-functions)), Simplicity compilation for contract verification, `ingest_*` for re-tracking, and convention enforcement (builder validation + ingestion rejection of non-conforming markets). The `ChainSource::issuance_transaction` method enables token holder recovery. See [chain-only-recovery.md](../protocol/chain-only-recovery.md) for the complete specification.

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

**What**: Every standalone function and deterministic computation — LMSR math (point evaluation, table generation, quoting), key derivation (`derive_order_params`, `derive_pool_params`), oracle attestation messages, OP_RETURN encoding/decoding (byte layout round-trips), expiry time snapping (block height → u24 → block height), XOR index masking/unmasking, CBF derivation chain, `BootstrapEstimate` computation, `FeeRate` conversions, pagination cursor encoding.

**How**: Standard `#[test]` functions, zero dependencies beyond core itself. Property-based tests (e.g., `proptest`) for encoding invariants — "for any valid params, `decode(encode(params)) == params`" generates thousands of random inputs and provides much stronger guarantees than hand-picked examples. Particularly valuable for OP_RETURN round-trips, LMSR point evaluation vs full table consistency, and XOR masking/unmasking.

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

### Each Market Transaction Is One Primitive; Multi-Contract Patterns Are Detected at the Transaction Level

**Chosen**: `MultiOutcomeMarketTransition` variants are all single-primitive (IssuedPair, CancelledPair, SplitYes, MergeYes, SplitNo, MergeNo, plus terminals). Multi-contract patterns that span multiple contracts atomically in one transaction (cross-outcome arb, trades) are detected and exposed via helper methods on `InterpretedTransaction` (`as_cross_outcome_arb`, `as_trade`).

**Rejected**:
- **A `CrossOutcomeSwap` variant on `MultiOutcomeMarketTransition`** (proposed earlier during Stage 1 design): incorrect. The multi-outcome market covenant enforces that every Unresolved-phase transition co-spends all 2N+1 covenant inputs and selects exactly one spend path — one primitive per transaction. A user-level cross-outcome swap (rotating YES_i into {NO_j : j ≠ i}) requires TWO separate transactions (SplitNo then CancelledPair), so no single transaction produces a `CrossOutcomeSwap` transition for a single market.
- **A `Composite(Vec<Primitive>)` variant within `MultiOutcomeMarketTransition`**: also incorrect — there's no in-transaction composition at the single-market-contract level because the covenant allows only one spend path per transaction.

**Why**: matching the covenant's structure keeps single-contract transitions honest (each transition records exactly what that contract's covenant did in this transaction). Multi-contract patterns are real — they happen when one transaction co-spends multiple contracts atomically (cross-outcome arb, trades with LOB fills). These get first-class detection at the transaction level, via helpers on `InterpretedTransaction`. The raw per-contract transitions remain available in `InterpretedTransaction.transitions` for consumers that want granular detail.

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

### Cross-Outcome Arb: Quote + Build Pattern Mirroring Trades

**Chosen**: `MultiOutcomeMarket::quote_cross_outcome_arb(fee_rate) → Option<ArbQuote>` + `build_cross_outcome_arb_pset(quote, sets, funding)`. `quote_*` returns `Ok(None)` only when raw profit-per-set is ≤ 0 (no price imbalance). Fee viability is the caller's check via `ArbQuote::break_even_sets()` and `net_profit_at_sets()`.

**Rejected**:
- Single builder `build_cross_outcome_arb_pset(direction, sets, funding)` with no quote step: no snapshot → no staleness check; pool states could change between caller's mental check and build.
- Quote returns `None` if fees exceed profit at any volume: fee is fixed per-tx, profit scales with sets; large-enough volumes always amortize fee. Caller knows their desired volume better than the engine does.

**Why**: the quote + build pattern mirrors `engine.quote_trade` + `engine.build_trade_pset` — familiar shape, same staleness protection (`CoreError::StaleQuote` when snapshotted outpoints change between quote and build). `break_even_sets` helper surfaces fee viability without making the engine pick a volume for the caller.

### View Types for Per-Contract Operations

**Chosen**: `ContractEngine` exposes per-contract operations via view types (`Market<'a, S>`, `Pool<'a, S>`, `Order<'a, S>`, `MultiOutcomeMarket<'a, S>`) returned by engine accessors (`engine.market(id)`, etc.). Each view holds `&'a ContractEngine<S>` plus cached `(params, state)`. Per-contract PSET builders, state accessors, oracle helpers, and relationship queries live on the views; engine surface shrinks to ingestion, chain sync, discovery/listing, asset identification, transaction interpretation, creation builders, and trade routing.

**Rejected**:
- **Flat engine surface with `contract_id` on every method**: original design. 25+ methods on the engine, mostly disambiguated by `contract_id` as first argument. IDE autocomplete is noisy, operations are mixed across contract kinds, discoverability suffers, `contract_id` repeated at every call site.
- **Free functions with `&engine` parameter**: e.g. `build_issuance_pset(&engine, &contract_id, pairs, ...)`. Avoids the engine-method bloat but loses method-call ergonomics and bundled-context benefits.
- **Trait-based dispatch (e.g. `ContractOps::build_issuance_pset`)**: adds type machinery for minimal gain over direct `impl` blocks on concrete view structs.

**Why**: operations naturally cluster by the object they operate on. A `Market` view groups everything you can do with a market (issue, cancel, resolve, expire, redeem, query state, fetch pools, fetch orders, oracle attestation helpers, and for multi-outcome via `as_multi_outcome()`: split/merge YES/NO and cross-outcome arb). This is idiomatic Rust API design (similar patterns in `std::fs::File`, `hyper::Client`, etc.) and it enables type-level dispatch for specializations (`MultiOutcomeMarket` only exists for multi-outcome markets — binary markets can't accidentally call `build_split_yes_pset`).

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
- **Stage 3 (complete)**: behavior — multi-outcome transaction interpretation (witness-based disambiguation across per-outcome and cross-outcome primitives), multi-contract pattern detection on `InterpretedTransaction` (`as_cross_outcome_arb`, `as_trade`, `net_effect_for`) while preserving raw per-contract transitions, probability accessors on views (liquidity-weighted by LMSR `b`), cross-outcome arb quote/build pattern mirroring trade routing, chain sync notes for N-scaling. Corrected a Stage 1 error: removed the `CrossOutcomeSwap` variant from `MultiOutcomeMarketTransition` — at the single-market-contract layer, every transaction is exactly one primitive.

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

### Contract-Level Atomicity Required, Transaction-Level Recommended

**Chosen**: Store must apply each contract's state update atomically. Applying all contracts from a multi-contract transaction atomically is recommended but not required.
**Rejected**: Requiring strict transaction-level atomicity.
**Why**: Contract-level atomicity is non-negotiable — a half-updated contract (outpoints changed but state not, or vice versa) is corrupted state. Transaction-level atomicity (all contracts in one tx updated together) is a "nice to have" for view consistency but not a correctness requirement. A "jagged" state where one contract has processed a tx but another hasn't is indistinguishable from staggered ingestion — which is already a normal condition when contracts are discovered at different times. Re-processing the transaction advances the remaining contracts (idempotency), and already-processed contracts are a no-op. Transaction-level atomicity is recommended because it's typically minimal extra burden (e.g., a single database transaction) and avoids the temporary jagged-view window.

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
**Rejected**: (a) Pattern-match transaction structure (input/output counts). (b) Uniform witness decoding on all transitions for all contract types. (c) Output-only detection for all transitions (no witness inspection). (d) Reserve-based s_index derivation for pool swap/admin transitions.
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

**Chosen**: The 5 prediction market builders that involve reissuance token outputs return `UnblindedPset` — an opaque newtype with `prepare(pubkey)` and `finalize()` methods. The 7 remaining builders return `PartiallySignedTransaction` directly.
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

**Chosen**: Creation builders take concrete param types (`&MarketCreationParams`, `&LmsrPoolParams`, `&MakerOrderParams`) instead of the `ContractParams` enum. `build_creation_pset` takes `MarketCreationParams` (only non-derivable fields) rather than full `BinaryMarketParams` because the 4 token/RT asset IDs depend on coin selection (see [MarketCreationParams](#marketcreationparams)).
**Rejected**: All creation builders take `&ContractParams`, with runtime validation of the variant.
**Why**: Passing the wrong variant (e.g., `ContractParams::LmsrPool` to `build_creation_pset`) would only be caught at runtime. Taking concrete types makes wrong-variant errors compile-time errors. The standalone `contract_cmr()` still takes `ContractParams` (the enum) since it is genuinely polymorphic.

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

**Chosen**: `ingest_market`, `ingest_pool`, `ingest_order` with type-specific snapshot enums.
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

### Flat MarketState (Dormant/Unresolved Hidden, No Settled Variant)

**Chosen**: The public `MarketState` has 4 variants (`Trading`, `ResolvedYes`, `ResolvedNo`, `Expired`). Dormant (0 pairs) and Unresolved (>0 pairs) are both `Trading`. Terminal state = `outstanding_pairs == 0` on any non-Trading variant. No `Settled` variant.
**Rejected**: (a) Exposing `CovenantPhase` with Dormant/Unresolved. (b) Separate `Settled { final_txid, outcome }` terminal variant with `MarketOutcome` type.
**Why**: The Dormant/Unresolved distinction is a covenant implementation detail. The `Settled` variant was removed because it created a routing ambiguity: resolution from non-dormant markets produced intermediate `ResolvedYes/ResolvedNo` states, while resolution from dormant markets had to route directly to `Settled` — a special case an implementor could miss. Without `Settled`, resolution/expiry always produce the corresponding variant regardless of outstanding pairs, and `outstanding_pairs` naturally reaches 0 through redemption (or starts at 0 for dormant terminals). See the "Flat MarketState (No Settled Variant)" entry below for the full rationale.

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

**Chosen**: All three creation builders always include a zero-value OP_RETURN output with a compact recovery hint. Markets: 37 bytes (compressed non-derivable params using well-known asset index, 1-2-5 denomination, absolute u24 expiry). Orders: 40 bytes (XOR-masked index, market txid, u24 price, u8 min_fill/remainder, side+direction in type tag). Pools: 41 bytes (market txid, 9-bit mantissa x exponent for max_loss/half_payout, u12 fee_bps, u16 initial_s_index, XOR-masked index). No opt-out. All fit within a single 80-byte OP_RETURN.
**Rejected**: (a) No on-chain hints (orders require brute-force scanning, tokens require Nostr for labeling/redemption). (b) Optional hint via builder flag (risk of users opting out). (c) Uncompressed params (wastes bytes). (d) Hints for orders and pools only, not markets (breaks the recovery chain for all user types including pure token holders).
**Why**: Chain-only recovery for ALL user types — market creators, order makers, pool operators, and pure token holders (via `issuance_transaction` → market creation tx → OP_RETURN). Maker orders are the only contract type directly "owned" by regular end users, making compression especially important. The OP_RETURN encoding uses standard denomination conventions (1-2-5 for markets, 26-value mantissa for pools), well-known collateral asset indices, and XOR-masked derivation indices for privacy. Convention compliance is enforced at three layers: derive functions (first line), builders (defense in depth), and market ingestion (protects all downstream users). See [chain-only-recovery.md](../protocol/chain-only-recovery.md) for the complete encoding specification and recovery flows.

### Taker Order Fills Via Trade Router

**Chosen**: Order filling (taker side) is handled exclusively through the trade system (`quote_trade` + `build_trade_pset`). No `build_fill_order_pset`.
**Rejected**: Direct `build_fill_order_pset` builder for explicit single-order fills.
**Why**: The trade router optimizes across all available pools and orders for best execution. A direct fill builder would allow suboptimal execution and create an inconsistency (pool swaps already go through the trade router — `build_lmsr_swap_pset` doesn't exist). The maker's lifecycle is directly exposed (`build_create_order_pset`, `build_cancel_order_pset`) because those are single-contract operations that don't benefit from routing. If explicit order targeting becomes a requested feature, a direct fill builder can be added as a non-breaking change (new engine method, no store or type changes).

### LMSR Adjust API Uses Deltas

**Chosen**: `build_lmsr_adjust_pset` takes `pair_delta: i64` (applied equally to YES and NO) and `collateral_delta: i64`, not `target_reserves: &PoolReserves`.
**Rejected**: Absolute target reserves with runtime validation of the paired-delta constraint.
**Why**: The LMSR covenant enforces that YES and NO reserve deltas are equal on the admin path. By taking a single `pair_delta` parameter, the API makes this constraint unrepresentable as an error — the caller cannot express asymmetric deltas. The only remaining validation is reserve floors (computed targets must meet minimums), which is a meaningful constraint rather than an input formatting error. Wallets can present absolute-target UIs by computing deltas from current reserves on their side.

### Pool/Order Ingestion Requires Parent Market

**Chosen**: `ingest_pool` and `ingest_order` validate that the referenced token asset IDs correspond to a known market. If the parent market isn't tracked, returns `CoreError::InvalidParams`. `ingest_market` has no parent requirement.
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

### Collateral Per Pair

**Chosen**: Covenant parameter `COLLATERAL_PER_PAIR` — the total collateral to issue one YES+NO pair.
**Rejected**: `COLLATERAL_PER_TOKEN` (original) — the collateral backing a single token, requiring `* 2` in every formula.
**Why**: The atomic unit of issuance is always a pair (1 YES + 1 NO). Every formula immediately multiplied by 2, and the naming caused a documentation bug (inconsistent formulas). `COLLATERAL_PER_PAIR` eliminates the factor of 2 everywhere: `pairs = collateral / collateral_per_pair`. See [collateral-per-pair-refactor.md](../contracts/prediction-market/collateral-per-pair-refactor.md).

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

### Convention Enforcement at Ingestion

**Chosen**: `ingest_market` rejects markets whose parameters don't conform to the recovery conventions (denomination, expiry, collateral asset).
**Rejected**: (a) Accept all markets, reject only at child-contract creation. (b) Accept all markets, warn but don't reject.
**Why**: With `issuance_transaction` recovery, market convention conformity matters for ALL users — even pure token holders trace back to the market creation tx. "I won't trade on your market unless it's mnemonic-recoverable" creates the right ecosystem incentive. If `deadcat-core` is the primary tool, virtually all markets will be conforming. Non-conforming markets created by custom tools are their problem — they can use `deadcat-core` and get conformity for free.

### Standard Denomination Conventions

**Chosen**: `collateral_per_pair` constrained to 16-value 1-2-5 table (4 bits). Pool `max_loss_sats` and `half_payout_sats` constrained to 26-value mantissa x 10^exponent encoding (9 bits each). Well-known collateral asset index (4 bits: L-BTC=0, USDt=1, escape=15).
**Rejected**: (a) Uncompressed u64 values in OP_RETURN (wastes bytes). (b) Single-digit mantissa (too coarse — 100K to 200K is a 100% jump). (c) Full two-digit mantissa (90 values x 16 exponents = too many combinations, limited practical benefit over the 26-value set).
**Why**: The conventions compress OP_RETURN hints (market: 77→37 bytes, pool: 51→41 bytes) while constraining parameters to "round numbers" that market creators naturally pick. The 26-value mantissa set (10-20 step 1, 25-95 step 5) balances precision and simplicity. The 4-bit exponent supports non-L-BTC assets (USDT needs exponent 8+). See [chain-only-recovery.md](../protocol/chain-only-recovery.md).

### XOR Index Masking for Privacy

**Chosen**: Derivation indices (`order_index`, `pool_index`) are XOR-masked in the OP_RETURN using `HMAC(deadcat_secret_key, tag || context)[0..2]` where `deadcat_secret_key` is a single key derived from `m/purpose'/deadcat'/secret'`. The mask context includes all other OP_RETURN fields, serialized as raw values in standard big-endian encoding (not the compact OP_RETURN bit-packing). Different HMAC tags (`"deadcat/order_mask"`, `"deadcat/pool_mask"`) provide domain separation.
**Rejected**: (a) Unmasked indices (reveals derivation order and contract count to observers). (b) No index in OP_RETURN (forces gap-limit scanning during recovery). (c) Mask context using OP_RETURN-encoded bytes (couples the mask to the OP_RETURN encoding format — a future V2 format change would break mask computation for V1 hints).
**Why**: The mask is deterministic from the mnemonic + public OP_RETURN data, so recovery is still O(1). Observers see random-looking u16 values. The privacy cost of unmasked indices is small (only meaningful if an observer can link two transactions to the same wallet), but the masking cost is zero (one HMAC computation). Using raw values for the context (not OP_RETURN encodings) makes the mask encoding-agnostic — the context length doesn't affect on-chain size (only the 2-byte mask output appears in the OP_RETURN). Known property: identical-param orders on the same market share a mask — a negligible concern in an already-pathological scenario. See [chain-only-recovery.md](../protocol/chain-only-recovery.md) for the exact byte-level context serialization.

### Deterministic Derivation via `derive_order_params` and `derive_pool_params`

**Chosen**: Both `derive_order_params` and `derive_pool_params` take `deadcat_xprv` (the xprv at `m/purpose'/deadcat'`) + an index and derive all keys, nonces, and masks internally. Both return `Result<(Params, u16 /* masked_index */), ConventionError>` — validating OP_RETURN convention constraints before deriving. A single `deadcat_secret_key` (derived from the xprv) is used for all HMAC operations (nonce derivation, index masking) with different HMAC tags providing domain separation. `derive_pool_params` additionally takes `starting_price_bps` — needed to compute `initial_s_index` for the XOR mask context (the mask includes `initial_s_index`, which is derived from `starting_price_bps` via the inverse logistic function). This parameter does not affect `LmsrPoolParams` (the primary output), only the masked index (the secondary output).
**Rejected**: (a) Caller passes a pre-derived nonce or separate secret key + pubkey (foot-gun — non-deterministic nonces break recovery; separate keys require callers to manage multiple HD paths). (b) Separate `order_secret_key` and `pool_secret_key` at different HD paths (HMAC tags already provide full domain separation; a second secret key adds an HD path with no security benefit). (c) Omit `initial_s_index` from the pool mask context (weaker mask, and two pools with identical params but different starting prices would share the same mask).
**Why**: The nonce and mask MUST be deterministic from the mnemonic for chain-only recovery. Encapsulating all derivation in these functions eliminates foot-guns (wrong key, non-deterministic nonce, mismatched pubkey). The unified `deadcat_secret_key` simplifies the HD path table from 4 paths to 3. See [Key Derivation Convenience Functions](#key-derivation-convenience-functions) and [chain-only-recovery.md](../protocol/chain-only-recovery.md).

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

**Chosen**: `build_creation_pset` takes `&MarketCreationParams` (4 non-derivable fields) and returns `(UnblindedPset, BinaryMarketParams)`. The builder derives the 4 token/RT asset IDs from the selected defining inputs.
**Rejected**: (a) Builder takes full `&BinaryMarketParams` (caller can't fill in the 4 derivable asset ID fields because they depend on coin selection, which happens inside the builder). (b) Caller pre-selects defining inputs (breaks the "pass all UTXOs, builder selects" pattern).
**Why**: The 4 asset IDs are derived from issuance entropy = `hash(defining_outpoint || contract_hash)`. The defining outpoints are UTXOs selected by the builder during coin selection. Since coin selection happens inside the builder, the caller can't know the asset IDs beforehand. `MarketCreationParams` makes the API honest about what data flows in which direction — the caller provides what they know, the builder returns what it computed.

### LMSR Deterministic Table Specification Required

**Chosen**: The deterministic integer-only F-value generation algorithm requires a formal specification in a separate satellite document, to be written after extracting the Merkle tree format from the `.simf` verification code.
**Why**: The derivation chain `max_loss_sats → b → q_step_lots → F-values → Merkle root` involves transcendental constants (`1/ln(2)`, `ln(999)`) and transcendental functions (`exp`, `ln`). Different rational approximations or different fixed-point precisions produce different F-values, different Merkle roots, and incompatible pools. Cross-implementation recovery (including the Rust implementation itself) requires exact constants, a defined algorithm, and test vectors. The `.simf` covenant defines the Merkle tree format (hash function, leaf encoding) via its verification code — the spec must extract and document this alongside the generation algorithm.

### Pool OP_RETURN Includes initial_s_index

**Chosen**: The pool OP_RETURN hint includes `initial_s_index` as u16 (2 bytes, pool hint grows from 39 to 41 bytes).
**Rejected**: (a) Derive s_index from reserve values via reverse LMSR lookup (fragile, requires specifying the bootstrap allocation formula, vulnerable to adversarial reserve values). (b) Brute-force script matching over all 65K s_index candidates (requires EC scalar multiplication per candidate, ~3-7 seconds worst case).
**Why**: The pool's taproot tree structure means each s_index candidate requires a full taproot tweak (EC scalar multiplication) to verify — hashing alone is insufficient. Including `initial_s_index` directly in the hint eliminates all reverse-derivation complexity: compile for one s_index, verify script matches, done. The 2-byte cost is negligible relative to the hint's total size and is amortized over the pool's entire lifetime.

### Convention Validation in Derive Functions

**Chosen**: `derive_order_params` and `derive_pool_params` return `Result<_, ConventionError>` and validate that all inputs conform to OP_RETURN encoding conventions before deriving parameters. Builders also validate (defense in depth).
**Rejected**: (a) Derive functions are infallible, validation only at builder time (error surfaces far from the logical mistake — caller gets valid-looking params back, wires up UI, hits wall at build time). (b) Derive functions validate but panic (convention violations are input errors, not bugs).
**Why**: The derive functions are the natural first line of defense — the caller is making the parameter decision at this point. Catching `fee_bps = 5000` at derivation time ("this value exceeds the u12 OP_RETURN encoding limit") is clearer than catching it at build time ("PSET construction failed"). Three enforcement layers total: derive functions → builders → market ingestion (see [Wallet Recovery](#wallet-recovery)).

### `max_loss_sats` in `LmsrPoolParams`

**Chosen**: `LmsrPoolParams` includes `max_loss_sats: u64` alongside the covenant parameters, even though it is not itself a covenant parameter.
**Rejected**: (a) Store `max_loss_sats` in a separate `PoolConfig` wrapper (cleaner separation but ripples through the entire API — store trait, `Contract` enum, ingestion methods, discovery types). (b) Pass `max_loss_sats` as a separate parameter on `ingest_pool` (ad-hoc, no natural place to store it). (c) Recover `b` from `q_step_lots + half_payout_sats` (impossible — the `ceil()` in the derivation is lossy).
**Why**: All off-chain LMSR computation — point evaluation for quoting, full table generation for Merkle proofs, spot price calculation — requires the liquidity parameter `b = max_loss_sats / ln(2)`. Without `max_loss_sats`, the engine literally cannot evaluate the LMSR cost function after ingestion. The struct already contains two derived fields (`q_step_lots`, `lmsr_table_root`) as compilation caches, so adding a third non-covenant field is consistent. Including `max_loss_sats` also enables automatic curve well-formedness verification at `Creation` ingestion: the engine derives `b`, recomputes the table, and verifies the Merkle root matches — catching misconfigured or adversarially-constructed pools.

### `estimate_bootstrap` Does Not Take `fee_bps`

**Chosen**: `estimate_bootstrap` takes `max_loss_sats`, `half_payout_sats`, and `starting_price_bps` — no `fee_bps`.
**Rejected**: Including `fee_bps` for API symmetry with `derive_pool_params`.
**Why**: The bootstrap reserves depend on the LMSR cost function shape (`b`, `q_step_lots`, `half_payout_sats`) and starting position (`starting_price_bps`). The fee has no effect on the cost function, initial reserves, or s_index mapping — it's a per-swap spread applied by the covenant, not a curve parameter. Accepting an unused parameter misleads callers into thinking fees affect capital requirements.
