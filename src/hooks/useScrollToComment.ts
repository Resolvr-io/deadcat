import { useEffect } from "react";
import { useStore } from "../store";

/** Duration of the post-scroll ring pulse on the target row. */
const PULSE_MS = 2000;

/** CSS class applied to the target row for `PULSE_MS`. */
const PULSE_CLASS = "animate-notification-pulse";

/** Max wait for the target row to appear in the DOM before giving
 *  up. Comments are fetched from relays with a 10s server-side
 *  timeout, so allow generous headroom — a user who clicked a
 *  notification expects the app to wait, not silently skip. */
const MAX_WAIT_MS = 15_000;

/**
 * When the navigation store carries a `focusCommentId`, scroll the
 * matching comment into view and apply a brief ring pulse so the
 * user can visually land on the comment a notification pointed at.
 *
 * Safe to mount before the comments query resolves. Tries an
 * immediate lookup (covers the case where the user was already on
 * the detail page); falls back to a MutationObserver watching for
 * the element to appear (covers the cold-navigation case where the
 * comment rows mount only after the React Query resolves).
 *
 * Callers (CommentRow) must stamp `data-comment-id` on the row's
 * root element. On success — or a `MAX_WAIT_MS` timeout — the
 * `focusCommentId` is cleared so subsequent renders don't re-pulse.
 */
export function useScrollToComment(): void {
  const focusCommentId = useStore((s) => s.focusCommentId);

  useEffect(() => {
    if (!focusCommentId) return;

    const selector = `[data-comment-id="${CSS.escape(focusCommentId)}"]`;

    const landOn = (el: HTMLElement) => {
      el.scrollIntoView({ behavior: "smooth", block: "center" });
      el.classList.add(PULSE_CLASS);
      window.setTimeout(() => el.classList.remove(PULSE_CLASS), PULSE_MS);
      useStore.setState({ focusCommentId: null });
    };

    const now = document.querySelector<HTMLElement>(selector);
    if (now) {
      landOn(now);
      return;
    }

    // Defer to a MutationObserver so we don't miss the row when it
    // appears between React Query's fetch settling and the list
    // rendering. Cheaper than polling + reliably catches the first
    // paint of the element.
    let done = false;
    const observer = new MutationObserver(() => {
      if (done) return;
      const found = document.querySelector<HTMLElement>(selector);
      if (found) {
        done = true;
        observer.disconnect();
        // requestAnimationFrame gives the layout pass a tick to
        // settle before the scroll — without it the element's
        // computed position can be 0 on the first tick.
        window.requestAnimationFrame(() => landOn(found));
      }
    });
    observer.observe(document.body, { childList: true, subtree: true });

    const timeout = window.setTimeout(() => {
      if (done) return;
      done = true;
      observer.disconnect();
      // Clear so a stale id doesn't re-fire the next time
      // something else pushes focusCommentId through the store.
      useStore.setState({ focusCommentId: null });
    }, MAX_WAIT_MS);

    return () => {
      done = true;
      observer.disconnect();
      window.clearTimeout(timeout);
    };
  }, [focusCommentId]);
}
