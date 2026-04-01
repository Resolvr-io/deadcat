# Maker Order: Remove Fill Path Cosigner + Pool Admin Key Rename

## Problem 1: Unnecessary Cosigner on Maker Order Fills

The maker order covenant (`maker_order.simf`) currently has an optional cosigner check on the fill path:

```simplicity
fn check_cosigner(i: u32, sig: Signature) {
    // Skip if COSIGNER_PUBKEY == NUMS (no cosigner configured)
    let is_nums: bool = jet::eq_256(param::COSIGNER_PUBKEY, nums_key());
    match is_nums {
        true => {},
        false => {
            // Verify cosigner signature over custom sighash
            // ...
        },
    };
}
```

When `COSIGNER_PUBKEY` is set to the NUMS key, the check is a no-op and fills are permissionless. When set to a real key, fills require co-signing.

There is no compelling reason to gate-keep who can fill a limit order on Liquid:
- **Anti-MEV**: Not a concern on Liquid (federated block production, no public mempool).
- **Spam/dust fills**: Self-punishing (filler pays fees). Can be addressed by minimum fill amounts if needed.
- **Batch matching**: The covenant already enforces correct pricing — a matching service adds latency without benefit.

The NUMS bypass being the expected default confirms this is speculative complexity.

## Proposed Change: Remove Cosigner from Fill Path

Remove the cosigner check entirely from `maker_order.simf`. The fill path becomes unconditionally permissionless:

```simplicity
// Before
fn main() {
    let i: u32 = jet::current_index();
    let cosigner_sig: Signature = witness::COSIGNER_SIGNATURE;
    let i_rem: u32 = safe_add_32(i, 1);
    check_cosigner(i, cosigner_sig);
    let out_spk_hash: u256 = get_output_script_hash(i);
    assert!(jet::eq_256(out_spk_hash, param::MAKER_RECEIVE_SPK_HASH));
    match param::IS_SELL_BASE {
        true => validate_sell_base_fill(i, i_rem),
        false => validate_sell_quote_fill(i, i_rem),
    };
}

// After
fn main() {
    let i: u32 = jet::current_index();
    let i_rem: u32 = safe_add_32(i, 1);
    let out_spk_hash: u256 = get_output_script_hash(i);
    assert!(jet::eq_256(out_spk_hash, param::MAKER_RECEIVE_SPK_HASH));
    match param::IS_SELL_BASE {
        true => validate_sell_base_fill(i, i_rem),
        false => validate_sell_quote_fill(i, i_rem),
    };
}
```

The following can be removed:
- `check_cosigner` function
- `witness::COSIGNER_SIGNATURE` witness declaration
- `param::COSIGNER_PUBKEY` parameter declaration
- `nums_key()` helper function (if not used elsewhere)

### Impact on deadcat-core

- `MakerOrderParams` no longer includes a `cosigner_pubkey` field.
- The PSET builder for order fills no longer needs to encode a cosigner signature in the Simplicity witness.
- Smaller transactions (one fewer signature in the witness).

## Problem 2: Misleading "Cosigner" Name for Pool Admin Key

The LMSR pool covenant uses `COSIGNER_PUBKEY` for the key that authorizes admin operations (adjust, close). This is misleading:
- The swap path is permissionless — no "co-signing" happens.
- The admin/close paths use this key as the **sole** authorization, not a co-signature.
- The pool operator controls the key themselves — no second party involved.

## Proposed Change: Rename Pool Key to `ADMIN_PUBKEY`

Rename `COSIGNER_PUBKEY` to `ADMIN_PUBKEY` in `lmsr_pool.simf` and all related Rust types:

- `.simf` parameter: `param::COSIGNER_PUBKEY` → `param::ADMIN_PUBKEY`
- Rust params: `LmsrPoolParams.cosigner_pubkey` → `LmsrPoolParams.admin_pubkey`
- Function names: `check_cosigner` → `check_admin` (in the pool contract)

This aligns with the existing "admin path" / "admin adjust" terminology used in the design doc.

The pool's close path uses the same authorization model — see [lmsr-pool-close-path.md](lmsr-pool-close-path.md).

## Key Files

- `src-tauri/crates/deadcat-sdk/contract/maker_order.simf` — remove cosigner check, `COSIGNER_PUBKEY` param, `COSIGNER_SIGNATURE` witness
- `src-tauri/crates/deadcat-sdk/src/maker_order/params.rs` — remove `cosigner_pubkey` field
- `src-tauri/crates/deadcat-sdk/src/maker_order/witness.rs` — remove cosigner witness satisfaction
- `src-tauri/crates/deadcat-sdk/contract/lmsr_pool.simf` — rename `COSIGNER_PUBKEY` → `ADMIN_PUBKEY`, `check_cosigner` → `check_admin`
- `src-tauri/crates/deadcat-sdk/src/lmsr_pool/params.rs` — rename `cosigner_pubkey` → `admin_pubkey`
