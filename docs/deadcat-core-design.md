# deadcat-core Design Document

## Purpose

`deadcat-core` is a pure computation library for interacting with Deadcat prediction market covenants on Liquid/Elements. It enables any wallet or application to create, track, interpret, and transact with prediction markets, LMSR pools, and limit orders — without prescribing how chain data is fetched, how state is persisted, or how keys are managed.

The primary motivating use case: integrating Deadcat functionality into existing wallets like Aqua, which already have their own wallet backend, chain connection, signer, and state management. These wallets need the covenant logic without an opinionated runtime.

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

```rust
impl<S: ContractStore> ContractEngine<S> {
    // Construction
    pub fn new(store: S, network: Network) -> Self;

    // Contract ingestion (per-type — see Contract Ingestion section)
    pub fn ingest_market(
        &mut self,
        params: &PredictionMarketParams,
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

    // Contract removal
    pub fn untrack_contract(&mut self, contract_id: &ContractId) -> Result<(), CoreError<S::Error>>;

    // Contract queries (reads — &self)
    pub fn contract(&self, contract_id: &ContractId) -> Result<Option<Contract>, CoreError<S::Error>>;
    pub fn watched_outpoints(&self, page: Pagination) -> Result<Page<OutPoint>, CoreError<S::Error>>;
    pub fn all_covenant_scripts(&self, contract_id: &ContractId) -> Result<Vec<Script>, CoreError<S::Error>>;

    // Per-type listing (reads — &self)
    pub fn list_markets(&self, filter: StateFilter, page: Pagination) -> Result<Page<MarketEntry>, CoreError<S::Error>>;
    pub fn list_pools(&self, filter: StateFilter, page: Pagination) -> Result<Page<PoolEntry>, CoreError<S::Error>>;
    pub fn list_orders(&self, filter: StateFilter, page: Pagination) -> Result<Page<OrderEntry>, CoreError<S::Error>>;

    // Relationship queries (reads — &self)
    pub fn pools_for_market(&self, market_id: &ContractId, page: Pagination) -> Result<Page<PoolEntry>, CoreError<S::Error>>;
    pub fn orders_for_market(&self, market_id: &ContractId, page: Pagination) -> Result<Page<OrderEntry>, CoreError<S::Error>>;

    // Transaction processing (writes — &mut self)
    pub fn process_transaction(&mut self, tx: &ChainTransaction) -> Result<Vec<ConfirmedTransition>, CoreError<S::Error>>;
    pub fn rollback_to_height(&mut self, height: u32) -> Result<(), CoreError<S::Error>>;
    pub fn prune_finalized(&mut self, current_height: u32, finality_depth: u32) -> Result<(), CoreError<S::Error>>;

    // Trade quoting (reads — &self)
    pub fn quote_trade(
        &self,
        market_id: &ContractId,
        spec: TradeSpec,
    ) -> Result<TradeQuote, CoreError<S::Error>>;

    // PSET builders (reads — &self)
    // All builders take a &WalletFunding for coin selection, fee computation, and change.
    // Creation builders take concrete param types (compile on the fly).
    // Post-ingestion builders take contract_id (recompile from stored params).

    // Prediction market builders
    pub fn build_creation_pset(&self, params: &PredictionMarketParams, funding: &WalletFunding) -> Result<PartiallySignedTransaction, CoreError<S::Error>>;
    pub fn build_issuance_pset(&self, contract_id: &ContractId, pairs: u64, yes_dest: &Script, no_dest: &Script, funding: &WalletFunding) -> Result<PartiallySignedTransaction, CoreError<S::Error>>;
    pub fn build_cancellation_pset(&self, contract_id: &ContractId, pairs_to_burn: Option<u64>, funding: &WalletFunding) -> Result<PartiallySignedTransaction, CoreError<S::Error>>;
    pub fn build_oracle_resolve_pset(&self, contract_id: &ContractId, oracle_attestation: &schnorr::Signature, funding: &WalletFunding) -> Result<PartiallySignedTransaction, CoreError<S::Error>>;
    pub fn build_expire_transition_pset(&self, contract_id: &ContractId, funding: &WalletFunding) -> Result<PartiallySignedTransaction, CoreError<S::Error>>;
    pub fn build_redemption_pset(&self, contract_id: &ContractId, side: Side, tokens_to_redeem: u64, funding: &WalletFunding) -> Result<PartiallySignedTransaction, CoreError<S::Error>>;
    // LMSR pool builders
    pub fn build_lmsr_bootstrap_pset(&self, params: &LmsrPoolParams, initial_reserves: &PoolReserves, funding: &WalletFunding) -> Result<PartiallySignedTransaction, CoreError<S::Error>>;
    pub fn build_lmsr_adjust_pset(&self, contract_id: &ContractId, pair_delta: i64, collateral_delta: i64, funding: &WalletFunding) -> Result<PartiallySignedTransaction, CoreError<S::Error>>;
    pub fn build_lmsr_close_pset(&self, contract_id: &ContractId, funding: &WalletFunding) -> Result<PartiallySignedTransaction, CoreError<S::Error>>;
    // Maker order builders (maker lifecycle only — taker fills go through build_trade_pset)
    pub fn build_create_order_pset(&self, params: &MakerOrderParams, offered_amount: u64, funding: &WalletFunding) -> Result<PartiallySignedTransaction, CoreError<S::Error>>;
    pub fn build_cancel_order_pset(&self, contract_id: &ContractId, funding: &WalletFunding) -> Result<PartiallySignedTransaction, CoreError<S::Error>>;
    // Trade builder (uses TradeQuote from quote_trade — handles all taker operations including order fills)
    pub fn build_trade_pset(&self, quote: &TradeQuote, funding: &WalletFunding) -> Result<PartiallySignedTransaction, CoreError<S::Error>>;

    // Transaction interpretation (reads — &self)
    pub fn interpret_transaction(&self, tx: &Transaction) -> Result<Vec<Transition>, CoreError<S::Error>>;
    pub fn identify_asset(&self, asset_id: &AssetId) -> Result<Option<AssetInfo>, CoreError<S::Error>>;
}

// Standalone pure functions (no engine needed — requires Simplicity compilation)
pub fn contract_cmr(params: &ContractParams, network: Network) -> Cmr;

// History methods — only available when the store implements ContractHistory
impl<S: ContractHistory> ContractEngine<S> {
    pub fn market_history(
        &self,
        contract_id: &ContractId,
        after: Option<ChainPosition>,
        limit: u32,
    ) -> Result<Vec<TypedStateUpdate<MarketTransition>>, CoreError<S::Error>>;

    pub fn pool_history(
        &self,
        contract_id: &ContractId,
        after: Option<ChainPosition>,
        limit: u32,
    ) -> Result<Vec<TypedStateUpdate<PoolTransition>>, CoreError<S::Error>>;

    pub fn order_history(
        &self,
        contract_id: &ContractId,
        after: Option<ChainPosition>,
        limit: u32,
    ) -> Result<Vec<TypedStateUpdate<OrderTransition>>, CoreError<S::Error>>;
}
```

Write methods take `&mut self`. Read methods (including all PSET builders) take `&self`. Rust's borrow rules enforce at compile time that only one writer OR multiple readers can access the engine at any given time — analogous to `RwLock` semantics without runtime overhead. This means store implementors only need to worry about atomic application of state updates, not concurrent access or out-of-order writes.

