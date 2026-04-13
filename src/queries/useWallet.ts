import { useQuery } from "@tanstack/react-query";
import { tauriApi } from "../api/tauri";
import type { AppNetwork } from "../types";

export type WalletStatusData = {
  walletStatus: "not_created" | "locked" | "unlocked";
  walletNetwork: AppNetwork;
  walletPolicyAssetId: string;
};

export function useWalletStatus() {
  return useQuery({
    queryKey: ["walletStatus"],
    queryFn: async (): Promise<WalletStatusData> => {
      const appState = await tauriApi.getAppState();
      return {
        walletStatus: appState.walletStatus,
        walletNetwork: appState.networkStatus.network,
        walletPolicyAssetId: appState.networkStatus.policyAssetId,
      };
    },
    staleTime: 60_000,
  });
}

export type WalletSnapshotData = {
  balance: Record<string, number>;
  transactions: import("../types").WalletTransaction[];
  swaps: import("../types").PaymentSwap[];
};

export function useWalletSnapshot(enabled = true) {
  return useQuery({
    queryKey: ["walletSnapshot"],
    queryFn: async (): Promise<WalletSnapshotData> => {
      const [balance, transactions, swaps] = await Promise.all([
        tauriApi.getWalletBalance(),
        tauriApi.getWalletTransactions(),
        tauriApi.listPaymentSwaps(),
      ]);
      return {
        balance: balance.assets,
        transactions,
        swaps,
      };
    },
    enabled,
    staleTime: 30_000,
  });
}
