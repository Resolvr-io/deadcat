import { state } from "../state.ts";
import { escapeAttr, escapeHtml } from "../utils/html.ts";
import { renderMnemonicGrid } from "./wallet-modals.ts";

const backBtn = `
  <button data-action="onboarding-back" class="mb-8 flex items-center gap-1.5 text-xs text-slate-500 hover:text-slate-300 transition">
    <svg class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M15 19l-7-7 7-7"/></svg>
    Back
  </button>`;

function renderPasswordFields(loading: boolean): string {
  const revealed = state.onboardingPasswordRevealed;
  const inputType = revealed ? "text" : "password";
  const disabled = loading ? "disabled" : "";

  const eyeIcon = revealed
    ? `<svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l3.59 3.59m0 0A9.953 9.953 0 0112 5c4.478 0 8.268 2.943 9.543 7a10.025 10.025 0 01-4.132 5.411m0 0L21 21"/></svg>`
    : `<svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/><path stroke-linecap="round" stroke-linejoin="round" d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z"/></svg>`;

  return `
    <div class="space-y-4">
      <div class="space-y-1.5">
        <label class="text-xs font-medium text-slate-400 uppercase tracking-wide">Password</label>
        <div class="relative">
          <input id="onboarding-wallet-password" type="${inputType}" maxlength="32" placeholder="At least 8 characters" autocomplete="new-password" onpaste="return false" class="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 pr-11 pl-4 text-sm outline-none ring-emerald-400 transition focus:ring-2 disabled:opacity-50" ${disabled} />
          <button type="button" data-action="onboarding-toggle-password-reveal" class="absolute right-3 top-1/2 -translate-y-1/2 text-slate-500 hover:text-slate-300 transition" tabindex="-1" ${disabled}>${eyeIcon}</button>
        </div>
      </div>
      <div class="space-y-1.5">
        <label class="text-xs font-medium text-slate-400 uppercase tracking-wide">Confirm password</label>
        <input id="onboarding-wallet-password-confirm" type="${inputType}" maxlength="32" placeholder="Repeat your password" autocomplete="new-password" onpaste="return false" class="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 transition focus:ring-2 disabled:opacity-50" ${disabled} />
      </div>
    </div>`;
}

