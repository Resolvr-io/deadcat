import { useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { tauriApi } from "../api/tauri";
import type {
  IdentityResponse,
  NostrBackupStatus,
  NostrProfile,
  RelayEntry,
  RelayBackupResult,
} from "../types";

export function useIdentity(enabled = true) {
  return useQuery({
    queryKey: ["identity"],
    queryFn: () => invoke<IdentityResponse | null>("init_nostr_identity"),
    enabled,
    staleTime: Infinity,
  });
}

export function useNostrProfile(enabled = true) {
  return useQuery({
    queryKey: ["nostrProfile"],
    queryFn: () => invoke<NostrProfile | null>("fetch_nostr_profile"),
    enabled,
    staleTime: 60_000,
  });
}

export function useRelays(enabled = true) {
  return useQuery({
    queryKey: ["relays"],
    queryFn: async (): Promise<RelayEntry[]> => {
      try {
        const urls = await tauriApi.fetchNip65RelayList();
        const relays: RelayEntry[] = urls.map((url) => ({
          url,
          has_backup: false,
        }));

        // Try to enrich with backup status
        try {
          const status = await tauriApi.checkNostrBackup();
          if (status.relay_results) {
            return relays.map((r) => ({
              ...r,
              has_backup:
                status.relay_results.find(
                  (rr: RelayBackupResult) => rr.url === r.url,
                )?.has_backup ?? false,
            }));
          }
        } catch {
          // backup status unavailable
        }

        return relays;
      } catch {
        return [
          { url: "wss://relay.damus.io", has_backup: false },
          { url: "wss://relay.primal.net", has_backup: false },
        ];
      }
    },
    enabled,
    staleTime: 60_000,
  });
}

export function useNostrBackup(enabled = true) {
  return useQuery({
    queryKey: ["nostrBackup"],
    queryFn: (): Promise<NostrBackupStatus> => tauriApi.checkNostrBackup(),
    enabled,
    staleTime: 60_000,
  });
}
