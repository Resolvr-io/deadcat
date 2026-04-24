import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { tauriApi } from "../api/tauri";
import { useStore } from "../store";
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
    // Relay indexing of kind:5 deletions lags; the next fetch can
    // briefly bring the comment back un-deleted before the relay
    // reflects our kind:5. Mark locally-deleted ids as tombstones so
    // the thread slot stays preserved (Reddit-style) regardless of
    // relay state.
    select: (data) =>
      deletedIds.size > 0
        ? data.map((c) => (deletedIds.has(c.id) ? { ...c, deleted: true } : c))
        : data,
  });
}

export function usePublishMarketComment(
  marketId: string,
  creatorPubkey: string,
) {
  const qc = useQueryClient();
  const sessionPubkey = useStore((s) => s.nostrPubkey);
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
    // Insert the user's new comment into the cache immediately so the
    // thread updates without waiting for relay propagation. The real
    // event (with its final id + signature) replaces this temporary
    // entry in `onSuccess`; on error the entry is removed so the
    // user sees the failure cleanly.
    onMutate: (args) => {
      if (!sessionPubkey) return;
      const tempId = `optimistic-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
      const optimistic: MarketComment = {
        id: tempId,
        author_pubkey: sessionPubkey,
        content: args.body,
        created_at: Math.floor(Date.now() / 1000),
        market_id: marketId,
        parent_id: args.parentEventIdHex ?? null,
        deleted: false,
      };
      const key = keyFor(marketId, creatorPubkey);
      qc.setQueryData<MarketComment[]>(key, (prev) => {
        const base = prev ?? [];
        return [optimistic, ...base];
      });
      return { tempId };
    },
    onSuccess: (real, _args, ctx) => {
      const key = keyFor(marketId, creatorPubkey);
      if (ctx?.tempId) {
        qc.setQueryData<MarketComment[]>(key, (prev) => {
          if (!prev) return prev;
          // Replace the optimistic placeholder with the real event so
          // the row picks up its actual id for reaction / zap / delete
          // affordances.
          return prev.map((c) => (c.id === ctx.tempId ? real : c));
        });
      }
      void qc.invalidateQueries({ queryKey: key });
    },
    onError: (_err, _args, ctx) => {
      if (!ctx?.tempId) return;
      const key = keyFor(marketId, creatorPubkey);
      qc.setQueryData<MarketComment[]>(key, (prev) => {
        if (!prev) return prev;
        return prev.filter((c) => c.id !== ctx.tempId);
      });
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
      // Relay indexing of the kind:5 deletion lags, so a naive
      // refetch could bring the comment back un-deleted before the
      // relay reflects the deletion. Track the id for the session so
      // `useMarketComments` keeps flipping it to `deleted: true` even
      // if the relay round-trip is slow, and flip the cache now so
      // the tombstone appears without waiting for the next fetch.
      useDeletedCommentIds.getState().add(commentEventIdHex);
      const key = keyFor(marketId, creatorPubkey);
      qc.setQueryData<MarketComment[]>(key, (prev) =>
        prev
          ? prev.map((c) =>
              c.id === commentEventIdHex ? { ...c, deleted: true } : c,
            )
          : prev,
      );
      void qc.invalidateQueries({ queryKey: key, refetchType: "none" });
    },
  });
}
