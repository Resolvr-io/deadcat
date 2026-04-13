import { useMemo } from "react";
import { useMarkets } from "../../queries/useMarkets";
import { useStore } from "../../store";
import type { CovenantState, Market, NavCategory } from "../../types";
import {
  formatProbabilityWithPercent,
  formatVolumeBtc,
} from "../../utils-react/format";
import {
  fullContractSats,
  getEndingSoonMarkets,
  getFilteredMarkets,
  getNewMarkets,
  getTrendingMarkets,
  isExpired,
} from "../../utils-react/market";
import FeaturedMarket from "./FeaturedMarket";
import MarketCard, { openMarket, TrendIndicator } from "./MarketCard";

// ── Category icon (JSX version) ──────────────────────────────────────
function CategoryIcon({
  category,
  className = "h-5 w-5",
}: {
  category: string;
  className?: string;
}) {
  const props = {
    className: `${className} shrink-0`,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 2,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
  };

  switch (category) {
    case "Trending":
      return (
        <svg aria-hidden="true" {...props}>
          <polyline points="22 7 13.5 15.5 8.5 10.5 2 17" />
          <polyline points="16 7 22 7 22 13" />
        </svg>
      );
    case "Ending Soon":
      return (
        <svg aria-hidden="true" {...props}>
          <circle cx="12" cy="12" r="10" />
          <polyline points="12 6 12 12 16 14" />
        </svg>
      );
    case "New":
      return (
        <svg aria-hidden="true" {...props}>
          <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
        </svg>
      );
    case "My Markets":
      return (
        <svg aria-hidden="true" {...props}>
          <path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2" />
          <circle cx="12" cy="7" r="4" />
        </svg>
      );
    default:
      return null;
  }
}

// ── Top-movers icon ──────────────────────────────────────────────────
function TopMoversIcon() {
  return (
    <svg
      aria-hidden="true"
      className="h-4 w-4 shrink-0"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M7 3v18" />
      <path d="m3 7 4-4 4 4" />
      <path d="M17 21V3" />
      <path d="m21 17-4 4-4-4" />
    </svg>
  );
}

// ── Sidebar market list item ─────────────────────────────────────────
function SidebarMarketItem({
  market,
  index,
}: {
  market: Market;
  index: number;
}) {
  const yesPct =
    market.yesPrice != null ? Math.round(market.yesPrice * 100) : null;
  const noPct =
    market.yesPrice != null ? Math.round((1 - market.yesPrice) * 100) : null;

  const resolvedBadge =
    market.state === 2 || market.state === 3 ? (
      <span
        className={`rounded-full ${market.state === 2 ? "bg-emerald-500/20 text-emerald-300" : "bg-rose-500/20 text-rose-300"} px-2.5 py-0.5 text-xs font-medium`}
      >
        {market.state === 2 ? "Resolved YES" : "Resolved NO"}
      </span>
    ) : market.state === 4 ? (
      <span className="rounded-full bg-amber-500/20 px-2.5 py-0.5 text-xs font-medium text-amber-300">
        Expired
      </span>
    ) : null;

  return (
    <div className="w-full py-3 text-left">
      <button
        type="button"
        onClick={() => openMarket(market)}
        className="w-full text-left"
      >
        <p className="mb-1 text-sm font-normal text-slate-300">
          {index + 1}. {market.question}
        </p>
      </button>
      <div className="flex items-center gap-2">
        {resolvedBadge ?? (
          <>
            <button
              type="button"
              onClick={() =>
                openMarket(market, { side: "yes", intent: "open" })
              }
              className="rounded-full bg-emerald-500 px-2.5 py-0.5 text-xs font-medium text-white transition hover:bg-emerald-400"
            >
              {yesPct != null ? `Yes ${yesPct}%` : "Buy Yes"}
            </button>
            <button
              type="button"
              onClick={() => openMarket(market, { side: "no", intent: "open" })}
              className="rounded-full bg-rose-500 px-2.5 py-0.5 text-xs font-medium text-white transition hover:bg-rose-400"
            >
              {noPct != null ? `No ${noPct}%` : "Buy No"}
            </button>
          </>
        )}
        <span className="ml-auto text-xs">
          <TrendIndicator change={market.change24h} />
        </span>
      </div>
    </div>
  );
}

