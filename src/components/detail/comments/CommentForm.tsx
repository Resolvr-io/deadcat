import { useCallback, useState } from "react";
import { usePublishMarketComment } from "../../../queries/useComments";
import { friendlyError } from "../../../utils-react/friendly-error";
import { SigningHint } from "../../shared/SigningHint";

// Client-side anti-spam cap. Backend enforces 4096 bytes; 1000 chars
// sits comfortably under that even for emoji-heavy input (4 bytes per
// code point worst case → 4000 bytes max).
const MAX_CHARS = 1000;

export function CommentForm({
  marketId,
  creatorPubkey,
  marketEventId,
}: {
  marketId: string;
  creatorPubkey: string;
  marketEventId: string;
}) {
  const [value, setValue] = useState("");
  const [error, setError] = useState<string | null>(null);
  const mutation = usePublishMarketComment(marketId, creatorPubkey);

  const trimmed = value.trim();
  const chars = [...value].length;
  const overLimit = chars > MAX_CHARS;
  const canSubmit = trimmed.length > 0 && !overLimit && !mutation.isPending;

  const handleSubmit = useCallback(async () => {
    if (!canSubmit) return;
    setError(null);
    try {
      await mutation.mutateAsync({
        marketEventIdHex: marketEventId,
        body: trimmed,
      });
      setValue("");
    } catch (e) {
      setError(friendlyError(String(e)));
    }
  }, [canSubmit, mutation, marketEventId, trimmed]);

  return (
    <div className="rounded-xl border border-slate-800 bg-slate-950/40 p-4">
      <textarea
        value={value}
        onChange={(e) => {
          setValue(e.target.value);
          if (error) setError(null);
        }}
        placeholder="Share your take on this market…"
        rows={3}
        className="dc-input h-auto min-h-[88px] resize-none"
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
        <button
          type="button"
          onClick={() => void handleSubmit()}
          disabled={!canSubmit}
          className="rounded-lg bg-emerald-400 px-4 py-2 text-sm font-semibold text-slate-950 transition hover:bg-emerald-300 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {mutation.isPending ? "Posting…" : "Post"}
        </button>
      </div>
      <SigningHint active={mutation.isPending} className="mt-2" />
      {error && <p className="mt-2 text-xs text-rose-400">{error}</p>}
    </div>
  );
}
