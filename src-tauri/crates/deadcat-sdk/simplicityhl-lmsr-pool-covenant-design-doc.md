# Binary LMSR Pool Covenant

**Design Document**

SimplicityHL on Liquid - Resolvr Inc. - February 2026

**DRAFT v0.1 - Table-Committed LMSR**

---

## 1. Overview

This document proposes a SimplicityHL covenant for a binary, collateral-quoted LMSR market maker for prediction tokens on Liquid. The pool holds three explicit reserve UTXOs:

- YES token reserve
- NO token reserve
- collateral reserve (for example, L-BTC)

Unlike the existing constant-product AMM (`amm_pool.simf`), this design prices swaps using an LMSR cost function and enforces pricing via a committed lookup table (Merkle root parameter). The script commits to a single `LMSR_TABLE_ROOT` for the pool, and every swap must prove both old/new points against that same root. This avoids on-chain `log`/`exp` while preserving LMSR-style coherent pricing.

A key advantage versus the current 3-asset AMM is complement coherence: YES and NO are priced as complementary claims from one shared LMSR state, so the model itself enforces the market-pair relation (`YES + NO = pair value` in normalized units). In the CPAMM design, that relation is restored only by external arbitrage after trades. This removes endogenous YES/NO-sum dislocations created by the AMM mechanism itself (while normal execution slippage and fee spread still apply).

The design keeps the same covenant architecture style as the current AMM:

- fixed reserve ordering
- tapdata-committed state in scriptPubKey
- primary input path + secondary co-membership path
- permissionless swap path

---

## 2. Goals

- Enforce YES/NO coherent pricing from one shared state (`p_yes + p_no = 1` in normalized units).
- Keep swap validation fully on-chain and deterministic.
- Avoid direct on-chain logarithm/exponential evaluation.
- Preserve simple UTXO layout and transaction construction patterns from the existing AMM SDK.
- Keep all on-chain checks in integer arithmetic + hash verification.
- Support atomic composition with the prediction market covenant in a single transaction.

### 2.1 Non-goals (v0.1)

- Permissionless LP shares and LP token mint/burn logic.
- Direct YES<->NO swap in one covenant path (wallet routes via collateral legs).
- Dynamic curve parameter updates in-place (deploy a new covenant for new table/root).

---

## 3. LMSR Model

### 3.1 Base cost function

For binary outcomes YES/NO with liquidity parameter `b` (in token-lot units), define:

```
C(q_yes, q_no) = U * b * ln(exp(q_yes / b) + exp(q_no / b))
```

Where:

- `U` = payout notional per winning token (sats)
- `q_yes`, `q_no` = outstanding YES/NO sold by the market maker (lot units)

### 3.2 1D decomposition used on-chain

Let:

```
t = q_yes + q_no
s = q_yes - q_no
F(s) = U * b * ln(2 * cosh(s / (2b)))
```

Then:

```
C = U * t / 2 + F(s)
```

For a trade of `x` lots:

- Buy YES (`s -> s + x`, `t -> t + x`):  
  `cost = x * U/2 + F(s + x) - F(s)`
- Sell YES (`s -> s - x`, `t -> t - x`):  
  `rebate = x * U/2 + F(s) - F(s - x)`
- Buy NO (`s -> s - x`, `t -> t + x`):  
  `cost = x * U/2 + F(s - x) - F(s)`
- Sell NO (`s -> s + x`, `t -> t - x`):  
  `rebate = x * U/2 + F(s) - F(s + x)`

Key point: the covenant only needs state `s` plus function values `F(s)` from a committed table.

### 3.3 Discrete state grid

`s` is represented as a bounded integer grid:

- `S_INDEX` in `[0, S_MAX_INDEX]`
- off-chain signed mapping for math/table generation:
  `s_steps_i64 = (S_INDEX as i64) - (S_BIAS as i64)`
- `s_lots_i64 = s_steps_i64 * (Q_STEP_LOTS as i64)`

On-chain semantics stay unsigned/index-based: the covenant validates `OLD_S_INDEX`, `NEW_S_INDEX`, direction, and `abs(NEW_S_INDEX - OLD_S_INDEX)` without requiring signed arithmetic.

The table stores `F` at every valid index:

```
F_i = floor( U * b * ln(2 * cosh(s_i / (2b))) )
```

The covenant enforces transitions between `old_index` and `new_index` using `F_old` and `F_new` proved against a committed Merkle root.

---

## 4. Why Table-Committed LMSR

`amm_pool.simf` avoided LMSR because SimplicityHL has no direct `log`/`exp` jets. This proposal resolves that by committing a deterministic, precomputed LMSR table at compile time:

- on-chain: verify Merkle inclusion and arithmetic inequalities only
- off-chain: generate table values from high-precision LMSR formula
- script constant: one fixed `LMSR_TABLE_ROOT` per deployed pool

This makes on-chain behavior exact with respect to the committed table (the curve is discrete by definition), not approximate at runtime.

---

## 5. Contract Parameters

| Parameter | Description |
|-----------|-------------|
| `YES_ASSET_ID` | YES outcome token asset ID |
| `NO_ASSET_ID` | NO outcome token asset ID |
| `COLLATERAL_ASSET_ID` | collateral asset ID (for example, L-BTC) |
| `LMSR_TABLE_ROOT` | Merkle root committing `(index, F(index))` values |
| `TABLE_DEPTH` | Fixed Merkle depth used by proof verifier |
| `Q_STEP_LOTS` | YES/NO lot quantum (asset atoms) per index step |
| `S_MAX_INDEX` | Max table index |
| `S_BIAS` | Index representing `s = 0` |
| `HALF_PAYOUT_SATS` | `U / 2` in sats |
| `FEE_BPS` | Swap fee in basis points |
| `COSIGNER_PUBKEY` | Admin key for liquidity-adjust path; NUMS makes this path fail-closed |
| `MIN_R_YES` | Minimum YES reserve |
| `MIN_R_NO` | Minimum NO reserve |
| `MIN_R_COLLATERAL` | Minimum collateral reserve |
| `CHAIN_GENESIS_HASH` | 32-byte chain identifier (genesis hash) for signature domain binding |

Fixed constant:

- `FEE_DENOM = 10000`

`Q_STEP_LOTS` is independent from asset decimal display precision. It defines the on-chain trading quantum, and reserve deltas must be exact multiples of `Q_STEP_LOTS`.

### 5.1 Market-linked pool constraints

For pools intended to atomically compose with a specific prediction market covenant, v0.1 requires:

1. `COLLATERAL_ASSET_ID == market.COLLATERAL_ASSET_ID`
2. `HALF_PAYOUT_SATS == market.COLLATERAL_PER_TOKEN`

This binds LMSR payout scale (`U/2`) to market payout units and avoids deterministic cross-covenant mispricing.

---

## 6. State Model (Tapdata)

The covenant state committed in tapdata is:

```
state = S_INDEX (u64)
```

Address derivation mirrors existing Astrolabe-style contracts:

