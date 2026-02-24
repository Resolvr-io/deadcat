# Three-Asset AMM Pool Covenant

**Design Document**

SimplicityHL on Liquid · Resolvr Inc. · February 2026

**DRAFT v1.0 — Reissuance LP with Tapdata State**

---

## 1. Overview

This document specifies a constant-product automated market maker (AMM) implemented as a SimplicityHL covenant on Liquid. The pool holds three trading assets—YES tokens, NO tokens, and L-BTC—plus the LP token's reissuance token, enabling traders to swap any pair in a single atomic transaction and LPs to deposit/withdraw reserves permissionlessly. LP tokens are minted on demand via reissuance (no pre-defined supply ceiling); the covenant tracks the cumulative issued LP count as tapdata state embedded in the scriptPubKey, following the Astrolabe pattern.

The design is purpose-built for binary prediction markets but is mechanically a generic three-asset constant-product AMM with a cubic LP model. The pool covenant does not interact with the market covenant; external arbitrageurs maintain price alignment between the two.

## 2. Goals

- **Atomic collateral-to-token swaps.** A user can buy YES tokens with L-BTC (or any other pair) in a single transaction.
- **Divisionless on-chain logic.** All covenant arithmetic uses multiplication and comparison only—no division, no rounding.
- **Permissionless swaps.** Anyone can trade against the pool without a signature or identity.
- **Permissionless LP participation.** Anyone can deposit reserves and receive LP tokens, or burn LP tokens to withdraw. The cubic invariant ensures fairness without trusted intermediaries.
- **Fee enforcement.** The covenant enforces a minimum swap fee, accruing value to LP token holders automatically.
- **Modular.** The pool covenant is independent of the market covenant. Multiple pools (different fees, different creators) can serve the same market.

### 2.1 Non-goals

- Concentrated liquidity or custom bonding curves.
- Cross-covenant interaction with the market covenant (arbitrage is external).
- Full Nostr discovery protocol specification (§21 provides a preliminary sketch; a complete protocol spec is future work).

---

## 3. Three-Asset Constant Product Invariant

The pool maintains reserves of three assets: YES tokens (R_yes), NO tokens (R_no), and L-BTC (R_lbtc). The invariant is:

```
R_yes * R_no * R_lbtc = k
```

On every swap the covenant enforces that the product does not decrease (and in fact increases by at least the fee). This is a direct generalization of the Uniswap `x * y = k` model to three assets.

### 3.1 Pairwise Reduction

For any single-pair swap (one reserve increases, one decreases, one unchanged), the unchanged reserve cancels from both sides of the invariant check. A swap of L-BTC for YES tokens (NO reserve unchanged):

```
(R_yes - Δ_out) * R_no * (R_lbtc + Δ_in) >= R_yes * R_no * R_lbtc
                  ^^^^^                                ^^^^^
                cancels                              cancels

→  (R_yes - Δ_out) * (R_lbtc + Δ_in) >= R_yes * R_lbtc
```

Each pairwise swap reduces to a standard two-asset CPMM between the changing reserves. Pricing behavior is well-understood: the pool is effectively three overlapping two-asset pools sharing liquidity.

### 3.2 Spot Prices (Computed Off-Chain)

The covenant never computes prices. Off-chain software derives spot prices for display:

| Swap | Spot price |
|------|-----------|
| YES in L-BTC terms | R_lbtc / R_yes |
| NO in L-BTC terms | R_lbtc / R_no |
| YES in NO terms | R_no / R_yes |

These are used by wallet UIs and taker software to compute trade amounts. The covenant only checks the product invariant.

---

## 4. Contract Parameters

The covenant is parameterized at compile time with seven values.

| Parameter | Description |
|-----------|-------------|
| `YES_ASSET_ID` | Asset ID of the YES outcome token. Must match the market's YES token. |
| `NO_ASSET_ID` | Asset ID of the NO outcome token. Must match the market's NO token. |
| `LBTC_ASSET_ID` | Asset ID of L-BTC (the collateral/quote asset). |
| `LP_ASSET_ID` | Asset ID of the LP token. Represents pro-rata pool ownership. |
| `LP_REISSUANCE_TOKEN_ID` | Asset ID of the LP token's reissuance token. The pool holds this to mint new LP tokens on demand. |
| `FEE_BPS` | Swap fee in basis points (e.g., 30 = 0.30%). Baked into the covenant; cannot be changed after deployment. |
| `COSIGNER_PUBKEY` | Optional cosigner pubkey. Set to NUMS key bytes to disable. |

**Fixed constant (compile-time):**

- `FEE_DENOM = 10000` — Hardcoded into the contract as a constant, not as a parameter. This is what makes `FEE_BPS` act as basis points, since a basis point is one 10,000th of 100%.

### 4.1 Design Decision: Fee as Compile-Time Constant

A compile-time constant was chosen over a witness or state value because: (a) simplest to validate, (b) transparent to anyone inspecting the covenant, and (c) different fee tiers are supported by deploying multiple pools (the Uniswap v3 pattern).

---

## 5. Pool UTXO Layout

The pool consists of four UTXOs at the same covenant address:

| Input/Output Index | Asset | Contents |
|---|---|---|
| 0 | YES token | YES reserve (R_yes) |
| 1 | NO token | NO reserve (R_no) |
| 2 | L-BTC | L-BTC reserve (R_lbtc) |
| 3 | LP reissuance token | Reissuance token for minting LP tokens (confidential, amount = 1) |

All four UTXOs share the same covenant scriptPubKey. On every spending transaction, all four are consumed and recreated with updated amounts (reserves change; reissuance token cycles through). The fixed index layout means the covenant always knows which input/output holds which asset without dynamic discovery.

### 5.1 Reissuance LP Token Model

LP tokens are minted on demand. The pool holds the LP token's reissuance token at index 3. The cumulative issued LP count (`issued_LP`) is tracked as tapdata state embedded in the pool's scriptPubKey (see §5.3).

- **LP deposit:** The depositor's transaction spends the reissuance token (input 3) with an `issuance_value_amount` set to mint new LP tokens. The freshly minted LP tokens go to the depositor. The reissuance token is cycled back to the pool (output 3). The state transitions from `issued_LP` to `issued_LP + minted_amount`.
- **LP withdraw:** The LP holder burns LP tokens (sent to an OP_RETURN output). No reissuance occurs—the reissuance token passes through unchanged. The state transitions from `issued_LP` to `issued_LP - burned_amount`.
- **Swap:** No LP tokens are minted or burned. The reissuance token passes through unchanged. The state is unchanged (pool address does not change).

### 5.2 Design Decision: Fixed Index Layout

