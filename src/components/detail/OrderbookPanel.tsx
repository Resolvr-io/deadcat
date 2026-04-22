import { useMemo } from "react";
import { useCancelLimitOrder } from "../../queries/mutations/useTrading";
import { useStore } from "../../store";
import type { Market } from "../../types";
import { getFullOrderbook } from "../../utils-react/market";
import { generateMockOrderbook } from "../../utils-react/mock-orderbook";

export default function OrderbookPanel({ market }: { market: Market }) {
  const selectedSide = useStore((s) => s.selectedSide);
  const nostrPubkey = useStore((s) => s.nostrPubkey);
  const cancellingOrderId = useStore((s) => s.cancellingOrderId);

  const cancelLimitOrderMutation = useCancelLimitOrder();

  const realBook = useMemo(
    () => getFullOrderbook(market, selectedSide),
    [market, selectedSide],
  );
  const book = useMemo(() => {
    const hasOrders = realBook.asks.length > 0 || realBook.bids.length > 0;
    return hasOrders ? realBook : generateMockOrderbook(market);
  }, [realBook, market]);

  // Cumulative sums — asks accumulate away from spread (top of book outward),
  // bids accumulate away from spread (top of book outward). Both grow outward
  // from the spread, producing a V-curve depth shape.
  const cumAsks = useMemo(() => {
    // asks are sorted ascending; cumulate from best (lowest) upward
    let running = 0;
    return book.asks.map((l) => {
      running += l.contracts;
      return { ...l, cumulative: running };
    });
  }, [book.asks]);

  const cumBids = useMemo(() => {
    // bids are sorted descending; cumulate from best (highest) downward
    let running = 0;
    return book.bids.map((l) => {
      running += l.contracts;
      return { ...l, cumulative: running };
    });
  }, [book.bids]);

  const maxCumulative = Math.max(
    cumAsks.length > 0 ? cumAsks[cumAsks.length - 1].cumulative : 0,
    cumBids.length > 0 ? cumBids[cumBids.length - 1].cumulative : 0,
    1,
  );

  // Asks displayed highest price at top → reverse so highest ask is first row
  const askRows = useMemo(() => [...cumAsks].reverse(), [cumAsks]);

  const myOrders = useMemo(
    () =>
      market.limitOrders.filter(
        (o) => nostrPubkey && o.creator_pubkey === nostrPubkey,
      ),
    [market.limitOrders, nostrPubkey],
  );

  const handleCancelOrder = (orderId: string) => {
    const order = market.limitOrders.find((o) => o.id === orderId);
    if (!order) return;

    useStore.setState({ cancellingOrderId: orderId });

    cancelLimitOrderMutation.mutate(
      { order },
      {
        onSettled: () => {
          useStore.setState({ cancellingOrderId: null });
        },
      },
    );
  };

  return (
    <>
      {/* Order Book */}
      <div className="mt-3 rounded-xl border border-slate-800 bg-slate-950/70 p-3">
        <div className="mb-2 flex items-center justify-between">
          <span className="text-[10px] font-semibold uppercase tracking-wider text-slate-400">
            Order Book
          </span>
          <span className="text-[10px] font-medium text-slate-500">
            {selectedSide.toUpperCase()}
          </span>
        </div>
        <div className="mb-1 flex items-center justify-between text-[10px] text-slate-500">
          <span>Price (sats)</span>
          <span>Contracts</span>
        </div>

        {/* Asks (reversed — highest at top, widest bar at top) */}
        {askRows.map((level) => {
          const pct = (level.cumulative / maxCumulative) * 100;
          return (
            <div
              key={`ask-${level.priceSats}`}
              className="relative flex items-center justify-between px-2 py-0.5 text-xs"
            >
              <div
                className="absolute inset-y-0 right-0 bg-rose-500/15"
                style={{ width: `${pct.toFixed(1)}%` }}
              />
              <span className="relative text-rose-400">{level.priceSats}</span>
              <span className="relative text-slate-300">
                {level.cumulative.toFixed(0)}
              </span>
            </div>
          );
        })}
        {book.asks.length === 0 && (
          <div className="py-2 text-center text-[10px] text-slate-600">
            No asks
          </div>
        )}

        {/* Spread */}
        <div className="border-y border-slate-800/50 py-1 text-center text-[10px] text-slate-500">
          {book.spread !== null ? `Spread: ${book.spread} sats` : "\u2014"}
        </div>

        {/* Bids */}
        {book.bids.length === 0 && (
          <div className="py-2 text-center text-[10px] text-slate-600">
            No bids
          </div>
        )}
        {/* Bids (best bid at top, widest bar at bottom) */}
        {cumBids.map((level) => {
          const pct = (level.cumulative / maxCumulative) * 100;
          return (
            <div
              key={`bid-${level.priceSats}`}
              className="relative flex items-center justify-between px-2 py-0.5 text-xs"
            >
              <div
                className="absolute inset-y-0 right-0 bg-emerald-500/15"
                style={{ width: `${pct.toFixed(1)}%` }}
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
      </div>

      {/* Your Orders */}
      {myOrders.length > 0 && (
        <div className="mt-3 rounded-xl border border-slate-800 bg-slate-950/70 p-3">
          <span className="mb-2 block text-[10px] font-semibold uppercase tracking-wider text-slate-400">
            Your Orders
          </span>
          {myOrders.map((o) => {
            const cancelling = cancellingOrderId === o.id;
            return (
              <div
                key={o.id}
                className="flex items-center justify-between py-1 text-xs"
              >
                <span className="text-slate-300">
                  {o.direction_label} @ {o.price} sats &middot;{" "}
                  {o.offered_amount} offered
                </span>
                <button
                  type="button"
                  onClick={() => handleCancelOrder(o.id)}
                  disabled={cancelling}
                  className={`rounded border ${cancelling ? "border-slate-700 text-slate-500" : "border-rose-800 text-rose-400 hover:bg-rose-900/30"} px-2 py-0.5 text-xs transition`}
                >
                  {cancelling ? "Cancelling..." : "Cancel"}
                </button>
              </div>
            );
          })}
        </div>
      )}
    </>
  );
}
