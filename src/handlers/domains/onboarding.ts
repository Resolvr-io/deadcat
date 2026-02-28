import { tauriInvoke as invoke } from "../../api/tauri.ts";
import { restoreWalletAndSync } from "../../services/wallet.ts";
import { state } from "../../state.ts";
import type { IdentityResponse, NostrBackupStatus } from "../../types.ts";
import {
  hideOverlayLoader,
  showOverlayLoader,
  updateOverlayMessage,
} from "../../ui/loader.ts";
import { showToast } from "../../ui/toast.ts";
import type { ClickDomainContext } from "./context.ts";

export async function handleOnboardingDomain(
  ctx: ClickDomainContext,
): Promise<void> {
  const { action, actionDomain, actionEl, render, finishOnboarding } = ctx;
  if (actionDomain === "onboarding" || actionDomain === null) {
    // -- Onboarding actions --

    if (action === "onboarding-set-nostr-mode") {
      state.onboardingNostrMode = (actionEl?.getAttribute("data-mode") ??
        "generate") as "generate" | "import";
      state.onboardingError = "";
      render();
      return;
    }

    if (action === "onboarding-generate-nostr") {
      state.onboardingLoading = true;
      state.onboardingError = "";
      render();
      (async () => {
        try {
          const identity = await invoke<IdentityResponse>(
            "generate_nostr_identity",
          );
          state.nostrPubkey = identity.pubkey_hex;
          state.nostrNpub = identity.npub;
          const nsec = await invoke<string>("export_nostr_nsec");
          state.onboardingNostrGeneratedNsec = nsec;
          state.onboardingNostrDone = true;
        } catch (e) {
          state.onboardingError = String(e);
        }
        state.onboardingLoading = false;
        render();
      })();
      return;
    }

    if (action === "onboarding-import-nostr") {
      const nsecInput = state.onboardingNostrNsec.trim();
      if (!nsecInput) {
        state.onboardingError = "Paste an nsec to import.";
        render();
        return;
      }
      state.onboardingLoading = true;
      state.onboardingError = "";
      render();
      (async () => {
        try {
          const identity = await invoke<IdentityResponse>("import_nostr_nsec", {
            nsec: nsecInput,
          });
          state.nostrPubkey = identity.pubkey_hex;
          state.nostrNpub = identity.npub;
          state.onboardingNostrDone = true;
          state.onboardingStep = "wallet";
          // Auto-scan relays for existing wallet backup
          state.onboardingBackupScanning = true;
          state.onboardingLoading = false;
          render();
          try {
            const status =
              await invoke<NostrBackupStatus>("check_nostr_backup");
            if (status.has_backup) {
              state.onboardingBackupFound = true;
              state.onboardingWalletMode = "nostr-restore";
            }
          } catch (_) {
            /* scan failed silently */
          }
          state.onboardingBackupScanning = false;
          render();
          return;
        } catch (e) {
          state.onboardingError = String(e);
        }
        state.onboardingLoading = false;
        render();
      })();
      return;
    }

    if (action === "onboarding-copy-npub") {
      if (state.nostrNpub) {
        void navigator.clipboard.writeText(state.nostrNpub);
        showToast("Copied npub to clipboard");
      }
      return;
    }

    if (action === "onboarding-reveal-nsec") {
      state.onboardingNsecRevealed = true;
      render();
      return;
    }

    if (action === "onboarding-copy-nsec") {
      if (state.onboardingNostrGeneratedNsec) {
        void navigator.clipboard.writeText(state.onboardingNostrGeneratedNsec);
        state.onboardingNsecRevealed = false;
        state.onboardingNostrGeneratedNsec = "";
        showToast("Copied nsec to clipboard");
        render();
      }
      return;
    }

    if (action === "onboarding-nostr-continue") {
      state.onboardingStep = "wallet";
      state.onboardingError = "";
      render();
      return;
    }

    if (action === "onboarding-set-wallet-mode") {
      state.onboardingWalletMode = (actionEl?.getAttribute("data-mode") ??
        "create") as "create" | "restore" | "nostr-restore";
      state.onboardingError = "";
      render();
      return;
    }

    if (action === "onboarding-create-wallet") {
      if (!state.onboardingWalletPassword) {
        state.onboardingError = "Password is required.";
        render();
        return;
      }
      if (
        state.onboardingWalletPassword !== state.onboardingWalletPasswordConfirm
      ) {
        state.onboardingError = "Passwords do not match.";
        render();
        return;
      }
      state.onboardingLoading = true;
      state.onboardingError = "";
      showOverlayLoader("Creating wallet...");
      render();
      (async () => {
        try {
          const mnemonic = await invoke<string>("create_wallet", {
            password: state.onboardingWalletPassword,
          });
          state.onboardingWalletMnemonic = mnemonic;
        } catch (e) {
          state.onboardingError = String(e);
        }
        state.onboardingLoading = false;
        hideOverlayLoader();
        render();
      })();
      return;
    }

    if (action === "onboarding-copy-mnemonic") {
      if (state.onboardingWalletMnemonic) {
        void navigator.clipboard.writeText(state.onboardingWalletMnemonic);
        showToast("Copied recovery phrase to clipboard");
      }
      return;
    }

    if (action === "onboarding-wallet-done") {
      showOverlayLoader("Loading markets...");
      (async () => {
        await finishOnboarding();
        hideOverlayLoader();
      })();
      return;
    }

    if (action === "onboarding-restore-wallet") {
      if (
        !state.onboardingWalletMnemonic.trim() ||
        !state.onboardingWalletPassword
      ) {
        state.onboardingError = "Recovery phrase and password are required.";
        render();
        return;
      }
      if (
        state.onboardingWalletPassword !== state.onboardingWalletPasswordConfirm
      ) {
        state.onboardingError = "Passwords do not match.";
        render();
        return;
      }
      state.onboardingLoading = true;
      state.onboardingError = "";
      showOverlayLoader("Restoring wallet...");
      render();
      (async () => {
        try {
          await restoreWalletAndSync({
            mnemonic: state.onboardingWalletMnemonic,
            password: state.onboardingWalletPassword,
            unlock: true,
            setStep: updateOverlayMessage,
          });
          updateOverlayMessage("Loading markets...");
          showToast("Wallet restored!", "success");
          await finishOnboarding();
          hideOverlayLoader();
        } catch (e) {
          state.onboardingError = String(e);
          state.onboardingLoading = false;
          hideOverlayLoader();
          render();
        }
      })();
      return;
    }

    if (action === "onboarding-nostr-restore-wallet") {
      if (!state.onboardingWalletPassword) {
        state.onboardingError = "Password is required.";
        render();
        return;
      }
      if (
        state.onboardingWalletPassword !== state.onboardingWalletPasswordConfirm
      ) {
        state.onboardingError = "Passwords do not match.";
        render();
        return;
      }
      state.onboardingLoading = true;
      state.onboardingError = "";
      showOverlayLoader("Fetching backup from relays...");
      render();
      (async () => {
        try {
          const mnemonic = await invoke<string>("restore_mnemonic_from_nostr");
          updateOverlayMessage("Restoring wallet...");
          await restoreWalletAndSync({
            mnemonic,
            password: state.onboardingWalletPassword,
            unlock: true,
            setStep: updateOverlayMessage,
          });
          updateOverlayMessage("Loading markets...");
          showToast("Wallet restored from Nostr backup!", "success");
          await finishOnboarding();
          hideOverlayLoader();
        } catch (e) {
          state.onboardingError = String(e);
          state.onboardingLoading = false;
          hideOverlayLoader();
          render();
        }
      })();
      return;
    }
  }
}
