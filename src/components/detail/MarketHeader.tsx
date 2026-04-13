import type { Market } from "../../types";
import { useStore } from "../../store";
import {
  getPositionContracts,
  getEstimatedSettlementDate,
} from "../../utils-react/market";
import {
  formatVolumeBtc,
  formatSettlementDateTime,
  formatTimeRemaining,
} from "../../utils-react/format";

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
    1: "Unresolved",
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

export default function MarketHeader({ market }: { market: Market }) {
  const walletData = useStore((s) => s.walletData);

  const noPrice =
    market.yesPrice != null ? 1 - market.yesPrice : null;
  const estimatedSettlementDate = getEstimatedSettlementDate(market);
  const positions = getPositionContracts(market, walletData);

  return (
    <>
      {/* Top bar: back button + badges */}
      <div className="mb-3 flex items-center justify-between">
        <button
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
          >
            <polyline points="15 18 9 12 15 6" />
          </svg>
          Markets
        </button>
        <div className="flex items-center gap-2">
          <span className="rounded-full bg-slate-800 px-2.5 py-0.5 text-xs text-slate-300">
            {market.category}
          </span>
          {stateBadge(market.state)}
          <span className="h-3.5 w-px bg-slate-700" />
          <button className="text-xs text-slate-400 transition hover:text-slate-200">
            Nostr Event
          </button>
          {market.creationTxid && (
            <button className="text-xs text-slate-400 transition hover:text-slate-200">
              Creation TX
            </button>
          )}
        </div>
      </div>

      {/* Title & description */}
      <h1 className="phi-title mb-2 text-2xl font-medium leading-tight text-slate-100 lg:text-[34px]">
        {market.question}
      </h1>
      <p className="mb-4 text-sm text-slate-400">{market.description}</p>

      {/* Probability display */}
      {market.yesPrice != null && (
        <p className="mb-2 text-5xl font-bold text-emerald-400">
          {Math.round(market.yesPrice * 100)}
          <span className="text-2xl text-slate-400">%</span>{" "}
          <span className="text-lg font-normal text-slate-500">chance</span>
        </p>
      )}

      {/* Yes / No quick-buy buttons */}
      <div className="mb-4 flex items-center gap-3">
        <button
          onClick={() =>
            useStore.setState({ selectedSide: "yes", tradeIntent: "open" })
          }
          className="w-36 rounded-full bg-emerald-500 px-4 py-2.5 text-center text-lg font-semibold text-white transition hover:bg-emerald-400"
        >
          {market.yesPrice != null
            ? `Yes ${Math.round(market.yesPrice * 100)}%`
            : "Buy Yes"}
        </button>
        <button
          onClick={() =>
            useStore.setState({ selectedSide: "no", tradeIntent: "open" })
          }
          className="w-36 rounded-full bg-rose-500 px-4 py-2.5 text-center text-lg font-semibold text-white transition hover:bg-rose-400"
        >
          {noPrice != null
            ? `No ${Math.round(noPrice * 100)}%`
            : "Buy No"}
        </button>
      </div>

      {/* Volume + settlement info */}
      <p className="mb-4 text-xs text-slate-500">
        {formatVolumeBtc(market.volumeBtc)} vol · Est. by{" "}
        {formatSettlementDateTime(estimatedSettlementDate)} ·{" "}
        {formatTimeRemaining(market.expiryHeight - market.currentHeight)}
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
