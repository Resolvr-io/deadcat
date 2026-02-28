import { asAction, resolveActionDomain } from "../actions.ts";
import { state } from "../state.ts";
import type {
  ActionTab,
  NavCategory,
  OrderType,
  Side,
  SizeMode,
  TradeIntent,
} from "../types.ts";
import {
  type ClickDomainContext,
  dispatchDomainAction,
} from "./domains/index.ts";

export type ClickDeps = {
  render: () => void;
  openMarket: (
    id: string,
    options?: { side?: string; intent?: string },
  ) => void;
  finishOnboarding: () => Promise<void>;
};

export async function handleClick(
  e: MouseEvent,
  deps: ClickDeps,
): Promise<void> {
  const { render, openMarket, finishOnboarding } = deps;

  const target = e.target as HTMLElement;
  const categoryEl = target.closest("[data-category]") as HTMLElement | null;
  const openMarketEl = target.closest(
    "[data-open-market]",
  ) as HTMLElement | null;
  const actionEl = target.closest("[data-action]") as HTMLElement | null;
  const sideEl = target.closest("[data-side]") as HTMLElement | null;
  const tradeChoiceEl = target.closest(
    "[data-trade-choice]",
  ) as HTMLElement | null;
  const tradeIntentEl = target.closest(
    "[data-trade-intent]",
  ) as HTMLElement | null;
  const sizeModeEl = target.closest("[data-size-mode]") as HTMLElement | null;
  const tradeSizePresetEl = target.closest(
    "[data-trade-size-sats]",
  ) as HTMLElement | null;
  const tradeSizeDeltaEl = target.closest(
    "[data-trade-size-delta]",
  ) as HTMLElement | null;
  const orderTypeEl = target.closest("[data-order-type]") as HTMLElement | null;
  const tabEl = target.closest("[data-tab]") as HTMLElement | null;

  const category = categoryEl?.getAttribute(
    "data-category",
  ) as NavCategory | null;
  const openMarketId = openMarketEl?.getAttribute("data-open-market") ?? null;
  const openSide = openMarketEl?.getAttribute("data-open-side") as Side | null;
  const openIntentRaw = openMarketEl?.getAttribute("data-open-intent");
  const action = asAction(actionEl?.getAttribute("data-action") ?? null);
  const side = sideEl?.getAttribute("data-side") as Side | null;
  const tradeChoiceRaw =
    tradeChoiceEl?.getAttribute("data-trade-choice") ?? null;
  const tradeIntent = tradeIntentEl?.getAttribute(
    "data-trade-intent",
  ) as TradeIntent | null;
  const sizeMode = sizeModeEl?.getAttribute(
    "data-size-mode",
  ) as SizeMode | null;
  const tradeSizePreset = Number(
    tradeSizePresetEl?.getAttribute("data-trade-size-sats") ?? "",
  );
  const tradeSizeDelta = Number(
    tradeSizeDeltaEl?.getAttribute("data-trade-size-delta") ?? "",
  );
  const limitPriceDelta = Number(
    actionEl?.getAttribute("data-limit-price-delta") ?? "",
  );
  const contractsStepDelta = Number(
    actionEl?.getAttribute("data-contracts-step-delta") ?? "",
  );
  const orderType = orderTypeEl?.getAttribute(
    "data-order-type",
  ) as OrderType | null;
  const tab = tabEl?.getAttribute("data-tab") as ActionTab | null;
  const actionDomain = resolveActionDomain(action);

  // Close user menu on any click that isn't inside the menu
  if (
    state.userMenuOpen &&
    action !== "toggle-user-menu" &&
    action !== "user-settings" &&
    action !== "user-logout" &&
    action !== "set-currency" &&
    action !== "copy-nostr-npub"
  ) {
    const inMenu = target
      .closest("[data-action='toggle-user-menu']")
      ?.parentElement?.contains(target);
    if (!inMenu) {
      state.userMenuOpen = false;
      render();
    }
  }

  // Close category dropdown on any click outside it
  if (
    state.createCategoryOpen &&
    action !== "toggle-category-dropdown" &&
    action !== "select-create-category"
  ) {
    const inDropdown = target.closest("#create-category-dropdown");
    if (!inDropdown) {
      state.createCategoryOpen = false;
      render();
    }
  }

  // Close settlement picker on any click outside it
  if (
    state.createSettlementPickerOpen &&
    action !== "toggle-settlement-picker" &&
    action !== "settlement-prev-month" &&
    action !== "settlement-next-month" &&
    action !== "pick-settlement-day" &&
    action !== "toggle-settlement-dropdown" &&
    action !== "pick-settlement-option"
  ) {
    const inPicker = target.closest("#settlement-picker");
    if (!inPicker) {
      state.createSettlementPickerOpen = false;
      render();
    }
  }

  if (category) {
    state.activeCategory = category;
    state.view = "home";
    state.chartHoverMarketId = null;
    state.chartHoverX = null;
    render();
    return;
  }

  if (openMarketId) {
    const openIntent: TradeIntent | undefined =
      openIntentRaw === "sell"
        ? "close"
        : openIntentRaw === "buy"
          ? "open"
          : openIntentRaw === "open" || openIntentRaw === "close"
            ? openIntentRaw
            : undefined;
    openMarket(openMarketId, {
      side: openSide ?? undefined,
      intent: openIntent,
    });
    return;
  }

  const context: ClickDomainContext = {
    target,
    actionEl,
    action,
    actionDomain,
    side,
    tradeChoiceRaw,
    tradeIntent,
    sizeMode,
    tradeSizePreset,
    tradeSizeDelta,
    limitPriceDelta,
    contractsStepDelta,
    orderType,
    tab,
    render,
    finishOnboarding,
  };

  await dispatchDomainAction(context);
}
