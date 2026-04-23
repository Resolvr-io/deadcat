import { useMemo, useState } from "react";
import { useStore } from "../../store";
import type { MarketGroup, MarketGroupOutcome } from "../../types";
import {
  formatSats,
  formatTimeRemaining,
  formatVolumeBtc,
} from "../../utils-react/format";
import { getMockMarketGroups } from "../../utils-react/mock-groups";

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
  isSelected,
  onSelect,
}: {
  outcome: MarketGroupOutcome;
  rank: number;
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
      className={`group flex w-full items-center gap-3 rounded-xl px-3 py-2.5 text-left transition ${
        isSelected
          ? "border border-violet-700/50 bg-violet-500/10"
          : "border border-transparent hover:border-slate-700 hover:bg-slate-900/50"
      }`}
    >
      {/* Rank */}
      <span className="w-5 shrink-0 text-center text-xs text-slate-600">
        {rank}
      </span>

      {/* Name + bar */}
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm text-slate-200">{outcome.name}</p>
        <div className="mt-1 h-1 w-full overflow-hidden rounded-full bg-slate-800">
          <div
            className="h-full rounded-full bg-emerald-500/50"
            style={{ width: `${Math.max(2, pct)}%` }}
          />
        </div>
      </div>

      {/* Probability */}
      <span className="w-10 shrink-0 text-right text-sm font-semibold text-emerald-400">
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
      <span className="hidden w-16 shrink-0 text-right text-xs text-slate-500 lg:block">
        {formatVolumeBtc(outcome.volumeBtc)}
      </span>

      {/* Yes / No buttons */}
      <div className="flex shrink-0 gap-1.5">
        <span className="rounded-full bg-emerald-500/15 px-2 py-0.5 text-xs font-medium text-emerald-400">
          Yes {pct}%
        </span>
        <span className="rounded-full bg-rose-500/15 px-2 py-0.5 text-xs font-medium text-rose-400">
          No {100 - pct}%
        </span>
      </div>
    </button>
  );
}

// ── Outcome trading panel ────────────────────────────────────────────
function OutcomeTradingPanel({
  group,
  outcome,
}: {
  group: MarketGroup;
  outcome: MarketGroupOutcome;
}) {
  const [side, setSide] = useState<"yes" | "no">("yes");
  const [amountSats, setAmountSats] = useState(10000);
  const [amountDraft, setAmountDraft] = useState("10,000");

  const pct = Math.round(outcome.yesPrice * 100);
  const noPct = 100 - pct;

  // Rough payout estimate: (collateral / price) * 2 * cptSats - collateral
  // For a YES position: payout = contracts * 2 * cptSats; contracts = amountSats / (yesPrice * 2 * cptSats)
  const activePrice = side === "yes" ? outcome.yesPrice : 1 - outcome.yesPrice;
  const contracts = amountSats / (activePrice * 2 * group.cptSats);
  const grossPayout = contracts * 2 * group.cptSats;
  const netProfit = grossPayout - amountSats;

  const handleAmountChange = (raw: string) => {
    const digits = raw.replace(/\D/g, "");
    const num = Math.min(100_000_000, Number(digits) || 0);
    setAmountSats(num);
    setAmountDraft(num > 0 ? num.toLocaleString() : "");
  };

  const ctaBg =
    side === "yes"
      ? "bg-emerald-300 text-slate-950 hover:bg-emerald-200"
      : "bg-rose-400 text-slate-950 hover:bg-rose-300";

  return (
    <div className="rounded-[21px] border border-slate-800 bg-slate-950/55 p-5">
      {/* Outcome title */}
      <p className="mb-1 text-xs font-semibold uppercase tracking-wider text-slate-500">
        {group.category} · Multi-outcome
      </p>
      <h2 className="mb-3 text-base font-semibold text-slate-100">
        {outcome.name}
      </h2>

      {/* Probability display */}
      <div className="mb-4 flex items-baseline gap-2">
        <p className="text-4xl font-bold text-emerald-400">
          {pct}
          <span className="text-xl text-slate-400">%</span>
        </p>
        <span className="text-base text-slate-500">chance</span>
      </div>

      {/* YES / NO selector */}
      <div className="mb-4 flex gap-2">
        <button
          type="button"
          onClick={() => setSide("yes")}
          className={`flex-1 rounded-xl py-2.5 text-sm font-semibold transition ${
            side === "yes"
              ? "bg-emerald-500 text-slate-950"
              : "border border-slate-700 text-slate-400 hover:border-slate-500 hover:text-slate-200"
          }`}
        >
          Yes {pct}%
        </button>
        <button
          type="button"
          onClick={() => setSide("no")}
          className={`flex-1 rounded-xl py-2.5 text-sm font-semibold transition ${
            side === "no"
              ? "bg-rose-500 text-slate-950"
              : "border border-slate-700 text-slate-400 hover:border-slate-500 hover:text-slate-200"
          }`}
        >
          No {noPct}%
        </button>
      </div>

      {/* Amount input */}
      <p className="mb-1.5 text-xs font-medium text-slate-400">Amount</p>
      <div className="relative mb-3">
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

      {/* Potential payout */}
      {amountSats > 0 && (
        <div className="mb-4 flex items-center justify-between rounded-lg bg-slate-900/60 px-3 py-2 text-sm">
          <span className="text-slate-400">Potential payout</span>
          <span className="font-semibold text-emerald-300">
            {formatSats(grossPayout)}
            {netProfit > 0 && (
              <span className="ml-1.5 text-xs text-slate-500">
                {(grossPayout / amountSats).toFixed(2)}x
              </span>
            )}
          </span>
        </div>
      )}

      {/* Social proof */}
      {outcome.traderCount > 0 && (
        <p className="mb-3 text-center text-xs text-slate-500">
          {outcome.traderCount.toLocaleString()} traders on this outcome
        </p>
      )}

      {/* CTA */}
      <button
        type="button"
        disabled
        className={`w-full rounded-xl py-3 text-sm font-semibold opacity-60 ${ctaBg}`}
      >
        {side === "yes"
          ? `Buy Yes — ${outcome.name}`
          : `Buy No — ${outcome.name}`}
      </button>
      <p className="mt-2 text-center text-xs text-slate-600">
        Multi-outcome trading coming soon
      </p>

      {/* Stats footer */}
      <div className="mt-4 border-t border-slate-800/60 pt-3 text-xs text-slate-500">
        <div className="flex justify-between">
          <span>Volume</span>
          <span className="text-slate-300">
            {formatVolumeBtc(outcome.volumeBtc)}
          </span>
        </div>
        <div className="mt-1 flex justify-between">
          <span>Collateral per token</span>
          <span className="text-slate-300">{formatSats(group.cptSats)}</span>
        </div>
        <div className="mt-1 flex justify-between">
          <span>Resolution</span>
          <span className="text-slate-300 text-right max-w-[60%] truncate">
            {group.resolutionSource}
          </span>
        </div>
      </div>
    </div>
  );
}

