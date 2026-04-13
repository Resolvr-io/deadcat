import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { queryClient } from "../queries/queryClient";
import { useStore } from "../store";
import type { WalletTransaction, WalletUtxo } from "../types";

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
          useStore.setState({ walletData: null });
          void queryClient.invalidateQueries({
            queryKey: ["walletSnapshot"],
          });
        }
      }),
    );

    // ── discovery events ─────────────────────────────────────────
    for (const eventName of ["discovery:market", "discovery:attestation"]) {
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
