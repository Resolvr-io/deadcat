import type { Market, PriceHistoryEntry } from "../types";

/** Simple deterministic hash for seeding. */
function hashCode(s: string): number {
  let h = 0;
  for (let i = 0; i < s.length; i++) {
    h = (Math.imul(31, h) + s.charCodeAt(i)) | 0;
  }
  return Math.abs(h);
}

/** Seeded pseudo-random float in [0, 1). */
function seededRand(seed: number): number {
  const x = Math.sin(seed + 1) * 10000;
  return x - Math.floor(x);
}

/** Clamp a BPS value to valid range [100, 9900]. */
function clampBps(v: number): number {
  return Math.max(100, Math.min(9900, Math.round(v)));
}

type ChartShape =
  | "trending-up"
  | "trending-down"
  | "volatile"
  | "v-shape"
  | "inverted-v"
  | "late-spike"
  | "early-peak"
  | "s-curve";

const SHAPES: ChartShape[] = [
  "trending-up",
  "trending-down",
  "volatile",
  "v-shape",
  "inverted-v",
  "late-spike",
  "early-peak",
  "s-curve",
];

/**
 * Compute the "raw" price path for a given shape, returning normalised
 * values in [0, 1] at each of `pointCount` steps, ending at `endNorm`.
 */
function shapePath(
  shape: ChartShape,
  endNorm: number,
  seed: number,
  pointCount: number,
): number[] {
  const noise = (i: number, amp: number) =>
    (seededRand(seed + i * 7) - 0.5) * 2 * amp;

  switch (shape) {
    case "trending-up": {
      // Starts 30–50% below end; smooth upward trend.
      const startNorm = Math.max(0.05, endNorm - 0.3 - seededRand(seed) * 0.2);
      return Array.from({ length: pointCount }, (_, i) => {
        const t = i / (pointCount - 1);
        return startNorm + (endNorm - startNorm) * t + noise(i, 0.025);
      });
    }

    case "trending-down": {
      // Starts 30–50% above end; smooth downward trend.
      const startNorm = Math.min(0.95, endNorm + 0.3 + seededRand(seed) * 0.2);
      return Array.from({ length: pointCount }, (_, i) => {
        const t = i / (pointCount - 1);
        return startNorm + (endNorm - startNorm) * t + noise(i, 0.025);
      });
    }

    case "volatile": {
      // High-noise random walk ending at endNorm.
      const startNorm = 0.3 + seededRand(seed) * 0.4;
      return Array.from({ length: pointCount }, (_, i) => {
        const t = i / (pointCount - 1);
        return startNorm + (endNorm - startNorm) * t + noise(i, 0.1);
      });
    }

    case "v-shape": {
      // Drops to a trough at 40% of series, then recovers to endNorm.
      const troughNorm = Math.max(0.05, endNorm - 0.35 - seededRand(seed) * 0.15);
      const startNorm = endNorm + (seededRand(seed + 1) - 0.5) * 0.1;
      const troughAt = 0.4;
      return Array.from({ length: pointCount }, (_, i) => {
        const t = i / (pointCount - 1);
        const base =
          t < troughAt
            ? startNorm + (troughNorm - startNorm) * (t / troughAt)
            : troughNorm + (endNorm - troughNorm) * ((t - troughAt) / (1 - troughAt));
        return base + noise(i, 0.02);
      });
    }

    case "inverted-v": {
      // Rises to a peak at ~50%, then falls to endNorm.
      const peakNorm = Math.min(0.95, endNorm + 0.3 + seededRand(seed) * 0.15);
      const startNorm = endNorm + (seededRand(seed + 1) - 0.5) * 0.1;
      const peakAt = 0.45 + seededRand(seed + 2) * 0.1;
      return Array.from({ length: pointCount }, (_, i) => {
        const t = i / (pointCount - 1);
        const base =
          t < peakAt
            ? startNorm + (peakNorm - startNorm) * (t / peakAt)
            : peakNorm + (endNorm - peakNorm) * ((t - peakAt) / (1 - peakAt));
        return base + noise(i, 0.02);
      });
    }

    case "late-spike": {
      // Flat for 65% of the series, then rapid move to endNorm.
      const flatNorm = endNorm - (endNorm > 0.5 ? 0.25 : -0.25) - seededRand(seed) * 0.1;
      const spikeStart = 0.65;
      return Array.from({ length: pointCount }, (_, i) => {
        const t = i / (pointCount - 1);
        const base =
          t < spikeStart
            ? flatNorm
            : flatNorm + (endNorm - flatNorm) * ((t - spikeStart) / (1 - spikeStart));
        return base + noise(i, t < spikeStart ? 0.015 : 0.03);
      });
    }

    case "early-peak": {
      // Fast move in first 30%, then gradual drift to endNorm.
      const peakNorm = endNorm + (endNorm < 0.5 ? 0.3 : -0.3) + seededRand(seed) * 0.1;
      const peakAt = 0.25 + seededRand(seed + 1) * 0.1;
      return Array.from({ length: pointCount }, (_, i) => {
        const t = i / (pointCount - 1);
        const base =
          t < peakAt
            ? endNorm + (peakNorm - endNorm) * (1 - t / peakAt)
            : peakNorm + (endNorm - peakNorm) * ((t - peakAt) / (1 - peakAt));
        return base + noise(i, 0.02);
      });
    }

    case "s-curve": {
      // Slow start, fast middle, slow end (logistic-like).
      const startNorm = Math.max(0.05, endNorm - 0.4 - seededRand(seed) * 0.2);
      return Array.from({ length: pointCount }, (_, i) => {
        const t = i / (pointCount - 1);
        // Logistic S-curve mapped to [0,1]
        const s = 1 / (1 + Math.exp(-10 * (t - 0.5)));
        return startNorm + (endNorm - startNorm) * s + noise(i, 0.02);
      });
    }
  }
}

/**
 * Generate a deterministic mock price history for a market.
 *
 * Each market gets a consistent chart shape (one of 8 patterns) derived
 * from its market ID hash. The series always ends at the market's current
 * `yesPrice`, so the sparkline and featured chart are visually coherent
 * with the displayed probability.
 */
export function generateMockPriceHistory(
  market: Market,
  pointCount = 200,
): PriceHistoryEntry[] {
  const seed = hashCode(market.marketId);
  const shape = SHAPES[seed % SHAPES.length];
  // Default to 50% when no real price is known — chart shows "unresolved" state
  const endNorm = market.yesPrice ?? 0.5;

  const rawPath = shapePath(shape, endNorm, seed, pointCount);

  // Always cover exactly the last 7 days (10,080 blocks) relative to
  // currentHeight, so every timescale window (1h–7d) finds points to plot.
  // Spanning from block 0 to currentHeight would spread 200 points across
  // millions of blocks, leaving no points inside a 240-block (4h) window.
  const rangeBlocks = 10080;
  const startBlock = Math.max(0, (market.currentHeight || rangeBlocks) - rangeBlocks);

  return rawPath.map((norm, i) => {
    const bps = clampBps(norm * 10000);
    const blockHeight = Math.round(
      startBlock + (i / (pointCount - 1)) * rangeBlocks,
    );
    return {
      pool_id: `mock-pool-${market.marketId}`,
      market_id: market.marketId,
      transition_txid: `mock-tx-${market.marketId}-${i}`,
      old_s_index: i,
      new_s_index: i + 1,
      reserve_yes: 10000,
      reserve_no: 10000,
      reserve_collateral: 50000,
      implied_yes_price_bps: bps,
      block_height: blockHeight,
    };
  });
}