```
h_primary   = tapleaf_hash(primary_leaf_cmr)
h_secondary = tapleaf_hash(secondary_leaf_cmr)
h_tapdata   = tapdata_hash(S_INDEX)
h_inner     = tapbranch_hash(h_primary, h_secondary)      // lexicographically sorted pair
h_root      = tapbranch_hash(h_inner, h_tapdata)          // lexicographically sorted pair
script_hash(S_INDEX) = P2TR(NUMS, h_root)
```

Tree shape and control-block paths are consensus-critical for interoperability:

1. fixed binary structure: `((primary, secondary), tapdata)`
2. branch hashing uses lexicographic child sorting at each level (`tapbranch_hash`)
3. primary leaf control-block merkle path siblings are exactly `[h_secondary, h_tapdata]`
4. secondary leaf control-block merkle path siblings are exactly `[h_primary, h_tapdata]`
5. tapdata leaf control-block merkle path sibling is exactly `[h_inner]`

State verification on spend:

```
script_hash(witness::OLD_S_INDEX) == input_script_hash(IN_BASE)
```

State transition:

- Swap path: `NEW_S_INDEX` must match reserve deltas and becomes new state.
- Admin liquidity path: `NEW_S_INDEX == OLD_S_INDEX` (no state recenter in v0.1).
- Every successful state transition changes the pool script hash/address because `S_INDEX` is committed in tapdata.

---

## 7. Pool UTXO Layout and Base-Indexed Addressing

The LMSR reserve set always has three covenant UTXOs:

| Relative index | Reserve role |
|------|--------------|
| `+0` | YES reserve |
| `+1` | NO reserve |
| `+2` | collateral reserve |

Absolute locations are provided by witnesses:

- `IN_BASE`: first LMSR input index
- `OUT_BASE`: first LMSR output index

The covenant interprets LMSR reserves as:

- inputs: `IN_BASE + {0,1,2}`
- outputs: `OUT_BASE + {0,1,2}`

All three LMSR inputs must share the same old script hash; all three LMSR outputs must share the same new script hash.

No LP reissuance token is used in v0.1.

### 7.1 Canonical reserve-bundle identity (v0.1)

Without an RT anchor, reserve identity must be tracked canonically by outpoint lineage, not by "first UTXO per asset at script hash."

Required v0.1 wallet/discovery rule:

1. define the canonical bundle at creation as the 3 reserve outpoints `(YES, NO, collateral)` created for the pool
2. on each accepted transition, the next canonical bundle is exactly outputs `OUT_BASE + {0,1,2}` of the transaction that spends all 3 prior canonical outpoints
3. ignore additional same-script UTXOs as foreign/polluting UTXOs; they are never treated as reserve identity
4. scanner/builder logic must reject ambiguous "asset-first" reserve selection and require canonical outpoint continuity

Deterministic next-bundle recovery algorithm:

1. find the next transaction `T` that spends all three prior canonical reserve outpoints
2. identify the primary LMSR input index `p` as the input spending prior canonical YES outpoint
3. decode primary LMSR witness at input `p` and recover claimed `OUT_BASE`
4. verify output window `(OUT_BASE, OUT_BASE+1, OUT_BASE+2)` satisfies:
   - assets are exactly `(YES, NO, collateral)` in order
   - amounts are explicit (non-confidential)
   - `script_hash(output[OUT_BASE]) == script_hash(output[OUT_BASE+1]) == script_hash(output[OUT_BASE+2])`
5. advance canonical bundle to `(OUT_BASE, OUT_BASE+1, OUT_BASE+2)` from `T`

Mandatory fallback rule:

- if primary input/witness cannot be decoded, or claimed `OUT_BASE` fails validation, transition tracking must fail closed:
  - do not advance canonical state
  - mark pool sync as ambiguous/error
  - require operator/wallet remediation (no heuristic first-match fallback, and no alternate-window guessing)

This is a mandatory v0.1 policy requirement for correct pool tracking and spend construction. A future version may add a stronger consensus-level anchor, but that is out of scope here.
There is no compatibility mode for LMSR reserve identity in v0.1: implementations that keep asset-first, first-pool, or alternate-window fallback behavior are non-compliant.

### 7.2 Witness decode contract for canonical tracking (v0.1)

Canonical bundle advancement depends on decoding the primary LMSR witness deterministically.

Required v0.1 decoder contract:

1. decode only the input that spends prior canonical YES outpoint and proves the primary leaf path
2. decode witness using schema id `DEADCAT/LMSR_WITNESS_SCHEMA_V1`
3. required decoded fields for tracking:
   - `PATH_PRIMARY` (swap/admin selector)
   - `OUT_BASE` (`u32`)
   - `OLD_S_INDEX`, `NEW_S_INDEX` (`u64`)
4. schema/version mismatch, parse failure, or missing required fields must fail closed

SDK must publish parser conformance vectors (witness bytes -> decoded fields) for this schema.

### 7.3 Why `IN_BASE` and `OUT_BASE`

The existing market covenant can keep its global index assumptions (for example, primary input fixed at index 0). LMSR uses base-indexed addressing so both covenants can be satisfied in one transaction without slot collisions.

This is not "loose flexibility." The covenant still enforces strict structure:

1. exact asset ordering at base-relative slots
2. co-membership across all LMSR reserve inputs
3. exact state transition and pricing checks
4. mandatory base-window guards (bounds + explicit 3-slot coverage)

So the only flexibility introduced is placement of the 3-LMSR-UTXO bundle inside a larger, multi-covenant transaction.

### 7.4 Normative index contract

To avoid redundant verification while preserving safety:

1. `IN_BASE + 0` (primary LMSR input) validates the full reserve/state transition:
   - all LMSR reserve inputs (`IN_BASE + 0..2`)
   - all LMSR reserve outputs (`OUT_BASE + 0..2`)
   - script-hash/state transition, Merkle proofs, and pricing inequalities
2. `IN_BASE + 1` and `IN_BASE + 2` (secondary LMSR inputs) validate anchored role + co-membership:
   - index relation to `IN_BASE`
   - role mapping at reserve slots: `IN_BASE + 0 = YES`, `IN_BASE + 1 = NO`, `IN_BASE + 2 = collateral`
   - own script hash equals `input_script_hash(IN_BASE)`

This keeps the primary/secondary split from current AMM structure but hardens secondary checks with explicit role mapping, and avoids triplicating the full output validation logic.

---

## 8. Spending Paths

Canonical v0.1 architecture is multi-leaf:

1. Primary leaf program:
   - witness `PATH_PRIMARY: Either<(), ()>`
   - `Left(())`: Path 1 Swap (permissionless)
   - `Right(())`: Path 2 Admin liquidity adjust (signature-gated)
2. Secondary leaf program:
   - dedicated co-membership checks only (no swap/admin logic)
   - used by non-primary LMSR reserve inputs

### 8.1 Path 1: Swap (permissionless)

Primary-leaf witness fields:

