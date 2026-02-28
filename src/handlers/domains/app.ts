import { tauriInvoke as invoke } from "../../api/tauri.ts";
import { refreshRelayBackupStatus } from "../../services/nostr.ts";
import {
  fetchWalletSnapshot,
  fetchWalletStatus,
  refreshWallet,
  resetReceiveState,
  resetSendState,
} from "../../services/wallet.ts";
import { state } from "../../state.ts";
import type { BaseCurrency, IdentityResponse } from "../../types.ts";
import {
  hideOverlayLoader,
  loaderHtml,
  showOverlayLoader,
} from "../../ui/loader.ts";
import { showToast } from "../../ui/toast.ts";
import type { ClickDomainContext } from "./context.ts";

export async function handleAppDomain(ctx: ClickDomainContext): Promise<void> {
  const { action, actionDomain, actionEl, render } = ctx;
  if (actionDomain === "app" || actionDomain === null) {
    // -- App actions --

    if (action === "go-home") {
      state.view = "home";
      state.chartHoverMarketId = null;
      state.chartHoverX = null;
      render();
      return;
    }

    if (action === "set-chart-timescale") {
      const scale = actionEl?.getAttribute("data-scale") as
        | "1H"
        | "3H"
        | "6H"
        | "12H"
        | "1D"
        | null;
      if (scale) {
        state.chartTimescale = scale;
        state.chartHoverMarketId = null;
        state.chartHoverX = null;
        render();
      }
      return;
    }

    if (action === "toggle-user-menu") {
      state.userMenuOpen = !state.userMenuOpen;
      render();
      return;
    }

    if (action === "open-search") {
      state.searchOpen = true;
      render();
      return;
    }

    if (action === "close-search") {
      state.searchOpen = false;
      render();
      return;
    }

    if (action === "open-help") {
      state.helpOpen = true;
      render();
      return;
    }

    if (action === "close-help") {
      state.helpOpen = false;
      render();
      return;
    }

    if (action === "copy-nostr-npub") {
      if (state.nostrNpub) {
        void navigator.clipboard.writeText(state.nostrNpub);
        showToast("Copied npub to clipboard");
      }
      return;
    }

    if (action === "copy-to-clipboard") {
      const value = actionEl?.getAttribute("data-copy-value");
      if (value) {
        void navigator.clipboard.writeText(value);
        showToast("Copied to clipboard");
      }
      return;
    }

    if (action === "set-currency") {
      const currency = actionEl?.getAttribute(
        "data-currency",
      ) as BaseCurrency | null;
      if (currency) {
        state.baseCurrency = currency;
        render();
      }
      return;
    }

    if (action === "user-settings") {
      state.userMenuOpen = false;
      state.settingsOpen = true;
      render();
      return;
    }

    if (action === "toggle-settings-section") {
      const section = actionEl?.getAttribute("data-section");
      if (section) {
        state.settingsSection[section] = !state.settingsSection[section];
        render();
      }
      return;
    }

    if (action === "close-settings") {
      state.settingsOpen = false;
      state.nostrNsecRevealed = null;
      state.nostrReplacePrompt = false;
      state.nostrReplacePanel = false;
      state.nostrReplaceConfirm = "";
      state.nostrImportNsec = "";
      state.walletDeletePrompt = false;
      state.walletDeleteConfirm = "";
      state.devResetPrompt = false;
      state.devResetConfirm = "";
      render();
      return;
    }

    if (action === "reveal-nostr-nsec") {
      (async () => {
        try {
          const nsec = await invoke<string>("export_nostr_nsec");
          state.nostrNsecRevealed = nsec;
          render();
        } catch (e) {
          showToast(`Failed to export nsec: ${String(e)}`, "error");
        }
      })();
      return;
    }

    if (action === "copy-nostr-nsec") {
      if (state.nostrNsecRevealed) {
        void navigator.clipboard.writeText(state.nostrNsecRevealed);
        state.nostrNsecRevealed = null;
        showToast("Copied nsec to clipboard");
        render();
      }
      return;
    }

    if (action === "nostr-replace-start") {
      state.nostrReplacePrompt = true;
      state.nostrReplaceConfirm = "";
      render();
      const input = document.getElementById(
        "nostr-replace-confirm",
      ) as HTMLInputElement | null;
      input?.focus();
      return;
    }

    if (action === "nostr-replace-cancel") {
      state.nostrReplacePrompt = false;
      state.nostrReplaceConfirm = "";
      render();
      return;
    }

    if (action === "nostr-replace-confirm") {
      if (state.nostrReplaceConfirm.trim().toUpperCase() !== "DELETE") return;
      (async () => {
        try {
          await invoke("delete_nostr_identity");
          state.nostrPubkey = null;
          state.nostrNpub = null;
          state.nostrNsecRevealed = null;
          state.nostrReplacePanel = true;
          state.nostrReplacePrompt = false;
          state.nostrReplaceConfirm = "";
        } catch (e) {
          showToast(`Failed to delete identity: ${String(e)}`, "error");
        }
        render();
      })();
      return;
    }

    if (action === "nostr-replace-back") {
      state.nostrReplacePanel = false;
      state.nostrImportNsec = "";
      render();
      return;
    }

    if (action === "import-nostr-nsec") {
      const nsecInput = state.nostrImportNsec.trim();
      if (!nsecInput) {
        showToast("Paste an nsec to import", "error");
        return;
      }
      state.nostrImporting = true;
      render();
      (async () => {
        try {
          const identity = await invoke<{ pubkey_hex: string; npub: string }>(
            "import_nostr_nsec",
            { nsec: nsecInput },
          );
          state.nostrPubkey = identity.pubkey_hex;
          state.nostrNpub = identity.npub;
          state.nostrImportNsec = "";
          state.nostrNsecRevealed = null;
          state.nostrReplacePanel = false;
          state.nostrReplaceConfirm = "";
          showToast("Nostr key imported successfully", "success");
        } catch (e) {
          showToast(`Import failed: ${String(e)}`, "error");
        }
        state.nostrImporting = false;
        render();
      })();
      return;
    }

    if (action === "generate-new-nostr-key") {
      (async () => {
        try {
          const identity = await invoke<IdentityResponse>(
            "generate_nostr_identity",
          );
          state.nostrPubkey = identity.pubkey_hex;
          state.nostrNpub = identity.npub;
          state.nostrNsecRevealed = null;
          state.nostrReplacePanel = false;
          state.nostrReplaceConfirm = "";
          showToast("New Nostr keypair generated", "success");
        } catch (e) {
          showToast(`Failed: ${String(e)}`, "error");
        }
        render();
      })();
      return;
    }

    if (action === "dev-restart") {
      state.settingsOpen = false;
      render();
      const splash = document.createElement("div");
      splash.id = "splash";
      splash.innerHTML = loaderHtml();
      document.body.appendChild(splash);
      setTimeout(() => {
        splash.classList.add("fade-out");
        splash.addEventListener("transitionend", () => splash.remove(), {
          once: true,
        });
      }, 4800);
      return;
    }

    if (action === "dev-reset-start") {
      state.devResetPrompt = true;
      state.devResetConfirm = "";
      render();
      return;
    }

    if (action === "dev-reset-cancel") {
      state.devResetPrompt = false;
      state.devResetConfirm = "";
      render();
      return;
    }

    if (action === "dev-reset-confirm") {
      if (state.devResetConfirm.trim().toUpperCase() !== "RESET") return;
      (async () => {
        try {
          await invoke("delete_nostr_identity");
          try {
            await invoke("delete_wallet");
          } catch (_) {
            /* no wallet is fine */
          }
          state.nostrPubkey = null;
          state.nostrNpub = null;
          state.nostrNsecRevealed = null;
          state.walletData = null;
          state.walletPassword = "";
          state.walletPasswordConfirm = "";
          state.walletMnemonic = "";
          state.walletError = "";
          state.walletStatus = "not_created";
          state.onboardingError = "";
          state.settingsOpen = false;
          state.devResetPrompt = false;
          state.devResetConfirm = "";
          await fetchWalletStatus();
          state.onboardingStep = "nostr";
          render();
          showToast("App data erased", "success");
        } catch (e) {
          showToast(`Reset failed: ${String(e)}`, "error");
        }
      })();
      return;
    }

    // -- Relay management handlers --

    if (action === "add-relay") {
      const input = document.getElementById(
        "relay-input",
      ) as HTMLInputElement | null;
      const url = (input?.value ?? state.relayInput).trim();
      if (!url) return;
      if (!url.startsWith("wss://") && !url.startsWith("ws://")) {
        showToast("Relay URL must start with wss://", "error");
        return;
      }
      state.relayLoading = true;
      render();
      (async () => {
        try {
          const list = await invoke<string[]>("add_relay", { url });
          state.relays = list.map((u) => ({ url: u, has_backup: false }));
          try {
            await refreshRelayBackupStatus();
          } catch (refreshError) {
            console.warn(
              "Failed to refresh relay backup status:",
              refreshError,
            );
          }
          state.relayInput = "";
          showToast("Relay added", "success");
        } catch (e) {
          showToast(`Failed to add relay: ${String(e)}`, "error");
        } finally {
          state.relayLoading = false;
          render();
        }
      })();
      return;
    }

    if (action === "remove-relay") {
      const url = actionEl?.dataset.relay;
      if (!url) return;
      state.relayLoading = true;
      render();
      (async () => {
        try {
          const list = await invoke<string[]>("remove_relay", { url });
          state.relays = list.map((u) => ({ url: u, has_backup: false }));
          try {
            await refreshRelayBackupStatus();
          } catch (refreshError) {
            console.warn(
              "Failed to refresh relay backup status:",
              refreshError,
            );
          }
          showToast("Relay removed", "success");
        } catch (e) {
          showToast(`Failed to remove relay: ${String(e)}`, "error");
        } finally {
          state.relayLoading = false;
          render();
        }
      })();
      return;
    }

    if (action === "reset-relays") {
      state.relayLoading = true;
      render();
      (async () => {
        try {
          await invoke("set_relay_list", {
            relays: ["wss://relay.damus.io", "wss://relay.primal.net"],
          });
          state.relays = [
            { url: "wss://relay.damus.io", has_backup: false },
            { url: "wss://relay.primal.net", has_backup: false },
          ];
          try {
            await refreshRelayBackupStatus();
          } catch (refreshError) {
            console.warn(
              "Failed to refresh relay backup status:",
              refreshError,
            );
          }
          showToast("Relays reset to defaults", "success");
        } catch (e) {
          showToast(`Failed to reset relays: ${String(e)}`, "error");
        } finally {
          state.relayLoading = false;
          render();
        }
      })();
      return;
    }

    // -- Nostr backup handlers --

    if (action === "nostr-backup-wallet") {
      state.nostrBackupLoading = true;
      render();
      (async () => {
        try {
          await invoke("backup_mnemonic_to_nostr", { password: "" });
          await refreshRelayBackupStatus();
          showToast("Wallet backed up to Nostr relays", "success");
        } catch (e) {
          showToast(`Backup failed: ${String(e)}`, "error");
        } finally {
          state.nostrBackupLoading = false;
          render();
        }
      })();
      return;
    }

    if (action === "cancel-backup-prompt") {
      state.nostrBackupPrompt = false;
      state.nostrBackupPassword = "";
      render();
      return;
    }

    if (action === "settings-backup-wallet") {
      // If wallet is locked and password prompt not yet shown, show it
      if (state.walletStatus !== "unlocked" && !state.nostrBackupPrompt) {
        state.nostrBackupPrompt = true;
        render();
        return;
      }
      const password =
        state.walletStatus === "unlocked" ? "" : state.nostrBackupPassword;
      if (state.walletStatus !== "unlocked" && !password) {
        showToast("Enter your wallet password", "error");
        return;
      }
      state.nostrBackupLoading = true;
      render();
      (async () => {
        try {
          await invoke("backup_mnemonic_to_nostr", { password });
          await refreshRelayBackupStatus();
          state.nostrBackupPassword = "";
          state.nostrBackupPrompt = false;
          showToast("Wallet backed up to Nostr relays", "success");
        } catch (e) {
          showToast(`Backup failed: ${String(e)}`, "error");
        } finally {
          state.nostrBackupLoading = false;
          render();
        }
      })();
      return;
    }

    if (action === "delete-nostr-backup") {
      state.nostrBackupLoading = true;
      render();
      (async () => {
        try {
          await invoke("delete_nostr_backup");
          await refreshRelayBackupStatus();
          const status = state.nostrBackupStatus;
          if (!status) {
            showToast("Backup deletion request sent to relays", "success");
            return;
          }
          if (status.has_backup) {
            const remaining = (status.relay_results ?? []).filter(
              (r) => r.has_backup,
            ).length;
            showToast(
              `Backup still on ${remaining} relay${remaining !== 1 ? "s" : ""} — some relays may delay deletion`,
              "warning",
            );
          } else {
            showToast("Backup deleted from all relays", "success");
          }
        } catch (e) {
          showToast(`Delete failed: ${String(e)}`, "error");
        } finally {
          state.nostrBackupLoading = false;
          render();
        }
      })();
      return;
    }

    if (action === "nostr-restore-wallet") {
      showOverlayLoader("Fetching backup from relays...");
      (async () => {
        try {
          const mnemonic = await invoke<string>("restore_mnemonic_from_nostr");
          hideOverlayLoader();
          // Pre-fill the mnemonic in the restore form
          state.walletShowRestore = true;
          state.walletRestoreMnemonic = mnemonic;
          render();
          showToast("Recovery phrase retrieved from Nostr", "success");
        } catch (e) {
          hideOverlayLoader();
          showToast(String(e), "error");
        }
      })();
      return;
    }

    if (action === "user-logout") {
      state.userMenuOpen = false;
      state.logoutOpen = true;
      render();
      return;
    }

    if (action === "close-logout") {
      state.logoutOpen = false;
      render();
      return;
    }

    if (action === "confirm-logout") {
      state.logoutOpen = false;
      (async () => {
        try {
          await invoke("lock_wallet");
          await fetchWalletStatus();
          state.walletData = null;
          state.walletPassword = "";
          state.walletPasswordConfirm = "";
          state.walletError = "";
          state.walletModal = "none";
          resetReceiveState();
          resetSendState();
          state.view = "home";
        } catch (e) {
          console.warn("Failed to lock wallet:", e);
        }
        render();
      })();
      return;
    }

    if (action === "open-create-market") {
      state.view = "create";
      render();
      return;
    }

    if (action === "open-wallet") {
      state.walletError = "";
      state.walletPassword = "";
      state.walletPasswordConfirm = "";
      state.settingsOpen = false;
      state.view = "wallet";
      render();
      // If already unlocked with cached balance, just do a silent background sync
      if (state.walletStatus === "unlocked" && state.walletData) {
        void invoke("sync_wallet")
          .then(async () => {
            const { balance, transactions, swaps } =
              await fetchWalletSnapshot();
            if (state.walletData) {
              state.walletData.balance = balance;
              state.walletData.transactions = transactions;
              state.walletData.swaps = swaps;
            }
            render();
          })
          .catch(() => {});
      } else {
        void fetchWalletStatus().then(() => {
          render();
          if (state.walletStatus === "unlocked") {
            void refreshWallet(render);
          }
        });
      }
      return;
    }
  }
}
