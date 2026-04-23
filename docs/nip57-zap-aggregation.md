# NIP-57 zap aggregation — current behavior and known gaps

Covers how the deadcat app counts and sums zap amounts on
comments and notes today, the simplifying choices we made, and the
items we should revisit.

## Data flow (as of 2026-04-24)

1. **Batch query**. `CommentsSection` hands every comment event id
   in the thread to `useCommentZaps`, which invokes the Tauri
   `fetch_comment_zaps` command once per render (30 s React Query
   staleTime, sort-stable cache key).
2. **Standalone relay client**. `fetch_comment_zaps` builds a fresh
   `nostr_sdk::Client`, adds the user's configured relays (falls
   back to `DEFAULT_RELAYS` when empty), and subscribes:
   `Filter::new().kind(9735).events([id1, id2, …])` with a 10 s
   timeout. Identity-free — works when the wallet is locked or no
   signer is attached.
3. **Aggregate per event**
   (`deadcat-sdk/src/discovery/zaps.rs::fetch_zap_summaries_for_events`).
   For every returned receipt:
   - read the `description` tag → serialized kind:9734 zap request
     JSON;
   - inside its `tags` array, find `["amount", "<msats>"]`;
   - for every `e` tag on the receipt (usually one), credit that
     event id with `+1 count, +amount_msats`.
   Returns `Vec<ZapSummary { event_id, count, total_msats }>`.
4. **Render**. `useCommentZaps` converts to
   `{ count, totalSats: floor(total_msats / 1000) }` and builds a
   `Map<eventId, ZapStats>` passed down to each `CommentRow`. The
   row shows `⚡ {totalSats}` when `count > 0`, tooltip reads
   `"<sats> sats from <count> zap(s)"`.
5. **Invalidation on local zap**. `ZapDialog.handleZap` invalidates
   `["commentZaps"]` immediately after the Lightning payment
   resolves, then once more after 4 s to catch the kind:9735 that
   lands on relays a few seconds behind the payment itself.

## Amount source: advertised, not cryptographically verified

We sum the **payer's declared amount** from the embedded kind:9734
zap request inside the receipt's `description` tag. We do **not**
decode the `bolt11` invoice tag to confirm the signed invoice's
msat amount matches.

This matches Primal / Jumble / Damus conventions and avoids
bundling a BOLT-11 decoder on the Rust side.

**Tradeoff:** a malicious payer could submit a receipt whose
declared `amount` doesn't match the actual paid invoice (e.g.
declare 100 000 sats, pay 1 sat). Today nothing in the aggregation
path catches that. Mainstream clients inherit the same trust model
and have not been spoofed in practice, but it's worth addressing
before we start surfacing zap totals in anywhere load-bearing
(leaderboards, reputation signals, etc.).

**Path to verification:**

- Parse the `bolt11` tag with a Rust BOLT-11 invoice decoder
  (`lightning-invoice` crate or similar).
- Compare the invoice's msat amount to the declared `amount`; take
  the minimum (or drop receipts where they disagree beyond a small
  tolerance).
- Optionally verify the receipt was signed by the LNURL provider's
  announced `nostrPubkey` (NIP-57 section "Verifying zaps") — this
  catches forged receipts that weren't actually paid.

Deferred until a concrete trust-sensitive surface needs it.

## Known gaps and follow-ups

- **Multi-relay efficiency.** We do a single shot with a 10 s
  timeout against the user's configured relays. Slow or
  unresponsive relays can make the whole fetch time out, which
  silently yields a `0` count even when receipts exist on other
  relays. Owner: Chris (Linear ticket filed).
- **No NIP-65 recipient-relay resolution.** When zapping via the
  LNURL provider, the relays list passed inside the zap request
  comes from the payer's config. Receipts are published there. If
  the viewer's relay pool differs from the payer's, receipts may
  not be visible. NIP-65 "relay list metadata" lookups for
  recipients would help, at the cost of another round-trip.
- **Polling, not subscription.** `fetch_events` with a timeout is
  a one-shot. For live threads (active zapping during a market
  event), a persistent subscription would push receipts into the
  UI in real time. Requires a longer-lived client and plumbing.
- **Unbounded `e`-tag crediting.** A single receipt with multiple
  `e` tags credits every referenced event — intentional for
  cross-threads today, but could be exploited to inflate counts
  across unrelated notes. Mitigate once we have receipt-signature
  verification.
- **No de-dup on receipt id.** If the same receipt hits two relays
  we currently risk double-counting (depending on whether nostr-sdk
  dedupes in the returned set). Spot-check and add an explicit
  `HashSet<EventId>` guard before aggregation.
- **Zap receipt → ZapIcon animation.** The counter pops in
  silently. A brief pulse / color flash on increment is a nice
  polish.