- `IN_BASE`, `OUT_BASE`
- `TRADE_KIND`: `0=BUY_YES, 1=SELL_YES, 2=BUY_NO, 3=SELL_NO`
- `OLD_S_INDEX`, `NEW_S_INDEX`
- `F_OLD`, `F_NEW`
- `OLD_PROOF[...]`, `NEW_PROOF[...]` (Merkle siblings)

Enforced checks:

1. `current_index == IN_BASE`
2. state hash check from `OLD_S_INDEX` against `input_script_hash(IN_BASE)`
3. primary input validates full LMSR reserve transition:
   - fixed asset layout checks on inputs `IN_BASE + 0..2` and outputs `OUT_BASE + 0..2`
   - exact amount deltas and destination script hash on all three reserve outputs
   - reserve input/output asset+amounts at those slots must be explicit (non-confidential)
4. Merkle proof: `(OLD_S_INDEX, F_OLD)` and `(NEW_S_INDEX, F_NEW)` under `LMSR_TABLE_ROOT`
5. bounds: both indices `<= S_MAX_INDEX`
6. `steps = abs(NEW_S_INDEX - OLD_S_INDEX)` and `x = steps * Q_STEP_LOTS`
7. reserve/state consistency by `TRADE_KIND` (exact token delta = `x`)
8. collateral inequality with fee (below)
9. minimum output reserves: `R_yes >= MIN_R_YES`, `R_no >= MIN_R_NO`, `R_collateral >= MIN_R_COLLATERAL`
10. outputs `OUT_BASE + 0..2` sent to `script_hash(NEW_S_INDEX)`
11. base-window safety:
    - compute `IN_BASE + 2` and `OUT_BASE + 2` with checked `u32` addition; fail on overflow
    - require `num_inputs >= 3` and `num_outputs >= 3` using checked subtraction for `num_inputs - 3` / `num_outputs - 3`
    - require `IN_BASE <= num_inputs - 3` and `OUT_BASE <= num_outputs - 3`

### 8.2 Path 2: Admin liquidity adjust (v0.1-enabled)

Purpose:

- add/remove inventory (partial or full)
- maintain liquidity depth over market lifetime
- graceful shutdown / migration around market resolution (subject to floors)

Checks:

1. `current_index == IN_BASE`
2. fail-closed key gate: `COSIGNER_PUBKEY != NUMS`
3. verify admin signature against `COSIGNER_PUBKEY`
4. fixed asset layout on `IN_BASE + 0..2` and `OUT_BASE + 0..2`
5. reserve input/output asset+amounts at those slots must be explicit (non-confidential)
6. `NEW_S_INDEX == OLD_S_INDEX` (no admin-driven price-state move)
7. paired token inventory rule: signed YES delta equals signed NO delta (`ΔYES == ΔNO`)
8. minimum reserve floors
9. outputs `OUT_BASE + 0..2` sent to `script_hash(NEW_S_INDEX)`
10. base-window safety matches Path 1 (checked add/sub, no wrap, in-range 3-slot coverage)

Collateral reserve may be adjusted independently; YES/NO inventories must move equally (including both zero for collateral-only operations).

### 8.2.1 Admin signature message (consensus spec)

Path 2 uses a custom domain-separated message and BIP340 verification against `COSIGNER_PUBKEY`:

```
msg = SHA256(
  "DEADCAT/LMSR_LIQUIDITY_ADJUST_V1" ||
  CHAIN_GENESIS_HASH ||
  LMSR_TABLE_ROOT ||
  YES_ASSET_ID || NO_ASSET_ID || COLLATERAL_ASSET_ID ||
  TX_SIGHASH_ALL_32 ||
  input_prevout_txid(IN_BASE + 0) ||
  be32(input_prevout_vout(IN_BASE + 0)) ||
  input_prevout_txid(IN_BASE + 1) ||
  be32(input_prevout_vout(IN_BASE + 1)) ||
  input_prevout_txid(IN_BASE + 2) ||
  be32(input_prevout_vout(IN_BASE + 2)) ||
  be32(IN_BASE) || be32(OUT_BASE) ||
  be64(OLD_S_INDEX) || be64(NEW_S_INDEX) ||
  be64(IN_YES) || be64(IN_NO) || be64(IN_COLLATERAL) ||
  be64(OUT_YES) || be64(OUT_NO) || be64(OUT_COLLATERAL) ||
  output_script_hash(OUT_BASE + 0) ||
  output_script_hash(OUT_BASE + 1) ||
  output_script_hash(OUT_BASE + 2)
)
```

Encoding rules:

1. all integers are big-endian fixed-width (`be32`, `be64`)
2. txid and script-hash values are raw 32-byte values from introspection jets
3. asset IDs, table root, genesis hash, and `TX_SIGHASH_ALL_32` are raw 32-byte values
4. `TX_SIGHASH_ALL_32` is the canonical current-input SIGHASH_ALL digest for this transaction (consensus-defined sighash engine)
5. ASCII domain tag is exact bytes shown above (no null terminator)

This binds authorization to the exact consumed 3-UTXO LMSR reserve bundle and exact new reserve bundle.

For v0.1, this exact preimage and encoding are consensus-critical. Any format change requires a new version/domain tag and an explicit spec revision.
Under Path 2, `OLD_S_INDEX == NEW_S_INDEX` is additionally required by consensus checks.

Public-mempool safety note (v0.1):

- Path 2 signatures bind full-transaction context via `TX_SIGHASH_ALL_32` in addition to reserve-specific fields.
- Third-party attempts to redirect non-reserve outputs or mutate carrier structure invalidate the signature.
- Fee/change modification requires re-signing by the admin key.

### 8.2.2 `TX_SIGHASH_ALL_32` prerequisite and conformance (v0.1)

`TX_SIGHASH_ALL_32` is consensus-critical in this spec. Before Path 2 can be enabled in production:

1. the exact introspection source for this 32-byte digest must be implemented and documented in the contract/witness interface
2. SDK signer serialization and on-chain verification must use one canonical byte encoding
3. conformance vectors must prove byte-identical digest derivation across implementations

Fail-closed deployment rule:

- if a deployment cannot satisfy the above for `TX_SIGHASH_ALL_32`, Path 2 must remain disabled (`COSIGNER_PUBKEY = NUMS`) and only Path 1/Path 3 are usable.

### 8.3 Path 3: Secondary covenant input

Secondary leaf enforces the hardened role/co-membership contract from Section 7.4 (strictly stronger than current AMM secondary checks):

- `current_index != IN_BASE`
- `current_index` must equal `IN_BASE + 1` or `IN_BASE + 2`
- role check by index: `IN_BASE + 1` must be NO asset, `IN_BASE + 2` must be collateral asset
- reserve anchor check: `IN_BASE + 0` must be YES asset
- own input script hash equals input `IN_BASE` script hash
- no need to re-validate output bundle (primary input already validates full transition)

### 8.4 Compatibility mode

If a deployment sets `IN_BASE = 0` and `OUT_BASE = 0`, behavior is equivalent to the legacy global-index style.

---

