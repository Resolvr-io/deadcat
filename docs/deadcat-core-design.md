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

deadcat-sdk      Opinionated wallet integration. Wraps core with a specific
                 wallet backend (LWK), chain backend (Electrum), and signer.
                 Provides the "prepare" pattern: sync wallet -> select UTXOs ->
                 call core PSET builder -> sign -> return prepared tx.
                 Imports deadcat-core.

deadcat-node     Full batteries-included runtime. Wraps SDK with Nostr discovery,
                 SQLite persistence, background sync, Boltz Lightning integration.
                 What Deadcat Live uses. Imports deadcat-sdk.
```

### What Each Layer Owns

| Capability                             | deadcat-core   | deadcat-sdk    | deadcat-node       |
| -------------------------------------- | -------------- | -------------- | ------------------ |
| Simplicity contract compilation        | Yes            |                |                    |
| PSET construction                      | Yes            |                |                    |
| LMSR math (quotes, spot price, tables) | Yes            |                |                    |
| Contract state machine                 | Yes            |                |                    |
| Transaction interpretation             | Yes            |                |                    |
| Asset identification                   | Yes            |                |                    |
| Coin selection utility (pure function) | Yes            |                |                    |
| ContractStore trait (required)         | Yes (defines)  |                | Yes (SQLite impl)  |
| ContractHistory trait (optional)       | Yes (defines)  |                | Yes (SQLite impl)  |
| Wallet (UTXO source, signer)          |                | Yes (LWK)      |                    |
| Chain backend (scan, broadcast)        |                | Yes (Electrum) |                    |
| Fee estimation                         |                | Yes            |                    |
| UTXO selection + preparation           |                | Yes            |                    |
| Nostr discovery                        |                |                | Yes                |
| Background sync                        |                |                | Yes                |
| Boltz swap integration                 |                |                | Yes                |

Note: `deadcat-core` defines the `ContractStore` and `ContractHistory` traits. `deadcat-node` provides concrete SQLite implementations of both. The distinction: core defines the interfaces, node provides storage implementations. A consumer like Aqua would implement these traits against their own database.

## ContractEngine

`ContractEngine` is the central type in `deadcat-core`. It owns the store, manages contract state, processes transactions, and provides interpretation and asset identification.

```rust
pub struct ContractEngine<S: ContractStore> {
    store: S,
    // Internal caches: compiled contracts, asset lookup tables, etc.
}
```

### Exclusive Store Ownership

The engine takes exclusive ownership of the store. The caller creates a `ContractStore` implementation, hands it to `ContractEngine::new()`, and never touches it again. All reads and writes go through the engine's API.

**Why**: If the caller could mutate the store directly, they could advance a contract's outpoints without the engine knowing, or modify state in ways that break the engine's internal invariants. Exclusive ownership ensures the engine is always the single source of truth for contract state.

### API Overview

```rust
impl<S: ContractStore> ContractEngine<S> {
    // Construction
    pub fn new(store: S) -> Self;

    // Contract ingestion
    pub fn ingest_contract(
        &mut self,
        params: ContractParams,
        creation_tx: &ChainTransaction,
    ) -> Result<String, CoreError<S::Error>>;

    // Contract queries (reads — &self)
    pub fn contract(&self, contract_id: &str) -> Result<Option<Contract>, CoreError<S::Error>>;
    pub fn watched_outpoints(&self, page: Pagination) -> Result<Page<OutPoint>, CoreError<S::Error>>;
    pub fn all_covenant_scripts(&self, contract_id: &str) -> Result<Vec<Script>, CoreError<S::Error>>;

    // Per-type listing (reads — &self)
    pub fn list_markets(&self, filter: StateFilter, page: Pagination) -> Result<Page<(String, PredictionMarketParams, MarketState)>, CoreError<S::Error>>;
    pub fn list_pools(&self, filter: StateFilter, page: Pagination) -> Result<Page<(String, LmsrPoolParams, LmsrPoolState)>, CoreError<S::Error>>;
    pub fn list_orders(&self, filter: StateFilter, page: Pagination) -> Result<Page<(String, MakerOrderParams, OrderState)>, CoreError<S::Error>>;

    // Relationship queries (reads — &self)
    pub fn pools_for_market(&self, market_id: &str, page: Pagination) -> Result<Page<(String, LmsrPoolParams, LmsrPoolState)>, CoreError<S::Error>>;
    pub fn orders_for_market(&self, market_id: &str, page: Pagination) -> Result<Page<(String, MakerOrderParams, OrderState)>, CoreError<S::Error>>;

    // Transaction processing (writes — &mut self)
    pub fn process_transaction(&mut self, tx: &ChainTransaction) -> Result<Vec<ConfirmedTransition>, CoreError<S::Error>>;
    pub fn rollback_to_height(&mut self, height: u32) -> Result<(), CoreError<S::Error>>;
    pub fn prune_finalized(&mut self, current_height: u32, finality_depth: u32) -> Result<(), CoreError<S::Error>>;

