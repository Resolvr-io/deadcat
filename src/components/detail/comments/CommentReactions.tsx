import { useMemo } from "react";
import {
  type ReactionStats,
  useDeleteCommentReaction,
  usePublishCommentReaction,
} from "../../../queries/useCommentReactions";
import { useStore } from "../../../store";
import { friendlyError } from "../../../utils-react/friendly-error";
import { EmojiPicker } from "../../shared/EmojiPicker";
import { showToast } from "../../shared/Toast";

/**
 * Deadcat-specific reaction roster. Mixes cat character with
 * prediction-market sentiment. Visible set lives on the row; the
 * rest sits behind a `…` popover so a single comment doesn't get
 * drowned in 16 emojis.
 */
const VISIBLE: readonly string[] = [
  "🐱",
  "😹",
  "😻",
  "🙀",
  "⚡",
  "📈",
  "📉",
  "🎯",
];
const MORE: readonly string[] = ["🔥", "❤️", "💀", "🎲", "🧨", "🏆", "💎", "👀"];

/**
 * Reaction row rendered beneath a comment body. Shows the non-zero
 * aggregated pills followed by the picker. Clicking a pill you
 * already reacted with toggles it off via kind:5 deletion; clicking
 * any other emoji publishes a new kind:7.
 *
 * Signed-out readers see the pills but the picker and toggle are
 * hidden — NIP-25 requires a signing key, same gate as zaps.
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

  const selected = useMemo(() => {
    const set = new Set<string>();
    for (const r of stats) if (r.mine) set.add(r.emoji);
    return set;
  }, [stats]);

  const toggle = (emoji: string) => {
    // Can't react to your own comment — mirrors the zap gate so the
    // social graph doesn't get polluted with self-reactions.
    if (isOwn) return;
    const existing = stats.find((r) => r.emoji === emoji);
    if (existing?.mine && existing.myEventId) {
      del.mutate(existing.myEventId, {
        onError: (e) => showToast(friendlyError(String(e)), "error"),
      });
      return;
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

  const busy = publish.isPending || del.isPending;

  return (
    <div className="mt-1.5 flex flex-wrap items-center gap-1">
      {stats.map((r) => (
        <button
          key={r.emoji}
          type="button"
          onClick={() => toggle(r.emoji)}
          disabled={busy || isOwn || !sessionPubkey}
          title={
            isOwn
              ? "You can't react to your own comment"
              : !sessionPubkey
                ? "Sign in to react"
                : r.mine
                  ? `Remove your ${r.emoji}`
                  : `React with ${r.emoji}`
          }
          aria-pressed={r.mine}
          className={`flex items-center gap-1 rounded-full border px-2 py-0.5 text-xs transition disabled:cursor-default disabled:opacity-60 ${
            r.mine
              ? "border-emerald-400/50 bg-emerald-400/10 text-emerald-200"
              : "border-slate-700 bg-slate-900/50 text-slate-300 hover:border-slate-500 hover:bg-slate-800"
          }`}
        >
          <span aria-hidden="true">{r.emoji}</span>
          <span className="mono">{r.count}</span>
        </button>
      ))}
      {sessionPubkey && !isOwn && (
        <EmojiPicker
          visible={VISIBLE}
          more={MORE}
          selected={selected}
          onPick={toggle}
          triggerLabel="More reactions"
        />
      )}
    </div>
  );
}