## 9. Atomic Composition With Market Covenant and Maker Orders

With base-indexed LMSR addressing and fixed-index market addressing, a single transaction can atomically compose:

1. market issuance or redemption
2. LMSR buy or sell
3. one or more maker-order fills
4. user settlement outputs

### 9.1 Compatibility model

Recommended split:

- market covenant keeps existing global index constraints
- LMSR uses `IN_BASE`/`OUT_BASE` windows for its 3-UTXO reserve bundle
- maker order contract keeps its current witness-free index rule:
  - maker receive output index equals maker input index
  - partial-fill remainder is at `i + 1`

No maker-order `OUT_BASE` witness is required.

### 9.2 Index planner requirements

The transaction builder must allocate non-colliding index windows:

1. reserve market-required fixed slots first
2. reserve LMSR `IN_BASE..IN_BASE+2` and `OUT_BASE..OUT_BASE+2`
3. place maker-order inputs such that their required maker outputs (`i`, and `i+1` when partial) do not collide with market/LMSR reserved outputs
4. treat maker-index mapping as a hard feasibility condition: if a maker input index cannot preserve its required output index relation, that route is invalid
5. for market covenant composition, enforce exactly one primary market input path; all additional market-script inputs must use market Path 7 (secondary input path)

v0.1 router constraint (current SDK behavior): at most one maker order may be partial, and it must be the last maker order in index order.

### 9.3 Composition scope constraints (v0.1)

Composition is market-path dependent. Current market covenant paths include fixed index/count assumptions that planners must preserve, including:

1. market primary input fixed at index `0` in issuance/resolve paths
2. fixed fee-output indices in issuance/resolve paths
3. hard output-count constraints on some paths (for example, pre-expiry oracle resolve enforces `num_outputs == 4`)

Path-by-path market composition matrix (current covenant):

| Market path | Key index/count constraints | LMSR+maker composition in same tx |
|------|------------------------------|-----------------------------------|
| Path 1 Initial Issuance | primary input fixed at `0`; market outputs fixed at `0..2`; fee output fixed at `5` | Allowed in v0.1 if planner preserves required market slots |
| Path 2 Subsequent Issuance | primary input fixed at `0`; market outputs fixed at `0..2`; fee output fixed at `5` | Allowed in v0.1 if planner preserves required market slots |
| Path 3 Oracle Resolve | primary input fixed at `0`; outputs fixed at `0..2`; `num_outputs == 4`; fee at `3` | Out of scope in v0.1 (no room for LMSR/maker outputs) |
| Path 4 Post-Resolution Redemption | market primary covenant semantics must be bound to absolute input `0`; collateral semantics read from input `0`; positional outputs plus fee-at-last | Conditionally allowed only when planner pins market primary at input `0` and satisfies path-specific output positions |
| Path 5 Expiry Redemption | market primary covenant semantics must be bound to absolute input `0`; collateral semantics read from input `0`; positional outputs plus fee-at-last | Conditionally allowed only when planner pins market primary at input `0` and satisfies path-specific output positions |
| Path 6 Cancellation | market primary covenant semantics must be bound to absolute input `0`; collateral semantics read from input `0`; additional fixed token/RT positions by branch plus fee-at-last | Conditionally allowed only when planner pins market primary at input `0` and satisfies path-specific output positions |
| Path 7 Secondary Input | non-primary co-membership helper path | Applicable only as companion to a valid primary market path above |

Planner invariant: for any composed market transaction, non-primary market covenant inputs MUST use market Path 7.

Hard precondition for current market covenant implementation:

- For market Paths 4/5/6, planner must pin the market primary covenant input to absolute input `0`. Those paths read collateral semantics from input `0` and do not independently assert `current_index == 0`.
- Any candidate composition that violates this must be rejected as invalid route construction.
- This is a planner-enforced safety boundary (not self-enforced by current market-path consensus checks for 4/5/6). If this planner guarantee cannot be proven in a deployment, Paths 4/5/6 co-composition must be disabled and treated as out of scope.

v0.1 planner scope is therefore:

1. issuance paths (Path 1/2)
2. redemption/cancellation paths only when their positional output contracts remain satisfiable
3. LMSR swap path
4. maker-order fills

Oracle-resolution + LMSR/maker co-composition is explicitly out of scope for v0.1 unless market path constraints are revised.
Market-linked pools must also satisfy the payout-unit binding in Section 5.1.

This is not planner-only work: current SDK assembly/signing flows assume fixed pool and market covenant indices and must be refactored to support base-indexed LMSR composition safely.

---

## 10. Swap Equations and Fee Enforcement

Let:

- `x = traded lots`
- `H = HALF_PAYOUT_SATS`
- `Fo = F_OLD`
- `Fn = F_NEW`
- `fee_c = FEE_DENOM - FEE_BPS`
- `L = x * H`

### 10.1 Signed-safe quote construction (before fee)

`F(s)` is even and not monotonic over all `s`, so `Fn - Fo` cannot be treated as sign-implied by index direction. The contract must compute with checked branches:

1. If `Fn >= Fo`:
   - `d_up = Fn - Fo`
   - buy quote: `base_cost = L + d_up`
   - sell quote: require `L >= d_up`, then `base_rebate = L - d_up`
2. If `Fo > Fn`:
   - `d_down = Fo - Fn`
   - buy quote: require `L >= d_down`, then `base_cost = L - d_down`
   - sell quote: `base_rebate = L + d_down`

Direction constraints still apply by trade kind:

- `BUY_YES`, `SELL_NO`: `NEW_S_INDEX > OLD_S_INDEX`
- `SELL_YES`, `BUY_NO`: `NEW_S_INDEX < OLD_S_INDEX`

### 10.2 Collateral delta constraints

Define:

- `collateral_in = NEW_R_COLLATERAL - OLD_R_COLLATERAL` (buy paths)
- `collateral_out = OLD_R_COLLATERAL - NEW_R_COLLATERAL` (sell paths)

Fee-on-notional constraints:

- Buy paths:
  ```
  collateral_in * fee_c >= base_cost * FEE_DENOM
  ```
- Sell paths:
  ```
  collateral_out * FEE_DENOM <= base_rebate * fee_c
  ```

This keeps all checks divisionless and biases rounding in favor of pool safety.

Rounding policy (pool-favoring, deterministic):

- table values use `floor(...)` to integer sats
- buy-side inequality uses `>=` so user must pay at least quoted notional+fee
- sell-side inequality uses `<=` so user receives at most quoted rebate after fee
- if branch math would require subtracting larger from smaller (`L < d_*`), the swap is invalid

### 10.3 Reserve consistency by trade kind

- `BUY_YES`: `YES` decreases by `x`, `NO` unchanged, `S_INDEX` increases
- `SELL_YES`: `YES` increases by `x`, `NO` unchanged, `S_INDEX` decreases
- `BUY_NO`: `NO` decreases by `x`, `YES` unchanged, `S_INDEX` decreases
- `SELL_NO`: `NO` increases by `x`, `YES` unchanged, `S_INDEX` increases

