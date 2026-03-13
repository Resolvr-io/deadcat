# deadcat-sdk Design Document

## What It Is

`deadcat-sdk` is the Liquid prediction-market SDK for Deadcat.

It provides two layers:

- `DeadcatSdk`: synchronous wallet + covenant transaction builder/executor.
- `DeadcatNode`: async orchestration layer that combines SDK execution, Nostr discovery, and store persistence.

The runtime architecture is LMSR-first: trade routing is composed of maker order liquidity plus canonical LMSR pool state scanned from on-chain reserve lineage.

## Runtime Modules

```
deadcat-sdk/src/
├── lib.rs
├── sdk.rs
├── node.rs
├── chain.rs
├── error.rs
├── network.rs
├── pset.rs
├── taproot.rs
├── assembly.rs
├── announcement.rs
├── pool.rs
│
├── prediction_market/
├── lmsr_pool/
├── maker_order/
├── trade/
└── discovery/
```

## Discovery and Identity

LMSR pool announcements are NIP-33 replaceable events keyed by canonical `lmsr_pool_id` (`d` tag).

The SDK validates:

- announcement version and required LMSR fields,
- canonical reserve anchor outpoints,
- runtime-network tag match,
- canonical derived `lmsr_pool_id` from network + params + covenant CMR + anchors.

Non-canonical or mismatched payloads are rejected (fail-closed).

## Canonical LMSR State Tracking

The pool scanner follows canonical reserve bundle lineage from creation anchors:

1. Start from the three initial reserve outpoints.
2. Find transition tx spending the full bundle.
3. Decode primary witness payload (`SCAN_PAYLOAD`) from canonical YES input.
4. Resolve `OUT_BASE`, validate output window, assets, scripts, and next state index.
5. Advance until no spender exists.

Ambiguity, malformed witness data, partial bundle spends, or unsupported schema versions are hard errors.

## Trade Routing and Execution

`DeadcatNode::quote_trade`:

1. fetches discovery pools/orders,
2. validates and parses canonical LMSR payload,
3. scans canonical live LMSR reserves,
4. routes exact-input trade across maker orders + LMSR remainder,
5. returns route legs and quote totals.

`DeadcatNode::execute_trade` executes only from quote-derived plans.

`TradeAmount::ExactOutput` is intentionally unsupported.

## Store Model

`deadcat-store` implements `DiscoveryStore` and persists:

- market metadata,
- maker orders,
- LMSR immutable pool identity/params,
- LMSR mutable canonical state (`s_index`, reserve outpoints, reserve balances, transition tx provenance).

Announcement snapshots are not treated as canonical live state.

## API Surface

Public API remains centered on:

- `DeadcatNode` high-level methods (`quote_trade`, `execute_trade`, discovery fetch/publish),
- prediction market + maker order + LMSR typed models,
- discovery DTOs and persistence trait contracts.

All active liquidity pool behavior is LMSR-only.
