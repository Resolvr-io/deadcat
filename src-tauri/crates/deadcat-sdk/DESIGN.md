# deadcat-sdk Design Document

## What It Is

`deadcat-sdk` is a Rust crate that implements a binary prediction market protocol on the Liquid network. It compiles Simplicity smart contracts into covenants, manages an Electrum-backed wallet, publishes and discovers markets/orders/pools via Nostr relays, and routes trades through a combination of AMM pools and limit orders.

The crate exposes two main entry points:

- **`DeadcatSdk`** — low-level, synchronous, blocking. Owns the wallet, signs transactions, talks to Electrum. Not meant to be called directly by app code.
- **`DeadcatNode`** — high-level, async. Wraps the SDK behind `Arc<Mutex<Option<DeadcatSdk>>>`, bridges blocking I/O to tokio via `spawn_blocking`, integrates Nostr discovery and store persistence. This is the public API surface for the Tauri app layer.

Pure functions (AMM math, key derivation, Simplicity compilation) are importable directly from the crate for use in contexts that don't need the full node.

---

## Module Map

```
deadcat-sdk/src/
├── lib.rs                    # Crate root, re-exports
├── sdk.rs                    # DeadcatSdk: wallet, signing, transaction building
├── node.rs                   # DeadcatNode: async wrapper, discovery integration
├── chain.rs                  # ElectrumBackend: chain queries and broadcast
├── chain_watcher.rs          # Persistent Electrum subscription relay
├── error.rs                  # Error and NodeError types
├── network.rs                # Network enum (Liquid, LiquidTestnet, LiquidRegtest)
├── pset.rs                   # PSET construction helpers, UnblindedUtxo
├── taproot.rs                # Simplicity taproot output construction
├── assembly.rs               # Shared transaction assembly helpers
├── announcement.rs           # ContractAnnouncement, ContractMetadata
│
├── prediction_market/
│   ├── params.rs             # PredictionMarketParams, MarketId
│   ├── state.rs              # MarketState enum (4 states)
│   ├── contract.rs           # CompiledPredictionMarket
│   ├── assembly.rs           # Issuance/resolve/redeem/cancel PSET assembly
│   ├── witness.rs            # Simplicity witness satisfaction (7 paths)
│   ├── oracle.rs             # Oracle message construction
│   └── pset/                 # Per-operation PSET builders
│       ├── creation.rs
│       ├── initial_issuance.rs
│       ├── issuance.rs
│       ├── oracle_resolve.rs
│       ├── post_resolution_redemption.rs
│       ├── expiry_redemption.rs
│       └── cancellation.rs
│
├── amm_pool/
│   ├── params.rs             # AmmPoolParams, PoolId
│   ├── math.rs               # Swap computation, LP math, implied probability
│   ├── contract.rs           # CompiledAmmPool
│   ├── chain_walk.rs         # On-chain state reconstruction
│   ├── assembly.rs           # Witness attachment for pool covenants
│   ├── witness.rs            # Witness value construction (3 paths)
│   └── pset/                 # Per-operation PSET builders
│       ├── creation.rs
│       ├── swap.rs
│       ├── lp_deposit.rs
│       └── lp_withdraw.rs
│
├── maker_order/
│   ├── params.rs             # MakerOrderParams, OrderDirection, key derivation
│   ├── contract.rs           # CompiledMakerOrder
│   ├── witness.rs            # Fill and cancel witness builders
│   ├── taproot.rs            # Order-specific P2TR with maker pubkey
│   └── pset/
│       ├── create_order.rs
│       ├── fill_order.rs
│       └── cancel_order.rs
│
├── trade/
│   ├── types.rs              # TradeQuote, ExecutionPlan, RouteLeg
│   ├── router.rs             # Greedy routing: orders first, pool remainder
│   ├── pset.rs               # Combined trade PSET (pool + orders)
│   └── convert.rs            # Nostr discovery → SDK type conversions
│
└── discovery/
    ├── config.rs             # DiscoveryConfig (relays, timeout)
    ├── events.rs             # DiscoveryEvent enum
    ├── service.rs            # DiscoveryService (Nostr client, pub/sub)
    ├── store_trait.rs        # DiscoveryStore trait (persistence abstraction)
    ├── market.rs             # Market announcement events
    ├── pool.rs               # Pool announcement events
    └── attestation.rs        # Oracle attestation events
```

