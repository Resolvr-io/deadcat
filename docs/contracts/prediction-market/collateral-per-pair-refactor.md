# Covenant Parameter Rename: `collateral_per_token` to `collateral_per_pair`

## Problem

The current covenant parameter is `COLLATERAL_PER_TOKEN` — the collateral backing a single token. But the atomic unit of issuance is always a pair (1 YES + 1 NO). Every formula in the codebase immediately multiplies by 2:

```simplicity
fn collateral_for_pairs(pairs: u64) -> u64 {
    let two_cpt: u64 = safe_multiply(2, param::COLLATERAL_PER_TOKEN);
    safe_multiply(pairs, two_cpt)
}
```

And in the SDK: `pairs * 2 * collateral_per_token`

This naming has already caused a documentation bug where formulas were inconsistent (one said `collateral / collateral_per_token`, another said `collateral / (2 * collateral_per_token)`).

## Proposed Change

Rename `COLLATERAL_PER_TOKEN` to `COLLATERAL_PER_PAIR` in the `.simf` covenant source. The value doubles: if `COLLATERAL_PER_TOKEN` was 500, `COLLATERAL_PER_PAIR` becomes 1000.

The `collateral_for_pairs` function simplifies:

```simplicity
// Before
fn collateral_for_pairs(pairs: u64) -> u64 {
    let two_cpt: u64 = safe_multiply(2, param::COLLATERAL_PER_TOKEN);
    safe_multiply(pairs, two_cpt)
}

// After
fn collateral_for_pairs(pairs: u64) -> u64 {
    safe_multiply(pairs, param::COLLATERAL_PER_PAIR)
}
```

All formulas simplify: `pairs = collateral / collateral_per_pair` — no factor of 2 anywhere.

## Impact on deadcat-core

All formulas in the design doc use the simpler form:
- `outstanding_pairs = collateral / collateral_per_pair`
- `collateral = outstanding_pairs * collateral_per_pair`
- `pairs_burned = (old_collateral - new_collateral) / collateral_per_pair`

The `PredictionMarketParams` Rust struct field changes from `collateral_per_token` to `collateral_per_pair`.

## Key Files

- `src-tauri/crates/deadcat-sdk/contract/prediction_market.simf` — rename param, simplify `collateral_for_pairs`
- `src-tauri/crates/deadcat-sdk/src/prediction_market/params.rs` — rename field
- `src-tauri/crates/deadcat-sdk/src/sdk.rs` — update issuance math (remove `* 2`)
