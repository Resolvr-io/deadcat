# Prediction-Market Store Redesign

## Goal

Make prediction-market discovery safe before chain validation, while keeping the
canonical on-chain market model simple.

The old single-table design mixed:

- off-chain discovery metadata
- canonical market identity
- sync-derived live state

That was unsafe once proof-carrying anchors were introduced, because conflicting
announcements could overwrite anchor data for the same `market_id` before any
chain-backed validation happened.

## Chosen Model

The store now has two prediction-market layers.

### `market_candidates`

This table stores level-2-valid off-chain candidates. A row contains:

- full prediction-market params
- proof-carrying dormant anchor
- raw `creation_tx` bytes
- compiled covenant CMR and cached slot script pubkeys
- provenance (`nevent`, nostr ids/json, first/last seen timestamps)
- TTL / promotion metadata

Identity is:

- `market_id`
- plus the canonical anchor tuple
  - `creation_txid`
  - YES dormant ABF/VBF
  - NO dormant ABF/VBF

This allows multiple conflicting candidates for the same `market_id` to exist
temporarily without corrupting the canonical market row.

### `markets`

This remains the canonical market table. It is keyed only by `market_id` and is
created only when a candidate is promoted after on-chain validation. It stores:

- `candidate_id` for the winning candidate row
- current canonical lifecycle state
- cached state txids / timestamps

Canonical rows do not duplicate the full anchor/param payload; they reference
the promoted candidate instead.

## Validation Stages

### Level 1: Syntax / canonical text

At ingest, the store canonicalizes and validates:

- anchor field encoding
- txid text
- dormant opening fields

### Level 2: Bootstrap validation

Discovery payloads now include `creation_tx_hex`, so ingest always validates:

- params
- proof-carrying anchor
- raw creation transaction bytes

This proves the discovered bootstrap is internally consistent without any chain
access.

### Level 3: On-chain validation

A higher-level sync/service layer is responsible for:

- checking whether the candidate anchor tx exists on-chain
- waiting for Liquid irreversibility
- promoting the candidate
- running canonical descendant state sync

`deadcat-store` exposes DB primitives for this, but does not schedule or own the
workflow itself.

## No-Reorg Contract

This design intentionally does **not** handle reorgs.

Hard rule:

- promotion is only valid after `2` confirmations on Liquid
- canonical market sync updates should also only be applied after the caller has
  enforced the same irreversibility rule

Once promoted, a canonical market is treated as final by the DB layer.

If that assumption is violated, recovery is outside the normal API surface and
should be handled by explicit operational reset/rebuild flows.

## Candidate TTL

Unpromoted candidates are visible for exactly 6 hours from ingest/re-ingest.

Implementation details:

- candidate rows store `expires_at`
- public candidate queries accept an explicit `now` timestamp and filter on TTL
- expired rows disappear from reads exactly at the read-time cutoff
- physical deletion is handled by explicit cleanup via
  `purge_expired_prediction_market_candidates(now)`

This keeps UI/API behavior deterministic while letting higher-level services run
cleanup later.

Promoted rows clear `expires_at` and remain indefinitely.

## Promotion Rules

Promotion is transactional:

1. mark the candidate promoted
2. clear its TTL
3. create the canonical `markets` row
4. delete sibling candidates for the same `market_id`

Why sibling deletion is safe:

- the store assumes no reorg support
- once one candidate is irreversible on-chain, the remaining same-`market_id`
  candidates are no longer relevant for normal operation

If a caller tries to promote a different candidate after a canonical market row
already exists, the DB rejects it.

## Why The Old Model Was Unsafe

Under the old single-table approach, repeated discovery ingest for the same
`market_id` could mutate the stored anchor fields before chain validation. That
made canonical lineage scanning depend on discovery order instead of chain
reality.

The candidate/canonical split fixes this by:

- allowing conflicting off-chain candidates to coexist safely
- making promotion the only path to canonical identity
- keeping canonical state and discovery identity separate

## API Expectations

`deadcat-store` now exposes two families of APIs.

### Candidate APIs

- `ingest_prediction_market_candidate`
- `get_prediction_market_candidate`
- `list_prediction_market_candidates`
- `list_unpromoted_prediction_market_candidates`
- `promote_prediction_market_candidate`
- `purge_expired_prediction_market_candidates`

### Canonical APIs

- `get_market`
- `list_markets`
- canonical UTXO queries
- `sync`

The `sync` path only acts on promoted canonical markets.

## Future Optimization

`creation_tx_hex` is the v1 wire format because it makes level-2 validation
simple and unambiguous.

If announcement size becomes a real issue, the design can later move to a
smaller proof payload. That optimization is intentionally deferred; correctness
and explicit validation are the priority here.
