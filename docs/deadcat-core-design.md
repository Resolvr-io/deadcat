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

    // Contract management
    pub fn track_contract(&mut self, contract: Contract) -> Result<String>;
    pub fn watched_outpoints(&self) -> Result<Vec<OutPoint>>;
    pub fn scripts_for_contract(&self, contract_id: &str) -> Result<Vec<Script>>;

    // Transaction processing (writes — &mut self)
    pub fn process_transaction(&mut self, tx: &ChainTransaction) -> Result<Vec<TransitionResult>>;
    pub fn rollback_to_height(&mut self, height: u32) -> Result<()>;
    pub fn prune_finalized(&mut self, current_height: u32, finality_depth: u32) -> Result<()>;

    // Transaction interpretation (reads — &self)
    pub fn interpret_transaction(&self, tx: &Transaction) -> Vec<TransitionResult>;
    pub fn identify_asset(&self, asset_id: &AssetId) -> Option<AssetInfo>;
}
```

Write methods take `&mut self`. Read methods take `&self`. Rust's borrow rules enforce at compile time that only one writer OR multiple readers can access the engine at any given time — analogous to `RwLock` semantics without runtime overhead. This means store implementors only need to worry about atomic application of state updates, not concurrent access or out-of-order writes.

### process_transaction

`process_transaction` is the core write method. It computes transitions, durably persists them, then returns the results:

```rust
pub fn process_transaction(
    &mut self,
    tx: &ChainTransaction,
) -> Result<Vec<TransitionResult>> {
    let results = self.compute_transitions(tx)?;
    if results.is_empty() {
        return Ok(results);  // idempotent: nothing affected
    }
    let updates: Vec<StateUpdate> = results.iter().map(to_state_update).collect();
    self.store.apply_transitions(&updates)?;  // durable write FIRST
    self.update_in_memory_state(&results);     // then update caches
    Ok(results)
}
```

**Idempotent**: If the same transaction is processed twice (e.g., after crash recovery), the second call is a no-op — the spent outpoints are no longer tracked, so no contracts are affected.

**Durable before returning**: The store write completes before the method returns. A crash between "computed" and "persisted" cannot leave the engine in an inconsistent state. The in-memory cache is updated only after the store confirms the write.

**Why the engine applies transitions internally (not the caller)**: Processing and persistence are a single atomic operation. If the caller had to manually apply transitions, a crash between "engine returned results" and "caller applied them" would leave the engine's in-memory state ahead of the store. The engine owns the store, so it owns the write.

### interpret_transaction

`interpret_transaction` is the primary read method for wallet integration. It uses the same witness decoding and output matching logic as `process_transaction` but does not modify state:

```rust
pub fn interpret_transaction(&self, tx: &Transaction) -> Vec<TransitionResult>;
```

**Point-in-time query**: The results reflect what the engine currently knows. If a transaction spends UTXOs from a contract the engine hasn't ingested yet, those contracts are simply absent from the results. After ingesting the contract and catching it up, calling `interpret_transaction` on the same transaction returns additional results.

**Partial knowledge grows over time**: A trade transaction that spends a known limit order and an unknown pool would initially return only the order fill. After the pool is ingested, the same call would also return the pool swap. The caller should be prepared to re-interpret transactions as new contracts are ingested.

**Shared return type**: Both `process_transaction` and `interpret_transaction` return `Vec<TransitionResult>`. This is intentional — one type, one mental model. The caller doesn't need to think about which method to use for a given situation. Processing mutates and returns. Interpretation reads and returns.

## Core Types

### ChainTransaction

The input to `process_transaction`. Core assumes this represents a consensus-valid transaction (confirmed on-chain or accepted into mempool). Core does not validate transactions — it interprets them. This is a safe assumption because the caller gets transactions from their chain backend, which only provides consensus-valid data.

```rust
pub struct ChainTransaction {
    pub tx: elements::Transaction,
    pub block_height: u32,
    pub tx_index: u32,  // position within block, for ordering
}
```

### Contract

The three covenant types core tracks:

```rust
pub enum Contract {
    PredictionMarket {
        params: PredictionMarketParams,
        anchor: PredictionMarketAnchor,
        state: MarketState,
        outpoints: Vec<(SlotType, OutPoint)>,
    },
    LmsrPool {
        params: LmsrPoolParams,
        s_index: u64,
        reserves: PoolReserves,
        outpoints: [OutPoint; 3],  // yes, no, collateral
    },
    MakerOrder {
        params: MakerOrderParams,
        outpoint: OutPoint,
        status: OrderStatus,
    },
}
```

Each variant knows its own outpoints and how to advance. The advance logic uses the Simplicity witness and covenant script structure to deterministically identify the transition and new state.

### TransitionResult

The full computed result returned to the caller from both `process_transaction` and `interpret_transaction`:

```rust
pub struct TransitionResult {
    pub contract_id: String,
    pub txid: Txid,
    pub block_height: u32,
    pub tx_index: u32,
    pub old_outpoints: Vec<OutPoint>,
    pub new_outpoints: Vec<OutPoint>,
    pub details: TransitionDetails,
    pub contract_outputs: Vec<u32>,          // which output indices are covenant state
    pub external_outputs: Vec<ExternalOutput>, // non-covenant outputs with roles
}
```

### StateUpdate

The stripped-down form passed to the store internally by the engine. Does not include the computed output classification fields (`contract_outputs`, `external_outputs`) since those are derived from the transaction at query time and do not need to be persisted:

```rust
pub struct StateUpdate {
    pub contract_id: String,
    pub txid: Txid,
    pub block_height: u32,
    pub tx_index: u32,
    pub old_outpoints: Vec<OutPoint>,
    pub new_outpoints: Vec<OutPoint>,
    pub details: TransitionDetails,
}
```

**Why two types**: `TransitionResult` is the caller-facing view (full data, including ephemeral computed fields). `StateUpdate` is the storage-facing view (only what needs to be persisted). The engine converts between them internally. This prevents store implementors from accidentally persisting wallet-specific data (output roles, classifications) alongside contract state, while ensuring callers always get the full picture.

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

Non-covenant outputs in a transaction, with their roles identified:

```rust
pub struct ExternalOutput {
    pub index: u32,
    pub script_pubkey: Script,
    pub asset: Option<AssetId>,  // None if blinded
    pub value: Option<u64>,      // None if blinded
    pub role: OutputRole,
}

