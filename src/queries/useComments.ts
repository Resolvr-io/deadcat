import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { tauriApi } from "../api/tauri";
import type { MarketComment } from "../types";
import { useDeletedCommentIds } from "./deletedComments";

const KEY = "marketComments" as const;

function keyFor(marketId: string, creatorPubkey: string) {
  return [KEY, marketId, creatorPubkey] as const;
}

export function useMarketComments(
  marketId: string | null | undefined,
  creatorPubkey: string | null | undefined,
) {
  const deletedIds = useDeletedCommentIds((s) => s.ids);
  return useQuery<MarketComment[]>({
    queryKey:
      marketId && creatorPubkey
        ? keyFor(marketId, creatorPubkey)
        : [KEY, "disabled"],
    queryFn: async () => {
      if (!marketId || !creatorPubkey) return [];
      const comments = await tauriApi.fetchMarketComments({
        marketIdHex: marketId,
        creatorPubkeyHex: creatorPubkey,
      });
      // Sort newest first — backend returns in relay order which is
      // close but not guaranteed.
      return [...comments].sort((a, b) => b.created_at - a.created_at);
    },
    enabled: !!(marketId && creatorPubkey),
    staleTime: 15_000,
    select: (data) =>
      deletedIds.size > 0 ? data.filter((c) => !deletedIds.has(c.id)) : data,
  });
}

export function usePublishMarketComment(
  marketId: string,
  creatorPubkey: string,
) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (args: {
      marketEventIdHex: string;
      body: string;
      parentEventIdHex?: string;
      parentAuthorPubkeyHex?: string;
    }) =>
      tauriApi.publishMarketComment({
        marketIdHex: marketId,
        creatorPubkeyHex: creatorPubkey,
        marketEventIdHex: args.marketEventIdHex,
        body: args.body,
        parentEventIdHex: args.parentEventIdHex,
        parentAuthorPubkeyHex: args.parentAuthorPubkeyHex,
      }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: keyFor(marketId, creatorPubkey) });
    },
  });
}

export function useDeleteMarketComment(
  marketId: string,
  creatorPubkey: string,
) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (commentEventIdHex: string) =>
      tauriApi.deleteMarketComment(commentEventIdHex),
    onSuccess: (_data, commentEventIdHex) => {
      // Relay indexing of the kind:5 deletion lags, so refetches can
      // briefly bring the comment back. Remember the deleted id for this
      // session and filter it out wherever comments are read; drop it
      // from the cache now so the UI updates immediately.
      useDeletedCommentIds.getState().add(commentEventIdHex);
      const key = keyFor(marketId, creatorPubkey);
      qc.setQueryData<MarketComment[]>(key, (prev) =>
        prev ? prev.filter((c) => c.id !== commentEventIdHex) : prev,
      );
      void qc.invalidateQueries({ queryKey: key, refetchType: "none" });
    },
  });
}
