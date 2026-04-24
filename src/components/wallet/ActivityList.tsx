import { openUrl } from "@tauri-apps/plugin-opener";
import { useCallback, useMemo } from "react";
import { useStore } from "../../store";
import type { Market, WalletData, WalletTransaction } from "../../types";
import { satsToFiatStr } from "../../utils-react/format";
import { formatLbtc } from "../../utils-react/wallet";
import { PawIcon } from "../shared/PawIcon";

const PAGE_SIZE = 10;

function TransactionRow({
  tx,
  label,
  labelColor,
  labelBg,
  labelMarketId,
  walletBalanceHidden,
  walletUnit,
  showLbtcLabel,
  baseCurrency,
  walletNetwork,
}: {
  tx: WalletTransaction;
  label: string | null;
  labelColor: string;
  labelBg: string;
  labelMarketId: string | null;
  walletBalanceHidden: boolean;
  walletUnit: "sats" | "btc";
  showLbtcLabel: boolean;
  baseCurrency: string;
  walletNetwork: string;
}) {
  const isLabeled = !!label;
  const sign = tx.balanceChange >= 0 ? "+" : "";
  const color = isLabeled
    ? labelColor === "amber"
      ? "text-amber-300"
      : "text-violet-300"
    : tx.balanceChange >= 0
      ? "text-emerald-300"
      : "text-red-300";
  const icon = isLabeled
    ? labelColor === "amber"
      ? "\u2666"
      : "\u25C6"
    : tx.balanceChange >= 0
      ? "\u2193"
      : "\u2191";

  const unconfirmed = tx.height == null;
  const date = tx.timestamp
    ? new Date(tx.timestamp * 1000).toLocaleString()
    : "unconfirmed";
  const shortTxid = `${tx.txid.slice(0, 10)}...${tx.txid.slice(-6)}`;
  const feeStr = formatLbtc(tx.fee, walletUnit, showLbtcLabel);
  const confirmations = unconfirmed ? "Unconfirmed" : `Block ${tx.height}`;

  const handleOpenMarket = useCallback(() => {
    if (labelMarketId) {
      useStore.setState({
        selectedMarketId: labelMarketId,
        view: "detail",
        walletOpen: false,
      });
    }
  }, [labelMarketId]);

  const handleOpenExplorerTx = useCallback(() => {
    const base =
      walletNetwork === "testnet"
        ? "https://liquid.network/testnet"
        : "https://liquid.network";
    void openUrl(`${base}/tx/${tx.txid}`);
  }, [tx.txid, walletNetwork]);

  return (
    <details className="border-b border-slate-800 text-sm select-none group">
      <summary className="flex items-center justify-between py-3 cursor-pointer list-none [&::-webkit-details-marker]:hidden">
        <div className="flex items-center gap-2">
          <span className={color}>{icon}</span>
          {label && labelMarketId ? (
            <button
              type="button"
              onClick={(e) => {
                e.preventDefault();
                handleOpenMarket();
              }}
              className={`rounded ${labelBg} px-1.5 py-0.5 text-[10px] font-medium ${labelColor === "amber" ? "text-amber-300 hover:bg-amber-500/30" : "text-violet-300 hover:bg-violet-500/30"} transition cursor-pointer`}
            >
              {label}
            </button>
          ) : label ? (
            <span
              className={`rounded ${labelBg} px-1.5 py-0.5 text-[10px] font-medium ${labelColor === "amber" ? "text-amber-300" : "text-violet-300"}`}
            >
              {label}
            </span>
          ) : null}
          <span
            className={
              unconfirmed
                ? tx.balanceChange >= 0
                  ? "text-emerald-400/80"
                  : "text-red-400/80"
                : "text-slate-500"
            }
          >
            {date}
          </span>
        </div>
        <div className="flex items-center gap-2">
          <div className="relative text-right">
            {walletBalanceHidden ? (
              <>
                <span className={`${color} invisible`}>
                  {sign}
                  {formatLbtc(tx.balanceChange, walletUnit, showLbtcLabel)}
                </span>
                <span className="absolute inset-0 inline-flex items-center justify-end gap-0.5 text-slate-500">
                  <PawIcon />
                  <PawIcon />
                </span>
              </>
            ) : (
              <>
                <span className={color}>
                  {sign}
                  {formatLbtc(tx.balanceChange, walletUnit, showLbtcLabel)}
                </span>
                {baseCurrency !== "BTC" && (
                  <div className="text-xs text-slate-500">
                    {satsToFiatStr(
                      Math.abs(tx.balanceChange),
                      baseCurrency as "USD" | "EUR" | "BTC",
                    )}
                  </div>
                )}
              </>
            )}
          </div>
          <span className="text-slate-600 text-xs transition-transform group-open:rotate-180">
            &#9660;
          </span>
        </div>
      </summary>
      <div className="pb-3 pt-1 pl-6 flex flex-col gap-1.5 text-xs">
        <div className="flex items-center gap-2">
          <span className="text-slate-500">TXID</span>
          <button
            type="button"
            onClick={handleOpenExplorerTx}
            className="mono text-slate-400 hover:text-slate-200 transition cursor-pointer"
          >
            {shortTxid}
          </button>
        </div>
        <div className="flex items-center gap-2">
          <span className="text-slate-500">Fee</span>
          <span className="mono text-slate-400">{feeStr}</span>
        </div>
        <div className="flex items-center gap-2">
          <span className="text-slate-500">Status</span>
          <span
            className={`mono ${
              unconfirmed
                ? tx.balanceChange >= 0
                  ? "text-emerald-400/80"
                  : "text-red-400/80"
                : "text-slate-400"
            }`}
          >
            {confirmations}
          </span>
        </div>
      </div>
    </details>
  );
}

