import "./style.css";
import { invoke } from "@tauri-apps/api/core";

import { app, state } from "./state.ts";
import type {
  Side,
  TradeIntent,
  IdentityResponse,
  NostrBackupStatus,
  RelayBackupResult,
  NostrProfile,
} from "./types.ts";

// Services
import { loadMarkets } from "./services/markets.ts";
import {
  fetchWalletStatus,
  refreshWallet,
  syncCurrentHeightFromLwk,
} from "./services/wallet.ts";

// Utils
import {
  getMarketById,
  getPositionContracts,
  getBasePriceSats,
  setLimitPriceSats,
} from "./utils/market.ts";
import { formatSatsInput, formatEstTime } from "./utils/format.ts";

// Components
import { renderTopShell } from "./components/shell.ts";
import { renderHome } from "./components/home.ts";
import { renderDetail } from "./components/detail.ts";
import { renderCreateMarket } from "./components/create.ts";
import { renderWallet } from "./components/wallet.ts";
import { renderNostrEventModal } from "./components/wallet-modals.ts";
import { renderOnboarding } from "./components/onboarding.ts";

// Handlers
import { handleClick } from "./handlers/click.ts";
import { handleInput } from "./handlers/input.ts";
import { handleKeydown } from "./handlers/keydown.ts";
import { handleFocusout } from "./handlers/focusout.ts";

// ── Core render ──────────────────────────────────────────────────────

let chartAspectSyncRaf: number | null = null;
const CHART_ASPECT_MIN = 1.2;
const CHART_ASPECT_MAX = 8;
const CHART_ASPECT_EPSILON = 0.005;

function syncChartAspectFromLayout(): void {
  const probes = Array.from(
    document.querySelectorAll<HTMLElement>("[data-chart-hover='1']"),
  );
  if (probes.length === 0) return;

  const homeRatios: number[] = [];
  const detailRatios: number[] = [];

  probes.forEach((probe) => {
    const rect = probe.getBoundingClientRect();
    if (rect.width < 16 || rect.height < 16) return;
    const ratio = rect.width / rect.height;
    if (!Number.isFinite(ratio) || ratio <= 0) return;
    if (probe.dataset.chartMode === "home") {
      homeRatios.push(ratio);
    } else {
      detailRatios.push(ratio);
    }
  });

  const average = (values: number[]): number =>
    values.reduce((sum, value) => sum + value, 0) / values.length;

  let changed = false;
  if (homeRatios.length > 0) {
    const next = Math.max(
      CHART_ASPECT_MIN,
      Math.min(CHART_ASPECT_MAX, average(homeRatios)),
    );
    if (Math.abs(next - state.chartAspectHome) > CHART_ASPECT_EPSILON) {
      state.chartAspectHome = next;
      changed = true;
    }
  }
  if (detailRatios.length > 0) {
    const next = Math.max(
      CHART_ASPECT_MIN,
      Math.min(CHART_ASPECT_MAX, average(detailRatios)),
    );
    if (Math.abs(next - state.chartAspectDetail) > CHART_ASPECT_EPSILON) {
      state.chartAspectDetail = next;
      changed = true;
    }
  }

  if (changed) {
    render();
  }
}

function scheduleChartAspectSync(): void {
  if (chartAspectSyncRaf !== null) return;
  chartAspectSyncRaf = requestAnimationFrame(() => {
    chartAspectSyncRaf = null;
    syncChartAspectFromLayout();
  });
}

function render(): void {
  if (state.onboardingStep !== null) {
    app.innerHTML = `<div class="min-h-screen text-slate-100 flex items-center justify-center">${renderOnboarding()}</div>`;
    scheduleChartAspectSync();
    return;
  }
  const html = `
    <div class="min-h-screen text-slate-100">
      ${renderTopShell()}
      <main>${state.view === "wallet" ? renderWallet() : state.view === "home" ? renderHome() : state.view === "detail" ? renderDetail() : renderCreateMarket()}</main>
    </div>
    ${renderNostrEventModal()}
  `;
  app.innerHTML = html;
  scheduleChartAspectSync();
}

function updateEstClockLabels(): void {
  const labels = document.querySelectorAll<HTMLElement>("[data-est-label]");
  if (!labels.length) return;
  labels.forEach((label) => {
    const offsetHours = Number(label.dataset.offsetHours ?? "0");
    const timestamp = Date.now() - offsetHours * 60 * 60 * 1000;
    label.textContent = formatEstTime(new Date(timestamp));
  });
}

function openMarket(
  marketId: string,
  options?: { side?: string; intent?: string },
): void {
  const market = getMarketById(marketId);
  const nextSide = (options?.side ?? "yes") as Side;
  const nextIntent = (options?.intent ?? "open") as TradeIntent;
  const positions = getPositionContracts(market);
  const selectedPosition = nextSide === "yes" ? positions.yes : positions.no;

  state.selectedMarketId = market.id;
  state.view = "detail";
  state.selectedSide = nextSide;
  state.orderType = "market";
  state.actionTab = "trade";
  state.tradeIntent = nextIntent;
  state.sizeMode = nextIntent === "close" ? "contracts" : "sats";
  state.showAdvancedDetails = false;
  state.showAdvancedActions = false;
  state.showOrderbook = false;
  state.showFeeDetails = false;
  state.tradeSizeSats = 10000;
  state.tradeSizeSatsDraft = formatSatsInput(10000);
  state.tradeContracts =
    nextIntent === "close"
      ? Math.max(0.01, Math.min(selectedPosition, selectedPosition / 2))
      : 10;
  state.tradeContractsDraft = state.tradeContracts.toFixed(2);
  state.chartHoverMarketId = null;
  state.chartHoverX = null;
  setLimitPriceSats(getBasePriceSats(market, nextSide));
  render();
}

