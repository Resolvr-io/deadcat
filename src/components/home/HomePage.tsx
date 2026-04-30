import { useEffect, useMemo } from "react";
import { useMarkets } from "../../queries/useMarkets";
import { useStore } from "../../store";
import type {
  CovenantState,
  Market,
  MarketGroup,
  NavCategory,
} from "../../types";
import {
  formatProbabilityWithPercent,
  formatSats,
  formatTimeRemaining,
  formatVolumeBtc,
} from "../../utils-react/format";
import {
  fullContractSats,
  getEndingSoonMarkets,
  getFilteredMarkets,
  getNewMarkets,
  getPositionContracts,
  getTrendingMarkets,
  isExpired,
} from "../../utils-react/market";
import { getMockMarketGroups } from "../../utils-react/mock-groups";
import FeaturedMarket, { FeaturedGroupCard } from "./FeaturedMarket";
import MarketCard, { openMarket, TrendIndicator } from "./MarketCard";
import MarketGroupCard from "./MarketGroupCard";

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
    case "Portfolio":
      return (
        <svg aria-hidden="true" {...props}>
          <path d="M3 3v18h18" />
          <rect x="7" y="13" width="9" height="4" rx="1" />
          <rect x="7" y="5" width="12" height="4" rx="1" />
        </svg>
      );
    case "Politics":
      return (
        <svg
          aria-hidden="true"
          className={`${className} shrink-0`}
          viewBox="0 0 24 24"
          fill="currentColor"
        >
          <path d="M20 6h3v2h-1v11h1v2H1v-2h1V8H1V6h3V4a1 1 0 0 1 1-1h14a1 1 0 0 1 1 1v2zm0 2H4v11h3v-7h2v7h2v-7h2v7h2v-7h2v7h3V8zM6 5v1h12V5H6z" />
        </svg>
      );
    case "Sports":
      return (
        <svg aria-hidden="true" {...props}>
          <path d="M6 9H4a2 2 0 0 1-2-2V4h4" />
          <path d="M18 9h2a2 2 0 0 0 2-2V4h-4" />
          <path d="M4 22h16" />
          <path d="M10 14.66V17c0 .55-.47.98-.97 1.21C7.85 18.75 7 20.24 7 22" />
          <path d="M14 14.66V17c0 .55.47.98.97 1.21C16.15 18.75 17 20.24 17 22" />
          <path d="M18 2H6v7a6 6 0 0 0 12 0V2Z" />
        </svg>
      );
    case "Culture":
      return (
        <svg aria-hidden="true" {...props}>
          <circle cx="12" cy="12" r="10" />
          <path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20" />
          <path d="M2 12h20" />
        </svg>
      );
    case "Bitcoin":
      return (
        <svg
          aria-hidden="true"
          className={`${className} shrink-0`}
          viewBox="0 0 24 24"
          fill="currentColor"
        >
          <path
            fillRule="evenodd"
            clipRule="evenodd"
            d="M9 2a1 1 0 0 1 2 0v2h2V2a1 1 0 1 1 2 0v2c3.59 0 5.88 2.01 5.88 4.5 0 1.41-.73 2.67-2.02 3.49 1.31.82 2.06 2.08 2.06 3.51 0 2.5-2.3 4.5-5.91 4.5v2a1 1 0 1 1-2 0v-2h-2v2a1 1 0 1 1-2 0v-2H4a1 1 0 1 1 0-2h2V6H4a1 1 0 0 1 0-2h5V2zm6 9c2.91 0 3.88-1.49 3.88-2.5S17.91 6 15 6H8v5h7zM8 13v5h7c2.94 0 3.91-1.5 3.91-2.5S17.94 13 15 13H8z"
          />
        </svg>
      );
    case "Weather":
      return (
        <svg aria-hidden="true" {...props}>
          <path d="M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z" />
        </svg>
      );
    case "Macro":
      return (
        <svg aria-hidden="true" {...props}>
          <path d="M3 3v18h18" />
          <path d="m19 9-5 5-4-4-3 3" />
        </svg>
      );
    case "Resolved":
      return (
        <svg aria-hidden="true" {...props}>
          <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" />
          <polyline points="22 4 12 14.01 9 11.01" />
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
              className="rounded-full bg-emerald-500/15 px-2.5 py-0.5 text-xs font-medium text-emerald-400 transition hover:bg-emerald-500/25"
            >
              {yesPct != null ? `Yes ${yesPct}%` : "Buy Yes"}
            </button>
            <button
              type="button"
              onClick={() => openMarket(market, { side: "no", intent: "open" })}
              className="rounded-full bg-rose-500/15 px-2.5 py-0.5 text-xs font-medium text-rose-400 transition hover:bg-rose-500/25"
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

// ── Empty state ──────────────────────────────────────────────────────
function EmptyState() {
  const walletStatus = useStore((s) => s.walletStatus);
  const marketMakerMode = useStore((s) => s.marketMakerMode);
  const nostrPubkey = useStore((s) => s.nostrPubkey);

  return (
    <div className="phi-container py-16 text-center">
      <div className="mx-auto mb-6 flex h-20 w-20 items-center justify-center rounded-full border border-slate-700 bg-slate-900/60">
        <CategoryIcon category="Trending" className="h-9 w-9" />
      </div>
      <h2 className="mb-3 text-2xl font-semibold text-slate-100">
        No markets yet
      </h2>
      <p className="mx-auto mb-6 max-w-md text-base text-slate-400">
        Prediction markets on Liquid will appear here once they're published to
        Nostr relays.
      </p>
      {walletStatus === "not_created" && !nostrPubkey && (
        <button
          type="button"
          onClick={() =>
            useStore.setState({ setupModalOpen: true, onboardingStep: "nostr" })
          }
          className="mr-3 rounded-xl border border-slate-600 px-6 py-3 text-base font-medium text-slate-200"
        >
          Set Up Account
        </button>
      )}
      {marketMakerMode && (
        <button
          type="button"
          onClick={() => useStore.setState({ view: "create" })}
          className="rounded-xl bg-emerald-300 px-6 py-3 text-base font-semibold text-slate-950"
        >
          <span className="mr-1">+</span> Create Market
        </button>
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
        <div className="mx-auto mb-6 flex h-20 w-20 items-center justify-center rounded-full border border-slate-700 bg-slate-900/60">
          <CategoryIcon category={title} className="h-9 w-9" />
        </div>
        <h2 className="mb-3 text-2xl font-semibold text-slate-100">
          No {title.toLowerCase()} markets
        </h2>
        <p className="mx-auto mb-4 max-w-md text-base text-slate-400">
          Markets matching this filter will appear here.
        </p>
        <button
          type="button"
          onClick={() => useStore.setState({ activeCategory: "Trending" })}
          className="rounded-xl border border-slate-600 px-6 py-3 text-base font-medium text-slate-200"
        >
          Browse All Markets
        </button>
      </div>
    );
  }

  return (
    <div className="phi-container py-6 lg:py-8">
      <div className="mb-4 flex items-center justify-between">
        <h1 className="flex items-center gap-2 text-xl font-medium text-slate-100">
          <span className="text-slate-500">
            <CategoryIcon category={title} className="h-5 w-5" />
          </span>
          {title}
        </h1>
        <p className="text-sm text-slate-400">
          {sortedMarkets.length} market
          {sortedMarkets.length !== 1 ? "s" : ""}
        </p>
      </div>
      <div className="grid gap-3 md:grid-cols-2 lg:grid-cols-3">
        {sortedMarkets.map((market, idx) => (
          <MarketCard key={market.id} market={market} index={idx} />
        ))}
      </div>
    </div>
  );
}

// ── Portfolio view ───────────────────────────────────────────────────
function PortfolioView({ markets }: { markets: Market[] }) {
  const walletData = useStore((s) => s.walletData);
  const walletStatus = useStore((s) => s.walletStatus);

  const positions = useMemo(
    () => getFilteredMarkets(markets, "", "Portfolio", null, walletData),
    [markets, walletData],
  );

  if (positions.length === 0) {
    const hasWallet = walletStatus === "unlocked";
    return (
      <div className="phi-container py-16 text-center">
        <div className="mx-auto mb-6 flex h-20 w-20 items-center justify-center rounded-full border border-slate-700 bg-slate-900/60">
          <CategoryIcon category="Portfolio" className="h-9 w-9" />
        </div>
        <h2 className="mb-3 text-2xl font-semibold text-slate-100">
          No positions yet
        </h2>
        <p className="mx-auto mb-6 max-w-md text-base text-slate-400">
          {hasWallet
            ? "When you buy Yes or No on a market, your positions and P&L will appear here."
            : "Set up a wallet and start trading to track your portfolio."}
        </p>
        <button
          type="button"
          onClick={() => useStore.setState({ activeCategory: "Trending" })}
          className="rounded-xl bg-emerald-300 px-6 py-3 text-base font-semibold text-slate-950"
        >
          Browse Markets
        </button>
      </div>
    );
  }

  const positionCards = positions.map((market) => {
    const pos = getPositionContracts(market, walletData);
    const fc = fullContractSats(market);
    const yesPriceSats =
      market.yesPrice != null ? Math.round(market.yesPrice * fc) : null;
    const noPriceSats = yesPriceSats != null ? fc - yesPriceSats : null;
    const yesValue = yesPriceSats != null ? pos.yes * yesPriceSats : 0;
    const noValue = noPriceSats != null ? pos.no * noPriceSats : 0;
    const totalValue = yesValue + noValue;
    const isResolved = market.state === 2 || market.state === 3;
    const winningSide =
      market.state === 2 ? "yes" : market.state === 3 ? "no" : null;
    return {
      market,
      pos,
      yesPriceSats,
      noPriceSats,
      totalValue,
      isResolved,
      winningSide,
    };
  });

  const totalPortfolioValue = positionCards.reduce(
    (sum, p) => sum + p.totalValue,
    0,
  );
  const activePositions = positionCards.filter((p) => !p.isResolved);
  const settledPositions = positionCards.filter((p) => p.isResolved);

  return (
    <div className="phi-container py-6 lg:py-8">
      <div className="mb-6 flex items-center justify-between">
        <h1 className="flex items-center gap-2 text-xl font-medium text-slate-100">
          <span className="text-slate-500">
            <CategoryIcon category="Portfolio" className="h-5 w-5" />
          </span>
          Portfolio
        </h1>
      </div>

      <div className="mb-6 grid gap-3 sm:grid-cols-3">
        <div className="rounded-xl bg-slate-950/45 px-4 py-3">
          <p className="text-xs uppercase tracking-wider text-slate-500">
            Portfolio Value
          </p>
          <p className="text-2xl font-semibold text-slate-100">
            {formatSats(totalPortfolioValue)}
          </p>
        </div>
        <div className="rounded-xl bg-slate-950/45 px-4 py-3">
          <p className="text-xs uppercase tracking-wider text-slate-500">
            Active Positions
          </p>
          <p className="text-2xl font-semibold text-emerald-300">
            {activePositions.length}
          </p>
        </div>
        <div className="rounded-xl bg-slate-950/45 px-4 py-3">
          <p className="text-xs uppercase tracking-wider text-slate-500">
            Settled
          </p>
          <p className="text-2xl font-semibold text-slate-400">
            {settledPositions.length}
          </p>
        </div>
      </div>

      {activePositions.length > 0 && (
        <>
          <h3 className="mb-3 text-sm font-medium uppercase tracking-wider text-slate-400">
            Active
          </h3>
          <div className="mb-6 space-y-3">
            {activePositions.map((p) => {
              const blocksLeft = p.market.expiryHeight - p.market.currentHeight;
              const timeLeft =
                blocksLeft > 0 ? formatTimeRemaining(blocksLeft) : "Expired";
              return (
                <button
                  type="button"
                  key={p.market.id}
                  onClick={() => openMarket(p.market)}
                  className="block w-full rounded-xl bg-slate-950/50 p-4 text-left transition hover:bg-slate-900/60"
                >
                  <div className="mb-2 flex items-center justify-between">
                    <span className="flex items-center gap-1.5 text-xs text-slate-500">
                      <CategoryIcon
                        category={p.market.category}
                        className="h-3 w-3"
                      />
                      {p.market.category} · {timeLeft}
                    </span>
                  </div>
                  <p className="mb-3 text-sm text-slate-200">
                    {p.market.question}
                  </p>
                  <div className="flex flex-wrap items-center gap-3">
                    {p.pos.yes > 0 && (
                      <div className="rounded-lg bg-emerald-500/10 px-3 py-1.5">
                        <span className="text-xs text-slate-400">YES</span>
                        <span className="ml-1 text-sm font-semibold text-emerald-300">
                          {p.pos.yes.toLocaleString()}
                        </span>
                        {p.yesPriceSats != null && (
                          <span className="ml-2 text-xs text-slate-500">
                            @ {p.yesPriceSats} sats
                          </span>
                        )}
                      </div>
                    )}
                    {p.pos.no > 0 && (
                      <div className="rounded-lg bg-rose-500/10 px-3 py-1.5">
                        <span className="text-xs text-slate-400">NO</span>
                        <span className="ml-1 text-sm font-semibold text-rose-300">
                          {p.pos.no.toLocaleString()}
                        </span>
                        {p.noPriceSats != null && (
                          <span className="ml-2 text-xs text-slate-500">
                            @ {p.noPriceSats} sats
                          </span>
                        )}
                      </div>
                    )}
                    <span className="ml-auto text-sm font-medium text-slate-300">
                      {formatSats(p.totalValue)}
                    </span>
                  </div>
                </button>
              );
            })}
          </div>
        </>
      )}

      {settledPositions.length > 0 && (
        <>
          <h3 className="mb-3 text-sm font-medium uppercase tracking-wider text-slate-400">
            Settled
          </h3>
          <div className="space-y-3">
            {settledPositions.map((p) => {
              const won =
                p.winningSide === "yes" ? p.pos.yes > 0 : p.pos.no > 0;
              return (
                <button
                  type="button"
                  key={p.market.id}
                  onClick={() => openMarket(p.market)}
                  className="block w-full rounded-xl bg-slate-950/50 p-4 text-left transition hover:bg-slate-900/60"
                >
                  <p className="mb-3 text-sm text-slate-200">
                    {p.market.question}
                  </p>
                  <div className="flex items-center gap-3">
                    <span
                      className={`rounded-full px-3 py-1 text-xs font-medium ${won ? "bg-emerald-500/20 text-emerald-300" : "bg-rose-500/20 text-rose-300"}`}
                    >
                      {won ? "Won" : "Lost"}
                    </span>
                    {p.pos.yes > 0 && (
                      <span className="text-xs text-slate-400">
                        YES: {p.pos.yes.toLocaleString()}
                      </span>
                    )}
                    {p.pos.no > 0 && (
                      <span className="text-xs text-slate-400">
                        NO: {p.pos.no.toLocaleString()}
                      </span>
                    )}
                  </div>
                </button>
              );
            })}
          </div>
        </>
      )}
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
        <div className="mx-auto mb-6 flex h-20 w-20 items-center justify-center rounded-full border border-slate-700 bg-slate-900/60">
          <CategoryIcon category="My Markets" className="h-9 w-9" />
        </div>
        <h2 className="mb-3 text-2xl font-semibold text-slate-100">
          No markets created yet
        </h2>
        <p className="mx-auto mb-6 max-w-md text-base text-slate-400">
          Markets you create as oracle will appear here. Create your first
          market to get started.
        </p>
        {marketMakerMode && (
          <button
            type="button"
            onClick={() => useStore.setState({ view: "create" })}
            className="rounded-xl bg-emerald-300 px-6 py-3 text-base font-semibold text-slate-950"
          >
            <span className="mr-1">+</span> Create Market
          </button>
        )}
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
        className="rounded-xl bg-slate-950/50 p-4 text-left transition hover:bg-slate-900/60"
      >
        <div className="mb-2 flex items-center justify-between text-sm">
          <span className="flex items-center gap-1.5 text-xs text-slate-500">
            <CategoryIcon category={market.category} className="h-3 w-3" />
            {market.category}
          </span>
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
          <span className="text-slate-500">
            <CategoryIcon category="My Markets" className="h-5 w-5" />
          </span>
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
        <div className="rounded-xl bg-slate-950/45 p-3">
          <p className="text-xs text-slate-500">Total</p>
          <p className="text-lg font-medium text-slate-100">
            {myMarkets.length}
          </p>
        </div>
        <div className="rounded-xl bg-slate-950/45 p-3">
          <p className="text-xs text-slate-500">Active</p>
          <p className="text-lg font-medium text-emerald-300">
            {active.length}
          </p>
        </div>
        <div className="rounded-xl bg-slate-950/45 p-3">
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
  const categorySortMode = useStore((s) => s.categorySortMode);
  const categoryFilter = useStore((s) => s.categoryFilter);

  const categoryMarkets = useMemo(
    () => getFilteredMarkets(markets, search, category, nostrPubkey),
    [markets, search, category, nostrPubkey],
  );

  const filteredMarkets = useMemo(() => {
    if (categoryFilter === "live")
      return categoryMarkets.filter((m) => m.isLive);
    if (categoryFilter === "ending-soon")
      return categoryMarkets.filter(
        (m) => m.isLive && m.expiryHeight - m.currentHeight <= 200,
      );
    return categoryMarkets;
  }, [categoryMarkets, categoryFilter]);

  const sortedMarkets = useMemo(() => {
    if (categorySortMode === "frequency")
      return [...filteredMarkets].sort((a, b) => b.traderCount - a.traderCount);
    return [...filteredMarkets].sort((a, b) => b.volumeBtc - a.volumeBtc);
  }, [filteredMarkets, categorySortMode]);

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

  const sidebarFilters = [
    { key: "all", label: "All markets" },
    { key: "live", label: "Live now" },
    { key: "ending-soon", label: "Ending soon" },
  ] as const;

  return (
    <div className="phi-container py-6 lg:py-8">
      <div className="grid gap-8 xl:grid-cols-[233px_1fr_320px]">
        {/* Left sidebar */}
        <aside className="hidden xl:block">
          <div className="space-y-1 text-sm text-slate-400">
            {sidebarFilters.map(({ key, label }) => {
              const active = categoryFilter === key;
              return (
                <button
                  key={key}
                  type="button"
                  onClick={() => useStore.setState({ categoryFilter: key })}
                  className={`block w-full rounded-md px-2 py-2 text-left transition focus-visible:outline-none ${active ? "bg-slate-900/70 text-emerald-300 focus-visible:bg-slate-900/90 focus-visible:text-emerald-200" : "hover:bg-slate-900/40 hover:text-slate-200 focus-visible:bg-slate-900/70 focus-visible:text-emerald-300"}`}
                >
                  {label}
                </button>
              );
            })}
          </div>
        </aside>

        {/* Center content */}
        <section>
          <div className="mb-4 flex items-center justify-between">
            <h1 className="flex items-center gap-2 text-xl font-medium text-slate-100">
              <span className="text-slate-500">
                <CategoryIcon category={category} className="h-5 w-5" />
              </span>
              {category}
            </h1>
            <div className="flex items-center gap-2 text-sm text-slate-400">
              {(["trending", "frequency"] as const).map((mode) => (
                <button
                  key={mode}
                  type="button"
                  onClick={() => useStore.setState({ categorySortMode: mode })}
                  className={`rounded-full px-3 py-1.5 capitalize transition ${categorySortMode === mode ? "bg-slate-800 text-slate-100" : "text-slate-500 hover:text-slate-300"}`}
                >
                  {mode}
                </button>
              ))}
            </div>
          </div>
          <div className="mb-4 grid gap-2 sm:grid-cols-3">
            <div className="rounded-xl bg-slate-950/45 p-3">
              <p className="text-xs text-slate-500">Contracts</p>
              <p className="text-lg font-medium text-slate-100">
                {sortedMarkets.length}
              </p>
            </div>
            <div className="rounded-xl bg-slate-950/45 p-3">
              <p className="text-xs text-slate-500">Live now</p>
              <p className="text-lg font-medium text-rose-300">
                {liveContracts.length}
              </p>
            </div>
            <div className="rounded-xl bg-slate-950/45 p-3">
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
            {sortedMarkets.map((market, idx) => (
              <MarketCard key={market.id} market={market} index={idx} />
            ))}
          </div>
        </section>

        {/* Right sidebar */}
        <aside className="space-y-4 xl:content-start">
          <section className="rounded-xl bg-slate-950/45 p-4">
            <h3 className="mb-3 border-b border-slate-800/60 pb-2 text-xs font-semibold uppercase tracking-wider text-slate-500">
              Live contracts
            </h3>
            <div className="divide-y divide-slate-800/60">
              {liveContracts.length ? (
                liveContracts.map((market) => (
                  <button
                    type="button"
                    key={market.id}
                    onClick={() => openMarket(market)}
                    className="w-full py-2.5 text-left"
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
                <p className="py-2.5 text-sm text-slate-500">
                  No live contracts in this category right now.
                </p>
              )}
            </div>
          </section>

          <section className="rounded-xl bg-slate-950/45 p-4">
            <h3 className="mb-3 border-b border-slate-800/60 pb-2 text-xs font-semibold uppercase tracking-wider text-slate-500">
              Highest liquidity
            </h3>
            <div className="divide-y divide-slate-800/60">
              {highestLiquidity.length ? (
                highestLiquidity.map((market) => {
                  const [amount, unit] = formatVolumeBtc(
                    market.liquidityBtc,
                  ).split(" ");
                  return (
                    <button
                      type="button"
                      key={market.id}
                      onClick={() => openMarket(market)}
                      className="flex w-full items-baseline justify-between gap-3 py-2.5 text-left"
                    >
                      <p className="text-sm text-slate-300">
                        {market.question}
                      </p>
                      <span className="shrink-0 whitespace-nowrap rounded-md bg-slate-800/70 px-2 py-0.5 text-sm font-medium tabular-nums text-slate-200">
                        {amount}
                        <span className="ml-1 text-slate-500">{unit}</span>
                      </span>
                    </button>
                  );
                })
              ) : (
                <p className="py-2.5 text-sm text-slate-500">
                  No liquidity in this category yet.
                </p>
              )}
            </div>
          </section>

          {(() => {
            const stateRows = [
              {
                label: "Dormant",
                color: "text-slate-400",
                count: stateMix[0],
              },
              { label: "Live", color: "text-emerald-400", count: stateMix[1] },
              {
                label: "Resolved YES",
                color: "text-slate-400",
                count: stateMix[2],
              },
              {
                label: "Resolved NO",
                color: "text-slate-400",
                count: stateMix[3],
              },
              {
                label: "Expired",
                color: "text-amber-400",
                count: stateMix[4],
              },
            ].filter((row) => row.count > 0);
            if (stateRows.length === 0) return null;
            return (
              <section className="rounded-xl bg-slate-950/45 p-4">
                <h3 className="mb-3 border-b border-slate-800/60 pb-2 text-xs font-semibold uppercase tracking-wider text-slate-500">
                  Market states
                </h3>
                <div className="divide-y divide-slate-800/60 text-sm text-slate-300">
                  {stateRows.map((row) => (
                    <p
                      key={row.label}
                      className="flex items-center justify-between py-2"
                    >
                      <span className={row.color}>{row.label}</span>
                      <span className="tabular-nums">{row.count}</span>
                    </p>
                  ))}
                </div>
              </section>
            );
          })()}
        </aside>
      </div>
    </div>
  );
}

// ── Skeleton cards for loading state ────────────────────────────────
function SkeletonCard() {
  return (
    <div className="animate-pulse rounded-xl bg-slate-950/50 p-4">
      <div className="mb-2 flex items-center justify-between">
        <div className="h-3 w-16 rounded bg-slate-800" />
        <div className="h-3 w-12 rounded bg-slate-800" />
      </div>
      <div className="mb-2 h-4 w-3/4 rounded bg-slate-800" />
      <div className="mb-3 h-8 w-16 rounded bg-slate-800" />
      <div className="flex gap-2">
        <div className="h-7 w-24 rounded-full bg-slate-800" />
        <div className="h-7 w-24 rounded-full bg-slate-800" />
      </div>
      <div className="mt-2 h-3 w-32 rounded bg-slate-800" />
    </div>
  );
}

function SkeletonFeatured() {
  return (
    <div className="animate-pulse rounded-xl bg-slate-950/50 p-6">
      <div className="mb-3 h-4 w-20 rounded bg-slate-800" />
      <div className="mb-2 h-6 w-2/3 rounded bg-slate-800" />
      <div className="mb-4 h-10 w-24 rounded bg-slate-800" />
      <div className="flex gap-3">
        <div className="h-9 w-28 rounded-full bg-slate-800" />
        <div className="h-9 w-28 rounded-full bg-slate-800" />
      </div>
    </div>
  );
}

function MarketSkeletons() {
  return (
    <div className="phi-container py-6 lg:py-8">
      <div className="grid gap-8 xl:grid-cols-[1.618fr_1fr]">
        <section className="space-y-6">
          <SkeletonFeatured />
          <div className="grid gap-3 md:grid-cols-2">
            {Array.from({ length: 4 }, (_, i) => (
              <SkeletonCard key={`sk-${i}`} />
            ))}
          </div>
        </section>
        <aside className="space-y-6">
          {Array.from({ length: 3 }, (_, i) => (
            <SkeletonCard key={`sk-side-${i}`} />
          ))}
        </aside>
      </div>
    </div>
  );
}

// ── Main trending home view ──────────────────────────────────────────
function TrendingHomeView({ markets }: { markets: Market[] }) {
  const trendingIndex = useStore((s) => s.trendingIndex);
  const trendingAutoAdvancePausedUntil = useStore(
    (s) => s.trendingAutoAdvancePausedUntil,
  );
  const search = useStore((s) => s.search);
  const nostrPubkey = useStore((s) => s.nostrPubkey);
  const marketsLoading = useStore((s) => s.marketsLoading);
  const marketGroups = getMockMarketGroups();

  const trending = useMemo(() => getTrendingMarkets(markets), [markets]);

  // Combined carousel: groups first, then binary markets
  const featuredItems = useMemo<(MarketGroup | Market)[]>(
    () => [...marketGroups, ...trending],
    [marketGroups, trending],
  );

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

  // Auto-advance carousel. One-shot timer re-armed after each
  // advance so the fire time can adapt to manual-click pauses: the
  // delay is `max(pausedUntil - now, 6000)`, meaning a fresh click
  // pushes the next advance out to exactly when the pause ends —
  // not the next tick of a fixed 6s grid, which could add up to
  // another 6s of wait on top of the user's browsing window.
  // `trendingIndex` is listed in the deps so each advance re-runs
  // this effect and schedules the next timer; biome flags it as
  // unused (the effect body doesn't read it) but removing it
  // breaks the chain.
  // biome-ignore lint/correctness/useExhaustiveDependencies: trendingIndex re-arms the chain
  useEffect(() => {
    const total = featuredItems.length;
    if (total === 0) return;
    const now = Date.now();
    const delay = Math.max(trendingAutoAdvancePausedUntil - now, 6000);
    const id = window.setTimeout(() => {
      useStore.setState((s) => ({
        trendingIndex: (s.trendingIndex + 1) % total,
        trendingDirection: "next",
      }));
    }, delay);
    return () => clearTimeout(id);
  }, [featuredItems.length, trendingIndex, trendingAutoAdvancePausedUntil]);

  if (trending.length === 0) {
    return marketsLoading ? <MarketSkeletons /> : <EmptyState />;
  }

  const featured = featuredItems[trendingIndex % featuredItems.length];
  const isGroup = (item: MarketGroup | Market): item is MarketGroup =>
    "outcomes" in item;

  return (
    <div className="phi-container py-6 lg:py-8">
      <div className="grid gap-8 xl:grid-cols-[1.618fr_0.8fr]">
        <section className="space-y-6">
          {/* Featured market carousel — groups + binary markets. Slide
              animation lives *inside* each featured card so the nav
              arrows stay stationary and only the content sweeps on
              advance. */}
          {isGroup(featured) ? (
            <FeaturedGroupCard
              group={featured}
              totalItems={featuredItems.length}
            />
          ) : (
            <FeaturedMarket
              market={featured}
              totalItems={featuredItems.length}
            />
          )}

          {/* All markets grid — binary + multi-outcome interleaved */}
          <section>
            <div className="mb-3 flex items-center justify-between">
              <h2 className="text-sm font-medium uppercase tracking-wider text-slate-500">
                All Markets
              </h2>
              <p className="text-sm text-slate-600">
                {topMarkets.length + marketGroups.length} shown
              </p>
            </div>
            <div className="grid gap-2 md:grid-cols-2">
              {marketGroups.map((group, idx) => (
                <MarketGroupCard key={group.id} group={group} index={idx} />
              ))}
              {topMarkets.map((market, idx) => (
                <MarketCard
                  key={market.id}
                  market={market}
                  index={marketGroups.length + idx}
                />
              ))}
            </div>
          </section>
        </section>

        {/* Right sidebar: Trending + Top Movers */}
        <aside className="grid gap-8 sm:grid-cols-2 xl:grid-cols-1 xl:content-start">
          {/* Trending sidebar */}
          <section>
            <h3 className="mb-3 flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wider text-slate-500">
              <CategoryIcon category="Trending" className="h-3.5 w-3.5" />
              Trending
            </h3>
            <div className="divide-y divide-slate-800/60">
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
          <section>
            <h3 className="mb-3 flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wider text-slate-500">
              <TopMoversIcon />
              Top movers
            </h3>
            <div className="divide-y divide-slate-800/60">
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
  const nostrPubkey = useStore((s) => s.nostrPubkey);

  const { data: markets = [] } = useMarkets();

  // Determine which view to render based on activeCategory
  const isTrendingLike =
    activeCategory === "Trending" ||
    activeCategory === "Ending Soon" ||
    activeCategory === "New" ||
    activeCategory === "Portfolio" ||
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

  // Portfolio
  if (activeCategory === "Portfolio") {
    return <PortfolioView markets={markets} />;
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
  return <TrendingHomeView markets={markets} />;
}
