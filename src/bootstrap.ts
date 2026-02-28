import { tauriApi } from "./api/tauri.ts";
import { loadMarkets } from "./services/markets.ts";
import { refreshRelaysAndBackup } from "./services/nostr.ts";
import {
  fetchWalletStatus,
  refreshWallet,
  syncCurrentHeightFromLwk,
} from "./services/wallet.ts";
import { state } from "./state.ts";

export async function finishOnboarding(
  render: () => void,
  updateEstClockLabels: () => void,
): Promise<void> {
  state.onboardingStep = null;
  state.onboardingWalletPassword = "";
  state.onboardingWalletMnemonic = "";
  state.onboardingNostrNsec = "";
  state.onboardingNostrGeneratedNsec = "";
  state.onboardingNsecRevealed = false;
  state.onboardingNostrDone = false;
  state.onboardingError = "";
  state.onboardingBackupFound = false;
  state.onboardingBackupScanning = false;

  await fetchWalletStatus();
  render();

  if (state.walletStatus === "unlocked") {
    void refreshWallet(render);
  }
  await loadMarkets();
  state.marketsLoading = false;
  render();
  void syncCurrentHeightFromLwk("liquid-testnet", render, updateEstClockLabels);

  // Fetch relay list + backup status in background
  if (state.nostrNpub) {
    void refreshRelaysAndBackup()
      .then(render)
      .catch(() => {});

    tauriApi
      .fetchNostrProfile()
      .then((profile) => {
        if (profile) {
          state.nostrProfile = profile;
          render();
        }
      })
      .catch(() => {});
  }
}

export async function initApp(
  render: () => void,
  dismissSplash: () => void,
  updateEstClockLabels: () => void,
): Promise<void> {
  render();
  updateEstClockLabels();

  // Track when the minimum loader animation time has elapsed (2 full cycles = 4.8s)
  const splashReady = new Promise<void>((r) => setTimeout(r, 4800));

  // 1. Try to load existing Nostr identity (no auto-generation)
  let hasNostrIdentity = false;
  try {
    const identity = await tauriApi.initNostrIdentity();
    if (identity) {
      state.nostrPubkey = identity.pubkey_hex;
      state.nostrNpub = identity.npub;
      hasNostrIdentity = true;
    }
  } catch (error) {
    console.warn("Failed to load nostr identity:", error);
  }

  // 1b. If we have identity, fetch relay list and profile in background
  if (hasNostrIdentity) {
    void refreshRelaysAndBackup({ fallbackToDefaults: true })
      .then(render)
      .catch(() => {});

    tauriApi
      .fetchNostrProfile()
      .then((profile) => {
        if (profile) {
          state.nostrProfile = profile;
          render();
        }
      })
      .catch(() => {});
  }

  // 2. Fetch wallet status
  await fetchWalletStatus();

  // 3. Determine onboarding state
  const needsNostr = !hasNostrIdentity;
  const needsWallet = state.walletStatus === "not_created";

  if (needsNostr || needsWallet) {
    state.onboardingStep = needsNostr ? "nostr" : "wallet";
    if (!needsNostr) {
      state.onboardingNostrDone = true;
    }
    render();
    await splashReady;
    dismissSplash();
    if (!needsNostr && needsWallet) {
      state.onboardingBackupScanning = true;
      render();
      tauriApi
        .checkNostrBackup()
        .then((status) => {
          if (status.has_backup) {
            state.onboardingBackupFound = true;
            state.onboardingWalletMode = "nostr-restore";
          }
        })
        .catch(() => {})
        .finally(() => {
          state.onboardingBackupScanning = false;
          render();
        });
    }
    return;
  }

  // 4. Normal boot — both identity and wallet exist
  if (state.walletStatus === "unlocked") {
    void refreshWallet(render);
  }

  await Promise.all([loadMarkets(), splashReady]);
  state.marketsLoading = false;
  render();
  dismissSplash();

  void syncCurrentHeightFromLwk("liquid-testnet", render, updateEstClockLabels);
}
