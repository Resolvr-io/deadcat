import { SATS_PER_FULL_CONTRACT, state } from "../state.ts";

export function handleInput(e: Event, render: () => void): void {
  const target = e.target as HTMLInputElement;

  if (target.id === "onboarding-nostr-nsec") {
    state.onboardingNostrNsec = target.value;
    return;
  }

  if (target.id === "onboarding-wallet-password") {
    state.onboardingWalletPassword = target.value;
    return;
  }

  if (target.id === "onboarding-wallet-mnemonic") {
    state.onboardingWalletMnemonic = (
      target as unknown as HTMLTextAreaElement
    ).value;
    return;
  }

  if (target.id === "global-search" || target.id === "global-search-mobile") {
    state.search = target.value;
    if (state.view === "home") render();
    return;
  }

  if (target.id === "trade-size-sats") {
    state.tradeSizeSatsDraft = target.value;
    return;
  }

  if (target.id === "trade-size-contracts") {
    const cleaned = target.value
      .replace(/[^\d.]/g, "")
      .replace(/(\..*)\./g, "$1");
    const [wholeRaw, fractionRaw] = cleaned.split(".");
    const whole = wholeRaw.slice(0, 6);
    const fraction = fractionRaw?.slice(0, 2);
    const normalized =
      cleaned.length === 0
        ? ""
        : fractionRaw !== undefined
          ? `${whole}.${fraction ?? ""}`
          : whole;
    state.tradeContractsDraft = normalized;
    if (target.value !== normalized) {
      target.value = normalized;
    }
    return;
  }

  if (target.id === "limit-price") {
    state.limitPriceDraft = target.value.replace(/[^\d]/g, "").slice(0, 2);
    return;
  }

  if (target.id === "pairs-input") {
    state.pairsInput = Math.max(1, Math.floor(Number(target.value) || 1));
    render();
    return;
  }

  if (target.id === "tokens-input") {
    state.tokensInput = Math.max(1, Math.floor(Number(target.value) || 1));
    render();
    return;
  }

  if (target.id === "wallet-password") {
    state.walletPassword = target.value;
    return;
  }

  if (target.id === "wallet-restore-mnemonic") {
    state.walletRestoreMnemonic = (
      target as unknown as HTMLTextAreaElement
    ).value;
    return;
  }

  if (target.id === "nostr-import-nsec") {
    state.nostrImportNsec = target.value;
    return;
  }

  if (target.id === "nostr-replace-confirm") {
    state.nostrReplaceConfirm = target.value;
    const confirmBtn = document.querySelector(
      "[data-action='nostr-replace-confirm']",
    ) as HTMLButtonElement | null;
    if (confirmBtn) {
      const enabled = target.value.trim().toUpperCase() === "DELETE";
      confirmBtn.disabled = !enabled;
      confirmBtn.className = `shrink-0 rounded-lg border border-rose-700/60 px-3 py-2 text-xs transition ${enabled ? "bg-rose-500/20 text-rose-300 hover:bg-rose-500/30" : "text-slate-600 cursor-not-allowed"}`;
    }
    return;
  }

  if (target.id === "wallet-delete-confirm") {
    state.walletDeleteConfirm = target.value;
    const confirmBtn = document.querySelector(
      "[data-action='wallet-delete-confirm']",
    ) as HTMLButtonElement | null;
    if (confirmBtn) {
      const enabled = target.value.trim().toUpperCase() === "DELETE";
      confirmBtn.disabled = !enabled;
      confirmBtn.className = `shrink-0 rounded-lg border border-rose-700/60 px-3 py-2 text-xs transition ${enabled ? "bg-rose-500/20 text-rose-300 hover:bg-rose-500/30" : "text-slate-600 cursor-not-allowed"}`;
    }
    return;
  }

  if (target.id === "dev-reset-confirm") {
    state.devResetConfirm = target.value;
    const confirmBtn = document.querySelector(
      "[data-action='dev-reset-confirm']",
    ) as HTMLButtonElement | null;
    if (confirmBtn) {
      const enabled = target.value.trim().toUpperCase() === "RESET";
      confirmBtn.disabled = !enabled;
      confirmBtn.className = `shrink-0 rounded-lg border border-rose-700/60 px-3 py-2 text-xs transition ${enabled ? "bg-rose-500/20 text-rose-300 hover:bg-rose-500/30" : "text-slate-600 cursor-not-allowed"}`;
    }
    return;
  }

  if (target.id === "relay-input") {
    state.relayInput = target.value;
    return;
  }

  if (target.id === "receive-amount") {
    const v = target.value.replace(/^-/, "");
    state.receiveAmount = v;
    if (target.value !== v) target.value = v;
    return;
  }

  if (target.id === "send-invoice") {
    state.sendInvoice = target.value;
    return;
  }

  if (target.id === "send-liquid-address") {
    state.sendLiquidAddress = target.value;
    return;
  }

  if (target.id === "send-liquid-amount") {
    const v = target.value.replace(/^-/, "");
    state.sendLiquidAmount = v;
    if (target.value !== v) target.value = v;
    return;
  }

  if (target.id === "send-btc-amount") {
    const v = target.value.replace(/^-/, "");
    state.sendBtcAmount = v;
    if (target.value !== v) target.value = v;
    return;
  }

  if (target.id === "wallet-backup-password") {
    if (state.walletData) state.walletData.backupPassword = target.value;
    return;
  }

  if (target.id === "settings-backup-password") {
    state.nostrBackupPassword = target.value;
    return;
  }

  if (target.id === "create-question") {
    state.createQuestion = target.value;
    return;
  }

  if (target.id === "create-description") {
    state.createDescription = target.value;
    return;
  }

  if (target.id === "create-resolution-source") {
    state.createResolutionSource = target.value;
    return;
  }

  if (target.id === "create-yes-sats") {
    const parsed = Math.round(Number(target.value) || 50);
    state.createStartingYesSats = Math.max(
      1,
      Math.min(SATS_PER_FULL_CONTRACT - 1, parsed),
    );
    return;
  }
}
