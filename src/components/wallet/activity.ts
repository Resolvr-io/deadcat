import {
  flowLabel,
  formatLbtc,
  formatSwapStatus,
} from "../../services/wallet.ts";
import { state } from "../../state.ts";
import type { WalletData } from "../../types.ts";
import { satsToFiatStr } from "../../utils/format.ts";
import { escapeAttr, escapeHtml } from "../../utils/html.ts";

export function renderWalletTransactionRows(params: {
  creationTxToMarket: Map<string, string>;
  orderTxLabel: Map<string, { label: string; marketId: string | null }>;
  poolCreationTx: Map<string, string>;
  poolTradeTx: Map<string, string>;
  covenantTxLabel: Map<string, { label: string; marketId: string }>;
  recentTxLabels: Map<string, { label: string; marketId: string }>;
  pawIcon: string;
  walletData: WalletData | null;
  page?: number;
  pageSize?: number;
}): string {
  const {
    creationTxToMarket,
    orderTxLabel,
    poolCreationTx,
    poolTradeTx,
    covenantTxLabel,
    recentTxLabels,
    pawIcon,
    walletData,
    page,
    pageSize,
  } = params;

  let txs = walletData?.transactions ?? [];
  if (page != null && pageSize != null) {
    txs = txs.slice(page * pageSize, (page + 1) * pageSize);
  }

  return txs
    .map((tx) => {
      const marketId = creationTxToMarket.get(tx.txid);
      const isCreation = !!marketId;
      const recentInfo = recentTxLabels.get(tx.txid);
      const isRecent = !!recentInfo;
      const poolMarketId = poolCreationTx.get(tx.txid);
      const isPoolCreation = !!poolMarketId;
      const poolTradeMarketId = poolTradeTx.get(tx.txid);
      const isPoolTrade = !!poolTradeMarketId;
      const covenantInfo = covenantTxLabel.get(tx.txid);
      const isCovenant = !!covenantInfo;
      const orderInfo = orderTxLabel.get(tx.txid);
      const isLimitOrder = !!orderInfo;
      const sign = tx.balanceChange >= 0 ? "+" : "";
      const isLabeled =
        isRecent || isCovenant || isPoolCreation || isPoolTrade || isCreation;
      const color = isLimitOrder
        ? "text-amber-300"
        : isLabeled
          ? "text-violet-300"
          : tx.balanceChange >= 0
            ? "text-emerald-300"
            : "text-red-300";
      const icon = isLimitOrder
        ? "&#9830;"
        : isLabeled
          ? "&#9670;"
          : tx.balanceChange >= 0
            ? "&#8595;"
            : "&#8593;";
      let label = "";
      if (isRecent) {
        label =
          '<button data-open-market="' +
          escapeAttr(recentInfo.marketId) +
          '" class="rounded bg-violet-500/20 px-1.5 py-0.5 text-[10px] font-medium text-violet-300 hover:bg-violet-500/30 transition cursor-pointer">' +
          escapeHtml(recentInfo.label) +
          "</button>";
      } else if (isLimitOrder) {
        const badgeContent = escapeHtml(orderInfo.label);
        if (orderInfo.marketId) {
          label =
            '<button data-open-market="' +
            escapeAttr(orderInfo.marketId) +
            '" class="rounded bg-amber-500/20 px-1.5 py-0.5 text-[10px] font-medium text-amber-300 hover:bg-amber-500/30 transition cursor-pointer">' +
            badgeContent +
            "</button>";
        } else {
          label =
            '<span class="rounded bg-amber-500/20 px-1.5 py-0.5 text-[10px] font-medium text-amber-300">' +
            badgeContent +
            "</span>";
        }
      } else if (isCovenant) {
        label =
          '<button data-open-market="' +
          escapeAttr(covenantInfo.marketId) +
          '" class="rounded bg-violet-500/20 px-1.5 py-0.5 text-[10px] font-medium text-violet-300 hover:bg-violet-500/30 transition cursor-pointer">' +
          escapeHtml(covenantInfo.label) +
          "</button>";
      } else if (isPoolCreation) {
        label =
          '<button data-open-market="' +
          escapeAttr(poolMarketId) +
          '" class="rounded bg-violet-500/20 px-1.5 py-0.5 text-[10px] font-medium text-violet-300 hover:bg-violet-500/30 transition cursor-pointer">Pool Creation</button>';
      } else if (isPoolTrade) {
        label =
          '<button data-open-market="' +
          escapeAttr(poolTradeMarketId) +
          '" class="rounded bg-violet-500/20 px-1.5 py-0.5 text-[10px] font-medium text-violet-300 hover:bg-violet-500/30 transition cursor-pointer">Pool Trade</button>';
      } else if (isCreation) {
        label =
          '<button data-open-market="' +
          escapeAttr(marketId) +
          '" class="rounded bg-violet-500/20 px-1.5 py-0.5 text-[10px] font-medium text-violet-300 hover:bg-violet-500/30 transition cursor-pointer">Market Creation</button>';
      }
      const date = tx.timestamp
        ? new Date(tx.timestamp * 1000).toLocaleString()
        : "unconfirmed";
      const shortTxid = `${tx.txid.slice(0, 10)}...${tx.txid.slice(-6)}`;
      const feeStr = (tx.fee / 1e8).toFixed(8).replace(/0+$/, "").replace(/\.$/, "");
      const confirmations = tx.height != null ? "Block " + tx.height : "Unconfirmed";
      return (
        '<details class="border-b border-slate-800 text-sm select-none group">' +
        '<summary class="flex items-center justify-between py-3 cursor-pointer list-none [&::-webkit-details-marker]:hidden">' +
        '<div class="flex items-center gap-2">' +
        '<span class="' +
        color +
        '">' +
        icon +
        "</span>" +
        label +
        '<span class="text-slate-500">' +
        escapeHtml(date) +
        "</span>" +
        "</div>" +
        '<div class="flex items-center gap-2">' +
        '<div class="text-right">' +
        (state.walletBalanceHidden
          ? '<span class="inline-flex gap-0.5 text-slate-500">' +
            pawIcon +
            pawIcon +
            "</span>"
          : '<span class="' +
            color +
            '">' +
            sign +
            formatLbtc(tx.balanceChange) +
            "</span>" +
            (state.baseCurrency !== "BTC"
              ? '<div class="text-xs text-slate-500">' +
                satsToFiatStr(Math.abs(tx.balanceChange)) +
                "</div>"
              : "")) +
        "</div>" +
        '<span class="text-slate-600 text-xs transition-transform group-open:rotate-180">&#9660;</span>' +
        "</div>" +
        "</summary>" +
        '<div class="pb-3 pt-1 pl-6 flex flex-col gap-1.5 text-xs">' +
        '<div class="flex items-center gap-2">' +
        '<span class="text-slate-500">TXID</span>' +
        '<button data-action="open-explorer-tx" data-txid="' +
        escapeAttr(tx.txid) +
        '" class="mono text-slate-400 hover:text-slate-200 transition cursor-pointer">' +
        escapeHtml(shortTxid) +
        "</button>" +
        "</div>" +
        '<div class="flex items-center gap-2">' +
        '<span class="text-slate-500">Fee</span>' +
        '<span class="mono text-slate-400">' +
        escapeHtml(feeStr) +
        " L-BTC</span>" +
        "</div>" +
        '<div class="flex items-center gap-2">' +
        '<span class="text-slate-500">Status</span>' +
        '<span class="mono text-slate-400">' +
        escapeHtml(confirmations) +
        "</span>" +
        "</div>" +
        '<div class="flex items-center gap-2">' +
        '<span class="text-slate-500">Type</span>' +
        '<span class="mono text-slate-400">' +
        escapeHtml(tx.txType) +
        "</span>" +
        "</div>" +
        "</div>" +
        "</details>"
      );
    })
    .join("");
}

