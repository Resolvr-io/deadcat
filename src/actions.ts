export type ActionDomain = "onboarding" | "app" | "wallet" | "market";

export const ONBOARDING_ACTION_LIST = [
  "onboarding-set-nostr-mode",
  "onboarding-generate-nostr",
  "onboarding-import-nostr",
  "onboarding-copy-npub",
  "onboarding-reveal-nsec",
  "onboarding-copy-nsec",
  "onboarding-nostr-continue",
  "onboarding-set-wallet-mode",
  "onboarding-create-wallet",
  "onboarding-copy-mnemonic",
  "onboarding-wallet-done",
  "onboarding-restore-wallet",
  "onboarding-nostr-restore-wallet",
] as const;

export const APP_ACTION_LIST = [
  "go-home",
  "set-chart-timescale",
  "toggle-user-menu",
  "open-search",
  "close-search",
  "open-help",
  "close-help",
  "copy-nostr-npub",
  "copy-to-clipboard",
  "set-currency",
  "user-settings",
  "toggle-settings-section",
  "close-settings",
  "reveal-nostr-nsec",
  "copy-nostr-nsec",
  "nostr-replace-start",
  "nostr-replace-cancel",
  "nostr-replace-confirm",
  "nostr-replace-back",
  "import-nostr-nsec",
  "generate-new-nostr-key",
  "dev-restart",
  "dev-reset-start",
  "dev-reset-cancel",
  "dev-reset-confirm",
  "add-relay",
  "remove-relay",
  "reset-relays",
  "nostr-backup-wallet",
  "cancel-backup-prompt",
  "settings-backup-wallet",
  "delete-nostr-backup",
  "nostr-restore-wallet",
  "user-logout",
  "close-logout",
  "confirm-logout",
  "open-create-market",
  "open-wallet",
] as const;

export const WALLET_ACTION_LIST = [
  "create-wallet",
  "dismiss-mnemonic",
  "toggle-restore",
  "restore-wallet",
  "unlock-wallet",
  "lock-wallet",
  "wallet-delete-start",
  "wallet-delete-cancel",
  "wallet-delete-confirm",
  "forgot-password-delete",
  "toggle-balance-hidden",
  "toggle-utxos-expanded",
  "toggle-mini-wallet",
  "set-wallet-unit",
  "sync-wallet",
  "open-explorer-tx",
  "open-nostr-event",
  "nostr-event-backdrop",
  "close-nostr-event-modal",
  "copy-nostr-event-json",
  "open-receive",
  "open-send",
  "close-modal",
  "modal-backdrop",
  "modal-tab",
  "receive-preset",
  "create-lightning-receive",
  "generate-liquid-address",
  "create-bitcoin-receive",
  "pay-lightning-invoice",
  "send-liquid",
  "create-bitcoin-send",
  "copy-modal-value",
  "refresh-swap",
  "copy-mnemonic",
  "show-backup",
  "hide-backup",
  "export-backup",
  "copy-backup-mnemonic",
] as const;

export const MARKET_ACTION_LIST = [
  "toggle-category-dropdown",
  "select-create-category",
  "toggle-settlement-picker",
  "settlement-prev-month",
  "settlement-next-month",
  "pick-settlement-day",
  "toggle-settlement-dropdown",
  "pick-settlement-option",
  "cancel-create-market",
  "oracle-attest-yes",
  "oracle-attest-no",
  "execute-resolution",
  "refresh-market-state",
  "toggle-advanced-details",
  "toggle-advanced-actions",
  "toggle-orderbook",
  "toggle-fee-details",
  "use-cashout",
  "sell-max",
  "sell-25",
  "sell-50",
  "trending-prev",
  "trending-next",
  "step-limit-price",
  "step-trade-contracts",
  "submit-trade",
  "submit-issue",
  "submit-redeem",
  "submit-cancel",
  "submit-create-market",
] as const;

export type OnboardingAction = (typeof ONBOARDING_ACTION_LIST)[number];
export type AppAction = (typeof APP_ACTION_LIST)[number];
export type WalletAction = (typeof WALLET_ACTION_LIST)[number];
export type MarketAction = (typeof MARKET_ACTION_LIST)[number];
export type Action = OnboardingAction | AppAction | WalletAction | MarketAction;

export const ONBOARDING_ACTIONS = new Set<string>(ONBOARDING_ACTION_LIST);
export const APP_ACTIONS = new Set<string>(APP_ACTION_LIST);
export const WALLET_ACTIONS = new Set<string>(WALLET_ACTION_LIST);
export const MARKET_ACTIONS = new Set<string>(MARKET_ACTION_LIST);

const ALL_ACTIONS = new Set<string>([
  ...ONBOARDING_ACTION_LIST,
  ...APP_ACTION_LIST,
  ...WALLET_ACTION_LIST,
  ...MARKET_ACTION_LIST,
]);

export const OPEN_WALLET_ACTION: AppAction = "open-wallet";

export function asAction(value: string | null): Action | null {
  if (!value || !ALL_ACTIONS.has(value)) return null;
  return value as Action;
}

export function resolveActionDomain(
  action: Action | null,
): ActionDomain | null {
  if (!action) return null;
  if (ONBOARDING_ACTIONS.has(action)) return "onboarding";
  if (APP_ACTIONS.has(action)) return "app";
  if (WALLET_ACTIONS.has(action)) return "wallet";
  if (MARKET_ACTIONS.has(action)) return "market";
  return null;
}