// ── Empty selection state ────────────────────────────────────────────
function NoOutcomeSelected({ group }: { group: MarketGroup }) {
  return (
    <div className="flex h-64 flex-col items-center justify-center rounded-[21px] border border-slate-800 bg-slate-950/30 px-6 text-center">
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
    <div className="phi-container py-6 lg:py-8">
      {/* Back + header */}
      <div className="mb-5">
        <div className="mb-3 flex items-center gap-2">
          <button
            type="button"
            onClick={() =>
              useStore.setState({ view: "home", selectedGroupId: null })
            }
            className="flex items-center gap-1.5 text-sm text-slate-400 transition hover:text-slate-200"
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
            Back
          </button>
          <span className="text-slate-700">·</span>
          <span className="text-sm text-slate-500">{group.category}</span>
          <span className="rounded border border-violet-700/50 bg-violet-500/10 px-1.5 py-0.5 text-[10px] font-medium text-violet-300">
            Multi-outcome
          </span>
          {group.state === "active" && (
            <span className="rounded border border-emerald-700/50 bg-emerald-500/10 px-1.5 py-0.5 text-[10px] font-medium text-emerald-300">
              Active
            </span>
          )}
        </div>

        <h1 className="mb-3 text-2xl font-semibold text-slate-100">
          {group.title}
        </h1>

        {/* Stats strip */}
        <div className="mb-3 flex flex-wrap items-center gap-x-5 gap-y-1 text-sm">
          <span className="text-slate-500">
            Vol{" "}
            <span className="text-slate-200">
              {formatVolumeBtc(group.totalVolumeBtc)}
            </span>
          </span>
          <span className="text-slate-700">·</span>
          <span className="text-slate-500">
            Traders{" "}
            <span className="text-slate-200">
              {group.traderCount.toLocaleString()}
            </span>
          </span>
          <span className="text-slate-700">·</span>
          <span className="text-slate-500">
            Outcomes{" "}
            <span className="text-slate-200">{group.outcomes.length}</span>
          </span>
          <span className="text-slate-700">·</span>
          <span className="text-slate-500">
            Closes{" "}
            <span className={closesColor}>
              {blocksLeft > 0 ? formatTimeRemaining(blocksLeft) : "Expired"}
            </span>
          </span>
        </div>

        <p className="text-sm text-slate-400">{group.description}</p>
      </div>

      {/* Two-column layout */}
      <div className="grid gap-5 lg:grid-cols-[1.618fr_1fr]">
        {/* Left: outcome list */}
        <section>
          {/* Search */}
          <div className="relative mb-3">
            <svg
              aria-hidden="true"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-slate-500"
            >
              <circle cx="11" cy="11" r="8" />
              <line x1="21" y1="21" x2="16.65" y2="16.65" />
            </svg>
            <input
              type="text"
              placeholder={`Search ${group.outcomes.length} outcomes...`}
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="h-9 w-full rounded-lg border border-slate-700 bg-slate-950/80 pl-9 pr-3 text-sm text-slate-100 placeholder-slate-600 focus:border-slate-500 focus:outline-none"
            />
          </div>

          {/* Column headers */}
          <div className="mb-1 flex items-center gap-3 px-3 text-[10px] font-semibold uppercase tracking-wider text-slate-600">
            <span className="w-5 text-center">#</span>
            <span className="flex-1">Outcome</span>
            <span className="w-10 text-right">Prob.</span>
            <span className="hidden w-14 text-right sm:block">24h</span>
            <span className="hidden w-16 text-right lg:block">Volume</span>
            <span className="w-28 text-right">Trade</span>
          </div>

          {/* Outcome rows */}
          <div className="space-y-1">
            {filteredOutcomes.map((outcome) => {
              const globalRank =
                group.outcomes
                  .slice()
                  .sort((a, b) => b.yesPrice - a.yesPrice)
                  .findIndex((o) => o.id === outcome.id) + 1;
              return (
                <OutcomeRow
                  key={outcome.id}
                  outcome={outcome}
                  rank={globalRank}
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
        </section>

        {/* Right: trading panel */}
        <aside>
          {selectedOutcome ? (
            <OutcomeTradingPanel group={group} outcome={selectedOutcome} />
          ) : (
            <NoOutcomeSelected group={group} />
          )}
        </aside>
      </div>
    </div>
  );
}
