import { useMemo } from "react";
import { useMarkets } from "../../queries/useMarkets";
import { useStore } from "../../store";
import type { CovenantState, Market, NavCategory } from "../../types";
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
    case "Portfolio":
      return (
        <svg aria-hidden="true" {...props}>
          <path d="M3 3v18h18" />
          <rect x="7" y="13" width="9" height="4" rx="1" />
          <rect x="7" y="5" width="12" height="4" rx="1" />
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

// ── Market loader (cat bag animation) ────────────────────────────────
function MarketLoader() {
  return (
    <div className="phi-container flex flex-col items-center justify-center py-24">
      <div className="deadcat-loader">
        <div className="deadcat-loader-scene">
          <div className="deadcat-loader-clip">
            <div className="deadcat-loader-cat">
              <CatLoaderSvg />
            </div>
          </div>
          <div className="deadcat-loader-bag">
            <BagLoaderSvg />
          </div>
        </div>
        <p className="mt-4 text-sm text-slate-500 animate-pulse">
          Fetching markets...
        </p>
      </div>
    </div>
  );
}

function CatLoaderSvg() {
  return (
    <svg
      viewBox="0 0 260 267"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden="true"
    >
      <path
        fillRule="evenodd"
        clipRule="evenodd"
        d="M0.146484 9.04605C0.146484 1.23441 10.9146 -3.16002 16.7881 2.6984L86.5566 71.7336C100.142 68.0294 114.765 66.0128 130 66.0128C145.239 66.0128 159.865 68.0306 173.453 71.7365L243.212 2.71207C249.085 -3.14676 259.854 1.24698 259.854 9.05875V161.26C259.949 162.835 260 164.42 260 166.013C260 221.241 201.797 266.013 130 266.013C58.203 266.013 0 221.241 0 166.013C1.54644e-06 164.42 0.0506677 162.835 0.146484 161.26V9.04605ZM100.287 187.013L120.892 207.087V208.903C120.892 217.907 114.199 225.23 105.974 225.231H91.0049C87.1409 225.231 84.0001 228.319 84 232.118C84 235.918 87.1446 239.013 91.0049 239.013H105.974C114.534 239.013 122.574 235.049 128.02 228.383C133.461 235.045 141.502 239.013 150.065 239.013C166.019 239.013 179 225.506 179 208.903C179 205.104 175.856 202.013 171.992 202.013C168.128 202.013 164.984 205.104 164.983 208.903C164.983 217.907 158.291 225.231 150.065 225.231C141.84 225.231 135.147 217.907 135.147 208.903V207.049L155.713 187.013H100.287ZM70.4697 140.12L52.4219 122.072L44 130.495L62.0469 148.542L44.0596 166.53L52.4824 174.953L70.4697 156.965L88.5176 175.013L96.9404 166.591L78.8916 148.542L97 130.435L88.5781 122.013L70.4697 140.12ZM195.367 123.557C200.554 128.783 204 138.006 204 148.513C204 158.3 201.01 166.973 196.408 172.339C216.243 169.73 231 159.83 231 148.013C231 135.99 215.724 125.951 195.367 123.557ZM175.489 123.7C155.707 126.33 141 136.217 141 148.013C141 159.603 155.197 169.349 174.456 172.181C169.931 166.803 167 158.204 167 148.513C167 138.102 170.382 128.951 175.489 123.7Z"
        fill="#34d399"
      />
    </svg>
  );
}

function BagLoaderSvg() {
  return (
    <svg
      viewBox="0 0 298 376"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden="true"
    >
      <path
        fillRule="evenodd"
        clipRule="evenodd"
        d="M11.897 1.04645L36.1423 14.835L60.3877 1.04645C62.8491 -0.348818 65.8439 -0.348818 68.3053 1.04645L92.5507 14.835L116.796 1.04645C119.257 -0.348818 122.252 -0.348818 124.714 1.04645L148.959 14.835L173.204 1.04645C175.666 -0.348818 178.661 -0.348818 181.122 1.04645L205.367 14.835L229.613 1.04645C232.074 -0.348818 235.069 -0.348818 237.53 1.04645L261.776 14.835L286.021 1.04645C288.523 -0.348818 291.559 -0.348818 294.021 1.08749C296.482 2.5238 298 5.1502 298 8.02282V350.973C298 364.228 287.252 375.02 273.96 375.02H24.0402C10.7894 375.02 0 364.269 0 350.973V8.02282C0 5.19123 1.5179 2.56484 3.97935 1.12853C6.4408 -0.307781 9.4766 -0.307781 11.9791 1.08749H11.9381L11.897 1.04645Z"
        fill="#1e293b"
      />
    </svg>
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
          <CategoryIcon category="Portfolio" className="h-5 w-5" />
          Portfolio
        </h1>
      </div>

      <div className="mb-6 grid gap-3 sm:grid-cols-3">
        <div className="rounded-xl border border-slate-800 bg-slate-950/45 px-4 py-3">
          <p className="text-xs uppercase tracking-wider text-slate-500">
            Portfolio Value
          </p>
          <p className="text-2xl font-semibold text-slate-100">
            {formatSats(totalPortfolioValue)}
          </p>
        </div>
        <div className="rounded-xl border border-slate-800 bg-slate-950/45 px-4 py-3">
          <p className="text-xs uppercase tracking-wider text-slate-500">
            Active Positions
          </p>
          <p className="text-2xl font-semibold text-emerald-300">
            {activePositions.length}
          </p>
        </div>
        <div className="rounded-xl border border-slate-800 bg-slate-950/45 px-4 py-3">
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
                  className="block w-full rounded-2xl border border-slate-800 bg-slate-950/55 p-4 text-left transition hover:border-slate-600"
                >
                  <div className="mb-2 flex items-center justify-between">
                    <span className="text-xs text-slate-500">
                      {p.market.category} · {timeLeft}
                    </span>
                  </div>
                  <p className="mb-3 text-sm text-slate-200">
                    {p.market.question}
                  </p>
                  <div className="flex flex-wrap items-center gap-3">
                    {p.pos.yes > 0 && (
                      <div className="rounded-lg border border-emerald-500/30 bg-emerald-500/10 px-3 py-1.5">
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
                      <div className="rounded-lg border border-rose-500/30 bg-rose-500/10 px-3 py-1.5">
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
                  className="block w-full rounded-2xl border border-slate-800 bg-slate-950/55 p-4 text-left transition hover:border-slate-600"
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
  const categorySortMode = useStore((s) => s.categorySortMode);
  const categoryFilter = useStore((s) => s.categoryFilter);

  const categoryMarkets = useMemo(
    () => getFilteredMarkets(markets, search, category, nostrPubkey),
    [markets, search, category, nostrPubkey],
  );

  const filteredMarkets = useMemo(() => {
    if (categoryFilter === "live") return categoryMarkets.filter((m) => m.isLive);
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
      <div className="grid gap-[21px] xl:grid-cols-[233px_1fr_320px]">
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
                  className={`block w-full rounded-md px-2 py-2 text-left transition ${active ? "bg-slate-900/70 text-emerald-300" : "hover:bg-slate-900/40 hover:text-slate-200"}`}
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
              {category}
            </h1>
            <div className="flex items-center gap-2 text-sm text-slate-400">
              {(["trending", "frequency"] as const).map((mode) => (
                <button
                  key={mode}
                  type="button"
                  onClick={() => useStore.setState({ categorySortMode: mode })}
                  className={`rounded-full border px-3 py-1.5 capitalize transition ${categorySortMode === mode ? "border-slate-500 text-slate-100" : "border-slate-700 hover:border-slate-500 hover:text-slate-300"}`}
                >
                  {mode}
                </button>
              ))}
            </div>
          </div>
          <div className="mb-4 grid gap-2 sm:grid-cols-3">
            <div className="rounded-xl border border-slate-800 bg-slate-950/45 p-3">
              <p className="text-xs text-slate-500">Contracts</p>
              <p className="text-lg font-medium text-slate-100">
                {sortedMarkets.length}
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
            {sortedMarkets.map((market, idx) => (
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
              Market states
            </h3>
            <div className="space-y-2 text-sm text-slate-300">
              <p className="flex items-center justify-between">
                <span>Dormant</span>
                <span>{stateMix[0]}</span>
              </p>
              <p className="flex items-center justify-between">
                <span className="text-emerald-400">Live</span>
                <span>{stateMix[1]}</span>
              </p>
              <p className="flex items-center justify-between">
                <span>Resolved YES</span>
                <span>{stateMix[2]}</span>
              </p>
              <p className="flex items-center justify-between">
                <span>Resolved NO</span>
                <span>{stateMix[3]}</span>
              </p>
              <p className="flex items-center justify-between">
                <span className="text-amber-400">Expired</span>
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
  const nostrPubkey = useStore((s) => s.nostrPubkey);

  const { data: markets = [], isLoading } = useMarkets();

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
  if (isLoading && markets.length === 0) {
    return <MarketLoader />;
  }

  return <TrendingHomeView markets={markets} />;
}
