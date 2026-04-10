import { state } from "../../state.ts";
import type { RelayBackupResult } from "../../types.ts";
import { escapeAttr, escapeHtml } from "../../utils/html.ts";

const SHIELD_ICON =
  '<svg class="h-4 w-4 shrink-0 text-emerald-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"/></svg>';

const SECURITY_INFO =
  '<details class="group">' +
  '<summary class="cursor-pointer text-[11px] text-slate-500 hover:text-slate-400 transition select-none">Why is this secure?</summary>' +
  '<div class="mt-2 space-y-1.5 text-[11px] text-slate-500">' +
  '<p><strong class="text-slate-400">NIP-44 encryption</strong> — Recovery phrase is encrypted using XChaCha20 + secp256k1 ECDH. Only your nsec can decrypt it.</p>' +
  '<p><strong class="text-slate-400">Self-encrypted</strong> — Encrypted to your own public key. Relay operators see only ciphertext.</p>' +
  '<p><strong class="text-slate-400">NIP-78 storage</strong> — Published as a kind 30078 addressable event, retrievable from any relay that has it.</p>' +
  '<p><strong class="text-slate-400">Relay redundancy</strong> — Sent to all your configured relays for resilience.</p>' +
  "</div>" +
  "</details>";

/**
 * Renders the Nostr Relay Backup section.
 *
 * @param action - The data-action prefix for buttons (e.g. "settings" or "nostr").
 *   Buttons will emit `${action}-backup-wallet`, `delete-nostr-backup`, and `cancel-backup-prompt`.
 * @param showPasswordPrompt - Whether to show the password input for re-upload when wallet is locked.
 */
export function renderNostrBackupSection(options: {
  action: string;
  showPasswordPrompt: boolean;
}): string {
  const { action, showPasswordPrompt } = options;
  const backupStatus = state.nostrBackupStatus;
  const loading = state.nostrBackupLoading;

  if (!state.nostrNpub) return "";

  const hasBackup = backupStatus?.has_backup ?? false;

  let statusHtml: string;
  if (hasBackup) {
    const relayResults = backupStatus?.relay_results;
    const backedUpCount = relayResults.filter(
      (r: RelayBackupResult) => r.has_backup,
    ).length;
    const relayListHtml = relayResults
      .map(
        (r: RelayBackupResult) =>
          `<div class="flex items-center gap-2 text-xs">
            <span class="h-1.5 w-1.5 rounded-full ${r.has_backup ? "bg-emerald-400" : "bg-slate-600"}"></span>
            <span class="mono text-slate-400">${escapeHtml(r.url)}</span>
          </div>`,
      )
      .join("");

    const actionsHtml = showPasswordPrompt
      ? `<div class="space-y-2">
          <input id="settings-backup-password" type="password" maxlength="32" value="${escapeAttr(state.nostrBackupPassword)}" placeholder="Wallet password" class="h-9 w-full rounded-lg border border-slate-700 bg-slate-900 px-3 text-xs outline-none ring-emerald-400 transition focus:ring-2" />
          <div class="flex gap-2">
            <button data-action="${action}-backup-wallet" class="flex-1 rounded-lg bg-emerald-400 px-4 py-2 text-xs font-medium text-slate-950 hover:bg-emerald-300 transition" ${loading ? "disabled" : ""}>${loading ? "Uploading..." : "Upload"}</button>
            <button data-action="cancel-backup-prompt" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-400 hover:bg-slate-800 transition">Cancel</button>
          </div>
        </div>`
      : `<div class="flex gap-2">
          <button data-action="${action}-backup-wallet" class="flex-1 rounded-lg border border-slate-700 px-4 py-2 text-xs text-slate-300 hover:bg-slate-800 transition" ${loading ? "disabled" : ""}>${loading ? "Uploading..." : "Re-upload to Relays"}</button>
          <button data-action="delete-nostr-backup" class="shrink-0 rounded-lg border border-rose-700/40 px-3 py-2 text-xs text-rose-400 hover:bg-rose-900/20 transition" ${loading ? "disabled" : ""}>${loading ? "Deleting..." : "Delete"}</button>
        </div>`;

    statusHtml =
      `<div class="flex items-center gap-2">${SHIELD_ICON}<p class="text-xs text-emerald-400">Encrypted backup on ${backedUpCount} of ${relayResults.length} relays</p></div>` +
      `<div class="space-y-1">${relayListHtml}</div>` +
      actionsHtml;
  } else {
    const actionsHtml = showPasswordPrompt
      ? `<div class="space-y-2">
          <input id="settings-backup-password" type="password" maxlength="32" value="${escapeAttr(state.nostrBackupPassword)}" placeholder="Wallet password" class="h-9 w-full rounded-lg border border-slate-700 bg-slate-900 px-3 text-xs outline-none ring-emerald-400 transition focus:ring-2" />
          <div class="flex gap-2">
            <button data-action="${action}-backup-wallet" class="flex-1 rounded-lg bg-emerald-400 px-4 py-2 text-xs font-medium text-slate-950 hover:bg-emerald-300 transition" ${loading ? "disabled" : ""}>${loading ? "Encrypting..." : "Upload"}</button>
            <button data-action="cancel-backup-prompt" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-400 hover:bg-slate-800 transition">Cancel</button>
          </div>
        </div>`
      : `<p class="text-xs text-slate-400">Encrypt your recovery phrase with NIP-44 and store it on your Nostr relays. Only your nsec can decrypt it.</p>
        <button data-action="${action}-backup-wallet" class="w-full rounded-lg bg-emerald-400 px-4 py-2 text-xs font-medium text-slate-950 hover:bg-emerald-300 transition" ${loading ? "disabled" : ""}>${loading ? "Encrypting..." : "Encrypt & Upload to Relays"}</button>`;

    statusHtml = actionsHtml;
  }

  return (
    '<div class="rounded-lg border border-slate-700 bg-slate-900/50 p-3 space-y-2">' +
    '<p class="text-[11px] font-medium uppercase tracking-wider text-slate-500">Nostr Relay Backup</p>' +
    statusHtml +
    SECURITY_INFO +
    "</div>"
  );
}