---

## 11. LMSR Table Commitment

### 11.1 Leaf encoding

All hashes are SHA256 over explicit domain-separated byte layouts.

Leaf:

```
leaf_i = SHA256(0x00 || "LMSR_TBL_V1" || be64(index) || be64(F_i))
```

Internal node:

```
node = SHA256(0x01 || left_hash_32 || right_hash_32)
```

Tree shape:

1. complete binary tree with `N = 2^TABLE_DEPTH` leaves
2. leaf indices are `0..N-1`
3. `S_MAX_INDEX = N - 1`

Root is `LMSR_TABLE_ROOT`.

The SimplicityHL script commits to this single root constant at deployment. Trades cannot switch tables or roots at runtime.

Policy note (v0.1): using a complete power-of-two tree (`S_MAX_INDEX = 2^TABLE_DEPTH - 1`) is a canonical profile choice for simpler tooling.  
Consensus-critical parts are the hash domains, byte encoding, sibling/path interpretation, and root equality checks; alternate tree/indexing schemes could be supported in a future version with explicit spec changes.

### 11.2 Proof verification

Witness provides, for both old and new leaves:

1. sibling arrays:
   - `OLD_PROOF[0..TABLE_DEPTH-1]`: `u256` hashes ordered bottom-up (`level 0` = leaf sibling)
   - `NEW_PROOF[0..TABLE_DEPTH-1]`: `u256` hashes ordered bottom-up (`level 0` = leaf sibling)
2. path-direction fields:
   - `OLD_PATH_BITS: u64`
   - `NEW_PATH_BITS: u64`
   - lower `TABLE_DEPTH` bits are significant; upper bits must be zero (`TABLE_DEPTH <= 63` by params check)

Path bit convention at level `k`:

- bit `0`: current hash is left child (`parent = H(0x01 || cur || sib[k])`)
- bit `1`: current hash is right child (`parent = H(0x01 || sib[k] || cur)`)
- bit ordering in the bitfield is LSB-first by level: bit `k` corresponds to level `k` (`k=0` at the leaf layer)
- canonical-position binding (v0.1 complete-tree profile): require
  - `OLD_PATH_BITS & ((1 << TABLE_DEPTH) - 1) == OLD_S_INDEX`
  - `NEW_PATH_BITS & ((1 << TABLE_DEPTH) - 1) == NEW_S_INDEX`

Contract recomputes each root and requires equality with `LMSR_TABLE_ROOT`.

### 11.3 Table generation rules

Off-chain generator must be deterministic and reproducible across implementations:

1. choose `b`, `U`, `Q_STEP_LOTS`, bounds
2. use canonical index mapping `s_steps(i) = (i as i64) - (S_BIAS as i64)`
3. evaluate `F(s_i)` under one canonical numeric profile:
   - binary precision: 256-bit
   - rounding mode during transcendental and arithmetic ops: round-to-nearest, ties-to-even
   - final integer projection: `F_i = floor(F(s_i))` into `u64`
4. output canonical leaf list + root + metadata manifest

Implementations may use different internal libraries only if they produce byte-identical `F_i` vectors for the same profile.
The profile id and generator version must be included in the published manifest.
The SDK must ship conformance test vectors (root + selected leaves) for each supported profile.

The manifest hash should be published in Nostr discovery data for wallet reproducibility.

### 11.4 Consensus-critical encoding summary (v0.1)

| Item | Encoding rule |
|------|----------------|
| Admin signature indices/amounts | `be32` / `be64` fixed-width |
| Table leaf fields `(index, F_i)` | `be64` / `be64` fixed-width |
| Merkle sibling witness element | `u256` (exact 32-byte hash) |
| Merkle path-bit witness fields | `u64`; lower `TABLE_DEPTH` bits significant, upper bits zero |
| Path bits vs index binding | lower `TABLE_DEPTH` bits of `PATH_BITS` must equal corresponding `S_INDEX` |
| Hash domains | exact ASCII tags and prefixes as specified |
| Merkle sibling order | bottom-up (`level 0` = leaf sibling) |
| Merkle path bits | LSB-first by level (`bit k` -> `level k`) |

These big-endian integer encodings align with existing Simplicity SHA context conventions (`sha_256_ctx_8_add_4/8`) and TapData state hashing in this codebase.

### 11.5 Depth vs granularity (practical guidance)

- table points = `2^TABLE_DEPTH`
- proof siblings per swap = `2 * TABLE_DEPTH` (old + new)
- witness bytes from siblings ~= `64 * TABLE_DEPTH`

These are primary-path sibling bytes only. Under legacy single-program SimplicityHL patterns in this codebase (witness read in `main`, per-input witness stacks), proof-sized witness fields are duplicated per covenant input. v0.1 avoids this by requiring separate primary/secondary tapleaves so proof-heavy fields remain primary-only.

Illustrative proof-size cost:

- `TABLE_DEPTH=12`: 4096 points, ~768 sibling bytes (primary-only lower bound), ~2304 bytes under current per-input duplication
- `TABLE_DEPTH=14`: 16384 points, ~896 sibling bytes (primary-only lower bound), ~2688 bytes under current per-input duplication
- `TABLE_DEPTH=16`: 65536 points, ~1024 sibling bytes (primary-only lower bound), ~3072 bytes under current per-input duplication

At typical Liquid fee rates (~`0.01 sat/vbyte`), this proof-size delta is negligible in absolute fee terms; choose depth primarily for quote granularity and execution budget, not fee minimization. A practical v0.1 range is `TABLE_DEPTH` 12-14 with a single standard table profile.

v0.1 requirement:

- LMSR must use separate primary and secondary tapleaves/programs.
  - primary leaf carries table-proof fields and full pricing/state checks
- secondary leaf carries only role/co-membership/index checks
- secondary inputs must not require table-proof witness fields

## 12. Arithmetic and Overflow

All arithmetic remains in `u64`/`u128` composed operations:

- `x = steps * Q_STEP_LOTS` uses checked multiply
- `x * HALF_PAYOUT_SATS` uses `multiply_64`
- fee inequalities use two 64x64->128 multiplications and 128-bit compare

No division, no floating-point, no on-chain transcendental math.

As in current `amm_pool.simf`, helper functions `safe_add`, `safe_subtract`, carry/borrow assertions are required.

### 12.1 Required parameter-validation matrix (SDK/generator)

Before deployment, params/table generation must enforce:

