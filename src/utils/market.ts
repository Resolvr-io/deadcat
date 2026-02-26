import {
  EXECUTION_FEE_RATE,
  markets,
  SATS_PER_FULL_CONTRACT,
  state,
  WIN_FEE_RATE,
} from "../state.ts";
import type {
  CovenantState,
  FillEstimate,
  Market,
  OrderbookLevel,
  OrderType,
  PathAvailability,
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
  return "RESOLVED NO";
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
          : "bg-rose-500/30 text-rose-200";
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
    issue: market.state === 1 && !expired,
    resolve: market.state === 1 && !expired,
    redeem: market.state === 2 || market.state === 3,
    expiryRedeem: market.state === 1 && expired,
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
  const raw = side === "yes" ? market.yesPrice : 1 - market.yesPrice;
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

export function getOrderbookLevels(
  market: Market,
  side: Side,
  intent: TradeIntent,
): OrderbookLevel[] {
  const seed = getMarketSeed(market);
  const base = getBasePriceSats(market, side);
  return Array.from({ length: 8 }).map((_, idx) => {
    const offset = intent === "open" ? idx + 1 : -(idx + 1);
    const priceSats = clampContractPriceSats(base + offset);
    const contracts = 12 + ((seed + idx * 11) % 34);
    return { priceSats, contracts };
  });
}

export function estimateFill(
  levels: OrderbookLevel[],
  requestedContracts: number,
  intent: TradeIntent,
  orderType: OrderType,
  limitPriceSats: number,
): FillEstimate {
  const request = Math.max(0.01, requestedContracts);
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
      ? Math.max(0.01, state.tradeContracts)
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
      : Math.max(1, Math.round(requestedContracts * referencePriceSats));
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
  const parsed = Number(state.tradeContractsDraft);
  const base = Number.isFinite(parsed) ? parsed : 0.01;
  const normalized = Math.max(0.01, base);
  const clamped =
    state.tradeIntent === "close"
      ? Math.min(normalized, available)
      : normalized;
  state.tradeContracts = clamped;
  state.tradeContractsDraft = clamped.toFixed(2);
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
