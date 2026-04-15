import { useCallback } from "react";
import { usePriceHistory } from "../../queries/usePools";
import { useStore } from "../../store";
import type { Market } from "../../types";
import { formatTimeRemaining, formatVolumeBtc } from "../../utils-react/format";
import { stateLabel } from "../../utils-react/market";
import { generateMockPriceHistory } from "../../utils-react/mock-price-history";
import MarketChart from "../chart/MarketChart";
import { openMarket, TrendIndicator } from "./MarketCard";

// ── State badge (React version) ──────────────────────────────────────
function StateBadge({ state }: { state: 0 | 1 | 2 | 3 | 4 }) {
  const label = stateLabel(state);
  const colors =
    state === 0
      ? "bg-slate-600/30 text-slate-300"
      : state === 1
        ? "bg-sky-500/20 text-sky-300"
        : state === 2
          ? "bg-emerald-500/30 text-emerald-200"
          : state === 3
            ? "bg-rose-500/30 text-rose-200"
            : "bg-amber-500/25 text-amber-200";
  return (
    <span
      className={`rounded-full px-2.5 py-0.5 text-xs font-medium ${colors}`}
    >
      {label}
    </span>
  );
}

// ── FeaturedMarket component ─────────────────────────────────────────
export default function FeaturedMarket({
  market,
  trending,
}: {
  market: Market;
  trending: Market[];
}) {
  const trendingIndex = useStore((s) => s.trendingIndex);
  const { data: rawPriceHistory } = usePriceHistory(market.marketId);
  const priceHistory =
    rawPriceHistory && rawPriceHistory.length > 0
      ? rawPriceHistory
      : generateMockPriceHistory(market);

  const featuredNo = market.yesPrice != null ? 1 - market.yesPrice : null;
  const featuredYesPct =
    market.yesPrice != null ? Math.round(market.yesPrice * 100) : null;
  const featuredNoPct =
    featuredNo != null ? Math.round(featuredNo * 100) : null;
  const blocksLeft = market.expiryHeight - market.currentHeight;
  const timeLeft = blocksLeft > 0 ? formatTimeRemaining(blocksLeft) : "Expired";

  const handlePrev = useCallback(() => {
    useStore.setState({
      trendingIndex: (trendingIndex - 1 + trending.length) % trending.length,
    });
  }, [trendingIndex, trending.length]);

  const handleNext = useCallback(() => {
    useStore.setState({
      trendingIndex: (trendingIndex + 1) % trending.length,
    });
  }, [trendingIndex, trending.length]);

  const handleOpenMarket = useCallback(() => {
    openMarket(market);
  }, [market]);

  const handleBuyYes = useCallback(() => {
    openMarket(market, { side: "yes", intent: "open" });
  }, [market]);

  const handleBuyNo = useCallback(() => {
    openMarket(market, { side: "no", intent: "open" });
  }, [market]);

  // ── Resolved / expired action area ─────────────────────────────────
  const resolvedActions =
    market.state === 2 || market.state === 3 ? (
      <span
        className={`w-40 rounded-full ${market.state === 2 ? "bg-emerald-500/20 text-emerald-300" : "bg-rose-500/20 text-rose-300"} px-4 py-2 text-center text-base font-semibold`}
      >
        {market.state === 2 ? "Resolved YES" : "Resolved NO"}
      </span>
    ) : market.state === 4 ? (
      <span className="w-40 rounded-full bg-amber-500/20 px-4 py-2 text-center text-base font-semibold text-amber-300">
        Expired
      </span>
    ) : null;

  return (
    <div className="rounded-[21px] border border-slate-800 bg-slate-950/60 p-[21px] lg:p-[34px]">
      {/* Header row: category + nav arrows */}
      <div className="mb-4 flex items-center justify-between gap-3">
        <div className="flex items-center gap-2 text-xs">
          <span className="text-slate-500">{market.category}</span>
          {market.isLive && (
            <span className="rounded-full bg-emerald-500/20 px-2 py-0.5 text-emerald-300">
              LIVE
            </span>
          )}
          <StateBadge state={market.state} />
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={handlePrev}
            className="h-9 w-9 rounded-full border border-slate-700 text-lg text-slate-300 hover:bg-slate-800"
          >
            &#8249;
          </button>
          <p className="w-16 text-center text-xs text-slate-400">
            {trendingIndex + 1} of {trending.length}
          </p>
          <button
            type="button"
            onClick={handleNext}
            className="h-9 w-9 rounded-full border border-slate-700 text-lg text-slate-300 hover:bg-slate-800"
          >
            &#8250;
          </button>
        </div>
      </div>

      {/* Question / title */}
      <button
        type="button"
        onClick={handleOpenMarket}
        className="mb-4 block text-left"
      >
        <h1 className="phi-title text-2xl font-medium leading-tight text-slate-100 transition hover:text-white lg:text-[34px]">
          {market.question}
        </h1>
      </button>

      {/* Action buttons + meta */}
      <div className="mb-4 flex flex-wrap items-center gap-3">
        {resolvedActions ?? (
          <>
            <button
              type="button"
              onClick={handleBuyYes}
              className="w-32 rounded-full bg-emerald-500 px-4 py-2 text-center text-base font-semibold text-white transition hover:bg-emerald-400"
            >
              {featuredYesPct != null ? `Yes ${featuredYesPct}%` : "Buy Yes"}
            </button>
            <button
              type="button"
              onClick={handleBuyNo}
              className="w-32 rounded-full bg-rose-500 px-4 py-2 text-center text-base font-semibold text-white transition hover:bg-rose-400"
            >
              {featuredNoPct != null ? `No ${featuredNoPct}%` : "Buy No"}
            </button>
          </>
        )}
        <span className="text-xs text-slate-500">
          {formatVolumeBtc(market.volumeBtc)} vol · {timeLeft} ·{" "}
          <TrendIndicator change={market.change24h} />
        </span>
      </div>

      {/* Description */}
      <p className="mb-4 line-clamp-2 text-sm text-slate-400">
        {market.description}
      </p>

      {/* Chart — real price history */}
      <div>
        <MarketChart market={market} priceHistory={priceHistory} mode="home" />
      </div>
    </div>
  );
}
