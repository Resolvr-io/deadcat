import { listen } from "@tauri-apps/api/event";
import { useEffect } from "react";
import { showToast } from "../components/shared/Toast";
import { queryClient } from "../queries/queryClient";
import { useStore } from "../store";
import type { WalletTransaction, WalletUtxo } from "../types";

function formatSats(sats: number): string {
  return `${Math.abs(sats).toLocaleString()} sats`;
}

function toastForNewTx(tx: WalletTransaction): void {
  if (tx.balanceChange > 0) {
    showToast(`Received ${formatSats(tx.balanceChange)}`, "success");
  } else if (tx.balanceChange < 0) {
    showToast(`Sent ${formatSats(tx.balanceChange)}`, "info");
  }
}

export function useTauriEvents(): void {
  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];

    const register = (p: Promise<() => void>): void => {
      void p.then((unlisten) => {
        if (disposed) {
          unlisten();
          return;
        }
        unlisteners.push(unlisten);
      });
    };

    // ── app_state_updated ────────────────────────────────────────
    register(
      listen<{ walletStatus: "not_created" | "locked" | "unlocked" }>(
        "app_state_updated",
        (event) => {
          if (disposed) return;
          const { walletStatus } = event.payload;
          const current = useStore.getState().walletStatus;
          if (walletStatus === "locked" && current === "unlocked") {
            useStore.setState({
              walletStatus: "locked",
              walletData: null,
              walletMnemonic: "",
              walletModal: "none",
            });
          }
          void queryClient.invalidateQueries({ queryKey: ["walletStatus"] });
        },
      ),
    );

    // ── wallet_snapshot ──────────────────────────────────────────
    let snapshotTimer: ReturnType<typeof setTimeout> | null = null;
    register(
      listen<{
        balance: { assets: Record<string, number> };
        transactions: WalletTransaction[];
        utxos: WalletUtxo[];
      } | null>("wallet_snapshot", (event) => {
        if (disposed) return;
        const payload = event.payload;
        if (payload) {
          const { walletData } = useStore.getState();
          const base = walletData ?? {
            balance: {},
            transactions: [],
            utxos: [],
            swaps: [],
            backupWords: [],
            backedUp: false,
            showBackup: false,
            backupPassword: "",
            backupCopied: false,
          };

          // Detect new transactions for toasts + badge
          const { walletSeenTxids, walletNewTxids } = useStore.getState();
          if (walletSeenTxids.size > 0) {
            const freshNew = new Set(walletNewTxids);
            for (const tx of payload.transactions) {
              if (!walletSeenTxids.has(tx.txid)) {
                toastForNewTx(tx);
                freshNew.add(tx.txid);
              }
            }
            if (freshNew.size !== walletNewTxids.size) {
              useStore.setState({ walletNewTxids: freshNew });
            }
          }

          // Update seen set to current txid list
          const nextSeen = new Set(payload.transactions.map((t) => t.txid));
          useStore.setState({ walletSeenTxids: nextSeen });

          useStore.setState({
            walletData: {
              ...base,
              balance: payload.balance.assets,
              transactions: payload.transactions,
              utxos: payload.utxos,
            },
          });
          // Debounce query invalidation during sync
          if (snapshotTimer === null) {
            snapshotTimer = setTimeout(() => {
              snapshotTimer = null;
              if (!disposed) {
                void queryClient.invalidateQueries({
                  queryKey: ["walletSnapshot"],
                });
              }
            }, 250);
          }
        } else {
          const current = useStore.getState().walletStatus;
          if (current !== "not_created") {
            useStore.setState({ walletStatus: "locked" });
          }
          useStore.setState({
            walletData: null,
            walletSeenTxids: new Set(),
            walletNewTxids: new Set(),
          });
          void queryClient.invalidateQueries({
            queryKey: ["walletSnapshot"],
          });
        }
      }),
    );

    // ── discovery events ─────────────────────────────────────────
    for (const eventName of [
      "discovery:market",
      "discovery:market-refresh",
      "discovery:attestation",
    ]) {
      register(
        listen(eventName, () => {
          if (disposed) return;
          void queryClient.invalidateQueries({ queryKey: ["markets"] });
        }),
      );
    }

    register(
      listen("discovery:pool", () => {
        if (disposed) return;
        void queryClient.invalidateQueries({ queryKey: ["markets"] });
        void queryClient.invalidateQueries({ queryKey: ["pools"] });
      }),
    );

    register(
      listen("discovery:order", () => {
        if (disposed) return;
        void queryClient.invalidateQueries({ queryKey: ["markets"] });
        void queryClient.invalidateQueries({ queryKey: ["orders"] });
        void queryClient.invalidateQueries({ queryKey: ["ownOrders"] });
      }),
    );

    register(
      listen("discovery:orders-invalidated", () => {
        if (disposed) return;
        void queryClient.invalidateQueries({ queryKey: ["markets"] });
        void queryClient.invalidateQueries({ queryKey: ["orders"] });
      }),
    );

    return () => {
      disposed = true;
      if (snapshotTimer !== null) clearTimeout(snapshotTimer);
      for (const unlisten of unlisteners) unlisten();
    };
  }, []);
}
