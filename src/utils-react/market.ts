import type {
  CovenantState,
  DiscoveredOrder,
  FillEstimate,
  FullOrderbook,
  LimitSellWarning,
  Market,
  OrderbookLevel,
  OrderType,
  PathAvailability,
  QuoteMarketTradeResult,
  Side,
  TradeIntent,
  TradePreview,
  WalletData,
} from "../types";
import { reverseHex } from "../utils/crypto";
import { formatSatsInput } from "./format";

export function stateLabel(value: CovenantState): string {
  if (value === 0) return "DORMANT";
  if (value === 1) return "UNRESOLVED";
  if (value === 2) return "RESOLVED YES";
  if (value === 3) return "RESOLVED NO";
  return "EXPIRED";
}

export function isExpired(market: Market): boolean {
  return market.currentHeight >= market.expiryHeight;
}

export function getEstimatedSettlementDate(market: Market): Date {
  const blocksRemaining = market.expiryHeight - market.currentHeight;
  return new Date(Date.now() + blocksRemaining * 60 * 1000);
}

export function getPathAvailability(market: Market): PathAvailability {
  const expired = isExpired(market);
  return {
    initialIssue: market.state === 0,
    issue: market.state === 1,
    resolve: market.state === 1,
    redeem: market.state === 2 || market.state === 3,
    expiryRedeem: market.state === 4 || (market.state === 1 && expired),
    cancel: market.state === 1,
  };
}

export function getMarketById(marketId: string, markets: Market[]): Market {
  return markets.find((market) => market.id === marketId) ?? markets[0];
}

function isSettled(market: Market): boolean {
  return market.state === 2 || market.state === 3 || market.state === 4;
}

export function getTrendingMarkets(markets: Market[]): Market[] {
  return markets.filter((m) => !isSettled(m)).slice(0, 7);
}

export function fullContractSats(market: Market): number {
  return 2 * market.cptSats;
}

export function clampContractPriceSats(
  value: number,
  fullContract: number,
): number {
  return Math.max(1, Math.min(fullContract - 1, Math.round(value)));
}

export function getBasePriceSats(market: Market, side: Side): number {
  const fc = fullContractSats(market);
  const raw =
    side === "yes" ? (market.yesPrice ?? 0.5) : 1 - (market.yesPrice ?? 0.5);
  return clampContractPriceSats(raw * fc, fc);
}

export function getMarketSeed(market: Market): number {
  return [...market.id].reduce((sum, ch) => sum + ch.charCodeAt(0), 0);
}

export function getPositionContracts(
  market: Market,
  walletData: WalletData | null,
): { yes: number; no: number } {
  const balance = walletData?.balance;
  if (!balance) return { yes: 0, no: 0 };
  const yesKey = reverseHex(market.yesAssetId);
  const noKey = reverseHex(market.noAssetId);
  return {
    yes: balance[yesKey] ?? 0,
    no: balance[noKey] ?? 0,
  };
}

function isMarketOrderForSide(
  market: Market,
  side: Side,
  order: DiscoveredOrder,
): boolean {
  const sideAssetId = side === "yes" ? market.yesAssetId : market.noAssetId;
  return (
    order.base_asset_id.toLowerCase() === sideAssetId.toLowerCase() &&
    order.quote_asset_id.toLowerCase() ===
      market.collateralAssetId.toLowerCase()
  );
}

export function getLimitOrdersForSide(
  market: Market,
  side: Side,
): DiscoveredOrder[] {
  return market.limitOrders
    .filter((order) => isMarketOrderForSide(market, side, order))
    .sort((a, b) => b.created_at - a.created_at);
}

export function getAvailableOrderContracts(order: DiscoveredOrder): number {
  if (order.price <= 0) return 0;
  if (order.direction === "sell-base") {
    return Math.max(0, Math.floor(order.offered_amount));
  }
  return Math.max(0, Math.floor(order.offered_amount / order.price));
}

export function getOrderbookLevels(
  market: Market,
  side: Side,
  intent: TradeIntent,
): OrderbookLevel[] {
  return getDiscoveredOrderbookLevels(market, side, intent);
}

