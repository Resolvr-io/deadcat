import { flowLabel, formatLbtc, formatSwapStatus } from "../../services/wallet.ts";
import { markets, state } from "../../state.ts";
import type { WalletUtxo } from "../../types.ts";
import { reverseHex } from "../../utils/crypto.ts";
import { satsToFiatStr } from "../../utils/format.ts";

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

  // Map token asset IDs to labels for display.
  // Market asset IDs are internal byte order; wallet balance keys are display order (reversed).
  const assetLabel = new Map<
    string,
    { side: string; question: string; marketId: string }
  >();
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

  const txRows = (wd?.transactions ?? [])
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
          marketId +
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
        tx.txid +
        '" class="mono text-slate-400 hover:text-slate-200 transition cursor-pointer">' +
        shortTxid +
        "</button>" +
        label +
        '<span class="text-slate-500">' +
        date +
        "</span>" +
        "</div>" +
        '<div class="text-right">' +
        (state.walletBalanceHidden
          ? '<span class="inline-flex gap-0.5 text-slate-500">' +
            PAW_ICON +
            PAW_ICON +
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

  const swapRows = (wd?.swaps ?? [])
    .map((sw) => {
      return (
        '<div class="flex items-center justify-between border-b border-slate-800 py-3 text-sm">' +
        "<div>" +
        '<span class="text-slate-300">' +
        flowLabel(sw.flow) +
        "</span>" +
        (state.walletBalanceHidden
          ? '<span class="ml-2 inline-flex gap-0.5 text-slate-500">' +
            PAW_ICON +
            PAW_ICON +
            "</span>"
          : '<span class="ml-2 text-slate-500">' +
            sw.invoiceAmountSat.toLocaleString() +
            " sats</span>") +
        "</div>" +
        '<div class="flex items-center gap-2">' +
        '<span class="text-xs text-slate-500">' +
        formatSwapStatus(sw.status) +
        "</span>" +
        '<button data-action="refresh-swap" data-swap-id="' +
        sw.id +
        '" class="rounded border border-slate-700 px-2 py-1 text-xs text-slate-400 hover:bg-slate-800">Refresh</button>' +
        "</div>" +
        "</div>"
      );
    })
    .join("");

  // Build UTXO section
  let utxoSection = "";
  const utxos = wd?.utxos ?? [];
  if (!state.walletBalanceHidden && utxos.length > 0) {
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
          '<span class="mono text-slate-500 shrink-0">' +
            shortAsset +
            "</span>",
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

    utxoSection =
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
      "</div>";
  }

  return `
    <div class="phi-container py-8">
      <div class="mx-auto max-w-2xl space-y-6">
        <div class="flex items-center justify-between">
          <h2 class="flex items-center gap-2 text-2xl font-medium text-slate-100">Liquid Bitcoin Wallet ${networkBadge}</h2>
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
            <button data-action="set-wallet-unit" data-unit="sats" class="rounded-full px-3 py-1 transition ${state.walletUnit === "sats" ? "bg-slate-700 text-slate-100" : "text-slate-400 hover:text-slate-200"}">L-sats</button>
            <button data-action="set-wallet-unit" data-unit="btc" class="rounded-full px-3 py-1 transition ${state.walletUnit === "btc" ? "bg-slate-700 text-slate-100" : "text-slate-400 hover:text-slate-200"}">L-BTC</button>
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
                  tp.info.marketId +
                  '" class="text-slate-300 hover:text-slate-100 transition cursor-pointer text-left">' +
                  truncQ +
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
                shortAsset +
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
