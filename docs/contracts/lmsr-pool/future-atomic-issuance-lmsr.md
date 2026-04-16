# Future Enhancement: Atomic Issuance + LMSR Pool Bootstrap

## Current Limitation

Pool creation requires pre-existing YES, NO, and L-BTC tokens. The user must issue token pairs via the market covenant in a separate transaction before creating an LMSR pool. This adds an extra on-chain step, increases fees, and complicates the UX.

## Proposed Enhancement

A single PSET that atomically issues YES+NO token pairs via the market covenant AND locks them into an LMSR pool covenant, all in one transaction.

## PSET Structure Sketch

```
Inputs:
  [0] Market reissuance token (YES)
  [1] Market reissuance token (NO)
  [2..N] L-BTC wallet UTXOs (collateral + fees)

Outputs:
  [0] YES reserve  → LMSR covenant script (s_index)
  [1] NO reserve   → LMSR covenant script (s_index)
  [2] L-BTC reserve → LMSR covenant script (s_index)
  [3] YES reissuance token change → market covenant
  [4] NO reissuance token change  → market covenant
  [5] Fee output
  [6..N] Wallet change outputs

Issuances (on inputs 0, 1):
  - YES tokens: amount = initial_reserves.r_yes
  - NO tokens:  amount = initial_reserves.r_no
```

## Covenant Considerations

Both the market covenant (governing reissuance tokens) and the LMSR covenant (governing reserves) must be satisfied in the same transaction:

- The market covenant validates that issuance follows its rules (correct amounts, entropy, blinding)
- The LMSR covenant validates the bootstrap transition (initial reserves, s_index, table commitment)
- Both covenants inspect the same transaction via Simplicity introspection jets

The LMSR bootstrap currently expects wallet UTXOs as inputs for reserves. For atomic issuance, the reserve outputs would be funded by freshly-issued tokens in the same transaction rather than pre-existing wallet UTXOs.

## Key Challenges

1. **Blinding factor coordination**: Issuance outputs must be blinded consistently. The LMSR covenant expects explicit (unblinded) reserve outputs, while the market covenant may require blinded issuance outputs. Reconciling these requirements may need covenant-level changes.

2. **Issuance entropy**: The market covenant derives issuance entropy from the reissuance token input. The LMSR pool identity derivation needs the creation txid, which isn't known until the transaction is built. This creates a chicken-and-egg dependency.

3. **Script derivation across covenants**: The LMSR reserve outputs must land at the correct covenant script pubkey (determined by pool params including the table root). The market covenant's issuance rules and the LMSR covenant's bootstrap rules must be simultaneously satisfiable.

4. **Witness size**: Satisfying two Simplicity programs in a single transaction increases witness size. Budget analysis is needed to ensure the combined witness fits within consensus limits.

## Benefits

- Single-transaction pool bootstrapping from pure L-BTC (no pre-issuance step)
- Better UX: one confirmation instead of two
- Lower total fees (one transaction instead of two)
- Atomic: no intermediate state where tokens exist outside a pool

## Estimated Scope

- New combined PSET builder that handles both covenant inputs/outputs
- Modifications to LMSR bootstrap validation to accept issuance-funded reserves
- Extensive integration testing across both covenant systems
- Potential covenant-level changes if blinding requirements conflict