export function renderWalletSwapRows(params: {
  pawIcon: string;
  walletData: WalletData | null;
}): string {
  const { pawIcon, walletData } = params;

  return (walletData?.swaps ?? [])
    .map((sw) => {
      return (
        '<div class="flex items-center justify-between border-b border-slate-800 py-3 text-sm">' +
        "<div>" +
        '<span class="text-slate-300">' +
        escapeHtml(flowLabel(sw.flow)) +
        "</span>" +
        (state.walletBalanceHidden
          ? '<span class="ml-2 inline-flex gap-0.5 text-slate-500">' +
            pawIcon +
            pawIcon +
            "</span>"
          : '<span class="ml-2 text-slate-500">' +
            sw.invoiceAmountSat.toLocaleString() +
            " sats</span>") +
        "</div>" +
        '<div class="flex items-center gap-2">' +
        '<span class="text-xs text-slate-500">' +
        escapeHtml(formatSwapStatus(sw.status)) +
        "</span>" +
        '<button data-action="refresh-swap" data-swap-id="' +
        escapeAttr(sw.id) +
        '" class="rounded border border-slate-700 px-2 py-1 text-xs text-slate-400 hover:bg-slate-800">Refresh</button>' +
        "</div>" +
        "</div>"
      );
    })
    .join("");
}
