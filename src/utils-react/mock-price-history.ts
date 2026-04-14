import type { Market, PriceHistoryEntry } from "../types";

/** Simple deterministic hash for seeding. */
function hashCode(s: string): number {
  let h = 0;
  for (let i = 0; i < s.length; i++) {
    h = (Math.imul(31, h) + s.charCodeAt(i)) | 0;
  }
  return Math.abs(h);
}

/**
 * Generate a deterministic mock price history for a market.
 * Creates a random-walk series ending at the market's current yesPrice,
 * spanning the block range leading up to currentHeight.
 */
export function generateMockPriceHistory(
  market: Market,
  pointCount = 40,
): PriceHistoryEntry[] {
  if (market.yesPrice == null) return [];

  const endBps = Math.round(market.yesPrice * 10000);
  const seed = hashCode(market.marketId);
  const startOffset = (seed % 2000) - 1000;
  const startBps = Math.max(200, Math.min(9800, endBps + startOffset));

  const rangeBlocks = Math.min(
    100,
    Math.max(10, market.currentHeight - (market.expiryHeight - 200)),
  );
  const startBlock = market.currentHeight - rangeBlocks;
  const entries: PriceHistoryEntry[] = [];

  for (let i = 0; i < pointCount; i++) {
    const t = i / (pointCount - 1);
    const base = startBps + (endBps - startBps) * t;
    const noise =
      ((hashCode(`${market.marketId}-${i}`) % 600) - 300) * (1 - t * 0.7);
    const bps = Math.max(100, Math.min(9900, Math.round(base + noise)));
    const blockHeight = Math.round(startBlock + rangeBlocks * t);

    entries.push({
      pool_id: `mock-pool-${market.marketId}`,
      market_id: market.marketId,
      transition_txid: `mock-tx-${i}`,
      old_s_index: i,
      new_s_index: i + 1,
      reserve_yes: 10000,
      reserve_no: 10000,
      reserve_collateral: 50000,
      implied_yes_price_bps: bps,
      block_height: blockHeight,
    });
  }

  return entries;
}