pub enum OutputRole {
    IssuedTokens { side: Side, amount: u64 },
    RedemptionPayout { amount: u64 },
    TradeReceive { side: Side, amount: u64 },
    Change { asset: AssetId, amount: u64 },
    Fee { amount: u64 },
    Unknown,
}
```

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

## Core Design: UTXO-Following State Machine

### Fundamental Loop

The core of `deadcat-core` is a state machine that tracks covenant UTXOs:

1. **Track**: Maintain a set of known contracts and their current UTXOs
2. **Spend**: When a transaction spends any tracked UTXO, process it
3. **Advance**: Determine the contract's new state from the transaction's outputs and witness data
4. **Repeat**: The new UTXOs become the tracked set

This is a UTXO-following model, not a transaction classifier. The distinction is important:

- **Transaction classifier** (rejected): "What kind of transaction is this?" — fragile, must recognize every possible tx shape, breaks on unknown combinations
- **UTXO-following state machine** (chosen): "My contract was at UTXO X. X got spent. Where is my contract now?" — robust, works for any valid covenant transition

### Why UTXO-Following Over Classification

The Simplicity covenant enforces that outputs follow a known pattern. If input UTXO X was a prediction market's UnresolvedCollateral slot, the covenant guarantees the spending transaction's outputs contain the next valid state. Core doesn't need to classify the transaction — it just needs to find its contract's new outputs.

This also handles unknown transaction combinations gracefully. If a future version combines an issuance with a pool adjustment in one transaction (something no current PSET builder produces), the UTXO-following model still works — each contract independently follows its own outpoints through the transaction. The classifier model would fail because it wouldn't recognize the combined shape.

### Processing a Transaction

When `process_transaction` is called:

1. Collect all input outpoints from the transaction
2. Check which tracked contracts own any of those outpoints
3. For each affected contract, decode the Simplicity witness to determine the transition type
4. Find the contract's new outputs by matching script pubkeys against expected covenant scripts (derived from params + new state)
5. Compute external output roles for non-covenant outputs
6. Durably persist the state updates
7. Return the full `TransitionResult`s

**Important**: A single transaction can affect multiple contracts. For example, a routed trade can spend LMSR pool reserves AND fill a limit order. Each contract advances independently based on its own outpoints — they don't need to know about each other.

### Output Matching via Script Pubkeys

Core identifies which outputs belong to which contract by matching `script_pubkey` values, not by assuming fixed output indices. This is robust across all transaction shapes:

- **Explicit covenant outputs** (collateral, reserves): script pubkey is the covenant address derived from contract params + state. Asset and value are readable.
- **Reissuance token outputs**: `Asset::Null` and `Value::Null` on-chain, but `script_pubkey` is set to the covenant address for the target slot. Core matches by script pubkey alone.
- **Blinded wallet outputs** (change, payouts): script pubkey is readable but asset/value may be confidential. Core identifies these as "not covenant" and reports them as `ExternalOutput` with `asset: None, value: None` when blinded.

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

- Contract parameters (from which core derives initial script pubkeys)
- The creation anchor (creation txid, initial output indices)

Core compiles the Simplicity contract from the parameters, derives the initial covenant address(es), and begins tracking the outpoints.

The caller is then responsible for scanning the chain from the creation point forward and feeding any relevant transactions to `process_transaction`. Core has no chain access — it processes whatever it's given.

### Catching Up New Contracts

When a contract is ingested that was created in the past, it needs to be "caught up" to the chain tip. Core doesn't manage this — it's the caller's responsibility:

1. Ingest the contract (core starts tracking initial outpoints)
2. Scan the chain for transactions touching the contract's scripts since creation
3. Feed those transactions to `process_transaction` in order

Core doesn't distinguish between "catch-up" and "tip" transactions. It processes whatever it receives. Existing fully-synced contracts can continue processing new tip transactions while a newly-ingested contract catches up — each contract tracks its own outpoints independently.

Whether a contract is "caught up" is determined by the caller, not core. The caller knows the chain tip and knows whether it has scanned all relevant scripts up to that point. Core has no concept of a global sync tip.

## Persistence: Store Trait

Core defines traits for state persistence. The implementor controls what to store and how.

### Required: ContractStore

```rust
pub trait ContractStore {
    // Reads — &self
    fn find_by_outpoints(&self, outpoints: &[OutPoint]) -> Result<Vec<ContractRef>>;
    fn current_state(&self, contract_id: &str) -> Result<Option<Contract>>;
    fn all_watched_outpoints(&self) -> Result<Vec<OutPoint>>;