export function getDiscoveredOrderbookLevels(
  market: Market,
  side: Side,
  intent: TradeIntent,
): OrderbookLevel[] {
  const targetDirection = intent === "open" ? "sell-base" : "sell-quote";
  const grouped = new Map<number, number>();

  for (const order of getLimitOrdersForSide(market, side)) {
    if (order.source === "recovered-local") continue;
    if (order.direction !== targetDirection) continue;
    if (!Number.isFinite(order.price) || order.price <= 0) continue;
    const contracts = getAvailableOrderContracts(order);
    if (contracts <= 0) continue;
    const priceSats = clampContractPriceSats(
      order.price,
      fullContractSats(market),
    );
    grouped.set(priceSats, (grouped.get(priceSats) ?? 0) + contracts);
  }

  if (grouped.size > 0) {
    const sorted = Array.from(grouped.entries())
      .map(([priceSats, contracts]) => ({ priceSats, contracts }))
      .sort((a, b) =>
        intent === "open"
          ? a.priceSats - b.priceSats
          : b.priceSats - a.priceSats,
      );
    return sorted.slice(0, 8);
  }

  return [];
}

export function getFullOrderbook(market: Market, side: Side): FullOrderbook {
  const asks = getDiscoveredOrderbookLevels(market, side, "open");
  const bids = getDiscoveredOrderbookLevels(market, side, "close");
  asks.sort((a, b) => a.priceSats - b.priceSats);
  bids.sort((a, b) => b.priceSats - a.priceSats);
  const bestAsk = asks[0]?.priceSats ?? null;
  const bestBid = bids[0]?.priceSats ?? null;
  const spread =
    bestAsk !== null && bestBid !== null ? bestAsk - bestBid : null;
  return { asks, bids, spread };
}

export function getLimitSellWarning(
  limitPriceSats: number,
  referencePriceSats: number | null,
): LimitSellWarning | null {
  if (
    referencePriceSats === null ||
    !Number.isFinite(referencePriceSats) ||
    referencePriceSats <= 0
  ) {
    return null;
  }
  if (limitPriceSats >= referencePriceSats) return null;
  const discountSats = referencePriceSats - limitPriceSats;
  const discountPct = (discountSats / referencePriceSats) * 100;
  return { referencePriceSats, discountSats, discountPct };
}

export function getSellQuoteReferencePriceSats(
  quote: Pick<
    QuoteMarketTradeResult,
    "direction" | "total_input" | "total_output"
  >,
): number | null {
  if (quote.direction !== "sell") return null;
  if (!Number.isFinite(quote.total_input) || quote.total_input <= 0)
    return null;
  if (!Number.isFinite(quote.total_output) || quote.total_output <= 0)
    return null;
  return quote.total_output / quote.total_input;
}

export function getLimitSellWarningFromSellQuote(
  limitPriceSats: number,
  quote: Pick<
    QuoteMarketTradeResult,
    "direction" | "total_input" | "total_output"
  >,
): LimitSellWarning | null {
  const referencePriceSats = getSellQuoteReferencePriceSats(quote);
  return getLimitSellWarning(limitPriceSats, referencePriceSats);
}

export function getQuoteEffectivePriceSatsPerContract(
  quote: Pick<
    QuoteMarketTradeResult,
    "direction" | "total_input" | "total_output"
  >,
): number | null {
  if (quote.direction === "buy") {
    if (!Number.isFinite(quote.total_output) || quote.total_output <= 0)
      return null;
    return quote.total_input / quote.total_output;
  }
  if (!Number.isFinite(quote.total_input) || quote.total_input <= 0)
    return null;
  return quote.total_output / quote.total_input;
}

export function getQuoteEffectivePriceContractsPerSat(
  quote: Pick<
    QuoteMarketTradeResult,
    "direction" | "total_input" | "total_output"
  >,
): number | null {
  const satsPerContract = getQuoteEffectivePriceSatsPerContract(quote);
  if (!Number.isFinite(satsPerContract) || satsPerContract === null)
    return null;
  if (satsPerContract <= 0) return null;
  return 1 / satsPerContract;
}

