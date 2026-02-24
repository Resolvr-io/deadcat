# Market Currency Conventions

## Status

v1.0 — Design rationale for base/quote pair ordering and rational pricing in the maker order covenant.

---

## 1. Overview

The maker order covenant (`maker_order.simf`) uses a generic BASE/QUOTE model with two compile-time asset IDs and a direction flag. This document explains:

1. Why the convention **must** be BASE = outcome token, QUOTE = L-BTC.
2. Why the direction flag (SellBase vs SellQuote) handles both sides of the order book.
3. Why flipped pairs (L-BTC/YES) are rejected.
4. Why PRICE is expressed as a rational number (NUM/DENOM) with ceiling rounding, rather than a plain integer.

---

## 2. The Two Mechanisms

### 2.1 Pair ordering: which asset is BASE vs QUOTE

`BASE_ASSET_ID` and `QUOTE_ASSET_ID` are two asset slots baked into the covenant at compile time. The `PRICE` is always interpreted as **quote units per BASE lot**.

The covenant itself is asset-agnostic — it does not enforce which Liquid asset occupies which slot. But the wallet convention must be fixed:

```
BASE_ASSET_ID  = outcome token (YES or NO)
QUOTE_ASSET_ID = L-BTC
PRICE          = satoshis per outcome token (expressed as PRICE_NUM / PRICE_DENOM)
```

### 2.2 Direction flag: which side of the market

`IS_SELL_BASE` (the `OrderDirection` enum in Rust) determines what the maker is offering:

| Direction | Maker offers | Maker wants | Order book role |
|-----------|-------------|-------------|-----------------|
| SellBase  | outcome tokens (BASE) | L-BTC (QUOTE) | Ask / sell order |
| SellQuote | L-BTC (QUOTE) | outcome tokens (BASE) | Bid / buy order |

Both directions use the same PRICE unit (sats per token) and the same pair ordering. The direction just controls which asset sits in the covenant UTXO and which asset the maker receives on fill.

---

## 3. Why Not Allow Flipped Pairs?

One might ask: instead of a direction flag, could we express buy orders by flipping the pair to BASE = L-BTC, QUOTE = YES? This would give `PRICE` = tokens per sat. Combined with the existing pair convention, this would mean two distinct market configurations for the same underlying trade:

| Configuration | PRICE meaning | PRICE for a 50k-sat token |
|--------------|---------------|--------------------------|
| YES/L-BTC (convention) | sats per token | 50,000 |
| L-BTC/YES (flipped) | tokens per sat | 0.00002 (not representable) |

### 3.1 The flipped pair cannot represent most prediction market prices

The covenant is divisionless — all conservation invariants use only multiply, add, and subtract. `PRICE` is represented as integer numerator and denominator. This means:

