import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useStore } from "../store";
import type { IdentityResponse } from "../types";

function dismissSplash(): void {
  const splash = document.getElementById("splash");
  if (!splash) return;
  splash.classList.add("fade-out");
  splash.addEventListener("transitionend", () => splash.remove(), {
    once: true,
  });
}

export function useBootstrap(): void {
  const ran = useRef(false);

  useEffect(() => {
    if (ran.current) return;
    ran.current = true;

    async function boot(): Promise<void> {
      const splashReady = new Promise<void>((r) => setTimeout(r, 1500));

      // 1. Load Nostr identity
      try {
        const identity = await invoke<IdentityResponse | null>(
          "init_nostr_identity",
        );
        if (identity) {
          useStore.setState({
            nostrPubkey: identity.pubkey_hex,
            nostrNpub: identity.npub,
          });
        }
      } catch (error) {
        console.warn("Failed to load nostr identity:", error);
      }

      // 2. Set default relays if no identity
      const { nostrNpub } = useStore.getState();
      if (!nostrNpub) {
        useStore.setState({
          relays: [
            { url: "wss://relay.damus.io", has_backup: false },
            { url: "wss://relay.primal.net", has_backup: false },
          ],
        });
      }

      // 3. Fetch wallet status
      try {
        const appState = await invoke<{
          walletStatus: "not_created" | "locked" | "unlocked";
          networkStatus: { network: "mainnet" | "testnet" | "regtest"; policyAssetId: string };
        }>("get_app_state");
        useStore.setState({
          walletStatus: appState.walletStatus,
          walletNetwork: appState.networkStatus.network,
          walletPolicyAssetId: appState.networkStatus.policyAssetId,
        });
      } catch (e) {
        console.warn("Failed to fetch app state:", e);
      }

      // 4. Load persisted tx labels
      try {
        const stored = JSON.parse(
          localStorage.getItem("deadcat_tx_labels") ?? "{}",
        );
        const labels = new Map<string, { label: string; marketId: string }>();
        for (const [txid, entry] of Object.entries(stored)) {
          const { label, marketId } = entry as {
            label: string;
            marketId: string;
          };
          labels.set(txid, { label, marketId });
        }
        if (labels.size > 0) {
          useStore.setState({ recentTxLabels: labels });
        }
      } catch {
        // localStorage unavailable
      }

      // 5. Dismiss splash
      await splashReady;
      dismissSplash();

      // Mark loading done
      useStore.setState({ marketsLoading: false });
    }

    void boot();
  }, []);
}
