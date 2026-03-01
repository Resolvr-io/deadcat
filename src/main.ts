import "./style.css";
import { finishOnboarding, initApp } from "./bootstrap.ts";
import { renderCreateMarket } from "./components/create.ts";
import { renderDetail } from "./components/detail.ts";
import { renderHome } from "./components/home.ts";
import { renderOnboarding } from "./components/onboarding.ts";
// Components
import { renderTopShell } from "./components/shell.ts";
import { renderWallet } from "./components/wallet.ts";
import { renderNostrEventModal } from "./components/wallet-modals.ts";
// Handlers
import { handleClick } from "./handlers/click.ts";
import { handleFocusout } from "./handlers/focusout.ts";
import { handleInput } from "./handlers/input.ts";
import { handleKeydown } from "./handlers/keydown.ts";
import { setupActivityTracking } from "./lifecycle/activity.ts";
import { setupChartHoverListeners } from "./lifecycle/chart-hover.ts";
import { setupTauriSubscriptions } from "./lifecycle/subscriptions.ts";
import { startRecurringTimers } from "./lifecycle/timers.ts";
import { app, state } from "./state.ts";
import type { Side, TradeIntent } from "./types.ts";
import { formatEstTime, formatSatsInput } from "./utils/format.ts";
// Utils
import {
  getBasePriceSats,
  getMarketById,
  getPositionContracts,
  setLimitPriceSats,
} from "./utils/market.ts";

// ── Core render ──────────────────────────────────────────────────────

let chartAspectSyncRaf: number | null = null;
let chartHoverRenderRaf: number | null = null;
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

function scheduleChartHoverRender(): void {
  if (chartHoverRenderRaf !== null) return;
  chartHoverRenderRaf = requestAnimationFrame(() => {
    chartHoverRenderRaf = null;
    render();
  });
}

let _savedScrollY = 0;
document.addEventListener("scroll", () => {
  _savedScrollY = window.scrollY;
});

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
  const prevHeight = app.scrollHeight;
  app.style.minHeight = `${prevHeight}px`;
  app.innerHTML = html;
  window.scrollTo(0, _savedScrollY);
  app.style.minHeight = "";
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

function dismissSplash(): void {
  const splash = document.getElementById("splash");
  if (!splash) return;
  splash.classList.add("fade-out");
  splash.addEventListener("transitionend", () => splash.remove(), {
    once: true,
  });
}

// ── Boot ─────────────────────────────────────────────────────────────

setupTauriSubscriptions(render);
setupActivityTracking();

// ── Event listeners ──────────────────────────────────────────────────

const clickDeps = {
  render,
  openMarket,
  finishOnboarding: () => finishOnboarding(render, updateEstClockLabels),
};

app.addEventListener("click", (event) => {
  void handleClick(event as MouseEvent, clickDeps);
});

app.addEventListener("input", (e) => {
  handleInput(e, render);
});

app.addEventListener("keydown", (e) => {
  handleKeydown(e as KeyboardEvent, { render });
});

app.addEventListener("focusout", (e) => {
  handleFocusout(e as FocusEvent, render);
});

setupChartHoverListeners({
  app,
  scheduleChartHoverRender,
  scheduleChartAspectSync,
});

// ── Timers ───────────────────────────────────────────────────────────

void initApp(render, dismissSplash, updateEstClockLabels);
startRecurringTimers(render, updateEstClockLabels);