    // Writes — &mut self
    fn apply_transitions(&mut self, transitions: &[StateUpdate]) -> Result<()>;
    fn track_contract(&mut self, contract: Contract) -> Result<String>;
    fn rollback_to_height(&mut self, height: u32) -> Result<()>;
    fn prune_finalized(&mut self, current_height: u32, finality_depth: u32) -> Result<()>;
}
```

Every consumer must implement this. Read methods take `&self`, write methods take `&mut self` — mirroring the engine's own borrow semantics. The engine calls read methods during interpretation (`&self` on the engine borrows the store as `&self`) and write methods during processing (`&mut self` on the engine borrows the store as `&mut self`).

`apply_transitions` must be durable when it returns — the engine depends on this for crash safety.

### Optional: ContractHistory

```rust
pub trait ContractHistory {
    fn transition_history(
        &self,
        contract_id: &str,
        since_height: Option<u32>,
        limit: Option<usize>,
    ) -> Result<Vec<StateUpdate>>;
}
```

Only implement if the consumer wants price charts, audit trails, etc. Core never depends on history for processing — it only needs current state. History returns `StateUpdate` (the persisted form), not `TransitionResult` (which includes ephemeral computed fields). To get full output classification for a historical transaction, the caller can call `interpret_transaction`.

### Implementor Controls Retention

The engine always passes full `StateUpdate` details to `apply_transitions`. The implementor decides what to keep:

- **Minimal (e.g., Aqua)**: Update current outpoints, discard old state. `transition_history` returns empty.
- **Full (e.g., Deadcat Live)**: Update current state AND append to history table. Supports price charts and audit trails.
- **Selective**: Keep LMSR pool history (for price charts) but discard order fill history (not needed).

This is an implementation detail — core doesn't need per-contract configuration flags.

## Separation of Concerns: Wallet vs Contract Layer

A key design principle: the contract layer and wallet layer have complementary, non-overlapping views of the same transaction.

| Output type    | Tracked by         | Example                          |
| -------------- | ------------------ | -------------------------------- |
| Covenant state | Contract engine    | Market collateral, pool reserves |
| Wallet balance | Wallet (LWK/GDK)  | L-BTC change, received tokens    |
| Fee            | Neither explicitly | Implicit from tx structure       |

The contract engine doesn't track wallet outputs. The wallet doesn't track covenant outputs. They correlate on txid when labeling is needed.

The `TransitionResult.external_outputs` bridges the gap — core identifies which outputs are NOT contract state, and labels their roles where possible, so the wallet can display "Received 10 YES tokens from issuance" without understanding covenant mechanics. For blinded external outputs, core provides the output index and script pubkey; the wallet uses its own blinding keys to determine asset and value.

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
) -> Result<Vec<UnblindedUtxo>>;
```

