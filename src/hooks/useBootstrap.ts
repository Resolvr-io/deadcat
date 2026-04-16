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

      // ── Immediate: non-network state ──────────────────────────────
      // Load persisted tx labels (sync localStorage)
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

      // ── Parallel: independent backend queries ─────────────────────
      // None of these depend on each other. Fire all at once.
      const [, , appState] = await Promise.all([
        // Source npub (needed for market filtering)
        invoke<string>("get_source_npub")
          .then((npub) =>
            useStore.setState({ sourceNpub: npub, sourceNpubInput: npub }),
          )
          .catch(() => {}),

        // Relay list
        invoke<{ url: string; has_backup: boolean }[]>("get_relay_list")
          .then((relays) => {
            if (relays.length > 0) {
              useStore.setState({ relays });
            }
          })
          .catch(() => {}),

        // Wallet / network status
        invoke<{
          walletStatus: "not_created" | "locked" | "unlocked";
          networkStatus: {
            network: "mainnet" | "testnet" | "regtest";
            policyAssetId: string;
          };
        }>("get_app_state").catch(() => null),
      ]);

      if (appState) {
        useStore.setState({
          walletNetwork: appState.networkStatus.network,
          walletPolicyAssetId: appState.networkStatus.policyAssetId,
        });
      }

      // ── Background: identity + node (does NOT block splash) ───────
      // Node construction is fast now (relay connect is spawned in bg).
      // Profile fetch retries after relays settle.
      invoke<IdentityResponse | null>("init_nostr_identity")
        .then((identity) => {
          if (identity) {
            useStore.setState({
              nostrPubkey: identity.pubkey_hex,
              nostrNpub: identity.npub,
            });
          }
          // Re-evaluate wallet status now that we know if identity exists
          if (appState) {
            const { nostrPubkey: pk } = useStore.getState();
            const effectiveStatus =
              appState.walletStatus === "locked" && !pk
                ? "not_created"
                : appState.walletStatus;
            useStore.setState({
              walletStatus: effectiveStatus as
                | "not_created"
                | "locked"
                | "unlocked",
            });
          }
          // Fetch profile once relays are connected (retry after delay)
          if (identity) {
            invoke<NostrProfile | null>("fetch_nostr_profile")
              .then((p) => {
                if (p) useStore.setState({ nostrProfile: p });
              })
              .catch(() => {
                setTimeout(() => {
                  invoke<NostrProfile | null>("fetch_nostr_profile")
                    .then((p) => {
                      if (p) useStore.setState({ nostrProfile: p });
                    })
                    .catch(() => {});
                }, 3000);
              });
          }
        })
        .catch((e) => console.warn("Failed to load nostr identity:", e));

      // Set initial wallet status (without identity check) so UI isn't blank
      if (appState) {
        useStore.setState({
          walletStatus: appState.walletStatus as
            | "not_created"
            | "locked"
            | "unlocked",
        });
      }

      // ── Dismiss splash ────────────────────────────────────────────
      await splashReady;
      dismissSplash();
    }

    void boot();
  }, []);
}
