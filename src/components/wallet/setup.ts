import { state } from "../../state.ts";
import { renderMnemonicGrid } from "../wallet-modals.ts";

export function renderWalletSetup(params: {
  errorHtml: string;
  loading: boolean;
  networkBadge: string;
}): string {
  const { errorHtml, loading, networkBadge } = params;

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
            <input id="wallet-password" type="password" maxlength="32" value="${state.walletPassword}" placeholder="Enter a password" autocomplete="new-password" onpaste="return false" class="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 focus:ring-2 disabled:opacity-50" ${loading ? "disabled" : ""} />
            <input id="wallet-password-confirm" type="password" maxlength="32" value="${state.walletPasswordConfirm}" placeholder="Confirm password" autocomplete="new-password" onpaste="return false" class="h-11 w-full rounded-lg border ${state.walletPasswordConfirm && state.walletPassword !== state.walletPasswordConfirm ? "border-red-500/50" : "border-slate-700"} bg-slate-900 px-4 text-sm outline-none ring-emerald-400 focus:ring-2 disabled:opacity-50" ${loading ? "disabled" : ""} />
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
            <input id="wallet-password" type="password" maxlength="32" value="${state.walletPassword}" placeholder="Enter a password" autocomplete="new-password" onpaste="return false" class="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 focus:ring-2 disabled:opacity-50" ${loading ? "disabled" : ""} />
            <input id="wallet-password-confirm" type="password" maxlength="32" value="${state.walletPasswordConfirm}" placeholder="Confirm password" autocomplete="new-password" onpaste="return false" class="h-11 w-full rounded-lg border ${state.walletPasswordConfirm && state.walletPassword !== state.walletPasswordConfirm ? "border-red-500/50" : "border-slate-700"} bg-slate-900 px-4 text-sm outline-none ring-emerald-400 focus:ring-2 disabled:opacity-50" ${loading ? "disabled" : ""} />
            <button data-action="restore-wallet" class="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300 transition disabled:opacity-50 disabled:cursor-not-allowed" ${loading ? "disabled" : ""}>${loading ? "Restoring..." : "Restore Wallet"}</button>
          </div>
        `
        }
      </div>
    </div>
  `;
}
