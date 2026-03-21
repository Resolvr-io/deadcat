# Simplicity Testing Methodology

**Applies to:** LMSR pool covenant operations (create, scan, swap, adjust, close)

---

## Three evaluation tiers

Simplicity programs are verified at three levels, each catching different
classes of bugs with different cost/speed tradeoffs.

### Tier 1: Rust BitMachine (unit tests, ~1-2s)

**What it is:** The `simplicity-lang` Rust evaluator. Control flow in Rust,
individual jets via C FFI into `simplicity-sys`.

**What it catches:**
- Wrong witness values (bad signature, wrong proof, missing fields)
- Contract assertion failures (fee inequality, reserve floors, delta rules)
- Merkle proof validation errors
- Pruning errors that produce structurally invalid programs

**What it misses:**
- `ElementsEnv` construction bugs (genesis hash, txHash, tapEnvHash)
- PSET construction bugs (value balance, blinding, output ordering)
- Serialization bugs where the Rust evaluator and C deserializer disagree

**When to use:** Any new Simplicity contract logic, witness field, or
signature scheme. This is the default — if you can test it here, do.

**How:** Construct a mock PSET with `explicit_txout` + `dummy_utxo`, build
an `ElementsEnv`, call `satisfy_*_with_env`, run `BitMachine::exec`.
See `admin_adjust_c_evaluator_agrees_with_rust` in `assembly.rs` for the
pattern.

### Tier 1.5: C evaluator via corrected FFI (unit tests, ~2s)

**What it is:** The full C `evalTCOExpression` pipeline (decode → type-infer
→ fill witness → analyse bounds → execute) called via `c_eval.rs`.

**What it catches:** Everything Tier 1 catches, plus:
- Serialization roundtrip bugs (Rust serializer → C deserializer)
- Pruning bugs where the Rust and C evaluators disagree on branch selection
- Value bit corruption (the `right_shift_1` bug from PR #348)

**What it misses:**
- Same `ElementsEnv` and PSET construction gaps as Tier 1
- Currently uses `simplicity-sys`'s bundled C code, which may differ from
  `elementsd`'s version (though in practice they're identical)

**When to use:** Alongside Tier 1 for any path that involves pruning
(admin path prunes the swap branch and vice versa). The overhead is
negligible — include it whenever the `testing` feature is enabled.

**How:** After `BitMachine::exec`, call
`crate::lmsr_pool::c_eval::run_program_with_env(&prog_bytes, &wit_bytes, env.c_tx_env())`.
This is already wired into `attach_lmsr_pool_witnesses` under
`#[cfg(feature = "testing")]`.

### Tier 2: Regtest integration tests (~20-30s each)

**What it is:** A real `elementsd` + `electrs` processing the actual
broadcast transaction.

**What it catches:** Everything, including:
- `ElementsEnv` construction divergence (genesis hash byte order, txHash
  computation, tapEnvHash)
- PSET value balance (explicit vs confidential mixing)
- Blinding (surjection proofs, input index mapping)
- Fee handling (absorption from surplus, separate wallet inputs)
- `sign_pset` / `finalize` mutations
- Chain walk / rescan correctness

**What it misses:** Nothing — this is the consensus reference.

**When to use:**
- New transaction construction logic (PSET layout, blinding, fee handling)
- Any code that reads `ElementsEnv` jets and uses those values externally
  (admin signatures, attestations)
- End-to-end flows (create → scan → trade → rescan)
- One-time verification that a Tier 1 test's mock env matches reality

**How:** Use the `Fixture` in `tests/lmsr_pool.rs`. Call
`set_chain_genesis_hash` from the RPC. See the `bootstrap_admin_pool` +
`adjust_lmsr_pool_*` tests for the pattern.

---

## Decision tree

```
Is the bug in Simplicity program logic (witnesses, signatures, proofs)?
  YES → Tier 1 unit test (BitMachine)
        + Tier 1.5 if pruning is involved

Is the bug in PSET construction (outputs, blinding, fee, value balance)?
  YES → Tier 1 unit test for the PSET structure (assert output count,
        values, asset IDs, which indices are blinded)
        + Tier 2 integration test for consensus acceptance

Is the bug in ElementsEnv construction (genesis hash, sighash)?
  YES → Tier 2 integration test (only way to compare against elementsd)

Does the code path only use pure Rust validation (no chain, no Simplicity)?
  YES → Regular unit test (no BitMachine, no regtest)
```

---

## Regtest genesis hash

Each `elementsd` regtest instance has a unique genesis block hash. The SDK's
`Network::LiquidRegtest.genesis_hash()` returns a hardcoded constant that
does NOT match. For any test that exercises the admin signature path:

1. Fetch the real genesis hash via RPC: `getblockhash 0`
2. Reverse the hex bytes (RPC returns display order, SDK uses internal order)
3. Call `node.set_chain_genesis_hash(bytes)` before any admin operations

This is already done in the `Fixture::new()` setup. For Liquid mainnet and
testnet, the hardcoded values are correct.

---

## Current coverage

| Code path | Tier 1 | Tier 1.5 | Tier 2 | Tests |
|---|---|---|---|---|
| Swap (primary witness) | ✓ | ✓ | ✓ | `swap_primary_executes_in_bitmachine`, `create_scan_*_regtest` |
| Swap (secondary witness) | ✓ | — | ✓ | `swap_secondary_executes_in_bitmachine`, `create_scan_*_regtest` |
| Admin adjust (signature + witness) | ✓ | ✓ | ✓ | `admin_adjust_c_evaluator_agrees_with_rust`, `adjust_*_regtest` |
| Admin adjust (PSET: decrease) | ✓ | — | ✓ | `admin_adjust_pset_decrease_structure`, `adjust_*_decreases_regtest` |
| Admin adjust (PSET: increase) | — | — | ✓ | `adjust_*_increases_regtest` |
| Close (reclaimed amounts) | ✓ | — | — | `close_reclaimed_amounts` |
| Pool creation | — | — | ✓ | `create_scan_*_regtest` |
| Pool scanning / chain walk | — | — | ✓ | `create_scan_*_regtest`, `scan_*_bootstraps_*_regtest` |
| Validation rejections (IDs, bounds) | — | — | ✓ | `scan_*_rejects_*_regtest` |
| Swap fee inequality | ✓ | — | — | `rejects_buy_leg_that_violates_fee_inequality` |
| Merkle proof validation | ✓ | — | — | `rejects_leg_with_invalid_merkle_proof`, `rejects_*_missing_*` |
