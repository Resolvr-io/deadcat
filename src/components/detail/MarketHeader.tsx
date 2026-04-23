import { useStore } from "../../store";
import type { Market } from "../../types";
import {
  formatSettlementDateTime,
  formatTimeRemaining,
  formatVolumeBtc,
} from "../../utils-react/format";
import {
  getEstimatedSettlementDate,
  getPositionContracts,
} from "../../utils-react/market";
import { categoryIcon } from "../layout/TopShell";
import { MarketActionsMenu } from "./MarketActionsMenu";

function stateBadge(state: number) {
  const colorMap: Record<number, string> = {
    0: "bg-slate-600/30 text-slate-300",
    1: "bg-sky-500/20 text-sky-300",
    2: "bg-emerald-500/30 text-emerald-200",
    3: "bg-rose-500/30 text-rose-200",
    4: "bg-amber-500/25 text-amber-200",
  };
  const labels: Record<number, string> = {
    0: "Dormant",
    1: "Trading",
    2: "Resolved YES",
    3: "Resolved NO",
    4: "Expired",
  };

  return (
    <span
      className={`rounded-full px-2.5 py-0.5 text-xs font-medium ${colorMap[state] ?? ""}`}
    >
      {labels[state] ?? "Unknown"}
    </span>
  );
}

/** Above the chart: nav, title, probability, stats strip. */
export function MarketHeaderTop({ market }: { market: Market }) {
  const blocksLeft = market.expiryHeight - market.currentHeight;
  const closesColor =
    blocksLeft < 2880
      ? "text-rose-400"
      : blocksLeft < 10080
        ? "text-amber-400"
        : "text-slate-200";

  const changePct =
    market.change24h !== 0
      ? `${market.change24h > 0 ? "+" : ""}${(market.change24h * 100).toFixed(1)}%`
      : null;

  return (
    <>
      {/* Top bar: back button + badges */}
      <div className="mb-3 flex items-center justify-between">
        <button
          type="button"
          onClick={() => useStore.setState({ view: "home" })}
          className="flex items-center gap-1 text-sm text-slate-400 transition hover:text-slate-200"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
          >
            <polyline points="15 18 9 12 15 6" />
          </svg>
          Markets
        </button>
        <div className="flex items-center gap-2">
          <span className="flex items-center gap-1.5 rounded-full bg-slate-800 px-2.5 py-0.5 text-xs text-slate-300">
            {categoryIcon(market.category, "h-3 w-3")}
            {market.category}
          </span>
          {stateBadge(market.state)}
          <MarketActionsMenu market={market} />
        </div>
      </div>

      {/* Title */}
      <h1 className="phi-title mb-3 text-2xl font-medium leading-tight text-slate-100 lg:text-[34px]">
        {market.question}
      </h1>

      {/* Probability + 24h change */}
      {market.yesPrice != null && (
        <div className="mb-3 flex items-baseline gap-3">
          <p className="text-5xl font-bold text-emerald-400">
            {Math.round(market.yesPrice * 100)}
            <span className="text-2xl text-slate-400">%</span>
          </p>
          <span className="text-lg font-normal text-slate-500">chance</span>
          {changePct && (
            <span
              className={`text-sm font-medium ${market.change24h > 0 ? "text-emerald-400" : "text-rose-400"}`}
            >
              {changePct} today
            </span>
          )}
        </div>
      )}

      {/* Stats strip */}
      <div className="mb-4 flex flex-wrap items-center gap-x-5 gap-y-1 text-sm">
        <span className="text-slate-500">
          Vol{" "}
          <span className="text-slate-200">
            {formatVolumeBtc(market.volumeBtc)}
          </span>
        </span>
        <span className="text-slate-700">·</span>
        <span className="text-slate-500">
          Traders{" "}
          <span className="text-slate-200">
            {market.traderCount > 0 ? market.traderCount.toLocaleString() : "—"}
          </span>
        </span>
        <span className="text-slate-700">·</span>
        <span className="text-slate-500">
          Liquidity{" "}
          <span className="text-slate-200">
            {formatVolumeBtc(market.liquidityBtc)}
          </span>
        </span>
        <span className="text-slate-700">·</span>
        <span className="text-slate-500">
          Closes{" "}
          <span className={closesColor}>{formatTimeRemaining(blocksLeft)}</span>
        </span>
      </div>
    </>
  );
}

/** Below the chart: description, resolution metadata, position display. */
export function MarketHeaderBottom({ market }: { market: Market }) {
  const walletData = useStore((s) => s.walletData);

  const estimatedSettlementDate = getEstimatedSettlementDate(market);
  const positions = getPositionContracts(market, walletData);

  return (
    <>
      {/* Description */}
      <p className="mb-4 text-sm text-slate-400">{market.description}</p>

      {/* Resolution metadata — compact single line */}
      <p className="mb-4 text-xs text-slate-500">
        <span className="font-semibold uppercase tracking-wider text-slate-400">
          Resolution
        </span>
        {" · "}Source:{" "}
        <span className="text-slate-300">{market.resolutionSource}</span>
        {" · "}Deadline:{" "}
        <span className="text-slate-300">
          Est. {formatSettlementDateTime(estimatedSettlementDate)}
        </span>
        {" · "}Block:{" "}
        <span className="text-slate-300">
          {market.expiryHeight.toLocaleString()}
        </span>
      </p>

      {/* Position display */}
      {(positions.yes > 0 || positions.no > 0) && (
        <div className="mb-4 flex items-center gap-3 rounded-xl border border-slate-700 bg-slate-900/40 px-4 py-2 text-sm">
          <span className="text-slate-400">Your position</span>
          {positions.yes > 0 && (
            <span className="rounded bg-emerald-500/20 px-2 py-0.5 font-medium text-emerald-300">
              YES {positions.yes.toLocaleString()}
            </span>
          )}
          {positions.no > 0 && (
            <span className="rounded bg-red-500/20 px-2 py-0.5 font-medium text-red-300">
              NO {positions.no.toLocaleString()}
            </span>
          )}
        </div>
      )}
    </>
  );
}

/** @deprecated Use MarketHeaderTop + MarketHeaderBottom split around the chart. */
export default function MarketHeader({ market }: { market: Market }) {
  return (
    <>
      <MarketHeaderTop market={market} />
      <MarketHeaderBottom market={market} />
    </>
  );
}