An alternative is dynamic asset discovery (covenant reads each input's asset ID and matches against parameters). Fixed indices are simpler: one comparison per input to verify the expected asset ID, with no loops or sorting logic. The cost is that transaction construction must respect the layout, which is a trivial wallet-side constraint.

### 5.3 Tapdata State Model (Astrolabe Pattern)

The pool tracks `issued_LP` (a u64 count of cumulative outstanding LP tokens) as state committed into the scriptPubKey via tapdata. This follows the same pattern used by the prediction market's Astrolabe contract for tracking cumulative issuance.

**How it works:**

The covenant script hash is computed as a function of the state:

```
script_hash(issued_LP) = P2TR(NUMS, tapbranch(tapleaf, tapdata(issued_LP)))
```

Where:
- `tapleaf` = the covenant's own tapleaf hash (`jet::tapleaf_hash()`)
- `tapdata(issued_LP)` = SHA256 of the state value, initialized via `jet::tapdata_init()`, extended with `jet::sha_256_ctx_8_add_8(ctx, issued_LP)`, and finalized to produce the leaf hash
- `tapbranch` = `jet::build_tapbranch(tapleaf, tapdata_leaf)`
- The final tweaked key = `jet::build_taptweak(NUMS_key, tapbranch)`
- The script hash = P2TR encoding of the tweaked key

**State verification:** On every spend, the covenant reads `issued_LP` from the witness and verifies:

```
script_hash(witness::ISSUED_LP) == input_script_hash(0)
```

If this check passes, the witness value is the true state—the covenant address itself proves it.

**State transitions:**

| Path | State change |
|------|-------------|
| Swap | None — `issued_LP` unchanged, output address = input address |
| LP deposit | `new_issued_LP = old_issued_LP + minted` — output address changes |
| LP withdraw | `new_issued_LP = old_issued_LP - burned` — output address changes |

On the LP deposit/withdraw paths, the covenant computes the new state, derives the new script hash, and enforces that all four outputs are sent to `script_hash(new_issued_LP)`. On the swap path, the covenant enforces that all four outputs are sent to the same address as the inputs (state unchanged).

**Implication: Pool address changes on LP operations.** Every deposit or withdrawal produces a new pool address (because `issued_LP` changes). Swaps do not change the address. Wallet software and Nostr discovery must track the current pool address.

### 5.4 Reissuance Token Verification (Pedersen Commitments)

The reissuance token at index 3 may be confidential (blinded asset and amount). The covenant verifies it using Pedersen commitment checks, following the Astrolabe pattern:

```
verify_token_commitment(actual_asset, actual_amount, LP_REISSUANCE_TOKEN_ID, abf, vbf):
  - Compute expected asset generator: hash_to_curve(LP_REISSUANCE_TOKEN_ID) + abf * G
  - Compute expected amount commitment: amount * expected_generator + vbf * H
  - Assert both match the actual confidential values from the input/output
```

The blinding factors (`abf`, `vbf`) are provided as witness values. See §18.8 for the full implementation.

---

## 6. Spending Paths

The covenant has three spending paths, selected via a nested `Either` type in the witness. All paths are fully trustless—no signature or privileged key can bypass the invariant checks.

### 6.1 Swap (Permissionless)

Any two of the three trading reserves (YES, NO, L-BTC) change; one remains unchanged. No LP tokens minted or burned. No signature required. State (`issued_LP`) is unchanged.

**Witness values:**

- `swap_pair`: which pair is being swapped (0 = YES↔NO, 1 = YES↔LBTC, 2 = NO↔LBTC).
- `ISSUED_LP`: current issued LP count (for state verification).
- `INPUT_ABF`, `INPUT_VBF`, `OUTPUT_ABF`, `OUTPUT_VBF`: blinding factors for reissuance token verification (§5.4).

**Preconditions:**

- Current input index is 0 (primary covenant input).

**Enforced constraints:**

1. **State verification.** Verify `script_hash(witness::ISSUED_LP) == input_script_hash(0)` (§5.3).

2. **Input layout verification.**
   - Input 0: asset == `YES_ASSET_ID`.
   - Input 1: asset == `NO_ASSET_ID`, script hash == own script hash.
   - Input 2: asset == `LBTC_ASSET_ID`, script hash == own script hash.
   - Input 3: reissuance token (verified via Pedersen commitment, see §5.4), script hash == own script hash.

3. **Output layout verification.**
   - Output 0: asset == `YES_ASSET_ID`, spk == `script_hash(witness::ISSUED_LP)` (same state).
   - Output 1: asset == `NO_ASSET_ID`, spk == same.
   - Output 2: asset == `LBTC_ASSET_ID`, spk == same.
   - Output 3: reissuance token (verified), spk == same.

4. **Unchanged trading reserve verification.** Based on `swap_pair`:
   - If `swap_pair == 0` (YES↔NO): output 2 amount == input 2 amount (L-BTC unchanged).
   - If `swap_pair == 1` (YES↔LBTC): output 1 amount == input 1 amount (NO unchanged).
   - If `swap_pair == 2` (NO↔LBTC): output 0 amount == input 0 amount (YES unchanged).

5. **Invariant with fee (see §7).** Applied to the two changing reserves.

6. **Minimum reserve.** All three trading output reserves must be > 0 (prevents draining a reserve to zero, which would break the invariant permanently).

#### Swap Pair Dispatch

The covenant branches on `swap_pair` to select the two changing reserves. The `swap_pair` witness is validated against the actual reserve changes (the covenant asserts the indicated unchanged reserve truly did not change). A dishonest `swap_pair` value causes the unchanged-reserve assertion to fail. Within each pair, the covenant determines the swap direction at runtime — which reserve decreased (output) and which increased (input) — and passes them to the fee formula in the correct order (see §7, §18.2).

### 6.2 LP Deposit / Withdraw (Permissionless, Cubic-Checked)

Anyone can deposit reserves and receive LP tokens (minted via reissuance), or burn LP tokens to withdraw reserves. The covenant enforces that `product / issued_LP^3` does not decrease (§13). The pool address transitions to reflect the new `issued_LP` state.

**Witness values:**

- `ISSUED_LP`: current issued LP count (for state verification).
- `INPUT_ABF`, `INPUT_VBF`, `OUTPUT_ABF`, `OUTPUT_VBF`: blinding factors for reissuance token verification (§5.4).

**Preconditions:**

- Current input index is 0 (primary covenant input).

**Enforced constraints:**

1. **State verification.** Verify `script_hash(witness::ISSUED_LP) == input_script_hash(0)` (§5.3).

2. **Input/output layout verification.** Same asset checks as swap path (inputs 0–3, outputs 0–3 with correct asset IDs). The reissuance token at index 3 is verified via Pedersen commitment on both input and output.

3. **Compute new issued LP.** The covenant determines `new_issued_LP` from the transaction:
   - **Deposit (minting):** The reissuance token at input 3 triggers issuance. The covenant reads the minted amount via `jet::issuance_asset_amount(3)`, which returns the explicit amount of LP tokens minted. `new_issued_LP = old_issued_LP + minted_amount`.
   - **Withdraw (burning):** LP tokens are burned by sending them to a provably unspendable OP_RETURN output (output 4). The covenant verifies the output is unspendable by checking its script hash against `SHA256("") = 0xe3b0c44...`, then reads the burn amount from `jet::output_amount(4)`. `new_issued_LP = old_issued_LP - burned_amount`.
   - Exactly one of minting or burning must occur (enforced by assertion). The covenant does not branch on direction — it computes `new_issued_LP = old - burned + minted` and applies the cubic check.

4. **Output addresses use new state.** All four outputs must be sent to `script_hash(new_issued_LP)`.

5. **Cubic invariant check (see §13).** The core LP fairness constraint:

   ```
   new_issued_LP^3 * old_product <= old_issued_LP^3 * new_product
   ```

   Where:
   - `old_product = old_R_yes * old_R_no * old_R_lbtc`
   - `new_product = new_R_yes * new_R_no * new_R_lbtc`
   - `old_issued_LP` = verified from witness via state check
   - `new_issued_LP` = computed from minting/burning amounts

   This ensures no LP can extract more value than their proportional share.

6. **Minimum reserve.** All three trading output reserves must be > 0.

7. **Minimum issued LP.** `new_issued_LP > 0` (prevents withdrawing all LP tokens).

This path enables:
- **Deposit (any combination of assets):** LP adds reserves and receives freshly minted LP tokens. The cubic check ensures they receive at most their fair share.
- **Withdraw (any combination of assets):** LP burns LP tokens and receives reserves. The cubic check ensures they extract at most their fair share.
- **Single-sided deposit/withdraw:** Only one reserve changes. Price impact naturally penalizes unbalanced operations, benefiting remaining LPs.

### 6.3 Secondary Covenant Input

Validates that secondary inputs (indices 1, 2, and 3) belong to the same covenant instance.

**Enforced constraints:**

- Own script hash == input 0's script hash.
- Own input index != 0.

This is identical to spending path 7 of the market covenant. The primary input (index 0) runs path 1 or 2 and enforces all transaction-level constraints. Secondary inputs only verify co-membership.

### 6.4 Design Decision: Cubic LP Check vs. Proportional-Only

An alternative LP model requires exactly proportional deposits (equal percentage increase in all three reserves). The cubic model was chosen because: (a) it permits single-sided and arbitrary-combination deposits/withdrawals, which are far more user-friendly, (b) price impact naturally penalizes unbalanced operations—depositors get fewer LP tokens per unit, benefiting remaining LPs, and (c) fee accrual is automatic—swap fees grow the product while LP supply stays constant, so the `product / LP^3` ratio increases and LP tokens appreciate.

### 6.5 Design Decision: No Privileged Withdraw Path

An earlier design included a creator signature path that could bypass the cubic check for "emergency" withdrawals. This was removed because it fundamentally breaks the trustless property of the pool: any LP depositing reserves would be trusting the creator not to rug-pull. The cubic LP check is the sole mechanism protecting LP deposits, and it must apply to everyone equally—including the pool creator. The creator withdraws like any other LP, by burning LP tokens through the cubic-checked path.

---

## 7. Fee Enforcement (No Division)

The fee is enforced using the Uniswap v2 approach adapted for divisionless arithmetic. For a swap where reserve A decreases (trader receives tokens) and reserve B increases (trader deposits tokens):

**Standard formula (requires division):**

```
effective_input = Δ_in * (FEE_DENOM - FEE_BPS) / FEE_DENOM
(old_R_A - Δ_out) * (old_R_B + effective_input) >= old_R_A * old_R_B
```

**Covenant formula (multiplication only):**

Cross-multiplying to eliminate the division:

```
new_R_A * (new_R_B * (FEE_DENOM - FEE_BPS) + old_R_B * FEE_BPS) >= old_R_A * old_R_B * FEE_DENOM
```

Where:
- `old_R_A`, `old_R_B` = reserves before swap (read from inputs)
- `new_R_A`, `new_R_B` = reserves after swap (read from outputs)
- `FEE_DENOM = 10000`, `FEE_BPS` = fee in basis points (compile-time constants)

This is algebraically equivalent to the Uniswap v2 invariant with fees. The fee portion of each swap stays in the pool, growing `k` over time. LP token holders capture fees automatically: as `k` grows, the `product / issued_LP^3` ratio increases, meaning each LP token represents a larger share of reserves (see §13.5).

### 7.1 Concrete Example

Pool state: R_yes = 1,000,000, R_lbtc = 500,000. Fee = 30 bps.

Trader swaps 10,000 sats L-BTC for YES tokens. Off-chain calculation:

```
effective_input = 10,000 * 9970 / 10000 = 9,970 sats
new_R_lbtc = 510,000 (total deposited, including fee)
new_R_yes = 1,000,000 * 500,000 / (500,000 + 9,970) ≈ 980,450
Δ_yes_out = 1,000,000 - 980,450 = 19,550 tokens
```

Covenant check (NO unchanged, so only YES↔LBTC pair matters):

```
980,450 * (510,000 * 9,970 + 500,000 * 30) >= 1,000,000 * 500,000 * 10,000

LHS: 980,450 * (5,084,700,000 + 15,000,000) = 980,450 * 5,099,700,000 ≈ 5.000 * 10^15
RHS: 5,000,000,000,000,000 = 5.0 * 10^15

LHS >= RHS ✓
```

### 7.2 Why Not Just Check product >= old_product?

A simpler check—`new_R_A * new_R_B >= old_R_A * old_R_B`—would allow zero-fee swaps. A sophisticated taker could extract the exact theoretical zero-fee output, leaving `k` unchanged. LPs earn nothing. The Uniswap-style formula guarantees a minimum fee on every swap, regardless of the taker's software.

---

## 8. Arithmetic Precision and Overflow

### 8.1 Swap Fee Check: 128-bit Sufficient

The largest intermediate value in the fee check is:

```
new_R_A * (new_R_B * (FEE_DENOM - FEE_BPS) + old_R_B * FEE_BPS)
```

Worst case (large pool, 1,000 BTC per reserve):

| Term | Max Value |
|------|-----------|
| new_R_B * (FEE_DENOM - FEE_BPS) | 10^11 * 10^4 = 10^15 |
| old_R_B * FEE_BPS | 10^11 * 30 = 3 * 10^12 |
| Inner sum | ≈ 10^15 |
| new_R_A * inner sum | 10^11 * 10^15 = 10^26 |

128-bit unsigned integer max: ≈ 3.4 * 10^38. Headroom of 10^12. Even a 10,000 BTC pool (≈ $1B) stays within bounds.

The right-hand side `old_R_A * old_R_B * FEE_DENOM` is at most 10^11 * 10^11 * 10^4 = 10^26—same order. 128-bit arithmetic is sufficient for the fee check at any plausible pool size.

### 8.2 Cubic LP Check: Composed Wide Arithmetic Required

The cubic LP check compares:

```
LHS: new_issued_LP^3 * old_product  (LP^3 * R_yes * R_no * R_lbtc)
RHS: old_issued_LP^3 * new_product  (LP^3 * R_yes * R_no * R_lbtc)
```

Each side is a product of six u64 values. Worst case: all values near 10^11 (100 billion sats ≈ 1,000 BTC per reserve, LP supply on similar order):

```
(10^11)^6 = 10^66
```

This far exceeds 128-bit (≈ 3.4 * 10^38). The covenant needs composed wide arithmetic.

**Single-sided optimization.** When only one reserve changes (and LP supply changes), the unchanged reserves cancel from both sides. For a single-sided deposit changing only R_yes:

```
LHS: new_LP^3 * old_R_yes * (old_R_no * old_R_lbtc)
RHS: old_LP^3 * new_R_yes * (old_R_no * old_R_lbtc)
                              ^^^^^^^^^^^^^^^^^^^^^^^^
                              cancels from both sides

→  new_LP^3 * old_R_yes <= old_LP^3 * new_R_yes
```

This is four u64 multiplications → max ≈ (10^11)^4 = 10^44, which still exceeds 128-bit but is smaller than the general case.

### 8.3 SimplicityHL Wide Multiplication Primitives

SimplicityHL provides `jet::multiply_64(u64, u64) -> u128` as the core multiplication primitive. There is no `jet::multiply_128`. Wider arithmetic is composed using schoolbook multi-word multiplication.

#### Building Blocks

**mul_128x64: u128 × u64 → u192**

Decompose the u128 as `(high, low)` where both are u64:

```
u128_val = high * 2^64 + low

u128_val * c = high * c * 2^64 + low * c
```

Each partial product (`high * c`, `low * c`) is computed by `jet::multiply_64` producing a u128. Then add the two u128 results with a 64-bit shift, producing a u192 (three u64 limbs).

Cost: 2 × `multiply_64` + 2 × `add_64` (with carry propagation).

**mul_192x64: u192 × u64 → u256**

Same approach: decompose the u192 as three u64 limbs `(w2, w1, w0)`:

```
u192_val = w2 * 2^128 + w1 * 2^64 + w0

u192_val * d = w2 * d * 2^128 + w1 * d * 2^64 + w0 * d
```

Three calls to `multiply_64`, then add with carries across four u64 limbs.

Cost: 3 × `multiply_64` + 5 × `add_64` (with carry propagation).

**Comparison of wide values:** Compare limb-by-limb from most significant to least significant.

#### Full Cubic Check Cost

Each side of `new_LP^3 * old_product <= old_LP^3 * new_product` is six u64 multiplications chained:

```
a * b → u128        (multiply_64)
(a*b) * c → u192    (mul_128x64: 2 multiply_64 + 2 add_64)
(a*b*c) * d → u256  (mul_192x64: 3 multiply_64 + 5 add_64)
(a*b*c*d) * e → u320  (mul_256x64: 4 multiply_64 + ~8 add_64)
(a*b*c*d*e) * f → u384  (mul_320x64: 5 multiply_64 + ~11 add_64)
```

Per side: 1 + 2 + 3 + 4 + 5 = **15 multiply_64** + 0 + 2 + 5 + ~8 + ~11 = **~26 add_64**. (Exact add counts depend on carry propagation strategy; multiply counts are exact.)

Both sides + comparison: **~30 multiply_64 + ~52 add_64 + 6-limb comparison**. This is well within SimplicityHL budget constraints.

For single-sided operations (4 multiplications per side), the cost drops to 6 multiply_64 + 6 add_64 per side (12 + 12 total).

### 8.4 Integer Exactness

All values are integers (satoshis or token lots). Both the fee formula and the cubic LP check use only multiplication, addition, and comparison—no division, no rounding. This is a strict improvement over floating-point AMM implementations.

---

## 9. Bootstrapping Sequence

### 9.1 Prerequisites

A binary prediction market must already exist (market covenant deployed, tokens issued). The pool creator must hold:
- YES tokens (obtained via issuance at the market covenant, or purchase via maker orders)
- NO tokens (same)
- L-BTC

### 9.2 Pool Creation (Not Covenant-Validated)

1. **Issue LP token asset.** Perform a standard Liquid issuance to create the LP token. This produces two assets: the LP token itself (sent to the creator) and the LP reissuance token (which will be locked in the pool).
2. **Compile the pool covenant** with all seven parameters (including `LP_ASSET_ID` and `LP_REISSUANCE_TOKEN_ID` from step 1). The initial `issued_LP` state is chosen by the creator (e.g., equal to the number of LP tokens minted in step 1). This produces the CMR and the initial pool covenant address `script_hash(initial_issued_LP)`.
3. **Construct the creation transaction** (plain Elements transaction, no Simplicity validation):

```
Inputs:
  - Creator's YES token UTXO(s)
  - Creator's NO token UTXO(s)
  - Creator's L-BTC UTXO(s)
  - Creator's LP reissuance token UTXO

Outputs:
  0: YES tokens          → pool covenant address  (amount = initial R_yes)
  1: NO tokens           → pool covenant address  (amount = initial R_no)
  2: L-BTC               → pool covenant address  (amount = initial R_lbtc)
  3: LP reissuance token → pool covenant address  (amount = 1, confidential)
  4: LP tokens           → creator address         (amount = initial_issued_LP)
  5+: Change outputs (any)
  N: Fee output
```

Where `pool covenant address = script_hash(initial_issued_LP)`.

4. Pool is live. The creator holds LP tokens representing 100% of the initial pool. The reissuance token is locked in the covenant, enabling future LP minting.

*Note: Like the market covenant's creation transaction, this is not covenant-validated. A malformed creation would produce UTXOs that don't satisfy the covenant on the first spend. The SDK validates the creation transaction before broadcast.*

### 9.3 Initial State and the Cubic Check

The creation transaction establishes the initial state with `issued_LP > 0` and reserves already funded. The cubic invariant (`product / issued_LP^3`) is established at creation time. The first covenant-validated transaction (swap or LP operation) reads this state via tapdata verification and enforces the invariant from that point forward. Subsequent LP minting occurs through the covenant's LP deposit path using reissuance.

### 9.4 Initial Price Setting

The initial reserve ratios determine the initial prices:

```
price_yes = R_lbtc / R_yes
price_no  = R_lbtc / R_no
```

For a 50/50 market with COLLATERAL_PER_TOKEN = 1,000 sats, rational initial reserves might be:

```
R_yes  = 10,000 lots
R_no   = 10,000 lots
R_lbtc = 10,000,000 sats  (= 10,000 * CPT)
```

This prices each token at 1,000 sats (= CPT), and `price_yes + price_no = 2 * CPT = COLLATERAL_PER_PAIR`. For a non-50/50 starting probability, adjust the ratio of R_yes to R_no while keeping R_lbtc proportional to maintain rational pricing.

---

## 10. Price Alignment and Arbitrage

### 10.1 The Alignment Property

In a binary prediction market, YES and NO tokens are complementary: 1 YES + 1 NO can always be minted (by depositing collateral at the market covenant) or redeemed (by cancelling a pair). This creates an economic invariant:

```
price_yes + price_no ≈ COLLATERAL_PER_PAIR  (= 2 * CPT)
```

The pool covenant does not enforce this. External arbitrageurs maintain alignment:

**If price_yes + price_no > COLLATERAL_PER_PAIR:**

1. Arb issues YES+NO pairs at the market covenant (depositing 2 * CPT per pair).
2. Arb sells both YES and NO tokens into the pool for L-BTC.
3. L-BTC received > 2 * CPT spent → profit.
4. Selling tokens into pool → pool prices decrease → sum approaches COLLATERAL_PER_PAIR.

**If price_yes + price_no < COLLATERAL_PER_PAIR:**

1. Arb buys YES and NO tokens from the pool with L-BTC.
2. Arb cancels pairs at the market covenant (burning 1 YES + 1 NO, receiving 2 * CPT).
3. 2 * CPT received > L-BTC spent → profit.
4. Buying tokens from pool → pool prices increase → sum approaches COLLATERAL_PER_PAIR.

### 10.2 Design Decision: Separation of Pool and Market Covenants

An alternative design would embed the market covenant's mint/cancel logic directly into the AMM, allowing the pool itself to mint pairs when a user buys YES with L-BTC. This was rejected because: (a) it couples the pool to the market covenant's state model, reissuance mechanics, and UTXO layout—dramatically increasing covenant complexity, (b) it creates a dependency on the market covenant's single collateral UTXO, worsening the serialization bottleneck (both pool and market UTXO must be available for every swap), and (c) arbitrage-based alignment is the standard DeFi pattern and works well in practice, with the arb transactions being separate from user swaps.

---

## 11. Transaction Templates

### 11.1 Swap (Buy YES with L-BTC)

Pool address unchanged (state unchanged).

```
Inputs:
  0: Pool YES reserve UTXO          (covenant, primary)
  1: Pool NO reserve UTXO           (covenant, secondary)
  2: Pool L-BTC reserve UTXO        (covenant, secondary)
  3: Pool LP reissuance token UTXO  (covenant, secondary)
  4+: Trader L-BTC UTXO(s)

Outputs:
  0: New YES reserve                → pool covenant address  (decreased)
  1: New NO reserve                 → pool covenant address  (unchanged)
  2: New L-BTC reserve              → pool covenant address  (increased)
  3: LP reissuance token            → pool covenant address  (passthrough)
  4: Trader YES tokens              → trader address
  5+: Trader L-BTC change           → trader address
  N: Fee output
```

### 11.2 Swap (Buy NO with YES)

Pool address unchanged (state unchanged).

```
Inputs:
  0: Pool YES reserve UTXO          (covenant, primary)
  1: Pool NO reserve UTXO           (covenant, secondary)
  2: Pool L-BTC reserve UTXO        (covenant, secondary)
  3: Pool LP reissuance token UTXO  (covenant, secondary)
  4+: Trader YES token UTXO(s)

Outputs:
  0: New YES reserve                → pool covenant address  (increased)
  1: New NO reserve                 → pool covenant address  (decreased)
  2: New L-BTC reserve              → pool covenant address  (unchanged)
  3: LP reissuance token            → pool covenant address  (passthrough)
  4: Trader NO tokens               → trader address
  5+: Trader change                 → trader address
  N: Fee output
```

### 11.3 LP Deposit (Single-Sided, L-BTC Only)

Pool address transitions: `script_hash(old_issued_LP)` → `script_hash(old_issued_LP + minted)`.

```
Inputs:
  0: Pool YES reserve UTXO          (covenant, primary)
  1: Pool NO reserve UTXO           (covenant, secondary)
  2: Pool L-BTC reserve UTXO        (covenant, secondary)
  3: Pool LP reissuance token UTXO  (covenant, secondary — triggers issuance)
  4+: LP's L-BTC UTXO(s)

Outputs:
  0: YES reserve                    → new pool covenant address  (unchanged)
  1: NO reserve                     → new pool covenant address  (unchanged)
  2: New L-BTC reserve              → new pool covenant address  (increased)
  3: LP reissuance token            → new pool covenant address  (cycled back)
  4: Freshly minted LP tokens       → LP's address  (issuance output)
  5+: LP's L-BTC change             → LP's address
  N: Fee output
```

### 11.4 LP Withdraw (Proportional, All Three Assets)

Pool address transitions: `script_hash(old_issued_LP)` → `script_hash(old_issued_LP - burned)`.

```
Inputs:
  0: Pool YES reserve UTXO          (covenant, primary)
  1: Pool NO reserve UTXO           (covenant, secondary)
  2: Pool L-BTC reserve UTXO        (covenant, secondary)
  3: Pool LP reissuance token UTXO  (covenant, secondary)
  4: LP's LP token UTXO(s)          (to be burned)

Outputs:
  0: New YES reserve                → new pool covenant address  (decreased)
  1: New NO reserve                 → new pool covenant address  (decreased)
  2: New L-BTC reserve              → new pool covenant address  (decreased)
  3: LP reissuance token            → new pool covenant address  (passthrough)
  4: LP tokens burned               → OP_RETURN  (unspendable)
  5: YES tokens withdrawn           → LP's address
  6: NO tokens withdrawn            → LP's address
  7: L-BTC withdrawn                → LP's address
  8+: Change                        → LP's address
  N: Fee output
```

---

## 12. Interaction with Maker Orders (Hybrid Model)

The AMM pool and the maker order book serve complementary roles:

| Property | AMM Pool | Maker Orders |
|----------|----------|--------------|
| Liquidity availability | Always on (permissionless swaps) | Only when makers post orders |
| Price efficiency | Slippage on large trades | Tight spreads at specific prices |
| Capital efficiency | Distributed across full price range | Concentrated at chosen price |
| UX complexity | Simple (just swap) | Requires order management |
| Best for | Bootstrapping new markets, small trades, casual users | Mature markets, large trades, sophisticated users |

### 12.1 Routing

A taker can split a trade across the AMM pool and the order book. For example, to buy 100 YES tokens:

1. Check AMM pool price for 100 lots.
2. Check visible maker orders at better prices.
3. Fill maker orders up to the AMM price, then fill the remainder from the AMM.

This is purely a wallet/UI optimization—no covenant-level routing support is needed. Each fill is a separate transaction (or the taker constructs a single transaction consuming both pool UTXOs and maker order UTXOs, if the covenant layouts are compatible).

### 12.2 AMM as Reference Price

Even when the order book is thin, the AMM pool provides a continuous reference price. This is useful for:
- UI display (show current market probability)
- Limit order pricing (makers can reference the AMM price when posting orders)
- Arbitrage (bots keep AMM and order book prices aligned)

---

## 13. Cubic LP Model

The pool supports permissionless multi-LP participation via LP tokens and a cubic invariant. Anyone can deposit any combination of reserves in exchange for LP tokens, or burn LP tokens to withdraw any combination of reserves. The cubic relationship between the three-asset product and issued LP supply ensures fairness without division.

### 13.1 The Cubic Relationship

For a three-asset constant product pool, the correct relationship between the product and LP supply is **cubic**:

```
product / issued_LP^3 = SCALE    (never decreases)
```

Where:
- `product = R_yes * R_no * R_lbtc`
- `issued_LP` = tapdata state value, verified against the pool's scriptPubKey (§5.3)
- `SCALE` is an implicit ratio that only increases over time (due to swap fees)

**Why cubic, not linear?** A proportional deposit that doubles all three reserves (2x each) multiplies the product by 2^3 = 8. If the LP share doubled (linear), the LP would control 8x the product with only 2x the tokens—later depositors would get a better deal than earlier ones. The cubic relationship ensures that doubling the LP supply requires octupling the product (2^3 = 8), maintaining fairness.

**Why not square or fourth power?** The exponent matches the number of reserves. A two-asset pool uses the square (Uniswap v2). A three-asset pool uses the cube. The general rule: for an N-asset constant product pool, `product / LP^N` must not decrease.

### 13.2 Covenant Check (Multiplication Only)

The covenant enforces:

```
new_issued_LP^3 * old_product <= old_issued_LP^3 * new_product
```

This is equivalent to `new_SCALE >= old_SCALE` with no division. Both `old_issued_LP` and `new_issued_LP` are known directly from the tapdata state (old = verified from witness, new = computed from minting/burning). All arithmetic is multiplication and comparison. The composed wide arithmetic from §8.3 handles overflow.

### 13.3 Single-Sided Deposit Optimization

When only one reserve changes (e.g., depositing only L-BTC), the unchanged reserves cancel:

```
new_LP^3 * old_R_yes * old_R_no * old_R_lbtc <= old_LP^3 * old_R_yes * old_R_no * new_R_lbtc
                                                            ^^^^^^^^^^^^^^^^^^^^
                                                     old_R_yes * old_R_no cancels

→  new_LP^3 * old_R_lbtc <= old_LP^3 * new_R_lbtc
```

This reduces to four u64 multiplications per side (two cubes and one reserve each). Worst case: (10^11)^4 = 10^44, fitting within 192-bit composed arithmetic—cheaper than the full six-multiplication general case.

### 13.4 Price Impact as Natural Penalty

Single-sided deposits face implicit price impact from the CPMM curve. An LP depositing only L-BTC effectively increases R_lbtc while leaving R_yes and R_no unchanged, which:

1. Shifts the L-BTC spot price downward relative to tokens.
2. Results in fewer LP tokens received per unit deposited (compared to a proportional three-sided deposit of equal total value).

This price impact benefits remaining LPs: their LP tokens now represent a share of a pool with a higher `product / LP^3` ratio. The penalty is proportional to the imbalance—depositing 1% of the pool single-sided has negligible impact; depositing 50% has severe impact.

### 13.5 Fee Accrual

Swap fees grow the product automatically (each swap increases `k` due to the fee formula in §7). The LP supply remains constant between deposits/withdrawals. Therefore:

```
SCALE = product / issued_LP^3
```

increases on every swap. LP tokens appreciate in value over time without any explicit fee distribution mechanism. When an LP withdraws, the cubic check ensures they receive their proportional share of the grown reserves.

### 13.6 Numerical Example

Initial state: R_yes = 10,000, R_no = 10,000, R_lbtc = 10,000,000. Creator holds 1,000 LP tokens (issued_LP = 1,000).

```
product = 10,000 * 10,000 * 10,000,000 = 10^15
SCALE = 10^15 / 1,000^3 = 10^15 / 10^9 = 10^6
```

A new LP wants to deposit 1,000,000 sats (L-BTC only, single-sided). They want to receive `lp_mint` tokens. The cubic check:

```
(1,000 + lp_mint)^3 * 10^15 <= 1,000^3 * (10,000 * 10,000 * 11,000,000)
(1,000 + lp_mint)^3 * 10^15 <= 10^9 * 1.1 * 10^15
(1,000 + lp_mint)^3 <= 1.1 * 10^9
1,000 + lp_mint <= (1.1 * 10^9)^(1/3) ≈ 1,032.3
lp_mint <= 32
```

The LP receives at most 32 tokens for a 10% single-sided deposit—less than the 100 tokens (10% of 1,000) they would get from a proportional deposit. The difference is the price impact penalty, captured by existing LPs.

---

## 14. Serialization and Throughput

The four pool UTXOs must be consumed together on every swap or LP operation, creating the same serialization bottleneck as the market covenant's single collateral UTXO: one swap per block per pool.

On Liquid (≈1-minute blocks), this means ≈60 swaps/hour per pool. For most prediction markets, this is sufficient. For high-volume markets:

1. **Multiple pools.** Deploy several pools with different fee tiers (e.g., 10 bps, 30 bps, 100 bps). Each pool has independent UTXOs, enabling parallel swaps.
2. **Order book overflow.** The maker order book absorbs excess volume. Unlike the AMM, each maker order is an independent UTXO—multiple fills can execute in parallel.
3. **Batching.** Multiple swap intents could theoretically be batched into one transaction if a coordinator collects them (each user gets their output, pool transitions once). This is not part of the current covenant design but is compatible with it.

### 14.1 Design Decision: Accept Serialization

Alternatives to reduce serialization (e.g., fragmented pool UTXOs with independent reserves per asset pair) break the three-asset invariant. The triple product `R_yes * R_no * R_lbtc` cannot be verified if the reserves are in separate, independently-spendable UTXOs. Serialization is the fundamental cost of a multi-asset pool on a UTXO chain. The hybrid model (AMM + order book) mitigates it at the system level.

---

## 15. Post-Resolution Behavior

After the market's oracle resolves (say YES wins):

- YES tokens become worth `2 * COLLATERAL_PER_TOKEN` each (redeemable at the market covenant).
- NO tokens become worth 0.
- The pool still functions—the covenant does not know about market resolution.

**What happens in practice:**

1. Arbitrageurs drain YES tokens and L-BTC from the pool by swapping in worthless NO tokens.
2. LPs should withdraw before or immediately after resolution to avoid value extraction.
3. Eventually only NO tokens remain in the pool—worthless, and the pool is effectively dead.

This is analogous to impermanent loss in standard AMMs, but the outcome is binary and total: one side goes to zero. LP risk management (withdrawing before resolution) is a user responsibility. The wallet UI should warn LP holders when a market's expiry or oracle resolution approaches. See §16 for a detailed analysis of LP economics, impermanent loss, and profitability conditions.

---

## 16. LP Incentive Analysis

The pool mechanics (§3, §7, §13) define how the AMM operates, but do not address when providing liquidity is profitable. This section analyzes LP economics: fee accrual, impermanent loss (IL), and the unique risks of binary prediction market resolution. The analysis informs both LP users and wallet UI design.

### 16.1 Sources of LP Return

**1. Swap fee accrual (§7, §13.5).** Every swap increases the product `k = R_yes * R_no * R_lbtc` by at least the fee amount. Since `SCALE = k / issued_LP^3` only increases, LP tokens appreciate automatically. No explicit fee distribution is needed — LPs capture fees by withdrawing a larger share of reserves than they deposited.

**2. Price oscillation (mean-reversion).** If prices move away from the LP's entry point and then return, impermanent loss reverses to zero while fees earned during the oscillation are permanently captured. Active two-way markets with high volume and moderate price movement are ideal for LPs.

### 16.2 Sources of LP Loss

**1. Impermanent loss (IL) from price drift.** As prices move from the LP's entry ratio, the AMM rebalances: selling the appreciating asset and accumulating the depreciating one. The LP ends up with more of the cheaper asset and less of the expensive one compared to HODL.

**2. Resolution loss (catastrophic, prediction-market-specific).** At binary resolution, one token goes to zero. Arbitrageurs flood the pool with worthless tokens, extracting all valuable assets (§15). Any LP position remaining at resolution suffers near-total loss. This is the dominant LP risk and is unique to prediction markets.

**3. Low-volume drag.** If trading volume is low relative to reserves, fee revenue is small. Meanwhile, any price movement causes IL. LPs in low-volume pools slowly lose value.

### 16.3 Three-Asset IL Formula

For a three-asset constant-product pool where the LP entered at prices P_yes_0, P_no_0, and the prices change by factors `a = P_yes / P_yes_0` and `b = P_no / P_no_0`:

```
IL = 3 * (a * b)^(1/3) / (a + b + 1) - 1
```

This is the ratio of pool value to HODL value, minus one. Negative values mean the LP underperforms HODL.

**Derivation sketch.** In a three-asset CPMM, each reserve's value contribution at spot price equals R_lbtc (the numeraire reserve). Total pool value = 3 * R_lbtc. From the invariant, R_lbtc scales as `(a * b)^(1/3)`. HODL value scales as `(a + b + 1) / 3`. The ratio gives the formula.

### 16.4 Prediction Market Constraint

Arbitrage enforces `P_yes + P_no ≈ 2 * CPT` (collateral per pair, §10). For a 50/50 entry (`P_yes_0 = P_no_0 = CPT`), this means `a + b = 2` always. The IL formula simplifies to:

```
IL = (a * (2 - a))^(1/3) - 1
```

where `a = P_yes / P_yes_0` (YES price multiplier).

### 16.5 IL Table: 50/50 Market Entry

| YES probability | a | b | a × b | IL |
|---|---|---|---|---|
| 50% (entry) | 1.0 | 1.0 | 1.00 | 0.0% |
| 55% | 1.1 | 0.9 | 0.99 | −0.3% |
| 60% | 1.2 | 0.8 | 0.96 | −1.4% |
| 65% | 1.3 | 0.7 | 0.91 | −3.1% |
| 70% | 1.4 | 0.6 | 0.84 | −5.7% |
| 80% | 1.6 | 0.4 | 0.64 | −13.8% |
| 90% | 1.8 | 0.2 | 0.36 | −28.9% |
| 95% | 1.9 | 0.1 | 0.19 | −42.5% |
| 100% (resolved) | 2.0 | 0.0 | 0.00 | −100.0% |

Key observations:

- IL is zero at entry and grows as the market moves toward certainty.
- IL accelerates: the 80→90% move costs −15.1% while the 50→60% move costs only −1.4%.
- At resolution (100% or 0%), IL is total. The pool holds only the worthless token.
- The three-asset pool has ~30% less IL than a two-asset YES/NO pool at the same price, because the L-BTC reserve acts as a stabilizer.

### 16.6 Breakeven Analysis: Fees vs. IL

An LP breaks even when cumulative fee revenue equals IL. With `FEE_BPS = 30` (0.3% per swap):

```
Required daily volume / TVL ≈ IL% / (days_active × 0.3%)
```

| Exit probability | IL | Days active | Volume/TVL needed per day |
|---|---|---|---|
| 60% | 1.4% | 30 | 16% |
| 70% | 5.7% | 30 | 63% |
| 80% | 13.8% | 30 | 153% |
| 90% | 28.9% | 30 | 321% |

For context, active DeFi pools typically see 10–200% daily volume/TVL. A 30-day market that resolves near 70% is breakeven-achievable. Beyond 80%, fees cannot plausibly compensate for IL.

### 16.7 Initial Position: No Directional Exposure

An LP entering a 50/50 market deposits equal-value YES, NO, and L-BTC. The YES + NO portion (2/3 of value) is equivalent to pure collateral (since 1 YES + 1 NO is always redeemable for 2 × CPT). Combined with the L-BTC portion, the LP has **zero directional exposure at entry** — they are not betting on any outcome. Directional exposure develops only as prices move and the AMM rebalances. This is a useful property for LPs who want to earn fees without taking a market view.

### 16.8 The Prediction Market LP Dilemma

Traditional AMM IL is "impermanent" because prices can reverse indefinitely. Prediction markets have a **terminal event** (resolution) where:

1. One token permanently goes to zero.
2. IL becomes permanent and total for any remaining LP position.
3. There is no recovery — the market is over.

This creates a time-decay dynamic analogous to options writing:

- **Early in market life:** High uncertainty → active two-way trading → strong fee generation. Prices near 50/50 → low IL. Best time to LP.
- **Late in market life:** Outcome becoming clear → one-directional flow → rapid IL accumulation. Less time remaining for fee earning. Worst time to LP.
- **At resolution:** Total IL. LP position value → 0.

### 16.9 LP Profitability Conditions

**LPs are likely profitable when:**

- Market has high two-way trading volume (volatile probability, not directional drift).
- Market probability stays within ~30–70% range (IL below ~6%).
- LP exits well before resolution.
- Fee rate is sufficient for the expected volume (higher FEE_BPS helps but may reduce volume).

**LPs are likely unprofitable when:**

- Market probability drifts strongly toward 0% or 100%.
- Low trading volume (fees don't cover IL from normal drift).
- LP holds position through or near resolution.
- Market is "boring" (stable probability, low volume, low fees, but any sudden move creates unrewarded IL).

### 16.10 Wallet UI Implications

The wallet should:

- Display current IL in real-time (vs. LP's entry reserves).
- Display cumulative fees earned (SCALE growth since entry).
- Display net P&L (fees − IL).
- Show the IL curve for the current market state (table above).
- **Warn when market probability moves past configurable thresholds** (e.g., beyond 75%, IL exceeds 10%).
- **Warn when market expiry/resolution is approaching** (see also §15, §20.4).
- Show estimated breakeven volume (how much more trading is needed to offset current IL).

---

## 17. Summary of Design Decisions

| Decision | Chosen | Rejected Alternative |
|----------|--------|---------------------|
| Asset count | Three trading assets (YES, NO, L-BTC) + LP token | Two (YES/NO only; requires two-step issuance+swap) |
| Invariant | Constant product (x·y·z=k) | LMSR (requires logarithms), constant sum (depletes) |
| Fee model | Uniswap v2 cross-multiplication | No fee enforcement (allows free-riding), product-growth check (doesn't scale with swap size) |
| Fee parameter | Compile-time constant | Witness value (manipulable), state variable (adds complexity) |
| LP token supply | Dynamic reissuance with tapdata state tracking (no supply ceiling) | Fixed pre-minted supply (requires choosing TOTAL_LP_SUPPLY, can be exhausted) |
| Trust model | Fully trustless (no privileged keys) | Creator emergency withdraw (breaks trustlessness for other LPs) |
| LP model | Cubic (product / LP^3, permissionless) | Proportional-only (restrictive UX), linear (unfair to early depositors), single trusted creator (centralized) |
| LP deposits | Any combination (single-sided, multi-sided) | Proportional-only (forces equal ratio deposits, poor UX) |
| Pool/market coupling | Independent (arb-linked) | Embedded mint/cancel (too complex, worsens serialization) |
| Input layout | Fixed indices (0=YES, 1=NO, 2=LBTC, 3=reissuance token) | Dynamic asset discovery (more complex covenant logic) |
| Wide arithmetic | Composed from multiply_64 (schoolbook) | Require 128-bit jets (not available), restructure to avoid (limits pool size) |
| Serialization | Accepted (mitigated by hybrid model) | Fragmented pools (breaks three-asset invariant) |

---

## 18. Contract Structure

### 18.1 main() Entry Point

The contract's `main()` function reads all witness values, then dispatches to the appropriate spending path via a 3-way nested `Either` match. All witnesses are read unconditionally in `main()` before dispatch—this is a SimplicityHL requirement, as witness values must be bound at the top level even if only a subset is used by any given path.

```rust
fn main() {
    // ── Read all witnesses (SimplicityHL requirement) ──────────────

    let path: Either<Either<(), ()>, ()> = witness::PATH;
    let swap_pair: u8 = witness::SWAP_PAIR;       // 0, 1, or 2
    let issued_lp: u64 = witness::ISSUED_LP;      // tapdata state

    // Blinding factors for reissuance token verification
    let input_abf: u256 = witness::INPUT_ABF;
    let input_vbf: u256 = witness::INPUT_VBF;
    let output_abf: u256 = witness::OUTPUT_ABF;
    let output_vbf: u256 = witness::OUTPUT_VBF;

    // ── Read current reserves from inputs ──────────────────────────

    let (_, old_r_yes):  (u256, u64) = get_input_explicit_asset_amount(0);
    let (_, old_r_no):   (u256, u64) = get_input_explicit_asset_amount(1);
    let (_, old_r_lbtc): (u256, u64) = get_input_explicit_asset_amount(2);

    // ── Dispatch ───────────────────────────────────────────────────

    match path {
        Left(swap_or_lp) => match swap_or_lp {
            Left(_)  => { /* 1. Swap (permissionless)               */ },
            Right(_) => { /* 2. LP deposit/withdraw (cubic-checked) */ },
        },
        Right(_) => {     /* 3. Secondary covenant input            */ },
    }
}
```

### 18.2 Path 1: Swap

```rust
// ── Precondition: primary input ────────────────────────────────
let ci: u32 = jet::current_index();
assert!(jet::eq_32(ci, 0));

// ── State verification ────────────────────────────────────────
// Verify that the witness issued_lp value matches the pool's
// current scriptPubKey (tapdata state commitment, see §5.3).
let expected_spk_hash: u256 = script_hash_for_state(issued_lp);
let own_spk_hash: u256 = get_input_script_hash(0);
assert!(jet::eq_256(expected_spk_hash, own_spk_hash));

// ── Verify input assets and co-membership ──────────────────────
assert!(jet::eq_256(input_asset(0), param::YES_ASSET_ID));
assert!(jet::eq_256(input_asset(1), param::NO_ASSET_ID));
assert!(jet::eq_256(get_input_script_hash(1), own_spk_hash));
assert!(jet::eq_256(input_asset(2), param::LBTC_ASSET_ID));
assert!(jet::eq_256(get_input_script_hash(2), own_spk_hash));
verify_input_reissuance_token(3, input_abf, input_vbf);
assert!(jet::eq_256(get_input_script_hash(3), own_spk_hash));

// ── Verify output layout (all four to same pool address) ───────
assert!(jet::eq_256(output_asset(0), param::YES_ASSET_ID));
ensure_output_script_hash_eq(0, own_spk_hash);
assert!(jet::eq_256(output_asset(1), param::NO_ASSET_ID));
ensure_output_script_hash_eq(1, own_spk_hash);
assert!(jet::eq_256(output_asset(2), param::LBTC_ASSET_ID));
ensure_output_script_hash_eq(2, own_spk_hash);
verify_output_reissuance_token(3, output_abf, output_vbf);
ensure_output_script_hash_eq(3, own_spk_hash);

let (_, new_r_yes):  (u256, u64) = get_output_explicit_asset_amount(0);
let (_, new_r_no):   (u256, u64) = get_output_explicit_asset_amount(1);
let (_, new_r_lbtc): (u256, u64) = get_output_explicit_asset_amount(2);

// ── Minimum reserve check ──────────────────────────────────────
assert!(jet::gt_64(new_r_yes, 0));
assert!(jet::gt_64(new_r_no, 0));
assert!(jet::gt_64(new_r_lbtc, 0));

// ── Swap pair dispatch + unchanged reserve + fee invariant ─────
//
// For each pair, verify:
//   (a) The third trading reserve is unchanged.
//   (b) The Uniswap v2 fee invariant holds for the two changing
//       reserves (see §7).
//
// The fee formula (§7) requires A = output (decreased) and
// B = input (increased). The covenant determines the direction
// at runtime by checking which reserve decreased.

if swap_pair == 0 {
    // YES ↔ NO (L-BTC unchanged)
    assert!(jet::eq_64(new_r_lbtc, old_r_lbtc));
    match jet::le_64(new_r_yes, old_r_yes) {
        true  => assert_fee_invariant(old_r_yes, old_r_no, new_r_yes, new_r_no),
        false => assert_fee_invariant(old_r_no, old_r_yes, new_r_no, new_r_yes),
    };
} else if swap_pair == 1 {
    // YES ↔ L-BTC (NO unchanged)
    assert!(jet::eq_64(new_r_no, old_r_no));
    match jet::le_64(new_r_yes, old_r_yes) {
        true  => assert_fee_invariant(old_r_yes, old_r_lbtc, new_r_yes, new_r_lbtc),
        false => assert_fee_invariant(old_r_lbtc, old_r_yes, new_r_lbtc, new_r_yes),
    };
} else {
    // NO ↔ L-BTC (YES unchanged)
    assert!(jet::eq_64(new_r_yes, old_r_yes));
    match jet::le_64(new_r_no, old_r_no) {
        true  => assert_fee_invariant(old_r_no, old_r_lbtc, new_r_no, new_r_lbtc),
        false => assert_fee_invariant(old_r_lbtc, old_r_no, new_r_lbtc, new_r_no),
    };
}
```

### 18.3 Fee Invariant Helper

```rust
fn assert_fee_invariant(old_a: u64, old_b: u64, new_a: u64, new_b: u64) {
    // new_a * (new_b * (FEE_DENOM - FEE_BPS) + old_b * FEE_BPS)
    //     >= old_a * old_b * FEE_DENOM
    //
    // All arithmetic in u128 to prevent overflow (§8.1).
    // Note: SimplicityHL has no native u128 operations. Each u128
    // multiply/add below is composed from jet::multiply_64 and
    // manual limb addition at the implementation level (see §18.6).

    let fee_complement: u128 = (param::FEE_DENOM - param::FEE_BPS) as u128;
    let fee_bps: u128        = param::FEE_BPS as u128;
    let fee_denom: u128      = param::FEE_DENOM as u128;

    let lhs_inner: u128 = (new_b as u128) * fee_complement
                        + (old_b as u128) * fee_bps;
    let lhs: u128 = (new_a as u128) * lhs_inner;

    let rhs: u128 = (old_a as u128) * (old_b as u128) * fee_denom;

    assert!(lhs >= rhs);
}
```

### 18.4 Path 2: LP Deposit / Withdraw

```rust
// ── Precondition: primary input ────────────────────────────────
let ci: u32 = jet::current_index();
assert!(jet::eq_32(ci, 0));

// ── State verification ────────────────────────────────────────
let expected_spk_hash: u256 = script_hash_for_state(issued_lp);
let own_spk_hash: u256 = get_input_script_hash(0);
assert!(jet::eq_256(expected_spk_hash, own_spk_hash));

// ── Verify input assets and co-membership ──────────────────────
assert!(jet::eq_256(input_asset(0), param::YES_ASSET_ID));
assert!(jet::eq_256(input_asset(1), param::NO_ASSET_ID));
assert!(jet::eq_256(get_input_script_hash(1), own_spk_hash));
assert!(jet::eq_256(input_asset(2), param::LBTC_ASSET_ID));
assert!(jet::eq_256(get_input_script_hash(2), own_spk_hash));
verify_input_reissuance_token(3, input_abf, input_vbf);
assert!(jet::eq_256(get_input_script_hash(3), own_spk_hash));

// ── Determine new_issued_lp from transaction ───────────────────
//
// Two cases: deposit (minting) or withdraw (burning).
// We read both sources and compute the net change.
//
// Minted amount: jet::issuance_asset_amount(3) returns the number
// of LP tokens minted via reissuance at input 3. Returns None if
// no issuance occurred (withdraw case), so we default to 0.
//
// Burned amount: LP tokens sent to the OP_RETURN output at index 4.
// We verify the output is unspendable, has the correct asset, and
// read the amount. If output 4 is not an LP burn, burned = 0.
//
// See §18.9 for helper implementations.

let minted: u64 = get_issuance_amount_or_zero(3);
let burned: u64 = get_lp_burn_amount_or_zero(4);

// Exactly one of minted/burned must be nonzero (no simultaneous
// mint+burn, and LP path must do something).
assert!(jet::is_zero_64(minted) != jet::is_zero_64(burned));

let new_issued_lp: u64 = safe_add(
    safe_subtract(issued_lp, burned),
    minted
);

// ── Verify output layout at NEW state address ──────────────────
let new_spk_hash: u256 = script_hash_for_state(new_issued_lp);

assert!(jet::eq_256(output_asset(0), param::YES_ASSET_ID));
ensure_output_script_hash_eq(0, new_spk_hash);
assert!(jet::eq_256(output_asset(1), param::NO_ASSET_ID));
ensure_output_script_hash_eq(1, new_spk_hash);
assert!(jet::eq_256(output_asset(2), param::LBTC_ASSET_ID));
ensure_output_script_hash_eq(2, new_spk_hash);
verify_output_reissuance_token(3, output_abf, output_vbf);
ensure_output_script_hash_eq(3, new_spk_hash);

let (_, new_r_yes):  (u256, u64) = get_output_explicit_asset_amount(0);
let (_, new_r_no):   (u256, u64) = get_output_explicit_asset_amount(1);
let (_, new_r_lbtc): (u256, u64) = get_output_explicit_asset_amount(2);

// ── Minimum reserves ───────────────────────────────────────────
assert!(jet::gt_64(new_r_yes, 0));
assert!(jet::gt_64(new_r_no, 0));
assert!(jet::gt_64(new_r_lbtc, 0));

// ── Minimum issued LP ──────────────────────────────────────────
assert!(jet::gt_64(new_issued_lp, 0));

// ── Cubic invariant check (see §13.2) ──────────────────────────
//
//   new_issued_LP^3 * old_product <= old_issued_LP^3 * new_product
//
// old_issued_lp = verified from witness via tapdata state check.
// new_issued_lp = computed from minting/burning amounts.
// Each side is a product of six u64 values, requiring composed
// wide arithmetic (see §8.3). The result is a ~384-bit value.

let lhs: Wide384 = wide_mul_6(
    new_issued_lp, new_issued_lp, new_issued_lp,
    old_r_yes, old_r_no, old_r_lbtc
);
let rhs: Wide384 = wide_mul_6(
    issued_lp, issued_lp, issued_lp,
    new_r_yes, new_r_no, new_r_lbtc
);
assert!(wide_le(lhs, rhs));
```

### 18.5 Path 3: Secondary Covenant Input

```rust
// ── This input is NOT index 0 ──────────────────────────────────
let ci: u32 = jet::current_index();
assert!(jet::ne_32(ci, 0));

// ── Same covenant as the primary input ─────────────────────────
let own_spk_hash: u256 = get_input_script_hash(ci);
let primary_spk_hash: u256 = get_input_script_hash(0);
assert!(jet::eq_256(own_spk_hash, primary_spk_hash));
```

### 18.6 Wide Arithmetic Helpers

These functions compose wide multiplication from `jet::multiply_64(u64, u64) -> u128`.

```rust
// ── Multi-word types (represented as tuples of u64 limbs) ──────
// Wide128 = (u64, u64)           — high, low
// Wide192 = (u64, u64, u64)      — w2, w1, w0
// Wide256 = (u64, u64, u64, u64) — w3, w2, w1, w0
// ...up to Wide384 for the full cubic check

// ── mul_128x64: u128 × u64 → Wide192 ──────────────────────────
fn mul_128x64(ab: (u64, u64), c: u64) -> (u64, u64, u64) {
    let (hi, lo) = ab;
    let lo_c: u128 = jet::multiply_64(lo, c);  // u128
    let hi_c: u128 = jet::multiply_64(hi, c);  // u128

    // lo_c = (lc1, lc0), hi_c = (hc1, hc0)
    // Result = hc1 : (hc0 + lc1) : lc0
    // with carry propagation on the middle limb addition.
    let (lc1, lc0): (u64, u64) = <u128>::into(lo_c);
    let (hc1, hc0): (u64, u64) = <u128>::into(hi_c);

    let (carry, w1): (bool, u64) = jet::add_64(hc0, lc1);
    let (_, w2): (bool, u64) = jet::add_64(hc1, <bool>::into(carry));

    (w2, w1, lc0)  // Wide192
}

// ── mul_192x64: Wide192 × u64 → Wide256 ────────────────────────
fn mul_192x64(abc: (u64, u64, u64), d: u64) -> (u64, u64, u64, u64) {
    let (w2, w1, w0) = abc;

    let w0_d: u128 = jet::multiply_64(w0, d);
    let w1_d: u128 = jet::multiply_64(w1, d);
    let w2_d: u128 = jet::multiply_64(w2, d);

    // Assemble four limbs with carry propagation
    let (w0d_hi, r0): (u64, u64) = <u128>::into(w0_d);
    let (w1d_hi, w1d_lo): (u64, u64) = <u128>::into(w1_d);
    let (w2d_hi, w2d_lo): (u64, u64) = <u128>::into(w2_d);

    let (c1, r1): (bool, u64) = jet::add_64(w1d_lo, w0d_hi);
    let (c2, r2): (bool, u64) = jet::add_64(w2d_lo, w1d_hi);
    let (c2b, r2): (bool, u64) = jet::add_64(r2, <bool>::into(c1));
    // Carries discarded: safe because high limbs are small for practical
    // reserve/LP values (see §8). A production implementation should
    // propagate carries into a 4th limb for full generality.
    let (_, r3): (bool, u64) = jet::add_64(w2d_hi, <bool>::into(c2));
    let (_, r3): (bool, u64) = jet::add_64(r3, <bool>::into(c2b));

    (r3, r2, r1, r0)  // Wide256
}

// ── wide_mul_6: six u64 values → Wide384 ───────────────────────
// Chains: a*b → u128, *c → u192, *d → u256, *e → u320, *f → u384
fn wide_mul_6(a: u64, b: u64, c: u64, d: u64, e: u64, f: u64) -> Wide384 {
    let ab: u128 = jet::multiply_64(a, b);
    let abc: Wide192 = mul_128x64(ab, c);
    let abcd: Wide256 = mul_192x64(abc, d);
    let abcde: Wide320 = mul_256x64(abcd, e);
    let abcdef: Wide384 = mul_320x64(abcde, f);
    abcdef
}

// ── wide_le: Wide384 <= Wide384 ────────────────────────────────
// Compare limb-by-limb from most significant to least significant.
fn wide_le(lhs: Wide384, rhs: Wide384) -> bool {
    // Compare w5, then w4, ..., then w0.
    // First unequal limb determines the result.
    // If all equal, lhs <= rhs is true.
    // (Implementation: chain of comparisons with early exit.)
}
```

> **Note:** `mul_256x64`, `mul_320x64`, and `wide_le` follow the same composed-arithmetic pattern as `mul_128x64` and `mul_192x64` above—bodies are omitted for brevity. Each wider multiply decomposes the wide input into u64 limbs, multiplies each by the u64 operand via `jet::multiply_64`, and propagates carries upward.

### 18.7 Utility Functions

The contract reuses utility functions from the existing codebase:

- `get_input_explicit_asset_amount(index)` — reads explicit amount and asset from a transaction input. Asserts the amount is explicit (non-confidential).
- `get_output_explicit_asset_amount(index)` — same for outputs.
- `input_asset(index)` — reads only the asset ID from an input.
- `output_asset(index)` — reads only the asset ID from an output.
- `get_input_script_hash(index)` — reads the scriptPubKey hash of an input.
- `ensure_output_script_hash_eq(index, expected)` — asserts output scriptPubKey hash matches expected value.
- `safe_subtract(a, b)` — subtracts with borrow assertion (from maker_order.simf).
- `safe_add(a, b)` — adds with carry assertion (from maker_order.simf).

New utility functions for the cubic LP check:
- `mul_128x64`, `mul_192x64`, `mul_256x64`, `mul_320x64` — composed wide multiplication (§18.6).
- `wide_mul_6` — chains six u64 multiplications into a Wide384 result.
- `wide_le` — compares two Wide384 values limb-by-limb.

New utility functions for the tapdata state model:
- `script_hash_for_state(issued_lp)` — computes the pool's P2TR scriptPubKey hash for a given state (§18.8).
- `verify_input_reissuance_token(index, abf, vbf)` — verifies the reissuance token on an input via Pedersen commitment (§18.8).
- `verify_output_reissuance_token(index, abf, vbf)` — same for outputs.

New utility functions for LP minting and burning (§18.9):
- `get_issuance_amount_or_zero(index)` — reads LP tokens minted via reissuance at an input. Uses `jet::issuance_asset_amount`. Returns 0 if no issuance occurred.
- `empty_script_hash()` — returns `SHA256("")`, the canonical OP_RETURN script hash.
- `get_lp_burn_amount_or_zero(index)` — reads LP tokens burned at an OP_RETURN output. Returns 0 if the output is not an LP burn.

### 18.8 State and Reissuance Helpers

These functions implement the tapdata state model (§5.3) and reissuance token verification (§5.4).

```rust
// ── NUMS key for taproot internal key (no key-path spend) ──────
fn covenant_nums_key() -> u256 {
    0x50929b74c1a04954b78b4b6035e97a5e078a5a0f28ec96d547bfee9ace803ac0
}

// ── Compute pool scriptPubKey hash for a given issued_lp state ─
fn script_hash_for_state(issued_lp: u64) -> u256 {
    // 1. Get this covenant's tapleaf hash (fixed, independent of state)
    let tap_leaf: u256 = jet::tapleaf_hash();

    // 2. Build tapdata leaf: SHA256 commitment to issued_lp
    let state_ctx: Ctx8 = jet::tapdata_init();
    let state_ctx: Ctx8 = jet::sha_256_ctx_8_add_8(state_ctx, issued_lp);
    let state_leaf: u256 = jet::sha_256_ctx_8_finalize(state_ctx);

    // 3. Combine into tapbranch
    let tap_node: u256 = jet::build_tapbranch(tap_leaf, state_leaf);

    // 4. Tweak the NUMS key to produce the output key
    let tweaked_key: u256 = jet::build_taptweak(covenant_nums_key(), tap_node);

    // 5. Compute P2TR script hash from the tweaked output key
    compute_p2tr_script_hash_from_output_key(tweaked_key)
}

// ── Pedersen commitment verification for reissuance token ──────

fn verify_input_reissuance_token(index: u32, abf: u256, vbf: u256) {
    let (actual_asset, actual_amount): (Asset1, Amount1) =
        unwrap(jet::input_amount(index));
    verify_token_commitment(
        actual_asset, actual_amount,
        param::LP_REISSUANCE_TOKEN_ID, abf, vbf
    );
}

fn verify_output_reissuance_token(index: u32, abf: u256, vbf: u256) {
    let (actual_asset, actual_amount): (Asset1, Amount1) =
        unwrap(jet::output_amount(index));
    verify_token_commitment(
        actual_asset, actual_amount,
        param::LP_REISSUANCE_TOKEN_ID, abf, vbf
    );
}

fn verify_token_commitment(
    actual_asset: Asset1,
    actual_amount: Amount1,
    expected_token_id: u256,
    abf: u256,
    vbf: u256,
) {
    // Asset commitment: H(token_id) + abf * G
    let gej_point: Gej = (jet::hash_to_curve(expected_token_id), 1);
    let asset_blind_point: Gej = jet::generate(abf);
    let expected_asset_generator: Gej = jet::gej_add(gej_point, asset_blind_point);

    // Amount commitment: 1 * expected_asset_generator + vbf * H
    // (reissuance token always has amount = 1)
    let amount_point: Gej = expected_asset_generator;
    let value_blind_point: Gej = jet::generate(vbf);
    let expected_amount_commitment: Gej = jet::gej_add(amount_point, value_blind_point);

    // Normalize and compare against actual values
    let expected_asset_ge: Ge = jet::gej_normalize(expected_asset_generator);
    let expected_amount_ge: Ge = jet::gej_normalize(expected_amount_commitment);

    match actual_asset {
        Left(conf_asset: Point) => {
            assert!(jet::eq_ge(conf_asset, expected_asset_ge));
        },
        Right(explicit_asset: u256) => {
            assert!(jet::eq_256(explicit_asset, expected_token_id));
        },
    };

    match actual_amount {
        Left(conf_amount: Point) => {
            assert!(jet::eq_ge(conf_amount, expected_amount_ge));
        },
        Right(explicit_amount: u256) => {
            assert!(jet::eq_64(explicit_amount, 1));
        },
    };
}
```

### 18.9 LP Minting and Burning Helpers

These functions read the minted and burned LP token amounts from the transaction. The minting amount is read via `jet::issuance_asset_amount`, the same jet used by the prediction market contract for YES/NO token issuance. The burn amount is read from an OP_RETURN output, the same pattern used by the prediction market for token cancellation and redemption.

```rust
// ── Read LP tokens minted via reissuance at input index ─────────
// jet::issuance_asset_amount(index) returns a nested Option type.
// Outer None = no issuance at this input. Inner unwrap yields
// (Asset1, Amount1). Returns the explicit minted amount, or 0.

fn get_issuance_amount_or_zero(index: u32) -> u64 {
    match jet::issuance_asset_amount(index) {
        None => 0,
        Some(inner) => {
            let (asset, amount): (Asset1, Amount1) = unwrap(inner);
            // Verify the issued asset is actually the LP token
            let asset_id: u256 = unwrap_right::<(u1, u256)>(asset);
            assert!(jet::eq_256(asset_id, param::LP_ASSET_ID));
            // Extract the explicit amount
            unwrap_right::<(u1, u256)>(amount)
        },
    }
}

// ── OP_RETURN burn verification ─────────────────────────────────
// SHA256 of the empty byte string — the canonical hash of an
// OP_RETURN scriptPubKey with no data. This is the standard
// unspendable output pattern on Liquid, used by prediction_market.simf
// for token cancellation and redemption burns.

fn empty_script_hash() -> u256 {
    0xe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
}

fn ensure_output_is_op_return(index: u32) {
    let script_hash: u256 = unwrap(jet::output_script_hash(index));
    assert!(jet::eq_256(script_hash, empty_script_hash()));
}

// ── Read LP tokens burned at an output index ────────────────────
// Checks that output is an OP_RETURN with the LP asset, and returns
// the explicit amount. Returns 0 if the output is not an LP burn.

fn get_lp_burn_amount_or_zero(index: u32) -> u64 {
    let script_hash: u256 = unwrap(jet::output_script_hash(index));
    let is_op_return: bool = jet::eq_256(script_hash, empty_script_hash());

    match is_op_return {
        false => 0,
        true => {
            let (asset, amount): (u256, u64) =
                get_output_explicit_asset_amount(index);
            // Verify the burned asset is actually the LP token
            assert!(jet::eq_256(asset, param::LP_ASSET_ID));
            amount
        },
    }
}
```

---

## 19. Off-Chain Swap Calculation

Taker software computes swap amounts off-chain before constructing the transaction. The covenant validates the result; it does not compute it.

### 19.1 Exact Output (trader specifies how many tokens to receive)

Given: trader wants `Δ_out` of asset A from the pool, paying asset B.

```
effective_out = Δ_out
Δ_in = ceil( old_R_B * Δ_out * FEE_DENOM / ((old_R_A - Δ_out) * (FEE_DENOM - FEE_BPS)) )
```

The `ceil()` ensures the covenant check passes (rounds in the pool's favor). The trader constructs:
- `new_R_A = old_R_A - Δ_out`
- `new_R_B = old_R_B + Δ_in`

### 19.2 Exact Input (trader specifies how much to deposit)

Given: trader deposits `Δ_in` of asset B, wants asset A.

```
Δ_in_effective = Δ_in * (FEE_DENOM - FEE_BPS) / FEE_DENOM
Δ_out = floor( old_R_A * Δ_in_effective / (old_R_B + Δ_in_effective) )
```

The `floor()` rounds in the pool's favor. The trader constructs:
- `new_R_A = old_R_A - Δ_out`
- `new_R_B = old_R_B + Δ_in`

### 19.3 Price Impact

Spot price before swap: `P = old_R_B / old_R_A` (units of B per unit of A).

Effective execution price: `P_exec = Δ_in / Δ_out`.

Price impact: `(P_exec - P) / P`. Larger swaps relative to reserves incur greater price impact. The wallet UI should display price impact and warn on large trades (e.g., > 5%).

### 19.4 Spot Price After Swap

```
P_after = new_R_B / new_R_A
```

This is the new marginal price. Since `k` has grown (due to fees), the post-swap price reflects both the trade's impact and the fee's reserve growth.

---

## 20. Wallet Requirements

### 20.1 Pool Discovery

The wallet must track pool covenant addresses to identify pool UTXOs on-chain. Because the pool address changes with each LP operation (the `issued_LP` state is embedded in the scriptPubKey), discovery is more involved than a static address scan.

1. **Nostr announcement.** The pool creator publishes pool parameters (asset IDs, fee, covenant CMR) and the current `issued_LP` state as a Nostr event. The wallet derives the current address from the state. Updated after each LP operation.
2. **Chain scanning with known state.** If the wallet knows the current `issued_LP`, it can derive the address and scan for UTXOs. If the state is unknown, the wallet can try sequential values starting from the last known state (LP operations change the state incrementally).
3. **Derivation from market parameters + initial state.** For new pools, the wallet compiles the covenant and derives the initial address from the creation state.

### 20.2 UTXO Identification

The wallet identifies pool reserve UTXOs by:
- Deriving `script_hash(issued_LP)` for the known or estimated state value.
- Matching the scriptPubKey against the derived address.
- Reading the explicit asset ID and amount from each UTXO.
- Grouping the four UTXOs (YES, NO, L-BTC, reissuance token) at the same covenant address.

If any of the four UTXOs is missing (e.g., pool was fully withdrawn), the pool is considered inactive.

### 20.3 Transaction Construction

For swaps, the wallet:

1. Fetches current pool reserve UTXOs (all four).
2. Computes swap amounts (§19) based on user's desired trade.
3. Constructs a PSET with:
   - Inputs 0–3: pool UTXOs (fixed layout).
   - Inputs 4+: trader's funding UTXOs.
   - Outputs 0–3: new pool reserves (fixed layout, to covenant address).
   - Outputs 4+: trader's received tokens, change, fee.
4. Signs the trader's inputs (pool inputs are covenant-satisfied, not signed).
5. Satisfies the pool covenant witnesses (path = swap, swap_pair = appropriate value).
6. Finalizes and broadcasts.

### 20.4 LP Management

All wallets that hold LP tokens should:

- Track LP token balances and the pools they belong to.
- Track the current `issued_LP` state for each pool (needed to derive the current pool address and compute LP value).
- Compute current LP token value: `(reserves * LP_held / issued_LP)` for each asset (off-chain division for display only).
- Provide UI for depositing reserves (any combination) and withdrawing via LP burn.
- Display accrued fees (current LP value vs. initial deposit value).
- Warn when a market's expiry or oracle resolution is approaching (§15). See §16.10 for additional LP display requirements including real-time IL, cumulative fees, and net P&L.

### 20.5 Pool Creator Responsibilities

The pool creator deploys the covenant and funds the initial reserves. After creation, the creator holds LP tokens and has no special on-chain privileges—they are an LP like any other. The creator's wallet should:

- Track pools they have created (by LP token asset ID / covenant address).
- Monitor pool health (reserve ratios, LP utilization, approaching market resolution).
- Publish and maintain Nostr discovery events (§21).

---

## 21. Nostr Discovery (Sketch)

Pool announcements follow the same patterns as maker order Nostr events.

### 21.1 Pool Announcement Event

Publish a replaceable event containing:

- `market_id` (derived from YES/NO asset IDs)
- `yes_asset_id`, `no_asset_id`, `lbtc_asset_id`, `lp_asset_id`
- `lp_reissuance_token_id`
- `issued_lp` (current tapdata state — required to derive the current pool address)
- `fee_bps`
- `covenant_cmr` (contract commitment — used to derive pool addresses for any state)
- `outpoints` (txid:vout for each of the four pool UTXOs)

Use NIP-33 (parameterized replaceable events) so the creator can update the active outpoints and state after liquidity changes.

### 21.2 Reserve and State Updates

After each swap, the pool's UTXOs change (new outpoints). After each LP operation, both the outpoints and the `issued_lp` state change (the pool address transitions). The pool creator—or any observer—can publish an updated event with the new outpoints and state. Takers query by market_id and fetch the latest event.

The `issued_lp` state is critical for discovery: without it, a new observer cannot derive the pool address. Nostr provides this state efficiently. Alternatively, if the observer knows a recent `issued_lp` value, they can scan incrementally by trying nearby state values (LP operations change the state by the minted/burned amount).

### 21.3 Pool Deprecation

When the creator fully withdraws, they publish a deletion event (NIP-09) or an update with empty outpoints, signaling that the pool is closed.

---

## 22. Explicit Amounts and Unblinding

### 22.1 Requirement

The three trading reserve UTXOs (indices 0–2) must use explicit (non-confidential) amounts and asset IDs. The covenant reads reserve values via `get_input_explicit_asset_amount` and `get_output_explicit_asset_amount`, which assert that the values are not Pedersen commitments.

The reissuance token UTXO (index 3) may be confidential. The covenant verifies it via Pedersen commitment checks using witness-provided blinding factors (§5.4). This follows the Astrolabe pattern, where the reissuance token is kept confidential for privacy.

This matches the market covenant's approach: trading UTXOs use explicit amounts for covenant verification, while reissuance tokens may be confidential.

### 22.2 Privacy Implications

Pool reserves are public by design—anyone can see the pool's balances and compute the current price. This is inherent to AMM transparency and is not a privacy concern.

Trader/LP inputs and outputs (indices 4+) may use confidential amounts. The covenant does not inspect non-pool outputs—only the four pool outputs. This preserves trader privacy for trade size while pool state remains public.

### 22.3 Unblinding for Pool Creation

The creation transaction must produce explicit outputs at indices 0–2 (trading reserves) and a properly-formed reissuance token at index 3 (may be confidential). The wallet SDK enforces this during transaction construction. If a creator accidentally creates confidential trading reserve outputs, no covenant-validated spending path will work (the covenant cannot read the reserves from confidential inputs). The funds would be permanently locked. This is why the SDK must validate the creation transaction before broadcast—the creation tx is the one step that is not covenant-protected, and getting it wrong is irrecoverable.

---

## 23. Open Questions

1. **Cosigner policy.** `COSIGNER_PUBKEY` is a compile-time parameter (§4) but no spending path currently uses it. Should the swap path require a cosigner signature (like the maker order's optional cosigner)? A cosigner could gate swaps to authorized takers, but this conflicts with the permissionless design goal. If not needed, remove the parameter. Likely only useful for regulated markets.

2. **Minimum reserves.** What should the minimum reserve per asset be? Too low risks dust UTXOs and precision loss. A compile-time `MIN_RESERVE` parameter (e.g., 1,000 lots / 1,000 sats) could enforce this.

3. **Nostr announcement format.** Pool discovery should follow the same Nostr event patterns as maker orders. Event schema (tags, replaceability, indexing) needs specification. The `issued_lp` state must be included in pool announcements for address derivation.

4. **Multi-pool routing.** When multiple pools exist for the same market, taker software needs a routing algorithm. This is an off-chain/wallet concern but affects UX design.

5. **LP deposit fees.** Should LP deposits/withdrawals incur a fee (captured by remaining LPs)? The cubic model already penalizes unbalanced operations via price impact, but a small explicit fee could discourage deposit/withdraw cycling. Currently no fee is enforced on the LP path—only the swap path has fees.

6. **Initial issued_LP granularity.** The creator chooses the initial `issued_LP` at pool creation. A larger value gives future depositors more granularity (more fractional LP positions possible) but increases the magnitude of values in the cubic check. Recommend choosing initial reserves and LP count so that `product / issued_LP^3` is a clean power of 10.

## Resolved Questions

- **LP token initial minting:** Creator mints via standard Liquid issuance at creation time; subsequent minting via covenant reissuance path (§9.2).
- **SimplicityHL arithmetic support:** Composed wide arithmetic from `jet::multiply_64` — no `multiply_128` jet needed (§8.3).
- **LP token supply ceiling:** Dynamic reissuance with tapdata state replaces fixed `TOTAL_LP_SUPPLY` (§5.1, §5.3).
- **LP burn mechanism:** OP_RETURN output with `SHA256("") = 0xe3b0c44...` — same pattern as prediction_market.simf token cancellation (§18.9).
- **Issuance amount reading:** `jet::issuance_asset_amount(index)` returns a nested Option type (see §18.9) — same jet as prediction_market.simf:240–242.
- **Explicit vs. confidential amounts:** Trading reserves (indices 0–2) use explicit amounts; reissuance token (index 3) may be confidential with Pedersen verification (§22, §5.4).
