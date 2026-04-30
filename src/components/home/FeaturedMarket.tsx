import { useCallback, useLayoutEffect, useRef, useState } from "react";
import { usePriceHistory } from "../../queries/usePools";
import { useStore } from "../../store";
import type { Market, MarketGroup } from "../../types";
import { formatTimeRemaining, formatVolumeBtc } from "../../utils-react/format";
import { stateLabel } from "../../utils-react/market";
import { generateMockPriceHistory } from "../../utils-react/mock-price-history";
import MarketChart from "../chart/MarketChart";
import GroupChart, { OUTCOME_COLORS } from "../group/GroupChart";
import { openMarket, TrendIndicator } from "./MarketCard";

// ── Auto-sizing title — steps down the type scale until single line ──
const TITLE_SIZES = ["text-[34px]", "text-2xl", "text-xl", "text-lg"] as const;

function FittedTitle({
  children,
  className,
}: {
  children: string;
  className: string;
}) {
  const ref = useRef<HTMLHeadingElement>(null);
  const [sizeClass, setSizeClass] = useState<(typeof TITLE_SIZES)[number]>(
    TITLE_SIZES[0],
  );

  // biome-ignore lint/correctness/useExhaustiveDependencies: children triggers remeasure when text changes
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    for (let i = 0; i < TITLE_SIZES.length; i++) {
      const size = TITLE_SIZES[i];
      el.className = `${className} ${size}`;
      const lh = parseFloat(getComputedStyle(el).lineHeight) || 999;
      // Largest size: only 1 line allowed. Smaller sizes: up to 2 lines OK.
      const threshold = i === 0 ? 1.4 : 2.1;
      if (el.scrollHeight <= Math.ceil(lh * threshold)) {
        setSizeClass(size);
        return;
      }
    }
    setSizeClass(TITLE_SIZES[TITLE_SIZES.length - 1]);
  }, [children, className]);

  return (
    <h1 ref={ref} className={`${className} ${sizeClass}`}>
      {children}
    </h1>
  );
}

// ── State badge ──────────────────────────────────────────────────────
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

/** Grace window after a manual prev/next click before auto-advance
 *  is allowed to tick again. Long enough for a user to read the card
 *  they just landed on without the rotation yanking them away; short
 *  enough that an inattentive visitor still sees the rotation resume
 *  on its own. */
const MANUAL_ADVANCE_PAUSE_MS = 12_000;

// ── Shared nav arrows ────────────────────────────────────────────────
function CarouselNav({
  index,
  total,
  onPrev,
  onNext,
}: {
  index: number;
  total: number;
  onPrev: () => void;
  onNext: () => void;
}) {
  return (
    <div className="flex items-center gap-2">
      <button
        type="button"
        onClick={onPrev}
        className="h-9 w-9 rounded-full border border-slate-700 text-lg text-slate-300 hover:bg-slate-800"
      >
        &#8249;
      </button>
      <p className="w-16 text-center text-xs text-slate-400">
        {index + 1} of {total}
      </p>
      <button
        type="button"
        onClick={onNext}
        className="h-9 w-9 rounded-full border border-slate-700 text-lg text-slate-300 hover:bg-slate-800"
      >
        &#8250;
      </button>
    </div>
  );
}

// ── FeaturedMarket (binary) ──────────────────────────────────────────
export default function FeaturedMarket({
  market,
  totalItems,
}: {
  market: Market;
  totalItems: number;
}) {
  const trendingIndex = useStore((s) => s.trendingIndex);
  const trendingDirection = useStore((s) => s.trendingDirection);
  const { data: rawPriceHistory } = usePriceHistory(market.marketId);
  const priceHistory =
    rawPriceHistory && rawPriceHistory.length > 0
      ? rawPriceHistory
      : generateMockPriceHistory(market);

  const featuredYesPct =
    market.yesPrice != null ? Math.round(market.yesPrice * 100) : null;
  const featuredNoPct =
    market.yesPrice != null ? Math.round((1 - market.yesPrice) * 100) : null;
  const blocksLeft = market.expiryHeight - market.currentHeight;
  const timeLeft = blocksLeft > 0 ? formatTimeRemaining(blocksLeft) : "Expired";

  const handlePrev = useCallback(() => {
    useStore.setState({
      trendingIndex: (trendingIndex - 1 + totalItems) % totalItems,
      trendingDirection: "prev",
      trendingAutoAdvancePausedUntil: Date.now() + MANUAL_ADVANCE_PAUSE_MS,
    });
  }, [trendingIndex, totalItems]);

  const handleNext = useCallback(() => {
    useStore.setState({
      trendingIndex: (trendingIndex + 1) % totalItems,
      trendingDirection: "next",
      trendingAutoAdvancePausedUntil: Date.now() + MANUAL_ADVANCE_PAUSE_MS,
    });
  }, [trendingIndex, totalItems]);

  const handleOpenMarket = useCallback(() => openMarket(market), [market]);
  const handleBuyYes = useCallback(
    () => openMarket(market, { side: "yes", intent: "open" }),
    [market],
  );
  const handleBuyNo = useCallback(
    () => openMarket(market, { side: "no", intent: "open" }),
    [market],
  );

  const resolvedActions =
    market.state === 2 || market.state === 3 ? (
      <span
        className={`w-40 rounded-full px-4 py-2 text-center text-base font-semibold ${market.state === 2 ? "bg-emerald-500/20 text-emerald-300" : "bg-rose-500/20 text-rose-300"}`}
      >
        {market.state === 2 ? "Resolved YES" : "Resolved NO"}
      </span>
    ) : market.state === 4 ? (
      <span className="w-40 rounded-full bg-amber-500/20 px-4 py-2 text-center text-base font-semibold text-amber-300">
        Expired
      </span>
    ) : null;

  return (
    <div className="flex h-[520px] flex-col overflow-hidden rounded-xl bg-slate-950/50 p-5 lg:p-8">
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
        <CarouselNav
          index={trendingIndex}
          total={totalItems}
          onPrev={handlePrev}
          onNext={handleNext}
        />
      </div>

      {/* Everything below the header slides on advance. Keyed on the
          market id so a new featured market remounts this subtree and
          the CSS keyframe replays; the header row above stays in
          place so the nav arrows never jump. */}
      <div
        key={market.marketId}
        className={`${trendingDirection === "prev" ? "animate-carousel-slide-reverse" : "animate-carousel-slide"} flex min-h-0 flex-1 flex-col`}
      >
        <button
          type="button"
          onClick={handleOpenMarket}
          className="mb-4 block text-left"
        >
          <FittedTitle className="phi-title font-medium leading-tight text-slate-100 transition hover:text-white">
            {market.question}
          </FittedTitle>
        </button>

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

        <div className="min-h-0 flex-1 overflow-hidden">
          <MarketChart
            market={market}
            priceHistory={priceHistory}
            mode="home"
          />
        </div>
      </div>
    </div>
  );
}

