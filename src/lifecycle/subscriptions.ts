import { listen } from "@tauri-apps/api/event";
import { refreshMarketsFromStore } from "../services/markets.ts";
import { createWalletData, state } from "../state.ts";
import type { WalletTransaction, WalletUtxo } from "../types.ts";

export function setupTauriSubscriptions(render: () => void): void {
  let marketRefreshInFlight = false;
  let marketRefreshQueued = false;

  const scheduleMarketRefresh = (): void => {
    if (marketRefreshInFlight) {
      marketRefreshQueued = true;
      return;
    }

    marketRefreshInFlight = true;
    void refreshMarketsFromStore()
      .then(render)
      .finally(() => {
        marketRefreshInFlight = false;
        if (marketRefreshQueued) {
          marketRefreshQueued = false;
          scheduleMarketRefresh();
        }
      });
  };

  void listen<{
    walletStatus: "not_created" | "locked" | "unlocked";
  }>("app_state_updated", (event) => {
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
  });

  void listen<{
    balance: { assets: Record<string, number> };
    transactions: WalletTransaction[];
    utxos: WalletUtxo[];
  } | null>("wallet_snapshot", (event) => {
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
  });

  for (const eventName of [
    "discovery:market",
    "discovery:attestation",
    "discovery:pool",
  ]) {
    void listen(eventName, scheduleMarketRefresh);
  }
}
