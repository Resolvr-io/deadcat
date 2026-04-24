import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../store";

/** Matches the Rust-side `ReactionCount` shape. */
type ReactionCount = {
  emoji: string;
  count: number;
  mine: boolean;
  my_event_id?: string;
};

/** Matches the Rust-side `EventReactionSummary` shape. */
type EventReactionSummary = {
  event_id: string;
  reactions: ReactionCount[];
};

export type ReactionStats = {
  emoji: string;
  count: number;
  mine: boolean;
  /** Viewer's own reaction event id — only set when `mine` is true. */
  myEventId?: string;
};

const EMPTY: ReactionStats[] = [];

const REACTION_KEY = "commentReactions" as const;

export function commentReactionsKey(
  marketId: string,
  creatorPubkey: string,
  viewerPubkey: string | null,
) {
  return [REACTION_KEY, marketId, creatorPubkey, viewerPubkey ?? ""] as const;
}

/**
 * Fetch NIP-25 reactions for every comment in a market thread.
 * Returns a `Map<eventId, ReactionStats[]>` for per-row rendering.
 * Same cache-key shape as `useCommentZaps` so the two live side by
 * side and can be invalidated in lockstep after a local reaction.
 *
 * The viewer pubkey is part of the cache key because the `mine` flag
 * on each reaction depends on who's asking — signed-out readers get
 * all `mine: false`, signed-in readers get accurate toggle state.
 */
export function useCommentReactions(
  marketId: string | null | undefined,
  creatorPubkey: string | null | undefined,
  commentIds: string[],
): Map<string, ReactionStats[]> {
  const viewerPubkey = useStore((s) => s.nostrPubkey);
  // Sort the id list so cache identity is order-independent.
  const sortedIds = [...commentIds].sort();
  const key =
    marketId && creatorPubkey
      ? [
          ...commentReactionsKey(marketId, creatorPubkey, viewerPubkey),
          sortedIds.join(","),
        ]
      : [REACTION_KEY, "disabled"];

  const { data } = useQuery<EventReactionSummary[]>({
    queryKey: key,
    queryFn: async () => {
      if (sortedIds.length === 0) return [];
      return invoke<EventReactionSummary[]>("fetch_comment_reactions", {
        eventIdsHex: sortedIds,
        viewerPubkeyHex: viewerPubkey,
      });
    },
    enabled: !!(marketId && creatorPubkey) && sortedIds.length > 0,
    staleTime: 30_000,
  });

  const map = new Map<string, ReactionStats[]>();
  if (data) {
    for (const summary of data) {
      map.set(
        summary.event_id,
        summary.reactions.map((r) => ({
          emoji: r.emoji,
          count: r.count,
          mine: r.mine,
          myEventId: r.my_event_id,
        })),
      );
    }
  }
  return map;
}

export function reactionStatsFor(
  map: Map<string, ReactionStats[]>,
  eventId: string,
): ReactionStats[] {
  return map.get(eventId) ?? EMPTY;
}

/**
 * Publish a kind:7 reaction for a comment. Invalidates the reactions
 * query twice — immediately (covers optimistic UI) and again after
 * ~2s so the newly-propagated kind:7 lands in the next fetch.
 */
export function usePublishCommentReaction() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (args: {
      commentEventId: string;
      commentAuthorPubkey: string;
      emoji: string;
    }) => {
      return invoke<string>("publish_comment_reaction", {
        commentEventIdHex: args.commentEventId,
        commentAuthorPubkeyHex: args.commentAuthorPubkey,
        emoji: args.emoji,
      });
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: [REACTION_KEY] });
      setTimeout(() => {
        void queryClient.invalidateQueries({ queryKey: [REACTION_KEY] });
      }, 2000);
    },
  });
}

/**
 * Delete an existing reaction we authored. Caller supplies the
 * reaction event id — typically sourced from the `myEventId` on the
 * `ReactionStats` the UI is rendering.
 */
export function useDeleteCommentReaction() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (reactionEventId: string) => {
      return invoke<void>("delete_comment_reaction", {
        reactionEventIdHex: reactionEventId,
      });
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: [REACTION_KEY] });
      setTimeout(() => {
        void queryClient.invalidateQueries({ queryKey: [REACTION_KEY] });
      }, 2000);
    },
  });
}