export function getQuoteRemainingSeconds(
  expiresAtUnix: number,
  nowUnix: number,
): number {
  return Math.max(0, Math.floor(expiresAtUnix) - Math.floor(nowUnix));
}

export function estimateFill(
  levels: OrderbookLevel[],
  requestedContracts: number,
  intent: TradeIntent,
  orderType: OrderType,
  limitPriceSats: number,
): FillEstimate {
  const request = Math.max(0, requestedContracts);
  const executable = levels.filter((level) =>
    orderType === "market"
      ? true
      : intent === "open"
        ? level.priceSats <= limitPriceSats
        : level.priceSats >= limitPriceSats,
  );

  let remaining = request;
  let totalSats = 0;
  let totalContracts = 0;
  const bestPrice = executable[0]?.priceSats ?? limitPriceSats;
  let worstPrice = bestPrice;

  for (const level of executable) {
    if (remaining <= 0) break;
    const take = Math.min(remaining, level.contracts);
    totalContracts += take;
    totalSats += take * level.priceSats;
    worstPrice = level.priceSats;
    remaining -= take;
  }

  const avgPriceSats =
    totalContracts > 0 ? totalSats / totalContracts : limitPriceSats;

  return {
    avgPriceSats,
    bestPriceSats: bestPrice,
    worstPriceSats: worstPrice,
    filledContracts: totalContracts,
    requestedContracts: request,
    totalSats: Math.round(totalSats),
    isPartial: totalContracts + 0.0001 < request,
  };
}

/** All parameters passed explicitly — no global state reads. */
export function getTradePreview(
  market: Market,
  tradeState: {
    selectedSide: Side;
    orderType: OrderType;
    tradeIntent: TradeIntent;
    sizeMode: "sats" | "contracts";
    limitPrice: number;
    tradeSizeSats: number;
    tradeContracts: number;
  },
  walletData: WalletData | null,
): TradePreview {
  const fc = fullContractSats(market);
  const limitPriceSats = clampContractPriceSats(tradeState.limitPrice * fc, fc);
  const basePriceSats = getBasePriceSats(market, tradeState.selectedSide);
  const levels = getOrderbookLevels(
    market,
    tradeState.selectedSide,
    tradeState.tradeIntent,
  );
  const referencePriceSats =
    tradeState.orderType === "limit" ? limitPriceSats : basePriceSats;
  const requestedContracts =
    tradeState.sizeMode === "contracts"
      ? tradeState.tradeIntent === "open"
        ? Math.max(1, Math.floor(tradeState.tradeContracts))
        : Math.max(0, Math.floor(tradeState.tradeContracts))
      : Math.max(1, tradeState.tradeSizeSats) / Math.max(1, referencePriceSats);
  const fill = estimateFill(
    levels,
    requestedContracts,
    tradeState.tradeIntent,
    tradeState.orderType,
    limitPriceSats,
  );
  const executionPriceSats =
    tradeState.orderType === "market"
      ? Math.max(1, fill.avgPriceSats)
      : limitPriceSats;
  const notionalSats =
    tradeState.sizeMode === "sats"
      ? Math.max(1, Math.floor(tradeState.tradeSizeSats))
      : Math.max(0, Math.round(requestedContracts * referencePriceSats));
  const executedSats = Math.max(0, fill.totalSats);
  const grossPayoutSats = Math.floor(fill.filledContracts * fc);
  const slippagePct =
    fill.bestPriceSats > 0
      ? Math.max(
          0,
          ((fill.worstPriceSats - fill.bestPriceSats) / fill.bestPriceSats) *
            100,
        )
      : 0;
  const position = getPositionContracts(market, walletData);
  const positionContracts =
    tradeState.selectedSide === "yes" ? position.yes : position.no;

  return {
    basePriceSats,
    limitPriceSats,
    referencePriceSats,
    requestedContracts,
    fill,
    executionPriceSats,
    notionalSats,
    executedSats,
    grossPayoutSats,
    slippagePct,
    positionContracts,
  };
}

