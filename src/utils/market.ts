import {
  EXECUTION_FEE_RATE,
  markets,
  SATS_PER_FULL_CONTRACT,
  state,
  WIN_FEE_RATE,
} from "../state.ts";
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
} from "../types.ts";
import { reverseHex } from "./crypto.ts";
import { formatSatsInput } from "./format.ts";

export function stateLabel(value: CovenantState): string {
  if (value === 0) return "DORMANT";
  if (value === 1) return "UNRESOLVED";
  if (value === 2) return "RESOLVED YES";
  if (value === 3) return "RESOLVED NO";
  return "EXPIRED";
}

export function stateBadge(value: CovenantState): string {
  const label = stateLabel(value);
  const colors =
    value === 0
      ? "bg-slate-600/30 text-slate-300"
      : value === 1
        ? "bg-emerald-500/20 text-emerald-300"
        : value === 2
          ? "bg-emerald-500/30 text-emerald-200"
          : value === 3
            ? "bg-rose-500/30 text-rose-200"
            : "bg-amber-500/25 text-amber-200";
  return `<span class="rounded-full px-2.5 py-0.5 text-xs font-medium ${colors}">${label}</span>`;
}

export function isExpired(market: Market): boolean {
  return market.currentHeight >= market.expiryHeight;
}

