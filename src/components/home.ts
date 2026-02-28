import { OPEN_WALLET_ACTION } from "../actions.ts";
import { chartSkeleton } from "../components/market-chart.ts";
import { markets, state } from "../state.ts";
import type { CovenantState, Market, MarketCategory } from "../types.ts";
import {
  formatPercent,
  formatProbabilityWithPercent,
  formatVolumeBtc,
} from "../utils/format.ts";
import {
  getFilteredMarkets,
  getTrendingMarkets,
  isExpired,
  stateBadge,
  stateLabel,
} from "../utils/market.ts";

const trendUp =
  '<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="shrink-0"><polyline points="22 7 13.5 15.5 8.5 10.5 2 17"/><polyline points="16 7 22 7 22 13"/></svg>';

const trendDown =
  '<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="shrink-0"><polyline points="22 17 13.5 8.5 8.5 13.5 2 7"/><polyline points="16 17 22 17 22 11"/></svg>';

function trendIndicator(change: number): string {
  const color = change >= 0 ? "text-emerald-300" : "text-rose-300";
  const arrow = change >= 0 ? trendUp : trendDown;
  return `<span class="inline-flex items-center gap-1 ${color}">${arrow}${formatPercent(change)}</span>`;
}

export function renderHome(): string {
  if (
    state.activeCategory !== "Trending" &&
    state.activeCategory !== "My Markets"
  ) {
    return renderCategoryPage();
  }

  if (state.activeCategory === "My Markets") {
    return renderMyMarkets();
  }

  if (state.marketsLoading) {
    return `
      <div class="phi-container py-16 text-center">
        <p class="text-lg text-slate-400">Discovering markets from Nostr relays...</p>
      </div>
    `;
  }

  const trending = getTrendingMarkets();

  if (trending.length === 0) {
    return `
      <div class="phi-container py-16 text-center">
        <h2 class="mb-3 text-2xl font-semibold text-slate-100">No markets discovered</h2>
        <p class="mb-6 text-base text-slate-400">Be the first to create a prediction market on Liquid Testnet.</p>
        ${
          state.walletStatus !== "unlocked"
            ? `
          <p class="mb-4 text-sm text-amber-300">Set up your wallet first to start trading</p>
          <button data-action="${OPEN_WALLET_ACTION}" class="mr-3 rounded-xl border border-slate-600 px-6 py-3 text-base font-medium text-slate-200">Set Up Wallet</button>
        `
            : ""
        }
        <button data-action="open-create-market" class="rounded-xl bg-emerald-300 px-6 py-3 text-base font-semibold text-slate-950"><span class="mr-1">+</span> Create New Market</button>
        ${state.nostrPubkey ? `<p class="mt-4 text-xs text-slate-500">Identity: ${state.nostrPubkey.slice(0, 8)}...${state.nostrPubkey.slice(-8)}</p>` : ""}
      </div>
    `;
  }

  const featured = trending[state.trendingIndex % trending.length];
  const featuredNo = featured.yesPrice != null ? 1 - featured.yesPrice : null;
  const topMarkets = getFilteredMarkets().slice(0, 6);
  const topMovers = [...markets]
    .sort((a, b) => Math.abs(b.change24h) - Math.abs(a.change24h))
    .slice(0, 3);

  return `
    <div class="phi-container py-6 lg:py-8">
      <div class="grid gap-[21px] xl:grid-cols-[1.618fr_1fr]">
        <section class="space-y-[21px]">
          <div class="rounded-[21px] border border-slate-800 bg-slate-950/60 p-[21px] lg:p-[34px]">
            <div class="mb-5 flex items-start justify-between gap-3">
              <h1 class="phi-title text-2xl font-medium leading-tight text-slate-100 lg:text-[34px]">${featured.question}</h1>
              <div class="flex items-center gap-2">
                <button data-action="trending-prev" class="h-11 w-11 rounded-full border border-slate-700 text-xl text-slate-200">&#8249;</button>
                <p class="w-20 text-center text-sm font-normal text-slate-300">${state.trendingIndex + 1} of ${trending.length}</p>
                <button data-action="trending-next" class="h-11 w-11 rounded-full border border-slate-700 text-xl text-slate-200">&#8250;</button>
              </div>
            </div>

            <div class="grid gap-[21px] lg:grid-cols-[1fr_1.618fr]">
              <div>
                <p class="mb-3 text-sm font-medium ${featured.isLive ? "text-rose-300" : "text-slate-400"}">${featured.isLive ? "Live" : "Scheduled"}</p>
                <div class="mb-3 grid grid-cols-2 gap-2 text-xs text-slate-400">
                  <div class="rounded-lg bg-slate-900/60 p-2">State<br/><span class="text-slate-200">${stateLabel(featured.state)}</span></div>
                  <div class="rounded-lg bg-slate-900/60 p-2">Volume<br/><span class="text-slate-200">${formatVolumeBtc(featured.volumeBtc)}</span></div>
                </div>
                <div class="space-y-3 text-lg text-slate-200">
                  <div class="flex items-center justify-between"><span>Yes contract</span><button data-open-market="${featured.id}" data-open-side="yes" data-open-intent="buy" class="rounded-full border border-emerald-600 px-4 py-1 text-emerald-300 transition hover:bg-emerald-500/10">${featured.yesPrice != null ? formatProbabilityWithPercent(featured.yesPrice) : "\u2014"}</button></div>
                  <div class="flex items-center justify-between"><span>No contract</span><button data-open-market="${featured.id}" data-open-side="no" data-open-intent="buy" class="rounded-full border border-rose-600 px-4 py-1 text-rose-300 transition hover:bg-rose-500/10">${featuredNo != null ? formatProbabilityWithPercent(featuredNo) : "\u2014"}</button></div>
                </div>
                <p class="mt-3 text-[15px] text-slate-400">${featured.description}</p>
                <button data-open-market="${featured.id}" class="mt-5 rounded-xl bg-emerald-300 px-5 py-2.5 text-base font-medium text-slate-950">Open contract</button>
              </div>
              <div>${chartSkeleton(featured, "home")}</div>
            </div>
          </div>

          <section>
            <div class="mb-3 flex items-center justify-between">
              <h2 class="text-base font-medium text-slate-400">Top Markets</h2>
              <p class="text-sm text-slate-400">${topMarkets.length} shown</p>
            </div>
            <div class="grid gap-3 md:grid-cols-2">
              ${topMarkets
                .map((market) => {
                  const no =
                    market.yesPrice != null ? 1 - market.yesPrice : null;
                  return `
                    <button data-open-market="${market.id}" class="rounded-2xl border border-slate-800 bg-slate-950/55 p-4 text-left transition hover:border-slate-600">
                      <p class="mb-2 text-xs text-slate-500">${market.category} ${market.isLive ? "· LIVE" : ""}</p>
                      <p class="mb-3 max-h-14 overflow-hidden text-base font-normal text-slate-200">${market.question}</p>
                      <div class="flex items-center justify-between text-xs sm:text-sm">
                        <span class="text-emerald-300">Yes ${market.yesPrice != null ? formatProbabilityWithPercent(market.yesPrice) : "\u2014"}</span>
                        <span class="text-rose-300">No ${no != null ? formatProbabilityWithPercent(no) : "\u2014"}</span>
                        ${trendIndicator(market.change24h)}
                      </div>
                    </button>
                  `;
                })
                .join("")}
            </div>
          </section>
        </section>

        <aside class="grid gap-[13px] sm:grid-cols-2 xl:grid-cols-1">
          <section class="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px]">
            <h3 class="mb-3 text-base font-medium text-slate-400">Trending</h3>
            <div class="space-y-4">
              ${trending
                .slice(0, 3)
                .map((market, idx) => {
                  return `
                    <button data-open-market="${market.id}" class="w-full text-left">
                      <div class="flex items-start justify-between gap-2">
                        <p class="w-full text-sm font-normal text-slate-300">${idx + 1}. ${market.question}</p>
                        <p class="text-sm font-normal text-slate-100">${market.yesPrice != null ? `${Math.round(market.yesPrice * 100)}%` : "\u2014"}</p>
                      </div>
                      <div class="mt-1 flex items-center justify-between">
                        <p class="text-xs text-slate-500">${market.category}</p>
                        <p class="text-xs">${trendIndicator(market.change24h)}</p>
                      </div>
                    </button>
                  `;
                })
                .join("")}
            </div>
          </section>

          <section class="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px]">
            <h3 class="mb-3 text-base font-medium text-slate-400">Top movers</h3>
            <div class="space-y-4">
              ${topMovers
                .map((market, idx) => {
                  return `
                    <button data-open-market="${market.id}" class="w-full text-left">
                      <div class="flex items-start justify-between gap-2">
                        <p class="w-full text-sm font-normal text-slate-300">${idx + 1}. ${market.question}</p>
                        <p class="text-sm font-normal">${trendIndicator(market.change24h)}</p>
                      </div>
                      <p class="mt-1 text-xs text-slate-500">${market.category}</p>
                    </button>
                  `;
                })
                .join("")}
            </div>
          </section>
        </aside>
      </div>
    </div>
  `;
}

