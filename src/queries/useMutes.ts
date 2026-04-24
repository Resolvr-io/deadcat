import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { useMemo } from "react";
import { useStore } from "../store";

/** Matches the Rust-side `MuteEntry` serde shape
 *  (`#[serde(tag = "kind", content = "value")]`). Other mute-entry
 *  kinds (events, hashtags, words) round-trip through the backend
 *  unchanged — we just don't expose UI for them yet. */
type MuteEntry =
  | { kind: "p"; value: string }
  | { kind: "e"; value: string }
  | { kind: "t"; value: string }
  | { kind: "word"; value: string };

/** Matches Rust-side `MuteVisibility`. */
type MuteVisibility = "none" | "public" | "private" | "mixed";

/** Matches Rust-side `MuteListResult`. */
type MuteListResult = {
  entries: MuteEntry[];
  visibility: MuteVisibility;
  created_at: number;
};

const MUTE_LIST_KEY = "muteList" as const;

function muteListQueryKey(viewerPubkey: string | null) {
  return [MUTE_LIST_KEY, viewerPubkey ?? ""] as const;
}

/**
 * Fetch the viewer's NIP-51 mute list, with private entries decrypted
 * by the Rust command via `NostrSigner::nip44_decrypt`. Keyed on the
 * session pubkey so identity switches invalidate cleanly; disabled
 * when signed out.
 */
export function useMuteList() {
  const sessionPubkey = useStore((s) => s.nostrPubkey);
  return useQuery({
    queryKey: muteListQueryKey(sessionPubkey),
    queryFn: () => invoke<MuteListResult>("fetch_mute_list"),
    enabled: !!sessionPubkey,
    staleTime: 120_000,
  });
}

export function useMutePubkey() {
  const sessionPubkey = useStore((s) => s.nostrPubkey);
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (targetPubkeyHex: string) =>
      invoke<MuteListResult>("mute_pubkey", {
        targetPubkeyHex,
        // Pass `null` so the backend mirrors the user's existing
        // visibility (public / private). Explicit public/private
        // toggles are a Settings-surface concern, not a per-row one.
        private: null,
      }),
    onMutate: async (targetPubkeyHex) => {
      const key = muteListQueryKey(sessionPubkey);
      await queryClient.cancelQueries({ queryKey: key });
      const previous = queryClient.getQueryData<MuteListResult>(key);
      if (previous) {
        const alreadyMuted = previous.entries.some(
          (e) => e.kind === "p" && e.value === targetPubkeyHex,
        );
        if (!alreadyMuted) {
          queryClient.setQueryData<MuteListResult>(key, {
            ...previous,
            entries: [
              ...previous.entries,
              { kind: "p", value: targetPubkeyHex },
            ],
          });
        }
      }
      return { previous };
    },
    onError: (_err, _target, context) => {
      if (context?.previous) {
        queryClient.setQueryData(
          muteListQueryKey(sessionPubkey),
          context.previous,
        );
      }
    },
    onSuccess: (data) => {
      queryClient.setQueryData<MuteListResult>(
        muteListQueryKey(sessionPubkey),
        data,
      );
    },
  });
}

export function useUnmutePubkey() {
  const sessionPubkey = useStore((s) => s.nostrPubkey);
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (targetPubkeyHex: string) =>
      invoke<MuteListResult>("unmute_pubkey", {
        targetPubkeyHex,
      }),
    onMutate: async (targetPubkeyHex) => {
      const key = muteListQueryKey(sessionPubkey);
      await queryClient.cancelQueries({ queryKey: key });
      const previous = queryClient.getQueryData<MuteListResult>(key);
      if (previous) {
        queryClient.setQueryData<MuteListResult>(key, {
          ...previous,
          entries: previous.entries.filter(
            (e) => !(e.kind === "p" && e.value === targetPubkeyHex),
          ),
        });
      }
      return { previous };
    },
    onError: (_err, _target, context) => {
      if (context?.previous) {
        queryClient.setQueryData(
          muteListQueryKey(sessionPubkey),
          context.previous,
        );
      }
    },
    onSuccess: (data) => {
      queryClient.setQueryData<MuteListResult>(
        muteListQueryKey(sessionPubkey),
        data,
      );
    },
  });
}

/** True when `targetPubkeyHex` is in the viewer's pubkey mutes. */
export function useIsMuted(targetPubkeyHex: string | null | undefined) {
  const { data } = useMuteList();
  if (!targetPubkeyHex || !data) return false;
  return data.entries.some(
    (e) => e.kind === "p" && e.value === targetPubkeyHex,
  );
}

/** Hex pubkeys currently muted by the viewer. Empty while loading or
 *  signed-out. Memoised on the source data reference so consumers can
 *  compare identity safely when feeding React deps. Used by
 *  `CommentsSection` to filter the rendered list. */
export function useMutedPubkeys(): Set<string> {
  const { data } = useMuteList();
  return useMemo(() => {
    if (!data) return EMPTY_SET;
    const set = new Set<string>();
    for (const entry of data.entries) {
      if (entry.kind === "p") set.add(entry.value);
    }
    return set;
  }, [data]);
}

const EMPTY_SET: Set<string> = new Set();
