import type { ChartTimescale, Market, PriceHistoryEntry } from "../types";

export type ChartHistoryPoint = {
  blockHeight: number;
  probability: number;
};

export type ChartSeriesData = {
  endBlockHeight: number;
  historyPoints: ChartHistoryPoint[];
  pointCount: number;
  scaleBlocks: number;
  startBlockHeight: number;
  xLabels: string[];
  yesSeries: Array<number | null>;
};

// Liquid: ~1 block per minute
const SCALE_BLOCKS_BY_KEY: Record<ChartTimescale, number> = {
  "1h": 60,
  "4h": 240,
  "1d": 1440,
  "3d": 4320,
  "7d": 10080,
  "1M": 43200,
  all: 0, // computed dynamically from history span
};

const POINT_COUNT_BY_KEY: Record<ChartTimescale, number> = {
  "1h": 60,
  "4h": 80,
  "1d": 96,
  "3d": 108,
  "7d": 120,
  "1M": 130,
  all: 150,
};

function clampProbability(value: number): number {
  return Math.max(0.02, Math.min(0.98, value));
}

function scaleConfig(
  timescale: ChartTimescale,
  historySpanBlocks = 0,
): {
  pointCount: number;
  scaleBlocks: number;
} {
  const pointCount = POINT_COUNT_BY_KEY[timescale];
  const scaleBlocks =
    timescale === "all"
      ? Math.max(SCALE_BLOCKS_BY_KEY["7d"], historySpanBlocks)
      : SCALE_BLOCKS_BY_KEY[timescale];
  return { pointCount, scaleBlocks };
}

/** Approximate wall-clock date for a given block height. */
function blockHeightToDate(blockHeight: number, currentHeight: number): Date {
  const blocksAgo = currentHeight - blockHeight;
  // ~1 block per minute on Liquid
  return new Date(Date.now() - blocksAgo * 60_000);
}

/** Format a block height as a short x-axis label (e.g. "12:30", "Apr 3"). */

/**
 * Format a block height as a precise hover tooltip timestamp.
 * Always includes time; adds date when not today.
 */
export function blockHeightToHoverLabel(
  blockHeight: number,
  currentHeight: number,
): string {
  const date = blockHeightToDate(blockHeight, currentHeight);
  const now = new Date();
  const sameDay =
    date.getFullYear() === now.getFullYear() &&
    date.getMonth() === now.getMonth() &&
    date.getDate() === now.getDate();
  const timeStr = date.toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  });
  if (sameDay) return timeStr;
  const dateStr = date.toLocaleDateString([], {
    month: "short",
    day: "numeric",
  });
  return `${dateStr} ${timeStr}`;
}

function blockHeightToAxisLabel(
  blockHeight: number,
  currentHeight: number,
  timescale: ChartTimescale,
): string {
  const date = blockHeightToDate(blockHeight, currentHeight);
  const timeStr = date.toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  });
  // Intraday views: time only
  if (timescale === "1h" || timescale === "4h") return timeStr;
  // 1d: time only (all labels fall within ~1 day, same or adjacent day)
  if (timescale === "1d") {
    const now = new Date();
    const sameDay =
      date.getFullYear() === now.getFullYear() &&
      date.getMonth() === now.getMonth() &&
      date.getDate() === now.getDate();
    if (sameDay) return timeStr;
    return date.toLocaleDateString([], { month: "short", day: "numeric" });
  }
  // 3d, 7d, 1M, all: date only — multi-day views don't need time precision
  return date.toLocaleDateString([], { month: "short", day: "numeric" });
}

function buildXAxisLabels(
  startBlockHeight: number,
  scaleBlocks: number,
  currentHeight: number,
  timescale: ChartTimescale,
): string[] {
  const fractions = [0, 0.25, 0.5, 0.75, 1];
  return fractions.map((fraction) => {
    const height = Math.round(startBlockHeight + fraction * scaleBlocks);
    return blockHeightToAxisLabel(height, currentHeight, timescale);
  });
}

function sampleHistoryProbabilityAtHeight(
  historyPoints: ChartHistoryPoint[],
  sampleHeight: number,
): number | null {
  if (historyPoints.length === 0) return null;
  // Clamp to first known value instead of returning null — this fills the
  // left portion of the chart when the first history point is mid-window.
  if (sampleHeight <= historyPoints[0].blockHeight)
    return historyPoints[0].probability;

  let leftPoint = historyPoints[0];
  for (let idx = 1; idx < historyPoints.length; idx += 1) {
    const rightPoint = historyPoints[idx];
    if (sampleHeight < rightPoint.blockHeight) {
      const heightSpan = rightPoint.blockHeight - leftPoint.blockHeight;
      if (heightSpan <= 0) {
        return rightPoint.probability;
      }
      const mix = (sampleHeight - leftPoint.blockHeight) / heightSpan;
      const interpolated =
        leftPoint.probability +
        (rightPoint.probability - leftPoint.probability) * mix;
      return clampProbability(interpolated);
    }
    leftPoint = rightPoint;
  }

  return historyPoints[historyPoints.length - 1].probability;
}

