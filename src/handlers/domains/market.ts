import { tauriApi } from "../../api/tauri.ts";
import {
  discoveredToMarket,
  issueTokens,
  marketToContractParams,
  refreshMarketsFromStore,
} from "../../services/markets.ts";
import { refreshWallet } from "../../services/wallet.ts";
import {
  defaultSettlementInput,
  markets,
  SATS_PER_FULL_CONTRACT,
  state,
} from "../../state.ts";
import type {
  ActionTab,
  ContractParamsPayload,
  CovenantState,
  DiscoveredOrder,
  Market,
  MarketCategory,
  OrderType,
  QuoteMarketTradeRequestPayload,
  Side,
  TradeDirection,
  TradeIntent,
} from "../../types.ts";
import { showToast } from "../../ui/toast.ts";
import { reverseHex } from "../../utils/crypto.ts";
import { formatSats, formatSatsInput } from "../../utils/format.ts";
import {
  clampContractPriceSats,
  commitLimitPriceDraft,
  commitTradeContractsDraft,
  getAvailableOrderContracts,
  getBasePriceSats,
  getLimitSellWarningFromSellQuote,
  getPathAvailability,
  getPositionContracts,
  getSelectedMarket,
  getTradePreview,
  getTrendingMarkets,
  isExpired,
  resetLimitSellWarningState,
  setLimitPriceSats,
  stateLabel,
} from "../../utils/market.ts";
import { runAsyncAction } from "./async-action.ts";
import type { ClickDomainContext } from "./context.ts";
import { getFillBlockedReason } from "./limit-order-guards.ts";

function ticketActionAllowed(market: Market, tab: ActionTab): boolean {
  const paths = getPathAvailability(market);
  if (tab === "trade") return true;
  if (tab === "issue") return paths.initialIssue || paths.issue;
  if (tab === "redeem") return paths.redeem || paths.expiryRedeem;
  return paths.cancel;
}

function toMakerOrderPayload(order: DiscoveredOrder) {
  return {
    base_asset_id_hex: order.base_asset_id,
    quote_asset_id_hex: order.quote_asset_id,
    price: order.price,
    min_fill_lots: order.min_fill_lots,
    min_remainder_lots: order.min_remainder_lots,
    direction: order.direction,
    maker_receive_spk_hash_hex: order.maker_receive_spk_hash,
    cosigner_pubkey_hex: order.cosigner_pubkey,
    maker_pubkey_hex: order.maker_base_pubkey,
  };
}

function resetTradeQuoteState(): void {
  state.tradeQuoteModalOpen = false;
  state.tradeQuoteLoading = false;
  state.tradeQuoteExecuting = false;
  state.tradeQuoteData = null;
  state.tradeQuoteError = "";
  state.tradeQuoteNowUnix = Math.floor(Date.now() / 1000);
}

function resetLimitSellOverride(): void {
  resetLimitSellWarningState();
}

function commitTradeSatsDraftForQuote(): void {
  const parsed = Math.floor(
    Number(state.tradeSizeSatsDraft.replace(/,/g, "")) ||
      state.tradeSizeSats ||
      1,
  );
  const clamped = Math.max(1, parsed);
  state.tradeSizeSats = clamped;
  state.tradeSizeSatsDraft = formatSatsInput(clamped);
}

function getSelectedPositionLots(market: Market): number {
  const positions = getPositionContracts(market);
  const raw = state.selectedSide === "yes" ? positions.yes : positions.no;
  return Math.max(0, Math.floor(raw));
}

function getSellSizeValidationMessage(market: Market): string {
  return getSelectedPositionLots(market) < 1
    ? "No contracts available on this side."
    : "Enter contracts to sell.";
}

function clampContractsForSell(
  contracts: number,
  availableLots: number,
): number {
  if (availableLots <= 0) return 0;
  return Math.max(0, Math.min(Math.floor(contracts), availableLots));
}

type LimitSellGuardSnapshot = {
  guardVersion: number;
  marketId: string;
  side: Side;
  tradeIntent: TradeIntent;
  orderType: OrderType;
  contracts: number;
  limitPriceSats: number;
};

function captureLimitSellGuardSnapshot(market: Market): LimitSellGuardSnapshot {
  return {
    guardVersion: state.limitSellGuardVersion,
    marketId: market.marketId,
    side: state.selectedSide,
    tradeIntent: state.tradeIntent,
    orderType: state.orderType,
    contracts: state.tradeContracts,
    limitPriceSats: clampContractPriceSats(
      state.limitPrice * SATS_PER_FULL_CONTRACT,
    ),
  };
}

