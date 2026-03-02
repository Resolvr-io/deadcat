import { state } from "../state.ts";

export function setupChartHoverListeners(params: {
  app: HTMLDivElement;
  scheduleChartHoverRender: () => void;
  scheduleChartAspectSync: () => void;
}): () => void {
  const { app, scheduleChartHoverRender, scheduleChartAspectSync } = params;

  const onMouseMove = (event: MouseEvent): void => {
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
  };

  const onMouseLeave = (): void => {
    if (state.chartHoverMarketId !== null || state.chartHoverX !== null) {
      state.chartHoverMarketId = null;
      state.chartHoverX = null;
      scheduleChartHoverRender();
    }
  };

  const onMouseOut = (event: MouseEvent): void => {
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
  };

  const onResize = (): void => {
    scheduleChartAspectSync();
  };

  app.addEventListener("mousemove", onMouseMove);
  app.addEventListener("mouseleave", onMouseLeave);
  app.addEventListener("mouseout", onMouseOut);
  window.addEventListener("resize", onResize);

  return () => {
    app.removeEventListener("mousemove", onMouseMove);
    app.removeEventListener("mouseleave", onMouseLeave);
    app.removeEventListener("mouseout", onMouseOut);
    window.removeEventListener("resize", onResize);
  };
}
