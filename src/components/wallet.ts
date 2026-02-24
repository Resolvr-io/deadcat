import { flowLabel, formatLbtc, formatSwapStatus } from "../services/wallet.ts";
import { markets, state } from "../state.ts";
import { reverseHex } from "../utils/crypto.ts";
import { satsToFiatStr } from "../utils/format.ts";
import {
  renderMnemonicGrid,
  renderModalTabs,
  renderReceiveModal,
  renderSendModal,
} from "./wallet-modals.ts";

export function renderWalletModal(): string {
  if (state.walletModal === "none") return "";

  const title =
    state.walletModal === "receive" ? "Receive Funds" : "Send Funds";
  const subtitle =
    state.walletModal === "receive"
      ? "Choose a method to receive funds into your Liquid wallet."
      : "Send funds from your wallet via Lightning, Liquid, or Bitcoin.";
  const body =
    state.walletModal === "receive" ? renderReceiveModal() : renderSendModal();

  return `
    <div data-action="modal-backdrop" class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div class="relative mx-4 w-full max-w-md rounded-2xl border border-slate-700 bg-slate-950 shadow-2xl">
        <div class="flex items-center justify-between border-b border-slate-800 px-6 py-4">
          <div>
            <h3 class="text-lg font-medium text-slate-100">${title}</h3>
            <p class="text-xs text-slate-400">${subtitle}</p>
          </div>
          <button data-action="close-modal" class="rounded-lg p-2 text-slate-400 hover:bg-slate-800 hover:text-slate-200">
            <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
          </button>
        </div>
        <div class="space-y-4 p-6">
          ${renderModalTabs()}
          ${body}
        </div>
      </div>
    </div>
  `;
}

