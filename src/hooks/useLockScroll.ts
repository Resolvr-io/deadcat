import { useEffect } from "react";

/**
 * Lock body scroll while `enabled` is true. Defaults to true so existing
 * callers that mount conditionally keep their behavior. Pass an explicit
 * `enabled` when the component itself stays mounted but has an open/closed
 * state (e.g. dialogs rendered inside list rows).
 */
export function useLockScroll(enabled = true): void {
  useEffect(() => {
    if (!enabled) return;
    const prev = document.body.style.overflow;
    const prevHtml = document.documentElement.style.overflow;
    document.body.style.overflow = "hidden";
    document.documentElement.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = prev;
      document.documentElement.style.overflow = prevHtml;
    };
  }, [enabled]);
}
