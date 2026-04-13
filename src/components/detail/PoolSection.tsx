import { useCallback, useState } from "react";
import { marketToContractParamsJson } from "../../services/markets";
import {
  buildPoolParamsJson,
  closeLmsrPool,
  createLmsrPool,
  generateLmsrTable,
  scanLmsrPool,
} from "../../services/pools";
import { DEFAULT_TX_OPTIONS } from "../../services/tx";
import { useStore } from "../../store";
import type { Market } from "../../types";

export default function PoolSection({ market }: { market: Market }) {
  const poolCreateOpen = useStore((s) => s.poolCreateOpen);
  const myPools = useStore((s) => s.myPools);
  const showAdvancedActions = useStore((s) => s.showAdvancedActions);

  const marketPools = myPools.filter((p) => p.market_id === market.marketId);

  return (
    <>
      {/* Advanced actions toggle */}
      <section className="mt-4 rounded-xl border border-slate-800 bg-slate-950/50 p-3">
        <div className="flex items-center justify-between">
          <p className="text-xs text-slate-500">Advanced actions</p>
          <button
            type="button"
            onClick={() =>
              useStore.setState({
                showAdvancedActions: !showAdvancedActions,
              })
            }
            className="rounded border border-slate-700 px-2 py-1 text-xs text-slate-300"
          >
            {showAdvancedActions ? "Hide" : "Show"}
          </button>
        </div>
      </section>

      {/* Pool management */}
      <section className="mt-4 rounded-xl border border-slate-800 bg-slate-950/50 p-3">
        <div className="mb-2 flex items-center justify-between">
          <span className="text-[10px] font-semibold uppercase tracking-wider text-slate-400">
            Active Pools
          </span>
          <button
            type="button"
            onClick={() =>
              useStore.setState({ poolCreateOpen: !poolCreateOpen })
            }
            className="rounded border border-slate-700 px-2 py-1 text-xs text-slate-300"
          >
            {poolCreateOpen ? "Cancel" : "Create Pool"}
          </button>
        </div>

        {/* Existing pools list */}
        {marketPools.length === 0 ? (
          <p className="text-xs text-slate-500">
            No active pools for this market.
          </p>
        ) : (
          marketPools.map((p) => {
            const poolParams = (() => {
              try {
                return JSON.parse(p.params_json) as {
                  fee_bps?: number;
                  half_payout_sats?: number;
                };
              } catch {
                return null;
              }
            })();
            const feeLine =
              poolParams?.fee_bps != null ? `${poolParams.fee_bps} bps` : "";
            const payoutLine =
              poolParams?.half_payout_sats != null
                ? `U/2=${poolParams.half_payout_sats}`
                : "";
            const paramsLabel = [feeLine, payoutLine]
              .filter(Boolean)
              .join(" · ");

            return (
              <div
                key={p.pool_id}
                className="border-b border-slate-800 py-2 text-xs"
              >
                <div className="flex items-center justify-between">
                  <span className="mono text-slate-300">
                    {p.pool_id.slice(0, 10)}...
                  </span>
                  <span className="text-slate-400">
                    Y:{p.reserve_yes} N:{p.reserve_no} L:
                    {p.reserve_collateral} · s:{p.current_s_index}
                  </span>
                  <span className="flex gap-1">
                    <button
                      type="button"
                      onClick={async () => {
                        try {
                          const result = await scanLmsrPool(p.pool_id);
                          // Update the pool in the store
                          const pools = [...useStore.getState().myPools];
                          const idx = pools.findIndex(
                            (pp) => pp.pool_id === p.pool_id,
                          );
                          if (idx >= 0) {
                            pools[idx] = {
                              ...pools[idx],
                              current_s_index: result.current_s_index,
                              reserve_yes: result.reserve_yes,
                              reserve_no: result.reserve_no,
                              reserve_collateral: result.reserve_collateral,
                            };
                            useStore.setState({ myPools: pools });
                          }
                        } catch {
                          // scan failed silently
                        }
                      }}
                      className="rounded border border-slate-700 px-2 py-0.5 text-slate-400 transition hover:bg-slate-800"
                      title="Refresh pool state"
                    >
                      &#x21bb;
                    </button>
                    <button
                      type="button"
                      onClick={async () => {
                        try {
                          await closeLmsrPool(p.pool_id);
                          useStore.setState({
                            myPools: useStore
                              .getState()
                              .myPools.filter((pp) => pp.pool_id !== p.pool_id),
                          });
                        } catch {
                          // close failed silently
                        }
                      }}
                      className="rounded border border-rose-800 px-2 py-0.5 text-rose-400 transition hover:bg-rose-900/30"
                    >
                      Close
                    </button>
                  </span>
                </div>
                {paramsLabel && (
                  <div className="mt-0.5 text-slate-500">{paramsLabel}</div>
                )}
              </div>
            );
          })
        )}

        {/* Create pool form */}
        {poolCreateOpen && <CreatePoolForm market={market} />}
      </section>
    </>
  );
}

