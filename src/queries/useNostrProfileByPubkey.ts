import { useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import type { NostrProfile } from "../types";

/**
 * Fetch a Nostr kind:0 profile by pubkey (hex or npub).
 *
 * Cached per pubkey across the app via React Query, so multiple comment
 * rows for the same author all share a single network fetch. Uses the
 * `preview_nostr_profile` Tauri command — standalone client, short
 * timeouts, no node required.
 */
export function useNostrProfileByPubkey(pubkey: string | null | undefined) {
  return useQuery<NostrProfile | null>({
    queryKey: ["nostrProfile", pubkey ?? "disabled"],
    queryFn: async () => {
      if (!pubkey) return null;
      try {
        return await invoke<NostrProfile | null>("preview_nostr_profile", {
          npub: pubkey,
        });
      } catch {
        // Profile not found / relay timeout — render with the npub
        // fallback instead of failing the whole row.
        return null;
      }
    },
    enabled: !!pubkey,
    staleTime: 5 * 60_000,
    // Don't refetch aggressively; profile updates are infrequent.
    refetchOnWindowFocus: false,
  });
}