export function renderMyMarkets(): string {
  const myMarkets = getFilteredMarkets();

  if (myMarkets.length === 0) {
    return `
      <div class="phi-container py-16 text-center">
        <h2 class="mb-3 text-2xl font-semibold text-slate-100">No markets created yet</h2>
        <p class="mb-6 text-base text-slate-400">Markets you create as oracle will appear here.</p>
        <button data-action="open-create-market" class="rounded-xl bg-emerald-300 px-6 py-3 text-base font-semibold text-slate-950"><span class="mr-1">+</span> Create New Market</button>
      </div>
    `;
  }

  const dormant = myMarkets.filter((m) => m.state === 0);
  const active = myMarkets.filter((m) => m.state === 1);
  const resolved = myMarkets.filter((m) => m.state === 2 || m.state === 3);

  const renderMarketCard = (market: Market): string => {
    const no = market.yesPrice != null ? 1 - market.yesPrice : null;
    return `
      <button data-open-market="${market.id}" class="rounded-2xl border border-slate-800 bg-slate-950/55 p-4 text-left transition hover:border-slate-600">
        <div class="mb-2 flex items-center justify-between text-sm">
          <span class="text-xs text-slate-500">${market.category}</span>
          <span>${stateBadge(market.state)}</span>
        </div>
        <p class="mb-3 text-base font-normal text-slate-200">${market.question}</p>
        <div class="flex items-center justify-between text-sm">
          <span class="text-emerald-300">Yes ${market.yesPrice != null ? formatProbabilityWithPercent(market.yesPrice) : "\u2014"}</span>
          <span class="text-rose-300">No ${no != null ? formatProbabilityWithPercent(no) : "\u2014"}</span>
        </div>
      </button>
    `;
  };

  const renderSection = (title: string, items: Market[]): string => {
    if (items.length === 0) return "";
    return `
      <div class="mb-6">
        <h3 class="mb-3 text-sm font-medium text-slate-400">${title} (${items.length})</h3>
        <div class="grid gap-3 md:grid-cols-2">${items.map(renderMarketCard).join("")}</div>
      </div>
    `;
  };

  return `
    <div class="phi-container py-6 lg:py-8">
      <div class="mb-4 flex items-center justify-between">
        <h1 class="text-xl font-medium text-slate-100">My Markets</h1>
        <button data-action="open-create-market" class="rounded-xl bg-emerald-300 px-5 py-2 text-sm font-semibold text-slate-950"><span class="mr-1">+</span> Create New Market</button>
      </div>
      <div class="mb-4 grid gap-2 sm:grid-cols-3">
        <div class="rounded-xl border border-slate-800 bg-slate-950/45 p-3">
          <p class="text-xs text-slate-500">Total</p>
          <p class="text-lg font-medium text-slate-100">${myMarkets.length}</p>
        </div>
        <div class="rounded-xl border border-slate-800 bg-slate-950/45 p-3">
          <p class="text-xs text-slate-500">Active</p>
          <p class="text-lg font-medium text-emerald-300">${active.length}</p>
        </div>
        <div class="rounded-xl border border-slate-800 bg-slate-950/45 p-3">
          <p class="text-xs text-slate-500">Awaiting resolution</p>
          <p class="text-lg font-medium text-amber-300">${active.filter((m) => isExpired(m)).length}</p>
        </div>
      </div>
      ${renderSection("Dormant — needs initial issuance", dormant)}
      ${renderSection("Active", active)}
      ${renderSection("Resolved", resolved)}
    </div>
  `;
}

