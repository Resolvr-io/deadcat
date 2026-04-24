import { useCallback, useEffect, useRef, useState } from "react";
import { usePublishMarketComment } from "../../../queries/useComments";
import { useNostrProfileByPubkey } from "../../../queries/useNostrProfileByPubkey";
import { friendlyError } from "../../../utils-react/friendly-error";
import { SigningHint } from "../../shared/SigningHint";

// Client-side anti-spam cap. Backend enforces 4096 bytes; 1000 chars
// sits comfortably under that even for emoji-heavy input (4 bytes per
// code point worst case → 4000 bytes max).
const MAX_CHARS = 1000;

/**
 * Text composer for a top-level comment or a one-level reply. The
 * parent props are optional — when present, the posted kind:1111
 * carries `parent_id` + `parent_author_pubkey` so the backend's
 * NIP-22 builder emits lowercase e/p/k tags targeting the reply's
 * parent comment. Reply mode also renders compactly (smaller
 * textarea, inline Post/Cancel buttons) so the form fits under the
 * row it's replying to without dominating the thread.
 */
export function CommentForm({
  marketId,
  creatorPubkey,
  marketEventId,
  parentEventId,
  parentAuthorPubkey,
  onCancel,
  onPosted,
  autoFocus = false,
}: {
  marketId: string;
  creatorPubkey: string;
  marketEventId: string;
  /** When set, the form posts as a reply to this comment id. */
  parentEventId?: string;
  /** Required when `parentEventId` is set — backend rejects replies
   *  without the parent author pubkey. The "Replying to @name" hint
   *  resolves itself from this pubkey via the profile cache. */
  parentAuthorPubkey?: string;
  /** Invoked when the user dismisses the reply form without posting.
   *  Only rendered when in reply mode. */
  onCancel?: () => void;
  /** Invoked after a successful publish so callers can close an
   *  inline reply form. */
  onPosted?: () => void;
  autoFocus?: boolean;
}) {
  const [value, setValue] = useState("");
  const [error, setError] = useState<string | null>(null);
  const mutation = usePublishMarketComment(marketId, creatorPubkey);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const isReply = !!parentEventId;
  const { data: parentProfile } = useNostrProfileByPubkey(
    parentAuthorPubkey ?? null,
  );
  const parentAuthorName =
    parentProfile?.display_name?.trim() || parentProfile?.name?.trim() || null;

  useEffect(() => {
    if (autoFocus) textareaRef.current?.focus();
  }, [autoFocus]);

  const trimmed = value.trim();
  const chars = [...value].length;
  const overLimit = chars > MAX_CHARS;
  const canSubmit = trimmed.length > 0 && !overLimit && !mutation.isPending;

  const handleSubmit = useCallback(async () => {
    if (!canSubmit) return;
    setError(null);
    // Clear the textarea immediately — the mutation pairs an
    // optimistic cache insert with the signing round-trip, so the
    // comment appears in the thread at the same moment the input
    // resets. Keep the form mounted until the signing round-trip
    // completes so an error surfaces inline; restore the draft on
    // failure so the user can retry without retyping.
    const body = trimmed;
    setValue("");
    try {
      await mutation.mutateAsync({
        marketEventIdHex: marketEventId,
        body,
        parentEventIdHex: parentEventId,
        parentAuthorPubkeyHex: parentAuthorPubkey,
      });
      onPosted?.();
    } catch (e) {
      setValue(body);
      setError(friendlyError(String(e)));
    }
  }, [
    canSubmit,
    mutation,
    marketEventId,
    trimmed,
    parentEventId,
    parentAuthorPubkey,
    onPosted,
  ]);

  return (
    <div
      className={
        isReply
          ? "rounded-lg border border-slate-800 bg-slate-950/30 p-3"
          : "rounded-xl border border-slate-800 bg-slate-950/40 p-4"
      }
    >
      {isReply && parentAuthorName && (
        <p className="mb-2 text-xs text-slate-500">
          Replying to{" "}
          <span className="text-slate-300">@{parentAuthorName}</span>
        </p>
      )}
      <textarea
        ref={textareaRef}
        value={value}
        onChange={(e) => {
          setValue(e.target.value);
          if (error) setError(null);
        }}
        placeholder={
          isReply ? "Write a reply…" : "Share your take on this market…"
        }
        rows={isReply ? 2 : 3}
        className={`dc-input resize-none ${
          isReply ? "h-auto min-h-[64px]" : "h-auto min-h-[88px]"
        }`}
      />
      <div className="mt-3 flex items-center justify-between gap-3">
        <span
          className={`text-xs ${
            overLimit
              ? "text-rose-400"
              : chars > MAX_CHARS * 0.9
                ? "text-amber-300"
                : "text-slate-500"
          }`}
        >
          {chars.toLocaleString()} / {MAX_CHARS.toLocaleString()} characters
        </span>
        <div className="flex items-center gap-2">
          {isReply && onCancel && (
            <button
              type="button"
              onClick={onCancel}
              disabled={mutation.isPending}
              className="rounded-lg border border-slate-700 px-3 py-1.5 text-xs text-slate-300 transition hover:bg-slate-800 disabled:cursor-not-allowed disabled:opacity-50"
            >
              Cancel
            </button>
          )}
          <button
            type="button"
            onClick={() => void handleSubmit()}
            disabled={!canSubmit}
            className={`rounded-lg bg-emerald-400 font-semibold text-slate-950 transition hover:bg-emerald-300 disabled:cursor-not-allowed disabled:opacity-50 ${
              isReply ? "px-3 py-1.5 text-xs" : "px-4 py-2 text-sm"
            }`}
          >
            {mutation.isPending ? "Posting…" : isReply ? "Reply" : "Post"}
          </button>
        </div>
      </div>
      <SigningHint active={mutation.isPending} className="mt-2" />
      {error && <p className="mt-2 text-xs text-rose-400">{error}</p>}
    </div>
  );
}