export function renderWallet(): string {
  const loading = state.walletLoading;
  const error = state.walletError;

  const networkBadge =
    state.walletNetwork !== "mainnet"
      ? `<span class="rounded-full bg-amber-500/20 px-2.5 py-0.5 text-xs font-medium text-amber-300">${state.walletNetwork}</span>`
      : "";

  const errorHtml = error
    ? `<div class="rounded-lg border border-red-500/30 bg-red-900/20 px-4 py-3 text-sm text-red-300">${error}</div>`
    : "";

  const loadingHtml = "";

  if (state.walletStatus === "not_created") {
    if (state.walletMnemonic) {
      return `
        <div class="phi-container py-8">
          <div class="mx-auto max-w-lg space-y-6">
            <h2 class="flex items-center gap-2 text-2xl font-medium text-slate-100">Liquid Bitcoin Wallet Created ${networkBadge}</h2>
            <div class="rounded-lg border border-slate-600 bg-slate-900/40 p-4 space-y-3">
              <p class="text-sm font-medium text-slate-200">Back up your recovery phrase in a safe place.</p>
              ${renderMnemonicGrid(state.walletMnemonic)}
              <button data-action="copy-mnemonic" class="mt-2 w-full rounded-lg border border-slate-700 px-4 py-2 text-sm text-slate-300 hover:bg-slate-800">Copy to clipboard</button>
            </div>
            ${errorHtml}
            <button data-action="dismiss-mnemonic" class="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300">I've saved my recovery phrase</button>
          </div>
        </div>
      `;
    }

    const isCreate = !state.walletShowRestore;
    const isRestore = state.walletShowRestore;

    return `
      <div class="phi-container py-8">
        <div class="mx-auto max-w-lg space-y-6">
          <h2 class="flex items-center gap-2 text-2xl font-medium text-slate-100">Liquid Bitcoin Wallet ${networkBadge}</h2>
          <p class="text-sm text-slate-400">Set up a Liquid (L-BTC) wallet to participate in markets.</p>
          ${errorHtml}

          ${
            state.nostrNpub && !loading
              ? `<button data-action="nostr-restore-wallet" class="w-full rounded-xl border border-slate-700 bg-slate-900/50 p-4 text-left transition hover:border-slate-600">
            <div class="flex items-center gap-3">
              <svg class="h-6 w-6 text-slate-500 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5"><path stroke-linecap="round" stroke-linejoin="round" d="M12 16.5V9.75m0 0l3 3m-3-3l-3 3M6.75 19.5a4.5 4.5 0 01-1.41-8.775 5.25 5.25 0 0110.233-2.33 3 3 0 013.758 3.848A3.752 3.752 0 0118 19.5H6.75z"/></svg>
              <div>
                <p class="text-sm font-medium text-slate-300">Restore from Nostr Backup</p>
                <p class="mt-0.5 text-xs text-slate-500">Fetch encrypted backup from your relays</p>
              </div>
            </div>
          </button>`
              : ""
          }

          <div class="grid grid-cols-2 gap-3">
            <button data-action="${isCreate || loading ? "" : "toggle-restore"}" class="rounded-xl border ${isCreate ? "border-emerald-500/50 bg-emerald-500/10" : "border-slate-700 bg-slate-900/50 hover:border-slate-600"} p-4 text-left transition ${loading ? "opacity-50 cursor-not-allowed" : ""}" ${loading ? "disabled" : ""}>
              <svg class="h-6 w-6 ${isCreate ? "text-emerald-400" : "text-slate-500"}" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5"><path stroke-linecap="round" stroke-linejoin="round" d="M12 4.5v15m7.5-7.5h-15"/></svg>
              <p class="mt-2 text-sm font-medium ${isCreate ? "text-emerald-300" : "text-slate-300"}">Create New</p>
              <p class="mt-0.5 text-xs ${isCreate ? "text-emerald-400/60" : "text-slate-500"}">Generate a fresh wallet</p>
            </button>
            <button data-action="${isRestore || loading ? "" : "toggle-restore"}" class="rounded-xl border ${isRestore ? "border-emerald-500/50 bg-emerald-500/10" : "border-slate-700 bg-slate-900/50 hover:border-slate-600"} p-4 text-left transition ${loading ? "opacity-50 cursor-not-allowed" : ""}" ${loading ? "disabled" : ""}>
              <svg class="h-6 w-6 ${isRestore ? "text-emerald-400" : "text-slate-500"}" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5"><path stroke-linecap="round" stroke-linejoin="round" d="M19.5 12c0-1.232-.046-2.453-.138-3.662a4.006 4.006 0 00-3.7-3.7 48.678 48.678 0 00-7.324 0 4.006 4.006 0 00-3.7 3.7c-.017.22-.032.441-.046.662M19.5 12l3-3m-3 3l-3-3m-12 3c0 1.232.046 2.453.138 3.662a4.006 4.006 0 003.7 3.7 48.656 48.656 0 007.324 0 4.006 4.006 0 003.7-3.7c.017-.22.032-.441.046-.662M4.5 12l3 3m-3-3l-3 3"/></svg>
              <p class="mt-2 text-sm font-medium ${isRestore ? "text-emerald-300" : "text-slate-300"}">Restore</p>
              <p class="mt-0.5 text-xs ${isRestore ? "text-emerald-400/60" : "text-slate-500"}">From recovery phrase</p>
            </button>
          </div>

          ${
            isCreate
              ? `
            <div class="space-y-4 rounded-xl border border-slate-700 bg-slate-900/50 p-6">
              <div>
                <label for="wallet-password" class="text-xs font-medium text-slate-400">Encryption Password</label>
                <p class="mt-0.5 text-[11px] text-slate-500">Used to encrypt your wallet on this device.</p>
              </div>
              <input id="wallet-password" type="password" maxlength="32" value="${state.walletPassword}" placeholder="Enter a password" onpaste="return false" class="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 focus:ring-2 disabled:opacity-50" ${loading ? "disabled" : ""} />
              <button data-action="create-wallet" class="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300 transition disabled:opacity-50 disabled:cursor-not-allowed" ${loading ? "disabled" : ""}>${loading ? "Creating..." : "Create Wallet"}</button>
            </div>
          `
              : `
            <div class="space-y-4 rounded-xl border border-slate-700 bg-slate-900/50 p-6">
              <div>
                <label for="wallet-restore-mnemonic" class="text-xs font-medium text-slate-400">Recovery Phrase</label>
                <p class="mt-0.5 text-[11px] text-slate-500">Enter your 12-word recovery phrase to restore your wallet.</p>
              </div>
              <textarea id="wallet-restore-mnemonic" placeholder="word1 word2 word3 ..." rows="3" class="w-full rounded-lg border border-slate-700 bg-slate-900 px-4 py-3 text-sm outline-none ring-emerald-400 focus:ring-2 mono disabled:opacity-50" ${loading ? "disabled" : ""}>${state.walletRestoreMnemonic}</textarea>
              <div>
                <label for="wallet-password" class="text-xs font-medium text-slate-400">Encryption Password</label>
                <p class="mt-0.5 text-[11px] text-slate-500">Set a password to encrypt the restored wallet.</p>
              </div>
              <input id="wallet-password" type="password" maxlength="32" value="${state.walletPassword}" placeholder="Enter a password" onpaste="return false" class="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 focus:ring-2 disabled:opacity-50" ${loading ? "disabled" : ""} />
              <button data-action="restore-wallet" class="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300 transition disabled:opacity-50 disabled:cursor-not-allowed" ${loading ? "disabled" : ""}>${loading ? "Restoring..." : "Restore Wallet"}</button>
            </div>
          `
          }
        </div>
      </div>
    `;
  }

  if (state.walletStatus === "locked") {
    return `
      <div class="phi-container py-8">
        <div class="mx-auto max-w-lg space-y-6">
          <h2 class="flex items-center gap-2 text-2xl font-medium text-slate-100">Liquid Bitcoin Wallet ${networkBadge}</h2>
          <p class="text-sm text-slate-400">Wallet locked. Enter your password to unlock.</p>
          ${errorHtml}
          ${loadingHtml}
          <div class="space-y-4 rounded-lg border border-slate-700 bg-slate-900/50 p-6">
            <input id="wallet-password" type="password" maxlength="32" value="${state.walletPassword}" placeholder="Password" class="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 focus:ring-2 disabled:opacity-50" ${loading ? "disabled" : ""} />
            <button data-action="unlock-wallet" class="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300 disabled:opacity-50 disabled:cursor-not-allowed" ${loading ? "disabled" : ""}>${loading ? "Unlocking..." : "Unlock"}</button>
          </div>
          <details class="group">
            <summary class="cursor-pointer text-xs text-slate-500 hover:text-slate-400 transition select-none">Forgot your password?</summary>
            <div class="mt-3 rounded-lg border border-slate-800 bg-slate-900/50 p-4 space-y-3">
              <p class="text-xs text-slate-400">The password protects your wallet on this device only. If you've forgotten it, you can delete the wallet and restore it using either method below. <strong class="text-slate-300">Your funds are safe</strong> as long as you have your recovery phrase or nsec.</p>
              <div class="space-y-1.5">
                ${
                  state.nostrNpub
                    ? `<div class="flex items-start gap-2">
                  <svg class="mt-0.5 h-3.5 w-3.5 shrink-0 text-emerald-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"/></svg>
                  <p class="text-xs text-slate-400"><strong class="text-slate-300">Restore from Nostr backup</strong> — If you backed up to relays, your nsec is all you need. No password required.</p>
                </div>`
                    : ""
                }
                <div class="flex items-start gap-2">
                  <svg class="mt-0.5 h-3.5 w-3.5 shrink-0 text-slate-500" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M19.5 12c0-1.232-.046-2.453-.138-3.662a4.006 4.006 0 00-3.7-3.7 48.678 48.678 0 00-7.324 0 4.006 4.006 0 00-3.7 3.7c-.017.22-.032.441-.046.662M19.5 12l3-3m-3 3l-3-3m-12 3c0 1.232.046 2.453.138 3.662a4.006 4.006 0 003.7 3.7 48.656 48.656 0 007.324 0 4.006 4.006 0 003.7-3.7c.017-.22.032-.441.046-.662M4.5 12l3 3m-3-3l-3 3"/></svg>
                  <p class="text-xs text-slate-400"><strong class="text-slate-300">Restore from recovery phrase</strong> — Enter your 12-word seed phrase and set a new password.</p>
                </div>
              </div>
              <button data-action="forgot-password-delete" class="w-full rounded-lg border border-rose-700/40 px-4 py-2 text-xs text-rose-400 hover:bg-rose-900/20 transition">Delete Wallet & Restore</button>
            </div>
          </details>
        </div>
      </div>
    `;
  }

  // Unlocked — clean dashboard
  const policyBalance =
    state.walletBalance && state.walletPolicyAssetId
      ? (state.walletBalance[state.walletPolicyAssetId] ?? 0)
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
  const tokenPositions = state.walletBalance
    ? Object.entries(state.walletBalance)
        .filter(([id, amt]) => id !== state.walletPolicyAssetId && amt > 0)
        .map(([id, amt]) => {
          const info = assetLabel.get(id);
          return { assetId: id, amount: amt, info };
        })
    : [];

  const txRows = state.walletTransactions
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
        '<span class="' +
        color +
        '">' +
        sign +
        formatLbtc(tx.balanceChange) +
        "</span>" +
        (state.baseCurrency !== "BTC"
          ? '<div class="text-xs text-slate-500">' +
            satsToFiatStr(Math.abs(tx.balanceChange)) +
            "</div>"
          : "") +
        "</div>" +
        "</div>"
      );
    })
    .join("");

  const swapRows = state.walletSwaps
    .map((sw) => {
      return (
        '<div class="flex items-center justify-between border-b border-slate-800 py-3 text-sm">' +
        "<div>" +
        '<span class="text-slate-300">' +
        flowLabel(sw.flow) +
        "</span>" +
        '<span class="ml-2 text-slate-500">' +
        sw.invoiceAmountSat.toLocaleString() +
        " sats</span>" +
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
                  ? `<svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24"/><line x1="1" y1="1" x2="23" y2="23"/></svg>`
                  : `<svg class="h-4 w-4" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/></svg>`
              }
            </button>
          </div>
          <div class="mt-1 text-3xl font-medium tracking-tight text-slate-100">${state.walletBalanceHidden ? "********" : formatLbtc(policyBalance)}</div>
          ${!state.walletBalanceHidden && state.baseCurrency !== "BTC" ? `<div class="mt-1 text-sm text-slate-400">${satsToFiatStr(policyBalance)}</div>` : ""}
          <div class="mt-3 flex items-center justify-center gap-1 rounded-full border border-slate-700 mx-auto w-fit text-xs">
            <button data-action="set-wallet-unit" data-unit="sats" class="rounded-full px-3 py-1 transition ${state.walletUnit === "sats" ? "bg-slate-700 text-slate-100" : "text-slate-400 hover:text-slate-200"}">L-sats</button>
            <button data-action="set-wallet-unit" data-unit="btc" class="rounded-full px-3 py-1 transition ${state.walletUnit === "btc" ? "bg-slate-700 text-slate-100" : "text-slate-400 hover:text-slate-200"}">L-BTC</button>
          </div>
        </div>

        ${
          tokenPositions.length === 0 && !state.walletBalanceHidden
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
          tokenPositions.length > 0 && !state.walletBalanceHidden
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
                  '<span class="mono text-slate-100">' +
                  tp.amount.toLocaleString() +
                  "</span>" +
                  "</div>"
                );
              }
              return (
                '<div class="flex items-center justify-between border-b border-slate-800 py-3 text-sm">' +
                '<span class="mono text-slate-400">' +
                shortAsset +
                "</span>" +
                '<span class="mono text-slate-100">' +
                tp.amount.toLocaleString() +
                "</span>" +
                "</div>"
              );
            })
            .join("")}
        </div>
        `
            : ""
        }

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

        <!-- Transactions -->
        <div class="rounded-lg border border-slate-700 bg-slate-900/50 p-6">
          <h3 class="mb-3 font-semibold text-slate-100">Transactions</h3>
          ${
            state.walletTransactions.length === 0
              ? `<p class="text-sm text-slate-500">No transactions yet.</p>`
              : txRows
          }
        </div>

        <!-- Swaps -->
        ${
          state.walletSwaps.length > 0
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
    ${renderWalletModal()}
  `;
}
