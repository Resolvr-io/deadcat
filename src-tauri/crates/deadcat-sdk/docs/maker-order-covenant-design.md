# SimplicityHL Maker-Order Covenant — Design Document

## Status

Draft v2.0 — Supersedes the original design doc (`simplicityhl-maker-order-covenant-design-doc.md`).

---

## 1. Overview

Maker limit orders are UTXOs locked by a SimplicityHL covenant on Liquid. A maker posts an order by funding a covenant-locked UTXO; a taker fills it by constructing a transaction that satisfies the covenant's validation rules. Partial fills produce a remainder UTXO locked by the same covenant. Nostr serves as the off-chain discovery layer; the chain is the source of truth.

This is a **separate covenant** from the prediction market contract, with its own `.simf` file, module tree, taproot construction, and PSET builders.

---

## 2. Goals

- Implement maker limit orders as covenant-locked UTXOs.
- Support partial fills with automatic remainder creation.
- Divisionless on-chain arithmetic (no division/rounding in the covenant).
- Enable multi-maker, multi-taker batched fills in a single transaction.
- Grief-resistant batch construction — any participant can be dropped without re-signing.
- Optional cosigner for high-volume serialization (double-spend prevention).
- Mnemonic-only wallet recovery (no external state required).
- Unblinded outputs where required for covenant introspection.

## 3. Non-goals

- Vault-style mint/merge mechanics for YES/NO issuance/redeem (layered separately).
- Central matching engine (optional relayers are fine, not required).
- Nostr event schema (out of scope for the SDK; wallet/relayer concern).

---

## 4. Assets and Units

| Term | Definition |
|------|-----------|
| **QUOTE** | Quote asset (e.g. L-BTC); smallest unit = sat |
| **BASE** | Outcome token (YES or NO); 1 token unit = 1 lot |
| **PRICE** | Integer quote-units per BASE lot, range `[1, 2^64)` |

Payment is always `lots * PRICE` — no division, no rounding.

**Complement pricing** (wallet-level only): if using a fixed collateral-per-lot constant `C`, then `p_no = C - p_yes`. This is off-chain metadata, not enforced by the covenant.

---

## 5. Design Decisions and Tradeoffs

### 5.1 Key-path cancel (no EXPIRY parameter)

**Decision:** The maker's base pubkey is the taproot internal key. Cancellation is a key-path spend — no covenant code executes, no cosigner is required.

**Tradeoff considered:** The original design included an `EXPIRY` parameter that would gate when fills are allowed. With key-path cancel, the maker can cancel at any time by signing with their key, making expiry redundant for the cancel path. An expiry could still protect makers who lose access to their signing key (orders would eventually become unspendable by takers), but this edge case doesn't justify the added covenant complexity. Makers who want time-limited orders can cancel and recreate.

**Why not covenant-path cancel?** Requiring covenant execution for cancellation would mean the cosigner could block cancellations. Key-path cancel is unconditional — the maker always retains full sovereignty over their funds, regardless of cosigner availability.

### 5.2 P_order script hash baked in (not derived in-covenant)

**Decision:** The maker's unique receive address (`P_order`) is derived off-chain by the wallet. Only its script hash (`SHA256(scriptPubKey)`) is baked into the covenant as a compile-time parameter.

**Alternatives considered:**
1. **Bake in x-only pubkey, derive P2TR in-covenant** — Requires the covenant to compute the P2TR encoding (`OP_1 <32-byte-key>`) and hash it. Adds ~3 jets but provides no functional benefit since the wallet must still derive the pubkey off-chain.
2. **Full in-covenant derivation from maker pubkey + nonce** — Requires EC point arithmetic jets (`scalar_multiply`, `point_add`), which are expensive in Simplicity. Adds significant program size and verification cost.

**Why script hash wins:** Minimizes covenant size. The wallet already knows the full derivation path. Batching safety is preserved because each order has a unique `MAKER_RECEIVE_SPK_HASH`, so two covenants can never share a single maker receive output.

