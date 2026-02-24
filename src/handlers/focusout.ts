import { SATS_PER_FULL_CONTRACT, state } from "../state.ts";
import {
  commitLimitPriceDraft,
  commitTradeContractsDraft,
  commitTradeSizeSatsDraft,
  getSelectedMarket,
} from "../utils/market.ts";

export function handleFocusout(e: FocusEvent, render: () => void): void {
  const target = e.target as HTMLInputElement;

  if (target.id === "trade-size-sats") {
    commitTradeSizeSatsDraft();
    render();
    return;
  }

  if (target.id === "trade-size-contracts") {
    commitTradeContractsDraft(getSelectedMarket());
    render();
    return;
  }

  if (target.id === "limit-price") {
    commitLimitPriceDraft();
    const nextFocus = e.relatedTarget as HTMLElement | null;
    if (nextFocus?.closest("[data-action='step-limit-price']")) {
      return;
    }
    render();
    return;
  }

  if (
    target.id === "create-question" ||
    target.id === "create-description" ||
    target.id === "create-resolution-source" ||
    target.id === "create-yes-sats"
  ) {
    if (target.id === "create-yes-sats") {
      const parsed = Math.round(Number(target.value) || 50);
      state.createStartingYesSats = Math.max(
        1,
        Math.min(SATS_PER_FULL_CONTRACT - 1, parsed),
      );
    }
    if (state.view === "create") render();
  }
}
