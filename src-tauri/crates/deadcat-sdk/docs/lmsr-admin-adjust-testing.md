# LMSR AdminAdjust — Testing Design & Lessons Learned

**Date:** 2026-03-20

---

## What this covers

The `adjust_lmsr_pool` SDK method lets a pool admin add or remove liquidity
from an LMSR reserve bundle on Liquid.  It constructs a PSET with covenant
inputs (explicit reserve UTXOs) and optional wallet inputs (confidential),
computes a BIP-340 admin signature over a Simplicity-specific message hash,
attaches Simplicity witnesses, and broadcasts.

Getting this to work end-to-end required fixing four layered bugs, each
invisible until the previous one was resolved.  This document captures the
testing architecture and the reasoning behind key design decisions so future
work doesn't have to re-derive them.

---

## Test matrix

| Test | Type | What it verifies | Runtime |
|---|---|---|---|
| `admin_adjust_c_evaluator_agrees_with_rust` | Unit | Rust BitMachine + C evaluator both accept the admin adjust program with a mock env | ~2s |
| `adjust_lmsr_pool_increases_reserves_regtest` | Integration | Full increase-liquidity flow on regtest (wallet inputs, blinding, broadcast, rescan) | ~20s |
| `adjust_lmsr_pool_decreases_reserves_regtest` | Integration | Full decrease-liquidity flow on regtest (no wallet inputs, fee from surplus, broadcast, rescan) | ~20s |
| All 6 `lmsr_pool` integration tests | Integration | Create, scan, trade, adjust (increase + decrease), validation rejection | ~105s |

---

## Bug 1: Zero genesis hash in assembly env

**Symptom:** Admin signature passed locally but `elementsd` rejected with
"Assertion failed inside jet".

**Root cause:** `attach_lmsr_pool_witnesses` used
`BlockHash::all_zeros()` for the `ElementsEnv` genesis hash. The swap path
never exercises `jet::genesis_block_hash()` directly (only via the
self-consistent `sig_all_hash`), so swaps worked. The admin path explicitly
hashes `genesis_block_hash()` into the admin signature message — a zero hash
produced a different message than on-chain.

**Fix:** Added `genesis_hash: [u8; 32]` parameter to
`attach_lmsr_pool_witnesses`. Callers pass the real genesis hash.

**Testing insight:** The unit test `admin_adjust_c_evaluator_agrees_with_rust`
uses `genesis_hash = [0u8; 32]` — this is fine because both the signature
computation and the env use the same zeros, so they're self-consistent. The
integration tests use the real chain genesis, which is what matters for
on-chain acceptance.

---

## Bug 2: Hardcoded regtest genesis hash

**Symptom:** After fixing bug 1, admin adjust still failed on regtest with
"Assertion failed inside jet".

**Root cause:** `Network::LiquidRegtest.genesis_hash()` returns LWK's
hardcoded `GENESIS_LIQUID_REGTEST` constant. But `lwk_test_util::TestEnv`
starts a fresh `elementsd` instance that generates a **unique** genesis block
each time. The admin signature was computed with the wrong genesis hash.

The swap path works despite the wrong genesis hash because both the local
env and `elementsd` use their own genesis bytes consistently — the sighash
is self-consistent within each implementation. The admin path breaks because
it explicitly includes `genesis_block_hash()` in the admin signature message.

**Fix:** Added `DeadcatSdk::set_chain_genesis_hash()` and
`chain_genesis_hash()`. The integration test fixture fetches the real genesis
hash via `elementsd` RPC (`getblockhash 0`) and sets it on the node before
any admin operations.

**Design decision:** Why not fetch from Electrum at runtime?  The test's
`electrs` process can't handle additional TCP connections while the SDK has
an open one.  `server.features` calls consistently fail.  For production
(Liquid / Liquid Testnet), the hardcoded genesis hashes are correct.  Only
regtest needs the override, and the test framework has RPC access to provide
it.

**For production:** On Liquid mainnet and testnet, the hardcoded genesis
hashes in LWK match the real chain.  `set_chain_genesis_hash` is only needed
for regtest.

---

## Bug 3: Explicit outputs with confidential inputs

**Symptom:** After fixing bugs 1-2, the increase-reserves test failed with
`"bad-txns-in-ne-out, value in != value out"`.

**Root cause:** When increasing reserves, the wallet provides confidential
L-BTC inputs for the extra collateral + fee. All outputs were explicit. On
Elements, confidential input values are Pedersen commitments that don't
participate in the explicit value balance check.  The explicit inputs (50k +
50k + 100k = 200k) don't cover the explicit outputs (60k + 60k + 110k +
500 + change = 240.5k) because the extra 40.5k comes from confidential
wallet inputs.

**Fix:** Wallet-surplus change outputs are now blinded using `blind_last`
before the admin signature is computed. This makes the transaction use
proper CT commitments for wallet-derived value flows while keeping reserve
outputs explicit (as the covenant requires).

