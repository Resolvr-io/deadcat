import { OPEN_WALLET_ACTION } from "../actions.ts";
import { chartSkeleton } from "../components/market-chart.ts";
import { categoryIcon } from "../components/shell.ts";
import { markets, state } from "../state.ts";
import type { CovenantState, Market, MarketCategory } from "../types.ts";
import {
  formatPercent,
  formatProbabilityWithPercent,
  formatTimeRemaining,
  formatVolumeBtc,
} from "../utils/format.ts";
import { escapeAttr, escapeHtml } from "../utils/html.ts";
import {
  getEndingSoonMarkets,
  getFilteredMarkets,
  getNewMarkets,
  getTrendingMarkets,
  isExpired,
  stateBadge,
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

function marketCard(market: Market, idx: number): string {
  const no = market.yesPrice != null ? 1 - market.yesPrice : null;
  const yesPct =
    market.yesPrice != null ? Math.round(market.yesPrice * 100) : null;
  const noPct = no != null ? Math.round(no * 100) : null;
  const blocksLeft = market.expiryHeight - market.currentHeight;
  const timeLeft = blocksLeft > 0 ? formatTimeRemaining(blocksLeft) : "Expired";
  return `
    <div class="market-card rounded-2xl border border-slate-800 bg-slate-950/55 p-4 text-left transition hover:border-slate-600" style="animation-delay:${idx * 50}ms">
      <button data-open-market="${escapeAttr(market.id)}" class="w-full text-left">
        <div class="mb-2 flex items-center justify-between text-xs">
          <span class="text-slate-500">${escapeHtml(market.category)} ${market.isLive ? '<span class="text-emerald-400">· LIVE</span>' : ""}</span>
          <span class="text-slate-500">${timeLeft}</span>
        </div>
        <p class="mb-2 max-h-12 overflow-hidden text-sm font-normal text-slate-200">${escapeHtml(market.question)}</p>
        ${yesPct != null ? `<p class="mb-2 text-2xl font-semibold text-slate-100">${yesPct}<span class="text-base text-slate-400">%</span></p>` : ""}
      </button>
      <div class="flex items-center gap-2">
        ${
          market.state === 2 || market.state === 3
            ? `<span class="w-48 rounded-full ${market.state === 2 ? "bg-emerald-500/20 text-emerald-300" : "bg-rose-500/20 text-rose-300"} px-3 py-1 text-center text-sm font-medium">${market.state === 2 ? "Resolved YES" : "Resolved NO"}</span>`
            : market.state === 4
              ? `<span class="w-48 rounded-full bg-amber-500/20 px-3 py-1 text-center text-sm font-medium text-amber-300">Expired</span>`
              : `<button data-open-market="${escapeAttr(market.id)}" data-open-side="yes" data-open-intent="buy" class="w-24 rounded-full bg-emerald-500 px-3 py-1 text-center text-sm font-medium text-white transition hover:bg-emerald-400">${yesPct != null ? `Yes ${yesPct}%` : "Buy Yes"}</button>
              <button data-open-market="${escapeAttr(market.id)}" data-open-side="no" data-open-intent="buy" class="w-24 rounded-full bg-rose-500 px-3 py-1 text-center text-sm font-medium text-white transition hover:bg-rose-400">${noPct != null ? `No ${noPct}%` : "Buy No"}</button>`
        }
        <span class="ml-auto text-xs">${trendIndicator(market.change24h)}</span>
      </div>
      <p class="mt-2 text-xs text-slate-500">${formatVolumeBtc(market.volumeBtc)} vol · ${timeLeft}</p>
    </div>`;
}

export function renderHome(): string {
  if (
    state.activeCategory !== "Trending" &&
    state.activeCategory !== "Ending Soon" &&
    state.activeCategory !== "New" &&
    state.activeCategory !== "My Markets"
  ) {
    return renderCategoryPage();
  }

  if (state.activeCategory === "My Markets") {
    return renderMyMarkets();
  }

  if (state.activeCategory === "Ending Soon") {
    return renderSortedView("Ending Soon", getEndingSoonMarkets());
  }

  if (state.activeCategory === "New") {
    return renderSortedView("New", getNewMarkets());
  }

  if (state.marketsLoading) {
    return `
      <div class="phi-container py-6 lg:py-8">
        <div class="grid gap-[21px] xl:grid-cols-[1.618fr_1fr]">
          <div class="space-y-[21px]">
            <div class="rounded-[21px] border border-slate-800 bg-slate-950/60 p-[21px] lg:p-[34px] space-y-5">
              <div class="flex items-start justify-between gap-3">
                <div class="space-y-3 flex-1">
                  <div class="h-8 w-3/4 animate-pulse rounded-lg bg-slate-800/60"></div>
                  <div class="h-5 w-1/2 animate-pulse rounded bg-slate-800/60"></div>
                </div>
                <div class="h-11 w-28 animate-pulse rounded-full bg-slate-800/60"></div>
              </div>
              <div class="grid gap-[21px] lg:grid-cols-[1fr_1.618fr]">
                <div class="space-y-3">
                  <div class="h-4 w-16 animate-pulse rounded bg-slate-800/60"></div>
                  <div class="grid grid-cols-2 gap-2">
                    <div class="h-14 animate-pulse rounded-lg bg-slate-900/60"></div>
                    <div class="h-14 animate-pulse rounded-lg bg-slate-900/60"></div>
                  </div>
                  <div class="h-10 w-full animate-pulse rounded-full bg-slate-800/60"></div>
                  <div class="h-10 w-full animate-pulse rounded-full bg-slate-800/60"></div>
                  <div class="h-16 w-full animate-pulse rounded bg-slate-800/60"></div>
                </div>
                <div class="h-48 animate-pulse rounded-lg bg-slate-900/40"></div>
              </div>
            </div>
            <div>
              <div class="mb-3 flex items-center justify-between">
                <div class="h-5 w-28 animate-pulse rounded bg-slate-800/60"></div>
                <div class="h-4 w-16 animate-pulse rounded bg-slate-800/60"></div>
              </div>
              <div class="grid gap-3 md:grid-cols-2">
                ${Array.from({ length: 4 })
                  .map(
                    () => `
                  <div class="rounded-2xl border border-slate-800 bg-slate-950/55 p-4 space-y-3">
                    <div class="h-3 w-20 animate-pulse rounded bg-slate-800/60"></div>
                    <div class="h-5 w-full animate-pulse rounded bg-slate-800/60"></div>
                    <div class="flex justify-between">
                      <div class="h-7 w-20 animate-pulse rounded-full bg-slate-800/60"></div>
                      <div class="h-7 w-20 animate-pulse rounded-full bg-slate-800/60"></div>
                    </div>
                  </div>`,
                  )
                  .join("")}
              </div>
            </div>
          </div>
          <aside class="grid gap-[13px] sm:grid-cols-2 xl:grid-cols-1">
            ${Array.from({ length: 2 })
              .map(
                () => `
              <div class="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px] space-y-4">
                <div class="h-5 w-24 animate-pulse rounded bg-slate-800/60"></div>
                <div class="h-4 w-full animate-pulse rounded bg-slate-800/60"></div>
                <div class="h-4 w-5/6 animate-pulse rounded bg-slate-800/60"></div>
                <div class="h-4 w-full animate-pulse rounded bg-slate-800/60"></div>
              </div>`,
              )
              .join("")}
          </aside>
        </div>
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
        ${state.marketMakerMode ? `<button data-action="open-create-market" class="rounded-xl bg-emerald-300 px-6 py-3 text-base font-semibold text-slate-950"><span class="mr-1">+</span> Create New Market</button>` : ""}
        ${state.nostrPubkey ? `<p class="mt-4 text-xs text-slate-500">Identity: ${escapeHtml(state.nostrPubkey.slice(0, 8))}...${escapeHtml(state.nostrPubkey.slice(-8))}</p>` : ""}
      </div>
    `;
  }

  const featured = trending[state.trendingIndex % trending.length];
  const featuredNo = featured.yesPrice != null ? 1 - featured.yesPrice : null;
  const featuredYesPct =
    featured.yesPrice != null ? Math.round(featured.yesPrice * 100) : null;
  const featuredNoPct =
    featuredNo != null ? Math.round(featuredNo * 100) : null;
  const featuredBlocksLeft = featured.expiryHeight - featured.currentHeight;
  const featuredTimeLeft =
    featuredBlocksLeft > 0
      ? formatTimeRemaining(featuredBlocksLeft)
      : "Expired";
  const topMarkets = getFilteredMarkets().slice(0, 6);
  const topMovers = [...markets]
    .sort((a, b) => Math.abs(b.change24h) - Math.abs(a.change24h))
    .slice(0, 3);

  return `
    <div class="phi-container py-6 lg:py-8">
      <div class="grid gap-[21px] xl:grid-cols-[1.618fr_1fr]">
        <section class="space-y-[21px]">
          <div class="rounded-[21px] border border-slate-800 bg-slate-950/60 p-[21px] lg:p-[34px]">
            <div class="mb-4 flex items-center justify-between gap-3">
              <div class="flex items-center gap-2 text-xs">
                <span class="text-slate-500">${escapeHtml(featured.category)}</span>
                ${featured.isLive ? '<span class="rounded-full bg-emerald-500/20 px-2 py-0.5 text-emerald-300">LIVE</span>' : ""}
                ${stateBadge(featured.state)}
              </div>
              <div class="flex items-center gap-2">
                <button data-action="trending-prev" class="h-9 w-9 rounded-full border border-slate-700 text-lg text-slate-300 hover:bg-slate-800">&#8249;</button>
                <p class="w-16 text-center text-xs text-slate-400">${state.trendingIndex + 1} of ${trending.length}</p>
                <button data-action="trending-next" class="h-9 w-9 rounded-full border border-slate-700 text-lg text-slate-300 hover:bg-slate-800">&#8250;</button>
              </div>
            </div>

            <button data-open-market="${escapeAttr(featured.id)}" class="mb-4 block text-left">
              <h1 class="phi-title text-2xl font-medium leading-tight text-slate-100 hover:text-white transition lg:text-[34px]">${escapeHtml(featured.question)}</h1>
            </button>

            <div class="mb-4 flex flex-wrap items-center gap-3">
              ${
                featured.state === 2 || featured.state === 3
                  ? `<span class="w-40 rounded-full ${featured.state === 2 ? "bg-emerald-500/20 text-emerald-300" : "bg-rose-500/20 text-rose-300"} px-4 py-2 text-center text-base font-semibold">${featured.state === 2 ? "Resolved YES" : "Resolved NO"}</span>`
                  : featured.state === 4
                    ? `<span class="w-40 rounded-full bg-amber-500/20 px-4 py-2 text-center text-base font-semibold text-amber-300">Expired</span>`
                    : `<button data-open-market="${escapeAttr(featured.id)}" data-open-side="yes" data-open-intent="buy" class="w-32 rounded-full bg-emerald-500 px-4 py-2 text-center text-base font-semibold text-white transition hover:bg-emerald-400">${featuredYesPct != null ? `Yes ${featuredYesPct}%` : "Buy Yes"}</button>
              <button data-open-market="${escapeAttr(featured.id)}" data-open-side="no" data-open-intent="buy" class="w-32 rounded-full bg-rose-500 px-4 py-2 text-center text-base font-semibold text-white transition hover:bg-rose-400">${featuredNoPct != null ? `No ${featuredNoPct}%` : "Buy No"}</button>`
              }
              <span class="text-xs text-slate-500">${formatVolumeBtc(featured.volumeBtc)} vol · ${featuredTimeLeft} · ${trendIndicator(featured.change24h)}</span>
            </div>
            <p class="mb-4 text-sm text-slate-400 line-clamp-2">${escapeHtml(featured.description)}</p>
            <div>${chartSkeleton(featured, "home")}</div>
          </div>

          <section>
            <div class="mb-3 flex items-center justify-between">
              <h2 class="text-base font-medium text-slate-400">Top Markets</h2>
              <p class="text-sm text-slate-400">${topMarkets.length} shown</p>
            </div>
            <div class="grid gap-3 md:grid-cols-2">
              ${topMarkets
                .map((market, idx) => marketCard(market, idx))
                .join("")}
            </div>
          </section>
        </section>

        <aside class="grid gap-[13px] sm:grid-cols-2 xl:grid-cols-1">
          <section class="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px]">
            <h3 class="mb-3 flex items-center gap-1.5 text-base font-medium text-slate-400">${categoryIcon("Trending", "h-4 w-4")}Trending</h3>
            <div class="divide-y divide-slate-800">
              ${trending
                .slice(0, 3)
                .map((market, idx) => {
                  const yesPct =
                    market.yesPrice != null
                      ? Math.round(market.yesPrice * 100)
                      : null;
                  const noPct =
                    market.yesPrice != null
                      ? Math.round((1 - market.yesPrice) * 100)
                      : null;
                  return `
                    <div class="w-full py-3 text-left">
                      <button data-open-market="${escapeAttr(market.id)}" class="w-full text-left">
                        <p class="mb-1 text-sm font-normal text-slate-300">${idx + 1}. ${escapeHtml(market.question)}</p>
                      </button>
                      <div class="flex items-center gap-2">
                        ${
                          market.state === 2 || market.state === 3
                            ? `<span class="rounded-full ${market.state === 2 ? "bg-emerald-500/20 text-emerald-300" : "bg-rose-500/20 text-rose-300"} px-2.5 py-0.5 text-xs font-medium">${market.state === 2 ? "Resolved YES" : "Resolved NO"}</span>`
                            : market.state === 4
                              ? `<span class="rounded-full bg-amber-500/20 px-2.5 py-0.5 text-xs font-medium text-amber-300">Expired</span>`
                              : `<button data-open-market="${escapeAttr(market.id)}" data-open-side="yes" data-open-intent="buy" class="rounded-full bg-emerald-500 px-2.5 py-0.5 text-xs font-medium text-white transition hover:bg-emerald-400">${yesPct != null ? `Yes ${yesPct}%` : "Buy Yes"}</button>
                        <button data-open-market="${escapeAttr(market.id)}" data-open-side="no" data-open-intent="buy" class="rounded-full bg-rose-500 px-2.5 py-0.5 text-xs font-medium text-white transition hover:bg-rose-400">${noPct != null ? `No ${noPct}%` : "Buy No"}</button>`
                        }
                        <span class="ml-auto text-xs">${trendIndicator(market.change24h)}</span>
                      </div>
                    </div>
                  `;
                })
                .join("")}
            </div>
          </section>

          <section class="rounded-[21px] border border-slate-800 bg-slate-950/55 p-[21px]">
            <h3 class="mb-3 flex items-center gap-1.5 text-base font-medium text-slate-400"><svg class="h-4 w-4 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M7 3v18"/><path d="m3 7 4-4 4 4"/><path d="M17 21V3"/><path d="m21 17-4 4-4-4"/></svg>Top movers</h3>
            <div class="divide-y divide-slate-800">
              ${topMovers
                .map((market, idx) => {
                  const yesPct =
                    market.yesPrice != null
                      ? Math.round(market.yesPrice * 100)
                      : null;
                  const noPct =
                    market.yesPrice != null
                      ? Math.round((1 - market.yesPrice) * 100)
                      : null;
                  return `
                    <div class="w-full py-3 text-left">
                      <button data-open-market="${escapeAttr(market.id)}" class="w-full text-left">
                        <p class="mb-1 text-sm font-normal text-slate-300">${idx + 1}. ${escapeHtml(market.question)}</p>
                      </button>
                      <div class="flex items-center gap-2">
                        ${
                          market.state === 2 || market.state === 3
                            ? `<span class="rounded-full ${market.state === 2 ? "bg-emerald-500/20 text-emerald-300" : "bg-rose-500/20 text-rose-300"} px-2.5 py-0.5 text-xs font-medium">${market.state === 2 ? "Resolved YES" : "Resolved NO"}</span>`
                            : market.state === 4
                              ? `<span class="rounded-full bg-amber-500/20 px-2.5 py-0.5 text-xs font-medium text-amber-300">Expired</span>`
                              : `<button data-open-market="${escapeAttr(market.id)}" data-open-side="yes" data-open-intent="buy" class="rounded-full bg-emerald-500 px-2.5 py-0.5 text-xs font-medium text-white transition hover:bg-emerald-400">${yesPct != null ? `Yes ${yesPct}%` : "Buy Yes"}</button>
                        <button data-open-market="${escapeAttr(market.id)}" data-open-side="no" data-open-intent="buy" class="rounded-full bg-rose-500 px-2.5 py-0.5 text-xs font-medium text-white transition hover:bg-rose-400">${noPct != null ? `No ${noPct}%` : "Buy No"}</button>`
                        }
                        <span class="ml-auto text-xs">${trendIndicator(market.change24h)}</span>
                      </div>
                    </div>
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

function renderSortedView(title: string, sortedMarkets: Market[]): string {
  if (sortedMarkets.length === 0) {
    return `
      <div class="phi-container py-16 text-center">
        <h2 class="mb-3 text-2xl font-semibold text-slate-100">No ${title.toLowerCase()} markets</h2>
        <p class="text-base text-slate-400">Check back soon for updates.</p>
      </div>
    `;
  }

  return `
    <div class="phi-container py-6 lg:py-8">
      <div class="mb-4 flex items-center justify-between">
        <h1 class="flex items-center gap-2 text-xl font-medium text-slate-100">${categoryIcon(title, "h-5 w-5")}${title}</h1>
        <p class="text-sm text-slate-400">${sortedMarkets.length} market${sortedMarkets.length !== 1 ? "s" : ""}</p>
      </div>
      <div class="grid gap-3 md:grid-cols-2">
        ${sortedMarkets.map((market, idx) => marketCard(market, idx)).join("")}
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
  const expiredMarkets = myMarkets.filter((m) => m.state === 4);

  const renderMarketCard = (market: Market): string => {
    const no = market.yesPrice != null ? 1 - market.yesPrice : null;
    return `
      <button data-open-market="${escapeAttr(market.id)}" class="rounded-2xl border border-slate-800 bg-slate-950/55 p-4 text-left transition hover:border-slate-600">
        <div class="mb-2 flex items-center justify-between text-sm">
          <span class="text-xs text-slate-500">${escapeHtml(market.category)}</span>
          <span>${stateBadge(market.state)}</span>
        </div>
        <p class="mb-3 text-base font-normal text-slate-200">${escapeHtml(market.question)}</p>
        <div class="flex items-center justify-between text-sm">
          <span class="text-emerald-300">${market.yesPrice != null ? `Yes ${formatProbabilityWithPercent(market.yesPrice)}` : "Buy Yes"}</span>
          <span class="text-rose-300">${no != null ? `No ${formatProbabilityWithPercent(no)}` : "Buy No"}</span>
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
        <h1 class="flex items-center gap-2 text-xl font-medium text-slate-100">${categoryIcon("My Markets", "h-5 w-5")}My Markets</h1>
        ${state.marketMakerMode ? `<button data-action="open-create-market" class="rounded-xl bg-emerald-300 px-5 py-2 text-sm font-semibold text-slate-950"><span class="mr-1">+</span> Create New Market</button>` : ""}
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
      ${renderSection("Expired", expiredMarkets)}
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
    { 0: 0, 1: 0, 2: 0, 3: 0, 4: 0 } as Record<CovenantState, number>,
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
            <h1 class="flex items-center gap-2 text-xl font-medium text-slate-100">${categoryIcon(category, "h-5 w-5")}${category}</h1>
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
            ${categoryMarkets.map((market, idx) => marketCard(market, idx)).join("")}
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
                      <button data-open-market="${escapeAttr(market.id)}" class="w-full text-left">
                        <p class="text-sm font-normal text-slate-300">${escapeHtml(market.question)}</p>
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
                <button data-open-market="${escapeAttr(market.id)}" class="flex w-full items-start justify-between gap-2 text-left">
                  <p class="text-sm text-slate-300">${idx + 1}. ${escapeHtml(market.question)}</p>
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
              <p class="flex items-center justify-between"><span>State 4 · Expired</span><span>${stateMix[4]}</span></p>
            </div>
          </section>
        </aside>
      </div>
    </div>
  `;
}
