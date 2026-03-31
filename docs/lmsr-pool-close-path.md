# LMSR Pool Close Script Path

## Problem

The LMSR pool covenant currently has no mechanism to close a pool and return all reserves to the operator. The two existing spend paths both produce new covenant outputs:

- **Swap path**: changes s_index, produces 3 new reserve outputs
- **Admin path**: preserves s_index, adjusts reserves, produces 3 new reserve outputs

The taproot internal key is NUMS (Nothing Up My Sleeve) — the key-spend path is unspendable by design, consistent with the prediction market contract. This means once a pool is created, its reserve UTXOs can never be fully consumed. The operator can drain reserves to the minimums (`min_r_yes`, `min_r_no`, `min_r_collateral`) via the admin path, but cannot reclaim the dust locked at those floors.

## Proposed Change

Add a third spend path to the LMSR pool's primary Simplicity leaf: a **close path** that atomically consumes all three reserve UTXOs and returns funds to the operator.

### Taproot Tree (Updated)

The tree structure remains the same — the close path is a new branch within the primary Simplicity program, not a new taproot leaf:

```
                    merkle_root
                   /            \
            primary_secondary    tapdata_leaf (s_index)
             /         \
        primary_leaf   secondary_leaf
```

The primary leaf's program gains a third top-level path (alongside swap and admin):

```
PathPrimary = Left   → Swap (existing)
PathPrimary = Right  → Admin or Close (new branching within Right)
```

Or alternatively, the close path could be a separate top-level branch in the primary program. The exact branching structure is an implementation detail — what matters is the constraints below.

### Close Path Constraints

The close script path must enforce:

1. **Cosigner authorization**: Valid BIP-340 signature from `COSIGNER_PUBKEY` (same authorization model as admin path). Uses a distinct domain string (e.g., `"DEADCAT/LMSR_POOL_CLOSE_V1"`) to prevent signature reuse across paths.

2. **All three reserve inputs present**: The transaction must spend all three reserve UTXOs (YES, NO, Collateral). This is the atomic guarantee — no partial closure.

3. **No new covenant outputs**: None of the transaction's outputs may match any covenant script pubkey for this pool (at any s_index). This ensures the pool is fully extinguished.

4. **Co-membership**: All three inputs must share the same script pubkey (same s_index), proving they belong to the same pool state. This reuses the existing co-membership verification pattern from the admin path.

### What the Close Path Does NOT Enforce

- **Destination of funds**: The reserves can go anywhere — the operator's wallet, another contract, multiple outputs. The covenant doesn't restrict where funds flow after closure, only that they leave the covenant.
- **Reserve minimums**: Unlike the admin path, the close path has no floor constraints. All reserves are consumed entirely.
- **s_index preservation**: Irrelevant — there are no new covenant outputs to carry an s_index.

## Impact on deadcat-core

### New State and Transition Variants

```rust
pub enum LmsrPoolState {
    Active {
        outpoints: BTreeMap<ReserveSlot, OutPoint>,
        s_index: u64,
        reserves: PoolReserves,
    },
    Closed {
        final_txid: Txid,
    },
}

pub enum PoolTransition {
    Swapped { old_s_index: u64, new_s_index: u64, old_reserves: PoolReserves, new_reserves: PoolReserves },
    Adjusted { old_reserves: PoolReserves, new_reserves: PoolReserves },
    Closed { final_reserves: PoolReserves },
}
```

### New PSET Builder

```rust
pub fn build_lmsr_close_pset(
    &self,
    contract_id: &ContractId,
    funding: &WalletFunding,
) -> Result<PartiallySignedTransaction, CoreError<S::Error>>;
```

Takes only the contract ID and wallet funding. The engine reads the current reserves from the stored state, compiles the covenant for witness encoding, and builds the PSET. All reserve outputs go to `funding.return_script`.

### State Advancement

`process_transaction` identifies closure by: all pool outpoints spent + no new outputs match any pool covenant script. Produces `PoolTransition::Closed { final_reserves }` and sets state to `LmsrPoolState::Closed { final_txid }`.

### StateFilter

`StateFilter::TerminalOnly` now returns closed pools (previously returned no pools since there was no terminal state).

## Consistency Across Contract Types

| Contract | Internal Key | Can Key-Spend? | Close Mechanism |
|---|---|---|---|
| Prediction Market | NUMS | No | Redemption / cancellation (existing script paths) |
| LMSR Pool | NUMS | No | Close script path (this proposal) |
| Maker Order | `maker_base_pubkey` | Yes | Key-spend by maker (existing), or fill/cancel script paths |

Maker orders intentionally use a real internal key — the maker's ability to key-spend is the cancellation mechanism (the maker can always reclaim their funds without going through the Simplicity program). Markets and pools use NUMS because their closure is governed by covenant logic, not a single party's key.

## Key Files

- `src-tauri/crates/deadcat-sdk/contract/lmsr_pool.simf` — add close path to primary program
- `src-tauri/crates/deadcat-sdk/src/lmsr_pool/contract.rs` — compilation (no structural change — same leaves)
- `src-tauri/crates/deadcat-sdk/src/lmsr_pool/witness.rs` — witness satisfaction for close path
