import { state } from "../state.ts";
import type { ChartTimescale, Market, PriceHistoryEntry } from "../types.ts";

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
  xLabels: number[];
  yesSeries: Array<number | null>;
};

const SCALE_BLOCKS_BY_KEY: Record<ChartTimescale, number> = {
  "10B": 10,
  "25B": 25,
  "50B": 50,
  "100B": 100,
};

const POINT_COUNT_BY_KEY: Record<ChartTimescale, number> = {
  "10B": 20,
  "25B": 28,
  "50B": 40,
  "100B": 56,
};

function clampProbability(value: number): number {
  return Math.max(0.02, Math.min(0.98, value));
}

function currentScaleConfig(): {
  pointCount: number;
  scaleBlocks: number;
  scaleKey: ChartTimescale;
} {
  const scaleKey = state.chartTimescale;
  return {
    scaleKey,
    scaleBlocks: SCALE_BLOCKS_BY_KEY[scaleKey],
    pointCount: POINT_COUNT_BY_KEY[scaleKey],
  };
}

function buildXAxisLabels(
  startBlockHeight: number,
  scaleBlocks: number,
): number[] {
  const fractions =
    scaleBlocks >= 100 ? [0, 0.25, 0.5, 0.75, 1] : [0, 1 / 3, 2 / 3, 1];
  return fractions.map((fraction) =>
    Math.round(startBlockHeight + fraction * scaleBlocks),
  );
}

function sampleHistoryProbabilityAtHeight(
  historyPoints: ChartHistoryPoint[],
  sampleHeight: number,
): number | null {
  // History-backed charts stay empty until the first visible confirmed point.
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

export function buildChartSeriesData(market: Market): ChartSeriesData {
  const yes = market.yesPrice ?? 0.5;
  const { pointCount, scaleBlocks } = currentScaleConfig();
  const endBlockHeight = Math.max(
    scaleBlocks,
    market.currentHeight || scaleBlocks,
  );
  const startBlockHeight = Math.max(0, endBlockHeight - scaleBlocks);
  const xLabels = buildXAxisLabels(startBlockHeight, scaleBlocks);
  const seed =
    [...market.id].reduce((sum, ch) => sum + ch.charCodeAt(0), 0) % 97;
  const trendSign = seed % 2 === 0 ? 1 : -1;
  let rngState = seed * 1103515245 + 12345;
  const rand = (): number => {
    rngState = (rngState * 1664525 + 1013904223) >>> 0;
    return rngState / 0xffffffff;
  };
  const baseSeriesCount = 240;
  const baseSeries: number[] = [];
  const historicalBias = (0.2 + (seed % 5) * 0.02) * trendSign;
  const historicalCenter = clampProbability(yes + historicalBias);
  for (let idx = 0; idx < baseSeriesCount; idx += 1) {
    const t = idx / (baseSeriesCount - 1);
    const transitionStart = 0.88;
    const transitionT =
      t <= transitionStart ? 0 : (t - transitionStart) / (1 - transitionStart);
    const smoothTransition = transitionT * transitionT * (3 - 2 * transitionT);
    const macroAnchor =
      historicalCenter + (yes - historicalCenter) * smoothTransition;
    const microWaveAmp = t > 0.8 ? 0.03 : 0.018;
    const microWave =
      Math.sin((t * 22 + seed * 0.117) * Math.PI * 2) * microWaveAmp;
    const anchor = clampProbability(macroAnchor + microWave);
    if (idx === 0) {
      baseSeries.push(anchor);
      continue;
    }
    const prev = baseSeries[idx - 1];
    const jump = idx % 20 === 0 ? (rand() - 0.5) * 0.18 : 0;
    const driftPull = (anchor - prev) * 0.2;
    const stepNoise =
      idx % 4 === 0 ? (rand() - 0.5) * 0.03 : (rand() - 0.5) * 0.004;
    const next = prev + jump + driftPull + stepNoise;
    if (idx % 3 !== 0) {
      baseSeries.push(clampProbability(prev + (rand() - 0.5) * 0.0028));
      continue;
    }
    baseSeries.push(clampProbability(next));
  }
  baseSeries[baseSeries.length - 1] = yes;
  const windowStartT = Math.max(0, 1 - scaleBlocks / 120);
  const yesSeries = Array.from({ length: pointCount }, (_v, idx) => {
    const localT = pointCount === 1 ? 1 : idx / (pointCount - 1);
    const baseT = windowStartT + localT * (1 - windowStartT);
    const basePosition = baseT * (baseSeriesCount - 1);
    const left = Math.max(
      0,
      Math.min(baseSeriesCount - 1, Math.floor(basePosition)),
    );
    const right = Math.max(
      left,
      Math.min(baseSeriesCount - 1, Math.ceil(basePosition)),
    );
    const mix = Math.max(0, Math.min(1, basePosition - left));
    return baseSeries[left] + (baseSeries[right] - baseSeries[left]) * mix;
  });
  yesSeries[yesSeries.length - 1] = yes;

  return {
    endBlockHeight,
    historyPoints: [],
    pointCount,
    scaleBlocks,
    startBlockHeight,
    xLabels,
    yesSeries,
  };
}

export function buildChartFromHistory(
  market: Market,
  history: PriceHistoryEntry[],
): ChartSeriesData {
  const { pointCount, scaleBlocks } = currentScaleConfig();
  const latestHistoryHeight =
    history.length > 0 ? history[history.length - 1].block_height : 0;
  const endBlockHeight = Math.max(
    scaleBlocks,
    market.currentHeight || 0,
    latestHistoryHeight,
  );
  const startBlockHeight = Math.max(0, endBlockHeight - scaleBlocks);
  const xLabels = buildXAxisLabels(startBlockHeight, scaleBlocks);
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