function isLimitSellGuardSnapshotCurrent(
  market: Market,
  snapshot: LimitSellGuardSnapshot,
): boolean {
  return (
    state.limitSellGuardVersion === snapshot.guardVersion &&
    market.marketId === snapshot.marketId &&
    state.selectedSide === snapshot.side &&
    state.tradeIntent === snapshot.tradeIntent &&
    state.orderType === snapshot.orderType &&
    state.tradeContracts === snapshot.contracts &&
    clampContractPriceSats(state.limitPrice * SATS_PER_FULL_CONTRACT) ===
      snapshot.limitPriceSats
  );
}

function submitLimitOrder(
  market: Market,
  intent: TradeIntent,
  render: () => void,
): void {
  if (intent === "close" && state.tradeContracts < 1) {
    showToast(getSellSizeValidationMessage(market), "error");
    return;
  }
  const preview = getTradePreview(market);
  const pairs = Math.max(1, Math.floor(preview.requestedContracts));
  const paths = getPathAvailability(market);
  if (!paths.issue) {
    showToast("Market is not in a tradeable state for limit orders", "error");
    return;
  }

  const sideAssetId =
    state.selectedSide === "yes" ? market.yesAssetId : market.noAssetId;
  const direction = intent === "open" ? "sell-quote" : "sell-base";
  const orderAmount =
    direction === "sell-quote" ? pairs * preview.limitPriceSats : pairs;
  const directionLabel = `${intent === "open" ? "buy" : "sell"}-${state.selectedSide}`;

  if (!Number.isSafeInteger(orderAmount) || orderAmount <= 0) {
    showToast("Computed order amount is invalid", "error");
    return;
  }

  if (intent === "close") {
    const availableLots = getSelectedPositionLots(market);
    if (pairs > availableLots) {
      showToast(
        `Insufficient ${state.selectedSide.toUpperCase()} tokens for this limit order`,
        "error",
      );
      return;
    }
  }

  const confirmation =
    intent === "open"
      ? `Place limit buy for ${pairs} ${state.selectedSide.toUpperCase()} token(s) at ${preview.limitPriceSats} sats each?\n\nThis posts a resting order and locks ${formatSats(orderAmount)} of quote collateral until filled/cancelled.`
      : `Place limit sell for ${pairs} ${state.selectedSide.toUpperCase()} token(s) at ${preview.limitPriceSats} sats each?\n\nThis posts a resting order and locks ${pairs.toLocaleString()} token(s) until filled/cancelled.`;
  if (!window.confirm(confirmation)) return;

  showToast("Placing limit order...", "info");
  runAsyncAction(async () => {
    try {
      const result = await tauriApi.createLimitOrder({
        base_asset_id_hex: sideAssetId,
        quote_asset_id_hex: market.collateralAssetId,
        price: preview.limitPriceSats,
        order_amount: orderAmount,
        direction,
        min_fill_lots: 1,
        min_remainder_lots: 1,
        market_id: market.marketId,
        direction_label: directionLabel,
      });
      showToast(
        `Limit order posted! txid: ${result.txid.slice(0, 16)}...`,
        "success",
      );
      resetLimitSellOverride();
      await refreshWallet(render);
      await refreshMarketsFromStore();
      render();
    } catch (error) {
      showToast(`Limit order failed: ${error}`, "error");
    }
  });
}

export function buildMarketQuoteRequestPayload(
  contractParams: ContractParamsPayload,
  marketId: string,
  side: Side,
  intent: TradeIntent,
  tradeSizeSats: number,
  tradeContracts: number,
): QuoteMarketTradeRequestPayload {
  const direction: TradeDirection = intent === "open" ? "buy" : "sell";
  const exactInput =
    direction === "buy"
      ? Math.max(1, Math.floor(tradeSizeSats))
      : Math.max(1, Math.trunc(tradeContracts));
  return {
    contract_params: contractParams,
    market_id: marketId,
    side,
    direction,
    exact_input: exactInput,
  };
}

