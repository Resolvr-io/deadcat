import { tauriApi } from "../../api/tauri.ts";
import {
  discoveredToMarket,
  issueTokens,
  marketToContractParams,
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
  CovenantState,
  Market,
  MarketCategory,
  Side,
  TradeIntent,
} from "../../types.ts";
import { showToast } from "../../ui/toast.ts";
import { reverseHex } from "../../utils/crypto.ts";
import { formatSats, formatSatsInput } from "../../utils/format.ts";
import {
  clampContractPriceSats,
  commitLimitPriceDraft,
  commitTradeContractsDraft,
  getBasePriceSats,
  getPathAvailability,
  getPositionContracts,
  getSelectedMarket,
  getTradePreview,
  getTrendingMarkets,
  setLimitPriceSats,
  stateLabel,
} from "../../utils/market.ts";
import { runAsyncAction } from "./async-action.ts";
import type { ClickDomainContext } from "./context.ts";

function ticketActionAllowed(market: Market, tab: ActionTab): boolean {
  const paths = getPathAvailability(market);
  if (tab === "trade") return true;
  if (tab === "issue") return paths.initialIssue || paths.issue;
  if (tab === "redeem") return paths.redeem || paths.expiryRedeem;
  return paths.cancel;
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
      const confirmed = window.confirm(
        `Resolve "${market.question}" as ${outcomeLabel}?\n\nThis publishes a Schnorr signature to Nostr that permanently attests the outcome. This cannot be undone.`,
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
      const confirmed = window.confirm(
        `Execute on-chain resolution for "${market.question}"?\n\nOutcome: ${outcomeYes ? "YES" : "NO"}\nThis submits a Liquid transaction that transitions the covenant state.`,
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

    if (action === "use-cashout") {
      const market = getSelectedMarket();
      const positions = getPositionContracts(market);
      const closeSide: Side = positions.yes >= positions.no ? "yes" : "no";
      const available = closeSide === "yes" ? positions.yes : positions.no;
      state.tradeIntent = "close";
      state.sizeMode = "contracts";
      state.selectedSide = closeSide;
      state.tradeContracts = Math.max(0.01, Math.min(available, available / 2));
      state.tradeContractsDraft = state.tradeContracts.toFixed(2);
      setLimitPriceSats(getBasePriceSats(market, closeSide));
      render();
      return;
    }

    if (action === "sell-max") {
      const market = getSelectedMarket();
      const positions = getPositionContracts(market);
      const available =
        state.selectedSide === "yes" ? positions.yes : positions.no;
      state.tradeIntent = "close";
      state.sizeMode = "contracts";
      state.tradeContracts = Math.max(0.01, available);
      state.tradeContractsDraft = state.tradeContracts.toFixed(2);
      render();
      return;
    }

    if (action === "sell-25" || action === "sell-50") {
      const market = getSelectedMarket();
      const positions = getPositionContracts(market);
      const available =
        state.selectedSide === "yes" ? positions.yes : positions.no;
      const ratio = action === "sell-25" ? 0.25 : 0.5;
      state.tradeIntent = "close";
      state.sizeMode = "contracts";
      state.tradeContracts = Math.max(0.01, available * ratio);
      state.tradeContractsDraft = state.tradeContracts.toFixed(2);
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
        const positions = getPositionContracts(market);
        const available = pickedSide === "yes" ? positions.yes : positions.no;
        setLimitPriceSats(getBasePriceSats(market, pickedSide));
        if (intent === "close") {
          state.sizeMode = "contracts";
          state.tradeContracts = Math.max(
            0.01,
            Math.min(Math.max(0.01, available), state.tradeContracts),
          );
          state.tradeContractsDraft = state.tradeContracts.toFixed(2);
        }
        render();
        return;
      }
    }

    if (tradeIntent) {
      state.tradeIntent = tradeIntent;
      const market = getSelectedMarket();
      const positions = getPositionContracts(market);
      const available =
        state.selectedSide === "yes" ? positions.yes : positions.no;
      if (tradeIntent === "close") {
        state.sizeMode = "contracts";
        state.tradeContracts = Math.max(
          0.01,
          Math.min(Math.max(0.01, available), state.tradeContracts),
        );
        state.tradeContractsDraft = state.tradeContracts.toFixed(2);
      }
      render();
      return;
    }

    if (sizeMode) {
      state.sizeMode = sizeMode;
      render();
      return;
    }

    if (Number.isFinite(tradeSizePreset) && tradeSizePreset > 0) {
      state.sizeMode = "sats";
      state.tradeSizeSats = Math.floor(tradeSizePreset);
      state.tradeSizeSatsDraft = formatSatsInput(state.tradeSizeSats);
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
      render();
      return;
    }

    if (
      action === "step-trade-contracts" &&
      Number.isFinite(contractsStepDelta) &&
      contractsStepDelta !== 0
    ) {
      const market = getSelectedMarket();
      const current = Number(state.tradeContractsDraft);
      const baseValue = Number.isFinite(current)
        ? current
        : Math.max(0.01, state.tradeContracts);
      const nextValue = Math.max(
        0.01,
        baseValue + Math.sign(contractsStepDelta) * 0.01,
      );
      state.tradeContractsDraft = nextValue.toFixed(2);
      commitTradeContractsDraft(market);
      render();
      return;
    }

    if (orderType) {
      state.orderType = orderType;
      if (orderType === "limit") {
        commitLimitPriceDraft();
      }
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
      action === "submit-issue" ||
      action === "submit-redeem" ||
      action === "submit-cancel" ||
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
      if (action === "submit-trade") {
        const preview = getTradePreview(market);
        const pairs = Math.max(1, Math.floor(preview.requestedContracts));

        if (state.tradeIntent === "open") {
          // Buy = Issue pairs (mint YES+NO tokens, user keeps the side they want)
          if (!market.creationTxid) {
            showToast("Market has no creation txid — cannot trade", "error");
            return;
          }
          const paths = getPathAvailability(market);
          if (!paths.issue && !paths.initialIssue) {
            showToast(
              "Market is not in a tradeable state for issuance",
              "error",
            );
            return;
          }
          const collateralNeeded = pairs * 2 * market.cptSats;
          const confirmed = window.confirm(
            `Issue ${pairs} token pair(s) for "${market.question.slice(0, 50)}"?\n\nYou will receive ${pairs} YES + ${pairs} NO tokens.\nCollateral required: ${formatSats(collateralNeeded)}\n\nProceed?`,
          );
          if (!confirmed) return;

          showToast(`Issuing ${pairs} pair(s)...`, "info");
          runAsyncAction(async () => {
            try {
              const result = await issueTokens(market, pairs);
              showToast(
                `Tokens issued! txid: ${result.txid.slice(0, 16)}...`,
                "success",
              );
              await refreshWallet(render);
            } catch (error) {
              showToast(`Issuance failed: ${error}`, "error");
            }
          });
        } else {
          // Sell = Cancel pairs (burn equal YES+NO -> reclaim collateral)
          const position = getPositionContracts(market);
          const maxPairs = Math.min(position.yes, position.no);
          if (maxPairs <= 0) {
            showToast(
              "You need both YES and NO tokens to cancel pairs. Use Advanced Actions for single-side operations.",
              "error",
            );
            return;
          }
          const actualPairs = Math.min(pairs, maxPairs);
          const refund = actualPairs * 2 * market.cptSats;
          const confirmed = window.confirm(
            `Cancel ${actualPairs} token pair(s) for "${market.question.slice(0, 50)}"?\n\nBurns ${actualPairs} YES + ${actualPairs} NO tokens.\nCollateral refund: ${formatSats(refund)}\n\nProceed?`,
          );
          if (!confirmed) return;

          showToast(`Cancelling ${actualPairs} pair(s)...`, "info");
          runAsyncAction(async () => {
            try {
              const result = await tauriApi.cancelTokens(
                marketToContractParams(market),
                actualPairs,
              );
              showToast(
                `Pairs cancelled! txid: ${result.txid.slice(0, 16)}... (${result.is_full_cancellation ? "full" : "partial"})`,
                "success",
              );
              await refreshWallet(render);
            } catch (error) {
              showToast(`Cancellation failed: ${error}`, "error");
            }
          });
        }
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

          showToast(`Redeeming ${tokens} expired token(s)...`, "info");
          runAsyncAction(async () => {
            try {
              const result = await tauriApi.redeemExpired(
                marketToContractParams(market),
                tokenAssetHex,
                tokens,
              );
              showToast(
                `Expired tokens redeemed! txid: ${result.txid.slice(0, 16)}... payout: ${formatSats(result.payout_sats)}`,
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
