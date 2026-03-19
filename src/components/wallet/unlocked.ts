import { btcLabel, formatLbtc, satsLabel } from "../../services/wallet.ts";
import { markets, state } from "../../state.ts";
import type { DiscoveredOrder, Market } from "../../types.ts";
import { reverseHex } from "../../utils/crypto.ts";
import { satsToFiatStr } from "../../utils/format.ts";
import { escapeAttr, escapeHtml } from "../../utils/html.ts";
import {
  renderWalletSwapRows,
  renderWalletTransactionRows,
} from "./activity.ts";
import { renderWalletUtxoSection, type WalletAssetLabel } from "./utxos.ts";

export function renderWalletUnlocked(params: {
  errorHtml: string;
  loading: boolean;
  loadingHtml: string;
  networkBadge: string;
  modalHtml: string;
  pawIcon: string;
}): string {
  const { errorHtml, loading, loadingHtml, networkBadge, modalHtml, pawIcon } =
    params;
  const PAW_ICON = pawIcon;
  const wd = state.walletData;
  const policyBalance =
    wd && state.walletPolicyAssetId
      ? (wd.balance[state.walletPolicyAssetId] ?? 0)
      : 0;

  const creationTxToMarket = new Map(
    markets
      .filter((m) => m.creationTxid)
      .map((m) => [m.creationTxid as string, m.id]),
  );

  // Build txid->label map from own limit orders for transaction labeling
  const orderTxLabel = new Map<string, { label: string; marketId: string | null }>();
  for (const o of state.ownOrders) {
    if (o.creation_txid) {
      orderTxLabel.set(o.creation_txid, {
        label: `${o.direction_label ?? "Limit Order"} @ ${o.price}`,
        marketId: o.market_id,
      });
    }
  }

  // Map token asset IDs to labels for display.
  // Market asset IDs are internal byte order; wallet balance keys are display order (reversed).
  const assetLabel = new Map<string, WalletAssetLabel>();
  for (const m of markets) {
    if (m.yesAssetId)
      assetLabel.set(reverseHex(m.yesAssetId), {
        side: "YES",
        question: m.question,
        marketId: m.id,
      });
    if (m.noAssetId)
      assetLabel.set(reverseHex(m.noAssetId), {
        side: "NO",
        question: m.question,
        marketId: m.id,
      });
  }

  // Token positions: non-policy assets with positive balance
  const tokenPositions = Object.entries(wd?.balance ?? {})
    .filter(([id, amt]) => id !== state.walletPolicyAssetId && amt > 0)
    .map(([id, amt]) => {
      const info = assetLabel.get(id);
      return { assetId: id, amount: amt, info };
    });

  // Collect the user's limit orders across all markets
  const myLimitOrders: Array<{ order: DiscoveredOrder; market: Market }> = [];
  for (const m of markets) {
    for (const o of m.limitOrders) {
      if (state.nostrPubkey && o.creator_pubkey === state.nostrPubkey) {
        myLimitOrders.push({ order: o, market: m });
      }
    }
  }

  const txRows = renderWalletTransactionRows({
    creationTxToMarket,
    orderTxLabel,
    pawIcon: PAW_ICON,
    walletData: wd ?? null,
  });

  const swapRows = renderWalletSwapRows({
    pawIcon: PAW_ICON,
    walletData: wd ?? null,
  });

  const utxoSection = renderWalletUtxoSection({
    assetLabel,
    utxos: wd?.utxos ?? [],
  });

  return `
    <div class="phi-container py-8">
      <div class="mx-auto max-w-2xl space-y-6">
        <div class="flex items-center justify-between">
          <h2 class="flex items-center gap-2 text-2xl font-medium text-slate-100">Wallet ${networkBadge}</h2>
          <div class="flex gap-2">
            <button data-action="sync-wallet" class="rounded-lg border border-slate-700 px-4 py-2 text-sm text-slate-300 hover:bg-slate-800" ${loading ? "disabled" : ""}>Sync</button>
            <button data-action="show-backup" class="rounded-lg border border-slate-700 px-4 py-2 text-sm text-slate-300 hover:bg-slate-800">Backup</button>
            <button data-action="lock-wallet" class="rounded-lg border border-slate-700 px-4 py-2 text-sm text-slate-300 hover:bg-slate-800">Lock</button>
          </div>
        </div>

        ${errorHtml}
        ${loadingHtml}

        <!-- Balance -->
        <div class="rounded-lg border border-slate-700 bg-slate-900/50 p-6 text-center">
          <div class="flex items-center justify-center gap-2 text-sm text-slate-400">
            <span>Balance</span>
            <button data-action="toggle-balance-hidden" class="text-slate-500 hover:text-slate-300" title="${state.walletBalanceHidden ? "Show balance" : "Hide balance"}">
              ${
                state.walletBalanceHidden
                  ? `<svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 12C5 7 8.5 5 12 5s7 2 10 7c-3 5-6.5 7-10 7S5 17 2 12z"/><ellipse cx="12" cy="12" rx="2" ry="3.5"/><line x1="2" y1="2" x2="22" y2="22"/></svg>`
                  : `<svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 12C5 7 8.5 5 12 5s7 2 10 7c-3 5-6.5 7-10 7S5 17 2 12z"/><ellipse cx="12" cy="12" rx="2" ry="3.5"/></svg>`
              }
            </button>
          </div>
          <div class="mt-1 text-3xl font-medium tracking-tight text-slate-100">${state.walletBalanceHidden ? `<span class="inline-flex gap-1 text-slate-500">${PAW_ICON}${PAW_ICON}${PAW_ICON}${PAW_ICON}</span>` : formatLbtc(policyBalance)}</div>
          ${!state.walletBalanceHidden && state.baseCurrency !== "BTC" ? `<div class="mt-1 text-sm text-slate-400">${satsToFiatStr(policyBalance)}</div>` : ""}
          <div class="mt-3 flex items-center justify-center gap-1 rounded-full border border-slate-700 mx-auto w-fit text-xs">
            <button data-action="set-wallet-unit" data-unit="sats" class="rounded-full px-3 py-1 transition ${state.walletUnit === "sats" ? "bg-slate-700 text-slate-100" : "text-slate-400 hover:text-slate-200"}">${satsLabel()}</button>
            <button data-action="set-wallet-unit" data-unit="btc" class="rounded-full px-3 py-1 transition ${state.walletUnit === "btc" ? "bg-slate-700 text-slate-100" : "text-slate-400 hover:text-slate-200"}">${btcLabel()}</button>
          </div>
        </div>

        <!-- Action Buttons -->
        <div class="grid grid-cols-2 gap-4">
          <button data-action="open-receive" class="flex items-center justify-center gap-3 rounded-xl border border-emerald-400/30 bg-emerald-900/20 px-6 py-4 font-semibold text-emerald-300 transition hover:bg-emerald-900/40">
            <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="5" x2="12" y2="19"/><polyline points="19 12 12 19 5 12"/></svg>
            Receive
          </button>
          <button data-action="open-send" class="flex items-center justify-center gap-3 rounded-xl border border-slate-600 bg-slate-800/60 px-6 py-4 font-medium text-slate-200 transition hover:bg-slate-800">
            <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="19" x2="12" y2="5"/><polyline points="5 12 12 5 19 12"/></svg>
            Send
          </button>
        </div>

        ${
          tokenPositions.length === 0
            ? `
        <!-- No Positions -->
        <div class="rounded-lg border border-slate-700 bg-slate-900/50 p-6 text-center">
          <p class="text-sm text-slate-400">No token positions yet</p>
          <p class="mt-1 text-xs text-slate-500">Issue tokens on a market to start trading</p>
        </div>
        `
            : ""
        }

        ${
          tokenPositions.length > 0
            ? `
        <!-- Token Positions -->
        <div class="rounded-lg border border-slate-700 bg-slate-900/50 p-6">
          <h3 class="mb-3 font-semibold text-slate-100">Token Positions</h3>
          ${tokenPositions
            .map((tp) => {
              const shortAsset = `${tp.assetId.slice(0, 8)}...${tp.assetId.slice(-4)}`;
              if (tp.info) {
                const sideColor =
                  tp.info.side === "YES" ? "text-emerald-300" : "text-red-300";
                const sideBg =
                  tp.info.side === "YES"
                    ? "bg-emerald-500/20"
                    : "bg-red-500/20";
                const truncQ =
                  tp.info.question.length > 50
                    ? `${tp.info.question.slice(0, 50)}...`
                    : tp.info.question;
                return (
                  '<div class="flex items-center justify-between border-b border-slate-800 py-3 text-sm">' +
                  '<div class="flex items-center gap-2">' +
                  '<span class="rounded ' +
                  sideBg +
                  " px-1.5 py-0.5 text-[10px] font-medium " +
                  sideColor +
                  '">' +
                  tp.info.side +
                  "</span>" +
                  '<button data-open-market="' +
                  escapeAttr(tp.info.marketId) +
                  '" class="text-slate-300 hover:text-slate-100 transition cursor-pointer text-left">' +
                  escapeHtml(truncQ) +
                  "</button>" +
                  "</div>" +
                  (state.walletBalanceHidden
                    ? '<span class="inline-flex gap-0.5 text-slate-500">' +
                      PAW_ICON +
                      PAW_ICON +
                      "</span>"
                    : '<span class="mono text-slate-100">' +
                      tp.amount.toLocaleString() +
                      "</span>") +
                  "</div>"
                );
              }
              return (
                '<div class="flex items-center justify-between border-b border-slate-800 py-3 text-sm">' +
                '<span class="mono text-slate-400">' +
                escapeHtml(shortAsset) +
                "</span>" +
                (state.walletBalanceHidden
                  ? '<span class="inline-flex gap-0.5 text-slate-500">' +
                    PAW_ICON +
                    PAW_ICON +
                    "</span>"
                  : '<span class="mono text-slate-100">' +
                    tp.amount.toLocaleString() +
                    "</span>") +
                "</div>"
              );
            })
            .join("")}
        </div>
        `
            : ""
        }

        ${
          myLimitOrders.length > 0
            ? `
        <!-- Limit Orders -->
        <div class="rounded-lg border border-slate-700 bg-slate-900/50 p-6">
          <h3 class="mb-3 font-semibold text-slate-100">Limit Orders</h3>
          ${myLimitOrders
            .map(({ order: o, market: m }) => {
              const truncQ =
                m.question.length > 45
                  ? m.question.slice(0, 45) + "..."
                  : m.question;
              const cancelling = state.cancellingOrderId === o.id;
              const dirColor =
                o.direction === "sell-quote"
                  ? "text-emerald-300 bg-emerald-500/20"
                  : "text-red-300 bg-red-500/20";
              const dirText =
                o.direction === "sell-quote" ? "BUY" : "SELL";
              return (
                '<div class="flex items-center justify-between border-b border-slate-800 py-3 text-sm">' +
                '<div class="flex items-center gap-2 min-w-0">' +
                '<span class="shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium ' +
                dirColor +
                '">' +
                dirText +
                "</span>" +
                '<button data-open-market="' +
                escapeAttr(m.id) +
                '" class="truncate text-slate-300 hover:text-slate-100 transition cursor-pointer text-left">' +
                escapeHtml(truncQ) +
                "</button>" +
                "</div>" +
                '<div class="flex items-center gap-3 shrink-0">' +
                (state.walletBalanceHidden
                  ? '<span class="inline-flex gap-0.5 text-slate-500">' +
                    PAW_ICON +
                    PAW_ICON +
                    "</span>"
                  : '<span class="text-xs text-slate-400">' +
                    o.price +
                    " sats &middot; " +
                    o.offered_amount.toLocaleString() +
                    " offered</span>") +
                '<button data-action="cancel-limit-order" data-order-id="' +
                escapeAttr(o.id) +
                '"' +
                (cancelling ? " disabled" : "") +
                ' class="rounded border px-2 py-0.5 text-xs transition ' +
                (cancelling
                  ? "border-slate-700 text-slate-500"
                  : "border-rose-800 text-rose-400 hover:bg-rose-900/30") +
                '">' +
                (cancelling ? "Cancelling..." : "Cancel") +
                "</button>" +
                "</div>" +
                "</div>"
              );
            })
            .join("")}
        </div>
        `
            : ""
        }

        ${utxoSection}

        <!-- Transactions -->
        <div class="rounded-lg border border-slate-700 bg-slate-900/50 p-6">
          <h3 class="mb-3 font-semibold text-slate-100">Transactions</h3>
          ${
            (wd?.transactions ?? []).length === 0
              ? `<p class="text-sm text-slate-500">No transactions yet.</p>`
              : txRows
          }
        </div>

        <!-- Swaps -->
        ${
          (wd?.swaps ?? []).length > 0
            ? `
        <div class="rounded-lg border border-slate-700 bg-slate-900/50 p-6">
          <h3 class="mb-3 font-semibold text-slate-100">Swaps</h3>
          ${swapRows}
        </div>
        `
            : ""
        }

        <!-- Backup modal rendered in renderTopShell -->
      </div>
    </div>
    ${modalHtml}
  `;
}