// ── Loading skeleton ─────────────────────────────────────────────────
function HomeSkeleton() {
  return (
    <div className="phi-container py-6 lg:py-8">
      <div className="grid gap-[21px] xl:grid-cols-[1.618fr_1fr]">
        <div className="space-y-[21px]">
          <div className="space-y-5 rounded-[21px] border border-slate-800 bg-slate-950/60 p-[21px] lg:p-[34px]">
            <div className="flex items-start justify-between gap-3">
              <div className="flex-1 space-y-3">
                <div className="h-8 w-3/4 animate-pulse rounded-lg bg-slate-800/60" />
                <div className="h-5 w-1/2 animate-pulse rounded bg-slate-800/60" />
              </div>
              <div className="h-11 w-28 animate-pulse rounded-full bg-slate-800/60" />
            </div>
            <div className="grid gap-[21px] lg:grid-cols-[1fr_1.618fr]">
              <div className="space-y-3">
                <div className="h-4 w-16 animate-pulse rounded bg-slate-800/60" />
                <div className="grid grid-cols-2 gap-2">
                  <div className="h-14 animate-pulse rounded-lg bg-slate-900/60" />
                  <div className="h-14 animate-pulse rounded-lg bg-slate-900/60" />
                </div>
                <div className="h-10 w-full animate-pulse rounded-full bg-slate-800/60" />
                <div className="h-10 w-full animate-pulse rounded-full bg-slate-800/60" />
                <div className="h-16 w-full animate-pulse rounded bg-slate-800/60" />
              </div>
              <div className="h-48 animate-pulse rounded-lg bg-slate-900/40" />
            </div>
          </div>
          <div>
            <div className="mb-3 flex items-center justify-between">
              <div className="h-5 w-28 animate-pulse rounded bg-slate-800/60" />
              <div className="h-4 w-16 animate-pulse rounded bg-slate-800/60" />
            </div>
            <div className="grid gap-3 md:grid-cols-2">
              {Array.from({ length: 4 }).map((_, i) => (
                <div
                  key={i}
                  className="space-y-3 rounded-2xl border border-slate-800 bg-slate-950/55 p-4"
                >
                  <div className="h-3 w-20 animate-pulse rounded bg-slate-800/60" />
                  <div className="h-5 w-full animate-pulse rounded bg-slate-800/60" />
                  <div className="flex justify-between">
                    <div className="h-7 w-20 animate-pulse rounded-full bg-slate-800/60" />
                    <div className="h-7 w-20 animate-pulse rounded-full bg-slate-800/60" />
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
        <aside className="grid gap-[13px] sm:grid-cols-2 xl:grid-cols-1">
          {Array.from({ length: 2 }).map((_, i) => (
            <div
              key={i}
              className="space-y-4 rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px]"
            >
              <div className="h-5 w-24 animate-pulse rounded bg-slate-800/60" />
              <div className="h-4 w-full animate-pulse rounded bg-slate-800/60" />
              <div className="h-4 w-5/6 animate-pulse rounded bg-slate-800/60" />
              <div className="h-4 w-full animate-pulse rounded bg-slate-800/60" />
            </div>
          ))}
        </aside>
      </div>
    </div>
  );
}

// ── Empty state ──────────────────────────────────────────────────────
function EmptyState() {
  const walletStatus = useStore((s) => s.walletStatus);
  const marketMakerMode = useStore((s) => s.marketMakerMode);
  const nostrPubkey = useStore((s) => s.nostrPubkey);

  return (
    <div className="phi-container py-16 text-center">
      <h2 className="mb-3 text-2xl font-semibold text-slate-100">
        No markets discovered
      </h2>
      <p className="mb-6 text-base text-slate-400">
        Be the first to create a prediction market on Liquid Testnet.
      </p>
      {walletStatus !== "unlocked" && (
        <>
          <p className="mb-4 text-sm text-amber-300">
            Set up your wallet first to start trading
          </p>
          <button
            type="button"
            onClick={() => useStore.setState({ walletOpen: true })}
            className="mr-3 rounded-xl border border-slate-600 px-6 py-3 text-base font-medium text-slate-200"
          >
            Set Up Wallet
          </button>
        </>
      )}
      {marketMakerMode && (
        <button
          type="button"
          onClick={() => useStore.setState({ view: "create" })}
          className="rounded-xl bg-emerald-300 px-6 py-3 text-base font-semibold text-slate-950"
        >
          <span className="mr-1">+</span> Create New Market
        </button>
      )}
      {nostrPubkey && (
        <p className="mt-4 text-xs text-slate-500">
          Identity: {nostrPubkey.slice(0, 8)}...{nostrPubkey.slice(-8)}
        </p>
      )}
    </div>
  );
}

// ── Sorted view (Ending Soon / New) ─────────────────────────────────
function SortedView({
  title,
  sortedMarkets,
}: {
  title: string;
  sortedMarkets: Market[];
}) {
  if (sortedMarkets.length === 0) {
    return (
      <div className="phi-container py-16 text-center">
        <h2 className="mb-3 text-2xl font-semibold text-slate-100">
          No {title.toLowerCase()} markets
        </h2>
        <p className="text-base text-slate-400">Check back soon for updates.</p>
      </div>
    );
  }

  return (
    <div className="phi-container py-6 lg:py-8">
      <div className="mb-4 flex items-center justify-between">
        <h1 className="flex items-center gap-2 text-xl font-medium text-slate-100">
          <CategoryIcon category={title} className="h-5 w-5" />
          {title}
        </h1>
        <p className="text-sm text-slate-400">
          {sortedMarkets.length} market
          {sortedMarkets.length !== 1 ? "s" : ""}
        </p>
      </div>
      <div className="grid gap-3 md:grid-cols-2">
        {sortedMarkets.map((market, idx) => (
          <MarketCard key={market.id} market={market} index={idx} />
        ))}
      </div>
    </div>
  );
}

// ── My Markets view ──────────────────────────────────────────────────
function MyMarketsView({ markets }: { markets: Market[] }) {
  const marketMakerMode = useStore((s) => s.marketMakerMode);
  const nostrPubkey = useStore((s) => s.nostrPubkey);

  const myMarkets = useMemo(
    () => getFilteredMarkets(markets, "", "My Markets", nostrPubkey),
    [markets, nostrPubkey],
  );

  if (myMarkets.length === 0) {
    return (
      <div className="phi-container py-16 text-center">
        <h2 className="mb-3 text-2xl font-semibold text-slate-100">
          No markets created yet
        </h2>
        <p className="mb-6 text-base text-slate-400">
          Markets you create as oracle will appear here.
        </p>
        <button
          type="button"
          onClick={() => useStore.setState({ view: "create" })}
          className="rounded-xl bg-emerald-300 px-6 py-3 text-base font-semibold text-slate-950"
        >
          <span className="mr-1">+</span> Create New Market
        </button>
      </div>
    );
  }

  const dormant = myMarkets.filter((m) => m.state === 0);
  const active = myMarkets.filter((m) => m.state === 1);
  const resolved = myMarkets.filter((m) => m.state === 2 || m.state === 3);
  const expiredMarkets = myMarkets.filter((m) => m.state === 4);

  const renderMyMarketCard = (market: Market) => {
    const no = market.yesPrice != null ? 1 - market.yesPrice : null;
    const stateColors =
      market.state === 0
        ? "bg-slate-600/30 text-slate-300"
        : market.state === 1
          ? "bg-sky-500/20 text-sky-300"
          : market.state === 2
            ? "bg-emerald-500/30 text-emerald-200"
            : market.state === 3
              ? "bg-rose-500/30 text-rose-200"
              : "bg-amber-500/25 text-amber-200";
    const stateLabels: Record<number, string> = {
      0: "DORMANT",
      1: "UNRESOLVED",
      2: "RESOLVED YES",
      3: "RESOLVED NO",
      4: "EXPIRED",
    };

    return (
      <button
        type="button"
        key={market.id}
        onClick={() => openMarket(market)}
        className="rounded-2xl border border-slate-800 bg-slate-950/55 p-4 text-left transition hover:border-slate-600"
      >
        <div className="mb-2 flex items-center justify-between text-sm">
          <span className="text-xs text-slate-500">{market.category}</span>
          <span
            className={`rounded-full px-2.5 py-0.5 text-xs font-medium ${stateColors}`}
          >
            {stateLabels[market.state] ?? "UNKNOWN"}
          </span>
        </div>
        <p className="mb-3 text-base font-normal text-slate-200">
          {market.question}
        </p>
        <div className="flex items-center justify-between text-sm">
          <span className="text-emerald-300">
            {market.yesPrice != null
              ? `Yes ${formatProbabilityWithPercent(market.yesPrice, fullContractSats(market))}`
              : "Buy Yes"}
          </span>
          <span className="text-rose-300">
            {no != null
              ? `No ${formatProbabilityWithPercent(no, fullContractSats(market))}`
              : "Buy No"}
          </span>
        </div>
      </button>
    );
  };

  const renderSection = (title: string, items: Market[]) => {
    if (items.length === 0) return null;
    return (
      <div className="mb-6">
        <h3 className="mb-3 text-sm font-medium text-slate-400">
          {title} ({items.length})
        </h3>
        <div className="grid gap-3 md:grid-cols-2">
          {items.map(renderMyMarketCard)}
        </div>
      </div>
    );
  };

  return (
    <div className="phi-container py-6 lg:py-8">
      <div className="mb-4 flex items-center justify-between">
        <h1 className="flex items-center gap-2 text-xl font-medium text-slate-100">
          <CategoryIcon category="My Markets" className="h-5 w-5" />
          My Markets
        </h1>
        {marketMakerMode && (
          <button
            type="button"
            onClick={() => useStore.setState({ view: "create" })}
            className="rounded-xl bg-emerald-300 px-5 py-2 text-sm font-semibold text-slate-950"
          >
            <span className="mr-1">+</span> Create New Market
          </button>
        )}
      </div>
      <div className="mb-4 grid gap-2 sm:grid-cols-3">
        <div className="rounded-xl border border-slate-800 bg-slate-950/45 p-3">
          <p className="text-xs text-slate-500">Total</p>
          <p className="text-lg font-medium text-slate-100">
            {myMarkets.length}
          </p>
        </div>
        <div className="rounded-xl border border-slate-800 bg-slate-950/45 p-3">
          <p className="text-xs text-slate-500">Active</p>
          <p className="text-lg font-medium text-emerald-300">
            {active.length}
          </p>
        </div>
        <div className="rounded-xl border border-slate-800 bg-slate-950/45 p-3">
          <p className="text-xs text-slate-500">Awaiting resolution</p>
          <p className="text-lg font-medium text-amber-300">
            {active.filter((m) => isExpired(m)).length}
          </p>
        </div>
      </div>
      {renderSection("Dormant \u2014 needs initial issuance", dormant)}
      {renderSection("Active", active)}
      {renderSection("Expired", expiredMarkets)}
      {renderSection("Resolved", resolved)}
    </div>
  );
}

// ── Category page view ───────────────────────────────────────────────
function CategoryPageView({
  markets,
  category,
  search,
  nostrPubkey,
}: {
  markets: Market[];
  category: NavCategory;
  search: string;
  nostrPubkey: string | null;
}) {
  const categoryMarkets = useMemo(
    () => getFilteredMarkets(markets, search, category, nostrPubkey),
    [markets, search, category, nostrPubkey],
  );

  const liveContracts = useMemo(
    () => categoryMarkets.filter((m) => m.isLive).slice(0, 4),
    [categoryMarkets],
  );

  const highestLiquidity = useMemo(
    () =>
      [...categoryMarkets]
        .sort((a, b) => b.liquidityBtc - a.liquidityBtc)
        .slice(0, 4),
    [categoryMarkets],
  );

  const stateMix = useMemo(
    () =>
      categoryMarkets.reduce(
        (acc, market) => {
          acc[market.state] += 1;
          return acc;
        },
        { 0: 0, 1: 0, 2: 0, 3: 0, 4: 0 } as Record<CovenantState, number>,
      ),
    [categoryMarkets],
  );

  return (
    <div className="phi-container py-6 lg:py-8">
      <div className="grid gap-[21px] xl:grid-cols-[233px_1fr_320px]">
        {/* Left sidebar */}
        <aside className="hidden xl:block">
          <div className="space-y-1 text-sm text-slate-400">
            <button
              type="button"
              className="block w-full rounded-md bg-slate-900/70 px-2 py-2 text-left text-emerald-300"
            >
              All markets
            </button>
            <button
              type="button"
              className="block w-full rounded-md px-2 py-2 text-left hover:bg-slate-900/40 hover:text-slate-200"
            >
              Live now
            </button>
            <button
              type="button"
              className="block w-full rounded-md px-2 py-2 text-left hover:bg-slate-900/40 hover:text-slate-200"
            >
              Resolved soon
            </button>
          </div>
        </aside>

        {/* Center content */}
        <section>
          <div className="mb-4 flex items-center justify-between">
            <h1 className="flex items-center gap-2 text-xl font-medium text-slate-100">
              {category}
            </h1>
            <div className="flex items-center gap-2 text-sm text-slate-400">
              <button
                type="button"
                className="rounded-full border border-slate-700 px-3 py-1.5"
              >
                Trending
              </button>
              <button
                type="button"
                className="rounded-full border border-slate-700 px-3 py-1.5"
              >
                Frequency
              </button>
            </div>
          </div>
          <div className="mb-4 grid gap-2 sm:grid-cols-3">
            <div className="rounded-xl border border-slate-800 bg-slate-950/45 p-3">
              <p className="text-xs text-slate-500">Contracts</p>
              <p className="text-lg font-medium text-slate-100">
                {categoryMarkets.length}
              </p>
            </div>
            <div className="rounded-xl border border-slate-800 bg-slate-950/45 p-3">
              <p className="text-xs text-slate-500">Live now</p>
              <p className="text-lg font-medium text-rose-300">
                {liveContracts.length}
              </p>
            </div>
            <div className="rounded-xl border border-slate-800 bg-slate-950/45 p-3">
              <p className="text-xs text-slate-500">24h volume</p>
              <p className="text-lg font-medium text-slate-100">
                {formatVolumeBtc(
                  categoryMarkets.reduce(
                    (sum, market) => sum + market.volumeBtc,
                    0,
                  ),
                )}
              </p>
            </div>
          </div>
          <div className="grid gap-3 md:grid-cols-2">
            {categoryMarkets.map((market, idx) => (
              <MarketCard key={market.id} market={market} index={idx} />
            ))}
          </div>
        </section>

        {/* Right sidebar */}
        <aside className="space-y-3">
          <section className="rounded-2xl border border-slate-800 bg-slate-950/55 p-4">
            <h3 className="mb-3 text-sm font-medium text-slate-400">
              Live contracts
            </h3>
            <div className="space-y-3">
              {liveContracts.length ? (
                liveContracts.map((market) => (
                  <button
                    type="button"
                    key={market.id}
                    onClick={() => openMarket(market)}
                    className="w-full text-left"
                  >
                    <p className="text-sm font-normal text-slate-300">
                      {market.question}
                    </p>
                    <p className="mt-1 text-xs text-slate-500">
                      {market.yesPrice != null
                        ? `Yes ${Math.round(market.yesPrice * 100)}% \u00b7 `
                        : ""}
                      {formatVolumeBtc(market.volumeBtc)} volume
                    </p>
                  </button>
                ))
              ) : (
                <p className="text-sm text-slate-500">
                  No live contracts in this category right now.
                </p>
              )}
            </div>
          </section>

          <section className="rounded-2xl border border-slate-800 bg-slate-950/55 p-4">
            <h3 className="mb-3 text-sm font-medium text-slate-400">
              Highest liquidity
            </h3>
            <div className="space-y-3">
              {highestLiquidity.map((market, idx) => (
                <button
                  type="button"
                  key={market.id}
                  onClick={() => openMarket(market)}
                  className="flex w-full items-start justify-between gap-2 text-left"
                >
                  <p className="text-sm text-slate-300">
                    {idx + 1}. {market.question}
                  </p>
                  <p className="text-sm font-normal text-emerald-300">
                    {formatVolumeBtc(market.liquidityBtc)}
                  </p>
                </button>
              ))}
            </div>
          </section>

          <section className="rounded-2xl border border-slate-800 bg-slate-950/55 p-4">
            <h3 className="mb-3 text-sm font-medium text-slate-400">
              State mix
            </h3>
            <div className="space-y-2 text-sm text-slate-300">
              <p className="flex items-center justify-between">
                <span>State 0 · Uninitialized</span>
                <span>{stateMix[0]}</span>
              </p>
              <p className="flex items-center justify-between">
                <span>State 1 · Unresolved</span>
                <span>{stateMix[1]}</span>
              </p>
              <p className="flex items-center justify-between">
                <span>State 2 · Resolved YES</span>
                <span>{stateMix[2]}</span>
              </p>
              <p className="flex items-center justify-between">
                <span>State 3 · Resolved NO</span>
                <span>{stateMix[3]}</span>
              </p>
              <p className="flex items-center justify-between">
                <span>State 4 · Expired</span>
                <span>{stateMix[4]}</span>
              </p>
            </div>
          </section>
        </aside>
      </div>
    </div>
  );
}

// ── Main trending home view ──────────────────────────────────────────
function TrendingHomeView({ markets }: { markets: Market[] }) {
  const trendingIndex = useStore((s) => s.trendingIndex);
  const search = useStore((s) => s.search);
  const nostrPubkey = useStore((s) => s.nostrPubkey);

  const trending = useMemo(() => getTrendingMarkets(markets), [markets]);

  const topMarkets = useMemo(
    () =>
      getFilteredMarkets(markets, search, "Trending", nostrPubkey).slice(0, 6),
    [markets, search, nostrPubkey],
  );

  const topMovers = useMemo(
    () =>
      [...markets]
        .sort((a, b) => Math.abs(b.change24h) - Math.abs(a.change24h))
        .slice(0, 3),
    [markets],
  );

  if (trending.length === 0) {
    return <EmptyState />;
  }

  const featured = trending[trendingIndex % trending.length];

  return (
    <div className="phi-container py-6 lg:py-8">
      <div className="grid gap-[21px] xl:grid-cols-[1.618fr_1fr]">
        <section className="space-y-[21px]">
          {/* Featured market carousel */}
          <FeaturedMarket market={featured} trending={trending} />

          {/* Top markets grid */}
          <section>
            <div className="mb-3 flex items-center justify-between">
              <h2 className="text-base font-medium text-slate-400">
                Top Markets
              </h2>
              <p className="text-sm text-slate-400">
                {topMarkets.length} shown
              </p>
            </div>
            <div className="grid gap-3 md:grid-cols-2">
              {topMarkets.map((market, idx) => (
                <MarketCard key={market.id} market={market} index={idx} />
              ))}
            </div>
          </section>
        </section>

        {/* Right sidebar: Trending + Top Movers */}
        <aside className="grid gap-[13px] sm:grid-cols-2 xl:grid-cols-1">
          {/* Trending sidebar */}
          <section className="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px]">
            <h3 className="mb-3 flex items-center gap-1.5 text-base font-medium text-slate-400">
              <CategoryIcon category="Trending" className="h-4 w-4" />
              Trending
            </h3>
            <div className="divide-y divide-slate-800">
              {trending.slice(0, 3).map((market, idx) => (
                <SidebarMarketItem
                  key={market.id}
                  market={market}
                  index={idx}
                />
              ))}
            </div>
          </section>

          {/* Top movers sidebar */}
          <section className="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px]">
            <h3 className="mb-3 flex items-center gap-1.5 text-base font-medium text-slate-400">
              <TopMoversIcon />
              Top movers
            </h3>
            <div className="divide-y divide-slate-800">
              {topMovers.map((market, idx) => (
                <SidebarMarketItem
                  key={market.id}
                  market={market}
                  index={idx}
                />
              ))}
            </div>
          </section>
        </aside>
      </div>
    </div>
  );
}

// ── Main HomePage component ──────────────────────────────────────────
export default function HomePage() {
  const activeCategory = useStore((s) => s.activeCategory);
  const search = useStore((s) => s.search);
  const marketsLoading = useStore((s) => s.marketsLoading);
  const nostrPubkey = useStore((s) => s.nostrPubkey);

  const { data: markets = [] } = useMarkets();

  // Determine which view to render based on activeCategory
  const isTrendingLike =
    activeCategory === "Trending" ||
    activeCategory === "Ending Soon" ||
    activeCategory === "New" ||
    activeCategory === "My Markets";

  // Category pages (Politics, Sports, Culture, Bitcoin, Weather, Macro, Resolved)
  if (!isTrendingLike) {
    return (
      <CategoryPageView
        markets={markets}
        category={activeCategory}
        search={search}
        nostrPubkey={nostrPubkey}
      />
    );
  }

  // My Markets
  if (activeCategory === "My Markets") {
    return <MyMarketsView markets={markets} />;
  }

  // Ending Soon
  if (activeCategory === "Ending Soon") {
    const endingSoon = getEndingSoonMarkets(markets);
    return <SortedView title="Ending Soon" sortedMarkets={endingSoon} />;
  }

  // New
  if (activeCategory === "New") {
    const newMarkets = getNewMarkets(markets);
    return <SortedView title="New" sortedMarkets={newMarkets} />;
  }

  // Trending (default home)
  if (marketsLoading && markets.length === 0) {
    return <HomeSkeleton />;
  }

  return <TrendingHomeView markets={markets} />;
}
