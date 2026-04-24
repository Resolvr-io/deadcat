# Notification deep-link: scroll-to-comment is broken

## Summary

Branch `feat/notifications` ships an in-app notifications bell
(kind:1111 replies, kind:7 reactions, kind:9735 zap receipts targeting
the current session pubkey). Clicking a notification is supposed to:

1. Close the popover.
2. Navigate to the source market's detail page.
3. Scroll the exact comment row into view and pulse it for ~2 s.

Step 1 works. Step 2 works (after a local fix, not yet pushed). **Step
3 does not work** — the detail page loads at the top and the user has
to scroll down manually to find the comment.

## What works

- Notification ingestion: `src-tauri/src/notifications.rs` subscribes
  to inbound `p`-tagged kind 1111/7/9735 events, dedupes on event id,
  writes `~/.local/share/.../notifications.json`, emits
  `notifications_updated` to the frontend.
- UI: `NotificationBell.tsx` in the top shell renders an unread dot,
  opens a popover listing items, routes a click through `handleClick`.
- The market navigation step: after fixing an id mismatch (see below),
  the detail page for the correct market renders on click.

## The broken step

On click, `NotificationBell` writes `focusCommentId` into the Zustand
store alongside the navigation state. The plan was that some scroll
effect — either page-level, section-level, or row-level — would read
that key and scroll the matching comment row into view.

All three attempts have failed to scroll:

| Attempt | Location | Strategy | Result |
| ------- | -------- | -------- | ------ |
| 1 | `src/hooks/useScrollToComment.ts` (now deleted) | Mounted on `DetailPage`. `requestAnimationFrame` retry loop up to 1.3 s polling for `[data-comment-id="…"]`. | Failed — loop finished before React Query resolved comments. |
| 2 | same hook | Swapped RAF loop for a `MutationObserver` with 15 s timeout. | Failed — user reported "no change, does not scroll". |
| 3 | `CommentsSection.tsx` (reverted in current diff) | Section-level `useEffect` with `[focusCommentId, comments, isLoading]` deps. Gated on `!isLoading` + `comments.some(c => c.id === focusCommentId)`. `requestAnimationFrame` + `querySelector`. | Failed — user reported "lands on the market at the top". |
| 4 (current HEAD of working tree) | `CommentRow.tsx` — `useFocusScroll` hook per row | Each `CommentRow` reads `focusCommentId`; when it matches `comment.id`, scroll its own `ref` into view, pulse, clear key. | Not yet verified — user interrupted before we could test. |

## Current diff (uncommitted — about to be committed)

Three files:

- **`src/components/layout/NotificationBell.tsx`** — added a
  `useMarkets()` lookup to resolve `notification.marketId` (hex market
  id from the `A` tag) to the market's Nostr event id, since
  `getMarketById` in `utils-react/market.ts` matches on
  `market.id` (event id), not `market.marketId` (hex). Without this,
  `selectedMarketId` was being set to the hex market id and
  `getMarketById` returned `null`, so the detail page rendered
  "Market not found". This is the fix that made step 2 work.

- **`src/components/detail/comments/CommentsSection.tsx`** — removed
  the section-level `useEffect` from attempt 3 and the related
  `PULSE_MS` / `PULSE_CLASS` constants. Back to a minimal component.

- **`src/components/detail/comments/CommentRow.tsx`** — added
  `useFocusScroll(commentId, ref)` hook, called from both the live and
  tombstone render paths. Hook reads `focusCommentId` from the store,
  compares to its own `comment.id`, and on match calls
  `ref.current.scrollIntoView({ behavior: "smooth", block: "center" })`,
  adds `animate-notification-pulse` for 2 s, then clears
  `focusCommentId` from the store.

## Suspected failure modes (for Codex to verify)

User has only tested attempt 4 briefly and reported no scroll. The
three candidate root causes, in order of likelihood:

1. **`notification.commentId` doesn't match any `comment.id` in the
   fetched list.** The notification is parsed in
   `src-tauri/src/notifications.rs` from the inbound event's `e` tag.
   If that's the kind:1111 reply event id, it should exist. But if
   it's somehow a parent id or mis-parsed, no row in
   `useMarketComments` would have that id, and no row would fire.
   Verify with a `console.log` in `useFocusScroll`: log
   `{focusCommentId, commentId}` on every effect run.

2. **`focusCommentId` gets cleared before the row mounts.** If
   something else in the store write path resets nav state (e.g. a
   re-render that computes `focusCommentId: null` via partial
   setState), the row would mount with `focusCommentId === null` and
   the effect's guard returns early. Nothing in the current codebase
   obviously does this, but worth confirming.

3. **`scrollIntoView` silently no-ops.** The comments section is
   inside a `rounded-[21px]` card with `overflow-hidden`? Let me
   check… Actually it's just a section; the page-level scroll container
   should be the document root. Possible but unlikely.

## Suggested diagnostic plan

Before shipping more code, verify which unknown is the actual failure:

```tsx
// In useFocusScroll in CommentRow.tsx:
useEffect(() => {
  console.log("[useFocusScroll]", { focusCommentId, commentId, hasRef: !!ref.current });
  if (focusCommentId !== commentId) return;
  // …
}, [focusCommentId, commentId, ref]);
```

Also log `{marketId: notification.marketId, commentId: notification.commentId}`
in `NotificationBell.handleClick`, and log received comments in
`CommentsSection`:

```tsx
console.log("[comments]", comments.map(c => c.id));
```

Open the app with devtools, click a notification, capture the three
log streams. That instantly identifies whether the comment id is in
the fetched list, whether the effect fires, and whether the ref
exists.

## Files / commits for reference

- Branch: `feat/notifications` (not merged)
- Base: `master` at `3c74165`
- Commits on branch (oldest → newest):
  - `70681a9 feat(notifications): add local store with JSON persistence`
  - `6580553 feat(notifications): Tauri commands for list / unread / mark-read`
  - `b02f17a feat(notifications): subscribe to inbound p-tagged events`
  - `448928d feat(notifications): frontend bell + scroll-to-comment deep-link`
  - `3346751 feat(reactions): full emoji picker behind 'More emojis'`
  - `27976a5 style(topshell): wallet → positions → notifications → user menu`
  - `14a3954 fix(notifications): wait longer for the target comment row to mount` (attempt 2)
  - `53a879b fix(notifications): scroll inside CommentsSection so the row is guaranteed rendered` (attempt 3)
  - (pending commit) attempt 4 — row-level hook + id-mismatch fix

## Out of scope

- The emoji picker and top-shell reorder in this branch are unrelated
  to the scroll issue and should not be touched.
- `src-tauri/src/notifications.rs` event parsing is working (the bell
  shows accurate unread counts and rows display the right author +
  verb). Only the comment-id-aware deep link is broken.
