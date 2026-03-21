import { state } from "../state.ts";
import type { Market, PriceHistoryEntry } from "../types.ts";

export type ChartSeriesData = {
  endTime: Date;
  pointCount: number;
  scaleHours: number;
  startTime: Date;
  xLabelOffsets: number[];
  xLabels: Date[];
  yesSeries: number[];
};

export function buildChartSeriesData(market: Market): ChartSeriesData {
  const yes = market.yesPrice ?? 0.5;
  const endTime = new Date();
  const scaleHoursByKey: Record<"1H" | "3H" | "6H" | "12H" | "1D", number> = {
    "1H": 1,
    "3H": 3,
    "6H": 6,
    "12H": 12,
    "1D": 24,
  };
  const pointCountByKey: Record<"1H" | "3H" | "6H" | "12H" | "1D", number> = {
    "1H": 28,
    "3H": 34,
    "6H": 40,
    "12H": 48,
    "1D": 56,
  };
  const scaleKey = state.chartTimescale;
  const scaleHours = scaleHoursByKey[scaleKey];
  const pointCount = pointCountByKey[scaleKey];
  const totalHours = 24;
  const startTime = new Date(endTime.getTime() - scaleHours * 60 * 60 * 1000);
  const xLabelFractions =
    scaleHours >= 12 ? [0, 0.25, 0.5, 0.75, 1] : [0, 1 / 3, 2 / 3, 1];
  const xLabels = xLabelFractions.map(
    (fraction) =>
      new Date(
        startTime.getTime() +
          fraction * (endTime.getTime() - startTime.getTime()),
      ),
  );
  const xLabelOffsets = xLabelFractions.map(
    (fraction) => scaleHours * (1 - fraction),
  );
  const seed =
    [...market.id].reduce((sum, ch) => sum + ch.charCodeAt(0), 0) % 97;
  const trendSign = seed % 2 === 0 ? 1 : -1;
  const clampProbability = (value: number): number =>
    Math.max(0.02, Math.min(0.98, value));
  let rngState = seed * 1103515245 + 12345;
  const rand = (): number => {
    rngState = (rngState * 1664525 + 1013904223) >>> 0;
    return rngState / 0xffffffff;
  };
  const baseSeriesCount = 24 * 12 + 1;
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
  const windowStartT = Math.max(0, 1 - scaleHours / totalHours);
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
    endTime,
    pointCount,
    scaleHours,
    startTime,
    xLabelOffsets,
    xLabels,
    yesSeries,
  };
}

export function buildChartFromHistory(
  market: Market,
  history: PriceHistoryEntry[],
): ChartSeriesData {
  const scaleHoursByKey: Record<"1H" | "3H" | "6H" | "12H" | "1D", number> = {
    "1H": 1,
    "3H": 3,
    "6H": 6,
    "12H": 12,
    "1D": 24,
  };
  const pointCountByKey: Record<"1H" | "3H" | "6H" | "12H" | "1D", number> = {
    "1H": 28,
    "3H": 34,
    "6H": 40,
    "12H": 48,
    "1D": 56,
  };
  const scaleKey = state.chartTimescale;
  const scaleHours = scaleHoursByKey[scaleKey];
  const pointCount = pointCountByKey[scaleKey];
  const endTime = new Date();
  const startTime = new Date(endTime.getTime() - scaleHours * 60 * 60 * 1000);
  const xLabelFractions =
    scaleHours >= 12 ? [0, 0.25, 0.5, 0.75, 1] : [0, 1 / 3, 2 / 3, 1];
  const xLabels = xLabelFractions.map(
    (fraction) =>
      new Date(
        startTime.getTime() +
          fraction * (endTime.getTime() - startTime.getTime()),
      ),
  );
  const xLabelOffsets = xLabelFractions.map(
    (fraction) => scaleHours * (1 - fraction),
  );

  // Convert history entries to timestamped probability values
  const historyPoints = history
    .map((entry) => ({
      time: new Date(entry.recorded_at).getTime(),
      probability: Math.max(
        0.02,
        Math.min(0.98, entry.implied_yes_price_bps / 10000),
      ),
    }))
    .filter((p) => p.time >= startTime.getTime() && p.time <= endTime.getTime())
    .sort((a, b) => a.time - b.time);

  // Resample to standard pointCount
  const timeSpan = endTime.getTime() - startTime.getTime();
  const yesSeries: number[] = [];
  const fallbackYes = market.yesPrice ?? 0.5;

  if (historyPoints.length === 0) {
    // No history in window — flat line at current price
    for (let i = 0; i < pointCount; i++) {
      yesSeries.push(fallbackYes);
    }
  } else {
    for (let i = 0; i < pointCount; i++) {
      const t = pointCount === 1 ? 1 : i / (pointCount - 1);
      const sampleTime = startTime.getTime() + t * timeSpan;

      // Find surrounding history points for interpolation
      let leftIdx = 0;
      for (let j = 0; j < historyPoints.length; j++) {
        if (historyPoints[j].time <= sampleTime) {
          leftIdx = j;
        } else {
          break;
        }
      }
      const rightIdx = Math.min(leftIdx + 1, historyPoints.length - 1);

      if (leftIdx === rightIdx) {
        yesSeries.push(historyPoints[leftIdx].probability);
      } else {
        const leftTime = historyPoints[leftIdx].time;
        const rightTime = historyPoints[rightIdx].time;
        const mix =
          rightTime === leftTime
            ? 0
            : (sampleTime - leftTime) / (rightTime - leftTime);
        const interpolated =
          historyPoints[leftIdx].probability +
          (historyPoints[rightIdx].probability -
            historyPoints[leftIdx].probability) *
            mix;
        yesSeries.push(Math.max(0.02, Math.min(0.98, interpolated)));
      }
    }
    // Snap last point to the last history entry's probability to avoid a
    // visual jump when the current market.yesPrice diverges from the most
    // recent on-chain transition.
    yesSeries[yesSeries.length - 1] =
      historyPoints[historyPoints.length - 1].probability;
  }

  return {
    endTime,
    pointCount,
    scaleHours,
    startTime,
    xLabelOffsets,
    xLabels,
    yesSeries,
  };
}
