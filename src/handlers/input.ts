import { state } from "../state.ts";

type InputHandler = (target: HTMLInputElement, render: () => void) => void;

function asTextarea(target: HTMLInputElement): HTMLTextAreaElement {
  return target as unknown as HTMLTextAreaElement;
}

const INPUT_HANDLERS: Record<string, InputHandler> = {
  "onboarding-nostr-nsec": (target) => {
    state.onboardingNostrNsec = target.value;
  },
  "onboarding-wallet-password": (target) => {
    state.onboardingWalletPassword = target.value;
  },
  "onboarding-wallet-password-confirm": (target) => {
    state.onboardingWalletPasswordConfirm = target.value;
  },
  "onboarding-wallet-mnemonic": (target) => {
    state.onboardingWalletMnemonic = asTextarea(target).value;
  },
  "global-search": (target, render) => {
    state.search = target.value;
    if (state.view === "home") render();
  },
  "global-search-mobile": (target, render) => {
    state.search = target.value;
    if (state.view === "home") render();
  },
  "trade-size-sats": (target) => {
    state.tradeSizeSatsDraft = target.value;
  },
  "trade-size-contracts": (target) => {
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
  },
  "limit-price": (target) => {
    state.limitPriceDraft = target.value.replace(/[^\d]/g, "").slice(0, 2);
  },
  "pairs-input": (target, render) => {
    state.pairsInput = Math.max(1, Math.floor(Number(target.value) || 1));
    render();
  },
  "tokens-input": (target, render) => {
    state.tokensInput = Math.max(1, Math.floor(Number(target.value) || 1));
    render();
  },
  "wallet-password": (target) => {
    state.walletPassword = target.value;
  },
  "wallet-password-confirm": (target) => {
    state.walletPasswordConfirm = target.value;
  },
  "wallet-restore-mnemonic": (target) => {
    state.walletRestoreMnemonic = asTextarea(target).value;
  },
  "nostr-import-nsec": (target) => {
    state.nostrImportNsec = target.value;
  },
  "nostr-replace-confirm": (target, render) => {
    state.nostrReplaceConfirm = target.value;
    render();
  },
  "wallet-delete-confirm": (target, render) => {
    state.walletDeleteConfirm = target.value;
    render();
  },
  "dev-reset-confirm": (target, render) => {
    state.devResetConfirm = target.value;
    render();
  },
  "relay-input": (target) => {
    state.relayInput = target.value;
  },
  "receive-amount": (target) => {
    const v = target.value.replace(/^-/, "");
    state.receiveAmount = v;
    if (target.value !== v) target.value = v;
  },
  "send-invoice": (target) => {
    state.sendInvoice = target.value;
  },
  "send-liquid-address": (target) => {
    state.sendLiquidAddress = target.value;
  },
  "send-liquid-amount": (target) => {
    const v = target.value.replace(/^-/, "");
    state.sendLiquidAmount = v;
    if (target.value !== v) target.value = v;
  },
  "send-btc-amount": (target) => {
    const v = target.value.replace(/^-/, "");
    state.sendBtcAmount = v;
    if (target.value !== v) target.value = v;
  },
  "wallet-backup-password": (target) => {
    if (state.walletData) state.walletData.backupPassword = target.value;
  },
  "settings-backup-password": (target) => {
    state.nostrBackupPassword = target.value;
  },
  "create-question": (target) => {
    state.createQuestion = target.value;
  },
  "create-description": (target) => {
    state.createDescription = target.value;
  },
  "create-resolution-source": (target) => {
    state.createResolutionSource = target.value;
  },
};

export function handleInput(e: Event, render: () => void): void {
  const target = e.target as HTMLInputElement;
  const handler = INPUT_HANDLERS[target.id];
  if (!handler) return;
  handler(target, render);
}
