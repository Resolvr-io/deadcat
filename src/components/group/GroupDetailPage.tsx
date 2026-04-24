import { useMemo, useState } from "react";
import { useStore } from "../../store";
import type { MarketGroup, MarketGroupOutcome } from "../../types";
import {
  formatSats,
  formatSettlementDateTime,
  formatTimeRemaining,
  formatVolumeBtc,
} from "../../utils-react/format";
import {
  generateMockOutcomeOrderbook,
  getMockMarketGroups,
} from "../../utils-react/mock-groups";
import { CommentsSection } from "../detail/comments/CommentsSection";
import { MarketActionsMenu } from "../detail/MarketActionsMenu";
import { categoryIcon } from "../layout/TopShell";
import GroupChart, { OUTCOME_COLORS } from "./GroupChart";

// ── Trend icons ──────────────────────────────────────────────────────
function TrendUpIcon() {
  return (
    <svg
      aria-hidden="true"
      xmlns="http://www.w3.org/2000/svg"
      width="12"
      height="12"
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
      width="12"
      height="12"
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

// ── Outcome row ──────────────────────────────────────────────────────
function OutcomeRow({
  outcome,
  rank,
  color,
  isSelected,
  onSelect,
}: {
  outcome: MarketGroupOutcome;
  rank: number;
  color: string;
  isSelected: boolean;
  onSelect: () => void;
}) {
  const pct = Math.round(outcome.yesPrice * 100);
  const changePositive = outcome.change24h >= 0;
  const changeAbs = Math.abs(outcome.change24h * 100).toFixed(1);

  return (
    <button
      type="button"
      onClick={onSelect}
      className={`group flex w-full items-center gap-3 rounded-lg px-3 py-3 text-left transition ${
        isSelected ? "bg-slate-800/60" : "hover:bg-slate-900/40"
      }`}
    >
      {/* Color swatch + rank */}
      <span className="flex w-6 shrink-0 items-center gap-1.5">
        <span
          className="h-2 w-2 shrink-0 rounded-full"
          style={{ backgroundColor: color }}
        />
        <span className="text-[11px] text-slate-600">{rank}</span>
      </span>

      {/* Name + bar */}
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm text-slate-200">{outcome.name}</p>
        <div className="mt-1.5 h-0.5 w-full overflow-hidden rounded-full bg-slate-800">
          <div
            className="h-full rounded-full"
            style={{
              width: `${Math.max(2, pct)}%`,
              backgroundColor: color,
              opacity: 0.6,
            }}
          />
        </div>
      </div>

      {/* Probability */}
      <span
        className="w-10 shrink-0 text-right text-sm font-semibold"
        style={{ color }}
      >
        {pct}%
      </span>

      {/* 24h change */}
      <span
        className={`hidden w-14 shrink-0 items-center justify-end gap-0.5 text-xs sm:flex ${changePositive ? "text-emerald-400" : "text-rose-400"}`}
      >
        {changePositive ? <TrendUpIcon /> : <TrendDownIcon />}
        {changeAbs}%
      </span>

      {/* Volume */}
      <span className="hidden w-16 shrink-0 text-right text-xs text-slate-600 lg:block">
        {formatVolumeBtc(outcome.volumeBtc)}
      </span>
    </button>
  );
}

// ── Outcome trading panel ────────────────────────────────────────────
function OutcomeTradingPanel({
  group,
  outcome,
  color,
}: {
  group: MarketGroup;
  outcome: MarketGroupOutcome;
  color: string;
}) {
  const [side, setSide] = useState<"yes" | "no">("yes");
  const [intent, setIntent] = useState<"open" | "close">("open");
  const [orderType, setOrderType] = useState<"market" | "limit">("market");
  const [amountSats, setAmountSats] = useState(10000);
  const [amountDraft, setAmountDraft] = useState("10,000");
  const [limitPriceSats, setLimitPriceSats] = useState(() =>
    Math.round(outcome.yesPrice * group.cptSats * 2),
  );
  const [limitPriceDraft, setLimitPriceDraft] = useState(() =>
    String(Math.round(outcome.yesPrice * group.cptSats * 2)),
  );
  const [showOrderbook, setShowOrderbook] = useState(false);

  const fc = group.cptSats * 2;
  const pct = Math.round(outcome.yesPrice * 100);
  const yesDisplaySats = Math.round(outcome.yesPrice * fc);
  const noDisplaySats = fc - yesDisplaySats;

  const clamp = (v: number) => Math.max(1, Math.min(fc - 1, v));

  const activePrice = side === "yes" ? outcome.yesPrice : 1 - outcome.yesPrice;
  const contracts = amountSats / (activePrice * fc);
  const grossPayout = contracts * fc;

  const handleAmountChange = (raw: string) => {
    const digits = raw.replace(/\D/g, "");
    const num = Math.min(100_000_000, Number(digits) || 0);
    setAmountSats(num);
    setAmountDraft(num > 0 ? num.toLocaleString() : "");
  };

  const handleLimitBlur = () => {
    const parsed = Number(limitPriceDraft.replace(/\D/g, ""));
    const clamped = clamp(Number.isFinite(parsed) ? parsed : limitPriceSats);
    setLimitPriceSats(clamped);
    setLimitPriceDraft(String(clamped));
  };

  const stepLimit = (delta: number) => {
    const next = clamp(limitPriceSats + delta);
    setLimitPriceSats(next);
    setLimitPriceDraft(String(next));
  };

  const ctaBg =
    intent === "open"
      ? side === "yes"
        ? "bg-emerald-300 text-slate-950 hover:bg-emerald-200"
        : "bg-rose-400 text-slate-950 hover:bg-rose-300"
      : "bg-slate-600 text-slate-100 hover:bg-slate-500";

  // Orderbook data
  const book = generateMockOutcomeOrderbook(
    outcome.id,
    outcome.yesPrice,
    group.cptSats,
  );
  let askRunning = 0;
  const cumAsks = book.asks.map((l) => {
    askRunning += l.contracts;
    return { ...l, cumulative: askRunning };
  });
  let bidRunning = 0;
  const cumBids = book.bids.map((l) => {
    bidRunning += l.contracts;
    return { ...l, cumulative: bidRunning };
  });
  const maxCum = Math.max(
    cumAsks.length > 0 ? cumAsks[cumAsks.length - 1].cumulative : 0,
    cumBids.length > 0 ? cumBids[cumBids.length - 1].cumulative : 0,
    1,
  );

  return (
    <aside className="sticky top-4 self-start rounded-xl border border-slate-800 bg-slate-900/80 p-5">
      {/* Outcome identity */}
      <h2 className="mb-3 text-base font-semibold" style={{ color }}>
        {outcome.name}
      </h2>

      {/* Probability */}
      <div className="mb-3 flex items-baseline gap-2">
        <p className="text-4xl font-bold text-slate-100">
          {pct}
          <span className="text-xl text-slate-400">%</span>
        </p>
        <span className="text-base text-slate-500">chance</span>
        {outcome.change24h !== 0 && (
          <span
            className={`text-sm font-medium ${outcome.change24h > 0 ? "text-emerald-400" : "text-rose-400"}`}
          >
            {outcome.change24h > 0 ? "+" : ""}
            {(outcome.change24h * 100).toFixed(1)}% today
          </span>
        )}
      </div>

      {/* Buy / Sell + Market / Limit toggles */}
      <div className="mb-3 flex items-center justify-between gap-3 border-b border-slate-800 pb-3">
        <div className="flex items-center gap-4">
          <button
            type="button"
            onClick={() => setIntent("open")}
            className={`border-b-2 pb-1 text-xl font-medium transition ${intent === "open" ? "border-slate-100 text-slate-100" : "border-transparent text-slate-500"}`}
          >
            Buy
          </button>
          <button
            type="button"
            onClick={() => setIntent("close")}
            className={`border-b-2 pb-1 text-xl font-medium transition ${intent === "close" ? "border-slate-100 text-slate-100" : "border-transparent text-slate-500"}`}
          >
            Sell
          </button>
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => setOrderType("market")}
            className={`rounded border px-2 py-1 text-xs transition ${orderType === "market" ? "border-slate-500 bg-slate-700 text-slate-100" : "border-slate-700 text-slate-400"}`}
          >
            Market
          </button>
          <button
            type="button"
            onClick={() => setOrderType("limit")}
            className={`rounded border px-2 py-1 text-xs transition ${orderType === "limit" ? "border-slate-500 bg-slate-700 text-slate-100" : "border-slate-700 text-slate-400"}`}
          >
            Limit
          </button>
        </div>
      </div>

      {/* YES / NO selector — matches binary panel exactly */}
      <div className="mb-3 grid grid-cols-2 gap-2">
        <button
          type="button"
          onClick={() => setSide("yes")}
          className={`rounded-xl border px-3 py-3 transition ${
            side === "yes"
              ? intent === "open"
                ? "border-emerald-500 bg-emerald-500 text-slate-950"
                : "border-slate-500 bg-slate-600 text-slate-100"
              : "border-slate-700 text-slate-300 hover:border-slate-500"
          }`}
        >
          <span className="block text-lg font-semibold">Yes</span>
          <span className="block text-sm font-medium opacity-75">
            {yesDisplaySats} sats
          </span>
        </button>
        <button
          type="button"
          onClick={() => setSide("no")}
          className={`rounded-xl border px-3 py-3 transition ${
            side === "no"
              ? intent === "open"
                ? "border-rose-500 bg-rose-500 text-slate-950"
                : "border-slate-500 bg-slate-600 text-slate-100"
              : "border-slate-700 text-slate-300 hover:border-slate-500"
          }`}
        >
          <span className="block text-lg font-semibold">No</span>
          <span className="block text-sm font-medium opacity-75">
            {noDisplaySats} sats
          </span>
        </button>
      </div>

      {/* Limit price input */}
      {orderType === "limit" && (
        <div className="mb-3">
          <label
            className="mb-1 block text-xs text-slate-400"
            htmlFor="outcome-limit-price"
          >
            Price (sats per contract)
          </label>
          <div className="grid grid-cols-[42px_1fr_42px] gap-2">
            <button
              type="button"
              onClick={() => stepLimit(-1)}
              className="h-10 rounded-lg border border-slate-700 bg-slate-900/70 text-lg font-semibold text-slate-200 transition hover:border-slate-500 hover:bg-slate-800"
              aria-label="Decrease price"
            >
              &minus;
            </button>
            <input
              id="outcome-limit-price"
              type="text"
              inputMode="numeric"
              value={limitPriceDraft}
              onChange={(e) => setLimitPriceDraft(e.target.value)}
              onBlur={handleLimitBlur}
              className="h-10 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 text-center text-base font-semibold text-slate-100 outline-none ring-emerald-400/70 transition focus:ring-2"
            />
            <button
              type="button"
              onClick={() => stepLimit(1)}
              className="h-10 rounded-lg border border-slate-700 bg-slate-900/70 text-lg font-semibold text-slate-200 transition hover:border-slate-500 hover:bg-slate-800"
              aria-label="Increase price"
            >
              +
            </button>
          </div>
        </div>
      )}

      {/* Amount */}
      <div className="mb-1 flex items-center justify-between">
        <span className="text-xs text-slate-400">Amount</span>
      </div>
      <div className="relative mb-2">
        <input
          type="text"
          inputMode="numeric"
          value={amountDraft}
          onChange={(e) => handleAmountChange(e.target.value)}
          placeholder="0"
          className="h-12 w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 pr-14 text-sm text-slate-100 placeholder-slate-600 focus:border-slate-500 focus:outline-none"
        />
        <span className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-xs text-slate-500">
          sats
        </span>
      </div>

      {/* Potential payout — buy mode only */}
      {intent === "open" && amountSats > 0 && (
        <div className="mb-3 flex items-center justify-between rounded-lg bg-slate-900/60 px-3 py-2 text-sm">
          <span className="text-slate-400">Potential payout</span>
          <span className="font-semibold text-emerald-300">
            {formatSats(grossPayout)}
            {grossPayout > amountSats && (
              <span className="ml-1.5 text-xs text-slate-500">
                {(grossPayout / amountSats).toFixed(2)}x
              </span>
            )}
          </span>
        </div>
      )}

      {/* Preview box */}
      <div className="rounded-lg bg-slate-950/50 p-3 text-sm">
        {orderType === "limit" ? (
          <>
            <div className="flex items-center justify-between py-1">
              <span>Order type</span>
              <span>Limit</span>
            </div>
            <div className="flex items-center justify-between py-1">
              <span>Price</span>
              <span>{limitPriceSats} sats</span>
            </div>
            <div className="flex items-center justify-between py-1">
              <span>Amount</span>
              <span>{formatSats(amountSats)}</span>
            </div>
            <div className="mt-1 flex items-center justify-between py-1 text-xs text-slate-500">
              <span>Side</span>
              <span>
                {side.toUpperCase()} · {intent === "open" ? "buy" : "sell"}
              </span>
            </div>
          </>
        ) : (
          <>
            <div className="flex items-center justify-between py-1">
              <span>{intent === "open" ? "Amount" : "You receive"}</span>
              <span>{formatSats(amountSats)}</span>
            </div>
            <div className="flex items-center justify-between py-1">
              <span>{intent === "open" ? "Max payout" : "Position after"}</span>
              <span>
                {intent === "open"
                  ? formatSats(Math.floor(grossPayout))
                  : "0 contracts"}
              </span>
            </div>
            <div className="mt-1 flex items-center justify-between py-1 text-xs text-slate-500">
              <span>Avg price</span>
              <span>
                {side === "yes" ? yesDisplaySats : noDisplaySats} sats · Yes +
                No = {fc}
              </span>
            </div>
          </>
        )}
      </div>

      {/* Social proof */}
      {outcome.traderCount > 0 && (
        <p className="mb-2 mt-4 text-center text-xs text-slate-500">
          {outcome.traderCount.toLocaleString()} traders on this outcome
        </p>
      )}

      {/* CTA */}
      <button
        type="button"
        disabled
        className={`${outcome.traderCount > 0 ? "" : "mt-4"} w-full rounded-lg ${ctaBg} px-4 py-2 font-semibold opacity-50 transition`}
      >
        {orderType === "limit"
          ? `Place Limit ${intent === "open" ? "Buy" : "Sell"} ${side === "yes" ? "Yes" : "No"}`
          : intent === "open"
            ? `Buy ${side === "yes" ? "Yes" : "No"}`
            : `Sell ${side === "yes" ? "Yes" : "No"}`}
      </button>
      <p className="mt-2 text-center text-xs text-slate-600">
        Multi-outcome trading coming soon
      </p>

      {/* Position info */}
      <div className="mt-3 flex items-center justify-between text-xs text-slate-400">
        <span>You hold: YES 0 · NO 0</span>
      </div>

      {/* Fee details */}
      <details className="mt-3 text-xs text-slate-500">
        <summary className="cursor-pointer select-none hover:text-slate-300">
          Fee details
        </summary>
        <div className="mt-2 space-y-1 rounded bg-slate-900/40 p-2">
          <p>Execution fee: 1% of matched notional.</p>
          <p>
            Winning PnL fee: 2% of positive payout minus entry cost (buy only).
          </p>
          <p>Final fee depends on actual matched fills.</p>
        </div>
      </details>

      {/* Collapsible order book */}
      <div className="mt-4 border-t border-slate-800 pt-3">
        <button
          type="button"
          onClick={() => setShowOrderbook((v) => !v)}
          className="flex w-full items-center justify-between text-xs font-semibold uppercase tracking-wider text-slate-400 transition hover:text-slate-200"
        >
          <span>Order Book</span>
          <span className="font-normal normal-case">
            {showOrderbook ? "Hide" : "Show"}
          </span>
        </button>

        {showOrderbook && (
          <>
            <div className="mb-1 mt-2 flex justify-between text-[10px] text-slate-500">
              <span>Price (sats)</span>
              <span>Depth</span>
            </div>

            {[...cumAsks].reverse().map((level) => {
              const depthPct = (level.cumulative / maxCum) * 100;
              return (
                <div
                  key={`ask-${level.priceSats}`}
                  className="relative flex items-center justify-between px-2 py-0.5 text-xs"
                >
                  <div
                    className="absolute inset-y-0 right-0 bg-rose-500/15"
                    style={{ width: `${depthPct.toFixed(1)}%` }}
                  />
                  <span className="relative text-rose-400">
                    {level.priceSats}
                  </span>
                  <span className="relative text-slate-300">
                    {level.cumulative.toFixed(0)}
                  </span>
                </div>
              );
            })}

            <div className="border-y border-slate-800/50 py-1 text-center text-[10px] text-slate-500">
              {book.spread !== null ? `Spread: ${book.spread} sats` : "—"}
            </div>

            {cumBids.map((level) => {
              const depthPct = (level.cumulative / maxCum) * 100;
              return (
                <div
                  key={`bid-${level.priceSats}`}
                  className="relative flex items-center justify-between px-2 py-0.5 text-xs"
                >
                  <div
                    className="absolute inset-y-0 right-0 bg-emerald-500/15"
                    style={{ width: `${depthPct.toFixed(1)}%` }}
                  />
                  <span className="relative text-emerald-400">
                    {level.priceSats}
                  </span>
                  <span className="relative text-slate-300">
                    {level.cumulative.toFixed(0)}
                  </span>
                </div>
              );
            })}
          </>
        )}
      </div>
    </aside>
  );
}

// ── Empty selection state ────────────────────────────────────────────
function NoOutcomeSelected({ group }: { group: MarketGroup }) {
  return (
    <div className="flex h-64 flex-col items-center justify-center px-6 text-center">
      <p className="mb-1 text-sm font-medium text-slate-400">
        Select an outcome
      </p>
      <p className="text-xs text-slate-600">
        Click any outcome on the left to see pricing and trading details
      </p>
      <p className="mt-4 text-xs text-slate-700">
        {group.outcomes.length} outcomes ·{" "}
        {formatVolumeBtc(group.totalVolumeBtc)} total volume
      </p>
    </div>
  );
}

// ── Main GroupDetailPage ────────────────────────────────────────────
export default function GroupDetailPage() {
  const selectedGroupId = useStore((s) => s.selectedGroupId);
  const selectedOutcomeId = useStore((s) => s.selectedOutcomeId);

  const group = useMemo(() => {
    if (!selectedGroupId) return null;
    return getMockMarketGroups().find((g) => g.id === selectedGroupId) ?? null;
  }, [selectedGroupId]);

  // Auto-select the top outcome (highest probability) when the group loads
  // or changes and nothing is selected yet.
  const topOutcomeId = useMemo(() => {
    if (!group) return null;
    return (
      [...group.outcomes].sort((a, b) => b.yesPrice - a.yesPrice)[0]?.id ?? null
    );
  }, [group]);

  if (group && !selectedOutcomeId && topOutcomeId) {
    useStore.setState({ selectedOutcomeId: topOutcomeId });
  }

  const [search, setSearch] = useState("");

  const filteredOutcomes = useMemo(() => {
    if (!group) return [];
    const q = search.trim().toLowerCase();
    const sorted = [...group.outcomes].sort((a, b) => b.yesPrice - a.yesPrice);
    if (!q) return sorted;
    return sorted.filter((o) => o.name.toLowerCase().includes(q));
  }, [group, search]);

  const selectedOutcome = useMemo(
    () => group?.outcomes.find((o) => o.id === selectedOutcomeId) ?? null,
    [group, selectedOutcomeId],
  );

  if (!group) {
    return (
      <div className="phi-container py-16 text-center">
        <p className="text-slate-400">Market group not found.</p>
      </div>
    );
  }

  const blocksLeft = group.expiryHeight - group.currentHeight;
  const closesColor =
    blocksLeft < 2880
      ? "text-rose-400"
      : blocksLeft < 10080
        ? "text-amber-400"
        : "text-slate-200";

  return (
    <div className="phi-container py-6 lg:py-10">
      {/* Back nav */}
      <div className="mb-2 flex items-center gap-2">
        <button
          type="button"
          onClick={() =>
            useStore.setState({ view: "home", selectedGroupId: null })
          }
          className="flex items-center gap-1.5 text-sm text-slate-500 transition hover:text-slate-200"
        >
          <svg
            aria-hidden="true"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
            className="h-4 w-4"
          >
            <polyline points="15 18 9 12 15 6" />
          </svg>
          Markets
        </button>
        <div className="flex items-center gap-2">
          <span className="flex items-center gap-1.5 rounded-full bg-slate-800 px-2.5 py-0.5 text-xs text-slate-300">
            {categoryIcon(group.category, "h-3 w-3")}
            {group.category}
          </span>
          {group.state === "active" && (
            <span className="rounded-full bg-sky-500/20 px-2.5 py-0.5 text-xs font-medium text-sky-300">
              Active
            </span>
          )}
          <MarketActionsMenu market={group} />
        </div>
      </div>

      {/* Title + stats */}
      <h1 className="mb-2 text-2xl font-semibold leading-tight text-slate-100 lg:text-3xl">
        {group.title}
      </h1>
      <div className="mb-8 flex flex-wrap items-center gap-x-4 gap-y-1 text-sm text-slate-500">
        <span>
          <span className="text-slate-300">
            {group.traderCount.toLocaleString()}
          </span>{" "}
          traders
        </span>
        <span className="text-slate-700">·</span>
        <span>
          Closes{" "}
          <span className={closesColor}>
            {blocksLeft > 0 ? formatTimeRemaining(blocksLeft) : "Expired"}
          </span>
        </span>
      </div>

      {/* Two-column layout */}
      <div className="grid gap-8 lg:grid-cols-[1.618fr_0.8fr]">
        {/* Left: chart + outcome list + description */}
        <section className="min-w-0 space-y-6">
          <GroupChart group={group} highlightedOutcomeId={selectedOutcomeId} />

          {/* Search */}
          <div className="relative">
            <svg
              aria-hidden="true"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-slate-600"
            >
              <circle cx="11" cy="11" r="8" />
              <line x1="21" y1="21" x2="16.65" y2="16.65" />
            </svg>
            <input
              type="text"
              placeholder={`Search ${group.outcomes.length} outcomes...`}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="h-9 w-full max-w-xs rounded-full border border-slate-800 bg-slate-950/60 pl-9 pr-3 text-sm text-slate-100 placeholder-slate-600 focus:border-slate-600 focus:outline-none"
            />
          </div>

          {/* Column headers */}
          <div className="-mb-4 flex items-center gap-3 px-3 text-[10px] font-semibold uppercase tracking-wider text-slate-500">
            <span className="w-5 text-center">#</span>
            <span className="flex-1">Outcome</span>
            <span className="w-10 text-right">Prob.</span>
            <span className="hidden w-14 text-right sm:block">24h</span>
            <span className="hidden w-16 text-right lg:block">Volume</span>
          </div>

          {/* Outcome rows */}
          <div className="space-y-0.5">
            {filteredOutcomes.map((outcome) => {
              const globalRank =
                group.outcomes
                  .slice()
                  .sort((a, b) => b.yesPrice - a.yesPrice)
                  .findIndex((o) => o.id === outcome.id) + 1;
              const color =
                OUTCOME_COLORS[(globalRank - 1) % OUTCOME_COLORS.length];
              return (
                <OutcomeRow
                  key={outcome.id}
                  outcome={outcome}
                  rank={globalRank}
                  color={color}
                  isSelected={selectedOutcomeId === outcome.id}
                  onSelect={() =>
                    useStore.setState({ selectedOutcomeId: outcome.id })
                  }
                />
              );
            })}
            {filteredOutcomes.length === 0 && (
              <p className="py-8 text-center text-sm text-slate-500">
                No outcomes match &ldquo;{search}&rdquo;
              </p>
            )}
          </div>

          {/* Description */}
          <p className="mb-4 text-sm text-slate-400">{group.description}</p>

          {/* Resolution metadata */}
          <p className="mb-4 text-xs text-slate-500">
            <span className="font-semibold uppercase tracking-wider text-slate-400">
              Resolution
            </span>
            {" · "}Source:{" "}
            <span className="text-slate-300">{group.resolutionSource}</span>
            {" · "}Deadline:{" "}
            <span className="text-slate-300">
              Est.{" "}
              {formatSettlementDateTime(
                new Date(Date.now() + blocksLeft * 60 * 1000),
              )}
            </span>
            {" · "}Block:{" "}
            <span className="text-slate-300">
              {group.expiryHeight.toLocaleString()}
            </span>
          </p>

          {/* Comments */}
          <CommentsSection
            market={{
              marketId: group.id,
              creatorPubkey: group.creatorPubkey ?? "",
              nostrEventJson: group.nostrEventJson ?? null,
            }}
          />
        </section>

        {/* Right: trading panel */}
        <aside>
          {(() => {
            if (!selectedOutcome) return <NoOutcomeSelected group={group} />;
            const rank = [...group.outcomes]
              .sort((a, b) => b.yesPrice - a.yesPrice)
              .findIndex((o) => o.id === selectedOutcome.id);
            const color = OUTCOME_COLORS[rank % OUTCOME_COLORS.length];
            return (
              <OutcomeTradingPanel
                group={group}
                outcome={selectedOutcome}
                color={color}
              />
            );
          })()}
        </aside>
      </div>
    </div>
  );
}
