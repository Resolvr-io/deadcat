import { state } from "../state.ts";

export function setupChartHoverListeners(params: {
  app: HTMLDivElement;
  scheduleChartHoverRender: () => void;
  scheduleChartAspectSync: () => void;
}): void {
  const { app, scheduleChartHoverRender, scheduleChartAspectSync } = params;

  app.addEventListener("mousemove", (event) => {
    const target = event.target as HTMLElement;
    const probe = target.closest(
      "[data-chart-hover='1']",
    ) as HTMLElement | null;

    if (!probe) {
      if (state.chartHoverMarketId !== null || state.chartHoverX !== null) {
        state.chartHoverMarketId = null;
        state.chartHoverX = null;
        scheduleChartHoverRender();
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
    scheduleChartHoverRender();
  });

  app.addEventListener("mouseleave", () => {
    if (state.chartHoverMarketId !== null || state.chartHoverX !== null) {
      state.chartHoverMarketId = null;
      state.chartHoverX = null;
      scheduleChartHoverRender();
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
      scheduleChartHoverRender();
    }
  });

  window.addEventListener("resize", () => {
    scheduleChartAspectSync();
  });
}
