import { formatLbtc } from "../../services/wallet.ts";
import { state } from "../../state.ts";
import type { WalletUtxo } from "../../types.ts";

export type WalletAssetLabel = {
  side: string;
  question: string;
  marketId: string;
};

export function renderWalletUtxoSection(params: {
  assetLabel: Map<string, WalletAssetLabel>;
  utxos: WalletUtxo[];
}): string {
  const { assetLabel, utxos } = params;
  if (state.walletBalanceHidden || utxos.length === 0) return "";

  const lbtcUtxos = utxos.filter(
    (u) => u.assetId === state.walletPolicyAssetId,
  );
  const tokenUtxos = utxos.filter(
    (u) => u.assetId !== state.walletPolicyAssetId,
  );
  const explorerBase =
    state.walletNetwork === "testnet"
      ? "https://blockstream.info/liquidtestnet"
      : "https://blockstream.info/liquid";

  const utxoRow = (u: WalletUtxo, labelHtml: string): string => {
    const shortOutpoint = `${u.txid.slice(0, 8)}...${u.txid.slice(-4)}:${u.vout}`;
    const conf = u.height !== null ? String(u.height) : "unconfirmed";
    const valueStr =
      u.assetId === state.walletPolicyAssetId
        ? formatLbtc(u.value)
        : u.value.toLocaleString();
    return (
      '<div class="flex items-center justify-between border-b border-slate-800 py-2 text-xs">' +
      '<div class="flex items-center gap-2 min-w-0">' +
      labelHtml +
      '<a href="' +
      explorerBase +
      "/tx/" +
      u.txid +
      '" target="_blank" rel="noopener" class="mono text-slate-500 hover:text-slate-300 transition truncate">' +
      shortOutpoint +
      "</a>" +
      '<span class="text-slate-600">' +
      conf +
      "</span>" +
      "</div>" +
      '<span class="mono text-slate-300 shrink-0 ml-2">' +
      valueStr +
      "</span>" +
      "</div>"
    );
  };

  const lbtcRows = lbtcUtxos
    .map((u) =>
      utxoRow(
        u,
        '<span class="rounded bg-slate-700 px-1.5 py-0.5 text-[10px] font-medium text-slate-300 shrink-0">L-BTC</span>',
      ),
    )
    .join("");

  const tokenUtxoRows = tokenUtxos
    .map((u) => {
      const info = assetLabel.get(u.assetId);
      if (info) {
        const sideColor =
          info.side === "YES" ? "text-emerald-300" : "text-red-300";
        const sideBg =
          info.side === "YES" ? "bg-emerald-500/20" : "bg-red-500/20";
        const truncQ =
          info.question.length > 35
            ? `${info.question.slice(0, 35)}...`
            : info.question;
        return utxoRow(
          u,
          '<span class="rounded ' +
            sideBg +
            " px-1.5 py-0.5 text-[10px] font-medium " +
            sideColor +
            ' shrink-0">' +
            info.side +
            "</span>" +
            '<button data-open-market="' +
            info.marketId +
            '" class="text-slate-400 hover:text-slate-200 transition cursor-pointer truncate text-left">' +
            truncQ +
            "</button>",
        );
      }
      const shortAsset = `${u.assetId.slice(0, 8)}...${u.assetId.slice(-4)}`;
      return utxoRow(
        u,
        `<span class="mono text-slate-500 shrink-0">${shortAsset}</span>`,
      );
    })
    .join("");

  const chevronClass = state.walletUtxosExpanded ? " rotate-180" : "";
  const expandedContent = state.walletUtxosExpanded
    ? '<div class="mt-3">' +
      (lbtcUtxos.length > 0
        ? '<div class="mb-1 text-[10px] font-medium uppercase tracking-wider text-slate-500">L-BTC</div>' +
          lbtcRows
        : "") +
      (tokenUtxos.length > 0
        ? '<div class="mt-3 mb-1 text-[10px] font-medium uppercase tracking-wider text-slate-500">Tokens</div>' +
          tokenUtxoRows
        : "") +
      "</div>"
    : "";

  return (
    "<!-- UTXOs -->" +
    '<div class="rounded-lg border border-slate-700 bg-slate-900/50 p-6">' +
    '<button data-action="toggle-utxos-expanded" class="flex w-full items-center justify-between">' +
    '<h3 class="font-semibold text-slate-100">UTXOs <span class="ml-1 text-xs font-normal text-slate-500">(' +
    utxos.length +
    ")</span></h3>" +
    '<svg class="h-4 w-4 text-slate-400 transition' +
    chevronClass +
    '" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M19 9l-7 7-7-7"/></svg>' +
    "</button>" +
    expandedContent +
    "</div>"
  );
}
