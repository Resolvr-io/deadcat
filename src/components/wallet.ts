import { state } from "../state.ts";
import { escapeHtml } from "../utils/html.ts";

const PAW_ICON = `<svg class="inline-block h-[1em] w-[1em] align-text-bottom" viewBox="0 0 90 79" fill="currentColor"><path d="M26.62,28.27c4.09,2.84,9.4,2.58,12.27-.69,2.3-2.63,3.06-5.82,3.08-10-.35-5.03-1.89-10.34-6.28-14.44C29.51-2.63,21.1-.1,19.06,8.08c-1.74,6.91,1.71,16.11,7.56,20.18Z"/><path d="M22.98,41.99c.21-1.73.04-3.62-.43-5.3-1.46-5.21-4-9.77-9.08-12.33C7.34,21.27-.31,24.39,0,32.36c-.03,7.11,5.17,14.41,11.8,16.58,5.57,1.82,10.49-1.16,11.17-6.95Z"/><path d="M63.4,28.27c5.85-4.06,9.3-13.26,7.57-20.19C68.92-.12,60.51-2.64,54.33,3.13c-4.4,4.1-5.93,9.41-6.28,14.44.02,4.18.78,7.37,3.08,10,2.87,3.28,8.17,3.54,12.27.7Z"/><path d="M76.54,24.36c-5.08,2.56-7.62,7.12-9.08,12.33-.47,1.68-.63,3.57-.43,5.3.69,5.79,5.61,8.77,11.16,6.96,6.63-2.17,11.83-9.47,11.8-16.58.32-7.99-7.32-11.1-13.45-8.01Z"/><path d="M65.95,49.84c-2.36-2.86-4.3-6.01-6.45-9.02-.89-1.24-1.8-2.47-2.78-3.65-2.76-3.35-7.24-5.02-11.72-5.02s-8.96,1.68-11.72,5.02c-.98,1.19-1.89,2.41-2.78,3.65-2.15,3.01-4.08,6.15-6.45,9.02-1.77,2.15-4.25,3.82-6.11,5.92-4.14,4.69-4.72,9.96-1.94,15.3,2.79,5.37,8.01,7.6,14.41,7.9,4.82.23,9.23-1.95,13.98-2.16.22-.01.42-.01.62-.01s.4,0,.61.01c4.75.21,9.16,2.38,13.98,2.16,6.39-.3,11.62-2.53,14.41-7.9,2.77-5.34,2.2-10.61-1.94-15.3-1.87-2.1-4.35-3.77-6.12-5.92Z"/></svg>`;

import { renderWalletLocked } from "./wallet/locked.ts";
import { renderWalletSetup } from "./wallet/setup.ts";
import { renderWalletUnlocked } from "./wallet/unlocked.ts";
import {
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
      ? `<span class="rounded-full bg-amber-500/20 px-2.5 py-0.5 text-xs font-medium text-amber-300">${escapeHtml(state.walletNetwork)}</span>`
      : "";

  const errorHtml = error
    ? `<div class="rounded-lg border border-red-500/30 bg-red-900/20 px-4 py-3 text-sm text-red-300">${escapeHtml(error)}</div>`
    : "";

  const loadingHtml = "";

  if (state.walletStatus === "not_created") {
    return renderWalletSetup({ errorHtml, loading, networkBadge });
  }

  if (state.walletStatus === "locked") {
    return renderWalletLocked({ errorHtml, loading, networkBadge });
  }

  return renderWalletUnlocked({
    errorHtml,
    loading,
    loadingHtml,
    networkBadge,
    modalHtml: renderWalletModal(),
    pawIcon: PAW_ICON,
  });
}