    // Transaction interpretation (reads — &self)
    pub fn interpret_transaction(&self, tx: &Transaction) -> Vec<Transition>;
    pub fn identify_asset(&self, asset_id: &AssetId) -> Option<AssetInfo>;
}

// History methods — only available when the store implements ContractHistory
impl<S: ContractHistory> ContractEngine<S> {
    pub fn market_history(
        &self,
        contract_id: &str,
        since_height: Option<u32>,
        limit: Option<usize>,
    ) -> Result<Vec<TypedStateUpdate<MarketTransition>>, CoreError<S::Error>>;

    pub fn pool_history(
        &self,
        contract_id: &str,
        since_height: Option<u32>,
        limit: Option<usize>,
    ) -> Result<Vec<TypedStateUpdate<PoolTransition>>, CoreError<S::Error>>;

    pub fn order_history(
        &self,
        contract_id: &str,
        since_height: Option<u32>,
        limit: Option<usize>,
    ) -> Result<Vec<TypedStateUpdate<OrderTransition>>, CoreError<S::Error>>;
}
```

Write methods take `&mut self`. Read methods take `&self`. Rust's borrow rules enforce at compile time that only one writer OR multiple readers can access the engine at any given time — analogous to `RwLock` semantics without runtime overhead. This means store implementors only need to worry about atomic application of state updates, not concurrent access or out-of-order writes.

Note: The history `impl` block uses `S: ContractHistory` rather than `S: ContractStore + ContractHistory` because `ContractHistory` is a supertrait of `ContractStore` — the `ContractStore` bound is implied. See [ContractHistory](#optional-contracthistory).

### Contract ID Generation

Contract IDs are deterministically derived by the engine from the contract's parameters. The same contract always produces the same ID regardless of who ingests it. `ingest_contract` returns the computed ID so the caller can reference it.

**Why deterministic**: If two different wallets ingest the same market, they get the same ID. This enables cross-wallet coordination and deduplication without requiring callers to know the hashing scheme.

### ingest_contract

`ingest_contract` is the entry point for tracking a new contract. It takes the contract's parameters and the on-chain creation transaction:

```rust
pub fn ingest_contract(
    &mut self,
    params: ContractParams,
    creation_tx: &ChainTransaction,
) -> Result<String, CoreError<S::Error>> {
    let contract_id = self.derive_id(&params);
    let compiled = self.compile(&params)?;
    let initial_scripts = compiled.scripts_for_initial_state();
    let outpoints = match_creation_outputs(&creation_tx.tx, &initial_scripts)?;
    let initial_state = derive_initial_state(&outpoints);
    let contract = Contract::from_params_and_state(params, initial_state);
    self.store.track_contract(contract_id.clone(), contract)?;
    Ok(contract_id)
}
```

**No anchor required**: Prediction market creation transactions include blinded reissuance token outputs. The blinding factors for these outputs are derived deterministically from public on-chain data (the defining outpoints), so no out-of-band anchor data is needed. See [Deterministic RT Blinding](deterministic-rt-blinding.md) for the full rationale and derivation spec.

**Why creation-specific logic**: `ingest_contract` scans the creation transaction's **outputs** for covenant scripts, because the creation tx produces outpoints from nothing — there are no prior outpoints to track. This is fundamentally different from `process_transaction`, which scans a transaction's **inputs** for tracked outpoints (the UTXO-following model). The two share downstream components (script derivation, store persistence) but their entry logic diverges because creation and state-transition are genuinely different operations.

**Caller responsibility**: The caller must ensure the creation transaction has already been confirmed on-chain before passing it to `ingest_contract`. Core does not verify chain inclusion — it verifies that the transaction's outputs match the expected covenant scripts derived from the provided parameters.

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
    self.store.apply_transitions(&updates)?;  // durable write FIRST
    self.update_in_memory_state(&transitions);  // then update caches
    Ok(transitions)
}
```

**Idempotent**: If the same transaction is processed twice (e.g., after crash recovery), the second call is a no-op — the spent outpoints are no longer tracked, so no contracts are affected.

**Durable before returning**: The store write completes before the method returns. A crash between "computed" and "persisted" cannot leave the engine in an inconsistent state. The in-memory cache is updated only after the store confirms the write.

**Why the engine applies transitions internally (not the caller)**: Processing and persistence are a single atomic operation. If the caller had to manually apply transitions, a crash between "engine returned results" and "caller applied them" would leave the engine's in-memory state ahead of the store. The engine owns the store, so it owns the write.

### interpret_transaction

`interpret_transaction` is the primary read method for wallet integration. It uses the same witness decoding and output matching logic as `process_transaction` but does not modify state:

```rust
pub fn interpret_transaction(&self, tx: &Transaction) -> Vec<Transition>;
```

**Works for confirmed and unconfirmed transactions**: Unlike `process_transaction` (confirmed only), `interpret_transaction` accepts any transaction — confirmed or unconfirmed. It works as long as the transaction spends outpoints the engine currently tracks in its durable state. This enables "pending transaction" UX: a wallet can interpret an unconfirmed mempool transaction to display "Pending issuance" or "Pending trade" before the transaction confirms.

