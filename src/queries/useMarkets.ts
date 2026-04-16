import { useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { discoveredToMarket } from "../services/markets";
import { useStore } from "../store";
import type { ChainTipResponse, DiscoveredMarket, Market } from "../types";

// Chain tip is fetched once and cached in module scope.
// Updated in the background — never blocks market display.
let cachedHeight = 0;
invoke<ChainTipResponse>("fetch_chain_tip", { network: "liquid-testnet" })
  .then((tip) => {
    cachedHeight = tip.height;
  })
  .catch(() => {});

export function useMarkets() {
  return useQuery({
    queryKey: ["markets"],
    queryFn: async (): Promise<Market[]> => {
      let stored: DiscoveredMarket[];
      try {
        stored = await invoke<DiscoveredMarket[]>("discover_contracts");
      } catch {
        try {
          stored = await invoke<DiscoveredMarket[]>("list_contracts");
        } catch (error) {
          console.warn("Failed to load markets:", error);
          return [];
        }
      }

      const markets = stored.map((d) => ({
        ...discoveredToMarket(d),
        currentHeight: cachedHeight,
      }));
      if (markets.length > 0 && useStore.getState().marketsLoading) {
        useStore.setState({ marketsLoading: false });
      }
      return markets;
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
