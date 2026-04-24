import { useEffect, useMemo, useRef, useState } from "react";
import {
  type ReactionStats,
  useDeleteCommentReaction,
  usePublishCommentReaction,
} from "../../../queries/useCommentReactions";
import { useStore } from "../../../store";
import { friendlyError } from "../../../utils-react/friendly-error";
import { FullEmojiPicker } from "../../shared/FullEmojiPicker";
import { showToast } from "../../shared/Toast";
import { HeartIcon } from "./icons";

/**
 * Deadcat-curated quick reactions — cat character + market sentiment.
 * Renders in the compact 2×4 grid of the popover. "More emojis"
 * opens the full `emoji-picker-element` surface for the long tail.
 */
const QUICK: readonly string[] = [
  "🐱",
  "😹",
  "😻",
  "🙀",
  "⚡",
  "📈",
  "📉",
  "🎯",
];

/**
 * Reaction control rendered in the comment action row. Matches the
 * zapcooking pattern: a single outline-heart trigger that shows the
 * total reaction count (or the viewer's own reaction emoji when
 * they've already reacted). Clicking opens a compact popover with 8
 * curated emojis and a "More emojis" button that opens the full
 * emoji picker (shared by primal-web-spark and zapcooking) for the
 * full Unicode set.
 *
 * Per-emoji aggregated pills are deliberately NOT rendered on the
 * row — matching the convention in Primal / zapcooking. The full
 * breakdown can surface later in a detail view; for MVP the single
 * count keeps the row uncluttered regardless of how many reactions
 * land.
 */