async function finishOnboarding(): Promise<void> {
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
    invoke<string[]>("fetch_nip65_relay_list")
      .then((relays) => {
        state.relays = relays.map((u) => ({ url: u, has_backup: false }));
        invoke<NostrBackupStatus>("check_nostr_backup")
          .then((status) => {
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
            render();
          })
          .catch(() => {});
      })
      .catch(() => {});

    invoke<NostrProfile | null>("fetch_nostr_profile")
      .then((profile) => {
        if (profile) {
          state.nostrProfile = profile;
          render();
        }
      })
      .catch(() => {});
  }
}

function dismissSplash(): void {
  const splash = document.getElementById("splash");
  if (!splash) return;
  splash.classList.add("fade-out");
  splash.addEventListener("transitionend", () => splash.remove(), {
    once: true,
  });
}

// ── Boot ─────────────────────────────────────────────────────────────

async function initApp(): Promise<void> {
  render();
  updateEstClockLabels();

  // Track when the minimum loader animation time has elapsed (2 full cycles = 4.8s)
  const splashReady = new Promise<void>((r) => setTimeout(r, 4800));

  // 1. Try to load existing Nostr identity (no auto-generation)
  let hasNostrIdentity = false;
  try {
    const identity = await invoke<IdentityResponse | null>(
      "init_nostr_identity",
    );
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
    invoke<string[]>("fetch_nip65_relay_list")
      .then((relays) => {
        state.relays = relays.map((u) => ({ url: u, has_backup: false }));
        invoke<NostrBackupStatus>("check_nostr_backup")
          .then((status) => {
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
            render();
          })
          .catch(() => {});
      })
      .catch(() => {
        state.relays = [
          { url: "wss://relay.damus.io", has_backup: false },
          { url: "wss://relay.primal.net", has_backup: false },
        ];
      });

    invoke<NostrProfile | null>("fetch_nostr_profile")
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
      invoke<NostrBackupStatus>("check_nostr_backup")
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

// ── Event listeners ──────────────────────────────────────────────────

const clickDeps = { render, openMarket, finishOnboarding };

app.addEventListener("click", (event) => {
  void handleClick(event as MouseEvent, clickDeps);
});

app.addEventListener("input", (e) => {
  handleInput(e, render);
});

app.addEventListener("keydown", (e) => {
  handleKeydown(e as KeyboardEvent, {
    render,
    openMarket: (id: string) => openMarket(id),
  });
});

app.addEventListener("focusout", (e) => {
  handleFocusout(e as FocusEvent, render);
});

app.addEventListener("mousemove", (event) => {
  const target = event.target as HTMLElement;
  const probe = target.closest("[data-chart-hover='1']") as HTMLElement | null;

  if (!probe) {
    if (state.chartHoverMarketId !== null || state.chartHoverX !== null) {
      state.chartHoverMarketId = null;
      state.chartHoverX = null;
      render();
    }
    return;
  }

  const marketId = probe.dataset.marketId ?? null;
  if (!marketId) return;
  const plotWidth = Number.parseFloat(probe.dataset.plotWidth ?? "100");
  const plotLeft = Number.parseFloat(probe.dataset.plotLeft ?? "0");
  const plotRight = Number.parseFloat(probe.dataset.plotRight ?? "100");
  const widthMax = Number.isFinite(plotWidth) ? plotWidth : 100;
  const xMin = Number.isFinite(plotLeft) ? plotLeft : 0;
  const xMax = Number.isFinite(plotRight) ? plotRight : widthMax;

  const rect = probe.getBoundingClientRect();
  if (rect.width <= 0) return;
  const relativeX = Math.max(
    0,
    Math.min(widthMax, ((event.clientX - rect.left) / rect.width) * widthMax),
  );
  const hoverX = Math.max(xMin, Math.min(xMax, relativeX));

  if (
    state.chartHoverMarketId === marketId &&
    state.chartHoverX !== null &&
    Math.abs(state.chartHoverX - hoverX) < 0.06
  ) {
    return;
  }

  state.chartHoverMarketId = marketId;
  state.chartHoverX = hoverX;
  render();
});

app.addEventListener("mouseleave", () => {
  if (state.chartHoverMarketId !== null || state.chartHoverX !== null) {
    state.chartHoverMarketId = null;
    state.chartHoverX = null;
    render();
  }
});

app.addEventListener("mouseout", (event) => {
  const from = (event.target as HTMLElement).closest(
    "[data-chart-hover='1']",
  ) as HTMLElement | null;
  if (!from) return;
  const related = event.relatedTarget as HTMLElement | null;
  if (related?.closest("[data-chart-hover='1']")) return;
  if (state.chartHoverMarketId !== null || state.chartHoverX !== null) {
    state.chartHoverMarketId = null;
    state.chartHoverX = null;
    render();
  }
});

window.addEventListener("resize", () => {
  scheduleChartAspectSync();
});

// ── Timers ───────────────────────────────────────────────────────────

initApp();
setInterval(updateEstClockLabels, 1_000);
setInterval(() => {
  if (state.onboardingStep === null) {
    void syncCurrentHeightFromLwk(
      "liquid-testnet",
      render,
      updateEstClockLabels,
    );
  }
}, 60_000);

// Auto-refresh wallet balance every 60s when unlocked (cached only, no Electrum sync)
setInterval(() => {
  if (
    state.onboardingStep === null &&
    state.walletStatus === "unlocked" &&
    !state.walletLoading
  ) {
    (async () => {
      try {
        const balance = await invoke<{ assets: Record<string, number> }>(
          "get_wallet_balance",
        );
        state.walletBalance = balance.assets;
        if (state.view === "wallet") render();
      } catch (_) {
        // Silent — don't disrupt the user
      }
    })();
  }
}, 60_000);
