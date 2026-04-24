import { useEffect, useRef, useState } from "react";

/**
 * Two-tier emoji picker: a small row of visible emojis plus a `…`
 * chip that opens a popover with the remaining set. Used today by
 * the comment-reaction row; structured as a reusable shared
 * component so future surfaces (mute reasons, tagged reactions,
 * etc.) slot in without duplicating the popover plumbing.
 *
 * Emoji choice lives at the call site — this component is just the
 * picker shell. See `CommentReactions.tsx` for the deadcat-specific
 * roster.
 */
export function EmojiPicker({
  visible,
  more,
  onPick,
  selected = new Set(),
  triggerLabel = "More reactions",
}: {
  /** Emojis shown inline on the trigger row (usually 6–8). */
  visible: readonly string[];
  /** Emojis tucked behind the `…` popover. */
  more: readonly string[];
  /** Fired when any emoji is clicked — inline or in the popover. */
  onPick: (emoji: string) => void;
  /**
   * Emojis the viewer has already reacted with. Used to render the
   * "toggled on" state so clicking flips the reaction off instead
   * of adding a duplicate.
   */
  selected?: Set<string>;
  triggerLabel?: string;
}) {
  const [open, setOpen] = useState(false);
  const wrapperRef = useRef<HTMLDivElement>(null);

  // Close the popover on outside click.
  useEffect(() => {
    if (!open) return;
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
  }, [open]);

  // Close on Escape.
  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [open]);

  return (
    <div ref={wrapperRef} className="relative inline-flex items-center gap-1">
      {visible.map((emoji) => (
        <button
          key={emoji}
          type="button"
          onClick={() => onPick(emoji)}
          aria-pressed={selected.has(emoji)}
          className={`flex h-7 w-7 items-center justify-center rounded-full text-base transition ${
            selected.has(emoji)
              ? "bg-emerald-400/20 ring-1 ring-emerald-400/50"
              : "hover:bg-slate-800"
          }`}
        >
          <span aria-hidden="true">{emoji}</span>
        </button>
      ))}
      {more.length > 0 && (
        <button
          type="button"
          onClick={() => setOpen((v) => !v)}
          aria-haspopup="menu"
          aria-expanded={open}
          title={triggerLabel}
          className="flex h-7 w-7 items-center justify-center rounded-full text-slate-400 transition hover:bg-slate-800 hover:text-slate-200"
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
            <circle cx="12" cy="12" r="1" />
            <circle cx="19" cy="12" r="1" />
            <circle cx="5" cy="12" r="1" />
          </svg>
        </button>
      )}
      {open && (
        <div
          role="menu"
          className="absolute bottom-full right-0 z-20 mb-1 grid grid-cols-4 gap-1 rounded-lg border border-slate-700 bg-slate-900 p-2 shadow-xl"
        >
          {more.map((emoji) => (
            <button
              key={emoji}
              type="button"
              onClick={() => {
                onPick(emoji);
                setOpen(false);
              }}
              aria-pressed={selected.has(emoji)}
              className={`flex h-8 w-8 items-center justify-center rounded-full text-base transition ${
                selected.has(emoji)
                  ? "bg-emerald-400/20 ring-1 ring-emerald-400/50"
                  : "hover:bg-slate-800"
              }`}
            >
              <span aria-hidden="true">{emoji}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
