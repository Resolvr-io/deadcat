import { useEffect, useRef } from "react";
import { createPortal } from "react-dom";

// Web component side-effect import. `emoji-picker-element` registers
// the `<emoji-picker>` custom element in the DOM once on load;
// subsequent imports are no-ops. Lazy dynamic-import would shave
// initial bundle weight but the picker is reachable from the
// reactions popover on every detail page, so eager is fine.
import "emoji-picker-element";

/**
 * React type extension for the web component so JSX accepts it.
 * `emoji-picker-element` is framework-agnostic — we interact via
 * DOM APIs (addEventListener for `emoji-click`, CSS custom
 * properties for theming). No React bindings needed beyond this.
 */
declare module "react" {
  namespace JSX {
    interface IntrinsicElements {
      "emoji-picker": React.DetailedHTMLProps<
        React.HTMLAttributes<HTMLElement> & { class?: string },
        HTMLElement
      >;
    }
  }
}

type EmojiClickEvent = CustomEvent<{ unicode?: string; skinTone?: number }>;

/**
 * Floating full emoji picker portal. Used by `CommentReactions` when
 * the user clicks "More emojis" — anchors to the triggering button's
 * rect, falls back to a sensible position when the rect would push
 * it off-screen, and closes on outside click or Escape.
 *
 * The underlying `<emoji-picker>` web component ships its own emoji
 * database, search bar, category tabs, and skin-tone picker — the
 * same library primal-web-spark and zapcooking use — so we don't
 * hand-roll any of that.
 */
export function FullEmojiPicker({
  anchorRect,
  onPick,
  onClose,
}: {
  /** Bounding rect of the button that opened the picker. Used to
   *  anchor the floating panel; `null` falls back to viewport
   *  center. Callers typically capture this in a `useRef` +
   *  `getBoundingClientRect()` at open-time. */
  anchorRect: DOMRect | null;
  onPick: (emoji: string) => void;
  onClose: () => void;
}) {
  const wrapperRef = useRef<HTMLDivElement>(null);
  const pickerRef = useRef<HTMLElement>(null);

  // Close on Escape.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [onClose]);

  // Close on outside click.
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (
        wrapperRef.current &&
        !wrapperRef.current.contains(e.target as Node)
      ) {
        onClose();
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [onClose]);

  // Subscribe to the web component's `emoji-click` event. The event
  // detail carries a `unicode` field with the selected character.
  useEffect(() => {
    const el = pickerRef.current;
    if (!el) return;
    const handler = (e: Event) => {
      const emoji = (e as EmojiClickEvent).detail?.unicode;
      if (emoji) onPick(emoji);
    };
    el.addEventListener("emoji-click", handler);
    return () => el.removeEventListener("emoji-click", handler);
  }, [onPick]);

  // Position: anchor under the trigger, clamp inside the viewport.
  // emoji-picker-element defaults to ~350px × 400px; we size to
  // that and keep 8px off the edges.
  const PICKER_W = 352;
  const PICKER_H = 400;
  const PAD = 8;
  let left = anchorRect
    ? anchorRect.left
    : window.innerWidth / 2 - PICKER_W / 2;
  let top = anchorRect ? anchorRect.bottom + PAD : window.innerHeight / 2;
  if (left + PICKER_W > window.innerWidth - PAD) {
    left = window.innerWidth - PICKER_W - PAD;
  }
  if (left < PAD) left = PAD;
  // Flip above the anchor when there's no room below.
  if (anchorRect && top + PICKER_H > window.innerHeight - PAD) {
    top = anchorRect.top - PICKER_H - PAD;
    if (top < PAD) top = PAD;
  }

  return createPortal(
    <div
      ref={wrapperRef}
      className="fixed z-[60]"
      style={{ top, left, width: PICKER_W }}
    >
      <emoji-picker
        ref={pickerRef}
        class="deadcat-emoji-picker"
        style={
          {
            "--background": "#0f172a",
            "--border-color": "#334155",
            "--input-border-color": "#334155",
            "--input-font-color": "#e2e8f0",
            "--input-placeholder-color": "#64748b",
            "--indicator-color": "#34d399",
            "--outline-color": "#34d399",
            "--category-emoji-size": "1.125rem",
            "--emoji-size": "1.25rem",
          } as React.CSSProperties
        }
      />
    </div>,
    document.body,
  );
}
