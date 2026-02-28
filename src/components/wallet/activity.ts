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
  pawIcon: string;
  walletData: WalletData | null;
}): string {
  const { creationTxToMarket, pawIcon, walletData } = params;

  return (walletData?.transactions ?? [])
    .map((tx) => {
      const marketId = creationTxToMarket.get(tx.txid);
      const isCreation = !!marketId;
      const isIssuance = tx.txType === "issuance" || tx.txType === "reissuance";
      const sign = tx.balanceChange >= 0 ? "+" : "";
      const color =
        isCreation || isIssuance
          ? "text-violet-300"
          : tx.balanceChange >= 0
            ? "text-emerald-300"
            : "text-red-300";
      const icon =
        isCreation || isIssuance
          ? "&#9670;"
          : tx.balanceChange >= 0
            ? "&#8595;"
            : "&#8593;";
      let label = "";
      if (isCreation) {
        label =
          '<button data-open-market="' +
          escapeAttr(marketId) +
          '" class="rounded bg-violet-500/20 px-1.5 py-0.5 text-[10px] font-medium text-violet-300 hover:bg-violet-500/30 transition cursor-pointer">Market Creation</button>';
      } else if (isIssuance) {
        label =
          '<span class="rounded bg-violet-500/20 px-1.5 py-0.5 text-[10px] font-medium text-violet-300">Issuance</span>';
      }
      const date = tx.timestamp
        ? new Date(tx.timestamp * 1000).toLocaleString()
        : "unconfirmed";
      const shortTxid = `${tx.txid.slice(0, 10)}...${tx.txid.slice(-6)}`;
      return (
        '<div class="flex items-center justify-between border-b border-slate-800 py-3 text-sm select-none">' +
        '<div class="flex items-center gap-2">' +
        '<span class="' +
        color +
        '">' +
        icon +
        "</span>" +
        '<button data-action="open-explorer-tx" data-txid="' +
        escapeAttr(tx.txid) +
        '" class="mono text-slate-400 hover:text-slate-200 transition cursor-pointer">' +
        escapeHtml(shortTxid) +
        "</button>" +
        label +
        '<span class="text-slate-500">' +
        escapeHtml(date) +
        "</span>" +
        "</div>" +
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
        "</div>"
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