### Companion Crate

`deadcat-store` (under `deadcat-sdk/deadcat-store/`) implements `DiscoveryStore` with Diesel + SQLite. Tables: `markets`, `maker_orders`, `amm_pools`, `pool_state_snapshots`, `utxos`, `sync_state`. The store crate depends on `deadcat-sdk` for trait definitions; the SDK never depends on the store, breaking the circular dependency.

---

## Prediction Markets

### State Machine

A market has four states, each corresponding to a distinct Taproot address derived from the same Simplicity CMR but different state values encoded in tapdata:

```
Dormant ──(initial issuance)──▶ Unresolved
    ▲                               │
    │                               ├──(subsequent issuance)──▶ Unresolved
 (full cancel)                      │
    │                               ├──(partial cancel)──▶ Unresolved
    │                               │
    │                               ├──(oracle resolve)──▶ ResolvedYes
    │                               │                          │
    │                               ├──(oracle resolve)──▶ ResolvedNo
    │                               │                          │
    │                               └──(expiry redeem)──▶ Unresolved
    │                                                          │
    └──────────────────────────────────────(post-res redeem)───┘
```

State transitions move covenant UTXOs from one address to another. The Simplicity contract validates every transition.

### How a Market Is Created

1. The SDK selects two wallet UTXOs as "defining outpoints" — one for YES, one for NO. These deterministically derive the four asset IDs (YES token, NO token, YES reissuance token, NO reissuance token) via Elements issuance.
2. A creation transaction is broadcast that deposits the two reissuance tokens into the Dormant covenant address.
3. A `ContractAnnouncement` (containing `PredictionMarketParams` + human-readable `ContractMetadata`) is published to Nostr relays.

### Simplicity Contract

The prediction market contract (`prediction_market.simf`) is a 7-path dispatch tree:

| Path | Transition | Key Enforcement |
|------|-----------|-----------------|
| 1 | Initial Issuance (Dormant → Unresolved) | Reissuance tokens verified via Pedersen commitments; collateral = pairs × 2 × cpt |
| 2 | Subsequent Issuance (Unresolved → Unresolved) | Old collateral accumulates; new issuance amounts equal |
| 3 | Oracle Resolve (Unresolved → Resolved) | BIP-340 Schnorr signature verified against oracle pubkey over `SHA256(market_id \|\| outcome_byte)` |
| 4 | Post-Resolution Redemption | Winning tokens burned; payout = tokens × 2 × cpt |
| 5 | Expiry Redemption | Block height ≥ expiry enforced via `check_lock_height`; payout = tokens × 1 × cpt |
| 6 | Cancellation | Equal YES + NO pairs burned; refund = pairs × 2 × cpt |
| 7 | Secondary Input | Co-membership check: this input's script hash == input 0's script hash |

Compile-time parameters (oracle key, asset IDs, collateral rate, expiry) are injected into a SimplicityHL template program. The resulting CMR (Commitment Merkle Root) is unique per market.

### Taproot Encoding

Each market state maps to a distinct P2TR address:

```
leaf_hash  = TaggedHash("TapLeaf/elements", 0xbe || compact_size(32) || CMR)
data_leaf  = TaggedHash("TapData", state_u64_be)
branch     = TaggedHash("TapBranch/elements", sort(leaf_hash, data_leaf))
tweak      = TaggedHash("TapTweak/elements", NUMS_KEY || branch)
output_key = NUMS_KEY + tweak × G
script     = OP_1 <output_key>
```

The NUMS key (Nothing Up My Sleeve) is provably unspendable, so the key-path spend is disabled — only Simplicity script-path spends work.

### Reissuance Token Mechanics

