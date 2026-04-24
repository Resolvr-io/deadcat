# Nostr Social Features — Roadmap

Plan for the next batch of Nostr-facing work after the zap / detail /
footer PR (#94). Groups the features into sub-PRs so each lands
reviewable; final branching is flexible.

## Context

Already shipped on `master`:
- NIP-22 market comments (kind:1111) — post, list, delete
- NIP-57 zaps — send via Bitcoin Connect + NWC; receipt aggregation
  (kind:9735)
- NIP-89 client tag (`client = "Deadcat.live"`) on outgoing comments
- NIP-19 mention + URL rendering in comment bodies and profile bios
- Profile dialog with follow/mute placeholder buttons (disabled)

## Priority + Sequencing

Recommend seven sub-PRs, roughly ordered by user impact and
dependencies:

1. **PR A — zap UX fixes** (1 day). Paper cuts from #94. Should land
   first.
2. **PR A.3 — NIP-78 prefs + logout purge** (1–2 days). Account-
   scoped data hygiene: wipe device localStorage on logout, migrate
   zap prefs to NIP-78 kind:30078 per-category events. Lands before
   any new pref would be added.
3. **PR B — reactions** (2–3 days). NIP-25 kind:7, emoji picker,
   aggregation.
4. **PR C — one-level threads** (2–3 days). Uses existing `parent_id`
   in the SDK. Requires the delete-with-replies decision.
5. **PR D — notifications** (3–4 days). Bell icon, relay
   subscription, deep-link. Makes B + C feel complete.
6. **PR E — follow / mute** (3–4 days). NIP-02 + NIP-51 behind a
   shared read-before-write safety helper.
7. **PR F — polish** (1 day). NIP-05 verification + comment sort.

## Shared infrastructure

Two helpers land with the first PR that needs them; both are
referenced across multiple later features.

### `fetchThenPublishList(kind, mutator, { timeoutMs })`

Wraps every write to kind:3 / kind:10000 / kind:10002 with
read-before-write so we never clobber an existing list with a blank
event:

1. Subscribe to `{kinds: [kind], authors: [me]}` with a 3s timeout.
2. If nothing arrives on any configured relay, reject with a user-
   facing error: *"Couldn't verify your current list — check your
   relay connection, then retry."* (Do not publish.)
3. If an existing event is found, pass its tags + content into
   `mutator(existing) → { tags, content }`. Publish the result.
4. For kind:10000, `mutator` gets access to both `tags` (public
   entries) and NIP-44-decrypted `content` (private entries) and
   returns the merged output in both fields.

This rule is the single most important invariant — one uncovered
write path will eventually wipe someone's follow list.

### `useScrollToComment(commentId?: string)`

Used by the detail page when a notification deep-link lands. Scrolls
the target comment into view, auto-expands its thread if it's a
reply, and applies a 2-second highlight pulse
(`animate-highlight-pulse` ring animation). Safe to call when the
comment isn't loaded yet — waits one RAF after the comments query
settles.

### Emoji picker component

Two-tier layout: 6–8 visible emojis + a `…` button that opens a
`<Popover>` with the full set. Used by both reactions and any future
emoji-driven UI (mute reasons, tags, etc.).

### Account-scoped data hygiene

Two rules to prevent account-to-account data leakage on a shared
device:

1. **Logout purges account-scoped device storage.** On logout, wipe
   every `deadcat:` / `deadcat_` localStorage key. Any pref that
   needs to survive a login/logout cycle lives on relays, not
   localStorage. Device-level exemptions (window position, etc.) go
   in a `KEEP` set referenced from the logout handler — cheaper than
   the inverse default.
2. **Cross-session prefs live on relays.** No new localStorage-
   backed pref may be added without first checking whether it should
   instead be an `useAppData` entry (next helper).

### `useAppData<T>(category, schemaVersion)` — NIP-78 per-category prefs

One kind:30078 event per pref category, keyed by a stable `d` tag:

- `live.deadcat.zaps` — default amount, default comment
- `live.deadcat.notifications` — last-seen timestamp, read markers
  (future)
- `live.deadcat.ui` — comment sort preference, emoji-picker MRU
  (future)

Per-category events (not a single fat blob) so each feature owns its
schema, categories are added without touching existing code, and
sensitive categories can be NIP-44 encrypted independently while
benign ones stay public.

Every event carries a `schema: <number>` field and is written via
read-merge-write so unknown fields from newer app versions aren't
dropped when an older client edits the same category. Unknown-field
preservation is the single invariant — forget it once and a future
version's data silently disappears the next time the user opens an
older build.

On login, fetch all known categories in parallel; hydrate Zustand
slices from the results; mark slices "remote-loaded" so writes only
fire after hydration (never overwrite an unseen remote event with
local defaults).

## Features

### A1 — Fix: signed-out zap gate

**Bug**: the zap button on comment rows and the profile dialog is
rendered regardless of session state. Clicking while signed out opens
`ZapDialog` and fails with *"Node not initialized — call
init_nostr_identity first"*. NIP-57 requires a zap request signed by
the sender's key — without a session there's nothing to sign with.

**Fix**:
- `CommentRow.tsx`, `CommentProfileDialog.tsx` — guard the zap button
  with `useStore((s) => s.nostrPubkey)`. Replace with a subtle "Sign
  in to zap" inline CTA that opens the onboarding overlay, or omit
  the zap affordance entirely on rows.
- Skip mounting `<ZapDialog>` when no session.

### A2 — Fix: QR-flow zap payment detection

**Bug**: when the user has no attached Lightning wallet, ZapDialog
falls back to showing a QR / invoice for the user to pay externally.
The app has no callback when the external wallet settles, so it
always shows *"Error: Payment cancelled"* — even when the zap landed.

**Fix (shipped in PR #96)**: soften the BC payment-modal cancel into
a `{ pending: true }` result. ZapDialog surfaces "If you paid, your
zap will appear shortly." as an info toast and schedules extra zap-
count invalidations at 4s + 15s so the late-arriving kind:9735
receipt updates the row counter.

Follow-up (deferred): subscribe to
`{kinds: [9735], #p: [recipientPubkey], #e: [commentId?], since: now}`
and match the bolt11 tag to our invoice for a definitive paid/not-
paid determination. Needs a new Tauri command. Not gating PR A.

### A.3 — NIP-78 prefs + logout purge

**Bug / gap**: zap defaults (sats, comment) persist in localStorage
across logout, so a restored user sees the previous account's
settings. Same risk applies to every future pref we'd want to hold
across sessions.

**Fix — logout purge (shipped in PR #96)**: wipe all `deadcat:` /
`deadcat_` localStorage on logout via the generic loop in
`confirmLogout`. Exempt device-level keys by name as they emerge.

**Fix — relay-backed prefs**: introduce `useAppData<T>(category,
schemaVersion)` (see shared infrastructure). Migrate zap prefs to
`live.deadcat.zaps` as the first category. Keep localStorage as an
optional write-through cache for offline reads, cleared on logout.

**Schema for zaps (v1)**:

```json
{
  "schema": 1,
  "defaultSats": 100,
  "defaultComment": ""
}
```

**Files**:
- `src-tauri/src/app_data.rs` (new) — fetch + publish kind:30078 via
  `NostrSigner`, returns decrypted content
- `src/queries/useAppData.ts` (new) — React hook + cache
- `src/queries/useZapPrefs.ts` — swap storage layer; keep API
- `src/components/layout/TopShell.tsx` — logout loop already shipped
  in PR #96

**Risks**:
- Read-merge-write footgun — must preserve unknown fields so a
  future version's data isn't silently dropped by an older client.
  Covered by a shared test in the `useAppData` module.
- Clock skew on `created_at` — when two clients edit concurrently,
  the highest `created_at` wins per NIP-33/replaceable-event rules;
  we don't need a CRDT, just make sure the read-then-write gap is
  short (single async task).

### B — Reactions (NIP-25)

**What**: render a reaction row on every comment showing unique
emoji counts (e.g., `🐱 3  📈 5  ⚡ 2`). Clicking an emoji toggles
the current user's reaction of that type. A `+` button opens the
picker.

**Protocol**: NIP-25 kind:7. `content` = emoji character (standard)
or `+` (default like) / `-` (dislike) / `:shortcode:` (custom emoji
with `emoji` tag). We'll only emit single unicode emojis in MVP.

**Aggregation**: new SDK method `fetch_comment_reactions(comment_id)`
returns `{ emoji: string, count: usize, mine: bool }[]`. Indexed in
the same shape as the existing `fetch_comment_zaps`. Cached via
React Query with key `["commentReactions", commentId]` and a
WebSocket-invalidation or 15s staleTime.

**Picker emoji set** (16 total, split visible + more):
- Visible (8): 🐱 😹 😻 🙀 ⚡ 📈 📉 🎯
- More (8): 🔥 ❤️ 💀 🎲 🧨 🏆 💎 👀

Character choices mix cat character (deadcat identity) with
prediction-market sentiment. Per-user MRU personalization
(most-used emojis float to the visible set) is a follow-up, not MVP.

**Files**:
- `src-tauri/crates/deadcat-sdk/src/discovery/reactions.rs` (new)
- `src/queries/useCommentReactions.ts` (new)
- `src/components/shared/EmojiPicker.tsx` (new, reusable)
- `src/components/detail/comments/CommentReactions.tsx` (new)
- `CommentRow.tsx` — insert the reaction row below the body

### C — One-level threaded comments

**What**: Reply action on every comment row. Clicking opens an inline
composer under the comment; posting creates a kind:1111 with
`parent_id` set to the parent comment's event id. Replies render
indented one level under their parent; a collapsed "N replies"
summary line expands them.

**Scope**: **exactly one visible level**. The backend's `parent_id`
chain can go arbitrarily deep — in MVP we render replies-to-replies
as siblings of the parent reply rather than further-indented. Keeps
the UI from turning into a thin right-hand sliver on mobile and
matches how prediction-market discussions actually flow.

**Delete-with-replies decision** (must pick before shipping):

- Option A — **tombstone**: a deleted comment with replies stays in
  the tree with its body replaced by *"[deleted]"*, author still
  shown. Replies remain visible and readable in context.
- Option B — **hide**: a deleted comment with replies is removed
  entirely; replies either (B1) orphan up to top-level or (B2)
  disappear with the parent.

**Recommendation**: **A (tombstone)**. Preserves thread context,
matches Reddit-style conventions users expect, and respects the
Nostr reality that a delete request is advisory — the original event
still exists on relays that ignore kind:5 for policy or archival
reasons. Hiding would be a UX lie in that case.

**Files**:
- `CommentRow.tsx` — Reply action + indented render mode
- `CommentForm.tsx` — accept an optional `parentId` prop
- `CommentsSection.tsx` — nest list by parent_id, pass tombstone
  prop when parent had a kind:5 on it
- SDK: extend `MarketComment` with `{ deleted: bool }` flag and have
  `fetch_market_comments` set it when a kind:5 is seen

### D — Notifications

**What**: bell icon next to the user avatar with an unread-count
dot. Dropdown lists recent notifications with timestamps; clicking
deep-links to the originating market with the triggering comment
scrolled into view and highlighted via `useScrollToComment`.

**Triggers (MVP)**:
- Someone replies to your comment (kind:1111 with your event id in
  `parent_id`)
- Someone mentions you in a comment body (`nostr:npub1…` matches
  your pubkey)
- Someone zaps your comment (kind:9735 with `#p: you, #e: yours`)
- Someone reacts to your comment (kind:7 with `#p: you, #e: yours`)

**Storage**: unread state is local-only for v1 (no NIP-51 "read
receipts" list). Persisted in a SQLite `notifications` table on the
Rust side, cleared on click.

**Subscription**: one rolling subscription per session,
`{kinds: [7, 9735, 1111], #p: [me], since: lastSeen}`. A Rust task
filters relevant events, computes a notification record, inserts
into the table, and emits a Tauri event the frontend listens for.

**Deep-link shape**: internal, not URL-based — the notification
record stores `{ marketId, commentEventId }` and the click handler
sets `view: "detail"`, `focusComment: commentEventId`.

**Files**:
- `src-tauri/src/notifications.rs` (new)
- `src-tauri/src/db.rs` — `notifications` table + migration
- `src/queries/useNotifications.ts` (new)
- `src/components/layout/NotificationBell.tsx` (new)
- `src/hooks/useScrollToComment.ts` (new)
- `TopShell.tsx` — mount the bell next to the avatar

### E — Follow / mute (NIP-02 + NIP-51)

**What**: the profile dialog's disabled Follow/Mute buttons become
real. Settings gets a section showing the user's follow + mute lists
with the ability to unfollow / unmute.

**Protocol**:
- **Follows**: NIP-02 kind:3. Public only — no private variant. Tags:
  `["p", pubkey, relayHint?, petname?]`. Legacy `content`-as-relay-
  list is deprecated; do **not** overwrite it on write.
- **Relay list**: NIP-65 kind:10002. Readable but not edited in this
  PR — we just need to know not to clobber it via the legacy kind:3
  content field.
- **Mutes**: NIP-51 kind:10000. Mixed visibility:
  - Public entries: `tags` array — `["p", pubkey]`, `["e", eventId]`,
    `["word", phrase]`, `["t", hashtag]`.
  - Private entries: NIP-44-encrypted JSON array in `content`,
    decrypted with the user's own key.

**Public vs private mute policy** (decided 2026-04-23): mirror
whatever the user's existing list already is — if their kind:10000
has public entries only, new mutes go public; if private only, new
mutes go private. For users with *no* existing list, default to
public (matches ecosystem behavior in Primal / Amethyst — avoids
silently fragmenting their list when they use another client).

Settings exposes: *"12 muted · public list"* with a one-click
"Switch to private mutes" migration button that re-publishes all
entries under `content`. Never overwrites existing entries without
the user's action.

**Safety**: every write goes through `fetchThenPublishList` — read
current event, mutate the tags / content, publish. If we can't
confirm the current state, refuse to publish. This rule covers kind:
3 and kind:10000 equally.

**Files**:
- `src-tauri/src/social.rs` (new) — fetchThenPublishList + follow /
  mute primitives
- `src/queries/useFollows.ts`, `src/queries/useMutes.ts` (new)
- `src/components/detail/comments/CommentProfileDialog.tsx` — wire
  the existing placeholder buttons
- `src/components/layout/SettingsPanel.tsx` — list management
  sections
- `CommentsSection.tsx` — filter out muted authors client-side

### F — Polish

**F1 — NIP-05 verification**: currently we render the purple
checkmark whenever `profile.nip05` is set but never fetch
`/.well-known/nostr.json?name=<local>` to confirm it resolves to the
same pubkey. Add a `useNip05Verification(pubkey, nip05)` hook that
fetches and caches the result with a 1-hour TTL. Show the purple
check only on verified entries; render a muted gray outline on
unverified or failed. Failure to fetch (offline, CORS) falls back to
unverified — never a false positive.

**F2 — Comment sort**: dropdown on `CommentsSection` with four
options: Newest (default), Oldest, Most zapped, Most reactions.
Client-side sort over the already-fetched list; the two "most" sorts
depend on B (reactions) and existing zap aggregation to have
meaningful counts.

## Out of scope

Deliberately not in this roadmap:

- **Deep threading** (> 1 visible level) — follow-up once users ask
  for it.
- **Dedicated profile view** — clicking a mention opens a modal
  today; a full profile page with all comments / activity is a
  bigger surface.
- **Inline quote rendering** (`nostr:nevent1…` → fetched preview
  card). Current chip placeholder is fine for MVP.
- **Long-form (NIP-23)** — not a fit for short market comments.
- **Report / flagging** — relay-level filtering is crude; a proper
  flow requires a moderation model we haven't designed.
- **Search** — searching comments / users by name is a different
  primitive; not gating social features.
- **Encrypted DMs** — separate feature area.
- **Gift wraps** — separate.