1. `0 < TABLE_DEPTH <= 63`
2. `S_MAX_INDEX = 2^TABLE_DEPTH - 1`
3. `S_BIAS <= S_MAX_INDEX`
4. `Q_STEP_LOTS > 0`
5. `FEE_BPS < FEE_DENOM`
6. `x_max = S_MAX_INDEX * Q_STEP_LOTS` fits in `u64`
7. `L_max = x_max * HALF_PAYOUT_SATS` fits in `u128`
8. `F_i` values fit in `u64`
9. `delta_f_max = max(F_i) - min(F_i)` fits in `u64`
10. `quote_max = L_max + delta_f_max` fits in `u128`
11. `quote_max * FEE_DENOM` fits in `u128`
12. signed-mapping safety for off-chain math: `S_MAX_INDEX <= i64::MAX` and `S_BIAS <= i64::MAX`
13. table-shape policy check (non-consensus): minimum at/near center (`F(S_BIAS)` is global minimum within rounding tolerance)
14. table-shape policy check (non-consensus): monotone away from center (`F_{i+1} >= F_i` for `i >= S_BIAS`, `F_{i+1} <= F_i` for `i < S_BIAS`)
15. table-shape policy check (non-consensus): discrete convexity (`F_{i+1} - 2F_i + F_{i-1} >= 0` for interior points, within rounding tolerance)
16. table-shape policy check (non-consensus): near-symmetry around bias (`|F_{S_BIAS+k} - F_{S_BIAS-k}| <= 1` for in-range `k`)
17. table-slope policy check (non-consensus): per-step absolute slope bound
    `|F_{i+1} - F_i| <= (HALF_PAYOUT_SATS * Q_STEP_LOTS) + SLOPE_TOL`
18. slope-tolerance policy parameter must be explicit in profile/manifest (canonical v0.1 profile recommends `SLOPE_TOL = 1`)
19. table index mapping policy check (non-consensus): manifest/table representation must encode a unique positional mapping `i -> F_i` for every `i in [0, S_MAX_INDEX]` (no missing/duplicate indices)

Checks 1-12 are mandatory for deployment safety. Checks 13-19 are required SDK/pool-creation policy checks (not consensus) to reject malformed tables before publication.

---

## 13. Bootstrapping

1. Generate LMSR table and `LMSR_TABLE_ROOT`.
2. Compile covenant with parameters and root.
3. Create initial pool transaction (non-covenant-validated creation), funding:
   - output 0 YES reserve
   - output 1 NO reserve
   - output 2 collateral reserve
   all to `script_hash(initial_s_index)`.
   - creation reserve outputs `0..2` MUST be explicit (non-confidential) asset+amount encodings
   - pools created with confidential reserve outputs are non-compliant in v0.1 and must be rejected by discovery/builders
4. Publish pool announcement:
   - params
   - `lmsr_pool_id` (mandatory deterministic ID; see Section 15)
   - `initial_s_index`
   - covenant CMR
   - `creation_txid` (mandatory)
   - `initial_reserve_outpoints` in canonical order `[YES, NO, collateral]` (mandatory)
   - `witness_schema_version = "DEADCAT/LMSR_WITNESS_SCHEMA_V1"` (mandatory)
   - table manifest hash

---

## 14. Transaction Templates

### 14.1 Swap: Buy YES with collateral (example: L-BTC)

Inputs:

- `IN_BASE + 0` pool YES (primary covenant input)
- `IN_BASE + 1` pool NO (secondary)
- `IN_BASE + 2` pool collateral reserve (secondary)
- other trader collateral-asset UTXOs (for example, L-BTC)

Outputs:

- `OUT_BASE + 0` new YES reserve (decreased) -> new pool address
- `OUT_BASE + 1` NO reserve (unchanged) -> new pool address
- `OUT_BASE + 2` new collateral reserve (increased) -> new pool address
- trader YES output
- N fee/change outputs

### 14.2 Swap: Sell NO for collateral (example: L-BTC)

Same structure as Section 14.1 with YES/NO and collateral deltas mirrored for the opposite trade direction.

---

## 15. Wallet Requirements

- Track `S_INDEX` for each pool (state in script hash).
- Expect address churn: each accepted LMSR state transition changes pool script hash/address (`state` is in tapdata).
- Track reserve identity as a canonical outpoint chain, not by "first matching asset at current script hash."
- Advance canonical reserve bundle only via transactions that spend all prior canonical reserve outpoints and recreate outputs `OUT_BASE + {0,1,2}`.
- Treat extra same-script UTXOs as foreign/polluting UTXOs (ignored for reserve identity and spend planning).
- Select non-colliding `IN_BASE` and `OUT_BASE` windows when composing with other covenants.
- Preserve maker-order output constraints when co-executing limit orders:
  - maker payout at output index `i` for maker input `i`
  - partial-fill remainder at `i + 1`
- Respect current router restriction: only the last maker order in a transaction may be partial.
- Fetch both old and new table proofs for proposed swaps.
- Compute `base_cost/base_rebate` from `(OLD_S_INDEX, NEW_S_INDEX, F_OLD, F_NEW)`.
- Warn on large `steps` transitions (high slippage).
- For v0.1, route YES<->NO trades as two transactions through collateral if needed.
- Verify discovery manifest hash and root match the script-committed `LMSR_TABLE_ROOT`.
- Verify `lmsr_pool_id` is canonical for announcement params and chain (rule below).
- Require discovery bootstrap anchors:
  - `creation_txid` must be present
  - canonical `initial_reserve_outpoints` (`YES, NO, collateral`) must be present
- Planner policy may default to `OUT_BASE >= IN_BASE` for readability, but this is not a consensus requirement.
- Treat reserve bundle UTXOs as explicit only (non-confidential) in builder/scanner paths.

Discovery payload additions vs current AMM:

- `lmsr_pool_id` (required canonical identity key)
- `lmsr_table_root`
- `table_depth`
- `q_step_lots`
- `s_bias`
- `s_max_index`
- `half_payout_sats`
- `current_s_index`
- table manifest hash
- `creation_txid` (required)
- `initial_reserve_outpoints` (required ordered tuple: YES, NO, collateral)
- `witness_schema_version` (required, currently `DEADCAT/LMSR_WITNESS_SCHEMA_V1`)

Canonical LMSR pool identity (v0.1):

```
LMSR_POOL_ID = SHA256(
  "DEADCAT/LMSR_POOL_ID_V1" ||
  CHAIN_GENESIS_HASH ||
  YES_ASSET_ID || NO_ASSET_ID || COLLATERAL_ASSET_ID ||
  LMSR_TABLE_ROOT ||
  be32(TABLE_DEPTH) ||
  be64(Q_STEP_LOTS) ||
  be64(S_BIAS) ||
  be64(S_MAX_INDEX) ||
  be64(HALF_PAYOUT_SATS) ||
  be32(FEE_BPS) ||
  COSIGNER_PUBKEY ||
  covenant_cmr ||
  creation_txid ||
  initial_yes_outpoint_txid || be32(initial_yes_outpoint_vout) ||
  initial_no_outpoint_txid || be32(initial_no_outpoint_vout) ||
  initial_collateral_outpoint_txid || be32(initial_collateral_outpoint_vout)
)
```

Discovery and storage must use `LMSR_POOL_ID` as the canonical replacement/dedup key (for example NIP-33 `d` tag and DB identity key).
Multiple simultaneous pools with identical economic parameters are allowed in v0.1; instance anchors above ensure unique identity.

---

## 16. Comparison vs Existing `amm_pool.simf`