YES and NO reissuance tokens cycle through every transaction as blinded (confidential) outputs. The contract verifies their identity via Pedersen commitment math using blinding factors provided in the witness. This prevents unauthorized token minting while keeping token amounts private.

### Witness Pruning

Simplicity requires all visible nodes to be executed. Unused branches are replaced with HIDDEN nodes (containing only the pruned subtree's CMR hash). Budget padding witnesses inflate the witness to meet Simplicity's cost model requirements.

---

## AMM Pools

### What a Pool Holds

An AMM pool is a set of 4 covenant UTXOs at the same Taproot address:

| Output Index | Contents |
|-------------|----------|
| 0 | YES token reserve |
| 1 | NO token reserve |
| 2 | L-BTC reserve |
| 3 | LP reissuance token |

The covenant address is derived from the pool's CMR + `issued_lp` (total LP tokens outstanding). Swaps don't change `issued_lp`, so the address stays the same. Deposits and withdrawals change `issued_lp`, moving the pool to a new address.

### Swap Math

Constant-product AMM with fee:

```
effective_in = delta_in × (10000 - fee_bps) / 10000
delta_out    = floor(r_buy × effective_in / (r_sell + effective_in))
```

Three swap pairs are supported: YES↔NO, YES↔LBTC, NO↔LBTC. Each pair operates on two of the three reserves; the third is untouched.

### LP Deposit Math

LP minting uses a cubic invariant to ensure fair pricing:

```
(issued_lp + lp_mint)³ × old_product ≤ issued_lp³ × new_product
```

where `product = r_yes × r_no × r_lbtc`. This is verified using 512-bit wide arithmetic (custom `U256` type) to avoid overflow for large reserves.

### Implied Probability

Market price is derived purely from reserve ratios:

```
yes_probability_bps = round(r_no / (r_yes + r_no) × 10000)
```

Higher NO reserve → higher YES probability (the market has priced YES up).

### Simplicity Contract

The pool contract (`amm_pool.simf`) is a 3-path dispatch tree:

| Path | Operation | Key Enforcement |
|------|-----------|-----------------|
| Swap | Reserve pair changes, LP unchanged | Fee deducted from input; output ≤ constant-product bound |
| LP Deposit/Withdraw | All reserves may change, LP changes | Cubic invariant verified; new address derived from new `issued_lp` |
| Secondary | Co-membership for inputs 1-3 | Script hash matches input 0 |

### Chain Walk

`walk_pool_chain()` reconstructs pool state from on-chain transaction history:

1. Start from the pool creation txid (or a resume point).
2. Compute the current covenant script pubkey from `contract.script_pubkey(issued_lp)`.
3. Fetch script history from Electrum.
4. Find the spending transaction that consumes outputs 0-3.
5. Parse new reserves from the spending tx's outputs 0-2.
6. Determine new `issued_lp` by checking for LP reissuance (deposit) or LP burn output (withdraw).
7. Record a `PoolStateSnapshot` and continue from the new state.

This enables incremental sync — the store tracks the latest snapshot's txid and issued_lp, and subsequent walks resume from there.

---

## Maker Orders (Limit Orders)

### How They Work

A maker locks tokens (or L-BTC) into a Simplicity covenant. The covenant enforces fill constraints (price, minimum fill, minimum remainder). Anyone can fill the order by constructing a valid transaction.

### Key Derivation

Each order has a deterministic identity:

```
order_uid = SHA256("deadcat/order_uid" || maker_pubkey || nonce || params...)
tweak     = SHA256("deadcat/order_tweak" || order_uid)
P_order   = P_maker + tweak × G
```

`P_order` is used as the maker's receive address (P2TR). The `maker_receive_spk_hash` (SHA256 of this script) is a compile-time parameter in the covenant, ensuring fills pay the maker correctly.

### Covenant Paths

| Path | Operation |
|------|-----------|
| Fill (Left) | Taker fills order; covenant checks price, minimums, maker receive output |
| Cancel (Right) | Maker cancels via key-path spend (signature over outpoint) |

Unlike market and pool covenants that use NUMS as the internal key, maker orders use the **maker's real pubkey** as the internal key. This means the cancel path is a standard key-path spend — no Simplicity execution needed.

### Order Direction

- **SellBase**: Maker holds outcome tokens in covenant, wants L-BTC. Taker sends L-BTC, receives tokens.
- **SellQuote**: Maker holds L-BTC in covenant, wants outcome tokens. Taker sends tokens, receives L-BTC.

---

## Trade Routing

When a user wants to buy or sell tokens, the router combines limit orders and AMM pool liquidity:

1. **Fetch** all pools and orders for the market from Nostr.
2. **Scan** each order's covenant UTXO and the pool's 4 UTXOs on-chain.
3. **Filter** orders that beat the AMM spot price.
4. **Sort** eligible orders by price (cheapest first for buys, highest payout first for sells).
5. **Fill greedily**: consume orders in price order, respecting `min_fill_lots` and `min_remainder_lots`. Only the last order in the sequence may be partially filled.
6. **Route remainder** through the AMM pool via `compute_swap_exact_input()`.
7. **Build a single combined PSET** containing pool covenant inputs (indices 0-3), order covenant inputs, taker funding inputs, and fee input. Output layout must match exactly: pool reserves at 0-3, maker receive outputs at corresponding indices, then taker receive/change/fee.

The combined PSET is blinded (taker outputs only), witnesses attached (Simplicity for pool + orders), signed, and broadcast as a single atomic transaction.

---

## Nostr Discovery

### Event Format

All events use NIP-78 (kind 30078, application-specific data). Content is JSON; routing is via hashtag (`t`) tags:

| Tag | Event Type | Content |
|-----|-----------|---------|
| `deadcat-contract` | Market announcement | `ContractAnnouncement` (params + metadata) |
| `deadcat-order` | Limit order | `OrderAnnouncement` (params + maker info) |
| `deadcat-pool` | AMM pool | `PoolAnnouncement` (params + reserves + issued_lp) |
| `deadcat-attestation` | Oracle resolution | `AttestationContent` (market_id + outcome + signature) |

All four event types use NIP-33 parameterized replaceable events (kind 30078 falls in the 30000-39999 range), keyed by pubkey + `d` tag. For pools, the `d` tag is the pool ID, so each swap/deposit/withdraw publishes a replacement with updated reserves. For attestations, the `d` tag is `"{market_id}:attestation"`, so re-attestation replaces the previous verdict. Markets and orders are keyed by market ID and order UID respectively.

### DiscoveryService

`DiscoveryService<S>` manages a Nostr client with background subscription:

- **`start()`** spawns a tokio task that subscribes to all 4 event filters and listens for relay notifications.
- Incoming events are parsed, persisted to the `DiscoveryStore` (if present), and broadcast via `tokio::broadcast` channel.
- **One-shot fetches** (`fetch_markets`, `fetch_orders`, `fetch_pools`, `fetch_attestation`) are available for on-demand queries.
- **Publishing** (`announce_market`, `announce_order`, `announce_pool`, `publish_attestation`) signs and sends events.
- **Reconciliation** (`reconcile()`) re-sends all stored Nostr event JSON to relays, ensuring events that were silently dropped remain available. NIP-33 replaceable events make this idempotent. A standalone `send_reconciliation_events()` function supports the two-phase lock-drop pattern used by the app layer.

The oracle's signing key is the same as the Nostr identity key (x-only Schnorr via BIP-340).

### Discovery → App Flow

```
Nostr Relay
  → DiscoveryService subscription loop
    → parse event → persist to DiscoveryStore → broadcast DiscoveryEvent
      → App-layer event loop receives DiscoveryEvent
        → Emit to frontend (Tauri event)
        → Subscribe contract scripts to ChainWatcher
```

---

## Chain Watcher

The `ChainWatcher` maintains a persistent Electrum TCP connection on a dedicated OS thread (because `electrum_client::Client` is `!Send`) and pushes typed events to the Node layer via `tokio::sync::mpsc`.

### Subscription Model

- **Markets**: 4 scripts per market (one per state). Subscribed at discovery time.
- **Orders**: 1 script per order. Subscribed at discovery time.
- **Pools**: 1 script per pool, derived from current `issued_lp`. Re-subscribed when `issued_lp` changes (deposit/withdraw moves the pool address). The spending transaction appears in the old address's history, so the notification always arrives before the pool "moves."

### Event Types

| Event | Trigger | Handler |
|-------|---------|---------|
| `NewBlock` | New block header | Sync wallet |
| `MarketActivity` | Script notification on market SPK | Re-scan market UTXOs (TODO: currently syncs wallet) |
| `OrderActivity` | Script notification on order SPK | Re-scan order (TODO) |
| `PoolActivity` | Script notification on pool SPK | Chain walk → persist snapshots → re-subscribe if `issued_lp` changed |
| `ConnectionLost` | Electrum disconnect | Log warning |
| `Reconnected` | Successful reconnect | Log, re-subscribe all scripts |

### Lock-Drop Pattern

The event processing loop acquires the node lock only to call `prepare_chain_event()`, which clones Arc handles and returns a `'static` boxed future. The lock is dropped before the future is awaited, preventing Tauri command starvation:

```rust
while let Some(event) = event_rx.recv().await {
    let work = {
        let guard = node_state.node.lock().await;
        let Some(node) = guard.as_ref() else { continue };
        node.prepare_chain_event(event, &watcher_handle)
    }; // guard dropped
    work.await;
}
```

---

## Wallet and Chain Interaction

### ElectrumBackend

`ElectrumBackend` implements a `ChainBackend` trait with four operations:
- `scan_script_utxos(script)` — find unspent outputs at a script address
- `fetch_transaction(txid)` — get a full transaction by ID
- `broadcast(tx)` — submit a transaction to the network
- `get_script_history(script)` — get all txids that touched a script (with confirmation heights)

Connections are cached in thread-local storage (`RAW_CLIENT` for `electrum_client::Client`, `LWK_CLIENT` for `lwk_wollet::ElectrumClient`). Both clients are `!Send`, so they can't live inside the `Send` `ElectrumBackend` struct — thread-locals sidestep this. All SDK calls run on Tokio's blocking pool via `spawn_blocking`, which reuses OS threads, so the cached client persists across calls. Cache entries are keyed by URL to handle network switches, and cleared on error to force reconnect.

### Wallet Sync

`DeadcatSdk::sync()` performs a full Electrum scan via LWK's `full_scan_with_electrum_client`. The wallet tracks confidential UTXOs, balances per asset, and transaction history. After every SDK operation, a `WalletSnapshot` (balance + utxos + transactions) is captured and broadcast via a `tokio::sync::watch` channel, giving readers lock-free access to the latest state.

### UTXO Selection

The SDK uses a min-by-value selection strategy: for a given asset and required amount, it picks the smallest UTXO that satisfies the requirement. Exclusion lists prevent double-spending across multi-input transactions.

### Transaction Flow

Every on-chain operation follows the same pipeline:

1. **Scan** covenant UTXOs and/or select wallet UTXOs
2. **Build PSET** (Partially Signed Elements Transaction) with correct input/output layout
3. **Blind** selected outputs using LWK's `blind_last()` — covenant outputs stay explicit, wallet outputs get confidential
4. **Recover blinding factors** from blinded outputs (for Simplicity witnesses)
5. **Attach Simplicity witnesses** — satisfy the contract with chosen path + blinding factors, prune unused branches, serialize into witness stack
6. **Sign** wallet inputs via SLIP-77 signer
7. **Broadcast** via Electrum and sync wallet until the tx appears

---

## Store (deadcat-store)

SQLite database via Diesel ORM with 6 tables:

| Table | Purpose |
|-------|---------|
| `markets` | Market params + metadata + 4 cached SPKs + state + Nostr event info |
| `maker_orders` | Order params + maker info + covenant SPK + status |
| `amm_pools` | Pool params + issued_lp + covenant SPK + market association |
| `pool_state_snapshots` | Per-pool reserve history (r_yes, r_no, r_lbtc, issued_lp, block_height) |
| `utxos` | Tracked covenant UTXOs with spend status |
| `sync_state` | Last synced block height/hash |

The `DiscoveryStore` trait (defined in the SDK) abstracts persistence. The store crate implements it. `NoopStore` and `TestStore` provide no-op and in-memory implementations for tests.

Cached SPKs in the `markets` and `amm_pools` tables enable the chain watcher to bootstrap subscriptions without recompiling Simplicity contracts.

---

## App Layer Integration

The Tauri app (`src-tauri/src/`) wires everything together:

### State

- **`NodeState`**: holds `tokio::sync::Mutex<Option<DeadcatNode<DeadcatStore>>>` + chain watcher handle + event loop join handle + reconciliation task handle
- **`AppStateManager`**: holds `std::Mutex`-wrapped app config (network, wallet state, persister, store). Separate mutex type from NodeState to avoid holding both locks simultaneously.

### Wallet Lifecycle

1. **`unlock_wallet`**: decrypt mnemonic → `node.unlock_wallet()` → spawn chain watcher → bootstrap subscriptions from store → spawn event processing loop → store handles for later shutdown
2. **`lock_wallet`**: shutdown watcher → `node.lock_wallet()` → clear mnemonic cache
3. **`delete_wallet`**: shutdown watcher → lock wallet → delete persister

### Discovery Event Forwarding

When Nostr discovers a new market or pool, the app-layer event loop:
1. Emits the event to the frontend via Tauri events
2. Compiles the Simplicity contract (outside any lock)
3. Subscribes the contract's scripts to the chain watcher

---

## Intentional Design Trade-offs

**Node lock held for entire SDK closure.** `with_sdk` holds the `Mutex<Option<DeadcatSdk>>` for the duration of every blocking operation (which may include multiple Electrum round-trips). This serializes all SDK calls. The alternative — fine-grained locking — would risk inconsistent wallet state between steps of a multi-step operation.

**Nostr operations are non-atomic with on-chain operations.** `create_market` broadcasts the on-chain transaction first, then publishes to Nostr. If Nostr fails, the on-chain state has already changed. The reconciliation task (runs on startup + every 30 minutes) mitigates this by re-sending all stored events to relays, but there is still a window where the market exists on-chain but isn't discoverable.

**Pool announcements carry stale outpoints.** `PoolAnnouncement.outpoints` is intentionally left empty on updates. The authoritative pool state comes from chain walking, not from Nostr. The announcement primarily serves as a discovery beacon; reserves in the announcement are best-effort.

**`issued_lp` encoded in tapdata, not in the script.** This means the covenant address changes when LP tokens are minted or burned (deposit/withdraw), but stays the same across swaps. This is the right trade-off: swaps are frequent and shouldn't change the address; LP operations are infrequent and need the address change to track the new LP supply.

**Greedy order routing without optimization.** The router fills the cheapest orders first, then routes the remainder to the pool. It doesn't solve for the globally optimal split between orders and pool. This is simpler, predictable, and good enough — the AMM acts as a price floor/ceiling, and orders that beat it are strictly better.

**Single oracle key = Nostr identity key.** The oracle signs attestations with the same key used for Nostr event publishing. This simplifies key management but means the oracle's Nostr identity is its signing identity. FROST aggregate keys are supported for threshold signing.

**512-bit wide arithmetic for LP invariant.** The cubic invariant `(lp + mint)^3 * old_product <= lp^3 * new_product` can overflow u128 for large reserves. Rather than using a bignum library, the SDK implements custom U256 and 512-bit arithmetic. This avoids a dependency and keeps the code self-contained, at the cost of ~100 lines of manual arithmetic.

**Store caches SPKs to avoid recompilation.** Simplicity compilation is CPU-intensive. The store caches all 4 market SPKs and the pool covenant SPK so that bootstrap (subscribing scripts to the chain watcher) doesn't require recompiling contracts. The trade-off is data duplication — SPKs could be derived from params — but the performance win is significant for wallets with many tracked contracts.