// ── FeaturedGroupCard (multi-outcome) ────────────────────────────────
export function FeaturedGroupCard({
  group,
  totalItems,
}: {
  group: MarketGroup;
  totalItems: number;
}) {
  const trendingIndex = useStore((s) => s.trendingIndex);
  const trendingDirection = useStore((s) => s.trendingDirection);

  const blocksLeft = group.expiryHeight - group.currentHeight;
  const timeLeft = blocksLeft > 0 ? formatTimeRemaining(blocksLeft) : "Expired";

  const topOutcomes = [...group.outcomes]
    .sort((a, b) => b.yesPrice - a.yesPrice)
    .slice(0, 6);

  const handlePrev = useCallback(() => {
    useStore.setState({
      trendingIndex: (trendingIndex - 1 + totalItems) % totalItems,
      trendingDirection: "prev",
      trendingAutoAdvancePausedUntil: Date.now() + MANUAL_ADVANCE_PAUSE_MS,
    });
  }, [trendingIndex, totalItems]);

  const handleNext = useCallback(() => {
    useStore.setState({
      trendingIndex: (trendingIndex + 1) % totalItems,
      trendingDirection: "next",
      trendingAutoAdvancePausedUntil: Date.now() + MANUAL_ADVANCE_PAUSE_MS,
    });
  }, [trendingIndex, totalItems]);

  const handleOpen = useCallback(() => {
    useStore.setState({
      selectedGroupId: group.id,
      selectedOutcomeId: null,
      view: "group",
    });
  }, [group.id]);

  return (
    <div className="flex h-[520px] flex-col overflow-hidden rounded-xl bg-slate-950/50 p-5 lg:p-8">
      {/* Header — stays put on advance so the nav arrows are stable. */}
      <div className="mb-4 flex items-center justify-between gap-3">
        <div className="flex items-center gap-2 text-xs">
          <span className="text-slate-500">{group.category}</span>
          {group.state === "active" && (
            <span className="rounded bg-emerald-500/10 px-1.5 py-0.5 text-[10px] font-medium text-emerald-400">
              Active
            </span>
          )}
        </div>
        <CarouselNav
          index={trendingIndex}
          total={totalItems}
          onPrev={handlePrev}
          onNext={handleNext}
        />
      </div>

      {/* Sliding body — see FeaturedMarket for the same pattern. */}
      <div
        key={group.id}
        className={`${trendingDirection === "prev" ? "animate-carousel-slide-reverse" : "animate-carousel-slide"} flex min-h-0 flex-1 flex-col`}
      >
        {/* Title */}
        <button
          type="button"
          onClick={handleOpen}
          className="mb-5 block text-left"
        >
          <FittedTitle className="phi-title font-medium leading-tight text-slate-100 transition hover:text-white">
            {group.title}
          </FittedTitle>
        </button>

        {/* Two-column: outcomes left, chart right */}
        <div className="grid min-h-0 flex-1 grid-cols-[0.6fr_1.618fr] gap-6">
          {/* Outcomes list */}
          <div className="space-y-2.5">
            {topOutcomes.map((outcome, i) => {
              const pct = Math.round(outcome.yesPrice * 100);
              const color = OUTCOME_COLORS[i % OUTCOME_COLORS.length];
              return (
                <div
                  key={outcome.id}
                  className="flex items-baseline justify-between gap-2"
                >
                  <div className="flex min-w-0 items-center gap-2">
                    <span
                      className="h-2 w-2 shrink-0 rounded-full"
                      style={{ backgroundColor: color }}
                    />
                    <span className="truncate text-sm text-slate-300">
                      {outcome.name}
                    </span>
                  </div>
                  <span className="shrink-0 text-sm font-semibold text-slate-100">
                    {pct < 1 ? "<1" : pct}%
                  </span>
                </div>
              );
            })}
            {group.outcomes.length > 6 && (
              <p className="pl-4 text-xs text-slate-600">
                +{group.outcomes.length - 6} more
              </p>
            )}
            {/* Footer meta */}
            <div className="pt-2 text-xs text-slate-500">
              {formatVolumeBtc(group.totalVolumeBtc)} vol · {timeLeft}
            </div>
          </div>

          {/* Chart — fills column height */}
          <GroupChart
            group={group}
            highlightedOutcomeId={null}
            showLegend={false}
            showControls={false}
            className="h-full"
          />
        </div>
      </div>
    </div>
  );
}
