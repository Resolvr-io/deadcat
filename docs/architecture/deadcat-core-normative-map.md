# deadcat-core Normative Map

Status: Normative guide to the `deadcat-core` design documents.

This document is a map of the `deadcat-core` design docs. The design docs are
the map of the implementation.

This document does not specify `deadcat-core` directly. It specifies which
design documents are authoritative for each part of `deadcat-core`, and how to
resolve scope, status, and priority between them. If this document conflicts
with a referenced spec, fix the underlying specs rather than treating this file
as a parallel implementation spec.

## Source Priority

When documents disagree, use this order:

1. [`deadcat-core-design.md`](deadcat-core-design.md)
   is authoritative for public Rust API shape, engine behavior, store traits,
   contract state enums, transition interpretation, PSET builder placement,
   ingestion, chain sync, error semantics, implementation scope, and design
   decisions for `deadcat-core`.

2. [`market-contract-principles.md`](../contracts/market-contract-principles.md)
   is authoritative for covenant-level security principles shared by market
   contracts: solvency, RT burn requirements, sibling UTXO checks, oracle power,
   deterministic RT blinding, permissionlessness, and terminal-path
   completeness.

3. [`contract-specification.md`](../contracts/contract-specification.md)
   is authoritative for per-contract covenant behavior where it agrees with
   `deadcat-core-design.md` and `market-contract-principles.md`.

4. Focused protocol specs are authoritative for their named surfaces:
   - [`chain-only-recovery.md`](../protocol/chain-only-recovery.md)
   - [`deterministic-rt-blinding.md`](../protocol/deterministic-rt-blinding.md)
   - [`oracle-bip340-tagged-hash.md`](../protocol/oracle-bip340-tagged-hash.md)

5. Focused architecture specs are authoritative for their named mechanisms,
   except where explicitly superseded by `deadcat-core-design.md`:
   - [`transaction-composability-model.md`](transaction-composability-model.md)
   - [`trade-routing-algorithm.md`](trade-routing-algorithm.md)
   - [`enforcement-layers.md`](enforcement-layers.md)

6. Contract-specific focused specs are authoritative for their named contract
   or math surface, subject to the higher-priority docs above:
   - [`lmsr-pool-design.md`](../contracts/lmsr-pool/lmsr-pool-design.md)
   - [`lmsr-deterministic-table-spec.md`](../contracts/lmsr-pool/lmsr-deterministic-table-spec.md)
   - [`lmsr-pool-close-path.md`](../contracts/lmsr-pool/lmsr-pool-close-path.md)
   - [`multi-outcome-market-contract.md`](../contracts/multi-outcome/multi-outcome-market-contract.md)
   - [`market-dormant-terminal-paths.md`](../contracts/prediction-market/market-dormant-terminal-paths.md)

7. Historical, decision-record, refactor, and future-design docs explain why
   choices were made or preserve rejected paths. They are not implementation
   specs unless a normative doc links to a specific section and says it is
   authoritative for the current implementation.

## Topic Ownership

