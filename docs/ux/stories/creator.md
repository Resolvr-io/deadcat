# User Stories: Market Creator & Oracle

Personas covered: **Market Creator** and **Oracle**. See [ux-design.md](../design.md) for persona definitions. Both personas require "Market maker mode" enabled in settings.

---

## US-MC1: Create a New Prediction Market

**As a** market creator, **I want to** define and publish a new prediction market, **so that** others can trade on my question.

**Acceptance criteria**:
- Create form collects: question text, description, category, resolution source, oracle pubkey (defaults to own Nostr pubkey), collateral asset (L-BTC default), collateral per pair (constrained to the 16-value 1-2-5 denomination table per convention), settlement date/time
- Settlement date input snaps to the nearest 60-block boundary (matching the `expiry_time` covenant convention)
- On submit: constructs `MarketCreationParams { oracle_public_key, collateral_asset_id, collateral_per_pair, expiry_time }` → `build_creation_pset` → sign → broadcast
- `build_creation_pset` returns `(UnblindedPset, PredictionMarketParams)` — the UI stores the returned full params for ingestion after confirmation
- After confirmation: `ingest_market(params, creation_tx)` to begin tracking, then publishes a Nostr announcement event for discovery
- On `CoreError::InvalidParams`: display specific validation error (e.g., "Collateral per pair must be one of: 1000, 2000, 5000, 10000...")
- Convention violations caught by the builder (defense in depth) surface as user-friendly messages

**Interaction design**:
- **Guided form**: Single-page form with clear sections. Question at top (large text input), description below (textarea), then parameters in a structured grid.
- **CPT selector**: Dropdown constrained to valid 1-2-5 denominations (1000, 2000, 5000, 10000, 20000, 50000 sats etc.). Not a free-text input — impossible to enter non-conforming values.
- **Settlement picker**: Calendar + time picker. The selected datetime is converted to an estimated block height using current chain tip + ~1 min/block. The snapped block height is shown: "Settles around block 2,150,400 (~June 15, 2027)."
- **Oracle default**: Pre-fills with the user's own Nostr pubkey. Advanced users can paste a different oracle's pubkey. The form validates it's a valid 32-byte hex or npub.
- **Cost preview**: Before submission, show estimated creation cost: "Transaction fee: ~X sats. This creates the market contract on-chain."
- **Post-creation flow**: After the market is created and confirmed, prompt: "Issue initial token pairs?" This naturally leads to US-MC2.

---

## US-MC2: Issue Token Pairs

**As a** market creator, **I want to** issue YES/NO token pairs backed by collateral, **so that** there are tokens available for trading.

**Acceptance criteria**:
- Issue tab (on detail view, only visible for markets the user created) shows: pairs to issue input, collateral required (`pairs * collateral_per_pair`), current outstanding pairs
- Calls `build_issuance_pset(contract_id, pairs, yes_dest, no_dest, funding)` — returns `UnblindedPset`
- The UI calls `unblinded.prepare(wallet_blinding_pubkey)` then `pset.blind_last()` then sign (RT blinding is handled transparently)
- After confirmation via `step`: wallet shows new YES and NO token balances
- `yes_dest` and `no_dest` default to the wallet's own addresses (the user typically keeps both sides initially)

**Interaction design**:
- **Pairs input**: Numeric input with real-time cost calculation: "Issue 100 pairs = lock 500,000 sats as collateral. You'll receive 100 YES + 100 NO tokens."
- **Destination choice**: By default, both token types go to the user's wallet. Advanced option (collapsed) to specify separate destinations (e.g., send NO tokens directly to a pool).
- **Blinding transparency**: The `UnblindedPset` → `prepare` → `blind_last` → sign flow is invisible to the user. They click "Issue" and see a success message. The RT blinding complexity is an implementation detail per Design Principle 1.

---

## US-MC3: Cancel Token Pairs

**As a** market creator, **I want to** burn token pairs and reclaim collateral, **so that** I can exit my position or reduce market exposure.

**Acceptance criteria**:
- Cancel tab shows: pairs to burn input (max = min of YES and NO token balances), collateral to reclaim (`pairs * collateral_per_pair`)
- `build_cancellation_pset(contract_id, Some(pairs), funding)` or `None` for max cancellation
- Requires equal YES and NO token balances — if the user sold some of one side, they can only cancel pairs up to the lesser balance
- After confirmation: wallet shows reduced token balances and increased L-BTC balance

**Interaction design**:
- **Balance constraint**: Show both YES and NO balances. If imbalanced (e.g., 80 YES, 50 NO), the max cancellable is 50 pairs with a note: "You can cancel up to 50 pairs (limited by your NO token balance)."
- **Full cancellation**: "Cancel All" button that passes `None` to the builder. Shows: "Burn X pairs, reclaim Y sats."

---

## US-O1: Resolve a Market as Oracle

**As an** oracle, **I want to** attest to a market's outcome, **so that** winning token holders can redeem their tokens.

**Acceptance criteria**:
- Resolution panel appears on the detail view ONLY when `state.nostrPubkey` matches the market's `oracle_public_key`
- Two buttons: "Resolve YES" and "Resolve NO"
- Clicking either triggers: `oracle_attestation_spec(contract_id, outcome_yes)` → returns `OracleAttestationSpec { message, oracle_pubkey }`
- The app signs the message with the user's Nostr key (BIP-340 Schnorr signature)
- Then: `build_oracle_resolve_pset(contract_id, signature, funding)` → sign → broadcast
- Also publishes the attestation as a Nostr event for public verifiability
- After confirmation: market state transitions to `ResolvedYes` or `ResolvedNo`

**Interaction design**:
- **Conditional visibility**: The resolution panel is NEVER shown to non-oracle users. This prevents confusion. The panel header says "You are the oracle for this market."
- **Confirmation gate**: Resolving is irreversible. The confirm modal shows: "You are resolving this market as [YES/NO]. This cannot be undone. X sats in collateral will become redeemable by [YES/NO] token holders."
- **Signature display**: After signing, show the attestation signature hash and Nostr event ID for auditability. The user can view the full Nostr event JSON.
- **Expiry alternative**: If a market has passed its expiry block height, show a note: "This market is past its settlement date. You can resolve it normally, or it can be expired (half-value redemption for all holders)."

---

## US-O2: Expire a Market

**As an** oracle or market participant, **I want to** trigger market expiry after the settlement deadline, **so that** token holders can redeem at half value.

**Acceptance criteria**:
- Expire button appears when `current_block_height >= market.expiry_height` AND market state is `Trading`
- Calls `build_expire_transition_pset(contract_id, funding)` → sign → broadcast
- After confirmation: market state transitions to `Expired`
- Any user can trigger expiry (not oracle-restricted) — the covenant enforces the timelock

**Interaction design**:
- **Passive notification**: When a market passes its expiry height, show an amber "Settlement deadline passed" banner on the detail view. If the user holds tokens, add: "This market can now be expired. Expiry pays 50% of contract value to all token holders."
- **Expire vs Resolve**: If the user is the oracle, show both options: "Resolve (full value to winners)" and "Expire (half value to all)." Make it clear that resolution is preferred when the outcome is known.
