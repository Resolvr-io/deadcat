import { useCallback, useEffect, useRef } from "react";
import { useEscapeKey } from "../../hooks/useEscapeKey";
import { useExecuteTrade } from "../../queries/mutations/useTrading";
import { DEFAULT_TX_OPTIONS } from "../../services/tx";
import { useStore } from "../../store";
import type { Market } from "../../types";
import { formatSats } from "../../utils-react/format";
import {
  getQuoteEffectivePriceSatsPerContract,
  getQuoteRemainingSeconds,
} from "../../utils-react/market";

export default function QuoteModal({ market }: { market: Market }) {
  const tradeQuoteModalOpen = useStore((s) => s.tradeQuoteModalOpen);
  const tradeQuoteExecuting = useStore((s) => s.tradeQuoteExecuting);
  const tradeQuoteData = useStore((s) => s.tradeQuoteData);
  const tradeQuoteError = useStore((s) => s.tradeQuoteError);
  const tradeQuoteNowUnix = useStore((s) => s.tradeQuoteNowUnix);
  const selectedSide = useStore((s) => s.selectedSide);
  const tradeIntent = useStore((s) => s.tradeIntent);
  const tradeSizeSats = useStore((s) => s.tradeSizeSats);
  const tradeContracts = useStore((s) => s.tradeContracts);

  const executeTradeMutation = useExecuteTrade();
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Tick countdown timer
  useEffect(() => {
    if (!tradeQuoteModalOpen) return;
    timerRef.current = setInterval(() => {
      useStore.setState({ tradeQuoteNowUnix: Math.floor(Date.now() / 1000) });
    }, 1000);
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, [tradeQuoteModalOpen]);

  const currentDirection = tradeIntent === "open" ? "buy" : "sell";
  const currentExactInput =
    currentDirection === "buy"
      ? Math.max(1, Math.floor(tradeSizeSats))
      : Math.max(1, Math.floor(tradeContracts));

  const handleConfirm = useCallback(() => {
    if (!tradeQuoteData) return;

    useStore.setState({ tradeQuoteExecuting: true, tradeQuoteError: "" });

    executeTradeMutation.mutate(
      {
        market,
        side: selectedSide,
        direction: currentDirection as "buy" | "sell",
        exactInput: currentExactInput,
        txOptions: DEFAULT_TX_OPTIONS,
        expectedQuote: {
          total_input: tradeQuoteData.total_input,
          total_output: tradeQuoteData.total_output,
          legs: tradeQuoteData.legs,
        },
      },
      {
        onSuccess: () => {
          useStore.setState({
            tradeQuoteModalOpen: false,
            tradeQuoteExecuting: false,
            tradeQuoteData: null,
          });
        },
        onError: (error) => {
          useStore.setState({
            tradeQuoteExecuting: false,
            tradeQuoteError: String(error),
          });
        },
      },
    );
  }, [
    tradeQuoteData,
    market,
    selectedSide,
    currentDirection,
    currentExactInput,
    executeTradeMutation,
  ]);

  const handleCancel = useCallback(() => {
    useStore.setState({
      tradeQuoteModalOpen: false,
      tradeQuoteData: null,
      tradeQuoteError: "",
      tradeQuoteExecuting: false,
    });
  }, []);

  useEscapeKey(tradeQuoteModalOpen, handleCancel);

  if (!tradeQuoteModalOpen || !tradeQuoteData) return null;

  const effectivePrice = getQuoteEffectivePriceSatsPerContract(tradeQuoteData);
  const remainingSeconds = tradeQuoteData.expires_at_unix
    ? getQuoteRemainingSeconds(
        tradeQuoteData.expires_at_unix,
        tradeQuoteNowUnix,
      )
    : null;
  const isExpired = remainingSeconds !== null && remainingSeconds <= 0;

  return (
    <div
      role="presentation"
      className="macos-overlay-safe-top fixed inset-0 z-50 flex items-center justify-center bg-black/60"
      onClick={(e) => {
        if (e.target === e.currentTarget) handleCancel();
      }}
    >
      <div className="w-full max-w-md rounded-2xl border border-slate-700 bg-slate-900 p-6 shadow-2xl">
        <h2 className="mb-4 text-lg font-semibold text-slate-100">
          Confirm Trade
        </h2>

        <div className="space-y-2 text-sm">
          <div className="flex items-center justify-between">
            <span className="text-slate-400">Side</span>
            <span className="text-slate-200">
              {selectedSide.toUpperCase()} · {currentDirection}
            </span>
          </div>
          <div className="flex items-center justify-between">
            <span className="text-slate-400">Total input</span>
            <span className="text-slate-200">
              {currentDirection === "buy"
                ? formatSats(Math.floor(tradeQuoteData.total_input))
                : `${tradeQuoteData.total_input.toLocaleString(undefined, { maximumFractionDigits: 2 })} contracts`}
            </span>
          </div>
          <div className="flex items-center justify-between">
            <span className="text-slate-400">Total output</span>
            <span className="text-slate-200">
              {currentDirection === "buy"
                ? `${tradeQuoteData.total_output.toLocaleString(undefined, { maximumFractionDigits: 2 })} contracts`
                : formatSats(Math.floor(tradeQuoteData.total_output))}
            </span>
          </div>
          {effectivePrice !== null && (
            <div className="flex items-center justify-between">
              <span className="text-slate-400">Effective price</span>
              <span className="text-slate-200">
                {effectivePrice.toFixed(1)} sats/contract
              </span>
            </div>
          )}
          <div className="flex items-center justify-between">
            <span className="text-slate-400">Route legs</span>
            <span className="text-slate-200">{tradeQuoteData.legs.length}</span>
          </div>
        </div>

        {/* Countdown */}
        {remainingSeconds !== null && (
          <div className="mt-4 text-center">
            {isExpired ? (
              <p className="text-sm font-medium text-rose-400">
                Quote expired — request a new one
              </p>
            ) : (
              <p className="text-sm text-slate-400">
                Expires in{" "}
                <span className="font-semibold text-slate-200">
                  {remainingSeconds}s
                </span>
              </p>
            )}
          </div>
        )}

        {tradeQuoteError && (
          <p className="mt-3 text-xs text-rose-300">{tradeQuoteError}</p>
        )}

        {/* Actions */}
        <div className="mt-6 flex items-center gap-3">
          <button
            type="button"
            onClick={handleCancel}
            className="flex-1 rounded-lg border border-slate-600 px-4 py-2.5 text-sm font-medium text-slate-300 transition hover:bg-slate-800"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={handleConfirm}
            disabled={tradeQuoteExecuting || isExpired}
            className={`flex-1 rounded-lg px-4 py-2.5 text-sm font-semibold transition ${
              tradeQuoteExecuting || isExpired
                ? "bg-slate-700 text-slate-400"
                : "bg-emerald-300 text-slate-950 hover:bg-emerald-200"
            }`}
          >
            {tradeQuoteExecuting ? "Executing..." : "Confirm Trade"}
          </button>
        </div>
      </div>
    </div>
  );
}