**PSET builders are engine methods**: All PSET builders live on the engine because they need Simplicity compilation for witness encoding (see [Simplicity Contracts](#simplicity-contracts-internal)). Simplicity contract compilation, CMR derivation, taproot tree construction, and script pubkey generation are all internal to the engine — consumers never interact with these concepts. Consumers provide contract params (plain data) and a `WalletFunding` struct, and receive PSETs back. See [PSET Construction](#pset-construction) for details.

**Creation builders** (`build_creation_pset`, `build_lmsr_bootstrap_pset`, `build_create_order_pset`) take the concrete param type (`&PredictionMarketParams`, `&LmsrPoolParams`, `&MakerOrderParams`) instead of a `ContractId` because the contract hasn't been ingested yet. The engine compiles the Simplicity contract on the fly at PSET build time. Post-ingestion builders also recompile from stored params on each call — see [Simplicity Contracts](#simplicity-contracts-internal) for the compilation cost model and rationale.

**No per-builder args structs**: PSET builders take operation-specific arguments as direct parameters alongside a shared `WalletFunding` struct (available UTXOs, fee rate, return script). This avoids a zoo of single-use parameter types — the function signature IS the documentation. See [WalletFunding](#walletfunding) and [PSET Construction](#pset-construction).

**Merged builders**: `build_issuance_pset` handles both initial and subsequent issuance — the engine determines which from the contract's current state (zero outstanding pairs vs non-zero). `build_redemption_pset` handles both post-resolution and post-expiry redemption — the engine determines which from the current state. The `side` parameter on `build_redemption_pset` specifies which token to burn: for resolved markets, the engine validates it matches the winning side; for expired markets, either side is valid.

**Maker order lifecycle vs taker trades**: The maker side of limit orders is directly exposed (`build_create_order_pset`, `build_cancel_order_pset`). The taker side — filling orders — is handled through the trade system (`quote_trade` + `build_trade_pset`), which routes across pools and orders for best execution. There is intentionally no `build_fill_order_pset` — direct order targeting adds API complexity without improving execution, since the router always finds the best available fill. If explicit order targeting becomes a requested feature, a direct fill builder can be added later as a non-breaking change (new method on the engine, no store or type changes required).

Note: The history `impl` block uses `S: ContractHistory` rather than `S: ContractStore + ContractHistory` because `ContractHistory` is a supertrait of `ContractStore` — the `ContractStore` bound is implied. See [ContractHistory](#optional-contracthistory).

### ContractId

Contract IDs uniquely identify a specific on-chain instance of a contract. They combine the program identity (CMR) with the instance identity (creation txid):

```rust
pub struct ContractId {
    pub cmr: Cmr,
    pub creation_txid: Txid,
}
```

`Cmr` is `simplicity_lang::Cmr` (re-exported as a public dependency of `deadcat-core`). It implements `AsRef<[u8]>` and `from_byte_array([u8; 32])`. `simplicity_lang` is a public dependency.

**Why both fields**: The CMR identifies the program (same params + same covenant source = same CMR), but not the instance. Two on-chain instances with identical params produce the same CMR. While collisions are self-defeating in practice (pools: cosigner=admin makes collision self-inflicted; orders: fresh nonces prevent it), the `creation_txid` component closes all theoretical collision vectors at minimal cost (32 extra bytes, already available from discovery). The struct preserves both fields: `cmr` for discovery dedup (O(1) "do I track anything with this CMR?"), `creation_txid` for instance uniqueness. See [Design Decisions Log](#design-decisions-log) for the full rationale.

The standalone function `contract_cmr(params, network)` returns only the CMR component — used for discovery dedup without requiring an engine. The full `ContractId` is only available after ingestion (when `creation_txid` is known from the creation transaction or snapshot).

### Contract Ingestion

Core uses per-type ingestion methods instead of a unified `ingest_contract`. Each contract type has genuinely different ingestion needs:

#### ingest_market

Markets are always ingested from their creation transaction:

```rust
pub fn ingest_market(
    &mut self,
    params: &PredictionMarketParams,
    creation_tx: &ChainTransaction,
) -> Result<ContractId, CoreError<S::Error>>;
```

The engine compiles the Simplicity contract from the parameters, derives deterministic blinding factors for creation verification (see [Deterministic RT Blinding](deterministic-rt-blinding.md)), verifies the creation transaction contains the expected covenant scripts, derives the initial outpoints, indexes asset IDs and scripts, and begins tracking. Returns the `ContractId` (CMR + creation txid).

**No anchor required**: Prediction market creation transactions include blinded reissuance token outputs. The blinding factors for these outputs are derived deterministically from public on-chain data (the defining outpoints), so no out-of-band anchor data is needed.

#### ingest_pool

Pools support both creation-tx and non-initial ingestion:

```rust
pub fn ingest_pool(
    &mut self,
    params: &LmsrPoolParams,
    snapshot: PoolSnapshot,
) -> Result<ContractId, CoreError<S::Error>>;

pub enum PoolSnapshot {
    Creation(ChainTransaction),
    Current {
        creation_txid: Txid,
        outpoints: BTreeMap<ReserveSlot, OutPoint>,
        s_index: u64,
        reserves: PoolReserves,
    },
}
```

With `PoolSnapshot::Creation`, the engine processes the creation transaction to derive initial state — same as market ingestion. With `PoolSnapshot::Current`, the engine starts tracking from the provided state without verifying history back to creation. The trade-off: `Current` = fast start (no history replay needed), but no prior transition history is recoverable. `Creation` = full history available via forward-sync from creation.

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
```

**Parent market required**: `ingest_pool` and `ingest_order` validate that the referenced token asset IDs correspond to a known market. If the parent market isn't tracked, the engine returns `CoreError::InvalidParams`. `ingest_market` has no parent requirement.

### untrack_contract

Removes a contract from the engine, deleting the contract, all derived data (asset index, scripts), and any history:

```rust
pub fn untrack_contract(&mut self, contract_id: &ContractId) -> Result<(), CoreError<S::Error>>;
```

Primary uses: cleanup of terminal/unwanted contracts, and the "untrack + re-ingest" promotion pattern — upgrading from non-initial to creation-based ingestion. A contract ingested with `PoolSnapshot::Current` can be untracked and re-ingested with `PoolSnapshot::Creation` to gain full history.

### process_transaction

`process_transaction` is the core write method. It accepts only **confirmed** (on-chain) transactions. It computes transitions, durably persists them, then returns the results:

```rust
pub fn process_transaction(
    &mut self,
    tx: &ChainTransaction,
) -> Result<Vec<ConfirmedTransition>, CoreError<S::Error>> {
    let transitions = self.compute_transitions(tx)?;
    if transitions.is_empty() {
        return Ok(vec![]);  // idempotent: nothing affected
    }
    let updates: Vec<StateUpdate> = transitions.iter().map(to_state_update).collect();
    self.store.apply_transitions(&updates)?;  // durable write
    Ok(transitions)
}
```

**Idempotent**: If the same transaction is processed twice (e.g., after crash recovery), the second call is a no-op — the spent outpoints are no longer tracked, so no contracts are affected.

**Durable before returning**: The store write completes before the method returns. A crash between "computed" and "persisted" cannot leave the engine in an inconsistent state.

**Why the engine applies transitions internally (not the caller)**: Processing and persistence are a single atomic operation. If the caller had to manually apply transitions, a crash between "engine returned results" and "caller applied them" would leave the engine's in-memory state ahead of the store. The engine owns the store, so it owns the write.

### interpret_transaction

`interpret_transaction` is the primary read method for wallet integration. It uses the same script matching and output value logic as `process_transaction` but does not modify state:

```rust
pub fn interpret_transaction(&self, tx: &Transaction) -> Result<Vec<Transition>, CoreError<S::Error>>;
```

**Works for confirmed and unconfirmed transactions**: Unlike `process_transaction` (confirmed only), `interpret_transaction` accepts any transaction — confirmed or unconfirmed. It works as long as the transaction spends outpoints the engine currently tracks in its durable state. This enables "pending transaction" UX: a wallet can interpret an unconfirmed mempool transaction to display "Pending issuance" or "Pending trade" before the transaction confirms.

**Known limitation — chained unconfirmed transactions**: If two unconfirmed transactions form a chain (tx2 spends an output created by tx1), only tx1 is interpretable. Tx2 spends outpoints that the engine hasn't durably recorded (tx1 was never processed), so the engine doesn't recognize them. Once both confirm and are fed to `process_transaction`, both are processed normally. This is rare in practice — Liquid has ~1-minute blocks, and chained unconfirmed covenant transactions require dependent operations within that window.

**No chain position metadata**: `interpret_transaction` takes a raw `elements::Transaction` without block height or tx index. Its return type (`Vec<Transition>` inside `Result`) omits chain position fields. This is in contrast to `process_transaction`, which returns `ConfirmedTransition` wrapping `Transition` with a `ChainPosition`. See [Transition and ConfirmedTransition](#transition-and-confirmedtransition).

**Point-in-time query**: The results reflect what the engine currently knows. If a transaction spends UTXOs from a contract the engine hasn't ingested yet, those contracts are simply absent from the results. After ingesting the contract and catching it up, calling `interpret_transaction` on the same transaction returns additional results.

**Partial knowledge grows over time**: A trade transaction that spends a known limit order and an unknown pool would initially return only the order fill. After the pool is ingested, the same call would also return the pool swap. The caller should be prepared to re-interpret transactions as new contracts are ingested.

### contract

Returns the current state of a single tracked contract by ID:

```rust
pub fn contract(&self, contract_id: &ContractId) -> Result<Option<Contract>, CoreError<S::Error>>;
```

Returns `None` if the contract hasn't been ingested. The caller matches on the `Contract` enum to access the typed state. Since callers typically know the contract type (they ingested it), this is a single-variant match.

### all_covenant_scripts

Returns covenant script pubkeys for a tracked contract. The behavior differs by contract type:

- **Markets**: Returns all possible scripts across all covenant phases and slot types (8 bounded scripts). This is the full set of scripts the contract could ever produce, derived deterministically from the contract's parameters.
- **Pools**: Returns current-state scripts only. The unbounded s_index makes full enumeration impractical at realistic table depths (2^table_depth x 3 slots). Pool output matching uses reserve-value-based s_index derivation instead of script matching.

**Primary use case**: Catch-up scanning for markets. After ingesting a market, the caller uses these scripts to scan the chain for historical transactions that touched the contract since creation.

**Not for steady-state monitoring**: To watch for new spends of a contract's current UTXOs, use `watched_outpoints()` instead — it returns the specific outpoints the engine is currently tracking, which is more precise than the full script set.

### Per-Type Listing Methods

The typed listing methods (`list_markets`, `list_pools`, `list_orders`) delegate to the store's per-type methods, which return typed results directly. All accept `StateFilter` and `Pagination` parameters.

```rust
pub fn list_markets(
    &self,
    filter: StateFilter,
    page: Pagination,
) -> Result<Page<MarketEntry>, CoreError<S::Error>>;
```

The store's listing methods return typed results (e.g., `Page<MarketEntry>` rather than `Page<(ContractId, Contract)>`). `MarketEntry` is a type alias for `ContractEntry<PredictionMarketParams, MarketState>` — see [ContractEntry](#contractentry). This enforces the type invariant at compile time — a `list_markets` implementation cannot accidentally return a pool or order. If the store's own data is corrupted (a "market" row deserializes to a different type), the error surfaces at the store layer via `Self::Error`, where data corruption errors belong.

**Pagination**: All listing methods use cursor-based pagination. See [Pagination Types](#pagination-types).

**State filtering**: `StateFilter::ActiveOnly` returns contracts with active outpoints. `StateFilter::TerminalOnly` returns contracts in terminal states (settled markets, closed pools, consumed/cancelled orders). `StateFilter::All` returns both. At Polymarket scale (thousands of markets), filtering at the store level avoids paging through thousands of irrelevant terminal contracts.

### Relationship Queries

```rust
pub fn pools_for_market(&self, market_id: &ContractId, page: Pagination) -> Result<Page<PoolEntry>, CoreError<S::Error>>;
pub fn orders_for_market(&self, market_id: &ContractId, page: Pagination) -> Result<Page<OrderEntry>, CoreError<S::Error>>;
```

Return pools or orders associated with a specific market. The relationship is encoded in pool/order params (they reference the market's token asset IDs). The store maintains a secondary index on market_id for efficient lookups, built during ingestion by resolving the pool/order's token asset IDs via the store's own `find_by_asset_id` index.

**Ingestion ordering constraint**: Pools and orders require their parent market to be ingested first. During `ingest_pool` or `ingest_order`, the engine validates that the referenced token asset IDs correspond to a known market. If the parent market isn't tracked, the engine returns `CoreError::InvalidParams`. This is a natural constraint — you shouldn't track a pool for a market you don't know about, and discovery naturally produces markets before their pools/orders.

Pools and orders are split into separate methods because they scale differently — a market typically has a handful of pools but potentially thousands of orders at Polymarket scale. Both are paginated.

### History Methods

The three typed history methods (`market_history`, `pool_history`, `order_history`) are only available when the store implements `ContractHistory`. They delegate to the store's unified `transition_history` method internally, then unwrap the `TransitionDetails` enum to return typed results:

```rust
// Engine calls store's unified method, then unwraps per-contract type
pub fn market_history(&self, contract_id: &ContractId, after: Option<ChainPosition>, limit: u32) -> Result<Vec<TypedStateUpdate<MarketTransition>>, ...> {
    let raw = self.store.transition_history(contract_id, after, limit)?;
    raw.into_iter().map(|u| {
        let TransitionDetails::Market(details) = u.details else {
            debug_assert!(false, "store returned non-market transition for market contract");
            // filter out mismatched entries
        };
        TypedStateUpdate { contract_id: u.contract_id, txid: u.txid, /* ... */ details }
    }).collect()
}
```

**Ordering**: History is returned in ascending chain order (oldest first). This aligns with the primary use cases: price chart construction, audit trails, and catch-up from a checkpoint. The `after` parameter provides a precise cursor using `ChainPosition` (block height + tx index), which handles multiple transitions within the same block correctly. The caller paginates by passing the `position` from the last returned item as `after` in the next call.

**Why typed convenience methods**: The caller always knows the contract type when querying history. The unified `StateUpdate` with `TransitionDetails` enum forces an unnecessary match on a variant the caller already knows. The typed methods eliminate this ergonomic cost. The store trait stays simple (one `transition_history` method); the engine does the trivial unwrapping.

**Invariant**: All transitions for a given `contract_id` always have the same `TransitionDetails` variant (a market contract only produces `Market` transitions). A mismatch indicates a bug in the store implementation — the engine asserts in debug and filters in release.

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

Used by `ChainTransaction`, `ConfirmedTransition`, `StateUpdate`, and `TypedStateUpdate`. Groups the two fields that are always known or unknown together — a confirmed transaction always has both; an unconfirmed transaction has neither.

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
    PredictionMarket(PredictionMarketParams),
    LmsrPool(LmsrPoolParams),
    MakerOrder(MakerOrderParams),
}
```

`ContractParams` is purely definitional — it contains only the data needed to derive the contract's identity (CMR) and addresses (covenant script pubkeys). No creation-time secrets or blinding factors. Given `ContractParams` + network + the Simplicity source code (built into `deadcat-core`), all covenant addresses for all states can be derived deterministically.

`PredictionMarketParams` defines the market's covenant parameters (oracle key, expiry, etc.). `LmsrPoolParams` defines the pool's parameters (token asset IDs referencing the parent market, liquidity parameters). `MakerOrderParams` defines the order's parameters (base/quote asset IDs, price, direction). These types map 1:1 to Simplicity covenant parameters. The `.simf` contract sources are authoritative for which parameters exist; see `src-tauri/crates/deadcat-sdk/src/{prediction_market,lmsr_pool,maker_order}/params.rs` for current Rust definitions.

### Contract

The three covenant types core tracks internally. This is an **internal type** managed by the engine and store — callers do not construct `Contract` values directly. Instead, they provide params + creation transaction (or snapshot) to the per-type ingestion methods, and the engine derives the initial contract state.

```rust
pub enum Contract {
    PredictionMarket {
        params: PredictionMarketParams,
        state: MarketState,
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

### Contract State Enums

Each contract type has a state enum that represents its current tip state — the latest snapshot, not a history log. This is stored durably via `ContractStore` (required) and updated each time `process_transaction` advances the contract. The tip state carries enough information for basic wallet UX without requiring `ContractHistory`.

#### MarketState

```rust
pub enum MarketState {
    Trading {
        outstanding_pairs: u64,
    },
    ResolvedYes {
        outstanding_pairs: u64,
    },
    ResolvedNo {
        outstanding_pairs: u64,
    },
    Expired {
        outstanding_pairs: u64,
    },
    Settled {
        final_txid: Txid,
        outcome: MarketOutcome,
    },
}

pub enum MarketOutcome {
    ResolvedYes,
    ResolvedNo,
    Expired,
}
```

`Trading` covers both the Dormant (zero outstanding pairs) and Unresolved (non-zero outstanding pairs) covenant phases. The distinction between these two phases is a covenant implementation detail — from the user's perspective, a market with 0 pairs is simply "a market where no one has issued yet" or "a fully cancelled market." `outstanding_pairs` is derived from collateral value: `collateral / (2 * collateral_per_token)`. Collateral amount is derivable in reverse: `outstanding_pairs * 2 * collateral_per_token`.

`MarketOutcome` carries the outcome so a wallet can answer "did this market resolve YES or NO?" without requiring transition history. This is essential for basic UX: showing "Your YES tokens are redeemable" vs "Your YES tokens are worthless."

Outpoints are not exposed in the public state — they are internal to the engine.

#### SlotType and CovenantPhase (Internal)

`SlotType` and `CovenantPhase` are internal types (`pub(crate)`) used for script matching and PSET routing. They are not exposed in the public API but are described here as an implementation spec.

```rust
// pub(crate) — internal to the engine
pub(crate) enum CovenantPhase {
    Dormant,
    Unresolved,
    ResolvedYes,
    ResolvedNo,
    Expired,
}

// pub(crate) — internal to the engine
pub(crate) enum SlotType {
    DormantYesRt,          // slot 0
    DormantNoRt,           // slot 1
    UnresolvedYesRt,       // slot 2
    UnresolvedNoRt,        // slot 3
    UnresolvedCollateral,  // slot 4
    ResolvedYesCollateral, // slot 5
    ResolvedNoCollateral,  // slot 6
    ExpiredCollateral,     // slot 7
}
```

Each `CovenantPhase` maps to a specific subset of slots. This mapping is the bridge between the state machine and the UTXO-following model — when a market transitions between phases, the engine knows which slots to expect in the new outputs:

| CovenantPhase | Live Slots |
| ------------- | ---------- |
| Dormant       | DormantYesRt (0), DormantNoRt (1) |
| Unresolved    | UnresolvedYesRt (2), UnresolvedNoRt (3), UnresolvedCollateral (4) |
| ResolvedYes   | ResolvedYesCollateral (5) |
| ResolvedNo    | ResolvedNoCollateral (6) |
| Expired       | ExpiredCollateral (7) |

The engine maps between the public `MarketState` and internal `CovenantPhase` as follows: `Trading` with `outstanding_pairs == 0` corresponds to `Dormant`; `Trading` with `outstanding_pairs > 0` corresponds to `Unresolved`; `ResolvedYes`/`ResolvedNo`/`Expired` map directly.

#### LmsrPoolState

```rust
pub enum ReserveSlot { Yes, No, Collateral }

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

`Closed` represents a pool whose admin has reclaimed all reserve UTXOs via the dedicated Simplicity close script path. See [lmsr-pool-close-path.md](lmsr-pool-close-path.md) for the covenant design. `StateFilter::TerminalOnly` returns closed pools.

#### OrderState

```rust
pub enum OrderState {
    Active {
        total_filled: u64,
    },
    Consumed {
        final_txid: Txid,
    },
    Cancelled {
        cancel_txid: Txid,
        total_filled: u64,
    },
}
```

`total_filled` on `Active` enables "5,000 of 10,000 sats filled" display while the order is still live. `Consumed` means fully filled — `total_filled` equals the original offered amount by definition, so it's not stored. `Cancelled` with `total_filled` enables "partially filled then cancelled" display (if `total_filled == 0`, it was a clean cancellation). Outpoints are internal to the engine and not exposed in the public state.

### Pagination Types

All listing and bulk-read methods use cursor-based pagination. Cursors are opaque — only the store generates and interprets them. This avoids the offset-based pagination instability problem (items shifting between pages due to concurrent ingestion or state changes) and allows each store implementation to choose its own ordering strategy.

```rust
pub struct Cursor(pub String);

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

**Cursor opacity and validation**: The store encodes whatever it needs into the cursor string (e.g., last seen contract_id, ordering position, method identity, filter). The caller passes cursors back without interpreting them. The store validates cursors on use: if a caller passes a cursor from one method to a different method, or reuses a cursor with different parameters (e.g., a cursor from `list_markets(ActiveOnly, ...)` used with `list_markets(TerminalOnly, ...)`), the store should return an error. Cursors encode a position within a specific filtered result set — changing the filter invalidates the position.

**Count-only queries**: Pass `limit: 0` to get just the `total` count without loading any items. No separate count methods needed. The response is `Page { items: vec![], next_cursor: None, total }` — `next_cursor` is `None` because no items were returned and there is no position to continue from. To fetch the first page after a count-only query, call again with `after: None` and a non-zero limit.

**`total` is always included**: Every paginated response includes the total count of matching items across all pages. This enables "showing 1-20 of 1,234 markets" UI without separate count calls. Implementors can cache counts if the overhead of `COUNT(*)` becomes significant.

### Side

```rust
pub enum Side { Yes, No }
```

Used throughout to identify which outcome token is referenced — in `OutputRole`, `TradeSpec`, `AssetInfo`, etc.

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

The wallet's contribution to a transaction. Shared across all PSET builders — the caller constructs one and passes it to any builder. `available_utxos` is the candidate UTXO pool (passing all wallet UTXOs is the expected usage — the builder selects the minimum needed). `fee_rate` is obtained from the chain backend. `return_script` receives all non-covenant, non-fee outputs: L-BTC change, collateral refunds, redemption payouts, trade proceeds, etc. Named `return_script` rather than `change_script` because it handles more than just change — it's the destination for all funds returning to the wallet. The builder consolidates outputs to the same script and asset for efficiency — see [Output Consolidation](#output-consolidation).

### ContractEntry

```rust
pub struct ContractEntry<P, S> {
    pub contract_id: ContractId,
    pub params: P,
    pub state: S,
}

pub type MarketEntry = ContractEntry<PredictionMarketParams, MarketState>;
pub type PoolEntry = ContractEntry<LmsrPoolParams, LmsrPoolState>;
pub type OrderEntry = ContractEntry<MakerOrderParams, OrderState>;
```

Generic entry type used by all listing and relationship query methods. The type aliases provide ergonomic names (`Page<MarketEntry>` vs `Page<ContractEntry<PredictionMarketParams, MarketState>>`).

### DerivedContractData

```rust
pub struct DerivedContractData {
    pub asset_ids: Vec<(AssetId, AssetInfo)>,
    pub covenant_scripts: Vec<Script>,
}
```

Pre-computed data passed to the store during contract tracking so the store can build indexes without knowing about Simplicity. `asset_ids` maps each asset (YES/NO tokens, YES/NO reissuance tokens) to its `AssetInfo`. `covenant_scripts` is the set of scripts for covenant states. Only prediction markets produce asset IDs; pools and orders have empty `asset_ids`.

### Oracle Attestation

The oracle resolve builder takes a `secp256k1::schnorr::Signature` — a BIP-340 Schnorr signature from the oracle. The oracle signs `SHA256(market_id || outcome_byte)` where `outcome_byte` is `0x01` for YES or `0x00` for NO. The engine extracts the outcome by trial verification against both possible messages using the oracle's public key (from market params). `secp256k1` is a public dependency of `deadcat-core`.

If the signature doesn't verify against either outcome message, the engine returns `CoreError::InvalidParams { detail: "oracle attestation does not verify against either outcome" }`.

### RedemptionKind

```rust
pub enum RedemptionKind {
    PostResolution,  // Resolved → Settled (winning tokens redeemed at full value)
    Expiry,          // Expired → Settled (any tokens redeemed at half value)
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
    pub side: Side,
    pub direction: TradeDirection,
    pub amount: TradeAmount,
}
```

`TradeSpec` is the input to `quote_trade`. The three axes are orthogonal — any combination of side, direction, and amount mode is valid.

### TradeQuote and Related Types

```rust
pub struct TradeQuote {
    // Public — for display to the user:
    pub side: Side,
    pub direction: TradeDirection,
    pub requested_amount: u64,
    pub filled_amount: u64,
    pub total_input: u64,
    pub total_output: u64,
    pub effective_price: f64,
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
        price: u64,   // quote asset smallest units per base lot (matches MakerOrderParams.price)
        lots: u64,    // number of base lots filled in this leg
    },
}
```

`TradeQuote` represents the best available fill. If `filled_amount < requested_amount`, the fill is partial — the wallet decides whether to proceed or warn the user. `quote_trade` returns `Err(CoreError::NoLiquidity)` only when zero liquidity is available (no pools, no orders); any positive fill returns `Ok`.

The `pub(crate)` field `route` makes `TradeQuote` non-constructable by external consumers — they can only receive one from the engine and pass it to `build_trade_pset`. See [Trade PSET Builder](#trade-pset-builder).

`TradeRoute` is a crate-internal type capturing the route plan (contract IDs, leg amounts, outpoint snapshots) needed by `build_trade_pset`. External consumers cannot inspect or construct it.

`RouteLeg` breaks down how the trade is routed across liquidity sources. `LiquiditySource::LmsrPool` includes s-index movement for "pool moved from 50 to 55" display. `LiquiditySource::LimitOrder` includes the matched price and lot count.

### ContractMatch

Returned by `ContractStore::find_by_outpoints`. Used internally by the engine to identify which tracked contracts are affected by a transaction and which specific outpoints matched. Callers of the engine never see this type — it exists at the engine-store boundary only.

```rust
pub struct ContractMatch {
    pub contract_id: ContractId,
    pub matched_outpoints: Vec<OutPoint>,
}
```

### Transition and ConfirmedTransition

The core transition data, split into two types based on whether chain position is known:

```rust
pub struct Transition {
    pub contract_id: ContractId,
    pub txid: Txid,
    pub old_outpoints: Vec<OutPoint>,
    pub new_outpoints: Vec<OutPoint>,
    pub details: TransitionDetails,
    pub external_outputs: Vec<ExternalOutput>, // non-covenant outputs with roles
}

pub struct ConfirmedTransition {
    pub transition: Transition,
    pub position: ChainPosition,
}
```

`process_transaction` returns `Vec<ConfirmedTransition>` — always has chain position (confirmed transactions only). `interpret_transaction` returns `Vec<Transition>` — no chain position (works for both confirmed and unconfirmed).

**Why two types instead of one with `Option<ChainPosition>`**: `process_transaction` always has chain position; `interpret_transaction` never does. Using `Option` would force `process_transaction` callers to unwrap a field that's always `Some`. The composition approach gives each method a return type that's fully precise — no optionals, no sentinel values. The core transition data (`Transition`) is defined once; `ConfirmedTransition` wraps it with position metadata.

### StateUpdate

The stripped-down form passed to the store internally by the engine. Does not include the computed output classification (`external_outputs`) since those are derived from the transaction at query time and do not need to be persisted:

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

**Why two types**: `ConfirmedTransition` is the caller-facing view (full data, including ephemeral computed fields). `StateUpdate` is the storage-facing view (only what needs to be persisted). The engine converts between them internally. This prevents store implementors from accidentally persisting wallet-specific data (output roles, classifications) alongside contract state, while ensuring callers always get the full picture.

### TypedStateUpdate

Generic wrapper used by the engine's typed history methods. Same fields as `StateUpdate` but with the `TransitionDetails` enum unwrapped to the concrete transition type:

```rust
pub struct TypedStateUpdate<D> {
    pub contract_id: ContractId,
    pub txid: Txid,
    pub position: ChainPosition,
    pub old_outpoints: Vec<OutPoint>,
    pub new_outpoints: Vec<OutPoint>,
    pub details: D,
}
```

Used as `TypedStateUpdate<MarketTransition>`, `TypedStateUpdate<PoolTransition>`, `TypedStateUpdate<OrderTransition>` by the history methods.

### TransitionDetails

Nested by contract type. Each variant carries the decoded details of what happened:

```rust
pub enum TransitionDetails {
    Market(MarketTransition),
    Pool(PoolTransition),
    Order(OrderTransition),
}

pub enum MarketTransition {
    Issued { pairs: u64, collateral_locked: u64 },
    Resolved { outcome_yes: bool },
    Redeemed { kind: RedemptionKind, side: Side, tokens_burned: u64, payout_sats: u64 },
    Cancelled { pairs_burned: u64, collateral_returned: u64 },
    Expired,
}

pub enum PoolTransition {
    Swapped { old_s_index: u64, new_s_index: u64, old_reserves: PoolReserves, new_reserves: PoolReserves },
    Adjusted { old_reserves: PoolReserves, new_reserves: PoolReserves },
    Closed { final_reserves: PoolReserves },
}

pub enum OrderTransition {
    Filled { fill_amount: u64 },  // delta for this fill, not cumulative
    Cancelled,
}
```

`MarketTransition::Issued` carries `pairs` and `collateral_locked` without an `IssuanceKind` discriminant. The engine still knows internally whether it was initial or subsequent issuance (for PSET routing), but this distinction is hidden from callers — it is a covenant implementation detail.

`PoolTransition::Swapped` corresponds to the LMSR covenant's swap path — someone traded through the pool, moving the s-index. `PoolTransition::Adjusted` corresponds to the admin path — the pool operator (with cosigner signature) adjusted liquidity without changing the s-index. The covenant enforces that YES and NO token deltas are equal on the admin path; collateral can change independently. `PoolTransition::Closed` indicates the pool admin reclaimed all reserve UTXOs via the close script path. See [lmsr-pool-close-path.md](lmsr-pool-close-path.md).

**Why nested by contract type**: When processing a market transition, the caller wants to match on market-specific variants without wading through pool and order cases. A flat enum mixing all contract types would force exhaustive matching across unrelated variants.

### ExternalOutput

Non-covenant outputs in a transaction. Split into two variants based on whether core can read the output's asset and value:

```rust
pub enum ExternalOutput {
    Explicit {
        index: u32,
        script_pubkey: Script,
        asset: AssetId,
        value: u64,
        role: OutputRole,
    },
    Confidential {
        index: u32,
        script_pubkey: Script,
    },
}
```

**Why an enum, not a struct with `Option` fields**: When core can identify an output (explicit), the asset, value, and role are always known together. When core can't (confidential/blinded), none of them are known. A flat struct with `asset: Option<AssetId>, value: Option<u64>` would allow impossible states like "asset known but value unknown." The enum makes the invariant unrepresentable.

`Explicit` / `Confidential` are the standard Elements terms for unblinded / blinded outputs. An explicit output that core can see but can't classify gets `role: OutputRole::Unknown` — "I know the asset and value, but I don't know this output's purpose in the transaction."

For `Confidential` outputs, the wallet uses its own blinding keys to determine asset and value. Core provides the output index and script pubkey so the wallet can correlate.

```rust
pub enum OutputRole {
    IssuedTokens { side: Side },
    CollateralReturn,
    TradeReceive,
    MakerReceive,
    OrderReturn,
    Burn,
    Fee,
    Unknown,
}
```

`OutputRole` is purely semantic — it labels what the output represents in the transaction, not its asset or value. The asset and value are already available on `ExternalOutput::Explicit`, so the role does not duplicate them. `side: Side` is kept on `IssuedTokens` because it adds genuinely new information that would otherwise require an asset lookup.

| Role | Meaning | Appears in |
| ---- | ------- | ---------- |
| `IssuedTokens { side }` | Newly minted YES or NO tokens | Issuance |
| `CollateralReturn` | Collateral released from covenant to user | Redemption, cancellation |
| `TradeReceive` | Tokens or L-BTC received by the taker | Trade, fill order |
| `MakerReceive` | Payment sent to the maker | Fill order, trade |
| `OrderReturn` | Order's locked asset returned to maker | Cancel order |
| `Burn` | Tokens or RTs destroyed (OP_RETURN) | Cancellation, redemption, resolve, expire |
| `Fee` | Transaction fee | All |
| `Unknown` | Core can see asset/value but can't classify | Any (wallet labels via key ownership) |

**Output consolidation**: PSET builders consolidate outputs that share the same script and asset into a single output for efficiency and privacy (see [Output Consolidation](#output-consolidation)). A `CollateralReturn` output may therefore include fee change. When exact amounts matter, use `TransitionDetails` — it is authoritative for semantic amounts (payout, tokens burned, collateral locked, etc.). `OutputRole` identifies *which* output serves a purpose; `TransitionDetails` provides *the precise numbers*.

### AssetInfo

Result of asset identification:

```rust
pub enum AssetInfo {
    YesToken { market_id: ContractId, params: PredictionMarketParams },
    NoToken { market_id: ContractId, params: PredictionMarketParams },
    YesReissuanceToken { market_id: ContractId },
    NoReissuanceToken { market_id: ContractId },
}
```

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
    InvalidCreationTx { reason: String },
    InvalidParams { detail: String },
    InvalidContractState { contract_id: ContractId, detail: String },
    ContractNotFound { contract_id: ContractId },
    ContractAlreadyTracked { contract_id: ContractId },
    InsufficientFunds { shortfalls: Vec<Shortfall> },
    NoLiquidity { market_id: ContractId },
    StaleQuote { detail: String },
    Unsupported { detail: String },
}
```

`InvalidParams` covers caller-provided inputs that violate covenant constraints (e.g., issuance amount exceeds limits, invalid collateral asset, pool/order referencing an unknown parent market). `InvalidContractState` is returned by PSET builders when the contract is in the wrong state for the requested operation (e.g., `build_issuance_pset` on a settled market, `build_redemption_pset` on a trading market). `InsufficientFunds` is returned by PSET builders when the caller's available UTXOs don't cover the required amounts — the `shortfalls` vec reports all insufficient assets at once (e.g., "need 50 more YES tokens AND 3,000 more sats"), enabling wallet UX that shows all missing resources rather than one at a time. `StaleQuote` is returned by `build_trade_pset` when the quote's snapshotted outpoints are no longer current (a `process_transaction` call consumed them between quoting and building) — the caller should re-quote. Internal construction errors (e.g., Pedersen commitment math failure) indicate bugs in core and panic rather than returning an error — every `CoreError` variant represents a condition the caller can meaningfully respond to.

**Why generic over the store error**: The engine is already generic over `S: ContractStore`, so `CoreError<S::Error>` adds no new generic parameters. Store error types are preserved — consumers can match on `CoreError::Store(e)` and handle their specific store error without downcasting. Store implementors define their own error type independently via an associated type on the trait.

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

When `process_transaction` is called:

1. Collect all input outpoints from the transaction
2. Check which tracked contracts own any of those outpoints (via `ContractMatch`)
3. For each affected contract, match the transaction's outputs against expected covenant scripts (from the store's persisted script index) to determine the new state
4. Derive transition details from the current state, new state, and output values
5. Compute external output roles for non-covenant outputs
6. Durably persist the state updates
7. Return the full `ConfirmedTransition`s

**Important**: A single transaction can affect multiple contracts. For example, a routed trade can spend LMSR pool reserves AND fill a limit order. Each contract advances independently based on its own outpoints — they don't need to know about each other.

### Output Matching via Script Pubkeys

Core identifies which outputs belong to which contract by matching `script_pubkey` values, not by assuming fixed output indices. This is robust across all transaction shapes:

- **Explicit covenant outputs** (collateral, reserves): script pubkey is the covenant address derived from contract params + state. Asset and value are readable.
- **Reissuance token outputs**: `Asset::Null` and `Value::Null` on-chain, but `script_pubkey` is set to the covenant address for the target slot. Core matches by script pubkey alone.
- **Confidential wallet outputs** (change, payouts): script pubkey is readable but asset/value are confidential. Core identifies these as "not covenant" and reports them as `ExternalOutput::Confidential` with the output index and script pubkey.

Core relies on the caller providing consensus-valid transactions. Since Liquid consensus has already verified reissuance token validity, confidential proofs, and Simplicity covenant witnesses, core does not need to re-verify — it only interprets.

### Transaction Ordering

Liquid transactions within a block have a strict serial order. Even if a contract's UTXO is spent and the resulting UTXO is re-spent within the same block, the two transactions have a defined order.

Core requires the caller to feed transactions in chain order. As long as this guarantee holds, core processes one transaction at a time sequentially — no concurrent writes, no locking needed. The "atomic write" for a single transaction is simply "write all state updates from this transaction before processing the next one."

### State Advancement via Script Matching and Output Values

Core determines transitions without decoding Simplicity witness data. Instead, it uses the current contract state, script pubkey matching (against the store's persisted script index), and explicit output values. This is possible because the covenant design encodes state into the script pubkey — different states produce different addresses — so the new state is identifiable from the transaction's outputs alone.

#### Prediction Markets

The internal `CovenantPhase` maps to a unique set of slot script pubkeys (see [SlotType and CovenantPhase](#slottype-and-covenantphase-internal)). The transition type is determined by which slot scripts the new outputs match:

- **Issuance** (Trading with 0 pairs to Trading with >0 pairs, or Trading to Trading with more pairs): Old outputs match Dormant or Unresolved slots; new outputs match Unresolved slots. `pairs` = new collateral value / `collateral_per_token`. `collateral_locked` = new collateral value - old collateral value (zero for initial issuance from Dormant). `IssuanceKind` is determined internally (`Initial` if old phase was Dormant, `Subsequent` if Unresolved) but not exposed in the public `MarketTransition::Issued`.
- **Resolution** (Trading → ResolvedYes/ResolvedNo): New output matches either `ResolvedYesCollateral` or `ResolvedNoCollateral` script. Which one determines `outcome_yes`.
- **Redemption** (ResolvedYes/ResolvedNo/Expired → Settled): No new covenant outputs. `payout_sats` is derived from the old collateral value. `side` from which token burn outputs are present. `RedemptionKind` is `PostResolution` if old state was ResolvedYes/ResolvedNo, `Expiry` if Expired.
- **Cancellation** (Trading → Trading with fewer pairs): New outputs match Unresolved or Dormant slots. `pairs_burned` = (old collateral - new collateral) / `collateral_per_token`. `collateral_returned` = old collateral - new collateral. If new outputs match Dormant slots (all collateral returned), it's a full cancellation back to zero outstanding pairs.
- **Expiry** (Trading → Expired): New output matches `ExpiredCollateral` script.
- **Dormant terminal paths**: Both RT outpoints spent from zero-pair state + oracle attestation results in `Settled(ResolvedYes/No)`, or timelock passed results in `Settled(Expired)`. See [market-dormant-terminal-paths.md](market-dormant-terminal-paths.md).

#### LMSR Pools

Different `s_index` values produce different covenant addresses (the s_index is a parameter in the script derivation). For pool output matching, the engine uses reserve-value-based s_index derivation: the new s_index is derived from explicit reserve output values via the LMSR table, rather than matching against pre-stored scripts. This is necessary because the unbounded s_index makes full script enumeration impractical.

- **Swap**: New outputs have different reserve values indicating a different s_index. `old_s_index` from stored state. `new_s_index` derived from the new reserve output values via the LMSR table. `old_reserves` from stored state. `new_reserves` from explicit output values (all three reserve outputs are explicit covenant outputs with readable asset and value).
- **Adjustment**: New outputs have the same s_index as old outputs (s_index frozen on admin path). `old_reserves` and `new_reserves` from stored state and output values.
- **Closure**: All pool outpoints are spent and no new covenant outputs are produced. The pool transitions to `Closed`. `final_reserves` from the stored state at time of closure.

The swap-vs-adjustment distinction is unambiguous: same s_index = adjustment, different s_index = swap. Closure is detected when all pool outpoints are spent with no new covenant outputs.

#### Maker Orders

- **Fill** (Active → Active or Consumed): A new covenant output exists with the same script pubkey as the old order. `fill_amount` = old locked value - new locked value. If no new covenant output exists and the maker received payment, the order was fully consumed.
- **Cancellation** (Active → Cancelled): No new covenant output, and the maker's locked asset was returned to the maker's address (identifiable from `MakerOrderParams.maker_receive_spk_hash`).

#### Why This Works Without Witness Decoding

The key insight is that Simplicity covenants encode state into the script pubkey. Each unique state (phase, s_index, slot type) produces a unique script. This is a deliberate design property — it allows trustless state identification from the chain alone without requiring covenant-specific witness parsing. The witness is needed for *authorization* (proving the spend is valid) and for *constructing* new spends (PSET builders), but not for *observing* what happened after the fact. This separation is what enables the no-cache architecture: `process_transaction` and `interpret_transaction` work entirely from persisted data (scripts, state, output values), while only PSET builders need the full compiled contract for witness encoding.

## Contract Ingestion

Core doesn't know or care about the discovery layer (Nostr, manual import, QR code, etc.). To start tracking a contract, the caller provides contract parameters and either a creation transaction or a current-state snapshot, depending on the contract type.

### Per-Type Ingestion

The three ingestion methods handle the different needs of each contract type:

**Markets** (`ingest_market`): Always ingested from the creation transaction. Markets have few transitions (bounded by the number of covenant phases) and fast catch-up, so there is no benefit to non-initial ingestion.

**Pools** (`ingest_pool`): Support both creation-tx and non-initial ingestion via `PoolSnapshot`. Pools can accumulate thousands of state transitions (one per swap), making forward-sync from creation expensive. Non-initial ingestion via `PoolSnapshot::Current` allows a trader to start using a pool immediately from its current state without replaying history.

**Orders** (`ingest_order`): Support both creation-tx and non-initial ingestion via `OrderSnapshot`. Takers need only the current state for filling — order history is irrelevant. Makers who need fill history (monitoring, recovery) use `OrderSnapshot::Creation`.

### What each snapshot variant provides

| Snapshot | History | Verification | Use case |
| -------- | ------- | ------------ | -------- |
| `Creation(ChainTransaction)` | Full — forward-sync from creation recovers all transitions | Creation tx verified against params | Makers, pool operators, anyone needing price history |
| `Current { ... }` | None — no prior transitions recoverable | No verification back to creation | Takers, traders who only need current state |

The trade-off is explicit in the type system: `Current` = fast start, no history; `Creation` = full history + verified.

### Catching Up New Contracts

When a contract is ingested that was created in the past, it needs to be "caught up" to the chain tip. Core doesn't manage this — it's the caller's responsibility. The approach differs by contract type:

**Markets**: Call `ingest_market`, then `all_covenant_scripts` to get all 8 scripts, scan the chain for matching transactions since creation height, and feed them to `process_transaction` in order.

**Pools and orders ingested from creation**: Forward-sync using outpoint-based "what spent this outpoint?" queries (forward-chaining), or use discovery-batched acceleration (see [Sync Patterns and Discovery](#sync-patterns-and-discovery)).

**Pools and orders ingested from current state**: Already at current state — just begin processing new transactions as they arrive.

Core doesn't distinguish between "catch-up" and "tip" transactions. It processes whatever it receives. Existing fully-synced contracts can continue processing new tip transactions while a newly-ingested contract catches up — each contract tracks its own outpoints independently.

Whether a contract is "caught up" is determined by the caller, not core. The caller knows the chain tip and knows whether it has scanned all relevant scripts up to that point. Core has no concept of a global sync tip.

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
    fn watched_outpoints(&self, page: Pagination) -> Result<Page<OutPoint>, Self::Error>;

    // Index lookups — &self (populated at ingestion via DerivedContractData)
    fn find_by_asset_id(&self, asset_id: &AssetId) -> Result<Option<AssetInfo>, Self::Error>;
    fn covenant_scripts(&self, contract_id: &ContractId) -> Result<Vec<Script>, Self::Error>;

    // Per-type listing — &self (typed results, not Contract enum)
    fn list_markets(&self, filter: StateFilter, page: Pagination) -> Result<Page<MarketEntry>, Self::Error>;
    fn list_pools(&self, filter: StateFilter, page: Pagination) -> Result<Page<PoolEntry>, Self::Error>;
    fn list_orders(&self, filter: StateFilter, page: Pagination) -> Result<Page<OrderEntry>, Self::Error>;

    // Relationship queries — &self (typed results)
    fn pools_for_market(&self, market_id: &ContractId, page: Pagination) -> Result<Page<PoolEntry>, Self::Error>;
    fn orders_for_market(&self, market_id: &ContractId, page: Pagination) -> Result<Page<OrderEntry>, Self::Error>;

    // Writes — &mut self
    fn track_contract(&mut self, contract_id: ContractId, contract: Contract, derived: DerivedContractData) -> Result<(), Self::Error>;
    fn untrack_contract(&mut self, contract_id: &ContractId) -> Result<(), Self::Error>;
    fn apply_transitions(&mut self, transitions: &[StateUpdate]) -> Result<(), Self::Error>;
    fn rollback_to_height(&mut self, height: u32) -> Result<(), Self::Error>;
    fn prune_finalized(&mut self, current_height: u32, finality_depth: u32) -> Result<(), Self::Error>;
}
```

Every consumer must implement this. Read methods take `&self`, write methods take `&mut self` — mirroring the engine's own borrow semantics. The engine calls read methods during interpretation (`&self` on the engine borrows the store as `&self`) and write methods during processing (`&mut self` on the engine borrows the store as `&mut self`).

`apply_transitions` must be durable when it returns — the engine depends on this for crash safety.

`find_by_outpoints` is the hot-path method called on every `process_transaction`. It is not paginated because its input is bounded by the transaction's input count (constrained by Liquid's transaction size limits).

`find_by_asset_id` and `covenant_scripts` are index lookups populated at ingestion time. The engine passes `DerivedContractData` (asset IDs + scripts) to `track_contract`, and the store indexes this data for fast lookups. `find_by_asset_id` backs the engine's `identify_asset` method. `covenant_scripts` backs the engine's `all_covenant_scripts` method. Neither requires Simplicity knowledge — the engine pre-computes the data and hands it over.

`untrack_contract` removes the contract, all derived data (asset ID index entries, covenant scripts), and any history. Store implementations must clean up all references.

**CMR-based lookups**: Store implementations should support lookups by CMR (for discovery dedup) in addition to full `ContractId`. This enables O(1) "do I track anything with this CMR?" checks during discovery, without requiring the full `ContractId`.

**Processing log**: The store must persist enough rollback metadata during `apply_transitions` for `rollback_to_height` to reverse transitions. At minimum: the contract ID, old outpoints, new outpoints, and block height for each processed transition. `prune_finalized` removes this metadata for transitions below the finality threshold. This processing log is separate from `ContractHistory`'s transition history — it exists for rollback, not for user-facing queries. `rollback_to_height` must also clean up derived data (asset ID index, covenant scripts) for contracts removed during rollback.

**Atomicity requirements**: Contract-level atomicity is a hard requirement — a single contract's state update (old outpoints -> new outpoints + state change) must be all-or-nothing. A half-updated contract is corrupted state. Transaction-level atomicity (all contracts updated together for a multi-contract transaction) is recommended but not strictly required for correctness. A "jagged" state where one contract has processed a transaction but another hasn't is indistinguishable from staggered ingestion — which is already a normal condition when contracts are discovered at different times. The system self-heals: re-processing the transaction advances the remaining contracts while already-processed contracts are a no-op (idempotency). Transaction-level atomicity is recommended because it's typically not much extra burden on top of the already-required contract-level atomicity (e.g., a single SQLite transaction) and avoids the jagged-view window.

### Optional: ContractHistory

```rust
pub trait ContractHistory: ContractStore {
    fn transition_history(
        &self,
        contract_id: &ContractId,
        after: Option<ChainPosition>,
        limit: u32,
    ) -> Result<Vec<StateUpdate>, Self::Error>;
}
```

`ContractHistory` is a supertrait of `ContractStore` — implementing it requires also implementing `ContractStore`. This means `Self::Error` is the same associated type from `ContractStore`, eliminating any error type mismatch. The engine's history methods can wrap the error in `CoreError::Store(e)` without ambiguity.

Only implement if the consumer wants price charts, audit trails, etc. Core never depends on history for processing — it only needs current state. History returns `StateUpdate` (the persisted form), not `ConfirmedTransition` (which includes ephemeral computed fields). To get full output classification for a historical transaction, the caller can call `interpret_transaction`.

The engine exposes history through typed convenience methods (`market_history`, `pool_history`, `order_history`) that are only available when the store implements `ContractHistory`. The store trait itself has a single unified `transition_history` method — the typed unwrapping happens in the engine. See [History Methods](#history-methods).

### Implementor Controls Retention

The engine always passes full `StateUpdate` details to `apply_transitions`. The implementor decides what to keep:

- **Minimal (e.g., Aqua)**: Update current outpoints, discard old state. Doesn't implement `ContractHistory`.
- **Full (e.g., Deadcat Live)**: Update current state AND append to history table. Implements `ContractHistory`. Supports price charts and audit trails.
- **Selective**: Keep LMSR pool history (for price charts) but discard order fill history (not needed).

This is an implementation detail — core doesn't need per-contract configuration flags.

### Tip State Principle

The current contract state (stored via `ContractStore`) carries enough information for basic wallet UX without requiring `ContractHistory`. A minimal consumer that only implements `ContractStore` can still answer:

- "Did this market resolve YES or NO?" -> `MarketState::Settled { outcome: MarketOutcome::ResolvedYes }` or `MarketState::ResolvedYes { .. }`
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

All PSET builders are engine methods. They return unsigned PSETs — no wallet access, no chain queries, no signing. The caller provides operation-specific arguments and a `WalletFunding` struct. The engine handles Simplicity contract compilation, script derivation, taproot tree construction, coin selection, and fee computation internally. The caller signs the resulting PSET and broadcasts.

**Simplicity is fully encapsulated**: Consumers never see compiled contracts (`CompiledPredictionMarket`, `CompiledLmsrPool`, `CompiledMakerOrder`), Commitment Merkle Roots (CMRs), taproot trees, or witness encoding. These are internal to the engine. Consumers provide contract params (plain data: oracle keys, asset IDs, prices, expiry times) and receive PSETs back. The word "Simplicity" need not appear in consumer code.

### Coin Selection and Fee Computation

PSET builders perform coin selection internally. The caller provides `available_utxos` via `WalletFunding` — their full candidate pool (or a pre-filtered subset if they want to exclude specific UTXOs). The builder selects the minimum needed. Passing all wallet UTXOs is the expected usage.

Fee computation is also internal. The caller provides `fee_rate: FeeRate` via `WalletFunding` (obtained from their chain backend). The builder constructs the transaction, measures its weight, and computes `fee = rate * weight`. This eliminates the chicken-and-egg problem of needing to know the transaction size to estimate the fee.

If the available UTXOs don't cover the required amount plus fee, the builder returns `CoreError::InsufficientFunds { shortfalls: Vec<Shortfall> }`.

### UnblindedUtxo

PSET builders accept `UnblindedUtxo` (via `WalletFunding`) — a UTXO with unblinded (revealed) asset, value, and blinding factors. This type is defined in `deadcat-core` and carries: outpoint, script pubkey, explicit asset ID, explicit value, asset blinding factor, and value blinding factor. `deadcat-sdk` imports this type from core (not the other way around — core has no dependency on SDK). The caller obtains these from their wallet's UTXO set.

### Output Consolidation

PSET builders consolidate outputs that share the same destination script and asset into a single output. For example, a redemption payout (L-BTC collateral returned) and fee change (excess L-BTC from the fee input) both go to `funding.return_script` — the builder merges them into one output. This reduces transaction size (~4.3 KB per confidential output on Liquid), lowers fees, reduces UTXO bloat in the wallet, and improves privacy (fewer outputs = less structural fingerprinting).

Because consolidated outputs may include fee change alongside their primary purpose, `OutputRole` identifies *which* output serves a purpose, but `TransitionDetails` is authoritative for *exact semantic amounts* (payout, tokens burned, collateral locked, etc.). The wallet should always use `TransitionDetails` for display amounts.

### No Per-Builder Args Structs

PSET builders take operation-specific arguments as direct function parameters alongside the shared `WalletFunding` struct. This avoids a proliferation of single-use parameter types — the function signature IS the documentation. Every builder needs wallet funding (every Liquid transaction requires an explicit fee). The `WalletFunding` struct carries the three common fields; operation-specific arguments are direct parameters. See [WalletFunding](#walletfunding).

### Prediction Market Builders

| Builder | Transaction | Covenant Transition |
| ------- | ----------- | ------------------- |
| `build_creation_pset` | Market creation (defines YES/NO assets, creates RT outputs) | — (creates initial state) |
| `build_issuance_pset` | Token issuance (initial or subsequent) | Trading (0 pairs) → Trading (>0 pairs), or Trading → Trading (more pairs) |
| `build_cancellation_pset` | Cancel market (burn tokens, return collateral) | Trading → Trading (fewer pairs) or → Trading (0 pairs) |
| `build_oracle_resolve_pset` | Oracle resolution | Trading → ResolvedYes/ResolvedNo |
| `build_expire_transition_pset` | Expire market | Trading → Expired |
| `build_redemption_pset` | Redeem tokens (post-resolution or post-expiry) | ResolvedYes/ResolvedNo/Expired → Settled |

Creation takes concrete param type `&PredictionMarketParams` (compiles on the fly). All others take `contract_id` (recompiles from stored params). `build_issuance_pset` handles both initial and subsequent issuance — the engine determines which from the contract's current state. `build_redemption_pset` handles both post-resolution and post-expiry redemption — the engine determines which from the current state. The `side` parameter specifies which token to burn; for resolved markets, the engine validates it matches the winning side.

`build_oracle_resolve_pset` and `build_expire_transition_pset` branch internally based on outstanding pairs — when called on a market with zero outstanding pairs (Dormant), they handle the dormant terminal paths (both RT UTXOs consumed, market reaches Settled). No new builder methods are needed for this case. See [market-dormant-terminal-paths.md](market-dormant-terminal-paths.md).

```rust
// Creation — takes concrete params, compiles on the fly
pub fn build_creation_pset(&self, params: &PredictionMarketParams, funding: &WalletFunding)
    -> Result<PartiallySignedTransaction, CoreError<S::Error>>;

// Post-ingestion — takes contract_id, recompiles from stored params
pub fn build_issuance_pset(&self, contract_id: &ContractId, pairs: u64, yes_dest: &Script, no_dest: &Script, funding: &WalletFunding)
    -> Result<PartiallySignedTransaction, CoreError<S::Error>>;

pub fn build_cancellation_pset(&self, contract_id: &ContractId, pairs_to_burn: Option<u64>, funding: &WalletFunding)
    -> Result<PartiallySignedTransaction, CoreError<S::Error>>;

// ... same pattern for all post-ingestion builders
```

`build_cancellation_pset` takes `pairs_to_burn: Option<u64>` — if `None`, the engine computes the maximum cancellable amount from the available YES and NO tokens in `funding.available_utxos` (minimum of the two token balances).

### LMSR Pool Builders

| Builder | Transaction | Covenant Path |
| ------- | ----------- | ------------- |
| `build_lmsr_bootstrap_pset` | Pool creation (fund initial reserves) | — (creates initial state) |
| `build_lmsr_adjust_pset` | Admin liquidity adjustment | Admin path (s_index unchanged) |
| `build_lmsr_close_pset` | Pool closure (reclaim all reserves) | Close script path |

```rust
pub fn build_lmsr_bootstrap_pset(&self, params: &LmsrPoolParams, initial_reserves: &PoolReserves, funding: &WalletFunding)
    -> Result<PartiallySignedTransaction, CoreError<S::Error>>;

pub fn build_lmsr_adjust_pset(&self, contract_id: &ContractId, pair_delta: i64, collateral_delta: i64, funding: &WalletFunding)
    -> Result<PartiallySignedTransaction, CoreError<S::Error>>;

pub fn build_lmsr_close_pset(&self, contract_id: &ContractId, funding: &WalletFunding)
    -> Result<PartiallySignedTransaction, CoreError<S::Error>>;
```

`build_lmsr_adjust_pset` takes `pair_delta` (applied equally to both YES and NO reserves) and `collateral_delta` (applied to collateral independently). This API shape makes the covenant's paired-delta constraint (YES and NO must move equally) unrepresentable as an error — the caller cannot express asymmetric deltas. The engine validates that the resulting reserves meet the covenant's minimum reserve floors (`MIN_R_YES`, `MIN_R_NO`, `MIN_R_COLLATERAL` from pool params) and returns `CoreError::InvalidParams` if violated. The wallet can present an absolute-target UI ("set pool to 1000 YES/NO") by computing the delta from current reserves on their side.

`build_lmsr_close_pset` atomically consumes all three reserve UTXOs via the dedicated Simplicity close script path (NUMS internal key makes key-spend unspendable). All reserve funds are returned to `funding.return_script`. See [lmsr-pool-close-path.md](lmsr-pool-close-path.md).

Pool swaps are not built directly — they are part of trade transactions (see [Trade PSET Builder](#trade-pset-builder) below).

### Maker Order Builders

The maker's lifecycle is directly exposed. The taker side (filling orders) is handled through trade transactions — see [Trade PSET Builder](#trade-pset-builder).

| Builder | Transaction | State Change |
| ------- | ----------- | ------------ |
| `build_create_order_pset` | Create limit order | — (creates initial state) |
| `build_cancel_order_pset` | Cancel order | Active → Cancelled |

```rust
pub fn build_create_order_pset(&self, params: &MakerOrderParams, offered_amount: u64, funding: &WalletFunding)
    -> Result<PartiallySignedTransaction, CoreError<S::Error>>;

pub fn build_cancel_order_pset(&self, contract_id: &ContractId, funding: &WalletFunding)
    -> Result<PartiallySignedTransaction, CoreError<S::Error>>;
```

### Trade PSET Builder

Trade transactions are unique: they route across multiple contracts (LMSR pools and/or maker orders) in a single transaction. This requires cross-contract route optimization — choosing which pools and orders to hit, in what amounts, for best execution. The engine has the state needed for this (pool reserves, order books, LMSR math), so trade PSET construction uses a two-step pattern:

**Step 1: Quote** (engine method, read-only):

```rust
pub fn quote_trade(
    &self,
    market_id: &ContractId,
    spec: TradeSpec,
) -> Result<TradeQuote, CoreError<S::Error>>;
```

The engine computes the optimal route across all available pools and orders for the market, using current state (reserves, s-indices, fill levels) and LMSR math. Returns a `TradeQuote` representing the best available fill. Returns `Err(CoreError::NoLiquidity)` only when zero liquidity is available; any positive fill returns `Ok` (see [TradeQuote](#tradequote-and-related-types) for partial fill handling).

**Step 2: Build** (engine method):

```rust
pub fn build_trade_pset(
    &self,
    quote: &TradeQuote,
    funding: &WalletFunding,
) -> Result<PartiallySignedTransaction, CoreError<S::Error>>;
```

Takes the accepted quote and the caller's wallet funding. The engine recompiles contracts from stored params, selects the needed UTXOs, computes the fee, and builds the PSET. The quote captures a snapshot of all contract state needed at quote time (outpoints, route parameters). If the underlying contracts change between quoting and building (a `process_transaction` call consumed the snapshotted outpoints), the engine returns `CoreError::StaleQuote` — the caller should re-quote. If the quote is still valid at build time but the transaction later fails on-chain (spent inputs due to a block arriving between build and broadcast), the caller re-quotes. This is standard trading UX — quotes are inherently ephemeral.

See [Trade Types](#trade-types) and [TradeQuote](#tradequote-and-related-types) for full type definitions.

**Why trades use a two-step pattern**: Trade is the only operation requiring cross-contract route optimization from engine state. The two-step pattern enables the standard trading UX of "show quote, user confirms, then build." All other PSET builders are single-step — the caller provides the operation params directly and gets a PSET back. They don't need a quoting step because they operate on a single contract whose state the caller already knows.

## Simplicity Contracts (Internal)

Core contains the `.simf` Simplicity contract source code and the compiler integration. Given contract parameters and a network type (testnet/mainnet), core internally:

- Compiles Simplicity contracts into committed programs with Commitment Merkle Roots (CMRs)
- Derives taproot script pubkeys for any contract state
- Generates control blocks for spending witnesses
- Decodes witness data from spending transactions

This is necessary for both PSET construction (building covenant outputs with correct scripts) and state advancement (matching output scripts to determine new state).

**Compilation model**: The Simplicity source templates are parsed once (process-wide `OnceLock` cache). Per-contract instantiation (binding parameters to the template) and commitment are performed on demand — there is no in-memory compiled contract cache. During ingestion, the engine compiles the contract, passes pre-computed scripts and asset IDs to the store as `DerivedContractData` for indexing, and discards the compiled result. PSET builders recompile from stored params on each call — the cost is moderate (~10-100ms, dominated by instantiation + commitment; template parsing is already cached). `process_transaction` and `interpret_transaction` do not need compiled contracts — they determine transitions from script pubkey matching (using the store's persisted script index) and output values, without witness decoding. `ContractEngine::new` is O(1) — it does not iterate existing contracts or compile anything at construction time.

**Why no compiled contract cache**: The only operation requiring a compiled contract is PSET construction (specifically, witness encoding for spending covenant inputs). The simplicityhl library's `CompiledProgram` type is opaque with no serialization API, so compiled contracts cannot be persisted to disk. An in-memory cache would only save recompilation across multiple PSET builds for the same contract within a single engine lifetime — a rare scenario that doesn't justify the cache's complexity (eviction during rollback, interior mutability for `&self` methods). If simplicityhl adds `CompiledProgram` serialization in the future, persisting compiled contracts at ingestion time would eliminate recompilation entirely — a transparent internal optimization with no API change. See [simplicityhl-compiled-program-serialization.md](simplicityhl-compiled-program-serialization.md) for the upstream request.

**None of this is exposed in the public API.** Consumers interact with contract params (plain data) and the engine's methods. Compiled contracts, CMRs, taproot tree structures, and witness encoding are implementation details.

## Reorg Handling

Core maintains a processing log: for each processed transaction, the contract ID, old outpoints, new outpoints, and block height. Rolling back to height N means:

1. Find all transitions from blocks strictly above N
2. Reverse them: restore old outpoints, remove new outpoints
3. Delete the processing records
4. Remove contracts whose creation transaction was in blocks strictly above N

Contracts created at height N are kept. Contracts created at height N+1 or above are removed — their creation transactions may no longer exist on the canonical chain after the reorg.

The caller detects the reorg (their chain backend tells them), calls `rollback_to_height`, then re-scans and feeds the new chain data.

**History cleanup**: For stores implementing `ContractHistory`, `rollback_to_height` must also remove any persisted transition history records above the rollback height. These records reference a chain that may no longer exist after the reorg. **Important**: Both state rollback and history cleanup must be atomic — a single database transaction, not two separate operations. A crash between rolling back state and rolling back history would leave the store in an inconsistent state (current state reflects the rollback but history still contains records from the pre-rollback chain). Since the store implements both `ContractStore` (with `rollback_to_height`) and `ContractHistory` (with its history table), it has everything needed to clean up both in a single operation. This is a `ContractStore::rollback_to_height` implementation concern, not a separate method on `ContractHistory`, because atomicity requires both cleanups to happen in one call.

**Known limitation**: `rollback_to_height` removes contracts ingested above the rollback height. The caller must re-discover and re-ingest these contracts if they reappear on the new canonical chain. Since discovery happens over Nostr, contracts can always be re-fetched. This is the correct behavior — a contract whose creation transaction was reorged out is not a valid contract on the current chain.

**Typical usage**: After rolling back, the caller calls `watched_outpoints()` to get the now-current outpoints for re-scanning:

```rust
engine.rollback_to_height(reorg_height)?;

// Paginate through all outpoints for re-scanning
let mut all_outpoints = Vec::new();
let mut cursor = None;
loop {
    let page = engine.watched_outpoints(Pagination { after: cursor, limit: 100 })?;
    all_outpoints.extend(page.items);
    cursor = page.next_cursor;
    if cursor.is_none() { break; }
}

let new_txs = chain.scan_from(reorg_height, &all_outpoints);
for tx in new_txs {
    engine.process_transaction(&tx)?;
}
```

### Finality-Based Pruning

On Liquid, transactions are considered absolutely irreversible after 2 confirmations. Processing log entries for finalized transactions can never be needed for reorg recovery and can be safely pruned:

```rust
fn prune_finalized(&mut self, current_height: u32, finality_depth: u32) -> Result<(), CoreError<S::Error>>;
```

The caller periodically calls `prune_finalized` with the current chain tip and the network's finality depth (2 for Liquid). This keeps the processing log bounded without sacrificing correctness.

**Important**: Pruning the processing log (for reorg rollback) is independent from retaining transition history (for price charts, audit trails). The `ContractHistory` trait stores historical transitions permanently — `prune_finalized` only removes the rollback metadata that's no longer needed.

## Thread Safety

Write methods (`process_transaction`, `ingest_market`, `ingest_pool`, `ingest_order`, `untrack_contract`, `rollback_to_height`, `prune_finalized`) take `&mut self`. Read methods (`interpret_transaction`, `identify_asset`, `watched_outpoints`, `all_covenant_scripts`, `contract`, `list_markets`, `list_pools`, `list_orders`, `pools_for_market`, `orders_for_market`, `quote_trade`, and all PSET builders) take `&self`.

Rust's borrow rules provide compile-time `RwLock` semantics: multiple concurrent readers OR one exclusive writer, enforced without runtime overhead. For single-threaded consumers this is invisible. For multi-threaded consumers who need concurrent access, wrap the engine in `RwLock<ContractEngine<S>>`:

```rust
let engine = Arc::new(RwLock::new(ContractEngine::new(store, Network::Liquid)));

// Writer thread
let engine_w = engine.clone();
std::thread::spawn(move || {
    for tx in new_transactions {
        engine_w.write().unwrap().process_transaction(&tx).unwrap();
    }
});

// Reader thread (can run concurrently with other readers)
let interpretations = engine.read().unwrap().interpret_transaction(&some_tx);
```

Core does not add `Send` or `Sync` bounds on the `ContractStore` trait. If a store implementation is `Send`, `ContractEngine<S>` is automatically `Send`. This lets single-threaded consumers use non-thread-safe stores without penalty, while multi-threaded consumers choose thread-safe implementations.

## LMSR Math

Core provides pure LMSR computation functions (these already exist and are clean):

- `fee_free_yes_spot_price_bps(manifest, params, s_index)` — implied probability
- `quote_from_table(trade_kind, old_s_index, new_s_index, ...)` — deterministic quote
- `quote_exact_input_from_manifest(manifest, params, trade_kind, s_index, input)` — best trade for a given input amount
- `generate_lmsr_table(liquidity, depth, q_step_lots, s_bias, half_payout)` — generate lookup table
- `lmsr_table_root(values)` — Merkle root from table values

These have zero dependencies beyond basic math — no wallet, chain, or state.

## Sync Patterns and Discovery

This section describes how the caller (discovery layer, wallet integration) synchronizes contract state using the engine's API. Core itself has no concept of sync — it processes whatever transactions it receives. The patterns described here are recommendations based on the API design.

### Per-Contract Sync Behavior

#### Markets

- Always ingested from creation transaction via `ingest_market`
- Forward-sync via batch script query: `all_covenant_scripts` returns all 8 bounded scripts (2 Dormant RT slots + 3 Unresolved slots + 1 ResolvedYes + 1 ResolvedNo + 1 Expired)
- No non-initial ingestion needed (few transitions, fast catch-up)
- No backward-sync needed

#### LMSR Pools

- Support creation-tx and non-initial ingestion via `PoolSnapshot`
- `all_covenant_scripts` returns current-state scripts only (unbounded s_index makes full enumeration impractical at realistic table depths — 2^table_depth x 3 slots)
- Output matching uses reserve-value-based s_index derivation instead of script matching (the new s_index is derived from explicit reserve output values via the LMSR table, not by matching against pre-stored scripts)
- Catch-up uses forward-chaining (outpoint-based "what spent this outpoint?" queries) rather than script scanning
- Discovery-batched acceleration: discovery payloads can include `Vec<Txid>` of transition history. The caller batch-fetches all TXIDs in parallel, sorts by chain position, feeds to `process_transaction` in order. The engine verifies chain of custody during processing — each tx must spend the previous outpoints.
- Backward-sync deferred to future — forward-sync from creation (with optional TXID batching) covers v1 price history needs

#### Limit Orders

- Support creation-tx and non-initial ingestion via `OrderSnapshot`
- Takers: non-initial ingestion, forward-sync from current state, no history needed
- Makers: creation-tx ingestion, forward-sync from creation for verified fill history
- Maker recovery: creation tx fetched via `creation_txid` from discovery payload
- No backward-sync needed

### Per-Persona Sync Tables

**Pool sync by persona:**

| Persona | Ingestion | Sync strategy | History needed? |
| ------- | --------- | ------------- | --------------- |
| Trader (taker) | `PoolSnapshot::Current` | Forward from current state | No — only current price matters |
| Pool operator (maker) | `PoolSnapshot::Creation` | Forward from creation (with TXID batching) | Yes — fee revenue, adjustment audit |
| Price chart viewer | `PoolSnapshot::Creation` | Forward from creation (with TXID batching) | Yes — full price history |

**Order sync by persona:**

| Persona | Ingestion | Sync strategy | History needed? |
| ------- | --------- | ------------- | --------------- |
| Taker | `OrderSnapshot::Current` | Forward from current state | No — only current fill level matters |
| Maker (monitoring) | `OrderSnapshot::Creation` | Forward from creation | Yes — fill-by-fill history |
| Maker (recovery) | `OrderSnapshot::Creation` | Fetch creation tx via `creation_txid`, forward-sync | Yes — full fill history |

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

Note: Market discovery payloads include `PredictionMarketParams` + `creation_txid`. Markets always use creation-tx ingestion, so no snapshot is needed.

### Untrack + Re-Ingest Promotion

A contract ingested with a `Current` snapshot (no history) can be promoted to full-history mode:

1. Call `untrack_contract` to remove the contract and all derived data
2. Re-ingest with `PoolSnapshot::Creation` or `OrderSnapshot::Creation`
3. Forward-sync from creation to rebuild full history

This is useful when a trader initially ingested a pool for quick trading (non-initial) and later wants price history (e.g., for charting).

## Example Integration: Aqua Wallet

```rust
use deadcat_core::{
    ContractEngine, PredictionMarketParams, ChainTransaction, FeeRate, WalletFunding,
    Network, Pagination, StateFilter, Side, TradeSpec, TradeDirection, TradeAmount,
    ContractId, ContractParams,
};

// 1. Initialize engine with a store implementation and network.
//    The store is exclusively owned by the engine from this point.
//    Construction is O(1) — no iteration, no compilation.
let mut engine = ContractEngine::new(aqua_deadcat_store, Network::Liquid);

// 2. Ingest a market (discovered via Nostr, import, etc.)
//    No anchor needed — core derives blinding factors deterministically.
//    Core compiles the contract, verifies the creation tx, and indexes asset IDs + scripts.
//    Returns ContractId (CMR + creation_txid).
let market_id = engine.ingest_market(&market_params, &creation_tx)?;

// 3. Catch up: scan chain for this contract's history since creation.
//    all_covenant_scripts returns all 8 bounded scripts for markets.
let scripts = engine.all_covenant_scripts(&market_id)?;
let historical_txs = aqua_chain.scan_history(&scripts, creation_tx.position.block_height);
for tx in historical_txs {
    engine.process_transaction(&tx)?;
}

// 4. Steady state: process new confirmed transactions as they arrive
for tx in aqua_chain.poll_new_transactions() {
    let transitions = engine.process_transaction(&tx)?;
    for ct in &transitions {
        log::info!("Contract {:?} transitioned at block {}", ct.transition.contract_id, ct.position.block_height);
    }
}

// 5. Pending UX: interpret unconfirmed mempool transactions (read-only)
if let Some(mempool_tx) = aqua_chain.get_mempool_tx(txid) {
    let interpretations = engine.interpret_transaction(&mempool_tx)?;
    for interp in &interpretations {
        render_pending_label(&interp.details);
    }
}

// 6. Label wallet history (read-only, can be called anytime)
for wallet_tx in wallet_history {
    let interpretations = engine.interpret_transaction(&wallet_tx)?;
    for interp in &interpretations {
        for output in &interp.external_outputs {
            match output {
                ExternalOutput::Explicit { index, role, .. } if *index == my_utxo_index => {
                    render_label(role);
                }
                _ => {}
            }
        }
    }
}

// 7. Identify assets in wallet balance (delegates to store's asset index)
for (asset_id, amount) in wallet_balance {
    if let Some(info) = engine.identify_asset(&asset_id)? {
        render_token_position(&info, amount);
    }
}

// 8. Browse markets with pagination
let page = engine.list_markets(
    StateFilter::ActiveOnly,
    Pagination { after: None, limit: 20 },
)?;
for entry in &page.items {
    render_market_card(&entry.contract_id, &entry.params, &entry.state);
}
// Next page:
if let Some(cursor) = page.next_cursor {
    let next_page = engine.list_markets(
        StateFilter::ActiveOnly,
        Pagination { after: Some(cursor), limit: 20 },
    )?;
}

// 9. Deduplicate during discovery (CMR-based, no compilation needed)
//    Discovery announces CMR — check if we already track anything with this CMR.
let announced_cmr = announcement.cmr; // Cmr included in Nostr announcement
if store_has_cmr(&announced_cmr) { continue; }
// Or, if you need to verify from params: let cmr = deadcat_core::contract_cmr(&params, Network::Liquid);

// 10. Trade: two-step quote + build (engine handles routing, coin selection, fee computation)
let spec = TradeSpec { side: Side::Yes, direction: TradeDirection::Buy, amount: TradeAmount::ExactInput(5000) };
let quote = engine.quote_trade(&market_id, spec)?;
// Display to user: "Spend 5,000 sats to buy ~100 YES tokens via Pool A (80) + Order B (20)"
if user_confirms(&quote) {
    let funding = WalletFunding {
        available_utxos: &aqua_wallet.list_utxos(),
        fee_rate: FeeRate::from_sat_per_vb(aqua_chain.estimate_fee_rate()),
        return_script: &aqua_wallet.next_return_script(),
    };
    let pset = engine.build_trade_pset(&quote, &funding)?;
    let signed = aqua_signer.sign(pset)?;
    aqua_chain.broadcast(signed)?;
}

// 11. Build a single-contract transaction (engine handles compilation, coin selection, fee)
//     No Simplicity knowledge needed — just provide the contract_id and operation args.
let funding = WalletFunding {
    available_utxos: &aqua_wallet.list_utxos(),
    fee_rate: FeeRate::from_sat_per_vb(aqua_chain.estimate_fee_rate()),
    return_script: &aqua_wallet.next_return_script(),
};
let pset = engine.build_issuance_pset(&market_id, 100, &token_dest, &token_dest, &funding)?;
let signed = aqua_signer.sign(pset)?;
aqua_chain.broadcast(signed)?;
```

## Design Decisions Log

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

### Separate Return Types for Processing and Interpretation

**Chosen**: `process_transaction` returns `Vec<ConfirmedTransition>` (with `ChainPosition`). `interpret_transaction` returns `Vec<Transition>` (without chain position). Core transition data is defined once in `Transition`; `ConfirmedTransition` wraps it with position metadata.
**Rejected**: Single `TransitionResult` type with `Option<ChainPosition>`.
**Why**: `process_transaction` always has chain position; `interpret_transaction` never does. Using `Option` would force `process_transaction` callers to unwrap a field that's guaranteed `Some`. The composition approach gives each method a return type that's fully precise — no optionals, no sentinel values, no impossible states. The core transition data is defined once, so there's no duplication.

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

**Chosen**: Determine transition type from script pubkey matching (against the store's persisted script index) and explicit output values. No Simplicity witness decoding. For LMSR pools, s_index derivation uses reserve output values via the LMSR table rather than script matching (unbounded s_index makes full script enumeration impractical).
**Rejected**: (a) Pattern-match transaction structure (input/output counts). (b) Decode Simplicity witness data.
**Why**: Script matching is deterministic and precise — each covenant state produces unique script pubkeys, so matching new outputs against expected scripts unambiguously identifies the new state. Output values provide the numeric transition details (reserves, amounts, fill levels). For pools, the new s_index is derived from the reserve output values via the LMSR table — this is equivalent to script matching but avoids pre-computing scripts for all possible s_index values. This approach avoids the need for compiled Simplicity contracts during transaction processing, which is critical for the no-cache architecture — only PSET builders (which need witness *encoding*) require compilation. Witness decoding would require compiled contracts on every `process_transaction` and `interpret_transaction` call, forcing either a cache or repeated recompilation on a hot path.

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

### Explicit/Confidential Output Split

**Chosen**: `ExternalOutput` is an enum with `Explicit` (asset, value, and role known) and `Confidential` (only index and script pubkey known) variants.
**Rejected**: Flat struct with `Option<AssetId>` and `Option<u64>` fields.
**Why**: When core can read an output, asset, value, and role are always known together. When it can't (confidential), none are known. The flat struct allows impossible states ("asset known but value unknown"). The enum makes the invariant unrepresentable. The variant names use standard Elements terminology.

### OutputRole as Pure Semantic Label

**Chosen**: `OutputRole` variants carry only semantic information (e.g., `Side` for token outputs). Asset and value are NOT duplicated from `ExternalOutput::Explicit`.
**Rejected**: `OutputRole` variants with `amount: u64` and `asset: AssetId` fields.
**Why**: In Elements, the output `value` field IS the raw unit count — token count for YES/NO tokens, satoshis for L-BTC. There is no denomination layer. Since `ExternalOutput::Explicit` already carries `asset` and `value`, duplicating them in `OutputRole` would be pure redundancy, with the risk of inconsistency between the two copies.

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
**Why**: After a reorg, a contract's creation transaction may no longer exist on the canonical chain. Keeping a phantom contract with invalid outpoints would corrupt the engine's state — `watched_outpoints` would return outpoints that don't exist, and `process_transaction` would silently fail to match. Removing the contract is correct: the caller re-discovers and re-ingests from Nostr if the creation tx reappears on the new chain. The "unverified" approach adds unnecessary state complexity for a problem that re-ingestion solves cleanly.

### Deterministic RT Blinding (Anchor Elimination)

**Chosen**: Prediction market creation transactions blind reissuance token outputs with deterministic blinding factors derived from public on-chain data. No out-of-band anchor data needed.
**Rejected**: Random blinding factors for RT outputs, requiring an anchor (blinding factors) to be shared via Nostr.
**Why**: The Elements protocol requires reissuance token outputs to be blinded (ABF != 0) for reissuance to work. Traditionally, random ABFs are used as an authorization mechanism — only someone who knows the ABF can reissue. With Simplicity covenants, authorization is enforced by the covenant itself, not by ABF secrecy. Using deterministic ABFs derived from public data (defining outpoints via tagged hash) satisfies the protocol requirement while eliminating the need for anchor distribution. This simplifies the ingestion API (no anchor parameter), the Nostr announcement format, and removes the "lost anchor" failure mode. See [Deterministic RT Blinding](deterministic-rt-blinding.md) for the derivation spec.

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

**Chosen**: Creation builders take concrete param types (`&PredictionMarketParams`, `&LmsrPoolParams`, `&MakerOrderParams`) instead of the `ContractParams` enum.
**Rejected**: All creation builders take `&ContractParams`, with runtime validation of the variant.
**Why**: Passing the wrong variant (e.g., `ContractParams::LmsrPool` to `build_creation_pset`) would only be caught at runtime. Taking concrete types makes wrong-variant errors compile-time errors. The standalone `contract_cmr()` still takes `ContractParams` (the enum) since it is genuinely polymorphic.

### Single Return Script for All Non-Covenant Outputs

**Chosen**: `WalletFunding.return_script` is the sole destination for all non-covenant, non-fee outputs: L-BTC change, collateral refunds, redemption payouts, trade proceeds, order refunds, etc. No separate `payout_script` or `refund_script` parameters.
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
**Why**: Asset ID lookups and covenant script lookups are hot-path operations (called by `identify_asset` and `all_covenant_scripts`). Persisting them in the store ensures O(1) construction of `ContractEngine::new` — no iteration or compilation at startup. `process_transaction` and `interpret_transaction` determine transitions from script pubkey matching and output values — no compiled contracts needed. Only PSET builders require compilation (for witness encoding), and their ~10-100ms recompilation cost is acceptable for a user-initiated operation. An in-memory cache would only save recompilation across multiple PSET builds for the same contract within a single engine lifetime — too rare to justify the complexity (eviction during rollback, interior mutability). If simplicityhl adds `CompiledProgram` serialization, persisting at ingestion time would eliminate recompilation entirely.

Note: `all_covenant_scripts` returns bounded scripts for markets (all 8 across all phases) but current-state-only scripts for pools (unbounded s_index makes full enumeration impractical). Pools use reserve-value-based s_index derivation for output matching instead of pre-stored scripts.

### CMR + Creation Txid as Contract ID

**Chosen**: `ContractId { cmr: Cmr, creation_txid: Txid }` — both fields extractable.
**Rejected**: (a) CMR alone. (b) Hash of CMR + creation_txid.
**Why**: CMR alone identifies the program, not the instance. Two on-chain instances with identical params produce the same CMR. While collisions are self-defeating in practice (pools: cosigner=admin makes collision self-inflicted; orders: fresh nonces prevent it), the `creation_txid` component closes all theoretical collision vectors at minimal cost (32 extra bytes, already available from discovery). The struct preserves both fields: `cmr` for discovery dedup (O(1) "do I track anything with this CMR?"), `creation_txid` for instance uniqueness. A hash would discard this extractability.
**Trade-off**: Discovery payloads must include `creation_txid`. The standalone `contract_cmr()` returns only the CMR component — the full `ContractId` is only available after ingestion (when `creation_txid` is known).

### Per-Type Ingestion Methods

**Chosen**: `ingest_market`, `ingest_pool`, `ingest_order` with type-specific snapshot enums.
**Rejected**: Unified `ingest_contract(ContractParams, ChainTransaction)`.
**Why**: Different contract types have genuinely different ingestion needs. Markets always need the creation tx (few transitions, fast catch-up). Pools and orders benefit from non-initial ingestion (pools can have thousands of transitions; order takers don't need history). Per-type methods make each contract's recommended pattern explicit in the type system. The snapshot enums (`PoolSnapshot`, `OrderSnapshot`) document the trade-off at the type level: `Creation` = full history + verified; `Current` = fast start, no prior history, no verification back to creation.

### Non-Initial Ingestion for Pools and Orders

**Chosen**: `PoolSnapshot::Current` and `OrderSnapshot::Current` allow ingestion at any point in a contract's lifecycle.
**Rejected**: Always requiring the creation transaction.
**Why**: LMSR pools can accumulate thousands of state transitions (one per swap). Forward-syncing from creation requires sequential chain queries for each transition. Non-initial ingestion allows a trader to start using a pool immediately from its current state, without replaying its entire history. For limit orders, takers need only the current state — order history is irrelevant for filling. Makers who need history (monitoring, recovery) use `OrderSnapshot::Creation`.
**Trade-off**: Ingesting at `Current` means no prior history is recoverable. This is a one-way choice — the caller cannot "upgrade" to full history without untracking and re-ingesting from creation. The API makes this trade-off explicit via the snapshot enum variants.

### Forward-Only Sync (Backward-Sync Deferred)

**Chosen**: v1 supports only forward-sync. No backward-sync or parallel multi-checkpoint sync.
**Rejected**: Backward-sync for pool price history in v1.
**Why**: Forward-sync from creation (with optional discovery-batched TXID acceleration) covers v1 price history needs. Discovery payloads can include `Vec<Txid>` of transition history, enabling parallel batch-fetching. The engine just processes transactions in chain order — the optimization lives entirely in the discovery/caller layer. Backward-sync adds engine API surface (history backfill method), store trait complexity (out-of-order insertion), and a verified/unverified history distinction that isn't needed when sync is forward-only. All of this can be added later as a non-breaking addition.

### Discovery-Batched Forward Sync

**Chosen**: Pool discovery payloads can include `Vec<Txid>` of historical transitions. The caller batch-fetches all TXIDs in parallel and feeds them to `process_transaction` in chain order.
**Rejected**: Sequential forward-chaining as the only catch-up mechanism.
**Why**: A pool with 1000 transitions requires 1000 sequential "what spent this outpoint?" queries when forward-chaining. With bundled TXIDs, the caller fetches all 1000 transactions in parallel (one batch request) and sorts by chain position. The engine doesn't change — it still processes transactions forward. The optimization is purely in the discovery layer. The engine verifies chain of custody during processing (each tx must spend the previous outpoints), so bundled TXIDs are verified, not blindly trusted.

### Discoverability Trust Gap (OP_RETURN Deferred)

An LMSR pool operator could create a pool, manipulate its price privately (no one can arbitrage because no one knows about it), then announce it on Nostr. The historical price data looks legitimate (all real on-chain transactions) but wasn't subject to market pressure during the private period. The same attack extends to markets: an undiscoverable market + discoverable pool means only the operator can issue tokens and trade.
The ideal solution: embed full contract params in an OP_RETURN output in the creation transaction, making the contract provably discoverable from the chain from the moment of creation. However, `LmsrPoolParams` is 228 bytes and `PredictionMarketParams` is 204 bytes — both exceed Liquid's default 80-byte OP_RETURN relay policy. This is a policy limit (configurable by federation, not a consensus constraint), and Bitcoin Core has recently removed it entirely. When Elements merges this change, OP_RETURN-based discoverability becomes viable. Deferred until then.

### Flat MarketState (Dormant/Unresolved Hidden)

**Chosen**: The public `MarketState` does not distinguish between Dormant (0 outstanding pairs) and Unresolved (>0 outstanding pairs). Both are `Trading`.
**Rejected**: Exposing `CovenantPhase` with Dormant/Unresolved in the public API.
**Why**: The Dormant/Unresolved distinction is a covenant implementation detail (whether a collateral UTXO exists). From the user's perspective, a market with 0 pairs is simply "a market where no one has issued yet" or "a fully cancelled market." The engine hides this distinction in the same way it already hides the initial-vs-subsequent issuance distinction (`build_issuance_pset` handles both). `CovenantPhase` and `SlotType` remain as internal types for script matching and PSET routing.

### Pool Closure via Simplicity Script Path

**Chosen**: Dedicated Simplicity close script path with NUMS internal key (key-spend unspendable).
**Rejected**: Taproot key-spend for pool closure.
**Why**: The LMSR pool already uses NUMS as its internal key — key-spend was never available. A Simplicity close path provides atomic enforcement (all three reserve UTXOs must be consumed together), auditability (the close operation is visible in the witness), and eliminates the partial-spend edge case. See [lmsr-pool-close-path.md](lmsr-pool-close-path.md).

### Dormant Terminal Paths

**Chosen**: Oracle resolution and timelock expiry are available from zero-pair state (both RT UTXOs consumed, market reaches Settled).
**Rejected**: Only allowing resolution/expiry from non-zero-pair state.
**Why**: The same terminal states should be reachable regardless of outstanding pairs. Without this, abandoned or fully-cancelled markets have RT UTXOs that sit on-chain forever. The existing PSET builders (`build_oracle_resolve_pset`, `build_expire_transition_pset`) branch internally based on outstanding pairs — no new builder methods needed. See [market-dormant-terminal-paths.md](market-dormant-terminal-paths.md).

### Untrack Contract

**Chosen**: `untrack_contract` method for removing contracts from the engine.
**Rejected**: No untrack mechanism (deferred indefinitely).
**Why**: With non-initial ingestion, the "untrack + re-ingest from creation" pattern enables promoting a contract from fast-start (no history) to full-history mode. Also serves general cleanup of terminal or unwanted contracts. Simple to implement (delete contract + derived data + history from store).

### Fresh Nonce Recommendation for Order Makers

Order makers should use a fresh nonce for each order, ensuring unique `maker_receive_spk_hash` values across orders. This prevents CMR collisions between orders from the same maker at the same price/direction. While collisions are self-defeating (the maker hides their own duplicate orders), fresh nonces eliminate the issue entirely. This is a recommendation, not an engine-enforced constraint.

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
