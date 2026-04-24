import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../store";

/** Mirrors the Rust-side `FollowListResult` payload. */
type FollowListResult = {
  follows: string[];
  /** Unix seconds on the source event, or 0 when none exists yet. */
  created_at: number;
};

const FOLLOW_LIST_KEY = "followList" as const;

function followListQueryKey(viewerPubkey: string | null) {
  return [FOLLOW_LIST_KEY, viewerPubkey ?? ""] as const;
}

/**
 * Fetch the viewer's NIP-02 follow list. Keyed on the viewer pubkey
 * so signing out / switching identity invalidates cleanly. Disabled
 * when no session — there's nothing to fetch since kind:3 is scoped
 * to a single author and the command requires a signer. Stale time
 * leans long (2 min) because follow lists don't change often and the
 * mutation helpers below invalidate on demand.
 */
export function useFollowList() {
  const sessionPubkey = useStore((s) => s.nostrPubkey);
  return useQuery({
    queryKey: followListQueryKey(sessionPubkey),
    queryFn: () => invoke<FollowListResult>("fetch_follow_list"),
    enabled: !!sessionPubkey,
    staleTime: 120_000,
  });
}

/**
 * Add a pubkey to the viewer's follow list. Optimistically updates
 * the cached `useFollowList` result so the UI flips to "Following"
 * immediately; rolled back on error. The backend is idempotent — a
 * second `follow_pubkey` call on an already-followed target returns
 * the current list without publishing, so racing clicks are safe.
 */
export function useFollowPubkey() {
  const sessionPubkey = useStore((s) => s.nostrPubkey);
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (targetPubkeyHex: string) =>
      invoke<FollowListResult>("follow_pubkey", {
        targetPubkeyHex,
      }),
    onMutate: async (targetPubkeyHex) => {
      const key = followListQueryKey(sessionPubkey);
      await queryClient.cancelQueries({ queryKey: key });
      const previous = queryClient.getQueryData<FollowListResult>(key);
      if (previous && !previous.follows.includes(targetPubkeyHex)) {
        queryClient.setQueryData<FollowListResult>(key, {
          ...previous,
          follows: [...previous.follows, targetPubkeyHex],
        });
      }
      return { previous };
    },
    onError: (_err, _target, context) => {
      if (context?.previous) {
        queryClient.setQueryData(
          followListQueryKey(sessionPubkey),
          context.previous,
        );
      }
    },
    onSuccess: (data) => {
      // Write the server-confirmed list straight to cache so the
      // next read sees the authoritative result without a refetch
      // round-trip.
      queryClient.setQueryData<FollowListResult>(
        followListQueryKey(sessionPubkey),
        data,
      );
    },
  });
}

/** Mirror of `useFollowPubkey` for removal. */
export function useUnfollowPubkey() {
  const sessionPubkey = useStore((s) => s.nostrPubkey);
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (targetPubkeyHex: string) =>
      invoke<FollowListResult>("unfollow_pubkey", {
        targetPubkeyHex,
      }),
    onMutate: async (targetPubkeyHex) => {
      const key = followListQueryKey(sessionPubkey);
      await queryClient.cancelQueries({ queryKey: key });
      const previous = queryClient.getQueryData<FollowListResult>(key);
      if (previous) {
        queryClient.setQueryData<FollowListResult>(key, {
          ...previous,
          follows: previous.follows.filter((hex) => hex !== targetPubkeyHex),
        });
      }
      return { previous };
    },
    onError: (_err, _target, context) => {
      if (context?.previous) {
        queryClient.setQueryData(
          followListQueryKey(sessionPubkey),
          context.previous,
        );
      }
    },
    onSuccess: (data) => {
      queryClient.setQueryData<FollowListResult>(
        followListQueryKey(sessionPubkey),
        data,
      );
    },
  });
}

/**
 * True when the given pubkey is in the viewer's follow list. Safe to
 * call during cache-miss — returns false while the list is loading,
 * which matches the UX of "assume not-following until confirmed."
 */
export function useIsFollowing(targetPubkeyHex: string | null | undefined) {
  const { data } = useFollowList();
  if (!targetPubkeyHex || !data) return false;
  return data.follows.includes(targetPubkeyHex);
}