export function renderCategoryPage(): string {
  const category = state.activeCategory as MarketCategory;
  const categoryMarkets = getFilteredMarkets();
  const liveContracts = categoryMarkets
    .filter((market) => market.isLive)
    .slice(0, 4);
  const highestLiquidity = [...categoryMarkets]
    .sort((a, b) => b.liquidityBtc - a.liquidityBtc)
    .slice(0, 4);
  const stateMix = categoryMarkets.reduce(
    (acc, market) => {
      acc[market.state] += 1;
      return acc;
    },
    { 0: 0, 1: 0, 2: 0, 3: 0 } as Record<CovenantState, number>,
  );

  return `
    <div class="phi-container py-6 lg:py-8">
      <div class="grid gap-[21px] xl:grid-cols-[233px_1fr_320px]">
        <aside class="hidden xl:block">
          <div class="space-y-1 text-sm text-slate-400">
            <button class="block w-full rounded-md bg-slate-900/70 px-2 py-2 text-left text-emerald-300">All markets</button>
            <button class="block w-full rounded-md px-2 py-2 text-left hover:bg-slate-900/40 hover:text-slate-200">Live now</button>
            <button class="block w-full rounded-md px-2 py-2 text-left hover:bg-slate-900/40 hover:text-slate-200">Resolved soon</button>
          </div>
        </aside>
        <section>
          <div class="mb-4 flex items-center justify-between">
            <h1 class="text-xl font-medium text-slate-100">${category}</h1>
            <div class="flex items-center gap-2 text-sm text-slate-400">
              <button class="rounded-full border border-slate-700 px-3 py-1.5">Trending</button>
              <button class="rounded-full border border-slate-700 px-3 py-1.5">Frequency</button>
            </div>
          </div>
          <div class="mb-4 grid gap-2 sm:grid-cols-3">
            <div class="rounded-xl border border-slate-800 bg-slate-950/45 p-3">
              <p class="text-xs text-slate-500">Contracts</p>
              <p class="text-lg font-medium text-slate-100">${categoryMarkets.length}</p>
            </div>
            <div class="rounded-xl border border-slate-800 bg-slate-950/45 p-3">
              <p class="text-xs text-slate-500">Live now</p>
              <p class="text-lg font-medium text-rose-300">${liveContracts.length}</p>
            </div>
            <div class="rounded-xl border border-slate-800 bg-slate-950/45 p-3">
              <p class="text-xs text-slate-500">24h volume</p>
              <p class="text-lg font-medium text-slate-100">${formatVolumeBtc(
                categoryMarkets.reduce(
                  (sum, market) => sum + market.volumeBtc,
                  0,
                ),
              )}</p>
            </div>
          </div>
          <div class="grid gap-3 md:grid-cols-2">
            ${categoryMarkets
              .map((market) => {
                const no = market.yesPrice != null ? 1 - market.yesPrice : null;
                return `
                  <button data-open-market="${market.id}" class="rounded-2xl border border-slate-800 bg-slate-950/55 p-4 text-left transition hover:border-slate-600">
                    <div class="mb-2 flex items-center justify-between text-sm">
                      <span class="text-xs text-slate-500">${market.category}</span>
                      <span class="${market.isLive ? "text-rose-300" : "text-slate-500"}">${market.isLive ? "LIVE" : "SCHEDULED"}</span>
                    </div>
                    <p class="mb-3 text-base font-normal text-slate-200">${market.question}</p>
                    <div class="flex items-center justify-between text-sm">
                      <span class="text-emerald-300">Yes ${market.yesPrice != null ? formatProbabilityWithPercent(market.yesPrice) : "\u2014"}</span>
                      <span class="text-rose-300">No ${no != null ? formatProbabilityWithPercent(no) : "\u2014"}</span>
                      ${trendIndicator(market.change24h)}
                    </div>
                    <p class="mt-2 text-xs text-slate-500">Volume ${formatVolumeBtc(market.volumeBtc)} · ${market.description}</p>
                  </button>
                `;
              })
              .join("")}
          </div>
        </section>
        <aside class="space-y-3">
          <section class="rounded-2xl border border-slate-800 bg-slate-950/55 p-4">
            <h3 class="mb-3 text-sm font-medium text-slate-400">Live contracts</h3>
            <div class="space-y-3">
              ${
                liveContracts.length
                  ? liveContracts
                      .map(
                        (market) => `
                      <button data-open-market="${market.id}" class="w-full text-left">
                        <p class="text-sm font-normal text-slate-300">${market.question}</p>
                        <p class="mt-1 text-xs text-slate-500">${market.yesPrice != null ? `Yes ${Math.round(market.yesPrice * 100)}% · ` : ""}${formatVolumeBtc(market.volumeBtc)} volume</p>
                      </button>`,
                      )
                      .join("")
                  : '<p class="text-sm text-slate-500">No live contracts in this category right now.</p>'
              }
            </div>
          </section>
          <section class="rounded-2xl border border-slate-800 bg-slate-950/55 p-4">
            <h3 class="mb-3 text-sm font-medium text-slate-400">Highest liquidity</h3>
            <div class="space-y-3">
              ${highestLiquidity
                .map(
                  (market, idx) => `
                <button data-open-market="${market.id}" class="flex w-full items-start justify-between gap-2 text-left">
                  <p class="text-sm text-slate-300">${idx + 1}. ${market.question}</p>
                  <p class="text-sm font-normal text-emerald-300">${formatVolumeBtc(market.liquidityBtc)}</p>
                </button>`,
                )
                .join("")}
            </div>
          </section>
          <section class="rounded-2xl border border-slate-800 bg-slate-950/55 p-4">
            <h3 class="mb-3 text-sm font-medium text-slate-400">State mix</h3>
            <div class="space-y-2 text-sm text-slate-300">
              <p class="flex items-center justify-between"><span>State 0 · Uninitialized</span><span>${stateMix[0]}</span></p>
              <p class="flex items-center justify-between"><span>State 1 · Unresolved</span><span>${stateMix[1]}</span></p>
              <p class="flex items-center justify-between"><span>State 2 · Resolved YES</span><span>${stateMix[2]}</span></p>
              <p class="flex items-center justify-between"><span>State 3 · Resolved NO</span><span>${stateMix[3]}</span></p>
            </div>
          </section>
        </aside>
      </div>
    </div>
  `;
}
