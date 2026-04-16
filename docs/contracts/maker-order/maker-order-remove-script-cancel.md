# Maker Order: Remove Script Cancel Path

## Problem

The maker order covenant (`maker_order.simf`) currently has two cancellation mechanisms:

1. **Key-spend**: The maker's real key (`maker_pubkey`) is the taproot internal key. The maker can key-spend to reclaim funds with no covenant constraints.
2. **Script cancel path**: A Simplicity spend path (`witness::PATH = Right`) that verifies a maker signature on `SHA256(prev_outpoint)` with no output constraints.

These are functionally identical — both allow the maker to reclaim funds with no output restrictions. The script cancel path is strictly heavier (script-spend witness vs key-spend witness) with no additional capability.

More importantly, having both paths makes it impossible for the `deadcat-core` engine to reliably distinguish between a complete fill and a cancellation using only structural witness checks. Both the fill path and the cancel path are script-spend paths in the same taproot leaf, so key-spend vs script-spend detection cannot differentiate them.

## Proposed Change

Remove the script cancel path from `maker_order.simf`. The `main()` function simplifies from a two-branch match to a single fill path:

```simplicity
// Before
fn main() {
    let i: u32 = jet::current_index();

    match witness::PATH {
        Left(u: ()) => {
            // Fill path
            let cosigner_sig: Signature = witness::COSIGNER_SIGNATURE;
            let i_rem: u32 = safe_add_32(i, 1);
            check_cosigner(i, cosigner_sig);
            let out_spk_hash: u256 = get_output_script_hash(i);
            assert!(jet::eq_256(out_spk_hash, param::MAKER_RECEIVE_SPK_HASH));
            match param::IS_SELL_BASE {
                true => validate_sell_base_fill(i, i_rem),
                false => validate_sell_quote_fill(i, i_rem),
            };
        },
        Right(u: ()) => {
            // Cancel path — maker signature authorizes reclaiming funds
            let maker_sig: Signature = witness::MAKER_CANCEL_SIGNATURE;
            check_cancel(i, maker_sig);
        },
    };
}

// After (reflects both script-cancel removal and cosigner removal per maker-order-remove-cosigner.md)
fn main() {
    let i: u32 = jet::current_index();
    let i_rem: u32 = safe_add_32(i, 1);
    let out_spk_hash: u256 = get_output_script_hash(i);
    assert!(jet::eq_256(out_spk_hash, param::MAKER_RECEIVE_SPK_HASH));
    match param::DIRECTION {
        SellBase => validate_sell_base_fill(i, i_rem),
        SellQuote => validate_sell_quote_fill(i, i_rem),
    };
}
```

The following can also be removed:
- `check_cancel` function (lines 90-97)
- `witness::MAKER_CANCEL_SIGNATURE` witness declaration
- `witness::PATH` witness declaration (no longer needed — only one path)

## Impact on deadcat-core

### Watertight Order Transition Detection

With key-spend as the only cancellation mechanism, the engine can use a simple structural check on the taproot witness to distinguish fills from cancellations:

1. **Partial fill**: Order outpoint spent, new covenant output exists with the same script pubkey → `OrderTransition::Filled`
2. **Complete fill**: Order outpoint spent, no new covenant output, witness is script-spend (taproot script path) → `OrderTransition::Filled` (the Simplicity covenant enforced valid payment)
3. **Cancellation**: Order outpoint spent, no new covenant output, witness is key-spend (taproot key path) → `OrderTransition::Cancelled`

Key-spend vs script-spend is trivially distinguishable from the witness stack structure — key-spend has a single stack element (64-byte signature), script-spend has multiple elements (witness data + script + control block). This is a Bitcoin/Elements-level structural check, not Simplicity witness decoding. It does not require compiled contracts.

### build_cancel_order_pset

`build_cancel_order_pset` constructs a key-spend transaction. This is simpler than the current implementation — no Simplicity witness encoding needed, just a taproot key-spend signature. The PSET builder still needs to know the taproot internal key and merkle root (to compute the tweak), but does not need the compiled Simplicity contract.

## Consistency Across Contract Types

| Contract | Internal Key | Can Key-Spend? | Script Paths |
|---|---|---|---|
| Prediction Market | NUMS | No | Issuance, resolution, redemption, cancellation, expiry |
| LMSR Pool | NUMS | No | Swap, admin adjust, close |
| Maker Order | `maker_pubkey` | Yes (cancellation) | Fill only |

Maker orders intentionally use a real internal key — the maker's ability to key-spend is the sole cancellation mechanism. Markets and pools use NUMS because their lifecycle is governed by covenant logic, not a single party's key.

## Key Files

- `src-tauri/crates/deadcat-sdk/contract/maker_order.simf` — remove cancel path, `check_cancel`, related witnesses
- `src-tauri/crates/deadcat-sdk/src/maker_order/witness.rs` — remove cancel witness satisfaction
- `src-tauri/crates/deadcat-sdk/src/maker_order/pset/cancel_order.rs` — simplify to key-spend construction
