import { state } from "../state.ts";
import { escapeAttr, escapeHtml } from "../utils/html.ts";
import { renderMnemonicGrid } from "./wallet-modals.ts";

export function renderOnboarding(): string {
  const step = state.onboardingStep as "nostr" | "wallet";
  const loading = state.onboardingLoading;
  const errorHtml = state.onboardingError
    ? `<p class="text-sm text-red-400">${escapeHtml(state.onboardingError)}</p>`
    : "";

  const stepIndicator = `
    <div class="flex items-center gap-3 mb-6">
      <div class="flex items-center gap-2">
        <div class="h-8 w-8 rounded-full ${step === "nostr" || state.onboardingNostrDone ? "bg-emerald-400 text-slate-950" : "border border-slate-700 text-slate-500"} flex items-center justify-center text-sm font-medium">${state.onboardingNostrDone && step !== "nostr" ? '<svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="3"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>' : "1"}</div>
        <span class="text-sm ${step === "nostr" ? "text-slate-100" : "text-slate-500"}">Nostr Identity</span>
      </div>
      <div class="h-px flex-1 ${step === "wallet" ? "bg-emerald-400" : "bg-slate-700"}"></div>
      <div class="flex items-center gap-2">
        <div class="h-8 w-8 rounded-full ${step === "wallet" ? "bg-emerald-400 text-slate-950" : "border border-slate-700 text-slate-500"} flex items-center justify-center text-sm font-medium">2</div>
        <span class="text-sm ${step === "wallet" ? "text-slate-100" : "text-slate-500"}">Liquid Wallet</span>
      </div>
    </div>
  `;

  if (step === "nostr") {
    // After generation — show keys for backup
    if (state.onboardingNostrDone) {
      const nsecHtml = state.onboardingNostrGeneratedNsec
        ? `<div class="flex items-center gap-2">
            <div class="min-w-0 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 py-2">
              <div class="text-[10px] text-slate-500">nsec (secret)</div>
              ${
                state.onboardingNsecRevealed
                  ? `<div class="mono truncate text-xs text-rose-300">${escapeHtml(state.onboardingNostrGeneratedNsec)}</div>`
                  : `<div class="text-xs text-slate-500">Hidden</div>`
              }
            </div>
            ${
              state.onboardingNsecRevealed
                ? `<button data-action="onboarding-copy-nsec" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:bg-slate-800 transition">Copy</button>`
                : `<button data-action="onboarding-reveal-nsec" class="shrink-0 rounded-lg border border-amber-700/60 bg-amber-950/20 px-3 py-2 text-xs text-amber-300 hover:bg-amber-900/30 transition">Reveal</button>`
            }
          </div>`
        : `<div class="flex items-center gap-2">
            <div class="min-w-0 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 py-2">
              <div class="text-[10px] text-slate-500">nsec (secret)</div>
              <div class="text-xs text-slate-500">Copied</div>
            </div>
          </div>`;

      return `
        <div class="w-full max-w-lg rounded-2xl border border-slate-800 bg-slate-950 p-8">
          ${stepIndicator}
          <h2 class="text-xl font-medium text-slate-100">Nostr Identity Created</h2>
          <p class="mt-2 text-sm text-slate-400">Back up your secret key (nsec) now. You will need it to resolve markets you create.</p>
          <div class="mt-4 rounded-lg border border-amber-700/40 bg-amber-950/20 px-3 py-2">
            <p class="text-[11px] text-amber-300/90">Save your nsec in a secure location. If you lose it, you cannot resolve markets created with this identity.</p>
          </div>
          <div class="mt-4 space-y-2">
            <div class="flex items-center gap-2">
              <div class="min-w-0 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 py-2">
                <div class="text-[10px] text-slate-500">npub (public)</div>
                <div class="mono truncate text-xs text-slate-300">${escapeHtml(state.nostrNpub)}</div>
              </div>
              <button data-action="onboarding-copy-npub" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:bg-slate-800 transition">Copy</button>
            </div>
            ${nsecHtml}
          </div>
          <button data-action="onboarding-nostr-continue" class="mt-6 w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300">Continue to Wallet Setup</button>
        </div>
      `;
    }

    // Initial nostr step — choose generate or import
    const modeGenerate = state.onboardingNostrMode === "generate";
    return `
      <div class="w-full max-w-lg rounded-2xl border border-slate-800 bg-slate-950 p-8">
        ${stepIndicator}
        <h2 class="text-xl font-medium text-slate-100">Welcome to Deadcat Live!</h2>
        <p class="mt-2 text-sm text-slate-400">Set up your Nostr identity. This keypair is used to publish and resolve prediction markets.</p>
        ${errorHtml}
        <div class="mt-5 flex gap-2">
          <button data-action="onboarding-set-nostr-mode" data-mode="generate" class="flex-1 rounded-lg border px-4 py-2 text-sm transition ${modeGenerate ? "border-emerald-400 bg-emerald-400/10 text-emerald-300" : "border-slate-700 text-slate-400 hover:bg-slate-800"}">Generate new</button>
          <button data-action="onboarding-set-nostr-mode" data-mode="import" class="flex-1 rounded-lg border px-4 py-2 text-sm transition ${!modeGenerate ? "border-emerald-400 bg-emerald-400/10 text-emerald-300" : "border-slate-700 text-slate-400 hover:bg-slate-800"}">Import existing</button>
        </div>
        ${
          modeGenerate
            ? `
          <button data-action="onboarding-generate-nostr" class="mt-5 w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300" ${loading ? "disabled" : ""}>${loading ? "Generating..." : "Generate Keypair"}</button>
        `
            : `
          <div class="mt-5 space-y-3">
            <input id="onboarding-nostr-nsec" type="password" value="${escapeAttr(state.onboardingNostrNsec)}" placeholder="nsec1..." class="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 transition focus:ring-2 mono" />
            <button data-action="onboarding-import-nostr" class="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300" ${loading ? "disabled" : ""}>${loading ? "Importing..." : "Import & Continue"}</button>
          </div>
        `
        }
      </div>
    `;
  }

  // Step 2: Wallet
  if (
    state.onboardingWalletMnemonic &&
    state.onboardingWalletMode === "create"
  ) {
    // Show mnemonic backup after creation
    return `
      <div class="w-full max-w-lg rounded-2xl border border-slate-800 bg-slate-950 p-8">
        ${stepIndicator}
        <h2 class="text-xl font-medium text-slate-100">Wallet Created</h2>
        <p class="mt-2 text-sm text-slate-400">Back up your recovery phrase in a safe place.</p>
        <div class="mt-4 rounded-lg border border-slate-600 bg-slate-900/40 p-4 space-y-3">
          ${renderMnemonicGrid(state.onboardingWalletMnemonic)}
          <button data-action="onboarding-copy-mnemonic" class="w-full rounded-lg border border-slate-700 px-4 py-2 text-sm text-slate-300 hover:bg-slate-800">Copy to clipboard</button>
        </div>
        <button data-action="onboarding-wallet-done" class="mt-5 w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300">I've saved my recovery phrase</button>
      </div>
    `;
  }

  const wMode = state.onboardingWalletMode;
  const modeCreate = wMode === "create";
  const modeRestore = wMode === "restore";
  const modeNostrRestore = wMode === "nostr-restore";

  // Scanning indicator
  const scanningHtml = state.onboardingBackupScanning
    ? '<div class="mt-4 flex items-center gap-2 rounded-lg border border-slate-700 bg-slate-900/50 px-4 py-3"><div class="h-4 w-4 animate-spin rounded-full border-2 border-slate-600 border-t-emerald-400"></div><p class="text-sm text-slate-400">Scanning relays for existing wallet backup...</p></div>'
    : "";

  // Nostr backup found banner
  const backupFoundHtml =
    state.onboardingBackupFound && !state.onboardingBackupScanning
      ? '<button data-action="onboarding-set-wallet-mode" data-mode="nostr-restore" class="mt-4 w-full rounded-xl border ' +
        (modeNostrRestore
          ? "border-emerald-500/50 bg-emerald-500/10"
          : "border-emerald-700/40 bg-emerald-950/20 hover:border-emerald-600/50") +
        ' p-4 text-left transition">' +
        '<div class="flex items-center gap-3">' +
        '<svg class="h-6 w-6 text-emerald-400 shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5"><path stroke-linecap="round" stroke-linejoin="round" d="M12 16.5V9.75m0 0l3 3m-3-3l-3 3M6.75 19.5a4.5 4.5 0 01-1.41-8.775 5.25 5.25 0 0110.233-2.33 3 3 0 013.758 3.848A3.752 3.752 0 0118 19.5H6.75z"/></svg>' +
        "<div>" +
        '<p class="text-sm font-medium text-emerald-300">Wallet backup found on your relays</p>' +
        '<p class="mt-0.5 text-xs text-emerald-400/60">Restore your existing Liquid wallet from your encrypted Nostr backup</p>' +
        "</div>" +
        "</div></button>"
      : "";

  // Mode-specific form content
  let formHtml = "";
  const confirmBorder =
    state.onboardingWalletPasswordConfirm &&
    state.onboardingWalletPassword !== state.onboardingWalletPasswordConfirm
      ? "border-red-500/50"
      : "border-slate-700";
  if (modeNostrRestore) {
    formHtml = `
      <div class="mt-5 space-y-3">
        <p class="text-sm text-slate-400">Your encrypted wallet backup will be fetched from your Nostr relays and decrypted locally. Set a password to protect the wallet on this device.</p>
        <input id="onboarding-wallet-password" type="password" maxlength="32" value="${escapeAttr(state.onboardingWalletPassword)}" placeholder="Set a password" autocomplete="new-password" onpaste="return false" class="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 transition focus:ring-2 disabled:opacity-50" ${loading ? "disabled" : ""} />
        <input id="onboarding-wallet-password-confirm" type="password" maxlength="32" value="${escapeAttr(state.onboardingWalletPasswordConfirm)}" placeholder="Confirm password" autocomplete="new-password" onpaste="return false" class="h-11 w-full rounded-lg border ${confirmBorder} bg-slate-900 px-4 text-sm outline-none ring-emerald-400 transition focus:ring-2 disabled:opacity-50" ${loading ? "disabled" : ""} />
        <button data-action="onboarding-nostr-restore-wallet" class="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300 disabled:opacity-50 disabled:cursor-not-allowed" ${loading ? "disabled" : ""}>${loading ? "Restoring..." : "Restore from Nostr Backup"}</button>
      </div>`;
  } else if (modeCreate) {
    formHtml = `
      <div class="mt-5 space-y-3">
        <input id="onboarding-wallet-password" type="password" maxlength="32" value="${escapeAttr(state.onboardingWalletPassword)}" placeholder="Set a password" autocomplete="new-password" onpaste="return false" class="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 transition focus:ring-2 disabled:opacity-50" ${loading ? "disabled" : ""} />
        <input id="onboarding-wallet-password-confirm" type="password" maxlength="32" value="${escapeAttr(state.onboardingWalletPasswordConfirm)}" placeholder="Confirm password" autocomplete="new-password" onpaste="return false" class="h-11 w-full rounded-lg border ${confirmBorder} bg-slate-900 px-4 text-sm outline-none ring-emerald-400 transition focus:ring-2 disabled:opacity-50" ${loading ? "disabled" : ""} />
        <button data-action="onboarding-create-wallet" class="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300 disabled:opacity-50 disabled:cursor-not-allowed" ${loading ? "disabled" : ""}>${loading ? "Creating..." : "Create Wallet"}</button>
      </div>`;
  } else {
    formHtml = `
      <div class="mt-5 space-y-3">
        <textarea id="onboarding-wallet-mnemonic" placeholder="Enter your 12-word recovery phrase" rows="3" class="w-full rounded-lg border border-slate-700 bg-slate-900 px-4 py-3 text-sm outline-none ring-emerald-400 transition focus:ring-2 disabled:opacity-50" ${loading ? "disabled" : ""}>${escapeHtml(state.onboardingWalletMnemonic)}</textarea>
        <input id="onboarding-wallet-password" type="password" maxlength="32" value="${escapeAttr(state.onboardingWalletPassword)}" placeholder="Set a password" autocomplete="new-password" onpaste="return false" class="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 transition focus:ring-2 disabled:opacity-50" ${loading ? "disabled" : ""} />
        <input id="onboarding-wallet-password-confirm" type="password" maxlength="32" value="${escapeAttr(state.onboardingWalletPasswordConfirm)}" placeholder="Confirm password" autocomplete="new-password" onpaste="return false" class="h-11 w-full rounded-lg border ${confirmBorder} bg-slate-900 px-4 text-sm outline-none ring-emerald-400 transition focus:ring-2 disabled:opacity-50" ${loading ? "disabled" : ""} />
        <button data-action="onboarding-restore-wallet" class="w-full rounded-lg bg-emerald-400 px-4 py-3 font-medium text-slate-950 hover:bg-emerald-300 disabled:opacity-50 disabled:cursor-not-allowed" ${loading ? "disabled" : ""}>${loading ? "Restoring..." : "Restore & Finish"}</button>
      </div>`;
  }

  return `
    <div class="w-full max-w-lg rounded-2xl border border-slate-800 bg-slate-950 p-8">
      ${stepIndicator}
      <h2 class="text-xl font-medium text-slate-100">Set Up Your Wallet</h2>
      <p class="mt-2 text-sm text-slate-400">Create a new Liquid (L-BTC) wallet or restore from an existing recovery phrase.</p>
      ${errorHtml}
      ${scanningHtml}
      ${backupFoundHtml}
      <div class="mt-5 flex gap-2">
        <button data-action="${modeCreate || loading ? "" : "onboarding-set-wallet-mode"}" data-mode="create" class="flex-1 rounded-lg border px-4 py-2 text-sm transition ${modeCreate ? "border-emerald-400 bg-emerald-400/10 text-emerald-300" : "border-slate-700 text-slate-400 hover:bg-slate-800"} ${loading ? "opacity-50 cursor-not-allowed" : ""}" ${loading ? "disabled" : ""}>Create new</button>
        <button data-action="${modeRestore || loading ? "" : "onboarding-set-wallet-mode"}" data-mode="restore" class="flex-1 rounded-lg border px-4 py-2 text-sm transition ${modeRestore ? "border-emerald-400 bg-emerald-400/10 text-emerald-300" : "border-slate-700 text-slate-400 hover:bg-slate-800"} ${loading ? "opacity-50 cursor-not-allowed" : ""}" ${loading ? "disabled" : ""}>Restore from seed</button>
      </div>
      ${formHtml}
    </div>
  `;
}
