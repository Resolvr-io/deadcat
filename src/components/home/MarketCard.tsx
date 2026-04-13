import { useCallback } from "react";
import { useStore } from "../../store";
import type { Market, Side, TradeIntent } from "../../types";
import {
  formatPercent,
  formatSatsInput,
  formatTimeRemaining,
  formatVolumeBtc,
} from "../../utils-react/format";
import {
  getBasePriceSats,
  getPositionContracts,
  setLimitPriceSats,
} from "../../utils-react/market";

// ── Inline SVG icons ─────────────────────────────────────────────────
function TrendUpIcon() {
  return (
    <svg
      aria-hidden="true"
      xmlns="http://www.w3.org/2000/svg"
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className="shrink-0"
    >
      <polyline points="22 7 13.5 15.5 8.5 10.5 2 17" />
      <polyline points="16 7 22 7 22 13" />
    </svg>
  );
}

function TrendDownIcon() {
  return (
    <svg
      aria-hidden="true"
      xmlns="http://www.w3.org/2000/svg"
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className="shrink-0"
    >
      <polyline points="22 17 13.5 8.5 8.5 13.5 2 7" />
      <polyline points="16 17 22 17 22 11" />
    </svg>
  );
}

function TrendIndicator({ change }: { change: number }) {
  const color = change >= 0 ? "text-emerald-300" : "text-rose-300";
  return (
    <span className={`inline-flex items-center gap-1 ${color}`}>
      {change >= 0 ? <TrendUpIcon /> : <TrendDownIcon />}
      {formatPercent(change)}
    </span>
  );
}

// ── Open-market action ───────────────────────────────────────────────
function openMarket(
  market: Market,
  options?: { side?: Side; intent?: TradeIntent },
) {
  const walletData = useStore.getState().walletData;
  const nextSide: Side = options?.side ?? "yes";
  const nextIntent: TradeIntent = options?.intent ?? "open";
  const positions = getPositionContracts(market, walletData);
  const selectedPosition = nextSide === "yes" ? positions.yes : positions.no;
  const limitPriceResult = setLimitPriceSats(
    market,
    getBasePriceSats(market, nextSide),
  );

  useStore.setState({
    selectedMarketId: market.id,
    view: "detail",
    selectedSide: nextSide,
    orderType: "market",
    actionTab: "trade",
    tradeIntent: nextIntent,
    sizeMode: nextIntent === "close" ? "contracts" : "sats",
    showAdvancedDetails: false,
    showAdvancedActions: false,
    showOrderbook: false,
    showFeeDetails: false,
    tradeQuoteLoading: false,
    tradeExecuteLoading: false,
    tradeQuoteSnapshot: null,
    tradeError: null,
    tradeSizeSats: 10000,
    tradeSizeSatsDraft: formatSatsInput(10000),
    tradeContracts:
      nextIntent === "close"
        ? Math.max(0.01, Math.min(selectedPosition, selectedPosition / 2))
        : 10,
    tradeContractsDraft:
      nextIntent === "close"
        ? Math.max(
            0.01,
            Math.min(selectedPosition, selectedPosition / 2),
          ).toFixed(2)
        : "10",
    chartHoverMarketId: null,
    chartHoverX: null,
    limitPrice: limitPriceResult.limitPrice,
    limitPriceDraft: limitPriceResult.limitPriceDraft,
  });
}

// ── Chart sparkline placeholder ──────────────────────────────────────
function ChartSparkline() {
  return (
    <svg
      aria-hidden="true"
      viewBox="0 0 80 24"
      fill="none"
      className="h-6 w-20 text-emerald-400/40"
    >
      <polyline
        points="0,18 10,14 20,16 30,10 40,12 50,6 60,8 70,4 80,6"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
        fill="none"
      />
    </svg>
  );
}

// ── MarketCard component ─────────────────────────────────────────────
export default function MarketCard({
  market,
  index,
}: {
  market: Market;
  index: number;
}) {
  const no = market.yesPrice != null ? 1 - market.yesPrice : null;
  const yesPct =
    market.yesPrice != null ? Math.round(market.yesPrice * 100) : null;
  const noPct = no != null ? Math.round(no * 100) : null;
  const blocksLeft = market.expiryHeight - market.currentHeight;
  const timeLeft = blocksLeft > 0 ? formatTimeRemaining(blocksLeft) : "Expired";

  const handleOpenMarket = useCallback(() => {
    openMarket(market);
  }, [market]);

  const handleBuyYes = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      openMarket(market, { side: "yes", intent: "open" });
    },
    [market],
  );

  const handleBuyNo = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      openMarket(market, { side: "no", intent: "open" });
    },
    [market],
  );

  // ── Resolved / expired badge ─────────────────────────────────────
  const resolvedBadge =
    market.state === 2 || market.state === 3 ? (
      <span
        className={`w-48 rounded-full ${market.state === 2 ? "bg-emerald-500/20 text-emerald-300" : "bg-rose-500/20 text-rose-300"} px-3 py-1 text-center text-sm font-medium`}
      >
        {market.state === 2 ? "Resolved YES" : "Resolved NO"}
      </span>
    ) : market.state === 4 ? (
      <span className="w-48 rounded-full bg-amber-500/20 px-3 py-1 text-center text-sm font-medium text-amber-300">
        Expired
      </span>
    ) : null;

  return (
    <div
      className="market-card rounded-2xl border border-slate-800 bg-slate-950/55 p-4 text-left transition hover:border-slate-600"
      style={{ animationDelay: `${index * 50}ms` }}
    >
      <button
        type="button"
        onClick={handleOpenMarket}
        className="w-full text-left"
      >
        <div className="mb-2 flex items-center justify-between text-xs">
          <span className="text-slate-500">
            {market.category}
            {market.isLive && <span className="text-emerald-400"> · LIVE</span>}
          </span>
          <span className="text-slate-500">{timeLeft}</span>
        </div>
        <p className="mb-2 max-h-12 overflow-hidden text-sm font-normal text-slate-200">
          {market.question}
        </p>
        {yesPct != null && (
          <p className="mb-2 text-2xl font-semibold text-slate-100">
            {yesPct}
            <span className="text-base text-slate-400">%</span>
          </p>
        )}
      </button>
      <div className="flex items-center gap-2">
        {resolvedBadge ?? (
          <>
            <button
              type="button"
              onClick={handleBuyYes}
              className="w-24 rounded-full bg-emerald-500 px-3 py-1 text-center text-sm font-medium text-white transition hover:bg-emerald-400"
            >
              {yesPct != null ? `Yes ${yesPct}%` : "Buy Yes"}
            </button>
            <button
              type="button"
              onClick={handleBuyNo}
              className="w-24 rounded-full bg-rose-500 px-3 py-1 text-center text-sm font-medium text-white transition hover:bg-rose-400"
            >
              {noPct != null ? `No ${noPct}%` : "Buy No"}
            </button>
          </>
        )}
        <span className="ml-auto text-xs">
          <TrendIndicator change={market.change24h} />
        </span>
      </div>
      <p className="mt-2 text-xs text-slate-500">
        {formatVolumeBtc(market.volumeBtc)} vol · {timeLeft}
      </p>
    </div>
  );
}

export { ChartSparkline, openMarket, TrendIndicator };
