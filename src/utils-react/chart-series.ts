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
  "30m": 30,
  "1h": 60,
  "4h": 240,
  "1d": 1440,
};

const POINT_COUNT_BY_KEY: Record<ChartTimescale, number> = {
  "30m": 30,
  "1h": 60,
  "4h": 80,
  "1d": 96,
};

function clampProbability(value: number): number {
  return Math.max(0.02, Math.min(0.98, value));
}

function scaleConfig(timescale: ChartTimescale): {
  pointCount: number;
  scaleBlocks: number;
} {
  return {
    scaleBlocks: SCALE_BLOCKS_BY_KEY[timescale],
    pointCount: POINT_COUNT_BY_KEY[timescale],
  };
}

/** Approximate wall-clock date for a given block height. */
function blockHeightToDate(blockHeight: number, currentHeight: number): Date {
  const blocksAgo = currentHeight - blockHeight;
  // ~1 block per minute on Liquid
  return new Date(Date.now() - blocksAgo * 60_000);
}

/** Format a block height as a short x-axis label (e.g. "12:30", "Apr 3"). */
function blockHeightToTimeLabel(
  blockHeight: number,
  currentHeight: number,
): string {
  const date = blockHeightToDate(blockHeight, currentHeight);
  const now = new Date();
  const sameDay =
    date.getFullYear() === now.getFullYear() &&
    date.getMonth() === now.getMonth() &&
    date.getDate() === now.getDate();
  if (sameDay) {
    return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  }
  return date.toLocaleDateString([], { month: "short", day: "numeric" });
}

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

function buildXAxisLabels(
  startBlockHeight: number,
  scaleBlocks: number,
  currentHeight: number,
): string[] {
  const fractions =
    scaleBlocks >= 100 ? [0, 0.25, 0.5, 0.75, 1] : [0, 1 / 3, 2 / 3, 1];
  return fractions.map((fraction) => {
    const height = Math.round(startBlockHeight + fraction * scaleBlocks);
    return blockHeightToTimeLabel(height, currentHeight);
  });
}

function sampleHistoryProbabilityAtHeight(
  historyPoints: ChartHistoryPoint[],
  sampleHeight: number,
): number | null {
  if (historyPoints.length === 0) return null;
  if (sampleHeight < historyPoints[0].blockHeight) return null;

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
  const { pointCount, scaleBlocks } = scaleConfig(timescale);
  const latestHistoryHeight =
    history.length > 0 ? history[history.length - 1].block_height : 0;
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
  );
  const historyPoints = history
    .map((entry) => ({
      blockHeight: entry.block_height,
      probability: clampProbability(entry.implied_yes_price_bps / 10_000),
    }))
    .filter(
      (point) =>
        point.blockHeight >= startBlockHeight &&
        point.blockHeight <= endBlockHeight,
    )
    .sort((a, b) => a.blockHeight - b.blockHeight);
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