| Topic | Current CPAMM | Proposed LMSR |
|------|----------------|---------------|
| Price function | 3-asset constant product | LMSR cost table |
| YES/NO coherence | external via arbitrage | internal by shared `s` state |
| On-chain math | products + wide multiply | table proofs + linear/affine arithmetic |
| LP model | reissuance LP + cubic invariant | none in v0.1 |
| Reserve UTXOs | 4 (YES, NO, collateral, RT) | 3 (YES, NO, collateral) |
| State value | `issued_LP` | `S_INDEX` |
| Indexing | global fixed slots | base-indexed slots via witness |

---

## 17. Security and Trust Model

- Swap path is permissionless and deterministic.
- Users do not trust the operator for swap pricing correctness.
- Admin liquidity path (Path 2) is specified in v0.1 and may be enabled only after the `TX_SIGHASH_ALL_32` prerequisite/conformance gate in Section 8.2.2 passes; otherwise deploy in NUMS fail-closed mode.
- When Path 2 is enabled, operator can add/remove liquidity (including progressive wind-down near resolution).
- With `NEW_S_INDEX == OLD_S_INDEX`, admin cannot directly move LMSR quote state via Path 2.
- Operator can still materially affect execution quality and capacity by changing liquidity depth; if reserve floors are low (or zero), operator may withdraw most or all liquidity.
- This remains an availability/liquidity and custody/trust assumption.
- Table commitment prevents post-deployment curve tampering.
- Base-index flexibility is constrained by strict per-slot asset, script-hash, and state-transition checks.

---

## 18. Decision Log (v0.1, Informative)

This section records the key v0.1 design choices and why they were chosen. Consensus-normative behavior is defined in Sections 7-12, and implementation MUST-checks are consolidated in Section 20.

1. Admin path is supported, but only enabled after `TX_SIGHASH_ALL_32` conformance is proven; otherwise NUMS fail-closed.
2. Admin cannot recenter price state (`NEW_S_INDEX == OLD_S_INDEX`), and token inventory edits must remain paired (`ΔYES == ΔNO`).
3. Operator-funded liquidity is used in v0.1; permissionless LP-share mechanics are deferred to avoid covenant/state complexity.
4. Swap surface is collateral-leg only; direct YES<->NO path is deferred for a smaller, easier-to-audit first release.
5. Table-committed LMSR is canonical (`LMSR_TABLE_ROOT` + deterministic generator/profile) to keep on-chain math division-free and reproducible.
6. Canonical big-endian encodings and fixed Merkle proof semantics are mandatory to avoid cross-implementation mismatches.
7. Base-indexed LMSR reserve windows are allowed for atomic composition, with hardened secondary role checks anchored to `IN_BASE`.
8. Market composition is path-scoped: issuance is supported; oracle resolve co-composition is out of scope; Paths 4/5/6 require strict planner pinning at market input `0` or are disabled.
9. Maker semantics remain unchanged in v0.1; current router behavior keeps at most one partial fill and only as the last maker order.
10. Market-linked pools bind payout units (`HALF_PAYOUT_SATS == market.COLLATERAL_PER_TOKEN`), and reserve amounts are explicit-only to avoid unspendable states.

---

## 19. Implementation Plan (Incremental)

1. Add deterministic table generator crate and manifest format.
2. Add new `contract/lmsr_pool.simf` with:
   - primary-leaf path dispatch (swap/admin) plus dedicated secondary-leaf program
   - `IN_BASE`/`OUT_BASE` indexed reserve access helpers
   - Merkle proof verifier
   - swap equations above
   - tapdata state transition on `S_INDEX`
   - separate primary/secondary tapleaves so proof-heavy witnesses are only required on the primary input
   - taproot-layer support for multi-simplicity-leaf trees (primary leaf + secondary leaf + tapdata leaf)
3. Refactor fixed-index and LP-era assumptions in existing SDK flows:
   - `src-tauri/crates/deadcat-sdk/src/taproot.rs`: generalize scriptPubKey/control-block construction beyond single Simplicity leaf (variable-depth tree and control blocks)
   - `src-tauri/crates/deadcat-sdk/src/amm_pool/contract.rs` (and LMSR equivalents): expose leaf-specific control block/program selection APIs for primary vs secondary leaves
   - `src-tauri/crates/deadcat-sdk/src/trade/router.rs`: replace CPAMM spot/quote integration (`amm_pool::math`) with LMSR quote path and mixed-liquidity routing hooks
   - `src-tauri/crates/deadcat-sdk/src/trade/types.rs`: add LMSR pool leg types/state (`S_INDEX`, 3-UTXO reserve model) alongside existing AMM-era structs
   - `src-tauri/crates/deadcat-sdk/src/trade/convert.rs`: support LMSR discovery/announcement conversion instead of AMM LP-only pool parsing
   - `src-tauri/crates/deadcat-sdk/src/trade/pset.rs`: remove hardcoded pool input/output slot assumptions (`0..3`)
   - `src-tauri/crates/deadcat-sdk/src/amm_pool/assembly.rs`: replace current fixed primary/secondary witness attachment assumptions with base-indexed LMSR attachment and separate witness sets for primary vs secondary leaves
   - `src-tauri/crates/deadcat-sdk/src/prediction_market/assembly.rs`: preserve market fixed-index path constraints while composing with non-zero LMSR base windows
   - `src-tauri/crates/deadcat-sdk/src/amm_pool/chain_walk.rs`: replace LP-era fixed-slot walk with LMSR canonical 3-outpoint bundle tracking (no asset-first ambiguity)
   - `src-tauri/crates/deadcat-sdk/src/sdk.rs`: replace asset-first reserve scanning with canonical outpoint-lineage selection for LMSR pools
   - `src-tauri/crates/deadcat-sdk/src/node.rs`: watcher/sync and pool-state refresh must be keyed by LMSR canonical bundle/state-hash churn, not LP-only `issued_lp` assumptions
   - `src-tauri/crates/deadcat-sdk/src/node.rs` quote/execution path: remove "first discovered pool" and legacy scan assumptions; resolve pool and reserves by canonical bundle identity
   - `src-tauri/crates/deadcat-sdk/src/discovery/pool.rs`: add LMSR discovery schema (table root/manifest + canonical bundle identity fields), separate from AMM LP announcement shape
   - `src-tauri/crates/deadcat-sdk/src/discovery/service.rs` and `src-tauri/crates/deadcat-sdk/src/discovery/store_trait.rs`: add LMSR-specific ingest/update interfaces and canonical-bundle state updates
   - `src-tauri/crates/deadcat-sdk/deadcat-store/src/schema.rs`: add LMSR pool/state tables keyed for canonical bundle lineage; do not overload AMM `issued_lp` schema
   - discovery/bootstrap contract: require `lmsr_pool_id`, `creation_txid`, `initial_reserve_outpoints`, and `witness_schema_version` at announcement ingest time; reject announcements missing any
   - discovery replacement/dedup: derive and validate `lmsr_pool_id` canonically; use as NIP-33 identity key and store identity key
   - canonical witness decoder support: implement `DEADCAT/LMSR_WITNESS_SCHEMA_V1` parser + conformance vectors for `OUT_BASE` extraction

