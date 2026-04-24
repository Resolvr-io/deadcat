import { useEffect } from "react";
import { useStore } from "../store";

/** Duration of the post-scroll ring pulse on the target row. */
const PULSE_MS = 2000;

/** CSS class applied to the target row for `PULSE_MS`. Matches the
 *  `@keyframes animate-highlight-pulse` rule in style.css. */
const PULSE_CLASS = "animate-notification-pulse";

/**
 * When the navigation store carries a `focusCommentId`, scroll the
 * matching comment into view and apply a brief ring pulse so the
 * user can visually land on the comment a notification pointed at.
 *
 * Safe to mount before the comments query resolves — the hook
 * re-runs until an element with the target id appears in the DOM,
 * then clears `focusCommentId` so subsequent renders don't re-pulse.
 * Callers (CommentRow) are expected to stamp `data-comment-id` on
 * their root element.
 */
export function useScrollToComment(): void {
  const focusCommentId = useStore((s) => s.focusCommentId);

  useEffect(() => {
    if (!focusCommentId) return;

    let raf = 0;
    let attempts = 0;
    const maxAttempts = 40; // ~1.5s worst case at 30fps

    const tryScroll = () => {
      const el = document.querySelector<HTMLElement>(
        `[data-comment-id="${CSS.escape(focusCommentId)}"]`,
      );
      if (!el) {
        attempts += 1;
        if (attempts < maxAttempts) {
          raf = window.requestAnimationFrame(tryScroll);
        } else {
          // Give up silently — the comment may have been deleted
          // between the notification arriving and the user clicking.
          useStore.setState({ focusCommentId: null });
        }
        return;
      }
      el.scrollIntoView({ behavior: "smooth", block: "center" });
      el.classList.add(PULSE_CLASS);
      window.setTimeout(() => el.classList.remove(PULSE_CLASS), PULSE_MS);
      useStore.setState({ focusCommentId: null });
    };

    raf = window.requestAnimationFrame(tryScroll);
    return () => window.cancelAnimationFrame(raf);
  }, [focusCommentId]);
}
