import { useQuery } from "@tanstack/react-query";

/** NIP-05 `name@domain` → canonical lowercase `{ local, domain }`.
 *  Returns `null` for malformed input so callers can short-circuit
 *  without a fetch. */
function parseNip05(nip05: string): { local: string; domain: string } | null {
  const trimmed = nip05.trim();
  if (!trimmed) return null;
  const at = trimmed.lastIndexOf("@");
  if (at <= 0 || at === trimmed.length - 1) return null;
  const local = trimmed.slice(0, at).toLowerCase();
  const domain = trimmed.slice(at + 1).toLowerCase();
  // Minimal domain sanity — at least one dot, no whitespace.
  if (!/^[a-z0-9.\-_]+\.[a-z0-9.\-_]+$/i.test(domain)) return null;
  // Local part per NIP-05 must be `[a-z0-9-_.]+`.
  if (!/^[a-z0-9\-_.]+$/i.test(local)) return null;
  return { local, domain };
}

type WellKnownResponse = {
  names?: Record<string, string>;
};

/**
 * Verify a NIP-05 identifier by fetching
 * `https://<domain>/.well-known/nostr.json?name=<local>` and checking
 * that the returned pubkey matches the expected one. Uses the WebView
 * `fetch` — Tauri v2 permits outbound HTTPS, and most mainstream
 * NIP-05 providers (nostrplebs, primal, iris, vanity-domain plebs)
 * set `Access-Control-Allow-Origin: *` because they expect browser
 * verification. Providers without CORS fall into the `null`
 * (unverified) bucket, which renders as a muted grey check rather
 * than a false-negative "failed verification" badge.
 *
 * Returns:
 * - `true` — verified match
 * - `false` — fetched successfully but the pubkey didn't match (or
 *   the local name was missing)
 * - `null` — fetch failed, network offline, CORS blocked, or the
 *   nip05 is malformed. Caller should treat as "unverified" rather
 *   than "invalid" so transient errors don't shame users.
 *
 * Cached for one hour per (pubkey, nip05) pair — NIP-05 mappings are
 * stable enough that aggressive caching is fine, and this query runs
 * once per comment row in long threads so cache hits matter.
 */
export function useNip05Verification(
  pubkeyHex: string | null | undefined,
  nip05: string | null | undefined,
) {
  const parsed = nip05 ? parseNip05(nip05) : null;
  return useQuery({
    queryKey: ["nip05Verification", pubkeyHex ?? "", nip05 ?? ""] as const,
    queryFn: async (): Promise<boolean | null> => {
      if (!pubkeyHex || !parsed) return null;
      try {
        const url = `https://${parsed.domain}/.well-known/nostr.json?name=${encodeURIComponent(
          parsed.local,
        )}`;
        const res = await fetch(url, { method: "GET" });
        if (!res.ok) return false;
        const json = (await res.json()) as WellKnownResponse;
        const mapped = json.names?.[parsed.local];
        if (!mapped) return false;
        return mapped.toLowerCase() === pubkeyHex.toLowerCase();
      } catch {
        return null;
      }
    },
    enabled: !!pubkeyHex && !!parsed,
    staleTime: 60 * 60 * 1000,
    // Don't spam retries — an offline domain should fall back to
    // "unverified" quickly rather than blocking re-render for 30s.
    retry: false,
  });
}