**Known limitation — chained unconfirmed transactions**: If two unconfirmed transactions form a chain (tx2 spends an output created by tx1), only tx1 is interpretable. Tx2 spends outpoints that the engine hasn't durably recorded (tx1 was never processed), so the engine doesn't recognize them. Once both confirm and are fed to `process_transaction`, both are processed normally. This is rare in practice — Liquid has ~1-minute blocks, and chained unconfirmed covenant transactions require dependent operations within that window.

**No chain position metadata**: `interpret_transaction` takes a raw `elements::Transaction` without block height or tx index. Its return type (`Transition`) omits chain position fields. This is in contrast to `process_transaction`, which returns `ConfirmedTransition` wrapping `Transition` with a `ChainPosition`. See [Transition and ConfirmedTransition](#transition-and-confirmedtransition).

**Point-in-time query**: The results reflect what the engine currently knows. If a transaction spends UTXOs from a contract the engine hasn't ingested yet, those contracts are simply absent from the results. After ingesting the contract and catching it up, calling `interpret_transaction` on the same transaction returns additional results.

**Partial knowledge grows over time**: A trade transaction that spends a known limit order and an unknown pool would initially return only the order fill. After the pool is ingested, the same call would also return the pool swap. The caller should be prepared to re-interpret transactions as new contracts are ingested.

### contract

Returns the current state of a single tracked contract by ID:

```rust
pub fn contract(&self, contract_id: &str) -> Result<Option<Contract>, CoreError<S::Error>>;
```

Returns `None` if the contract hasn't been ingested. The caller matches on the `Contract` enum to access the typed state. Since callers typically know the contract type (they ingested it), this is a single-variant match.

### all_covenant_scripts

Returns all possible covenant script pubkeys for a tracked contract, across **all** covenant phases and slot types. This is the full set of scripts the contract could ever produce, derived deterministically from the contract's parameters.

**Primary use case**: Catch-up scanning. After ingesting a contract, the caller uses these scripts to scan the chain for historical transactions that touched the contract since creation.

**Not for steady-state monitoring**: To watch for new spends of a contract's current UTXOs, use `watched_outpoints()` instead — it returns the specific outpoints the engine is currently tracking, which is more precise than the full script set.

### Per-Type Listing Methods

The typed listing methods (`list_markets`, `list_pools`, `list_orders`) delegate to the store's per-type methods internally, then unwrap the `Contract` enum to return typed results. All accept `StateFilter` and `Pagination` parameters.

```rust
pub fn list_markets(
    &self,
    filter: StateFilter,
    page: Pagination,
) -> Result<Page<(String, PredictionMarketParams, MarketState)>, CoreError<S::Error>>;
```

The store returns `Page<(String, Contract)>` (the generic enum). The engine unwraps each `Contract` variant to extract the typed params and state. This keeps the store trait consistent while giving callers typed results.

**Pagination**: All listing methods use cursor-based pagination. See [Pagination Types](#pagination-types).

**State filtering**: `StateFilter::ActiveOnly` returns contracts with active outpoints. `StateFilter::TerminalOnly` returns settled/closed/consumed/cancelled contracts. `StateFilter::All` returns both. At Polymarket scale (thousands of markets), filtering at the store level avoids paging through thousands of irrelevant terminal contracts.

### Relationship Queries

```rust
pub fn pools_for_market(&self, market_id: &str, page: Pagination) -> Result<Page<(String, LmsrPoolParams, LmsrPoolState)>, CoreError<S::Error>>;
pub fn orders_for_market(&self, market_id: &str, page: Pagination) -> Result<Page<(String, MakerOrderParams, OrderState)>, CoreError<S::Error>>;
```

Return pools or orders associated with a specific market. The relationship is encoded in pool/order params (they reference the market's token asset IDs). The store maintains a secondary index on market_id for efficient lookups.

Pools and orders are split into separate methods because they scale differently — a market typically has a handful of pools but potentially thousands of orders at Polymarket scale. Both are paginated.

### History Methods

The three typed history methods (`market_history`, `pool_history`, `order_history`) are only available when the store implements `ContractHistory`. They delegate to the store's unified `transition_history` method internally, then unwrap the `TransitionDetails` enum to return typed results:

```rust
// Engine calls store's unified method, then unwraps per-contract type
pub fn market_history(&self, contract_id: &str, ...) -> Result<Vec<TypedStateUpdate<MarketTransition>>, ...> {
    let raw = self.store.transition_history(contract_id, since_height, limit)?;
    raw.into_iter().map(|u| {
        let TransitionDetails::Market(details) = u.details else {
            debug_assert!(false, "store returned non-market transition for market contract");
            // filter out mismatched entries
        };
        TypedStateUpdate { contract_id: u.contract_id, txid: u.txid, /* ... */ details }
    }).collect()
}
```

**Why typed convenience methods**: The caller always knows the contract type when querying history. The unified `StateUpdate` with `TransitionDetails` enum forces an unnecessary match on a variant the caller already knows. The typed methods eliminate this ergonomic cost. The store trait stays simple (one `transition_history` method); the engine does the trivial unwrapping.

**Invariant**: All transitions for a given `contract_id` always have the same `TransitionDetails` variant (a market contract only produces `Market` transitions). A mismatch indicates a bug in the store implementation — the engine asserts in debug and filters in release.

## Core Types

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

The input to `process_transaction` and `ingest_contract`. Represents a **confirmed** on-chain transaction. Core does not validate consensus — it interprets. This is a safe assumption because the caller gets transactions from their chain backend, which only provides consensus-valid data.

```rust
pub struct ChainTransaction {
    pub tx: elements::Transaction,
    pub position: ChainPosition,
}
```

**Confirmed only**: `process_transaction` and `ingest_contract` require confirmed transactions with valid block height and tx index. For unconfirmed/mempool transactions, use `interpret_transaction` which takes a raw `elements::Transaction` without chain position metadata.

### ContractParams

The input to `ingest_contract`. The parameters that define a contract, from which core derives script pubkeys, compiles Simplicity contracts, and generates the contract ID.

```rust
pub enum ContractParams {
    PredictionMarket(PredictionMarketParams),
    LmsrPool(LmsrPoolParams),
    MakerOrder(MakerOrderParams),
}
```

`ContractParams` is purely definitional — it contains only the data needed to derive the contract's identity (contract ID) and addresses (covenant script pubkeys). No creation-time secrets or blinding factors. Given `ContractParams` + network + the Simplicity source code (built into `deadcat-core`), all covenant addresses for all states can be derived deterministically.

`PredictionMarketParams` defines the market's covenant parameters (oracle key, expiry, etc.). `LmsrPoolParams` defines the pool's parameters (token asset IDs referencing the parent market, liquidity parameters). `MakerOrderParams` defines the order's parameters (base/quote asset IDs, price, direction).

### Contract

The three covenant types core tracks internally. This is an **internal type** managed by the engine and store — callers do not construct `Contract` values directly. Instead, they provide `ContractParams` + creation transaction to `ingest_contract`, and the engine derives the initial contract state.

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

Each variant's mutable state (outpoints, reserves, fill amounts) lives inside the state enum, not alongside it. This prevents stale field values when a contract reaches a terminal state. See [Contract State Enums](#contract-state-enums) below.

### Contract State Enums

Each contract type has a state enum that represents its current tip state — the latest snapshot, not a history log. This is stored durably via `ContractStore` (required) and updated each time `process_transaction` advances the contract. The tip state carries enough information for basic wallet UX without requiring `ContractHistory`.

#### MarketState

```rust
pub enum MarketState {
    Active {
        phase: CovenantPhase,
        outpoints: Vec<(SlotType, OutPoint)>,
    },
    Settled {
        final_txid: Txid,
        settlement: SettlementKind,
    },
}

pub enum CovenantPhase {
    Dormant,
    Unresolved,
    ResolvedYes,
    ResolvedNo,
    Expired,
}

pub enum SettlementKind {
    Redeemed { outcome_yes: bool },
    ExpiryRedeemed,
    Cancelled,
}
```

`CovenantPhase` represents the on-chain Simplicity covenant state. `MarketState` is broader — it includes both the on-chain phases (within `Active`) and the terminal state (`Settled`) where no covenant UTXOs remain.

`SettlementKind` carries the outcome so a wallet can answer "did this market resolve YES or NO?" without requiring transition history. This is essential for basic UX: showing "Your YES tokens are redeemable" vs "Your YES tokens are worthless."

#### SlotType and CovenantPhase Mapping

```rust
pub enum SlotType {
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
        outpoints: BTreeMap<ReserveSlot, OutPoint>,
        s_index: u64,
        reserves: PoolReserves,
    },
    Closed {
        final_txid: Txid,
        reclaimed: PoolReserves,
    },
}
```

Active pools track their outpoints as a map from reserve slot to outpoint. This is flexible enough to handle the current covenant (always 3 outpoints) and future covenant versions where individual reserves can be fully consumed (fewer than 3 active outpoints).

`Closed` carries `reclaimed` reserves so the wallet can display "You closed this pool and reclaimed X YES, Y NO, Z L-BTC" without requiring transition history.

#### OrderState

```rust
pub enum OrderState {
    Active {
        outpoint: OutPoint,
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

`total_filled` on `Active` enables "5,000 of 10,000 sats filled" display while the order is still live. `Consumed` means fully filled — `total_filled` equals the original offered amount by definition, so it's not stored. `Cancelled` with `total_filled` enables "partially filled then cancelled" display (if `total_filled == 0`, it was a clean cancellation).

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

**Cursor opacity**: The store encodes whatever it needs into the cursor string (e.g., last seen contract_id, ordering position). The caller passes cursors back without interpreting them. If a caller passes a cursor from one method to a different method, the store should return an error.

**Count-only queries**: Pass `limit: 0` to get just the `total` count without loading any items. No separate count methods needed.

**`total` is always included**: Every paginated response includes the total count of matching items across all pages. This enables "showing 1-20 of 1,234 markets" UI without separate count calls. Implementors can cache counts if the overhead of `COUNT(*)` becomes significant.

### Side

```rust
pub enum Side { Yes, No }
```

Used in `OutputRole` to identify which outcome token an output represents.

### ContractMatch

Returned by `ContractStore::find_by_outpoints`. Used internally by the engine to identify which tracked contracts are affected by a transaction and which specific outpoints matched. Callers of the engine never see this type — it exists at the engine-store boundary only.

```rust
pub struct ContractMatch {
    pub contract_id: String,
    pub matched_outpoints: Vec<OutPoint>,
}
```

### Transition and ConfirmedTransition

The core transition data, split into two types based on whether chain position is known:

```rust
pub struct Transition {
    pub contract_id: String,
    pub txid: Txid,
    pub old_outpoints: Vec<OutPoint>,
    pub new_outpoints: Vec<OutPoint>,
    pub details: TransitionDetails,
    pub contract_outputs: Vec<u32>,          // which output indices are covenant state
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

The stripped-down form passed to the store internally by the engine. Does not include the computed output classification fields (`contract_outputs`, `external_outputs`) since those are derived from the transaction at query time and do not need to be persisted:

```rust
pub struct StateUpdate {
    pub contract_id: String,
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
    pub contract_id: String,
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
    Issued { pairs: u64, collateral_locked: u64, initial: bool },
    Resolved { outcome_yes: bool },
    Redeemed { tokens_burned: u64, payout_sats: u64 },
    Cancelled { pairs_burned: u64, collateral_returned: u64 },
    Expired,
}

pub enum PoolTransition {
    Swapped { old_s_index: u64, new_s_index: u64, new_reserves: PoolReserves },
    Adjusted { old_reserves: PoolReserves, new_reserves: PoolReserves },
    Closed { reclaimed: PoolReserves },
}

pub enum OrderTransition {
    Filled { amount: u64 },
    Cancelled,
}
```

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
    RedemptionPayout,
    TradeReceive { side: Side },
    Change,
    Fee,
    Unknown,
}
```

`OutputRole` is purely semantic — it labels what the output represents in the transaction, not its asset or value. The asset and value are already available on `ExternalOutput::Explicit`, so the role does not duplicate them. `side: Side` is kept on `IssuedTokens` and `TradeReceive` because it adds genuinely new information that would otherwise require an asset lookup to determine.

### AssetInfo

Result of asset identification:

```rust
pub enum AssetInfo {
    YesToken { market_id: String, params: PredictionMarketParams },
    NoToken { market_id: String, params: PredictionMarketParams },
    YesReissuanceToken { market_id: String },
    NoReissuanceToken { market_id: String },
}
```

### CoreError

The error type for all engine operations. Generic over the store's error type, which piggybacks on the engine's existing `S: ContractStore` generic — no additional type parameter burden for consumers.

```rust
pub enum CoreError<E: std::error::Error> {
    Store(E),
    InvalidCreationTx { reason: String },
    ContractNotFound { contract_id: String },
    ContractAlreadyTracked { contract_id: String },
    DataIntegrity { detail: String },
}
```

**Why generic over the store error**: The engine is already generic over `S: ContractStore`, so `CoreError<S::Error>` adds no new generic parameters. Store error types are preserved — consumers can match on `CoreError::Store(e)` and handle their specific store error without downcasting. Store implementors define their own error type independently via an associated type on the trait.

### InsufficientFunds

Error type for the standalone `select_utxos` function:

```rust
pub struct InsufficientFunds {
    pub available: u64,
    pub required: u64,
}
```

The only failure mode for coin selection is insufficient funds. A dedicated struct (rather than an enum) reflects that there is exactly one failure case. The `available` and `required` fields enable wallet UX like "you have 5,000 sats but need 10,000."

## Core Design: UTXO-Following State Machine

### Fundamental Loop

The core of `deadcat-core` is a state machine that tracks covenant UTXOs:

1. **Ingest**: Register a contract and its creation transaction (core derives initial outpoints)
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

Contract creation is the one case with no prior outpoints, which is why `ingest_contract` has its own output-scanning logic (see [ingest_contract](#ingest_contract)). All subsequent state transitions use input-based tracking via `process_transaction`.

### Processing a Transaction

When `process_transaction` is called:

1. Collect all input outpoints from the transaction
2. Check which tracked contracts own any of those outpoints (via `ContractMatch`)
3. For each affected contract, decode the Simplicity witness to determine the transition type
4. Find the contract's new outputs by matching script pubkeys against expected covenant scripts (derived from params + new state)
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

### State Advancement via Witness Decoding

Rather than guessing which outputs belong to a contract by checking scripts alone, core also decodes the Simplicity witness data from the spending transaction's inputs. The witness encodes exactly what transition happened:

- **LMSR pools**: Witness contains `OLD_S_INDEX`, `NEW_S_INDEX`, and trade parameters
- **Prediction markets**: The covenant script path identifies the transition type (issuance path, resolution path, redemption path, etc.)
- **Maker orders**: Witness identifies fill vs cancellation

This is deterministic — no ambiguity about what each output represents.

## Contract Ingestion

Core doesn't know or care about the discovery layer (Nostr, manual import, QR code, etc.). To start tracking a contract, the caller provides:

- Contract parameters (from which core derives script pubkeys and the contract ID)
- The confirmed creation transaction

Core compiles the Simplicity contract from the parameters, derives deterministic blinding factors for creation verification (see [Deterministic RT Blinding](deterministic-rt-blinding.md)), verifies the creation transaction contains the expected covenant scripts, derives the initial outpoints, and begins tracking.

The caller is then responsible for scanning the chain from the creation point forward and feeding any relevant transactions to `process_transaction`. Core has no chain access — it processes whatever it's given.

### Catching Up New Contracts

When a contract is ingested that was created in the past, it needs to be "caught up" to the chain tip. Core doesn't manage this — it's the caller's responsibility:

1. Call `ingest_contract` with params and creation transaction (core derives initial outpoints)
2. Call `all_covenant_scripts` to get the full set of scripts for chain scanning
3. Scan the chain for transactions matching those scripts since creation height
4. Feed those transactions to `process_transaction` in order

Core doesn't distinguish between "catch-up" and "tip" transactions. It processes whatever it receives. Existing fully-synced contracts can continue processing new tip transactions while a newly-ingested contract catches up — each contract tracks its own outpoints independently.

Whether a contract is "caught up" is determined by the caller, not core. The caller knows the chain tip and knows whether it has scanned all relevant scripts up to that point. Core has no concept of a global sync tip.

## Persistence: Store Trait

Core defines traits for state persistence. The implementor controls what to store and how. All methods are required (no default implementations) — the store trait is designed for Polymarket-scale operation where every query needs efficient indexing. If an implementor wants to take shortcuts (e.g., implement `list_markets` by scanning all contracts and filtering), the performance cost is visible in their own code, not hidden behind a default.

**Future optimization**: If profiling shows that the one-method-on-store + typed-convenience-on-engine pattern for listing becomes a bottleneck, the store trait could be extended with per-type methods that allow the store to optimize queries at the storage level. The current design keeps the trait surface minimal while the engine handles typed unwrapping.

### Required: ContractStore

```rust
pub trait ContractStore {
    type Error: std::error::Error;

    // Single contract lookup — &self
    fn current_state(&self, contract_id: &str) -> Result<Option<Contract>, Self::Error>;

    // Bulk reads — &self
    fn find_by_outpoints(&self, outpoints: &[OutPoint]) -> Result<Vec<ContractMatch>, Self::Error>;
    fn watched_outpoints(&self, page: Pagination) -> Result<Page<OutPoint>, Self::Error>;

    // Per-type listing — &self
    fn list_markets(&self, filter: StateFilter, page: Pagination) -> Result<Page<(String, Contract)>, Self::Error>;
    fn list_pools(&self, filter: StateFilter, page: Pagination) -> Result<Page<(String, Contract)>, Self::Error>;
    fn list_orders(&self, filter: StateFilter, page: Pagination) -> Result<Page<(String, Contract)>, Self::Error>;

    // Relationship queries — &self
    fn pools_for_market(&self, market_id: &str, page: Pagination) -> Result<Page<(String, Contract)>, Self::Error>;
    fn orders_for_market(&self, market_id: &str, page: Pagination) -> Result<Page<(String, Contract)>, Self::Error>;

    // Writes — &mut self
    fn track_contract(&mut self, contract_id: String, contract: Contract) -> Result<(), Self::Error>;
    fn apply_transitions(&mut self, transitions: &[StateUpdate]) -> Result<(), Self::Error>;
    fn rollback_to_height(&mut self, height: u32) -> Result<(), Self::Error>;
    fn prune_finalized(&mut self, current_height: u32, finality_depth: u32) -> Result<(), Self::Error>;
}
```

Every consumer must implement this. Read methods take `&self`, write methods take `&mut self` — mirroring the engine's own borrow semantics. The engine calls read methods during interpretation (`&self` on the engine borrows the store as `&self`) and write methods during processing (`&mut self` on the engine borrows the store as `&mut self`).

`apply_transitions` must be durable when it returns — the engine depends on this for crash safety.

`find_by_outpoints` is the hot-path method called on every `process_transaction`. It is not paginated because its input is bounded by the transaction's input count (constrained by Liquid's transaction size limits).

**Atomicity requirements**: Contract-level atomicity is a hard requirement — a single contract's state update (old outpoints -> new outpoints + state change) must be all-or-nothing. A half-updated contract is corrupted state. Transaction-level atomicity (all contracts updated together for a multi-contract transaction) is recommended but not strictly required for correctness. A "jagged" state where one contract has processed a transaction but another hasn't is indistinguishable from staggered ingestion — which is already a normal condition when contracts are discovered at different times. The system self-heals: re-processing the transaction advances the remaining contracts while already-processed contracts are a no-op (idempotency). Transaction-level atomicity is recommended because it's typically not much extra burden on top of the already-required contract-level atomicity (e.g., a single SQLite transaction) and avoids the jagged-view window.

### Optional: ContractHistory

```rust
pub trait ContractHistory: ContractStore {
    fn transition_history(
        &self,
        contract_id: &str,
        since_height: Option<u32>,
        limit: Option<usize>,
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

- "Did this market resolve YES or NO?" -> `MarketState::Settled { settlement: SettlementKind::Redeemed { outcome_yes } }`
- "How much of my order has been filled?" -> `OrderState::Active { total_filled }` or `OrderState::Cancelled { total_filled }`
- "What reserves did I reclaim when I closed my pool?" -> `LmsrPoolState::Closed { reclaimed }`

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

Core provides pure PSET builder functions. These take explicit inputs (UTXOs, params, amounts) and return unsigned PSETs. No wallet access, no chain queries.

The existing PSET builders already follow this pattern:

```rust
pub fn build_initial_issuance_pset(
    contract: &CompiledPredictionMarket,
    params: &InitialIssuanceParams,  // contains explicit UnblindedUtxo inputs
) -> Result<PartiallySignedTransaction>;
```

The caller (SDK or wallet) is responsible for selecting UTXOs, constructing the params, and signing the resulting PSET.

### Coin Selection Utility

Core provides a standalone coin selection function. It takes a UTXO list from the caller's wallet and returns a selected subset. It does not own or access any wallet state — it's a pure function:

```rust
pub fn select_utxos(
    available: &[UnblindedUtxo],
    target_asset: &AssetId,
    target_amount: u64,
    exclude: &[OutPoint],
) -> Result<Vec<UnblindedUtxo>, InsufficientFunds>;
```

## Simplicity Contracts

Core contains the `.simf` Simplicity contract source code and the compiler integration. Given contract parameters and a network type (testnet/mainnet), core can:

- Compile the Simplicity contract
- Derive script pubkeys for any state
- Decode witness data from spending transactions

This is necessary for both PSET construction (building covenant outputs with correct scripts) and state advancement (matching output scripts to determine new state).

## Reorg Handling

Core maintains a processing log: for each processed transaction, the contract ID, old outpoints, new outpoints, and block height. Rolling back to height N means:

1. Find all transitions from blocks strictly above N
2. Reverse them: restore old outpoints, remove new outpoints
3. Delete the processing records
4. Remove contracts whose creation transaction was in blocks strictly above N

Contracts created at height N are kept. Contracts created at height N+1 or above are removed — their creation transactions may no longer exist on the canonical chain after the reorg.

The caller detects the reorg (their chain backend tells them), calls `rollback_to_height`, then re-scans and feeds the new chain data.

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

Write methods (`process_transaction`, `ingest_contract`, `rollback_to_height`, `prune_finalized`) take `&mut self`. Read methods (`interpret_transaction`, `identify_asset`, `watched_outpoints`, `all_covenant_scripts`, `contract`, `list_markets`, `list_pools`, `list_orders`, `pools_for_market`, `orders_for_market`) take `&self`.

Rust's borrow rules provide compile-time `RwLock` semantics: multiple concurrent readers OR one exclusive writer, enforced without runtime overhead. For single-threaded consumers this is invisible. For multi-threaded consumers who need concurrent access, wrap the engine in `RwLock<ContractEngine<S>>`:

```rust
let engine = Arc::new(RwLock::new(ContractEngine::new(store)));

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

## Example Integration: Aqua Wallet

```rust
use deadcat_core::{ContractEngine, ContractParams, ChainTransaction, Pagination, StateFilter};

// 1. Initialize engine with a store implementation.
//    The store is exclusively owned by the engine from this point.
let mut engine = ContractEngine::new(aqua_deadcat_store);

// 2. Ingest a market (discovered via Nostr, import, etc.)
//    No anchor needed — core derives blinding factors deterministically.
//    Core verifies the creation tx, derives initial outpoints, and returns the contract ID.
let contract_id = engine.ingest_contract(
    ContractParams::PredictionMarket(params),
    &creation_tx,
)?;

// 3. Catch up: scan chain for this contract's history since creation.
//    all_covenant_scripts returns scripts for ALL possible states.
let scripts = engine.all_covenant_scripts(&contract_id)?;
let historical_txs = aqua_chain.scan_history(&scripts, creation_tx.position.block_height);
for tx in historical_txs {
    engine.process_transaction(&tx)?;
}

// 4. Steady state: process new confirmed transactions as they arrive
for tx in aqua_chain.poll_new_transactions() {
    let transitions = engine.process_transaction(&tx)?;
    for ct in &transitions {
        log::info!("Contract {} transitioned at block {}", ct.transition.contract_id, ct.position.block_height);
    }
}

// 5. Pending UX: interpret unconfirmed mempool transactions (read-only)
if let Some(mempool_tx) = aqua_chain.get_mempool_tx(txid) {
    let interpretations = engine.interpret_transaction(&mempool_tx);
    for interp in &interpretations {
        render_pending_label(&interp.details);
    }
}

// 6. Label wallet history (read-only, can be called anytime)
for wallet_tx in wallet_history {
    let interpretations = engine.interpret_transaction(&wallet_tx);
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

// 7. Identify assets in wallet balance
for (asset_id, amount) in wallet_balance {
    if let Some(info) = engine.identify_asset(&asset_id) {
        render_token_position(&info, amount);
    }
}

// 8. Browse markets with pagination
let page = engine.list_markets(
    StateFilter::ActiveOnly,
    Pagination { after: None, limit: 20 },
)?;
for (id, params, state) in &page.items {
    render_market_card(id, params, state);
}
// Next page:
if let Some(cursor) = page.next_cursor {
    let next_page = engine.list_markets(
        StateFilter::ActiveOnly,
        Pagination { after: Some(cursor), limit: 20 },
    )?;
}

// 9. Build a transaction (core provides PSET, Aqua signs)
let utxos = aqua_wallet.list_utxos();
let selected = deadcat_core::select_utxos(&utxos, &asset, amount, &[])?;
let pset = deadcat_core::build_initial_issuance_pset(&contract, &params)?;
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
**Why**: Input-based tracking is precise — a tracked outpoint is either spent or not. Output-based scanning requires checking against all scripts x all states x all contracts, and can false-positive when someone sends coins to a covenant address without going through the covenant spend path. Contract creation is the one exception (no prior outpoints), handled by `ingest_contract`'s output-scanning logic.

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

**Chosen**: `ingest_contract` has its own output-scanning logic, separate from `process_transaction`.
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
**Why**: The same witness decoding and output matching logic serves both purposes. Having two systems would mean two places to update when covenant logic changes. The only difference is mutability — processing writes new state, interpretation just reads.

### Advance Logic Uses Witness Decoding

**Chosen**: Decode Simplicity witness data to determine transition type.
**Rejected**: Pattern-match transaction structure (input/output counts, script matching).
**Why**: Witness decoding is deterministic and precise. The Simplicity witness encodes exactly what happened (old state, new state, trade parameters). Pattern matching is fragile — it requires recognizing every valid transaction layout and breaks on novel combinations. Witness decoding works for any valid covenant spend regardless of the surrounding transaction structure.

### Output Identification via Script Pubkey Matching

**Chosen**: Identify contract outputs by matching `script_pubkey` against expected covenant scripts.
**Rejected**: Assume fixed output indices based on known PSET layouts.
**Why**: Multi-contract transactions (routed trades, combined operations) have variable output indices. LMSR pool reserve inputs can be interleaved with order inputs at arbitrary positions. Script pubkey matching works regardless of output ordering and handles reissuance token outputs (which have `Asset::Null, Value::Null` but valid script pubkeys) correctly.

### Interpretation Reflects Current Knowledge

**Chosen**: `interpret_transaction` returns results based on currently-ingested contracts. Unknown contracts are silently absent from results. Results grow as more contracts are ingested.
**Rejected**: Returning "unknown contract detected" markers or requiring all contracts to be ingested before interpretation works.
**Why**: Core can't know what it doesn't know. A transaction might touch contracts from markets the caller hasn't discovered yet. Returning empty for unknown contracts is honest and composable — the caller re-interprets after ingesting new contracts. Markers would require heuristic detection of "looks like a covenant" which is fragile. The caller's mental model is simple: "interpretation results may improve over time as I discover more contracts."

### Consensus-Validity Assumption

**Chosen**: Core assumes all transactions passed to it are consensus-valid. `process_transaction` and `ingest_contract` require confirmed transactions. `interpret_transaction` accepts unconfirmed transactions but still assumes consensus validity (mempool-accepted).
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
**Why**: `ContractHistory` is optional — minimal consumers (Aqua) may not implement it. The tip state must answer basic wallet questions independently: market outcome (YES/NO/cancelled/expired), order fill progress, pool reclaimed reserves. Richer features (price charts, fill-by-fill breakdowns, audit trails) belong in transition history.

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
**Why**: The Elements protocol requires reissuance token outputs to be blinded (ABF != 0) for reissuance to work. Traditionally, random ABFs are used as an authorization mechanism — only someone who knows the ABF can reissue. With Simplicity covenants, authorization is enforced by the covenant itself, not by ABF secrecy. Using deterministic ABFs derived from public data (defining outpoints via tagged hash) satisfies the protocol requirement while eliminating the need for anchor distribution. This simplifies the `ingest_contract` API (no anchor parameter), the Nostr announcement format, and removes the "lost anchor" failure mode. See [Deterministic RT Blinding](deterministic-rt-blinding.md) for the derivation spec.