- **YES/L-BTC convention**: PRICE = sats per token. With rational pricing, any fractional sat value is expressible. Works for any token in the prediction market range.
- **L-BTC/YES flipped**: PRICE = tokens per sat. Can only represent tokens worth <= 1 sat (multiple tokens per satoshi). A token worth 5 sats would need PRICE = 0.2 tokens per sat — not representable even with rational pricing, because the flipped convention has the wrong direction (you'd need the denominator on the wrong side of the equation).

For prediction markets, token prices represent probabilities and range from near-zero to `COLLATERAL_PER_TOKEN` sats. The YES/L-BTC convention covers this entire range. The flipped pair cannot represent most of it.

### 3.2 Liquidity fragmentation

If both pair orderings were allowed, the same economic trade (swap YES tokens for L-BTC) would be split across two incompatible order books with different price units. A SellBase order on the YES/L-BTC book and a SellBase order on the L-BTC/YES book could represent the same intent, but a taker would need to understand both price conventions and search both books. This is pure complexity with no benefit.

### 3.3 The direction flag already covers both sides

Within the fixed YES/L-BTC convention:

- **"I want to sell YES tokens"** → SellBase: lock YES tokens in covenant, receive L-BTC on fill
- **"I want to buy YES tokens"** → SellQuote: lock L-BTC in covenant, receive YES tokens on fill

No pair flip needed. The direction flag provides a complete two-sided order book with a single, consistent price unit.

---

## 4. Why Rational Pricing (NUM/DENOM)

### 4.1 The problem with integer-only PRICE

With a plain integer PRICE (sats per token), price granularity is coupled to `COLLATERAL_PER_TOKEN` (CPT). A market with CPT = 1,000 gets 999 distinct price points (0.1% granularity). A market with CPT = 10 gets only 9 price points (10% granularity — you cannot distinguish a 5% event from a 15% event).

This creates a tension: low CPT makes markets accessible (cheap to mint and trade), but sacrifices price resolution. High CPT gives fine-grained pricing but increases minimum trade sizes.

Critically, CPT is a per-contract parameter — it is fixed when the prediction market contract is deployed. If a market's CPT turns out to be wrong (too coarse for the liquidity it attracts, or too capital-intensive for casual participants), the only remedy is redeploying with a different CPT. This creates new token asset IDs and fragments liquidity for the same event across two incompatible markets.

### 4.2 Alternatives considered and rejected

Three other approaches were evaluated before arriving at rational pricing:

**Division in the covenant.** Adding integer division would allow `payment = consumed * PRICE / SCALE` with arbitrary precision. However, integer division truncates: `7 / 3 = 2` with 1 sat lost. This breaks the exact conservation invariants — equalities become inequalities, and the maker or taker systematically loses the remainder on every fill. Across many partial fills, rounding errors accumulate as dust. The covenant must either pick a beneficiary for the rounding (introducing bias) or track remainders (adding state). This was rejected as it undermines the divisionless design that the batching safety and grief-resistance properties depend on.

**Require large CPT.** Simply mandating CPT >= 1,000 (or 100,000) ensures sufficient integer price points. This works and is the simplest option — it requires no covenant changes. However, it couples granularity to CPT permanently at contract deployment time. All orders on a given market share the same granularity/lot-size tradeoff. If the chosen CPT is too high for casual participants or too low for precise pricing, the only fix is a new contract with new tokens.

**Small CPT with inverted pricing (tokens per sat).** With CPT = 1 and PRICE expressed as tokens per sat, prices follow the harmonic series: 1 token/sat (100%), 2 tokens/sat (50%), 3 tokens/sat (33%), 4 tokens/sat (25%)... The spacing between expressible probabilities is 100%→50% (50pp gap), 50%→33% (17pp gap), 33%→25% (8pp gap). Granularity is worst in the 20-80% range where most prediction market activity occurs. This was rejected as strictly worse than any reasonable CPT with normal pricing.

### 4.3 Rational PRICE decouples granularity from CPT

By expressing PRICE as `PRICE_NUM / PRICE_DENOM` (two u64 compile-time parameters), the covenant supports fractional sat-per-token prices without introducing division. The conservation equation becomes a cross-multiplication:

```
// Integer PRICE (PRICE_DENOM = 1):
payment == consumed * PRICE

// Rational PRICE:
payment * PRICE_DENOM >= consumed * PRICE_NUM
```

This allows a CPT = 10 market to express PRICE = 7/10 (70% probability = 7 sats per token), which is not representable as an integer.

Crucially, `PRICE_NUM` and `PRICE_DENOM` are per-order parameters (baked into each order's covenant at compile time), not per-contract. Two makers on the same market with the same fungible tokens can independently choose different price precision — one posting at 7/10 (coarse, small lots) and another at 699/1000 (fine, large lots). This avoids the CPT lock-in problem entirely: the contract deployer chooses CPT for the market's economic scale, and makers independently choose their pricing precision.

### 4.4 Ceiling rounding eliminates lot-size constraints

With exact rational arithmetic, `consumed * PRICE_NUM` must be divisible by `PRICE_DENOM` for the payment to be a whole number of sats. This restricts valid fill quantities to multiples of `PRICE_DENOM / gcd(PRICE_NUM, PRICE_DENOM)` — an "effective lot size" that varies per price point and creates edge cases for partial fills.

Ceiling rounding removes this constraint entirely. The taker may fill **any** token quantity, paying at most 1 sat above the theoretical price. The covenant enforces:

```
payment * PRICE_DENOM >= consumed * PRICE_NUM                  // at least fair price
payment * PRICE_DENOM - consumed * PRICE_NUM < PRICE_DENOM     // overpayment < 1 sat
```

The maker always receives at least fair value. The taker accepts up to 1 sat of rounding per fill, which is negligible for any practical CPT.

### 4.5 Strict superset of integer pricing

When `PRICE_DENOM = 1`, the ceiling rounding inequality collapses to exact equality:

```
payment * 1 >= consumed * NUM       →  payment >= consumed * NUM
payment * 1 - consumed * NUM < 1    →  payment == consumed * NUM
```

This is identical to integer PRICE behavior. Rational pricing with ceiling rounding is a strict superset — `DENOM = 1` recovers Option 2 exactly, and `DENOM > 1` unlocks finer granularity.

### 4.6 Rounding properties

- **Maker-protective:** the maker always receives at least the theoretical price. Rounding favors the maker.
- **Bounded:** overpayment is strictly less than 1 sat per fill, regardless of fill size.
- **Cumulative:** across N partial fills, total overpayment is at most N sats. For CPT >= 100, this is negligible even with many small fills.
- **Voluntary:** the taker sees the effective cost before submitting. They can always buy in multiples of the effective lot size to avoid rounding entirely.
- **Not game-able:** the maker cannot force odd-lot fills. The taker chooses their quantity.

---

## 5. Price Semantics

### 5.1 PRICE for YES tokens

```
PRICE = PRICE_NUM / PRICE_DENOM   (sats per YES token)
```

A maker selling YES tokens at a 70% implied probability with `COLLATERAL_PER_TOKEN = 100`:

```
PRICE_NUM  = 70
PRICE_DENOM = 1
// equivalent: PRICE = 70 sats per YES token
```

Or with `COLLATERAL_PER_TOKEN = 10`:

```
PRICE_NUM  = 7
PRICE_DENOM = 1
// equivalent: PRICE = 7 sats per YES token
```

Or for finer granularity on a small CPT market (`COLLATERAL_PER_TOKEN = 10`, 73% implied probability):

```
PRICE_NUM  = 73
PRICE_DENOM = 10
// equivalent: PRICE = 7.3 sats per YES token
```

### 5.2 Complement pricing for NO tokens (wallet-level only)

The prediction market contract enforces that YES + NO token pairs are backed by `2 * COLLATERAL_PER_TOKEN` sats of collateral. At the wallet display level, the NO token price is the complement:

```
p_no = COLLATERAL_PER_TOKEN - p_yes
```

For CPT = 100 and p_yes = 70: `p_no = 30 sats`. This is **not** enforced by the maker order covenant — it is a wallet/UI convention derived from the prediction market contract's collateral invariant.

### 5.3 NO token orders use the same convention

When trading NO tokens directly, the convention is the same — just with a different `BASE_ASSET_ID`:

```
BASE_ASSET_ID  = NO token asset ID
QUOTE_ASSET_ID = L-BTC
PRICE          = PRICE_NUM / PRICE_DENOM   (sats per NO token)
```

The direction flag works identically: SellBase = selling NO tokens, SellQuote = buying NO tokens.

---

## 6. Conservation Invariants

PRICE is always the multiplier (via cross-multiplication), never the divisor. The covenant uses ceiling rounding: the maker receives at least the theoretical price, with overpayment bounded to less than 1 sat.

| Direction | Fill type | Invariant |
|-----------|-----------|-----------|
| SellBase | Full | `maker_amount * DENOM >= input_lots * NUM` and `maker_amount * DENOM - input_lots * NUM < DENOM` |
| SellBase | Partial | `maker_amount * DENOM >= consumed * NUM` and `maker_amount * DENOM - consumed * NUM < DENOM` (where `consumed = input_lots - remainder_lots`) |
| SellQuote | Full | `maker_amount * NUM <= input_quote * DENOM` and `input_quote * DENOM - maker_amount * NUM < DENOM` |
| SellQuote | Partial | `maker_amount * NUM + remainder_quote <= input_quote * DENOM` and `input_quote * DENOM - maker_amount * NUM - remainder_quote < DENOM` |

In SellBase, the maker offers tokens and receives sats — so the rounding favors the maker by rounding the sat payment up (maker receives slightly more sats). In SellQuote, the maker offers sats and receives tokens — so the rounding favors the maker by rounding the token payment up (maker receives slightly more tokens).

When `DENOM = 1`, all inequalities collapse to exact equalities, recovering integer PRICE behavior.

---

## 7. Summary

| Concept | Needed? | Why |
|---------|---------|-----|
| BASE/QUOTE distinction | Yes | Defines which side of PRICE each asset is on; enables divisionless arithmetic |
| Direction flag (SellBase/SellQuote) | Yes | Two-sided order book within a single pair convention |
| Flipped pair (L-BTC/YES) | No | Cannot represent typical prediction market prices; fragments liquidity; direction flag already covers both market sides |
| Rational PRICE (NUM/DENOM) | Yes | Decouples price granularity from COLLATERAL_PER_TOKEN; strict superset of integer pricing |
| Ceiling rounding | Yes | Eliminates lot-size constraints from rational pricing; at most 1 sat overpayment per fill; maker-protective |

**The convention is: BASE = outcome token, QUOTE = L-BTC, always.** PRICE is expressed as `PRICE_NUM / PRICE_DENOM` sats per token. The direction flag handles bid vs ask. The covenant is intentionally generic (it doesn't validate asset semantics), but the wallet layer must enforce this convention.
