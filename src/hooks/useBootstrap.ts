import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef } from "react";
import { useStore } from "../store";
import type { IdentityResponse, NostrProfile } from "../types";

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

      // 2. Fetch Nostr profile (kind 0 metadata)
      const { nostrPubkey: loadedPubkey } = useStore.getState();
      if (loadedPubkey) {
        // First attempt — may fail if relays aren't connected yet
        let fetched = false;
        try {
          const profile = await invoke<NostrProfile | null>(
            "fetch_nostr_profile",
          );
          if (profile) {
            useStore.setState({ nostrProfile: profile });
            fetched = true;
          }
        } catch {
          // Will retry below
        }
        // If first attempt failed, retry in background after relays settle
        if (!fetched) {
          setTimeout(async () => {
            try {
              const profile = await invoke<NostrProfile | null>(
                "fetch_nostr_profile",
              );
              if (profile) useStore.setState({ nostrProfile: profile });
            } catch {
              // Give up
            }
          }, 3000);
        }
      }

      // Load relay list
      const { nostrNpub } = useStore.getState();
      if (nostrNpub) {
        try {
          const relays =
            await invoke<{ url: string; has_backup: boolean }[]>(
              "get_relay_list",
            );
          if (relays.length > 0) {
            useStore.setState({ relays });
          } else {
            useStore.setState({
              relays: [
                { url: "wss://relay.damus.io", has_backup: false },
                { url: "wss://relay.primal.net", has_backup: false },
              ],
            });
          }
        } catch {
          useStore.setState({
            relays: [
              { url: "wss://relay.damus.io", has_backup: false },
              { url: "wss://relay.primal.net", has_backup: false },
            ],
          });
        }
      } else {
        useStore.setState({
          relays: [
            { url: "wss://relay.damus.io", has_backup: false },
            { url: "wss://relay.primal.net", has_backup: false },
          ],
        });
      }

      // Load source npub
      try {
        const npub = await invoke<string>("get_source_npub");
        useStore.setState({ sourceNpub: npub, sourceNpubInput: npub });
      } catch {
        /* use default from store */
      }

      // 3. Fetch wallet status
      try {
        const appState = await invoke<{
          walletStatus: "not_created" | "locked" | "unlocked";
          networkStatus: {
            network: "mainnet" | "testnet" | "regtest";
            policyAssetId: string;
          };
        }>("get_app_state");
        const { nostrPubkey: currentPubkey } = useStore.getState();
        // If wallet is locked but no identity exists, the wallet can't be
        // unlocked (the node requires an identity first). Treat as not_created
        // so the user sees wallet setup instead of an unlock prompt.
        const effectiveStatus =
          appState.walletStatus === "locked" && !currentPubkey
            ? "not_created"
            : appState.walletStatus;
        useStore.setState({
          walletStatus: effectiveStatus as
            | "not_created"
            | "locked"
            | "unlocked",
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