export function ActivityList({
  walletData,
  markets,
}: {
  walletData: WalletData | null;
  markets: Market[];
}) {
  const walletTxPage = useStore((s) => s.walletTxPage);
  const walletBalanceHidden = useStore((s) => s.walletBalanceHidden);
  const walletUnit = useStore((s) => s.walletUnit);
  const showLbtcLabel = useStore((s) => s.showLbtcLabel);
  const baseCurrency = useStore((s) => s.baseCurrency);
  const walletNetwork = useStore((s) => s.walletNetwork);
  const recentTxLabels = useStore((s) => s.recentTxLabels);
  const ownOrders = useStore((s) => s.ownOrders);
  const myPools = useStore((s) => s.myPools);
  const priceHistory = useStore((s) => s.priceHistory);

  // Build maps for labeling
  const {
    creationTxToMarket,
    covenantTxLabel,
    orderTxLabel,
    poolCreationTx,
    poolTradeTx,
  } = useMemo(() => {
    const creation = new Map(
      markets
        .filter((m) => m.creationTxid)
        .map((m) => [m.creationTxid as string, m.id]),
    );

    const covenant = new Map<string, { label: string; marketId: string }>();
    for (const m of markets) {
      const entries: Array<[string | null, string]> = [
        [m.unresolvedTxid, "Issuance"],
        [m.resolvedYesTxid, "Resolved YES"],
        [m.resolvedNoTxid, "Resolved NO"],
        [m.expiredTxid, "Expired"],
      ];
      for (const [txid, label] of entries) {
        if (txid) covenant.set(txid, { label, marketId: m.id });
      }
    }

    const order = new Map<string, { label: string; marketId: string | null }>();
    for (const o of ownOrders) {
      if (o.creation_txid) {
        order.set(o.creation_txid, {
          label: `${o.direction_label ?? "Limit Order"} @ ${o.price}`,
          marketId: o.market_id,
        });
      }
    }

    const poolCreation = new Map(
      myPools
        .filter((p) => p.creation_txid)
        .map((p) => [p.creation_txid, p.market_id]),
    );

    const poolTrade = new Map<string, string>();
    for (const [marketId, entries] of priceHistory) {
      for (const entry of entries) {
        if (entry.transition_txid) {
          poolTrade.set(entry.transition_txid, marketId);
        }
      }
    }

    return {
      creationTxToMarket: creation,
      covenantTxLabel: covenant,
      orderTxLabel: order,
      poolCreationTx: poolCreation,
      poolTradeTx: poolTrade,
    };
  }, [markets, ownOrders, myPools, priceHistory]);

  const allTxs = walletData?.transactions ?? [];
  const totalPages = Math.max(1, Math.ceil(allTxs.length / PAGE_SIZE));
  const pageTxs = allTxs.slice(
    walletTxPage * PAGE_SIZE,
    (walletTxPage + 1) * PAGE_SIZE,
  );

  const getTxLabel = useCallback(
    (
      tx: WalletTransaction,
    ): {
      label: string | null;
      color: string;
      bg: string;
      marketId: string | null;
    } => {
      const recentInfo = recentTxLabels.get(tx.txid);
      if (recentInfo)
        return {
          label: recentInfo.label,
          color: "violet",
          bg: "bg-violet-500/20",
          marketId: recentInfo.marketId,
        };

      const orderInfo = orderTxLabel.get(tx.txid);
      if (orderInfo)
        return {
          label: orderInfo.label,
          color: "amber",
          bg: "bg-amber-500/20",
          marketId: orderInfo.marketId,
        };

      const covenantInfo = covenantTxLabel.get(tx.txid);
      if (covenantInfo)
        return {
          label: covenantInfo.label,
          color: "violet",
          bg: "bg-violet-500/20",
          marketId: covenantInfo.marketId,
        };

      const poolMarketId = poolCreationTx.get(tx.txid);
      if (poolMarketId)
        return {
          label: "Pool Creation",
          color: "violet",
          bg: "bg-violet-500/20",
          marketId: poolMarketId,
        };

      const poolTradeMarketId = poolTradeTx.get(tx.txid);
      if (poolTradeMarketId)
        return {
          label: "Pool Trade",
          color: "violet",
          bg: "bg-violet-500/20",
          marketId: poolTradeMarketId,
        };

      const marketId = creationTxToMarket.get(tx.txid);
      if (marketId)
        return {
          label: "Market Creation",
          color: "violet",
          bg: "bg-violet-500/20",
          marketId,
        };

      return { label: null, color: "", bg: "", marketId: null };
    },
    [
      recentTxLabels,
      orderTxLabel,
      covenantTxLabel,
      poolCreationTx,
      poolTradeTx,
      creationTxToMarket,
    ],
  );

  return (
    <div className="rounded-lg border border-slate-700 bg-slate-900/50 p-6">
      <h3 className="mb-3 font-semibold text-slate-100">Transactions</h3>
      {allTxs.length === 0 ? (
        <p className="text-sm text-slate-500">No transactions yet.</p>
      ) : (
        <>
          {pageTxs.map((tx) => {
            const { label, color, bg, marketId } = getTxLabel(tx);
            return (
              <TransactionRow
                key={tx.txid}
                tx={tx}
                label={label}
                labelColor={color}
                labelBg={bg}
                labelMarketId={marketId}
                walletBalanceHidden={walletBalanceHidden}
                walletUnit={walletUnit}
                showLbtcLabel={showLbtcLabel}
                baseCurrency={baseCurrency}
                walletNetwork={walletNetwork}
              />
            );
          })}
          {totalPages > 1 && (
            <div className="mt-3 flex items-center justify-between text-xs text-slate-400">
              <button
                type="button"
                disabled={walletTxPage <= 0}
                onClick={() =>
                  useStore.setState((s) => ({
                    walletTxPage: Math.max(0, s.walletTxPage - 1),
                  }))
                }
                className={`rounded-lg border border-slate-700 px-2.5 py-1 transition ${walletTxPage <= 0 ? "text-slate-600 cursor-not-allowed" : "text-slate-300 hover:bg-slate-800"}`}
              >
                &lsaquo; Prev
              </button>
              <span>
                {walletTxPage + 1} / {totalPages}
              </span>
              <button
                type="button"
                disabled={walletTxPage >= totalPages - 1}
                onClick={() =>
                  useStore.setState((s) => ({
                    walletTxPage: s.walletTxPage + 1,
                  }))
                }
                className={`rounded-lg border border-slate-700 px-2.5 py-1 transition ${walletTxPage >= totalPages - 1 ? "text-slate-600 cursor-not-allowed" : "text-slate-300 hover:bg-slate-800"}`}
              >
                Next &rsaquo;
              </button>
            </div>
          )}
        </>
      )}
    </div>
  );
}
