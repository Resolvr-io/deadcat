## SimplicityHL Maker-Order Covenant Design (Liquid + Nostr)

### Status

Draft v0.1

---

## Goals

- Implement maker limit orders as **UTXOs locked by a SimplicityHL covenant**.
- **Partial fills are required**, producing a **remainder UTXO** that preserves the order.
- **Divisionless on-chain logic** (no price division/rounding inside the covenant).
- Support **batching**: a single taker transaction may consume **multiple maker orders** safely.
- Use **Nostr as the discovery layer** (no central orderbook); the chain is the source of truth.
- Permit **unblinded outputs** where required for covenant validation.

## Non-goals (for this doc)

- Vault-style “mint/merge” mechanics for YES/NO issuance/redeem (can be layered later).
- Central matching engine requirements (optional relayers are fine, but not required).

---

## Assets and units

- **QUOTE**: L-BTC; smallest unit = sat.
- **BASE**: an outcome token (YES or NO), issued such that **1 token unit = 1 lot**.
- **LOT_SATS**: collateral-per-lot constant (**1,000 sats** initially).
- **Price**: `p` = **sats per BASE lot**, integer `0..LOT_SATS`.
  - Covenant payment is always `q * p` (no division).

Convenience:

- If you want YES/NO complement symmetry at the wallet level: `p_no = LOT_SATS - p_yes`.

---

## High-level lifecycle

1. **Maker creates** an order by locking funds into an **order UTXO** (covenant script).
2. Maker **publishes** order terms + outpoint on **Nostr**.
3. **Taker fills** by creating a tx that spends the order UTXO and satisfies covenant rules.
4. If partially filled, tx recreates a **remainder order UTXO** with the **same covenant**.

---

## Key decision: Option A (witness indices + per-order unique maker receive script)

### Why

Batching multiple maker inputs in one tx is safe **without loops** if each covenant validates a constant set of outputs and the **maker’s receive output is unique per order** (so two maker inputs can’t “share” one payout output).

### Per-order unique maker receive script

Each order derives an **order-unique scriptPubKey** for the maker’s receive output:

- `order_nonce`: per-order unique value derived deterministically by the wallet.
- `order_uid = H(tag || maker_base_pubkey || order_nonce || BASE || QUOTE || p || LOT_SATS || expiry || min_* …)`
- `tweak = H(tag2 || order_uid)`
- `P_order = P_maker_base + tweak·G`
- Maker receive output script is pay-to-`P_order`.

#### Wallet restore property (mnemonic-only)

If `order_nonce` is derived from the mnemonic via an HD index (like address derivation), then **no extra stored state is required** to recover:

- active order UTXOs (scan for covenant scripts over indices within a gap limit)
- maker receipts (scan for tweaked pubkeys over the same index range)

Nostr can accelerate discovery (query by maker pubkey), but recovery should not depend on it.

**Wallet requirement:** implement an **order-index gap limit** policy.

---

## One covenant, both directions

Single covenant supports:

- **Sell-BASE order**: maker offers BASE lots, wants QUOTE sats.
- **Sell-QUOTE order**: maker offers QUOTE sats, wants BASE lots.

This is a single contract with a small branch on direction; no division required.

---

## Covenant parameters

### Constants baked into the covenant

- `BASE_asset_id`
- `QUOTE_asset_id` (L-BTC)
- `LOT_SATS` (1000)
- `p` (sats per BASE lot)
- `expiry` (optional; 0 = no expiry)
- `min_fill_lots`
- `min_remainder_lots`
- `maker_base_pubkey` (for deriving `P_order`)
- `order_nonce`

### Witness provided at fill time

- `q` (fill lots)
- `i_maker` (output index for maker receive output)
- `i_rem` (output index for remainder; only on partial fills)

> Intentional: the covenant does **not** need to validate the taker’s receive output. It enforces maker receipt + remainder conservation; takers can aggregate outputs as desired.

---

## Spend paths and invariants

### Common checks (all fills)

- **Expiry**: allow fill iff `expiry == 0 || now <= expiry`.
- **q constraints**:
  - `q >= min_fill_lots`
  - `q > 0`

- **Maker output index**:
  - output `i_maker` exists
  - output scriptPubKey == `P_order` (derived inside covenant)

