import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { MOCK_MARKETS } from "../mock-markets.ts";
import {
  cancelLimitOrder,
  createLimitOrder,
  discoveredToMarket,
  executeTrade,
  fetchOrders,
  fetchOwnOrders,
  issueTokens,
  marketToContractParamsJson,
  mergeOrdersIntoMarket,
  quoteTrade,
} from "../services/markets.ts";
import {
  fetchWalletStatus,
  generateQr,
  refreshWallet,
  resetReceiveState,
  resetSendState,
} from "../services/wallet.ts";
import {
  createWalletData,
  defaultSettlementInput,
  markets,
  SATS_PER_FULL_CONTRACT,
  setMarkets,
  state,
} from "../state.ts";
import type {
  ActionTab,
  AttestationResult,
  BaseCurrency,
  BoltzChainSwapCreated,
  BoltzChainSwapPairsInfo,
  BoltzLightningReceiveCreated,
  BoltzSubmarineSwapCreated,
  CovenantState,
  DiscoveredMarket,
  IdentityResponse,
  Market,
  MarketCategory,
  NavCategory,
  NostrBackupStatus,
  OrderType,
  PaymentSwap,
  RelayBackupResult,
  Side,
  SizeMode,
  TradeDirection,
  TradeIntent,
  WalletTransaction,
} from "../types.ts";
import {
  hideOverlayLoader,
  loaderHtml,
  showOverlayLoader,
  updateOverlayMessage,
} from "../ui/loader.ts";
import { showToast } from "../ui/toast.ts";
import { reverseHex } from "../utils/crypto.ts";
import { formatSats, formatSatsInput } from "../utils/format.ts";
import {
  clampContractPriceSats,
  commitTradeContractsDraft,
  getBasePriceSats,
  getPathAvailability,
  getPositionContracts,
  getSelectedMarket,
  getTrendingMarkets,
  setLimitPriceSats,
  stateLabel,
} from "../utils/market.ts";

export type ClickDeps = {
  render: () => void;
  openMarket: (
    id: string,
    options?: { side?: string; intent?: string },
  ) => void;
  finishOnboarding: () => Promise<void>;
};

function ticketActionAllowed(market: Market, tab: ActionTab): boolean {
  const paths = getPathAvailability(market);
  if (tab === "trade") return true;
  if (tab === "issue") return paths.initialIssue || paths.issue;
  if (tab === "redeem") return paths.redeem || paths.expiryRedeem;
  return paths.cancel;
}

function clearTradeQuoteSnapshot(): void {
  state.tradeQuoteSnapshot = null;
  state.tradeError = null;
}

function currentTradeDirection(): TradeDirection {
  return state.tradeIntent === "open" ? "buy" : "sell";
}

function currentExactInput(): number {
  if (currentTradeDirection() === "buy") {
    return Math.max(1, Math.floor(state.tradeSizeSats));
  }
  return Math.max(1, Math.floor(state.tradeContracts));
}

function enforceSizeModeForIntent(): void {
  state.sizeMode = state.tradeIntent === "open" ? "sats" : "contracts";
}

function formatTradeAmount(amount: number, unit: "sats" | "contracts"): string {
  if (unit === "sats") {
    return formatSats(Math.max(0, Math.floor(amount)));
  }
  const normalized = Math.max(0, amount);
  return `${normalized.toLocaleString(undefined, { maximumFractionDigits: 2 })} contracts`;
}

function requireMarketAnchor(
  market: Market,
  action: string,
): NonNullable<Market["anchor"]> | null {
  if (!market.anchor) {
    showToast(`Market has no canonical anchor — cannot ${action}`, "error");
    return null;
  }
  return market.anchor;
}

