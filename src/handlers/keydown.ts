import { SATS_PER_FULL_CONTRACT, state } from "../state.ts";
import {
  clampContractPriceSats,
  commitLimitPriceDraft,
  commitTradeContractsDraft,
  commitTradeSizeSatsDraft,
  getSelectedMarket,
  setLimitPriceSats,
} from "../utils/market.ts";

function clickAction(action: string): void {
  const button = document.querySelector(
    `[data-action='${action}']`,
  ) as HTMLElement | null;
  button?.click();
}

type EnterHandler = (target: HTMLInputElement, render: () => void) => void;

const ENTER_HANDLERS: Record<string, EnterHandler> = {
  "onboarding-nostr-nsec": (_target, _render) => {
    clickAction("onboarding-import-nostr");
  },
  "onboarding-wallet-password": (_target, _render) => {
    const actionName =
      state.onboardingWalletMode === "nostr-restore"
        ? "onboarding-nostr-restore-wallet"
        : state.onboardingWalletMode === "create"
          ? "onboarding-create-wallet"
          : "onboarding-restore-wallet";
    clickAction(actionName);
  },
  "nostr-replace-confirm": (_target, _render) => {
    if (state.nostrReplaceConfirm.trim().toUpperCase() === "DELETE") {
      clickAction("nostr-replace-confirm");
    }
  },
  "wallet-delete-confirm": (_target, _render) => {
    if (state.walletDeleteConfirm.trim().toUpperCase() === "DELETE") {
      clickAction("wallet-delete-confirm");
    }
  },
  "dev-reset-confirm": (_target, _render) => {
    if (state.devResetConfirm.trim().toUpperCase() === "RESET") {
      clickAction("dev-reset-confirm");
    }
  },
  "wallet-backup-password": (_target, _render) => {
    clickAction("export-backup");
  },
  "settings-backup-password": (_target, _render) => {
    clickAction("settings-backup-wallet");
  },
  "wallet-password": (_target, _render) => {
    if (state.walletStatus === "not_created") {
      clickAction(state.walletShowRestore ? "restore-wallet" : "create-wallet");
      return;
    }
    if (state.walletStatus === "locked") {
      clickAction("unlock-wallet");
    }
  },
  "trade-size-sats": (_target, render) => {
    commitTradeSizeSatsDraft();
    render();
  },
  "trade-size-contracts": (_target, render) => {
    commitTradeContractsDraft(getSelectedMarket());
    render();
  },
};

export function handleKeydown(
  e: KeyboardEvent,
  deps: { render: () => void },
): void {
  const { render } = deps;
  const target = e.target as HTMLInputElement;

  if (target.id === "limit-price") {
    if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      e.preventDefault();
      const delta = e.key === "ArrowUp" ? 1 : -1;
      const currentSats = clampContractPriceSats(
        state.limitPriceDraft.length > 0
          ? Number(state.limitPriceDraft)
          : state.limitPrice * SATS_PER_FULL_CONTRACT,
      );
      setLimitPriceSats(currentSats + delta);
      render();
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      commitLimitPriceDraft();
      render();
      return;
    }
  }

  if (target.id === "trade-size-contracts") {
    if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      e.preventDefault();
      const market = getSelectedMarket();
      const current = Number(state.tradeContractsDraft);
      const baseValue = Number.isFinite(current)
        ? current
        : Math.max(0.01, state.tradeContracts);
      const delta = e.key === "ArrowUp" ? 0.01 : -0.01;
      state.tradeContractsDraft = Math.max(0.01, baseValue + delta).toFixed(2);
      commitTradeContractsDraft(market);
      render();
      return;
    }
  }

  if (e.key !== "Enter") return;

  const handler = ENTER_HANDLERS[target.id];
  if (!handler) return;

  e.preventDefault();
  handler(target, render);
}
