import { listen } from "@tauri-apps/api/event";
import { refreshMarketsFromStore } from "../services/markets.ts";
import { createWalletData, state } from "../state.ts";
import type { WalletTransaction, WalletUtxo } from "../types.ts";

export function setupTauriSubscriptions(render: () => void): () => void {
  let marketRefreshInFlight = false;
  let marketRefreshQueued = false;
  let disposed = false;
  const unlisteners: Array<() => void> = [];

  const registerListener = (listenerPromise: Promise<() => void>): void => {
    void listenerPromise.then((unlisten) => {
      if (disposed) {
        void unlisten();
        return;
      }
      unlisteners.push(unlisten);
    });
  };

  const scheduleMarketRefresh = (): void => {
    if (disposed) return;
    if (marketRefreshInFlight) {
      marketRefreshQueued = true;
      return;
    }

    marketRefreshInFlight = true;
    void refreshMarketsFromStore()
      .then(() => {
        if (disposed) return;
        render();
      })
      .finally(() => {
        if (disposed) return;
        marketRefreshInFlight = false;
        if (marketRefreshQueued) {
          marketRefreshQueued = false;
          scheduleMarketRefresh();
        }
      });
  };

  registerListener(
    listen<{
      walletStatus: "not_created" | "locked" | "unlocked";
    }>("app_state_updated", (event) => {
      if (disposed) return;
      const payload = event.payload;
      if (
        payload.walletStatus === "locked" &&
        state.walletStatus === "unlocked"
      ) {
        state.walletStatus = "locked";
        state.walletData = null;
        state.walletMnemonic = "";
        state.walletModal = "none";
        render();
      }
    }),
  );

  registerListener(
    listen<{
      balance: { assets: Record<string, number> };
      transactions: WalletTransaction[];
      utxos: WalletUtxo[];
    } | null>("wallet_snapshot", (event) => {
      if (disposed) return;
      const payload = event.payload;
      if (payload) {
        if (!state.walletData) state.walletData = createWalletData();
        state.walletData.balance = payload.balance.assets;
        state.walletData.transactions = payload.transactions;
        state.walletData.utxos = payload.utxos;
      } else {
        // A null snapshot means the wallet was locked
        state.walletStatus = "locked";
        state.walletData = null;
      }
      render();
    }),
  );

  for (const eventName of [
    "discovery:market",
    "discovery:attestation",
    "discovery:pool",
  ]) {
    registerListener(listen(eventName, scheduleMarketRefresh));
  }

  return () => {
    disposed = true;
    while (unlisteners.length > 0) {
      const unlisten = unlisteners.pop();
      if (!unlisten) continue;
      void unlisten();
    }
  };
}
