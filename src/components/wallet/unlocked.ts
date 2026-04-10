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

const PAGE_SIZE = 10;

function paginationControls(
  page: number,
  totalItems: number,
  prevAction: string,
  nextAction: string,
): string {
  const totalPages = Math.max(1, Math.ceil(totalItems / PAGE_SIZE));
  if (totalPages <= 1) return "";
  return (
    '<div class="mt-3 flex items-center justify-between text-xs text-slate-400">' +
    "<button " +
    (page <= 0 ? "disabled " : "") +
    'data-action="' +
    prevAction +
    '" class="rounded-lg border border-slate-700 px-2.5 py-1 transition ' +
    (page <= 0
      ? "text-slate-600 cursor-not-allowed"
      : "text-slate-300 hover:bg-slate-800") +
    '">&lsaquo; Prev</button>' +
    "<span>" +
    (page + 1) +
    " / " +
    totalPages +
    "</span>" +
    "<button " +
    (page >= totalPages - 1 ? "disabled " : "") +
    'data-action="' +
    nextAction +
    '" class="rounded-lg border border-slate-700 px-2.5 py-1 transition ' +
    (page >= totalPages - 1
      ? "text-slate-600 cursor-not-allowed"
      : "text-slate-300 hover:bg-slate-800") +
    '">Next &rsaquo;</button>' +
    "</div>"
  );
}

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
  const showBackupBadge =
    !wd?.backedUp && state.nostrBackupStatus?.has_backup === false;

  const creationTxToMarket = new Map(
    markets
      .filter((m) => m.creationTxid)
      .map((m) => [m.creationTxid as string, m.id]),
  );

  // Map covenant transition txids to labels for wallet transaction display.
  const covenantTxLabel = new Map<
    string,
    { label: string; marketId: string }
  >();
  for (const m of markets) {
    const entries: Array<[string | null, string]> = [
      [m.unresolvedTxid, "Issuance"],
      [m.resolvedYesTxid, "Resolved YES"],
      [m.resolvedNoTxid, "Resolved NO"],
      [m.expiredTxid, "Expired"],
    ];
    for (const [txid, label] of entries) {
      if (txid) covenantTxLabel.set(txid, { label, marketId: m.id });
    }
  }

  // Build txid->label map from own limit orders for transaction labeling
  const orderTxLabel = new Map<
    string,
    { label: string; marketId: string | null }
  >();
  for (const o of state.ownOrders) {
    if (o.creation_txid) {
      orderTxLabel.set(o.creation_txid, {
        label: `${o.direction_label ?? "Limit Order"} @ ${o.price}`,
        marketId: o.market_id,
      });
    }
  }

  // Well-known Liquid testnet assets (display-order hex).
  const KNOWN_ASSETS: Record<string, string> = {
    "38fca2d939696061a8f76d4e6b5eecd54e3b4221c846f24a6b279e79952850a5": "TEST",
  };

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
    if (m.yesReissuanceToken)
      assetLabel.set(reverseHex(m.yesReissuanceToken), {
        side: "YES RT",
        question: m.question,
        marketId: m.id,
      });
    if (m.noReissuanceToken)
      assetLabel.set(reverseHex(m.noReissuanceToken), {
        side: "NO RT",
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
    })
    .sort((a, b) => a.assetId.localeCompare(b.assetId));

  // Collect the user's limit orders across all markets
  const myLimitOrders: Array<{ order: DiscoveredOrder; market: Market }> = [];
  for (const m of markets) {
    for (const o of m.limitOrders) {
      if (state.nostrPubkey && o.creator_pubkey === state.nostrPubkey) {
        myLimitOrders.push({ order: o, market: m });
      }
    }
  }

  const poolCreationTx = new Map(
    state.myPools
      .filter((p) => p.creation_txid)
      .map((p) => [p.creation_txid, p.market_id]),
  );

  // Map LMSR pool trade transitions to labels for wallet transaction display.
  const poolTradeTx = new Map<string, string>();
  for (const [marketId, entries] of state.priceHistory) {
    for (const entry of entries) {
      if (entry.transition_txid) {
        poolTradeTx.set(entry.transition_txid, marketId);
      }
    }
  }

  const allTxCount = (wd?.transactions ?? []).length;
  const txRows = renderWalletTransactionRows({
    creationTxToMarket,
    orderTxLabel,
    poolCreationTx,
    poolTradeTx,
    covenantTxLabel,
    recentTxLabels: state.recentTxLabels,
    pawIcon: PAW_ICON,
    walletData: wd ?? null,
    page: state.walletTxPage,
    pageSize: PAGE_SIZE,
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
    <div class="px-6 py-6">
      <div class="space-y-6">
        <div class="flex items-center justify-between">
          <h2 class="flex items-center gap-2 text-xl font-medium text-slate-100">${networkBadge}</h2>
          <div class="flex gap-2">
            <button data-action="sync-wallet" class="relative rounded-lg border border-slate-700 px-4 py-2 text-sm text-slate-300 hover:bg-slate-800" ${loading ? "disabled" : ""}>Sync${loading ? '<span class="absolute inset-0 flex items-center justify-center rounded-lg bg-slate-800"><span class="h-3.5 w-3.5 rounded-full border-2 border-transparent border-t-emerald-400 animate-[spin_0.8s_steps(8)_infinite]"></span></span>' : ""}</button>
            <button data-action="show-backup" class="rounded-lg border px-4 py-2 text-sm transition ${showBackupBadge ? "border-amber-500/40 text-amber-200 hover:bg-amber-500/10" : "border-slate-700 text-slate-300 hover:bg-slate-800"}">
              <span class="flex items-center gap-2">
                <span>Backup</span>
                ${
                  showBackupBadge
                    ? '<span class="h-2 w-2 rounded-full bg-amber-300"></span>'
                    : ""
                }
              </span>
            </button>
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
          <div class="mt-1 font-medium tracking-tight text-slate-100 h-[2.5rem] flex items-center justify-center">${state.walletBalanceHidden ? `<span class="inline-flex items-center gap-1.5 text-3xl text-slate-500">${PAW_ICON}${PAW_ICON}${PAW_ICON}${PAW_ICON}</span>` : `<span class="text-3xl">${formatLbtc(policyBalance)}</span>`}</div>
          ${!state.walletBalanceHidden && state.baseCurrency !== "BTC" ? `<div class="text-sm text-slate-400 h-5 flex items-center justify-center">${satsToFiatStr(policyBalance)}</div>` : ""}
          <div class="mt-2 flex items-center justify-center gap-1 rounded-full border border-slate-700 mx-auto w-fit text-xs">
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
            .slice(state.walletTokenPage * PAGE_SIZE, (state.walletTokenPage + 1) * PAGE_SIZE)
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
                    ? '<span class="inline-flex items-center h-5 gap-0.5 text-slate-500">' +
                      PAW_ICON +
                      PAW_ICON +
                      "</span>"
                    : '<span class="inline-flex items-center h-5 mono text-slate-100">' +
                      tp.amount.toLocaleString() +
                      "</span>") +
                  "</div>"
                );
              }
              const knownName = KNOWN_ASSETS[tp.assetId];
              const label = knownName ?? shortAsset;
              const badge = knownName ?? "TOKEN";
              return (
                '<div class="flex items-center justify-between border-b border-slate-800 py-3 text-sm">' +
                '<div class="flex items-center gap-2">' +
                '<span class="rounded bg-slate-600/30 px-1.5 py-0.5 text-[10px] font-medium text-slate-400">' +
                escapeHtml(badge) +
                "</span>" +
                '<button data-action="open-explorer-asset" data-asset="' +
                escapeAttr(tp.assetId) +
                '" class="' +
                (knownName
                  ? "text-slate-300 hover:text-slate-100"
                  : "mono text-xs text-slate-500 hover:text-slate-300") +
                ' transition cursor-pointer">' +
                escapeHtml(label) +
                "</button></div>" +
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
          ${paginationControls(state.walletTokenPage, tokenPositions.length, "wallet-tokens-prev", "wallet-tokens-next")}
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
                  ? `${m.question.slice(0, 45)}...`
                  : m.question;
              const cancelling = state.cancellingOrderId === o.id;
              const dirColor =
                o.direction === "sell-quote"
                  ? "text-emerald-300 bg-emerald-500/20"
                  : "text-red-300 bg-red-500/20";
              const dirText = o.direction === "sell-quote" ? "BUY" : "SELL";
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
                  ? '<span class="inline-flex items-center h-5 gap-0.5 text-slate-500">' +
                    PAW_ICON +
                    PAW_ICON +
                    "</span>"
                  : '<span class="inline-flex items-center h-5 text-xs text-slate-400">' +
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

        ${
          state.myPools.length > 0
            ? `
        <!-- My Pools -->
        <div class="rounded-lg border border-slate-700 bg-slate-900/50 p-6">
          <h3 class="mb-3 font-semibold text-slate-100">My Pools</h3>
          ${state.myPools
            .map((p) => {
              return (
                '<div class="flex items-center justify-between border-b border-slate-800 py-3 text-sm">' +
                '<div class="flex items-center gap-2 min-w-0">' +
                '<span class="mono text-slate-300">' +
                escapeHtml(`${p.pool_id.slice(0, 10)}...`) +
                "</span>" +
                '<span class="text-xs text-slate-500">' +
                escapeHtml(`${p.market_id.slice(0, 10)}...`) +
                "</span>" +
                "</div>" +
                '<div class="flex items-center gap-3 shrink-0">' +
                (state.walletBalanceHidden
                  ? '<span class="inline-flex items-center h-5 gap-0.5 text-slate-500">' +
                    PAW_ICON +
                    PAW_ICON +
                    "</span>"
                  : '<span class="inline-flex items-center h-5 text-xs text-slate-400">Y:' +
                    p.reserve_yes +
                    " N:" +
                    p.reserve_no +
                    " L:" +
                    p.reserve_collateral +
                    "</span>") +
                '<span class="mono text-[10px] text-slate-500">' +
                escapeHtml(`${p.creation_txid.slice(0, 10)}...`) +
                "</span>" +
                "</div>" +
                "</div>"
              );
            })
            .join("")}
        </div>
        `
            : ""
        }

        <!-- Transactions -->
        <div class="rounded-lg border border-slate-700 bg-slate-900/50 p-6">
          <h3 class="mb-3 font-semibold text-slate-100">Transactions</h3>
          ${
            allTxCount === 0
              ? `<p class="text-sm text-slate-500">No transactions yet.</p>`
              : txRows + paginationControls(state.walletTxPage, allTxCount, "wallet-tx-prev", "wallet-tx-next")
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

        ${utxoSection}

        <!-- Backup modal rendered in renderTopShell -->
      </div>
    </div>
    ${modalHtml}
  `;
}