function CreatePoolForm({ market }: { market: Market }) {
  const [liquidity, setLiquidity] = useState(100);
  const [feeBps, setFeeBps] = useState(50);
  const [reservesYes, setReservesYes] = useState(100);
  const [reservesNo, setReservesNo] = useState(100);
  const [reservesLbtc, setReservesLbtc] = useState(50000);
  const [creating, setCreating] = useState(false);

  const handleCreate = useCallback(async () => {
    setCreating(true);
    try {
      const marketParamsJson = marketToContractParamsJson(market);
      const tableValues = await generateLmsrTable(
        liquidity,
        liquidity,
        1,
        0,
        market.cptSats,
      );
      const poolParamsJson = await buildPoolParamsJson({
        yesAssetId: market.yesAssetId,
        noAssetId: market.noAssetId,
        collateralAssetId: market.collateralAssetId,
        tableDepth: liquidity,
        qStepLots: 1,
        sBias: 0,
        sMaxIndex: tableValues.length - 1,
        halfPayoutSats: market.cptSats,
        feeBps,
        minRYes: 0,
        minRNo: 0,
        minRCollateral: 0,
        cosignerPubkey: "",
        tableValues,
      });

      await createLmsrPool(
        marketParamsJson,
        poolParamsJson,
        0,
        reservesYes,
        reservesNo,
        reservesLbtc,
        tableValues,
        DEFAULT_TX_OPTIONS,
      );

      // Refresh pools list
      const { listLmsrPools } = await import("../../services/pools");
      const pools = await listLmsrPools();
      useStore.setState({ myPools: pools, poolCreateOpen: false });
    } catch (error) {
      console.error("Pool creation failed:", error);
    }
    setCreating(false);
  }, [market, liquidity, feeBps, reservesYes, reservesNo, reservesLbtc]);

  return (
    <div className="mt-3 space-y-2">
      <div>
        <label
          className="mb-1 block text-xs text-slate-400"
          htmlFor="pool-liquidity"
        >
          Liquidity parameter
        </label>
        <input
          id="pool-liquidity"
          type="number"
          min="1"
          step="1"
          value={liquidity}
          onChange={(e) => setLiquidity(Number(e.target.value))}
          className="w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm"
        />
      </div>
      <div>
        <label
          className="mb-1 block text-xs text-slate-400"
          htmlFor="pool-fee-bps"
        >
          Fee BPS
        </label>
        <input
          id="pool-fee-bps"
          type="number"
          min="0"
          step="1"
          value={feeBps}
          onChange={(e) => setFeeBps(Number(e.target.value))}
          className="w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm"
        />
      </div>
      <div className="grid grid-cols-3 gap-2">
        <div>
          <label
            className="mb-1 block text-xs text-slate-400"
            htmlFor="pool-reserves-yes"
          >
            YES reserves
          </label>
          <input
            id="pool-reserves-yes"
            type="number"
            min="0"
            step="1"
            value={reservesYes}
            onChange={(e) => setReservesYes(Number(e.target.value))}
            className="w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm"
          />
        </div>
        <div>
          <label
            className="mb-1 block text-xs text-slate-400"
            htmlFor="pool-reserves-no"
          >
            NO reserves
          </label>
          <input
            id="pool-reserves-no"
            type="number"
            min="0"
            step="1"
            value={reservesNo}
            onChange={(e) => setReservesNo(Number(e.target.value))}
            className="w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm"
          />
        </div>
        <div>
          <label
            className="mb-1 block text-xs text-slate-400"
            htmlFor="pool-reserves-lbtc"
          >
            L-BTC reserves
          </label>
          <input
            id="pool-reserves-lbtc"
            type="number"
            min="0"
            step="1"
            value={reservesLbtc}
            onChange={(e) => setReservesLbtc(Number(e.target.value))}
            className="w-full rounded-lg border border-slate-700 bg-slate-950/80 px-3 py-2 text-sm"
          />
        </div>
      </div>
      <button
        type="button"
        onClick={handleCreate}
        disabled={creating}
        className={`mt-2 w-full rounded-lg ${creating ? "bg-slate-700 text-slate-400" : "bg-emerald-300 text-slate-950"} px-4 py-2 text-sm font-semibold`}
      >
        {creating ? "Creating..." : "Create Pool"}
      </button>
    </div>
  );
}