### 5.3 No LOT_SATS in covenant

**Decision:** The `LOT_SATS` (collateral-per-lot) constant from the original design is removed from covenant parameters entirely. It exists only as off-chain metadata.

**Reasoning:** The covenant never uses `LOT_SATS` in any validation equation. All conservation checks use `PRICE` directly: `payment = lots * PRICE`. The complement pricing formula `p_no = LOT_SATS - p_yes` is a wallet convenience, not a covenant invariant.

### 5.4 Fill lots derived from outputs (zero fill-lot witnesses)

**Decision:** Instead of the taker providing `fill_lots` as a witness value, the covenant derives it from introspection of the transaction's input and output amounts.

**How it works:**
- **Sell-BASE full fill:** `maker_amount == input_amount * PRICE` (all lots consumed)
- **Sell-BASE partial fill:** `consumed = input_amount - remainder_amount`, then `maker_amount == consumed * PRICE`
- **Sell-QUOTE full fill:** `maker_amount * PRICE == input_amount` (all quote consumed)
- **Sell-QUOTE partial fill:** `maker_amount * PRICE + remainder_amount == input_amount`

**Tradeoff:** This requires the covenant to read one additional output (the remainder) on partial fills. But it eliminates a witness value and removes the need to trust the witness — the amounts are verified directly from the transaction.

**Division-free property preserved:** All equations use only multiplication, addition, and subtraction. No division is ever needed.

### 5.5 Fixed output indices (no witness-provided indices)

**Decision:** The maker receive output is always at `output[current_index]`. The remainder output (partial fills only) is always at `output[current_index + 1]`.

**Attack prevented:** With dynamic witness-provided output indices, two identical covenants in the same transaction could both point their `MAKER_OUTPUT_IDX` or `REMAINDER_IDX` at the same output, satisfying both covenants with a single payment. Fixed indices tied to `current_index` make this impossible — each covenant can only reference its own positional output.

