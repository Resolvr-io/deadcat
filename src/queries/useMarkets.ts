import { useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { discoveredToMarket } from "../services/markets";
import type { ChainTipResponse, DiscoveredMarket, Market } from "../types";

export function useMarkets() {
  return useQuery({
    queryKey: ["markets"],
    queryFn: async (): Promise<Market[]> => {
      // Fetch chain tip so time-remaining calculations are accurate.
      let currentHeight = 0;
      try {
        const tip = await invoke<ChainTipResponse>("fetch_chain_tip", {
          network: "liquid-testnet",
        });
        currentHeight = tip.height;
      } catch {
        // Non-fatal — falls back to 0 (shows raw block distance)
      }

      try {
        const stored = await invoke<DiscoveredMarket[]>("discover_contracts");
        return stored.map((d) => ({
          ...discoveredToMarket(d),
          currentHeight,
        }));
      } catch {
        try {
          const stored = await invoke<DiscoveredMarket[]>("list_contracts");
          return stored.map((d) => ({
            ...discoveredToMarket(d),
            currentHeight,
          }));
        } catch (error) {
          console.warn("Failed to load markets:", error);
          return [];
        }
      }
    },
    staleTime: 30_000,
  });
}

export function useMarketOrders(marketId: string | null) {
  return useQuery({
    queryKey: ["orders", marketId],
    queryFn: () =>
      invoke<import("../types").DiscoveredOrder[]>("fetch_orders", {
        marketId,
      }),
    enabled: !!marketId,
    staleTime: 15_000,
  });
}

export function useOwnOrders(enabled = true) {
  return useQuery({
    queryKey: ["ownOrders"],
    queryFn: () =>
      invoke<import("../types").OwnOrderSummary[]>("list_own_orders"),
    enabled,
    staleTime: 30_000,
  });
}
