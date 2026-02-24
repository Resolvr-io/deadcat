import { SATS_PER_FULL_CONTRACT, state } from "../state.ts";
import {
  clampContractPriceSats,
  commitLimitPriceDraft,
  commitTradeContractsDraft,
  commitTradeSizeSatsDraft,
  getSelectedMarket,
  setLimitPriceSats,
} from "../utils/market.ts";

export function handleKeydown(
  e: KeyboardEvent,
  deps: { render: () => void; openMarket: (id: string) => void },
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

  if (e.key !== "Enter") return;

  if (target.id === "onboarding-nostr-nsec") {
    e.preventDefault();
    const btn = document.querySelector(
      "[data-action='onboarding-import-nostr']",
    ) as HTMLElement | null;
    btn?.click();
    return;
  }

  if (target.id === "onboarding-wallet-password") {
    e.preventDefault();
    const actionName =
      state.onboardingWalletMode === "nostr-restore"
        ? "onboarding-nostr-restore-wallet"
        : state.onboardingWalletMode === "create"
          ? "onboarding-create-wallet"
          : "onboarding-restore-wallet";
    const btn = document.querySelector(
      `[data-action='${actionName}']`,
    ) as HTMLElement | null;
    btn?.click();
    return;
  }

  if (target.id === "nostr-replace-confirm") {
    e.preventDefault();
    if (state.nostrReplaceConfirm.trim().toUpperCase() === "DELETE") {
      const btn = document.querySelector(
        "[data-action='nostr-replace-confirm']",
      ) as HTMLElement | null;
      btn?.click();
    }
    return;
  }

  if (target.id === "wallet-delete-confirm") {
    e.preventDefault();
    if (state.walletDeleteConfirm.trim().toUpperCase() === "DELETE") {
      const btn = document.querySelector(
        "[data-action='wallet-delete-confirm']",
      ) as HTMLElement | null;
      btn?.click();
    }
    return;
  }

  if (target.id === "dev-reset-confirm") {
    e.preventDefault();
    if (state.devResetConfirm.trim().toUpperCase() === "RESET") {
      const btn = document.querySelector(
        "[data-action='dev-reset-confirm']",
      ) as HTMLElement | null;
      btn?.click();
    }
    return;
  }

  if (target.id === "wallet-backup-password") {
    e.preventDefault();
    const btn = document.querySelector(
      "[data-action='export-backup']",
    ) as HTMLElement | null;
    btn?.click();
    return;
  }

  if (target.id === "settings-backup-password") {
    e.preventDefault();
    const btn = document.querySelector(
      "[data-action='settings-backup-wallet']",
    ) as HTMLElement | null;
    btn?.click();
    return;
  }

  if (target.id === "wallet-password") {
    e.preventDefault();
    if (state.walletStatus === "not_created") {
      if (state.walletShowRestore) {
        target
          .closest("[data-action='restore-wallet']")
          ?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
        const btn = document.querySelector(
          "[data-action='restore-wallet']",
        ) as HTMLElement | null;
        btn?.click();
      } else {
        const btn = document.querySelector(
          "[data-action='create-wallet']",
        ) as HTMLElement | null;
        btn?.click();
      }
    } else if (state.walletStatus === "locked") {
      const btn = document.querySelector(
        "[data-action='unlock-wallet']",
      ) as HTMLElement | null;
      btn?.click();
    }
    return;
  }

  if (target.id === "trade-size-sats") {
    e.preventDefault();
    commitTradeSizeSatsDraft();
    render();
    return;
  }

  if (target.id === "trade-size-contracts") {
    e.preventDefault();
    commitTradeContractsDraft(getSelectedMarket());
    render();
  }
}