**Design decision:** Reserve-surplus change outputs (from decreasing reserves)
stay explicit. Their source inputs are explicit covenant outputs — the blinder
can't produce surjection proofs without confidential inputs for those assets.
This is fine because the reserve amounts are already public on-chain.

For the decrease-only case (no wallet inputs), the fee is absorbed from the
explicit collateral surplus (`fee_absorbed_by_collateral_surplus`), avoiding
any confidential value flows entirely.

---

## Bug 4: Blinding input index mismatch

**Symptom:** After fixing bug 3, the increase test still failed with
`"value in != value out"`.

**Root cause:** `blind_order_pset` indexes `wallet_inputs` starting at 0,
but in the PSET, wallet inputs are at indices 3+ (after the 3 reserve
inputs at indices 0-2).  `blind_last` looks up input secrets by PSET index.
When building the surjection proof for a wallet-surplus change output, it
looks for the wallet input at PSET index 3 but the secrets map only has
indices 0-2 (the first 3 wallet inputs mapped to the wrong positions).

**Fix:** `adjust_lmsr_pool` builds the `inp_txout_sec` map manually with
correct PSET indices: reserve inputs at 0-2, wallet inputs at 3+.  This
bypasses `blind_order_pset` for the adjust case.

**Note:** `blind_order_pset` works correctly for its other callers
(`build_lmsr_bootstrap_pset`, limit orders) because in those cases ALL
inputs are wallet inputs, so the 0-based enumeration matches the PSET
indices.

---

## Simplicity evaluation architecture

Three distinct evaluation paths exist, and understanding their differences
was critical for debugging:

### Rust `BitMachine` (local, pre-broadcast check)

- In `simplicity-lang` (`src/bit_machine/mod.rs`)
- Control flow (case/comp/disconnect) in **Rust**
- Individual jets called via **C FFI** into `simplicity-sys`
- Used by the `#[cfg(debug_assertions)]` check in `attach_lmsr_pool_witnesses`
- **Limitation:** Cannot detect genesis hash mismatches with `elementsd`,
  because both the signature and the env use the same (possibly wrong) bytes

### C evaluator via corrected FFI (`c_eval::run_program_with_env`)

- Uses `simplicity-sys`'s bundled C code (`eval.c`)
- Entire program evaluation (control flow + jets) in **C**
- Called via our corrected FFI binding in `lmsr_pool/c_eval.rs`
- **Note:** `simplicity-sys` ≤0.6.2 has a bug in its `evalTCOExpression`
  binding (missing `minCost` parameter). Our `c_eval.rs` works around this
  with a corrected `extern "C"` declaration.

### `elementsd` on-chain evaluation

- Same C code as above, but `elementsd` constructs the `txEnv` from its
  internal `CTransaction` + `PrecomputedTransactionData`
- The genesis hash comes from `uint256::data()` which returns the real
  chain genesis in internal byte order
- **This is the consensus reference.** If the Rust evaluator and C evaluator
  both accept a program but `elementsd` rejects it, the issue is in how the
  `txEnv` is constructed, not in the program itself.

### What the unit test covers

`admin_adjust_c_evaluator_agrees_with_rust` runs both the Rust BitMachine
AND the C evaluator (via `c_eval::run_program_with_env`) against the same
`ElementsEnv`. This catches:

- Witness serialization bugs (C deserializer rejects)
- Pruning/value corruption bugs (C evaluator disagrees with Rust)
- Admin signature computation errors (jet assertion failures)

It does **not** catch genesis hash mismatches with `elementsd` (both
evaluators use the same locally-constructed env).  That's what the
integration tests are for.

---

## `simplicity-sys` FFI bug (known issue, ≤0.6.2)

The `evalTCOExpression` Rust binding in `simplicity-sys/src/tests/ffi.rs`
is missing the `minCost: ubounded` parameter, shifting all subsequent
arguments.  This makes `simplicity_sys::tests::run_program` with
`TestUpTo::Everything` and a non-null env produce undefined behavior.

Our `c_eval.rs` module works around this with a corrected `extern "C"`
declaration that includes `min_cost`.

Additionally, `run_program`'s budget handling casts a `u32` value to a raw
pointer (`budget.map(|b| b as *const _)`) instead of taking a reference.

Both bugs are in the test-utilities module only, not in the jet FFI used by
`BitMachine` at runtime.

---

## Dependency patches

The workspace `Cargo.toml` patches `simplicity-lang` and `simplicity-sys`
to point to upstream `BlockstreamResearch/rust-simplicity` master at commit
`8839c919` (post-PR-#348, pre-jet-rename).  This includes the
`Value::right_shift_1` bit corruption fix from PR #348 which prevents
corrupted Left/Right tag bits during pruning.  Once `simplicity-lang` ≥0.7.1
is released with the fix, the patches can be removed.

The SDK also has an optional dependency on `simplicity-sys` with the
`test-utils` feature, gated behind `feature = "testing"`, to enable the
in-process C evaluator check.