- **Unblinding requirement**:
  - order input amount/asset unblinded
  - maker receive output amount/asset unblinded
  - remainder output amount/asset unblinded (partial fills)

---

### A) Maker sells BASE (order input holds BASE lots)

Let:

- `in_base = input_amount` (lots)
- `pay_sats = q * p`
- `rem_base = in_base - q`

#### Full fill (`q == in_base`)

- Maker output `i_maker`:
  - asset == QUOTE
  - amount == `pay_sats`
  - spk == `P_order`

- No remainder required.

#### Partial fill (`q < in_base`)

- Require `rem_base >= min_remainder_lots`.
- Maker output `i_maker` as above.
- Remainder output `i_rem`:
  - asset == BASE
  - amount == `rem_base`
  - spk == **same covenant script** (exact script hash/commitment)

---

### B) Maker sells QUOTE (order input holds QUOTE sats)

Let:

- `in_sats = input_amount` (sats)
- `pay_sats = q * p`
- `rem_sats = in_sats - pay_sats`

#### Full fill (`pay_sats == in_sats`)

- Maker output `i_maker`:
  - asset == BASE
  - amount == `q` (lots)
  - spk == `P_order`

- No remainder required.

#### Partial fill (`pay_sats < in_sats`)

- Require `rem_sats >= min_remainder_lots * p`.
- Maker output `i_maker` as above.
- Remainder output `i_rem`:
  - asset == QUOTE
  - amount == `rem_sats`
  - spk == same covenant script

> Note: no division needed; remainder is computed as `in_sats - q*p`.

---

## Why batching is safe (prevents shared payout output bug)

Each maker input requires its maker receive output to:

- pay to a **per-order unique scriptPubKey** (`P_order`), and
- have an **exact** amount (`q*p` sats or `q` lots depending on direction).

Therefore, a single output cannot satisfy two different maker inputs unless `P_order` collides (assumed cryptographically infeasible). This avoids needing global uniqueness constraints or iteration.

---

## Fees and incentives

- No covenant-level protocol fee or relayer fee (current decision).
- **Maker** pays fees when creating the order UTXO.
- **Taker** pays fees when filling (and for creating remainder outputs).
- **Wallet/relayer policy** may impose a batching cap for UX/propagation reasons, but **no covenant-enforced cap**.

---

## Expiry and cancellation

- `expiry` is optional:
  - `expiry == 0` means “never expires.”

- **Cancel path** (recommended):
  - maker can always cancel by spending the order UTXO with a signature under `maker_base_pubkey`.

---

## Transaction templates

### Single maker full fill (sell BASE)

Inputs:

- maker order UTXO: `BASE in_base`
- taker inputs sufficient to pay `QUOTE pay_sats`

Outputs:

- maker receive (unique `P_order`): `QUOTE pay_sats`
- taker outputs (any structure; can be aggregated/blinded)

### Multi-maker batch fill

Inputs:

- order1, order2, …, orderN
- taker provides enough QUOTE/BASE as required

Outputs:

- maker1 receive @ `P_order1`
- maker2 receive @ `P_order2`
- …
- optional aggregated taker outputs
- remainder outputs for any partially-filled orders

---

## Nostr discovery (sketch)

Publish an event containing:

- `outpoint` (txid:vout)
- `BASE_asset_id`, `QUOTE_asset_id`
- `p`, `LOT_SATS`
- direction (`maker_sells`: BASE|QUOTE)
- `expiry`, `min_fill_lots`, `min_remainder_lots`
- covenant script hash/commitment

Use replaceable/addressable events so makers can update the active outpoint after partial fills.

---

## Wallet requirements

- Deterministic **order index / nonce derivation** from mnemonic.
- Gap-limit scanning for:
  - covenant scripts (active/remainder orders)
  - tweaked maker receive pubkeys (`P_order`)

- Policy knobs:
  - batching cap (wallet/relayer only)
  - whether to allow multiple partial fills in one tx

---

## Open questions (small)

1. Finalize exact derivation spec for `order_nonce` (HD path + encoding) and `P_order` tweak hashing/tagging.
2. Cancel policy: always allowed vs only before expiry (recommended: always allowed).
3. Do we enforce “QUOTE input must be multiple of p” for sell-QUOTE orders? (likely wallet policy).
4. Finalize Nostr event kind/schema (tags, replaceability, indexing strategy).