## Simplicity Contracts

Core contains the `.simf` Simplicity contract source code and the compiler integration. Given contract parameters and a network type (testnet/mainnet), core can:

- Compile the Simplicity contract
- Derive script pubkeys for any state
- Decode witness data from spending transactions

This is necessary for both PSET construction (building covenant outputs with correct scripts) and state advancement (matching output scripts to determine new state).

## Reorg Handling

Core maintains a processing log: for each processed transaction, the contract ID, old outpoints, new outpoints, and block height. Rolling back to height N means:

1. Find all transitions from blocks above N
2. Reverse them: restore old outpoints, remove new outpoints
3. Delete the processing records
4. Return the now-current outpoints (so the caller can re-scan)

The caller detects the reorg (their chain backend tells them), calls `rollback_to_height`, then feeds the new chain data.

### Finality-Based Pruning

On Liquid, transactions are considered absolutely irreversible after 2 confirmations. Processing log entries for finalized transactions can never be needed for reorg recovery and can be safely pruned:

```rust
fn prune_finalized(&mut self, current_height: u32, finality_depth: u32) -> Result<()>;
```

The caller periodically calls `prune_finalized` with the current chain tip and the network's finality depth (2 for Liquid). This keeps the processing log bounded without sacrificing correctness.

**Important**: Pruning the processing log (for reorg rollback) is independent from retaining transition history (for price charts, audit trails). The `ContractHistory` trait stores historical transitions permanently — `prune_finalized` only removes the rollback metadata that's no longer needed.

## Thread Safety

Write methods (`process_transaction`, `track_contract`, `rollback_to_height`, `prune_finalized`) take `&mut self`. Read methods (`interpret_transaction`, `identify_asset`, `watched_outpoints`) take `&self`.

Rust's borrow rules provide compile-time `RwLock` semantics: multiple concurrent readers OR one exclusive writer, enforced without runtime overhead. For single-threaded consumers this is invisible. For multi-threaded consumers who wrap the engine in `RwLock<ContractEngine>`, they get the expected semantics automatically.

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
use deadcat_core::{ContractEngine, Contract, ChainTransaction};

// 1. Initialize engine with a store implementation.
//    The store is exclusively owned by the engine from this point.
let mut engine = ContractEngine::new(aqua_deadcat_store);

// 2. Ingest a market (discovered via Nostr, import, etc.)
let contract_id = engine.track_contract(Contract::PredictionMarket {
    params, anchor, state: MarketState::Dormant, outpoints: initial_outpoints,
})?;

// 3. Catch up: scan chain for this contract's history
let scripts = engine.scripts_for_contract(&contract_id)?;
let historical_txs = aqua_chain.scan_history(&scripts, creation_height);
for tx in historical_txs {
    // process_transaction is idempotent and persists before returning
    engine.process_transaction(&tx)?;
}

// 4. Steady state: process new transactions as they arrive
aqua_chain.on_new_transaction(|tx| {
    engine.process_transaction(&tx)?;
});

