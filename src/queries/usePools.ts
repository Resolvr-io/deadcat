import { useQuery } from "@tanstack/react-query";
import { getPriceHistory, listLmsrPools } from "../services/pools";
import type { LmsrPoolInfo, PriceHistoryEntry } from "../types";

export function usePools(marketId?: string) {
  return useQuery({
    queryKey: ["pools", marketId ?? "all"],
    queryFn: (): Promise<LmsrPoolInfo[]> => listLmsrPools(marketId),
    staleTime: 30_000,
  });
}

export function usePriceHistory(marketId: string | null, limit = 500) {
  return useQuery({
    queryKey: ["priceHistory", marketId],
    queryFn: (): Promise<PriceHistoryEntry[]> =>
      getPriceHistory(marketId ?? "", limit),
    enabled: !!marketId,
    staleTime: 30_000,
  });
}