**Constraint introduced:** Only the last maker order in a batch can be partially filled (its remainder at `current_index + 1` would collide with the next maker's receive output otherwise). This is acceptable for all practical use cases — the PSET builder enforces this ordering.

### 5.6 IS_SELL_BASE direction flag

**Decision:** A compile-time boolean `IS_SELL_BASE` determines whether the order input holds BASE (maker sells BASE for QUOTE) or QUOTE (maker sells QUOTE for BASE).

**Why a single covenant with a branch?** Both directions share 90% of their logic (cosigner check, script hash validation, conservation arithmetic). Two separate contracts would duplicate all shared code and double the maintenance surface. The direction flag adds a single branch with minimal overhead.

### 5.7 Maker pubkey as taproot internal key

**Decision:** The taproot internal key is the maker's base pubkey, not a NUMS (Nothing Up My Sleeve) key.

**Consequence:** The taproot tree has a single leaf (the Simplicity covenant). There is no tapdata leaf for state encoding (unlike the prediction market contract which uses a 2-leaf tree: Simplicity + tapdata). The control block is 33 bytes (`[0xbe | maker_base_pubkey]`) vs. 65 bytes for the prediction market.

**Key-path spending:** Because the internal key is the maker's real pubkey, the maker can always spend the UTXO via key-path (BIP-341 key-path spend). This provides the unconditional cancel mechanism described in §5.1.

---

## 6. Covenant Specification

### 6.1 Parameters (compile-time, 8)

| Name | Type | Description |
|------|------|-------------|
| `BASE_ASSET_ID` | `u256` | Outcome token asset ID |
| `QUOTE_ASSET_ID` | `u256` | Quote asset ID (e.g. L-BTC) |
| `PRICE` | `u64` | Quote units per BASE lot |
| `MIN_FILL_LOTS` | `u64` | Minimum lots per fill |
| `MIN_REMAINDER_LOTS` | `u64` | Minimum lots remaining after partial fill |
| `IS_SELL_BASE` | `bool` | `true` = maker offers BASE, wants QUOTE |
| `MAKER_RECEIVE_SPK_HASH` | `u256` | SHA256 of P_order scriptPubKey |
| `COSIGNER_PUBKEY` | `u256` | Optional cosigner x-only pubkey (NUMS = bypass) |

### 6.2 Witnesses (1)

| Name | Type | Description |
|------|------|-------------|
| `COSIGNER_SIGNATURE` | `Signature` | 64-byte Schnorr signature (zeros when no cosigner) |

### 6.3 Spend paths

There are two spend paths for an order UTXO:

1. **Key-path cancel** — Maker signs with their (tweaked) key. No covenant executes. No cosigner needed.
2. **Script-path fill** — Taker constructs a transaction satisfying the covenant. Cosigner signs if enabled.

The covenant (script-path) validates fills only. There is no covenant-level cancel logic.

### 6.4 Covenant logic (pseudocode)

```
fn main() {
    let cosigner_sig: Signature = witness::COSIGNER_SIGNATURE;
    let i: u32 = jet::current_index();
    let i_rem: u32 = jet::add_32(i, 1);  // safe: i < 2^31 in practice

    // ── COSIGNER CHECK (custom sighash) ──
    // Skip if COSIGNER_PUBKEY == NUMS (no cosigner configured)
    if COSIGNER_PUBKEY != NUMS_KEY:
        let ctx = sha256_init()
        ctx = sha256_add(ctx, input_prev_outpoint(i))    // 36 bytes
        ctx = sha256_add(ctx, output_asset(i))            // 32 bytes
        ctx = sha256_add(ctx, output_amount(i))           //  8 bytes
        ctx = sha256_add(ctx, output_script_hash(i))      // 32 bytes
        let msg = sha256_finalize(ctx)
        bip_0340_verify(COSIGNER_PUBKEY, msg, cosigner_sig)

    // ── READ TRANSACTION DATA ──
    let (input_asset, input_amount) = introspect_input(i)
    let (maker_asset, maker_amount) = introspect_output(i)
    assert output_script_hash(i) == MAKER_RECEIVE_SPK_HASH

    // ── DIRECTION BRANCH ──
    if IS_SELL_BASE:
        // Maker sells BASE lots, receives QUOTE
        assert input_asset == BASE_ASSET_ID
        assert maker_asset == QUOTE_ASSET_ID

        let is_full: bool = (maker_amount == input_amount * PRICE)
        if is_full:
            assert input_amount >= MIN_FILL_LOTS
        else:
            let (rem_asset, rem_amount) = introspect_output(i_rem)
            assert rem_asset == BASE_ASSET_ID
            let consumed = input_amount - rem_amount
            assert maker_amount == consumed * PRICE     // conservation
            assert consumed >= MIN_FILL_LOTS
            assert rem_amount >= MIN_REMAINDER_LOTS
            assert output_script_hash(i_rem) == input_script_hash(i)
    else:
        // Maker sells QUOTE, receives BASE lots
        assert input_asset == QUOTE_ASSET_ID
        assert maker_asset == BASE_ASSET_ID

        let is_full: bool = (maker_amount * PRICE == input_amount)
        if is_full:
            assert maker_amount >= MIN_FILL_LOTS
        else:
            let (rem_asset, rem_amount) = introspect_output(i_rem)
            assert rem_asset == QUOTE_ASSET_ID
            assert maker_amount * PRICE + rem_amount == input_amount  // conservation
            assert maker_amount >= MIN_FILL_LOTS
            assert rem_amount >= MIN_REMAINDER_LOTS * PRICE
            assert output_script_hash(i_rem) == input_script_hash(i)
}
```

### 6.5 Conservation invariants

All equations are **divisionless** — only multiply, add, subtract:

| Direction | Fill type | Invariant |
|-----------|-----------|-----------|
| Sell-BASE | Full | `maker_amount == input_amount * PRICE` |
| Sell-BASE | Partial | `maker_amount == (input_amount - rem_amount) * PRICE` |
| Sell-QUOTE | Full | `maker_amount * PRICE == input_amount` |
| Sell-QUOTE | Partial | `maker_amount * PRICE + rem_amount == input_amount` |

### 6.6 Remainder script continuity

On partial fills, the remainder output must have the same script hash as the input being spent:

```
output_script_hash(current_index + 1) == input_script_hash(current_index)
```

This ensures the remainder UTXO is locked by the **exact same covenant** — same parameters, same CMR, same taproot construction. Anyone can fill the remainder order under identical rules.

---

## 7. Batching Safety

### 7.1 Per-order unique P_order

Each order derives a unique maker receive scriptPubKey via EC tweak:

```
order_uid = SHA256("deadcat/order_uid" || maker_base_pubkey || order_nonce || BASE_ASSET_ID || QUOTE_ASSET_ID || PRICE || ...)
tweak     = SHA256("deadcat/order_tweak" || order_uid)
P_order   = P_maker_base + tweak * G
```

The corresponding `MAKER_RECEIVE_SPK_HASH = SHA256(OP_1 || PUSH32 || P_order)` is baked into the covenant.

**Result:** Two covenants in the same transaction cannot share a single maker receive output unless `P_order` collides (cryptographically infeasible). No iteration or global uniqueness check required.

### 7.2 Wallet restoration

If `order_nonce` is derived from the mnemonic via an HD index (like standard address derivation), then **no external state** is needed to recover:

- **Active orders:** Scan for covenant scripts over indices within a gap limit.
- **Maker receipts:** Scan for P_order tweaked pubkeys over the same index range.

Nostr accelerates discovery (query by maker pubkey) but is not required for recovery.

---

## 8. Cosigner Architecture

### 8.1 Motivation: the serialization problem

Without a cosigner, high-value trading creates a race condition: two takers can construct fills for the same order UTXO simultaneously. Only one transaction confirms; the other is a wasted double-spend attempt. For low-frequency trading this is acceptable. For high-volume markets, it causes significant UX degradation and potential MEV-like extraction.

### 8.2 Optional cosigner via COSIGNER_PUBKEY

The covenant accepts a `COSIGNER_PUBKEY` parameter. When set to a real pubkey, fills require a valid Schnorr signature from the cosigner. When set to the NUMS key (`0x50929b74...`), the signature check is bypassed and the order operates in permissionless mode.

**Trust model:** The cosigner can **censor** (refuse to sign a fill) but cannot **steal** (it never holds keys to the order UTXO or the maker's funds). The maker retains unconditional cancel via key-path spend regardless of cosigner cooperation.

### 8.3 Custom sighash (position-independent)

The cosigner does **not** use the standard `sig_all_hash()`. Instead, the covenant computes a custom message from data at `current_index`:

```
cosigner_msg = SHA256(
    input_prev_outpoint(current_index) ||    // 36 bytes: which order UTXO
    output_asset(current_index)         ||    // 32 bytes: what asset the maker receives
    output_amount(current_index)        ||    //  8 bytes: how much the maker receives
    output_script_hash(current_index)         // 32 bytes: where the maker receives
)
```

**Why custom sighash?** Standard `sig_all_hash()` commits to the full transaction — all inputs, all outputs, indices. If any participant is added or removed, the hash changes and the signature is invalidated. The custom sighash commits only to order-specific data that **shifts with the input when items are added/removed**, making it position-independent.

### 8.4 Future: high-volume cosigning server

The cosigner architecture is designed to support a future dedicated cosigning server that coordinates fills across many concurrent orders. Here is the envisioned operation:

**Server responsibilities:**
1. **Order registration:** Makers register their order UTXOs with the server (or the server discovers them via Nostr).
2. **Fill serialization:** When a taker requests a fill, the server locks the relevant order UTXO(s) to prevent concurrent fills. If the taker doesn't complete within a timeout, the lock expires.
3. **Batch construction:** The server can aggregate multiple taker requests into a single transaction, filling multiple orders atomically.
4. **Cosigner signing:** For each order in the batch, the server computes the custom sighash and produces a Schnorr signature. Because the sighash is position-independent, the server signs once per order and these signatures remain valid regardless of how the final transaction is composed.
5. **Grief handling:** If a taker fails to sign or disappears, the server drops that taker's input/output pair from the batch. All other signatures (from other takers and the cosigner) remain valid. The server resubmits the reduced transaction without any re-signing.

**Server trust properties:**
- **Cannot steal funds:** The cosigner key has no spending authority over order UTXOs (maker's key is the internal key) or over maker/taker receive outputs.
- **Cannot forge fills:** The covenant validates conservation invariants independently. The cosigner signature only authorizes the fill — it doesn't bypass any validation.
- **Can censor:** The server can refuse to sign fills for specific orders or takers. This is the cost of serialization. Mitigation: makers can deploy orders with `COSIGNER_PUBKEY = NUMS` to opt out.
- **Can go offline:** If the server is unavailable, orders with a cosigner are temporarily unfillable. Makers can always cancel via key-path and recreate without a cosigner.

**Server doesn't need to be trusted for correctness** — the chain validates everything. The server is trusted only for **liveness** (it must be online to sign) and **fairness** (it shouldn't discriminate). Both properties can be monitored and enforced socially or contractually.

---

## 9. Grief-Resistant Transaction Layout

### 9.1 The griefing problem

In a multi-taker batch, if one taker refuses to sign (or disappears after receiving PSET details), the entire transaction is blocked. Rebuilding requires all remaining participants to re-sign.

### 9.2 Takers-first layout

```
Inputs:  [0..T-1]     T taker funding inputs   (signed ANYONECANPAY|SINGLE)
         [T..T+M-1]   M maker order inputs      (covenant script-path spend)
         [T+M]        fee input

Outputs: [0..T-1]     T taker receive outputs   (1:1 matched with taker inputs)
         [T..T+M-1]   M maker receive outputs   (1:1 matched with order inputs)
         [T+M]        remainder                  (only if last order is partial fill)
         [T+M+1..]    fee, change
```

### 9.3 Why this layout is grief-resistant

Three properties combine to allow any participant to be dropped without invalidating other signatures:

**1. Taker signatures: ANYONECANPAY | SINGLE**

Per BIP-341 sighash computation:
- `ANYONECANPAY` (bit 0x80): excludes `sha_inputs`, `sha_amounts`, `sha_scriptpubkeys`, `sha_sequences`, and critically, **`input_index`** from the sighash.
- `SINGLE` (0x03): commits to only the output at the same index as the input.

Because `input_index` is excluded when ANYONECANPAY is set (BIP-341: "If `hash_type & 0x80` does NOT equal `SIGHASH_ANYONECANPAY`: `input_index` (4)"), removing any input/output pair and shifting subsequent indices does not change the data that any taker signed. Each taker's signature commits to: their specific input's prevout + their matched output — nothing else.

**2. Cosigner signatures: custom position-independent sighash**

The cosigner's message is `SHA256(input_outpoint(i) || output_data(i))` where `i = current_index`. When an input/output pair is removed and indices shift, the data at `current_index` for every remaining order is unchanged (it moved with the input). The cosigner signature remains valid.

**3. Covenant validation: introspection at current_index**

The covenant reads `input_amount(current_index)`, `output_amount(current_index)`, etc. These values are relative to the input's position and shift with it. No absolute index is hardcoded.

**Result:** Any taker input/output pair can be dropped. Any maker order input/output pair can be dropped (unless it's not the last one and there's a partial fill after it). The fee input and change outputs at the end can be adjusted freely. No signature from any remaining participant is invalidated.

### 9.4 Partial fill constraint

Only the **last** maker order in the batch may be partially filled. A partial fill produces a remainder at `output[current_index + 1]`. If a partially-filled order were in the middle, its remainder output would occupy the slot needed by the next order's maker receive output.

This is enforced by the PSET builder, not the covenant. The covenant simply checks `current_index` and `current_index + 1` — it has no concept of batch position.

---

## 10. Taproot Construction

### 10.1 Tree structure

```
Internal key: maker_base_pubkey (real key — enables key-path cancel)
Leaves:       [Simplicity leaf (version 0xbe, CMR of maker_order.simf)]
```

Single leaf — no tapdata, no state encoding. The covenant is stateless.

### 10.2 Taptweak

```
leaf_hash  = TaggedHash("TapLeaf", [0xbe || CMR])
tweak      = TaggedHash("TapTweak", [maker_base_pubkey || leaf_hash])
output_key = maker_base_pubkey + tweak * G
```

### 10.3 Control block

33 bytes: `[0xbe | maker_base_pubkey]`

Compared to the prediction market's 65-byte control block (`[0xbe | NUMS_KEY | tapdata_sibling_hash]`), the maker order control block is smaller because there's only one leaf and no sibling to prove.

### 10.4 Script pubkey

Standard P2TR: `OP_1 <32-byte output_key>`

### 10.5 Key-path spend (cancel)

The maker signs using BIP-341 key-path spending. The tweaked private key is:
```
d_tweaked = d_maker + tweak
```
where `d_maker` is the maker's private key and `tweak` is the taptweak scalar. Standard wallet software can compute this.

---

## 11. P_order Derivation Specification

### 11.1 Order UID

```
order_uid = SHA256(
    "deadcat/order_uid" ||
    maker_base_pubkey   ||    // 32 bytes
    order_nonce         ||    // 32 bytes (HD-derived)
    BASE_ASSET_ID       ||    // 32 bytes
    QUOTE_ASSET_ID      ||    // 32 bytes
    PRICE               ||    //  8 bytes (big-endian)
    MIN_FILL_LOTS       ||    //  8 bytes (big-endian)
    MIN_REMAINDER_LOTS  ||    //  8 bytes (big-endian)
    IS_SELL_BASE              //  1 byte (0x00 or 0x01)
)
```

### 11.2 Tweak and P_order

```
tweak   = SHA256("deadcat/order_tweak" || order_uid)
P_order = P_maker_base + tweak * G
```

### 11.3 Maker receive scriptPubKey

```
maker_receive_spk = OP_1 <P_order>    // P2TR, 34 bytes
```

### 11.4 Covenant parameter

```
MAKER_RECEIVE_SPK_HASH = SHA256(maker_receive_spk)
```

### 11.5 HD derivation for order_nonce

`order_nonce` is derived from the mnemonic via a dedicated HD path, analogous to address derivation. The exact HD path is a wallet implementation detail (e.g. `m/purpose'/coin'/account'/order_index`). The wallet implements a gap-limit scanning policy to discover active orders and receipts on recovery.

---

## 12. Fees and Incentives

- No covenant-level protocol fee or relayer fee.
- **Maker** pays transaction fees when creating the order UTXO.
- **Taker** pays transaction fees when filling (including remainder output creation).
- Wallet/relayer policy may impose batching caps for UX/propagation, but there is no covenant-enforced limit.

---

## 13. Transaction Templates

### 13.1 Create order

```
Inputs:  [0] funding input (offered asset)
         [1] fee input
Outputs: [0] order UTXO -> covenant address
         [1] fee output
         [2] change (optional, if funding > order amount)
         [3] fee change (optional)
```

### 13.2 Single taker, single maker fill

```
Inputs:  [0] taker funding input     (ANYONECANPAY|SINGLE)
         [1] maker order input        (covenant script-path)
         [2] fee input
Outputs: [0] taker receive output
         [1] maker receive output     (P_order script, exact amount)
         [2] remainder output         (optional, same covenant script)
         [3] fee output
         [4] fee change               (optional)
```

### 13.3 Multi-taker, multi-maker batch fill

```
Inputs:  [0]     taker1 funding       (ANYONECANPAY|SINGLE)
         [1]     taker2 funding       (ANYONECANPAY|SINGLE)
         [2]     order1 input         (covenant)
         [3]     order2 input         (covenant)
         [4]     fee input
Outputs: [0]     taker1 receive
         [1]     taker2 receive
         [2]     maker1 receive       (P_order1)
         [3]     maker2 receive       (P_order2)
         [4]     remainder            (only if order2 is partial)
         [5]     fee
         [6]     fee change           (optional)
```

### 13.4 Cancel order

```
Inputs:  [0] order UTXO (key-path spend -- maker signature)
         [1] fee input
Outputs: [0] refund to maker
         [1] fee output
         [2] fee change (optional)
```

---

## 14. SDK Implementation Plan

### 14.1 New files

| File | Purpose |
|------|---------|
| `contract/maker_order.simf` | SimplicityHL covenant source |
| `src/maker_order/mod.rs` | Module root |
| `src/maker_order/params.rs` | `MakerOrderParams`, `OrderDirection`, P_order derivation |
| `src/maker_order/contract.rs` | `CompiledMakerOrder` (compile, CMR, addresses) |
| `src/maker_order/taproot.rs` | Taproot with real internal key, single leaf |
| `src/maker_order/witness.rs` | Cosigner signature witness satisfaction |
| `src/maker_order/pset/mod.rs` | PSET submodule root |
| `src/maker_order/pset/create_order.rs` | Create order PSET builder |
| `src/maker_order/pset/fill_order.rs` | Fill order PSET builder (takers-first layout) |
| `src/maker_order/pset/cancel_order.rs` | Cancel order PSET builder |

### 14.2 Modified files

| File | Change |
|------|--------|
| `src/taproot.rs` | Promote `tagged_hash` and `taptweak_hash` to `pub(crate)` |
| `src/error.rs` | Add error variants for maker order validation |
| `src/lib.rs` | Add `pub mod maker_order` and re-exports |

### 14.3 Implementation phases

1. **Phase 1 — Foundation** (parallel): taproot visibility, error variants, `.simf` contract
2. **Phase 2 — Core module** (depends on 1): params, taproot, witness modules
3. **Phase 3 — Contract compilation** (depends on 2): `CompiledMakerOrder`
4. **Phase 4 — PSET builders** (depends on 3): create, fill, cancel
5. **Phase 5 — Integration** (depends on all): lib.rs exports, design doc update, tests

### 14.4 Testing

**Unit tests** (per module): params determinism, taproot format, control block size (33 bytes), P_order derivation

**Integration tests** (`tests/maker_order.rs`):
- Compilation: sell-base, sell-quote, different prices produce different CMRs
- Witness satisfaction: with cosigner, without cosigner (NUMS pubkey)
- Create order: happy path, with change
- Fill order: partial/full fills for both directions, batch with 2 takers + 2 orders, fill-below-minimum rejection, remainder-below-minimum rejection, maker-receive-hash mismatch rejection
- Cancel order: happy path, with fee change

**Verification:** `cargo check && cargo test && cargo fmt --check && cargo clippy`

---

## 15. Open Questions

1. Finalize exact HD path for `order_nonce` derivation (e.g. `m/purpose'/coin'/account'/order_index`).
2. Nostr event kind/schema for order publication (out of scope for SDK, but needed for interop).
3. Gap limit policy for wallet scanning (recommended: same as address gap limit).
4. Whether to support taker-provided fee bump (CPFP) as an alternative to pre-funded fee inputs.
