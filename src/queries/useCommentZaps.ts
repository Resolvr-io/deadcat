import { useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";

/** Matches the Rust-side `ZapSummary` shape. */
type ZapSummary = {
  event_id: string;
  count: number;
  total_msats: number;
};

export type ZapStats = {
  count: number;
  totalSats: number;
};

const EMPTY: ZapStats = { count: 0, totalSats: 0 };

const ZAP_KEY = "commentZaps" as const;

export function commentZapsKey(marketId: string, creatorPubkey: string) {
  return [ZAP_KEY, marketId, creatorPubkey] as const;
}

/**
 * Fetch NIP-57 zap receipts for every comment in a market thread and
 * expose a `Map<eventId, ZapStats>` for per-row rendering. Same query
 * shape as `useMarketComments` so the pair live together in the
 * cache and can be invalidated in lockstep after a local zap.
 */
export function useCommentZaps(
  marketId: string | null | undefined,
  creatorPubkey: string | null | undefined,
  commentIds: string[],
): Map<string, ZapStats> {
  // Sort the id list so cache identity is order-independent — two
  // renders of the same comment thread share a query even if React
  // reorders them.
  const sortedIds = [...commentIds].sort();
  const key =
    marketId && creatorPubkey
      ? [...commentZapsKey(marketId, creatorPubkey), sortedIds.join(",")]
      : [ZAP_KEY, "disabled"];

  const { data } = useQuery<ZapSummary[]>({
    queryKey: key,
    queryFn: async () => {
      if (sortedIds.length === 0) return [];
      return invoke<ZapSummary[]>("fetch_comment_zaps", {
        eventIdsHex: sortedIds,
      });
    },
    enabled: !!(marketId && creatorPubkey) && sortedIds.length > 0,
    staleTime: 30_000,
  });

  const map = new Map<string, ZapStats>();
  if (data) {
    for (const summary of data) {
      map.set(summary.event_id, {
        count: summary.count,
        totalSats: Math.floor(summary.total_msats / 1000),
      });
    }
  }
  return map;
}

export function zapStatsFor(
  map: Map<string, ZapStats>,
  eventId: string,
): ZapStats {
  return map.get(eventId) ?? EMPTY;
}
