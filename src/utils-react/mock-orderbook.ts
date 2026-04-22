import type { FullOrderbook, Market, OrderbookLevel } from "../types";
import { fullContractSats } from "./market";

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

/**
 * Generate a deterministic mock orderbook for a market side.
 *
 * Asks cluster above the current mid-price, bids cluster below it.
 * The spread, depth, and level sizes all vary deterministically per market so
 * each card looks distinct but stays stable across re-renders.
 */
export function generateMockOrderbook(market: Market): FullOrderbook {
  const seed = hashCode(market.marketId);
  const fc = fullContractSats(market);
  // Mid-price in sats based on current yes probability
  const midPriceSats = Math.round((market.yesPrice ?? 0.5) * fc);

  // Spread: 1–4% of full contract, at least 1 sat
  const spreadFraction = 0.01 + seededRand(seed) * 0.03;
  const halfSpread = Math.max(1, Math.round((spreadFraction * fc) / 2));

  const bestAsk = Math.min(fc - 1, midPriceSats + halfSpread);
  const bestBid = Math.max(1, midPriceSats - halfSpread);

  // Number of levels: 4–7 per side
  const askLevelCount = 4 + Math.floor(seededRand(seed + 1) * 4);
  const bidLevelCount = 4 + Math.floor(seededRand(seed + 2) * 4);

  // Tick size: 1–3 sats between levels
  const askTick = 1 + Math.floor(seededRand(seed + 3) * 3);
  const bidTick = 1 + Math.floor(seededRand(seed + 4) * 3);

  // Max contracts at the deepest level: 50–300
  const maxDepth = 50 + Math.floor(seededRand(seed + 5) * 250);

  const asks: OrderbookLevel[] = [];
  for (let i = 0; i < askLevelCount; i++) {
    const priceSats = Math.min(fc - 1, bestAsk + i * askTick);
    // Volume tapers away from best price (deeper = more liquidity)
    const depthFraction = 0.3 + (i / (askLevelCount - 1)) * 0.7;
    const contracts = Math.round(
      depthFraction * maxDepth * (0.7 + seededRand(seed + 10 + i) * 0.6),
    );
    if (contracts > 0) asks.push({ priceSats, contracts });
  }

  const bids: OrderbookLevel[] = [];
  for (let i = 0; i < bidLevelCount; i++) {
    const priceSats = Math.max(1, bestBid - i * bidTick);
    const depthFraction = 0.3 + (i / (bidLevelCount - 1)) * 0.7;
    const contracts = Math.round(
      depthFraction * maxDepth * (0.7 + seededRand(seed + 20 + i) * 0.6),
    );
    if (contracts > 0) bids.push({ priceSats, contracts });
  }

  // Sort: asks ascending (lowest ask first), bids descending (highest bid first)
  asks.sort((a, b) => a.priceSats - b.priceSats);
  bids.sort((a, b) => b.priceSats - a.priceSats);

  const spread =
    asks.length > 0 && bids.length > 0
      ? asks[0].priceSats - bids[0].priceSats
      : null;

  return { asks, bids, spread };
}
