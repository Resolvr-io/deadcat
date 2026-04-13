import { useQuery } from "@tanstack/react-query";
import { tauriApi } from "../api/tauri";
import type { ChainTipResponse, WalletNetwork } from "../types";

export function useChainTip(network: WalletNetwork = "liquid-testnet") {
  return useQuery({
    queryKey: ["chainTip", network],
    queryFn: (): Promise<ChainTipResponse> => tauriApi.fetchChainTip(network),
    refetchInterval: 60_000,
    staleTime: 30_000,
  });
}