| Topic | Authoritative source |
| --- | --- |
| Public Rust API | [`deadcat-core-design.md`](deadcat-core-design.md) |
| Core type shapes | [`deadcat-core-design.md`](deadcat-core-design.md) |
| Engine methods and view types | [`deadcat-core-design.md`](deadcat-core-design.md) |
| Store trait and atomicity | [`deadcat-core-design.md`](deadcat-core-design.md) |
| Ingestion and tracking policy | [`deadcat-core-design.md`](deadcat-core-design.md), then [`chain-only-recovery.md`](../protocol/chain-only-recovery.md) |
| Chain sync and rollback | [`deadcat-core-design.md`](deadcat-core-design.md) |
| Output classification | [`deadcat-core-design.md`](deadcat-core-design.md) |
| Market state machines | [`deadcat-core-design.md`](deadcat-core-design.md), then [`contract-specification.md`](../contracts/contract-specification.md) |
| Market covenant principles | [`market-contract-principles.md`](../contracts/market-contract-principles.md) |
| Binary market covenant behavior | [`contract-specification.md`](../contracts/contract-specification.md), subject to [`market-contract-principles.md`](../contracts/market-contract-principles.md) |
| Multi-outcome market covenant behavior | [`multi-outcome-market-contract.md`](../contracts/multi-outcome/multi-outcome-market-contract.md), subject to [`deadcat-core-design.md`](deadcat-core-design.md) and [`market-contract-principles.md`](../contracts/market-contract-principles.md) |
| LMSR pool parameters and lifecycle | [`lmsr-pool-design.md`](../contracts/lmsr-pool/lmsr-pool-design.md) |
| LMSR deterministic table generation | [`lmsr-deterministic-table-spec.md`](../contracts/lmsr-pool/lmsr-deterministic-table-spec.md) |
| LMSR pool close path | [`lmsr-pool-close-path.md`](../contracts/lmsr-pool/lmsr-pool-close-path.md) |
| Maker order behavior | [`deadcat-core-design.md`](deadcat-core-design.md), then [`contract-specification.md`](../contracts/contract-specification.md) |
| Trade routing | [`trade-routing-algorithm.md`](trade-routing-algorithm.md), with public API from [`deadcat-core-design.md`](deadcat-core-design.md) |
| Multi-covenant transaction layout | [`transaction-composability-model.md`](transaction-composability-model.md) |
| OP_RETURN recovery | [`chain-only-recovery.md`](../protocol/chain-only-recovery.md) |
| Oracle attestation message | [`oracle-bip340-tagged-hash.md`](../protocol/oracle-bip340-tagged-hash.md) |
| RT blinding | [`deterministic-rt-blinding.md`](../protocol/deterministic-rt-blinding.md) |
| Burn script and enforcement layering | [`market-contract-principles.md`](../contracts/market-contract-principles.md), then [`enforcement-layers.md`](enforcement-layers.md) |
| Implementation phases | [`deadcat-core-implementation-plan.md`](deadcat-core-implementation-plan.md) |

## V1 Scope

V1 includes:

- Binary markets.
- Multi-outcome markets for N in `{3, 4}`.
- Market issuance, cancellation, oracle resolution, expiry, and redemption.
- Multi-outcome split/merge primitives.
- LMSR pools.
- Maker orders.
- Trade routing across pools and maker orders.
- Existing-pool market-assisted routes via `build_trade_pset -> PreBlindedPset`.
- Chain-only recovery using canonical OP_RETURN hints.
- Strict-canonical tracking policy.
- Store compliance requirements sufficient for atomic multi-contract state
  updates.

V1 excludes:

- Cross-outcome arb quote/build API.
- Cross-outcome arb aggregate transaction classification.
- LP-tokenized pools.
- Atomic market creation plus pool bootstrap.
- Exact-output trade routing.
- N greater than 4 unless explicitly added.
- Fixed-point Taylor LMSR runtime optimization.

## Conflict Handling

If an implementation agent finds a contradiction:

1. Do not infer a new protocol rule.
2. Prefer the highest-priority source listed above.
3. If the conflict is only in examples or historical rationale, follow the
   normative source and leave a note for docs cleanup.
4. Ask for human review if the conflict affects:
   - fund safety,
   - RT issuance or destruction,
   - deterministic blinding,
   - oracle authorization,
   - chain-only recovery,
   - PSET layout,
   - store atomicity,
   - public API shape.

## Known Cleanup Items

Resolve these before treating the docs as fully implementation-ready:

- Multi-outcome public params Rust representation for runtime `outcome_count`.
- Exact LMSR admin/close signature preimages and domain strings.
- Whether `half_payout_sats` is intentionally independent from the parent
  market denomination, or should be constrained by it.

## Document Status Labels

Related markdown files should start with one of:

- `Status: Normative`
- `Status: Normative for <specific topic>`
- `Status: Decision record, non-normative`
- `Status: Historical, non-normative`
- `Status: Future / v2, non-normative for v1`

Implementation agents should not implement from non-normative docs unless a
normative doc links to a specific section and says it is authoritative for the
current implementation.
