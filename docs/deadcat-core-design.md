# deadcat-core Design Document

## Purpose

`deadcat-core` is a pure computation library for interacting with Deadcat prediction market covenants on Liquid/Elements. It enables any wallet or application to create, track, interpret, and transact with prediction markets, LMSR pools, and limit orders — without prescribing how chain data is fetched, how state is persisted, or how keys are managed.

The primary motivating use case: integrating Deadcat functionality into existing wallets like Aqua, which already have their own wallet backend, chain connection, signer, and state management. These wallets need the covenant logic without the opinionated runtime that Deadcat Live uses.

## Architecture Overview

### Layer Separation

```
deadcat-core     Pure computation. No IO, no wallet, no chain, no Nostr.
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

| Capability | deadcat-core | deadcat-sdk | deadcat-node |
|---|---|---|---|
| Simplicity contract compilation | Yes | | |
| PSET construction | Yes | | |
| LMSR math (quotes, spot price, tables) | Yes | | |
| Contract state machine | Yes | | |
| Transaction interpretation | Yes | | |
| Asset identification | Yes | | |
| Coin selection algorithm | Yes | | |
| Wallet (UTXO source, signer) | | Yes (LWK) | |
| Chain backend (scan, broadcast) | | Yes (Electrum) | |
| Fee estimation | | Yes | |
| UTXO selection + preparation | | Yes | |
| Nostr discovery | | | Yes |
| State persistence (SQLite) | | | Yes |
| Background sync | | | Yes |
| Boltz swap integration | | | Yes |
| Price history tracking | | | Yes |

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

```rust
pub struct ChainTransaction {
    pub tx: elements::Transaction,
    pub block_height: u32,
    pub tx_index: u32,
}
```

When `process_transaction` is called:

1. Collect all input outpoints from the transaction
2. Check which tracked contracts own any of those outpoints
3. For each affected contract, decode the Simplicity witness to determine the transition type
4. Find the contract's new outputs by matching against expected script pubkeys (derived from params + new state)
5. Return the transitions with old outpoints, new outpoints, and decoded details

**Important**: A single transaction can affect multiple contracts. For example, a routed trade can spend LMSR pool reserves AND fill a limit order. Each contract advances independently based on its own outpoints — they don't need to know about each other.

### Transaction Ordering

Liquid transactions within a block have a strict serial order. Even if a contract's UTXO is spent and the resulting UTXO is re-spent within the same block, the two transactions have a defined order.

Core requires the caller to feed transactions in chain order. As long as this guarantee holds, core processes one transaction at a time sequentially — no concurrent writes, no locking needed. The "atomic write" for a single transaction is simply "write all state updates from this transaction before processing the next one."

## Contract Types

Core tracks three types of covenants:

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

### State Advancement via Witness Decoding

Rather than guessing which outputs belong to a contract by checking scripts, core decodes the Simplicity witness data from the spending transaction's inputs. The witness encodes exactly what transition happened:

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

### Required: Current State

```rust
pub trait ContractStore {
    fn find_by_outpoints(&self, outpoints: &[OutPoint]) -> Result<Vec<ContractRef>>;
    fn current_state(&self, contract_id: &str) -> Result<Option<Contract>>;
    fn apply_transitions(&mut self, transitions: &[Transition]) -> Result<()>;
    fn all_watched_outpoints(&self) -> Result<Vec<OutPoint>>;
    fn track_contract(&mut self, contract: Contract) -> Result<String>;
    fn rollback_to_height(&mut self, height: u32) -> Result<()>;
}
```

Every consumer must implement this. It stores the current outpoints and state for each contract.

### Optional: History

```rust
pub trait ContractHistory {
    fn transition_history(
        &self,
        contract_id: &str,
        since_height: Option<u32>,
        limit: Option<usize>,
    ) -> Result<Vec<Transition>>;
}
```

Only implement if the consumer wants price charts, audit trails, etc. Core never depends on history for processing — it only needs current state.

### Implementor Controls Retention

Core always calls `apply_transitions` with full transition details. The implementor decides what to keep:

- **Minimal (e.g., Aqua)**: Update current outpoints, discard old state. `transition_history` returns empty.
- **Full (e.g., Deadcat Live)**: Update current state AND append to history table. Supports price charts and audit trails.
- **Selective**: Keep LMSR pool history (for price charts) but discard order fill history (not needed).

This is an implementation detail — core doesn't need per-contract configuration flags.

## Transaction Interpretation

Core provides two read-only capabilities that don't modify state:

### Asset Identification

```rust
impl ContractEngine {
    pub fn identify_asset(&self, asset_id: &AssetId) -> Option<AssetInfo>;
}

pub enum AssetInfo {
    YesToken { market_id: String, params: PredictionMarketParams },
    NoToken { market_id: String, params: PredictionMarketParams },
    YesReissuanceToken { market_id: String },
    NoReissuanceToken { market_id: String },
}
```

Given an asset ID, core checks against all tracked markets' token asset IDs. Useful for labeling wallet balances: "this asset is the YES token for 'Will Bitcoin hit $150k?'"

### Transaction Interpretation

```rust
impl ContractEngine {
    pub fn interpret_transaction(&self, tx: &Transaction) -> Vec<TxInterpretation>;
}

pub struct TxInterpretation {
    pub contract_id: String,
    pub operation: Operation,
    pub contract_outputs: Vec<u32>,
    pub external_outputs: Vec<ExternalOutput>,
}