export async function handleClick(
  e: MouseEvent,
  deps: ClickDeps,
): Promise<void> {
  const { render, openMarket, finishOnboarding } = deps;

  const target = e.target as HTMLElement;
  const categoryEl = target.closest("[data-category]") as HTMLElement | null;
  const openMarketEl = target.closest(
    "[data-open-market]",
  ) as HTMLElement | null;
  const actionEl = target.closest("[data-action]") as HTMLElement | null;
  const sideEl = target.closest("[data-side]") as HTMLElement | null;
  const tradeChoiceEl = target.closest(
    "[data-trade-choice]",
  ) as HTMLElement | null;
  const tradeIntentEl = target.closest(
    "[data-trade-intent]",
  ) as HTMLElement | null;
  const sizeModeEl = target.closest("[data-size-mode]") as HTMLElement | null;
  const tradeSizePresetEl = target.closest(
    "[data-trade-size-sats]",
  ) as HTMLElement | null;
  const tradeSizeDeltaEl = target.closest(
    "[data-trade-size-delta]",
  ) as HTMLElement | null;
  const orderTypeEl = target.closest("[data-order-type]") as HTMLElement | null;
  const tabEl = target.closest("[data-tab]") as HTMLElement | null;

  const category = categoryEl?.getAttribute(
    "data-category",
  ) as NavCategory | null;
  const openMarketId = openMarketEl?.getAttribute("data-open-market") ?? null;
  const openSide = openMarketEl?.getAttribute("data-open-side") as Side | null;
  const openIntentRaw = openMarketEl?.getAttribute("data-open-intent");
  const action = actionEl?.getAttribute("data-action") ?? null;
  const side = sideEl?.getAttribute("data-side") as Side | null;
  const tradeChoiceRaw =
    tradeChoiceEl?.getAttribute("data-trade-choice") ?? null;
  const tradeIntent = tradeIntentEl?.getAttribute(
    "data-trade-intent",
  ) as TradeIntent | null;
  const sizeMode = sizeModeEl?.getAttribute(
    "data-size-mode",
  ) as SizeMode | null;
  const tradeSizePreset = Number(
    tradeSizePresetEl?.getAttribute("data-trade-size-sats") ?? "",
  );
  const tradeSizeDelta = Number(
    tradeSizeDeltaEl?.getAttribute("data-trade-size-delta") ?? "",
  );
  const limitPriceDelta = Number(
    actionEl?.getAttribute("data-limit-price-delta") ?? "",
  );
  const contractsStepDelta = Number(
    actionEl?.getAttribute("data-contracts-step-delta") ?? "",
  );
  const orderType = orderTypeEl?.getAttribute(
    "data-order-type",
  ) as OrderType | null;
  const tab = tabEl?.getAttribute("data-tab") as ActionTab | null;

  // Close user menu on any click that isn't inside the menu
  if (
    state.userMenuOpen &&
    action !== "toggle-user-menu" &&
    action !== "user-settings" &&
    action !== "user-logout" &&
    action !== "set-currency" &&
    action !== "copy-nostr-npub"
  ) {
    // Check if click is inside the dropdown
    const inMenu = target
      .closest("[data-action='toggle-user-menu']")
      ?.parentElement?.contains(target);
    if (!inMenu) {
      state.userMenuOpen = false;
      render();
    }
  }

  // Close category dropdown on any click outside it
  if (
    state.createCategoryOpen &&
    action !== "toggle-category-dropdown" &&
    action !== "select-create-category"
  ) {
    const inDropdown = target.closest("#create-category-dropdown");
    if (!inDropdown) {
      state.createCategoryOpen = false;
      render();
    }
  }

  // Close settlement picker on any click outside it
  if (
    state.createSettlementPickerOpen &&
    action !== "toggle-settlement-picker" &&
    action !== "settlement-prev-month" &&
    action !== "settlement-next-month" &&
    action !== "pick-settlement-day" &&
    action !== "toggle-settlement-dropdown" &&
    action !== "pick-settlement-option"
  ) {
    const inPicker = target.closest("#settlement-picker");
    if (!inPicker) {
      state.createSettlementPickerOpen = false;
      render();
    }
  }

  if (category) {
    state.activeCategory = category;
    state.view = "home";
    state.chartHoverMarketId = null;
    state.chartHoverX = null;
    render();
    return;
  }

  if (openMarketId) {
    const openIntent: TradeIntent | undefined =
      openIntentRaw === "sell"
        ? "close"
        : openIntentRaw === "buy"
          ? "open"
          : openIntentRaw === "open" || openIntentRaw === "close"
            ? openIntentRaw
            : undefined;
    openMarket(openMarketId, {
      side: openSide ?? undefined,
      intent: openIntent,
    });
    return;
  }

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
          const status = await invoke<NostrBackupStatus>("check_nostr_backup");
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
    state.onboardingLoading = true;
    state.onboardingError = "";
    showOverlayLoader("Restoring wallet...");
    render();
    (async () => {
      try {
        await invoke("restore_wallet", {
          mnemonic: state.onboardingWalletMnemonic.trim(),
          password: state.onboardingWalletPassword,
        });
        updateOverlayMessage("Unlocking wallet...");
        await invoke("unlock_wallet", {
          password: state.onboardingWalletPassword,
        });
        updateOverlayMessage("Scanning blockchain...");
        await invoke("sync_wallet");
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
    state.onboardingLoading = true;
    state.onboardingError = "";
    showOverlayLoader("Fetching backup from relays...");
    render();
    (async () => {
      try {
        const mnemonic = await invoke<string>("restore_mnemonic_from_nostr");
        updateOverlayMessage("Restoring wallet...");
        await invoke("restore_wallet", {
          mnemonic: mnemonic.trim(),
          password: state.onboardingWalletPassword,
        });
        updateOverlayMessage("Unlocking wallet...");
        await invoke("unlock_wallet", {
          password: state.onboardingWalletPassword,
        });
        updateOverlayMessage("Scanning blockchain...");
        await invoke("sync_wallet");
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

  if (action === "load-demo-markets") {
    setMarkets(MOCK_MARKETS);
    state.marketsLoading = false;
    state.settingsOpen = false;
    state.view = "home";
    state.activeCategory = "Trending";
    render();
    showToast("Loaded 20 demo markets", "success");
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
        // Refresh backup status
        const status = await invoke<NostrBackupStatus>("check_nostr_backup");
        state.nostrBackupStatus = status;
        // Update relay backup indicators
        if (status.relay_results) {
          state.relays = state.relays.map((r) => ({
            ...r,
            has_backup:
              status.relay_results.find(
                (rr: RelayBackupResult) => rr.url === r.url,
              )?.has_backup ?? false,
          }));
        }
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
        const status = await invoke<NostrBackupStatus>("check_nostr_backup");
        state.nostrBackupStatus = status;
        if (status.relay_results) {
          state.relays = state.relays.map((r) => ({
            ...r,
            has_backup:
              status.relay_results.find(
                (rr: RelayBackupResult) => rr.url === r.url,
              )?.has_backup ?? false,
          }));
        }
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
    if (
      !confirm(
        "Delete your encrypted wallet backup from all relays? You can re-upload it later.",
      )
    )
      return;
    state.nostrBackupLoading = true;
    render();
    (async () => {
      try {
        await invoke("delete_nostr_backup");
        const status = await invoke<NostrBackupStatus>("check_nostr_backup");
        state.nostrBackupStatus = status;
        if (status.relay_results) {
          state.relays = state.relays.map((r) => ({
            ...r,
            has_backup:
              status.relay_results.find(
                (rr: RelayBackupResult) => rr.url === r.url,
              )?.has_backup ?? false,
          }));
        }
        showToast("Backup deletion request sent to relays", "success");
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
        showToast(`No backup found: ${String(e)}`, "error");
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
    if (!state.marketMakerMode) return;
    state.view = "create";
    render();
    return;
  }

  if (action === "open-wallet") {
    state.walletError = "";
    state.walletPassword = "";
    state.settingsOpen = false;
    state.view = "wallet";
    render();
    // If already unlocked with cached balance, just do a silent background sync
    if (state.walletStatus === "unlocked" && state.walletData) {
      void invoke("sync_wallet")
        .then(async () => {
          const [balance, txs, swaps] = await Promise.all([
            invoke<{ assets: Record<string, number> }>("get_wallet_balance"),
            invoke<WalletTransaction[]>("get_wallet_transactions"),
            invoke<PaymentSwap[]>("list_payment_swaps"),
          ]);
          if (state.walletData) {
            state.walletData.balance = balance.assets;
            state.walletData.transactions = txs;
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

  if (action === "create-wallet") {
    if (!state.walletPassword) {
      state.walletError = "Password is required.";
      render();
      return;
    }
    state.walletLoading = true;
    state.walletError = "";
    showOverlayLoader("Creating wallet...");
    render();
    (async () => {
      try {
        const mnemonic = await invoke<string>("create_wallet", {
          password: state.walletPassword,
        });
        state.walletMnemonic = mnemonic;
        state.walletPassword = "";
        await fetchWalletStatus();
        // Stay on not_created so mnemonic screen shows
        state.walletStatus = "not_created";
      } catch (e) {
        state.walletError = String(e);
      }
      state.walletLoading = false;
      hideOverlayLoader();
      render();
    })();
    return;
  }

  if (action === "dismiss-mnemonic") {
    state.walletMnemonic = "";
    state.walletStatus = "locked";
    state.walletPassword = "";
    render();
    return;
  }

  if (action === "toggle-restore") {
    state.walletShowRestore = !state.walletShowRestore;
    state.walletError = "";
    render();
    return;
  }

  if (action === "restore-wallet") {
    if (!state.walletRestoreMnemonic.trim() || !state.walletPassword) {
      state.walletError = "Recovery phrase and password are required.";
      render();
      return;
    }
    state.walletLoading = true;
    state.walletError = "";
    showOverlayLoader("Restoring wallet...");
    render();
    (async () => {
      try {
        await invoke("restore_wallet", {
          mnemonic: state.walletRestoreMnemonic.trim(),
          password: state.walletPassword,
        });
        state.walletRestoreMnemonic = "";
        state.walletPassword = "";
        updateOverlayMessage("Scanning blockchain...");
        await invoke("sync_wallet");
        await fetchWalletStatus();
        if (state.walletStatus === "unlocked") {
          const balance = await invoke<{ assets: Record<string, number> }>(
            "get_wallet_balance",
          );
          const txs = await invoke<WalletTransaction[]>(
            "get_wallet_transactions",
          );
          state.walletData = {
            ...createWalletData(),
            balance: balance.assets,
            transactions: txs,
          };
        }
        state.walletLoading = false;
        hideOverlayLoader();
        render();
        showToast("Wallet restored successfully", "success");
      } catch (e) {
        state.walletError = String(e);
        state.walletLoading = false;
        hideOverlayLoader();
        render();
      }
    })();
    return;
  }

  if (action === "unlock-wallet") {
    if (!state.walletPassword) {
      state.walletError = "Password is required.";
      render();
      return;
    }
    state.walletLoading = true;
    state.walletError = "";
    showOverlayLoader("Unlocking wallet...");
    render();
    (async () => {
      try {
        await invoke("unlock_wallet", { password: state.walletPassword });
        state.walletPassword = "";
        await fetchWalletStatus();
        // Load cached wallet data instantly (no Electrum sync)
        const [balance, txs, swaps] = await Promise.all([
          invoke<{ assets: Record<string, number> }>("get_wallet_balance"),
          invoke<WalletTransaction[]>("get_wallet_transactions"),
          invoke<PaymentSwap[]>("list_payment_swaps"),
        ]);
        state.walletData = {
          ...createWalletData(),
          balance: balance.assets,
          transactions: txs,
          swaps,
        };
        state.walletLoading = false;
        hideOverlayLoader();
        render();
        // Fetch own orders for transaction labeling
        fetchOwnOrders()
          .then((orders) => {
            state.ownOrders = orders;
            render();
          })
          .catch(() => {});
        // Background Electrum sync -- updates balances when done
        invoke("sync_wallet")
          .then(async () => {
            const [freshBalance, freshTxs] = await Promise.all([
              invoke<{ assets: Record<string, number> }>("get_wallet_balance"),
              invoke<WalletTransaction[]>("get_wallet_transactions"),
            ]);
            if (state.walletData) {
              state.walletData.balance = freshBalance.assets;
              state.walletData.transactions = freshTxs;
            }
            render();
          })
          .catch(() => {
            /* silent background sync failure */
          });
      } catch (e) {
        state.walletError = String(e);
        state.walletLoading = false;
        hideOverlayLoader();
        render();
      }
    })();
    return;
  }

  if (action === "lock-wallet") {
    (async () => {
      try {
        await invoke("lock_wallet");
        await fetchWalletStatus();
        state.walletData = null;
        state.walletPassword = "";
        state.walletModal = "none";
        resetReceiveState();
        resetSendState();
        render();
      } catch (e) {
        state.walletError = String(e);
        render();
      }
    })();
    return;
  }

  if (action === "wallet-delete-start") {
    state.walletDeletePrompt = true;
    state.walletDeleteConfirm = "";
    render();
    return;
  }

  if (action === "wallet-delete-cancel") {
    state.walletDeletePrompt = false;
    state.walletDeleteConfirm = "";
    render();
    return;
  }

  if (action === "wallet-delete-confirm") {
    if (state.walletDeleteConfirm.trim().toUpperCase() !== "DELETE") return;
    (async () => {
      try {
        await invoke("delete_wallet");
        await fetchWalletStatus();
        state.walletData = null;
        state.walletPassword = "";
        state.walletMnemonic = "";
        state.walletError = "";
        state.walletModal = "none";
        resetReceiveState();
        resetSendState();
        state.walletDeletePrompt = false;
        state.walletDeleteConfirm = "";
        state.settingsOpen = false;
        showToast("Wallet removed", "success");
      } catch (e) {
        showToast(`Failed to remove wallet: ${String(e)}`, "error");
      }
      render();
    })();
    return;
  }

  if (action === "forgot-password-delete") {
    (async () => {
      try {
        await invoke("delete_wallet");
        await fetchWalletStatus();
        state.walletData = null;
        state.walletPassword = "";
        state.walletMnemonic = "";
        state.walletError = "";
        state.walletModal = "none";
        resetReceiveState();
        resetSendState();
        showToast(
          "Wallet removed — restore from backup or recovery phrase",
          "info",
        );
      } catch (e) {
        showToast(`Failed to remove wallet: ${String(e)}`, "error");
      }
      render();
    })();
    return;
  }

  if (action === "toggle-balance-hidden") {
    state.walletBalanceHidden = !state.walletBalanceHidden;
    render();
    return;
  }

  if (action === "toggle-utxos-expanded") {
    state.walletUtxosExpanded = !state.walletUtxosExpanded;
    render();
    return;
  }

  if (action === "toggle-mini-wallet") {
    state.showMiniWallet = !state.showMiniWallet;
    render();
    return;
  }

  if (action === "toggle-lbtc-label") {
    state.showLbtcLabel = !state.showLbtcLabel;
    render();
    return;
  }

  if (action === "toggle-market-maker") {
    state.marketMakerMode = !state.marketMakerMode;
    if (!state.marketMakerMode && state.activeCategory === "My Markets") {
      state.activeCategory = "Trending";
    }
    render();
    return;
  }

  if (action === "set-wallet-unit") {
    const unit = actionEl?.getAttribute("data-unit") as "sats" | "btc" | null;
    if (unit) {
      state.walletUnit = unit;
      render();
    }
    return;
  }

  if (action === "sync-wallet") {
    void refreshWallet(render);
    return;
  }

  if (action === "open-explorer-tx") {
    const txid = actionEl?.getAttribute("data-txid");
    if (txid) {
      const base =
        state.walletNetwork === "testnet"
          ? "https://blockstream.info/liquidtestnet"
          : "https://blockstream.info/liquid";
      void openUrl(`${base}/tx/${txid}`);
    }
    return;
  }

  if (action === "open-nostr-event") {
    const marketId = actionEl?.getAttribute("data-market-id");
    const nevent = actionEl?.getAttribute("data-nevent");
    const market = marketId ? markets.find((m) => m.id === marketId) : null;
    if (market?.nostrEventJson) {
      state.nostrEventModal = true;
      state.nostrEventJson = market.nostrEventJson;
      state.nostrEventNevent = nevent ?? null;
      render();
    } else {
      showToast("Nostr event data not available", "error");
    }
    return;
  }

  if (action === "nostr-event-backdrop" && actionEl === target) {
    state.nostrEventModal = false;
    state.nostrEventJson = null;
    state.nostrEventNevent = null;
    render();
    return;
  }

  if (action === "close-nostr-event-modal") {
    state.nostrEventModal = false;
    state.nostrEventJson = null;
    state.nostrEventNevent = null;
    render();
    return;
  }

  if (action === "copy-nostr-event-json") {
    if (state.nostrEventJson) {
      void navigator.clipboard.writeText(state.nostrEventJson);
      showToast("Copied to clipboard");
    }
    return;
  }

  if (action === "open-receive") {
    state.walletModal = "receive";
    state.walletModalTab = "lightning";
    resetReceiveState();
    render();
    (async () => {
      try {
        const pairs = await invoke<BoltzChainSwapPairsInfo>(
          "get_chain_swap_pairs",
        );
        state.receiveBtcPairInfo = pairs.bitcoinToLiquid;
      } catch {
        /* ignore */
      }
      render();
    })();
    return;
  }

  if (action === "open-send") {
    state.walletModal = "send";
    state.walletModalTab = "lightning";
    resetSendState();
    render();
    (async () => {
      try {
        const pairs = await invoke<BoltzChainSwapPairsInfo>(
          "get_chain_swap_pairs",
        );
        state.sendBtcPairInfo = pairs.liquidToBitcoin;
      } catch {
        /* ignore */
      }
      render();
    })();
    return;
  }

  if (action === "close-modal") {
    state.walletModal = "none";
    resetReceiveState();
    resetSendState();
    render();
    return;
  }

  if (action === "modal-backdrop" && actionEl === target) {
    state.walletModal = "none";
    resetReceiveState();
    resetSendState();
    render();
    return;
  }

  if (action === "modal-tab") {
    const tab = actionEl?.getAttribute("data-tab-value") as
      | "lightning"
      | "liquid"
      | "bitcoin"
      | null;
    if (tab) {
      state.walletModalTab = tab;
      state.modalQr = "";
      render();
    }
    return;
  }

  if (action === "receive-preset") {
    const preset = actionEl?.getAttribute("data-preset") ?? "";
    if (preset) {
      state.receiveAmount = preset;
      render();
    }
    return;
  }

  if (action === "create-lightning-receive") {
    const amt = Math.floor(Number(state.receiveAmount) || 0);
    if (amt <= 0) {
      state.receiveError = "Enter a valid amount.";
      render();
      return;
    }
    state.receiveCreating = true;
    state.receiveError = "";
    render();
    (async () => {
      try {
        const swap = await invoke<BoltzLightningReceiveCreated>(
          "create_lightning_receive",
          { amountSat: amt },
        );
        state.receiveLightningSwap = swap;
        await generateQr(swap.invoice);
      } catch (e) {
        state.receiveError = String(e);
      }
      state.receiveCreating = false;
      render();
    })();
    return;
  }

  if (action === "generate-liquid-address") {
    (async () => {
      try {
        const addr = await invoke<{ address: string }>("get_wallet_address", {
          index: state.receiveLiquidAddressIndex,
        });
        state.receiveLiquidAddress = addr.address;
        await generateQr(addr.address);
        state.receiveLiquidAddressIndex += 1;
      } catch (e) {
        state.receiveError = String(e);
      }
      render();
    })();
    return;
  }

  if (action === "create-bitcoin-receive") {
    const amt = Math.floor(Number(state.receiveAmount) || 0);
    if (amt <= 0) {
      state.receiveError = "Enter a valid amount.";
      render();
      return;
    }
    state.receiveCreating = true;
    state.receiveError = "";
    render();
    (async () => {
      try {
        const swap = await invoke<BoltzChainSwapCreated>(
          "create_bitcoin_receive",
          { amountSat: amt },
        );
        state.receiveBitcoinSwap = swap;
        const addr = swap.lockupAddress;
        await generateQr(swap.bip21 || addr);
      } catch (e) {
        state.receiveError = String(e);
      }
      state.receiveCreating = false;
      render();
    })();
    return;
  }

  if (action === "pay-lightning-invoice") {
    const invoice = state.sendInvoice.trim();
    if (!invoice) {
      state.sendError = "Paste a BOLT11 invoice.";
      render();
      return;
    }
    state.sendCreating = true;
    state.sendError = "";
    render();
    (async () => {
      try {
        const swap = await invoke<BoltzSubmarineSwapCreated>(
          "pay_lightning_invoice",
          { invoice },
        );
        state.sentLightningSwap = swap;
      } catch (e) {
        state.sendError = String(e);
      }
      state.sendCreating = false;
      render();
    })();
    return;
  }

  if (action === "send-liquid") {
    const address = state.sendLiquidAddress.trim();
    const amountSat = Math.floor(Number(state.sendLiquidAmount) || 0);
    if (!address || amountSat <= 0) {
      state.sendError = "Enter address and amount.";
      render();
      return;
    }
    state.sendCreating = true;
    state.sendError = "";
    render();
    (async () => {
      try {
        const result = await invoke<{ txid: string; feeSat: number }>(
          "send_lbtc",
          {
            address,
            amountSat,
            feeRate: null,
          },
        );
        state.sentLiquidResult = { txid: result.txid, feeSat: result.feeSat };
      } catch (e) {
        state.sendError = String(e);
      }
      state.sendCreating = false;
      render();
    })();
    return;
  }

  if (action === "create-bitcoin-send") {
    const amt = Math.floor(Number(state.sendBtcAmount) || 0);
    if (amt <= 0) {
      state.sendError = "Enter a valid amount.";
      render();
      return;
    }
    state.sendCreating = true;
    state.sendError = "";
    render();
    (async () => {
      try {
        const swap = await invoke<BoltzChainSwapCreated>(
          "create_bitcoin_send",
          { amountSat: amt },
        );
        state.sentBitcoinSwap = swap;
        const addr = swap.claimLockupAddress;
        await generateQr(swap.bip21 || addr);
      } catch (e) {
        state.sendError = String(e);
      }
      state.sendCreating = false;
      render();
    })();
    return;
  }

  if (action === "copy-modal-value") {
    const val = actionEl?.getAttribute("data-copy-value") ?? "";
    if (val) void navigator.clipboard.writeText(val);
    return;
  }

  if (action === "refresh-swap") {
    const swapId = actionEl?.getAttribute("data-swap-id") ?? "";
    if (!swapId) return;
    (async () => {
      try {
        await invoke("refresh_payment_swap_status", { swapId });
        const swaps = await invoke<PaymentSwap[]>("list_payment_swaps");
        if (state.walletData) state.walletData.swaps = swaps;
      } catch (e) {
        state.walletError = String(e);
      }
      render();
    })();
    return;
  }

  if (action === "copy-mnemonic") {
    void navigator.clipboard.writeText(state.walletMnemonic);
    return;
  }

  if (action === "show-backup") {
    if (state.walletData) {
      state.walletData.showBackup = true;
      state.walletData.backupWords = [];
      state.walletData.backupPassword = "";
    }
    state.walletError = "";
    render();
    return;
  }

  if (action === "hide-backup") {
    if (state.walletData) {
      state.walletData.showBackup = false;
      state.walletData.backupWords = [];
      state.walletData.backupPassword = "";
    }
    render();
    return;
  }

  if (action === "export-backup") {
    if (!state.walletData?.backupPassword) {
      state.walletError = "Password is required to export recovery phrase.";
      render();
      return;
    }
    state.walletLoading = true;
    state.walletError = "";
    showOverlayLoader("Decrypting backup...");
    render();
    (async () => {
      try {
        const password = state.walletData?.backupPassword ?? "";
        const count = await invoke<number>("get_mnemonic_word_count", {
          password,
        });
        const words: string[] = [];
        for (let i = 0; i < count; i++) {
          words.push(
            await invoke<string>("get_mnemonic_word", { password, index: i }),
          );
        }
        if (state.walletData) {
          state.walletData.backupWords = words;
          state.walletData.backedUp = true;
          state.walletData.backupPassword = "";
        }
      } catch (e) {
        state.walletError = String(e);
      }
      state.walletLoading = false;
      hideOverlayLoader();
      render();
    })();
    return;
  }

  if (action === "copy-backup-mnemonic") {
    void navigator.clipboard.writeText(
      (state.walletData?.backupWords ?? []).join(" "),
    );
    return;
  }

  if (action === "toggle-category-dropdown") {
    state.createCategoryOpen = !state.createCategoryOpen;
    render();
    return;
  }

  if (action === "select-create-category") {
    const value = actionEl?.dataset.value;
    if (value) {
      state.createCategory = value as MarketCategory;
      state.createCategoryOpen = false;
      render();
    }
    return;
  }

  if (action === "toggle-settlement-picker") {
    state.createSettlementPickerOpen = !state.createSettlementPickerOpen;
    // Sync view month to currently selected date when opening
    if (state.createSettlementPickerOpen && state.createSettlementInput) {
      const d = new Date(state.createSettlementInput);
      state.createSettlementViewYear = d.getFullYear();
      state.createSettlementViewMonth = d.getMonth();
    }
    render();
    return;
  }

  if (action === "settlement-prev-month") {
    state.createSettlementViewMonth--;
    if (state.createSettlementViewMonth < 0) {
      state.createSettlementViewMonth = 11;
      state.createSettlementViewYear--;
    }
    render();
    return;
  }

  if (action === "settlement-next-month") {
    state.createSettlementViewMonth++;
    if (state.createSettlementViewMonth > 11) {
      state.createSettlementViewMonth = 0;
      state.createSettlementViewYear++;
    }
    render();
    return;
  }

  if (action === "pick-settlement-day") {
    const day = Number(actionEl?.dataset.day);
    if (!day) return;
    let hours = 12;
    let minutes = 0;
    if (state.createSettlementInput) {
      const prev = new Date(state.createSettlementInput);
      hours = prev.getHours();
      minutes = prev.getMinutes();
    }
    const y = state.createSettlementViewYear;
    const m = String(state.createSettlementViewMonth + 1).padStart(2, "0");
    const d = String(day).padStart(2, "0");
    const hh = String(hours).padStart(2, "0");
    const mm = String(minutes).padStart(2, "0");
    state.createSettlementInput = `${y}-${m}-${d}T${hh}:${mm}`;
    render();
    return;
  }

  if (action === "toggle-settlement-dropdown") {
    const name = actionEl?.dataset.dropdown ?? "";
    state.createSettlementPickerDropdown =
      state.createSettlementPickerDropdown === name ? "" : name;
    render();
    return;
  }

  if (action === "pick-settlement-option") {
    const dropdown = actionEl?.dataset.dropdown ?? "";
    const value = actionEl?.dataset.value ?? "";
    state.createSettlementPickerDropdown = "";

    if (dropdown === "month") {
      state.createSettlementViewMonth = Number(value);
    } else if (dropdown === "year") {
      state.createSettlementViewYear = Number(value);
    } else if (
      (dropdown === "hour" || dropdown === "minute" || dropdown === "ampm") &&
      state.createSettlementInput
    ) {
      const prev = new Date(state.createSettlementInput);
      let h = prev.getHours();
      let min = prev.getMinutes();
      const wasPM = h >= 12;
      let h12 = h % 12 || 12;

      if (dropdown === "hour") h12 = Number(value);
      if (dropdown === "minute") min = Number(value);
      let pm = wasPM;
      if (dropdown === "ampm") pm = value === "PM";

      h = (h12 % 12) + (pm ? 12 : 0);

      const y = prev.getFullYear();
      const mo = String(prev.getMonth() + 1).padStart(2, "0");
      const d = String(prev.getDate()).padStart(2, "0");
      const hh = String(h).padStart(2, "0");
      const mm = String(min).padStart(2, "0");
      state.createSettlementInput = `${y}-${mo}-${d}T${hh}:${mm}`;
    }
    render();
    return;
  }

  if (action === "cancel-create-market") {
    state.createCategoryOpen = false;
    state.createSettlementPickerOpen = false;
    state.createSettlementPickerDropdown = "";
    state.view = "home";
    render();
    return;
  }

  if (action === "oracle-attest-yes" || action === "oracle-attest-no") {
    const market = getSelectedMarket();
    const outcomeYes = action === "oracle-attest-yes";
    const outcomeLabel = outcomeYes ? "YES" : "NO";
    const confirmed = window.confirm(
      `Resolve "${market.question}" as ${outcomeLabel}?\n\nThis publishes a Schnorr signature to Nostr that permanently attests the outcome. This cannot be undone.`,
    );
    if (!confirmed) return;

    (async () => {
      try {
        const result = await invoke<AttestationResult>("oracle_attest", {
          marketIdHex: market.marketId,
          outcomeYes,
        });
        // Save attestation for on-chain execution
        state.lastAttestationSig = result.signature_hex;
        state.lastAttestationOutcome = outcomeYes;
        state.lastAttestationMarketId = market.marketId;
        market.resolveTx = {
          txid: result.nostr_event_id,
          outcome: outcomeYes ? "yes" : "no",
          sigVerified: true,
          height: market.currentHeight,
          signatureHash: `${result.signature_hex.slice(0, 16)}...`,
        };
        showToast(
          `Attestation published to Nostr! Now execute on-chain to finalize.`,
          "success",
        );
        render();
      } catch (error) {
        window.alert(`Failed to attest: ${error}`);
      }
    })();
    return;
  }

  if (action === "execute-resolution") {
    const market = getSelectedMarket();
    if (!state.lastAttestationSig || state.lastAttestationOutcome === null) {
      showToast("No attestation available to execute", "error");
      return;
    }
    const anchor = requireMarketAnchor(market, "resolve market");
    if (!anchor) return;
    const outcomeYes = state.lastAttestationOutcome;
    const confirmed = window.confirm(
      `Execute on-chain resolution for "${market.question}"?\n\nOutcome: ${outcomeYes ? "YES" : "NO"}\nThis submits a Liquid transaction that transitions the covenant state.`,
    );
    if (!confirmed) return;

    state.resolutionExecuting = true;
    render();
    (async () => {
      try {
        const result = await invoke<{
          txid: string;
          previous_state: number;
          new_state: number;
          outcome_yes: boolean;
        }>("resolve_market", {
          contractParamsJson: marketToContractParamsJson(market),
          anchor,
          outcomeYes,
          oracleSignatureHex: state.lastAttestationSig,
        });
        market.state = result.outcome_yes ? 2 : 3;
        state.lastAttestationSig = null;
        state.lastAttestationOutcome = null;
        state.lastAttestationMarketId = null;
        showToast(
          `Resolution executed! txid: ${result.txid.slice(0, 16)}... State: ${result.new_state}`,
          "success",
        );
        await refreshWallet(render);
      } catch (error) {
        showToast(`Resolution failed: ${error}`, "error");
      } finally {
        state.resolutionExecuting = false;
        render();
      }
    })();
    return;
  }

  if (action === "refresh-market-state") {
    const market = getSelectedMarket();
    const anchor = requireMarketAnchor(market, "query market state");
    if (!anchor) return;
    showToast("Querying on-chain market state...", "info");
    (async () => {
      try {
        const result = await invoke<{ state: number }>("get_market_state", {
          contractParamsJson: marketToContractParamsJson(market),
          anchor,
        });
        market.state = result.state as CovenantState;
        showToast(`Market state: ${stateLabel(market.state)}`, "success");
        render();
      } catch (error) {
        showToast(`State query failed: ${error}`, "error");
      }
    })();
    return;
  }

  if (action === "toggle-advanced-details") {
    state.showAdvancedDetails = !state.showAdvancedDetails;
    render();
    return;
  }

  if (action === "toggle-advanced-actions") {
    state.showAdvancedActions = !state.showAdvancedActions;
    if (state.showAdvancedActions && state.actionTab === "trade") {
      state.actionTab = "issue";
    }
    render();
    return;
  }

  if (action === "toggle-orderbook") {
    state.showOrderbook = !state.showOrderbook;
    render();
    return;
  }

  if (action === "toggle-fee-details") {
    state.showFeeDetails = !state.showFeeDetails;
    render();
    return;
  }

  if (action === "toggle-pool-create") {
    state.poolCreateOpen = !state.poolCreateOpen;
    render();
    return;
  }

  if (action === "create-pool") {
    showToast("Pool creation coming soon", "info");
    return;
  }

  if (action === "use-cashout") {
    const market = getSelectedMarket();
    const positions = getPositionContracts(market);
    const closeSide: Side = positions.yes >= positions.no ? "yes" : "no";
    const available = closeSide === "yes" ? positions.yes : positions.no;
    state.tradeIntent = "close";
    state.sizeMode = "contracts";
    state.selectedSide = closeSide;
    state.tradeContracts = Math.max(0.01, Math.min(available, available / 2));
    state.tradeContractsDraft = state.tradeContracts.toFixed(2);
    setLimitPriceSats(getBasePriceSats(market, closeSide));
    clearTradeQuoteSnapshot();
    render();
    return;
  }

  if (action === "sell-max") {
    const market = getSelectedMarket();
    const positions = getPositionContracts(market);
    const available =
      state.selectedSide === "yes" ? positions.yes : positions.no;
    state.tradeIntent = "close";
    state.sizeMode = "contracts";
    state.tradeContracts = Math.max(0.01, available);
    state.tradeContractsDraft = state.tradeContracts.toFixed(2);
    clearTradeQuoteSnapshot();
    render();
    return;
  }

  if (action === "sell-25" || action === "sell-50") {
    const market = getSelectedMarket();
    const positions = getPositionContracts(market);
    const available =
      state.selectedSide === "yes" ? positions.yes : positions.no;
    const ratio = action === "sell-25" ? 0.25 : 0.5;
    state.tradeIntent = "close";
    state.sizeMode = "contracts";
    state.tradeContracts = Math.max(0.01, available * ratio);
    state.tradeContractsDraft = state.tradeContracts.toFixed(2);
    clearTradeQuoteSnapshot();
    render();
    return;
  }

  if (action === "trending-prev") {
    const total = getTrendingMarkets().length;
    state.trendingIndex = (state.trendingIndex - 1 + total) % total;
    render();
    return;
  }

  if (action === "trending-next") {
    const total = getTrendingMarkets().length;
    state.trendingIndex = (state.trendingIndex + 1) % total;
    render();
    return;
  }

  if (side) {
    state.selectedSide = side;
    const market = getSelectedMarket();
    setLimitPriceSats(getBasePriceSats(market, side));
    enforceSizeModeForIntent();
    clearTradeQuoteSnapshot();
    render();
    return;
  }

  if (tradeChoiceRaw) {
    const [intentRaw, sideRaw] = tradeChoiceRaw.split(":");
    const intent = intentRaw as TradeIntent;
    const pickedSide = sideRaw as Side;
    if (
      (intent === "open" || intent === "close") &&
      (pickedSide === "yes" || pickedSide === "no")
    ) {
      state.tradeIntent = intent;
      state.selectedSide = pickedSide;
      const market = getSelectedMarket();
      const positions = getPositionContracts(market);
      const available = pickedSide === "yes" ? positions.yes : positions.no;
      setLimitPriceSats(getBasePriceSats(market, pickedSide));
      if (intent === "close") {
        state.sizeMode = "contracts";
        state.tradeContracts = Math.max(
          0.01,
          Math.min(Math.max(0.01, available), state.tradeContracts),
        );
        state.tradeContractsDraft = state.tradeContracts.toFixed(2);
      }
      enforceSizeModeForIntent();
      clearTradeQuoteSnapshot();
      render();
      return;
    }
  }

  if (tradeIntent) {
    state.tradeIntent = tradeIntent;
    const market = getSelectedMarket();
    const positions = getPositionContracts(market);
    const available =
      state.selectedSide === "yes" ? positions.yes : positions.no;
    if (tradeIntent === "close") {
      state.sizeMode = "contracts";
      state.tradeContracts = Math.max(
        0.01,
        Math.min(Math.max(0.01, available), state.tradeContracts),
      );
      state.tradeContractsDraft = state.tradeContracts.toFixed(2);
    }
    enforceSizeModeForIntent();
    clearTradeQuoteSnapshot();
    render();
    return;
  }

  if (sizeMode) {
    if (state.tradeIntent === "open" && sizeMode === "contracts") {
      showToast(
        "Buy orders are exact-input collateral. Use sats size for buys.",
        "info",
      );
      state.sizeMode = "sats";
    } else if (state.tradeIntent === "close" && sizeMode === "sats") {
      showToast(
        "Sell orders are exact-input token amount. Use contracts size for sells.",
        "info",
      );
      state.sizeMode = "contracts";
    } else {
      state.sizeMode = sizeMode;
    }
    clearTradeQuoteSnapshot();
    render();
    return;
  }

  if (Number.isFinite(tradeSizePreset) && tradeSizePreset > 0) {
    if (state.tradeIntent !== "open") {
      showToast("Sats presets are only available for buys.", "info");
      return;
    }
    state.sizeMode = "sats";
    state.tradeSizeSats = Math.floor(tradeSizePreset);
    state.tradeSizeSatsDraft = formatSatsInput(state.tradeSizeSats);
    clearTradeQuoteSnapshot();
    render();
    return;
  }

  if (Number.isFinite(tradeSizeDelta) && tradeSizeDelta !== 0) {
    if (state.tradeIntent !== "open") {
      showToast("Sats step controls are only available for buys.", "info");
      return;
    }
    state.sizeMode = "sats";
    const current = Math.max(
      1,
      Math.floor(Number(state.tradeSizeSatsDraft.replace(/,/g, "")) || 1),
    );
    const next = Math.max(1, current + Math.floor(tradeSizeDelta));
    state.tradeSizeSats = next;
    state.tradeSizeSatsDraft = formatSatsInput(next);
    clearTradeQuoteSnapshot();
    render();
    return;
  }

  if (
    action === "step-limit-price" &&
    Number.isFinite(limitPriceDelta) &&
    limitPriceDelta !== 0
  ) {
    const currentSats = clampContractPriceSats(
      state.limitPriceDraft.length > 0
        ? Number(state.limitPriceDraft)
        : state.limitPrice * SATS_PER_FULL_CONTRACT,
    );
    setLimitPriceSats(currentSats + Math.sign(limitPriceDelta));
    clearTradeQuoteSnapshot();
    render();
    return;
  }

  if (
    action === "step-trade-contracts" &&
    Number.isFinite(contractsStepDelta) &&
    contractsStepDelta !== 0
  ) {
    const market = getSelectedMarket();
    const current = Number(state.tradeContractsDraft);
    const baseValue = Number.isFinite(current)
      ? current
      : Math.max(0.01, state.tradeContracts);
    const nextValue = Math.max(
      0.01,
      baseValue + Math.sign(contractsStepDelta) * 0.01,
    );
    state.tradeContractsDraft = nextValue.toFixed(2);
    commitTradeContractsDraft(market);
    clearTradeQuoteSnapshot();
    render();
    return;
  }

  if (orderType) {
    state.orderType = orderType as OrderType;
    clearTradeQuoteSnapshot();
    render();
    return;
  }

  if (tab) {
    const market = getSelectedMarket();
    if (ticketActionAllowed(market, tab)) {
      state.actionTab = tab;
      render();
    }
    return;
  }

  if (action === "cancel-limit-order") {
    const orderId = actionEl?.dataset.orderId;

    // Search all markets for the order (cancel can be triggered from wallet or detail view)
    let order: import("../types.ts").DiscoveredOrder | undefined;
    let orderMarketId: string | undefined;
    for (const m of markets) {
      const found = m.limitOrders.find((o) => o.id === orderId);
      if (found) {
        order = found;
        orderMarketId = m.marketId;
        break;
      }
    }

    if (!order || !orderMarketId) {
      showToast("Order not found — try refreshing the page", "error");
      return;
    }

    state.cancellingOrderId = orderId ?? null;
    render();

    (async () => {
      try {
        const result = await cancelLimitOrder(order);
        showToast(
          `Order cancelled! Refunded ${result.refunded_amount} sats. txid: ${result.txid.slice(0, 16)}...`,
          "success",
        );
        await refreshWallet(render);
        const orders = await fetchOrders(orderMarketId);
        mergeOrdersIntoMarket(orderMarketId, orders);
        fetchOwnOrders()
          .then((own) => {
            state.ownOrders = own;
            render();
          })
          .catch(() => {});
      } catch (error) {
        showToast(`Cancel failed: ${error}`, "error");
      }
      state.cancellingOrderId = null;
      render();
    })();
    return;
  }

  if (
    action === "submit-trade" ||
    action === "submit-issue" ||
    action === "submit-redeem" ||
    action === "submit-cancel" ||
    action === "submit-create-market"
  ) {
    if (action === "submit-create-market") {
      const question = state.createQuestion.trim();
      const description = state.createDescription.trim();
      const source = state.createResolutionSource.trim();
      if (
        !question ||
        !description ||
        !source ||
        !state.createSettlementInput
      ) {
        window.alert(
          "Complete question, settlement rule, source, and settlement deadline before creating.",
        );
        return;
      }
      const deadlineUnix = Math.floor(
        new Date(state.createSettlementInput).getTime() / 1000,
      );

      state.marketCreating = true;
      render();
      (async () => {
        try {
          const result = await invoke<DiscoveredMarket>(
            "create_contract_onchain",
            {
              request: {
                question,
                description,
                category: state.createCategory,
                resolution_source: source,
                settlement_deadline_unix: deadlineUnix,
                collateral_per_token: 5000,
              },
            },
          );
          markets.push(discoveredToMarket(result));
          state.view = "home";
          state.createQuestion = "";
          state.createDescription = "";
          state.createResolutionSource = "";
          state.createSettlementInput = defaultSettlementInput();
          showToast(
            `Market created! txid: ${result.anchor?.creation_txid ?? "unknown"}`,
            "success",
          );
        } catch (error) {
          showToast(`Failed to create market: ${error}`, "error");
        } finally {
          state.marketCreating = false;
          render();
        }
      })();
      return;
    }

    const market = getSelectedMarket();
    if (action === "submit-trade") {
      if (state.orderType === "limit") {
        const side = state.selectedSide;
        const direction = currentTradeDirection();
        const priceSats = Math.round(state.limitPrice * SATS_PER_FULL_CONTRACT);
        const amount = currentExactInput();

        if (amount <= 0) {
          showToast("Enter an amount greater than zero", "error");
          return;
        }

        state.tradeExecuteLoading = true;
        state.tradeError = null;
        render();

        createLimitOrder(market, side, direction, priceSats, amount)
          .then(async (result) => {
            showToast(
              `Limit order placed! txid: ${result.txid.slice(0, 16)}...`,
              "success",
            );
            await refreshWallet(render);
            const orders = await fetchOrders(market.marketId);
            mergeOrdersIntoMarket(market.marketId, orders);
            fetchOwnOrders()
              .then((own) => {
                state.ownOrders = own;
                render();
              })
              .catch(() => {});
          })
          .catch((error) => {
            const msg = String(error);
            if (msg.includes("excluding") && msg.includes("fee")) {
              state.tradeError =
                "Limit orders need two L-BTC UTXOs (one for the order, one for the fee). Send yourself a small amount first to split your balance.";
            } else {
              state.tradeError = msg;
            }
            showToast(state.tradeError, "error");
          })
          .finally(() => {
            state.tradeExecuteLoading = false;
            render();
          });
        return;
      }

      const side = state.selectedSide;
      const direction = currentTradeDirection();
      const exactInput = currentExactInput();
      state.tradeQuoteLoading = true;
      state.tradeExecuteLoading = false;
      state.tradeError = null;
      render();
      (async () => {
        try {
          const quote = await quoteTrade(market, side, direction, exactInput);
          state.tradeQuoteSnapshot = {
            marketId: market.id,
            side,
            direction,
            exactInput,
            quote,
          };
          state.tradeQuoteLoading = false;
          render();

          const inputUnit = direction === "buy" ? "sats" : "contracts";
          const outputUnit = direction === "buy" ? "contracts" : "sats";
          const legsSummary = quote.legs
            .map((leg, idx) => {
              if (leg.source.kind === "lmsr_pool") {
                return `${idx + 1}. LMSR ${leg.source.pool_id.slice(0, 12)}... in ${formatTradeAmount(leg.input_amount, inputUnit)} out ${formatTradeAmount(leg.output_amount, outputUnit)}`;
              }
              return `${idx + 1}. Maker ${leg.source.order_id.slice(0, 12)}... in ${formatTradeAmount(leg.input_amount, inputUnit)} out ${formatTradeAmount(leg.output_amount, outputUnit)}`;
            })
            .join("\n");
          const confirmed = window.confirm(
            `${direction === "buy" ? "Execute buy" : "Execute sell"} ${side.toUpperCase()} on "${market.question.slice(0, 50)}"?\n\nInput: ${formatTradeAmount(quote.total_input, inputUnit)}\nOutput: ${formatTradeAmount(quote.total_output, outputUnit)}\nEffective price: ${quote.effective_price.toFixed(6)}\nRoute legs:\n${legsSummary || "No route legs"}\n\nProceed?`,
          );
          if (!confirmed) {
            return;
          }

          state.tradeExecuteLoading = true;
          render();
          const result = await executeTrade(
            market,
            side,
            direction,
            exactInput,
            500,
            {
              total_input: quote.total_input,
              total_output: quote.total_output,
              legs: quote.legs,
            },
          );
          showToast(
            `Trade executed! txid: ${result.txid.slice(0, 16)}...`,
            "success",
          );
          await refreshWallet(render);
        } catch (error) {
          state.tradeError = String(error);
          showToast(`Trade failed: ${error}`, "error");
        } finally {
          state.tradeQuoteLoading = false;
          state.tradeExecuteLoading = false;
          render();
        }
      })();
      return;
    }

    if (action === "submit-issue") {
      const pairs = Math.max(1, Math.floor(state.pairsInput));
      if (!market.anchor) {
        showToast(
          "Market has no canonical anchor — cannot issue tokens",
          "error",
        );
        return;
      }
      showToast(
        `Issuing ${pairs} pair(s) for ${market.question.slice(0, 40)}...`,
        "info",
      );
      (async () => {
        try {
          const result = await issueTokens(market, pairs);
          showToast(
            `Tokens issued! txid: ${result.txid.slice(0, 16)}...`,
            "success",
          );
        } catch (error) {
          showToast(`Issuance failed: ${error}`, "error");
        }
      })();
      return;
    }

    if (action === "submit-cancel") {
      const pairs = Math.max(1, Math.floor(state.pairsInput));
      const anchor = requireMarketAnchor(market, "cancel tokens");
      if (!anchor) return;
      showToast(
        `Cancelling ${pairs} pair(s) for ${market.question.slice(0, 40)}...`,
        "info",
      );
      (async () => {
        try {
          const result = await invoke<{
            txid: string;
            previous_state: number;
            new_state: number;
            pairs_burned: number;
            is_full_cancellation: boolean;
          }>("cancel_tokens", {
            contractParamsJson: marketToContractParamsJson(market),
            anchor,
            pairs,
          });
          showToast(
            `Tokens cancelled! txid: ${result.txid.slice(0, 16)}... (${result.is_full_cancellation ? "full" : "partial"})`,
            "success",
          );
          await refreshWallet(render);
        } catch (error) {
          showToast(`Cancellation failed: ${error}`, "error");
        }
      })();
      return;
    }

    if (action === "submit-redeem") {
      const tokens = Math.max(1, Math.floor(state.tokensInput));
      const paths = getPathAvailability(market);

      if (paths.redeem) {
        const anchor = requireMarketAnchor(market, "redeem tokens");
        if (!anchor) return;
        showToast(`Redeeming ${tokens} winning token(s)...`, "info");
        (async () => {
          try {
            const result = await invoke<{
              txid: string;
              previous_state: number;
              tokens_redeemed: number;
              payout_sats: number;
            }>("redeem_tokens", {
              contractParamsJson: marketToContractParamsJson(market),
              anchor,
              tokens,
            });
            showToast(
              `Redeemed! txid: ${result.txid.slice(0, 16)}... payout: ${formatSats(result.payout_sats)}`,
              "success",
            );
            await refreshWallet(render);
          } catch (error) {
            showToast(`Redemption failed: ${error}`, "error");
          }
        })();
      } else if (paths.expiryRedeem) {
        // For expiry redemption, determine which token side the user holds
        const yesBalance =
          state.walletData?.balance?.[reverseHex(market.yesAssetId)] ?? 0;
        // Use whichever side the user holds (prefer YES if both)
        const tokenAssetHex =
          yesBalance > 0 ? market.yesAssetId : market.noAssetId;
        const anchor = requireMarketAnchor(market, "redeem expired tokens");
        if (!anchor) return;

        showToast(`Redeeming ${tokens} expired token(s)...`, "info");
        (async () => {
          try {
            const result = await invoke<{
              txid: string;
              previous_state: number;
              tokens_redeemed: number;
              payout_sats: number;
            }>("redeem_expired", {
              contractParamsJson: marketToContractParamsJson(market),
              anchor,
              tokenAssetHex: tokenAssetHex,
              tokens,
            });
            showToast(
              `Expired tokens redeemed! txid: ${result.txid.slice(0, 16)}... payout: ${formatSats(result.payout_sats)}`,
              "success",
            );
            await refreshWallet(render);
          } catch (error) {
            showToast(`Expiry redemption failed: ${error}`, "error");
          }
        })();
      } else {
        showToast("No redemption path available for this market", "error");
      }
      return;
    }
  }
}
