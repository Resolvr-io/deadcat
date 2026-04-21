import { openUrl } from "@tauri-apps/plugin-opener";
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
  const walletNetwork = useStore((s) => s.walletNetwork);

  const estimatedSettlementDate = getEstimatedSettlementDate(market);
  const positions = getPositionContracts(market, walletData);

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
          <span className="rounded-full bg-slate-800 px-2.5 py-0.5 text-xs text-slate-300">
            {market.category}
          </span>
          {stateBadge(market.state)}
          <span className="h-3.5 w-px bg-slate-700" />
          <button
            type="button"
            onClick={() =>
              useStore.setState({
                nostrEventModal: true,
                nostrEventJson: market.nostrEventJson,
                nostrEventNevent: market.nevent,
              })
            }
            className="text-xs text-slate-400 transition hover:text-slate-200"
          >
            Nostr Event
          </button>
          {market.creationTxid && (
            <button
              type="button"
              onClick={() => {
                const base =
                  walletNetwork === "testnet"
                    ? "https://blockstream.info/liquidtestnet"
                    : "https://blockstream.info/liquid";
                void openUrl(`${base}/tx/${market.creationTxid}`);
              }}
              className="text-xs text-slate-400 transition hover:text-slate-200"
            >
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

      {/* Market stats */}
      <div className="mb-4 grid grid-cols-2 gap-3 sm:grid-cols-4">
        <div className="rounded-xl border border-slate-800 bg-slate-900/60 px-3 py-2.5">
          <p className="text-xs uppercase tracking-wider text-slate-500">
            Volume
          </p>
          <p className="text-base font-semibold text-slate-200">
            {formatVolumeBtc(market.volumeBtc)}
          </p>
        </div>
        <div className="rounded-xl border border-slate-800 bg-slate-900/60 px-3 py-2.5">
          <p className="text-xs uppercase tracking-wider text-slate-500">
            Traders
          </p>
          <p className="text-base font-semibold text-slate-200">
            {market.traderCount > 0
              ? market.traderCount.toLocaleString()
              : "\u2014"}
          </p>
        </div>
        <div className="rounded-xl border border-slate-800 bg-slate-900/60 px-3 py-2.5">
          <p className="text-xs uppercase tracking-wider text-slate-500">
            Liquidity
          </p>
          <p className="text-base font-semibold text-slate-200">
            {formatVolumeBtc(market.liquidityBtc)}
          </p>
        </div>
        <div className="rounded-xl border border-slate-800 bg-slate-900/60 px-3 py-2.5">
          <p className="text-xs uppercase tracking-wider text-slate-500">
            Ends
          </p>
          <p className="text-base font-semibold text-slate-200">
            {formatTimeRemaining(market.expiryHeight - market.currentHeight)}
          </p>
        </div>
      </div>

      {/* Resolution rules */}
      <div className="mb-4 rounded-xl border border-slate-700/60 bg-slate-900/40 p-4">
        <div className="mb-2 flex items-center gap-2">
          <svg
            aria-hidden="true"
            className="h-4 w-4 text-slate-400"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
            <polyline points="14 2 14 8 20 8" />
            <line x1="16" y1="13" x2="8" y2="13" />
            <line x1="16" y1="17" x2="8" y2="17" />
          </svg>
          <span className="text-xs font-semibold uppercase tracking-wider text-slate-400">
            Resolution Rules
          </span>
        </div>
        <p className="mb-2 text-sm text-slate-300">{market.description}</p>
        <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-slate-500">
          <span>
            Source:{" "}
            <span className="text-slate-300">{market.resolutionSource}</span>
          </span>
          <span>
            Deadline:{" "}
            <span className="text-slate-300">
              Est. {formatSettlementDateTime(estimatedSettlementDate)}
            </span>
          </span>
          <span>
            Block:{" "}
            <span className="text-slate-300">
              {market.expiryHeight.toLocaleString()}
            </span>
          </span>
        </div>
      </div>

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