// 5. Label wallet history (read-only, can be called anytime)
for wallet_tx in wallet_history {
    let interpretations = engine.interpret_transaction(&wallet_tx);
    for interp in &interpretations {
        for output in &interp.external_outputs {
            if output.index == my_utxo_index {
                render_label(&output.role);
            }
        }
    }
}

// 6. Identify assets in wallet balance
for (asset_id, amount) in wallet_balance {
    if let Some(info) = engine.identify_asset(&asset_id) {
        render_token_position(&info, amount);
    }
}

// 7. Build a transaction (core provides PSET, Aqua signs)
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

### Unified TransitionResult for Processing and Interpretation

**Chosen**: Both `process_transaction` and `interpret_transaction` return `Vec<TransitionResult>`, which includes state update data (outpoints, details) AND computed output classification (contract outputs, external outputs with roles).
**Rejected**: Separate `Transition` and `TxInterpretation` types.
**Why**: The output classification is computed as a byproduct of the advance logic — it adds negligible cost. Having one return type means one mental model for consumers. The engine internally converts to `StateUpdate` (without computed fields) before persisting, so the store trait's interface stays focused on contract state only. No risk of over-storage.

### Store Trait with Optional History

**Chosen**: Required `ContractStore` trait + optional `ContractHistory` trait.
**Rejected**: Single trait with history methods that return empty vecs.
**Why**: Separating the traits makes the contract clearer. A minimal consumer (Aqua) only implements `ContractStore`. A full consumer (Deadcat Live) implements both. Core's processing pipeline never calls history methods — they're purely for consumer-facing features like price charts.

### Persistence Owned by Caller, Not Core

**Chosen**: Core defines traits; caller implements persistence.
**Rejected**: Core owns a database or storage engine.
**Why**: Aqua already has its own database. Deadcat Live uses SQLite. A CLI tool might use flat files. Core shouldn't prescribe storage. The trait approach lets each consumer integrate deadcat state into their existing persistence layer.

### Process and Persist Atomically

**Chosen**: `process_transaction` computes transitions and durably persists them in one call, returning results only after the write succeeds.
**Rejected**: Two-step pattern where the engine returns results and the caller manually triggers persistence.
**Why**: Since the engine exclusively owns the store, there's no reason for the caller to inspect results before deciding to persist — the transitions are deterministic from the transaction. Splitting compute and persist would create a window where a crash could leave the engine's in-memory state ahead of the store. The single-call pattern eliminates this by design.

### Idempotent Transaction Processing

**Chosen**: `process_transaction` is idempotent — calling it twice with the same transaction is a no-op.
**Rejected**: Requiring the caller to deduplicate transactions before calling.
**Why**: During catch-up and crash recovery, the same transaction may be fed to the engine more than once. Idempotency means the caller doesn't need to track what's already been processed — they can safely replay blocks without double-counting.

### Interpretation as Read-Only Replay

**Chosen**: `interpret_transaction` uses same logic as `process_transaction` but read-only.
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

**Chosen**: Core assumes all transactions passed to `process_transaction` and `interpret_transaction` are consensus-valid (confirmed on-chain or mempool-accepted). Core does not re-verify transactions.
**Rejected**: Core validates Simplicity witnesses, confidential proofs, and reissuance token derivation internally.
**Why**: The caller's chain backend only provides consensus-valid data. Liquid consensus has already verified everything — Simplicity covenant witnesses satisfy the script, confidential range proofs are valid, reissuance tokens match their issuance entropy. Re-verifying would duplicate work the network already did. This assumption is what allows core to identify reissuance token outputs (which have `Asset::Null, Value::Null` on-chain) purely by script pubkey matching, and to classify blinded outputs as "not covenant" without needing to unblind them.

### No Thread Safety Constraints on Store Trait

**Chosen**: `ContractStore` has no `Send`/`Sync` bounds. Thread safety is determined by the implementor's choice.
**Rejected**: Requiring `Send + Sync` on the store trait.
**Why**: Single-threaded consumers (the common case) shouldn't pay for thread safety they don't need. If a consumer wraps the engine in `RwLock` for multi-threaded access, Rust automatically requires their store to be `Send` — the constraint propagates naturally without being forced on everyone.