export function commitTradeSizeSatsDraft(draft: string): {
  tradeSizeSats: number;
  tradeSizeSatsDraft: string;
} {
  const sanitized = draft.replace(/,/g, "");
  const parsed = Math.floor(Number(sanitized) || 1);
  const clamped = Math.max(1, parsed);
  return {
    tradeSizeSats: clamped,
    tradeSizeSatsDraft: formatSatsInput(clamped),
  };
}

export function commitTradeContractsDraft(
  draft: string,
  market: Market,
  selectedSide: Side,
  tradeIntent: TradeIntent,
  walletData: WalletData | null,
): { tradeContracts: number; tradeContractsDraft: string } {
  const positions = getPositionContracts(market, walletData);
  const available = selectedSide === "yes" ? positions.yes : positions.no;
  const parsed = Math.floor(Number(draft));
  const isSell = tradeIntent === "close";
  const base = Number.isFinite(parsed) ? parsed : isSell ? 0 : 1;
  const normalized = isSell ? Math.max(0, base) : Math.max(1, base);
  const availableLots = Math.max(0, Math.floor(available));
  const clamped = isSell ? Math.min(normalized, availableLots) : normalized;
  return { tradeContracts: clamped, tradeContractsDraft: String(clamped) };
}

export function setLimitPriceSats(
  market: Market,
  limitPriceSats: number,
): { limitPrice: number; limitPriceDraft: string } {
  const fc = fullContractSats(market);
  const clampedSats = clampContractPriceSats(limitPriceSats, fc);
  return {
    limitPrice: clampedSats / fc,
    limitPriceDraft: String(clampedSats),
  };
}

export function commitLimitPriceDraft(
  draft: string,
  market: Market,
  currentLimitPrice: number,
): { limitPrice: number; limitPriceDraft: string } {
  const fc = fullContractSats(market);
  const sanitized = draft.replace(/[^\d]/g, "");
  if (sanitized.length === 0) {
    return {
      limitPrice: currentLimitPrice,
      limitPriceDraft: String(
        clampContractPriceSats(currentLimitPrice * fc, fc),
      ),
    };
  }
  return setLimitPriceSats(market, Math.floor(Number(sanitized)));
}

export function getEndingSoonMarkets(markets: Market[]): Market[] {
  return markets
    .filter((m) => m.isLive && m.expiryHeight > m.currentHeight)
    .sort((a, b) => {
      const aLeft = a.expiryHeight - a.currentHeight;
      const bLeft = b.expiryHeight - b.currentHeight;
      return aLeft - bLeft;
    });
}

export function getNewMarkets(markets: Market[]): Market[] {
  return [...markets].sort((a, b) => b.expiryHeight - a.expiryHeight);
}

export function getFilteredMarkets(
  markets: Market[],
  search: string,
  activeCategory: import("../types").NavCategory,
  nostrPubkey: string | null,
  walletData?: WalletData | null,
): Market[] {
  const lowered = search.trim().toLowerCase();
  return markets
    .filter((market) => {
      const settled = isSettled(market);
      if (activeCategory === "Resolved") return settled;
      if (activeCategory === "My Markets") {
        return nostrPubkey != null && market.oraclePubkey === nostrPubkey;
      }
      if (activeCategory === "Portfolio") {
        const pos = getPositionContracts(market, walletData ?? null);
        return pos.yes > 0 || pos.no > 0;
      }
      if (settled) return false;
      const categoryMatch =
        activeCategory === "Trending" ||
        activeCategory === "Ending Soon" ||
        activeCategory === "New" ||
        market.category === activeCategory;
      const searchMatch =
        lowered.length === 0 ||
        market.question.toLowerCase().includes(lowered) ||
        market.category.toLowerCase().includes(lowered);
      return categoryMatch && searchMatch;
    })
    .sort((a, b) => b.volumeBtc - a.volumeBtc);
}