pub struct ExternalOutput {
    pub index: u32,
    pub asset: AssetId,
    pub value: u64,
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

This uses the same witness decoding and output matching logic as `process_transaction` but doesn't modify state. It identifies:

- Which outputs belong to contract state (covenant outputs)
- Which outputs went to external destinations (wallet, other parties)
- The role of each external output (issued tokens, payout, change, etc.)

This enables precise wallet transaction labeling. Instead of tracking txids in frontend state (fragile, doesn't persist), the wallet asks core to interpret any transaction at any time.

**`interpret_transaction` vs `process_transaction`**: They use the same underlying logic. `process_transaction` mutates state and is called once per transaction in chain order. `interpret_transaction` is read-only and can be called anytime (e.g., when rendering wallet history on startup, re-interpreting old transactions).

## Separation of Concerns: Wallet vs Contract Layer

A key design principle: the contract layer and wallet layer have complementary, non-overlapping views of the same transaction.

| Output type | Tracked by | Example |
|---|---|---|
| Covenant state | Contract engine | Market collateral, pool reserves |
| Wallet balance | Wallet (LWK/GDK) | L-BTC change, received tokens |
| Fee | Neither explicitly | Implicit from tx structure |

The contract engine doesn't track wallet outputs. The wallet doesn't track covenant outputs. They correlate on txid when labeling is needed.

The `TxInterpretation.external_outputs` bridges the gap — core identifies which outputs are NOT contract state, and labels their roles, so the wallet can display "Received 10 YES tokens from issuance" without understanding covenant mechanics.

## PSET Construction

Core provides pure PSET builder functions. These take explicit inputs (UTXOs, params, amounts) and return unsigned PSETs. No wallet access, no chain queries.

The existing PSET builders already follow this pattern:

```rust
pub fn build_initial_issuance_pset(
    contract: &CompiledPredictionMarket,
    params: &InitialIssuanceParams,  // contains explicit UnblindedUtxo inputs
) -> Result<PartiallySignedTransaction>;
```

The caller (SDK or wallet) is responsible for selecting UTXOs, constructing the params, and signing the resulting PSET. Core provides a standalone coin selection algorithm to help:

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

The store trait includes:

```rust
fn rollback_to_height(&mut self, height: u32) -> Result<()>;
```

The caller detects the reorg (their chain backend tells them), calls rollback, then feeds the new chain data.

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

// 1. Initialize engine with a store implementation
let mut engine = ContractEngine::new(aqua_deadcat_store);

// 2. Ingest a market (discovered via Nostr, import, etc.)
engine.track_contract(Contract::PredictionMarket {
    params, anchor, state: MarketState::Dormant, outpoints: initial_outpoints,
})?;

// 3. Catch up: scan chain for this contract's history
let scripts = engine.scripts_for_contract(&contract_id);
let historical_txs = aqua_chain.scan_history(&scripts, creation_height);
for tx in historical_txs {
    let transitions = engine.process_transaction(&tx)?;
    store.apply_transitions(&transitions)?;
}

// 4. Steady state: process new transactions as they arrive
aqua_chain.on_new_transaction(|tx| {
    let transitions = engine.process_transaction(&tx)?;
    store.apply_transitions(&transitions)?;
});

// 5. Label wallet history
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
**Why**: Different consumers have fundamentally different IO capabilities. LWK uses Electrum. GDK uses its own backend. Esplora-based wallets use HTTP. Core shouldn't prescribe the IO layer. The caller converts their chain data into core's input format (`ChainTransaction`, `ObservedSpend`) and feeds it.

### No Global Sync Tip
**Chosen**: Each contract tracks independently; caller determines sync status.
**Rejected**: Core maintains a global sync tip that gates processing.
**Why**: A global tip creates unnecessary coordination. When a new contract is ingested, existing synced contracts would be blocked until the new one catches up. Without a global tip, synced contracts continue processing tip transactions while new contracts catch up independently.

### Store Trait with Optional History
**Chosen**: Required `ContractStore` trait + optional `ContractHistory` trait.
**Rejected**: Single trait with history methods that return empty vecs.
**Why**: Separating the traits makes the contract clearer. A minimal consumer (Aqua) only implements `ContractStore`. A full consumer (Deadcat Live) implements both. Core's processing pipeline never calls history methods — they're purely for consumer-facing features like price charts.

### Persistence Owned by Caller, Not Core
**Chosen**: Core defines traits; caller implements persistence.
**Rejected**: Core owns a database or storage engine.
**Why**: Aqua already has its own database. Deadcat Live uses SQLite. A CLI tool might use flat files. Core shouldn't prescribe storage. The trait approach lets each consumer integrate deadcat state into their existing persistence layer.

### Interpretation as Read-Only Replay
**Chosen**: `interpret_transaction` uses same logic as `process_transaction` but read-only.
**Rejected**: Separate classification system for interpretation.
**Why**: The same witness decoding and output matching logic serves both purposes. Having two systems would mean two places to update when covenant logic changes. The only difference is mutability — processing writes new state, interpretation just reads.

### Advance Logic Uses Witness Decoding
**Chosen**: Decode Simplicity witness data to determine transition type.
**Rejected**: Pattern-match transaction structure (input/output counts, script matching).
**Why**: Witness decoding is deterministic and precise. The Simplicity witness encodes exactly what happened (old state, new state, trade parameters). Pattern matching is fragile — it requires recognizing every valid transaction layout and breaks on novel combinations. Witness decoding works for any valid covenant spend regardless of the surrounding transaction structure.
