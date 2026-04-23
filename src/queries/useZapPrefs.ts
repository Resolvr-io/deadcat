import { useCallback, useEffect, useState } from "react";

/**
 * User preferences for the default zap amount and optional default
 * zap comment, persisted in `localStorage`. Defaults match typical
 * Nostr client conventions (21 sats, empty comment).
 */
const DEFAULT_SATS_KEY = "deadcat:zapDefaultSats";
const DEFAULT_COMMENT_KEY = "deadcat:zapDefaultComment";

const FALLBACK_SATS = 21;
const FALLBACK_COMMENT = "";

function readSats(): number {
  if (typeof localStorage === "undefined") return FALLBACK_SATS;
  const raw = localStorage.getItem(DEFAULT_SATS_KEY);
  if (!raw) return FALLBACK_SATS;
  const n = Number.parseInt(raw, 10);
  return Number.isFinite(n) && n > 0 ? n : FALLBACK_SATS;
}

function readComment(): string {
  if (typeof localStorage === "undefined") return FALLBACK_COMMENT;
  return localStorage.getItem(DEFAULT_COMMENT_KEY) ?? FALLBACK_COMMENT;
}

/** Read-only accessors usable outside React. */
export function getDefaultZapSats(): number {
  return readSats();
}

export function getDefaultZapComment(): string {
  return readComment();
}

export function useZapPrefs() {
  const [defaultSats, setDefaultSatsState] = useState<number>(readSats);
  const [defaultComment, setDefaultCommentState] =
    useState<string>(readComment);

  // Keep state in sync if another tab (or the settings panel itself in
  // a different component tree) mutates the underlying value.
  useEffect(() => {
    function onStorage(e: StorageEvent) {
      if (e.key === DEFAULT_SATS_KEY) setDefaultSatsState(readSats());
      if (e.key === DEFAULT_COMMENT_KEY) setDefaultCommentState(readComment());
    }
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, []);

  const setDefaultSats = useCallback((sats: number) => {
    const clean = Math.max(1, Math.floor(sats));
    localStorage.setItem(DEFAULT_SATS_KEY, String(clean));
    setDefaultSatsState(clean);
  }, []);

  const setDefaultComment = useCallback((comment: string) => {
    localStorage.setItem(DEFAULT_COMMENT_KEY, comment);
    setDefaultCommentState(comment);
  }, []);

  return {
    defaultSats,
    defaultComment,
    setDefaultSats,
    setDefaultComment,
  };
}