export function CommentReactions({
  commentEventId,
  commentAuthorPubkey,
  stats,
}: {
  commentEventId: string;
  commentAuthorPubkey: string;
  stats: ReactionStats[];
}) {
  const sessionPubkey = useStore((s) => s.nostrPubkey);
  const publish = usePublishCommentReaction();
  const del = useDeleteCommentReaction();
  const isOwn = sessionPubkey === commentAuthorPubkey;
  const [open, setOpen] = useState(false);
  const [fullPickerOpen, setFullPickerOpen] = useState(false);
  const [fullPickerAnchor, setFullPickerAnchor] = useState<DOMRect | null>(
    null,
  );
  const wrapperRef = useRef<HTMLDivElement>(null);
  const moreButtonRef = useRef<HTMLButtonElement>(null);

  const total = useMemo(
    () => stats.reduce((sum, r) => sum + r.count, 0),
    [stats],
  );
  const mySet = useMemo(() => {
    const s = new Set<string>();
    for (const r of stats) if (r.mine) s.add(r.emoji);
    return s;
  }, [stats]);
  /** Show the viewer's own reaction emoji in place of the heart when
   *  they've reacted — "at a glance" recall of what you picked. If
   *  they reacted with multiple emojis, any one is fine; sorting is
   *  already stable on the stats side so picks will be deterministic. */
  const myEmoji = useMemo(() => {
    const mine = stats.find((r) => r.mine);
    return mine?.emoji ?? null;
  }, [stats]);

  const disabled = isOwn || !sessionPubkey;
  const disabledTitle = isOwn
    ? "You can't react to your own comment"
    : !sessionPubkey
      ? "Sign in to react"
      : null;

  // Close on outside click. Skip while the full picker is open —
  // its own portal handles dismiss.
  useEffect(() => {
    if (!open || fullPickerOpen) return;
    const handler = (e: MouseEvent) => {
      if (
        wrapperRef.current &&
        !wrapperRef.current.contains(e.target as Node)
      ) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open, fullPickerOpen]);

  // Close on Escape. Full picker handles its own Escape.
  useEffect(() => {
    if (!open || fullPickerOpen) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [open, fullPickerOpen]);

  const handleTriggerClick = () => {
    if (disabled) return;
    setOpen((v) => !v);
  };

  const handleOpenFullPicker = () => {
    const rect = moreButtonRef.current?.getBoundingClientRect() ?? null;
    setFullPickerAnchor(rect);
    setFullPickerOpen(true);
    setOpen(false);
  };

  /** One reaction per user per comment. Clicking the emoji you
   *  already reacted with removes it; clicking any other emoji
   *  replaces it — delete-the-old-then-publish-the-new, so the row
   *  can never carry two reactions from the same user. */
  const toggle = async (emoji: string) => {
    setOpen(false);
    setFullPickerOpen(false);
    const myReactions = stats.filter((r) => r.mine && r.myEventId);
    const clickedIsMine = myReactions.some((r) => r.emoji === emoji);

    if (clickedIsMine) {
      const target = myReactions.find((r) => r.emoji === emoji);
      if (!target?.myEventId) return;
      del.mutate(target.myEventId, {
        onError: (e) => showToast(friendlyError(String(e)), "error"),
      });
      return;
    }

    // Clean up any existing reaction(s) we've previously left on this
    // comment before publishing the new one. Usually there's at most
    // one; the loop covers the edge case where an older build left
    // multiple from this same pubkey that haven't been reconciled.
    for (const r of myReactions) {
      if (r.myEventId) {
        del.mutate(r.myEventId, {
          onError: (e) => showToast(friendlyError(String(e)), "error"),
        });
      }
    }
    publish.mutate(
      {
        commentEventId,
        commentAuthorPubkey,
        emoji,
      },
      {
        onError: (e) => showToast(friendlyError(String(e)), "error"),
      },
    );
  };

  const trailing = total > 0 ? total.toLocaleString() : "0";
  const triggerTitle = disabledTitle
    ? disabledTitle
    : myEmoji
      ? "Change your reaction"
      : "Add a reaction";

  return (
    <div ref={wrapperRef} className="relative inline-flex">
      <button
        type="button"
        onClick={handleTriggerClick}
        disabled={disabled}
        title={triggerTitle}
        aria-haspopup="menu"
        aria-expanded={open}
        className={`flex items-center gap-1 rounded-full px-2 py-1.5 text-slate-500 transition disabled:cursor-default disabled:opacity-60 ${
          myEmoji ? "text-rose-300" : "hover:bg-rose-400/10 hover:text-rose-300"
        }`}
      >
        {myEmoji ? (
          <span aria-hidden="true" className="text-base leading-none">
            {myEmoji}
          </span>
        ) : (
          <HeartIcon className="h-[18px] w-[18px]" />
        )}
        <span className="text-xs">{trailing}</span>
      </button>

      {open && (
        <div
          role="menu"
          className="absolute bottom-full left-0 z-20 mb-1 rounded-xl border border-slate-700 bg-slate-900 p-3 shadow-xl"
          style={{ minWidth: "264px" }}
        >
          <div className="grid grid-cols-4 gap-2">
            {QUICK.map((emoji) => (
              <button
                key={emoji}
                type="button"
                onClick={() => toggle(emoji)}
                aria-pressed={mySet.has(emoji)}
                title={
                  mySet.has(emoji)
                    ? `Remove your ${emoji}`
                    : myEmoji
                      ? `Replace with ${emoji}`
                      : `React with ${emoji}`
                }
                className={`flex aspect-square items-center justify-center rounded-lg border text-xl transition hover:scale-110 ${
                  mySet.has(emoji)
                    ? "border-emerald-400/60 bg-emerald-400/15"
                    : "border-transparent bg-slate-800/60 hover:border-slate-600"
                }`}
              >
                <span aria-hidden="true">{emoji}</span>
              </button>
            ))}
          </div>
          <div className="mt-3 border-t border-slate-800 pt-3">
            <button
              ref={moreButtonRef}
              type="button"
              onClick={handleOpenFullPicker}
              className="flex w-full items-center justify-center gap-1.5 rounded-lg py-2 text-sm text-slate-400 transition hover:bg-slate-800 hover:text-slate-200"
            >
              <svg
                aria-hidden="true"
                className="h-4 w-4"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <line x1="12" x2="12" y1="5" y2="19" />
                <line x1="5" x2="19" y1="12" y2="12" />
              </svg>
              More emojis
            </button>
          </div>
        </div>
      )}

      {fullPickerOpen && (
        <FullEmojiPicker
          anchorRect={fullPickerAnchor}
          onPick={(emoji) => {
            setFullPickerOpen(false);
            void toggle(emoji);
          }}
          onClose={() => setFullPickerOpen(false)}
        />
      )}
    </div>
  );
}