export async function handleMarketDomain(
  ctx: ClickDomainContext,
): Promise<void> {
  const {
    action,
    actionDomain,
    actionEl,
    render,
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
  } = ctx;
  if (actionDomain === "market" || actionDomain === null) {
    if (action === "toggle-category-dropdown") {
      state.createCategoryOpen = !state.createCategoryOpen;
      render();
      return;
    }

    if (action === "select-create-category") {
      const value = actionEl?.dataset.value;
      if (value) {
        state.createCategory = value as MarketCategory;
        state.createCategoryOpen = false;
        render();
      }
      return;
    }

    if (action === "toggle-settlement-picker") {
      state.createSettlementPickerOpen = !state.createSettlementPickerOpen;
      // Sync view month to currently selected date when opening
      if (state.createSettlementPickerOpen && state.createSettlementInput) {
        const d = new Date(state.createSettlementInput);
        state.createSettlementViewYear = d.getFullYear();
        state.createSettlementViewMonth = d.getMonth();
      }
      render();
      return;
    }

    if (action === "settlement-prev-month") {
      state.createSettlementViewMonth--;
      if (state.createSettlementViewMonth < 0) {
        state.createSettlementViewMonth = 11;
        state.createSettlementViewYear--;
      }
      render();
      return;
    }

    if (action === "settlement-next-month") {
      state.createSettlementViewMonth++;
      if (state.createSettlementViewMonth > 11) {
        state.createSettlementViewMonth = 0;
        state.createSettlementViewYear++;
      }
      render();
      return;
    }

    if (action === "pick-settlement-day") {
      const day = Number(actionEl?.dataset.day);
      if (!day) return;
      let hours = 12;
      let minutes = 0;
      if (state.createSettlementInput) {
        const prev = new Date(state.createSettlementInput);
        hours = prev.getHours();
        minutes = prev.getMinutes();
      }
      const y = state.createSettlementViewYear;
      const m = String(state.createSettlementViewMonth + 1).padStart(2, "0");
      const d = String(day).padStart(2, "0");
      const hh = String(hours).padStart(2, "0");
      const mm = String(minutes).padStart(2, "0");
      state.createSettlementInput = `${y}-${m}-${d}T${hh}:${mm}`;
      render();
      return;
    }

    if (action === "toggle-settlement-dropdown") {
      const name = actionEl?.dataset.dropdown ?? "";
      state.createSettlementPickerDropdown =
        state.createSettlementPickerDropdown === name ? "" : name;
      render();
      return;
    }

    if (action === "pick-settlement-option") {
      const dropdown = actionEl?.dataset.dropdown ?? "";
      const value = actionEl?.dataset.value ?? "";
      state.createSettlementPickerDropdown = "";

      if (dropdown === "month") {
        state.createSettlementViewMonth = Number(value);
      } else if (dropdown === "year") {
        state.createSettlementViewYear = Number(value);
      } else if (
        (dropdown === "hour" || dropdown === "minute" || dropdown === "ampm") &&
        state.createSettlementInput
      ) {
        const prev = new Date(state.createSettlementInput);
        let h = prev.getHours();
        let min = prev.getMinutes();
        const wasPM = h >= 12;
        let h12 = h % 12 || 12;

        if (dropdown === "hour") h12 = Number(value);
        if (dropdown === "minute") min = Number(value);
        let pm = wasPM;
        if (dropdown === "ampm") pm = value === "PM";

        h = (h12 % 12) + (pm ? 12 : 0);

        const y = prev.getFullYear();
        const mo = String(prev.getMonth() + 1).padStart(2, "0");
        const d = String(prev.getDate()).padStart(2, "0");
        const hh = String(h).padStart(2, "0");
        const mm = String(min).padStart(2, "0");
        state.createSettlementInput = `${y}-${mo}-${d}T${hh}:${mm}`;
      }
      render();
      return;
    }

    if (action === "cancel-create-market") {
      state.createCategoryOpen = false;
      state.createSettlementPickerOpen = false;
      state.createSettlementPickerDropdown = "";
      state.view = "home";
      render();
      return;
    }

    if (action === "oracle-attest-yes" || action === "oracle-attest-no") {
      const market = getSelectedMarket();
      const outcomeYes = action === "oracle-attest-yes";
      const outcomeLabel = outcomeYes ? "YES" : "NO";
      const postExpiryWarning = isExpired(market)
        ? "\n\nWarning: market is past expiry height. Oracle actions are still allowed, but this may conflict with pending/possible expiry finalization."
        : "";
      const confirmed = window.confirm(
        `Resolve "${market.question}" as ${outcomeLabel}?\n\nThis publishes a Schnorr signature to Nostr that permanently attests the outcome. This cannot be undone.${postExpiryWarning}`,
      );
      if (!confirmed) return;

      runAsyncAction(async () => {
        try {
          const result = await tauriApi.oracleAttest(
            market.marketId,
            outcomeYes,
          );
          // Save attestation for on-chain execution
          state.lastAttestationSig = result.signature_hex;
          state.lastAttestationOutcome = outcomeYes;
          state.lastAttestationMarketId = market.marketId;
          market.resolveTx = {
            txid: result.nostr_event_id,
            outcome: outcomeYes ? "yes" : "no",
            sigVerified: true,
            height: market.currentHeight,
            signatureHash: `${result.signature_hex.slice(0, 16)}...`,
          };
          showToast(
            `Attestation published to Nostr! Now execute on-chain to finalize.`,
            "success",
          );
          render();
        } catch (error) {
          window.alert(`Failed to attest: ${error}`);
        }
      });
      return;
    }

    if (action === "execute-resolution") {
      const market = getSelectedMarket();
      if (!state.lastAttestationSig || state.lastAttestationOutcome === null) {
        showToast("No attestation available to execute", "error");
        return;
      }
      const outcomeYes = state.lastAttestationOutcome;
      const oracleSignatureHex = state.lastAttestationSig;
      const postExpiryWarning = isExpired(market)
        ? "\n\nWarning: market is past expiry height. Resolution remains permitted, but expiry finalization may race if not yet finalized."
        : "";
      const confirmed = window.confirm(
        `Execute on-chain resolution for "${market.question}"?\n\nOutcome: ${outcomeYes ? "YES" : "NO"}\nThis submits a Liquid transaction that transitions the covenant state.${postExpiryWarning}`,
      );
      if (!confirmed) return;

      state.resolutionExecuting = true;
      render();
      runAsyncAction(async () => {
        try {
          const result = await tauriApi.resolveMarket(
            marketToContractParams(market),
            outcomeYes,
            oracleSignatureHex,
          );
          market.state = result.outcome_yes ? 2 : 3;
          state.lastAttestationSig = null;
          state.lastAttestationOutcome = null;
          state.lastAttestationMarketId = null;
          showToast(
            `Resolution executed! txid: ${result.txid.slice(0, 16)}... State: ${result.new_state}`,
            "success",
          );
          await refreshWallet(render);
        } catch (error) {
          showToast(`Resolution failed: ${error}`, "error");
        } finally {
          state.resolutionExecuting = false;
          render();
        }
      });
      return;
    }

    if (action === "refresh-market-state") {
      const market = getSelectedMarket();
      if (!market.creationTxid) {
        showToast("Market has no on-chain creation tx", "error");
        return;
      }
      showToast("Querying on-chain market state...", "info");
      runAsyncAction(async () => {
        try {
          const result = await tauriApi.getMarketState(
            marketToContractParams(market),
          );
          market.state = result.state as CovenantState;
          showToast(`Market state: ${stateLabel(market.state)}`, "success");
          render();
        } catch (error) {
          showToast(`State query failed: ${error}`, "error");
        }
      });
      return;
    }

    if (action === "toggle-advanced-details") {
      state.showAdvancedDetails = !state.showAdvancedDetails;
      render();
      return;
    }

    if (action === "toggle-advanced-actions") {
      state.showAdvancedActions = !state.showAdvancedActions;
      if (state.showAdvancedActions && state.actionTab === "trade") {
        state.actionTab = "issue";
      }
      render();
      return;
    }

    if (action === "toggle-orderbook") {
      state.showOrderbook = !state.showOrderbook;
      render();
      return;
    }

    if (action === "toggle-fee-details") {
      state.showFeeDetails = !state.showFeeDetails;
      render();
      return;
    }

    if (action === "toggle-buy-limit-composer") {
      state.buyLimitComposerOpen = !state.buyLimitComposerOpen;
      if (state.buyLimitComposerOpen) {
        state.orderType = "limit";
        state.sizeMode = "contracts";
      } else {
        state.orderType = "market";
        state.sizeMode = "sats";
      }
      render();
      return;
    }

    if (action === "ack-limit-sell-warning") {
      state.limitSellOverrideAccepted = true;
      render();
      return;
    }

    if (action === "close-trade-quote") {
      resetTradeQuoteState();
      render();
      return;
    }

    if (action === "trade-quote-backdrop" && actionEl === ctx.target) {
      resetTradeQuoteState();
      render();
      return;
    }

    if (action === "request-trade-quote") {
      const market = getSelectedMarket();
      const direction: TradeDirection =
        state.tradeIntent === "open" ? "buy" : "sell";

      if (direction === "buy") {
        state.sizeMode = "sats";
        commitTradeSatsDraftForQuote();
      } else {
        state.sizeMode = "contracts";
        commitTradeContractsDraft(market);
        if (state.tradeContracts < 1) {
          showToast(getSellSizeValidationMessage(market), "error");
          return;
        }
      }
      const quoteRequest = buildMarketQuoteRequestPayload(
        marketToContractParams(market),
        market.marketId,
        state.selectedSide,
        state.tradeIntent,
        state.tradeSizeSats,
        state.tradeContracts,
      );

      state.tradeQuoteModalOpen = true;
      state.tradeQuoteLoading = true;
      state.tradeQuoteExecuting = false;
      state.tradeQuoteData = null;
      state.tradeQuoteError = "";
      render();

      runAsyncAction(async () => {
        try {
          const quote = await tauriApi.quoteMarketTrade(quoteRequest);
          state.tradeQuoteData = quote;
        } catch (error) {
          state.tradeQuoteError = String(error);
          showToast(`Failed to fetch trade quote: ${error}`, "error");
        } finally {
          state.tradeQuoteLoading = false;
          render();
        }
      });
      return;
    }

    if (action === "confirm-trade-quote") {
      const quoteId = state.tradeQuoteData?.quote_id;
      if (!quoteId) {
        showToast("No active quote to execute", "error");
        return;
      }
      state.tradeQuoteExecuting = true;
      state.tradeQuoteError = "";
      render();

      runAsyncAction(async () => {
        try {
          const result = await tauriApi.executeMarketTradeQuote({
            quote_id: quoteId,
          });
          showToast(
            `Trade executed! txid: ${result.txid.slice(0, 16)}...`,
            "success",
          );
          resetTradeQuoteState();
          await refreshWallet(render);
          await refreshMarketsFromStore();
        } catch (error) {
          const message = String(error);
          state.tradeQuoteError = message;
          if (
            message.toLowerCase().includes("expired") ||
            message.toLowerCase().includes("not found")
          ) {
            state.tradeQuoteData = null;
          }
          showToast(`Trade execution failed: ${error}`, "error");
        } finally {
          state.tradeQuoteExecuting = false;
          render();
        }
      });
      return;
    }

    if (action === "use-cashout") {
      const market = getSelectedMarket();
      const positions = getPositionContracts(market);
      const closeSide: Side = positions.yes >= positions.no ? "yes" : "no";
      const availableLots = Math.max(
        0,
        Math.floor(closeSide === "yes" ? positions.yes : positions.no),
      );
      state.tradeIntent = "close";
      state.sizeMode = "contracts";
      state.buyLimitComposerOpen = false;
      state.selectedSide = closeSide;
      state.tradeContracts =
        availableLots > 0 ? Math.max(1, Math.floor(availableLots / 2)) : 0;
      state.tradeContractsDraft = String(state.tradeContracts);
      setLimitPriceSats(getBasePriceSats(market, closeSide));
      resetLimitSellOverride();
      resetTradeQuoteState();
      render();
      return;
    }

    if (action === "sell-max") {
      const market = getSelectedMarket();
      const availableLots = getSelectedPositionLots(market);
      state.tradeIntent = "close";
      state.sizeMode = "contracts";
      state.buyLimitComposerOpen = false;
      state.tradeContracts = Math.max(0, availableLots);
      state.tradeContractsDraft = String(state.tradeContracts);
      resetLimitSellOverride();
      resetTradeQuoteState();
      render();
      return;
    }

    if (action === "sell-25" || action === "sell-50") {
      const market = getSelectedMarket();
      const availableLots = getSelectedPositionLots(market);
      const ratio = action === "sell-25" ? 0.25 : 0.5;
      state.tradeIntent = "close";
      state.sizeMode = "contracts";
      state.buyLimitComposerOpen = false;
      state.tradeContracts = Math.max(0, Math.floor(availableLots * ratio));
      state.tradeContractsDraft = String(state.tradeContracts);
      resetLimitSellOverride();
      resetTradeQuoteState();
      render();
      return;
    }

    if (action === "trending-prev") {
      const total = getTrendingMarkets().length;
      state.trendingIndex = (state.trendingIndex - 1 + total) % total;
      render();
      return;
    }

    if (action === "trending-next") {
      const total = getTrendingMarkets().length;
      state.trendingIndex = (state.trendingIndex + 1) % total;
      render();
      return;
    }

    if (side) {
      state.selectedSide = side;
      const market = getSelectedMarket();
      setLimitPriceSats(getBasePriceSats(market, side));
      resetLimitSellOverride();
      resetTradeQuoteState();
      render();
      return;
    }

    if (tradeChoiceRaw) {
      const [intentRaw, sideRaw] = tradeChoiceRaw.split(":");
      const intent = intentRaw as TradeIntent;
      const pickedSide = sideRaw as Side;
      if (
        (intent === "open" || intent === "close") &&
        (pickedSide === "yes" || pickedSide === "no")
      ) {
        state.tradeIntent = intent;
        state.selectedSide = pickedSide;
        const market = getSelectedMarket();
        const availableLots = getSelectedPositionLots(market);
        setLimitPriceSats(getBasePriceSats(market, pickedSide));
        if (intent === "close") {
          state.sizeMode = "contracts";
          state.buyLimitComposerOpen = false;
          state.tradeContracts = clampContractsForSell(
            state.tradeContracts,
            availableLots,
          );
          state.tradeContractsDraft = String(state.tradeContracts);
        } else {
          state.sizeMode = "sats";
          state.orderType = "market";
          state.buyLimitComposerOpen = false;
        }
        resetLimitSellOverride();
        resetTradeQuoteState();
        render();
        return;
      }
    }

    if (tradeIntent) {
      state.tradeIntent = tradeIntent;
      const market = getSelectedMarket();
      const availableLots = getSelectedPositionLots(market);
      if (tradeIntent === "close") {
        state.sizeMode = "contracts";
        state.buyLimitComposerOpen = false;
        state.tradeContracts = clampContractsForSell(
          state.tradeContracts,
          availableLots,
        );
        state.tradeContractsDraft = String(state.tradeContracts);
      } else {
        state.sizeMode = "sats";
        state.orderType = "market";
        state.buyLimitComposerOpen = false;
      }
      resetLimitSellOverride();
      resetTradeQuoteState();
      render();
      return;
    }

    if (sizeMode) {
      state.sizeMode = sizeMode;
      resetLimitSellOverride();
      resetTradeQuoteState();
      render();
      return;
    }

    if (Number.isFinite(tradeSizePreset) && tradeSizePreset > 0) {
      state.sizeMode = "sats";
      state.tradeSizeSats = Math.floor(tradeSizePreset);
      state.tradeSizeSatsDraft = formatSatsInput(state.tradeSizeSats);
      resetLimitSellOverride();
      resetTradeQuoteState();
      render();
      return;
    }

    if (Number.isFinite(tradeSizeDelta) && tradeSizeDelta !== 0) {
      state.sizeMode = "sats";
      const current = Math.max(
        1,
        Math.floor(Number(state.tradeSizeSatsDraft.replace(/,/g, "")) || 1),
      );
      const next = Math.max(1, current + Math.floor(tradeSizeDelta));
      state.tradeSizeSats = next;
      state.tradeSizeSatsDraft = formatSatsInput(next);
      resetLimitSellOverride();
      resetTradeQuoteState();
      render();
      return;
    }

    if (
      action === "step-limit-price" &&
      Number.isFinite(limitPriceDelta) &&
      limitPriceDelta !== 0
    ) {
      const currentSats = clampContractPriceSats(
        state.limitPriceDraft.length > 0
          ? Number(state.limitPriceDraft)
          : state.limitPrice * SATS_PER_FULL_CONTRACT,
      );
      setLimitPriceSats(currentSats + Math.sign(limitPriceDelta));
      resetLimitSellOverride();
      resetTradeQuoteState();
      render();
      return;
    }

    if (
      action === "step-trade-contracts" &&
      Number.isFinite(contractsStepDelta) &&
      contractsStepDelta !== 0
    ) {
      const market = getSelectedMarket();
      const minContracts = state.tradeIntent === "close" ? 0 : 1;
      const current = Math.floor(Number(state.tradeContractsDraft));
      const baseValue = Number.isFinite(current)
        ? current
        : Math.max(minContracts, Math.floor(state.tradeContracts));
      const nextValue = Math.max(
        minContracts,
        baseValue + Math.sign(contractsStepDelta),
      );
      state.tradeContractsDraft = String(nextValue);
      commitTradeContractsDraft(market);
      resetLimitSellOverride();
      resetTradeQuoteState();
      render();
      return;
    }

    if (orderType) {
      state.orderType = orderType;
      if (orderType === "limit") {
        commitLimitPriceDraft();
      }
      resetLimitSellOverride();
      resetTradeQuoteState();
      render();
      return;
    }

    if (tab) {
      const market = getSelectedMarket();
      if (ticketActionAllowed(market, tab)) {
        state.actionTab = tab;
        render();
      }
      return;
    }

    if (
      action === "submit-trade" ||
      action === "submit-limit-buy" ||
      action === "submit-limit-sell" ||
      action === "submit-issue" ||
      action === "submit-redeem" ||
      action === "submit-cancel" ||
      action === "cancel-limit-order" ||
      action === "fill-limit-order" ||
      action === "submit-create-market"
    ) {
      if (action === "submit-create-market") {
        const question = state.createQuestion.trim();
        const description = state.createDescription.trim();
        const source = state.createResolutionSource.trim();
        if (
          !question ||
          !description ||
          !source ||
          !state.createSettlementInput
        ) {
          window.alert(
            "Complete question, settlement rule, source, and settlement deadline before creating.",
          );
          return;
        }
        const deadlineUnix = Math.floor(
          new Date(state.createSettlementInput).getTime() / 1000,
        );

        state.marketCreating = true;
        render();
        runAsyncAction(async () => {
          try {
            const result = await tauriApi.createContractOnchain({
              question,
              description,
              category: state.createCategory,
              resolution_source: source,
              settlement_deadline_unix: deadlineUnix,
              collateral_per_token: 5000,
            });
            markets.push(discoveredToMarket(result));
            state.view = "home";
            state.createQuestion = "";
            state.createDescription = "";
            state.createResolutionSource = "";
            state.createSettlementInput = defaultSettlementInput();
            showToast(
              `Market created! txid: ${result.creation_txid ?? "unknown"}`,
              "success",
            );
          } catch (error) {
            showToast(`Failed to create market: ${error}`, "error");
          } finally {
            state.marketCreating = false;
            render();
          }
        });
        return;
      }

      const market = getSelectedMarket();
      if (action === "cancel-limit-order") {
        const orderId = actionEl?.dataset.orderId;
        if (!orderId) {
          showToast("Missing order id", "error");
          return;
        }
        const order = market.limitOrders.find((item) => item.id === orderId);
        if (!order) {
          showToast("Order is no longer available", "error");
          return;
        }
        if (order.is_recoverable_by_current_wallet !== true) {
          showToast(
            "This order is not recoverable by the current wallet and cannot be cancelled",
            "error",
          );
          return;
        }

        const confirmed = window.confirm(
          `Cancel this order at ${order.price.toLocaleString()} sats?\n\nAny unfilled locked funds will be refunded to your wallet.\n\nProceed?`,
        );
        if (!confirmed) return;

        showToast("Cancelling limit order...", "info");
        runAsyncAction(async () => {
          try {
            const result = await tauriApi.cancelLimitOrder({
              order_params: toMakerOrderPayload(order),
              maker_base_pubkey_hex: order.maker_base_pubkey,
              order_nonce_hex: order.order_nonce,
            });
            showToast(
              `Order cancelled! txid: ${result.txid.slice(0, 16)}...`,
              "success",
            );
            await refreshWallet(render);
            await refreshMarketsFromStore();
            render();
          } catch (error) {
            showToast(`Cancel failed: ${error}`, "error");
          }
        });
        return;
      }

      if (action === "fill-limit-order") {
        const orderId = actionEl?.dataset.orderId;
        if (!orderId) {
          showToast("Missing order id", "error");
          return;
        }
        const order = market.limitOrders.find((item) => item.id === orderId);
        if (!order) {
          showToast("Order is no longer available", "error");
          return;
        }
        const fillBlockedReason = getFillBlockedReason(order);
        if (fillBlockedReason) {
          showToast(fillBlockedReason, "error");
          return;
        }

        const availableContracts = getAvailableOrderContracts(order);
        if (availableContracts <= 0) {
          showToast("Order has no fillable quantity", "error");
          return;
        }

        const requestedLots = Math.max(
          1,
          Math.floor(getTradePreview(market).requestedContracts),
        );
        const lotsToFill = Math.min(requestedLots, availableContracts);
        const actionLabel =
          order.direction === "sell-base"
            ? `buy ${lotsToFill.toLocaleString()} token(s)`
            : `sell ${lotsToFill.toLocaleString()} token(s)`;
        const confirmed = window.confirm(
          `Fill order at ${order.price.toLocaleString()} sats?\n\nThis will ${actionLabel} on ${state.selectedSide.toUpperCase()}.\nLots to fill: ${lotsToFill.toLocaleString()}\n\nProceed?`,
        );
        if (!confirmed) return;

        showToast("Filling limit order...", "info");
        runAsyncAction(async () => {
          try {
            const result = await tauriApi.fillLimitOrder({
              order_params: toMakerOrderPayload(order),
              maker_base_pubkey_hex: order.maker_base_pubkey,
              order_nonce_hex: order.order_nonce,
              lots_to_fill: lotsToFill,
            });
            showToast(
              `Order filled! txid: ${result.txid.slice(0, 16)}...`,
              "success",
            );
            await refreshWallet(render);
            await refreshMarketsFromStore();
            render();
          } catch (error) {
            showToast(`Fill failed: ${error}`, "error");
          }
        });
        return;
      }

      if (action === "submit-trade") {
        showToast("Use the quote flow for market trades", "info");
        return;
      }

      if (action === "submit-limit-buy") {
        state.tradeIntent = "open";
        state.orderType = "limit";
        state.sizeMode = "contracts";
        commitTradeContractsDraft(market);
        commitLimitPriceDraft();
        submitLimitOrder(market, "open", render);
        return;
      }

      if (action === "submit-limit-sell") {
        state.tradeIntent = "close";
        state.orderType = "limit";
        state.sizeMode = "contracts";
        commitTradeContractsDraft(market);
        commitLimitPriceDraft();
        if (state.tradeContracts < 1) {
          showToast(getSellSizeValidationMessage(market), "error");
          return;
        }
        if (state.limitSellOverrideAccepted) {
          submitLimitOrder(market, "close", render);
          return;
        }
        if (state.limitSellGuardChecking) {
          return;
        }

        const preview = getTradePreview(market);
        const guardSnapshot = captureLimitSellGuardSnapshot(market);
        const quoteRequest = buildMarketQuoteRequestPayload(
          marketToContractParams(market),
          market.marketId,
          state.selectedSide,
          "close",
          state.tradeSizeSats,
          state.tradeContracts,
        );

        state.limitSellGuardChecking = true;
        render();

        runAsyncAction(
          async () => {
            try {
              const sellQuote = await tauriApi.previewMarketTrade(quoteRequest);
              if (!isLimitSellGuardSnapshotCurrent(market, guardSnapshot)) {
                return;
              }
              const warning = getLimitSellWarningFromSellQuote(
                preview.limitPriceSats,
                sellQuote,
              );
              if (warning && !state.limitSellOverrideAccepted) {
                state.limitSellWarning = warning;
                state.limitSellWarningInfo = "";
                showToast(
                  `Limit price is ${warning.discountSats} sats below executable reference (${warning.referencePriceSats.toFixed(2)} sats). Acknowledge override to continue.`,
                  "error",
                );
                render();
                return;
              }
              state.limitSellWarning = null;
              state.limitSellWarningInfo = "";
            } catch {
              if (!isLimitSellGuardSnapshotCurrent(market, guardSnapshot)) {
                return;
              }
              state.limitSellWarning = null;
              state.limitSellWarningInfo =
                "Could not fetch a live executable sell reference quote. You can still place this limit sell.";
              showToast(
                "Executable reference quote unavailable. Continuing without limit-sell warning check.",
                "info",
              );
              render();
            }
            if (!isLimitSellGuardSnapshotCurrent(market, guardSnapshot)) {
              return;
            }
            submitLimitOrder(market, "close", render);
          },
          {
            onFinally: () => {
              if (!isLimitSellGuardSnapshotCurrent(market, guardSnapshot)) {
                return;
              }
              state.limitSellGuardChecking = false;
              render();
            },
          },
        );
        return;
      }

      if (action === "submit-issue") {
        const pairs = Math.max(1, Math.floor(state.pairsInput));
        if (!market.creationTxid) {
          showToast(
            "Market has no creation txid — cannot issue tokens",
            "error",
          );
          return;
        }
        showToast(
          `Issuing ${pairs} pair(s) for ${market.question.slice(0, 40)}...`,
          "info",
        );
        runAsyncAction(async () => {
          try {
            const result = await issueTokens(market, pairs);
            showToast(
              `Tokens issued! txid: ${result.txid.slice(0, 16)}...`,
              "success",
            );
          } catch (error) {
            showToast(`Issuance failed: ${error}`, "error");
          }
        });
        return;
      }

      if (action === "submit-cancel") {
        const pairs = Math.max(1, Math.floor(state.pairsInput));
        showToast(
          `Cancelling ${pairs} pair(s) for ${market.question.slice(0, 40)}...`,
          "info",
        );
        runAsyncAction(async () => {
          try {
            const result = await tauriApi.cancelTokens(
              marketToContractParams(market),
              pairs,
            );
            showToast(
              `Tokens cancelled! txid: ${result.txid.slice(0, 16)}... (${result.is_full_cancellation ? "full" : "partial"})`,
              "success",
            );
            await refreshWallet(render);
          } catch (error) {
            showToast(`Cancellation failed: ${error}`, "error");
          }
        });
        return;
      }

      if (action === "submit-redeem") {
        const tokens = Math.max(1, Math.floor(state.tokensInput));
        const paths = getPathAvailability(market);

        if (paths.redeem) {
          showToast(`Redeeming ${tokens} winning token(s)...`, "info");
          runAsyncAction(async () => {
            try {
              const result = await tauriApi.redeemTokens(
                marketToContractParams(market),
                tokens,
              );
              showToast(
                `Redeemed! txid: ${result.txid.slice(0, 16)}... payout: ${formatSats(result.payout_sats)}`,
                "success",
              );
              await refreshWallet(render);
            } catch (error) {
              showToast(`Redemption failed: ${error}`, "error");
            }
          });
        } else if (paths.expiryRedeem) {
          // For expiry redemption, determine which token side the user holds
          const yesBalance =
            state.walletData?.balance?.[reverseHex(market.yesAssetId)] ?? 0;
          // Use whichever side the user holds (prefer YES if both)
          const tokenAssetHex =
            yesBalance > 0 ? market.yesAssetId : market.noAssetId;

          const autoFinalize = market.state === 1 && isExpired(market);
          showToast(
            autoFinalize
              ? `Finalizing expiry and redeeming ${tokens} token(s)...`
              : `Redeeming ${tokens} expired token(s)...`,
            "info",
          );
          runAsyncAction(async () => {
            try {
              const result = await tauriApi.redeemExpired(
                marketToContractParams(market),
                tokenAssetHex,
                tokens,
              );
              showToast(
                `${autoFinalize ? "Finalize + redeem complete!" : "Expired tokens redeemed!"} txid: ${result.txid.slice(0, 16)}... payout: ${formatSats(result.payout_sats)}`,
                "success",
              );
              await refreshWallet(render);
            } catch (error) {
              showToast(`Expiry redemption failed: ${error}`, "error");
            }
          });
        } else {
          showToast("No redemption path available for this market", "error");
        }
        return;
      }
    }
  }
}