These watcher/discovery/store/schema items are mandatory deliverables for v0.1 LMSR support, not optional follow-on cleanup.
4. Add SDK modules mirroring existing AMM structure:
   - `lmsr_pool/params.rs`
   - `lmsr_pool/math.rs`
   - `lmsr_pool/pset/swap.rs`
5. Add router/planner acceptance guards matching Section 9.3 path matrix:
   - reject market path 3 (oracle resolve) composition attempts with LMSR/maker legs
   - reject any candidate plan that violates path-specific market output positioning/count constraints
   - reject any candidate use of market Paths 4/5/6 where market primary covenant input is not absolute input `0`
   - reject plan if maker index/output constraints cannot be satisfied
   - reject plan where any non-primary market covenant input is not assigned market Path 7
6. Add release blockers (must pass before LMSR mainline enablement):
   - no asset-first reserve scanning remains in LMSR paths (`sdk.rs` and related callsites)
   - canonical-bundle tracker is the sole reserve-identity source in scanner/watcher/discovery
   - canonical tracker derives `OUT_BASE` from primary witness decode (no alternate-window ambiguity heuristic)
   - builder/assembler no longer assumes pool slots `0..3`
   - quote/execution no longer relies on first-discovered-pool selection
   - discovery/bootstrap path rejects pools without canonical anchor fields (`lmsr_pool_id`, `creation_txid`, ordered initial reserve outpoints, witness schema version)
   - separate primary/secondary tapleaves are deployed and exercised in tests
   - LMSR deployment path has no legacy reserve-tracking fallback mode (fail closed on canonical-tracking errors)
   - `TX_SIGHASH_ALL_32` source/encoding is implemented with canonical serialization and verified by cross-implementation vectors before enabling Path 2
   - if market Paths 4/5/6 co-composition is enabled, planner tests must prove primary-market-input-at-0 pinning; otherwise 4/5/6 co-composition remains disabled
7. Add test vectors and integration tests:
   - proof verification
   - signed-delta branches around `s=0` (`Fn >= Fo` and `Fo > Fn`)
   - fee inequality boundaries
   - min reserve guards
   - admin signature message serialization/verification
   - admin signature redirection-resistance in public mempool model (mutated non-reserve outputs must fail)
   - `TX_SIGHASH_ALL_32` conformance vectors (same tx -> same digest across implementations)
   - table-slope bound acceptance/rejection at deployment-time validation
   - taproot control-block vectors for each LMSR leaf spend:
     - primary leaf spend control block and merkle sibling path
     - secondary leaf spend control block and merkle sibling path
     - tapdata leaf control block and merkle sibling path
   - canonical-bundle anti-pollution cases:
     - extra same-script foreign UTXO injection (must not affect canonical selection)
     - multiple matching output windows plus valid primary witness (`OUT_BASE`-driven canonical selection must still advance correctly)
     - missing or unparseable primary witness for canonical transition (must halt sync, not guess)
   - witness parser conformance vectors for `DEADCAT/LMSR_WITNESS_SCHEMA_V1` (`OUT_BASE`, state fields)

---

## 20. Normative Checklist (v0.1)

This appendix is the single implementation checklist for audits. It mirrors normative requirements in Sections 7-12 and required policy/deliverables in Sections 15 and 19.

### 20.1 Consensus MUSTs

1. Path 1 and Path 2 enforce checked base-window bounds (`IN_BASE/OUT_BASE` no wrap, in-range 3-slot coverage).
2. Path 1 enforces explicit reserve amounts, role ordering, Merkle proof validity, pricing inequalities, and reserve floors.
3. Path 2 enforces fail-closed NUMS gate, signature validity, `NEW_S_INDEX == OLD_S_INDEX`, paired token delta rule, reserve floors.
4. Path 2 signature preimage uses canonical v0.1 encoding and binds prevouts `IN_BASE+0..2`, amounts, indices, and reserve-output script hashes.
5. Path 2 signature preimage also binds chain/deployment identity (`CHAIN_GENESIS_HASH`, `LMSR_TABLE_ROOT`, asset IDs) and full-transaction context (`TX_SIGHASH_ALL_32`).
6. Taproot commitment uses the canonical binary tree shape from Section 6 (`((primary, secondary), tapdata)` with sorted tapbranch hashing).
7. Secondary path enforces role/co-membership anchor to `IN_BASE`.
8. Merkle encodings/domains/bit ordering/witness typing follow Section 11.4 exactly.

### 20.2 Wallet/SDK Policy MUSTs

1. Reserve identity is canonical outpoint lineage, never first-match-by-asset.
2. Canonical transition derives `OUT_BASE` from primary witness decode; no alternate-window guessing is allowed.
3. Reserve outputs in LMSR bundle are explicit (non-confidential).
4. Bootstrap reserve outputs (`creation_txid` outputs `0..2` in canonical YES/NO/collateral order) are explicit (non-confidential) asset+amount encodings.
5. Table generation follows canonical profile and conformance vectors.
6. Table-shape checks (including slope bound, unique index mapping, and manifest tolerance parameter) pass before publication.
7. Discovery ingest requires canonical bootstrap anchors (`creation_txid`, ordered `initial_reserve_outpoints`).
8. Discovery identity key uses canonical `LMSR_POOL_ID` derivation (including creation anchors) and NIP-33 replacement keying.
9. Witness decode for canonical tracking uses `DEADCAT/LMSR_WITNESS_SCHEMA_V1` parser rules and conformance vectors.
10. Section 9.3 composition matrix is enforced by router/planner acceptance checks.
11. Non-primary market covenant inputs are always assigned market Path 7 in composed transactions.
12. For market Paths 4/5/6 composition, planner pins market primary covenant input at absolute input `0`; otherwise route is rejected.
13. LMSR reserve tracking has no legacy compatibility fallback (asset-first/first-pool/alternate-window guessing).
14. If market Paths 4/5/6 co-composition is enabled, planner guardrails proving primary-market-input-at-0 are mandatory; otherwise 4/5/6 co-composition stays disabled.

### 20.3 v0.1 Deliverable MUSTs

1. Taproot helper/contract layer refactor supports multi-leaf LMSR spends (primary + secondary + tapdata).
2. Base-index LMSR builder/assembler refactor removes fixed `0..3` pool-slot assumptions.
3. Scanner/watcher/discovery/store/quote execution migration is complete for canonical bundle lineage and state-churn tracking.
4. Primary/secondary tapleaf split is implemented so proof-heavy witnesses are primary-only.
5. Anti-pollution and ambiguity integration tests are present and passing.
6. Taproot control-block conformance vectors for primary/secondary/tapdata branches are published and passing.
7. `TX_SIGHASH_ALL_32` derivation/encoding conformance is proven before Path 2 is enabled (otherwise NUMS fail-closed mode remains).