export function renderOnboarding(): string {
  const step = state.onboardingStep as "nostr" | "wallet";
  const loading = state.onboardingLoading;
  const errorHtml = state.onboardingError
    ? `<div class="rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-3"><p class="text-sm text-red-400">${escapeHtml(state.onboardingError)}</p></div>`
    : "";

  const walletOnly = state.onboardingWalletOnly;
  const stepIndicator = walletOnly ? "" : `
    <div class="flex items-center gap-3 mb-10">
      <div class="flex items-center gap-2.5">
        <div class="h-7 w-7 rounded-full ${step === "nostr" || state.onboardingNostrDone ? "bg-emerald-400 text-slate-950" : "border border-slate-700 text-slate-500"} flex items-center justify-center text-xs font-semibold shrink-0">
          ${state.onboardingNostrDone && step !== "nostr" ? '<svg class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="3"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>' : "1"}
        </div>
        <span class="text-xs font-medium ${step === "nostr" ? "text-slate-200" : "text-slate-500"}">Nostr identity</span>
      </div>
      <div class="h-px flex-1 ${step === "wallet" ? "bg-emerald-400" : "bg-slate-700"}"></div>
      <div class="flex items-center gap-2.5">
        <div class="h-7 w-7 rounded-full ${step === "wallet" ? "bg-emerald-400 text-slate-950" : "border border-slate-700 text-slate-500"} flex items-center justify-center text-xs font-semibold shrink-0">2</div>
        <span class="text-xs font-medium ${step === "wallet" ? "text-slate-200" : "text-slate-500"}">Wallet</span>
      </div>
    </div>
  `;

  if (step === "nostr") {

    // Nostr backup screen
    if (state.onboardingNostrDone) {
      const isImport = state.onboardingNostrMode === "import";

      const eyebrow = isImport ? "Identity imported" : "Identity created";
      const title = isImport ? "Confirm your identity" : "Back up your secret key";
      const description = isImport
        ? "Your Nostr identity has been imported. Confirm your details below before continuing."
        : "Your nsec is the only way to prove ownership of markets you create. Store it somewhere safe — it cannot be recovered if lost.";

      const identityRows = isImport ? (() => {
        const profile = state.nostrProfile;
        const displayName = profile?.display_name || profile?.name || null;
        const npub = state.onboardingPendingNpub || state.nostrNpub || "";
        const truncatedNpub = npub.length > 20 ? `${npub.slice(0, 10)}...${npub.slice(-8)}` : npub;
        const avatarHtml = profile?.picture && !state.profilePicError
          ? `<img src="${escapeAttr(profile.picture)}" class="h-10 w-10 rounded-full object-cover shrink-0" onerror="this.style.display='none'" />`
          : `<div class="h-10 w-10 rounded-full bg-slate-800 flex items-center justify-center shrink-0"><svg class="h-5 w-5 text-slate-500" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5"><path stroke-linecap="round" stroke-linejoin="round" d="M15.75 6a3.75 3.75 0 11-7.5 0 3.75 3.75 0 017.5 0zM4.501 20.118a7.5 7.5 0 0114.998 0A17.933 17.933 0 0112 21.75c-2.676 0-5.216-.584-7.499-1.632z"/></svg></div>`;
        return `
          <div class="py-4 flex items-center gap-3">
            ${avatarHtml}
            <div class="min-w-0">
              <p class="text-sm font-semibold text-white truncate">${escapeHtml(displayName ?? "No display name")}</p>
              <p class="mt-0.5 text-[11px] text-slate-600 italic mono truncate">${escapeHtml(truncatedNpub)}</p>
            </div>
          </div>`;
      })() : `
        <div class="py-4 flex items-center justify-between gap-3">
          <div class="min-w-0">
            <p class="text-[10px] font-medium uppercase tracking-wide text-slate-500 mb-1">npub — public key</p>
            <p class="mono truncate text-xs text-slate-600">${escapeHtml(state.onboardingPendingNpub || state.nostrNpub || "")}</p>
          </div>
          <button data-action="onboarding-copy-npub" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:bg-slate-800 transition">Copy</button>
        </div>
        <div class="py-4 flex items-center justify-between gap-3">
          <div class="min-w-0">
            <p class="text-[10px] font-medium uppercase tracking-wide text-slate-500 mb-1">nsec — secret key</p>
            ${state.onboardingNsecRevealed
              ? `<p class="mono truncate text-xs text-rose-300">${escapeHtml(state.onboardingNostrGeneratedNsec)}</p>`
              : `<p class="text-xs text-slate-500 italic">Hidden for your protection</p>`
            }
          </div>
          ${state.onboardingNsecRevealed
            ? `<button data-action="onboarding-copy-nsec" class="shrink-0 rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:bg-slate-800 transition">Copy</button>`
            : `<button data-action="onboarding-reveal-nsec" class="shrink-0 rounded-lg border border-amber-700/60 bg-amber-950/20 px-3 py-2 text-xs text-amber-300 hover:bg-amber-900/30 transition">Show</button>`
          }
        </div>`;

      return `
        <div class="w-full max-w-[432px] rounded-2xl border border-slate-800 bg-slate-950 p-10">
          ${stepIndicator}
          ${backBtn}
          <p class="text-xs font-semibold uppercase tracking-widest text-emerald-400 mb-2">${eyebrow}</p>
          <h2 class="text-2xl font-semibold text-white">${title}</h2>
          <p class="mt-3 text-sm text-slate-400 leading-relaxed">${description}</p>
          ${!isImport ? `<div class="mt-5 rounded-lg border border-amber-700/30 bg-amber-950/20 px-4 py-3">
            <p class="text-xs text-amber-300/90 leading-relaxed">Never share your nsec with anyone. Anyone who has it can act as you on Nostr.</p>
          </div>` : ""}
          <div class="mt-4 border-t border-slate-800 divide-y divide-slate-800">
            ${identityRows}
          </div>
          ${!isImport ? `<label class="mt-4 flex items-start gap-3 cursor-pointer select-none">
            <span class="mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded ${state.onboardingNsecAcknowledged ? "bg-emerald-400" : "border border-slate-600 bg-slate-800"}">
              ${state.onboardingNsecAcknowledged ? `<svg class="h-3 w-3 text-slate-950" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="2,6 5,9 10,3"/></svg>` : ""}
            </span>
            <input type="checkbox" id="onboarding-nsec-ack" ${state.onboardingNsecAcknowledged ? "checked" : ""} class="sr-only" />
            <span class="text-sm text-slate-300 leading-relaxed">I have saved my secret key in a safe place</span>
          </label>` : ""}
          <button data-action="onboarding-nostr-continue" class="mt-6 w-full rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 hover:bg-emerald-300 disabled:opacity-40 disabled:cursor-not-allowed transition" ${!isImport && !state.onboardingNsecAcknowledged ? "disabled" : ""}>Continue to wallet setup</button>
        </div>
      `;
    }

    // Import sub-page
    if (state.onboardingNostrMode === "import") {
      return `
        <div class="w-full max-w-[432px] rounded-2xl border border-slate-800 bg-slate-950 p-10">
          ${stepIndicator}
          ${backBtn}
          <p class="text-xs font-semibold uppercase tracking-widest text-slate-500 mb-2">Step 1 of 2</p>
          <h2 class="text-2xl font-semibold text-white">Import Nostr identity</h2>
          <p class="mt-3 text-sm text-slate-400 leading-relaxed">Paste your existing secret key (nsec) to restore your identity and access markets you've previously created.</p>
          ${errorHtml ? `<div class="mt-5">${errorHtml}</div>` : ""}
          <div class="mt-8 space-y-4">
            <div class="space-y-1.5">
              <label class="text-xs font-medium text-slate-400 uppercase tracking-wide">Secret Key</label>
              <input id="onboarding-nostr-nsec" type="text" placeholder="nsec1..." autocomplete="off" spellcheck="false" class="h-11 w-full rounded-lg border border-slate-700 bg-slate-900 px-4 text-sm outline-none ring-emerald-400 transition focus:ring-2 mono" />
            </div>
            <button data-action="onboarding-import-nostr" class="w-full rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 hover:bg-emerald-300 disabled:opacity-50 disabled:cursor-not-allowed transition" ${loading ? "disabled" : ""}>${loading ? "Importing..." : "Import & continue"}</button>
          </div>
        </div>
      `;
    }

    // Main nostr step
    return `
      <div class="w-full max-w-[432px] rounded-2xl border border-slate-800 bg-slate-950 p-10">
        ${stepIndicator}
        <p class="text-xs font-semibold uppercase tracking-widest text-slate-500 mb-2">Step 1 of 2</p>
        <h2 class="text-2xl font-semibold text-white">Set up your identity</h2>
        <p class="mt-3 text-sm text-slate-400 leading-relaxed">deadcat uses Nostr keypairs to publish markets and sign attestations. Generate a new identity or bring your own.</p>
        ${errorHtml ? `<div class="mt-5">${errorHtml}</div>` : ""}
        <div class="mt-10 space-y-3">
          <button data-action="onboarding-generate-nostr" class="w-full rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 hover:bg-emerald-300 disabled:opacity-50 transition" ${loading ? "disabled" : ""}>${loading ? "Generating..." : "Generate new identity"}</button>
          <button data-action="onboarding-set-nostr-mode" data-mode="import" class="w-full rounded-lg border border-slate-700 px-4 py-3.5 text-sm font-medium text-slate-300 hover:bg-slate-800 hover:border-slate-600 transition">Import existing identity</button>
        </div>
      </div>
    `;
  }

  // Password page — must be checked before mnemonic display since walletMnemonic is still set
  if (state.onboardingWalletPasswordStep) {
    const wMode2 = state.onboardingWalletMode;
    const submitAction = wMode2 === "create"
      ? "onboarding-create-wallet"
      : wMode2 === "restore"
        ? "onboarding-restore-wallet"
        : "onboarding-nostr-restore-wallet";
    const submitLabel = loading
      ? (wMode2 === "create" ? "Creating..." : "Restoring...")
      : "Create password";
    return `
      <div class="w-full max-w-[432px] rounded-2xl border border-slate-800 bg-slate-950 p-10">
        ${stepIndicator}
        ${backBtn}
        <p class="text-xs font-semibold uppercase tracking-widest text-slate-500 mb-2">Protect your wallet</p>
        <h2 class="text-2xl font-semibold text-white">Set a password</h2>
        <p class="mt-3 text-sm text-slate-400 leading-relaxed">Your wallet will be encrypted with this password on this device. You'll need it every time you open deadcat to unlock your wallet.</p>
        ${errorHtml ? `<div class="mt-5">${errorHtml}</div>` : ""}
        <div class="mt-8 space-y-6">
          ${renderPasswordFields(loading)}
          <button data-action="${submitAction}" class="w-full rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 hover:bg-emerald-300 disabled:opacity-50 disabled:cursor-not-allowed transition" ${loading ? "disabled" : ""}>${submitLabel}</button>
        </div>
      </div>
    `;
  }

  // Mnemonic verify page
  if (
    state.onboardingWalletMnemonic &&
    state.onboardingWalletMode === "create" &&
    state.onboardingMnemonicVerifyStep
  ) {
    const verifyIndices = state.onboardingMnemonicVerifyIndices;
    const verifyFieldsHtml = verifyIndices.map((wordIdx, i) => {
      return `
        <div class="flex items-center gap-4">
          <span class="w-16 shrink-0 text-right text-xs font-medium text-slate-500">Word ${wordIdx + 1}</span>
          <input id="onboarding-verify-word-${i}" type="text" placeholder="type word ${wordIdx + 1}" autocomplete="off" spellcheck="false" class="h-10 flex-1 rounded-lg border border-slate-700 bg-slate-900 px-3 text-sm outline-none ring-emerald-400 transition focus:ring-2 mono" />
        </div>`;
    }).join("");
    return `
      <div class="w-full max-w-[432px] rounded-2xl border border-slate-800 bg-slate-950 p-10">
        ${stepIndicator}
        ${backBtn}
        <p class="text-xs font-semibold uppercase tracking-widest text-slate-500 mb-2">Verify backup</p>
        <h2 class="text-2xl font-semibold text-white">Confirm your recovery phrase</h2>
        <p class="mt-3 text-sm text-slate-400 leading-relaxed">Enter the 3 words below to confirm you've written them down correctly.</p>
        <div class="mt-8 space-y-4">
          ${verifyFieldsHtml}
        </div>
        ${errorHtml ? `<div class="mt-5">${errorHtml}</div>` : ""}
        <button data-action="onboarding-verify-mnemonic" class="mt-8 w-full rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 hover:bg-emerald-300 transition">Confirm</button>
      </div>
    `;
  }

  // Mnemonic display page
  if (
    state.onboardingWalletMnemonic &&
    state.onboardingWalletMode === "create"
  ) {
    return `
      <div class="w-full max-w-[432px] rounded-2xl border border-slate-800 bg-slate-950 p-10">
        ${stepIndicator}
        ${backBtn}
        <p class="text-xs font-semibold uppercase tracking-widest text-emerald-400 mb-2">Wallet created</p>
        <h2 class="text-2xl font-semibold text-white">Save your recovery phrase</h2>
        <p class="mt-3 text-sm text-slate-400 leading-relaxed">Write these 12 words down in order and store them somewhere safe. This is the only way to recover your wallet.</p>
        <div class="mt-6 rounded-xl border border-slate-700 bg-slate-900/60 p-5 space-y-4">
          ${renderMnemonicGrid(state.onboardingWalletMnemonic)}
          <button data-action="onboarding-copy-mnemonic" class="w-full rounded-lg border border-slate-700 px-4 py-2.5 text-sm text-slate-300 hover:bg-slate-800 transition">Copy to clipboard</button>
        </div>
        <button data-action="onboarding-wallet-done" class="mt-8 w-full rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 hover:bg-emerald-300 transition">I've saved my recovery phrase</button>
      </div>
    `;
  }

  const wMode = state.onboardingWalletMode;
  const modeRestore = wMode === "restore";
  const modeNostrRestore = wMode === "nostr-restore";

  // Restore from seed sub-page
  if (modeRestore) {
    return `
      <div class="w-full max-w-[432px] rounded-2xl border border-slate-800 bg-slate-950 p-10">
        ${stepIndicator}
        ${backBtn}
        ${walletOnly ? "" : `<p class="text-xs font-semibold uppercase tracking-widest text-slate-500 mb-2">Step 2 of 2</p>`}
        <h2 class="text-2xl font-semibold text-white">Restore from seed</h2>
        <p class="mt-3 text-sm text-slate-400 leading-relaxed">Enter your 12-word recovery phrase to restore your existing wallet.</p>
        ${errorHtml ? `<div class="mt-5">${errorHtml}</div>` : ""}
        <div class="mt-8 space-y-4">
          <div class="space-y-1.5">
            <label class="text-xs font-medium text-slate-400 uppercase tracking-wide">Recovery Phrase</label>
            <textarea id="onboarding-wallet-mnemonic" placeholder="word1 word2 word3 ... word12" rows="3" class="w-full rounded-lg border border-slate-700 bg-slate-900 px-4 py-3 text-sm outline-none ring-emerald-400 transition focus:ring-2 resize-none mono"></textarea>
          </div>
          <button data-action="onboarding-wallet-continue" class="w-full rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 hover:bg-emerald-300 transition">Restore wallet</button>
        </div>
      </div>
    `;
  }

  // Nostr restore sub-page
  if (modeNostrRestore) {
    const wallets = state.nostrBackupStatus?.wallets ?? [];
    const selectedDTag = state.onboardingSelectedWalletDTag || (wallets[0]?.d_tag ?? "");
    const backupRelays = state.nostrBackupStatus?.relay_results?.filter(r => r.has_backup) ?? [];
    const fallbackRelay = state.relays[0]?.url ?? state.nostrBackupStatus?.relay_results?.[0]?.url ?? "";
    const primaryRelayUrl = backupRelays.length > 0 ? backupRelays[0].url : fallbackRelay;
    const primaryRelay = primaryRelayUrl.replace(/^wss?:\/\//, "").replace(/\/$/, "");
    const othersCount = backupRelays.length > 1 ? backupRelays.length - 1 : 0;

    const walletCardsHtml = (wallets.length > 0 ? wallets : [{ name: "My Wallet", d_tag: "" }]).map(w => {
      const isSelected = w.d_tag === selectedDTag || (wallets.length === 1);
      return `<button data-action="select-backup-wallet" data-dtag="${escapeAttr(w.d_tag)}"
        class="w-full flex items-center justify-between rounded-xl border ${isSelected ? "border-emerald-600/50 bg-emerald-950/20" : "border-slate-700 bg-slate-900/40 hover:border-slate-600"} px-4 py-3.5 transition text-left">
        <div class="flex items-center gap-3 min-w-0">
          <svg class="h-4 w-4 shrink-0 ${isSelected ? "text-emerald-400" : "text-slate-500"}" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5"><path stroke-linecap="round" stroke-linejoin="round" d="M21 12a2.25 2.25 0 00-2.25-2.25H5.25A2.25 2.25 0 003 12m18 0v6a2.25 2.25 0 01-2.25 2.25H5.25A2.25 2.25 0 013 18v-6m18 0V9M3 12V9m18-3a2.25 2.25 0 00-2.25-2.25H5.25A2.25 2.25 0 003 9m18 0V9a2.25 2.25 0 00-2.25-2.25H5.25A2.25 2.25 0 003 9"/></svg>
          <div class="min-w-0">
            <p class="text-sm font-medium ${isSelected ? "text-emerald-300" : "text-slate-300"} truncate">${escapeHtml(w.name)}</p>
            <p class="text-xs text-slate-500 mono truncate">${escapeHtml(primaryRelay)}${othersCount > 0 ? ` <span class="not-mono">and ${othersCount} other${othersCount > 1 ? "s" : ""}</span>` : ""}</p>
          </div>
        </div>
        ${isSelected ? `<svg class="h-4 w-4 text-emerald-400 shrink-0 ml-2" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5"><path stroke-linecap="round" stroke-linejoin="round" d="M5 13l4 4L19 7"/></svg>` : ""}
      </button>`;
    }).join("");

    return `
      <div class="w-full max-w-[432px] rounded-2xl border border-slate-800 bg-slate-950 p-10">
        ${stepIndicator}
        ${walletOnly ? "" : backBtn}
        <p class="text-xs font-semibold uppercase tracking-widest text-emerald-400 mb-2">Backup found</p>
        <h2 class="text-2xl font-semibold text-white">Restore from Nostr backup</h2>
        <p class="mt-3 text-sm text-slate-400 leading-relaxed">Select a wallet to restore.</p>
        <div class="mt-6 space-y-2">
          ${walletCardsHtml}
        </div>
        <div class="mt-6 space-y-3">
          <button data-action="onboarding-wallet-continue" class="w-full rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 hover:bg-emerald-300 transition">Restore wallet</button>
          ${walletOnly ? `<button data-action="onboarding-set-wallet-mode" data-mode="create" class="w-full rounded-lg border border-slate-700 px-4 py-3.5 text-sm font-medium text-slate-300 hover:bg-slate-800 hover:border-slate-600 transition">Set up a new wallet</button>` : ""}
        </div>
      </div>
    `;
  }

  // Scanning state — full card while relay scan is in progress
  if (state.onboardingBackupScanning) {
    return `
      <div class="w-full max-w-[432px] rounded-2xl border border-slate-800 bg-slate-950 p-10">
        ${stepIndicator}
        <div class="flex flex-col items-center justify-center py-8 text-center">
          <div class="h-10 w-10 animate-spin rounded-full border-2 border-slate-700 border-t-emerald-400 mb-6"></div>
          <p class="text-xs font-semibold uppercase tracking-widest text-slate-500 mb-2">Please wait</p>
          <h2 class="text-2xl font-semibold text-white">Checking for backups</h2>
          <p class="mt-3 text-sm text-slate-400 leading-relaxed max-w-xs">Scanning relays for an existing encrypted wallet backup.</p>
        </div>
      </div>
    `;
  }

  // Main wallet setup page — two action buttons, no tabs
  const backupFoundWallets = state.nostrBackupStatus?.wallets ?? [];
  const backupFoundHtml = state.onboardingBackupFound
    ? `<button data-action="onboarding-set-wallet-mode" data-mode="nostr-restore" class="w-full rounded-xl border border-emerald-700/40 bg-emerald-950/20 hover:border-emerald-600/50 p-4 text-left transition">
        <div class="flex items-start gap-3">
          <svg class="h-5 w-5 text-emerald-400 shrink-0 mt-0.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="1.5"><path stroke-linecap="round" stroke-linejoin="round" d="M12 16.5V9.75m0 0l3 3m-3-3l-3 3M6.75 19.5a4.5 4.5 0 01-1.41-8.775 5.25 5.25 0 0110.233-2.33 3 3 0 013.758 3.848A3.752 3.752 0 0118 19.5H6.75z"/></svg>
          <div>
            <p class="text-sm font-medium text-emerald-300">
              ${backupFoundWallets.length === 1
                ? escapeHtml(backupFoundWallets[0].name)
                : `${backupFoundWallets.length} wallet backup${backupFoundWallets.length !== 1 ? "s" : ""} found`}
            </p>
            <p class="mt-1 text-xs text-emerald-400/60 leading-relaxed">Restore your existing wallet from an encrypted Nostr backup</p>
          </div>
        </div>
      </button>`
    : "";

  return `
    <div class="w-full max-w-[432px] rounded-2xl border border-slate-800 bg-slate-950 p-10">
      ${stepIndicator}
      ${walletOnly ? "" : backBtn}
      ${walletOnly ? "" : `<p class="text-xs font-semibold uppercase tracking-widest text-slate-500 mb-2">Step 2 of 2</p>`}
      <h2 class="text-2xl font-semibold text-white">Set up your wallet</h2>
      <p class="mt-3 text-sm text-slate-400 leading-relaxed">Create a new Liquid wallet or restore an existing one.</p>
      ${errorHtml ? `<div class="mt-5">${errorHtml}</div>` : ""}
      ${backupFoundHtml ? `<div class="mt-5">${backupFoundHtml}</div>` : ""}
      <div class="mt-10 space-y-3">
        <button data-action="onboarding-wallet-continue" class="w-full rounded-lg bg-emerald-400 px-4 py-3.5 font-semibold text-slate-950 hover:bg-emerald-300 transition">Create new wallet</button>
        <button data-action="onboarding-set-wallet-mode" data-mode="restore" class="w-full rounded-lg border border-slate-700 px-4 py-3.5 text-sm font-medium text-slate-300 hover:bg-slate-800 hover:border-slate-600 transition">Restore from seed</button>
      </div>
    </div>
  `;
}

export function renderSetupModalOverlay(): string {
  return `
    <div data-action="setup-modal-backdrop" class="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm">
      <div class="relative w-full max-w-[432px] mx-4">
        <button data-action="close-setup-modal" class="absolute -top-10 right-0 flex items-center gap-1.5 text-xs text-slate-500 hover:text-slate-300 transition">
          <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2"><path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12"/></svg>
          Close
        </button>
        ${renderOnboarding()}
      </div>
    </div>
  `;
}