function sampleDenseSeriesAtFraction(
  series: Array<number | null>,
  fraction: number,
): number | null {
  if (series.length === 0) return null;
  const clampedFraction = Math.max(0, Math.min(1, fraction));
  const seriesPosition = clampedFraction * (series.length - 1);
  const leftIndex = Math.max(
    0,
    Math.min(series.length - 1, Math.floor(seriesPosition)),
  );
  const rightIndex = Math.max(
    leftIndex,
    Math.min(series.length - 1, Math.ceil(seriesPosition)),
  );
  const leftValue = series[leftIndex];
  const rightValue = series[rightIndex];
  if (leftValue === null || rightValue === null) return null;

  const mix = Math.max(0, Math.min(1, seriesPosition - leftIndex));
  return leftValue + (rightValue - leftValue) * mix;
}

/** Accepts timescale as param instead of reading from global state. */
export function buildChartSeriesData(
  market: Market,
  timescale: ChartTimescale,
): ChartSeriesData {
  const { pointCount, scaleBlocks } = scaleConfig(timescale);
  const endBlockHeight = Math.max(
    scaleBlocks,
    market.currentHeight || scaleBlocks,
  );
  const startBlockHeight = Math.max(0, endBlockHeight - scaleBlocks);
  const xLabels = buildXAxisLabels(
    startBlockHeight,
    scaleBlocks,
    market.currentHeight || endBlockHeight,
    timescale,
  );

  return {
    endBlockHeight,
    historyPoints: [],
    pointCount,
    scaleBlocks,
    startBlockHeight,
    xLabels,
    yesSeries: Array.from({ length: pointCount }, () => null),
  };
}

export function buildChartFromHistory(
  market: Market,
  history: PriceHistoryEntry[],
  timescale: ChartTimescale,
): ChartSeriesData {
  const firstHistoryHeight = history.length > 0 ? history[0].block_height : 0;
  const latestHistoryHeight =
    history.length > 0 ? history[history.length - 1].block_height : 0;
  const historySpanBlocks =
    latestHistoryHeight > firstHistoryHeight
      ? latestHistoryHeight - firstHistoryHeight
      : 0;
  const { pointCount, scaleBlocks } = scaleConfig(timescale, historySpanBlocks);
  const endBlockHeight = Math.max(
    scaleBlocks,
    market.currentHeight || 0,
    latestHistoryHeight,
  );
  const startBlockHeight = Math.max(0, endBlockHeight - scaleBlocks);
  const xLabels = buildXAxisLabels(
    startBlockHeight,
    scaleBlocks,
    market.currentHeight || endBlockHeight,
    timescale,
  );
  // Sort all entries first so we can find the last point before the window.
  const allSorted = history
    .map((entry) => ({
      blockHeight: entry.block_height,
      probability: clampProbability(entry.implied_yes_price_bps / 10_000),
    }))
    .sort((a, b) => a.blockHeight - b.blockHeight);

  // Include one point before startBlockHeight so left-edge interpolation works:
  // without it, any sample between startBlockHeight and the first in-window
  // point would clamp to that point rather than interpolating from before it.
  const beforeWindow = allSorted.filter(
    (p) => p.blockHeight < startBlockHeight,
  );
  const lastBeforeWindow =
    beforeWindow.length > 0 ? beforeWindow[beforeWindow.length - 1] : undefined;

  const inWindow = allSorted.filter(
    (p) => p.blockHeight >= startBlockHeight && p.blockHeight <= endBlockHeight,
  );

  const historyPoints = lastBeforeWindow
    ? [lastBeforeWindow, ...inWindow]
    : inWindow;
  const yesSeries: Array<number | null> = [];
  const fallbackYes = market.yesPrice ?? 0.5;

  if (historyPoints.length === 0) {
    for (let idx = 0; idx < pointCount; idx += 1) {
      yesSeries.push(fallbackYes);
    }
  } else {
    for (let idx = 0; idx < pointCount; idx += 1) {
      const t = pointCount === 1 ? 1 : idx / (pointCount - 1);
      const sampleHeight = startBlockHeight + t * scaleBlocks;
      yesSeries.push(
        sampleHistoryProbabilityAtHeight(historyPoints, sampleHeight),
      );
    }
  }

  return {
    endBlockHeight,
    historyPoints,
    pointCount,
    scaleBlocks,
    startBlockHeight,
    xLabels,
    yesSeries,
  };
}

export function sampleChartProbabilityAtFraction(
  seriesData: ChartSeriesData,
  fraction: number,
): number | null {
  const clampedFraction = Math.max(0, Math.min(1, fraction));
  if (seriesData.historyPoints.length > 0) {
    const sampleHeight =
      seriesData.startBlockHeight + clampedFraction * seriesData.scaleBlocks;
    return sampleHistoryProbabilityAtHeight(
      seriesData.historyPoints,
      sampleHeight,
    );
  }

  return sampleDenseSeriesAtFraction(seriesData.yesSeries, clampedFraction);
}

export function lastDefinedChartProbability(
  seriesData: ChartSeriesData,
  fallbackProbability: number,
): number {
  for (let idx = seriesData.yesSeries.length - 1; idx >= 0; idx -= 1) {
    const value = seriesData.yesSeries[idx];
    if (value !== null) {
      return value;
    }
  }

  return fallbackProbability;
}