export function getEstimatedSettlementDate(market: Market): Date {
  const blocksRemaining = market.expiryHeight - market.currentHeight;
  const minutesPerBlock = 1;
  return new Date(Date.now() + blocksRemaining * minutesPerBlock * 60 * 1000);
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

export function getMarketById(marketId: string): Market {
  return markets.find((market) => market.id === marketId) ?? markets[0];
}

export function getSelectedMarket(): Market {
  return getMarketById(state.selectedMarketId);
}

export function getTrendingMarkets(): Market[] {
  return markets.slice(0, 7);
}

export function clampContractPriceSats(value: number): number {
  return Math.max(1, Math.min(SATS_PER_FULL_CONTRACT - 1, Math.round(value)));
}

export function getBasePriceSats(market: Market, side: Side): number {
  const raw =
    side === "yes" ? (market.yesPrice ?? 0.5) : 1 - (market.yesPrice ?? 0.5);
  return clampContractPriceSats(raw * SATS_PER_FULL_CONTRACT);
}

export function getMarketSeed(market: Market): number {
  return [...market.id].reduce((sum, ch) => sum + ch.charCodeAt(0), 0);
}

export function getPositionContracts(market: Market): {
  yes: number;
  no: number;
} {
  const balance = state.walletData?.balance;
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
    const priceSats = clampContractPriceSats(order.price);
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

  // asks: ascending (best ask = lowest price first)
  asks.sort((a, b) => a.priceSats - b.priceSats);
  // bids: descending (best bid = highest price first)
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

export function resetLimitSellWarningState(): void {
  state.limitSellOverrideAccepted = false;
  state.limitSellWarning = null;
  state.limitSellWarningInfo = "";
  state.limitSellGuardChecking = false;
  state.limitSellGuardVersion += 1;
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

export function getTradePreview(market: Market): TradePreview {
  const limitPriceSats = clampContractPriceSats(
    state.limitPrice * SATS_PER_FULL_CONTRACT,
  );
  const basePriceSats = getBasePriceSats(market, state.selectedSide);
  const levels = getOrderbookLevels(
    market,
    state.selectedSide,
    state.tradeIntent,
  );
  const referencePriceSats =
    state.orderType === "limit" ? limitPriceSats : basePriceSats;
  const requestedContracts =
    state.sizeMode === "contracts"
      ? state.tradeIntent === "open"
        ? Math.max(1, Math.floor(state.tradeContracts))
        : Math.max(0, Math.floor(state.tradeContracts))
      : Math.max(1, state.tradeSizeSats) / Math.max(1, referencePriceSats);
  const fill = estimateFill(
    levels,
    requestedContracts,
    state.tradeIntent,
    state.orderType,
    limitPriceSats,
  );
  const executionPriceSats =
    state.orderType === "market"
      ? Math.max(1, fill.avgPriceSats)
      : limitPriceSats;
  const notionalSats =
    state.sizeMode === "sats"
      ? Math.max(1, Math.floor(state.tradeSizeSats))
      : Math.max(0, Math.round(requestedContracts * referencePriceSats));
  const executedSats = Math.max(0, fill.totalSats);
  const executionFeeSats = Math.round(executedSats * EXECUTION_FEE_RATE);
  const grossPayoutSats = Math.floor(
    fill.filledContracts * SATS_PER_FULL_CONTRACT,
  );
  const grossProfitSats = Math.max(0, grossPayoutSats - executedSats);
  const winFeeSats =
    state.tradeIntent === "open"
      ? Math.round(grossProfitSats * WIN_FEE_RATE)
      : 0;
  const netIfCorrectSats = Math.max(
    0,
    grossPayoutSats - executionFeeSats - winFeeSats,
  );
  const maxProfitSats = Math.max(0, netIfCorrectSats - executedSats);
  const netAfterFeesSats = Math.max(0, executedSats - executionFeeSats);
  const slippagePct =
    fill.bestPriceSats > 0
      ? Math.max(
          0,
          ((fill.worstPriceSats - fill.bestPriceSats) / fill.bestPriceSats) *
            100,
        )
      : 0;
  const position = getPositionContracts(market);
  const positionContracts =
    state.selectedSide === "yes" ? position.yes : position.no;

  return {
    basePriceSats,
    limitPriceSats,
    referencePriceSats,
    requestedContracts,
    fill,
    executionPriceSats,
    notionalSats,
    executedSats,
    executionFeeSats,
    winFeeSats,
    grossPayoutSats,
    netIfCorrectSats,
    maxProfitSats,
    netAfterFeesSats,
    slippagePct,
    positionContracts,
  };
}

export function commitTradeSizeSatsDraft(): void {
  const sanitized = state.tradeSizeSatsDraft.replace(/,/g, "");
  const parsed = Math.floor(Number(sanitized) || 1);
  const clamped = Math.max(1, parsed);
  state.tradeSizeSats = clamped;
  state.tradeSizeSatsDraft = formatSatsInput(clamped);
}

export function commitTradeContractsDraft(market: Market): void {
  const positions = getPositionContracts(market);
  const available = state.selectedSide === "yes" ? positions.yes : positions.no;
  const parsed = Math.floor(Number(state.tradeContractsDraft));
  const isSell = state.tradeIntent === "close";
  const base = Number.isFinite(parsed) ? parsed : isSell ? 0 : 1;
  const normalized = isSell ? Math.max(0, base) : Math.max(1, base);
  const availableLots = Math.max(0, Math.floor(available));
  const clamped = isSell ? Math.min(normalized, availableLots) : normalized;
  state.tradeContracts = clamped;
  state.tradeContractsDraft = String(clamped);
}

export function setLimitPriceSats(limitPriceSats: number): void {
  const clampedSats = clampContractPriceSats(limitPriceSats);
  state.limitPrice = clampedSats / SATS_PER_FULL_CONTRACT;
  state.limitPriceDraft = String(clampedSats);
}

export function commitLimitPriceDraft(): void {
  const sanitized = state.limitPriceDraft.replace(/[^\d]/g, "");
  if (sanitized.length === 0) {
    state.limitPriceDraft = String(
      clampContractPriceSats(state.limitPrice * SATS_PER_FULL_CONTRACT),
    );
    return;
  }
  setLimitPriceSats(Math.floor(Number(sanitized)));
}

export function getFilteredMarkets(): Market[] {
  const lowered = state.search.trim().toLowerCase();
  return markets
    .filter((market) => {
      const categoryMatch =
        state.activeCategory === "Trending" ||
        (state.activeCategory === "My Markets"
          ? state.nostrPubkey != null &&
            market.oraclePubkey === state.nostrPubkey
          : market.category === state.activeCategory);
      const searchMatch =
        lowered.length === 0 ||
        market.question.toLowerCase().includes(lowered) ||
        market.category.toLowerCase().includes(lowered);
      return categoryMatch && searchMatch;
    })
    .sort((a, b) => b.volumeBtc - a.volumeBtc);
}
